#!/usr/bin/env bash
# Cross-reference gate — POLICY.md § "Build pipeline" step 9 states the rule; THIS
# SCRIPT is the source of truth for what is checked. Eight mechanical checks: three
# over the citations the codebase makes into the SDD registers, two over the test
# architecture — that the main-thread GTK suite's duplicated module list has not
# drifted (check 4) and that the superseded `#[gtk::test]` attribute has not returned
# (check 5) — one over document paths, that every file this tree points at still
# exists (check 6), one over the application ID, which several non-Rust packaging
# files restate and none of them derived (check 7), and one over the citation FORM
# itself, that no bare `AP-N` (illegal, because it names neither register unambiguously)
# has been written (check 8). Checks 4, 5, 6, 7 and 8 were all mutation-tested when
# written.
#
# WHY THIS EXISTS. Both classes below were found in one session, in a single 103-line
# change, by human review — after `cargo fmt`, `cargo clippy -D warnings` and 625
# passing tests had all gone green. No compiler or test can see a wrong reference:
# every gate we had passed identically whether the citations were right or wrong.
# That is the gap this closes, and it is why the checks are worth their three lines.
#
# WHAT IT CANNOT DO — read this before trusting a PASS. Check 2 proves a cited entry
# EXISTS, never that it is the RIGHT one. The error that motivated this script cited
# ScrAP-61 (a real entry, about a menu-button startup freeze) for a claim only
# ScrAP-90 (set_parent'd popovers are not auto-unparented) supports. Both checks pass
# on that. It slipped in because two neighbouring call sites cite 61 correctly for an
# ADJACENT clause, and the next author read the number as belonging to the detach it
# sat beside — citation proximity drift. The only guard is to place a number against
# the claim it supports and to re-read it when copying a sibling comment.
#
# AND A GATE MUST BE PROVEN TO FIRE. The first version of check 1 shipped with a
# regex that missed three real citation forms — including `sdd/ISSUES.md entry P`,
# which is the exact form of one of the two defects that motivated writing this
# script, and `ISSUES: P`, which the header claimed was covered. It was found by the
# Windows seat mutation-testing its own port rather than trusting a clean PASS. Hence
# `--self-test`: the corpus below is the evidence that the check discriminates, and it
# is a gate on the gate. Run it after touching either pattern.
#
# Usage:
#   scripts/lint-references.sh              # run the gate
#   scripts/lint-references.sh --self-test  # prove the patterns still discriminate
#   scripts/lint-references.sh --list-scan  # print the scan set (diff vs -ListScan)
set -euo pipefail
cd "$(dirname "$0")/.."

# The citation forms check 1 must catch. `ISSUES` is matched case-SENSITIVELY: a
# case-insensitive match hits ordinary prose ("issues a queue_draw"), which is a real
# false positive the PowerShell port hit because Select-String defaults the other way.
# The trailing `\b` after `[A-Z]` is what keeps `sdd/ISSUES.md TOC` from matching — an
# ID is a single letter standing alone, not the first letter of a word.
#
# The second alternative catches the `#`-sigil form (`ISSUES #P`, `ISSUES #BBB`), which
# the single-letter branch cannot: `#` is not a separator it accepts, and a MULTI-letter
# ID (this register has used `WW`/`ZZ`/`BBB`) never satisfies `[A-Z]\b`. Three such
# references were sitting in `tests/MANUAL-TEST.md`, all naming issues long since
# resolved — dangling exactly as principle 6 predicts — while this gate passed. A
# multi-letter ID is only recognised BEHIND the `#`, because bare uppercase runs after
# `ISSUES` are ordinary prose (`ISSUES.md TOC`, `ISSUES.md IDs`).
#
# The connecting word is `[a-z]+` — ANY lowercase word, not an enumeration. It was
# `(entry )?`, the one noun the pattern's author happened to write, and
# an `ISSUES.md item <letter>` citation walked through it into `src/`, in the very commit
# that rewrote the entry it named. That is this script's OWN recorded failure (check 6 passing over 21 danglers
# because its pattern required a `.md` the citations did not carry) recurring one check
# across — which is also the argument against fixing it by adding `item` to the list:
# the next citation says `issue`, or `letter`, or `row`. Enumerating the vocabulary of a
# free-text citation is a losing game, so match the SHAPE instead: `ISSUES`, a
# connector, a lone capital. The discrimination still comes from `[A-Z]\b` and
# case-sensitivity, and a lowercase connector cannot reach a capitalised document name,
# so `read ISSUES.md and TECH.md together` stays clean. Both engines agree here because
# the check is boolean — leftmost-longest vs leftmost-first cannot change whether a
# match exists, only which one is reported, and nothing reads the capture.
ISSUES_RX='\bISSUES(\.md)?([ .:_-]*([a-z]+ )?[A-Z]\b|[ .:_-]*#[A-Z]+\b)'

# The document-path forms check 6a must catch. Pinned beside ISSUES_RX and self-tested
# for the same reason: check 6 passed clean over 21 live danglers because its pattern
# required a `.md` the citations did not carry, and nothing proved otherwise. The `.md`
# alternative is deliberately FIRST — POSIX ERE takes leftmost-longest and .NET takes
# leftmost-first, and this ordering makes `PLAN.x.md` match whole under both, which is
# what keeps the twin honest.
PATH_RX='((sdd|tests|packaging|gtktest|scripts|data|src)/[A-Za-z0-9._/-]+\.md|\bPLAN\.[A-Za-z0-9._-]+\.md|\bPLAN\.[A-Za-z0-9_-]+)'

# Check 8's two patterns. A citation's correctness is not a property of its syntax, so
# this pair does not try to check one — it makes the ONE form that cannot be checked at
# all illegal. `ScrAP-N` resolves in this tree (checks 2 and 3 prove the entry exists);
# `GTK4Rs/AP-N` names the `gtk4-rs` skill, which may not be installed, so it is checkable
# for FORM only. A BARE `AP-N` is neither: it is simultaneously the current skill form
# and the historical project form, so the same string is correct or wrong depending on
# when it was written and nothing in the text says which.
#
# BOTH LEGAL FORMS ARE SINGLE TOKENS, AND THAT IS THE POINT — not a style preference.
# The first version of this convention spelled the skill form `skill AP-N`, with a space,
# and it was already broken in the tree the day the gate was written: `sdd/TECH.md` and
# `src/preview/annotate/overlay.rs` each carried one where a Markdown/rustfmt wrap had
# split `skill` from `AP-147`/`AP-78` across two lines. That is not a lint inconvenience,
# it defeats the ONE job the form was chosen for. The whole justification for legalising
# an unverifiable citation is that it becomes ENUMERABLE — a greppable list a human can
# audit deliberately — and `grep 'skill AP-'` silently MISSES every wrapped one, so the
# audit set was quietly incomplete while looking exhaustive. A form whose correctness
# depends on where a line happens to wrap cannot carry that guarantee; a token that
# contains no whitespace cannot be broken by any wrapper, because both break on
# whitespace and nowhere else.
#
# It also makes the check enforce the SPELLING rather than hope for it: `/` is not in
# `[a-zA-Z-]`, so a mistyped prefix (`gtk4rs/AP-9`, `Gtk4Rs/AP-9`) survives stage 1 and
# is reported, where the old form let `skilll AP-9` pass as ordinary prose. The one
# blind spot left is a typo that eats the separator too (`GTK4RsAP-9`) — the same
# unavoidable hole `ScrXAP-9` has, and not worth more machinery.
#
# TWO STAGES, NOT ONE REGEX, and the reason is portability rather than taste. The rule
# is "an `AP-N` not preceded by `Scr` or by `GTK4Rs/`", which is a lookbehind — POSIX ERE
# has none, `grep -P` is absent from the BSD grep macOS ships, and this file already
# refuses `\s` for that reason. So: BARE_AP_RX finds candidates (it excludes `ScrAP-N`
# by construction, since `r` is in `[a-zA-Z-]`), then LEGAL_AP_RX is DELETED from the
# line and the candidate pattern is re-applied to what is left. A line reading
# `GTK4Rs/AP-57 / AP-34b` is thereby reported for the SECOND citation only, which is the
# correct answer and the one a single regex cannot give.
#
# `[^a-zA-Z-]` also excludes a hyphen, so `ScrAP-` written as `Scr-AP-` and identifier
# fragments like `x-AP-9` do not match; `SOAP-12` is excluded by the alpha class.
BARE_AP_RX='(^|[^a-zA-Z-])AP-[0-9]+'
LEGAL_AP_RX='GTK4Rs/AP-[0-9]+'

