//! **A measurement, not a feature guard.** Does the splice actually avoid the scroll
//! excursion that a full in-place re-render causes?
//!
//! [`super`]'s module docs assert that it does, and the plan records the re-render's
//! cost as MEASURED — but the splice's own saving had never been measured, only
//! reasoned about from the re-render's behaviour. This module measures both arms of
//! the same scenario against the same fixture and reports them side by side:
//!
//! * **CONTROL** — [`crate::preview::re_render`], which is what
//!   `wire_disclosure_toggles` does today.
//! * **SPLICE** — [`super::splice`], the landed mechanism.
//!
//! **The control is load-bearing and is asserted first.** A splice that shows no
//! collapse proves nothing unless the re-render on the same rig *does* show one — a
//! negative result with no positive control is a statement about the rig, not about
//! the code (GTK4Rs/AP-78's family: here the risk is that the fixture is too short,
//! or the view too settled, for the phenomenon to arise at all). So
//! [`the_splice_avoids_the_excursion_a_full_re_render_causes`] fails, naming the rig,
//! if the control does not reproduce the documented collapse.
//!
//! # What is sampled, and why the trough rather than the endpoints
//!
//! The re-render's defect is **not** visible in the settled numbers: the plan records
//! the landing position as exact (11 705 → 11 705 across a toggle). What the reader
//! sees is the *journey* — `upper` collapsing to roughly the viewport height, which
//! clamps `value` to zero, followed by a glide back as GTK re-validates line heights
//! top-down. So the instrument is the **minimum** `value` and `upper` observed at any
//! point between the toggle and the settle, not a before/after pair. An endpoint-only
//! measurement would report the two routes as identical.
//!
//! # What it measured (GTK 4.6.9, X11/Xvfb, 700×600 pane, ~38 000 px document)
//!
//! | | control (re-render) | splice |
//! |---|---|---|
//! | `upper` before | 36 168 | 36 168 |
//! | `upper` **trough** | **672** | **36 168** (no excursion) |
//! | `value` before | 21 340 | 21 340 |
//! | `value` **trough** | **2** | **21 340** (no excursion) |
//! | `value` settled | **2** — never recovers | 23 468 |
//! | reader's anchor | **destroyed** | survives |
//!
//! So the splice removes the excursion outright: neither `upper` nor `value` moves at
//! all between the toggle and the settle, and `value` lands where the content went.
//!
//! **The control's reading position is not merely disturbed, it is lost.** `value`
//! settles at 2 and stays there, because `re_render` deliberately restores no scroll
//! position of its own (its own doc comment says so — the split-pane sync owns that
//! re-projection). The plan's "glides back" is that sync at work, one layer up; this
//! rig measures the render route alone, so what it shows is the raw damage.
//!
//! # The residue, and what the wiring does with it
//!
//! The splice does not land the reader on *exactly* the same content. MEASURED, with
//! the reader's own line tracked by a `GtkTextMark`: expanding drifts the content
//! **+32 px** down the screen against a block that added 2 160 px above the viewport,
//! and that figure scales with the number of ANCHORED CHILDREN the toggled region
//! draws — +368 px at ten, +880 px at thirty. Collapsing stays inside one text row
//! whatever the count.
//!
//! **The whole account of why is upstream, not here.** It is one `top_margin` per
//! compensating `::changed` emission, the defect GTK fixed in 4.19.3 by commit
//! `b300698629`; the emission count is the region's own child count, bounded by the
//! 2 000 px validation budget `gtk_text_layout_validate` spends. That is recorded as
//! **ScrAP-339** and, in reusable form, as the gtk4-rs skill's
//! **GTK4Rs/AP-321**. Seven characterization arms once measured it here — a dose
//! grid, a chunk-height grid, a `top-margin` knob, a whole-list confound control, a
//! per-emission trace and a falsification of the kink the budget predicts. They were
//! **deleted** once their conclusion had landed in those two entries: they asserted
//! facts about GTK's validation budget rather than about this project, they were
//! bimodal and order-dependent by their own admission, and they carried a required CI
//! gate that could go red on a distro GTK patch with nothing here broken. Read them in
//! `git log -- src/preview/splice/excursion/`; do not re-derive them from scratch.
//!
//! The one conclusion worth repeating at this altitude, because it is what the design
//! rests on: **the drift cannot be compensated, only undone.** The emission count is
//! BIMODAL at a fixed dose — 55 or 63 for the identical toggle — so any arithmetic that
//! multiplies it is multiplying a number that is not stable.
//!
//! [`wired`] is what the wiring does instead: `splice::install::ReaderAnchor` records
//! the reader's offset from the viewport top BEFORE the splice and re-establishes it
//! after quiescence. It reaches ZERO through the production path: MEASURED +368 px with
//! the restore removed and +0 px with it, on a 20-separator body.
//!
//! **The two bodies below still drive `splice` directly, without the restore.** They
//! measure the excursion (the `upper`/`value` collapse), which is this project's own
//! contract and is asserted; the sub-line drift above them is not asserted anywhere in
//! this file, because it is a property of the GTK build. The reader-facing contract
//! lives in [`wired`], stated against the reader rather than against the mechanism.

