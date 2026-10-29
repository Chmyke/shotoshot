# Cross-compiles Shotoshot for a Linux ARM64 machine and runs its tests there over SSH.
# Nothing is compiled on the remote host; binaries run with low priority in ~/shotoshot-dev.
# Needs: rustup target aarch64-unknown-linux-gnu, cargo-zigbuild, Zig (pip install ziglang),
# and key-based SSH access to the host (default alias "pi").
# -SelfTest runs the remote scripts on this machine with stand-in tests (needs Git's sh on
# Windows): no build, no SSH.
param([string]$RemoteHost = 'pi', [switch]$Diagnostic, [switch]$SelfTest)
$ErrorActionPreference = 'Stop'
$target = 'aarch64-unknown-linux-gnu'

# Each run works in its own remote folder, removed at the end even after a failure: runs from
# parallel sessions used to share ~/shotoshot-dev/bin, and each one's `for t in *` executed the
# logs of the other (2026-10-22). Prints the new folder.
$setupScript = 'mkdir -p "$HOME/shotoshot-dev" && ' +
    # Folders of runs whose connection dropped before their cleanup.
    'find "$HOME/shotoshot-dev" -maxdepth 1 -name "run-*" -mmin +720 -exec rm -rf {} + ; ' +
    'd=$(mktemp -d "$HOME/shotoshot-dev/run-XXXXXX") && mkdir -p "$d/bin" "$d/logs" "$d/package/icons" && echo "$d"'

# Runs exactly the named binaries copied into $Dir/bin, never a glob of the folder; logs go to
# $Dir/logs, out of the way of the binaries.
function Get-RunScript([string]$Dir, [string[]]$Tests, [switch]$Diagnostic) {
    if ($Dir -notmatch '^/[^''\r\n]+$') { throw "Unexpected remote folder: $Dir" }
    foreach ($name in $Tests) {
        if ($name -notmatch '^[A-Za-z0-9._-]+$') { throw "Unexpected test binary name: $name" }
    }
    $script = "d='$Dir'; cd `"`$d/bin`" || exit 1; status=0; for t in $($Tests -join ' '); do " +
        'chmod +x "./$t"; ' +
        'nice -n 19 "./$t" -q >"$d/logs/$t.log" 2>&1 || { status=1; echo "FAILED: $t"; tail -n 30 "$d/logs/$t.log"; }; done; '
    if ($Diagnostic) {
        # The diagnostic opens the real capture window on the remote desktop session, then exits.
        $script += 'chmod +x ./shotoshot; export XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-0}; ' +
            'nice -n 10 ./shotoshot --linux-diagnostic "$d/out" || { status=1; echo "FAILED: diagnostic"; }; ' +
            # Instant capture into a throwaway home, so the real Desktop stays untouched.
            'mkdir -p "$d/home" && HOME="$d/home" nice -n 10 ./shotoshot --instant && ls "$d"/home/Desktop/*.png >/dev/null ' +
            '&& echo "instant: saved" || { status=1; echo "FAILED: instant"; }; ' +
            # The install script declares the .shoto file type and the removal takes it back,
            # in another throwaway home: the script prints its own verdict.
            'cp shotoshot "$d/package/" && sh "$d/package/test-linux-install.sh" "$d/package" "$d/install-home" || status=1; ' +
            # X11 path on the first X server that allows root captures (rootless XWayland
            # refuses them): checks size and format, not content. Informative only.
            'x11=unavailable; for s in /tmp/.X11-unix/X*; do x="${s##*X}"; ' +
            'DISPLAY=":$x" WAYLAND_DISPLAY= XDG_SESSION_TYPE=x11 timeout 10 nice -n 10 ./shotoshot --linux-x11-probe 2>/dev/null && { x11=ok; break; }; done; ' +
            '[ $x11 = ok ] || echo "x11: unavailable"; ' +
            # labwc's magnifier, switched as the menu does, in the real session: its files are
            # copied first and must come back byte for byte (put back and reported otherwise).
            'pid=$(pgrep -u "$(id -u)" -x labwc | head -n 1); c="${XDG_CONFIG_HOME:-$HOME/.config}/labwc"; ' +
            'if [ -n "$pid" ]; then mkdir -p "$d/labwc"; for f in rc.xml themerc-override; do [ ! -e "$c/$f" ] || cp -p "$c/$f" "$d/labwc/$f"; done; ' +
            'LABWC_PID=$pid nice -n 10 ./shotoshot --linux-magnifier-probe || { status=1; echo "FAILED: magnifier"; }; changed=; ' +
            'for f in rc.xml themerc-override; do if [ -e "$d/labwc/$f" ]; then cmp -s "$d/labwc/$f" "$c/$f" || { changed=1; cp -p "$d/labwc/$f" "$c/$f"; }; ' +
            'elif [ -e "$c/$f" ]; then changed=1; rm -f "$c/$f"; fi; done; ' +
            'if [ -n "$changed" ]; then status=1; echo "FAILED: magnifier left labwc files changed (put back)"; LABWC_PID=$pid labwc --reconfigure; fi; ' +
            'else echo "magnifier: no labwc session"; fi; ' +
            # The resident must publish its notification-area icon, then stop.
            # Own lock directory: an installed resident may already hold the session lock.
            'mkdir -p "$d/xdg"; XDG_RUNTIME_DIR="$d/xdg" DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus ' +
            'nice -n 10 ./shotoshot & pid=$!; sleep 3; ' +
            'if busctl --user list 2>/dev/null | grep -q "StatusNotifierItem-$pid"; then echo "resident: tray icon registered"; ' +
            'else status=1; echo "FAILED: resident tray icon"; fi; kill $pid; wait $pid 2>/dev/null; '
    }
    $script + 'exit $status'
}

