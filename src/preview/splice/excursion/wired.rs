//! **TDD 2.26h / 2.26i / 2.26j against the WIRED path** — the whole application, not
//! the mechanism.
//!
//! Every other file in this directory drives [`super::super::splice`] directly on a
//! hand-built pane. That proves the region write and the adjustment behaviour, and it
//! proves nothing about the one wire between the reader's click and it: a toggle whose
//! `toggled` handler never reaches the splice emits the signal and changes nothing,
//! and that is exactly how this construct has failed before (`outline_nav`'s
//! `activating_the_control_shows_and_hides_the_body` exists for the same reason).
//!
//! So these build a real window with a real tab, park a reader well down the document,
//! and **activate the control**. Everything after that is the production path:
//! `preview::render`'s toggle handler, `window::splice_disclosure_in_place`,
//! `preview::splice::install`, and the deferred restore.
//!
//! # The positive control is load-bearing and is asserted first
//!
//! "The wired path did not collapse `upper`" says nothing unless a full re-render of
//! the SAME fixture, on the SAME rig, does (GTK4Rs/AP-78's family: the fixture may
//! simply be too short for the phenomenon). [`control_collapse`] runs
//! [`crate::preview::re_render`] over an identically built pane and reports its trough;
//! the wired assertions are stated against it rather than against a constant.

use gtk::prelude::*;
use std::time::Duration;

use crate::codeview::CodePreviewView;
use crate::testpump::{self, Clock};

use super::harness::{FILLER, PANE_H, PANE_W};
use super::recorder::{runs_summary, Trace};
use super::rig::{anchor_reader, reader_offset, top_line_text, Reading};

/// Paragraphs inside the disclosure body, each followed by a thematic break.
///
/// The break is what makes the body carry ANCHORED CHILDREN, which is the dose the
/// upstream `top_margin` drift scales with (MEASURED 32 / 368 / 880 px at 0 / 10 / 30).
/// A body of plain prose exercises the smallest case and would let a broken restore
/// pass by being within a text row of correct.
const BODY_PARAS: usize = 20;

/// Paragraphs after the disclosure. Enough that the settled range towers over the
/// viewport, which is the precondition for the clamp a full re-render trips.
const TAIL_PARAS: usize = 220;

/// Text that appears ONLY when the body is genuinely expanded.
///
/// **Not the body's opening words**, which is the obvious choice and is wrong: a
/// COLLAPSED block previews its body's opening text on the summary line (TDD 2.26), so
/// `contains("<the first body paragraph>")` is true in BOTH fold states. MEASURED as a
/// vacuous precondition, and — worse — as a `pump_until` that returned before the
/// toggle's deferred work had run at all, so the assertions that followed reported on
/// the pre-toggle state and one of them PASSED. This sits in the LAST body paragraph,
/// far past the preview's character limit.
const DEEP_IN_THE_BODY: &str = "the deepest hidden paragraph";

/// How long the adjustment must hold still before the transition counts as finished.
///
/// Deliberately longer than `farscroll::settle`'s own quiet window (3 x 50 ms), so the
/// deferred restore has always landed by the time this returns — otherwise every
/// reading below would describe the state the restore exists to correct, and the guards
/// would pass with the restore deleted.
const QUIET: Duration = Duration::from_millis(400);

/// A document with one collapsed disclosure near the top and — BELOW it — a heading, a
/// link, a distinctive paragraph and a table, which are what rubric 2.26j is about.
fn fixture() -> String {
    let mut md = String::from(
        "# Wired fixture\n\nAn opening paragraph, before the disclosure.\n\n\
         <details>\n<summary>A collapsible section</summary>\n\n",
    );
    for i in 0..BODY_PARAS {
        md.push_str(&format!("Hidden body paragraph {i}. {FILLER}\n\n---\n\n"));
    }
    md.push_str(&format!("{DEEP_IN_THE_BODY}. {FILLER}\n\n"));
    md.push_str("</details>\n\n");
    md.push_str("| before | col |\n|---|---|\n| a | b |\n\n");
    for i in 0..TAIL_PARAS {
        md.push_str(&format!("Tail paragraph {i}. {FILLER}\n\n"));
    }
    md.push_str("## Below the block\n\na distinctive tail paragraph\n\n");
    md.push_str("[link text](https://example.invalid/target)\n\n");
    md.push_str("| after | col |\n|---|---|\n| e | f |\n");
    md
}

/// A real application window over `md`, settled and parked at the reading position.
struct Wired {
    window: gtk::ApplicationWindow,
    view: CodePreviewView,
    adjustment: gtk::Adjustment,
    /// Held so the `gtk::Application` outlives the window it built.
    _app: gtk::Application,
}

