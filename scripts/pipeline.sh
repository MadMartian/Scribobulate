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

# --------------------------------------------------------------------------------------
# Contract parsing
#
# Grammar is `keyword  key  value...` with the value as the remainder of the line, so a
# value may contain spaces. Uniform arity is deliberate — see the contract header.
# --------------------------------------------------------------------------------------

# contract_value <keyword> <key> -> the remainder of the line, or empty.
contract_value() {
    awk -v kw="$1" -v key="$2" '
        $1 == kw && $2 == key {
            $1 = ""; $2 = ""
            sub(/^[[:space:]]+/, "")
            print
            exit
        }
    ' "$CONTRACT"
}

# derived_step_ids -> the ordered step ids, one per line.
#
# FILE ORDER IS THE ORDER. The `step` lines' ordinals (1, 2, ... 4b, 5 ...) are POLICY's
# display labels, not sort keys — a numeric sort cannot order "4b" without a convention
# every port would have to reimplement identically, which is exactly the class of quiet
# disagreement this contract exists to remove. The ordinals are validated for
# non-decreasing order below, so a reorder that forgets to renumber fails loudly.
derived_step_ids() {
    awk '$1 == "step" { print $3 }' "$CONTRACT"
}

step_ordinal() {
    awk -v id="$1" '$1 == "step" && $3 == id { print $2; exit }' "$CONTRACT"
}

