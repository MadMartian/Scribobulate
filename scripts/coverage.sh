#!/usr/bin/env bash
# Scoped, unit-tests-only coverage gate — POLICY.md § "Build pipeline" step 6 states
# the rule; THIS SCRIPT is the source of truth for the two values it turns on, the
# floor and the scope. POLICY deliberately does not restate either: when the number
# lived in both places it drifted, and the floor sat ~2pt below the real figure,
# silently gating nothing.
#
# FLOOR is a no-regression RATCHET, not a target — `--fail-under-lines` exits
# non-zero when scoped line coverage drops below it. Never lower it to make a run pass.
#
# FLOOR IS A WHOLE NUMBER, AND IT MOVES ONE WHOLE POINT AT A TIME. It rises only once
# measured coverage RELIABLY reaches the next integer — on every host that runs this
# gate, not on whichever machine happened to measure it. Coverage at 76.8 keeps a floor
# of 76; the floor becomes 77 when the figure reaches 77 with room to spare.
#
# This is the settled answer to a defect this file used to have twice over: a floor
# quoted to the second decimal tracked the TESTER as much as the tree (two hosts of the
# SAME platform measured 0.05pt apart, because a test read the ambient config directory),
# and every fractional re-derivation was itself a chance to copy the wrong column. A whole
# number is wider than any residual host-dependence in the scoped set, so it cannot be
# moved by one. ScrAP-123 carries the lesson.
#
# SUB-POINT MOVEMENT IS NOT A FINDING. Coverage drifting by fractions of a percent —
# between hosts, between runs, or across a change — is measurement noise and the
# ordinary consequence of work. Do not adjust FLOOR for it, do not explain it in a
# commit message, and do not raise it with the operator. The event worth reporting is
# the gate going RED: a whole point lost means real coverage was removed.
#
# READ THE RIGHT COLUMN WHEN YOU RAISE IT. `--summary-only`'s TOTAL row prints
# THREE percentages and the one this gate turns on is the THIRD:
#
#   TOTAL   24951  5609  77.52%   1788  371  79.25%   14634  3441  76.49%
#           └─ regions ─┘         └─ functions ─┘     └───── LINES ─────┘
#
# Region coverage leads the row and runs about a point HIGHER, so reading
# left-to-right sets the floor above what the run can ever reach and the gate
# then fails on correct code — which reads as "your change tanked coverage".
# (The old companion rule — round DOWN by 0.01, because the printed figure is
# rounded — is retired by the whole-number floor: a full point of margin swallows
# the rounding it existed to defend against. Reading the right column still matters,
# because regions run about a point higher, which is exactly one ratchet step.)
#
# Usage:
#   scripts/coverage.sh                 # run the gate (summary + fail-under)
#   scripts/coverage.sh --html          # + HTML report (extra args pass through)
#   scripts/coverage.sh --features gtk-integration-tests   # UNSCOPED-ish; note the
#     # floor is defined WITHOUT this feature (unit-only) — don't gate with it on.
set -euo pipefail
cd "$(dirname "$0")/.."