impl Wired {
    fn present(suffix: &str, md: &str) -> Self {
        let app = crate::window::testkit::test_app(&format!(
            "com.extollit.scribobulate.integrationtest.{suffix}"
        ));
        let window = crate::window::new_window(&app, "IT-SPLICE", md, None);
        window.set_default_size(PANE_W, PANE_H);
        window.present();

        // The tab's own preview scroller. `window::get_preview_sw` is `window`-private
        // and this file is not in `window`; this is the same resolution it performs,
        // through the accessor `window::scrollsync` itself uses.
        let sw = crate::winstate::state(&window)
            .and_then(|st| st.split.preview_scroller())
            .expect("a preview scroller");
        let view = sw
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("a preview view");
        let adjustment = sw.vadjustment();
        {
            let (view, adjustment) = (view.clone(), adjustment.clone());
            testpump::until(
                Clock::Idle,
                "the preview to map and acquire a viewport",
                move || view.is_mapped() && adjustment.page_size() > 0.0,
            );
        }
        let rig = Wired {
            window,
            view,
            adjustment,
            _app: app,
        };
        rig.settle();
        rig
    }

    /// See [`super::rig::settle`] — the same discipline the mechanism rig uses, with
    /// this rig's longer [`QUIET`] window.
    fn settle(&self) {
        super::rig::settle(&self.view, &self.adjustment, QUIET);
    }

    /// See [`super::rig::settle_watching_the_trough`].
    fn settle_watching_the_trough(&self) -> (f64, f64) {
        super::rig::settle_watching_the_trough(&self.adjustment, QUIET)
    }

    /// See [`super::rig::park_the_reader`].
    fn park_the_reader(&self) {
        super::rig::park_the_reader(&self.view, &self.adjustment, QUIET);
    }

    /// The live `RenderData` — the maps every 2.26j consumer reads.
    fn render_data(&self) -> std::rc::Rc<std::cell::RefCell<crate::preview::qdata::RenderData>> {
        crate::preview::scrib_render_data(&self.view).expect("the preview has render data")
    }

    /// **Activate the control**, exactly as a click does — the one step that makes
    /// this a test of the wiring rather than of the mechanism.
    fn activate_the_disclosure(&self) {
        let toggle = self.render_data().borrow().disclosure_lines[0].1.clone();
        toggle.set_active(!toggle.is_active());
    }

    fn buffer_text(&self) -> String {
        let buf = self.view.buffer();
        buf.slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string()
    }
}

impl Drop for Wired {
    fn drop(&mut self) {
        self.window.destroy();
    }
}

/// The positive control: a full in-place re-render of the same fixture on the same
/// kind of pane, and the trough it produces.
///
/// Returns `(before_upper, min_upper, min_value, page_size)`. Run on its OWN window,
/// because it deliberately damages the pane it runs on — that damage is the point.
fn control_collapse(md: &str) -> (f64, f64, f64, f64) {
    let rig = Wired::present("splicectl", md);
    rig.park_the_reader();
    let before = Reading::of(&rig.adjustment);

    let sw = crate::winstate::state(&rig.window)
        .and_then(|st| st.split.preview_scroller())
        .expect("a preview scroller");
    let folds = crate::fold::FoldState::default();
    crate::preview::re_render(&sw, md, None, 1.0, false, &folds);
    let (min_value, min_upper) = rig.settle_watching_the_trough();
    (before.upper, min_upper, min_value, before.page_size)
}

