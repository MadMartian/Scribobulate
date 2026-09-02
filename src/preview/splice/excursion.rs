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
//! # The residue, and the one thing the wiring still owes
//!
//! The splice does not land the reader on *exactly* the same content. MEASURED, with
//! the reader's own line tracked by a `GtkTextMark`: expanding drifts the content
//! **+32 px** down the screen, against a block that added 2 160 px above the viewport.
//! Sub-line, and nothing a reader would notice — but it is not rounding, and it scales.
//!
//! Adding anchored children (thematic-break separators) to the disclosure body:
//!
//! | anchored separators in the body | `upper` delta | content drift, EXPANDING | content drift, COLLAPSING |
//! |---|---|---|---|
//! | 0 | ±2 160 | **+32 px** | +16 px |
//! | 10 | ±2 430 | **+368 px** | +32 px |
//! | 30 | ±2 970 | **+880 px** | +32 px |
//!
//! **The asymmetry is the diagnosis, not the linearity.** The same children, removed
//! rather than inserted, cost nothing that scales — collapsing stays inside one text
//! row whatever the count, while expanding tracks it.
//!
//! **The mechanism this file once inferred is REFUTED — see [`drift`], which is that
//! experiment.** The reading was that GTK compensates the offset from the height it
//! can compute at the moment of the change, and that an anchored child's height is
//! zero then because `attach_anchored` parents the widget after the region render
//! returns. Two testable consequences follow, and both were measured and failed:
//! parenting each child in the SAME turn as its anchor changes the drift by nothing
//! at all (identical to the pixel, with the parenting proved to have happened), and
//! the per-child drift is the same figure for a 27 px child and a 50 px one, so it is
//! not a function of the child's height in any form — neither its whole height nor
//! its height minus a placeholder. Do not re-derive either; `drift` records the
//! numbers that close them.
//!
//! **Which NUMBER, and how the compensation spends it, are now measured too — see
//! [`wholelist`] and [`trace`].** The count is the children the toggled REGION draws,
//! not every anchored child in the view: thirty extra children placed OUTSIDE the
//! region, in the same `anchored_children` list, cost the reader nothing and add not
//! one compensating adjustment write, while each child inside it adds one. And the
//! compensation is not a single write — it is a run of one uniform write per region
//! child, then a bulk write, all of it accounted for exactly by `value-changed`.
//!
//! What is MEASURED, and survives that: **the splice needs no scroll work to avoid
//! the excursion — something already carries the offset to within a text row — but
//! expanding a region drifts the reader in proportion to the NUMBER of anchored
//! children that region draws.** The +32 px at zero separators is the same effect at
//! two emissions rather than a separate term: [`margin`] identifies what the
//! compensation is missing, and it is `top_margin` per emission — the upstream defect
//! fixed by GTK commit `b300698629` in 4.19.3.
//!
//! # What the wiring did with that, and why the drift is still not asserted HERE
//!
//! [`wired`] is the answer: `splice::install::ReaderAnchor` records the reader's offset
//! from the viewport top BEFORE the splice and re-establishes it after quiescence, so
//! the drift is undone rather than compensated (a compensation cannot be right — the
//! emission count is BIMODAL at a fixed dose, 55 or 63 for the identical toggle). It
//! reaches ZERO through the production path: MEASURED +368 px with the restore removed
//! and +0 px with it, on a 20-separator body.
//!
//! **These files still drive `splice` directly, WITHOUT the restore, and that is
//! deliberate** — they are the positive controls for the upstream bug, so they must go
//! on measuring it. The drift they report stays reported and not asserted for the same
//! reason as before: it is a property of the GTK build, not a contract of this project.
//! The contract lives in [`wired`], where it belongs, stated against the reader rather
//! than against the mechanism.

use harness::{measure, splice_toggle, tall_document, truncate, Parenting, ZOOM};

/// The presented pane and the settle discipline — see [`rig`]'s own module docs for
/// why the cut falls there.
mod rig;

/// The per-child drift measurement that tests this module's own INFERRED mechanism.
/// Separate file because it is a third shape again (a dose-response table over a
/// varying fixture, not one comparison) and because this file is at the 500-line
/// soft limit.
mod drift;

