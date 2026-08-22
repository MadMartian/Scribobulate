#!/usr/bin/env bash
#
# macOS build pipeline runner.
#
# It DERIVES every step from scripts/pipeline.steps rather than restating them — the same
# design as scripts/pipeline.sh (Linux). `derived_step_ids` is the ONLY producer of the
# ordered step list, and BOTH `--list-steps` and the run loop consume it, so there is no
# second list that could disagree. `--self-test` proves that property still holds after an
# edit; it does not create it.
#
# This file used to MIRROR the Linux runner rather than share with it — about three hundred
# duplicated lines, on the stated reasoning that three thin per-platform ports reading one
# contract was the same shape as lint-references.sh/.ps1. That reasoning held for the
# PowerShell port, which genuinely cannot share, and did not hold here: the two bash copies
# had already drifted three times, each drift making one platform quietly the lenient one.
# The machinery now lives in scripts/pipeline-lib.sh, whose header states what must not move
# into it and why Windows is still compared by `--list-steps` output rather than by sharing.
#
# This runner's per-platform command bodies (cmd.macos.* in the contract) are what differ
# from Linux, not the parsing/execution logic:
#   - Step 5 runs `--test gtk_suite` plus three standalone targets, never `--lib` —
#     the dual-harness bodies abort the process off the main thread on Quartz
#     (ScrAP-171; measured GTK 4.22.4). That difference lives in the contract, not here.
#   - Step 6 (coverage) is declared `na.macos permanent` in the contract: this
#     runner does not special-case it, it just prints whatever the contract says.
#
# Usage:
#   packaging/macos/pipeline.sh                 # run the pipeline
#   packaging/macos/pipeline.sh --list-steps    # print the derived step list (diff vs other ports)
#   packaging/macos/pipeline.sh --self-test     # validate the contract and this runner's derivation
#   packaging/macos/pipeline.sh --skip-integration
#
set -euo pipefail

PLATFORM="macos"
CONTRACT="scripts/pipeline.steps"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

if [ ! -f "$CONTRACT" ]; then
    echo "pipeline: $CONTRACT not found (run from anywhere in the repo)" >&2
    exit 2
fi

LIB="scripts/pipeline-lib.sh"
if [ ! -f "$LIB" ]; then
    echo "pipeline: $LIB not found; this runner is a platform shell around it" >&2
    exit 2
fi
# shellcheck source=scripts/pipeline-lib.sh
. "$LIB"

