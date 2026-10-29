//! Making content inside a collapsed disclosure reachable: expand the folds that
//! hide it, re-render, then act on the render that results.
//!
//! Two features need exactly this and would otherwise each invent it — the outline
//! navigating to a heading inside a collapsed block (rubric 12.22) and find landing
//! on a match inside one (rubric 11.10). Both are the same three steps in the same
//! order, and both fail the same way if the third runs against the old render, so
//! the sequencing lives here once rather than twice (POLICY "prefer extending an
//! existing code path").

use super::*;
use crate::fold::FoldKey;

/// Run `f` on the next main-loop idle against the window and the tab `st` — or not at
/// all, if by then the window has gone, another tab is active, or `st`'s text has
/// changed.
///
/// **This is the only deferral the fold paths have, and the binding is why.** Every
/// caller arms it with a reference into ONE document as it stood at that instant — a
/// fold key resolved against its text, a heading index into its render, a resume
/// offset into its buffer — and all of them are held across the idle (Document-Reference
/// CAM). Two things can be dispatched before the idle and make that reference name the
/// wrong place:
///
/// - **a tab switch** (class B): input outranks `DEFAULT_IDLE`, and a still-validating
///   preview starves the idle for as long as its validation runs (GTK4Rs/T-5). The idle
///   then finds the OTHER document active, and applying the reference there acts on a
///   place its reader never chose;
/// - **a change to the same tab's text** (classes A and B): an edit in split mode, an
///   external reload. `TabState::text_generation` moves on every change to the editor
///   buffer — a reload writes the buffer too — so a reference whose generation is no
///   longer current was resolved against text that is gone.
///
/// Either way the work is DROPPED, not re-aimed: the caller resolved its reference at
/// the event, from what the reader was looking at, and nothing here can re-derive what
/// they meant. Each caller defers the state change itself (the fold expansion, the fold
/// toggle) into `f`, so a dropped deferral leaves the fold model agreeing with the
/// render the reader still sees. The disclosure toggle used to bind nothing but the
/// window, and after a tab switch spliced one document's fold key into the other.
///
/// **The window capture is WEAK.** POLICY's "widget-owned closures capture weakly" rule
/// (GTK4Rs/AP-63): these deferrals are armed from inside a widget's own signal handler,
/// so a strong `ApplicationWindow` in the closure would keep the whole window tree alive
/// for as long as the idle is pending. The tab is re-read from the window, not captured.
///
/// **Why the deferral itself is needed:** a re-render unparents and rebuilds every
/// anchored child of the preview, and every caller reaches here from inside a widget's
/// own signal handler (an outline row's `row-activated`, a find-bar button's `clicked`,
/// a toggle's `toggled`). Tearing down a widget subtree synchronously inside a handler
/// still on the stack is the hazard GTK4Rs/AP-30 records.
pub(crate) fn defer_with_tab(
    window: &ApplicationWindow,
    st: &TabState,
    f: impl FnOnce(&ApplicationWindow, &Rc<TabState>) + 'static,
) {
    let win = window.downgrade();
    let stamp = TabStamp::of(st);
    glib::idle_add_local_once(move || {
        let Some(window) = win.upgrade() else { return };
        let Some(st) = state(&window) else { return };
        if !stamp.still_describes(&st) {
            log::debug!(
                "window::foldreveal: tab {} changed or lost focus before its deferred \
                 fold work ran; dropped",
                stamp.tab
            );
            return;
        }
        f(&window, &st);
    });
}

/// Which document, at which revision, a deferral was armed against.
#[derive(Clone, Copy)]
struct TabStamp {
    tab: crate::winstate::TabId,
    generation: u64,
}

impl TabStamp {
    fn of(st: &TabState) -> Self {
        Self {
            tab: st.id,
            generation: st.text_generation.get(),
        }
    }

    /// `active` is the same tab, and its text has not changed since the stamp.
    fn still_describes(self, active: &TabState) -> bool {
        active.id == self.tab && active.text_generation.get() == self.generation
    }
}