# The check-8 predicate, defined ONCE so `--self-test` exercises the program the gate
# runs rather than a copy of it — the same rule as PROSE_AWK below, and for the same
# reason: 5b shipped a bug its self-test could not see because the test asserted against
# its own copy.
#
# `stripped` is computed into a variable and matched with a HERE-STRING, never as
# `sed … | grep -q`. Under `pipefail` a matching `grep -q` closes the pipe early, `sed`
# takes SIGPIPE, and the PIPELINE reports 141 — so the predicate would return FALSE
# exactly when it should return true. That trap is documented twice more in this file;
# it is repeated here because the pipe form is what one naturally writes.
is_bare_ap() {
    local stripped
    # `s|…|…|`, NOT `s/…/…/`: LEGAL_AP_RX contains a `/` (`GTK4Rs/AP-N`), which as a
    # delimiter makes sed abort with "unknown option to `s'". Measured the moment the form
    # changed — and note HOW it failed: sed wrote to stderr, the substitution silently did
    # nothing, and `is_bare_ap` then reported every legal citation as bare. A gate that
    # over-reports is at least loud; the same collision on the other side of a `grep -v`
    # would have been a silent under-report.
    stripped=$(sed -E "s|$LEGAL_AP_RX||g" <<<"$1")
    grep -qE "$BARE_AP_RX" <<<"$stripped"
}

# ── The shared scan-set contract ───────────────────────────────────────────────
#
# scripts/lint-references.scan is the ONE enumeration definition this script and its
# PowerShell twin both consume. Read its header for why it exists; the short version
# is that the two scripts enumerated different file sets, so `.agents/`, `docs/` and
# `THIRD-PARTY-LICENSES.md` were linted here and invisible there — and POLICY claimed
# neither could be the lenient one. Pinning the PATTERN was never enough: a gate is
# its pattern AND the set it runs over.
#
# EVERY check below draws from that one set, each narrowing it rather than walking the
# tree for itself, and `--list-scan` prints exactly it. The contract's `maxdepth` is a
# TRIPWIRE, not a filter: a file past the budget makes this gate refuse to run and name
# the path, which is what makes one shared set safe.
SCAN_CONTRACT="scripts/lint-references.scan"
# `|| true` for the same reason as every other extraction here: an absent field is a
# state this script must be able to REPORT, and under `set -euo pipefail` a no-match
# grep would instead abort it silently before any diagnostic could run.
scan_field() { grep -E "^$1[[:space:]]+" "$SCAN_CONTRACT" | awk '{print $2}' || true; }
if [ ! -e "$SCAN_CONTRACT" ]; then
    echo "lint-references: $SCAN_CONTRACT is missing — it is the shared scan-set" >&2
    echo "  contract this gate and its PowerShell twin both read. Without it there is" >&2
    echo "  no file set to check, which is NOT the same as a clean tree." >&2
    exit 1
fi
SCAN_ROOTS=$(scan_field root)
SCAN_FILES=$(scan_field file)
SCAN_MAXDEPTH=$(scan_field maxdepth)
SCAN_PRESCRIPTIVE=$(scan_field prescriptive)
# Fail fast on an empty/garbled contract, and do it HERE rather than letting the
# checks run over nothing. Measured, not theorised: with `$SCAN_ROOTS` empty, check
# 6a's `grep -rnoE "$PATH_RX" $SCAN_ROOTS $SCAN_FILES` receives no file operands and
# so reads STDIN — the gate HANGS FOREVER instead of failing. In CI that is a job
# that never returns; run by hand it looks like a slow check. An empty scan set must
# be a loud error, because the one thing worse than a gate that fails is a gate that
# silently has nothing to look at.
if [ -z "$SCAN_ROOTS" ] || [ -z "$SCAN_MAXDEPTH" ]; then
    echo "lint-references: $SCAN_CONTRACT defines no 'root' and/or no 'maxdepth'." >&2
    echo "  Refusing to run: an empty scan set would pass every check vacuously." >&2
    exit 1
fi
# Same rule, its own message, because it fails differently. Measured: delete the
# `prescriptive` lines from the contract and check 5b reports PASS — over nothing.
# That is the failure this contract's header calls strictly worse than disagreement,
# since BOTH ports read the same file and would agree, cleanly and vacuously, while
# `--list-scan` showed no difference at all. An empty class is a garbled contract, not
# an empty opinion; the way to switch check 5b off is to delete check 5b.
if [ -z "$SCAN_PRESCRIPTIVE" ]; then
    echo "lint-references: $SCAN_CONTRACT defines no 'prescriptive' document." >&2
    echo "  Refusing to run: check 5b would PASS over an empty set, which reads" >&2
    echo "  identically to a clean tree. Restore the class or remove the check." >&2
    exit 1
fi
# Reject any line that is not a comment, blank, or a recognised `<field> <value>`.
#
# A typo'd field name is otherwise INVISIBLE: it simply fails to match, the field
# reads as absent, and "absent" is indistinguishable from "legitimately empty". That
# is the one failure mode this contract cannot afford, because BOTH scripts read the
# SAME file — so they would agree perfectly on scanning nothing, and the `--list-scan`
# diff (the only parity proof available) would come back identical and clean. R1-03
# was the two gates disagreeing; two gates agreeing vacuously is strictly worse,
# because the diff that is supposed to catch it reports success.
bad_contract=$(grep -nvE '^[[:space:]]*(#.*)?$|^(root|file|prescriptive|exclude|maxdepth)[[:space:]]+[^[:space:]]+[[:space:]]*$' \
                 "$SCAN_CONTRACT" || true)
if [ -n "$bad_contract" ]; then
    echo "lint-references: $SCAN_CONTRACT has lines that are neither comments nor" >&2
    echo "  a recognised 'root|file|prescriptive|exclude|maxdepth <value>' pair:" >&2
    echo "$bad_contract" | sed 's/^/    /' >&2
    echo "  A misspelled field reads as an ABSENT field, and both gates would then" >&2
    echo "  agree on scanning nothing. Fix the line rather than deleting it." >&2
    exit 1
fi
# Anchored alternation of excluded path prefixes, e.g. `^(target/|docs/|\.git/)`.
scan_excludes=$(scan_field exclude)
if [ -n "$scan_excludes" ]; then
    SCAN_EXCLUDE_RX="^($(echo "$scan_excludes" | sed 's/\./\\./g' | paste -sd'|' -))"
else
    # No excludes to apply: `^$` matches only a wholly empty line, which none of the
    # three enumeration sites ever emit, so this is a safe no-op filter.
    SCAN_EXCLUDE_RX='^$'
fi

# ── Symlink-exclusion set ────────────────────────────────────────────────────
#
# See scripts/lint-references.scan, "SYMLINKS ARE EXCLUDED", for why. Built once, here,
# and applied at the ONE place the scan set is produced (immediately below), so there is
# no longer a set of enumeration sites that could drift apart. A path is excluded if
# EITHER the filesystem says it's a symlink (`-type l`/`-L`) OR the git index records
# mode 120000 for it — each test is blind on a different platform, so the union is what
# keeps this and the PowerShell twin agreeing.
#
# The walk is UNBOUNDED, and that is deliberate: `maxdepth` is no longer a filter
# anywhere in this script (see the scan-set block below, and the contract's `maxdepth`
# paragraph). Bounding it here would hide a symlink past the budget from the exclusion
# while the depth tripwire was still deciding whether to fire on it.
#
# The index half is a SINGLE `git ls-files -s` call, not one per file: field 1 is the
# mode, field 4 is the path (awk's default field splitting already does the right thing
# with the tab before the path). It must not hard-fail outside a git checkout — same
# `|| true` idiom as scan_field() above, for the same pipefail reason: a no-match/absent
# `git` would otherwise abort this assignment silently, and an absent git is a state
# this script must degrade from, not die on.
symlink_paths=$(
    { find $SCAN_ROOTS -type l 2>/dev/null | sed 's|^\./||'
      for n in $SCAN_FILES; do [ -L "$n" ] && echo "$n"; done
      git ls-files -s 2>/dev/null | awk '$1 == "120000" {print $4}' || true
    } | sed '/^$/d' | LC_ALL=C sort -u)
# `(:|$)` rather than a bare `$`: check 6a's hits are `path:line:match` lines, while
# --list-scan and check 6b emit bare paths. One regex has to exclude a symlink in both
# shapes without drifting, so it matches a full line OR a path immediately followed by
# the `:` that starts grep -n's line/match suffix.
if [ -n "$symlink_paths" ]; then
    SCAN_SYMLINK_RX="^($(echo "$symlink_paths" | sed 's/\./\\./g' | paste -sd'|' -))(:|\$)"
else
    # No symlink to exclude: `^$` matches only a wholly empty line, which the
    # enumeration below never emits, so this is a safe no-op filter.
    SCAN_SYMLINK_RX='^$'
fi

