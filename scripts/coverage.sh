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
# THE GATE HAS TWO VERDICTS AND THEY ARE NOT INTERCHANGEABLE. The SCOPE verdict comes
# first: the set of files being measured is recorded in `scripts/coverage.scope` and
# compared on every run, because a change in WHAT IS MEASURED used to arrive disguised as
# a change in the percentage — see that file's header for the mechanism and the three
# times it happened. Only if the scope is unchanged is the FLOOR verdict rendered at all;
# a ratchet compared across two different scopes measures nothing.
#
# Usage:
#   scripts/coverage.sh                 # run the gate (summary + scope + fail-under)
#   scripts/coverage.sh --html          # + HTML report (extra args pass through)
#   scripts/coverage.sh --update-scope  # rewrite scripts/coverage.scope from this run,
#     # then carry on to the floor verdict. Consumed here, not passed to cargo.
#   scripts/coverage.sh --features gtk-integration-tests   # UNSCOPED-ish; note the
#     # floor is defined WITHOUT this feature (unit-only) — don't gate with it on.
set -euo pipefail
cd "$(dirname "$0")/.."

# LOWERED 82 -> 80 by operator decision, and it is the SAME cause as the 76.76 -> 76.30
# entry below — which is itself the reason to record this one rather than treat a second
# instance as routine. If this keeps happening, the gate is the thing to fix, not the
# floor.
#
# Cause: wiring the disclosure fold splice to the live toggle added ~433 lines of
# GTK-wired production code — `preview/splice/install.rs`, `farscroll/settle.rs`,
# `window/foldsplice.rs` — un-gated from `#[cfg(test)]` because they now have a
# production caller. They are exercised by `#[gtktest::test]` bodies behind the
# `gtk-integration-tests` feature, which this unit-only run deliberately does not enable,
# so they read 0% here.
#
# What was MEASURED before choosing this (so nobody re-derives it):
#   82.09  branch tip before the wiring
#   80.65  after it, unit-only — this run
#   94.84  the SAME tree with `--features gtk-integration-tests`
# That last figure is the point: the code is not untested, and this gate cannot see
# fourteen points of it. The prescribed remedy was applied as far as it honestly goes —
# `keep_survivors`, `offset_below_viewport_top` and `restored_value` were extracted as
# pure cores with unit tests — and recovers a fraction of a point, not 1.4.
#
# DELIBERATELY NOT DONE HERE: switching this gate to compile the GTK suite. That is
# arguably the right long-term answer, since a gate blind to fourteen points of tested
# code is measuring the wrong thing. But it changes the measured SCOPE (verified: the
# run above withheld its verdict for exactly that reason), needs the floor recalibrated
# from 80 to near 94, needs a display, and redesigning a required gate inside the change
# that gate is currently failing is how a gate ends up calibrated to its own fixture.
# RAISED WITH THE OPERATOR AS SEPARATE WORK AND DECLINED — this gate stays unit-only.
# So the note above stands as the standing answer rather than as a deferral: when this
# cause forces a floor drop again, the choice is another recorded drop or a different
# remedy, NOT this one. Do not re-propose it as though it had merely gone unconsidered.
#
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
#
# 2026-08-21, 79.60 -> 79.70, AND the correction of a ratchet this log never recorded.
# QA round 1 (F-GATE-007, found by three reviewers independently) established that
# `b890c9c` moved FLOOR 79.00 -> 79.40 in the SAME commit that added `clipboard` to
# IGNORE below, with no entry here. Re-measured on this machine: the clipboard exclusion
# is worth +0.40pt (79.64% with the file in scope, 80.04% with it out), which is the
# whole of that move. So the floor did not rise against a constant scope — the SCOPE
# narrowed and the number followed it, which is the one thing a ratchet must never be
# allowed to do silently. A floor that climbs by exclusion measures nothing and reports
# progress. The value is not being rolled back (the exclusion is still correct, and its
# rationale below is now argued honestly rather than from an undercount); what is being
# corrected is the RECORD, because the next author reading "79.00 -> 79.40" would
# otherwise credit it to tests that were never written.
#
# The +0.10 to 79.70 is a real gain and is separable from the above: `primarysel.rs`
# extracted the PRIMARY-selection decision out of `clipboard.rs` (QA F-CLIP-001), which
# is the extraction-raises-the-floor mechanism POLICY's scope rule describes. Measured
# total 80.04%; the deliberate margin below the measurement is kept, for the reason the
# 2026-08-20 entry above gives.
#
# 2026-08-22, 79.70 -> 79.75, finding M34: `pdftable.rs`'s single-example cross-check
# against `widgets::table::layout::fit_columns` became a sweep over many shapes/bounds,
# and both files gained an invariant sweep of their own (floor, bound, determinism). No
# code moved between files — this is test-only, on already-scoped, already-covered
# modules — so the gain is small: measured 80.14% before, 80.22% after. Raised by half
# the gain rather than to the new measurement, keeping the deliberate margin the
# 2026-08-20 entry explains.
#
# 2026-08-22, 79.75 -> 79.95, finding L29: `export/pdf.rs` (1917 lines, ~4x the POLICY
# soft limit) became `export/pdf/`, and the split was made along the toolkit boundary
# rather than by line count. Two of the new modules need NO toolkit at all -- `geometry`
# (page arithmetic: where an indented block starts, how wide it really is) and `decide`
# (list markers, heading-scale index, column count, image splitting) -- and both had
# been unreachable from a unit test, because the only route to them was to build a
# document, build a Pango context, and run the whole measurement pass. They now measure
# 100.00% and 93.15% on tests that need no display. This is exactly the
# extraction-raises-the-floor mechanism the scope rule below describes, and the second
# recorded instance of it after `primarysel.rs`. Measured 80.22% before, 80.64% after.
# Raised by half the gain, keeping the margin the 2026-08-20 entry explains.
#
# 2026-08-25, 79.95 -> 80.20, TDD 18.21-18.23 (the Phase 1 decoration keys). No code
# moved between files this time — every gain is test-side, on already-scoped modules:
# `theme/` picked up the per-level heading fold, the line-style vocabulary and their
# clamp/merge/floor cases; `export/html.rs`, `export/markup.rs` and `preview/css.rs`
# each gained sink tests for the new keys, including two that assert the generated Pango
# markup PARSES rather than merely spelling right. Measured 80.97% before (at `35aab05`,
# in a clean worktree), 81.46% after. Raised by half the gain, per the 2026-08-20 entry.
# Worth noting for whoever reads this next: the floor was already ~1pt behind the tree
# at 79.95, and this entry does not close that gap — closing it is a deliberate decision
# about how much margin the ratchet should carry, not something a feature commit should
# make on the way past.
#
# 2026-08-26, 80.20 -> 80.30, TDD 18.26 (depth-tiered bullet colour/glyph/sprite, plus the
# PDF marker-ink prerequisite). Test-only again, on already-scoped modules: `theme/`
# picked up the tier map and the shallower-tier fallback cases, `codeview/gutter.rs` the
# per-depth substitution and ink, `export/pdf/decide.rs` the marker ink and the per-depth
# arms, and `export/html.rs` the depth-scoped selectors. Measured 81.45% before (at
# `ed0f7c3`, in a clean worktree), 81.67% after. Raised by half the gain, per the
# 2026-08-20 entry above. The ~1pt standing margin the 2026-08-25 entry names is
# unchanged and still deliberate.
#
# 2026-08-26, 80.30 -> 80.33, TDD 18.27/18.28 plus 18.25's band-padding fix. The SMALLEST
# move this log records, and deliberately made rather than skipped: the work added about
# as much code as it did test (three decorations across the theme model, the gutter and
# both sinks), so measured 81.68% before (at `51caea6`, clean worktree) and 81.74% after.
# Half the gain is +0.03. Skipping a move because it is small is how a ratchet quietly
# stops tracking; the ~1.4pt standing margin the 2026-08-25 entry names absorbs it either
# way.
#
# 2026-08-27, 80.33 -> 81.45, QA round 1's mitigations (TDD 18.36-18.44, 2.25, 25.9's two
# new clauses) — and the entry that closes the standing margin rather than adding to it.
#
# THREE things, because this entry answers a finding rather than logging a feature:
#
#   1. THE 0.9pt DROP THE LOG NEVER RECORDED. The previous entry claims 81.74% at
#      `51caea6`. MEASURED at `7f6b09d` — the branch tip this round started from, in a
#      clean worktree — the tree was at 80.85%, and the gate stayed green the whole way
#      because the slack absorbed it. The three commits between them (`f61a7fa`'s
#      panel/header/rule, `595d517`'s registry rewrite, `7f6b09d`'s module split) added
#      about 780 gated lines and their tests did not keep pace. Nothing was wrong with
#      the gate; the gate simply was not tracking, which is exactly what the 2026-08-26
#      entry above warns a small skipped move leads to.
#
#   2. THE MARGIN IS NOW ~0.3pt, DELIBERATELY. 1.4pt of a ~23k-line gated scope is
#      roughly 320 lines that can evaporate with the gate green — larger than most
#      changes it is meant to gate, and `src/theme/` alone is ~2,300 new lines this
#      branch. A gate whose margin exceeds the size of the change it gates is not gating
#      that change. This supersedes the "raise by half the gain" convention for this one
#      move: half-the-gain is a rule for keeping headroom, and the headroom had become
#      the problem. Future entries can go back to it from here.
#
#   3. A CONST-EVALUATED CONSTRUCTOR SCORES ZERO, AND THAT IS AN INSTRUMENT ARTEFACT.
#      `theme/keys.rs` fell from 100% to 74.5% purely because `Reach`'s and `Bound`'s
#      `const fn` constructors are evaluated inside the `keys!` table at COMPILE time, so
#      llvm-cov sees no run-time execution and scores every line of them zero. They are
#      exercised at every build. The answer was to call them from a unit test that also
#      asserts the shape each one builds — a `preview_only` that quietly set `pdf: true`
#      would silently licence a key on a surface it never reaches — which makes the
#      number honest AND adds a guard. Worth knowing before someone reads a similar drop
#      as untested code.
#
# Where the gain came from: `theme/decor.rs` and `pangospan.rs` are new and both 100%;
# `theme/resolve.rs` and `theme/model.rs` are 100%; `theme/keys.rs` 98.4%;
# `export/pdf/ink.rs` 71.9% -> 87.1% and `export/pdf/measure.rs` 95.1% -> 99.2% from the
# sprite-paint and PDF-key tests. `codeview/`'s exclusion needed no narrowing after all:
# `marker_substitute` — the one pure decision function the exclusion was swallowing —
# moved OUT of `codeview/gutter.rs` into `theme/decor.rs` as part of the sprite-seam
# work, which is precisely the extraction POLICY step 6 describes as the mechanism by
# which the floor rises. Measured 81.78% after, at a clean worktree.
#
# RAISED 81.45 -> 82.15 by QA round 1's Medium mitigation batch. 82.16 printed in the
# LINES column, rounded DOWN per the rule at the top — and the rule's own warning was
# earned again here: the first attempt read 82.68 off the REGIONS column, which leads
# the row, and set a floor no run could ever reach. The gain is the extraction rule working
# as designed rather than a windfall — three decision cores came OUT of files this scope
# excludes and landed with their unit tests:
#
#   * `window/find/plan.rs` — the preview find highlight's hit->attribute mapping, which
#     was interleaved with GTK mutation inside `window/find.rs` (excluded), so neither
#     the suite nor this gate could see it. NOTE the path: `window/<name>.rs` is
#     excluded and `window/find/<name>.rs` is NOT, because the IGNORE pattern names its
#     three excluded subdirectories explicitly.
#   * `widgets/mod.rs`'s `tile_texture`/`draw_sprite_into` — one seam replacing three
#     open-coded copies in `codeview/` (excluded).
#   * `cssfrag.rs` — the fragments the preview sheet and the HTML sink genuinely share.
#
# The rest is test bodies in already-scoped modules: `theme/tests/diagnostics.rs`,
# `theme/tests/registry.rs`, `palette/tests.rs` (the palette split its tests out at the
# 500-line limit; both halves stay in scope) and the sprite/PDF guards.
#
# `tags/spec.rs` is the last extraction of the batch — the theme->tag decisions came out
# of `tags.rs`, which this scope EXCLUDES, so the ink floor's condition and the band
# inset's per-level gate went from unassertable-without-a-view to five unit tests.
#
# 82.15 -> 82.18 with TDD 25.25's named-face guard. The rise is +0.01 and the other
# 0.02 was the floor being stale, and the two are recorded separately on purpose: this
# figure was measured on the parent commit as well (82.17 Lines, in a clean worktree)
# rather than inferred from "the change looks coverage-neutral", so the ratchet is
# closing a known gap rather than quietly crediting one change with another's ground.
#
# 82.18 -> 82.21 with TDD 18.46's shadowed-key diagnostic. Measured 82.18 Lines on the
# parent commit in a clean worktree and 82.24 after, so the gain is +0.06 and real: the
# predicate (`Key::bare_shadow`) and the sweep over it (`Themes::warn_on_shadowed_keys`)
# are pure decision logic in already-scoped files, plus their guards in
# `theme/tests/diagnostics.rs`. No code moved between files and the scope is untouched,
# so this is not a floor climbing by exclusion. Raised by half the gain rather than to
# the measurement, keeping the deliberate margin the 2026-08-20 entry explains.
#
# 82.21 -> 82.31 with the `snapshot_layer` decomposition (F-GOD-001's deferred half).
# Measured 82.25 Lines on the parent commit in a clean worktree and 82.38 after, so the
# gain is +0.13 and real. Raised by roughly half the gain, per the 2026-08-20 margin.
#
# This is the scope rule working exactly as step 6 describes it, and worth recording
# BECAUSE the arithmetic looks wrong at first glance: the change ADDS seven files under
# `codeview/`, every one of them excluded here, and the number still went up. The reason
# is that nothing was moved INTO the exclusion — the 774-line draw callback was already
# inside it, so its painters cost this gate nothing on the way out. What came out to the
# other side is `src/decorplan.rs` (98% Lines, 22 unit tests): the paint's ordered step
# list, the viewport gates, the heading band's corner-radius clamp and level slot, the
# copy button's reveal rule, and the pending-popover precedence between expiry, the
# scroll landing and the chip painting. None of those could be asserted at all while
# they lived in a draw callback; all of them can now be asserted without a display.
#
# `codeview/`'s exclusion needed no narrowing and got none. The new painters are flat
# files directly under it, so they match the existing `codeview[/\\][a-z_]+` term as
# written — deliberately, since a `codeview/paint/` DIRECTORY would have stopped matching
# and silently pulled seven view-bound files into scope at 0%, which is the trap the
# `tabs/`, `editbar/` and `navhistory/` entries above were each written after hitting.
#
# THAT REASONING IS NOW A MACHINE CHECK rather than an author's care. `scripts/coverage.scope`
# records the measured set, and the SCOPE verdict below fails on any drift in it, by name,
# before this gate says anything about the percentage. Keeping the new painters flat was
# still the right call — but it is no longer the only thing standing between a new
# subdirectory and a silent scope change, which is what the four warnings above were each
# trying and failing to be.
#
# -- 2026-08-30, THE MERGE OF `master` INTO `ci`: ONE RULE OVER TWO LEDGERS ------------
#
# Both tails above this line are kept, because between them they carry the whole
# scope-rule argument. They disagree on FORM, not on direction: `ci` retired
# second-decimal floors and wrote the whole-number rule into POLICY step 6; `master`
# never saw that rule and went on ratcheting in hundredths, each step deliberately short
# of its own measurement for the same reason the whole-number rule exists -- a floor
# pinned to the last run fails on arithmetic noise.
#
# THE MERGE SETTLES IT ON THE WRITTEN RULE. POLICY step 6 is the only place either
# discipline is stated as a rule, it merged without conflict, and a script that
# contradicts it is the second copy this file's own header warns about. So FLOOR is a
# whole number here.
#
# THIS IS NOT A LOWERING of 82.75, and the test is whether any covered line became
# uncovered: none did. 82.75 and 82 are not on the same scale -- one is a
# hundredth-precision floor the rule retired, the other is the largest whole number the
# merged measurement supports. Note that 82.75 was itself ABOVE every measurement the
# entries above record (the last is 82.38), which is the failure mode the rule names:
# a floor half a point past the tree, held green only by the margin convention it was
# meant to replace.
#
# MEASURED on the merged tree, this host: 24799 lines, 4232 missed -> 82.93% LINES
# (regions 83.34%, functions 84.93% -- third column, per the header's warning). 82.93
# clears 82 by 0.93pt, where it cleared master's 82.75 by 0.18 -- and 0.18pt is inside the
# residual host-dependence this scope still carries (the theme search path reads
# $XDG_DATA_DIRS, which a hosted runner advertises differently from a developer box), so
# the old floor was one runner away from a false red. ScrAP-123 holds the lesson.
#
# THE SCOPE ALSO MOVED, and it is recorded rather than absorbed. `ci` brings two files
# into the measured set -- `src/notices.rs` (84.31%) and `src/testtiming.rs` (83.33%) --
# both test-support modules that belong IN scope on the same footing as `testlog.rs` and
# `testsymlink.rs`, which were already there. Neither was excluded to protect the number:
# both are above the floor they joined. 146 files measured, re-pinned in
# scripts/coverage.scope by `--update-scope` in this same commit.
#
# -- 2026-08-31, THE THEME RESIDUAL IS CLOSED, AND IT WAS MEASURED --------------------
#
# The caveat every note above carries -- "the theme search path reads $XDG_DATA_DIRS,
# which a hosted runner advertises differently from a developer box, ~3 lines, ~0.02pt"
# -- is no longer true of this tree, so the margin arithmetic above no longer has to
# reserve room for it. The directory list became a data seam (`theme::SearchBases` and
# the pure `candidates()` over it): the LOOP that varied with the host now runs in
# deterministic unit tests over temp directories, and `from_env()` is a fixed read whose
# line count no host can move.
#
# MEASURED by the same two-run differential that first exposed the config-dir leak:
# `cargo llvm-cov --lib --json` twice on this tree, XDG_DATA_DIRS=/usr/share (one entry)
# against this box's own seven-entry list. 37990 lines, 21594 covered, and covered-line
# counts IDENTICAL in every file the run measured -- where the config leak showed four
# lines of difference. Note that is the unscoped `--lib` run, not this gate's scoped
# invocation; the differential is the claim, not the percentage.
#
# This buys MARGIN, not a raise. FLOOR stays 82 and the rule is unchanged -- what is
# retired is a caveat, so the next raise is argued against the measurement alone.
FLOOR=80

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
#   window/editbar/ dialog (a modal GtkWindow + GtkEntry form and the native file
#                   chooser), edit (GtkTextBuffer splices inside one undo group),
#                   focusgate (focus-in/out signal tracking), formatbar and overlay
#                   (widget rows, a GtkMenuButton, a GtkPopover pointed at the caret),
#                   insert (runs the dialog, then splices), newline (a keystroke
#                   handler), relabel (retitles live surfaces) — every one of them is
#                   buffer mutation or widget/signal wiring against a live editor, so
#                   none is decidable from data. **The decisions were moved out
#                   wholesale**: `src/format/` holds the pure half — `continuation.rs`
#                   (what Enter continues), `codeblock`, `heading`, `hr`, `inline`,
#                   `insert`, `list`, `quote`, `text` — each returning a `format::Edit`
#                   that these files only apply. That is the floor-raising direction
#                   POLICY's scope rule names, and it is why this entry is a thin
#                   application layer rather than a hiding place. Spelled out in the
#                   regex for the same reason `tabs/` and `navhistory/` are: the
#                   directory split stopped the path matching `window/[a-z_]+\.rs`.
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
#                   rebalance — GTK wiring end to end. This entry used to argue "no
#                   decision core to extract: its one branch is
#                   `selection_bounds().is_some()`", and that was an UNDERCOUNT (the
#                   file had four branches) which a QA reviewer caught and a live
#                   defect then proved: the PRIMARY release arm needed an ownership
#                   test, which is exactly a decision, and it now lives in
#                   `primarysel.rs` where this gate can see it. The exclusion stands on
#                   what is LEFT, not on the claim that nothing was ever there — and
#                   the general lesson is that "no decision core here" is a claim about
#                   code someone has to keep re-checking, not a property of a file.
#                   What remains is NOT untested — all four
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
#                   `marker_substitute` used to be a second such piece, and QA round 1
#                   named the exclusion swallowing it. It is no longer here: the
#                   marker's precedence moved to `theme::decor`, which is gated and at
#                   100%. That is the shape POLICY step 6 asks for — extract the
#                   decision core rather than widen the gate — and it is why this
#                   exclusion did not need narrowing.
#   outline_view.rs the outline sidebar's GObject subclass (HeadingObject), its
#                   GtkTreeListModel/GtkSignalListItemFactory wiring, and the
#                   expand-all TreeListRow walk — all live widget construction and
#                   signal wiring, on the same terms as codeview/ above → excluded.
#                   The heading data model and tree-folding it renders live in
#                   outline.rs, gated and unit-tested — the module's own doc comment
#                   already states this split; this entry just brings it here too.
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
# `logging` bridges glib's structured-log writer into the `log` facade and its
# `init()` installs PROCESS-GLOBAL state (`glib::log_set_writer_func`,
# `log::set_logger`) that a running binary can arm exactly once — neither call is
# something a unit-test run can exercise repeatedly, and both need a live glib
# runtime besides. The one pure piece, `is_benign_gtk_startup_noise` (the substring
# match that demotes known-benign GTK startup diagnostics), keeps its own unit test
# (`demotes_only_the_known_benign_gtk_startup_transients`) but rides along inside
# the excluded file, the same shape as `codeview/markers.rs`'s group_by_line above.
#
# `tags` registers every fixed `GtkTextTag` via direct property setters
# (`set_scale`, `set_left_margin`, `set_background_rgba`, …) against a live
# `GtkTextBuffer`'s tag table — GObject construction that needs a live GTK runtime
# and so cannot run in this unit-only pass. The pure piece, `TagName::name()` /
# `TagName::is_list_item()` (the fixed-tag-name vocabulary and its totality over
# every depth, including depths the caller contract says cannot occur), keeps its
# own unit tests (`list_depth_tests`) but rides along inside the excluded file, the
# same shape as `logging` and `codeview/markers.rs` above.
#
# Every separator below is the class `[/\\]`, not a literal `/`, because llvm-cov
# reports paths in the host's native form — `src\window\toolbar.rs` on Windows. With
# a `/`-only regex NOTHING matches there, so the scope silently evaporates and the
# gate compares the FLOOR against the UNSCOPED total (37.9% vs 71.7%) and fails every
# run. That reads as "your change tanked coverage" rather than "the filter missed",
# which is the worst way for a gate to break — keep the class if you edit this.
# This guards a HAND-RUN invocation of this script on a Windows box (e.g. under Git
# Bash/WSL), not the pipeline: `scripts/pipeline.steps`' `na.windows coverage
# permanent` entry means step 6 never runs here through the pipeline at all — a
# Windows figure would sit below the Linux floor for an unrelated reason
# (`atomic_io.rs`'s unix-only code not compiled) and POLICY already says never to
# chase that gap. The class still has to hold for that hand-run case, because
# `cargo llvm-cov` reports native Windows paths regardless of who invokes it.
IGNORE='src[/\\](window[/\\](tabs[/\\]|editbar[/\\]|navhistory[/\\])?[a-z_]+|app[/\\](appactions|menubar|openbatch|open|setup)|clipboard|main|lib|gtk_suite|suite_registry|logging|tags|codeview[/\\][a-z_]+|outline_view|preview[/\\]annotate[/\\]overlay|widgets[/\\](table[/\\]mod|tab[/\\](imp|bar|ops|view|mod)))\.rs'

