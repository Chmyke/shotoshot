# End-to-end installer test, no human needed. Two scenarios:
# 1. Fresh: silent install, checks, launch, silent uninstall (which must close the running
#    app), checks that nothing is left.
# 2. Upgrade from the former name (before 0.2.0, "Shot Shot"): old setup installed with a
#    desktop shortcut, a running shotshot.exe and its Run value; the new setup must close it,
#    remove the old folder and shortcuts, keep "start with Windows" under the new name, and
#    the uninstaller must leave nothing. Uses -OldSetup, else the newest
#    out/Shot-Shot-Setup-*.exe; skipped (and said) when none exists.
# Refuses to run if Shotoshot or Shot Shot is already installed for this user, so a real
# installation is never touched. A development build running from target/ is
# left alone; while it runs, the launched copy shows "already open" for ~3 s.
param([string]$Setup, [string]$OldSetup)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$iss = Join-Path $PSScriptRoot 'shotoshot.iss'
$issText = [System.IO.File]::ReadAllText($iss, [System.Text.Encoding]::UTF8)
$version = [regex]::Match($issText, '(?m)^#define AppVersion "([^"]+)"').Groups[1].Value
$appId = [regex]::Match($issText, '(?m)^AppId=\{\{([0-9A-F-]+)\}').Groups[1].Value
if (-not $version -or -not $appId) { throw 'Cannot read AppVersion or AppId from shotoshot.iss.' }
if (-not $Setup) { $Setup = Join-Path $PSScriptRoot "out/Shotoshot-Setup-$version.exe" }
if (-not (Test-Path $Setup)) { throw "Missing $Setup. Run build-installer.ps1 first." }
if (-not $OldSetup) {
    $OldSetup = Get-ChildItem (Join-Path $PSScriptRoot 'out') -Filter 'Shot-Shot-Setup-*.exe' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
}

$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{$appId}_is1"
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\Shotoshot'
$installedExe = Join-Path $installDir 'shotoshot.exe'
$startMenuLink = Join-Path ([Environment]::GetFolderPath('Programs')) 'Shotoshot.lnk'
$desktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Shotoshot.lnk'
$oldDir = Join-Path $env:LOCALAPPDATA 'Programs\Shot Shot'
$oldExe = Join-Path $oldDir 'shotshot.exe'
$oldStartMenuLink = Join-Path ([Environment]::GetFolderPath('Programs')) 'Shot Shot.lnk'
$oldDesktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Shot Shot.lnk'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$extensionKey = 'HKCU:\Software\Classes\.shoto'
$documentKey = 'HKCU:\Software\Classes\Shotoshot.Document'
$sourceHash = (Get-FileHash -Algorithm SHA256 (Join-Path $root 'target/release/shotoshot.exe')).Hash
$log = Join-Path $env:TEMP "shotoshot-installer-test-$PID.log"
$uninstallLog = Join-Path $env:TEMP "shotoshot-uninstaller-test-$PID.log"
$upgradeLog = Join-Path $env:TEMP "shotoshot-upgrade-test-$PID.log"
$upgradeUninstallLog = Join-Path $env:TEMP "shotoshot-upgrade-uninstaller-test-$PID.log"
$failures = 0