# ── The scan set: ONE enumeration, produced once, consumed by every check ──────
#
# There used to be three, and they disagreed. Measured by planting a Markdown file one
# level past the budget: check 6a (`grep -r`) was UNBOUNDED here and BOUNDED in the
# PowerShell twin, so the shell side caught a dangler at depth 7 and the Windows side
# did not — a lenient/strict split in the one gate whose whole purpose is to prove the
# two platforms enforce the same rule. Check 1 walked its own four-root list, unbounded,
# and disagreed with check 6 IN THE SAME RUN. And `--list-scan`, the only parity proof
# this project has, printed a set that some of the checks did not consume, which proves
# nothing about those checks — the same defect the artifact was introduced to close, one
# level down (ScrAP-207).
#
# So: this block produces the set, `--list-scan` prints exactly it, and every check
# below narrows it with `scan_subset`/`scan_subset_not` rather than walking the
# filesystem again. A second walk is a second enumeration; there is no such thing as a
# harmless one here.
scan_walk() {
    for r in $SCAN_ROOTS; do
        [ -d "$r" ] && find "$r" -type f 2>/dev/null
    done
    for n in $SCAN_FILES; do
        [ -e "$n" ] && echo "$n"
    done
    # The trailing `true` is the `--list-scan` exit-code defect, fixed at its source: the
    # last `[ -e ] && echo` returns 1 when that `file` entry is absent, `pipefail`
    # propagates it, and under `set -e` the whole script died BEFORE `--list-scan`'s own
    # `exit 0` — so the parity proof could print a correct list and still exit non-zero,
    # inviting the reader to take the exit code as the finding.
    true
}
# `LC_ALL=C sort -u` rather than a bare `sort -u`: the ONLY proof that the two ports
# agree is a human diffing this output against the twin's `-ListScan`, and that diff has
# to be readable. Measured at the time of writing — both ports enumerated the same 235
# files and the diff still showed fifteen changed lines, purely because one collates
# case-insensitively and the other by locale. A parity proof whose clean state is not
# byte-identical trains its reader to skim it. Both ports now sort by ORDINAL byte value.
SCAN_SET=$(scan_walk | sed 's|^\./||' \
             | grep -Ev "$SCAN_EXCLUDE_RX" \
             | grep -Ev "$SCAN_SYMLINK_RX" \
             | LC_ALL=C sort -u || true)
if [ -z "$SCAN_SET" ]; then
    echo "lint-references: $SCAN_CONTRACT enumerates no file at all." >&2
    echo "  Refusing to run: every check would PASS over an empty set, which reads" >&2
    echo "  identically to a clean tree." >&2
    exit 1
fi

