<#
.SYNOPSIS
    Runs the build pipeline locally on Windows, deriving every step from
    scripts/pipeline.steps, and optionally stages the runtime and builds the installer.

.DESCRIPTION
    This is a LOCAL developer script. It is deliberately not CI: Scribobulate has no
    GitHub Actions workflow, and adding one is out of scope for the Windows port (see
    packaging/windows/README.md "Why this is a script, not CI").

    IT DERIVES, IT DOES NOT RESTATE. Every step, its ordinal, class, intent, verdict
    rule and command come from scripts/pipeline.steps. This file previously carried its
    own hand-written list -- numbered 1-5, 9 plus unnumbered staging steps, with a 4b
    POLICY never mentioned -- and nothing detected the divergence because nothing had an
    opinion about what the step list was. A port that RESTATED the list would make
    `-ListSteps` prove only that two restatements match, which two people copying the
    same wrong list also achieve (ScrAP-207).

    The load-bearing structural property, mirroring scripts/pipeline.sh: `Get-DerivedStepIds`
    is the ONLY producer of the ordered step list, and BOTH -ListSteps and the run loop
    consume it. There is no second list that could disagree. -SelfTest proves that property
    still holds after an edit; it does not create it.

    THE CONTRACT IS VALIDATED IN FULL, FOR EVERY DECLARED PLATFORM, from this runner --
    not just for Windows. Validating only the running platform is what let a garbled
    `na.windows` line pass cleanly from Linux: the platform nobody runs becomes the
    lenient one, which is ScrAP-207's shape reproduced inside the artefact built to
    prevent it. Cross-platform validation makes that class of gap fail on whichever
    runner is invoked first.

.PARAMETER Prefix
    The gvsbuild install prefix. Defaults to %SCRIB_GTK_PREFIX% when set, else gvsbuild's
    own default -- the same resolution order build.bat uses, so the two entry points
    cannot build against different GTKs on the same machine.

.PARAMETER ListSteps
    Print the derived step list as `<ordinal><TAB><id><TAB><class>` and exit. This is the
    parity artefact: diff it against `scripts/pipeline.sh --list-steps`. Needs no GTK and
    touches no environment, deliberately -- an artefact meant for cross-port diffing must
    not depend on whether this box has a toolchain installed.

.PARAMETER SelfTest
    Validate the contract and this runner's derivation, then exit.

.PARAMETER SkipIntegration
    Skip the `integration` step. It opens real windows, so it is unsuitable for an
    unattended run. This is an OPERATOR OVERRIDE and says so in the output -- it is not a
    contract non-applicability declaration, and the two must not read alike.

.PARAMETER Package
    Run the contract's `packaging`-class step, which stages the redistributable tree and
    builds the installer via packaging/windows/package.ps1. Packaging is OPT-IN because
    building an installer adds minutes to a gate meant to run after every change; a skipped
    packaging step is still ANNOUNCED in the run output rather than omitted. It IS a contract
    step and so does appear in -ListSteps. Requires Inno Setup 6; a missing ISCC.exe FAILS
    rather than warning, because -Package was asked for explicitly and a green run that
    produced no installer is worse than a red one.

.EXAMPLE
    .\packaging\windows\pipeline.ps1
    .\packaging\windows\pipeline.ps1 -ListSteps
    .\packaging\windows\pipeline.ps1 -SelfTest
    .\packaging\windows\pipeline.ps1 -SkipIntegration
    .\packaging\windows\pipeline.ps1 -Package
