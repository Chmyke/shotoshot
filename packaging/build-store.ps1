# Builds the Microsoft Store package (MSIX) from target/release/shotoshot.exe, the same binary as
# the website's installer. Does not run cargo: build the release binary first (check.ps1 does it).
#   (no switch)  packaging/out/Shotoshot-<version>.msix, unsigned, for Partner Center: the Store
#                signs it. Identity from packaging/store/identity.json (Partner Center values).
#   -Test        packaging/out/Shotoshot-<version>-test.msix, signed with the local test
#                certificate CN=Mickael Mancini (already trusted on this PC), to install and try.
#   -Check       packs into target/ with the test identity and validates it, signs nothing:
#                the check of check.ps1 (manifest schema, files, logos, version).
# Also writes packaging/out/Shotoshot-store-icon-300.png, the listing's tile icon.
# Why and how: docs/contexte/microsoft-store.md.
param([switch]$Test, [switch]$Check, [string]$Thumbprint = $env:SHOTOSHOT_SIGN_THUMBPRINT)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target/release/shotoshot.exe'
$template = Join-Path $PSScriptRoot 'store/AppxManifest.xml'
$identityFile = Join-Path $PSScriptRoot 'store/identity.json'
$testIdentity = @{ name = 'MickaelMancini.Shotoshot.Test'; publisher = 'CN=Mickael Mancini'; publisherDisplayName = 'Mickael Mancini' }

function Read-Utf8([string]$path) { return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8) }

function Find-SdkTool([string]$name) {
    $sdk = Get-ChildItem 'C:/Program Files (x86)/Windows Kits/10/bin' -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName "x64/$name") } |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if (-not $sdk) { throw "$name not found: install the Windows SDK (winget install Microsoft.WindowsSDK.10.0.26100)." }
    return (Join-Path $sdk.FullName "x64/$name")
}

# The package version follows the workspace; the Store wants X.Y.Z.0.
$cargoMatch = [regex]::Match((Read-Utf8 (Join-Path $root 'Cargo.toml')), '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
if (-not $cargoMatch.Success) { throw 'Cannot read [workspace.package] version in Cargo.toml.' }
$version = $cargoMatch.Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Version '$version' is not X.Y.Z." }
if (-not (Test-Path $exe)) { throw "Missing $exe. Build the release binary first." }
$built = "$([System.Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $exe).Path).ProductVersion)".Trim()
if ($built -ne "$version.0") { throw "$exe is version '$built', not '$version.0'. Rebuild it (check.ps1) first." }
# The Store refuses a leading 0 and wants a last 0: package = (major + 1).minor.patch.0, so 0.3.49
# is 1.3.49.0 (the rule of Sortypix's package). The app still shows its own version.
$parts = $version.Split('.')
$packageVersion = "$([int]$parts[0] + 1).$($parts[1]).$($parts[2]).0"

if ($Test -or $Check) {
    $identity = $testIdentity
} else {
    if (-not (Test-Path $identityFile)) { throw "Missing $identityFile (Partner Center, Product identity)." }
    $json = Read-Utf8 $identityFile | ConvertFrom-Json
    $identity = @{ name = $json.name; publisher = $json.publisher; publisherDisplayName = $json.publisherDisplayName }
}
foreach ($key in @('name', 'publisher', 'publisherDisplayName')) {
    if (-not $identity[$key]) { throw "Identity has no $key." }
}

$stage = Join-Path $root 'target/store-package'
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
$assets = Join-Path $stage 'Assets'
[System.IO.Directory]::CreateDirectory($assets) | Out-Null
$out = if ($Check) { Join-Path $root 'target' } else { Join-Path $PSScriptRoot 'out' }
[System.IO.Directory]::CreateDirectory($out) | Out-Null

# The binary and, as with the installer, the license and the pointer to the source code (GPL).
Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'shotoshot.exe')
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination (Join-Path $stage 'LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ABOUT.txt') -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY-NOTICES.txt') -Destination $stage

# Logos drawn from the icon's own geometry at the sizes the manifest names.
$draw = Join-Path $root 'crates/shotoshot-app/assets/generate-icon.ps1'
& $draw -Sizes @(44, 50, 150, 300) -OutDir $assets -NoIco
$names = @{ 44 = 'Square44x44Logo.png'; 50 = 'StoreLogo.png'; 150 = 'Square150x150Logo.png' }
foreach ($size in $names.Keys) {
    Move-Item -LiteralPath (Join-Path $assets "shotoshot-$size.png") -Destination (Join-Path $assets $names[$size])
}
$icon = Join-Path $out 'Shotoshot-store-icon-300.png'
Move-Item -LiteralPath (Join-Path $assets 'shotoshot-300.png') -Destination $icon -Force

$manifest = (Read-Utf8 $template).Replace('@NAME@', $identity.name).Replace('@PUBLISHER@', $identity.publisher).Replace('@PUBLISHER_NAME@', $identity.publisherDisplayName).Replace('@VERSION@', $packageVersion)
if ($manifest -match '@[A-Z_]+@') { throw "Unfilled placeholder in the manifest: $($Matches[0])." }
# Every file the manifest names must be in the package (makeappx checks the logos too).
foreach ($file in [regex]::Matches($manifest, '(?:Executable="|Logo="|<Logo>|<uap:Logo>)([^"<]+)')) {
    $path = Join-Path $stage $file.Groups[1].Value
    if (-not (Test-Path -LiteralPath $path)) { throw "The manifest names a missing file: $($file.Groups[1].Value)." }
}
[System.IO.File]::WriteAllText((Join-Path $stage 'AppxManifest.xml'), $manifest, (New-Object System.Text.UTF8Encoding($false)))

$suffix = if ($Test) { '-test' } elseif ($Check) { '-check' } else { '' }
$package = Join-Path $out "Shotoshot-$version$suffix.msix"
$makeappx = Find-SdkTool 'makeappx.exe'
# makeappx validates the manifest against the schema and the files it names.
$log = & $makeappx pack /o /h SHA256 /d $stage /p $package 2>&1
if ($LASTEXITCODE -ne 0) { $log | Write-Output; throw "makeappx failed with exit code $LASTEXITCODE." }

if ($Test) {
    if (-not $Thumbprint) {
        $found = @(Get-ChildItem Cert:/CurrentUser/My | Where-Object { $_.Subject -ceq $testIdentity.publisher -and $_.HasPrivateKey })
        if ($found.Count -ne 1) { throw "Expected one certificate $($testIdentity.publisher) with its key in CurrentUser\My; pass -Thumbprint." }
        $Thumbprint = $found[0].Thumbprint
    }
    if ($Thumbprint -notmatch '^[A-Fa-f0-9]{40}$') { throw 'Invalid signing thumbprint.' }
    $signtool = Find-SdkTool 'signtool.exe'
    & $signtool sign /q /fd SHA256 /sha1 $Thumbprint $package
    if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE." }
}

$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $package).Hash
$size = [math]::Round((Get-Item -LiteralPath $package).Length / 1MB, 2)
Write-Output "Store package: $package ($size MB, $($identity.name) $packageVersion)"
Write-Output "SHA256: $hash"
