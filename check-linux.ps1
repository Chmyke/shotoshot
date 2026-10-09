# Cross-compiles Shotoshot for a Linux ARM64 machine and runs its tests there over SSH.
# Nothing is compiled on the remote host; binaries run with low priority in ~/shotoshot-dev.
# Needs: rustup target aarch64-unknown-linux-gnu, cargo-zigbuild, Zig (pip install ziglang),
# and key-based SSH access to the host (default alias "pi").
param([string]$RemoteHost = 'pi', [switch]$Diagnostic)
$ErrorActionPreference = 'Stop'
$target = 'aarch64-unknown-linux-gnu'
if (-not $env:CARGO_ZIGBUILD_ZIG_PATH) {
    $zig = python -c "import ziglang, os; print(os.path.join(os.path.dirname(ziglang.__file__), 'zig.exe'))"
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $zig)) { throw 'Zig not found: python -m pip install --user ziglang' }
    $env:CARGO_ZIGBUILD_ZIG_PATH = $zig
}
# Build scripts run on Windows and need the MSVC linker (same setup as check.ps1).
if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    $locator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $studio = & $locator -latest -products '*' -property installationPath
    if (-not $studio) { throw 'Visual Studio C++ build tools are required.' }
    $devShell = Join-Path $studio 'Common7/Tools/Launch-VsDevShell.ps1'
    & $devShell -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
        $installed = Get-ChildItem (Join-Path $studio 'VC/Tools/MSVC') -Directory |
            Where-Object { Test-Path (Join-Path $_.FullName 'bin/HostX64/x64/link.exe') } |
            Sort-Object Name -Descending | Select-Object -First 1
        if (-not $installed) { throw 'No installed x64 MSVC linker found.' }
        $env:PATH = (Join-Path $installed.FullName 'bin/HostX64/x64') + ';' + $env:PATH
        $env:LIB = (Join-Path $installed.FullName 'lib/x64') + ';' + $env:LIB
    }
}
Push-Location $PSScriptRoot
try {
    # Debian 12 (Raspberry Pi OS Bookworm) ships glibc 2.36.
    $artifacts = cargo zigbuild --target "$target.2.36" --workspace --tests --locked --message-format json |
        Where-Object { $_ -like '{*' } | ForEach-Object { $_ | ConvertFrom-Json }
    if ($LASTEXITCODE -ne 0) { throw 'Cross-compilation failed.' }
    $tests = $artifacts | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } |
        ForEach-Object { $_.executable }
    if ($Diagnostic) {
        cargo zigbuild --target "$target.2.36" --release --locked -p shotoshot-app --bin shotoshot
        if ($LASTEXITCODE -ne 0) { throw 'Release cross-compilation failed.' }
        $tests += Join-Path $PSScriptRoot "target/$target/release/shotoshot"
    }
    ssh -o BatchMode=yes $RemoteHost 'rm -rf ~/shotoshot-dev/bin && mkdir -p ~/shotoshot-dev/bin'
    if ($LASTEXITCODE -ne 0) { throw "SSH to $RemoteHost failed." }
    scp -q -o BatchMode=yes @tests "${RemoteHost}:shotoshot-dev/bin/"
    if ($LASTEXITCODE -ne 0) { throw 'Copy failed.' }
    $run = 'cd ~/shotoshot-dev/bin && chmod +x * && status=0; for t in *; do [ "$t" = shotoshot ] && continue; ' +
        'nice -n 19 "./$t" -q >"$t.log" 2>&1 || { status=1; echo "FAILED: $t"; tail -n 30 "$t.log"; }; done; '
    if ($Diagnostic) {
        # The diagnostic opens the real capture window on the remote desktop session, then exits.
        $run += 'export XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-0}; ' +
            'nice -n 10 ./shotoshot --linux-diagnostic ~/shotoshot-dev/out || { status=1; echo "FAILED: diagnostic"; }; ' +
            # Instant capture into a throwaway home, so the real Desktop stays untouched.
            'rm -rf ~/shotoshot-dev/home && mkdir -p ~/shotoshot-dev/home && ' +
            'HOME=~/shotoshot-dev/home nice -n 10 ./shotoshot --instant && ls ~/shotoshot-dev/home/Desktop/*.png >/dev/null ' +
            '&& echo "instant: saved" || { status=1; echo "FAILED: instant"; }; ' +
            # X11 path on the first X server that allows root captures (rootless XWayland
            # refuses them): checks size and format, not content. Informative only.
            'x11=unavailable; for s in /tmp/.X11-unix/X*; do d=":${s##*X}"; ' +
            'DISPLAY=$d WAYLAND_DISPLAY= XDG_SESSION_TYPE=x11 timeout 10 nice -n 10 ./shotoshot --linux-x11-probe 2>/dev/null && { x11=ok; break; }; done; ' +
            '[ $x11 = ok ] || echo "x11: unavailable"; ' +
            # The resident must publish its notification-area icon, then stop.
            # Own lock directory: an installed resident may already hold the session lock.
            'mkdir -p ~/shotoshot-dev/run; XDG_RUNTIME_DIR=~/shotoshot-dev/run DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus ' +
            'nice -n 10 ./shotoshot & pid=$!; sleep 3; ' +
            'if busctl --user list 2>/dev/null | grep -q "StatusNotifierItem-$pid"; then echo "resident: tray icon registered"; ' +
            'else status=1; echo "FAILED: resident tray icon"; fi; kill $pid; wait $pid 2>/dev/null; '
    }
    $run += 'exit $status'
    ssh -o BatchMode=yes $RemoteHost $run
    if ($LASTEXITCODE -ne 0) { throw "Linux checks failed on $RemoteHost." }
    Write-Output "PASS ($($tests.Count) binaries on $RemoteHost)"
} finally { Pop-Location }