#>
[CmdletBinding()]
param(
    [string] $Prefix = $(if ($env:SCRIB_GTK_PREFIX) { $env:SCRIB_GTK_PREFIX } else { 'C:\gtk-build\gtk\x64\release' }),
    [switch] $ListSteps,
    [switch] $SelfTest,
    [switch] $SkipIntegration,
    [switch] $Package
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# EVERY STRING LITERAL IN THIS FILE IS ASCII, deliberately. A .ps1 without a BOM is
# decoded as the ANSI codepage by Windows PowerShell 5.1, and a UTF-8 em dash inside a
# string literal arrives as a curly quote that PowerShell honours as a STRING DELIMITER:
# the string ends early, the file stops parsing, and the error points at an unrelated
# brace hundreds of lines away. So `--`, never an em dash. The Linux runner's output uses
# em dashes; that cosmetic difference is confined to prose and does not touch -ListSteps,
# which is the only output required to diff clean.

$PLATFORM = 'windows'
$repo     = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$CONTRACT = Join-Path $repo 'scripts\pipeline.steps'

function Write-Err { param([string] $Message) [Console]::Error.WriteLine($Message) }

if (-not (Test-Path $CONTRACT)) {
    Write-Err "pipeline: $CONTRACT not found (run from anywhere in the repo)"
    exit 2
}

# --------------------------------------------------------------------------------------
# Contract parsing
#
# Grammar is `keyword  key  value...` with the value as the remainder of the line, so a
# value may contain spaces. Uniform arity is deliberate -- see the contract header.
#
# -Encoding UTF8 is not optional: PS 5.1's Get-Content defaults to the ANSI codepage, and
# the contract's comments contain UTF-8 punctuation. Comment lines are skipped either way,
# so a misdecode would be harmless TODAY -- which is exactly why it would go unnoticed
# until a `na.` reason or a `setup` description acquired a non-ASCII character and started
# being printed as mojibake in the run output.
# --------------------------------------------------------------------------------------
$script:ContractLines = @(Get-Content -Encoding UTF8 -LiteralPath $CONTRACT)

# The recognised line shape. Matched with -cmatch, CASE-SENSITIVELY, because PowerShell's
# matching defaults are case-INSENSITIVE where grep's are not: porting this gate with the
# default operator would silently accept `Step 1 fmt` and `CMD.WINDOWS`, quietly relaxing
# the contract's strictness relative to the Linux runner.
$script:LineShape = '^(platform|step|intent|verdict|class|setup|cmd\.[a-z]+|na\.[a-z]+|carveout\.[a-z]+)\s+\S+(\s+.*)?$'

function Split-ContractLine {
    param([string] $Line)
    # Returns keyword/key/value, value being the remainder. -split with a limit of 3 is
    # the direct analogue of the awk `$1 = ""; $2 = ""` idiom the shell port uses.
    $parts = $Line.Trim() -split '\s+', 3
    [pscustomobject]@{
        Keyword = $parts[0]
        Key     = if ($parts.Count -ge 2) { $parts[1] } else { '' }
        Value   = if ($parts.Count -ge 3) { $parts[2] } else { '' }
    }
}

function Get-ContractRecords {
    # Every non-blank, non-comment line, parsed. Callers wrap in @() -- see the note on
    # Get-DerivedStepIds about return-idiom matching.
    [OutputType([pscustomobject[]])]
    param()
    foreach ($line in $script:ContractLines) {
        if ($line -match '^\s*(#.*)?$') { continue }
        Split-ContractLine $line
    }
}

function Get-ContractValue {
    # The remainder of the first matching line, or ''. First match wins, like awk's `exit`.
    [OutputType([string])]
    param([string] $Keyword, [string] $Key)
    foreach ($r in @(Get-ContractRecords)) {
        if ($r.Keyword -ceq $Keyword -and $r.Key -ceq $Key) { return $r.Value }
    }
    return ''
}

# THE ONLY PRODUCER of the ordered step list. Both -ListSteps and the run loop consume
# this; nothing else may build a step list.
#
# FILE ORDER IS THE ORDER. The `step` lines' ordinals (1, 2, ... 4b, 5 ...) are POLICY's
# display labels, not sort keys -- a numeric sort cannot order "4b" without a convention
# every port would have to reimplement identically-or-not, which is the exact class of
# quiet disagreement this contract exists to remove. The ordinals are validated
# non-decreasing below, so a reorder that forgets to renumber fails loudly.
#
# Returns a [string[]]. Callers read it as @(Get-DerivedStepIds): the return idiom and the
# call-site idiom are matched on purpose, because `,$array` protects an empty array from
# unwrapping but DOUBLE-wraps when the caller also writes @(...), and PS 5.1 enumerates a
# method call on an accidental collection instead of rejecting it -- Set-StrictMode does
# not catch either.
function Get-DerivedStepIds {
    [OutputType([string[]])]
    param()
    foreach ($r in @(Get-ContractRecords)) {
        if ($r.Keyword -ceq 'step') { $r.Value }
    }
}

function Get-StepOrdinal {
    [OutputType([string])]
    param([string] $Id)
    foreach ($r in @(Get-ContractRecords)) {
        if ($r.Keyword -ceq 'step' -and $r.Value -ceq $Id) { return $r.Key }
    }
    return ''
}

function Get-DeclaredPlatforms {
    [OutputType([string[]])]
    param()
    foreach ($r in @(Get-ContractRecords)) {
        if ($r.Keyword -ceq 'platform') { $r.Key }
    }
}

# Ordinal comparison, standing in for the shell port's `sort -V`.
#
# THIS IS A REIMPLEMENTED CONVENTION and therefore the single most likely place for the
# two ports to disagree silently -- the contract header names that hazard explicitly. It
# is verified against `sort -V` over the real ordinals and a set of adversarial pairs
# rather than assumed to agree; see packaging/windows/README.md and the port's report.
# Numeric prefix compared numerically, alphabetic suffix compared ordinally, so 4 < 4b < 5.
function Compare-StepOrdinal {
    [OutputType([int])]
    param([string] $A, [string] $B)
    $rx = '^(\d*)(.*)$'
    $ma = [regex]::Match($A, $rx); $mb = [regex]::Match($B, $rx)
    $na = if ($ma.Groups[1].Value) { [int] $ma.Groups[1].Value } else { -1 }
    $nb = if ($mb.Groups[1].Value) { [int] $mb.Groups[1].Value } else { -1 }
    if ($na -ne $nb) { return $(if ($na -lt $nb) { -1 } else { 1 }) }
    return [string]::CompareOrdinal($ma.Groups[2].Value, $mb.Groups[2].Value)
}

# --------------------------------------------------------------------------------------
# Contract validation
#
# A garbled contract must fail LOUDLY, never degrade into agreeing on nothing. Two ports
# that both parse a broken file into an empty step list agree perfectly and prove nothing.
# --------------------------------------------------------------------------------------
function Test-Contract {
    [OutputType([bool])]
    param()
    $errs = 0

    $bad = @()
    for ($i = 0; $i -lt $script:ContractLines.Count; $i++) {
        $line = $script:ContractLines[$i]
        if ($line -match '^\s*(#.*)?$') { continue }
        if ($line -cnotmatch $script:LineShape) { $bad += "$($i + 1):$line" }
    }
    if ($bad.Count) {
        Write-Err "pipeline: $CONTRACT has lines that are neither blank, a comment, nor a"
        Write-Err "  recognised 'keyword key value' triple:"
        foreach ($b in $bad) { Write-Err $b }
        $errs++
    }

    $ids = @(Get-DerivedStepIds)
    if ($ids.Count -eq 0) {
        Write-Err "pipeline: $CONTRACT defines no steps. An empty step list is a garbled"
        Write-Err '  contract, not a pipeline with nothing to do.'
        return $false
    }

    $dupes = @($ids | Group-Object | Where-Object { $_.Count -gt 1 } | ForEach-Object { $_.Name })
    if ($dupes.Count) {
        Write-Err "pipeline: duplicate step id(s) in ${CONTRACT}: $($dupes -join ' ')"
        $errs++
    }

    $platforms = @(Get-DeclaredPlatforms)
    if ($platforms.Count -eq 0) {
        Write-Err "pipeline: $CONTRACT declares no platforms"
        return $false
    }

    # Every step needs intent, verdict and class; and for EVERY declared platform either a
    # command or a non-applicable declaration -- unless it is a review step. Cross-platform
    # by design; see this file's .DESCRIPTION.
    foreach ($id in $ids) {
        $intent  = Get-ContractValue 'intent'  $id
        $verdict = Get-ContractValue 'verdict' $id
        $class   = Get-ContractValue 'class'   $id

        if (-not $intent)  { Write-Err "pipeline: step '$id' has no intent";  $errs++ }
        if (-not $verdict) { Write-Err "pipeline: step '$id' has no verdict"; $errs++ }
        if (-not $class)   { Write-Err "pipeline: step '$id' has no class";   $errs++ }

        if ($class -and $class -cnotin @('required', 'informational', 'review', 'packaging')) {
            Write-Err "pipeline: step '$id' has unknown class '$class'"
            $errs++
        }

        foreach ($plat in $platforms) {
            $cmd = Get-ContractValue "cmd.$plat" $id
            $na  = Get-ContractValue "na.$plat"  $id

            if ($class -ceq 'review') {
                if ($cmd) {
                    Write-Err "pipeline: review step '$id' must not carry cmd.$plat"
                    $errs++
                }
            } else {
                if (-not $cmd -and -not $na) {
                    Write-Err "pipeline: step '$id' has neither cmd.$plat nor na.$plat."
                    Write-Err '  A step that does not apply must be DECLARED, never omitted.'
                    $errs++
                }
                if ($cmd -and $na) {
                    Write-Err "pipeline: step '$id' has BOTH cmd.$plat and na.$plat"
                    $errs++
                }
            }

            # A non-applicable declaration must record its KIND, because the two kinds are
            # acted on differently and the tooling one is a trap (contract header).
            if ($na) {
                $naParts = $na -split '\s+', 2
                $kind    = $naParts[0]
                $reason  = if ($naParts.Count -ge 2) { $naParts[1].Trim() } else { '' }
                if ($kind -cnotin @('permanent', 'tooling')) {
                    Write-Err "pipeline: step '$id' na.$plat has kind '$kind'; expected"
                    Write-Err "  'permanent' or 'tooling'"
                    $errs++
                }
                if (-not $reason) {
                    Write-Err "pipeline: step '$id' na.$plat has a kind but no reason"
                    $errs++
                }
            }
        }

        # Repo-relative .ps1/.sh paths named in THIS PLATFORM'S commands must exist.
        #
        # The asymmetry with the checks above is deliberate: contract ENTRIES are validated
        # for every declared platform from every runner, but FILES only where they live. A
        # Linux runner cannot check that a .ps1 is present, and pretending otherwise would
        # fail the gate on every non-Windows machine.
        #
        # Worth having because an OPT-IN step is exactly where a typo survives: a packaging
        # step does not run unless asked for, so a misspelled script path would otherwise be
        # discovered by whoever first tries to cut a release -- the worst possible moment.
        $ownCmd = Get-ContractValue "cmd.$PLATFORM" $id
        if ($ownCmd) {
            foreach ($m in [regex]::Matches($ownCmd, '[A-Za-z0-9_.\-]+(?:[/\\][A-Za-z0-9_.\-]+)*\.(?:ps1|sh)')) {
                $ref = $m.Value
                # Absolute paths are not the contract's to guarantee.
                if ($ref -cmatch '^[A-Za-z]:') { continue }
                if (-not (Test-Path (Join-Path $repo $ref))) {
                    Write-Err "pipeline: step '$id' cmd.$PLATFORM names '$ref', which does not exist"
                    $errs++
                }
            }
        }
    }

    # Ordinals must be non-decreasing in file order, so a reorder that forgets to renumber
    # is caught rather than silently changing the run order.
    $prev = ''
    foreach ($id in $ids) {
        $ord = Get-StepOrdinal $id
        if ($prev -and (Compare-StepOrdinal $prev $ord) -gt 0) {
            Write-Err "pipeline: step ordinals are out of order in file order: '$prev' then '$ord'"
            $errs++
        }
        $prev = $ord
    }

    return ($errs -eq 0)
}

# --------------------------------------------------------------------------------------
# -ListSteps -- the parity artefact
#
# Prints the DERIVED list, one step per line, as `<ordinal>\t<id>\t<class>`. The setup
# phase is deliberately absent: it produces environment, has no verdict, and cannot be
# skipped or reordered, so modelling it as a step would force platforms without one to
# declare a non-applicable step 0 -- noise that dilutes the declarations carrying real
# information.
#
# Written through [Console]::Out with explicit LF. PowerShell would emit CRLF, which makes
# a byte-exact diff against the shell port's output fail on every line for a reason that
# has nothing to do with the step list -- and the usual fix, normalising CR away at
# compare time, weakens the very artefact whose job is to be compared.
# --------------------------------------------------------------------------------------
function Write-StepList {
    $sb = New-Object System.Text.StringBuilder
    foreach ($id in @(Get-DerivedStepIds)) {
        $null = $sb.Append((Get-StepOrdinal $id)).Append("`t").Append($id).Append("`t").Append((Get-ContractValue 'class' $id)).Append("`n")
    }
    [Console]::Out.Write($sb.ToString())
}

# --------------------------------------------------------------------------------------
# -SelfTest
#
# Proves the derivation property still holds after an edit. It cannot CREATE the property
# -- that comes from Get-DerivedStepIds being the only producer -- but it catches an edit
# that introduces a second list.
# --------------------------------------------------------------------------------------
function Invoke-SelfTest {
    [OutputType([bool])]
    param()
    Write-Host ":: validating $CONTRACT"
    if (-not (Test-Contract)) { return $false }
    Write-Host '   contract is well-formed'

    # The list -ListSteps prints must be the list the run loop iterates. Both derive from
    # Get-DerivedStepIds, so this compares the artefact against its own source.
    $derived = @(Get-DerivedStepIds)
    $printed = @()
    foreach ($id in $derived) { $printed += (Get-StepOrdinal $id) + "`t" + $id + "`t" + (Get-ContractValue 'class' $id) }
    $printedIds = @($printed | ForEach-Object { ($_ -split "`t")[1] })
    if (($printedIds -join '|') -cne ($derived -join '|')) {
        Write-Err 'pipeline: -ListSteps does not print the derived step list'
        Write-Err "  derived: $($derived -join ' ')"
        Write-Err "  printed: $($printedIds -join ' ')"
        return $false
    }
    Write-Host '   -ListSteps prints exactly the derived step list'

    if ($derived -ccontains 'setup') {
        Write-Err "pipeline: 'setup' appears as a step; it is a phase, not a step"
        return $false
    }
    Write-Host '   setup phase is absent from the step list (correct)'

    Write-Host "   $($derived.Count) steps derived"
    return $true
}

# --------------------------------------------------------------------------------------
# Contract operations that need no environment. Handled BEFORE the GTK setup on purpose:
# the parity artefact must not depend on whether this box has a toolchain installed, or
# -ListSteps would be unrunnable exactly where a port is being brought up.
# --------------------------------------------------------------------------------------
if ($ListSteps) {
    if (-not (Test-Contract)) { exit 2 }
    Write-StepList
    exit 0
}
if ($SelfTest) {
    if (-not (Invoke-SelfTest)) { exit 1 }
    exit 0
}

if (-not (Test-Contract)) { exit 2 }

# --------------------------------------------------------------------------------------
# Step execution
# --------------------------------------------------------------------------------------
$script:Failed = @()
$script:StepExitCode = 0
$script:StepOk = $true

function Show-Announce { param([string] $Text) Write-Host ''; Write-Host "=== $Text ===" -ForegroundColor Cyan }

# Runs a contract command line VERBATIM.
#
# THE EXECUTOR IS `cmd /c`, NOT Invoke-Expression, and this is measured rather than
# stylistic. Invoke-Expression is the direct analogue of the shell port's `eval`, and it
# CANNOT run this contract: `cmd.windows diagram` is
# `powershell -NoProfile -Command "$d = New-Object ...; $d.Load(...)"`, and under
# Invoke-Expression the `$d` inside that double-quoted argument is expanded by THIS shell
# before the inner powershell ever sees it -- which under Set-StrictMode throws
# "The variable '$d' cannot be retrieved because it has not been set." Measured, verbatim.
# cmd.exe does not treat `$` as special, so the argument reaches the inner interpreter
# intact. Confirmed to still DISCRIMINATE: fed a malformed `<svg><g></svg>` the same route
# exits 1, so the gate is not merely running, it is judging.
#
# The stderr handling below is the same hard-won machinery the hand-written version of
# this file carried, and it is kept because the reasons are unchanged:
#   * $LASTEXITCODE is cleared first -- it is only written by NATIVE commands, so a step
#     would otherwise be judged on the PREVIOUS step's exit code.
#   * The body runs under 'Continue', not the script-level 'Stop'. Under 'Stop', PS 5.1
#     wraps what a native executable writes to stderr in a NativeCommandError and makes it
#     TERMINATING -- and cargo writes ordinary progress to stderr by design, so the fatal
#     record is a success message. Measured pre-fix: the gate failed because the build had
#     succeeded. The trigger is the redirection operator into the PowerShell pipeline, not
#     stderr capture as such, which is why a plain interactive run was never affected and
#     `.\pipeline.ps1 2>&1 | Tee-Object build.log` -- the unattended form -- died at clippy.
#   * The records are UNWRAPPED to their message rather than suppressed. NOT
#     'SilentlyContinue': cargo writes compiler DIAGNOSTICS to stderr as well as progress,
#     so suppressing them discards the error text of a failing build and leaves a bare
#     "FAILED (exit 101)". Quiet, and useless.
#   * Emitted to the OUTPUT stream, not Write-Host, because Write-Host writes to the
#     information stream which `2>&1` does not capture -- so the documented Tee-Object form
#     would otherwise produce a log with every cargo line missing.
# The verdict is taken ONLY from the explicit exit-code check, which behaves the same
# however the script was invoked, rather than from ambient preference-variable behaviour.
# THE EXIT CODE COMES BACK OUT-OF-BAND, in $script:StepExitCode, and the command's output
# is this function's OUTPUT. A caller either lets that flow to the console (a normal step)
# or captures it (a marker step) -- the same distinction the shell port draws between
# `eval "$cmd"` and `$(eval "$cmd")`.
#
# It is NOT a returned [pscustomobject], and that is a measured correction rather than a
# preference. Returning an object from a function that ALSO streams makes the return value
# an array of every output line PLUS the object, so `$r.ExitCode` reads a property off an
# accidental collection and throws under Set-StrictMode: "The property 'ExitCode' cannot be
# found on this object." It failed on the FIRST step of the first real run -- while
# -ListSteps, -SelfTest and twelve contract mutations were all green, because not one of
# them executes a step. A green contract-parsing suite is evidence about contract parsing.
function Invoke-ContractCommand {
    [OutputType([string[]])]
    param([string] $CommandLine)

    $eap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        cmd /c $CommandLine 2>&1 | ForEach-Object {
            if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { $_ }
        }
        $script:StepExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $eap
    }
}

