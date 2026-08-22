# shellcheck shell=bash
#
# Shared pipeline-runner machinery, sourced by the bash ports.
#
# WHY THIS FILE EXISTS. `scripts/pipeline.sh` and `packaging/macos/pipeline.sh` each
# carried their own copy of the same thirteen functions — about three hundred identical
# lines — and the copies had already drifted three times in ways no gate could see:
# the Linux copy validated that a carve-out lands on a `cargo test` command and the macOS
# copy did not; the Linux copy checked EVERY repo-relative script token named by a command
# and the macOS copy checked only the first word; and the macOS copy's `report_carveouts`
# re-implemented `carveouts_for` inline with its own `awk`. Each drift made one platform
# quietly the lenient one, which is the exact failure `scripts/pipeline.steps` exists to
# prevent one level down (ScrAP-207). Reconciling three divergences by hand would have
# fixed three instances and left the mechanism that produced them intact. One
# implementation removes the class.
#
# ── WHAT MUST NOT MOVE INTO THIS FILE ────────────────────────────────────────────────
#
# This is a boundary, not a dumping ground, and the same discipline the lint-references
# rule split follows. Keep out:
#
#   * Anything platform-conditional. No `uname`, no `case $PLATFORM in`, no branch on the
#     host. The moment a conditional lands here, this file becomes the place the platforms
#     differ, and it will differ in ways neither runner's reader can see — which is worse
#     than the duplication it replaced, because duplication is at least visible.
#   * The runner's own identity: `PLATFORM`, `CONTRACT`, the repo-root resolution, the
#     usage string, and any per-platform precondition (macOS's Darwin guard, which is
#     deliberately placed AFTER argument parsing so `--list-steps` and `--self-test` remain
#     runnable off-platform — that placement is a property of that runner, not of this).
#   * Any actual command. Commands live in `scripts/pipeline.steps`, which is the contract.
#     A command that appeared here would be a second place a step is defined.
#
# ── THE POWERSHELL PORT IS NOT A CANDIDATE ───────────────────────────────────────────
#
# `packaging/windows/pipeline.ps1` does NOT and CANNOT source this file — PowerShell cannot
# read a bash library, and there is no shell on a stock Windows box that would make it
# possible. Its agreement with the bash ports is proven exactly as POLICY § Build pipeline
# says: by diffing `--list-steps` / `-ListSteps` output between the ports. Do not "finish
# the unification" by porting this file to PowerShell and calling the result shared — two
# translations of one library are two implementations wearing one name, which is precisely
# the state this file was written to end. Three ports, one contract, and one bash library
# serving the two ports that can share it.
#
# ── WHAT THE SOURCING RUNNER OWES ────────────────────────────────────────────────────
#
# Set `PLATFORM` and `CONTRACT`, and `cd` to the repo root, BEFORE sourcing. Everything
# else has a default here.

# Sourced, never executed: run directly it would define functions into a shell that exits
# immediately, which looks like success and does nothing.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    echo "pipeline-lib.sh is a library; source it from a runner, do not execute it" >&2
    exit 2
fi

: "${PLATFORM:?pipeline-lib: the sourcing runner must set PLATFORM}"
: "${CONTRACT:?pipeline-lib: the sourcing runner must set CONTRACT}"

