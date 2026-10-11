param(
    [Parameter(Mandatory)][string]$InstallerPath,
    [Parameter(Mandatory)][string]$CertificateThumbprint,
    [string]$TimestampUrl = 'http://timestamp.digicert.com'
)
$ErrorActionPreference = 'Stop'
$installer = (Resolve-Path -LiteralPath $InstallerPath).Path
if ([IO.Path]::GetExtension($installer) -ne '.exe') { throw 'Select a Windows installer executable.' }
$thumbprint = $CertificateThumbprint.Replace(' ', '')
if ($thumbprint -notmatch '^[A-Fa-f0-9]{40}$') { throw 'Enter a valid certificate thumbprint.' }
$certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$thumbprint"
if (!$certificate.HasPrivateKey -or $certificate.NotBefore -gt (Get-Date) -or $certificate.NotAfter -le (Get-Date)) { throw 'The signing certificate must be valid and have an accessible private key.' }
if ($certificate.EnhancedKeyUsageList.ObjectId -notcontains '1.3.6.1.5.5.7.3.3') { throw 'The certificate does not permit code signing.' }
$signTool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (!$signTool) {
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $signToolPath = Get-ChildItem -LiteralPath $kits -Directory | Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName 'x64\signtool.exe' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (!$signToolPath) { throw 'Install Windows SDK signing tools.' }
} else { $signToolPath = $signTool.Source }
& $signToolPath sign /sha1 $thumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $installer
if ($LASTEXITCODE -ne 0) { throw 'Installer signing failed.' }
& $signToolPath verify /pa /all $installer
if ($LASTEXITCODE -ne 0) { throw 'Installer signature verification failed.' }
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $thumbprint) { throw 'The installer did not verify against the requested certificate.' }
Write-Output 'Installer signature verified.'