# `maxdepth` IS A TRIPWIRE, NOT A FILTER, and that distinction is the point.
#
# As a filter it was a silent coverage cliff: a file past the budget was simply absent
# from the set, and "absent because the budget dropped it" is indistinguishable from
# "absent because the tree does not contain it". Binding every check to one enumeration
# would then have REDUCED what check 1 catches, silently — the exact shape of the defect
# this block exists to remove. Failing loudly is what makes the single set safe, and it
# turns the budget into a statement that the tree has outgrown the contract.
#
# Depth is counted the way `find <root> -maxdepth N` counts it: segments BELOW the root.
# Deriving it from the repo-relative path rather than from the walker retires the
# `find -maxdepth` -> `Get-ChildItem -Depth` off-by-one conversion entirely, which was
# the one place the two implementations were licensed to differ and had nothing
# asserting the conversion was right. `file` entries match no root prefix and are
# therefore never counted — they are named individually, not reached by a walk.
SCAN_TOO_DEEP=$(echo "$SCAN_SET" | awk -v roots="$(echo $SCAN_ROOTS)" -v max="$SCAN_MAXDEPTH" '
    BEGIN { nr = split(roots, R, " ") }
    {
        for (i = 1; i <= nr; i++) {
            p = R[i] "/"
            if (substr($0, 1, length(p)) == p) {
                if (split(substr($0, length(p) + 1), seg, "/") > max) print
                next
            }
        }
    }' || true)
if [ -n "$SCAN_TOO_DEEP" ]; then
    echo "lint-references: these files are deeper than $SCAN_CONTRACT's 'maxdepth $SCAN_MAXDEPTH':" >&2
    echo "$SCAN_TOO_DEEP" | sed 's/^/    /' >&2
    echo "  Refusing to run. Raise 'maxdepth' in the contract — then re-run --list-scan" >&2
    echo "  here and -ListScan on Windows and diff them — or move the files. Truncating" >&2
    echo "  the set instead would make this gate lenient WITHOUT SAYING SO." >&2
    exit 1
fi

# Members of the contract set, narrowed. `$1` is an ERE tested against each repo-relative
# path; `scan_subset` keeps what matches, `scan_subset_not` drops it. Every check draws
# its inputs from one of these two and never from the filesystem.
scan_subset()     { echo "$SCAN_SET" | grep -E  "$1" || true; }
scan_subset_not() { echo "$SCAN_SET" | grep -Ev "$1" || true; }

# The `prescriptive` class is a LABEL ON SET MEMBERS, not a fourth enumeration. Before
# this, check 5b read an explicit list of documents that was neither the bounded nor the
# unbounded set, so `--list-scan` did not print what 5b consumed either. Asserting
# membership is what makes the class a subset rather than a separate opinion — and it
# fires the moment a document is marked prescriptive but sits outside every `root`, is
# excluded, or has been deleted.
for p in $SCAN_PRESCRIPTIVE; do
    grep -qxF "$p" <<<"$SCAN_SET" && continue
    echo "lint-references: $SCAN_CONTRACT marks '$p' prescriptive, but that path is not" >&2
    echo "  in the scan set --list-scan prints. A prescriptive document is a LABEL on a" >&2
    echo "  set member, not a separate list: check 5b would read a file no parity proof" >&2
    echo "  covers. Add a 'root'/'file' entry that contains it, or drop the label." >&2
    exit 1
done

# The check-5b program, defined ONCE because `--self-test` must exercise the program
# the gate actually runs. A self-test asserting against its own copy proves the copy,
# which is the same two-copies-drift defect the shared scan contract exists to stop —
# and 5b is the check that already shipped a bug the self-test could not see.
PROSE_AWK='
    function flush() {
        if (hit && !repl) printf "  %s:%d: %s\n", hitfile, hitline, hittext
        hit = 0; repl = 0
    }
    FNR == 1 { flush() }
    /^[[:space:]]*$/ { flush(); next }
    {
        if (index($0, "#[gtk::test]") && !hit) {
            hit = 1; hitline = FNR; hittext = $0; hitfile = FILENAME
        }
        if (index($0, "gtktest")) repl = 1
    }
    END { flush() }
'

if [ "${1:-}" = "--self-test" ]; then
    st_fail=0
    must_match='see ISSUES P
see sdd/ISSUES.md entry P for details
see ISSUES.md entry P
see ISSUES-P
see ISSUES_P
see ISSUES: P
(TDD 17.31 / ISSUES #BBB)
regression, see ISSUES #P
see sdd/ISSUES.md #WW
(ISSUES.md item P)
see ISSUES issue P
the ISSUES.md letter P'
    must_not_match='issues a queue_draw that CLEARS the cache
see sdd/ISSUES.md
the sdd/ISSUES.md TOC lists them
the sdd/ISSUES.md IDs are ephemeral
ISSUES.md is not a changelog
the ISSUES register empties as it works
ISSUES.md entries are deleted when fixed
read ISSUES.md and TECH.md together'
    while IFS= read -r line; do
        grep -qE "$ISSUES_RX" <<<"$line" || { echo "  MISS (should match): $line"; st_fail=1; }
    done <<<"$must_match"
    while IFS= read -r line; do
        grep -qE "$ISSUES_RX" <<<"$line" && { echo "  FALSE POSITIVE: $line"; st_fail=1; }
    done <<<"$must_not_match"

    # Check 6a's corpus. The `expected -> line` form pins WHAT is extracted, not merely
    # that something matched: the bug this closes was a pattern that matched the wrong
    # substring, which a boolean corpus cannot see.
    path_must_match='PLAN.help|// Footer status bar (PLAN.help D3): one accessible label
PLAN.annotations-viewer|/// no row (PLAN.annotations-viewer Q1 / TDD 20.2).
PLAN.copy-path|// enabled in connect_open at the point the path is set (PLAN.copy-path D1/D3).
PLAN.typed-gtk-seams.md|see PLAN.typed-gtk-seams.md for the seam list
sdd/PLAN.help.md|retired: sdd/PLAN.help.md
sdd/TECH.md|the module map in sdd/TECH.md
tests/MANUAL-TEST.md|the plan lives in tests/MANUAL-TEST.md'
    # Prose mentions that resolve against nothing, and the one fixture form that made
    # the extension look unsafe. `PLAN.md` is not a plan filename under SDD naming.
    # NOTE a directory-qualified path that EXISTS (`sdd/ISSUES.md`) is not a false
    # positive and is deliberately absent here: 6a is supposed to extract it, then
    # resolve it and find it present. This corpus is about what must not be extracted
    # AS A PATH AT ALL — bare prose mentions, the `PLAN.<topic>.md` placeholder, and
    # the link-parser fixture.
    path_must_not_match='see POLICY.md for the rule
a PLAN.<topic>.md is deleted by design once implemented
doc_link_fragment("./sub/PLAN.md#caf%C3%A9")'
    # `|| true` on every extraction: under `set -euo pipefail` a no-match `grep` exits
    # 1, `pipefail` propagates it through `| head`, and the assignment kills the script
    # SILENTLY — self-test exits 1 having printed nothing, which reads exactly like a
    # failing corpus and is not one. The no-match case is the whole point of the
    # negative corpus, so it must be a value, not an error.
    while IFS= read -r line; do
        want=${line%%|*}; subject=${line#*|}
        got=$(grep -oE "$PATH_RX" <<<"$subject" | head -1 || true)
        [ "$got" = "$want" ] || { echo "  WRONG EXTRACT: want '$want', got '$got' from: $subject"; st_fail=1; }
    done <<<"$path_must_match"
    while IFS= read -r line; do
        got=$(grep -oE "$PATH_RX" <<<"$line" | head -1 || true)
        # `PLAN.md` may be MATCHED; check 6a skips it by name. Anything else is a
        # false positive.
        case "$got" in ""|PLAN.md) ;; *) echo "  FALSE POSITIVE: $got from: $line"; st_fail=1 ;; esac
    done <<<"$path_must_not_match"

    # Check 8's corpus. It exercises `is_bare_ap`, the predicate the check itself calls,
    # not the two regexes in isolation: the rule is a strip-then-match COMPOSITE, and a
    # corpus over BARE_AP_RX alone would pass while the strip did nothing.
    #
    # The discriminating cases are the tail of each list. A line carrying BOTH a legal and
    # a bare citation must be reported (for the bare one only); the near-misses
    # (`Scr-AP-`, `SOAP-`) must not be, because a check with false positives gets disabled
    # while an incomplete one gets extended (check 3's rule); and a MISSPELLED prefix must
    # be reported, which is the property the single-token form buys and the retired
    # `skill AP-N` form did not have. `the gtk4-rs skill AP-78` is in the FLAG list on
    # purpose — the old two-token spelling is no longer legal, and this corpus is what
    # stops it drifting back in. No apostrophes in these strings: they sit inside a
    # single-quoted shell string, and `the skill's AP-78` would terminate it.
    bare_ap_must_flag='see AP-79 for the pump-loop lesson
(AP-30 deferred popdown)
per the gtk4-rs skill AP-78 masking
AP-1 leads the router
GTK4Rs/AP-57 / AP-34b enum split
see ScrAP-79 and AP-88 together
gtk4rs/AP-9 is a misspelled prefix
GTK4Rs / AP-9 is not one token'
    bare_ap_must_not_flag='see ScrAP-79 for the tab-close gesture guard
see GTK4Rs/AP-79 for the pump-loop bound
a bare `AP-N` is illegal anywhere in the tree
ScrAP-162 pairs with GTK4Rs/AP-153
SOAP-12 is not a citation
the shorthand #79 inside this register
Scr-AP-9 is not a citation either'
    while IFS= read -r line; do
        is_bare_ap "$line" || { echo "  MISS (should flag): $line"; st_fail=1; }
    done <<<"$bare_ap_must_flag"
    while IFS= read -r line; do
        if is_bare_ap "$line"; then echo "  FALSE POSITIVE (check 8): $line"; st_fail=1; fi
    done <<<"$bare_ap_must_not_flag"

    # Symlink-exclusion canary — two independent assertions, both required (see
    # scripts/lint-references.scan, "SYMLINKS ARE EXCLUDED"). (1) at least one
    # mode-120000 path exists in the repo, currently exactly one:
    # tests/fixtures/escapes-via-symlink.md; without it, (2) would pass vacuously,
    # which is the exact defect this project already paid for once. (2) no mode-120000
    # path appears in --list-scan's output. --list-scan is run as a fresh subprocess
    # (via "$0") since this block runs inside the --self-test branch, before the
    # --list-scan branch further down is even reached.
    st_git_symlinks=$(git ls-files -s 2>/dev/null | awk '$1 == "120000" {print $4}' || true)
    if [ -z "$st_git_symlinks" ]; then
        echo "  FAIL: no mode-120000 path found in the git index — the canary fixture"
        echo "    (tests/fixtures/escapes-via-symlink.md) is gone, so the next assertion"
        echo "    (no symlink in --list-scan) would now pass vacuously."
        st_fail=1
    else
        st_list_scan=$(bash "$0" --list-scan)
        while IFS= read -r st_sl; do
            [ -z "$st_sl" ] && continue
            if grep -qxF "$st_sl" <<<"$st_list_scan"; then
                echo "  FAIL: symlink '$st_sl' (git mode 120000) appears in --list-scan output"
                st_fail=1
            fi
        done <<<"$st_git_symlinks"
    fi

    # Check 5b's corpus, and it is a MULTI-FILE one on purpose (ScrAP-226).
    #
    # 5b was the only check this self-test did not exercise, and that is precisely
    # where a defect lived through a mutation-tested release: two file-boundary bugs
    # (a filename read at flush time, and paragraph state carried across a boundary)
    # that CANNOT be reproduced on a single file. The Python cross-check that
    # "validated the algorithm" ran one file at a time and was structurally incapable
    # of seeing either. So this corpus asserts across a boundary and pins WHICH FILE is
    # reported, not merely that something was.
    #
    # Case 1 — a stale prescription in a NON-LAST file must be attributed to that file.
    # Case 2 — a later file mentioning `gtktest` must not suppress an earlier file's
    #          hit (the false negative, which is the worse half).
    # Case 3 — a genuine contrast, naming both attributes, must stay silent.
    st_5b_dir=$(mktemp -d)
    # No trailing newline: an unterminated final paragraph is the trigger for both bugs
    # and is the shape every real document in the prescriptive set has.
    printf 'intro\n\nalways annotate with #[gtk::test] first.' > "$st_5b_dir/first.md"
    printf 'Note: gtktest is the harness.\n\ntail\n'           > "$st_5b_dir/second.md"
    printf 'Never write #[gtk::test]; write #[gtktest::test].\n' > "$st_5b_dir/contrast.md"
    st_5b_out=$(awk "$PROSE_AWK" "$st_5b_dir/first.md" "$st_5b_dir/second.md" 2>/dev/null)
    case "$st_5b_out" in
        *first.md:3*) ;;
        *) echo "  FAIL (check 5b): a stale prescription in the FIRST of two files was"
           echo "    reported as '$st_5b_out' — expected first.md:3. Either it was"
           echo "    swallowed by the next file's paragraph state, or it was attributed"
           echo "    to the wrong file."
           st_fail=1 ;;
    esac
    st_5b_quiet=$(awk "$PROSE_AWK" "$st_5b_dir/contrast.md" 2>/dev/null)
    if [ -n "$st_5b_quiet" ]; then
        echo "  FALSE POSITIVE (check 5b): a passage naming BOTH attributes is a"
        echo "    contrast, not an instruction, and must not be flagged: $st_5b_quiet"
        st_fail=1
    fi
    rm -rf "$st_5b_dir"

    # ── The scan set itself: three assertions, each able to FAIL on its own ────
    #
    # The `.scan` header used to claim the depth conversion "is asserted by
    # --self-test". Nothing asserted it — a documented guarantee with nothing behind
    # it, which is worse than an undocumented one because it stops the next reader from
    # writing the check. These are that check, and each is written so that the mutation
    # it is meant to catch actually fails it (ScrAP-209 applied prospectively).
    #
    # (i) `--list-scan` prints EXACTLY what the checks consume. This is the assertion
    #     that would have caught the original defect: the parity artifact enumerated a
    #     bounded set while check 6a grepped an unbounded one, so it certified a set
    #     some checks did not consume. It is close to tautological while both read one
    #     variable — say so rather than overclaim — but it is not vacuous: re-introduce
    #     a separate `find` in the `--list-scan` branch, with any different bound or
    #     filter, and this fires. Run against the PRISTINE tree, before the depth probe
    #     below plants anything, since the parent's $SCAN_SET was built at startup.
    st_listed=$(bash "$0" --list-scan)
    if [ "$st_listed" != "$SCAN_SET" ]; then
        echo "  FAIL: --list-scan's output is not the set the checks consume:"
        diff <(echo "$SCAN_SET") <(echo "$st_listed") | sed 's/^/    /'
        st_fail=1
    fi

    # (ii) a file at EXACTLY maxdepth is in the set, and (iii) a file at maxdepth+1
    # makes the gate FAIL. (iii) is deliberately "the gate fails and names the path"
    # rather than "the file is absent from the set": absence is also what a broken
    # enumerator produces, so an absence assertion passes for the wrong reason. It is
    # also the behaviour that makes one shared set safe — a budget that quietly drops
    # files is indistinguishable from a tree that has none.
    #
    # The probe is planted under the first contract root rather than a named one, so it
    # follows the contract instead of restating it, and its depth is DERIVED from
    # `maxdepth` — hard-coding six here would leave the assertion passing for a
    # contract that had moved.
    st_probe_root=""
    for st_r in $SCAN_ROOTS; do [ -d "$st_r" ] && { st_probe_root="$st_r"; break; }; done
    if [ -z "$st_probe_root" ]; then
        echo "  FAIL: no contract root exists on disk — the depth assertions cannot run,"
        echo "    and a self-test that silently skips them is the defect they exist to close."
        st_fail=1
    else
        # `rest` (the path below the root) must have exactly $SCAN_MAXDEPTH segments:
        # the probe directory is the first, then maxdepth-2 more, then the file.
        st_probe="$st_probe_root/lint-scan-depth-probe"
        trap 'rm -rf "$st_probe"' EXIT
        rm -rf "$st_probe"
        st_dir="$st_probe"; st_i=2
        while [ "$st_i" -lt "$SCAN_MAXDEPTH" ]; do st_dir="$st_dir/d$st_i"; st_i=$((st_i + 1)); done
        mkdir -p "$st_dir"
        # Empty on purpose: the probe must be invisible to every OTHER check. A file
        # with content here would have to carry a citation or a link, and this entry's
        # own investigation tripped check 6 by writing one out.
        : > "$st_dir/at-max.md"
        st_at_max="$st_dir/at-max.md"
        # Captured, then matched — NOT `bash "$0" --list-scan | grep -q`. Under
        # `pipefail` a `grep -q` that matches closes the pipe early, the writer takes
        # SIGPIPE, and the PIPELINE reports 141 on success, so the assertion failed
        # precisely when it should have passed. The symlink canary above already avoids
        # this; it is repeated here because it reads like an idiom and is a trap.
        st_probe_listed=$(bash "$0" --list-scan)
        if ! grep -qxF "$st_at_max" <<<"$st_probe_listed"; then
            echo "  FAIL: a file at exactly maxdepth ($SCAN_MAXDEPTH) is MISSING from the scan set:"
            echo "    $st_at_max"
            echo "    The budget is off by one, or the enumeration stops short of it."
            st_fail=1
        fi
        mkdir -p "$st_dir/over"
        : > "$st_dir/over/too-deep.md"
        st_too_deep="$st_dir/over/too-deep.md"
        st_rc=0
        st_out=$(bash "$0" 2>&1) || st_rc=$?
        if [ "$st_rc" -eq 0 ]; then
            echo "  FAIL: a file at maxdepth+1 did not fail the gate — it was silently"
            echo "    truncated out of the scan set, which is the coverage cliff this"
            echo "    assertion exists to catch: $st_too_deep"
            st_fail=1
        elif ! grep -qF "$st_too_deep" <<<"$st_out"; then
            echo "  FAIL: the gate failed with a file at maxdepth+1 present, but did not"
            echo "    NAME it, so the exit code cannot be attributed to the depth budget:"
            echo "    $st_too_deep"
            st_fail=1
        fi
        rm -rf "$st_probe"
        trap - EXIT
    fi

    [ $st_fail -eq 0 ] && echo "self-test: PASS" || echo "self-test: FAIL"
    exit $st_fail