# Run-state the runners' entry points assign into. Defaulted here so `set -u` cannot turn
# a runner that legitimately never sets one into an obscure unbound-variable abort.
FAILED="${FAILED:-}"
DO_PACKAGE="${DO_PACKAGE:-0}"
SKIP_INTEGRATION="${SKIP_INTEGRATION:-0}"
OVERRIDDEN_STEPS="${OVERRIDDEN_STEPS:-}"

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
    bad=$(grep -nvE '^[[:space:]]*(#.*)?$|^(platform|step|intent|verdict|class|setup|cmd\.[a-z]+|na\.[a-z]+|carveout\.[a-z]+|disarm\.[a-z]+)[[:space:]]+[^[:space:]]+([[:space:]]+.*)?$' \
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

    # A DISARM declaration says the step RUNS on this platform with a named capability
    # deliberately absent — the case `na.` and `carveout.` between them cannot express.
    # `na.` means the step does not run at all; `carveout.` means named tests are skipped.
    # Neither covers "runs, but one of its gates is off", which is how a disarmed gate came
    # to be documented in a source comment where the pipeline's user could never see it.
    #
    # Three things are checked, and the last two are the ones that make it honest:
    # a kind, a reason, and that the step it names actually RUNS here. A disarm on a step
    # that is `na.` on the same platform, or that has no command, describes a capability of
    # something that never happens — which reads to the run's reader as a live caveat.
    # Checked for EVERY platform, since a disarm is per-platform data.
    for id in $ids; do
        local plat dis dkind dreason dcmd dna
        for plat in $platforms; do
            dis=$(contract_value "disarm.$plat" "$id")
            [ -n "$dis" ] || continue

            dkind=$(echo "$dis" | awk '{print $1}')
            case "$dkind" in
                permanent|tooling) ;;
                *)
                    echo "pipeline: step '$id' disarm.$plat has kind '$dkind'; expected" >&2
                    echo "  'permanent' or 'tooling'" >&2
                    errs=$((errs + 1))
                    ;;
            esac

            dreason=$(echo "$dis" | cut -d' ' -f2-)
            if [ -z "$dreason" ] || [ "$dreason" = "$dkind" ]; then
                echo "pipeline: step '$id' disarm.$plat has a kind but no reason." >&2
                echo "  The reason IS the declaration — the fact alone is what a source" >&2
                echo "  comment already said." >&2
                errs=$((errs + 1))
            fi

            dna=$(contract_value "na.$plat" "$id")
            if [ -n "$dna" ]; then
                echo "pipeline: step '$id' has BOTH na.$plat and disarm.$plat. A step that" >&2
                echo "  does not run cannot have a disarmed capability." >&2
                errs=$((errs + 1))
            fi

            dcmd=$(contract_value "cmd.$plat" "$id")
            if [ -z "$dcmd" ]; then
                echo "pipeline: step '$id' has disarm.$plat but no cmd.$plat, so there is" >&2
                echo "  no run for the declaration to qualify." >&2
                errs=$((errs + 1))
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
    # EVERY token, not just the first word: a command's script may be an argument rather
    # than the program (`xvfb-run -a scripts/foo.sh`, `bash scripts/bar.sh`), and a
    # first-word-only check silently passes a misspelling in every one of those positions.
    # That was a real divergence between the two copies of this function.
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
#
# This artefact is how the WINDOWS port's agreement is established, since it cannot source
# this file. Both bash ports keep their own `--list-steps` entry point for that reason:
# sharing the implementation removes the drift between them, and does not remove the need
# for the artefact.
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
#
# This covers the machinery in THIS file. A runner with a platform-specific property worth
# asserting defines `platform_self_test`, which is called at the end if present; a runner
# with none defines nothing and pays nothing.
# --------------------------------------------------------------------------------------
self_test() {
    echo ":: validating $CONTRACT"
    validate_contract || return 1
    echo "   contract is well-formed"

    # The list --list-steps prints must be the list the run loop iterates. Both call
    # derived_step_ids, so this compares the artefact against its own source.
    #
    # WHAT THIS DOES NOT COVER, stated so a reader counting green lines does not over-credit
    # it: because `printed` and `derived` share that source, they move together. A
    # `derived_step_ids` that DROPS a step passes this assertion, and so does a
    # `step_ordinal` that returns a constant, since ordinals are never asserted here. That is
    # correct rather than broken — the property being defended is that no SECOND list exists
    # to disagree with the first, which is exactly what the comment claims and all it claims.
    # Conformance of the shared source itself is the contract's own validation, above.
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

    # The shared functions must actually be the shared ones. A runner that re-defines one
    # after sourcing has re-created the drift this file exists to remove, and would do it
    # invisibly — bash silently takes the later definition.
    # The shared functions must be the LIBRARY'S functions, byte for byte.
    #
    # This check used to be `[ "$(command -v "$fn")" != "$fn" ]`, which proves only that
    # SOME shell function of that name exists — `command -v` prints the bare name for a
    # library definition and for a runner's override identically. Its comment claimed it
    # caught a runner re-defining a shared function after sourcing, and it did not: a
    # planted `apply_carveouts() { printf "%s" "$1"; }` immediately after `. "$LIB"` left
    # this printing "all 14 shared functions resolve from the library" and exiting 0 while
    # the override was the live body. A comment asserting a property the code does not have,
    # in the file whose whole purpose is preventing drift.
    #
    # Fingerprints are captured at the END of this file, after every definition, so a
    # redefinition anywhere later — a runner, a sourced fragment, an interactive shell —
    # changes the body and fails here by name.
    local fn expect got missing=0 changed=0
    for fn in $PIPELINE_LIB_FNS; do
        if ! declare -f "$fn" >/dev/null 2>&1; then
            echo "pipeline: '$fn' is not defined; the library did not load" >&2
            missing=$((missing + 1))
            continue
        fi
        got=$(declare -f "$fn" | cksum | awk '{print $1}')
        expect=$(printf '%s\n' "$PIPELINE_LIB_FN_PRINTS" | awk -v f="$fn" '$1 == f { print $2 }')
        if [ -z "$expect" ]; then
            echo "pipeline: '$fn' has no fingerprint; PIPELINE_LIB_FNS and the capture at" >&2
            echo "  the end of pipeline-lib.sh disagree" >&2
            missing=$((missing + 1))
        elif [ "$got" != "$expect" ]; then
            echo "pipeline: '$fn' is NOT the library's definition — it was redefined after" >&2
            echo "  the library was sourced. The shared implementation is not the one running." >&2
            changed=$((changed + 1))
        fi
    done
    [ "$missing" -eq 0 ] && [ "$changed" -eq 0 ] || return 1
    # Counted from the list, never written as a literal beside it: a fifteenth function
    # added to PIPELINE_LIB_FNS would otherwise leave this still saying 14.
    echo "   all $(set -- $PIPELINE_LIB_FNS; echo $#) shared functions are the library's own (fingerprinted)"

    # The step-disposition ORDER, pinned by table. The second row is the one that matters
    # and the reason this table exists: a packaging step that is also `na.` here must report
    # the DECLARED REASON, not the generic opt-in notice. No contract state can exercise
    # that today — nothing is both packaging and na. — so without this table the order is
    # unverifiable and drifts back silently.
    local case_in expect got disp_errs=0
    while IFS='|' read -r class na pkg expect; do
        [ -n "$class" ] || continue
        got=$(step_disposition "$class" "$na" "$pkg")
        if [ "$got" != "$expect" ]; then
            echo "pipeline: step_disposition('$class','$na',$pkg) = '$got', expected '$expect'" >&2
            disp_errs=$((disp_errs + 1))
        fi
    done <<'DISP'
review|permanent a reason|0|review
review||0|review
packaging|permanent a reason|0|not-applicable
packaging||0|packaging-skipped
packaging||1|run
required|permanent a reason|0|not-applicable
required||0|run
informational||0|run
DISP
    [ "$disp_errs" -eq 0 ] || return 1
    echo "   step_disposition order pinned (declared reason outranks the opt-in notice)"

    contract_negative_cases || return 1
    echo "   validate_contract refuses every malformed contract, each for its own stated reason"

    execution_cases || return 1
    echo "   run_step executes: carve-outs applied and reported, verdicts distinguished,"
    echo "   disarm and provenance announced (each paired with its negative)"

    echo "   $(echo "$derived" | wc -l | tr -d '[:space:]') steps derived"

    if declare -F platform_self_test >/dev/null 2>&1; then
        platform_self_test || return 1
    fi
    return 0
}