use harness::{measure, splice_toggle, tall_document, truncate, ZOOM};

/// The presented pane and the settle discipline — see [`rig`]'s own module docs for
/// why the cut falls there.
mod rig;

/// What the experiments share once there was more than one of them — see its own
/// module docs for where the cut falls.
mod harness;

/// The same question asked of the WHOLE APPLICATION rather than of the mechanism: a
/// real window, a real tab, and the reader activating the control. This is where TDD
/// 2.26h/i/j are asserted — the files above measure `splice` and prove nothing about
/// the one wire between a click and it.
mod wired;

/// The `value-changed` instrument: every adjustment write across a settle, with its
/// delta. [`wired`] reads it to assert that the restore does not itself provoke a fresh
/// compensation burst.
mod recorder;

/// **The measurement.** Both routes over one fixture and one toggle direction,
/// reported side by side and checked against each other.
///
/// The control runs inside the same function as the splice, deliberately: it is not
/// an independent check but the thing that makes the splice's number mean anything,
/// and a control in a separate test can be filtered out, or fail while the other
/// passes and is read as good news.
fn compare_routes(direction: &str, start_expanded: bool) {
    let md = tall_document();
    let control = measure("re-render", &md, start_expanded, |rig, folds, _key| {
        crate::preview::re_render(&rig.scroller, &tall_document(), None, ZOOM, false, folds);
    });

    let spliced = measure("splice", &md, start_expanded, |rig, folds, key| {
        let _ = splice_toggle(rig, folds, key, &md);
    });

    let report = format!(
        "\n=== {direction} a disclosure ABOVE the reading position ===\n{}{}",
        control.report(),
        spliced.report()
    );
    println!("{report}");

    // ── The positive control, first. ────────────────────────────────────────────
    assert!(
        control.upper_collapsed(),
        "THE RIG IS NOT EXERCISING THE PHENOMENON. A full in-place re-render is \
         documented to collapse the vadjustment's `upper` (MEASURED ~28 000 -> ~650 \
         on a 107 KB document), and on this rig it did not: upper went {:.0} -> a \
         trough of {:.0}. Every other number here is therefore meaningless — a \
         negative result with no positive control is a statement about the fixture, \
         not about the splice. Re-establish the control before reading anything \
         below.{report}",
        control.before.upper,
        control.min_upper,
    );
    assert!(
        control.thrown_to_the_top(),
        "the control collapsed `upper` but never threw `value` to the top, so it is \
         reproducing only half the documented defect and the splice's `value` number \
         has nothing to be compared against.{report}"
    );

    // ── The splice. ────────────────────────────────────────────────────────────
    assert!(
        !spliced.upper_collapsed(),
        "the splice collapsed `upper` too ({:.0} -> a trough of {:.0}), so it does \
         not avoid the excursion the control exhibits and the mechanism does not buy \
         what it was built for.{report}",
        spliced.before.upper,
        spliced.min_upper,
    );
    assert!(
        !spliced.thrown_to_the_top(),
        "the splice let `value` fall to the top ({:.0} -> a trough of {:.0}) — the \
         reader was still thrown back to the start of the document, which is the \
         whole defect (TDD 2.26h).{report}",
        spliced.before.value,
        spliced.min_value,
    );

    // ── The two routes must agree about the document they produced. ────────────
    //
    // Not a second copy of `tests::assert_splice_matches_full_render` (which compares
    // buffer TEXT on a bare buffer): this compares the settled HEIGHT of two live,
    // laid-out views. It is what makes the comparison above fair — a splice that
    // avoided the excursion by rendering less content would pass every assertion
    // here, and only the height says otherwise.
    assert_eq!(
        spliced.after.upper, control.after.upper,
        "the two routes settled at different document heights, so they did not render \
         the same document and the excursion comparison above is between two \
         different things.{report}"
    );

    // ── TDD 2.26h's "same content" half, which the adjustment cannot answer. ───
    assert!(
        spliced.anchor_survived(),
        "the splice destroyed the reader's anchor — the marked line now reads {:?} \
         rather than {:?}, so the content drift below describes some other line.{report}",
        truncate(&spliced.anchor_text_after),
        truncate(&spliced.top_before),
    );
}

/// **Expanding** a collapsed block above the reader — the document GROWS above them.
#[gtktest::test]
fn the_splice_avoids_the_excursion_a_full_re_render_causes() {
    compare_routes("expanding", false);
}

/// **Collapsing** an expanded block above the reader — the document SHRINKS above
/// them, which is the direction where a stale `value` is not merely wrong but
/// unrepresentable: the clamp has a genuinely smaller maximum to answer with, so a
/// route that holds `value` still can be forced to move by the range itself.
///
/// Measured separately rather than assumed symmetric with the expand case. TDD 2.26h
/// states both directions, and the two are not the same mechanism — growth above the
/// viewport can always be compensated, shrinkage past `upper - page_size` cannot.
#[gtktest::test]
fn the_splice_holds_the_reader_when_the_block_above_them_collapses() {
    compare_routes("collapsing", true);
}