fi

# `--list-scan` prints the enumerated scan set, one repo-relative path per line, and
# exits. It is how the two implementations are checked against each other: no test can
# run both, because neither platform has the other's shell, so the only available proof
# that they agree is to run this on each and diff the output. That the SETS agree is
# the half a shared regex corpus cannot demonstrate — and the half that had drifted.
if [ "${1:-}" = "--list-scan" ]; then
    # Prints `$SCAN_SET` itself, not a re-derivation of it. That is the whole fix: this
    # branch used to run its own `find`, so the artifact certifying the set could not
    # certify the set. `--self-test` asserts the two are byte-identical, which is the
    # assertion that would have caught the original defect.
    echo "$SCAN_SET"
    exit 0
fi

# ── Preflight: the files the checks READ must exist ────────────────────────────
#
# Guarding the extractions with `|| true` (above) makes an empty match a value rather
# than a silent abort — but it cannot distinguish "this file has no entries" from
# "this file is not there", and a MISSING register would make check 3 compare nothing
# against nothing and print PASS. A gate that reports success because its input
# vanished is the worst outcome available to it, and it is the same defect as every
# other one closed here: an absent input treated as an empty result rather than as an
# error. Check existence once, explicitly, and name the file.
for required in sdd/ANTI-PATTERNS.md sdd/ISSUES.md src/icons.rs src/lib.rs src/gtk_suite.rs; do
    [ -e "$required" ] && continue
    echo "lint-references: $required is missing — this gate reads it." >&2
    echo "  Refusing to run: checks over a file that is not there report PASS." >&2
    exit 1
done

fail=0

# ── Check 1: no ISSUES.md entry referenced from outside ISSUES.md ────────────────
#
# SDD principle 6. ISSUES entries are ephemeral BY DESIGN — the register is working
# when it empties — so every pointer to one is born with an expiry date and dangles
# the moment the fix lands. Worse, if IDs are ever compacted it does not break
# loudly, it lies quietly: the letter now names an unrelated issue.
#
# The cross-branch case is sharper still and is what caught us: a reference written
# on a platform branch, where the issue is real, dangles ON ARRIVAL when the change
# is cherry-picked to a tree that never had that issue.
#
# Durable alternatives: cite an ANTI-PATTERNS.md entry (permanent by design), or
# write a self-contained comment that explains the constraint and cites nothing.
#
# PLAN.*.md is excluded: its dated session-log entries are a historical record of
# what a thing was called on a given day, not a live instruction. Revisit that
# exclusion if plans start carrying actionable issue pointers.
echo "── Check 1: ISSUES referenced outside sdd/ISSUES.md ──"
# This script is excluded from its own scan: its header and `--self-test` corpus
# quote the very forms it hunts for, exactly as a linter's own fixtures do.
#
# The PowerShell twin is excluded for the same reason, and the two exclusions must stay
# paired. POLICY requires both scripts to share one pattern and one corpus
# string-for-string, so the .ps1 necessarily quotes the same forms — which means
# whichever script scans the other reports its fixtures as violations. Excluding only
# itself made this gate fail on the mere PRESENCE of its own port.
#
# THE INPUT IS THE CONTRACT SET, NARROWED — not a walk of its own. This used to be
# `grep -r src/ tests/ sdd/ scripts/` here and a matching four-element literal in the
# PowerShell twin: two hard-coded lists, unbounded, disagreeing with check 6 in the same
# run (ScrAP-207). Drawing from the set also widens the rule to `packaging/`, `gtktest/`,
# `data/` and the root `file` entries, where a pointer at an ephemeral issue ID dangles
# exactly as it does anywhere else, and narrows it by the contract's `exclude` prefixes,
# so the verdict stops depending on which generated trees the developer happens to have.
scan1=$(scan_subset_not '^(sdd/ISSUES\.md$|sdd/PLAN\.|scripts/lint-references\.(sh|ps1)$)')
hits=""
if [ -n "$scan1" ]; then
    hits=$(grep -HnE "$ISSUES_RX" $scan1 2>/dev/null || true)
fi
if [ -n "$hits" ]; then
    echo "  FAIL — replace with an ANTI-PATTERNS citation or a self-contained comment:"
    echo "$hits" | sed 's/^/    /'
    fail=1
else
    echo "  PASS"
fi

# ── Check 2: every ScrAP-N cited in src/ exists in ANTI-PATTERNS.md ──────────────
#
# Anti-patterns are permanent and ARE the correct citation target, so these
# references are legitimate — but a number that names nothing is still a dead end
# for the reader, and the failure is silent.
#
# Expect this to fire during a cross-branch transfer: an entry authored on a port
# branch is cited by code that reaches master first. That is a real finding, not
# noise — it means the citing code and the cited entry need to land together, or in
# that order.
echo "── Check 2: ScrAP-N cited in src/ but absent from sdd/ANTI-PATTERNS.md ──"
# `[[:space:]]+`, not a literal single space: the PowerShell twin matches `^##\s+(\d+)\.`
# and would see a heading written with two spaces that this half silently would not —
# the lenient/strict split POLICY step 9 forbids. POSIX classes rather than `\s`, which
# is a GNU extension BSD/macOS grep does not honour.
# `|| true` on every extraction below. Under `set -euo pipefail` a no-match `grep`
# exits 1, `pipefail` propagates it out of the pipeline, and the assignment kills the
# script SILENTLY — it prints the check's header and nothing else, exits 1, and reads
# exactly like a failing check while having run none of it. An empty result is a
# legitimate state for every one of these (a register with no entries, a tree with no
# citations, a renamed enum arm), and the code below already handles it; the shell must
# be allowed to reach that code. Verified by mutation, not assumed: making the
# `Icon::App` arm unreadable used to abort check 7 before its own diagnostic could run.
defined=$(grep -oE '^##[[:space:]]+[0-9]+\.' sdd/ANTI-PATTERNS.md | grep -oE '[0-9]+' | sort -u || true)
# The contract set narrowed to `src/`, not a `grep -r src/` of its own — same reason as
# check 1 above. This was the fourth enumeration in the file.
src_files=$(scan_subset '^src/')
cited=""
if [ -n "$src_files" ]; then
    cited=$(grep -hoE 'ScrAP-[0-9]+' $src_files | grep -oE '[0-9]+' | sort -u || true)
