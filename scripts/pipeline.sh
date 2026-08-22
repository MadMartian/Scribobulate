#!/usr/bin/env bash
#
# Linux build pipeline runner.
#
# It DERIVES every step from scripts/pipeline.steps rather than restating them. That is
# the whole design: a runner that restated the list would let `--list-steps` prove only
# that two restatements match, which two people copying the same wrong list also achieve
# (ScrAP-207, one level down). The contract's header carries the full rationale.
#
# The load-bearing structural property, and the reason this file has no second step list:
# `derived_step_ids` is the ONLY producer of the ordered step list, and BOTH `--list-steps`
# and the run loop consume it. Conformance is therefore not something a self-test asserts
# after the fact — there is no second list that could disagree. `--self-test` exists to
# prove that property still holds after an edit, not to create it.
#
# The parsing, validation and execution machinery lives in scripts/pipeline-lib.sh, shared
# with the macOS runner; this file holds only what is Linux's. That library's header states
# what must not move into it, and why the PowerShell port is not a candidate.
#
# Usage:
#   scripts/pipeline.sh                 # run the pipeline
#   scripts/pipeline.sh --list-steps    # print the derived step list (diff vs other ports)
#   scripts/pipeline.sh --self-test     # validate the contract and this runner's derivation
#   scripts/pipeline.sh --skip-integration
#
set -euo pipefail

PLATFORM="linux"
CONTRACT="scripts/pipeline.steps"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
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

# --------------------------------------------------------------------------------------
# Entry point
# --------------------------------------------------------------------------------------
SKIP_INTEGRATION=0
DO_PACKAGE=0
while [ $# -gt 0 ]; do
    case "$1" in
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
            echo "pipeline: unknown argument '$1'" >&2
            echo "usage: $0 [--list-steps|--self-test|--skip-integration] [--package]" >&2
            exit 2
            ;;
    esac
    shift
done

validate_contract || exit 2

run_setup_phase

# Required steps an OPERATOR OVERRIDE prevented from running. Accumulated so a later
# packaging step can name them: an installer's provenance is part of the installer.
OVERRIDDEN_STEPS=""

rc=0
run_all_steps || rc=$?
exit "$rc"
