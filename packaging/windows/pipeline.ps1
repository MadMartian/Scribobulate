<#
.SYNOPSIS
    Runs POLICY.md's build pipeline locally on Windows, and optionally stages the
    runtime and builds the installer.

.DESCRIPTION
    This is a LOCAL developer script. It is deliberately not CI: Scribobulate has
    no GitHub Actions workflow, and adding one is out of scope for the Windows
    port (see packaging/windows/README.md § "Why this is a script, not CI").

    It configures the gvsbuild GTK environment, then runs the POLICY § "Build
    pipeline" gates that are meaningful on this platform, stopping at the first
    failure.

    Steps 1-5 mirror POLICY exactly. Step 6 (the coverage ratchet) is NOT run and
    is not expected to be: it is Linux-canonical. `atomic_io.rs` is in scope but
    carries substantial unix-only code and tests, and `workaround.rs` is compiled
    out entirely on Windows, so both numerator and denominator shift and a
    Windows figure can legitimately fall below FLOOR with no real regression.
    Never lower the Linux floor to make a Windows run pass.

.PARAMETER Prefix
    The gvsbuild install prefix. Defaults to %SCRIB_GTK_PREFIX% when set, else
    gvsbuild's own default -- the same resolution order build.bat uses, so the two
    entry points cannot build against different GTKs on the same machine.

.PARAMETER SkipIntegration
    Skip step 5 (the GTK integration suite). It opens real windows, so it is
    unsuitable for an unattended run.

.PARAMETER Package
    After the gates pass, stage the redistributable tree and build the installer.
    Requires Inno Setup 6; a missing ISCC.exe FAILS this run rather than warning,
    because -Package was asked for explicitly and a green pipeline that produced no
    installer is worse than a red one.

.EXAMPLE
    .\packaging\windows\pipeline.ps1
    .\packaging\windows\pipeline.ps1 -SkipIntegration
    .\packaging\windows\pipeline.ps1 -Package
