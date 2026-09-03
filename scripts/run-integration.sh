#!/usr/bin/env bash
#
# Build-pipeline step 5 (Linux): the GTK integration suite.
#
# A helper rather than a contract one-liner because the step needs a throwaway GTK
# session — a private display, a private bus, criticals fatal, output that cannot wedge a
# reader, and a wall-clock bound — and none of that fits on a command line. All five now
# live in `scripts/gtk-run.sh`, which is where their measurements are written up, because
# step 6's full-suite coverage leg needs exactly the same session. Two hand-written copies
# of that discipline would be one copy right on the day it was written: the nesting order
# in particular fails favourably, so a green run in the wrong order proves nothing.
#
# What stays HERE is what is specific to this step: which command satisfies it, and how
# long a wedge is allowed to look like slowness.
set -uo pipefail

# Generous: ~10x the reference host's runtime. Raise it rather than let a slow machine
# learn to distrust this gate.
BUDGET="${SCRIB_INTEGRATION_BUDGET:-1200}"

exec "$(dirname "$0")/gtk-run.sh" integration "$BUDGET" \
    cargo test --features gtk-integration-tests
