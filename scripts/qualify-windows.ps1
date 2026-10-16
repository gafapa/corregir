param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$env:CARGO_BUILD_JOBS = '4'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location -LiteralPath $projectRoot
$runName = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$runDirectory = Join-Path (Join-Path $projectRoot 'output\desktop-qualification') $runName
$installDirectory = Join-Path $runDirectory 'installation'
$dataDirectory = Join-Path $env:APPDATA 'com.corregir.qualification'
if (Test-Path -LiteralPath $dataDirectory) { throw 'The isolated qualification profile already exists; archive its previous test data before starting a clean qualification.' }
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
if (!$SkipBuild) {
    & pnpm tauri build --features qualification --config src-tauri/tauri.qualification.conf.json --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw 'Qualification build failed.' }
}
$installer = Get-ChildItem 'src-tauri/target/release/bundle/nsis/*0.1.1*x64*.exe' | Where-Object Name -Like '*Qualification*' | Select-Object -First 1
if (!$installer) { throw 'Qualification installer version 0.1.1 is missing.' }
Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $runDirectory $installer.Name)
$installation = Start-Process -FilePath $installer.FullName -ArgumentList '/S',"/D=$installDirectory" -Wait -PassThru -WindowStyle Hidden
if ($installation.ExitCode -ne 0) { throw 'Clean installation failed.' }
$application = Join-Path $installDirectory 'corregir.exe'
if (!(Test-Path -LiteralPath $application)) { throw 'Installed application is missing.' }
$manifest = Get-Content (Join-Path $installDirectory 'manifest.json') -Raw | ConvertFrom-Json
foreach ($resource in $manifest.resources) {
    $resourcePath = Join-Path $installDirectory $resource.path
    if ((Get-Item -LiteralPath $resourcePath).Length -ne $resource.size_bytes -or (Get-FileHash -LiteralPath $resourcePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $resource.sha256) { throw "Installed resource verification failed: $($resource.path)" }
}
foreach ($mode in 'create','reopen') {
    & node scripts/qualify-desktop.cjs $application $mode $runDirectory
    if ($LASTEXITCODE -ne 0) { throw "Desktop qualification failed: $mode" }
}
$upgradeConfig = Get-Content 'src-tauri/tauri.qualification.conf.json' -Raw | ConvertFrom-Json
$upgradeConfig.version = '0.1.2'
$upgradeConfigPath = Join-Path $runDirectory 'tauri-upgrade.conf.json'
$upgradeConfig | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $upgradeConfigPath -Encoding utf8
& pnpm tauri build --features qualification --config $upgradeConfigPath --bundles nsis
if ($LASTEXITCODE -ne 0) { throw 'Qualification upgrade build failed.' }
$upgradeInstaller = Get-ChildItem 'src-tauri/target/release/bundle/nsis/*0.1.2*x64*.exe' | Where-Object Name -Like '*Qualification*' | Select-Object -First 1
if (!$upgradeInstaller) { throw 'Qualification upgrade installer is missing.' }
Copy-Item -LiteralPath $upgradeInstaller.FullName -Destination (Join-Path $runDirectory $upgradeInstaller.Name)
$upgrade = Start-Process -FilePath $upgradeInstaller.FullName -ArgumentList '/S',"/D=$installDirectory" -Wait -PassThru -WindowStyle Hidden
if ($upgrade.ExitCode -ne 0) { throw 'Installer upgrade failed.' }
& node scripts/qualify-desktop.cjs $application upgrade $runDirectory
if ($LASTEXITCODE -ne 0) { throw 'Records did not survive the installer upgrade.' }
# Both resolved targets are explicitly confined to this isolated test profile/run.
$resolvedData = [IO.Path]::GetFullPath($dataDirectory)
$expectedData = [IO.Path]::GetFullPath((Join-Path $env:APPDATA 'com.corregir.qualification'))
$archive = [IO.Path]::GetFullPath((Join-Path $runDirectory 'workspace-before-restore'))
if ($resolvedData -ne $expectedData -or !$archive.StartsWith($runDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Qualification archive path escaped the intended test scope.' }
if (Test-Path -LiteralPath $archive) { throw 'A previous qualification archive already exists.' }
Move-Item -LiteralPath $resolvedData -Destination $archive
& node scripts/qualify-desktop.cjs $application restore $runDirectory
if ($LASTEXITCODE -ne 0) { throw 'Restore into a fresh qualification workspace failed.' }
& node scripts/qualify-desktop.cjs $application restored-reopen $runDirectory
if ($LASTEXITCODE -ne 0) { throw 'Restored records did not survive reopening.' }
$restoredArchive = [IO.Path]::GetFullPath((Join-Path $runDirectory 'workspace-after-restore'))
$resolvedInstall = [IO.Path]::GetFullPath($installDirectory)
if ($resolvedData -ne $expectedData -or !$restoredArchive.StartsWith($runDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or !$resolvedInstall.StartsWith($runDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Qualification cleanup path escaped the isolated test scope.' }
if (Test-Path -LiteralPath $restoredArchive) { throw 'A restored-workspace archive already exists.' }
Move-Item -LiteralPath $resolvedData -Destination $restoredArchive
$uninstaller = Join-Path $resolvedInstall 'uninstall.exe'
$cleanup = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
if ($cleanup.ExitCode -ne 0 -or (Test-Path -LiteralPath $application)) { throw 'Qualification application cleanup failed.' }
Write-Output "Desktop and installer qualification passed. Evidence: $runDirectory"
