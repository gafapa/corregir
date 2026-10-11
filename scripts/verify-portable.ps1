param([Parameter(Mandatory)][string]$ArchivePath)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$runDirectory = Join-Path $projectRoot ('output\portable-verification\' + [Guid]::NewGuid().ToString('N'))
$extractDirectory = Join-Path $runDirectory 'Extracted package'
Expand-Archive -LiteralPath (Resolve-Path -LiteralPath $ArchivePath).Path -DestinationPath $extractDirectory
$package = Get-ChildItem -LiteralPath $extractDirectory -Directory
if ($package.Count -ne 1) { throw 'Expected exactly one portable application folder.' }
$relocated = [IO.Path]::GetFullPath((Join-Path $runDirectory 'Relocated package'))
$resolvedSource = [IO.Path]::GetFullPath($package.FullName)
if (!$resolvedSource.StartsWith($runDirectory + '\') -or !$relocated.StartsWith($runDirectory + '\')) { throw 'Relocation escaped the isolated verification directory.' }
Move-Item -LiteralPath $resolvedSource -Destination $relocated
$application = Join-Path $relocated 'Corregir.exe'
$config = Get-Content (Join-Path $projectRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$metadata = (Get-Item -LiteralPath $application).VersionInfo
if ($metadata.ProductName -ne $config.productName -or $metadata.ProductVersion -ne $config.version) { throw 'Unexpected portable executable metadata.' }
$location = Get-Content (Join-Path $relocated 'corregir-portable.json') -Raw | ConvertFrom-Json
if ($location.workspace -ne 'work') { throw 'The portable package has an unexpected default work-directory location.' }
if (Test-Path -LiteralPath (Join-Path $relocated 'work')) { throw 'The archive unexpectedly includes an existing workspace.' }
$manifest = Get-Content (Join-Path $relocated 'manifest.json') -Raw | ConvertFrom-Json
foreach ($resource in $manifest.resources) {
    $file = Join-Path $relocated $resource.path
    if ((Get-Item -LiteralPath $file).Length -ne $resource.size_bytes -or (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $resource.sha256) { throw "Portable resource mismatch: $($resource.path)" }
}
foreach ($fixture in 'language-student-1.pdf','language-student-2-mixed.pdf') {
    $source = Join-Path $projectRoot (Join-Path 'synthetic-data' $fixture)
    $worker = [Diagnostics.Process]::new()
    $worker.StartInfo.FileName = $application
    $worker.StartInfo.ArgumentList.Add('--document-worker')
    $worker.StartInfo.WorkingDirectory = $runDirectory
    $worker.StartInfo.UseShellExecute = $false
    $worker.StartInfo.CreateNoWindow = $true
    $worker.StartInfo.RedirectStandardInput = $true
    $worker.StartInfo.RedirectStandardOutput = $true
    $started = $false
    try {
        $started = $worker.Start()
        if (!$started) { throw 'Portable document worker did not start.' }
        $request = @{ file_path = $source; pdfium_directory = (Join-Path $relocated 'pdfium/bin'); models_directory = (Join-Path $relocated 'models/ocr'); force_ocr = $false } | ConvertTo-Json -Compress
        $worker.StandardInput.WriteLine($request)
        $worker.StandardInput.Close()
        $output = $worker.StandardOutput.ReadToEndAsync()
        if (!$worker.WaitForExit(120000)) { throw 'Portable document worker timed out.' }
        if ($worker.ExitCode -ne 0) { throw 'Portable document worker failed.' }
        $messages = $output.GetAwaiter().GetResult().Split("`n", [StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { $_ | ConvertFrom-Json }
        $result = $messages | Where-Object kind -eq 'complete' | Select-Object -Last 1
        $expectedOcr = $fixture -like '*mixed*'
        if (!$result -or $result.used_ocr -ne $expectedOcr -or $result.source_hash -ne (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Portable extraction returned an unexpected result.' }
        $text = $result.texts -join ' '
        if ((!$expectedOcr -and $text -notmatch 'nostalgia') -or ($expectedOcr -and $text -notmatch 'poem|perfect')) { throw 'Portable extraction omitted the expected answer.' }
        Write-Output "Portable extraction passed: $fixture"
    } finally {
        if ($started -and !$worker.HasExited) { $worker.Kill($true); $worker.WaitForExit() }
        $worker.Dispose()
    }
}
@{ passed = $true; application_version = $metadata.ProductVersion; resource_checks = $manifest.resources.Count; relocated_directory = $relocated; archive_sha256 = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runDirectory 'result.json') -Encoding utf8
Write-Output "Portable verification passed: $runDirectory"