# LOWERED 76.76 -> 76.30 by operator decision. Recording it because the rule above says
# never to, and an unexplained drop is indistinguishable from the silent drift that rule
# exists to catch — so this is the deliberate exception, not the failure mode recurring.
#
# Cause: 6d73875 added two modules whose tests are real but INVISIBLE to this gate —
# src/farscroll.rs and src/saferizer/scrollpos.rs are GTK machinery (idle sources,
# adjustments, scroll calls) exercised by `#[gtktest::test]` bodies behind the
# `gtk-integration-tests` feature, which this unit-only run deliberately does not enable.
# So they read 0% here while being covered in the integration suite. Their uncovered lines
# alone took the total from >=76.76 to 76.20.
#
# What was measured before choosing this (so nobody re-derives it):
#   76.20  as 6d73875 left it
#   76.31  after extracting farscroll's pure decision cores and unit-testing them (15
#          tests, all mutation-verified). Small because test bodies count in the
#          DENOMINATOR too — 58 added lines bought 2 net covered production lines.
#   76.66  measured, farscroll.rs excluded from IGNORE entirely
#   ~76.77 predicted, both modules excluded — would have cleared the old floor untouched
# The exclusion route was offered and not taken; this is the recorded alternative.
#
# RAISED 76.30 -> 76.61 by the Back/Forward within-document navigation work (the feature
# of TDD 23.11-14, the decomposition that followed it, and the two traversal fixes).
# 76.62 printed, rounded down per the rule above. Recorded not because a ratchet step is
# notable, but because the arithmetic is counter-intuitive in BOTH directions and someone
# will otherwise re-derive it — this one change pushed the number down and up repeatedly
# before settling:
#
#   DOWN — it added GTK-wired code this unit-only run cannot reach, exactly the
#     farscroll/scrollpos situation above: ~49 lines across `preview/*` and
#     `winstate/registry.rs` for the feature (-0.26pt on their own), and 13 lines in
#     `src/saferizer/viewport.rs` for the ScrAP-263 allocation gate (-0.06pt), a file that
#     reads 0.00% here (37 lines, 37 missed) while being covered in the integration suite.
#     The gate adds no decision core that could offset itself: the whole of its new logic
#     is one predicate over a GTK rectangle.
#
#   UP, more than paying for all of it — the decision cores went into
#     `winstate/navhistory/` WITH the unit tests that exercise them (99%+ across the tree),
#     and test bodies count in the numerator as well as the denominator. The decomposition
#     raised it again: splitting the pure core into place/record/maintain/decide gave the
#     two judgements the GTK half used to take inline (`departure_stamp`, `traversal_to`) a
#     home where they are unit-testable at all — 14 new tests, both files at 100%.
#
# The exclusion route is NOT taken for any of it, for the reason it was declined above: it
# was offered and refused once, and reversing that quietly inside a feature change would
# make the scope drift on a maintainer's convenience rather than on a decision. The honest
# alternative for the ScrAP-263 gate specifically — contorting its predicate into a pure
# function over an integer so a unit test could reach it — was rejected as writing code to
# satisfy the metric rather than the reader.
#
# The trap that costs an afternoon: those unit tests count ONLY while they live inside a
# `#[cfg(test)] mod tests` in the file itself. Split them into a sibling `navhistory/
# tests.rs` and cargo-llvm-cov stops reporting the file at all (`copymap/tests.rs` is
# likewise absent from the summary) — 132 covered lines silently leave the denominator and
# the gate fails with nothing about the product having changed. Weigh that against the
# 500-line soft limit before splitting a test module out; `preview/scroll.rs` and
# `window/outline_nav.rs` both keep theirs inline for this reason.
#
# RAISED 76.61 -> 76.77 by the ScrAP-264 anchored-child navigation-key repair. 76.78
# printed, rounded DOWN to 76.77 per the rule above — the printed figure is already
# rounded, so 76.78 fails its own measurement. The arithmetic is the shape the note
# above predicts, in the favourable direction for once: the GTK half (`codeview/navkeys.rs`) lands in an
# already-IGNOREd path and so moves nothing, while the whole of the decision — the
# key->movement table and the focus-site rule — went into `src/keynav.rs` WITH its 14
# unit tests, in scope and at 99.26%. That is the extraction the scope rule asks for
# working as intended, not a windfall.
#
# RAISED 76.77 -> 76.86 by the ScrAP-265 fatal-handler test-hygiene fix. 76.87 printed,
# rounded DOWN per the rule above. Recorded only because the direction is the opposite of
# what the note two paragraphs up would predict: the change adds no product code at all —
# it is a test-only RAII guard plus its guard test — and `forensics/` is deliberately IN
# scope (see IGNORE below), so the added test bodies land in the numerator with almost
# nothing new in the denominator. A pure-hygiene change moving the ratchet is not an
# anomaly here; it is what "test bodies count too" looks like when the module is gated.
#
# RAISED 76.86 -> 76.96 by the annotations keyboard-navigation change. 76.97 printed,
# rounded DOWN per the rule above. The direction is again test-weighted rather than
# product-weighted: the walk's decision core (`annotations::step_index`) and the caret
# conversion (`saferizer::byte_offset_at_char`) are both pure and both land in scope
# fully covered, while the GTK halves they replaced — the action bodies, the viewer
# widget, the focus moves — are in excluded files. Extracting the choice out of the
# marker layer is exactly the move the scope rule asks for, so the ratchet moving is
# the mechanism working.
#
# RAISED 76.96 -> 77.01 by the ScrAP-268 SIGTRAP fix. 77.02 printed, rounded DOWN per
# the rule above. Test-weighted again, and for the ScrAP-265 reason two notes up:
# `forensics/` is in scope, the product change is two lines (`SIGTRAP` in the const, its
# name in `signal_name`), and everything else is test bodies landing in the numerator.
#
# Recorded also as a live instance of the READ THE RIGHT COLUMN warning at the top of
# this file, because I walked straight into it: the same run prints 77.81% for REGIONS
# and 77.02% for LINES, and a floor set from the leading figure passes review, passes
# reading, and fails every run thereafter. The two columns being ~0.8pt apart is exactly
# wide enough to look like a plausible ratchet step.
#
# The aspiration is still 80%.
#
# 2026-08-15: 77.01 → 77.42, banked by the document-rename work. The gain is where
# the scope rule says it should be — `docio/rename.rs` (the filename rules and the
# rename primitive) and `winstate/decisions.rs` (the enablement predicate) are pure
# decision cores carrying the feature's real judgement, so the GTK half in
# `window/rename.rs` could stay thin. Read from the LINES column, per the warning
# above: the same run printed 78.29% for regions.