# --------------------------------------------------------------------------------------
# Contract validation
#
# A garbled contract must fail LOUDLY, never degrade into agreeing on nothing. Two ports
# that both parse a broken file into an empty step list agree perfectly and prove nothing
# — the failure mode lint-references.sh guards with the same shape of check.
# --------------------------------------------------------------------------------------
validate_contract() {
    local errs=0

    local bad
    bad=$(grep -nvE '^[[:space:]]*(#.*)?$|^(platform|step|intent|verdict|class|setup|cmd\.[a-z]+|na\.[a-z]+|carveout\.[a-z]+)[[:space:]]+[^[:space:]]+([[:space:]]+.*)?$' \
        "$CONTRACT" || true)
    if [ -n "$bad" ]; then
        echo "pipeline: $CONTRACT has lines that are neither blank, a comment, nor a" >&2
        echo "  recognised 'keyword key value' triple:" >&2
        echo "$bad" >&2
        errs=$((errs + 1))
    fi

    local ids
    ids=$(derived_step_ids)
    if [ -z "$ids" ]; then
        echo "pipeline: $CONTRACT defines no steps. An empty step list is a garbled" >&2
        echo "  contract, not a pipeline with nothing to do." >&2
        return 1
    fi

    local dupes
    dupes=$(echo "$ids" | sort | uniq -d)
    if [ -n "$dupes" ]; then
        echo "pipeline: duplicate step id(s) in $CONTRACT: $dupes" >&2
        errs=$((errs + 1))
    fi

    local platforms
    platforms=$(awk '$1 == "platform" { print $2 }' "$CONTRACT")
    if [ -z "$platforms" ]; then
        echo "pipeline: $CONTRACT declares no platforms" >&2
        return 1
    fi

    # Every step needs intent, verdict and class; and for EVERY declared platform either
    # a command or a non-applicable declaration — unless it is a review step.
    #
    # THE CONTRACT IS VALIDATED IN FULL FROM EVERY RUNNER, not just for the platform doing
    # the running. This was found by mutation: validating only `$PLATFORM` let a garbled
    # `na.windows` line pass from Linux, so a contract defect on the platform nobody
    # happens to be running would surface only when someone finally ran it there — the
    # exact "the platform nobody runs is the lenient one" shape that ScrAP-207 records and
    # that this whole contract exists to prevent. Cross-platform validation makes that
    # class of gap fail everywhere, immediately, on whichever runner is invoked first.
    local id
    for id in $ids; do
        local intent verdict class
        intent=$(contract_value intent "$id")
        verdict=$(contract_value verdict "$id")
        class=$(contract_value class "$id")

        [ -n "$intent" ]  || { echo "pipeline: step '$id' has no intent" >&2;  errs=$((errs + 1)); }
        [ -n "$verdict" ] || { echo "pipeline: step '$id' has no verdict" >&2; errs=$((errs + 1)); }
        [ -n "$class" ]   || { echo "pipeline: step '$id' has no class" >&2;   errs=$((errs + 1)); }

        case "$class" in
            required|informational|review|packaging) ;;
            *)
                echo "pipeline: step '$id' has unknown class '$class'" >&2
                errs=$((errs + 1))
                ;;
        esac

        local plat cmd na
        for plat in $platforms; do
            cmd=$(contract_value "cmd.$plat" "$id")
            na=$(contract_value "na.$plat" "$id")

            if [ "$class" = "review" ]; then
                if [ -n "$cmd" ]; then
                    echo "pipeline: review step '$id' must not carry cmd.$plat" >&2
                    errs=$((errs + 1))
                fi
            else
                if [ -z "$cmd" ] && [ -z "$na" ]; then
                    echo "pipeline: step '$id' has neither cmd.$plat nor na.$plat." >&2
                    echo "  A step that does not apply must be DECLARED, never omitted." >&2
                    errs=$((errs + 1))
                fi
                if [ -n "$cmd" ] && [ -n "$na" ]; then
                    echo "pipeline: step '$id' has BOTH cmd.$plat and na.$plat" >&2
                    errs=$((errs + 1))
                fi
            fi

            # A non-applicable declaration must record its KIND, because the two kinds
            # are acted on differently and the tooling one is a trap (contract header).
            if [ -n "$na" ]; then
                local kind reason
                kind=$(echo "$na" | awk '{print $1}')
                case "$kind" in
                    permanent|tooling) ;;
                    *)
                        echo "pipeline: step '$id' na.$plat has kind '$kind'; expected" >&2
                        echo "  'permanent' or 'tooling'" >&2
                        errs=$((errs + 1))
                        ;;
                esac
                reason=$(echo "$na" | cut -d' ' -f2-)
                if [ -z "$reason" ] || [ "$reason" = "$kind" ]; then
                    echo "pipeline: step '$id' na.$plat has a kind but no reason" >&2
                    errs=$((errs + 1))
                fi
            fi
        done
    done

    # A carve-out is applied by appending libtest `--skip` arguments, so a step that
    # carries one must be a `cargo test` invocation. Checked rather than assumed: the
    # assumption holds right up until somebody adds a carve-out to a step where it cannot
    # mean anything, and then it is applied silently to a command that ignores it.
    # Checked for EVERY platform, since a carve-out is per-platform data.
    for id in $ids; do
        local plat cmdline
        for plat in $platforms; do
            [ -n "$(carveouts_for "$id" "$plat")" ] || continue
            cmdline=$(contract_value "cmd.$plat" "$id")
            case "$cmdline" in
                *"cargo test"*) ;;
                *)
                    echo "pipeline: step '$id' has carveout.$plat, but cmd.$plat is not a" >&2
                    echo "  'cargo test' invocation, so --skip cannot apply to it." >&2
                    errs=$((errs + 1))
                    ;;
            esac
        done
    done

    # Repo-relative scripts named in THIS platform's commands must exist.
    #
    # Only this platform's: a Linux runner cannot check that a .ps1 is present, and
    # pretending otherwise would make the gate fail on every non-Windows machine. That
    # asymmetry is deliberate and is the one place cross-platform validation legitimately
    # stops — the entries are checked everywhere (above), the FILES only where they live.
    #
    # Worth having because an opt-in step is exactly where a typo survives: `package`
    # does not run unless asked, so a misspelled path there would otherwise be found by
    # whoever first tries to cut a release, which is the worst moment to find it.
    for id in $ids; do
        local cmdline tok
        cmdline=$(contract_value "cmd.$PLATFORM" "$id")
        [ -n "$cmdline" ] || continue
        for tok in $cmdline; do
            case "$tok" in
                */*.sh|*/*.ps1)
                    if [ ! -e "$tok" ]; then
                        echo "pipeline: step '$id' cmd.$PLATFORM names '$tok', which does not exist" >&2
                        errs=$((errs + 1))
                    fi
                    ;;
            esac
        done
    done

    # Ordinals must be non-decreasing in file order, so a reorder that forgets to
    # renumber is caught rather than silently changing the run order.
    local prev="" ord
    for id in $ids; do
        ord=$(step_ordinal "$id")
        if [ -n "$prev" ]; then
            local lower
            lower=$(printf '%s\n%s\n' "$prev" "$ord" | sort -V | head -1)
            if [ "$lower" != "$prev" ]; then
                echo "pipeline: step ordinals are out of order in file order: '$prev' then '$ord'" >&2
                errs=$((errs + 1))
            fi
        fi
        prev="$ord"
    done

    [ "$errs" -eq 0 ]
}

