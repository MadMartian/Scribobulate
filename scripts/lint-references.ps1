<#
.SYNOPSIS
    POLICY.md § "Build pipeline" step 9 — static reference lint. PowerShell port of
    scripts/lint-references.sh, for Windows developers who have no bash.

.DESCRIPTION
    Eight checks, all cheap and all static:

      1. No `sdd/ISSUES.md` entry is cited from outside that file (SDD principle 6).
         Issues exist in order to be deleted, so every reference to one is born with
         an expiry date.

      2. Every `ScrAP-N` cited in src/ actually exists in `sdd/ANTI-PATTERNS.md`.
         A citation to a number nobody has written yet is a dead pointer.

      3. Every `ScrAP-N` cited INSIDE `sdd/ANTI-PATTERNS.md` has a `## N.` body in
         that same file. Check 2 scans src/ against the register, so it cannot see a
         register entry citing another entry that is not there -- and
         register-to-register is exactly where a cross-branch transfer breaks, because
         a lesson authored on a port branch may cite a sibling that stays on it. A
         transfer is also precisely when nobody is looking.

      4. `src/lib.rs` and `src/gtk_suite.rs` declare the same module list. The suite
         is a second crate root that must re-declare it, and the duplication fails
         SILENTLY -- a module missing from the suite root drops every
         `#[gtktest::test]` inside it, with nothing erroring.

      5. `#[gtk::test]` has not returned in place of `#[gtktest::test]`. The wrong
         attribute still passes on Linux while omitting the body from the only run
         available where GTK must initialise on the main thread.

      6. Every document path this tree points at still exists. Plan files are deleted
         by design once implemented, and each "see PLAN.x.md" written while one
         existed dangles the moment it goes -- as does each bare `PLAN.<topic>`
         SECTION citation, which is the form code comments actually use and the form
         that let 21 danglers pass a clean gate.

      7. The application ID is not restated inconsistently. src/icons.rs is its
         source of truth; the desktop entry, GResource manifest, Info.plist template
         and the install/uninstall scripts all repeat the literal, and a change to
         one of them breaks a different platform's icon or Launch Services
         registration without failing any build.

      8. No bare `AP-N` citation exists. Two anti-pattern registers are in play with
         unrelated numbering -- this project's (`ScrAP-N`) and the gtk4-rs skill's
         (`GTK4Rs/AP-N`) -- and a bare number names neither unambiguously: it was the
         project's spelling historically and the skill's later, so its correct and
         incorrect uses are textually identical. Both legal forms are single tokens on
         purpose; a two-word form is split by any wrap, which is what makes the
         unverifiable set unenumerable. See ScrAP-231.

    The file set is NOT written here. It comes from scripts/lint-references.scan, the
    one enumeration definition this script and the bash gate both consume, and EVERY
    check above draws from it -- each narrowing that one set rather than walking the
    tree for itself. Run -ListScan on each platform and diff the output: that is the
    only available proof the two agree on the SET, and the set is where they had
    drifted while POLICY claimed they could not. The output is ordinal-sorted on both
    sides so a clean state is an EMPTY diff.

    The contract's `maxdepth` is a TRIPWIRE, not a filter: a file past the budget makes
    this gate refuse to run and name the path. That is what makes one shared set safe --
    a budget that silently truncated would make the gate leniently incomplete without
    saying so.

    Exits non-zero if any check fails, and names every offending reference rather
    than only reporting a count.

.NOTES
    ── WHAT THIS CANNOT DO. Read before trusting a PASS. ──────────────────────────
    Check 2 proves an entry EXISTS. It does NOT prove it is the RIGHT one. The
    GTK4Rs/AP-106 error that prompted this gate passes both checks cleanly, because 61 is
    a real entry -- it was simply the wrong one for the claim it was attached to.
    A gate that looks like it covers citations, and doesn't, is worse than no gate,
    because the next author trusts the PASS. Nothing here substitutes for reading
    the entry you are citing.

    ── WHY THERE IS A --self-test, AND WHY IT IS NOT OPTIONAL. ───────────────────
    The first version of this gate shipped with a pattern that missed THREE forms it
    was believed to cover: `sdd/ISSUES.md entry P`, `ISSUES.md entry P`, and
    `ISSUES: P` (the last fails the old pattern because `[ .:_-]` consumes the colon
    and then demands a capital immediately, but a space follows). One of those --
    `sdd/ISSUES.md entry P` -- was the exact form of a defect the gate was written to
    catch, so it would have passed the case that motivated it.

    A gate must be PROVEN TO FIRE, not assumed to. `--self-test` runs the pattern
    against a corpus of must-match and must-not-match strings and fails if either
    direction breaks. The corpus is deliberately shared, string-for-string, with
    scripts/lint-references.sh: that is what makes the two implementations checkable
    against the SAME evidence rather than against each other's prose.

    Run it whenever the pattern changes:  .\scripts\lint-references.ps1 -SelfTest

    ── HOW THE WRONG-NUMBER SLIP HAPPENS: citation proximity drift. ───────────────
    `dnd.rs` and `lifecycle.rs` both cite GTK4Rs/AP-106 correctly, for "the window's
    single caret overlay" -- and the very next clause in each says "detach it
    before...". So the number reads as though it belongs to the detach. A third call
    site was then written as "GTK4Rs/AP-106's shape" by pattern-matching two siblings
    whose local evidence pointed the wrong way. The detach obligation is GTK4Rs/AP-80.
    The only real guard is placing a number against the claim it supports, and
    re-reading it when you copy a sibling comment.

    ── WHY THIS IS STEP 9 AND NOT STEP 1. ────────────────────────────────────────
    A static check logically belongs beside `cargo fmt`. It is appended anyway,
    because renumbering the pipeline would invalidate every existing reference to a
    step: pipeline.ps1 and the port ledger say "step 5", coverage.sh's own header
    says "step 6", MANUAL-TEST cites launch items by step. Renumbering to make an
    ordering prettier would manufacture exactly the dangling-reference class this
    gate exists to catch. Do not "tidy" it.

.EXAMPLE
    .\scripts\lint-references.ps1
    .\scripts\lint-references.ps1 -SelfTest
    .\scripts\lint-references.ps1 -ListScan   # diff against bash --list-scan