# --------------------------------------------------------------------------------------
# Negative coverage for validate_contract
#
# WHY A SYNTHETIC CONTRACT AND NOT A MUTATED REAL ONE. `self_test` only ever ran
# `validate_contract` against `scripts/pipeline.steps`, which is valid — so the function
# could only ever return 0, and stubbing it to `return 0` outright left the whole self-test
# green. Twelve mutations survived that gap. Every validation rule below was, until this
# harness existed, unverified in the failing direction, including the `disarm.` rules that
# F-GATE-006 rests on.
#
# The contract is BUILT here rather than derived from the real one because a mutation of the
# real file is only reachable on the platform whose lines it happens to touch: no step is
# `na.` on Linux, so a case built by mutating `na.macos coverage` cannot run from the Linux
# runner at all. A synthetic contract declares all three platforms, so every case runs
# identically from either bash port — which is the property the whole library exists for.
#
# ASSERT ON THE MESSAGE, NEVER ON THE BOOLEAN. Several of these fail for different reasons
# and a bare non-zero cannot tell them apart, so a case could pass while the rule it names
# is dead and some other rule is firing. Model courtesy of the `windows` seat's
# Test-SyntheticContract; the bash implementation and its cases are this seat's.

# synthetic_contract [extra lines...] -> path to a temp contract file
#
# A minimal WELL-FORMED contract, plus whatever the caller appends. Kept deliberately small:
# every line a case does not care about is a line that can fail a case for the wrong reason.
synthetic_contract() {
    local f
    f=$(mktemp) || return 1
    cat >"$f" <<'BASE'
platform linux Linux
platform macos macOS
platform windows Windows
setup linux none
setup macos none
setup windows none
step 1 alpha
intent alpha alpha does a thing
verdict alpha exit
class alpha required
cmd.linux alpha true
cmd.macos alpha true
cmd.windows alpha true
BASE
    local line
    for line in "$@"; do
        printf '%s\n' "$line" >>"$f"
    done
    printf '%s' "$f"
}