# SCOPE_FILE — the measured set, recorded. Its own header states its role; the one thing
# worth repeating HERE, where the enforcement lives, is what keeps the two files from
# becoming two policies: this script never passes SCOPE_FILE to llvm-cov. `IGNORE` above
# is the only filter, and the manifest is only ever an argument to `comm`.
SCOPE_FILE="scripts/coverage.scope"

UPDATE_SCOPE=0
PASSTHRU=()
for arg in "$@"; do
    case "$arg" in
        --update-scope) UPDATE_SCOPE=1 ;;
        *)              PASSTHRU+=("$arg") ;;
    esac
done

# --------------------------------------------------------------------------------------
# 1. MEASURE. This is the run: it builds, executes the unit tests, and leaves the profile
#    data behind. Everything after it is a `report` against that same data — sub-second,
#    and derived from ONE `IGNORE`, so no verdict below can be reading a different scope
#    than another (a gate is its pattern, its input set, AND the invocation consuming
#    both; a second enumeration here would be the defect this whole change is about).
#    Deliberately WITHOUT --fail-under-lines: the floor verdict must not pre-empt the
#    scope verdict.
# --------------------------------------------------------------------------------------
cargo llvm-cov --summary-only --ignore-filename-regex "$IGNORE" ${PASSTHRU[@]+"${PASSTHRU[@]}"}