fi
missing=$(comm -13 <(echo "$defined") <(echo "$cited"))
if [ -n "$missing" ]; then
    for n in $missing; do
        echo "  FAIL — ScrAP-$n is cited but not defined:"
        grep -Hn "ScrAP-$n" $src_files | sed 's/^/    /'
    done
    fail=1
else
    echo "  PASS"
fi

# ── Check 3: every ScrAP-N cited INSIDE the register has a body in the register ──
#
# Check 2 scans `src/` against the register. It cannot see a register entry citing
# another entry that is not there — and register-to-register is exactly where a
# cross-branch transfer breaks, because a lesson authored on a port branch may cite a
# sibling that stays on it. A transfer is also precisely when nobody is looking.
#
# Found in practice twice: the macOS port caught one by hand while auditing its own
# handover package (an entry citing a branch-local sibling, which would have dangled in
# shipped documentation the moment it landed), and this check caught one of ours on its
# first run.
#
# Deliberately matches only the `ScrAP-N` form, NOT the bare `#N` in-file shorthand.
# `#N` is documented as the local shorthand, but it cannot be told apart mechanically
# from ordinary prose — measured on this file, a bare-`#N` sweep picks up `#287` and
# `#819`, which are not citations at all. A check with false positives gets disabled;
# one that is merely incomplete gets extended. Prefer `ScrAP-N` when citing here.
echo "── Check 3: ScrAP-N cited inside sdd/ANTI-PATTERNS.md but not defined there ──"
cited_in_reg=$(grep -oE 'ScrAP-[0-9]+' sdd/ANTI-PATTERNS.md | grep -oE '[0-9]+' | sort -u || true)
missing_reg=$(comm -13 <(echo "$defined") <(echo "$cited_in_reg"))
if [ -n "$missing_reg" ]; then
    for n in $missing_reg; do
        echo "  FAIL — ScrAP-$n is cited in the register but has no \`## $n.\` body:"
        grep -n "ScrAP-$n" sdd/ANTI-PATTERNS.md | sed 's/^/    /' | cut -c1-160
    done
    fail=1
else
    echo "  PASS"
fi

# ── Check 4 ────────────────────────────────────────────────────────────────────
#
# `src/gtk_suite.rs` is a SECOND CRATE ROOT (a `harness = false` target that runs the
# GTK bodies on the process main thread). It has to
# re-declare `src/lib.rs`'s module list, and that duplication fails SILENTLY: a new
# top-level module added to `lib.rs` and not to the suite root simply drops every
# `#[gtktest::test]` body inside it from the suite. Nothing errors, no test fails, and
# the only visible symptom is a case count nobody has memorised — on the platform
# where the suite is the ONLY way those bodies run at all.
#
# So the duplication is guarded rather than trusted: compare the two lists as sets,
# and fail either way round. Only plain `mod x;` lines are compared — a `#[cfg]` on
# the preceding line is ignored, which is what makes `suite_registry` (cfg-gated in
# the lib, unconditional in the suite root) and the `#[cfg(unix)]`/`#[cfg(windows)]`
# modules compare equal rather than reading as drift.
#
# Mutation-tested when written, per the register's rule about vacuous guards: deleting
# `mod copymap;` from the suite root makes this print the module and exit 1.
echo "── Check 4: module list drift between src/lib.rs and src/gtk_suite.rs ──"
# `[[:space:]]*` rather than `\s*`: `\s` is a GNU grep extension, absent from the BSD
# grep macOS ships, where it would match a literal `s` and drop every indented `mod`.
# The rest of this file already uses the POSIX class; this line had been the exception.
mods_of() { grep -oE '^[[:space:]]*(pub\(crate\) )?mod [a-z_0-9]+;' "$1" | grep -oE '[a-z_0-9]+;' | tr -d ';' | sort -u || true; }
lib_mods=$(mods_of src/lib.rs)
suite_mods=$(mods_of src/gtk_suite.rs)
only_lib=$(comm -23 <(echo "$lib_mods") <(echo "$suite_mods"))
only_suite=$(comm -13 <(echo "$lib_mods") <(echo "$suite_mods"))
if [ -n "$only_lib" ]; then
    echo "  FAIL — declared in src/lib.rs but MISSING from src/gtk_suite.rs:"
    echo "$only_lib" | sed 's/^/    /'
    echo "    Every #[gtktest::test] in these modules is silently absent from the suite."
    fail=1
fi
if [ -n "$only_suite" ]; then
    echo "  FAIL — declared in src/gtk_suite.rs but not in src/lib.rs:"
    echo "$only_suite" | sed 's/^/    /'
    fail=1
fi
if [ -z "$only_lib" ] && [ -z "$only_suite" ]; then
    echo "  PASS"
fi

# ── Check 5 ────────────────────────────────────────────────────────────────────
#
# `#[gtk::test]` must not come back. It still exists and still works — it is simply
# the wrong choice here, and choosing it is INVISIBLE: the test passes on Linux, so
# nothing fails, while the body is absent from `src/gtk_suite.rs`'s main-thread run and
# therefore from the only run available on a platform where GTK must initialise on the
# main thread. That is the "works on the machine that wrote it" failure this whole
# suite exists to remove, and it would be reintroduced one convenient attribute at a
# time.
#
# The register's enforcement ladder is convention -> lint -> compiler. A clippy
# `disallowed-methods` ban cannot reach an attribute macro, and the compiler cannot be
# made to care, so a lint is the strongest rung available: grep for the attribute and
# fail. Write `#[gtktest::test]` instead — it takes no arguments, needs no change to
# the body, and registers with BOTH harnesses (see gtktest/src/lib.rs).
echo "── Check 5: #[gtk::test] used instead of #[gtktest::test] ──"
# `.rs` under `src/`, taken from the contract set. Deliberately NOT widened to every
# `.rs` in the set: `tests/` and `gtktest/` name the banned attribute in doc comments
# and prose by necessity (gtktest's whole purpose is to replace it), and a check with
# false positives gets disabled while an incomplete one gets extended — check 3's rule.
rs_files=$(scan_subset '^src/.*\.rs$')
legacy=""
if [ -n "$rs_files" ]; then
    legacy=$(grep -Hn '^[[:space:]]*#\[gtk::test\][[:space:]]*$' $rs_files || true)
fi
if [ -n "$legacy" ]; then
    echo "  FAIL — these bodies would be absent from the main-thread suite:"
    echo "$legacy" | sed 's/^/    /'
    echo "    Replace with #[gtktest::test] (drop-in; no other change needed)."
    fail=1
else
    echo "  PASS"
fi

# ── Check 5b ───────────────────────────────────────────────────────────────────
#
# RATIFIED BY THE OPERATOR (2026-07-31). Added by an agent during QA round 4 and
# flagged unratified on the reasoning in ScrAP-222 -- a check IS a rule, it just
# does not look like one, so it needs the same authority a POLICY line does. The
# operator has since adopted it. Kept as a note rather than deleted because the
# provenance is the point: this constraint now stands on their authority, not on
# the authorship of whoever wrote the grep.
# The same attribute, PRESCRIBED IN PROSE. Check 5 above greps `src/*.rs`, so it can
# only ever see a developer who has already written the wrong thing — it cannot see
# the DOCUMENT that told them to. QA round 4 found `sdd/POLICY.md` instructing
# `#[gtk::test]` in its testing section while check 5 rejected that exact attribute:
# two artifacts, each internally correct, enforcing opposite things, and neither able
# to detect the other. A reader obeying the written rule was punished by the
# automated one (ScrAP-222).
#
# A sweep then found a SECOND stale prescription the reviewer had not seen, because
# only the first sat near text that check 5's output made anyone look at. That is the
# argument for a gate rather than a one-line correction.
#
# THE DISCRIMINATOR, and the reason this is not a blanket grep. These documents MUST
# be able to name the banned attribute — the paragraph explaining why it is banned
# says it three times, and a gate that failed on that would be ScrAP-206 inverted:
# unable to tell a MENTION from a USE, and so guaranteed to be disabled. A legitimate
# mention always contrasts the two attributes and therefore sits within a line or two
# of `gtktest`; a stale PRESCRIPTION stands alone. So: flag `#[gtk::test]` only where
# no `gtktest` appears in a 2-line window around it. Mutation-tested both ways —
# re-inserting either corrected prescription FAILS, and deleting `gtktest` from the
# macOS section's contrast sentences FAILS (as it should: that sentence would then be
# prescribing the ban with nothing to redirect to).
# THE INPUT SET IS HALF THIS GATE (ScrAP-207): only documents that INSTRUCT are
# scanned. `sdd/ANTI-PATTERNS.md` and `docs/` are excluded on purpose and not by
# oversight — the register DESCRIBES a past state ("every #[gtk::test] aborts on
# macOS" is its entry title) and the review reports QUOTE the defect verbatim, which
# is the whole point of a report. Scanning them made this check fail on 13 lines that
# were all correct, and a gate that cries wolf on correct text is a gate someone
# deletes. Add a document here only if a reader could reasonably ACT on its wording.
# The set comes from the shared contract (`prescriptive` class), NOT a literal here:
# a list hard-coded in this script and again in the PowerShell twin is two lists, and
# the reason lint-references.scan exists is that two such lists had already drifted.