# contract_rejects <label> <expected message fragment> [extra contract lines...]
# `expect` may name SEVERAL fragments, separated by the record separator below, and ALL of
# them must appear. That exists for the co-occurring-rule case: where one input violates two
# rules and the validator states both, a case asserting only one fragment is satisfied by
# the OTHER rule's rejection, so deleting the rule under test leaves it green.
CONTRACT_EXPECT_SEP='|@|'

contract_rejects() {
    local label="$1" expect="$2"
    shift 2
    local f out rc=0
    f=$(synthetic_contract "$@") || { echo "pipeline: mktemp failed" >&2; return 1; }
    out=$( CONTRACT="$f"; validate_contract 2>&1 ) || rc=$?
    rm -f "$f"

    if [ "$rc" -eq 0 ]; then
        echo "pipeline: NEGATIVE CASE NOT REJECTED — $label" >&2
        echo "  validate_contract accepted a contract it must refuse" >&2
        return 1
    fi
    local rest="$expect" frag
    while [ -n "$rest" ]; do
        case "$rest" in
            *"$CONTRACT_EXPECT_SEP"*)
                frag=${rest%%"$CONTRACT_EXPECT_SEP"*}
                rest=${rest#*"$CONTRACT_EXPECT_SEP"}
                ;;
            *) frag=$rest; rest='' ;;
        esac
        case "$out" in
            *"$frag"*) ;;
            *)
                echo "pipeline: WRONG REASON — $label" >&2
                echo "  rejected, but not for the rule under test." >&2
                echo "  expected to see: $frag" >&2
                echo "  actually said:   $(printf '%s' "$out" | head -2)" >&2
                return 1
                ;;
        esac
    done
    return 0
}