#>
[CmdletBinding()]
param(
    [string] $Prefix = $(if ($env:SCRIB_GTK_PREFIX) { $env:SCRIB_GTK_PREFIX } else { 'C:\gtk-build\gtk\x64\release' }),
    [switch] $SkipIntegration,
    [switch] $Package
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# Tests currently known to fail or hang on Windows, skipped BY NAME so nothing
# else can hide behind the carve-out. Remove an entry the moment it passes;
# a skip with no expiry is how a suite rots. Each is tracked in sdd/ISSUES.md --
# look it up by test name there rather than by an ID quoted here, since issue IDs
# exist in order to be deleted and a pointer to one rots by design.
#
# EMPTY, and that is the goal state, not a reason to delete the machinery: the
# whole GTK suite passes on Windows. Keeping the list means the next carve-out is
# one line rather than a re-derivation, and it is what lets step 5 say "no skips"
# and have that mean something.
$skippedTests = @()

function Invoke-Step {
    param([string] $Name, [scriptblock] $Body)
    Write-Host ''
    Write-Host "=== $Name ===" -ForegroundColor Cyan
    # Cleared first: $LASTEXITCODE is only written by NATIVE commands, so a step
    # whose body is a PowerShell script would otherwise be judged on the previous
    # step's exit code. Such a body reports failure by throwing (`throw` is
    # terminating whatever the preference is), and this keeps the native check
    # below from passing a stale 0 off as its own result.
    $global:LASTEXITCODE = 0

    # The body runs under 'Continue', NOT the script-level 'Stop'. This is a
    # correctness fix, not a relaxation, and it makes this script immune to HOW it
    # was invoked.
    #
    # Under 'Stop', Windows PowerShell 5.1 wraps what a native executable writes to
    # STDERR in a NativeCommandError ErrorRecord, and 'Stop' makes that terminating.
    # `cargo` writes its ordinary progress to stderr BY DESIGN, so the fatal record
    # is a success message. Measured here, verbatim, running step 2 pre-fix:
    #
    #     THREW: NativeCommandError
    #     message: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
    #
    # The gate failed because the build had succeeded.
    #
    # The trigger is narrower than "stderr is captured", and the difference matters
    # to anyone re-testing this. Measured on PS 5.1, native command exiting 0 while
    # writing one stderr line:
    #     bare call, stderr to console or an inherited pipe .... does NOT throw
    #     output assigned to a variable, no redirection ....... does NOT throw
    #     `cmd 2>&1` .......................................... THROWS
    #     `cmd 2>&1 | Out-String` ............................. THROWS
    # So the redirection operator into the PowerShell pipeline is what does it. A
    # plain `.\pipeline.ps1` was never affected, on any host tested -- which is
    # exactly why this survived being run by hand many times. The exposure is a
    # CALLER capturing a build log, `.\pipeline.ps1 2>&1 | Tee-Object build.log`,
    # the ordinary way to run this unattended. That form died at step 2 before the
    # fix and completes after it; both directions were measured, as was a genuine
    # `cargo fmt --check` failure still aborting with "FAILED: ... (exit 1)" -- the
    # check that this fix does not buy quiet by swallowing real failures.
    #
    # The step's verdict is therefore taken ONLY from the explicit $LASTEXITCODE
    # check below, which behaves the same however the script was invoked, rather
    # than from ambient preference-variable behaviour. (PS7+ is reported to differ
    # again here; not measured on this machine, and deliberately not relied upon.)
    # Nothing is weakened: `throw` stays terminating regardless of the preference,
    # and both PowerShell scripts invoked as step bodies (lint-references.ps1,
    # stage.ps1) set their own $ErrorActionPreference = 'Stop' at their own script
    # scope and report failure by `exit`/`throw`, so a caller's preference cannot
    # soften them.
    #
    # Assigning the preference here is function-scoped, so it reverts on return;
    # PowerShell's preference variables are dynamically scoped, so $Body -- defined
    # at script scope but invoked from here -- correctly sees this value.
    # 'Continue' stops the record being TERMINATING. It does not stop it being
    # WRITTEN -- and that is the second half of the same defect, fixed by the unwrap
    # below. Measured here on PS 5.1, native command exiting 0 with one stderr line,
    # caller redirecting:
    #
    #     cargo :     Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.14s
    #     At R:\Scribobulate\packaging\windows\pipeline.ps1:206 char:5
    #     + CategoryInfo          : NotSpecified: (...) [], RemoteException
    #     + FullyQualifiedErrorId : NativeCommandError
    #
    # The run completes and every gate is honest, but each successful step prints a
    # block that reads as a failure, quoting a line number in THIS file -- so a real
    # failure has to be told apart from four lookalikes, which is the same reading
    # error the fatal version caused, just cheaper.
    #
    # `2>&1` here is deliberately the very construct that creates the records: it is
    # what lets us CATCH them as objects and re-emit the message as a plain string.
    # Non-ErrorRecord output passes through untouched, so a body that emits real
    # objects (stage.ps1) keeps its normal formatting.
    #
    # NOT $ErrorActionPreference = 'SilentlyContinue', the obvious one-word fix: cargo
    # writes COMPILER DIAGNOSTICS to stderr as well as progress, so suppressing the
    # records discards the error text of a failing build and leaves the operator with
    # a bare "FAILED: ... (exit 101)". Quiet, and useless.
    #
    # Emitted to the OUTPUT stream, not Write-Host: Write-Host writes to the
    # information stream, which `2>&1` does not capture, so the documented
    # `.\pipeline.ps1 2>&1 | Tee-Object build.log` form would have produced a log with
    # every cargo line missing. Measured: with this form, `| Tee-Object` and
    # `2>&1 | Tee-Object` both capture the full output, stdout/stderr interleaving is
    # preserved in order, a body that `throw`s still terminates, and a called .ps1's
    # `exit 1` is still seen by the $LASTEXITCODE check below.
    $ErrorActionPreference = 'Continue'
    & $Body 2>&1 | ForEach-Object {
        if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { $_ }
    }
    if ($LASTEXITCODE -ne 0) {
        throw "FAILED: $Name (exit $LASTEXITCODE)"
    }
}

# Locate Inno Setup's compiler. Probed rather than hardcoded because the two
# supported install scopes put it in different places and BOTH are normal:
# `winget install --id JRSoftware.InnoSetup` defaults to USER scope
# (%LOCALAPPDATA%\Programs), which is what packaging/windows/README.md documents,
# while a machine-scope install lands under Program Files. Hardcoding either one
# makes the other look like "Inno Setup is not installed".
function Find-Iscc {
    $onPath = Get-Command iscc -CommandType Application -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    # Roots checked for emptiness BEFORE Join-Path: with $ErrorActionPreference =
    # 'Stop', Join-Path on an unset root is a terminating error, so a probe written
    # the obvious way would abort the run instead of moving on to the next candidate.
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

# Every exit path from here on has to leave the caller's directory as it found it,
# including the throws -- hence one `finally` rather than a Pop-Location before each
# `throw`, which is how the GTK-prefix check below came to have none at all.
Push-Location $repo
try {

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------

# `cmd.exe` skips the working directory when this variable is DEFINED, and
# gvsbuild's gettext step invokes create-lists.bat by bare name -- it then fails
# as though the source tree were corrupt. Note that
# [Environment]::SetEnvironmentVariable(..., $null) does NOT work: it leaves the
# variable defined-but-empty, which cmd still honours, and `$env:VAR` reads empty
# either way so the check that "confirms" the fix cannot detect the broken case.
# Only Remove-Item actually deletes it. ScrAP-165.
Remove-Item Env:NoDefaultCurrentDirectoryInExePath -ErrorAction SilentlyContinue

if (-not (Test-Path $Prefix)) {
    throw "GTK prefix not found at $Prefix. Build it with gvsbuild first -- see packaging/windows/README.md, or set SCRIB_GTK_PREFIX."
}

# pkgconf/pkg-config ship inside the gvsbuild tree; there is no system-wide
# pkg-config, so $Prefix\bin must be on PATH or gtk4-sys's build script cannot
# probe. build.rs also shells out to glib-compile-resources from the same place.
#
# LIB/INCLUDE are appended to only when there is something to append -- the same
# guard build.bat:67-68 carries, and for the same reason: prepending onto an unset
# variable leaves a trailing `;`, and an empty entry in those lists means "the
# current directory" to the toolchain. Keeping the two forms identical is what
# makes build.bat's claim to set "the same four" true.
$env:PATH            = "$Prefix\bin;$env:PATH"
$env:PKG_CONFIG_PATH = "$Prefix\lib\pkgconfig"
$env:LIB             = if ($env:LIB)     { "$Prefix\lib;$env:LIB" }         else { "$Prefix\lib" }
$env:INCLUDE         = if ($env:INCLUDE) { "$Prefix\include;$env:INCLUDE" } else { "$Prefix\include" }

Write-Host '=== GTK discoverable? ===' -ForegroundColor Cyan
Write-Host ('{0,-18} {1}' -f 'prefix', $Prefix)
foreach ($m in @('gtk4', 'gtksourceview-5', 'glib-2.0')) {
    $v = pkgconf --modversion $m
    if (-not $v) { throw "pkg-config cannot resolve $m" }
    Write-Host ('{0,-18} {1}' -f $m, $v)
}

# ---------------------------------------------------------------------------
# POLICY § Build pipeline
# ---------------------------------------------------------------------------

Invoke-Step '1. cargo fmt --check' { cargo fmt --check }

# The feature flag is not optional: without it the gtk-integration-tests modules
# are not compiled, so clippy never sees them and they rot unnoticed (ScrAP-124).
Invoke-Step '2. cargo clippy (-D warnings)' {
    cargo clippy --all-targets --features gtk-integration-tests -- -D warnings
}

Invoke-Step '3. cargo build --release' { cargo build --release }

Invoke-Step '4. cargo test' { cargo test }

# 4b. Surface tests that ran but verified nothing on this host.
#
# libtest has no concept of a skip: a test that cannot exercise its subject either
# fails, or returns early and is reported `ok` exactly like one that checked
# everything. Worse, libtest CAPTURES a passing test's output, so a body that
# announces "I verified nothing" is silenced precisely when it passes. The result
# reads as full coverage.
#
# That is not hypothetical here. TDD 19.2's symlink half needs a real symlink, and
# creating one on Windows requires Developer Mode or elevation, so on a stock box
# those tests legitimately cannot run. They print `SKIPPED [...]` and return. This
# step re-runs the suite with --nocapture purely to let those lines through, greps
# for the marker, and prints what it finds. It is INFORMATIONAL and never fails the
# pipeline -- an unmet privilege is not a defect -- but it must be SEEN, because the
# whole reason this exists is that the same limb previously had no coverage on
# Windows at all and nothing said so (the automated test was `#[cfg(unix)]`, so it
# did not exist rather than being skipped, and the checked-in fixture materialised
# as an ordinary file that the app correctly navigates to).
#
# Re-running costs a second or two: everything is already compiled, and this is the
# only way to read output libtest hides on success.
#
# The `2>&1` below is exactly the construct that makes a native command's stderr
# terminating under the script-level 'Stop' (see Invoke-Step's comment), and cargo
# writes both its progress AND these notices to stderr -- so this line needs the
# same local 'Continue', restored immediately after. Written the obvious way it
# aborts the pipeline on a healthy run, which is the defect this file was just
# fixed for; it is repeated here because the preference is AMBIENT and does not
# follow the pattern that avoided it upstream.
Write-Host ''
Write-Host '=== 4b. tests that reported SKIPPED (informational) ===' -ForegroundColor Cyan
# The re-run's exit code is captured, because "no SKIPPED lines" and "the re-run
# never produced any output" were previously the SAME branch — and the second one
# printed the MORE reassuring message, in green (QA round 5, F-SEC5-001). An
# all-clear must first prove that the thing it is clearing actually executed.
$eapOuter = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$global:LASTEXITCODE = 0
try {
    $skipRun     = @(cargo test --lib -- --nocapture --quiet 2>&1)
    $skipRunCode = $LASTEXITCODE
    $skipNotices = @($skipRun |
        Select-String -Pattern 'SKIPPED \[' |
        ForEach-Object { $_.Line.Trim() })
} finally {
    $ErrorActionPreference = $eapOuter
}
if ($skipRunCode -ne 0) {
    # Deliberately NOT fatal: this step is informational by design, and the step-4
    # `cargo test` above it is already wrapped in Invoke-Step and would have failed
    # first. What matters is that the operator can tell a measurement that did not
    # happen from one that came back clean.
    Write-Host "   UNKNOWN - the skip-report re-run exited $skipRunCode; no conclusion available" -ForegroundColor Yellow
    Write-Host '   This is NOT "none": nothing was measured, so nothing is cleared.' -ForegroundColor Yellow
} elseif ($skipNotices.Count -eq 0) {
    Write-Host '   none - every environment-dependent test verified its subject' -ForegroundColor Green
} else {
    foreach ($n in $skipNotices) { Write-Host "   $n" -ForegroundColor Yellow }
    Write-Host ''
    Write-Host "   $($skipNotices.Count) test(s) passed WITHOUT verifying their subject on this host." -ForegroundColor Yellow
    Write-Host '   Green above does not cover the behaviour named in each line.' -ForegroundColor Yellow
}

if ($SkipIntegration) {
    Write-Host ''
    Write-Host '=== 5. GTK integration tests - SKIPPED (-SkipIntegration) ===' -ForegroundColor Yellow
} else {
    # --test-threads=1 is REQUIRED, not a preference: libtest buffers output per
    # thread, so a parallel run of a wedging suite prints no test names at all and
    # reads as "hangs before the first test", pointing every subsequent hypothesis
    # at process startup. ScrAP-166.
    # @(...) around the projection, not just the pipeline: with an empty
    # $skippedTests the bare pipeline yields $null, and splatting $null is not the
    # same thing as splatting no arguments. Wrapping keeps it an array either way.
    $skipArgs = @($skippedTests | ForEach-Object { '--skip', $_ })
    $carveOut = if ($skippedTests.Count) { "skipping $($skippedTests.Count) by name" } else { 'no skips' }
    Invoke-Step "5. GTK integration tests ($carveOut)" {
        cargo test --features gtk-integration-tests -- --test-threads=1 @skipArgs
    }
    foreach ($t in $skippedTests) { Write-Host "    skipped: $t" -ForegroundColor Yellow }
}

Write-Host ''
Write-Host '=== 6. coverage ratchet - NOT RUN (Linux-canonical, by design) ===' -ForegroundColor DarkGray

# Steps 7 and 8 are change-triggered review gates (UI-behaviour coverage alignment,
# architecture-diagram alignment), not commands -- nothing to run here.
Invoke-Step '9. static reference lint' { & "$repo\scripts\lint-references.ps1" }

if ($Package) {
    # The staged tree's location is decided HERE, not left to stage.ps1's default,
    # because the installer has to be told the same path: scribobulate.iss opens
    # with `#ifndef StageDir / #error`, so an ISCC invocation without /DStageDir
    # does not build a wrong installer -- it builds none.
    $stageDir = Join-Path $repo 'build\stage\Scribobulate'
    # -GtkPrefix forwarded, so -Prefix is not silently ignored: stage.ps1 has its
    # own default, and without this the tree was staged from gvsbuild's default
    # prefix no matter what the caller asked for.
    Invoke-Step 'stage the redistributable tree' {
        & "$PSScriptRoot\stage.ps1" -GtkPrefix $Prefix -OutDir $stageDir -RepoRoot $repo
    }

    $iscc = Find-Iscc
    if (-not $iscc) {
        throw @"
FAILED: build the installer -- Inno Setup 6 not found.
Looked for iscc on PATH, then:
  $env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe   (winget default, user scope)
  ${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe     (machine scope)
Install it with:  winget install --id JRSoftware.InnoSetup
The staged tree at $stageDir is complete; only the installer step is missing.
"@
    }
    Write-Host ''
    Write-Host "Inno Setup: $iscc" -ForegroundColor DarkGray
    Invoke-Step 'build the installer' {
        & $iscc "/DStageDir=$stageDir" "$PSScriptRoot\scribobulate.iss"
    }
}

}
finally {
    Pop-Location
}

Write-Host ''
Write-Host 'Pipeline complete.' -ForegroundColor Green
