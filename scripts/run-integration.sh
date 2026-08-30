#!/usr/bin/env bash
#
# Build-pipeline step 5 (Linux): the GTK integration suite.
#
# A helper rather than a contract one-liner for the same reason step 6 is
# `scripts/coverage.sh`: the step owns three things a single command line cannot express,
# and each of them is load-bearing.
#
#
# 1. IT ARMS `dbus-run-session`, WHICH POLICY HAS ALWAYS PRESCRIBED AND NOTHING RAN.
#
# In a session with no accessibility bus — every agent session, and any session whose
# `at-spi-dbus-bus.service` has gone stale — GTK emits `Unable to connect to the
# accessibility bus` as a `Gtk-CRITICAL`, and this step's own `G_DEBUG=fatal-criticals`
# promotes it to a SIGTRAP before a single test runs. The failure names accessibility, is
# not about accessibility, is not about the change under test, and reproduces identically
# on an untouched tree, so it costs a control build to disbelieve every time.
#
# POLICY § Build pipeline has said to run this step under `dbus-run-session` since that was
# measured. The contract's command did not, so the instruction lived only in prose and each
# caller supplied it by hand or got the SIGTRAP. That is precisely the failure the step's
# own contract comment records about `G_DEBUG=fatal-criticals` — a prescribed gate with
# nothing in the toolchain arming it — committed a second time, one line below.
#
#
# 2. IT KEEPS BUS-ACTIVATED DAEMONS OFF THE CALLER'S STDOUT.
#
# ⚠ Without this the pipeline HANGS A READER FOREVER ON A PASSING RUN, which is the worst
# shape a gate can fail in: a green run and a wedged one are indistinguishable from
# outside. MEASURED 2026-08-28 — `scripts/pipeline.sh | tail -45` sat for 70 minutes on a
# pipeline that had finished in about 12.
#
# The mechanism: `dbus-run-session` starts a private bus, and the suite's GTK activity
# activates a crowd of services on it — reproduced in isolation as portal.Desktop,
# portal.Documents, PermissionStore, portal-{gnome,kde,gtk}, gvfs, org.a11y.Bus,
# atspi.Registry and secrets. The private `dbus-daemon` forks each one, so every one
# inherits this process's stdout and stderr. They outlive the bus (systemd --user reaps
# them as subreaper), so the write end of the caller's pipe stays open after the pipeline
# exits, and any reader waiting for EOF waits forever. `timeout` does not help: the command
# already finished.
#
# The fix is to hand the daemons a FILE instead of the caller's pipe. Everything below runs
# with its output redirected to `$log`, which is emitted here once the command is done — so
# a daemon that lingers holds a descriptor on a temp file nobody is waiting on, and this
# script's own exit closes the caller's pipe normally.
#
# Do NOT "simplify" this into a direct `dbus-run-session -- cargo test`. The redirect IS the
# fix, and its absence is invisible until someone pipes the pipeline. Chasing the daemons
# individually (`GTK_USE_PORTAL=0` and friends) is whack-a-mole: the list above is nine
# services deep and grows with the desktop, and one missed entry restores the hang.
#
#
# 4. THE NESTING ORDER IS LOAD-BEARING: `xvfb-run` OUTSIDE, `dbus-run-session` INSIDE.
#
# ⚠ Inverted — which is how this ran until it was measured — the private bus is started
# BEFORE the display exists, so it inherits the ambient `DISPLAY`. Every service it
# activates then inherits that too, and the crowd from note 2 connects to the DEVELOPER'S
# REAL X SERVER instead of the Xvfb this step exists to isolate. `xvfb-run` only rewrites
# `DISPLAY` for the command it wraps, so wrapping `cargo test` alone isolates the tests and
# nothing else.
#
# MEASURED 2026-08-30 on the reference host: inverted, one run made ~20 connections to the
# live `:0` and printed `qt.qpa.xcb: could not connect to display :0` twenty-one times
# alongside `Maximum number of clients reached`; the session's X server was at 251 of
# X.org's 256-client default, so the step aborted under `G_DEBUG=fatal-criticals` with a
# `SIGTRAP` naming the accessibility bus — note 1's failure, arriving by a different road
# and immune to note 1's fix. In this order: zero contacts with `:0`, zero refusals, 1682
# tests green.
#
# IT FAILS FAVOURABLY, which is why it survived. On a desktop with client slots to spare
# the leak is invisible and every test passes; the step only goes red once the developer's
# own session is full, at which point it reports a fault in the accessibility bus. So a
# GREEN RUN IN THE INVERTED ORDER WAS NEVER EVIDENCE OF ISOLATION — it was evidence that
# the machine had room. An isolation boundary has to enclose everything that inherits the
# environment, not just the process under test.
#
#
# 3. IT BOUNDS THE RUN.
#
# A wedged GTK suite is a real failure mode (ScrAP-166 is a whole entry about misdiagnosing
# one), and an unbounded step turns it into a run nobody can tell from a slow one. The
# budget is deliberately generous — this suite takes about two minutes on the reference
# host — because the timeout exists to catch a WEDGE, not to police duration. A timeout is
# reported as its own distinct verdict rather than as a test failure, because "the suite
# said no" and "the suite never answered" are different findings and must not print the
# same way.
set -uo pipefail

# Generous: ~10x the reference host's runtime. Raise it rather than let a slow machine
# learn to distrust this gate.
BUDGET="${SCRIB_INTEGRATION_BUDGET:-1200}"

log=$(mktemp -t scrib-integration.XXXXXX)
# shellcheck disable=SC2064  # $log is expanded now, deliberately: the trap must name it.
trap "rm -f '$log'" EXIT

# `--kill-after` so a suite ignoring SIGTERM still dies rather than becoming the hang this
# script exists to prevent.
timeout --kill-after=60s "$BUDGET" \
    xvfb-run -a \
    dbus-run-session -- \
    env G_DEBUG=fatal-criticals \
    cargo test --features gtk-integration-tests \
    >"$log" 2>&1
rc=$?

cat "$log"

if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
    echo
    echo "integration: NO VERDICT — the suite did not finish within ${BUDGET}s and was killed."
    echo "integration: this is a WEDGE, not a test failure; the output above is whatever it"
    echo "integration: managed to print. Do not diagnose it from a parallel run (ScrAP-166)."
fi

exit "$rc"