# The whole harness, run from self_test. Returns non-zero if any case fails.
contract_negative_cases() {
    # DELIBERATELY NOT `errs=$((errs + 1))`, which is what `validate_contract` uses.
    # MEASURED: with an arithmetic accumulator here, neutralising every `errs=$((errs + 1))`
    # in this file — the audit's own file-wide mutation — disarmed the HARNESS along with
    # the target. Fifteen cases printed "NEGATIVE CASE NOT REJECTED" and the self-test still
    # exited 0, because the harness's verdict was computed by the same construct the
    # mutation removed. A test that shares an idiom with its subject can be switched off by
    # the same edit, and it fails in the direction that reads as success. Names accumulate
    # into a string instead, and the verdict is emptiness rather than arithmetic.
    #
    # Both directions re-measured after this comment was found naming the two idioms the
    # wrong way round: neutralising every `errs=$((errs + 1))` gives exit 1 with fifteen
    # cases named (the fix working), and neutralising every `failed="$failed case"` gives
    # exit 0 with nothing printed (the harness alone, as intended). A reader following the
    # inverted version literally would have moved this accumulator TO arithmetic, which is
    # the defect the paragraph exists to prevent.
    local failed="" f out rc=0

    # POSITIVE CONTROL FIRST. If the synthetic base is itself invalid, every case below
    # "passes" by being rejected for the wrong reason, and the harness certifies nothing.
    f=$(synthetic_contract) || return 1
    out=$( CONTRACT="$f"; validate_contract 2>&1 ) || rc=$?
    rm -f "$f"
    if [ "$rc" -ne 0 ]; then
        echo "pipeline: the synthetic BASE contract is not valid — every negative case" >&2
        echo "  below would be rejected for the wrong reason and prove nothing:" >&2
        printf '%s\n' "$out" >&2
        return 1
    fi

    # An empty contract must fail loudly rather than agree with everyone about nothing.
    out=$( CONTRACT="/dev/null"; validate_contract 2>&1 ) || rc=$?
    case "$out" in
        *"defines no steps"*) ;;
        *) echo "pipeline: an empty contract was not refused" >&2; failed="$failed case" ;;
    esac

    # --- grammar ----------------------------------------------------------------------
    contract_rejects "unrecognised line" "neither blank, a comment" \
        "this is not a keyword line" || failed="$failed case"

    # --- step completeness ------------------------------------------------------------
    contract_rejects "duplicate step id" "duplicate step id" \
        "step 2 alpha" || failed="$failed case"
    contract_rejects "unknown class" "unknown class" \
        "step 2 beta" "intent beta b" "verdict beta exit" "class beta nonsense" \
        "cmd.linux beta true" "cmd.macos beta true" "cmd.windows beta true" \
        || failed="$failed case"
    contract_rejects "missing intent" "has no intent" \
        "step 2 beta" "verdict beta exit" "class beta required" \
        "cmd.linux beta true" "cmd.macos beta true" "cmd.windows beta true" \
        || failed="$failed case"
    contract_rejects "neither cmd nor na" "neither cmd." \
        "step 2 beta" "intent beta b" "verdict beta exit" "class beta required" \
        "cmd.linux beta true" "cmd.windows beta true" || failed="$failed case"
    contract_rejects "both cmd and na" "has BOTH cmd." \
        "na.macos alpha permanent a reason" || failed="$failed case"
    contract_rejects "ordinals out of order" "out of order" \
        "step 0 beta" "intent beta b" "verdict beta exit" "class beta required" \
        "cmd.linux beta true" "cmd.macos beta true" "cmd.windows beta true" \
        || failed="$failed case"

    # --- na. --------------------------------------------------------------------------
    contract_rejects "na kind not in {permanent,tooling}" "na.macos has kind" \
        "step 2 beta" "intent beta b" "verdict beta exit" "class beta required" \
        "cmd.linux beta true" "cmd.windows beta true" "na.macos beta bogus a reason" \
        || failed="$failed case"
    contract_rejects "na kind with no reason" "na.macos has a kind but no reason" \
        "step 2 beta" "intent beta b" "verdict beta exit" "class beta required" \
        "cmd.linux beta true" "cmd.windows beta true" "na.macos beta permanent" \
        || failed="$failed case"

    # --- carve-outs -------------------------------------------------------------------
    contract_rejects "carve-out on a non-cargo-test command" "cargo test" \
        "carveout.macos alpha some::body" || failed="$failed case"

    # --- disarm. (the rules F-GATE-006 rests on) --------------------------------------
    contract_rejects "disarm kind not in {permanent,tooling}" "disarm.macos has kind" \
        "disarm.macos alpha bogus a reason" || failed="$failed case"
    contract_rejects "disarm kind with no reason" "has a kind but no reason" \
        "disarm.macos alpha permanent" || failed="$failed case"
    # ONE case, TWO required fragments, and that is the whole point of it.
    #
    # This input violates two rules at once and the validator states BOTH — a ruling made
    # deliberately, because the facts are different (the step is declared absent here, AND
    # there is no command to disarm) and collapsing them into one message would make a
    # reader guess which fired.
    #
    # It used to be TWO cases over byte-identical contracts, which pinned that ruling only
    # by an accident of duplication: nothing said the sameness was deliberate, so it read
    # as copy-paste and the obvious tidy was to merge them — silently dropping one
    # message's pin, since the surviving case still passed. Documenting the coincidence was
    # the first fix; the Windows seat then demonstrated the better one in its own port, so
    # this follows it. The pairing is now the case's SUBJECT and there is nothing left to
    # merge.
    #
    # It is also the round's clearest evidence that asserting on the MESSAGE is
    # load-bearing rather than stylistic. Because both rules fire on this input, disabling
    # either leaves `validate_contract` non-zero — so a boolean assertion is PROVABLY
    # VACUOUS here, passing with the rule under test deleted. Requiring both fragments is
    # the only thing that makes either rule's removal detectable.
    contract_rejects "disarm on an na. step names BOTH conflicts" \
        "BOTH na.macos and disarm.macos${CONTRACT_EXPECT_SEP}no cmd." \
        "step 2 beta" "intent beta b" "verdict beta exit" "class beta required" \
        "cmd.linux beta true" "cmd.windows beta true" "na.macos beta permanent a reason" \
        "disarm.macos beta permanent a reason" || failed="$failed case"

    # --- script existence (this platform only, so build the line for it) --------------
    contract_rejects "command names a script that does not exist" "does not exist" \
        "step 2 beta" "intent beta b" "verdict beta exit" "class beta required" \
        "cmd.linux beta bash scripts/definitely-not-here.sh" \
        "cmd.macos beta bash scripts/definitely-not-here.sh" \
        "cmd.windows beta bash scripts/definitely-not-here.sh" || failed="$failed case"

    [ -z "$failed" ]
}


