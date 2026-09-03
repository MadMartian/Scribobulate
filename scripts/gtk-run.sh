#!/usr/bin/env bash
#
# Run a command against a THROWAWAY GTK session: a private X display, a private D-Bus,
# criticals fatal, output that cannot wedge a reader, and a wall-clock bound.
#
#     scripts/gtk-run.sh <label> <budget-seconds> <command> [args...]
#
# `<label>` names the caller in this script's own verdicts ("integration", "coverage");
# `<budget-seconds>` is that caller's wedge bound. Exit status is the command's own,
# except that a timeout is reported as its own verdict and passed through as 124/137.
#
# ── WHY THIS IS A SCRIPT AND NOT FOUR WORDS ON A COMMAND LINE ─────────────────────────
#
# Two pipeline steps need this session — step 5 (the GTK integration suite) and step 6's
# full-suite coverage leg — and every one of the four concerns below was measured the
# hard way. A second hand-written copy of them is a copy that will be right on the day it
# is written and wrong afterwards: the nesting order in particular FAILS FAVOURABLY, so a
# green run in the wrong order is not evidence of anything.
#
#
# 1. IT ARMS `dbus-run-session`, WHICH POLICY HAS ALWAYS PRESCRIBED AND NOTHING RAN.
#
# In a session with no accessibility bus — every agent session, and any session whose
# `at-spi-dbus-bus.service` has gone stale — GTK emits `Unable to connect to the
# accessibility bus` as a `Gtk-CRITICAL`, and `G_DEBUG=fatal-criticals` promotes it to a
# SIGTRAP before a single test runs. The failure names accessibility, is not about
# accessibility, is not about the change under test, and reproduces identically on an
# untouched tree, so it costs a control build to disbelieve every time.
#
# POLICY § Build pipeline has said to run under `dbus-run-session` since that was
# measured. The contract's command did not, so the instruction lived only in prose and
# each caller supplied it by hand or got the SIGTRAP — precisely the failure the pipeline
# contract records about `G_DEBUG=fatal-criticals`, a prescribed gate with nothing in the
# toolchain arming it.
#
#
# 2. IT KEEPS BUS-ACTIVATED DAEMONS OFF THE CALLER'S STDOUT.
#
# ⚠ Without this the pipeline HANGS A READER FOREVER ON A PASSING RUN, which is the worst
# shape a gate can fail in: a green run and a wedged one are indistinguishable from
# outside. MEASURED 2026-08-28 — `scripts/pipeline.sh | tail -45` sat for 70 minutes on a
# pipeline that had finished in about 12.
#
# The mechanism: `dbus-run-session` starts a private bus, and GTK activity activates a
# crowd of services on it — reproduced in isolation as portal.Desktop, portal.Documents,
# PermissionStore, portal-{gnome,kde,gtk}, gvfs, org.a11y.Bus, atspi.Registry and
# secrets. The private `dbus-daemon` forks each one, so every one inherits this process's
# stdout and stderr. They outlive the bus (systemd --user reaps them as subreaper), so the
# write end of the caller's pipe stays open after the pipeline exits, and any reader
# waiting for EOF waits forever. `timeout` does not help: the command already finished.
#
# The fix is to hand the daemons a FILE instead of the caller's pipe. Everything below
# runs with its output redirected to `$log`, which is emitted here once the command is
# done — so a daemon that lingers holds a descriptor on a temp file nobody is waiting on,
# and this script's own exit closes the caller's pipe normally.
#
# Do NOT "simplify" this into a direct `dbus-run-session -- <cmd>`. The redirect IS the
# fix, and its absence is invisible until someone pipes the pipeline. Chasing the daemons
# individually (`GTK_USE_PORTAL=0` and friends) is whack-a-mole: the list above is nine
# services deep and grows with the desktop, and one missed entry restores the hang.
#
#
# 3. THE NESTING ORDER IS LOAD-BEARING: `xvfb-run` OUTSIDE, `dbus-run-session` INSIDE.
#
# ⚠ Inverted — which is how this ran until it was measured — the private bus is started
# BEFORE the display exists, so it inherits the ambient `DISPLAY`. Every service it
# activates then inherits that too, and the crowd from note 2 connects to the DEVELOPER'S
# REAL X SERVER instead of the Xvfb this exists to isolate. `xvfb-run` only rewrites
# `DISPLAY` for the command it wraps, so wrapping the test command alone isolates the
# tests and nothing else.
#
# MEASURED 2026-08-30 on the reference host: inverted, one run made ~20 connections to the
# live `:0` and printed `qt.qpa.xcb: could not connect to display :0` twenty-one times
# alongside `Maximum number of clients reached`; the session's X server was at 251 of
# X.org's 256-client default, so the step aborted under `G_DEBUG=fatal-criticals` with a
# `SIGTRAP` naming the accessibility bus — note 1's failure, arriving by a different road
# and immune to note 1's fix. In this order: zero contacts with `:0`, zero refusals.
#
# IT FAILS FAVOURABLY, which is why it survived. On a desktop with client slots to spare
# the leak is invisible and every test passes; it only goes red once the developer's own
# session is full, at which point it reports a fault in the accessibility bus. So a GREEN
# RUN IN THE INVERTED ORDER WAS NEVER EVIDENCE OF ISOLATION — it was evidence that the
# machine had room. An isolation boundary has to enclose everything that inherits the
# environment, not just the process under test.
#
#
# 4. IT BOUNDS THE RUN.
#
# A wedged GTK suite is a real failure mode (ScrAP-166 is a whole entry about
# misdiagnosing one), and an unbounded step turns it into a run nobody can tell from a
# slow one. Budgets are the caller's, and are deliberately generous: the timeout exists to
# catch a WEDGE, not to police duration. A timeout is reported as its own distinct verdict
# rather than as a command failure, because "it said no" and "it never answered" are
# different findings and must not print the same way.
set -uo pipefail

