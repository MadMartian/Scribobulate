//! **A measurement, not a feature guard** — the second one, and it exists to test
//! [`super`]'s own INFERRED mechanism rather than to guard a behaviour.
//!
//! [`super`]'s docs record a residue: the splice avoids the excursion outright, but
//! expanding a region that contains anchored children leaves the reader displaced,
//! and the displacement SCALES with the number of children. The reading offered there
//! was that GTK compensates a change above the viewport using the height it can
//! compute at the moment of the change, and that an anchor whose widget has not been
//! parented yet contributes nothing to that height.
//!
//! Two candidate correctives follow from that reading and this file separates them.
//!
//! 1. **Parent the child in the SAME turn as its anchor**, before the priority-125
//!    incremental validate wraps the (above-viewport) new lines, so the first wrap
//!    already reads `gtk_widget_get_preferred_size`
//!    ([`crate::renderer::Renderer::push_anchored`]). **MEASURED NOT TO WORK** — see
//!    the table below. It is kept as an ARM of this measurement, and as the reason
//!    nobody need re-derive the hypothesis, not as a fix.
//! 2. That the uncompensated amount is the child's real height MINUS a hard-coded
//!    placeholder (`gtktextlayout.c`'s `add_child_attrs` is reported to wrap a
//!    widget-less anchor at 30x20, so the leftover would be `real − 20`).
//!    **ALSO REFUTED**, and by the cheaper experiment: measured at two child heights
//!    23 px apart, the per-child drift is the SAME FIGURE. It is not a function of
//!    the child's height at all — which kills the original "its height is zero then"
//!    reading as well, since that one predicts drift == height.
//!
//! What survives both refutations is only the correlation: the drift scales with the
//! NUMBER of anchored children the region draws, and with nothing else measured here.
//! No mechanism is offered, deliberately — two plausible ones have now been recorded
//! and knocked down, and a third guess is worth less than the numbers below.
//!
//! # What it measured (GTK 4.6.9, X11/Xvfb, 700x600 pane)
//!
//! Expanding a disclosure above the reader, drift over the zero-child base of the
//! same arm ("excess"), and the document height that arm actually gained:
//!
//! | children | deferred excess | eager excess | height gained |
//! |---|---|---|---|
//! | 0 | 0 px (base +32 px) | 0 px (base +32 px) | +2 160 px |
//! | 10 | +336 px | +336 px | +2 430 px |
//! | 30 | +848 px | +848 px | +2 970 px |
//!
//! Identical to the pixel, and the assertion that the eager arm's children really
//! were parented by the region render (`super::harness::splice_toggle`) is what makes that an
//! observation rather than a no-op.
//!
//! At ten children, per child — the height probe:
//!
//! | child | per-child drift | per-child height |
//! |---|---|---|
//! | `---` (a `GtkSeparator`) | 33.6 px | 27.0 px |
//! | a broken image (a `GtkImage`) | 33.6 px | 50.0 px |
//!
//! Collapsing costs nothing that scales, under either parenting, as [`super`] already
//! recorded: 16 px at every child count.
//!
//! # Why the numbers are printed and not asserted
//!
//! For the same reason [`super`] reports its own drift rather than pinning it: a
//! per-child figure is a fact for the wiring to act on, not a contract, and pinning
//! one would freeze a host's font metrics. What IS asserted is the rig — that the
//! reader's anchor survived, that each arm's parenting actually happened, and that
//! the deferred route reproduces the dose-response at all. A negative result with no
//! positive control is a statement about the fixture (GTK4Rs/AP-78's family).

use super::harness::{measure, splice_toggle, tall_document_with_children, Arm, Parenting};

/// The dose. Zero is the control — MEASURED, a region render whose body holds no
/// such construct draws no anchored child at all, the disclosure's own toggle button
/// belonging to the summary line written before the region begins — and the two
/// loaded counts are far enough apart that a per-child cost cannot hide inside
/// measurement noise.
const CHILD_COUNTS: [usize; 3] = [0, 10, 30];

/// The dose at which the SECOND child height is measured. One loaded count is enough
/// there: the question is whether the per-child figure tracks the child's own height,
/// which two heights answer and a third count does not.
const HEIGHT_PROBE_COUNT: usize = 10;

/// Slack for the two shape assertions below, in pixels — about one text row on this
/// rig. Everything asserted here is a SHAPE (is there a dose-response at all; does
/// the per-child figure track the child's own height), so the tolerance only has to
/// be small against a per-child cost of tens of pixels.
const SLACK_PX: f64 = 12.0;

/// A body construct that renders as exactly ONE anchored widget, at a height of its
/// own. Two of them, because one height cannot distinguish "the child's whole height
/// goes uncompensated" from "its height minus a fixed placeholder does".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Child {
    /// A thematic break — an anchored `GtkSeparator` plus its themed rule margins.
    Rule,
    /// An unresolvable image reference — an anchored `GtkImage` placeholder, which is
    /// several times a separator's height. `doc_dir` is `None` throughout this rig,
    /// so nothing on disk can accidentally resolve it.
    BrokenImage,
}