# --------------------------------------------------------------------------------------
# --list-steps — the parity artefact
#
# Prints the DERIVED list, one step per line, as `<ordinal>\t<id>\t<class>`. The setup
# phase is deliberately absent: it produces environment, has no verdict, and cannot be
# skipped or reordered, so modelling it as a step would force platforms without one to
# declare a non-applicable step 0 — noise that dilutes the declarations carrying real
# information.
# --------------------------------------------------------------------------------------
list_steps() {
    local id
    for id in $(derived_step_ids); do
        printf '%s\t%s\t%s\n' "$(step_ordinal "$id")" "$id" "$(contract_value class "$id")"
    done
}

# --------------------------------------------------------------------------------------
# --self-test
#
# Proves the derivation property still holds after an edit. It cannot CREATE the property
# — that comes from `derived_step_ids` being the only producer — but it catches an edit
# that introduces a second list.
# --------------------------------------------------------------------------------------
self_test() {
    echo ":: validating $CONTRACT"
    validate_contract || return 1
    echo "   contract is well-formed"

    # The list --list-steps prints must be the list the run loop iterates. Both call
    # derived_step_ids, so this compares the artefact against its own source.
    local printed derived
    printed=$(list_steps | cut -f2)
    derived=$(derived_step_ids)
    if [ "$printed" != "$derived" ]; then
        echo "pipeline: --list-steps does not print the derived step list" >&2
        diff <(echo "$derived") <(echo "$printed") >&2 || true
        return 1
    fi
    echo "   --list-steps prints exactly the derived step list"

    # The setup phase must not leak into the step list.
    if echo "$derived" | grep -qx "setup"; then
        echo "pipeline: 'setup' appears as a step; it is a phase, not a step" >&2
        return 1
    fi
    echo "   setup phase is absent from the step list (correct)"

    echo "   $(echo "$derived" | wc -l) steps derived"
    return 0
}

# --------------------------------------------------------------------------------------
# Step execution
# --------------------------------------------------------------------------------------
FAILED=""

announce() { printf '\n=== %s ===\n' "$1"; }

run_step() {
    local id="$1"
    local ord class intent cmd na
    ord=$(step_ordinal "$id")
    class=$(contract_value class "$id")
    intent=$(contract_value intent "$id")
    cmd=$(contract_value "cmd.$PLATFORM" "$id")
    na=$(contract_value "na.$PLATFORM" "$id")

    # A review step is announced, never judged. Pretending a script can decide whether a
    # behaviour change carries its check would be a gate that always passes.
    if [ "$class" = "review" ]; then
        announce "$ord. $id — REVIEW (human obligation, not machine-checkable)"
        echo "    intent: $intent"
        return 0
    fi

    # A packaging step is opt-in, and a skipped one is ANNOUNCED rather than omitted —
    # same reason as a non-applicable step. An omission the reader cannot see is how a
    # step quietly stops existing.
    if [ "$class" = "packaging" ] && [ "$DO_PACKAGE" -eq 0 ]; then
        announce "$ord. $id — not run (packaging is opt-in; pass --package)"
        echo "    intent: $intent"
        return 0
    fi


    # A non-applicable step is DECLARED here, in the run output, with its kind and
    # reason — not omitted, and not left in a source comment where the pipeline's user
    # cannot see it.
    if [ -n "$na" ]; then
        local kind reason
        kind=$(echo "$na" | awk '{print $1}')
        reason=$(echo "$na" | cut -d' ' -f2-)
        announce "$ord. $id — NOT APPLICABLE on $PLATFORM ($kind)"
        echo "    intent: $intent"
        echo "    reason: $reason"
        return 0
    fi

    local verdict
    verdict=$(contract_value verdict "$id")

    # A marker verdict never fails the run: its job is to make something VISIBLE that
    # would otherwise be silent. libtest captures a passing test's output, so a body
    # announcing "I verified nothing" is silenced exactly when it passes.
    case "$verdict" in
        marker:*)
            local marker="${verdict#marker:}"
            announce "$ord. $id — informational"
            echo "    intent: $intent"
            local out rc=0
            out=$(eval "$cmd" 2>&1) || rc=$?
            local hits
            hits=$(echo "$out" | grep -F "$marker" || true)
            if [ -n "$hits" ]; then
                echo "$hits" | sed 's/^/    /'
            else
                echo "    no tests reported '$marker'"
            fi
            # The re-run's exit code is reported but not fatal: "no marker lines" and
            # "the re-run itself failed" must not look identical.
            if [ "$rc" -ne 0 ]; then
                echo "    NOTE: the informational re-run exited $rc (the required run above already gated this)"
            fi
            return 0
            ;;
    esac

    announce "$ord. $id"
    echo "    intent: $intent"
    # Provenance travels with the artefact. Packaging is ALLOWED to run after an operator
    # override skipped a required gate — that combination is legitimate on a box where the
    # GTK suite cannot run — but the run must SAY SO, or an installer built from an
    # ungated tree looks exactly like one that passed everything.
    # Overrides only, never contract-declared non-applicable steps: those are always absent
    # on their platform, so naming them every time is noise that trains the reader to skim
    # the line whose whole value is appearing rarely.
    if [ "$class" = "packaging" ] && [ -n "$OVERRIDDEN_STEPS" ]; then
        echo "    WARNING: built without${OVERRIDDEN_STEPS%;}"
        echo "    The artefact below comes from a tree those gates did not check."
    fi
    report_carveouts "$id"
    cmd=$(apply_carveouts "$cmd" "$(carveout_skip_args "$id")")
    echo "    \$ $cmd"
    if eval "$cmd"; then
        echo "    PASS"
    else
        echo "    FAIL"
        FAILED="$FAILED $ord"
        return 1
    fi
}