# --------------------------------------------------------------------------------------
# Execution coverage for run_step and the carve-out machinery
#
# WHY IT EXISTS. `--self-test` proved the DERIVATION property and nothing about the
# EXECUTOR, so six mutations survived it: `apply_carveouts` gutted, `carveout_skip_args`
# gutted, `report_carveouts` silenced, `run_step`'s verdict forced true, the disarm
# announcement deleted, the provenance warning deleted. All green. That was scope rather
# than a broken promise — but after `disarm.` landed, the PowerShell self-test exercised its
# executor and this one did not, so the two ports stopped verifying comparable properties in
# a library BOTH bash runners inherit. A defect here is two platforms, not one.
#
# ASSERT ON WHAT THE EXECUTION PRODUCED, NOT ON THE COMMAND TEXT. `run_step` echoes the
# assembled command before running it, so a fragment asserted against that line proves the
# command was BUILT and says nothing about whether it RAN — delete the `eval` and such an
# assertion still passes. The carve-out cases therefore run `printf EXECUTED-%s\n`, whose
# OUTPUT (`EXECUTED---skip`) is a string that appears nowhere in the command text. Building
# without running, or running without applying, both fail it.
#
# EVERY PASSING CASE IS PAIRED WITH A FAILING ONE. A suite of positives cannot distinguish
# "the assertion holds" from "the assertion always holds".

# run_step_capture <step-id> [contract lines...] -> sets RS_OUT and RS_RC
#
# The capture subshell keeps run_step's writes to FAILED from leaking into the real run.
RS_OUT=""
RS_RC=0
run_step_capture() {
    local id="$1"
    shift
    local f
    f=$(synthetic_contract "$@") || return 1
    RS_OUT=$( CONTRACT="$f"; run_step "$id" 2>&1 )
    RS_RC=$?
    rm -f "$f"
    return 0
}

