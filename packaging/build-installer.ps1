# Builds packaging/out/Shotoshot-Setup-<version>.exe from target/release/shotoshot.exe.
# Does not run cargo: build the release binary first (check.ps1 does it).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$iss = Join-Path $PSScriptRoot 'shotoshot.iss'
$exe = Join-Path $root 'target/release/shotoshot.exe'

function Find-Iscc {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs/Inno Setup 6/ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6/ISCC.exe')
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) { return $candidate }
    }
    throw 'ISCC.exe not found. Install it: winget install --id JRSoftware.InnoSetup -e --scope user'
}

function Read-Utf8([string]$path) {
    return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
}

# The installer version must follow the workspace version.
$cargo = Read-Utf8 (Join-Path $root 'Cargo.toml')
$cargoMatch = [regex]::Match($cargo, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
$issMatch = [regex]::Match((Read-Utf8 $iss), '(?m)^#define AppVersion "([^"]+)"')
if (-not $cargoMatch.Success) { throw 'Cannot read [workspace.package] version in Cargo.toml.' }
if (-not $issMatch.Success) { throw 'Cannot read #define AppVersion in shotoshot.iss.' }
$version = $issMatch.Groups[1].Value
if ($version -ne $cargoMatch.Groups[1].Value) {
    throw "Version mismatch: shotoshot.iss says $version, Cargo.toml says $($cargoMatch.Groups[1].Value)."
}
if (-not (Test-Path $exe)) { throw "Missing $exe. Build the release binary first." }

$iscc = Find-Iscc
& $iscc /Q $iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE." }

$setup = Join-Path $PSScriptRoot "out/Shotoshot-Setup-$version.exe"
if (-not (Test-Path $setup)) { throw "Expected output not found: $setup" }
$item = Get-Item $setup
$hash = (Get-FileHash -Algorithm SHA256 $setup).Hash
Write-Output "Setup:  $($item.FullName)"
Write-Output "Size:   $($item.Length) bytes"
Write-Output "SHA256: $hash"
Write-Output "Binary SHA256: $((Get-FileHash -Algorithm SHA256 $exe).Hash)"
