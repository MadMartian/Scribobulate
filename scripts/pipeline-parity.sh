#!/usr/bin/env bash
#
# Cross-platform parity comparator for the artefacts the three pipeline ports derive.
#
# WHY THIS EXISTS. `scripts/pipeline.steps` is consumed by three independently written
# runners, and `scripts/lint-references.scan` by two. Each port DERIVES its list from the
# contract, and each prints that derived list (`--list-steps` / `-ListSteps`,
# `--list-scan` / `-ListScan`) so the ports can be diffed. Until now nothing performed the
# diff: no single machine has all three ports, so the comparison was an errand a human
# remembered rather than a gate. This script is the errand made mechanical; CI is what
# finally puts all three artefacts in one place (.github/workflows/pipeline.yml).
#
# IT COMPARES; THE PORTS CONFORM. Comparison proves the ports agree, derivation proves
# they conform, and only the second is worth having (ScrAP-207). This script is the
# first half only, and it is deliberately not the whole gate: each port also runs its own
# `--self-test` on its own runner, which is where conformance is established. A clean diff
# here over three restatements would prove nothing, and if the runners ever stop deriving,
# this script cannot tell.
#
# THE EXPECTED SET IS DECLARED, NEVER DISCOVERED. The failure this script must not have is
# the vacuous pass: if one platform's job dies before uploading, a comparator that diffs
# "whatever is in the directory" finds the two survivors identical and reports PARITY OK.
# That output is indistinguishable from a real three-way agreement, and it appears exactly
# when something has gone wrong — the shape of a guard whose setup prevents the condition
# it guards from arising (ScrAP-209), and of a negative result with no positive control
# (ScrAP-217). So every kind declares who must be present, a missing or empty member is a
# hard failure that names the port, and `--self-test` mutation-proves both.
#
# THE `steps` EXPECTATION IS DERIVED FROM THE CONTRACT, for the same reason the runners'
# step lists are: a fourth restatement of the platform list is a fourth thing to drift.
# Adding `platform <id>` to pipeline.steps therefore makes this gate demand that
# platform's artefact, rather than silently continuing to certify the old three.
#
# `scan` CANNOT BE DERIVED THE SAME WAY and is named below instead: lint-references.scan
# enumerates FILES, not ports, so there is nothing in it to read. The three names are a
# constant here, with the self-test's missing-member mutation as their only guard.
#
# AND `scan` HAS THREE MEMBERS FOR TWO PORTS. The contract names scripts/lint-references.sh
# for BOTH Linux and macOS, so the bash port runs on two platforms — and BSD find/sed/sort
# are not GNU's, which makes macOS a place one port can diverge from itself. That is
# ScrAP-207's shape (the platform nobody runs becomes the lenient one) on an axis sharing a
# file was assumed to close. Compared per PLATFORM, therefore, not per port.
#
# LINE ENDINGS AND BOM ARE NORMALISED, AUDIBLY. A PowerShell port emits CRLF, and a
# byte-exact diff against a POSIX port's LF would then fail forever on a difference that
# is the platform's line terminator rather than any drift in the list. Normalising is
# therefore correct — but silent normalisation is how a comparator starts hiding things,
# so a member that actually needed normalising is announced.
#
# Usage:
#   scripts/pipeline-parity.sh <artefact-dir>   # compare; artefacts named <kind>.<port>.txt
#   scripts/pipeline-parity.sh --self-test      # prove it PASSES clean and FAILS injected
#
# Exit: 0 parity holds · 1 parity is broken · 2 the comparator could not run
#
set -euo pipefail

CONTRACT="scripts/pipeline.steps"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# The kinds this gate knows how to compare. `steps` derives its expected ports from the
# contract; `scan` names them, because the scan contract enumerates files and not ports.
SCAN_PORTS="linux macos windows"

fail() { echo "parity: $*" >&2; }

# --------------------------------------------------------------------------------------
# Expected membership
# --------------------------------------------------------------------------------------
expected_ports() {
    local kind="$1"
    case "$kind" in
        steps)
            [ -f "$CONTRACT" ] || { fail "$CONTRACT not found"; return 2; }
            awk '$1 == "platform" { print $2 }' "$CONTRACT"
            ;;
        scan)
            printf '%s\n' $SCAN_PORTS
            ;;
        *)
            fail "unknown artefact kind '$kind'"
            return 2
            ;;
    esac
}

# --------------------------------------------------------------------------------------
# Normalisation — strip a UTF-8 BOM and any trailing CR, and report when it mattered.
# --------------------------------------------------------------------------------------
normalise() {
    local src="$1" dst="$2"
    sed -e '1s/^\xEF\xBB\xBF//' -e 's/\r$//' "$src" > "$dst"
    ! cmp -s "$src" "$dst"
}