execution_cases() {
    local failed="" saved_pkg="$DO_PACKAGE" saved_over="$OVERRIDDEN_STEPS"

    # Contract lines are collected in an ARRAY, one line per element. An earlier draft built
    # them with unquoted command substitution, which word-split every line into its
    # constituent words — synthetic_contract then wrote one WORD per line, no step resolved,
    # `cmd` came back empty, `eval ""` succeeded, and the cases reported PASS. A harness that
    # silently tests an empty command is the failure this whole audit is about, so it is
    # written down rather than just fixed.
    local CASE_LINES
    _reset() { CASE_LINES=(); }
    _add()   { CASE_LINES=(${CASE_LINES[@]+"${CASE_LINES[@]}"} "$1"); }
    _step()  {
        _add "step 2 $1"
        _add "intent $1 does a thing"
        _add "verdict $1 exit"
        _add "class $1 $2"
    }
    _cmds()  {
        _add "cmd.linux $1 $2"
        _add "cmd.macos $1 $2"
        _add "cmd.windows $1 $2"
    }
    _carve() {
        _add "carveout.linux $1 $2"
        _add "carveout.macos $1 $2"
        _add "carveout.windows $1 $2"
    }
    _disarm() {
        _add "disarm.linux $1 $2"
        _add "disarm.macos $1 $2"
        _add "disarm.windows $1 $2"
    }
    _run()   { run_step_capture "$1" ${CASE_LINES[@]+"${CASE_LINES[@]}"}; }

    _want() {
        case "$RS_OUT" in
            *"$2"*) ;;
            *) echo "pipeline: EXECUTION CASE FAILED — $1" >&2
               echo "  expected output to contain: $2" >&2
               failed="$failed case" ;;
        esac
    }
    _want_not() {
        case "$RS_OUT" in
            *"$2"*) echo "pipeline: EXECUTION CASE FAILED — $1" >&2
               echo "  output must NOT contain: $2" >&2
               failed="$failed case" ;;
        esac
    }
    _want_rc() {
        [ "$RS_RC" = "$2" ] || {
            echo "pipeline: EXECUTION CASE FAILED — $1" >&2
            echo "  run_step returned $RS_RC, expected $2" >&2
            failed="$failed case"
        }
    }

    # ── carve-outs, PROVEN applied by the running command's own output ───────────────
    # `printf EXECUTED-%s\n` emits one line per argument, so an appended `--skip some::body`
    # surfaces as EXECUTED---skip / EXECUTED-some::body — strings that appear nowhere in the
    # command text `run_step` echoes. Asserting on the echoed text instead would pass with
    # the `eval` deleted. Contains "cargo test" to satisfy the carve-out precondition.
    _reset; _step beta required; _cmds beta 'printf EXECUTED-%s\n cargo test'
    _carve beta 'some::body'; _run beta
    _want    "carve-out reaches the RUNNING command"      "EXECUTED---skip"
    _want    "carve-out name reaches the RUNNING command" "EXECUTED-some::body"
    _want    "carve-out is reported"                      "1 by name, applied via --skip"
    _want    "carve-out name is listed"                   "skipped: some::body"
    _want_rc "carve-out case succeeds"                    0

    # Paired negative: identical command, no carve-out. Without this, the positive proves
    # only that printf prints.
    _reset; _step beta required; _cmds beta 'printf EXECUTED-%s\n cargo test'; _run beta
    _want     "the command still runs without carve-outs" "EXECUTED-cargo"
    _want_not "no carve-out means no skip args"           "EXECUTED---skip"

    # ── the "none" report, which fires only for the test/integration ids ─────────────
    _reset; _step test required; _cmds test 'true'; _run test
    _want "absence of carve-outs is stated, not silent" "carve-outs: none"

    # ── verdicts: a passing and a failing command must not look alike ────────────────
    _reset; _step beta required; _cmds beta 'true'; _run beta
    _want    "a passing command reports PASS" "PASS"
    _want_rc "a passing command returns 0"    0

    _reset; _step beta required; _cmds beta 'false'; _run beta
    _want    "a failing command reports FAIL" "FAIL"
    _want_rc "a failing command returns 1"    1

    # ── disarm announcement, with its reason ─────────────────────────────────────────
    _reset; _step beta required; _cmds beta 'true'; _disarm beta 'permanent REASONMARKER'
    _run beta
    _want "a disarmed gate is announced" "DISARMED on"
    _want "the disarm REASON is printed" "REASONMARKER"

    _reset; _step beta required; _cmds beta 'true'; _run beta
    _want_not "an undisarmed step says nothing about disarming" "DISARMED on"

    # ── provenance: an artefact built after an override must say so ──────────────────
    DO_PACKAGE=1
    OVERRIDDEN_STEPS=" step 5 (integration), skipped by --skip-integration;"
    _reset; _step beta packaging; _cmds beta 'true'; _run beta
    _want "packaging names that gates did not run" "WARNING: built without"
    _want "packaging names WHICH gate"             "integration"

    OVERRIDDEN_STEPS=""
    _reset; _step beta packaging; _cmds beta 'true'; _run beta
    _want_not "a clean build claims no missing gates" "WARNING: built without"

    DO_PACKAGE="$saved_pkg"
    OVERRIDDEN_STEPS="$saved_over"
    unset -f _reset _add _step _cmds _carve _disarm _run _want _want_not _want_rc

    [ -z "$failed" ]
}

# --------------------------------------------------------------------------------------
# Step execution
# --------------------------------------------------------------------------------------

announce() { printf '\n=== %s ===\n' "$1"; }

# step_disposition <class> <na> <do_package> -> review | not-applicable | packaging-skipped | run
#
# WHAT DECIDES, EXTRACTED FROM WHAT PRINTS. The ORDER of these tests is the whole content of
# this function, and it was a live parity divergence: this file tested packaging-opt-in
# BEFORE `na.`, and `Invoke-ContractStep` in the PowerShell port tested `na.` first. For a
# packaging step that is non-applicable on a platform, run without --package, bash therefore
# printed the generic "not run (packaging is opt-in)" where Windows printed the declared
# kind and reason. Same non-execution, two different answers to "why".
#
# Windows' order is the correct one and is what this now implements: a DECLARED REASON beats
# a GENERIC NOTICE. That is the same principle as the `disarm.` work — the reason is the
# declaration, and a notice that withholds it costs the reader the one thing they came for.
#
# It is a separate function because the divergence is UNREACHABLE from the current contract:
# no step is both `packaging` and `na.` on any platform, so no run of the real pipeline can
# distinguish the two orders, and a reorder verified only against real data would be
# verified against nothing. A pure function over its three inputs can be pinned by a table
# (see self_test), which is the only way this order stays put.
step_disposition() {
    local class="$1" na="$2" do_package="$3"

    # A review step is announced, never judged, whatever else is true of it.
    if [ "$class" = "review" ]; then printf 'review'; return; fi

    # Declared non-applicable outranks the opt-in notice. This is the branch that moved.
    if [ -n "$na" ]; then printf 'not-applicable'; return; fi

    if [ "$class" = "packaging" ] && [ "$do_package" -eq 0 ]; then
        printf 'packaging-skipped'; return
    fi

    printf 'run'
}