#>
[CmdletBinding()]
param(
    [switch] $SelfTest,
    # Prints the enumerated scan set, one repo-relative path per line, and exits.
    # No test can run both implementations -- neither platform has the other's shell --
    # so running this on each and diffing the output is the only available proof that
    # they agree on the SET, which is the half a shared regex corpus cannot show, and
    # the half that had actually drifted.
    [switch] $ListScan
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
$failed = $false

# Repo-relative, forward-slashed path, for both the exclusion tests and the reported
# output. Every scanned file is found under $repo, so trimming that prefix is the whole
# job.
#
# Do NOT "modernise" this to `[IO.Path]::GetRelativePath`. That method is .NET 5+ only,
# so it exists under `pwsh` and NOT under Windows PowerShell 5.1 -- which runs on .NET
# Framework and is what a stock Windows box has. It threw MethodNotFound on the FIRST
# check, so this gate did not run at all there, and the error named `Where-Object`
# rather than the missing API. `-SelfTest` never reaches this line, so it kept
# reporting OK: a green that means nothing (compare ScrAP-165's false confirmation).
# Substring works on both runtimes; keep it that way.
# Tolerant of a path that is ALREADY repo-relative, because the scan set is now carried
# as repo-relative strings and `Select-String` hands back whatever form it was given.
# Substring on a path that does not start with $repo would silently slice the wrong
# characters off a relative one, which is a wrong ANSWER rather than an error.
function Get-RepoRelativePath {
    param([Parameter(Mandatory)] [string] $Path)
    $p = $Path.Replace('\', '/')
    $prefix = $repo.Replace('\', '/') + '/'
    if ($p.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $p.Substring($prefix.Length)
    }
    $p
}

# The ONE place check 12's Win32 path-legality predicate lives, so its corpus drives the
# CHECK rather than a re-implementation of it. Kept structurally paired with the bash
# port's `win_illegal_path`, which is TWO STAGES for a reason that does not apply here
# but must not be optimised away: the bash port had this as a single `grep -qP`, and
# `grep -P` is absent from the BSD grep macOS ships, so check 12 was a permanent false
# green on that platform. .NET regex has both `(?i)` and `\x01-\x1f`, so this port
# expresses the same rule in one pass — the RULE is shared, the spelling cannot be.
#
# The newline case is explicit here for the same reason it is there: it is the member of
# the control-character class most likely to occur, and the likeliest to be lost to a
# line-oriented tool.
# PER COMPONENT, not per path. The device-name rule was anchored `(^|/)...$` and the
# trailing dot/space rule to `[. ]$`, so both only ever saw the LAST component. A DIRECTORY
# named for a reserved device breaks a checkout exactly as a file does, and git refuses the
# whole tree either way.
#
# MEASURED against real `git clone` on git 2.49.0.windows.1 -- paths planted with
# `update-index --cacheinfo core.protectNTFS=false`, then cloned with default settings:
#
#   REFUSED   src/nul/thing.rs   a./b.md   a /b.md   src/com1/x.rs
#             deep/a/b/nul/c/x.rs   src/nul.txt/x.rs   src/con.foo/x.rs   src/lpt0/x.rs
#   ACCEPTED  ok/fine.md   src/console/x.rs   src/nullable/x.rs   src/a.b/x.rs
#             src/com0/x.rs   src/conin/x.rs   src/conout/x.rs
#
# The old rule was blind to every one of the eight refusals except a final-component one.
#
# LPT[0-9], NOT LPT[1-9], AND THAT IS DELIBERATE. Measured, twice, in isolation: git
# REFUSES `lpt0` while the filesystem ACCEPTS it -- `Directory.CreateDirectory` makes
# `lpt0` and `com0` happily, and git refuses `lpt0` but accepts `com0`. The asymmetry is
# git's, not this gate's. This check exists to predict "git checkout refuses the whole
# tree", so it encodes what GIT refuses; a path git will not write is unusable here whether
# or not Win32 would have allowed it. Version-scoped: measured on 2.49.0.windows.1, and if
# a later git drops the quirk this rule is merely one name stricter than necessary, which
# is the safe direction for a gate.
#
# Trailing dot and space are not refused by Win32 itself -- they are SILENTLY STRIPPED.
# Measured: creating `a.` and `a ` both yield a directory called `a`, so two distinct
# contract paths collapse onto one. For a tracked tree that is worse than a refusal, since
# git then reports the file missing forever. git refuses them outright, which is why they
# belong here rather than in a warning.
function Test-WinIllegalPath {
    param([string]$Path)
    # Whole-path rules: these characters are illegal wherever they appear.
    if (($Path -match "`n") -or ($Path -match '[<>:"|?*]') -or ($Path -match '[\x01-\x1f]')) {
        return $true
    }
    # Component rules. `/` is the separator in every path this gate sees (git ls-files),
    # so splitting on it is exact rather than a guess.
    foreach ($seg in $Path.Split('/')) {
        if ($seg -match '[. ]$') { return $true }
        if ($seg -match '(?i)^(CON|PRN|AUX|NUL|COM[1-9]|LPT[0-9])(\..*)?$') { return $true }
    }
    return $false
}

# Matches `ISSUES P`, `ISSUES-P`, `ISSUES_P`, `ISSUES: P`, `ISSUES.md entry P`, and
# `sdd/ISSUES.md entry P`. Kept byte-identical to the bash gate's pattern so the two
# platforms enforce the SAME rule -- a lint whose strictness depends on who ran it is
# worse than one that is merely incomplete, because a clean run then means nothing
# without knowing the platform.
#
# The trailing `\b` after `[A-Z]` is load-bearing: an issue ID is a SINGLE LETTER
# STANDING ALONE, not the first letter of a word, so it is what keeps
# `sdd/ISSUES.md TOC` from firing.
#
# The second alternative catches the `#`-sigil form (`ISSUES #P`, `ISSUES #BBB`), which
# the single-letter branch cannot: `#` is not a separator it accepts, and a MULTI-letter
# ID (this register has used `WW`/`ZZ`/`BBB`) never satisfies `[A-Z]\b`. Three such
# references were sitting in `tests/MANUAL-TEST.md`, all naming issues long since
# resolved -- dangling exactly as principle 6 predicts -- while this gate passed. A
# multi-letter ID is only recognised BEHIND the `#`, because bare uppercase runs after
# `ISSUES` are ordinary prose (`ISSUES.md TOC`, `ISSUES.md IDs`).
#
# The connecting word is `[a-z]+` -- ANY lowercase word, not an enumeration. It was
# `(entry )?`, the one noun the pattern's author happened to write, and
# an `ISSUES.md item <letter>` citation walked through it into `src/`, in the very commit
# that rewrote the entry it named. That is this script's OWN recorded failure (check 6 passing over 21 danglers
# because its pattern required a `.md` the citations did not carry) recurring one check
# across -- which is also the argument against fixing it by adding `item` to the list:
# the next citation says `issue`, or `letter`, or `row`. Enumerating the vocabulary of a
# free-text citation is a losing game, so match the SHAPE instead: `ISSUES`, a
# connector, a lone capital. The discrimination still comes from `[A-Z]\b` and
# case-sensitivity, and a lowercase connector cannot reach a capitalised document name,
# so `read ISSUES.md and TECH.md together` stays clean. Both engines agree here because
# the check is boolean -- leftmost-longest vs leftmost-first cannot change whether a
# match exists, only which one is reported, and nothing reads the capture.
# The third alternative is the TITLE form, and it is here because check 1 reported PASS
# over a live violation: `probes/native-chooser-rss.m` cited an entry by its QUOTED TITLE
# rather than its letter, and a pattern hunting letter IDs cannot see that. A title
# pointer dangles exactly as an ID pointer does -- the entry is deleted when fixed either
# way -- so the rule was right and only its pattern was narrow. Prose ABOUT the register
# is still legal, which is what the `mustNotMatch` corpus pins.
$issuePattern = '\bISSUES(\.md)?([ .:_-]*([a-z]+ )?[A-Z]\b|[ .:_-]*#[A-Z]+\b|[ .:_-]*"[^"]+")'

# The document-path forms check 6a must catch. Byte-identical to the bash gate's
# PATH_RX for the same reason $issuePattern is.
#
# The bare `PLAN.<topic>` alternative (no `.md`) is load-bearing, not a widening: the
# `.md`-only pattern passed clean over 21 live danglers, because code cites a SECTION
# of a plan -- `PLAN.help D3`, `PLAN.annotations-viewer root cause` -- and a section
# citation names the plan, not its filename. The `.md` alternative is listed FIRST so a
# full filename matches whole under BOTH engines: POSIX ERE takes leftmost-longest,
# .NET takes leftmost-first, and only this ordering makes them agree.
$pathPattern = '((sdd|tests|packaging|gtktest|scripts|data|src)/[A-Za-z0-9._/-]+\.md|\bPLAN\.[A-Za-z0-9._-]+\.md|\bPLAN\.[A-Za-z0-9_-]+)'

# The ONE place check 6a's path extraction happens, flags included -- so the self-test can
# drive the check's own invocation instead of re-implementing it. `-CaseSensitive` is
# LOAD-BEARING and lives HERE, not at the call site: `Select-String` and `-match` are
# case-INSENSITIVE by default in PowerShell and `grep -E` is not, so without it this gate is
# strictly stricter than its bash twin over identical source -- measured, the bare-PLAN arm
# matched the Rust field accesses `plan.switch_to` and `plan.spot` in
# `window/navhistory/traverse.rs`, failing check 6 here and passing on Linux. That is the
# gate-parity divergence POLICY step 9 says no automated test can catch.
#
# It went unseen because the self-test used to validate `$pathPattern` through
# `[regex]::Match`, which IS case-sensitive -- a corpus over a re-implementation is evidence
# about a pattern, never about the check. Mutation-tested by the windows seat: with the
# corpus fixed but the flag still at the call site, removing the flag left the self-test
# GREEN. Check 8's corpus comment already said this ("it exercises the predicate the check
# itself calls, not the two patterns in isolation"); 6a is the one that had not followed it.
function Get-PathHits {
    param([string]$File)
    Select-String -LiteralPath $File -Pattern $pathPattern -AllMatches -CaseSensitive -ErrorAction SilentlyContinue
}

# Check 8's two patterns, byte-identical to the bash gate's BARE_AP_RX and LEGAL_AP_RX.
# A BARE `AP-N` is illegal anywhere in the tree: two registers are in play with unrelated
# numbering (`ScrAP-N` here, `GTK4Rs/AP-N` for the gtk4-rs skill), and the bare form is
# simultaneously the current skill spelling and the historical project spelling -- the
# same string is correct or wrong depending on the day it was typed, with nothing in the
# text to say which.
#
# BOTH LEGAL FORMS ARE SINGLE TOKENS, deliberately. The first spelling of this convention
# was `skill AP-N`, with a space, and two citations in the tree were ALREADY split across
# a Markdown/rustfmt line wrap when the gate was written. That is not cosmetic: the whole
# justification for legalising an unverifiable citation is that it becomes ENUMERABLE, and
# `grep 'skill AP-'` silently misses every wrapped one, so the human-audit set was
# incomplete while looking exhaustive. Nothing wraps inside a whitespace-free token. It
# also makes the SPELLING enforced -- `/` is not in `[a-zA-Z-]`, so `gtk4rs/AP-9` survives
# stage 1 and is reported, where `skilll AP-9` used to read as prose.
#
# TWO STAGES, NOT ONE REGEX. The rule is "an `AP-N` not preceded by `Scr` or `GTK4Rs/`",
# which is a lookbehind. .NET has lookbehind and POSIX ERE does not, and the bash twin
# cannot use `grep -P` (absent from the BSD grep macOS ships) -- so BOTH ports do it the
# portable way: find candidates with $bareApPattern (which excludes `ScrAP-N` by
# construction, `r` being in `[a-zA-Z-]`), DELETE $legalApPattern from the line, then
# re-apply the candidate pattern to what is left. Writing the natural .NET lookbehind
# here instead would be the lenient/strict split POLICY step 9 forbids -- and worse, it
# would look like a faithful port. A line reading `GTK4Rs/AP-57 / AP-34b` must be reported
# for the SECOND citation only, which is what the strip produces and a single regex
# cannot.
#
# One porting note that bit the bash twin and cannot bite here: `/` inside the pattern is
# a sed `s///` delimiter collision there (the twin uses `s|…|…|`), while
# `[regex]::Replace` takes the pattern as an argument and has no delimiter at all. Do not
# "harmonise" that difference away -- the patterns are shared, the substitution mechanism
# is each port's own business.
$bareApPattern  = '(^|[^a-zA-Z-])AP-[0-9]+'
$legalApPattern = 'GTK4Rs/AP-[0-9]+'

# The check-8 predicate, defined ONCE so -SelfTest exercises the program the gate runs
# rather than a copy of it -- the same rule the bash twin's `is_bare_ap` follows, and the
# same one check 5b's shared program exists for.
function Test-BareAp {
    param([Parameter(Mandatory)] [AllowEmptyString()] [string] $Line)
    $stripped = [regex]::Replace($Line, $legalApPattern, '')
    [regex]::IsMatch($stripped, $bareApPattern)
}

# ── The shared scan-set contract ───────────────────────────────────────────────
# scripts/lint-references.scan is the ONE enumeration definition this script and the
# bash gate both consume. Read its header for why. The short version: this script
# enumerated seven named directories plus four named files while bash swept the repo,
# so `.agents/`, `docs/` and `THIRD-PARTY-LICENSES.md` were linted there and invisible
# here -- while POLICY claimed neither script could be the lenient one. Pinning the
# pattern was never enough; a gate is its pattern AND the set it runs over.
$scanContract = Join-Path $PSScriptRoot 'lint-references.scan'
function Get-ScanField {
    param([Parameter(Mandatory)] [string] $Name)
    $out = @()
    foreach ($line in $script:scanContractLines) {
        if ($line -match "^$Name\s+(\S+)\s*$") { $out += $Matches[1] }
    }
    # The leading comma is LOAD-BEARING, not style. Measured on PowerShell 5.1 under
    # `Set-StrictMode -Version Latest`: `return $out` with `$out = @()` yields $null,
    # and the caller's `.Count` then THROWS ("The property 'Count' cannot be found on
    # this object"). `return ,$out` yields an Object[] with .Count = 0. So this comma
    # is what holds the empty-field case together — the same empty-set case that made
    # the bash twin abort silently. Do not "tidy" it away.
    ,$out
}

# -Encoding UTF8 ON EVERY Get-Content IN THIS FILE, all nine call sites, and it is not
# optional. PS 5.1's Get-Content sniffs a BOM and falls back to the ANSI CODEPAGE when
# there is none. `sdd/ANTI-PATTERNS.md` has no BOM and is UTF-8, so every em dash in it
# was decoded as three Windows-1252 characters.
#
# BOUNDED BEFORE IT WAS FIXED, because "add an encoding flag everywhere" is the kind of
# change that quietly moves a verdict: no verdict changes. Every pattern this file matches
# on is ASCII and line-anchored, a multi-byte sequence mis-decoded mid-line cannot produce
# a `^##` or a `^|`, and line splitting is on CR/LF which the misdecode leaves alone.
# MEASURED on the register under both decodings: 300 bodies and 300 TOC rows either way,
# Compare-Object empty. Check 11 reads the file's real byte length (630687B, matching
# `stat`), not a decoded string, so its warning is unaffected too.
#
# What it does change is the DIAGNOSTIC. Check 13's FAIL echoes the offending entry's
# heading, and it came back as `A portability gate ... on PATH â€” and`. That line is read
# by a human who is already dealing with a failure, which is the worst moment to hand
# someone a corrupted rendering of the thing that failed.
#
# The sibling port already knew: packaging/windows/pipeline.ps1 reads the contract with
# -Encoding UTF8 and carries a comment explaining this exact trap. Same author, same trap,
# one file knowing it and the other not is drift, not an open question. The shell twin has
# no analogue -- it is byte-transparent -- so nothing here needs a matching change there.
#
# `Get-Content -Raw` on an EMPTY file returns $null, and every method call on that
# ($null.Contains(...)) throws with a message naming neither the file nor the check —
# so a truncated input would be reported by the runtime instead of by the gate, and
# the gate's own diagnostic for that file becomes unreachable. That is the same defect
# class as the bash twin's silent abort: "no content" is a legitimate state that must
# be REPORTED, not a crash. Returns '' for missing, empty and whitespace-only alike.
function Get-FileText {
    param([Parameter(Mandatory)] [string] $Path)
    if (-not (Test-Path $Path)) { return '' }
    $raw = Get-Content -Encoding UTF8 $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) { return '' }
    $raw
}
if (-not (Test-Path $scanContract)) {
    Write-Host "lint-references: $scanContract is missing -- it is the shared scan-set" -ForegroundColor Red
    Write-Host '  contract this gate and its bash twin both read. Without it there is no' -ForegroundColor Red
    Write-Host '  file set to check, which is NOT the same as a clean tree.' -ForegroundColor Red
    Pop-Location
    exit 1
}
# Read ONCE, here, immediately after the existence check, and let both `Get-ScanField`
# and the validator below iterate this. Not a micro-optimisation: reading inside the
# function left the diagnostic above dependent on CALL ORDER, so moving one line would
# have handed a missing contract back to `Get-Content`, which under
# `ErrorActionPreference = 'Stop'` reports it as "Cannot find path '<absolute path>'"
# — the failure named by the shell rather than by the gate, which is the exact defect
# class this whole guard exists to close. One read after one check removes the ordering
# dependency instead of documenting it. `@()` so a single-line contract is still an
# array rather than a scalar.
$scanContractLines = @(Get-Content -Encoding UTF8 $scanContract)
$scanRoots    = Get-ScanField 'root'
$scanFiles    = Get-ScanField 'file'
$scanExcludes = Get-ScanField 'exclude'
# Fail fast on an empty/garbled contract, matching the bash twin. Its failure mode
# there is worse than an error -- with no roots, `grep -r` gets no file operands and
# reads STDIN, so the gate HANGS rather than failing. Here it would throw on the index
# below, which is loud but unhelpful; say what is actually wrong instead. An empty scan
# set must never be allowed to pass every check vacuously.
$scanMaxDepthRaw = Get-ScanField 'maxdepth'
$scanPrescriptive = Get-ScanField 'prescriptive'
# Same rule as below, its own message, because it fails differently. Measured on the
# bash side: delete the `prescriptive` lines and check 5b reports PASS -- over nothing.
# That is the failure this contract's header calls strictly worse than disagreement,
# since BOTH ports read the same file and would agree, cleanly and vacuously, while
# `-ListScan` showed no difference at all. The way to switch check 5b off is to delete
# check 5b.
if ($scanPrescriptive.Count -eq 0) {
    Write-Host "lint-references: $scanContract defines no 'prescriptive' document." -ForegroundColor Red
    Write-Host '  Refusing to run: check 5b would PASS over an empty set, which reads' -ForegroundColor Red
    Write-Host '  identically to a clean tree. Restore the class or remove the check.' -ForegroundColor Red
    Pop-Location
    exit 1
}
if ($scanRoots.Count -eq 0 -or $scanMaxDepthRaw.Count -eq 0) {
    Write-Host "lint-references: $scanContract defines no 'root' and/or no 'maxdepth'." -ForegroundColor Red
    Write-Host '  Refusing to run: an empty scan set would pass every check vacuously.' -ForegroundColor Red
    Pop-Location
    exit 1
}
# Reject any line that is not a comment, blank, or a recognised `<field> <value>`.
# A typo'd field name is otherwise INVISIBLE -- it fails to match, the field reads as
# absent, and absent is indistinguishable from legitimately empty. Both scripts read
# the SAME file, so they would agree perfectly on scanning nothing and the -ListScan
# diff (the only parity proof available) would come back identical and clean. Two
# gates agreeing vacuously is worse than the R1-03 disagreement that motivated this
# file, because the check meant to catch it reports success.
$badContract = @()
$lineNo = 0
foreach ($line in $scanContractLines) {
    $lineNo++
    if ($line -match '^\s*(#.*)?$') { continue }
    if ($line -notmatch '^(root|file|prescriptive|exclude|maxdepth)\s+\S+\s*$') {
        $badContract += ("{0}:{1}" -f $lineNo, $line)
    }
}
if ($badContract.Count) {
    Write-Host "lint-references: $scanContract has lines that are neither comments nor" -ForegroundColor Red
    Write-Host "  a recognised 'root|file|prescriptive|exclude|maxdepth <value>' pair:" -ForegroundColor Red
    foreach ($b in $badContract) { Write-Host ('    ' + $b) -ForegroundColor Red }
    Write-Host '  A misspelled field reads as an ABSENT field, and both gates would then' -ForegroundColor Red
    Write-Host '  agree on scanning nothing. Fix the line rather than deleting it.' -ForegroundColor Red
    Pop-Location
    exit 1
}
# `maxdepth` IS A TRIPWIRE, NOT A FILTER — see the contract's `maxdepth` paragraph and
# the matching block in the bash twin. Read the line below as-is: there is no longer a
# `find -maxdepth N` -> `Get-ChildItem -Depth N-1` conversion, because depth is derived
# from the repo-relative PATH rather than from the walker. That conversion used to be
# the one place the two implementations were licensed to differ, and nothing asserted it
# was right; removing the difference beats asserting it. The value keeps `find`'s
# meaning: segments BELOW a root.
$scanMaxDepth = [int]($scanMaxDepthRaw[0])

# ── Symlink-exclusion set ─────────────────────────────────────────────────────
#
# See scripts/lint-references.scan, "SYMLINKS ARE EXCLUDED", for why this exists and
# why the mechanism is the git index rather than a filesystem test. The short version
# of the Windows half: with core.symlinks=false git materialises the link as an
# ordinary 32-byte text file whose content is the target PATH, `.LinkType` is empty and
# the attributes are a plain file -- so the natural port of `find -type l`
# (.LinkType / ReparsePoint / ResolveLinkTarget) excludes NOTHING here and the two
# gates would still disagree while reading as parity.
#
# The index half is built here, once, from a SINGLE `git ls-files -s` call rather than
# one per file. The filesystem half is applied in Test-ScanExcluded via `.LinkType`,
# during the one walk, so the union costs no extra pass. Union, not either-or: each
# test is blind where the other sees, and it degrades to "include" only when both are
# blind -- the safe direction, since it can never silently skip an authored document.
#
# `git ls-files -s` emits `<mode> <sha> <stage>\t<path>`, so the mode is the first
# whitespace-delimited token and the path is everything after the TAB. Splitting on the
# tab rather than on whitespace is what keeps a path containing spaces intact.
#
# Deliberately tolerant of git being absent or this not being a checkout: the gate must
# degrade to the filesystem half, not die. Note the local 'Continue' -- with the
# script-level 'Stop' in force, the `2>$null` redirection below would promote git's
# "fatal: not a git repository" into a terminating error and abort the whole gate on a
# condition this block exists to survive. That is the same PowerShell trap that was
# measured in packaging/windows/pipeline.ps1: `2>&1`/`2>$null` on a native command is
# what makes its stderr fatal under 'Stop', not merely running it.
$symlinkIndexPaths = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase)
$idxLines = @()
$prevEAP = $ErrorActionPreference
try {
    $ErrorActionPreference = 'Continue'
    $idxLines = @(git ls-files -s 2>$null)
    if ($LASTEXITCODE -ne 0) { $idxLines = @() }
} catch {
    $idxLines = @()
} finally {
    $ErrorActionPreference = $prevEAP
}
foreach ($l in $idxLines) {
    if ($l -match '^120000\s') {
        $p = ($l -split "`t", 2)[1]
        if ($p) { [void] $symlinkIndexPaths.Add($p) }
    }
}

# ── The scan set: ONE enumeration, produced once, consumed by every check ─────
#
# There used to be three, and they disagreed. Measured by planting a Markdown file one
# level past the budget: check 6a was BOUNDED here (`-Depth`) and UNBOUNDED in the bash
# twin (`grep -r`), so the shell side caught a dangler at depth 7 and this side did not
# — a lenient/strict split in the one gate whose whole purpose is to prove the two
# platforms enforce the same rule. Check 1 drew from `Get-ScannedFiles` (no depth bound)
# while check 6 drew from here, so the two disagreed IN THE SAME RUN. And `-ListScan`,
# the only parity proof this project has, printed a bounded enumeration that some checks
# did not consume, which proves nothing about those checks (ScrAP-207).
#
# So: this block produces the set, `-ListScan` prints exactly it, and every check below
# narrows it with `Get-ScanSubset` rather than walking the filesystem again. A second
# walk is a second enumeration; there is no such thing as a harmless one here.
function Test-ScanExcluded {
    param([Parameter(Mandatory)] [string] $Rel, $LinkType)
    # ORDINAL, CASE-SENSITIVE, LITERAL prefix -- the same comparison the scan-set index and
    # the depth tripwire in this file already use, and NOT `-like`.
    #
    # `-like` was wrong twice over for a contract-driven prefix. It is case-INSENSITIVE
    # where the bash twin's `grep -E` is not, so `exclude Docs/` would have excluded
    # `docs/` here and nothing there -- a scan set whose membership depends on which
    # platform enumerated it, in the one gate whose entire purpose is proving the two
    # platforms see the same files. And it treats `[`, `?` and `*` as WILDCARDS, so an
    # exclude value containing any of them means something other than itself.
    #
    # A prefix from a contract is a literal. Compare it as one.
    foreach ($x in $scanExcludes) {
        if ($x -and $Rel.StartsWith($x, [System.StringComparison]::Ordinal)) { return $true }
    }
    # Union of the two symlink tests: the git index (authoritative on both platforms, and
    # the ONLY half that fires on a core.symlinks=false checkout) OR the filesystem
    # (which catches an untracked symlink the index cannot see). See the block above.
    if ($symlinkIndexPaths.Contains($Rel) -or $LinkType) { return $true }
    $false
}
function Get-ContractScanSet {
    $paths = @()
    foreach ($d in $scanRoots) {
        if (-not (Test-Path $d)) { continue }
        # `-Force` so a file carrying the Hidden attribute is still enumerated. `find` on
        # the other platform has no notion of that attribute, so without this the twin
        # scans a file this port cannot see — enumeration drift of exactly the kind this
        # contract exists to stop, and invisible until someone marks a file hidden.
        foreach ($f in (Get-ChildItem -Path $d -Recurse -File -Force -ErrorAction SilentlyContinue)) {
            $rel = Get-RepoRelativePath $f.FullName
            if (-not (Test-ScanExcluded $rel $f.LinkType)) { $paths += $rel }
        }
    }
    foreach ($n in $scanFiles) {
        if (-not (Test-Path $n)) { continue }
        $item = Get-Item -LiteralPath $n -Force
        $rel = Get-RepoRelativePath $item.FullName
        if (-not (Test-ScanExcluded $rel $item.LinkType)) { $paths += $rel }
    }
    # ORDINAL sort and ORDINAL de-duplication, which is what makes the parity proof
    # readable. `Sort-Object -Unique` collates case-insensitively by culture; the bash
    # twin's `sort -u` collates by locale. Measured at the time of writing — both ports
    # enumerated the same 235 files and the diff STILL showed fifteen changed lines,
    # purely from collation. A parity proof whose clean state is not byte-identical
    # trains its reader to skim it. Both ports now order by ordinal value, and the twin
    # pins `LC_ALL=C` for the same reason. (These agree because every path here is
    # ASCII; ordinal is UTF-16 code-unit order and `LC_ALL=C` is byte order.)
    #
    # Copied into a plain `string[]` rather than returned as the SortedSet, and that is
    # not ceremony. Returning the collection let PowerShell hand the CALLER a single
    # object instead of 235 strings, and the failure was silent in the worst way:
    # `$rel.StartsWith(...)` on an array does not throw under StrictMode, it invokes the
    # method on every ELEMENT and returns an array of booleans, which `if` reads as
    # true. Every file in the tree was then reported as past the depth budget. A typed
    # array has one meaning.
    $sorted = [System.Collections.Generic.SortedSet[string]]::new(
        [string[]] $paths, [System.StringComparer]::Ordinal)
    $out = New-Object 'string[]' $sorted.Count
    $sorted.CopyTo($out)
    ,$out
}
$scanSet = Get-ContractScanSet
if ($scanSet.Count -eq 0) {
    Write-Host "lint-references: $scanContract enumerates no file at all." -ForegroundColor Red
    Write-Host '  Refusing to run: every check would PASS over an empty set, which reads' -ForegroundColor Red
    Write-Host '  identically to a clean tree.' -ForegroundColor Red
    Pop-Location
    exit 1
}
$scanSetOrdinal = [System.Collections.Generic.HashSet[string]]::new(
    [string[]] $scanSet, [System.StringComparer]::Ordinal)

# `maxdepth` as a tripwire. As a FILTER it was a silent coverage cliff: a file past the
# budget was simply absent from the set, and "absent because the budget dropped it" is
# indistinguishable from "absent because the tree does not contain it". Binding every
# check to one enumeration would then have REDUCED what check 1 catches, silently —
# the exact shape of the defect this block removes. Failing loudly is what makes the
# single set safe, and it turns the budget into a statement that the tree has outgrown
# the contract. `file` entries match no root prefix and are never counted: they are
# named individually, not reached by a walk.
$tooDeep = @()
foreach ($rel in $scanSet) {
    foreach ($r in $scanRoots) {
        $p = $r + '/'
        if ($rel.StartsWith($p, [System.StringComparison]::Ordinal)) {
            if (($rel.Substring($p.Length) -split '/').Count -gt $scanMaxDepth) { $tooDeep += $rel }
            break
        }
    }
}
if ($tooDeep.Count) {
    Write-Host "lint-references: these files are deeper than $scanContract's 'maxdepth $scanMaxDepth':" -ForegroundColor Red
    foreach ($t in $tooDeep) { Write-Host ('    ' + $t) -ForegroundColor Red }
    # ASCII `--`, not an em dash, and this is not a style preference. This file has no
    # BOM, so Windows PowerShell 5.1 decodes it as ANSI: a UTF-8 em dash arrives as
    # `a-EUR-"`, whose last byte is U+201D, which PowerShell honours as a STRING
    # DELIMITER. Inside a comment that is merely ugly; inside a string literal it ends
    # the string and the file stops parsing several hundred lines later, with the error
    # pointing at an innocent `{`. Every other message in this script is ASCII for the
    # same reason.
    Write-Host "  Refusing to run. Raise 'maxdepth' in the contract -- then re-run -ListScan" -ForegroundColor Red
    Write-Host '  here and --list-scan on the bash side and diff them -- or move the files.' -ForegroundColor Red
    Write-Host '  Truncating the set instead would make this gate lenient WITHOUT SAYING SO.' -ForegroundColor Red
    Pop-Location
    exit 1
}

# The `prescriptive` class is a LABEL ON SET MEMBERS, not a fourth enumeration. Before
# this, check 5b read an explicit list of documents that was neither the bounded nor the
# unbounded set, so `-ListScan` did not print what 5b consumed either. Asserting
# membership is what makes the class a subset rather than a separate opinion.
foreach ($p in $scanPrescriptive) {
    if ($scanSetOrdinal.Contains($p)) { continue }
    Write-Host "lint-references: $scanContract marks '$p' prescriptive, but that path is not" -ForegroundColor Red
    Write-Host '  in the scan set -ListScan prints. A prescriptive document is a LABEL on a' -ForegroundColor Red
    Write-Host '  set member, not a separate list: check 5b would read a file no parity proof' -ForegroundColor Red
    Write-Host "  covers. Add a 'root'/'file' entry that contains it, or drop the label." -ForegroundColor Red
    Pop-Location
    exit 1
}

# Members of the contract set, narrowed. `$Pattern` is tested against each repo-relative
# path; `-Not` inverts. Every check draws its inputs from here and never from the
# filesystem — the twin's `scan_subset`/`scan_subset_not`.
function Get-ScanSubset {
    param([Parameter(Mandatory)] [string] $Pattern, [switch] $Not)
    $out = @()
    foreach ($rel in $script:scanSet) {
        if ([regex]::IsMatch($rel, $Pattern) -ne [bool]$Not) { $out += $rel }
    }
    # Leading comma for the same reason as Get-ScanField's: an empty $out returns $null
    # without it under StrictMode, and the caller's `.Count` then throws.
    ,$out
}

# ── Self-test: prove the pattern discriminates, in BOTH directions ─────────────
if ($SelfTest) {
    # Kept string-for-string with scripts/lint-references.sh's own corpus. That
    # sharing is the whole point: it makes the two implementations checkable against
    # the SAME evidence rather than against each other's prose. If you add a case
    # here, add the identical line there in the same change.
    $mustMatch = @(
        'see ISSUES P',
        'see sdd/ISSUES.md entry P for details',
        'see ISSUES.md entry P',
        'see ISSUES-P',
        'see ISSUES_P',
        'see ISSUES: P',
        '(TDD 17.31 / ISSUES #BBB)',
        'regression, see ISSUES #P',
        'see sdd/ISSUES.md #WW',
        '(ISSUES.md item P)',
        'see ISSUES issue P',
        'the ISSUES.md letter P',
        'attribute the growth (ISSUES.md "native file chooser RSS growth")',
        'see ISSUES "the rename suite finalizes a cancelled monitor"'
    )
    $mustNotMatch = @(
        'issues a queue_draw that CLEARS the cache',
        'see sdd/ISSUES.md',
        'the sdd/ISSUES.md TOC lists them',
        'the sdd/ISSUES.md IDs are ephemeral',
        'ISSUES.md is not a changelog',
        'the ISSUES register empties as it works',
        'ISSUES.md entries are deleted when fixed',
        'read ISSUES.md and TECH.md together',
        'the register lives in sdd/ISSUES.md and shrinks as it works'
    )
    # Check 12's corpus, kept string-for-string with the bash gate's. It shipped with NO
    # self-test in either port, which is the mechanism that would have caught the bash
    # port's `grep -P` on a BSD-grep host -- so the corpus is the fix and the pattern
    # change is only half of it. It drives `Test-WinIllegalPath`, the predicate the check
    # itself calls.
    #
    # `PLAN.console.md`, `contents.md` and `nullable.rs` are the negative controls that
    # matter: each CONTAINS a reserved device name and none of them IS one, so a
    # device-name rule written without the anchors would fail the whole tree.
    $winMustMatch = @(
        'a<b.md',
        'a>b.md',
        'a:b.md',
        'a"b.md',
        'a|b.md',
        'a?b.md',
        'a*b.md',
        'trailing.',
        'trailing ',
        'con',
        'CON.txt',
        'src/nul',
        'lpt9.md',
        'com1',
        'PRN.md',
        'src/dir/AUX.rs',
        # NON-FINAL components. Every one of these was planted in a real index and REFUSED
        # by `git clone` on 2.49.0.windows.1; the pre-M65 rule saw none of them.
        'src/nul/thing.rs',
        'src/com1/x.rs',
        'deep/a/b/nul/c/x.rs',
        'src/nul.txt/x.rs',
        'src/con.foo/x.rs',
        'src/lpt0/x.rs',
        'a./b.md',
        'a /b.md',
        # NOT "a`u{0001}b.md": `u{...} is a PowerShell 6+ escape. Windows PowerShell 5.1
        # does not recognise it and an unrecognised backtick escape yields the LITERAL
        # character, so that spelling produces the 12-char string `au{0001}b.md` -- a
        # corpus entry with no control character in it, which the predicate then
        # correctly refuses to flag. The self-test read as a predicate defect and was a
        # corpus defect. `[char]1` is the spelling both editions share.
        "a$([char]1)b.md",
        "a`nb.md"
    )
    $winMustNotMatch = @(
        'ok.md',
        'src/normal-file.rs',
        'contents.md',
        'nullable.rs',
        'PLAN.console.md',
        'sdd/ANTI-PATTERNS.md',
        'scripts/lint-references.sh',
        'a-b_c.rs',
        'Cargo.toml',
        # The negative controls that matter for the PER-COMPONENT rule: each contains a
        # device name inside a directory component without BEING one, and each was planted
        # and ACCEPTED by the same real `git clone`. A per-component rule written without
        # its anchors would reject the whole src tree.
        'src/console/x.rs',
        'src/nullable/x.rs',
        'src/a.b/x.rs',
        # COM0 and LPT0 are the asymmetry, measured: git accepts com0 and refuses lpt0.
        # This pins the accepting half, so widening COM[1-9] to COM[0-9] "for symmetry"
        # fails here rather than quietly rejecting a legal path.
        'src/com0/x.rs',
        'src/conin/x.rs',
        'src/conout/x.rs'
    )
    # Check 6a's corpus, kept string-for-string with the bash gate's. The
    # `expected|line` form pins WHAT is extracted, not merely that something matched:
    # the bug this closes was a pattern that matched the wrong substring, and a boolean
    # corpus cannot see that.
    $pathMustMatch = @(
        'PLAN.help|// Footer status bar (PLAN.help D3): one accessible label',
        'PLAN.annotations-viewer|/// no row (PLAN.annotations-viewer Q1 / TDD 20.2).',
        'PLAN.copy-path|// enabled in connect_open at the point the path is set (PLAN.copy-path D1/D3).',
        'PLAN.typed-gtk-seams.md|see PLAN.typed-gtk-seams.md for the seam list',
        'sdd/PLAN.help.md|retired: sdd/PLAN.help.md',
        'sdd/TECH.md|the module map in sdd/TECH.md',
        'tests/MANUAL-TEST.md|the plan lives in tests/MANUAL-TEST.md'
    )
    # A directory-qualified path that EXISTS (`sdd/ISSUES.md`) is not a false positive
    # and is deliberately absent: 6a is supposed to extract it, then resolve it and
    # find it present. This corpus is about what must not be extracted AS A PATH.
    $pathMustNotMatch = @(
        'see POLICY.md for the rule',
        'a PLAN.<topic>.md is deleted by design once implemented',
        'doc_link_fragment("./sub/PLAN.md#caf%C3%A9")',
        'let done = plan.switch_to.is_some() && plan.spot.as_ref();'
    )
    # Check 8's corpus, kept string-for-string with the bash gate's. It exercises
    # `Test-BareAp` -- the predicate the check itself calls -- not the two patterns in
    # isolation: the rule is a strip-then-match COMPOSITE, and a corpus over
    # $bareApPattern alone would pass while the strip did nothing.
    #
    # The discriminating cases are the tail of each list: a line carrying BOTH a legal and
    # a bare citation must be flagged (for the bare one), the near-misses (`Scr-AP-`,
    # `SOAP-`) must not be, and a MISSPELLED or SPACE-BROKEN prefix must be -- the property
    # the single-token form buys and `skill AP-N` did not have. The retired two-token
    # spelling is in the FLAG list on purpose, so it cannot drift back in. No apostrophes
    # -- these are single-quoted in the bash twin, where `the skill's AP-78` would end the
    # string.
    $bareApMustFlag = @(
        'see AP-79 for the pump-loop lesson',
        '(AP-30 deferred popdown)',
        'per the gtk4-rs skill AP-78 masking',
        'AP-1 leads the router',
        'GTK4Rs/AP-57 / AP-34b enum split',
        'see GTK4Rs/AP-109 and AP-88 together',
        'gtk4rs/AP-9 is a misspelled prefix',
        'GTK4Rs / AP-9 is not one token'
    )
    $bareApMustNotFlag = @(
        'see GTK4Rs/AP-109 for the tab-close gesture guard',
        'see GTK4Rs/AP-79 for the pump-loop bound',
        'a bare `AP-N` is illegal anywhere in the tree',
        'GTK4Rs/AP-153 pairs with GTK4Rs/AP-153',
        'SOAP-12 is not a citation',
        'the shorthand #79 inside this register',
        'Scr-AP-9 is not a citation either'
    )
    $bad = 0
    foreach ($s in $bareApMustFlag) {
        if (-not (Test-BareAp $s)) { Write-Host "   MISS (should flag): $s" -ForegroundColor Red; $bad++ }
    }
    foreach ($s in $bareApMustNotFlag) {
        if (Test-BareAp $s) { Write-Host "   FALSE POSITIVE (check 8): $s" -ForegroundColor Red; $bad++ }
    }
    foreach ($line in $winMustMatch) {
        if (-not (Test-WinIllegalPath $line)) { Write-Host "   MISS (should match): [$line]" -ForegroundColor Red; $bad++ }
    }
    foreach ($line in $winMustNotMatch) {
        if (Test-WinIllegalPath $line) { Write-Host "   FALSE POSITIVE: [$line]" -ForegroundColor Red; $bad++ }
    }
    foreach ($s in $mustMatch) {
        if ($s -cnotmatch $issuePattern) { Write-Host "   MUST MATCH but did not: '$s'" -ForegroundColor Red; $bad++ }
    }
    foreach ($s in $mustNotMatch) {
        if ($s -cmatch $issuePattern) { Write-Host "   MUST NOT MATCH but did: '$s'" -ForegroundColor Red; $bad++ }
    }
    foreach ($s in $pathMustMatch) {
        $want = $s.Substring(0, $s.IndexOf('|'))
        $subject = $s.Substring($s.IndexOf('|') + 1)
        $m = [regex]::Match($subject, $pathPattern)
        $got = if ($m.Success) { $m.Value } else { '' }
        if ($got -cne $want) {
            Write-Host "   WRONG EXTRACT: want '$want', got '$got' from: $subject" -ForegroundColor Red; $bad++
        }
    }
    # Driven through `Get-PathHits` -- the function check 6a itself calls -- over a real
    # temp file, NOT through a `[regex]::Match` or a bare `Select-String` here. The two
    # differ by the check's flags, and a corpus run through a re-implementation cannot fail
    # when a flag is dropped from the real one. Mutation-verified: remove `-CaseSensitive`
    # from `Get-PathHits` and this loop goes red.
    $stTmp = [System.IO.Path]::GetTempFileName()
    Set-Content -LiteralPath $stTmp -Value $pathMustNotMatch -Encoding UTF8
    foreach ($h in (Get-PathHits $stTmp)) {
        foreach ($m in $h.Matches) {
            $got = $m.Value
            # `PLAN.md` may be MATCHED; check 6a skips it by name. Anything else is a
            # false positive.
            if ($got -cne 'PLAN.md') {
                Write-Host "   FALSE POSITIVE: '$got' from: $($h.Line)" -ForegroundColor Red; $bad++
            }
        }
    }
    Remove-Item -LiteralPath $stTmp -Force -ErrorAction SilentlyContinue

    # Symlink-exclusion canary -- two independent assertions, both required (see
    # scripts/lint-references.scan, "SYMLINKS ARE EXCLUDED"). Kept behaviourally
    # identical to the bash twin's block, including which half is consulted: the canary
    # reads the git INDEX only, never the filesystem, so the assertion means the same
    # thing on both platforms.
    #
    # (1) at least one mode-120000 path exists in the repo -- currently exactly one,
    #     tests/fixtures/escapes-via-symlink.md. Without this, (2) starts passing
    #     VACUOUSLY the day that fixture is deleted or renamed, which is the exact
    #     defect this project has already paid for once.
    # (2) no mode-120000 path appears in -ListScan's output.
    #
    # -ListScan runs as a FRESH SUBPROCESS rather than by reading `$scanSet` in process:
    # what is being asserted is that the ARTIFACT a reader diffs says the same thing as
    # the set the checks consume, and only a real run of the switch can show that. The
    # bash twin re-invokes itself for the same reason.
    #
    # UNLIKE ON LINUX, THIS ASSERTION IS LOAD-BEARING HERE. It tests the OUTCOME (no
    # symlink in the set), not the mechanism; on Linux `find -type f` already produces
    # that outcome unaided, so neutering the exclusion there leaves the self-test
    # PASSING and the code is unfalsifiable. On Windows the exclusion above is the ONLY
    # thing producing the outcome, so that same mutation MUST fail here -- verified by
    # doing it. This platform is where the rule is actually tested.
    if ($symlinkIndexPaths.Count -eq 0) {
        Write-Host "   FAIL: no mode-120000 path found in the git index -- the canary" -ForegroundColor Red
        Write-Host "     fixture (tests/fixtures/escapes-via-symlink.md) is gone, so the" -ForegroundColor Red
        Write-Host "     next assertion (no symlink in -ListScan) would now pass vacuously." -ForegroundColor Red
        $bad++
    } else {
        $stListScan = @(& powershell.exe -NoProfile -File $PSCommandPath -ListScan)
        foreach ($sl in $symlinkIndexPaths) {
            if ($stListScan -contains $sl) {
                Write-Host "   FAIL: symlink '$sl' (git mode 120000) appears in -ListScan output" -ForegroundColor Red
                $bad++
            }
        }
    }

    # ── The scan set itself: three assertions, each able to FAIL on its own ────
    #
    # The `.scan` header used to claim the depth conversion "is asserted by -SelfTest".
    # Nothing asserted it -- a documented guarantee with nothing behind it, which is
    # worse than an undocumented one because it stops the next reader from writing the
    # check. These are that check, and each is written so the mutation it is meant to
    # catch actually fails it (ScrAP-209 applied prospectively). Kept behaviourally
    # identical to the bash twin's block.
    #
    # (i) -ListScan prints EXACTLY what the checks consume. This is the assertion that
    #     would have caught the original defect: the parity artifact enumerated a bounded
    #     set while check 6a scanned an unbounded one, so it certified a set some checks
    #     did not consume. Close to tautological while both read one variable -- say so
    #     rather than overclaim -- but not vacuous: re-introduce a separate
    #     `Get-ChildItem` in the -ListScan branch, with any different bound or filter,
    #     and this fires. Run against the PRISTINE tree, before the depth probe below
    #     plants anything, since $scanSet was built at startup.
    #     Reported as MEMBERSHIP differences plus the first ORDERING divergence, not as
    #     a positional compare. `Compare-Object -SyncWindow 0` was tried and is useless
    #     here: drop one early path and every line after it reads as changed, so a
    #     one-file defect printed 235 lines and the reader learns nothing. Both halves
    #     are asserted because the claim is byte-identical output -- the ordering half
    #     is what makes the cross-platform diff clean enough to be read at all.
    $stListed = @(& powershell.exe -NoProfile -File $PSCommandPath -ListScan)
    $stConsumed = [System.Collections.Generic.HashSet[string]]::new(
        [string[]] $scanSet, [System.StringComparer]::Ordinal)
    $stPrinted = [System.Collections.Generic.HashSet[string]]::new(
        [string[]] $stListed, [System.StringComparer]::Ordinal)
    $stOnlyConsumed = @($scanSet  | Where-Object { -not $stPrinted.Contains($_) })
    $stOnlyPrinted  = @($stListed | Where-Object { -not $stConsumed.Contains($_) })
    $stFirstOrder = -1
    $stCommon = [Math]::Min($scanSet.Count, $stListed.Count)
    for ($i = 0; $i -lt $stCommon; $i++) {
        if ($scanSet[$i] -cne $stListed[$i]) { $stFirstOrder = $i; break }
    }
    if ($stOnlyConsumed.Count -or $stOnlyPrinted.Count -or $stFirstOrder -ge 0 -or
        $scanSet.Count -ne $stListed.Count) {
        Write-Host '   FAIL: -ListScan output is not the set the checks consume' -ForegroundColor Red
        Write-Host ("     checks consume {0} paths, -ListScan prints {1}" -f $scanSet.Count, $stListed.Count) -ForegroundColor Red
        foreach ($p in ($stOnlyConsumed | Select-Object -First 10)) {
            Write-Host ('     consumed but not printed: ' + $p) -ForegroundColor Red
        }
        foreach ($p in ($stOnlyPrinted | Select-Object -First 10)) {
            Write-Host ('     printed but not consumed: ' + $p) -ForegroundColor Red
        }
        if ($stFirstOrder -ge 0) {
            Write-Host ("     first ordering divergence at index {0}: consumed '{1}', printed '{2}'" -f `
                $stFirstOrder, $scanSet[$stFirstOrder], $stListed[$stFirstOrder]) -ForegroundColor Red
        }
        $bad++
    }

    # (ii) a file at EXACTLY maxdepth is in the set, and (iii) a file at maxdepth+1 makes
    # the gate FAIL. (iii) is deliberately "the gate fails and names the path" rather
    # than "the file is absent from the set": absence is also what a broken enumerator
    # produces, so an absence assertion passes for the wrong reason. It is also the
    # behaviour that makes one shared set safe -- a budget that quietly drops files is
    # indistinguishable from a tree that has none.
    #
    # The probe is planted under the first contract root rather than a named one, so it
    # follows the contract instead of restating it, and its depth is DERIVED from
    # maxdepth -- hard-coding six would leave the assertion passing for a contract that
    # had moved.
    $stProbeRoot = $null
    foreach ($r in $scanRoots) { if (Test-Path $r) { $stProbeRoot = $r; break } }
    if (-not $stProbeRoot) {
        Write-Host '   FAIL: no contract root exists on disk -- the depth assertions cannot' -ForegroundColor Red
        Write-Host '     run, and a self-test that silently skips them is the defect they' -ForegroundColor Red
        Write-Host '     exist to close.' -ForegroundColor Red
        $bad++
    } else {
        $stProbe = Join-Path $stProbeRoot 'lint-scan-depth-probe'
        try {
            if (Test-Path $stProbe) { Remove-Item -LiteralPath $stProbe -Recurse -Force }
            # The path below the root must have exactly $scanMaxDepth segments: the probe
            # directory is the first, then maxdepth-2 more, then the file.
            $stRel = 'lint-scan-depth-probe'
            for ($i = 2; $i -lt $scanMaxDepth; $i++) { $stRel += "/d$i" }
            $stDirAbs = Join-Path $stProbeRoot $stRel
            [void] (New-Item -ItemType Directory -Path $stDirAbs -Force)
            # Empty on purpose: the probe must be invisible to every OTHER check. A file
            # with content would have to carry a citation or a link, and this entry's own
            # investigation tripped check 6 by writing one out.
            [void] (New-Item -ItemType File -Path (Join-Path $stDirAbs 'at-max.md'))
            $stAtMax = "$stProbeRoot/$stRel/at-max.md"
            $stProbeListed = @(& powershell.exe -NoProfile -File $PSCommandPath -ListScan)
            if ($stProbeListed -cnotcontains $stAtMax) {
                Write-Host "   FAIL: a file at exactly maxdepth ($scanMaxDepth) is MISSING from the scan set:" -ForegroundColor Red
                Write-Host ('     ' + $stAtMax) -ForegroundColor Red
                Write-Host '     The budget is off by one, or the enumeration stops short of it.' -ForegroundColor Red
                $bad++
            }
            [void] (New-Item -ItemType Directory -Path (Join-Path $stDirAbs 'over') -Force)
            [void] (New-Item -ItemType File -Path (Join-Path $stDirAbs 'over/too-deep.md'))
            $stTooDeep = "$stProbeRoot/$stRel/over/too-deep.md"
            $stGateOut = @(& powershell.exe -NoProfile -File $PSCommandPath)
            if ($LASTEXITCODE -eq 0) {
                Write-Host '   FAIL: a file at maxdepth+1 did not fail the gate -- it was silently' -ForegroundColor Red
                Write-Host '     truncated out of the scan set, which is the coverage cliff this' -ForegroundColor Red
                Write-Host ('     assertion exists to catch: ' + $stTooDeep) -ForegroundColor Red
                $bad++
            } elseif (-not ($stGateOut -join "`n").Contains($stTooDeep)) {
                Write-Host '   FAIL: the gate failed with a file at maxdepth+1 present, but did not' -ForegroundColor Red
                Write-Host '     NAME it, so the exit code cannot be attributed to the depth budget:' -ForegroundColor Red
                Write-Host ('     ' + $stTooDeep) -ForegroundColor Red
                $bad++
            }
        } finally {
            if (Test-Path $stProbe) { Remove-Item -LiteralPath $stProbe -Recurse -Force }
        }
    }

    Pop-Location
    if ($bad) {
        Write-Host "self-test: FAILED ($bad case(s))" -ForegroundColor Red
        exit 1
    }
    # EVERY corpus this self-test ran must be named here. Check 8's two were absent while
    # running and passing, so the summary said "four corpora" over six -- a reader auditing
    # whether check 8 is covered would conclude it is not, and would "add" what exists.
    # If you add a corpus above, add its count here in the same change.
    # Check 12's two corpora were the next omission, running and passing unnamed -- the
    # same shape one check later. They are the ones a reader is MOST likely to audit,
    # because check 12 is the check whose bash twin was a permanent false green.
    Write-Host ("self-test: OK ({0}/{0} must-match, {1}/{1} must-not-match, {2}/{2} path-extract, {3}/{3} path-must-not-match, {4}/{4} bare-AP-flag, {5}/{5} bare-AP-silent, {6}/{6} win-illegal, {7}/{7} win-legal)" -f `
        $mustMatch.Count, $mustNotMatch.Count, $pathMustMatch.Count, $pathMustNotMatch.Count, `
        $bareApMustFlag.Count, $bareApMustNotFlag.Count, `
        $winMustMatch.Count, $winMustNotMatch.Count) -ForegroundColor Green
    exit 0
}

if ($ListScan) {
    # Prints `$scanSet` itself, not a re-derivation of it. That is the whole fix: this
    # branch used to run its own bounded `Get-ChildItem`, so the artifact certifying the
    # set could not certify the set. `-SelfTest` asserts the two are byte-identical,
    # which is the assertion that would have caught the original defect.
    $scanSet
    Pop-Location
    exit 0
}

# ── Preflight: the files the checks READ must exist ────────────────────────────
# `Get-FileText` returns '' for a missing file, which is what keeps the .Contains()
# sites from throwing -- but '' cannot distinguish "no entries" from "not there", and a
# MISSING register would make check 3 compare nothing against nothing and print PASS. A
# gate that reports success because its input vanished is the worst outcome available
# to it. Check existence once, explicitly, and name the file. Kept in step with the
# bash twin's identical list.
foreach ($required in @('sdd/ANTI-PATTERNS.md', 'sdd/ISSUES.md', 'src/icons.rs',
                        'src/lib.rs', 'src/gtk_suite.rs')) {
    if (-not (Test-Path $required)) {
        Write-Host "lint-references: $required is missing -- this gate reads it." -ForegroundColor Red
        Write-Host '  Refusing to run: checks over a file that is not there report PASS.' -ForegroundColor Red
        Pop-Location
        exit 1
    }
}

# ── Check 1: ISSUES entries cited outside sdd/ISSUES.md ────────────────────────
Write-Host ''
Write-Host '-- Check 1: ISSUES referenced outside sdd/ISSUES.md --' -ForegroundColor Cyan

# sdd/ISSUES.md is the register itself. sdd/PLAN.* is excluded deliberately: a dated
# session-log entry is a HISTORICAL record of what a thing was called on a given day,
# and rewriting it falsifies the record, whereas a code comment is a live instruction.
# Revisit this exclusion if a PLAN ever carries an ACTIONABLE issue pointer rather
# than a historical one.
# -CaseSensitive is MANDATORY, not a refinement. Select-String defaults to
# case-INSENSITIVE where `grep` defaults to case-sensitive, so without it the
# pattern matches ordinary prose -- "issues a queue_draw that CLEARS" fires as
# `issues` + ` ` + `a`. Two false positives in src/widgets/tab/bar.rs were the
# proof. A port of a gate must reproduce the ORIGINAL's matching semantics, not
# merely its regex text.
#
# THE INPUT IS THE CONTRACT SET, NARROWED -- not a walk of its own. This used to be
# `Get-ScannedFiles @('src','tests','sdd','scripts')` here and a matching four-root
# literal in the bash twin: two hard-coded lists, unbounded, disagreeing with check 6 in
# the same run (ScrAP-207). Drawing from the set also widens the rule to `packaging/`,
# `gtktest/`, `data/` and the root `file` entries, where a pointer at an ephemeral issue
# ID dangles exactly as it does anywhere else, and narrows it by the contract's
# `exclude` prefixes, so the verdict stops depending on which generated trees the
# developer happens to have. This script and its twin necessarily contain the pattern in
# order to implement it, so both are excluded, and the two exclusions stay paired.
$scan1 = Get-ScanSubset -Not '^(sdd/ISSUES\.md$|sdd/PLAN\.|scripts/lint-references\.(sh|ps1)$)'
$hits1 = @()
if ($scan1.Count) {
    $hits1 = @(Select-String -LiteralPath $scan1 -Pattern $issuePattern -CaseSensitive -ErrorAction SilentlyContinue)
}

if ($hits1.Count) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    foreach ($h in $hits1) {
        $rel = Get-RepoRelativePath $h.Path
        Write-Host ("      {0}:{1}: {2}" -f $rel, $h.LineNumber, $h.Line.Trim())
    }
    Write-Host '      -> cite a durable ScrAP-N instead; issue IDs are deleted by design.'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

# ── Check 2: ScrAP-N cited in src/ but absent from the register ────────────────
Write-Host ''
Write-Host '-- Check 2: ScrAP-N cited but absent --' -ForegroundColor Cyan

$register = Get-FileText 'sdd/ANTI-PATTERNS.md'
$defined = [regex]::Matches($register, '(?m)^##\s+(\d+)\.') |
    ForEach-Object { [int]$_.Groups[1].Value }
$definedSet = [System.Collections.Generic.HashSet[int]]::new([int[]]$defined)

# The contract set narrowed to `src/`, not a `Get-ChildItem -Recurse` of its own -- same
# reason as check 1 above. This was the fourth enumeration in the file.
$srcFiles = Get-ScanSubset '^src/'
$cited = @{}
foreach ($rel in $srcFiles) {
    foreach ($m in [regex]::Matches((Get-FileText $rel), 'ScrAP-(\d+)')) {
        $n = [int]$m.Groups[1].Value
        if (-not $cited.ContainsKey($n)) { $cited[$n] = @() }
        if ($cited[$n] -notcontains $rel) { $cited[$n] += $rel }
    }
}

$missing = $cited.Keys | Where-Object { -not $definedSet.Contains($_) } | Sort-Object
if ($missing) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    foreach ($n in $missing) {
        Write-Host ("      ScrAP-{0} cited in {1}: {2}" -f $n, $cited[$n].Count, ($cited[$n] -join ', '))
    }
    Write-Host '      -> the entry does not exist in sdd/ANTI-PATTERNS.md (yet).'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

# ── Check 3: ScrAP-N cited INSIDE the register but with no body there ──────────
#
# Deliberately matches only the `ScrAP-N` form, NOT the bare `#N` in-file shorthand.
# `#N` is documented as the local shorthand, but it cannot be told apart mechanically
# from ordinary prose -- measured on the register, a bare-`#N` sweep picks up `#287`
# and `#819`, which are not citations at all. A check with false positives gets
# disabled; one that is merely incomplete gets extended. Prefer `ScrAP-N` when citing
# inside the register.
Write-Host ''
Write-Host '-- Check 3: ScrAP-N cited inside sdd/ANTI-PATTERNS.md but not defined there --' -ForegroundColor Cyan

$citedInReg = [regex]::Matches($register, 'ScrAP-(\d+)') |
    ForEach-Object { [int]$_.Groups[1].Value } |
    Sort-Object -Unique
$missingReg = @($citedInReg | Where-Object { -not $definedSet.Contains($_) })

if ($missingReg.Count) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    $regLines = Get-Content -Encoding UTF8 'sdd/ANTI-PATTERNS.md'
    foreach ($n in $missingReg) {
        Write-Host ('      ScrAP-{0} is cited in the register but has no `## {0}.` body:' -f $n)
        for ($i = 0; $i -lt $regLines.Count; $i++) {
            if ($regLines[$i] -match ("ScrAP-{0}\b" -f $n)) {
                $line = "{0}:{1}" -f ($i + 1), $regLines[$i].Trim()
                if ($line.Length -gt 160) { $line = $line.Substring(0, 160) }
                Write-Host ('        ' + $line)
            }
        }
    }
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host '-- Check 4: module list drift between src/lib.rs and src/gtk_suite.rs --'
# `src/gtk_suite.rs` is a second crate root (the harness-free, main-thread GTK suite)
# and must re-declare `src/lib.rs`'s module list. That duplication fails SILENTLY: a
# module added to lib.rs and not here drops every `#[gtktest::test]` inside it from the
# suite, with nothing erroring. Only plain `mod x;` lines are compared, so a `#[cfg]`
# on the preceding line is ignored — which is what makes `suite_registry` and the
# unix/windows-gated modules compare equal rather than reading as drift.
function Get-ModuleList([string]$Path) {
    Get-Content -Encoding UTF8 $Path |
        Select-String -Pattern '^\s*(pub\(crate\) )?mod ([a-z_0-9]+);' |
        ForEach-Object { $_.Matches[0].Groups[2].Value } |
        Sort-Object -Unique
}
$libMods = @(Get-ModuleList 'src/lib.rs')
$suiteMods = @(Get-ModuleList 'src/gtk_suite.rs')
$onlyLib = @($libMods | Where-Object { $suiteMods -notcontains $_ })
$onlySuite = @($suiteMods | Where-Object { $libMods -notcontains $_ })

if ($onlyLib.Count -or $onlySuite.Count) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    foreach ($m in $onlyLib) {
        Write-Host ('      declared in src/lib.rs but MISSING from src/gtk_suite.rs: {0}' -f $m)
        Write-Host '        Every #[gtktest::test] in it is silently absent from the suite.'
    }
    foreach ($m in $onlySuite) {
        Write-Host ('      declared in src/gtk_suite.rs but not in src/lib.rs: {0}' -f $m)
    }
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host '-- Check 5: #[gtk::test] used instead of #[gtktest::test] --'
# `#[gtk::test]` must not come back. It still works, but choosing it is INVISIBLE: the
# test passes on Linux while the body is absent from `src/gtk_suite.rs`'s main-thread
# run, and therefore from the only run available where GTK must initialise on the main
# thread. Write `#[gtktest::test]` -- drop-in, registers with BOTH harnesses.
# `.rs` under `src/`, taken from the contract set. Deliberately NOT widened to every
# `.rs` in the set: `tests/` and `gtktest/` name the banned attribute in doc comments and
# prose by necessity (gtktest's whole purpose is to replace it), and a check with false
# positives gets disabled while an incomplete one gets extended -- check 3's rule.
$rsFiles = Get-ScanSubset '^src/.*\.rs$'
$legacy = @()
if ($rsFiles.Count) {
    $legacy = @(Select-String -LiteralPath $rsFiles -Pattern '^\s*#\[gtk::test\]\s*$' -CaseSensitive)
}

if ($legacy.Count) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    foreach ($h in $legacy) {
        Write-Host ("      {0}:{1}" -f (Get-RepoRelativePath $h.Path), $h.LineNumber)
    }
    Write-Host '      -> replace with #[gtktest::test] (no other change needed).'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host '-- Check 5b: #[gtk::test] PRESCRIBED in prose without naming the replacement --'
# RATIFIED BY THE OPERATOR (2026-07-31). Added by an agent during QA round 4 and
# flagged unratified on the reasoning in ScrAP-222 -- a check IS a rule, it just
# does not look like one, so it needs the same authority a POLICY line does. The
# operator has since adopted it. Kept as a note rather than deleted because the
# provenance is the point: this constraint now stands on their authority, not on
# the authorship of whoever wrote the grep.
# Check 5 greps `src/*.rs`, so it can only see a developer who has ALREADY written the
# wrong thing -- never the DOCUMENT that told them to. QA round 4 found POLICY.md
# instructing `#[gtk::test]` while check 5 rejected that exact attribute: two artifacts,
# each internally correct, enforcing opposite things, neither able to detect the other
# (ScrAP-222).
#
# THE UNIT IS THE PARAGRAPH, not the line: a legitimate passage CONTRASTS the two
# attributes, a stale prescription names only the banned one. A line-proximity window
# was tried first and flagged three correct lines whose replacement mention sat five to
# nine lines away. The document set is the contract's `prescriptive` class, not a
# literal here -- see lint-references.scan for why a hard-coded list is the bug.
# NOTE FOR THE WINDOWS SEAT: this port was AUTHORED ON LINUX AND NEVER EXECUTED --
# no PowerShell on that host. Run it (and `-SelfTest`) before trusting it. It is
# written flat, with no scriptblock and no scope-qualified accumulator, precisely
# because those are what misbehave under `Set-StrictMode -Version Latest` and could
# not be checked here.
$prosehits = @()
foreach ($doc in $scanPrescriptive) {
    if (-not (Test-Path $doc)) { continue }
    $lines = @(Get-Content -Encoding UTF8 $doc)
    $i = 0
    while ($i -lt $lines.Count) {
        if ($lines[$i] -match '^\s*$') { $i++; continue }
        # One blank-line-delimited block, consumed to the next blank line.
        $hasBan  = $false
        $hasRepl = $false
        $banLine = -1
        while ($i -lt $lines.Count -and $lines[$i] -notmatch '^\s*$') {
            if (-not $hasBan -and $lines[$i].Contains('#[gtk::test]')) {
                $hasBan = $true
                $banLine = $i
            }
            if ($lines[$i].Contains('gtktest')) { $hasRepl = $true }
            $i++
        }
        if ($hasBan -and -not $hasRepl) {
            $prosehits += ("      {0}:{1}: {2}" -f $doc, ($banLine + 1), $lines[$banLine])
        }
    }
}
if ($prosehits.Count) {
    $failed = $true
    Write-Host '   FAIL -- prose prescribing the attribute check 5 rejects:' -ForegroundColor Red
    foreach ($h in $prosehits) { Write-Host $h -ForegroundColor Red }
    Write-Host '      -> a reader who obeys this breaks the build. Name #[gtktest::test]'
    Write-Host '         in the same paragraph, so it reads as a contrast not an instruction.'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host '-- Check 6: referenced document paths that do not exist --'
# A pointer at a document that no longer exists -- check 1's failure class from the
# other direction: check 1 stops a reference to an entry that is SUPPOSED to vanish,
# this stops one left behind when a whole FILE vanished. Plan retirement is when it
# happens: a PLAN.*.md is deleted by design once implemented, and every "see
# PLAN.x.md" written while it existed dangles at once.
#
# Two forms. 6a: a directory-qualified path, or a bare `PLAN.*.md` (plan files live in
# sdd/ by convention, and the bare form is what code comments use), resolved from the
# repo root and from sdd/. 6b: a Markdown link target in a .md file, resolved relative
# to the linking file, since that is how a reader's viewer resolves it.
#
# Deliberately NOT matched: a bare document name in prose ("see POLICY.md"), which is
# a mention rather than a path -- src/ alone carries ~100, and a check with false
# positives gets switched off. tests/fixtures/ (deliberately broken link corpora) and
# inline code spans (a backticked `[x](TECH.md)` is an EXAMPLE of link syntax) are
# excluded for the same reason.
$missingPaths = @()
# The lint scripts quote dangling paths as worked examples, so both are excluded and the
# two exclusions stay paired. `tests/reports/` needs no exclusion here any more -- the
# contract's `exclude` prefixes already keep those generated artifacts out of the set --
# but a live document may still point AT one, which is filtered target-side below.
$scan6a = Get-ScanSubset -Not '^scripts/lint-references\.(sh|ps1)$'
foreach ($rel in $scan6a) {
    foreach ($h in (Get-PathHits $rel)) {
        foreach ($m in $h.Matches) {
            $target = $m.Value
            # Excluded as a TARGET, not merely as a scanned file: MANUAL-TEST.md points
            # at its own report artifacts, which are gitignored and absent on a fresh
            # clone. The bash original filters the whole grep line, so it drops these
            # either way; a port that only skips the containing file reproduces the
            # regex but not the semantics, and fails wherever the artifacts happen not
            # to exist. Matching semantics is the thing being ported.
            if ($target -like 'tests/reports/*') { continue }
            # A bare `PLAN.<topic>` citation names a document, so give it the extension
            # the document has. `PLAN.md` strips to an EMPTY topic and is not a plan
            # citation at all -- SDD names plans `PLAN.<topic>.md`. Without that rule
            # src/links.rs's `./sub/PLAN.md#caf%C3%A9` link-parser fixture reads as a
            # dangling plan, which is a false positive in the one class of file
            # guaranteed to hold link-shaped strings that point nowhere.
            if ($target -like 'PLAN.*' -and $target -notlike 'PLAN.*.md') {
                if ($target -eq 'PLAN.md') { continue }
                $target = $target + '.md'
            }
            if (-not (Test-Path $target) -and -not (Test-Path (Join-Path 'sdd' $target))) {
                $missingPaths += ("{0}:{1} -> {2}" -f $rel, $h.LineNumber, $target)
            }
        }
    }
}
# The `.md` members of the contract set. `tests/fixtures/` stays an inline exclusion for
# 6b ONLY: those corpora are broken ON PURPOSE, and 6a still scans them for citations.
$md6b = Get-ScanSubset '\.md$'
foreach ($rel in $md6b) {
    if ($rel -like 'tests/fixtures/*') { continue }
    # A root-level `file` entry has no parent directory, and `Join-Path ''` throws under
    # 5.1 rather than resolving to the repo root.
    $dir = Split-Path $rel -Parent
    if (-not $dir) { $dir = '.' }
    $text = (Get-FileText $rel) -replace '`[^`]*`', ''
    foreach ($m in [regex]::Matches($text, '\]\(([^)# ]+\.md)(#[^) ]*)?\)')) {
        $target = $m.Groups[1].Value
        if ($target -match '[:~]' -or $target -like 'tests/reports/*') { continue }
        if (-not (Test-Path (Join-Path $dir $target)) -and -not (Test-Path $target)) {
            $missingPaths += ("{0} -> {1}" -f $rel, $target)
        }
    }
}
$missingPaths = @($missingPaths | Sort-Object -Unique)
if ($missingPaths.Count) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    foreach ($p in $missingPaths) { Write-Host ('      ' + $p) }
    Write-Host '      -> delete the pointer, or replace it with the fact it was carrying.'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host '-- Check 7: app-ID drift between src/icons.rs and the packaging files --' -ForegroundColor Cyan
# The application ID is ONE string with several jobs -- GApplication id, icon name,
# desktop-entry Icon=/StartupWMClass, GResource path, macOS CFBundleIdentifier, the
# install/uninstall targets. src/icons.rs is its source of truth and Rust derives it
# from there, so the Rust side cannot drift; the NON-Rust side is restated text that
# nothing checked. Change the ID and everything still builds and runs while the
# desktop entry stops matching the window, the GResource path stops resolving (the
# About logo becomes a broken-image placeholder) and Launch Services keeps the old
# identifier -- each on a different platform, checked by a different person or nobody.
#
# Only files that OPERATIONALLY declare it are listed. README.md, the SVG and the port
# READMEs mention it in prose, and a mention is not a declaration.
$appIdFiles = @('install.sh', 'uninstall.sh', 'data/scribobulate.desktop',
                'data/resources.gresource.xml', 'packaging/macos/Info.plist.in')
$appIdProblems = @()
$iconsSrc = Get-FileText 'src/icons.rs'
$canonicalMatch = [regex]::Match($iconsSrc, 'Icon::App => "([^"]+)"')
if (-not $canonicalMatch.Success) {
    $appIdProblems += 'could not read the canonical app ID from src/icons.rs (Icon::App arm)'
} else {
    $canonicalAppId = $canonicalMatch.Groups[1].Value
    foreach ($f in $appIdFiles) {
        if (-not (Test-Path $f)) { $appIdProblems += "$f is missing"; continue }
        # Null-safe: an EMPTY declaring file must be reported as "does not carry the
        # app ID", which is true and actionable, rather than throwing a null-reference
        # that names neither this file nor this check.
        $text = Get-FileText $f
        if (-not $text.Contains($canonicalAppId)) {
            $appIdProblems += "$f does not carry the app ID '$canonicalAppId'"
        }
        # A DIFFERENT reverse-DNS id in the same file is drift the presence test above
        # cannot see -- both can be true at once while one surface is stale.
        foreach ($m in [regex]::Matches($text, '\b[a-z0-9]+\.[a-z0-9]+\.scribobulate\b')) {
            if ($m.Value -cne $canonicalAppId) {
                $appIdProblems += "$f carries a foreign app ID '$($m.Value)' (canonical: '$canonicalAppId')"
            }
        }
    }
}
$appIdProblems = @($appIdProblems | Sort-Object -Unique)
if ($appIdProblems.Count) {
    $failed = $true
    Write-Host '   FAIL' -ForegroundColor Red
    foreach ($p in $appIdProblems) { Write-Host ('      ' + $p) }
    Write-Host '      -> src/icons.rs''s Icon::App arm is the source of truth; update the others.'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host "-- Check 8: bare AP-N citations (must be ScrAP-N or GTK4Rs/AP-N) --" -ForegroundColor Cyan
# A BARE `AP-N` is illegal anywhere in this tree. Not "means the skill" -- illegal.
#
# WHY A FORM AND NOT A TARGET. `sdd/ANTI-PATTERNS.md` (`ScrAP-N`) and the gtk4-rs skill
# (`GTK4Rs/AP-N`) number independently: a QA sample put 83 of 107 entries at different
# numbers, and this register's #79/#88 hold the skill's AP-88/AP-79 lessons INVERTED, so
# a bare `AP-79` is a coin toss between two real entries about different things.
#
# WHAT IT CANNOT DO, read before trusting a PASS. It cannot confirm a `GTK4Rs/AP-N` names
# the right skill entry -- the skill may not be installed, so nothing on this machine can
# resolve it. That is the reason the scheme is right rather than a hole in it: making the
# bare form illegal does not make skill citations VERIFIABLE, it makes them ENUMERABLE.
# The unverifiable set stops being unbounded and textually identical to the verifiable
# one, and becomes exactly the `GTK4Rs/AP-N` citations -- a greppable list a human can
# audit deliberately (ScrAP-217's control-arm principle, one layer up).
#
# Nor can it see the other half of the exposure: an already-prefixed `ScrAP-N` pointing at
# a real entry about the wrong subject. `src/widgets/tab/bar.rs` carried exactly that, put
# there BY THE FIX -- the sweep that introduced the prefix rewrote the prefix without
# re-resolving the number. That audit is human-only and has no completion criterion.
#
# Mutation-tested when written by PLANTING a bare `AP-123` in a scanned file: a clean tree
# passes this check whether or not the pattern can match (ScrAP-206).
#
# The two lint scripts are excluded, PAIRED, for check 1's reason: both quote bare
# citations in their -SelfTest corpus, as a linter's own fixtures do.
$scan8 = Get-ScanSubset -Not '^scripts/lint-references\.(sh|ps1)$'
$bare8 = @()
$bare8Count = 0
foreach ($rel in $scan8) {
    # `-CaseSensitive` because Select-String defaults the OTHER WAY from `grep -E` -- the
    # same default that gave this port a real false positive on check 1's `issues a
    # queue_draw`. But be exact about what it buys, because the first version of this
    # comment was not: it claimed that without the flag this sweep is "LENIENT-side-strict
    # on Windows", and the Windows seat MEASURED otherwise (planting `ap-123` alongside
    # `AP-124`, with and without the flag, gives an identical verdict). It cannot change
    # the result, because `Test-BareAp` re-filters case-sensitively downstream
    # (`[regex]::IsMatch` is case-sensitive by default), so a lowercase candidate is
    # admitted here and dropped there. Note the old comment's NEXT LINE said exactly that
    # -- it shipped its own refutation one line from the claim.
    #
    # So: belt-and-braces plus a perf saving (fewer lines reach the predicate), and it
    # keeps this sweep's semantics matching `grep -E`'s so the two ports read alike. It
    # becomes load-bearing the moment the predicate stops being case-sensitive, which is
    # the reason to keep it rather than the reason it is here. Do not restore the stronger
    # wording: a guarantee with nothing testing it is the shape that cost this project the
    # `.scan` header's "asserted by -SelfTest" line (ScrAP-231, and #216/#226 before it).
    foreach ($h in (Select-String -LiteralPath $rel -Pattern $bareApPattern -CaseSensitive -ErrorAction SilentlyContinue)) {
        if (-not (Test-BareAp $h.Line)) { continue }
        # Report the CITATIONS, not the line: register entries and MANUAL-TEST items run
        # past a thousand characters, and a migration this size is read as a worklist.
        $stripped = [regex]::Replace($h.Line, $legalApPattern, '')
        $toks = @([regex]::Matches($stripped, 'AP-[0-9]+') | ForEach-Object { $_.Value } | Sort-Object -Unique)
        $bare8Count += $toks.Count
        $bare8 += ('   {0}:{1} - {2}' -f $rel, $h.LineNumber, ($toks -join ' '))
    }
}
if ($bare8.Count) {
    # FAILS -- and it landed in WARN mode first, deliberately, for exactly one run. 443
    # sites could not be re-resolved in one pass, and a wrong bulk decision is WORSE than
    # the ambiguity it replaces: it converts "unclear" into "confidently wrong", which is
    # what happened to bar.rs. So the gate went in with the count visible, migration ran
    # per directory (smallest first), and the flip landed with the last of it. The tree is
    # at zero now, so any hit here is NEW.
    $failed = $true
    Write-Host ("   FAIL -- {0} bare citation(s) on {1} line(s):" -f $bare8Count, $bare8.Count) -ForegroundColor Red
    foreach ($b in $bare8) { Write-Host $b }
    Write-Host '      -> resolve each PER SITE: read the surrounding comment, decide which'
    Write-Host '         lesson it describes, then confirm that number IN THE REGISTER NAMED.'
    Write-Host '         Re-derive it, never carry it across. If the lesson is in both, prefer'
    Write-Host '         ScrAP-N (always resolvable; the skill may be absent). NEVER bulk-rewrite'
    Write-Host '         the prefix -- that is the mechanism that produced the one known defect.'
} else {
    Write-Host '   PASS' -ForegroundColor Green
}


Write-Host ''
Write-Host "-- Check 9: ScrAP number immutability (no removed, renamed or reused numbers) --" -ForegroundColor Cyan
# Port of the bash gate's check 9. Both halves must exist or this gate is exactly the
# defect ScrAP-207 records: one gate living twice, where the port nobody runs is the
# lenient one. The shared manifest makes the INPUT identical here by construction --
# both ports read sdd/scrap-numbers.manifest rather than restating the number set,
# which is the half of a duplicated gate that reviewers never diff.
#
# The manifest is a DIFF SEED, NOT A SNAPSHOT: it was generated from `master`, where
# the heading set is independently known good. Never regenerate it from the working
# file -- that would bless an already-missing heading and hold this green forever.
$manifest = 'sdd/scrap-numbers.manifest'
if (-not (Test-Path $manifest)) {
    Write-Host "   FAIL - manifest $manifest is missing; number immutability is unenforced" -ForegroundColor Red
    $failed = $true
} else {
    # -CaseSensitive because the shell twin uses `grep -oE`, which is case-sensitive,
    # and Select-String is NOT by default. Without it this port would accept an entry
    # heading spelled with a suffix letter in the wrong case that the shell port
    # rejects -- the lenient-port drift POLICY step 9 exists to prevent, in the one
    # check that was missed when -CaseSensitive was added to the others.
    $have9 = Select-String -Path 'sdd/ANTI-PATTERNS.md' -Pattern '^## ([0-9]+[a-z]?)\.' -AllMatches -CaseSensitive |
             ForEach-Object { $_.Matches[0].Groups[1].Value }
    $want9 = Get-Content -Encoding UTF8 $manifest | Where-Object { $_ -and $_ -notmatch '^\s*#' }
    $missing9 = $want9 | Where-Object { $have9 -notcontains $_ }
    $added9   = $have9 | Where-Object { $want9 -notcontains $_ }
    $dupes9   = $have9 | Group-Object | Where-Object { $_.Count -gt 1 } | ForEach-Object { $_.Name }
    $bad9 = $false
    if ($missing9) {
        Write-Host "   FAIL - allocated number(s) no longer have a '## N.' heading:" -ForegroundColor Red
        $missing9 | ForEach-Object { Write-Host "     ScrAP-$_" }
        Write-Host '     A number is never released. If the entry was merged or superseded, leave a'
        Write-Host '     one-line landing-spot stub under its heading -- code and sibling entries'
        Write-Host '     still cite it.'
        $bad9 = $true
    }
    if ($dupes9) {
        Write-Host '   FAIL - number(s) used by more than one heading:' -ForegroundColor Red
        $dupes9 | ForEach-Object { Write-Host "     ScrAP-$_" }
        $bad9 = $true
    }
    if ($added9) {
        Write-Host "   INFO - new number(s) not yet in the manifest: $($added9 -join ' ')"
        Write-Host "     Append them to $manifest in the same commit that adds the entry."
    }
    if ($bad9) { $failed = $true } else { Write-Host '   PASS' -ForegroundColor Green }
}


Write-Host ''
Write-Host "-- Check 10: a compressed entry keeps its implementation line --" -ForegroundColor Cyan
# Port of the bash gate's check 10. THE LABEL IS ENUMERATED, NOT GUESSED: this file
# spells the field five ways (**Scribobulate**, **Scribobulate.**, **Where Scribobulate
# implements the fix**, the same with a period, and one Non-core variant naming it), so
# both ports match the STEM. A check keyed on one spelling reports a confident absence
# for every other -- during this migration that came within one batch of deleting a
# field the detector "could not see".
$bad10 = @()
$cur10 = ''; $hasImpl = $false; $isStub = $true; $sawSym = $false
foreach ($line in Get-Content -Encoding UTF8 'sdd/ANTI-PATTERNS.md') {
    if ($line -match '^## ([0-9]+[a-z]?)\.') {
        if ($cur10 -and $isStub -and $sawSym -and -not $hasImpl) { $bad10 += $cur10 }
        $cur10 = $Matches[1]; $hasImpl = $false; $isStub = $true; $sawSym = $false
    } elseif ($line -match '^\*\*Symptom') { $sawSym = $true }
    elseif ($line -match '^\*\*[^*]*Scribobulate') { $hasImpl = $true }
    elseif ($line -match '^\*\*(Resolution|Root cause|Lesson|What was tried)') { $isStub = $false }
}
if ($cur10 -and $isStub -and $sawSym -and -not $hasImpl) { $bad10 += $cur10 }
if ($bad10.Count) {
    Write-Host '   FAIL - compressed entr(ies) with no implementation line:' -ForegroundColor Red
    $bad10 | ForEach-Object { Write-Host "     ScrAP-$_" }
    Write-Host '     A stub without it points at a lesson and answers nothing locally.'
    $failed = $true
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

Write-Host ''
Write-Host "-- Check 11: register growth (bytes, not lines) --" -ForegroundColor Cyan
# BYTES, not lines, and this register is the evidence: trimming its index cut 72% of
# that block's bytes while the line count went UP. A line budget watched this file
# bloat sideways and would not have seen it shrink either. Thresholds sit ABOVE the
# measured state -- a ratchet that trips on the commit installing it teaches people to
# raise the number rather than to consolidate.
$REG_WARN = 520000; $REG_FAIL = 650000; $ENTRY_WARN = 11000; $ENTRY_FAIL = 15000
$bad11 = $false
$regBytes = (Get-Item 'sdd/ANTI-PATTERNS.md').Length
if ($regBytes -gt $REG_FAIL) {
    Write-Host "   FAIL - register is ${regBytes}B, past the ${REG_FAIL}B ceiling" -ForegroundColor Red
    $bad11 = $true
} elseif ($regBytes -gt $REG_WARN) {
    Write-Host "   WARN - register is ${regBytes}B, past the ${REG_WARN}B soft limit" -ForegroundColor Yellow
}
$sizes = @{}; $n11 = ''
foreach ($line in Get-Content -Encoding UTF8 'sdd/ANTI-PATTERNS.md') {
    if ($line -match '^## ([0-9]+[a-z]?)\.') { $n11 = $Matches[1]; $sizes[$n11] = 0 }
    elseif ($n11) { $sizes[$n11] += $line.Length + 1 }
}
$over = $sizes.GetEnumerator() | Where-Object { $_.Value -gt $ENTRY_FAIL }
$warn = $sizes.GetEnumerator() | Where-Object { $_.Value -gt $ENTRY_WARN -and $_.Value -le $ENTRY_FAIL }
if ($over) {
    Write-Host "   FAIL - entr(ies) past the ${ENTRY_FAIL}B per-entry ceiling:" -ForegroundColor Red
    $over | ForEach-Object { Write-Host "     ScrAP-$($_.Key) $($_.Value)B" }
    $bad11 = $true
} elseif ($warn) {
    Write-Host "   WARN - entr(ies) past the ${ENTRY_WARN}B per-entry soft limit:" -ForegroundColor Yellow
    $warn | ForEach-Object { Write-Host "     ScrAP-$($_.Key) $($_.Value)B" }
}
if ($bad11) { $failed = $true } elseif (-not $warn -and $regBytes -le $REG_WARN) {
    Write-Host '   PASS' -ForegroundColor Green
}

# -- Check 13: entry bodies with no TOC row --------------------------------------
#
# THE CONVERSE OBLIGATION, and it exists because it was violated by the seat that owns
# this register, on the entry it had just landed. ScrAP-319's body was correct and
# complete and had no row in the table of contents. SDD principle 7 makes the TOC a
# FILTER -- an agent reads it to decide which bodies to open -- so an entry missing from
# it is invisible to the one path meant to find it while still existing, which is worse
# than a missing entry because nothing anywhere reads as wrong.
#
# Nothing could see it. Check 9 is number immutability, check 10 is the implementation
# line, checks 2 and 3 prove a cited number HAS a body. None asks whether a body can be
# FOUND.
#
# The row is matched on the leading `| N |` cell only. Matching the title too would make
# this a second place the title is written down, which is the defect one register over.
#
# COVERAGE OF THE CONVERSE DIRECTION, mapped by the macOS seat so nobody re-derives it:
# a row left behind after its BODY was deleted is already caught by check 9, which is
# manifest-driven and reports an allocated number with no `## N.` heading. The one shape
# that slips through both is an INVENTED row for a number that was never allocated -- a
# planted `| 999 | ... |` with no body leaves the whole gate green, since 999 is not in
# the manifest either. Assessed and deliberately NOT gated: it needs someone to hand-type
# a row for an entry that does not exist, and the row is written beside the body. The two
# directions that actually occur are both covered.
Write-Host '-- Check 13: entry bodies with no TOC row --'
$apLines13 = Get-Content -Encoding UTF8 'sdd/ANTI-PATTERNS.md'
$bodies13  = @($apLines13 | ForEach-Object { if ($_ -match '^##\s+([0-9]+)\.') { [int]$Matches[1] } }) | Sort-Object -Unique
$rows13    = @($apLines13 | ForEach-Object { if ($_ -match '^\|\s*([0-9]+)\s*\|') { [int]$Matches[1] } }) | Sort-Object -Unique
$missing13 = @($bodies13 | Where-Object { $rows13 -notcontains $_ })
if ($missing13.Count -gt 0) {
    Write-Host '   FAIL - entr(ies) with a body but no row in the table of contents:' -ForegroundColor Red
    foreach ($n13 in $missing13) {
        $head13 = ($apLines13 | Where-Object { $_ -match "^##\s+$n13\." } | Select-Object -First 1)
        if ($head13.Length -gt 90) { $head13 = $head13.Substring(0, 90) }
        Write-Host "     ScrAP-$n13  $head13"
    }
    Write-Host '     The TOC is how an agent decides which bodies to read (SDD principle 7), so an'
    Write-Host '     entry absent from it is unreachable by the path meant to find it.'
    $failed = $true
} else {
    Write-Host '   PASS' -ForegroundColor Green
}

# -- Check 12: paths Windows cannot check out -----------------------------------
# Parity with lint-references.sh check 12. See that script for the measured
# rationale; the input set is `git ls-files`, NOT lint-references.scan, because the
# path that prompted this landed in the repository root, outside the curated scan.
# This platform is the one that actually suffers the failure, so the check is here
# as much to name the cause as to catch it.
Write-Host '-- Check 12: paths Windows cannot check out --'
# ENUMERATION PARITY with the shell port, which POLICY requires alongside pattern parity:
# both read `git ls-files -z`. Plain `ls-files` C-quotes any path containing a control
# character or a `"`, which replaces the raw bytes with an escaped rendering -- so the
# `[\x01-\x1f]` clause could never fire against it, and such a path was only ever caught
# by ACCIDENT, matching on the `"` its own quoting wrapped it in. (That quoting is
# unconditional, MEASURED: it still happens with `core.quotePath=false`, which governs
# non-ASCII bytes, not control characters.) `-z` gives the true bytes and makes the catch
# principled.
#
# BUT `-z` ALONE IS NOT ENOUGH IN POWERSHELL, and this is the part that does not port.
# PowerShell splits a native command's stdout into lines BEFORE the pipeline sees it, so a
# path containing \x0a is already torn in two by the time `-split "`0"` runs -- the split
# operates on fragments, not on the stream. MEASURED against a planted `nl<LF>name.txt`:
# the pipeline form yielded 350 elements including `nl` and `name.txt` as separate
# entries, zero elements containing a raw LF, and check 12 reported the other seven
# planted classes while silently missing that one. Reading the child's stdout as a STREAM
# yields 349 elements, one of which is `6e 6c 0a 6e 61 6d 65 2e 74 78 74` -- intact.
#
# So the enumeration goes through ProcessStartInfo rather than the native-command
# pipeline. The shell port has the same defect in different clothing (`tr '\0' '\n'`
# converts the separator and keeps the newline, which is the same tear); it needs
# `mapfile -d ''` or `while IFS= read -r -d ''`, and that half is the Linux seat's.
$psi12 = New-Object System.Diagnostics.ProcessStartInfo
$psi12.FileName               = 'git'
$psi12.Arguments              = 'ls-files -z'
$psi12.WorkingDirectory       = (Get-Location).Path
$psi12.RedirectStandardOutput = $true
$psi12.UseShellExecute        = $false
$psi12.StandardOutputEncoding = [System.Text.Encoding]::UTF8
$proc12 = [System.Diagnostics.Process]::Start($psi12)
$out12  = $proc12.StandardOutput.ReadToEnd()
$proc12.WaitForExit()
# THE ENUMERATOR'S EXIT STATUS IS READ. This port never looked at `$proc12.ExitCode`, so
# a failed enumeration was indistinguishable from a clean tree: zero paths in, zero
# violations out, PASS. Same defect as the bash port's process substitution, different
# clothing.
# The violation scan is NESTED INSIDE the success branch, not sequenced after it. When
# the enumerator's failure only set `$tracked12 = @()` and fell through, the empty set
# reached a scan that found nothing in it and printed `PASS` on the line below the FAIL --
# one check emitting both verdicts, with PASS last. The exit status was right and the
# transcript was not, which is the worse of the two to get wrong: a reader greps the
# per-check verdict, and the gate's whole subject here is that an empty input is
# indistinguishable from a clean tree. MEASURED with a `git` that exits 3 on `ls-files`.
if ($proc12.ExitCode -ne 0) {
    Write-Host "   FAIL - 'git ls-files' exited $($proc12.ExitCode); the tree was not enumerated, so this" -ForegroundColor Red
    Write-Host '     check proved nothing. A silent empty input reads exactly like a clean tree.'
    $failed = $true
} else {
    $tracked12 = @($out12 -split "`0" | Where-Object { $_ })
    $bad12 = @($tracked12 | Where-Object { Test-WinIllegalPath $_ })
    if ($bad12.Count -gt 0) {
        Write-Host '   FAIL - tracked path(s) illegal on Win32 (git checkout refuses the whole tree):' -ForegroundColor Red
        $bad12 | ForEach-Object { Write-Host "     $_" }
        $failed = $true
    } else {
        Write-Host '   PASS' -ForegroundColor Green
    }
}

Pop-Location
Write-Host ''
if ($failed) {
    Write-Host 'lint-references: FAILED' -ForegroundColor Red
    exit 1
}
Write-Host 'lint-references: OK' -ForegroundColor Green
exit 0
