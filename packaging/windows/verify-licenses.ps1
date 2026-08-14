<#
.SYNOPSIS
    Check the staged Windows tree against packaging\windows\licenses.psd1.

.DESCRIPTION
    The table says which upstream project every staged file belongs to and where
    its licence text comes from. This script is what makes the table true rather
    than plausible, by failing on any of four disagreements:

      1. A STAGED FILE WITH NO ROW. Someone added a DLL to stage.ps1 and shipped
         somebody else's code with no attribution. This is the direction people
         remember to check.

      2. A ROW WITH NO STAGED FILE. The dependency was dropped, or was never
         staged in the first place, and the table now describes an artefact that
         does not exist. This is the direction people leave out, and it is the
         one that turns a licence manifest into fiction: every row still reads
         correctly, and the thing it describes is gone.

      3. A ROW WHOSE LICENCE TEXT IS MISSING. A row may name a Source that is
         not there. gvsbuild ships EMPTY share\doc directories for freetype,
         graphene and libxml2, so a table written from the directory listing
         alone would name three licence files that do not exist and nothing
         would notice until someone opened the installer looking for one.

      4. A ROW WHOSE LICENCE TEXT IS NOT THE LICENCE. This is the condition that
         earns its keep. Checking that a licence FILE EXISTS is a file-existence
         predicate standing in for a semantic question, and it answers "fine"
         for all three of these, measured in this prefix:
           - pcre2\COPYING is four lines telling you to read a LICENCE file that
             is not shipped.
           - cairo\COPYING is a summary pointing at COPYING-LGPL-2.1 and
             COPYING-MPL-1.1, neither of which is shipped.
           - gettext\COPYING is the GPL-3.0 covering the gettext TOOLS, while
             the DLL we ship is libintl under LGPL-2.1.
         So each row also declares a string that MUST occur in its Source, and
         all three of the above fail on it.

    Exit code 0 when the staged tree and the table agree, 1 otherwise.

.PARAMETER StageDir
    The staged tree to check. Defaults to what stage.ps1 writes.

.PARAMETER GtkPrefix
    The gvsbuild prefix that "prefix:" Sources resolve against. Same resolution
    order as stage.ps1, so the two cannot disagree about which GTK is in play.

.PARAMETER Report
    Print the full table and every problem, and exit 0 regardless. For looking
    at where things stand without gating anything on it.

.PARAMETER SelfTest
    Prove the four conditions can fail, then exit. See the block at the bottom.

.EXAMPLE
    .\packaging\windows\verify-licenses.ps1