/// The experiment that resolves a CONFOUND in the two above: every anchored child
/// their fixtures create lives inside the toggled body, so "the children the region
/// draws" and "every anchored child in the view" are one count and neither table can
/// tell them apart. Separate file because it needs a fixture knob the others do not.
mod wholelist;

/// The per-emission view of the same toggle: every `vadjustment::value-changed` across
/// the settle, with its value and its delta. The other three files read what the
/// compensation LEFT BEHIND; this one watches it happen. Separate file because it is a
/// different shape again (a sequence per cell, not a figure per cell).
mod trace;

/// The FALSIFICATION experiment: the same dose knob turned at six counts either side of
/// a predicted kink at N=22/23, several runs each. The four files above describe what
/// the compensation does; this one tries to break a formula that claims to say why the
/// emission count is what it is. Separate file because it is a different shape again (a
/// distribution per dose, not a figure per cell) and because the bimodality forces the
/// arithmetic — steps computed WITHIN a mode — that no other file here needs.
/// One run of one dose and the distribution a repeated one produces — what [`kink`] and
/// [`budget`] share, split out for the same reason [`recorder`] was.
mod dose;

mod kink;

/// [`kink`]'s companion, testing the same 2000-pixel validation budget by moving the
/// CHUNK HEIGHT rather than the dose — the budget's arithmetic directly, rather than the
/// kink that arithmetic produces. Separate file because it is a different question, and
/// because it took `kink` past the 500-line soft limit.
mod budget;

/// The knob experiment: the same grid at three values of the view's own `top-margin`,
/// which is what tests a model that says the drift is a MULTIPLE of that margin.
/// Separate file because it is the first experiment here to vary something about the
/// VIEW rather than about the document.
mod margin;

/// What the experiments share once there was more than one of them — see its own
/// module docs for where the cut falls.
mod harness;

/// The same question asked of the WHOLE APPLICATION rather than of the mechanism: a
/// real window, a real tab, and the reader activating the control. This is where TDD
/// 2.26h/i/j are asserted — the files above measure `splice` and prove nothing about
/// the one wire between a click and it.
mod wired;

/// The `value-changed` instrument [`trace`] and [`margin`] both record through — one
/// copy, so their numbers stay comparable.
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
        let _ = splice_toggle(rig, folds, key, &md, Parenting::Eager);
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

/// Does this GTK carry the `top_margin` compensation fix, and therefore NOT the defect
/// every body in this module is built to observe?
///
/// **These experiments are positive controls for an upstream BUG.** They assert that a
/// splice above the viewport drifts the reader by `emissions x top_margin`, which is
/// true of GTK before 4.19.3 and false after it — the one-line fix is commit
/// `b300698629` ("textview: fix yoffset position when top_margin is set",
/// GNOME/gtk#4134), which adds the missing `+ priv->top_margin`. So on a newer GTK
/// these bodies must NOT fail; there is simply nothing for them to measure.
///
/// Gated at RUNTIME on the loaded library rather than by `#[cfg]`, because a cfg'd-out
/// test is deleted rather than skipped — not compiled, not reported, not counted
/// (POLICY § Unit tests) — and this is precisely the case where a silent absence
/// misleads: a seat on a fixed GTK would see a green suite and conclude the drift is
/// gone from OUR code. The skip announces itself instead.
///
/// The defect itself is ScrAP-339 / GTK4Rs/AP-321.
///
/// Linux runs 4.6.9 and observes the defect; the macOS seat's 4.22.4 carries the fix,
/// so it skips loudly. Neither is a platform claim — it is the library version, which
/// is why this asks the runtime rather than the target.
pub(super) fn skip_if_gtk_compensates_top_margin(limb: &str) -> bool {
    let (major, minor) = (gtk::major_version(), gtk::minor_version());
    // 4.19.3 is the first release carrying the fix; anything at or past 4.19 has it.
    let fixed = major > 4 || (major == 4 && minor >= 19);
    if fixed {
        crate::testsymlink::skipped(
            limb,
            &format!(
                "GTK {major}.{minor} carries the top_margin compensation fix (b300698629, first in 4.19.3), so the drift this measures does not occur on this runtime"
            ),
        );
    }
    fixed
}
