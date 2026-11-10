param(
    [ValidateSet('dev', 'release')]
    [string]$Profile = 'release',
    [string]$Target = $env:CARGO_BUILD_TARGET
)

$ErrorActionPreference = 'Stop'

function Test-SpeechArchive([string]$Path, $Asset) {
    (Test-Path -LiteralPath $Path -PathType Leaf) -and
        (Get-Item -LiteralPath $Path).Length -eq $Asset.size -and
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $Asset.digest.Substring(7)
}

function Save-SpeechArchive([string]$Version, [string]$Name, [string]$Path) {
    $partialPath = "$Path.part"
    for ($attempt = 1; $attempt -le 4; $attempt++) {
        try {
            $releaseJson = gh api "repos/k2-fsa/sherpa-onnx/releases/tags/v$Version"
            if ($LASTEXITCODE -ne 0) { throw 'Cannot read speech runtime release metadata.' }
            $assets = @(($releaseJson | ConvertFrom-Json).assets | Where-Object { $_.name -ceq $Name })
            if ($assets.Count -ne 1 -or $assets[0].digest -notmatch '^sha256:[0-9a-f]{64}$') {
                throw "Cannot find a checksummed speech runtime archive: $Name"
            }
            $asset = $assets[0]
            if (Test-SpeechArchive $Path $asset) {
                Write-Output "Using verified speech archive '$Name'."
                return
            }

            Write-Output "Downloading speech runtime '$Name' (attempt $attempt/4)."
            gh release download "v$Version" --repo k2-fsa/sherpa-onnx --pattern $Name --output $partialPath --clobber
            if ($LASTEXITCODE -ne 0) { throw 'Speech runtime archive download failed.' }
            if (-not (Test-SpeechArchive $partialPath $asset)) {
                throw 'Speech runtime archive size or SHA-256 does not match the release.'
            }
            Move-Item -LiteralPath $partialPath -Destination $Path -Force
            return
        } catch {
            if (Test-Path -LiteralPath $partialPath) {
                Remove-Item -LiteralPath $partialPath -Force
            }
            if ($attempt -eq 4) { throw }
            Write-Warning "$_ Retrying speech runtime preparation."
            Start-Sleep -Seconds (3 * $attempt)
        }
    }
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryRoot
try {
    $targetArgs = @()
    if ([string]::IsNullOrWhiteSpace($Target)) {
        $rustInfo = rustc -vV
        if ($LASTEXITCODE -ne 0) { throw 'Cannot determine the Rust host target.' }
        $Target = ($rustInfo | Where-Object { $_.StartsWith('host: ') }).Substring(6)
    } else {
        $targetArgs = @('--target', $Target)
    }
    $architecture = switch -Regex ($Target) {
        '^x86_64-pc-windows-' { 'x64'; break }
        '^aarch64-pc-windows-' { 'arm64'; break }
        default { throw "Unsupported speech runtime target: $Target" }
    }
    $metadataJson = cargo metadata --locked --format-version 1 --filter-platform $Target
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the Cargo target directory.' }
    $metadata = $metadataJson | ConvertFrom-Json -AsHashtable
    $runtime = @($metadata.packages | Where-Object { $_.name -ceq 'sherpa-onnx-sys' })
    if ($runtime.Count -ne 1) { throw 'Expected one resolved sherpa-onnx-sys version.' }
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

    $archiveStem = "sherpa-onnx-v$($runtime[0].version)-win-$architecture-shared-MT-Release-lib"
    $libraryPath = Join-Path $cacheRoot "$archiveStem/lib"
    if (-not $env:SHERPA_ONNX_LIB_DIR -and -not $env:SHERPA_ONNX_ARCHIVE_DIR -and
        -not (Test-Path -LiteralPath $libraryPath -PathType Container)) {
        [IO.Directory]::CreateDirectory($cacheRoot) | Out-Null
        $archivePath = [IO.Path]::GetFullPath((Join-Path $cacheRoot "$archiveStem.tar.bz2"))
        if (-not $archivePath.StartsWith($cachePrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Speech archive path is outside its Cargo cache directory.'
        }
        Save-SpeechArchive $runtime[0].version "$archiveStem.tar.bz2" $archivePath
    }

    # Re-run DLL staging even when the native library cache is complete.
    cargo clean --profile $Profile --package sherpa-onnx-sys @targetArgs
    if ($LASTEXITCODE -ne 0) { throw 'Cannot refresh the speech runtime build outputs.' }
} finally {
    Pop-Location
}
