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
# READ THE RIGHT COLUMN WHEN YOU RAISE IT. `--summary-only`'s TOTAL row prints
# THREE percentages and the one this gate turns on is the THIRD:
#
#   TOTAL   24951  5609  77.52%   1788  371  79.25%   14634  3441  76.49%
#           └─ regions ─┘         └─ functions ─┘     └───── LINES ─────┘
#
# Region coverage leads the row and runs about a point HIGHER, so reading
# left-to-right sets the floor above what the run can ever reach and the gate
# then fails on correct code — which reads as "your change tanked coverage".
# Also round DOWN by 0.01: the printed figure is rounded, so a floor set to the
# displayed value fails against the unrounded one (76.49% printed, 76.48 here).
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
# 2026-08-17: 77.53 → 77.72, banked by the remote-image HTTP fetch (ScrAP-292). The run
# PRINTS 77.73 and a floor of 77.73 FAILS it — the printed figure is rounded up from
# something a hair under, so the rule above ("round down") is not a style preference, it
# is the difference between a gate that passes and one that fails on the very run that
# set it. Worth
# recording because the change first pushed the number DOWN, to 77.49, and failed this
# ratchet 2026-08-18 (77.72 -> 77.75): the code-block copy button's placement and the
# one hit test every drawn affordance shares were extracted out of the excluded
# `codeview/` tree into the gated `affordance.rs` and unit-tested there, which is the
# floor-raising direction POLICY's scope rule names. Worth recording WHY the extraction
# happened: the button's GTK wiring landed in the in-scope `preview/interactions.rs`
# (0% covered, like every preview wiring file) and pushed the total 0.08pt UNDER the
# floor. Widening IGNORE to cover those files would have "fixed" it by hiding them;
# moving the decidable half into scope fixed it by testing more code.
# gate: replacing a one-line GIO call with a module the unit run could not reach added
# uncovered lines in both `imagefetch.rs` and the renderer's loader. What recovered it —
# and then some — was making the fetch testable OFFLINE rather than declaring it
# untestable: the cap became a parameter so a refusal could be driven over a real
# response, and the tests serve their own canned HTTP from a loopback socket. The rule
# to carry: "needs the network" is usually "needs A network", and a loopback server is
# one, so reach for that before writing the exclusion. Read from the LINES column, per
# the warning above: the same run printed 78.63% for regions.
# ratchet 2026-08-18 (77.75 -> 77.78), and DELIBERATELY SHORT OF THE MEASUREMENT (77.81).
# The previous bump set the floor to the exact figure the run produced, which left zero
# headroom -- and the very next change, two lines of GTK signal wiring in an in-scope file,
# failed the gate by 0.01pt while adding no untested logic at all. A gate that fails on
# arithmetic noise is a gate that pressures the next author into widening IGNORE, which is
# precisely the failure ScrAP-294 records. So the ratchet takes the gain and leaves a
# margin: it still only ever moves up, and it still fails a real regression, but it does not
# manufacture one. Do not "tidy" this up to the measured value.
#
# ratchet 2026-08-18b (77.78 -> 77.85), measurement 77.88, same deliberate margin. The
# gain is the DocMonitor seam (ScrAP-297) arriving with its own unit tests rather than
# with an IGNORE entry: the seam is thin GIO wiring and would have been excludable on
# the scope rule, which is exactly the reasoning ScrAP-294 warns produces a number that
# measures less code every time it moves.
#
# ratchet 2026-08-19 (77.85 -> 79.50), measurement 79.57, same deliberate margin. The
# gain is the export feature (TDD §25) landing display-free: the model, both sinks and
# the paginator are pure, and the two decision cores that would otherwise have hidden
# behind the `src/window/` exclusion — the default-name guarantee and the PDF promote
# gate — were moved into `src/export/` rather than left there. The largest single jump
# is `export/pdf.rs`, 12.57% -> 94.77%, from testing the Pango layout and the cairo draw
# against a font-map context and an `ImageSurface`: a sink that touches the toolkit is
# still reachable headlessly, and "it needs GTK" is not on its own a reason to exclude
# a file — the question is whether a DISPLAY is needed, and here it is not.
#
# ratchet DOWN 2026-08-20 (79.50 -> 79.00), operator's decision. The line above records a
# measurement of 79.57 behind the 79.50 ratchet; that figure does not reproduce. Measured
# on this canonical platform at `1a19546` and at every commit since: **79.31%**. So the
# ratchet was set roughly 0.2pt ABOVE what the tree achieves, and step 6 was red on the
# only platform that runs it from the moment the export feature landed — macOS and Windows
# both contract-declare the step not-applicable, so neither seat was a witness to it.
# A ratchet no tree state can satisfy is not a ratchet, it is a permanently red gate, and a
# permanently red gate teaches people to skip the step. Corrected to 79.00, which restores
# the deliberate margin below the real measurement rather than pretending to a number that
# was never met. This is NOT the prohibited move of lowering the Linux floor to make
# another platform pass: it is bringing a mis-set value back to the evidence.
#
# 2026-08-20, 79.40 -> 79.60. The PDF sink's table renderer moved its column-width
# arithmetic into `export/pdftable.rs`, a display-free module the suite can execute, and
# it lands at 99.51% (204 lines, 1 missed) — the extraction-raises-the-floor mechanism
# POLICY's scope rule describes, doing exactly that. Measured total 79.93%; a
# counterfactual with `pdftable.rs`'s lines removed is 79.73%, so the pre-change baseline
# was at or below that and the gain is real rather than a reshuffle. Raised to 79.60 and
# not to 79.93: the margin is what absorbs the ordinary drift of unrelated work, and a
# ratchet pinned to the last measurement is the permanently-red gate the paragraph above
# was written about.
FLOOR=79.60

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
#   clipboard.rs    a `GdkContentProvider` GObject subclass plus two signal handlers
#                   (`copy-clipboard`, `cut-clipboard`) and a realize/unrealize
#                   rebalance — GTK wiring end to end, with no decision core to
#                   extract: its one branch is `selection_bounds().is_some()`, which
#                   is a buffer read rather than logic. It is NOT untested — all four
#                   of its behaviours are driven by `#[gtktest::test]` bodies in
#                   pipeline step 5, including a mutation-checked assertion that a
#                   same-application paste arrives as exactly ONE `insert-text`
#                   emission (6 with the override removed). Those run under the
#                   `gtk-integration-tests` feature, which this gate deliberately does
#                   not enable, so counting the file here would report 0% for code the
#                   suite exercises thoroughly.
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
IGNORE='src[/\\](window[/\\](tabs[/\\]|editbar[/\\]|navhistory[/\\])?[a-z_]+|app[/\\](appactions|menubar|openbatch|open|setup)|clipboard|main|lib|gtk_suite|suite_registry|logging|tags|codeview[/\\][a-z_]+|outline_view|preview[/\\]annotate[/\\]overlay|widgets[/\\](table[/\\]mod|tab[/\\](imp|bar|ops|view|mod)))\.rs'

exec cargo llvm-cov --summary-only --fail-under-lines "$FLOOR" --ignore-filename-regex "$IGNORE" "$@"
