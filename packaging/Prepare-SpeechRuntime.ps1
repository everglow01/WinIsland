$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryRoot
try {
    $metadataJson = cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the Cargo target directory.' }
    $metadata = $metadataJson | ConvertFrom-Json
    $cacheRoot = [IO.Path]::GetFullPath((Join-Path $metadata.target_directory 'sherpa-onnx-prebuilt'))
    $cachePrefix = $cacheRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $required = @('sherpa-onnx-c-api.lib', 'onnxruntime.lib', 'sherpa-onnx-c-api.dll', 'onnxruntime.dll', 'onnxruntime_providers_shared.dll')

    if (Test-Path -LiteralPath $cacheRoot -PathType Container) {
        foreach ($package in Get-ChildItem -LiteralPath $cacheRoot -Directory -Filter '*-win-*-shared-*-lib') {
            $packagePath = [IO.Path]::GetFullPath($package.FullName)
            if (-not $packagePath.StartsWith($cachePrefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Speech cache path is outside its Cargo cache directory.'
            }
            $missing = @($required | Where-Object {
                -not (Test-Path -LiteralPath (Join-Path $packagePath "lib/$_") -PathType Leaf)
            })
            if ($missing.Count -gt 0) {
                Write-Output "Discarding incomplete speech cache '$($package.Name)': $($missing -join ', ')"
                Remove-Item -LiteralPath $packagePath -Recurse -Force
            }
        }
    }

    # Re-run DLL staging even when the native library cache is complete.
    cargo clean --release --package sherpa-onnx-sys
    if ($LASTEXITCODE -ne 0) { throw 'Cannot refresh the speech runtime build outputs.' }
} finally {
    Pop-Location
}