# Carve-outs are reported even when there are none -- that is what lets the statement MEAN
# something rather than be silence.
#
# ANNOUNCE-ONLY, matching scripts/pipeline.sh, which prints carve-outs and then runs the
# contract command unmodified. That parity is deliberate: injecting `--skip` here would
# make the Windows run genuinely exclude tests the Linux run does not, which is precisely
# the per-platform divergence the contract exists to remove, and it would require assuming
# every carve-out-bearing command is a cargo test invocation. The list is empty today so
# nothing is currently mis-reported; the gap is flagged for a contract-level decision
# rather than patched unilaterally on one platform.
function Show-Carveouts {
    param([string] $Id)
    $list = @()
    foreach ($r in @(Get-ContractRecords)) {
        if ($r.Keyword -ceq "carveout.$PLATFORM" -and $r.Key -ceq $Id) { $list += $r.Value }
    }
    if ($list.Count -eq 0) {
        if ($Id -cin @('integration', 'test')) { Write-Host '    carve-outs: none' }
    } else {
        Write-Host "    carve-outs: $($list.Count) by name (ANNOUNCED, NOT APPLIED -- see Show-Carveouts)"
        foreach ($t in $list) { Write-Host "      skipped: $t" -ForegroundColor Yellow }
    }
}

# Reports its verdict in $script:StepOk, NOT as a return value, for the same reason
# Invoke-ContractCommand reports its exit code out-of-band -- and this is where the trap
# bit twice. A step body streams the command's output through this function, so a returned
# bool joins that output in one collection: `if (-not (Invoke-ContractStep $id))` then tests
# a NON-EMPTY ARRAY, which is always truthy, so `-not` is always false. Measured: the run
# printed "FAIL (exit 101)" for the integration step and then reported "pipeline PASSED"
# with exit 0. Fixing the inner function had merely relocated the same defect one level up,
# and the second sighting is the one that shows the rule: in PowerShell, a function that
# writes to the output stream cannot also return a verdict through it.
function Invoke-ContractStep {
    param([string] $Id)

    $script:StepOk = $true
    $ord     = Get-StepOrdinal $Id
    $class   = Get-ContractValue 'class'   $Id
    $intent  = Get-ContractValue 'intent'  $Id
    $cmd     = Get-ContractValue "cmd.$PLATFORM" $Id
    $na      = Get-ContractValue "na.$PLATFORM"  $Id

    # A review step is announced, never judged. Pretending a script can decide whether a
    # behaviour change carries its check would be a gate that always passes.
    if ($class -ceq 'review') {
        Show-Announce "$ord. $Id - REVIEW (human obligation, not machine-checkable)"
        Write-Host "    intent: $intent"
        return
    }

    # A non-applicable step is DECLARED here, in the run output, with its kind and reason
    # -- not omitted, and not left in a source comment where the pipeline's user cannot
    # see it. The hand-written version of this file declared step 6 at runtime but left 7
    # and 8 in comments, so a reader of the run saw a gap with no explanation.
    if ($na) {
        $naParts = $na -split '\s+', 2
        $kind    = $naParts[0]
        $reason  = if ($naParts.Count -ge 2) { $naParts[1].Trim() } else { '' }
        Show-Announce "$ord. $Id - NOT APPLICABLE on $PLATFORM ($kind)"
        Write-Host "    intent: $intent"
        Write-Host "    reason: $reason"
        return
    }

    # `packaging` is OPT-IN: skipped unless the runner is asked (-Package), required when it
    # does run. Building an installer on every edit would add minutes to a gate meant to run
    # after every change.
    #
    # A skipped packaging step is still ANNOUNCED, never omitted -- the same reason a
    # non-applicable step is declared in the run output: an omission the reader cannot see is
    # how a step quietly stops existing. Checked AFTER the na branch, so a platform that
    # declares packaging non-applicable still gets its reason printed rather than this notice.
    if ($class -ceq 'packaging' -and -not $Package) {
        Show-Announce "$ord. $Id - not run (packaging is opt-in; pass -Package)"
        Write-Host "    intent: $intent"
        return
    }

    $verdict = Get-ContractValue 'verdict' $Id

    # A marker verdict never fails the run: its job is to make something VISIBLE that
    # would otherwise be silent. libtest CAPTURES a passing test's output, so a body
    # announcing "I verified nothing" is silenced exactly when it passes.
    if ($verdict -cmatch '^marker:') {
        $marker = $verdict.Substring('marker:'.Length)
        Show-Announce "$ord. $Id - informational"
        Write-Host "    intent: $intent"
        $out = @(Invoke-ContractCommand -CommandLine $cmd)
        $rc  = $script:StepExitCode
        # -SimpleMatch because the marker is a literal containing regex metacharacters
        # ("SKIPPED ["), and -CaseSensitive because Select-String defaults to
        # case-INSENSITIVE where the shell port's `grep -F` does not.
        $hits = @($out | Select-String -SimpleMatch -CaseSensitive -Pattern $marker |
                  ForEach-Object { $_.Line.Trim() })
        if ($hits.Count) {
            foreach ($h in $hits) { Write-Host "    $h" -ForegroundColor Yellow }
        } else {
            Write-Host "    no tests reported '$marker'"
        }
        # The re-run's exit code is reported but not fatal: "no marker lines" and "the
        # re-run itself failed" must not look identical. An all-clear must first prove that
        # the thing it is clearing actually executed.
        if ($rc -ne 0) {
            Write-Host "    NOTE: the informational re-run exited $rc (the required run above already gated this)" -ForegroundColor Yellow
        }
        return
    }

    Show-Announce "$ord. $Id"
    Write-Host "    intent: $intent"
    Show-Carveouts $Id
    Write-Host "    `$ $cmd"
    Invoke-ContractCommand -CommandLine $cmd
    if ($script:StepExitCode -eq 0) {
        Write-Host '    PASS' -ForegroundColor Green
        return
    }
    Write-Host "    FAIL (exit $($script:StepExitCode))" -ForegroundColor Red
    $script:Failed += $ord
    $script:StepOk = $false
}

