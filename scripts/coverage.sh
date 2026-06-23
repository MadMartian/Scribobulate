#!/usr/bin/env bash
# Scoped, unit-tests-only coverage gate — POLICY.md § "Build pipeline" step 6 states
# the rule; THIS SCRIPT is the source of truth for the two values it turns on, the
# floor and the scope. POLICY deliberately does not restate either: when the number
# lived in both places it drifted, and the floor sat ~2pt below the real figure,
# silently gating nothing.
#
# FLOOR is a no-regression RATCHET, not a target — `--fail-under-lines` exits
# non-zero when scoped line coverage drops below it. Raise it in the same change
# that raises coverage; never lower it to make a run pass.
#
# Usage:
#   scripts/coverage.sh                 # run the gate (summary + fail-under)
#   scripts/coverage.sh --html          # + HTML report (extra args pass through)
#   scripts/coverage.sh --features gtk-integration-tests   # UNSCOPED-ish; note the
#     # floor is defined WITHOUT this feature (unit-only) — don't gate with it on.
set -euo pipefail
cd "$(dirname "$0")/.."

FLOOR=75.55

# IGNORE — the scope. Excluded: GTK signal-wiring that cannot be exercised
# headlessly (including it would make the number meaningless). Included, always:
# pure decision logic. Every entry below is one half of a deliberate split — the
# GObject/wiring half is excluded, the pure half beside it is gated, which is why
# extracting a decision core out of an excluded file is what raises the floor:
#
#   app/            appactions, menubar, open, openbatch, setup are GTK-wired →
#                   excluded; mnemonics (access-key logic) and commands (const
#                   descriptor tables) are pure → gated. `openbatch.rs` was carved
#                   out of the already-excluded `setup.rs` and is the same window/tab
#                   construction code under a new name, so it inherits the same
#                   decision — the document READS it now defers are in `docio`, which
#                   is gated and near-fully covered.
#   widgets/table/  mod.rs is GObject glue → excluded; layout.rs (column-fit and
#                   cell-placement arithmetic) → gated.
#   widgets/tab/    mod, imp, bar, ops, view are GObject/façade → excluded;
#                   layout.rs (gutter reservation, hit-testing, reorder index,
#                   scroll reveal, easing) → gated.
#   window/tabs/    switch, lifecycle, actions, dnd, contextmenu, documents, mod —
#                   all tab-lifecycle wiring → excluded via the `window/(tabs/)?` term.
#   preview/        annotate.rs (selection→source mapping, entry-card placement
#                   math) is pure → gated; annotate/overlay.rs (GtkPopover/GtkOverlay
#                   wiring) → excluded.
#   forensics/      deliberately NOT excluded, including signal.rs. It reports ~43%
#                   and is nonetheless fully driven: the fatal-signal handler is
#                   exercised end to end by a forked child that dies from the signal
#                   it is handling, and a process killed by a signal never flushes
#                   its coverage counters — so the missing lines are an artefact of
#                   how the test must work, not untested code. Excluding it would
#                   trade a small, explainable dent for a blind spot in the one
#                   module whose failure mode is silence.
#   codeview/       mod (GObject subclass + snapshot chip painting), geometry
#                   (line/cell buffer-Y reads), markers (popover UI) are all
#                   view-bound → excluded. The one pure piece, group_by_line, keeps
#                   its unit tests but rides along inside the excluded markers.rs.
#
# `gtk_suite` and `suite_registry` are the main-thread GTK suite's own plumbing (a
# second crate root and the registry `#[gtktest::test]` submits into). Excluding
# test infrastructure from a coverage figure
# about the product is not a concession; counting a test runner's own lines would move
# the number without saying anything about the code under test.
#
# `main|lib` is ONE exclusion, not two: `src/lib.rs` is the crate root and holds the
# startup sequence + `GApplication` construction that used to live in `src/main.rs`
# (the library split — `main.rs` is now three lines of delegation). It is the same
# GTK-wired, headlessly-unexercisable code under a new name, so it keeps the same
# scoping decision. Both names stay listed because both files still exist.
#
# Every separator below is the class `[/\\]`, not a literal `/`, because llvm-cov
# reports paths in the host's native form — `src\window\toolbar.rs` on Windows. With
# a `/`-only regex NOTHING matches there, so the scope silently evaporates and the
# gate compares the FLOOR against the UNSCOPED total (37.9% vs 71.7%) and fails every
# run. That reads as "your change tanked coverage" rather than "the filter missed",
# which is the worst way for a gate to break — keep the class if you edit this.
IGNORE='src[/\\](window[/\\](tabs[/\\]|editbar[/\\])?[a-z_]+|app[/\\](appactions|menubar|openbatch|open|setup)|main|lib|gtk_suite|suite_registry|logging|tags|codeview[/\\][a-z_]+|outline_view|preview[/\\]annotate[/\\]overlay|widgets[/\\](table[/\\]mod|tab[/\\](imp|bar|ops|view|mod)))\.rs'

exec cargo llvm-cov --summary-only --fail-under-lines "$FLOOR" --ignore-filename-regex "$IGNORE" "$@"
