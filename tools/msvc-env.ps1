# Loads the MSVC x64 linker into the current PowerShell session. Dot-source it:
#   . (Join-Path $PSScriptRoot 'tools/msvc-env.ps1')
# Used by check.ps1, check-linux.ps1, packaging/build-linux.ps1 and tools/cargo.ps1, so the
# setup lives in one place. Git's coreutils link.exe (Git Bash PATH) is not a linker: it is
# ignored, which is why the check looks at where link.exe comes from.
function Test-MsvcLinker {
    $link = Get-Command link.exe -ErrorAction SilentlyContinue
    return ($null -ne $link) -and ($link.Source -notlike '*\Git\*')
}
if (-not (Test-MsvcLinker)) {
    $programs = ${env:ProgramFiles(x86)}
    if (-not $programs) { $programs = 'C:\Program Files (x86)' }
    $locator = Join-Path $programs 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $locator)) { throw "vswhere.exe not found at $locator" }
    $studio = & $locator -latest -products '*' -property installationPath
    if (-not $studio) { throw 'Visual Studio C++ build tools are required.' }
    $devShell = Join-Path $studio 'Common7\Tools\Launch-VsDevShell.ps1'
    if (Test-Path $devShell) {
        try { & $devShell -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null } catch {}
    }
    if (-not (Test-MsvcLinker)) {
        $installed = Get-ChildItem (Join-Path $studio 'VC\Tools\MSVC') -Directory |
            Where-Object { Test-Path (Join-Path $_.FullName 'bin\HostX64\x64\link.exe') } |
            Sort-Object Name -Descending | Select-Object -First 1
        if (-not $installed) { throw 'No installed x64 MSVC linker found.' }
        $env:PATH = (Join-Path $installed.FullName 'bin\HostX64\x64') + ';' + $env:PATH
        $env:LIB = (Join-Path $installed.FullName 'lib\x64') + ';' + $env:LIB
    }
}
