# Authoritative check. The full run ends with the Linux tests and diagnostic on the Raspberry Pi
# (check-linux.ps1); -SkipLinux is only for when the Pi is unreachable, and says so in the verdict.
param([switch]$Fast, [switch]$UpdateGolden, [switch]$SkipLinux)
$ErrorActionPreference = 'Stop'
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
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { exit 1 }
    # Every dependency must stay usable under the project license.
    & (Join-Path $PSScriptRoot 'check-licenses.ps1')
    # Maintainer only, the tooling is not published: nothing private may reach the public tree.
    $export = Join-Path $PSScriptRoot 'publish/export-public.ps1'
    if (Test-Path $export) { & $export -ScanOnly }
    cargo clippy --workspace --all-targets --locked -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit 1 }
    # The portable crates and the Linux layer must keep type-checking for Linux.
    if (-not (rustup target list --installed | Select-String -SimpleMatch 'x86_64-unknown-linux-gnu')) {
        throw 'Missing Linux target: rustup target add x86_64-unknown-linux-gnu'
    }
    cargo clippy --workspace --all-targets --locked --target x86_64-unknown-linux-gnu -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit 1 }
    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) { exit 1 }
    if (-not $Fast) {
        # The tests above never read the screen. The ones that need a real display are named
        # real_desktop_* and run here, one at a time; finding none is a failure, not a pass.
        $probes = cargo test -p shotoshot-app --lib --locked -- --ignored --test-threads=1 real_desktop_
        if ($LASTEXITCODE -ne 0 -or -not ($probes -match '^test result: ok\. [1-9]')) {
            $probes
            throw 'Real-desktop tests failed, or none ran.'
        }
        cargo build --workspace --release --locked
        if ($LASTEXITCODE -ne 0) { exit 1 }
        Get-ChildItem (Join-Path $PSScriptRoot 'target') -Filter 'shotoshot-*.png' | Remove-Item
        # The probes stand in for the user without being the user: the real settings and board
        # files must come out unchanged (2026-10-10: a probe rewrote the settings in its format).
        $userState = {
            $root = Join-Path $env:LOCALAPPDATA 'Shotoshot'
            $settings = Join-Path $root 'settings.txt'
            $board = Join-Path $root 'board'
            if (Test-Path -LiteralPath $settings) {
                'settings.txt ' + (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
            }
            if (Test-Path -LiteralPath $board) {
                'board/'
                Get-ChildItem -LiteralPath $board -Recurse -File -Force | Sort-Object FullName |
                    ForEach-Object { '{0} {1}' -f $_.FullName.Substring($board.Length), $_.Length }
            }
        }
        $before = (& $userState) -join "`n"
        foreach ($argument in @('--render-demo', '--diagnostic', '--warm-diagnostic', '--presentation-diagnostic', '--document-diagnostic', '--board-diagnostic')) {
            $probe = Start-Process -FilePath (Join-Path $PSScriptRoot 'target/release/shotoshot.exe') -ArgumentList $argument -WindowStyle Hidden -PassThru
            $null = $probe.Handle
            $finished = $probe.WaitForExit(20000)
            if (-not $finished) { $probe.Kill() }
            if (((& $userState) -join "`n") -ne $before) {
                throw "$argument changed the user's settings or board under $env:LOCALAPPDATA\Shotoshot."
            }
            if (-not $finished) { throw "Timed out: $argument" }
            $probe.Refresh()
            if ($probe.ExitCode -ne 0) { throw "Failed: $argument ($($probe.ExitCode))" }
        }
        # Rendered demo images must match byte for byte; -UpdateGolden records an intended change.
        $golden = Join-Path $PSScriptRoot 'crates/shotoshot-app/tests/render-golden.sha256'
        $rendered = Get-ChildItem (Join-Path $PSScriptRoot 'target') -Filter 'shotoshot-*.png' | Sort-Object Name |
            ForEach-Object { '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash, $_.Name }
        if ($UpdateGolden) {
            [System.IO.File]::WriteAllLines($golden, [string[]]$rendered)
        } elseif (Compare-Object @(Get-Content $golden) @($rendered)) {
            throw 'Rendered demo differs from crates/shotoshot-app/tests/render-golden.sha256. Inspect target/shotoshot-*.png; if intended, rerun with -UpdateGolden.'
        }
        $clipboardProbe = Start-Process -FilePath (Join-Path $PSScriptRoot 'target/release/clipboard-probe.exe') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $PSScriptRoot 'target/clipboard-probe.log') -RedirectStandardError (Join-Path $PSScriptRoot 'target/clipboard-probe-error.log')
        $null = $clipboardProbe.Handle
        if (-not $clipboardProbe.WaitForExit(20000)) { $clipboardProbe.Kill(); throw 'Clipboard probe timed out.' }
        $clipboardProbe.Refresh()
        if ($clipboardProbe.ExitCode -ne 0) { throw 'Clipboard probe failed. See target/clipboard-probe-error.log.' }
        if ($SkipLinux) {
            Write-Output 'PASS (Windows only: Linux checks skipped with -SkipLinux)'
            return
        }
        try {
            & (Join-Path $PSScriptRoot 'check-linux.ps1') -Diagnostic
        } catch {
            throw "Linux checks failed: $_ If the Pi is unreachable, rerun with -SkipLinux and report it."
        }
    }
    Write-Output 'PASS'
} finally { Pop-Location }