impl Child {
    fn md(self) -> &'static str {
        match self {
            Child::Rule => "---",
            Child::BrokenImage => "![absent](no-such-image.png)",
        }
    }
}

/// One cell of the table.
struct Cell {
    children: usize,
    parenting: Parenting,
    arm: Arm,
}

impl Cell {
    /// The drift attributable to the children — this cell's content drift with the
    /// zero-child base of the same arm removed. [`super`] records that base as a
    /// separate cause, so subtracting it is what isolates the per-child term.
    fn excess_over(&self, base: f64) -> f64 {
        self.arm.content_drift_px() - base
    }

    /// The document height this cell's children added, over the zero-child base of
    /// the same arm — the honest denominator for a per-child figure, because it is
    /// what the children actually contribute rather than what a theme metric says
    /// they should.
    fn height_over(&self, base: f64) -> f64 {
        (self.arm.after.upper - self.arm.before.upper) - base
    }
}

/// Measure one cell: build the fixture, park the reader, toggle through the splice
/// with the given parenting, and read the drift.
fn cell(children: usize, child: Child, start_expanded: bool, parenting: Parenting) -> Cell {
    let md = tall_document_with_children(children, child.md());
    let drawn = std::cell::Cell::new(0usize);
    let arm = measure("splice", &md, start_expanded, |rig, folds, key| {
        drawn.set(splice_toggle(rig, folds, key, &md, parenting));
    });
    // The dose arrived. Only the EXPANDING direction draws the body's children —
    // collapsing deletes them and the region render writes a summary line with no
    // anchored widget at all — so this is where the count is checkable, and a cell
    // whose fixture silently rendered no separator would otherwise report a
    // perfectly good-looking zero.
    if !start_expanded {
        assert!(
            drawn.get() >= children,
            "the fixture asked for {children} {child:?} children in the disclosure \
             body and the region render drew only {} anchored widgets, so this cell \
             measures a smaller dose than it reports",
            drawn.get(),
        );
    }
    assert!(
        arm.anchor_survived(),
        "the splice destroyed the reader's anchor at {children} {child:?} children, so \
         the drift measured for this cell describes some other line"
    );
    Cell {
        children,
        parenting,
        arm,
    }
}

/// Both parenting arms over `counts`, reported as a table. Returns the cells so a
/// caller can assert whatever shape it is actually testing.
fn table(direction: &str, start_expanded: bool, child: Child, counts: &[usize]) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for parenting in [Parenting::Deferred, Parenting::Eager] {
        for &children in counts {
            cells.push(cell(children, child, start_expanded, parenting));
        }
    }

    let mut report = format!(
        "\n=== per-child drift: {direction} a disclosure of {child:?} children ABOVE \
         the reading position ===\n\
         \x20 {:<10} {:>9} {:>10} {:>10} {:>10} {:>10}\n",
        "parenting", "children", "drift", "excess", "height", "per child",
    );
    for c in &cells {
        let drift_base = base(&cells, c.parenting, Cell::content_drift);
        let height_base = base(&cells, c.parenting, Cell::upper_delta);
        let excess = c.excess_over(drift_base);
        let height = c.height_over(height_base);
        report.push_str(&format!(
            "  {:<10} {:>9} {:>8.0}px {:>8.0}px {:>8.0}px {}\n",
            format!("{:?}", c.parenting),
            c.children,
            c.arm.content_drift_px(),
            excess,
            height,
            if c.children == 0 {
                "         -".to_string()
            } else {
                format!(
                    "{:>5.1}px of {:.1}px",
                    excess / c.children as f64,
                    height / c.children as f64
                )
            },
        ));
    }
    println!("{report}");
    cells
}

impl Cell {
    fn content_drift(&self) -> f64 {
        self.arm.content_drift_px()
    }
    fn upper_delta(&self) -> f64 {
        self.arm.after.upper - self.arm.before.upper
    }
}

/// The zero-child reading of `arm`, which every "excess" in the table is measured
/// against.
fn base(cells: &[Cell], parenting: Parenting, of: fn(&Cell) -> f64) -> f64 {
    cells
        .iter()
        .find(|c| c.parenting == parenting && c.children == 0)
        .map(of)
        .expect("every arm measures its own zero-child base")
}

/// Per-child drift and per-child height for one arm at one count.
fn per_child(cells: &[Cell], parenting: Parenting, children: usize) -> (f64, f64) {
    let c = cells
        .iter()
        .find(|c| c.parenting == parenting && c.children == children)
        .expect("the arm measured this count");
    let n = children as f64;
    (
        c.excess_over(base(cells, parenting, Cell::content_drift)) / n,
        c.height_over(base(cells, parenting, Cell::upper_delta)) / n,
    )
}

