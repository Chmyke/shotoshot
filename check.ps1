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
    # No long dash in text the user sees (rule of 2026-10-21). Maintainer only: tools/ is not
    # published, so the public tree skips it.
    $dashes = Join-Path $PSScriptRoot 'tools/check-dashes.ps1'
    if (Test-Path $dashes) {
        & $dashes
        if ($LASTEXITCODE -ne 0) { exit 1 }
    }
    # Linux is a real build, not a "preview" (Mickael, 2026-10-24): public texts never call it one.
    # Some of these files stay private, so the public tree checks those it has.
    $public = @('README.md', 'packaging/itch/description.html', 'packaging/linux/README.txt',
        'changelog.json') | Where-Object { Test-Path $_ }
    $previews = Select-String -Pattern '(?i)\bpreview\b' -Path $public
    if ($previews) {
        $previews | ForEach-Object { Write-Host "$($_.Path):$($_.LineNumber): $($_.Line.Trim())" }
        throw 'Public text calls Shotoshot or its Linux build a preview.'
    }
    # The remote Linux scripts run only the tests they copied, each run in its own folder.
    & (Join-Path $PSScriptRoot 'check-linux.ps1') -SelfTest
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { exit 1 }
    # Every dependency must stay usable under the project license.
    & (Join-Path $PSScriptRoot 'check-licenses.ps1')
    # Maintainer only, the tooling is not published: nothing private may reach the public tree.
    $export = Join-Path $PSScriptRoot 'publish/export-public.ps1'
    if (Test-Path $export) { & $export -ScanOnly }
    # The changelog is valid, has the current version, and CHANGELOG.md is generated from it.
    $changelog = Join-Path $PSScriptRoot 'publish/changelog.ps1'
    if (Test-Path $changelog) { & $changelog -SelfTest }
    $release = Join-Path $PSScriptRoot 'publish/release-github.ps1'
    if (Test-Path $release) { & $release -SelfTest }
    cargo clippy --workspace --all-targets --locked -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit 1 }
    # The portable crates and the Linux layer must keep type-checking for Linux.
    if (-not (rustup target list --installed | Select-String -SimpleMatch 'x86_64-unknown-linux-gnu')) {
        throw 'Missing Linux target: rustup target add x86_64-unknown-linux-gnu'
    }
    cargo clippy --workspace --all-targets --locked --target x86_64-unknown-linux-gnu -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit 1 }
    # Every test binary runs even after a failure, so one run shows every failing test.
    # None may take the foreground, and so the keyboard, from the user meanwhile.
    & (Join-Path $PSScriptRoot 'check-foreground.ps1') -Under (Join-Path $PSScriptRoot 'target') -Run {
        cargo test --workspace --locked --no-fail-fast
    }
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
        # The probes time the real capture path, so other programs compiling or drawing on the GPU
        # push them past their budget (2026-10-17: first frame at 245-645 ms for 200). Wait until
        # the machine has been calm for 30 seconds, at most 15 minutes; a failure then names the load.
        $load = {
            $cpu = (Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average
            # CIM, not Get-Counter: Get-Counter throws when a process ends while it samples (19
            # samples in 40 on 2026-10-21), and each lost sample used to read as an idle GPU.
            $gpu = [int](Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine |
                Where-Object Name -like '*engtype_3D' | Measure-Object UtilizationPercentage -Sum).Sum
            [pscustomobject]@{ Cpu = $cpu; Gpu = $gpu; Calm = $cpu -lt 25 -and $gpu -lt 15 }
        }
        $deadline = (Get-Date).AddMinutes(15)
        $waitForCalm = {
            $calmSince = $null
            do {
                $machine = & $load
                if (-not $machine.Calm) { $calmSince = $null }
                elseif (-not $calmSince) { $calmSince = Get-Date }
                if ($calmSince -and ((Get-Date) - $calmSince).TotalSeconds -ge 30) { return '' }
                Start-Sleep -Seconds 5
            } while ((Get-Date) -lt $deadline)
            " The machine never stayed calm for 30 s in 15 min (last: CPU $($machine.Cpu) %, GPU $($machine.Gpu) %): rerun when it is."
        }
        $busy = & $waitForCalm
        $before = (& $userState) -join "`n"
        foreach ($argument in @('--render-demo', '--diagnostic', '--warm-diagnostic', '--presentation-diagnostic', '--loupe-diagnostic', '--document-diagnostic', '--board-diagnostic')) {
            # --diagnostic and --loupe-diagnostic hold a time budget. Their verdict counts on a run
            # the machine stayed calm through: other programs often start within their few seconds
            # (2026-10-21: a test of another project at 80 % of the CPU took the first frame from
            # 113 to 194 ms), so a disturbed failure is run again once they settle, within the same
            # 15 minutes. The loupe's figures land in target/loupe-diagnostics.txt.
            $timed = $argument -in @('--diagnostic', '--loupe-diagnostic')
            do {
                $disturbed = $null
                $probe = Start-Process -FilePath (Join-Path $PSScriptRoot 'target/release/shotoshot.exe') -ArgumentList $argument -WindowStyle Hidden -PassThru
                $null = $probe.Handle
                $started = Get-Date
                do {
                    $finished = $probe.WaitForExit(1000)
                    if ($timed -and -not $finished -and -not $disturbed) {
                        $machine = & $load
                        if (-not $machine.Calm) { $disturbed = $machine }
                    }
                } while (-not $finished -and ((Get-Date) - $started).TotalSeconds -lt 20)
                if (-not $finished) { $probe.Kill() }
                if (((& $userState) -join "`n") -ne $before) {
                    throw "$argument changed the user's settings or board under $env:LOCALAPPDATA\Shotoshot."
                }
                if (-not $finished) { throw "Timed out: $argument" }
                $probe.Refresh()
                $retry = $probe.ExitCode -ne 0 -and $disturbed -and -not $busy -and (Get-Date) -lt $deadline
                if ($retry) {
                    Write-Output "$argument failed while disturbed (CPU $($disturbed.Cpu) %, GPU $($disturbed.Gpu) %): run again once calm."
                    $busy = & $waitForCalm
                    $retry = -not $busy
                }
            } while ($retry)
            if ($probe.ExitCode -ne 0) {
                $during = if ($disturbed) { " Disturbed: CPU $($disturbed.Cpu) %, GPU $($disturbed.Gpu) % during the run." } elseif ($timed) { ' The machine stayed calm during the run; figures in target/diagnostics.json.' } else { '' }
                throw "Failed: $argument ($($probe.ExitCode)).$during$busy"
            }
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