# LOWERED AGAIN 76.30 -> 76.00 by operator decision, and this one is a different KIND of
# lowering from the drop above: not a concession to new uncovered code, but a correction to
# a floor that was never reproducible.
#
# Cause: 76.30 was calibrated against ONE MACHINE. The first CI run of this gate on a
# hosted Linux runner measured 76.26% on the same commit that prints 76.31% here, and the
# whole 0.05pt is src/config.rs: `Config::load()` covers four more lines on the calibrating
# developer's box than on a fresh runner, because it reads the ambient config directory and
# takes a different branch depending on whose home directory it finds. The number was
# tracking the tester as much as the tree.
#
# So "never lower the Linux floor to make another platform pass" (POLICY § Build) is not
# what happened here — this IS Linux, twice, disagreeing with itself. A ratchet whose value
# depends on the host is not a ratchet; it is a number that happens to hold where it was
# set. 76.00 is below both readings with room for the same class of environment sensitivity
# elsewhere, and it is a ROUND figure deliberately: a floor derived to the second decimal
# from one host's run is precisely the false precision that produced this.
#
# The underlying environment dependence in src/config.rs is NOT fixed by this and remains
# worth removing — pinning a config dir the way `.cargo/config.toml`'s `[env]` already pins
# XDG_STATE_HOME would make both hosts take the same branch. Do that and the floor can be
# re-derived honestly and raised. Until then, treat any figure in the low 76s as
# host-dependent rather than as headroom.
#
# RAISE IT AGAIN as soon as farscroll.rs or saferizer/scrollpos.rs gains unit-reachable
# logic — the aspiration is still 80%, and this is a lower starting point for the ratchet,
# not a new normal.

# ── AND THEN THE WHOLE PRECISION ARGUMENT WAS RETIRED ─────────────────────────────────
#
# Every note above this line is history: a ledger of fractional raises and two lowerings,
# each correct on its own terms, and collectively the evidence that the ledger was the
# problem. They are kept because they explain what the scope rule does to the number
# (extracting a decision core RAISES it; adding GTK-wired code LOWERS it), which is still
# worth knowing. Their PRECISION is no longer the standard.
#
# THE RULE NOW: FLOOR is a whole number and moves one whole point at a time, per the
# header and POLICY step 6. Sub-point movement is noise, not news.
#
# MEASURED on this tree, this host: 15737 lines, 3620 missed -> 77.00% LINES (regions read
# 77.79%, functions 79.26% — third column, per the header's warning).
#
# WHY 76 AND NOT 77, since 77.00 does technically "reach" 77: it reaches it by exactly
# ZERO margin, on one host, and the one residual host-dependence left after the config-dir
# pin (theme.rs walking the ambient XDG_DATA_DIRS, measured at 3 lines = ~0.02pt) puts a
# hosted runner at ~76.97. A floor of 77 would therefore go red on CI while nothing was
# wrong — the precise failure this whole-number rule exists to end, re-committed one order
# of magnitude up. "Reliably reaches the next integer" means with room to spare, and 0.00
# is not room. 76 it is, and the ~1pt of headroom is the feature, not slack.
#
# RAISE IT TO 77 when coverage clears 77 with margin on the hosts that run this gate.
# The aspiration is still 80%.