run_step() {
    local id="$1"
    local ord class intent cmd na
    ord=$(step_ordinal "$id")
    class=$(contract_value class "$id")
    intent=$(contract_value intent "$id")
    cmd=$(contract_value "cmd.$PLATFORM" "$id")
    na=$(contract_value "na.$PLATFORM" "$id")

    case "$(step_disposition "$class" "$na" "$DO_PACKAGE")" in
        review)
            # Announced, never judged. Pretending a script can decide whether a behaviour
            # change carries its check would be a gate that always passes.
            announce "$ord. $id — REVIEW (human obligation, not machine-checkable)"
            echo "    intent: $intent"
            return 0
            ;;
        not-applicable)
            # DECLARED here, in the run output, with its kind and reason — not omitted, and
            # not left in a source comment where the pipeline's user cannot see it.
            local kind reason
            kind=$(echo "$na" | awk '{print $1}')
            reason=$(echo "$na" | cut -d' ' -f2-)
            announce "$ord. $id — NOT APPLICABLE on $PLATFORM ($kind)"
            echo "    intent: $intent"
            echo "    reason: $reason"
            return 0
            ;;
        packaging-skipped)
            # Opt-in, and a skipped one is ANNOUNCED rather than omitted — same reason as a
            # non-applicable step. An omission the reader cannot see is how a step quietly
            # stops existing.
            announce "$ord. $id — not run (packaging is opt-in; pass --package)"
            echo "    intent: $intent"
            return 0
            ;;
    esac

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
    # A DISARMED capability is stated in the run output, with its reason, for the same
    # reason a non-applicable step is: the pipeline's user cannot see a source comment.
    # The reason is printed, not just the fact — a run that said only "disarmed" would
    # move the comment into the output without moving the information.
    local disarm
    disarm=$(contract_value "disarm.$PLATFORM" "$id")
    if [ -n "$disarm" ]; then
        local dkind dreason
        dkind=$(echo "$disarm" | awk '{print $1}')
        dreason=$(echo "$disarm" | cut -d' ' -f2-)
        echo "    DISARMED on $PLATFORM ($dkind) — this step runs, with a gate deliberately off"
        echo "    reason: $dreason"
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

# Reads through carveouts_for rather than re-querying the contract. The macOS copy of this
# function used to inline its own awk, which is how it drifted into omitting the "applied
# via --skip" half of the statement while still claiming to report carve-outs.
report_carveouts() {
    local id="$1"
    local list
    list=$(carveouts_for "$id")
    if [ -z "$list" ]; then
        case "$id" in
            integration|test) echo "    carve-outs: none" ;;
        esac
    else
        echo "    carve-outs: $(echo "$list" | wc -l | tr -d '[:space:]') by name, applied via --skip"
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
# The run
#
# Shared for the same reason as everything above: the two copies of this loop were
# identical, and an identical copy is a drift waiting to happen. Returns the run's exit
# code so each runner keeps ownership of its own `exit`.
# --------------------------------------------------------------------------------------
run_all_steps() {
    local rc=0 step_id
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
    return "$rc"
}

# --------------------------------------------------------------------------------------
# Load-time fingerprints
#
# MUST STAY LAST. These capture each shared function's body as it leaves this file, so
# `self_test` can prove the running definitions are still the library's rather than merely
# that something of that name exists. Anything defined after this point is not covered.
# --------------------------------------------------------------------------------------
PIPELINE_LIB_FNS="contract_value derived_step_ids step_ordinal validate_contract list_steps
self_test announce step_disposition run_step carveouts_for carveout_skip_args
apply_carveouts report_carveouts run_setup_phase run_all_steps
synthetic_contract contract_rejects contract_negative_cases
run_step_capture execution_cases"

PIPELINE_LIB_FN_PRINTS="$(
    for _plf in $PIPELINE_LIB_FNS; do
        if declare -f "$_plf" >/dev/null 2>&1; then
            printf '%s %s\n' "$_plf" "$(declare -f "$_plf" | cksum | awk '{print $1}')"
        fi
    done
)"
unset _plf
