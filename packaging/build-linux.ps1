# Builds packaging/out/shotoshot-<version>-linux-<arch>.tar.gz for ARM64 and x86_64.
# Cross-compiles with cargo-zigbuild (glibc 2.31: Debian 11, Ubuntu 20.04 and later) and
# gathers the license files of every crate shipped in the Linux binary.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $env:CARGO_ZIGBUILD_ZIG_PATH) {
    $zig = python -c "import ziglang, os; print(os.path.join(os.path.dirname(ziglang.__file__), 'zig.exe'))"
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $zig)) { throw 'Zig not found: python -m pip install --user ziglang' }
    $env:CARGO_ZIGBUILD_ZIG_PATH = $zig
}
if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    $locator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $studio = & $locator -latest -products '*' -property installationPath
    if (-not $studio) { throw 'Visual Studio C++ build tools are required.' }
    & (Join-Path $studio 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
        $installed = Get-ChildItem (Join-Path $studio 'VC/Tools/MSVC') -Directory |
            Where-Object { Test-Path (Join-Path $_.FullName 'bin/HostX64/x64/link.exe') } |
            Sort-Object Name -Descending | Select-Object -First 1
        if (-not $installed) { throw 'No installed x64 MSVC linker found.' }
        $env:PATH = (Join-Path $installed.FullName 'bin/HostX64/x64') + ';' + $env:PATH
        $env:LIB = (Join-Path $installed.FullName 'lib/x64') + ';' + $env:LIB
    }
}
$utf8 = New-Object System.Text.UTF8Encoding $false
$cargo = [System.IO.File]::ReadAllText((Join-Path $root 'Cargo.toml'), $utf8)
$version = [regex]::Match($cargo, '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Cannot read the workspace version.' }
$out = Join-Path $root 'packaging/out'
New-Item -ItemType Directory -Force $out | Out-Null

function Get-Notices([string]$triple) {
    # Crates linked into the binary: normal dependencies reachable from shotoshot-app.
    $meta = cargo metadata --format-version 1 --locked --filter-platform $triple | ConvertFrom-Json
    $packages = @{}
    foreach ($p in $meta.packages) { $packages[$p.id] = $p }
    $nodes = @{}
    foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }
    $start = ($meta.packages | Where-Object { $_.name -eq 'shotoshot-app' }).id
    $seen = @{}
    $queue = New-Object System.Collections.Generic.Queue[string]
    $queue.Enqueue($start)
    while ($queue.Count) {
        foreach ($dep in $nodes[$queue.Dequeue()].deps) {
            $normal = $dep.dep_kinds | Where-Object { $null -eq $_.kind }
            if ($normal -and -not $seen.ContainsKey($dep.pkg)) { $seen[$dep.pkg] = $true; $queue.Enqueue($dep.pkg) }
        }
    }
    $text = New-Object System.Text.StringBuilder
    [void]$text.AppendLine('Shotoshot for Linux - third-party notices')
    [void]$text.AppendLine('==========================================')
    foreach ($id in ($seen.Keys | Sort-Object { $packages[$_].name })) {
        $p = $packages[$id]
        if ($p.name -like 'shotoshot-*') { continue }
        [void]$text.AppendLine()
        [void]$text.AppendLine("--- $($p.name) $($p.version) ($($p.license)) ---")
        $files = Get-ChildItem (Split-Path $p.manifest_path) -File |
            Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE)' }
        if (-not $files) { [void]$text.AppendLine("License: $($p.license). Source: $($p.repository)") }
        foreach ($f in $files) {
            [void]$text.AppendLine("[$($f.Name)]")
            [void]$text.AppendLine([System.IO.File]::ReadAllText($f.FullName, $utf8).Trim())
        }
    }
    $text.ToString()
}

Push-Location $root
try {
    foreach ($arch in @(@{ Name = 'arm64'; Triple = 'aarch64-unknown-linux-gnu' }, @{ Name = 'x86_64'; Triple = 'x86_64-unknown-linux-gnu' })) {
        cargo zigbuild --target "$($arch.Triple).2.31" --release --locked -p shotoshot-app --bin shotoshot
        if ($LASTEXITCODE -ne 0) { throw "Build failed for $($arch.Name)." }
        $name = "shotoshot-$version-linux-$($arch.Name)"
        $stage = Join-Path $out "linux-stage/$name"
        if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
        New-Item -ItemType Directory -Force (Join-Path $stage 'icons') | Out-Null
        Copy-Item (Join-Path $root "target/$($arch.Triple)/release/shotoshot") $stage
        Copy-Item (Join-Path $PSScriptRoot 'linux/*') $stage
        foreach ($size in 32, 48, 256) {
            Copy-Item (Join-Path $root "crates/shotoshot-app/assets/shotoshot-$size.png") (Join-Path $stage 'icons')
        }
        [System.IO.File]::WriteAllText((Join-Path $stage 'THIRD-PARTY-NOTICES.txt'), (Get-Notices $arch.Triple), $utf8)
        $archive = Join-Path $out "$name.tar.gz"
        tar -czf $archive -C (Split-Path $stage) $name
        if ($LASTEXITCODE -ne 0) { throw "Archive failed for $($arch.Name)." }
        Write-Output ("{0}  {1} ({2} bytes)" -f (Get-FileHash $archive -Algorithm SHA256).Hash, (Split-Path $archive -Leaf), (Get-Item $archive).Length)
    }
} finally { Pop-Location }