# THE UNIT IS THE PARAGRAPH, NOT THE LINE. A first attempt flagged any line naming the
# attribute with no `gtktest` within two lines, and it failed on three CORRECT lines —
# the sentences explaining WHY the attribute is banned sit five to nine lines below the
# one that names the replacement. Widening the window is a fudge; the honest unit is
# the blank-line-delimited block, because "names the ban without ever naming the
# replacement" is precisely what distinguishes an INSTRUCTION from an EXPLANATION, and
# it does not depend on how the author happened to wrap their prose.
echo "── Check 5b: #[gtk::test] PRESCRIBED in prose without naming the replacement ──"
# TWO STATE RESETS AT THE FILE BOUNDARY, both load-bearing, both found by the Windows
# seat running the port this one was assumed to be safer than (ScrAP-226):
#
#   * `hitfile` is captured WHEN THE HIT IS FOUND, not read from `FILENAME` at flush
#     time. `awk` evaluates `FILENAME` when the action runs, so a paragraph still
#     pending as awk crossed into the next file was printed with the NEXT file's name
#     and the previous file's line number. MEASURED: a mutant planted in POLICY.md was
#     reported as `sdd/TDD.md`, one in TDD.md as `AGENTS.md`, one in AGENTS.md as
#     `README.md` — always the next entry in the list. Only README.md read correctly,
#     because it is last.
#   * `FNR == 1 { flush() }` ends the previous file's trailing paragraph. Without it the
#     pending `hit`/`repl` carried across, so the NEXT file's first paragraph could set
#     `repl=1` and silently swallow a real prescription in the PREVIOUS one — a FALSE
#     NEGATIVE, which is worse than the misnaming.
#
# The trigger for both is a file whose last paragraph is not blank-terminated, which is
# every file in this set. `END` alone flushes once, after ALL files.
prose=$(awk "$PROSE_AWK" $SCAN_PRESCRIPTIVE 2>/dev/null)
if [ -n "$prose" ]; then
    echo "  FAIL — prose prescribing the attribute check 5 rejects:"
    echo "$prose"
    echo "    A reader who obeys this breaks the build. Name #[gtktest::test] in the"
    echo "    same paragraph, so the passage reads as a contrast rather than an"
    echo "    instruction."
    fail=1
else
    echo "  PASS"
fi

# ── Check 6 ────────────────────────────────────────────────────────────────────
#
# A pointer at a document that no longer exists. This is the same failure class as
# check 1, arriving from the opposite direction: check 1 stops a reference to an
# entry that is SUPPOSED to disappear, while this one catches a reference left behind
# when a whole FILE disappeared. Plan retirement is when it happens — a PLAN.*.md is
# deleted by design once implemented, and every "see sdd/PLAN.x.md" written into the
# tree while it existed dangles at once, scattered across code comments, Cargo.toml
# and these scripts.
#
# It found four such references in the commit that retired one plan (whose message
# claimed the sweep was complete) plus a fifth left over from an earlier retirement,
# in `scripts/gen-splash.sh`. Nothing else can see them: they are prose to the
# compiler, and the human doing the sweep greps the file types they happen to think
# of.
#
# TWO FORMS, deliberately, because they dangle differently:
#
#   6a — a directory-qualified path anywhere in the tree (`sdd/PLAN.x.md`), OR a bare
#        `PLAN.<topic>` citation with or WITHOUT the `.md` extension, which needs no
#        directory to be unambiguous: plan files live in `sdd/` by convention and
#        nowhere else, and the bare form is how code comments overwhelmingly cite
#        them. Resolved from `sdd/` and from the repo root. Plan files are the ones
#        that get DELETED, so this half carries most of the check's value — requiring
#        a directory prefix would have missed 48 live danglers across five
#        already-retired plans.
#
#        THE EXTENSION IS OPTIONAL BECAUSE THAT IS HOW COMMENTS ACTUALLY CITE A PLAN.
#        Requiring `.md` was a blind spot that let 21 danglers survive the sweep that
#        believed itself complete: code cites a SECTION of a plan, and a section
#        citation names the plan, not its filename — `PLAN.help D3`,
#        `PLAN.annotations-viewer root cause`, `PLAN.copy-path D1/D3`. Every one of
#        those resolves to a file, so every one of them can dangle, and none of them
#        carries `.md`. The `.md` alternative is listed FIRST so a full filename
#        matches whole under both POSIX leftmost-longest and .NET leftmost-first.
#
#        `PLAN.md` alone is NOT a plan citation and is skipped: SDD names plans
#        `PLAN.<topic>.md`, so stripping the extension must leave a topic behind.
#        Without that rule `src/links.rs`'s `./sub/PLAN.md#caf%C3%A9` link-parser
#        fixture reads as a dangling plan — a false positive in the one class of file
#        guaranteed to contain link-shaped strings that point nowhere.
#   6b — a Markdown link target in a `.md` file (`[text](PLAN.x.md)`). Resolved
#        relative to the linking file, since that is how a reader's viewer resolves
#        it. SDD principle 8 asks for cross-references as clickable links, so this is
#        the form the SDD documents themselves use, and a bare filename with no
#        directory only makes sense under this rule.
#
# WHAT IT DELIBERATELY DOES NOT MATCH — a bare document name in prose ("see POLICY.md
# for the rule"), which is a mention, not a path: it resolves against nothing, and
# `src/` alone carries ~100 of them. Matching those would flood the check with
# non-findings and it would be turned off within a week — check 3's rule, that a
# check with false positives gets disabled while an incomplete one gets extended.
# Two narrower exclusions serve the same end: `tests/fixtures/` (link corpora that
# are broken ON PURPOSE, including `no-such-file.md`) and inline code spans (a
# backticked `[architecture](TECH.md)` in README.md is an example OF link syntax,
# not a link — it was this check's only false positive, and stripping code spans
# before matching is what removed it).
#
# Mutation-tested when written: pointing any reference at a non-existent file makes
# this print that reference and exit 1, in both forms.
echo "── Check 6: referenced document paths that do not exist ──"
missing_paths=""
# 6a. Directory-qualified paths. `tests/reports/` is excluded: those are generated
# manual-test artifacts, gitignored by design, so they are absent on a fresh clone.
# This script and its PowerShell twin are excluded for check 1's reason, and the two
# exclusions stay paired: the header above uses `sdd/PLAN.x.md` as its worked example
# of a dangling pointer, which is a fixture, not a reference.
scan6a=$(scan_subset_not '^scripts/lint-references\.(sh|ps1)$')
hits6a=""
if [ -n "$scan6a" ]; then
    # The remaining `tests/reports/` filter is TARGET-side: the contract already keeps
    # those generated artifacts out of the scanned files, but a live document may still
    # point AT one, and it is absent on a fresh clone.
    hits6a=$(grep -HnoE "$PATH_RX" $scan6a 2>/dev/null \
               | grep -v 'tests/reports/' | LC_ALL=C sort -u || true)
fi
for hit in $hits6a; do
    path=${hit##*:}
    # A bare `PLAN.<topic>` citation names a document, so give it the extension the
    # document has. `PLAN.md` strips to an empty topic and is not a plan citation.
    case "$path" in
        PLAN.*.md) ;;
        PLAN.md)   continue ;;
        PLAN.*)    path="$path.md" ;;
    esac
    [ -e "$path" ] || [ -e "sdd/$path" ] || missing_paths+="  ${hit%:*} -> $path"$'\n'