# ── 2026-08-16, THE TWO-BRANCH STANDOFF, RESOLVED BY RE-MEASURING ─────────────────────
#
# Everything above this line is TWO ledgers, not one. `master` and `ci` diverged before
# the whole-number rule existed and each kept writing: `master` ratcheted 76.30 -> 76.61
# -> ... -> 77.01 -> 77.42 and shipped a floor of 77.53; `ci` adopted the whole-number
# rule, measured 77.00 on its own tree, and set 76 for headroom. Both notes are kept
# because both are true about the tree that produced them, and NEITHER value survives the
# merge: 77.53 is a precision this file has retired, and 76 was derived from a tree that
# did not yet contain the rename work's decision cores.
#
# The standoff was NOT resolved by picking a side, which is what made it a standoff --
# under `master`'s rule any move to 77 is a forbidden lowering, and under `ci`'s rule
# 77.53 is not a legal value at all. It was resolved by measuring the MERGED tree, which
# is the only tree either rule was ever meant to describe.
#
# MEASURED on the merged tree, this host: 16529 lines, 3702 missed -> 77.60% LINES
# (regions 78.50%, functions 79.74% -- third column, per the header's warning).
#
# WHY 77 AND NOT 76: 77.60 clears 77 by 0.60pt, and the residual host-dependence this
# file has actually measured -- theme.rs walking the ambient XDG_DATA_DIRS, 3 lines,
# ~0.02pt -- is thirty times smaller than that margin. This is the "with room to spare"
# the rule asks for, and it is the case the ci note above declined to claim at 77.00/0.00
# margin. The rename work's pure decision cores (docio/rename.rs, winstate/decisions.rs)
# are where the 0.60 came from, so the raise is the scope rule working, not drift.
#
# WHY THIS IS NOT A LOWERING of master's 77.53: the two numbers are not on the same
# scale. 77.53 was a floor quoted to the second decimal, which POLICY step 6 retired
# precisely because it tracked the host; 77 is the largest whole number the merged
# measurement supports. Nothing that was covered has become uncovered -- 77.60 measured
# now is ABOVE the 77.42 that note banked.
FLOOR=77

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
#   window/navhistory/  mod (GAction registration + mouse gestures), traverse (page
#                   switch + scroll calls), record (live view reads) → excluded, on
#                   the same terms as its siblings. **This is a restoration, not a
#                   widening**: the identical code was excluded as the single file
#                   `window/navhistory.rs`, and decomposing it into a directory made
#                   the path stop matching `window/[a-z_]+\.rs` — the same reason
#                   `tabs/` and `editbar/` are spelled out above. The split moved its
#                   decisions the other way, into the GATED `winstate/navhistory/`
#                   (`decide.rs`, `place.rs`, both 100%), which is the floor-raising
#                   direction POLICY's scope rule names.
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
IGNORE='src[/\\](window[/\\](tabs[/\\]|editbar[/\\]|navhistory[/\\])?[a-z_]+|app[/\\](appactions|menubar|openbatch|open|setup)|main|lib|gtk_suite|suite_registry|logging|tags|codeview[/\\][a-z_]+|outline_view|preview[/\\]annotate[/\\]overlay|widgets[/\\](table[/\\]mod|tab[/\\](imp|bar|ops|view|mod)))\.rs'

exec cargo llvm-cov --summary-only --fail-under-lines "$FLOOR" --ignore-filename-regex "$IGNORE" "$@"