# The one platform-specific property worth asserting here: the Darwin guard must not gate
# the two contract-only modes, or the artefact whose entire purpose is cross-port diffing
# could only be produced on the platform it describes. Asserted rather than trusted to the
# ordering of the file, because "moved a guard three lines up" is a plausible tidy-up and
# nothing else would notice it.
platform_self_test() {
    # WHAT THIS ASSERTS, AND WHY IT IS A SOURCE CHECK.
    #
    # The property is that the Darwin guard sits AFTER the argument-parsing loop, so
    # `--list-steps` and `--self-test` stay runnable off-platform — they read only the
    # contract, and the artefact whose whole purpose is cross-port diffing must not be
    # producible solely on the platform it describes.
    #
    # This function used to be an if/else printing one of two messages and then
    # `return 0`, under a comment claiming the placement was "asserted rather than
    # trusted to the ordering of the file". Nothing was asserted. It could not go red.
    #
    # The behavioural form of the check is UNAVAILABLE HERE, and that is the whole
    # difficulty: hoist the guard above the loop and, on Darwin, the guard passes,
    # `self_test` runs, this function prints and returns 0 — green. Only a NON-Darwin
    # host observes the regression, and no such host runs this file as its gate. So the
    # seat that owns this runner cannot detect the regression its own comment exists to
    # prevent.
    #
    # The ordering is a fact about the SOURCE, which is platform-independent and therefore
    # checkable exactly where the behaviour is not. Source-level assertion is an
    # established tier in this project rather than an exception — `lint-references` check 5
    # rejects a banned attribute by reading source, and check 3 pins `gtk_suite.rs`'s module
    # list against `lib.rs`. It also stays on the correct side of the library boundary:
    # `scripts/pipeline-lib.sh` says the guard's placement is a property of the runner and
    # not of the library, so a runner-local assertion about its own placement belongs here
    # and nothing moves into the shared file.
    local src="${BASH_SOURCE[0]}" loop_start loop_end guard

    # PATTERNS ARE ANCHORED AT COLUMN 0 BECAUSE THIS FILE CONTAINS THEM.
    # A check that greps its own source matches its own search string: `grep -n 'macOS only'`
    # found the line of THIS function that contains the literal `macOS only`, reported the
    # guard as line 90, and failed the ordering test against a file whose guard was in the
    # right place. Same family as everything else audited today — the instrument answering
    # about itself rather than its subject — and it fails toward red here only by luck of
    # which line came first. The guard and the loop are the only statements in this file
    # beginning at column 0 with these shapes; every mention of them inside a function is
    # indented, so the anchors discriminate the subject from the search for it.
    loop_start=$(grep -n '^for arg in' "$src" | head -1 | cut -d: -f1)
    loop_end=$(awk -v s="${loop_start:-0}" 'NR > s && $0 == "done" { print NR; exit }' "$src")
    guard=$(grep -n '^\[\[ "$(uname)"' "$src" | head -1 | cut -d: -f1)

    # A check that cannot find its subject must FAIL, never quietly succeed. Refactor this
    # file's argument loop or guard into a shape these patterns miss and the answer is
    # "inconclusive", which is a red — the alternative is a check that silently stops
    # checking, which is the defect this function is being repaired for.
    if [ -z "$loop_start" ] || [ -z "$loop_end" ] || [ -z "$guard" ]; then
        echo "pipeline: platform self-test INCONCLUSIVE — could not locate the argument" >&2
        echo "  loop (start=${loop_start:-?}, end=${loop_end:-?}) or the Darwin guard" >&2
        echo "  (${guard:-?}) in $src. Not reporting success for a check that found no" >&2
        echo "  subject." >&2
        return 1
    fi

    if [ "$guard" -le "$loop_end" ]; then
        echo "pipeline: the Darwin guard is at line $guard, which is NOT after the" >&2
        echo "  argument-parsing loop (ends line $loop_end). In that position it gates" >&2
        echo "  --list-steps and --self-test behind the platform they exist to be diffed" >&2
        echo "  ACROSS, so neither other port could check this one's step list." >&2
        return 1
    fi

    echo "   platform: Darwin guard at line $guard, after the argument loop (ends $loop_end)"

    if [ "$(uname)" = "Darwin" ]; then
        # Stated plainly, in the run output, for the same reason a non-applicable step is:
        # this line sits among lines that ARE checks, so silence about what it does not
        # cover reads as coverage.
        echo "   platform: NOT VERIFIED BEHAVIOURALLY on this host — a hoisted guard exits"
        echo "   before this function is reached, so the ordering above is asserted from"
        echo "   source and the running form of it is only observable off Darwin."
    else
        # Reaching this line off Darwin is itself the behavioural observation: the guard
        # did not block a contract-only mode on a non-Darwin host.
        echo "   platform: reached off Darwin, which observes the running form directly —"
        echo "   the contract-only modes were not gated by the guard."
    fi
    return 0
}

# --------------------------------------------------------------------------------------
# Entry point
# --------------------------------------------------------------------------------------
SKIP_INTEGRATION=0
DO_PACKAGE=0
for arg in "$@"; do
    case "$arg" in
        --list-steps)
            validate_contract || exit 2
            list_steps
            exit 0
            ;;
        --self-test)
            self_test || exit 1
            exit 0
            ;;
        --skip-integration) SKIP_INTEGRATION=1 ;;
        --package)          DO_PACKAGE=1 ;;
        *)
            echo "pipeline: unknown argument '$arg'" >&2
            echo "usage: $0 [--list-steps|--self-test] [--skip-integration] [--package]" >&2
            exit 2
            ;;
    esac
done

# The Darwin check belongs HERE, not at the top of the file: --list-steps and --self-test
# read only the contract — no toolchain, no GTK, no Quartz — so gating them behind this
# platform would mean the one artefact whose entire purpose is cross-port diffing could
# only be produced on the platform it describes, and neither of the other two ports could
# check this one's row set at all. Only an actual RUN needs macOS.
[[ "$(uname)" == "Darwin" ]] || { echo "pipeline: macOS only" >&2; exit 1; }

validate_contract || exit 2

run_setup_phase

# Required steps an OPERATOR OVERRIDE prevented from running. Accumulated so a later
# packaging step can name them: an installer's provenance is part of the installer.
OVERRIDDEN_STEPS=""

rc=0
run_all_steps || rc=$?
exit "$rc"