# --------------------------------------------------------------------------------------
# Setup phase -- environment, no verdict, cannot be skipped or reordered, and NOT a step.
# Its description comes from the contract; the four assignments live here because only
# this platform has them.
# --------------------------------------------------------------------------------------
function Invoke-SetupPhase {
    Show-Announce "setup phase ($PLATFORM)"
    $desc = Get-ContractValue 'setup' $PLATFORM
    if ($desc -ceq 'none') {
        Write-Host "    none required on $PLATFORM"
        return
    }
    Write-Host "    $desc"

    # `cmd.exe` skips the working directory when this variable is DEFINED, and gvsbuild's
    # gettext step invokes create-lists.bat by bare name -- it then fails as though the
    # source tree were corrupt. [Environment]::SetEnvironmentVariable(..., $null) does NOT
    # work: it leaves the variable defined-but-empty, which cmd still honours, and
    # `$env:VAR` reads empty either way so the check that "confirms" the fix cannot detect
    # the broken case. Only Remove-Item actually deletes it. ScrAP-165.
    #
    # It matters more now, not less: every contract command is dispatched through `cmd /c`.
    Remove-Item Env:NoDefaultCurrentDirectoryInExePath -ErrorAction SilentlyContinue

    if (-not (Test-Path $Prefix)) {
        throw "GTK prefix not found at $Prefix. Build it with gvsbuild first -- see packaging/windows/README.md, or set SCRIB_GTK_PREFIX."
    }

    # pkgconf/pkg-config ship inside the gvsbuild tree; there is no system-wide
    # pkg-config, so $Prefix\bin must be on PATH or gtk4-sys's build script cannot probe.
    # build.rs also shells out to glib-compile-resources from the same place.
    #
    # LIB/INCLUDE are appended to only when there is something to append -- the same guard
    # build.bat carries, and for the same reason: prepending onto an unset variable leaves
    # a trailing `;`, and an empty entry in those lists means "the current directory" to
    # the toolchain.
    $env:PATH            = "$Prefix\bin;$env:PATH"
    $env:PKG_CONFIG_PATH = "$Prefix\lib\pkgconfig"
    $env:LIB             = if ($env:LIB)     { "$Prefix\lib;$env:LIB" }         else { "$Prefix\lib" }
    $env:INCLUDE         = if ($env:INCLUDE) { "$Prefix\include;$env:INCLUDE" } else { "$Prefix\include" }

    Write-Host ('    {0,-18} {1}' -f 'prefix', $Prefix)
    foreach ($m in @('gtk4', 'gtksourceview-5', 'glib-2.0')) {
        $v = pkgconf --modversion $m
        if (-not $v) { throw "pkg-config cannot resolve $m" }
        Write-Host ('    {0,-18} {1}' -f $m, $v)
    }
}