if [ "$#" -lt 3 ]; then
    echo "usage: $0 <label> <budget-seconds> <command> [args...]" >&2
    exit 2
fi
label="$1"
budget="$2"
shift 2

# The label reaches a `mktemp` TEMPLATE and, until this line, the EXIT trap's shell.
# Neither is a place to put an argument verbatim: a label containing a quote closed the
# trap's string and ran the rest as a command, and one containing a `/` produces a
# template `mktemp` refuses outright (F-SEC-207). Reduced to the character class a label
# is actually made of.
label_safe=${label//[^A-Za-z0-9_-]/_}
log=$(mktemp -t "scrib-$label_safe.XXXXXX")
# Single-quoted, so `$log` expands when the trap FIRES rather than being pasted into the
# trap's source now. It is still in scope then, which is what makes the deferred
# expansion both safe and correct — and it is why the SC2064 suppression that used to
# sit here is gone rather than moved.
trap 'rm -f "$log"' EXIT

# `--kill-after` so a command ignoring SIGTERM still dies rather than becoming the hang
# this script exists to prevent.
#
# Wall clock around it, because the exit status alone cannot tell a timeout from a kill
# (F-SEC-208). 137 is "died on SIGKILL" and says nothing about who sent it: `timeout`
# does after the budget, and so does the kernel's OOM killer — or `earlyoom` — after a
# few seconds. Diagnosing the second as the first sends the reader after a wedge that
# never happened, which is the one misdiagnosis this project has a written anti-pattern
# about (GTK4Rs/AP-133).
started=$(date +%s)
timeout --kill-after=60s "$budget" \
    xvfb-run -a \
    dbus-run-session -- \
    env G_DEBUG=fatal-criticals \
    "$@" \
    >"$log" 2>&1
rc=$?
elapsed=$(( $(date +%s) - started ))

cat "$log"

# `$budget` is a `timeout` duration and may carry a suffix; strip it for the comparison,
# and treat an unparseable one as "assume the budget elapsed" — the old behaviour, and
# the conservative direction for a verdict about a hang.
budget_secs=${budget%%[!0-9]*}
if [ "$rc" -eq 124 ] || { [ "$rc" -eq 137 ] && [ -n "$budget_secs" ] && [ "$elapsed" -ge "$budget_secs" ]; }; then
    echo
    echo "$label: NO VERDICT — the command did not finish within ${budget}s and was killed."
    echo "$label: this is a WEDGE, not a failure of the thing under test; the output above"
    echo "$label: is whatever it managed to print. Do not diagnose it from a parallel run"
    echo "$label: (ScrAP-166)."
elif [ "$rc" -eq 137 ]; then
    echo
    echo "$label: NO VERDICT — the command was killed by SIGKILL after ${elapsed}s, BEFORE"
    echo "$label: its ${budget}s budget elapsed. This is NOT a timeout and NOT a wedge:"
    echo "$label: something outside the command killed it. Check 'dmesg | grep -i oom' and"
    echo "$label: 'pgrep -a earlyoom' (GTK4Rs/AP-133) before reading anything above as a"
    echo "$label: result — an OOM kill mid-suite leaves output that looks like a failure."
fi

exit "$rc"