/// **Expanding** — the direction the residue was measured in, where the region's
/// children are created by the toggle and so are the ones whose height the
/// compensation cannot see.
#[gtktest::test]
fn parenting_a_region_child_in_the_same_turn_does_not_remove_the_per_child_drift() {
    if super::skip_if_gtk_compensates_top_margin("drift: parenting is not the cause") {
        return;
    }
    let cells = table("expanding", false, Child::Rule, &CHILD_COUNTS);

    // ── The positive control. ──────────────────────────────────────────────────
    let (deferred_drift, deferred_height) = per_child(&cells, Parenting::Deferred, 30);
    assert!(
        deferred_drift > SLACK_PX,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. Deferring the region's parenting is \
         documented to drift the reader in proportion to the number of anchored \
         children the region draws, and on this rig each of thirty children cost only \
         {deferred_drift:.1}px (against {deferred_height:.1}px of height each). Every \
         number in the table above is therefore meaningless."
    );

    // ── The finding, stated as the shape rather than as a number. ──────────────
    //
    // Deliberately NOT asserted as an equality on the drift itself: a per-child
    // figure is a host's font metrics, and pinning it would make this test a
    // portability hazard rather than a record. What is asserted is that eager
    // parenting made no material difference, which is the measured result and the
    // thing a future reader must not re-derive by hand.
    let (eager_drift, _) = per_child(&cells, Parenting::Eager, 30);
    assert!(
        (eager_drift - deferred_drift).abs() <= SLACK_PX,
        "eager parenting has started to matter ({eager_drift:.1}px per child against \
         the deferred route's {deferred_drift:.1}px). That is a CHANGE from the \
         measured result this test records — the same-turn parenting made no \
         difference on GTK 4.6.9 — so re-read the table above and update this \
         module's docs rather than adjusting the tolerance."
    );
}

/// **Collapsing** — the direction that showed no dose-response to begin with, because
/// the children being removed were already parented and laid out. Measured rather
/// than assumed symmetric: the claim that eager parenting changes nothing here is
/// part of the mechanism, and a fix that perturbed this direction would falsify it.
#[gtktest::test]
fn collapsing_shows_no_per_child_drift_under_either_parenting() {
    if super::skip_if_gtk_compensates_top_margin("drift: collapse direction") {
        return;
    }
    let cells = table("collapsing", true, Child::Rule, &CHILD_COUNTS);
    for parenting in [Parenting::Deferred, Parenting::Eager] {
        let (drift, height) = per_child(&cells, parenting, 30);
        assert!(
            drift.abs() <= SLACK_PX,
            "collapsing is documented to cost nothing that scales with the child \
             count, because those children were already parented and laid out when \
             the region was deleted — but under {parenting:?} parenting each of \
             thirty removed children cost {drift:.1}px (of {height:.1}px each)."
        );
    }
}

/// **The second child height**, which is what turns a reading into a measurement —
/// and it refutes every height-based account of the drift at once.
///
/// Two models were on the table, and each predicts the per-child drift VARIES with
/// the child's own height: "the child's height is zero at compensation time" predicts
/// drift == height, and "a widget-less anchor wraps at a hard-coded 30x20 placeholder"
/// predicts drift == height − 20. MEASURED at two heights differing by 23 px, the
/// per-child drift is **the same figure to a tenth of a pixel**. It is not a function
/// of the child's height at all, so whatever the compensation is missing, it is not
/// the child's pixels.
#[gtktest::test]
fn the_per_child_drift_is_independent_of_the_childs_height() {
    if super::skip_if_gtk_compensates_top_margin("drift: height independence") {
        return;
    }
    let short = table("expanding", false, Child::Rule, &[0, HEIGHT_PROBE_COUNT]);
    let tall = table(
        "expanding",
        false,
        Child::BrokenImage,
        &[0, HEIGHT_PROBE_COUNT],
    );

    let (short_drift, short_height) = per_child(&short, Parenting::Deferred, HEIGHT_PROBE_COUNT);
    let (tall_drift, tall_height) = per_child(&tall, Parenting::Deferred, HEIGHT_PROBE_COUNT);
    println!(
        "\n=== per-child drift against per-child height ===\n\
         \x20 {:<12} {:>12} {:>12} {:>12}\n\
         \x20 {:<12} {:>10.1}px {:>10.1}px {:>10.1}px\n\
         \x20 {:<12} {:>10.1}px {:>10.1}px {:>10.1}px\n",
        "child",
        "drift",
        "height",
        "shortfall",
        "Rule",
        short_drift,
        short_height,
        short_height - short_drift,
        "BrokenImage",
        tall_drift,
        tall_height,
        tall_height - tall_drift,
    );

    assert!(
        tall_height > short_height + SLACK_PX,
        "the two probes must differ in child HEIGHT or this measures one height twice \
         — Rule {short_height:.1}px against BrokenImage {tall_height:.1}px"
    );
    assert!(
        (tall_drift - short_drift).abs() <= SLACK_PX,
        "the per-child drift has started to follow the child's HEIGHT — a {short_height:.1}px \
         Rule cost {short_drift:.1}px and a {tall_height:.1}px BrokenImage cost \
         {tall_drift:.1}px. This test records the opposite (the same figure at both \
         heights, which is what refutes every height-based account of the drift), so \
         a difference here is a change to the finding: re-read the tables above and \
         update this module's docs rather than widening the tolerance."
    );
}