# Inno Setup discovery and the installer invocation now live in package.ps1, which the
# contract names as the packaging step's command. Deliberately NOT duplicated here: a second
# copy of the ISCC probe is how the two silently stop matching.

# Every exit path from here on has to leave the caller's directory as it found it,
# including the throws -- hence one `finally` rather than a Pop-Location before each throw.
Push-Location $repo
$rc = 0
try {
    Invoke-SetupPhase

    foreach ($stepId in @(Get-DerivedStepIds)) {
        if ($SkipIntegration -and $stepId -ceq 'integration') {
            Show-Announce "$(Get-StepOrdinal $stepId). $stepId - SKIPPED (-SkipIntegration)"
            Write-Host '    NOTE: this is an operator override, not a contract declaration.' -ForegroundColor Yellow
            continue
        }
        # The verdict is read from $script:StepOk, never from a return value -- see the
        # note on Invoke-ContractStep. Calling it as a statement lets the step's command
        # output flow to the console live, which is the whole reason the verdict cannot
        # travel that way.
        Invoke-ContractStep $stepId
        if (-not $script:StepOk) {
            $rc = 1
            break
        }
    }

    # Packaging is now a CONTRACT STEP (class `packaging`, opt-in via -Package) running in
    # its proper place in the ordered list, not a bolt-on after the loop. The staging and
    # installer logic moved to packaging/windows/package.ps1 so the contract can name ONE
    # command for it, the way cmd.linux names build-deb.sh and cmd.macos names dmg.sh.
    #
    # Until a contract carries such a step, -Package would silently do nothing. Silence is
    # the wrong answer for a flag the operator explicitly passed, so say it plainly.
    if ($Package) {
        $hasPackaging = $false
        foreach ($pid_ in @(Get-DerivedStepIds)) {
            if ((Get-ContractValue 'class' $pid_) -ceq 'packaging') { $hasPackaging = $true }
        }
        if (-not $hasPackaging) {
            Write-Host ''
            Write-Host '-Package was passed, but this contract declares no step of class "packaging".' -ForegroundColor Yellow
            Write-Host 'NOTHING WAS PACKAGED. Run packaging/windows/package.ps1 directly, or update the contract.' -ForegroundColor Yellow
        }
    }
}
finally {
    Pop-Location
}

Write-Host ''
if ($rc -eq 0) {
    Write-Host '=== pipeline PASSED ===' -ForegroundColor Green
} else {
    Write-Host "=== pipeline FAILED at step(s): $($script:Failed -join ' ') ===" -ForegroundColor Red
}
exit $rc
