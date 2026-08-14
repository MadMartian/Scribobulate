<#
.SYNOPSIS
    Stages the Windows redistributable tree and builds the installer.

.DESCRIPTION
    Extracted from pipeline.ps1 so the shared step contract can name ONE command for the
    packaging step, the way cmd.linux names build-deb.sh/build-rpm.sh and cmd.macos names
    dmg.sh. The logic is unchanged; only its home is.

    NOT a gate that runs on every edit. Packaging is opt-in (pipeline.ps1 -Package), because
    building an installer adds minutes to a pipeline meant to run after every change.

    VERSION IS NOT DERIVED HERE, deliberately. scribobulate.iss reads it straight out of
    Cargo.toml (`ReadIni(... "package", "version" ...)`) and `#error`s if it comes back
    empty rather than falling back to a hardcoded string. Introducing a version variable in
    this script would create exactly the second copy that arrangement exists to prevent, so
    this script never mentions a version at all.

.PARAMETER GtkPrefix
    The gvsbuild install prefix, forwarded to stage.ps1. Defaults to %SCRIB_GTK_PREFIX%
    when set, else gvsbuild's own default -- the same resolution order build.bat and
    pipeline.ps1 use, so no two entry points can build against different GTKs on one machine.

.PARAMETER StageDir
    Where the redistributable tree is assembled. Defaults to <repo>\build\stage\Scribobulate.

.EXAMPLE
    powershell -NoProfile -File packaging/windows/package.ps1
#>
[CmdletBinding()]
param(
    [string] $GtkPrefix = $(if ($env:SCRIB_GTK_PREFIX) { $env:SCRIB_GTK_PREFIX } else { 'C:\gtk-build\gtk\x64\release' }),
    [string] $StageDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ASCII-only string literals: a .ps1 without a BOM is decoded as the ANSI codepage by
# Windows PowerShell 5.1, and a UTF-8 em dash inside a string literal becomes a curly quote
# that PowerShell honours as a string DELIMITER -- the file then stops parsing and the error
# points somewhere unrelated. So `--`, never an em dash.

$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not $StageDir) { $StageDir = Join-Path $repo 'build\stage\Scribobulate' }

# Locate Inno Setup's compiler. Probed rather than hardcoded because the two supported
# install scopes put it in different places and BOTH are normal: `winget install --id
# JRSoftware.InnoSetup` defaults to USER scope (%LOCALAPPDATA%\Programs), which is what
# packaging/windows/README.md documents, while a machine-scope install lands under Program
# Files. Hardcoding either one makes the other look like "Inno Setup is not installed".
function Find-Iscc {
    # `-First 1` is load-bearing, not tidiness. Get-Command returns EVERY match on PATH,
    # so on a machine carrying two Inno installs `$onPath.Source` is an ARRAY, and a
    # returned array interpolates space-joined into the command line built below --
    # producing `ISCC.exe C:\...\ISCC.exe C:\...\ISCC.exe` and a failure whose message
    # names Inno Setup twice and explains nothing. MEASURED on a GitHub windows-latest
    # runner, which ships ISCC via Chocolatey on PATH: a second, user-scope install put a
    # sibling ahead of it and step 10 died with both paths concatenated. Two installs is
    # an ordinary state for a developer box too; the first match is the one PATH order
    # already chose.
    $onPath = Get-Command iscc -CommandType Application -ErrorAction SilentlyContinue |
              Select-Object -First 1
    if ($onPath) { return $onPath.Source }
    # Roots checked for emptiness BEFORE Join-Path: with $ErrorActionPreference = 'Stop',
    # Join-Path on an unset root is a terminating error, so a probe written the obvious way
    # would abort the run instead of moving on to the next candidate.
    $roots = @(
        (Join-Path "$env:LOCALAPPDATA" 'Programs'),
        ${env:ProgramFiles(x86)},
        $env:ProgramFiles
    )
    foreach ($r in $roots) {
        if (-not $r) { continue }
        $c = Join-Path $r 'Inno Setup 6\ISCC.exe'
        if (Test-Path $c) { return $c }
    }
    return $null
}