done
# 6b. Markdown link targets, resolved relative to the linking file.
#
# The `.md` members of the contract set. Enumerated from the FILESYSTEM, not
# `git ls-files`: a tracked-only list silently skips a document that has been written
# but not yet added, which is exactly when its links are least likely to have been
# checked. It also has to match the PowerShell twin — measured: a probe file caught by
# the twin was invisible here while this used `git ls-files`, so the two gates disagreed
# on the same tree.
#
# The set used to end in a bare `.` root, which quietly made this half repo-wide —
# sweeping `docs/` (QA review artifacts) and `.agents/skills` (installed third-party
# skill documents whose links this project does not own), both gitignored, so the
# verdict depended on what the developer had run locally rather than on the commit.
# `tests/fixtures/` stays an inline exclusion for 6b ONLY: those corpora are broken ON
# PURPOSE, and 6a still scans them for citations.
md_files=$(scan_subset '\.md$' | grep -Ev '^tests/fixtures/' || true)
for f in $md_files; do
    dir=$(dirname "$f")
    while IFS= read -r target; do
        [ -z "$target" ] && continue
        # Same target-side exclusion as 6a: a link to a generated report artifact is
        # absent on a fresh clone, so excluding only the containing FILE would make
        # this half machine-dependent.
        case "$target" in *:*|*'~'*|tests/reports/*) continue ;; esac
        [ -e "$dir/$target" ] || [ -e "$target" ] || missing_paths+="  $f -> $target"$'\n'
    done < <(sed 's/`[^`]*`//g' "$f" \
             | grep -oE '\]\([^)# ]+\.md(#[^) ]*)?\)' \
             | sed -E 's/^\]\(//; s/\)$//; s/#.*$//')
done
if [ -n "$missing_paths" ]; then
    echo "  FAIL — these point at files that are not there:"
    printf '%s' "$missing_paths"
    echo "    Delete the pointer, or replace it with the fact it was carrying."
    fail=1
else
    echo "  PASS"
fi

# ── Check 7 ────────────────────────────────────────────────────────────────────
#
# The application ID is ONE string with several jobs — GApplication id, icon name,
# desktop-entry `Icon=`/`StartupWMClass`, GResource path, macOS `CFBundleIdentifier`,
# the install/uninstall scripts' install target. `src/icons.rs` is its source of
# truth, and Rust code derives it from there (`icons::APP_ID`), so the Rust side
# cannot drift. The NON-Rust side is plain restated text, and nothing checked it.
#
# The failure is silent and platform-shaped, which is why it is worth a gate: change
# the ID in `icons.rs` and the app keeps building and running everywhere, while the
# installed desktop entry stops matching the window (no taskbar icon on Linux), the
# GResource path stops resolving (the About logo becomes a broken-image placeholder),
# and macOS Launch Services keeps registering the OLD identifier. Each surface is
# checked by a different person on a different platform, or by nobody.
#
# Deliberately NOT scanning all files for the literal: `README.md`, the SVG, MANUAL-TEST
# and the port READMEs all mention it in prose, where a mention is not a declaration.
# This lists the files that OPERATIONALLY declare it — the ones a wrong value breaks.
echo "── Check 7: app-ID drift between src/icons.rs and the packaging files ──"
APP_ID_FILES="install.sh uninstall.sh data/scribobulate.desktop data/resources.gresource.xml packaging/macos/Info.plist.in"
canonical_app_id=$(grep -oE 'Icon::App => "[^"]+"' src/icons.rs | sed -E 's/.*"(.*)"/\1/' || true)
app_id_problems=""
if [ -z "$canonical_app_id" ]; then
    app_id_problems="  could not read the canonical app ID from src/icons.rs (Icon::App arm)"$'\n'
else
    for f in $APP_ID_FILES; do
        [ -e "$f" ] || { app_id_problems+="  $f is missing"$'\n'; continue; }
        grep -qF "$canonical_app_id" "$f" \
            || app_id_problems+="  $f does not carry the app ID '$canonical_app_id'"$'\n'
        # A DIFFERENT reverse-DNS id in the same file is drift that the presence test
        # above cannot see — both can be true at once while one surface is stale.
        while IFS= read -r found; do
            [ -z "$found" ] && continue
            [ "$found" = "$canonical_app_id" ] || \
                app_id_problems+="  $f carries a foreign app ID '$found' (canonical: '$canonical_app_id')"$'\n'
        done < <(grep -ohE '\b[a-z0-9]+\.[a-z0-9]+\.scribobulate\b' "$f" | sort -u)
    done
fi
if [ -n "$app_id_problems" ]; then
    echo "  FAIL — the app ID is declared in more than one place and they disagree:"
    printf '%s' "$app_id_problems"
    echo "    src/icons.rs's Icon::App arm is the source of truth; update the others."
    fail=1
else
    echo "  PASS"
fi

# ── Check 8 ────────────────────────────────────────────────────────────────────
#
# A BARE `AP-N` is illegal anywhere in this tree. Not "means the skill" — illegal.
#
# WHY A FORM AND NOT A TARGET. Two registers are in play with unrelated numbering:
# `sdd/ANTI-PATTERNS.md` (`ScrAP-N`) and the `gtk4-rs` skill (`GTK4Rs/AP-N`). A QA sample
# put 83 of 107 entries at different numbers between them, and the collision is not
# incidental — this register's #79 is the container-gesture-on-a-child-button lesson
# while the skill's AP-79 is the wall-clock-bounded pump loop, and its #88 and the
# skill's AP-88 are the same pair INVERTED. So the two registers hold each other's
# lesson at those numbers, and a bare `AP-79` is a coin toss between them.
#
# The bare form is the one that cannot be told apart from a missed sweep: it is
# simultaneously the current skill spelling and the historical project spelling, so the
# same string is correct or wrong depending on the day it was typed. A convention whose
# correct and incorrect uses are textually identical is not a convention.
#
# WHAT THIS DOES NOT DO, read before trusting a PASS — the same warning check 2 carries.
# It cannot confirm a `GTK4Rs/AP-N` names the right skill entry: the skill may not be
# installed (the register refuses to name a filesystem path for exactly that reason), so
# nothing on this machine can resolve it. That looks like a hole and is the reason the
# scheme is right. Making the bare form illegal does not make skill citations
# VERIFIABLE, it makes them ENUMERABLE — before, the unverifiable set was unbounded and
# textually indistinguishable from the verifiable one; after, it is exactly the
# `GTK4Rs/AP-N` citations, a greppable list a human can audit deliberately. Separating
# "checked" from "must be checked by a human" is strictly better than a scheme in which
# both look alike (ScrAP-217's control-arm principle, one layer up).
#
# AND IT CANNOT SEE THE OTHER HALF OF THE EXPOSURE. An already-prefixed `ScrAP-N`
# pointing at a real entry about the wrong subject is invisible here and to every other
# check — `src/widgets/tab/bar.rs` carried exactly that, a `ScrAP-79` on a helper whose
# comment described the skill's AP-79 pump-loop lesson (this register's #88), sitting
# among three CORRECT `ScrAP-79` citations about the tab-close gesture. It got there by
# the FIX, not the original: the sweep that introduced the prefix rewrote the prefix
# without re-resolving the number. That audit is human-only and has no completion
# criterion; this check is the half that has one.
#
# Mutation-tested when written, per the register's rule about vacuous guards: a clean
# tree passes this check whether or not the pattern can match, so it was proven by
# PLANTING a bare `AP-123` in a scanned file and confirming it was named.
echo "── Check 8: bare AP-N citations (must be ScrAP-N or GTK4Rs/AP-N) ──"
# The two lint scripts are excluded, and the exclusions stay PAIRED for check 1's
# reason: both quote bare citations in the `--self-test` corpus above, exactly as a
# linter's own fixtures do, so whichever script scanned the other would report its
# fixtures as violations.
scan8=$(scan_subset_not '^scripts/lint-references\.(sh|ps1)$')
cand8=""
if [ -n "$scan8" ]; then
    cand8=$(grep -HnE "$BARE_AP_RX" $scan8 2>/dev/null || true)
fi
bare8=""
bare8_count=0
while IFS= read -r cline; do
    [ -z "$cline" ] && continue
    if is_bare_ap "$cline"; then
        # Report the CITATIONS, not the line. Register entries and MANUAL-TEST items run
        # to well over a thousand characters, and a migration this size is read as a
        # worklist — `path:line — AP-79 AP-88` is actionable where a truncated wall of
        # prose is not.
        toks=$(sed -E "s|$LEGAL_AP_RX||g" <<<"$cline" \
                 | grep -oE "$BARE_AP_RX" | grep -oE 'AP-[0-9]+' | sort -u | paste -sd' ' -)
        bare8_count=$((bare8_count + $(wc -w <<<"$toks")))
        bare8+="  ${cline%%:*}:$(echo "$cline" | cut -d: -f2) — $toks"$'\n'
    fi
done <<<"$cand8"
if [ -n "$bare8" ]; then
    # FAILS — and it landed in WARN mode first, deliberately, for exactly one run. 443
    # sites could not be re-resolved in one pass, and a wrong bulk decision is WORSE than
    # the ambiguity it replaces, because it converts "unclear" into "confidently wrong" —
    # which is precisely what happened to bar.rs. So the gate went in with the count
    # visible, migration ran per directory (smallest first), and the flip to `fail=1`
    # landed with the last of it. The tree is at zero now, so any hit here is NEW: resolve
    # it at the keystroke rather than adding it to a backlog that no longer exists.
    echo "  FAIL — $bare8_count bare citation(s) on $(grep -c . <<<"$bare8") line(s):"
    printf '%s' "$bare8"
    fail=1
    echo "    Resolve each PER SITE: read the surrounding comment, decide which lesson it"
    echo "    describes, then confirm that lesson's number IN THE REGISTER NAMED — the"
    echo "    number must be RE-DERIVED, never carried across. If the lesson is in both"
    echo "    registers, prefer ScrAP-N: this one is always resolvable, the skill may be"
    echo "    absent. NEVER bulk-rewrite the prefix; that is the mechanism that produced"
    echo "    the one confirmed defect."
else
    echo "  PASS"
fi

exit $fail