# --------------------------------------------------------------------------------------
# Compare one kind across its expected ports.
# --------------------------------------------------------------------------------------
compare_kind() {
    local kind="$1" dir="$2"
    local ports port path ref_port="" ref="" work rc=0

    ports=$(expected_ports "$kind") || return 2
    work=$(mktemp -d)
    # shellcheck disable=SC2064  # expand $work now, not at trap time
    trap "rm -rf '$work'" RETURN

    echo "=== $kind"

    for port in $ports; do
        path="$dir/$kind.$port.txt"
        if [ ! -f "$path" ]; then
            fail "$kind: no artefact for '$port' ($path) — the port did not report"
            rc=1
            continue
        fi
        if [ ! -s "$path" ]; then
            fail "$kind: artefact for '$port' is empty ($path)"
            rc=1
            continue
        fi
        if normalise "$path" "$work/$port"; then
            echo "    $port — normalised (BOM and/or CRLF)"
        fi
        if [ -z "$ref_port" ]; then
            ref_port="$port"
            ref="$work/$port"
            echo "    $port — reference, $(wc -l < "$ref") lines"
            continue
        fi
        if diff -u "$ref" "$work/$port" > "$work/diff.$port" 2>&1; then
            echo "    $port — identical to $ref_port"
        else
            fail "$kind: '$port' differs from '$ref_port'"
            sed -e "s|$work/$ref_port|$ref_port|" -e "s|$work/$port|$port|" "$work/diff.$port" >&2
            rc=1
        fi
    done

    # Nothing to compare is not agreement. Reaching here with no reference means every
    # expected member was missing, which the loop already reported; guard anyway so a
    # future edit cannot turn an empty directory into a pass.
    if [ -z "$ref_port" ] && [ "$rc" -eq 0 ]; then
        fail "$kind: no artefacts present at all"
        rc=1
    fi

    return "$rc"
}

compare_all() {
    local dir="$1" kind rc=0
    [ -d "$dir" ] || { fail "artefact directory '$dir' not found"; return 2; }
    for kind in steps scan; do
        compare_kind "$kind" "$dir" || rc=1
    done
    if [ "$rc" -eq 0 ]; then
        echo
        echo "parity holds across every expected port"
    fi
    return "$rc"
}

# --------------------------------------------------------------------------------------
# --self-test
#
# The plan this script closes states the bar plainly: the verification bar for a CI gate
# is INJECTION, not a green run — a gate that reports success while something failed is
# the defect a gate exists to prevent, and this campaign has already produced one (a
# PowerShell output-stream bug reported `pipeline PASSED` with exit 0 after a step had
# failed, behind a byte-identical -ListSteps and a twelve-case mutation battery).
#
# A hosted-runner injection needs a deliberately broken push and is therefore an errand
# again. This is the half that can be checked in-repo on every run: each mutation below
# is a way the comparator could report parity over a tree that has none, and each must
# make it exit non-zero. The clean case runs first as the positive control — without it,
# "every mutation failed" is also what a comparator that fails on everything produces.
# --------------------------------------------------------------------------------------
self_test() {
    local work base errs=0
    work=$(mktemp -d)
    trap 'rm -rf "$work"' RETURN

    base=$(scripts/pipeline.sh --list-steps) || { fail "could not produce a base artefact"; return 2; }

    seed() {
        local dir="$1" port
        mkdir -p "$dir"
        for port in $(expected_ports steps); do printf '%s\n' "$base" > "$dir/steps.$port.txt"; done
        for port in $SCAN_PORTS; do printf 'AGENTS.md\nREADME.md\n' > "$dir/scan.$port.txt"; done
    }

    expect() {
        local want="$1" label="$2" dir="$3" got=0
        compare_all "$dir" > "$work/out" 2>&1 || got=$?
        if [ "$got" -eq "$want" ]; then
            echo "   PASS  $label (exit $got)"
        else
            echo "   FAIL  $label — expected exit $want, got $got" >&2
            sed 's/^/         /' "$work/out" >&2
            errs=$((errs + 1))
        fi
    }

    echo ":: self-test"

    # Positive control. Without it, a comparator that failed unconditionally would score
    # a clean sweep on every mutation below.
    seed "$work/clean"
    expect 0 "clean three-way set passes" "$work/clean"

    # A CRLF-only difference is the platform's line terminator, not drift.
    seed "$work/crlf"
    sed 's/$/\r/' "$work/crlf/steps.windows.txt" > "$work/crlf/steps.windows.crlf" \
        && mv "$work/crlf/steps.windows.crlf" "$work/crlf/steps.windows.txt"
    expect 0 "CRLF-only difference still passes" "$work/crlf"

    # THE injection: one port's list drifts.
    seed "$work/reordered"
    tac "$work/reordered/steps.macos.txt" > "$work/reordered/steps.macos.tac" \
        && mv "$work/reordered/steps.macos.tac" "$work/reordered/steps.macos.txt"
    expect 1 "a reordered step list fails" "$work/reordered"

    seed "$work/classchange"
    sed 's/required/informational/' "$work/classchange/steps.windows.txt" > "$work/classchange/steps.windows.new" \
        && mv "$work/classchange/steps.windows.new" "$work/classchange/steps.windows.txt"
    expect 1 "a changed step class fails" "$work/classchange"

    # The vacuous pass this comparator exists to refuse: a port that never reported.
    seed "$work/missing"
    rm "$work/missing/steps.windows.txt"
    expect 1 "a port that did not report fails" "$work/missing"

    seed "$work/empty"
    : > "$work/empty/steps.macos.txt"
    expect 1 "an empty artefact fails" "$work/empty"

    seed "$work/scandrift"
    printf 'AGENTS.md\n' > "$work/scandrift/scan.windows.txt"
    expect 1 "a drifted lint scan set fails" "$work/scandrift"

    # An artefact directory with nothing in it must never read as agreement.
    mkdir -p "$work/bare"
    expect 1 "an empty artefact directory fails" "$work/bare"

    if [ "$errs" -ne 0 ]; then
        fail "$errs self-test case(s) failed"
        return 1
    fi
    echo "   all self-test cases behaved as specified"
    return 0
}

# --------------------------------------------------------------------------------------
case "${1-}" in
    --self-test) self_test ;;
    -h|--help|"")
        echo "usage: $0 <artefact-dir> | --self-test" >&2
        exit 2
        ;;
    -*)
        fail "unknown option '$1'"
        exit 2
        ;;
    *) compare_all "$1" ;;
esac
