param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location -LiteralPath $projectRoot
if (!$SkipBuild) {
    & pnpm tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) { throw 'Portable release build failed.' }
}
$config = Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
$executable = Get-Item -LiteralPath 'src-tauri/target/release/tauri-app.exe'
if ($executable.VersionInfo.ProductName -ne $config.productName -or $executable.VersionInfo.ProductVersion -ne $config.version) { throw 'The release executable does not match the normal application/version.' }
$outputDirectory = Join-Path $projectRoot 'output\portable'
$packageName = "Corregir_$($config.version)_windows_x64_portable"
$stagingDirectory = Join-Path $outputDirectory ([Guid]::NewGuid().ToString('N'))
$packageDirectory = Join-Path $stagingDirectory $packageName
New-Item -ItemType Directory -Path $packageDirectory -Force | Out-Null
Copy-Item -LiteralPath $executable.FullName -Destination (Join-Path $packageDirectory 'Corregir.exe')
'{"workspace":"work"}' | Set-Content -LiteralPath (Join-Path $packageDirectory 'corregir-portable.json') -Encoding utf8
foreach ($mapping in $config.bundle.resources.PSObject.Properties) {
    $source = Get-Item -LiteralPath (Join-Path $projectRoot (Join-Path 'src-tauri' $mapping.Name))
    $destination = [IO.Path]::GetFullPath((Join-Path $packageDirectory $mapping.Value))
    if (!$destination.StartsWith($packageDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Resource destination escaped the portable package.' }
    if ($source.PSIsContainer) {
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        foreach ($file in Get-ChildItem -LiteralPath $source.FullName -Recurse -File) {
            $relative = [IO.Path]::GetRelativePath($source.FullName, $file.FullName)
            $target = Join-Path $destination $relative
            New-Item -ItemType Directory -Path (Split-Path $target -Parent) -Force | Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $target
        }
    } else {
        New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath $source.FullName -Destination $destination
    }
}
$manifest = Get-Content (Join-Path $packageDirectory 'manifest.json') -Raw | ConvertFrom-Json
foreach ($resource in $manifest.resources) {
    $resourcePath = Join-Path $packageDirectory $resource.path
    if ((Get-Item -LiteralPath $resourcePath).Length -ne $resource.size_bytes -or (Get-FileHash -LiteralPath $resourcePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $resource.sha256) { throw "Portable resource verification failed: $($resource.path)" }
}
Copy-Item -LiteralPath 'docs/PORTABLE-WINDOWS.md' -Destination (Join-Path $packageDirectory 'README-PORTABLE.md')
$archivePath = Join-Path $outputDirectory ($packageName + '.zip')
Compress-Archive -LiteralPath $packageDirectory -DestinationPath $archivePath -CompressionLevel Optimal -Force
$hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $packageName.zip" | Set-Content -LiteralPath ($archivePath + '.sha256') -Encoding ascii
Write-Output "Portable package: $archivePath"
Write-Output "SHA-256: $hash"