/// **TDD 2.26h and 2.26i, through the control the reader actually clicks.**
///
/// One test rather than two because they are two assertions about ONE transition and
/// splitting them would need the whole rig twice: 2.26i is about the journey (the
/// trough) and 2.26h about the destination (the reader's own line), and both are read
/// from the same drive.
#[gtktest::test]
fn activating_a_disclosure_above_the_reader_keeps_them_on_their_line() {
    let md = fixture();

    // ── The positive control, first. ────────────────────────────────────────────
    let (ctl_before_upper, ctl_min_upper, ctl_min_value, ctl_page) = control_collapse(&md);
    assert!(
        ctl_min_upper < ctl_before_upper / 2.0,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. A full in-place re-render is \
         documented to collapse the vadjustment's `upper`, and on this fixture it did \
         not: {ctl_before_upper:.0} -> a trough of {ctl_min_upper:.0}. Every number \
         below is therefore meaningless."
    );
    assert!(
        ctl_min_value <= ctl_page,
        "the control collapsed `upper` but never threw `value` to the top (trough \
         {ctl_min_value:.0} against a page of {ctl_page:.0}), so the wired path's \
         `value` number has nothing to be compared against"
    );

    // ── The wired path. ────────────────────────────────────────────────────────
    let rig = Wired::present("splicewired", &md);
    rig.park_the_reader();
    assert!(
        rig.adjustment.upper() > rig.adjustment.page_size() * 8.0,
        "precondition: the fixture must tower over the viewport, or the clamp the \
         control trips is not reachable at all"
    );
    let before = Reading::of(&rig.adjustment);
    assert!(
        before.value > before.page_size,
        "precondition: the reader must be parked well below the top, or 'thrown to \
         the top' and 'stayed put' are the same reading"
    );
    assert!(
        !rig.buffer_text().contains(DEEP_IN_THE_BODY),
        "precondition: the block starts collapsed"
    );

    let top_before = top_line_text(&rig.view);
    let mark = anchor_reader(&rig.view);
    let (offset_before, _) = reader_offset(&rig.view, &rig.adjustment, &mark);

    let trace = Trace::default();
    trace.arm(&rig.adjustment);
    let started = std::time::Instant::now();
    rig.activate_the_disclosure();
    // The toggle defers its work by one idle (GTK4Rs/AP-30), so the splice has not run
    // when `set_active` returns.
    testpump::until(Clock::Idle, "the disclosure to expand", || {
        rig.buffer_text().contains(DEEP_IN_THE_BODY)
    });
    let (min_value, min_upper) = rig.settle_watching_the_trough();
    let elapsed = started.elapsed();
    trace.disarm();

    let after = Reading::of(&rig.adjustment);
    let (offset_after, anchor_text_after) = reader_offset(&rig.view, &rig.adjustment, &mark);
    let drift = offset_after - offset_before;
    println!(
        "\n=== WIRED expand, TDD 2.26h/i ===\n  \
         before   {before}\n  \
         TROUGH   value {min_value:>9.0}  upper {min_upper:>9.0}\n  \
         settled  {after}\n  \
         reader   drift {drift:+.0}px   settle {elapsed:?}\n  \
         line     {top_before:?}\n  \
         writes   {}\n",
        runs_summary(&trace.emissions())
    );

    // 2.26i — the journey. Neither reading may show the excursion the control does.
    assert!(
        min_upper >= before.upper / 2.0,
        "TDD 2.26i: `upper` collapsed on the wired path too ({:.0} -> a trough of \
         {min_upper:.0}), so the toggle is still going through a full re-render",
        before.upper
    );
    assert!(
        min_value > before.page_size,
        "TDD 2.26i: `value` fell to within a screenful of the top (trough \
         {min_value:.0}, page {:.0}) — the reader was thrown back to the start of the \
         document, which is the whole defect",
        before.page_size
    );

    // 2.26h — the destination. The anchor's TEXT first: a route that rebuilt the
    // buffer collapses every mark to offset 0, and the drift below would then describe
    // the top of the document rather than the reader.
    assert_eq!(
        anchor_text_after, top_before,
        "TDD 2.26h: the reader's anchor must still name the line they were reading"
    );
    assert!(
        drift.abs() <= DRIFT_TOLERANCE_PX,
        "TDD 2.26h: the reader's own line moved {drift:+.0}px on screen. The upstream \
         `top_margin` compensation defect drifts it by `emissions x top_margin` \
         (MEASURED 32/368/880px at 0/10/30 anchored children), and \
         `splice::install::ReaderAnchor` exists to put it back — this says it did not."
    );
    assert!(
        after.upper > before.upper,
        "sanity: expanding the block must have made the document taller, or the \
         transition under test did not happen"
    );
}

/// How far the reader's line may move on screen and still count as held.
///
/// A restore computes `y − offset` from integers, so a correct one lands EXACTLY: this
/// is not a tolerance for the mechanism but for anything else that may write the
/// adjustment a pixel either way in the same settle. Deliberately far below the
/// smallest drift the defect produces (32px at zero anchored children), so the guard
/// cannot pass on a restore that did not happen.
const DRIFT_TOLERANCE_PX: f64 = 4.0;