function Get-CleanupScript([string]$Dir) {
    if ($Dir -notmatch '^/[^''\r\n]+/shotoshot-dev/run-[A-Za-z0-9]+$') { throw "Unexpected remote folder: $Dir" }
    "rm -rf '$Dir'"
}

# The script travels as base64: Windows PowerShell drops double quotes from native arguments.
function Invoke-Remote([string]$Script) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($Script))
    ssh -o BatchMode=yes $RemoteHost "echo $encoded | base64 -d | sh"
}

if ($SelfTest) {
    $sh = '/bin/sh'
    if ($env:OS -eq 'Windows_NT') {
        $git = Get-Command git -ErrorAction SilentlyContinue
        if (-not $git) { throw 'Self-test needs Git for Windows (its sh).' }
        $sh = Join-Path (Split-Path (Split-Path $git.Source)) 'bin/sh.exe'
        if (-not (Test-Path $sh)) { throw "Git's sh not found: $sh" }
    }
    $root = Join-Path $PSScriptRoot 'target/check-linux-selftest'
    if (Test-Path $root) { Remove-Item $root -Recurse -Force }
    $null = New-Item -ItemType Directory (Join-Path $root 'home/ran'), (Join-Path $root 'fakes'), (Join-Path $root 'scripts')
    $utf8 = New-Object System.Text.UTF8Encoding $false
    $count = 0
    # Writes a script with LF endings; /usr/bin first so Git's find wins over Windows' FIND.
    $write = {
        param([string]$Script)
        $script:count++
        $file = Join-Path $root "scripts/$script:count.sh"
        [System.IO.File]::WriteAllText($file, "PATH=/usr/bin:`$PATH`n$Script`n", $utf8)
        $file
    }
    $local = { param([string]$Script) & $sh (& $write $Script) }
    # A stand-in test marks each execution with its name and run folder, then passes or fails.
    $fake = { param([string]$Name, [int]$Code)
        $body = "#!/bin/sh`nsleep 1`n: > `"`$HOME/ran/$Name-`$(basename `"`$(dirname `"`$PWD`")`")`"`nexit $Code`n"
        [System.IO.File]::WriteAllText((Join-Path $root "fakes/$Name"), $body, $utf8)
    }
    & $fake 'pass_a' 0
    & $fake 'pass_b' 0
    & $fake 'stray' 0
    & $fake 'fails' 3
    $savedHome = $env:HOME
    $env:HOME = Join-Path $root 'home'
    try {
        # A forgotten folder from a dropped run goes; a recent one stays.
        & $local 'mkdir -p "$HOME/shotoshot-dev/run-old" "$HOME/shotoshot-dev/run-new" && touch -d "2 days ago" "$HOME/shotoshot-dev/run-old"'
        $runs = 1..3 | ForEach-Object {
            $dir = @(& $local $setupScript)[-1]
            if ($LASTEXITCODE -ne 0 -or -not $dir) { throw 'Self-test: setup failed.' }
            & $local "cp `"`$HOME`"/../fakes/* '$dir/bin/'"
            $dir
        }
        if (@($runs | Select-Object -Unique).Count -ne 3) { throw "Self-test: runs share a folder: $runs" }
        $left = @(Get-ChildItem (Join-Path $root 'home/shotoshot-dev') -Directory | ForEach-Object Name)
        if ($left -contains 'run-old' -or $left -notcontains 'run-new') { throw "Self-test: stale folders not pruned as expected: $left" }
        # Two passing runs at once, as two sessions would; "stray" sits in bin but is not listed.
        $parallel = $runs[0..1] | ForEach-Object {
            $file = & $write (Get-RunScript $_ @('pass_a', 'pass_b'))
            Start-Process -FilePath $sh -ArgumentList "`"$file`"" -NoNewWindow -PassThru
        }
        $parallel | ForEach-Object { $null = $_.Handle }
        $parallel | ForEach-Object { $_.WaitForExit() }
        $codes = $parallel | ForEach-Object { $_.ExitCode }
        if ($codes | Where-Object { $_ -ne 0 }) { throw "Self-test: a parallel run failed ($codes)." }
        $failed = @(& $local (Get-RunScript $runs[2] @('pass_a', 'fails')))
        if ($LASTEXITCODE -eq 0 -or -not ($failed -match '^FAILED: fails$')) { throw "Self-test: a failing test went unreported: $failed" }
        $ran = @(Get-ChildItem (Join-Path $root 'home/ran') | ForEach-Object { $_.Name -replace '-run-.*$' } | Sort-Object)
        if (($ran -join ',') -ne 'fails,pass_a,pass_a,pass_a,pass_b,pass_b') { throw "Self-test: unexpected executions: $ran" }
        if (Get-ChildItem $root -Recurse -Filter '*.log.log') { throw 'Self-test: a log was executed as a test.' }
        & $sh -n (& $write (Get-RunScript $runs[0] @('pass_a') -Diagnostic))
        if ($LASTEXITCODE -ne 0) { throw 'Self-test: the diagnostic script does not parse.' }
        $runs | ForEach-Object { & $local (Get-CleanupScript $_) }
        $left = @(Get-ChildItem (Join-Path $root 'home/shotoshot-dev') -Directory | ForEach-Object Name)
        if (($left -join ',') -ne 'run-new') { throw "Self-test: run folders left behind: $left" }
    } finally { $env:HOME = $savedHome }
    Write-Output 'PASS (check-linux self-test)'
    return
}

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
    $tests = @($artifacts | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } |
        ForEach-Object { $_.executable })
    $names = @($tests | ForEach-Object { Split-Path $_ -Leaf })
    if (@($names | Select-Object -Unique).Count -ne $names.Count) { throw "Two test binaries share a name: $names" }
    $copied = $tests
    if ($Diagnostic) {
        cargo zigbuild --target "$target.2.36" --release --locked -p shotoshot-app --bin shotoshot
        if ($LASTEXITCODE -ne 0) { throw 'Release cross-compilation failed.' }
        $copied += Join-Path $PSScriptRoot "target/$target/release/shotoshot"
    }
    $dir = @(Invoke-Remote $setupScript)[-1]
    if ($LASTEXITCODE -ne 0 -or -not $dir) { throw "SSH to $RemoteHost failed." }
    try {
        scp -q -o BatchMode=yes @copied "${RemoteHost}:$dir/bin/"
        if ($LASTEXITCODE -ne 0) { throw 'Copy failed.' }
        if ($Diagnostic) {
            # The package as build-linux.ps1 lays it out, for the install check.
            $scripts = 'packaging/linux/install.sh', 'packaging/linux/uninstall.sh', 'packaging/test-linux-install.sh'
            scp -q -o BatchMode=yes @scripts "${RemoteHost}:$dir/package/"
            if ($LASTEXITCODE -ne 0) { throw 'Copy failed.' }
            $icons = 32, 48, 256 | ForEach-Object { "crates/shotoshot-app/assets/shotoshot-$_.png" }
            scp -q -o BatchMode=yes @icons "${RemoteHost}:$dir/package/icons/"
            if ($LASTEXITCODE -ne 0) { throw 'Copy failed.' }
        }
        Invoke-Remote (Get-RunScript $dir $names -Diagnostic:$Diagnostic)
        if ($LASTEXITCODE -ne 0) { throw "Linux checks failed on $RemoteHost." }
    } finally {
        Invoke-Remote (Get-CleanupScript $dir)
    }
    Write-Output "PASS ($($copied.Count) binaries on $RemoteHost)"
} finally { Pop-Location }