/// Expand every fold in `chain`, re-render the preview, and then run `after` against
/// the window and the tab — with the new render in place, so `after` may read buffer
/// offsets that only exist once the block is open.
///
/// **Bound to the tab that is active NOW, at its text as it stands NOW**
/// ([`defer_with_tab`]). The chain, the heading index and the resume offset `after`
/// carries were all resolved against this document's current text, so the fold
/// expansion waits in the idle with the re-render, and if another tab is active by
/// then, or this one's text has changed, the whole reveal is dropped: nothing
/// expanded, nothing re-rendered, `after` not run. The navigation was overtaken, and
/// the tab it was for is left exactly as the reader saw it, its fold model still
/// agreeing with its render.
///
/// An empty `chain` means nothing is hidden: `after` runs immediately, with no
/// re-render. That is not an optimisation but the common case — the great majority
/// of navigations are to content already on screen, and a re-render there would
/// throw away the reading position for nothing.
///
/// # Why the re-render is deferred to an idle
///
/// A re-render unparents and rebuilds every anchored child of the preview, and the
/// callers reach here from inside a widget's own signal handler — an outline row's
/// `row-activated`, a find-bar button's `clicked`. Tearing down a widget subtree
/// synchronously inside a handler that is still on the stack is the hazard
/// GTK4Rs/AP-30 records, and the disclosure toggle's own handler already defers for
/// it. `after` is queued on the SAME idle, after the re-render, rather than on a
/// second one: two idles would let anything else scheduled in between observe the
/// half-finished state, and the caller would have no way to know its follow-up had
/// been overtaken.
pub(crate) fn reveal_folds(
    window: &ApplicationWindow,
    chain: &[FoldKey],
    after: impl FnOnce(&ApplicationWindow, &Rc<TabState>) + 'static,
) {
    let Some(st) = state(window) else { return };
    if chain.is_empty() {
        after(window, &st);
        return;
    }
    let chain = chain.to_vec();
    defer_with_tab(window, &st, move |window, st| {
        expand_chain(st, &chain);
        let mode = st.view_mode.get();
        crate::window::rerender_preview_in_place(window, mode, RenderShape::ChangedContent);
        after(window, st);
    });
}