/// **The other direction.** Collapsing a block above the reader shrinks the document,
/// which is where a stale `value` is not merely wrong but unrepresentable: the clamp
/// has a genuinely smaller maximum to answer with.
///
/// Measured separately rather than assumed symmetric with the expand case — TDD 2.26h
/// states both directions, and growth above the viewport can always be compensated
/// where shrinkage past `upper − page_size` cannot.
#[gtktest::test]
fn collapsing_a_disclosure_above_the_reader_keeps_them_on_their_line() {
    let md = fixture().replace("<details>", "<details open>");
    let rig = Wired::present("splicecollapse", &md);
    rig.park_the_reader();
    assert!(
        rig.buffer_text().contains(DEEP_IN_THE_BODY),
        "precondition: the block starts EXPANDED, so the measured toggle collapses it"
    );
    let before = Reading::of(&rig.adjustment);
    let top_before = top_line_text(&rig.view);
    let mark = anchor_reader(&rig.view);
    let (offset_before, _) = reader_offset(&rig.view, &rig.adjustment, &mark);

    rig.activate_the_disclosure();
    testpump::until(Clock::Idle, "the disclosure to collapse", || {
        !rig.buffer_text().contains(DEEP_IN_THE_BODY)
    });
    let (min_value, min_upper) = rig.settle_watching_the_trough();
    let after = Reading::of(&rig.adjustment);
    let (offset_after, anchor_text_after) = reader_offset(&rig.view, &rig.adjustment, &mark);
    let drift = offset_after - offset_before;
    println!(
        "\n=== WIRED collapse, TDD 2.26h/i ===\n  before   {before}\n  \
         TROUGH   value {min_value:>9.0}  upper {min_upper:>9.0}\n  \
         settled  {after}\n  reader   drift {drift:+.0}px\n  line     {top_before:?}\n"
    );

    assert!(
        min_upper >= before.upper / 2.0,
        "TDD 2.26i: `upper` collapsed ({:.0} -> {min_upper:.0})",
        before.upper
    );
    assert!(
        min_value > before.page_size,
        "TDD 2.26i: `value` fell to the top (trough {min_value:.0})"
    );
    assert_eq!(
        anchor_text_after, top_before,
        "TDD 2.26h: the reader's anchor must still name the line they were reading"
    );
    assert!(
        drift.abs() <= DRIFT_TOLERANCE_PX,
        "TDD 2.26h: the reader's own line moved {drift:+.0}px on screen while the \
         block above them closed"
    );
    assert!(
        after.upper < before.upper,
        "sanity: collapsing the block must have made the document shorter"
    );
}

/// **The restore converges in one shot** — MEASURED, not assumed.
///
/// The restore's own `set_value` re-enters `gtk_text_view_value_changed`, which is the
/// function whose compensating writes it exists to undo. If that provoked a fresh burst
/// the fix would oscillate: each correction would move `first_para_top`, each move would
/// compensate again, and the reader would see the position creep. This arms a SECOND
/// recorder after the transition has converged and asserts the adjustment is not
/// written to again.
///
/// The control that makes the zero mean something is the first recorder: it must have
/// seen writes, or "no writes afterwards" would be a statement about a dead instrument.
#[gtktest::test]
fn the_restore_does_not_provoke_a_fresh_compensation_burst() {
    let md = fixture();
    let rig = Wired::present("splicesettle", &md);
    rig.park_the_reader();

    let during = Trace::default();
    during.arm(&rig.adjustment);
    rig.activate_the_disclosure();
    testpump::until(Clock::Idle, "the disclosure to expand", || {
        rig.buffer_text().contains(DEEP_IN_THE_BODY)
    });
    rig.settle_watching_the_trough();
    during.disarm();
    let seen = during.emissions();
    assert!(
        !seen.is_empty(),
        "the oracle is dead: the toggle provoked no adjustment write at all, so the \
         quiet window below would be evidence about the instrument"
    );

    let settled_at = rig.adjustment.value();
    let after = Trace::default();
    after.arm(&rig.adjustment);
    testpump::drain_for(Clock::Frame, Duration::from_millis(500));
    after.disarm();
    println!(
        "\n=== restore convergence ===\n  during {} writes: {}\n  after  {} writes\n",
        seen.len(),
        runs_summary(&seen),
        after.emissions().len()
    );
    assert!(
        after.emissions().is_empty(),
        "the restore provoked {} further adjustment writes, so it did not converge in \
         one shot: {}",
        after.emissions().len(),
        runs_summary(&after.emissions())
    );
    assert_eq!(
        rig.adjustment.value(),
        settled_at,
        "and the position it settled on is the position it stayed at"
    );
}

