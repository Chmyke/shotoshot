# Fails when a crate of the dependency tree (every target, build and dev included) cannot be used
# in a GPL-3.0 program, or when a workspace crate does not declare the project license.
$ErrorActionPreference = 'Stop'
$project = 'GPL-3.0-only'
# Reviewed as compatible with GPL-3.0. Adding an identifier is a decision, not a formality.
$accepted = @('MIT', 'Apache-2.0', 'Apache-2.0 WITH LLVM-exception', 'BSD-2-Clause', 'BSD-3-Clause',
    'ISC', 'Zlib', '0BSD', 'Unlicense', 'BSL-1.0', 'Unicode-3.0')

# SPDX as crates.io writes it: WITH binds tighter than AND, AND tighter than OR; '/' is a legacy OR.
function Test-License([string]$expression) {
    $script:tokens = @(($expression -replace '/', ' OR ' -replace '([()])', ' $1 ') -split '\s+' | Where-Object { $_ })
    $script:at = 0
    (Read-Or) -and $script:at -eq $script:tokens.Count
}
function Read-Or {
    $usable = Read-And
    while ($script:tokens[$script:at] -eq 'OR') { $script:at++; $other = Read-And; $usable = $usable -or $other }
    $usable
}
function Read-And {
    $usable = Read-Term
    while ($script:tokens[$script:at] -eq 'AND') { $script:at++; $other = Read-Term; $usable = $usable -and $other }
    $usable
}
function Read-Term {
    $token = $script:tokens[$script:at]
    $script:at++
    if ($token -eq '(') {
        $usable = Read-Or
        $closed = $script:tokens[$script:at] -eq ')'
        $script:at++
        return $closed -and $usable
    }
    if ($script:tokens[$script:at] -eq 'WITH') {
        $token = "$token WITH $($script:tokens[$script:at + 1])"
        $script:at += 2
    }
    $accepted -contains $token
}

# A check that cannot refuse checks nothing: prove the parser on known cases at every run.
$probes = @{
    'MIT' = $true; 'Unlicense/MIT' = $true; 'Apache-2.0 OR GPL-2.0-only' = $true
    '(MIT OR Apache-2.0) AND Unicode-3.0' = $true; 'Apache-2.0 WITH LLVM-exception OR MIT' = $true
    '' = $false; 'GPL-2.0-only' = $false; 'MIT AND GPL-2.0-only' = $false; 'MIT GPL-2.0-only' = $false
    '(MIT OR GPL-2.0-only' = $false; 'MIT WITH Unreviewed-exception' = $false
}
foreach ($probe in $probes.GetEnumerator()) {
    if ((Test-License $probe.Key) -ne $probe.Value) { throw "License parser self-test failed on '$($probe.Key)'." }
}

Push-Location $PSScriptRoot
try {
    # Plain text whatever the environment: CARGO_TERM_COLOR=always (GitHub Actions) colours the (*)
    # marker, which then no longer matches below and refuses every repeated crate.
    $tree = cargo tree --workspace --target all --prefix none --format '{p}|{l}' --locked --color never
    if ($LASTEXITCODE -ne 0) { throw 'cargo tree failed.' }
    # cargo separates the workspace members with blank lines and marks repeated crates with (*).
    $refused = foreach ($line in ($tree -replace ' \(\*\)$', '' | Where-Object { $_ } | Sort-Object -Unique)) {
        $package, $license = $line -split '\|', 2
        if ($package -like 'shotoshot-*') {
            if ($license -ne $project) { "$package declares '$license' instead of $project" }
        } elseif (-not (Test-License $license)) { "$package ($license)" }
    }
    if ($refused) { throw "Licenses refused for a $project program (see check-licenses.ps1):`n  $($refused -join "`n  ")" }
    # Every package ships packaging/ABOUT.txt: it must name this license and where the source code is.
    $about = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'packaging/ABOUT.txt'))
    foreach ($expected in $project, 'https://github.com/Chmyke/shotoshot') {
        if (-not $about.Contains($expected)) { throw "packaging/ABOUT.txt does not mention $expected." }
    }
} finally { Pop-Location }