/// Expand every fold in `chain` in `st`'s fold model, logging any key that names no
/// disclosure in the source.
fn expand_chain(st: &TabState, chain: &[FoldKey]) {
    // `set_collapsed(.., false)`, never `toggle`. This function's NAME is the
    // postcondition — every key in `chain` is expanded when it returns — and a
    // toggle only delivers that while every key really is collapsed, an invariant
    // held by the caller's caller and enforced by nothing. Hand it an already
    // expanded block and a toggle CLOSES one the reader asked to see, which is the
    // exact inversion of what a reveal is for.
    //
    // The document's own `<details open>` is what the fold model needs to answer
    // that, so the spans are scanned here. Taken as a chain rather than one key at
    // a time because a disclosure nested inside a collapsed one renders nothing —
    // opening the outer block alone would only reveal the inner block's summary
    // line, and the caller would have to re-render once per level of nesting to
    // discover that (`renderer::CollapsedSite`).
    //
    // **The CLEANED text, in the same coordinate space the keys are in.** This read
    // the raw source, and a `FoldKey` is an offset into the text the RENDERER
    // walked — CriticMarkup extracted. On a document with an annotation above a
    // disclosure the two disagree, the lookup below found no span, and the
    // diverged-key fallback flipped the fold instead of expanding it (F-SEC-209).
    let md = st.previewed_cleaned(st.view_mode.get());
    let spans = crate::renderer::disclosure::scan_document(
        &md,
        crate::renderer::frontmatter::Show::AsDisclosure,
    );
    // The DECISION is `FoldState`'s and is unit-tested there; this file owns only
    // the wiring around it and the diagnostic for what it reports back.
    let diverged = st.folds.borrow_mut().expand_chain(&spans, chain);
    for key in diverged {
        log::error!(
            "window::foldreveal: key {key:?} names no disclosure in the current \
             source; nothing was expanded for it"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::testpump::{drain_for, until, Clock};
    use crate::window::testkit::test_app;
    use std::time::Duration;

    /// A collapsed disclosure at byte 5 whose body sits past the summary's preview
    /// limit, so its marker word shows only once the block is open.
    fn doc(heading: &str, marker: &str) -> String {
        format!(
            "# {heading}\n\n<details>\n<summary>S</summary>\n\nfiller filler filler filler \
             filler filler filler filler filler filler filler filler.\n\n{marker}\n\n\
             </details>\n\n# End\n"
        )
    }

    /// The fold key of the fixture's one disclosure.
    fn key_of(md: &str) -> FoldKey {
        FoldKey::from_source_offset(md.find("<details>").expect("the fixture's disclosure"))
    }

    fn preview_text(window: &ApplicationWindow) -> String {
        let view = crate::window::get_preview_sw(window)
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("a preview view");
        let buf = view.buffer();
        buf.slice(&buf.start_iter(), &buf.end_iter(), true)
            .to_string()
    }

    fn first_control(st: &TabState) -> gtk::ToggleButton {
        st.split
            .preview_scroller()
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .and_then(|v| crate::preview::scrib_render_data(&v))
            .and_then(|rd| rd.borrow().disclosure_lines.first().map(|(_, t)| t.clone()))
            .expect("the render emitted a disclosure control")
    }

    /// **A reload between scheduling a reveal and its idle drops the reveal.**
    ///
    /// The chain was resolved against the text the reader was looking at. A reload
    /// that replaces it with a document holding a DIFFERENT block at the same offset
    /// used to have the stale key expand that other block, and `after` act on the new
    /// render with a heading index or resume offset meant for the old one. The control
    /// at the end runs the same reveal against the new text, so the test cannot pass
    /// on a reveal that no longer does anything. Mutation: dropping the generation half
    /// of `TabStamp::still_describes` fails here.
    #[gtktest::test]
    fn a_reload_before_the_reveal_runs_drops_it() {
        let app = test_app("com.extollit.scribobulate.integrationtest.foldrevealreload");
        let old = doc("Old", "OldSecret");
        let new = doc("New", "NewSecret");
        assert_eq!(
            key_of(&old),
            key_of(&new),
            "precondition: the key names both blocks"
        );
        let window = crate::window::new_window(&app, "IT-FOLDRELOAD", &old, None);
        until(Clock::Idle, "the first render", || {
            preview_text(&window).contains("Old")
        });
        let st = state(&window).expect("state");
        let generation = st.text_generation.get();

        let ran = Rc::new(Cell::new(false));
        reveal_folds(&window, &[key_of(&old)], {
            let ran = ran.clone();
            move |_, _| ran.set(true)
        });
        super::super::reload::apply_external_reload(&window, &new);
        assert_ne!(
            st.text_generation.get(),
            generation,
            "precondition: the reload moved the generation the deferral checks"
        );
        drain_for(Clock::Idle, Duration::from_millis(500));

        assert!(
            preview_text(&window).contains("New"),
            "precondition: the reload's render is on screen"
        );
        assert!(
            !ran.get(),
            "the follow-up of a reveal resolved against the old text ran on the new one"
        );
        assert!(
            !preview_text(&window).contains("NewSecret"),
            "a key resolved against the old text expanded a block in the new one"
        );
        assert!(
            st.folds.borrow().is_collapsed(key_of(&new), false),
            "the dropped reveal left the fold model agreeing with the render"
        );

        // Armed against the text now on screen, the same reveal opens the block.
        reveal_folds(&window, &[key_of(&new)], |_, _| {});
        until(Clock::Idle, "a reveal against the current text", || {
            preview_text(&window).contains("NewSecret")
        });

        window.destroy();
    }

    /// **A tab switch between a disclosure click and its idle leaves both tabs alone.**
    ///
    /// The toggle is resolved at the click against the clicked tab's text; the splice
    /// runs in an idle. That idle used to act on whichever tab was active by then, so
    /// a switch dispatched first spliced A's fold key into B — and A's fold model had
    /// already been toggled at the click, disagreeing with A's untouched render. B
    /// holds a block at the same offset, so a key applied there opens it. The control
    /// at the end clicks again on A, so the test cannot pass on a toggle that no longer
    /// does anything. Mutation: binding the toggle's idle to the window alone fails on
    /// B; toggling the fold model at the click fails on A's model.
    #[gtktest::test]
    fn a_tab_switch_before_a_disclosure_toggle_runs_drops_it() {
        let app = test_app("com.extollit.scribobulate.integrationtest.foldtoggletab");
        let md_a = doc("TabA", "SecretA");
        let md_b = doc("TabB", "SecretB");
        let window = crate::window::new_window(&app, "IT-FOLDTOGGLETAB", &md_a, None);
        until(Clock::Idle, "A's first render", || {
            preview_text(&window).contains("TabA")
        });
        let tab_a = state(&window).expect("tab A");
        let toggle = first_control(&tab_a);

        // The click, then the switch, in one turn: the toggle's idle has not run.
        toggle.set_active(!toggle.is_active());
        let tab_b_id =
            crate::window::create_tab_in_window(&window, &md_b, None, false, false).expect("tab B");
        assert_eq!(
            state(&window).map(|s| s.id),
            Some(tab_b_id),
            "precondition: B is active before the toggle's idle runs"
        );
        drain_for(Clock::Idle, Duration::from_millis(500));

        let tab_b = state(&window).expect("tab B");
        assert!(
            preview_text(&window).contains("TabB"),
            "precondition: B's render is on screen"
        );
        assert!(
            !preview_text(&window).contains("SecretB"),
            "a disclosure click in tab A opened a block in tab B"
        );
        assert!(
            tab_b.folds.borrow().is_collapsed(key_of(&md_b), false),
            "a disclosure click in tab A toggled tab B's fold model"
        );
        assert!(
            tab_a.folds.borrow().is_collapsed(key_of(&md_a), false),
            "the dropped toggle left A's fold model agreeing with A's render"
        );

        // Back on A, a click on the control acts.
        let chrome = crate::winstate::chrome(&window).expect("chrome");
        chrome.tabs.focus_page(&tab_a.content_box);
        until(Clock::Idle, "A to be active", || {
            state(&window).map(|s| s.id) == Some(tab_a.id)
        });
        assert!(
            !preview_text(&window).contains("SecretA"),
            "the dropped toggle left A's block as the reader saw it"
        );
        let toggle = first_control(&tab_a);
        toggle.set_active(!toggle.is_active());
        until(
            Clock::Idle,
            "the click on the active tab to open the block",
            || preview_text(&window).contains("SecretA"),
        );

        window.destroy();
    }
}