/// **TDD 2.26j against the wired path** — everything that points into the document
/// still addresses its own text after a real toggle.
///
/// The failure this guards does not look like a failure: a stale map still resolves,
/// still returns text and still names a position — it simply names the wrong one, so
/// nothing appears broken until a reader reads what they actually copied.
///
/// Asserted through the LIVE `RenderData` the running pane holds, not through a
/// `SpliceOutcome` a test drove itself: installing the maps is a separate step from
/// producing them, and a splice that produced correct maps and installed none of them
/// passes the mechanism's own tests (`splice::tests`) while failing every reader.
#[gtktest::test]
fn everything_below_a_toggled_block_still_addresses_its_own_text_in_the_live_pane() {
    let md = fixture();
    let rig = Wired::present("splicemaps", &md);
    rig.park_the_reader();

    // The table BELOW the block, by identity, before the toggle. A full re-render
    // rebuilds it; the splice must not.
    let table_before = rig
        .render_data()
        .borrow()
        .table_anchors
        .last()
        .map(|(_, table)| table.clone())
        .expect("the fixture drew a table below the block");

    rig.activate_the_disclosure();
    testpump::until(Clock::Idle, "the disclosure to expand", || {
        rig.buffer_text().contains(DEEP_IN_THE_BODY)
    });
    rig.settle_watching_the_trough();

    let text = rig.buffer_text();
    let chars: Vec<char> = text.chars().collect();
    let off = |needle: &str| -> i32 {
        let byte = text.find(needle).expect("fixture text is present");
        text[..byte].chars().count() as i32
    };

    let rd = rig.render_data();
    let rd = rd.borrow();

    // Copy, at a position BELOW the toggled block.
    const TAIL: &str = "a distinctive tail paragraph";
    let at = off(TAIL);
    assert_eq!(
        crate::copymap::resolve(
            &rd.copymap,
            &rd.md_owned,
            at,
            at + TAIL.chars().count() as i32
        ),
        TAIL,
        "TDD 2.26j: text below the block must copy as ITSELF, not as its neighbour"
    );

    // The link span below the block still covers the link's own rendered text.
    let link = rd
        .links
        .iter()
        .find(|(_, _, url)| url.contains("example.invalid"))
        .expect("the link below the block is mapped");
    let covered: String = chars[link.0 as usize..link.1 as usize].iter().collect();
    assert_eq!(
        covered, "link text",
        "TDD 2.26j: the link span below the block must cover its own text — activating \
         it is what opens a URL, so a shifted span opens the wrong one"
    );

    // The heading the outline navigates to still names its own line.
    let below = rd
        .heading_sites
        .iter()
        .find(|h| h.slug.as_deref().is_some_and(|s| s.contains("below")))
        .expect("the heading below the block has a site");
    let at_heading: String = chars[below.offset as usize..].iter().take(5).collect();
    assert_eq!(
        at_heading, "Below",
        "TDD 2.26j: the outline's scroll target below the block must name its own line"
    );

    // And find's cell targets: the table below the block is the SAME widget, at a
    // position that really holds a table.
    let (anchor, table_after) = rd
        .table_anchors
        .last()
        .cloned()
        .expect("the table below the block is still mapped");
    assert_eq!(
        table_after, table_before,
        "TDD 2.26j: the table below the block must be the same live widget — a full \
         re-render would have rebuilt it, losing every per-cell map find reads"
    );
    assert_eq!(
        chars[rig.view.buffer().iter_at_child_anchor(&anchor).offset() as usize],
        '\u{fffc}',
        "and its anchor still sits on its own object-replacement character"
    );

    // **What a CLICK resolves, which is the clause the span assertion above does not
    // reach.** The span being right and the hit test being right are two facts; a
    // reader activates the second one. `link_url_at` is the same function the click
    // gesture and the context menu's Copy Link both call, so this is the composition a
    // real activation performs rather than a restatement of `rd.links`.
    //
    // The borrow above must be released first: `link_url_at` takes the view and borrows
    // `RenderData` itself, and holding a `Ref` across it is ScrAP-53's abort rather than
    // a failure.
    let link_start = link.0;
    drop(rd);
    let iter = rig.view.buffer().iter_at_offset(link_start);
    let rect = rig.view.iter_location(&iter);
    let (wx, wy) = rig.view.buffer_to_window_coords(
        gtk::TextWindowType::Widget,
        rect.x() + rect.width() / 2,
        rect.y() + rect.height() / 2,
    );
    assert_eq!(
        crate::preview::interactions::link_url_at(&rig.view, f64::from(wx), f64::from(wy))
            .as_deref(),
        Some("https://example.invalid/target"),
        "TDD 2.26j: a click on the link below the toggled block must resolve to ITS OWN \
         url. A stale span still resolves, still opens something, and takes the reader \
         somewhere plausible — which is why this is asserted rather than eyeballed"
    );
}