function Check([bool]$ok, [string]$what) {
    if ($ok) { Write-Output "PASS  $what" }
    else { Write-Output "FAIL  $what"; $script:failures++ }
}
function Installed-Processes {
    @(Get-Process shotoshot -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and ($_.Path -ieq $installedExe) })
}
function Run-Value([string]$name = 'Shotoshot') {
    $value = Get-ItemProperty -Path $runKey -Name $name -ErrorAction SilentlyContinue
    if ($value) { return $value.$name }
    return $null
}
function Wait-Until([scriptblock]$condition, [int]$seconds) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        if (& $condition) { return $true }
        Start-Sleep -Milliseconds 200
    }
    return [bool](& $condition)
}
function Log-Contains([string]$path, [string]$text) {
    Wait-Until { try { [System.IO.File]::ReadAllText($path).Contains($text) } catch { $false } } 10
}
function Run-Setup([string]$path, [string[]]$extra) {
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') + $extra
    (Start-Process -FilePath $path -ArgumentList $arguments -Wait -PassThru).ExitCode
}
function Uninstall-Current([string]$logPath) {
    $code = Run-Setup (Join-Path $installDir 'unins000.exe') @("/LOG=`"$logPath`"")
    Check ($code -eq 0) "silent uninstall exit code 0 (got $code)"
    # The uninstaller relaunches itself from %TEMP%, so wait for the real work.
    $gone = Wait-Until { -not (Test-Path $installDir) -and -not (Test-Path $uninstallKey) } 30
    Check $gone 'install directory and uninstall entry removed'
}

# Guard: never overwrite a real installation, old or new name.
foreach ($existing in @($uninstallKey, $installDir, $startMenuLink, $desktopLink, $oldDir, $oldStartMenuLink, $oldDesktopLink)) {
    if (Test-Path $existing) {
        Write-Output "ABORT Shotoshot already present ($existing). Uninstall it first; nothing was changed."
        exit 2
    }
}
$others = @(Get-Process shotoshot, shotshot -ErrorAction SilentlyContinue)
if ($others.Count -gt 0) {
    Write-Output "INFO  Other Shotoshot processes left untouched: $(($others | ForEach-Object { "$($_.Id) $($_.Path)" }) -join '; ')"
}
# Decoy: a harmless process also named shotoshot.exe, outside the install dir.
# The uninstaller must not close it (closing is scoped by full path).
$decoyDir = Join-Path $env:TEMP "shotoshot-decoy-$PID"
$null = New-Item -ItemType Directory -Force $decoyDir
Copy-Item (Join-Path $env:SystemRoot 'System32\PING.EXE') (Join-Path $decoyDir 'shotoshot.exe')
$decoy = Start-Process -FilePath (Join-Path $decoyDir 'shotoshot.exe') -ArgumentList '-n', '300', '127.0.0.1' -WindowStyle Hidden -PassThru
$runBefore = Run-Value
$oldRunBefore = Run-Value 'Shotshot'
$runWasSet = $false
$oldApp = $null

try {
    Write-Output "INFO  Setup: $Setup"
    $code = Run-Setup $Setup @("/LOG=`"$log`"")
    Check ($code -eq 0) "silent install exit code 0 (got $code)"
    Check (Test-Path $installedExe) "installed exe exists: $installedExe"
    if (Test-Path $installedExe) {
        Check ((Get-FileHash -Algorithm SHA256 $installedExe).Hash -eq $sourceHash) "installed exe SHA256 = target/release ($sourceHash)"
    }
    Check (Test-Path (Join-Path $installDir 'THIRD-PARTY-NOTICES.txt')) 'third-party notices installed'
    foreach ($legal in @(@{ Name = 'LICENSE.txt'; Source = (Join-Path $root 'LICENSE') }, @{ Name = 'ABOUT.txt'; Source = (Join-Path $PSScriptRoot 'ABOUT.txt') })) {
        $installed = Join-Path $installDir $legal.Name
        Check ((Test-Path $installed) -and (Get-FileHash $installed).Hash -eq (Get-FileHash $legal.Source).Hash) "$($legal.Name) installed, same as the repository's"
    }
    $entry = Get-ItemProperty -Path $uninstallKey -ErrorAction SilentlyContinue
    Check ($null -ne $entry) "uninstall entry exists: $uninstallKey"
    if ($entry) {
        Check ($entry.DisplayName -eq 'Shotoshot' -and $entry.DisplayVersion -eq $version) "uninstall entry names Shotoshot $version"
    }
    Check (Test-Path $startMenuLink) "Start menu shortcut exists: $startMenuLink"
    if (Test-Path $startMenuLink) {
        $target = (New-Object -ComObject WScript.Shell).CreateShortcut($startMenuLink).TargetPath
        Check ($target -ieq $installedExe) "Start menu shortcut targets the installed exe"
    }
    Check (-not (Test-Path $desktopLink)) 'no desktop shortcut by default'
    $extension = Get-ItemProperty -Path $extensionKey -ErrorAction SilentlyContinue
    Check ($extension -and $extension.'(default)' -eq 'Shotoshot.Document' -and $extension.PerceivedType -eq 'image' -and
        $extension.'Content Type' -eq 'image/png') '.shoto is a Shotoshot document and an image'
    $command = (Get-ItemProperty -Path "$documentKey\shell\open\command" -ErrorAction SilentlyContinue).'(default)'
    Check ($command -eq "`"$installedExe`" `"%1`"") ".shoto opens with the installed exe (got $command)"
    $thumbnailProvider = (Get-ItemProperty -Path "$extensionKey\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}" -ErrorAction SilentlyContinue).'(default)'
    Check ($thumbnailProvider -eq '{C7657C4A-9F68-40fa-A4DF-96BC08EB3551}') ".shoto thumbnails come from Windows' photo thumbnail provider (got $thumbnailProvider)"
    $overlay = (Get-ItemProperty -Path $documentKey -ErrorAction SilentlyContinue).TypeOverlay
    Check ($overlay -eq "$installedExe,0") ".shoto thumbnails are stamped with the Shotoshot icon (got $overlay)"
    $thumbnail = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'test-thumbnail.ps1') | Out-String
    Check ($LASTEXITCODE -eq 0) "Explorer gets the image as the .shoto thumbnail ($($thumbnail.Trim()))"
    Check ((Installed-Processes).Count -eq 0) 'silent install does not launch the app'

    # The uninstaller must close a running installed copy and clean its autostart value.
    $null = Start-Process -FilePath $installedExe -PassThru
    Check (Wait-Until { (Installed-Processes).Count -gt 0 } 10) 'installed app launched for the uninstall test'
    if ($null -eq $runBefore) {
        Set-ItemProperty -Path $runKey -Name Shotoshot -Value "`"$installedExe`""
        $runWasSet = $true
    }

    Start-Sleep -Seconds 1
    Check ((Installed-Processes).Count -gt 0) 'installed app still running before uninstall'

    Uninstall-Current $uninstallLog
    # The log may stay open for a moment while the uninstaller exits.
    $closedByUninstaller = Log-Contains $uninstallLog 'Closing running Shotoshot processes'
    Check ($closedByUninstaller -and (Installed-Processes).Count -eq 0) 'running app was closed by the uninstaller'
    Check (-not (Test-Path $startMenuLink)) 'Start menu shortcut removed'
    Check (-not (Test-Path $extensionKey) -and -not (Test-Path $documentKey)) '.shoto association removed'
    if ($runWasSet) {
        Check ($null -eq (Run-Value)) 'autostart value pointing to the installation removed'
        $runWasSet = $false
    } else {
        Check ((Run-Value) -eq $runBefore) 'foreign autostart value preserved'
    }

    if (-not $OldSetup) {
        Write-Output 'SKIP  upgrade from "Shot Shot": no out/Shot-Shot-Setup-*.exe and no -OldSetup'
    } elseif ($null -ne $oldRunBefore -or $null -ne $runBefore) {
        Write-Output 'SKIP  upgrade from "Shot Shot": a foreign autostart value exists and must not be touched'
    } else {
        Write-Output "INFO  Old setup: $OldSetup"
        $code = Run-Setup $OldSetup @('/TASKS="desktopicon"')
        Check ($code -eq 0 -and (Test-Path $oldExe) -and (Test-Path $oldStartMenuLink) -and (Test-Path $oldDesktopLink)) 'old "Shot Shot" installed with both shortcuts'
        # A harmless stand-in for the running old resident, at the old exe's exact path.
        Copy-Item (Join-Path $env:SystemRoot 'System32\PING.EXE') $oldExe -Force
        $oldApp = Start-Process -FilePath $oldExe -ArgumentList '-n', '300', '127.0.0.1' -WindowStyle Hidden -PassThru
        Set-ItemProperty -Path $runKey -Name Shotshot -Value "`"$oldExe`""
        $runWasSet = $true

        $code = Run-Setup $Setup @("/LOG=`"$upgradeLog`"")
        Check ($code -eq 0) "upgrade install exit code 0 (got $code)"
        Check ($oldApp.HasExited) 'running old shotshot.exe closed by the upgrade'
        Check (-not (Test-Path $oldDir)) 'old "Shot Shot" folder removed'
        Check (-not (Test-Path $oldStartMenuLink) -and -not (Test-Path $oldDesktopLink)) 'old shortcuts removed'
        Check ((Test-Path $installedExe) -and (Get-FileHash -Algorithm SHA256 $installedExe).Hash -eq $sourceHash) 'upgraded exe = target/release'
        Check (Test-Path $startMenuLink) 'new Start menu shortcut exists'
        Check (Test-Path $desktopLink) 'desktop shortcut kept under the new name'
        $entry = Get-ItemProperty -Path $uninstallKey -ErrorAction SilentlyContinue
        Check ($entry -and $entry.DisplayName -eq 'Shotoshot' -and $entry.DisplayVersion -eq $version -and
            $entry.InstallLocation.TrimEnd('\') -ieq $installDir) 'single uninstall entry now names Shotoshot and its folder'
        Check ($null -eq (Run-Value 'Shotshot')) 'old autostart value removed'
        Check ((Run-Value) -eq "`"$installedExe`"") 'start with Windows kept under the new name'

        Uninstall-Current $upgradeUninstallLog
        Check (-not (Test-Path $startMenuLink) -and -not (Test-Path $desktopLink)) 'shortcuts removed after uninstalling the upgrade'
        Check ($null -eq (Run-Value) -and $null -eq (Run-Value 'Shotshot')) 'both autostart values removed'
        Check (-not (Test-Path $oldDir)) 'nothing left under the old name'
        $runWasSet = $false
    }

    Check (-not $decoy.HasExited) 'shotoshot.exe outside the install dir left running'
    foreach ($other in $others) {
        Check ($null -ne (Get-Process -Id $other.Id -ErrorAction SilentlyContinue)) "pre-existing process $($other.Id) left running"
    }
} finally {
    if (-not $decoy.HasExited) { $decoy.Kill() }
    $decoy.WaitForExit(5000) | Out-Null
    Remove-Item -Recurse -Force $decoyDir -ErrorAction SilentlyContinue
    if ($oldApp -and -not $oldApp.HasExited) { $oldApp.Kill(); $oldApp.WaitForExit(5000) | Out-Null }
    # Leave the machine as found even when a check threw.
    Installed-Processes | Stop-Process -Force -ErrorAction SilentlyContinue
    if ($runWasSet) {
        foreach ($name in @('Shotoshot', 'Shotshot')) {
            $value = Run-Value $name
            if ($value -ieq "`"$installedExe`"" -or $value -ieq "`"$oldExe`"") {
                Remove-ItemProperty -Path $runKey -Name $name -ErrorAction SilentlyContinue
            }
        }
    }
    foreach ($leftover in @((Join-Path $installDir 'unins000.exe'), (Join-Path $oldDir 'unins000.exe'))) {
        if (Test-Path $leftover) {
            Write-Output "INFO  Cleaning up the test installation: $leftover"
            Start-Process -FilePath $leftover -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
        }
    }
}

Write-Output "INFO  Logs: $log ; $uninstallLog ; $upgradeLog ; $upgradeUninstallLog"
if ($failures -gt 0) { Write-Output "FAIL ($failures check(s))"; exit 1 }
Write-Output 'PASS'