#>
[CmdletBinding()]
param(
    [string]$StageDir,
    [string]$GtkPrefix = $(if ($env:SCRIB_GTK_PREFIX) { $env:SCRIB_GTK_PREFIX } else { "C:\gtk-build\gtk\x64\release" }),
    [string]$RepoRoot,
    [switch]$Report,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'

# THESE DEFAULTS ARE FILLED IN HERE AND NOT IN THE PARAM BLOCK, and the reason is
# a Windows PowerShell behaviour worth knowing before writing another entry point.
# MEASURED on 5.1: when a script is started with `powershell.exe -File script.ps1`,
# $PSScriptRoot is EMPTY while parameter defaults are being evaluated, and correct
# by the time the body runs. Started with `-Command "& script.ps1"` -- or dot-sourced,
# or called as .\script.ps1 -- it is correct in both places.
#
# So "$PSScriptRoot\..\.." in a param default silently becomes "\..\.." under -File,
# which resolves against the ROOT OF THE CURRENT DRIVE. The failure is not a missing
# variable; it is a path that still looks like a path.
#
# This matters here because .github\workflows\pipeline.yml:562 starts the Windows
# packaging entry point with exactly `powershell -NoProfile -File`. NOT RUN: whether
# pwsh 7 behaves the same. pwsh is not installed on the box these measurements were
# taken on, and the contract-windows job runs pwsh while execute-windows runs 5.1,
# so the two engines meeting the same script is not hypothetical here.
if (-not $StageDir) { $StageDir = Join-Path $PSScriptRoot '..\..\build\stage\Scribobulate' }
if (-not $RepoRoot) { $RepoRoot = Join-Path $PSScriptRoot '..\..' }

# ---------------------------------------------------------------------------
# The evaluator, kept free of the filesystem so the self-test drives the SAME
# code the gate runs rather than a re-implementation of it. A self-test that
# re-states the rule in its own words certifies the restatement.
#
# $ReadSource is handed a row's Source string and returns its text, or $null
# when there is nothing there.
# ---------------------------------------------------------------------------
function Get-LicenseTableProblem {
    param(
        [Parameter(Mandatory)] [object[]]   $Rows,
        [Parameter(Mandatory)] [AllowEmptyCollection()] [string[]] $Staged,
        [Parameter(Mandatory)] [scriptblock] $ReadSource
    )

    $problems = @()

    # Condition 1, and the ambiguity case with it. Two rows matching one file is
    # not a harmless overlap: it means the file is attributed twice, and which
    # attribution a reader believes depends on table order.
    foreach ($path in $Staged) {
        $hit = @($Rows | Where-Object { $path -match $_.Match })
        if ($hit.Count -eq 0) {
            $problems += [pscustomobject]@{
                Condition = 1
                Id        = ''
                Detail    = "staged file attributed to nothing: $path"
            }
        } elseif ($hit.Count -gt 1) {
            $problems += [pscustomobject]@{
                Condition = 1
                Id        = ($hit.Id -join '+')
                Detail    = "staged file matches $($hit.Count) rows ($($hit.Id -join ', ')): $path"
            }
        }
    }

    foreach ($row in $Rows) {

        # Condition 2.
        $matched = @($Staged | Where-Object { $_ -match $row.Match })
        if ($matched.Count -eq 0) {
            $problems += [pscustomobject]@{
                Condition = 2
                Id        = $row.Id
                Detail    = "row matches no staged file; pattern was $($row.Match)"
            }
        }

        # Conditions 3 and 4. Source and Expect are each one string OR a list of
        # them, because a component's terms are not always one document: cairo is
        # dual-licensed and its own COPYING is a summary naming two texts, so a
        # row that could hold only one Source would have to pick which half of
        # cairo's licence to ship. EVERY Expect must occur SOMEWHERE across the
        # concatenation -- asserting one of a dual licence's two texts is the same
        # half-measure the fourth condition exists to catch.
        #
        # An empty file counts as missing: a zero-byte licence is the same absence
        # with a filename on it.
        $all = ''
        foreach ($s in @($row.Source)) {
            $text = & $ReadSource $s
            if ($null -eq $text -or $text.Length -eq 0) {
                $problems += [pscustomobject]@{
                    Condition = 3
                    Id        = $row.Id
                    Detail    = "licence text missing or empty: $s"
                }
            } else {
                $all += $text
            }
        }
        if ($all.Length -gt 0) {
            foreach ($e in @($row.Expect)) {
                if (-not $all.Contains($e)) {
                    $problems += [pscustomobject]@{
                        Condition = 4
                        Id        = $row.Id
                        Detail    = "licence text does not contain the expected string '$e': $(@($row.Source) -join ', ')"
                    }
                }
            }
        }
    }

    return $problems
}

function Import-LicenseTable {
    param([Parameter(Mandatory)] [string] $Path)
    $data = Import-PowerShellDataFile -LiteralPath $Path
    if (-not $data.Rows -or $data.Rows.Count -eq 0) {
        throw "No rows in $Path"
    }
    # Ids must be unique, or the gate's own output stops naming one thing.
    # Group-Object { $_.Id }, not Group-Object Id: the rows are Hashtables, and
    # the property form groups every one of them under the empty string, which
    # reports a duplicate on any table at all -- including a correct one.
    $dupes = @($data.Rows | Group-Object { $_.Id } | Where-Object Count -gt 1)
    if ($dupes.Count) { throw "Duplicate row Ids in ${Path}: $(($dupes.Name) -join ', ')" }
    return $data.Rows
}

# Resolve "prefix:" and "repo:" against the two roots. Anything else is a typo
# in the table rather than a path that happens not to exist, so it throws.
function New-SourceReader {
    param([string] $Prefix, [string] $Repo)
    return {
        param([string] $Source)
        if ($Source -like 'prefix:*') {
            $full = Join-Path $Prefix $Source.Substring(7)
        } elseif ($Source -like 'repo:*') {
            $full = Join-Path $Repo $Source.Substring(5)
        } else {
            throw "Source must start with 'prefix:' or 'repo:', got: $Source"
        }
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { return $null }
        return (Get-Content -LiteralPath $full -Raw)
    }.GetNewClosure()
}

# ---------------------------------------------------------------------------
if ($SelfTest) {
    # WHAT THIS ASSERTS. Not that the checker runs -- that it can say NO, once
    # per condition, and that it says yes when it should. A gate is only worth
    # its exit code if a violation actually changes it, and the way that fails
    # in practice is not a wrong rule but a rule the input never reaches.
    #
    # Every case below drives Get-LicenseTableProblem, the function the real run
    # calls, over a hand-built tree and a hand-built table.
    $bad = 0
    Write-Host '-- verify-licenses self-test --' -ForegroundColor Cyan

    $goodRow = @{
        Id = 'demo'; Component = 'Demo'; Match = '^bin\\demo\.dll$'
        License = 'MIT'; Source = 'repo:DEMO'; Expect = 'GRANT'
        Evidence = 'M'; Basis = 'fixture'
    }
    $reader = { param($s) if ($s -eq 'repo:DEMO') { 'text with GRANT inside' } else { $null } }

    # Baseline: agreement must produce nothing. Without this the three failure
    # cases below are also satisfied by a checker that always complains.
    $p = @(Get-LicenseTableProblem -Rows @($goodRow) -Staged @('bin\demo.dll') -ReadSource $reader)
    if ($p.Count -ne 0) {
        Write-Host "   FAIL: a table that agrees with the tree reported $($p.Count) problem(s)" -ForegroundColor Red
        $p | ForEach-Object { Write-Host "     $($_.Detail)" -ForegroundColor Red }
        $bad++
    }

    $cases = @(
        @{ N = 1; Why = 'a staged file no row claims'
           Rows = @($goodRow); Staged = @('bin\demo.dll', 'bin\stowaway.dll'); Reader = $reader },
        @{ N = 1; Why = 'a staged file two rows claim'
           Rows = @($goodRow, ($goodRow.Clone() + @{})); Staged = @('bin\demo.dll'); Reader = $reader },
        @{ N = 2; Why = 'a row matching nothing in the tree'
           Rows = @($goodRow); Staged = @(); Reader = $reader },
        @{ N = 3; Why = 'a row whose licence text is absent'
           Rows = @($goodRow); Staged = @('bin\demo.dll'); Reader = { param($s) $null } },
        @{ N = 3; Why = 'a row whose licence text is empty'
           Rows = @($goodRow); Staged = @('bin\demo.dll'); Reader = { param($s) '' } },
        @{ N = 4; Why = 'a row whose licence text is the wrong licence'
           Rows = @($goodRow); Staged = @('bin\demo.dll'); Reader = { param($s) 'a different licence entirely' } },

        # The two multi-document cases. A dual-licensed row that ships one of its
        # two texts, or asserts one of its two anchors, is the exact half-measure
        # this file is against -- and it is the state a row lands in when someone
        # adds a second Source and forgets the second Expect.
        @{ N = 3; Why = 'a row with two Sources, one of them absent'
           Rows = @(@{ Id = 'dual'; Component = 'Dual'; Match = '^bin\\demo\.dll$'
                       License = 'A OR B'; Source = @('repo:ONE', 'repo:TWO'); Expect = 'GRANT'
                       Evidence = 'M'; Basis = 'fixture' })
           Staged = @('bin\demo.dll')
           Reader = { param($s) if ($s -eq 'repo:ONE') { 'has GRANT' } else { $null } } },
        @{ N = 4; Why = 'a row with two Expects, only one of them present'
           Rows = @(@{ Id = 'dual'; Component = 'Dual'; Match = '^bin\\demo\.dll$'
                       License = 'A OR B'; Source = 'repo:ONE'; Expect = @('GRANT', 'OTHER GRANT')
                       Evidence = 'M'; Basis = 'fixture' })
           Staged = @('bin\demo.dll')
           Reader = { param($s) 'has GRANT and nothing else' } }
    )

    foreach ($c in $cases) {
        # The duplicate-Id case needs two rows with the SAME Match and different
        # Ids; Clone gives a shallow copy so changing the Id does not change both.
        $rows = $c.Rows
        if ($c.Why -eq 'a staged file two rows claim') {
            $second = $goodRow.Clone()
            $second.Id = 'demo-again'
            $rows = @($goodRow, $second)
        }
        # $fired, not $hit: the evaluator above uses $hit for its own match set,
        # and while the two never meet at runtime, the identical surrounding line
        # made a mutation aimed at the evaluator land on this assertion as well --
        # which disabled the check that was supposed to catch the mutation, so the
        # mutant survived and looked like a blind self-test. Distinct names keep a
        # mutant aimable at one of the two.
        $p = @(Get-LicenseTableProblem -Rows $rows -Staged $c.Staged -ReadSource $c.Reader)
        $fired = @($p | Where-Object { $_.Condition -eq $c.N })
        if ($fired.Count -eq 0) {
            Write-Host "   FAIL: condition $($c.N) did not fire for $($c.Why)" -ForegroundColor Red
            $bad++
        }
    }

    # The table that ships must parse and have unique Ids, checked here so a
    # malformed table is caught by the self-test and not only by a staged run,
    # which needs a built tree and a GTK prefix to reach.
    try {
        $rows = Import-LicenseTable -Path (Join-Path $PSScriptRoot 'licenses.psd1')
        if ($rows.Count -lt 30) {
            Write-Host "   FAIL: licenses.psd1 parsed to only $($rows.Count) rows" -ForegroundColor Red
            $bad++
        }
        foreach ($r in $rows) {
            foreach ($f in @('Id','Component','Match','License','Source','Expect','Evidence','Basis')) {
                if (-not $r.ContainsKey($f) -or [string]::IsNullOrWhiteSpace([string]$r[$f])) {
                    Write-Host "   FAIL: row '$($r.Id)' is missing field $f" -ForegroundColor Red
                    $bad++
                }
            }
            if ($r.Evidence -cne 'M' -and $r.Evidence -cne 'I') {
                Write-Host "   FAIL: row '$($r.Id)' has Evidence '$($r.Evidence)', expected M or I" -ForegroundColor Red
                $bad++
            }
            if ($r.Source -notlike 'prefix:*' -and $r.Source -notlike 'repo:*') {
                Write-Host "   FAIL: row '$($r.Id)' Source '$($r.Source)' has no prefix:/repo: scheme" -ForegroundColor Red
                $bad++
            }
            try { [void][regex]::new($r.Match) } catch {
                Write-Host "   FAIL: row '$($r.Id)' Match is not a valid regex: $($r.Match)" -ForegroundColor Red
                $bad++
            }
        }
    } catch {
        Write-Host "   FAIL: licenses.psd1 did not load: $_" -ForegroundColor Red
        $bad++
    }

    if ($bad -eq 0) {
        Write-Host '   self-test: OK' -ForegroundColor Green
        exit 0
    }
    Write-Host "   self-test: $bad failure(s)" -ForegroundColor Red
    exit 1
}

# ---------------------------------------------------------------------------
if (-not (Test-Path -LiteralPath $StageDir)) {
    throw "Staged tree not found at $StageDir. Run packaging\windows\stage.ps1 first."
}

$rows   = Import-LicenseTable -Path (Join-Path $PSScriptRoot 'licenses.psd1')
$root   = (Resolve-Path -LiteralPath $StageDir).Path
$staged = @(Get-ChildItem -LiteralPath $root -Recurse -File |
            ForEach-Object { $_.FullName.Substring($root.Length + 1) })

Write-Host ("Checking {0} staged files against {1} rows" -f $staged.Count, $rows.Count)

$problems = @(Get-LicenseTableProblem -Rows $rows -Staged $staged `
                                      -ReadSource (New-SourceReader -Prefix $GtkPrefix -Repo $RepoRoot))

if ($Report) {
    Write-Host ''
    Write-Host 'Table' -ForegroundColor Cyan
    foreach ($r in $rows) {
        $n = @($staged | Where-Object { $_ -match $r.Match }).Count
        Write-Host ("  [{0}] {1,-22} {2,-46} {3,4} file(s)  {4}" -f `
                    $r.Evidence, $r.Id, $r.License, $n, $r.Source)
    }
}

if ($problems.Count -eq 0) {
    Write-Host 'Licences: PASS' -ForegroundColor Green
    exit 0
}

Write-Host ''
foreach ($n in 1, 2, 3, 4) {
    $these = @($problems | Where-Object { $_.Condition -eq $n })
    if ($these.Count -eq 0) { continue }
    $title = switch ($n) {
        1 { 'staged files with no licence entry' }
        2 { 'licence entries with no staged file' }
        3 { 'licence entries whose text is missing' }
        4 { 'licence entries whose text is not the licence they name' }
    }
    Write-Host ("Condition {0} -- {1} ({2})" -f $n, $title, $these.Count) -ForegroundColor Red
    foreach ($p in $these) { Write-Host ("   {0,-22} {1}" -f $p.Id, $p.Detail) }
}

if ($Report) { exit 0 }
Write-Host ''
Write-Host ("Licences: FAIL ({0} problem(s))" -f $problems.Count) -ForegroundColor Red
exit 1