# --------------------------------------------------------------------------------------
# 2. SCOPE verdict — reported first, and separately, from any verdict about the number.
# --------------------------------------------------------------------------------------
# llvm-cov reports absolute, host-native paths. Normalise to repo-relative `/` form, and
# TRIPWIRE on anything that will not normalise: a path this cannot anchor is a file from
# somewhere the gate has never measured, and silently dropping it would make the scope
# check leniently incomplete without saying so. Same rule as the scan set's `maxdepth`.
measured_scope() {
    cargo llvm-cov report --lcov --summary-only --ignore-filename-regex "$IGNORE" \
        | awk -v root="$PWD/" '
            /^SF:/ {
                p = substr($0, 4)
                gsub(/\\/, "/", p)
                if (index(p, root) == 1)    p = substr(p, length(root) + 1)
                else if (match(p, /.*\/src\//)) p = substr(p, RSTART + RLENGTH - 4)
                if (p !~ /^src\//) {
                    print "coverage: SCOPE CHECK REFUSED — cannot make this repo-relative: " p > "/dev/stderr"
                    exit 3
                }
                print p
            }' \
        | LC_ALL=C sort -u
}

now="$(measured_scope)"
if [ -z "$now" ]; then
    echo "coverage: SCOPE CHECK REFUSED — llvm-cov reported no files at all." >&2
    echo "coverage: an empty measured set is not a passing one. Check IGNORE and the run above." >&2
    exit 2
fi

if [ "$UPDATE_SCOPE" = 1 ]; then
    [ -f "$SCOPE_FILE" ] || { echo "coverage: $SCOPE_FILE missing; cannot preserve its header." >&2; exit 2; }
    # Keep the file's own header (everything above its first path) and replace the list.
    tmp="$SCOPE_FILE.tmp.$$"
    awk '/^[^#]/ && !/^[[:space:]]*$/ { exit } { print }' "$SCOPE_FILE" > "$tmp"
    printf '%s\n' "$now" >> "$tmp"
    mv "$tmp" "$SCOPE_FILE"
    echo "coverage: rewrote $SCOPE_FILE from this run ($(printf '%s\n' "$now" | wc -l) files measured)."
fi

if [ ! -f "$SCOPE_FILE" ]; then
    echo "coverage: SCOPE CHECK REFUSED — $SCOPE_FILE is missing." >&2
    echo "coverage: the gate does not know what it is supposed to be measuring, so it will" >&2
    echo "coverage: not report on how much of it is covered. Regenerate with --update-scope." >&2
    exit 2
fi
recorded="$(grep -vE '^[[:space:]]*(#|$)' "$SCOPE_FILE" | LC_ALL=C sort -u || true)"
if [ -z "$recorded" ]; then
    echo "coverage: SCOPE CHECK REFUSED — $SCOPE_FILE records no files." >&2
    exit 2
fi

entered="$(LC_ALL=C comm -13 <(printf '%s\n' "$recorded") <(printf '%s\n' "$now"))"
departed="$(LC_ALL=C comm -23 <(printf '%s\n' "$recorded") <(printf '%s\n' "$now"))"

if [ -n "$entered" ] || [ -n "$departed" ]; then
    {
        echo
        echo "=== coverage: SCOPE CHANGED ==="
        echo "The set of files this gate MEASURES no longer matches $SCOPE_FILE."
        echo "This is NOT a statement about how well-tested anything is. What changed is"
        echo "WHAT IS BEING MEASURED, and the percentage moving is a consequence of that."
        if [ -n "$entered" ]; then
            echo
            echo "ENTERED scope — now measured, and at 0% until tested:"
            printf '%s\n' "$entered" | sed 's/^/  + /'
        fi
        if [ -n "$departed" ]; then
            echo
            echo "LEFT scope — no longer measured. Coverage here is surrendered, not earned:"
            printf '%s\n' "$departed" | sed 's/^/  - /'
        fi
        cat <<'EOF'

Decide which side each file belongs on, then record the decision:
  * it holds pure decision logic -> it is IN scope. Test it, then re-run with
    --update-scope in the same commit.
  * it is GTK signal-wiring that cannot be exercised headlessly -> extend IGNORE in
    scripts/coverage.sh with the NARROWEST term that names it, and its rationale beside
    the others, then re-run with --update-scope in the same commit.

Do not widen IGNORE merely to restore the number. POLICY step 6's scope rule is to
extract the decision core out of the excluded file instead; every widened exclusion is
coverage quietly surrendered, and this gate now makes you say so out loud.

The FLOOR verdict is WITHHELD: a ratchet compared across two different scopes measures
nothing, and would report this as a coverage regression, which it is not.
EOF
    } >&2
    exit 1
fi
echo "coverage: SCOPE OK — $(printf '%s\n' "$now" | wc -l) files measured, matching $SCOPE_FILE."

# --------------------------------------------------------------------------------------
# 3. FLOOR verdict — only now, and only over a scope that has been shown to be unchanged.
#    `--fail-under-lines` stays the authority on the comparison (it reads the right column
#    by construction); the number below is echoed for the reader, not used to decide.
# --------------------------------------------------------------------------------------
lines_pct="$(cargo llvm-cov report --summary-only --ignore-filename-regex "$IGNORE" \
             | awk '$1 == "TOTAL" { print $10 }')"
if cargo llvm-cov report --summary-only --fail-under-lines "$FLOOR" \
       --ignore-filename-regex "$IGNORE" >/dev/null; then
    echo "coverage: FLOOR OK — scoped LINES ${lines_pct:-?} >= FLOOR $FLOOR."
    exit 0
fi
echo "coverage: FLOOR FAILED — scoped LINES ${lines_pct:-?} is below FLOOR=$FLOOR." >&2
echo "coverage: the measured scope is UNCHANGED (SCOPE OK above), so this is a real" >&2
echo "coverage: coverage regression: the same files are being measured and less of them" >&2
echo "coverage: is covered. Add tests; do not lower the floor and do not widen IGNORE." >&2
exit 1