# Locate Microsoft's C++ redistributable installer. MOVED HERE FROM stage.ps1, which
# used to copy vcruntime140.dll app-local out of this same directory -- doing that made
# us a redistributor of Microsoft's Distributable Code. The installer now ships
# Microsoft's own redistributable instead, so the discovery belongs to the packaging
# step rather than the staging step, and the staged tree contains no CRT at all.
#
# Every path component is DISCOVERED. The version directory differs per machine and per
# VS update, and the toolset directory differs per Visual Studio generation -- CI runs
# VS 18 Enterprise, a developer box runs VS 2022 Community. stage.ps1 records what
# hardcoding one component while discovering another already cost once.
function Find-VCRedist {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        throw "vswhere.exe not found at $vswhere -- cannot locate the MSVC redistributable. " +
              "Visual Studio 2017 or later (or Build Tools) is required to build the installer."
    }
    $vsRoot = & $vswhere -latest -products * -property installationPath
    if (-not $vsRoot) { throw "vswhere reported no Visual Studio installation." }

    $verFile = Join-Path $vsRoot 'VC\Auxiliary\Build\Microsoft.VCRedistVersion.default.txt'
    if (-not (Test-Path $verFile)) { throw "VC redist version file not found at $verFile" }
    $ver = (Get-Content $verFile -Raw).Trim()

    $exe = Join-Path $vsRoot "VC\Redist\MSVC\$ver\vc_redist.x64.exe"
    if (-not (Test-Path $exe)) { throw "vc_redist.x64.exe not found at $exe" }
    return $exe
}

Push-Location $repo
try {
    Write-Host "staging to $StageDir" -ForegroundColor Cyan
    # SEEDED, and this is not defensive noise -- it is the bug this script shipped with.
    # $LASTEXITCODE is written ONLY by native commands, so in a fresh `powershell -File`
    # process it does not exist until one has run, and reading a non-existent variable under
    # Set-StrictMode -Version Latest THROWS: "The variable '$LASTEXITCODE' cannot be
    # retrieved because it has not been set." While this logic lived inline in pipeline.ps1
    # the setup phase's `pkgconf` had already populated it, so the check worked by accident
    # of ambient state that its new home does not provide. Measured: staging succeeded (861
    # files, 41.8 MB) and the step then failed on the success path.
    $global:LASTEXITCODE = 0
    # -GtkPrefix FORWARDED, so the caller's prefix is not silently ignored: stage.ps1 has
    # its own default, and without this the tree was staged from gvsbuild's default prefix
    # no matter what was asked for.
    & "$PSScriptRoot\stage.ps1" -GtkPrefix $GtkPrefix -OutDir $StageDir -RepoRoot $repo
    if ($LASTEXITCODE -ne 0) { throw "FAILED: stage the redistributable tree (exit $LASTEXITCODE)" }

    $iscc = Find-Iscc
    if (-not $iscc) {
        # A missing ISCC.exe FAILS rather than warning: packaging was asked for explicitly,
        # and a green run that produced no installer is worse than a red one. The last
        # clause is the useful part on a machine without Inno Setup -- it tells the operator
        # what they already have.
        throw @"
FAILED: build the installer -- Inno Setup 6 not found.
Looked for iscc on PATH, then:
  $env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe   (winget default, user scope)
  ${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe     (machine scope)
Install it with:  winget install --id JRSoftware.InnoSetup
The staged tree at $StageDir is complete; only the installer step is missing.
"@
    }

    Write-Host "Inno Setup: $iscc" -ForegroundColor DarkGray
    # Seeded for the same reason as above. ISCC is native so it will set it, but the check
    # must not depend on that being true of whatever runs here next.
    $global:LASTEXITCODE = 0
    # /DStageDir is passed EXPLICITLY and has no default here. scribobulate.iss opens with
    # `#ifndef StageDir / #error`, so an ISCC invocation without it builds NO installer
    # rather than a wrong one. Do not "helpfully" default it -- that property is the whole
    # protection against shipping an installer assembled from somewhere unintended.
    # Announced for the same reason stage.ps1 announced the toolset directory: the
    # resolved path and the file version are the two facts a future reader wants, and
    # neither is guessable from this source.
    $redist = Find-VCRedist
    Write-Host "MSVC redistributable: $redist" -ForegroundColor DarkGray
    Write-Host ("  vc_redist.x64.exe  {0}" -f (Get-Item $redist).VersionInfo.FileVersion) -ForegroundColor DarkGray

    & $iscc "/DStageDir=$StageDir" "/DRedistFile=$redist" "$PSScriptRoot\scribobulate.iss"
    if ($LASTEXITCODE -ne 0) { throw "FAILED: build the installer (exit $LASTEXITCODE)" }

    Write-Host 'installer built.' -ForegroundColor Green
}
finally {
    Pop-Location
}