# Carve-outs are reported even when there are none — that is what lets the statement
# MEAN something rather than be silence.
carveouts_for() {
    local id="$1" plat="${2:-$PLATFORM}"
    awk -v kw="carveout.$plat" -v key="$id" '
        $1 == kw && $2 == key { $1=""; $2=""; sub(/^[[:space:]]+/,""); print }
    ' "$CONTRACT"
}

# The libtest arguments that APPLY the carve-outs, so the run does what the
# announcement says. Announcing without applying is worse than silence: while the list
# is empty nothing is mis-reported, which is exactly why the gap stays invisible until
# the first carve-out is added and the run prints `skipped: <test>` for a test it ran.
carveout_skip_args() {
    local name args=""
    while IFS= read -r name; do
        [ -n "$name" ] || continue
        args="$args --skip $name"
    done <<EOF
$(carveouts_for "$1")
EOF
    printf '%s' "$args"
}

# Append the skip args past a `--` separator, reusing one if the command already has it
# (`cmd.windows integration` carries `-- --test-threads=1`), since a second `--` would be
# passed through to libtest as a literal argument rather than parsed as a separator.
apply_carveouts() {
    local cmd="$1" skips="$2"
    [ -n "$skips" ] || { printf '%s' "$cmd"; return; }
    case "$cmd" in
        *" -- "*) printf '%s%s' "$cmd" "$skips" ;;
        *)        printf '%s --%s' "$cmd" "$skips" ;;
    esac
}

report_carveouts() {
    local id="$1"
    local list
    list=$(carveouts_for "$id")
    if [ -z "$list" ]; then
        case "$id" in
            integration|test) echo "    carve-outs: none" ;;
        esac
    else
        echo "    carve-outs: $(echo "$list" | wc -l) by name, applied via --skip"
        echo "$list" | sed 's/^/      skipped: /'
    fi
}

run_setup_phase() {
    local desc
    desc=$(contract_value setup "$PLATFORM")
    announce "setup phase ($PLATFORM)"
    if [ "$desc" = "none" ]; then
        echo "    none required on $PLATFORM"
    else
        echo "    $desc"
    fi
}

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

rc=0
# Required steps an OPERATOR OVERRIDE prevented from running. Accumulated so a later
# packaging step can name them: an installer's provenance is part of the installer.
OVERRIDDEN_STEPS=""

for step_id in $(derived_step_ids); do
    if [ "$SKIP_INTEGRATION" -eq 1 ] && [ "$step_id" = "integration" ]; then
        announce "$(step_ordinal "$step_id"). $step_id — SKIPPED (--skip-integration)"
        echo "    NOTE: this is an operator override, not a contract declaration."
        OVERRIDDEN_STEPS="$OVERRIDDEN_STEPS step $(step_ordinal "$step_id") ($step_id), skipped by --skip-integration;"
        continue
    fi
    if ! run_step "$step_id"; then
        rc=1
        break
    fi
done

echo
if [ "$rc" -eq 0 ]; then
    echo "=== pipeline PASSED ==="
else
    echo "=== pipeline FAILED at step(s):$FAILED ==="
fi
exit "$rc"
