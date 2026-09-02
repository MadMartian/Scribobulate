//! Split-pane scroll synchronisation and cross-mode scroll-position preservation.

use super::*;
use crate::preview::scrib_render_data;
use crate::readingpos::{self, DocPosition};
/// The `(editor_sw, preview_sw)` scrollers of the split `SplitView`, when in
/// split mode. Both are distinct persistent widgets on the SplitView (the editor
/// scroller is permanent; the preview scroller is rebuilt per mode switch), so
/// this is inherently swap-agnostic — the `win.split-swap` toggle only changes
/// their allocation order, never which widget is which (unlike the old
/// `GtkPaned` start/end, which the swap flag DID reassign). `None` outside split
/// mode or before a preview exists.
pub(super) fn split_scrollers(
    window: &ApplicationWindow,
) -> Option<(gtk::ScrolledWindow, gtk::ScrolledWindow)> {
    let st = state(window)?;
    if st.view_mode.get() != ViewMode::Split {
        return None;
    }
    let preview = st.split.preview_scroller()?;
    Some((st.split.editor_scroller(), preview))
}
/// Note a scroll-relevant change on one pane's adjustment and schedule a coalesced
/// re-projection. Drops the redundant notifications GtkAdjustment emits during
/// height validation, and ignores the echo from our own `set_value` (the guard).
fn note_adj_change(window: &ApplicationWindow, adj: &gtk::Adjustment, is_editor: bool) {
    let Some(st) = state(window) else { return };
    if st.scroll.guard.get() {
        return; // echo from our own projection's set_value — not a real change
    }
    let cur = (adj.value(), adj.upper());
    let last = if is_editor {
        st.scroll.ed_last.get()
    } else {
        st.scroll.pv_last.get()
    };
    // Epsilon de-dup absorbs GtkScrolledWindow's deferred pixel-snap value-changed.
    if (cur.0 - last.0).abs() < 0.5 && (cur.1 - last.1).abs() < 0.5 {
        return;
    }
    if is_editor {
        st.scroll.ed_last.set(cur);
    } else {
        st.scroll.pv_last.set(cur);
    }
    queue_scroll_sync(window);
}
/// Connect a pane's vadjustment `value-changed` AND `notify::upper` to the coalesced
/// sync. The host window is resolved from the pane widget AT EMISSION TIME
/// (`host_window`), NOT captured — this is what makes the split sync survive a
/// cross-window tab move: the tab's split subtree (Paned + both `GtkScrolledWindow`s)
/// is REUSED, not rebuilt, so these handlers stay connected but must now drive the
/// window they've been re-homed under, not the one they were wired in. Because the
/// handlers live and die with the split scrollers (which ARE rebuilt on every
/// mode change), this dynamic resolution is sufficient on its own — the split sync
/// needs no `scroll_spy_conn`-style re-wire-on-switch tracking. Listening to `upper`
/// (not just `value`) is what lets the projection re-converge as a re-rendered
/// preview's content height settles — without it the two panes' differing `upper`s
/// make the mirrored fractions never cancel (GTK4Rs/AP-16).
fn wire_adj_sync(sw: &gtk::ScrolledWindow, is_editor: bool) {
    let adj = sw.vadjustment();
    adj.connect_value_changed(glib::clone!(
        #[weak]
        sw,
        move |adj| {
            let Some(w) = host_window(&sw) else { return };
            note_adj_change(&w, adj, is_editor);
        }
    ));
    adj.connect_notify_local(
        Some("upper"),
        glib::clone!(
            #[weak]
            sw,
            move |adj, _| {
                let Some(w) = host_window(&sw) else { return };
                note_adj_change(&w, adj, is_editor);
            }
        ),
    );
}
/// Schedule one coalesced scroll re-projection on the next frame-clock tick. At
/// most one tick is outstanding, so the multi-pass validation storm after a
/// `set_buffer` collapses into one projection per frame (GtkSourceMap's
/// `queue_update` pattern). The tick fires in the frame-clock UPDATE phase
/// (BEFORE this frame's layout/allocate — ANTI-PATTERNS deferred-work meta-pattern
/// correction), so it does NOT read post-layout-settled values; instead the
/// projection is idempotent and re-convergent, re-running each frame until the
/// validation thrash settles.
pub(super) fn queue_scroll_sync(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    if st.scroll.tick.borrow().is_some() {
        return; // already scheduled for the next frame
    }
    let id = window.add_tick_callback(glib::clone!(
        #[weak(rename_to = window)]
        window,
        #[upgrade_or]
        glib::ControlFlow::Break,
        move |_w, _clock| {
            if let Some(st) = state(&window) {
                st.scroll.tick.replace(None);
            }
            project_scroll(&window);
            glib::ControlFlow::Break
        }
    ));
    st.scroll.tick.replace(Some(id));
}
/// Re-project the driver pane onto the follower, LINE-ACCURATELY via the render's
/// `source_map` waypoints (Model A spike): map the driver pane's
/// top-of-viewport document position to the follower's corresponding document
/// position, then set the follower's adjustment straight to that position's y-pixel
/// (non-animated). Replaces the old scale-invariant fraction projection, which
/// drifted when one source line rendered at a different height than the next.
/// Keeps all of the fraction path's anti-oscillation machinery unchanged (the
/// `guard`, the input-chosen `driver`, the coalesced tick, `upper` listening).
/// Idempotent — re-runs harmlessly as the follower's layout settles.
fn project_scroll(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let Some((editor_sw, preview_sw)) = split_scrollers(window) else {
        return;
    };
    let Some(preview_view) = preview_sw
        .child()
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
    else {
        return;
    };
    let Some(rd) = scrib_render_data(&preview_view) else {
        return;
    };

    let driver = st.scroll.driver.get();

    // Resolve the follower scroller + view + the buffer offset in it that
    // corresponds to the driver's top-of-viewport document position. The char<->byte
    // conversions read the render's OWN source snapshot (`rd.md_owned`) rather than
    // cloning the live editor text every frame — and it is the snapshot the map was
    // built from, so the two stay mutually consistent even while a debounced
    // re-render lags the editor by a keystroke or two (hardening).
    let (dst_sw, dst_view, dst_offset): (gtk::ScrolledWindow, gtk::TextView, i32) = {
        let rd = rd.borrow();
        // The render maps are in CLEANED coordinates (CriticMarkup extracted), while
        // the EDITOR buffer holds the ORIGINAL source. Translate each position
        // across the shift table (cleaned↔original mapping): editor char↔byte uses
        // `original_owned`; the map side is cleaned bytes. Both translations are
        // identity when `shifts == [(0,0)]`, so an un-annotated document behaves
        // exactly as before — the annotated case is the only one that shifts.
        let original = &rd.original_owned;
        match driver {
            // Editor drives: editor top char -> ORIGINAL byte -> CLEANED byte ->
            // preview buffer char (inverse map, binary-searched).
            ScrollDriver::Editor => {
                let pos = readingpos::from_editor_char(original, view_top_offset(&st.editor));
                let buf_char = readingpos::to_preview_char(&rd.source_map_inv, &rd.shifts, pos);
                (preview_sw.clone(), preview_view.clone().upcast(), buf_char)
            }
            // Preview drives: preview top buffer char -> CLEANED byte (forward map)
            // -> ORIGINAL byte -> editor char (the editor buffer IS the original).
            ScrollDriver::Preview => {
                let pos = readingpos::from_preview_char(
                    &rd.source_map,
                    &rd.shifts,
                    view_top_offset(&preview_view),
                );
                let ed_char = readingpos::to_editor_char(original, pos);
                (editor_sw.clone(), st.editor.clone().upcast(), ed_char)
            }
        }
    };

    let dst_adj = dst_sw.vadjustment();
    let max = dst_adj.upper() - dst_adj.page_size();
    // Range not established yet (a freshly re-rendered follower's `upper` is a
    // stale low estimate) — skip rather than snap it to the top; the coalesced
    // tick retries on the next `notify::upper` (the edit->split top-snap trap).
    if max <= 0.0 {
        return;
    }
    let iter = dst_view.buffer().iter_at_offset(dst_offset);
    // Y of the target line via `line_yrange`, NOT `iter_location` (ScrAP-105).
    // This tick fires in the frame-clock UPDATE phase, BEFORE this frame's
    // layout/allocate, and often right after a `re_render` rebuilt the follower's
    // content. `iter_location` builds+caches a line DISPLAY, inserting it into the
    // view's line-display GSequence which is kept sorted by line number — an insertion
    // that runs its comparator over every neighbouring cached entry. `line_yrange` is a
    // pure cached btree-height READ (validates nothing, touches no display cache — GTK
    // 4.6.9), so it neither caches nor compares, and returning the target line's top-y
    // is exactly what this projection needs; the coalesced tick re-runs and re-converges
    // as heights validate.
    //
    // Note what the `set_value` below is, on a GtkTextView's own vadjustment: NOT a
    // scalar store. It emits `value-changed` synchronously inside this call, and
    // `gtk_text_view_value_changed` updates the IM spot via
    // `gtk_text_view_get_cursor_locations`, which inserts into that same display cache.
    // So this function both avoids caching a display itself AND drives one from GTK,
    // one line apart — worth knowing before treating any adjustment write as cheap or
    // side-effect-free (researcher-verified, 4.6.9).
    let (y, _) = dst_view.line_yrange(&iter);
    let target = (f64::from(y)).clamp(0.0, max);

    // One-stack-frame guard: set_value emits the follower's value-changed
    // synchronously; the guard makes note_adj_change ignore exactly that echo.
    st.scroll.guard.set(true);
    crate::saferizer::scrollpos::jump(&dst_adj, target);
    st.scroll.guard.set(false);
    log::trace!(
        target: "scribobulate::scroll",
        "project_scroll(map): driver={:?} dst_offset={} y={} target={:.1} max={:.1}",
        driver, dst_offset, y, target, max
    );

    // Record the follower's resulting position so its deferred pixel-snap
    // value-changed de-dups out instead of scheduling a redundant tick.
    let snap = (dst_adj.value(), dst_adj.upper());
    match driver {
        ScrollDriver::Editor => st.scroll.pv_last.set(snap),
        ScrollDriver::Preview => st.scroll.ed_last.set(snap),
    }
}

/// Char offset of the iter at the top of `view`'s viewport (GTK4Rs/AP-15: `visible_rect`
/// + `line_at_y`, not `iter_at_location`).
fn view_top_offset(view: &impl IsA<gtk::TextView>) -> i32 {
    crate::saferizer::viewport::ViewportTopIter::top_offset(view)
}

/// Mark `pane` as the active sync driver whenever the user genuinely interacts with
/// it — wheel/touchpad (`EventControllerScroll`), a press anywhere in the pane incl.
/// the scrollbar (`GestureClick`, capture phase), or keyboard focus
/// (`EventControllerFocus`). This is how the bidirectional loop is broken by input
/// source rather than by guessing which `value-changed` was user-driven (GTK4Rs/AP-16).
fn mark_driver_on_input(pane: &gtk::ScrolledWindow, driver: ScrollDriver) {
    // Resolve the host window from the pane at input time (not a captured weak
    // window ref), so a re-homed tab's driver marks the destination window's per-tab
    // scroll state — same reason `wire_adj_sync` resolves dynamically.
    let set_driver = glib::clone!(
        #[weak(rename_to = sw)]
        pane,
        move || {
            let Some(w) = host_window(&sw) else { return };
            if let Some(st) = state(&w) {
                st.scroll.driver.set(driver);
            }
        }
    );

    let scroll_ctrl = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let f = set_driver.clone();
    scroll_ctrl.connect_scroll(move |_, _, _| {
        f();
        glib::Propagation::Proceed
    });
    pane.add_controller(scroll_ctrl);

    // Capture-phase click sees the press before children consume it (covers a
    // scrollbar-drag start); it never claims the event, so normal handling proceeds.
    let click = gtk::GestureClick::new();
    click.set_button(0); // any button
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let f = set_driver.clone();
    click.connect_pressed(move |_, _, _, _| f());
    pane.add_controller(click);

    let focus = gtk::EventControllerFocus::new();
    let f = set_driver.clone();
    focus.connect_enter(move |_| f());
    pane.add_controller(focus);
}
/// Wire the coalesced editor↔preview scroll sync for a freshly entered split, and
/// align the preview to the editor's current position. Replaces the old
/// synchronous, `value-changed`-only mirroring (GTK4Rs/AP-16).
pub(super) fn setup_split_scroll_sync(window: &ApplicationWindow) {
    let Some((_editor_sw, preview_sw)) = split_scrollers(window) else {
        return;
    };
    if let Some(st) = state(window) {
        st.scroll.driver.set(ScrollDriver::Editor);
        st.scroll.guard.set(false);
        st.scroll.ed_last.set((-1.0, -1.0));
        st.scroll.pv_last.set((-1.0, -1.0));
        st.scroll.tick.replace(None);
    }
    // Only the PREVIEW scroller is wired here: it is a fresh widget on every
    // entry into a preview-visible mode, so its handlers/controllers die with it
    // and never accumulate. The EDITOR scroller is persistent for the tab's whole
    // life, so its sync handlers + input-driver controllers are wired exactly
    // once, at tab construction (`wire_persistent_editor_scroll_sync`) — re-wiring
    // it here on every split entry would stack duplicate handlers on the same
    // widget (a leak + double-firing).
    wire_adj_sync(&preview_sw, false);
    mark_driver_on_input(&preview_sw, ScrollDriver::Preview);
    // Align the preview to the editor's current position now.
    queue_scroll_sync(window);
}

/// Wire the split scroll-sync handlers and input-driver controllers on the tab's
/// PERSISTENT editor scroller — called exactly once, at tab construction
/// (`build_window` / `create_tab_in_window`). The editor scroller is mounted for
/// the tab's whole life and never rebuilt, so unlike the preview
/// side (rewired per split entry in `setup_split_scroll_sync`) it must be wired a
/// single time or handlers would accumulate. All handlers resolve the host window
/// dynamically (`host_window`), so they keep driving the correct window across a
/// cross-window tab move (GTK4Rs/AP-52) — and they simply no-op while not in split mode
/// (`project_scroll`/`split_scrollers` return early), so wiring them up-front is
/// harmless in preview/edit.
pub(super) fn wire_persistent_editor_scroll_sync(split: &crate::window::SplitView) {
    let editor_sw = split.editor_scroller();
    wire_adj_sync(&editor_sw, true);
    mark_driver_on_input(&editor_sw, ScrollDriver::Editor);
}
/// Re-render the split-mode preview from `md`, forcing the editor as the
/// scroll-sync driver so the preview's post-render validation noise can never
/// drag the editor (GTK4Rs/AP-16), then queue one coalesced re-projection so the
/// preview lands at the editor's current position. Unlike
/// `rerender_preview_in_place` (zoom.rs), this does NOT capture/restore a
/// preview scroll fraction: these split-only callers rely on the coalesced
/// sync — not a captured fraction — to reposition the preview, because the
/// editor (not the preview) is always the driver here. Shared by the
/// live-preview debounce and an external-reload's split-mode re-render
/// (DRY consolidation).
pub(super) fn rerender_split_preview_driven_by_editor(window: &ApplicationWindow, md: &str) {
    let Some(st) = state(window) else { return };
    let Some((_, preview_sw)) = split_scrollers(window) else {
        return;
    };
    let zoom = st.chrome().zoom_level.get();
    let allow_unsafe = st.allow_unsafe_images.get();
    st.scroll.driver.set(ScrollDriver::Editor);
    st.scroll.pv_last.set((-1.0, -1.0));
    re_render(
        &preview_sw,
        md,
        st.doc_dir().as_deref(),
        zoom,
        allow_unsafe,
        &st.folds.borrow(),
    );
    // The FIRST live re-render after a fresh preview mount is the one whose
    // `set_buffer`-into-an-already-visible-empty-pane can leave the overlay terminally
    // blank. Force one healing follow-up frame for exactly that render.
    if st.split.take_preview_first_render() {
        if let Some(overlay) = preview_sw
            .parent()
            .and_then(|p| p.downcast::<gtk::Overlay>().ok())
        {
            arm_first_content_repaint(&overlay);
        }
    }
    queue_scroll_sync(window);
}

/// Blank-overlay heal: after the first content-bearing re-render since a fresh preview mount,
/// force ONE follow-up frame so the preview overlay can never stay terminally blank.
///
/// A brand-new (empty) doc mounts the preview empty and SHOWS it, then fills it via
/// `set_buffer` while already visible — so its first REAL content-validation is a
/// decoupled `first_validate_idle` pass on an on-screen pane that can carry
/// `alloc_needed` into a terminal paint (a `GtkOverlay` snapshotted without an
/// allocation → whole-pane blank; the GTK4Rs/AP-22/GTK4Rs/AP-23/GTK4Rs/AP-29 family).
/// Researcher-verified (GTK 4.6.9): deferring the render to idle and pre-warming while
/// hidden are both no-ops (the decoupling is intrinsic to `set_buffer`'s own
/// re-invalidation); the working shape is to GUARANTEE a follow-up frame. By the time
/// this `after-paint` fires, the view's first-content validation has settled and the
/// overlay's `alloc_needed` is cleared, so one `queue_draw` repaints it with a real
/// render node — converting a stuck blank into a self-healed one.
///
/// Frame-clock-anchored (not wall-clock — GTK4Rs/AP-122), one-shot (disconnects itself on the
/// first fire), weak overlay (never keeps it alive — ScrAP-152/GTK4Rs/AP-128). Gated to the first
/// render since mount, so steady-state edits pay nothing.
fn arm_first_content_repaint(overlay: &gtk::Overlay) {
    let Some(clock) = overlay.frame_clock() else {
        return; // unmapped: no on-screen pane that could blank
    };
    overlay.queue_draw(); // guarantee a paint so `after-paint` fires
    let overlay_weak = overlay.downgrade();
    let slot: std::rc::Rc<std::cell::RefCell<Option<glib::SignalHandlerId>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_c = slot.clone();
    let id = clock.connect_after_paint(move |clock| {
        // One-shot: unhook self on the first fire — the paint that just ran settled the
        // first-content validation, so a single follow-up frame is enough.
        if let Some(id) = slot_c.borrow_mut().take() {
            clock.disconnect(id);
        }
        if let Some(overlay) = overlay_weak.upgrade() {
            overlay.queue_draw();
        }
    });
    *slot.borrow_mut() = Some(id);
}

/// Where the reader is, as a [`DocPosition`] — captured BEFORE a view-mode swap,
/// from whichever pane the OLD mode was showing.
///
/// The editor is preferred whenever it is visible: in split it is the scroll-sync
/// driver, and in edit it is the only pane there is. Pure preview reads the
/// preview.
///
/// **This deliberately does not return a scroll fraction, which is what it used
/// to.** A fraction is a ratio of view-specific content heights, and the two panes
/// never share one, so every switch re-derived the position from geometry and lost
/// precision doing it. Repeated round trips then accumulated the loss in one
/// direction — measured at four preview↔split trips walking a 40-section fixture's
/// top line 79 → 110 → 152 → 158, terminating clamped at the document's end rather
/// than settling. A document position has no such error to accumulate: it is the
/// same byte offset however many times it is handed across.
pub(crate) fn content_reading_position(window: &ApplicationWindow) -> DocPosition {
    let Some(st) = state(window) else {
        return DocPosition::start();
    };
    // If the pane still sits on the line a previous hand-off wrote, nothing has
    // happened that the stored position does not already describe — hand it back
    // unchanged. Re-deriving it here instead is what made the round trip lossy:
    // a restore does not land its line at exactly the top pixel, so the re-read
    // maps to the preceding waypoint and the pair ratchets one block per trip.
    let stored = st.scroll.applied_reading.get();
    if st.view_mode.get().is_editor_visible() {
        let top = st
            .editor_buf
            .iter_at_offset(view_top_offset(&st.editor))
            .line();
        if let Some((pos, line)) = stored {
            if line == top {
                return pos;
            }
        }
        // The editor buffer IS the original source, so no map is involved.
        return readingpos::from_editor_char(&st.editor_text(), view_top_offset(&st.editor));
    }
    if let Some((pos, line)) = stored {
        if st
            .split
            .preview_scroller()
            .and_then(|sw| crate::preview::preview_top_line(&sw))
            == Some(line)
        {
            return pos;
        }
    }
    let Some(preview_view) = st
        .split
        .preview_scroller()
        .and_then(|sw| sw.child())
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
    else {
        return DocPosition::start();
    };
    let Some(rd) = scrib_render_data(&preview_view) else {
        return DocPosition::start();
    };
    let rd = rd.borrow();
    readingpos::from_preview_char(&rd.source_map, &rd.shifts, view_top_offset(&preview_view))
}

/// Put the reader back at `pos` in whichever panes `mode` shows, AFTER the swap has
/// built them. The counterpart of [`content_reading_position`].
///
/// Each pane resolves the same document position into its own coordinates — the
/// editor directly (its buffer is the original source), the preview through the
/// fresh render's waypoint map. Both then restore by buffer LINE through
/// `restore_preview_scroll_to_line`, which is the validation-safe path
/// (ScrAP-65/ScrAP-13): a freshly built preview has unvalidated line heights, so a
/// raw adjustment write here would land near the top.
pub(super) fn apply_content_reading_position(
    window: &ApplicationWindow,
    mode: ViewMode,
    pos: DocPosition,
) {
    let Some(st) = state(window) else { return };
    // Remember what is being written and where, so the next capture can tell "the
    // reader has not moved" from "the reader is here now" (see `applied_reading`).
    // Recorded against the pane the NEXT capture will read: the editor whenever it
    // is visible, since that is the pane `content_reading_position` prefers.
    let mut written_line = None;
    if mode.is_editor_visible() {
        let char_off = readingpos::to_editor_char(&st.editor_text(), pos);
        let line = st.editor_buf.iter_at_offset(char_off).line();
        written_line = Some(line);
        restore_preview_scroll_to_line(&st.split.editor_scroller(), line);
    }
    if mode.is_preview_visible() {
        if let Some(preview_sw) = st.split.preview_scroller() {
            let line = preview_sw
                .child()
                .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
                .and_then(|view| {
                    let rd = scrib_render_data(&view)?;
                    let rd = rd.borrow();
                    let buf_char = readingpos::to_preview_char(&rd.source_map_inv, &rd.shifts, pos);
                    Some(view.buffer().iter_at_offset(buf_char).line())
                })
                .unwrap_or(0);
            if written_line.is_none() {
                written_line = Some(line);
            }
            restore_preview_scroll_to_line(&preview_sw, line);
        }
    }
    st.scroll
        .applied_reading
        .set(written_line.map(|line| (pos, line)));
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Sections in the fixture below, and the lines each one costs in the buffer —
    /// `## Section n`, a blank, one line of prose, a blank.
    ///
    /// **A BLOCK IS `FIXTURE_LINES_PER_SECTION` LINES, and the settle bound is derived
    /// from it** rather than written as a literal. The crossing between two panes
    /// resolves a position to the waypoint at or before it, so "the block containing the
    /// reader" is a fixture property; a bound spelled as a bare number silently widens
    /// the moment someone edits the fixture, which is how a guard stops guarding without
    /// anyone touching it.
    const FIXTURE_SECTIONS: i32 = 40;
    const FIXTURE_LINES_PER_SECTION: i32 = 4;

    /// Build a window on a fixture tall enough that a drift is visible, mapped and
    /// pumped to a real scroll range.
    fn windowed(app_id: &str) -> (ApplicationWindow, std::rc::Rc<crate::winstate::TabState>) {
        let app = gtk::Application::new(Some(app_id), gtk::gio::ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let mut md = String::new();
        for n in 1..=FIXTURE_SECTIONS {
            md.push_str(&format!(
                "## Section {n}\n\nSome prose for section {n}.\n\n"
            ));
        }
        let window = crate::window::new_window(&app, "IT", &md, None);
        window.set_default_size(800, 600);
        window.present();
        let st = state(&window).expect("tab state");

        // ASSERTED, not slept for. "A real scroll range" is the precondition the whole
        // fixture rests on — the tests park the reader at `upper * 0.5`, which on a view
        // whose content height has not been established yet is a jump to nowhere and
        // silently makes every later reading a measurement of the wrong document. A
        // fixed drain claimed this and checked nothing.
        crate::testpump::until_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(8000),
            "the preview to mount and report a scrollable range",
            || {
                st.split
                    .preview_scroller()
                    .map(|sw| sw.vadjustment())
                    .is_some_and(|adj| adj.upper() > adj.page_size() && adj.page_size() > 0.0)
            },
        );
        settle(&st, "the freshly built preview");
        (window, st)
    }

    fn round_trip(window: &ApplicationWindow, via: &str) {
        for mode in [via, "preview"] {
            crate::window::change_action_state(window, "view-mode", &mode.to_variant());
            // The switch AWAY is where a hand-off reads the pane it is leaving, so the
            // settle belongs after every mode change, not only after the return trip.
            settle(&st_of(window), &format!("the {mode} pane after the switch"));
        }
    }

    fn st_of(window: &ApplicationWindow) -> std::rc::Rc<crate::winstate::TabState> {
        state(window).expect("tab state")
    }

    /// The live viewport top offset of the pane the app would READ right now —
    /// deliberately mirroring [`content_reading_position`]'s own choice of pane, since
    /// that is the read whose timing decides whether a hand-off is honest.
    ///
    /// **This is raw geometry on purpose.** `preview_top_line` prefers
    /// `restore_target_line`, a `Cell` the restore path writes synchronously, so it
    /// answers the same number from the moment a restore is *scheduled* — before any of
    /// the validation this wait exists to outlast. Polling it observes a constant.
    fn live_top_offset(st: &std::rc::Rc<crate::winstate::TabState>) -> i32 {
        if st.view_mode.get().is_editor_visible() {
            return view_top_offset(&st.editor);
        }
        st.split
            .preview_scroller()
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .map_or(-1, |v| view_top_offset(&v))
    }

    /// Hold the precondition every reading in this module depends on: the pane the app
    /// reads has stopped moving.
    ///
    /// **A failure here is NOT a drift failure, and says so.** Folding the two together
    /// is what made these guards flaky across harness, platform and run: an
    /// under-settled pane hands back the top of an unvalidated view (line 0), the
    /// accumulation assertion sees 0 against 78 and reports "that is a ratchet, not
    /// jitter" — a confident claim about the application, produced by a starved test.
    /// MEASURED: shortening the pre-switch wait on an otherwise green Linux run
    /// reproduces exactly the sequence `[0, 78, 0, 78]` and exactly that message.
    fn settle(st: &std::rc::Rc<crate::winstate::TabState>, what: &str) {
        let settled = crate::testpump::until_stable(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(8000),
            std::time::Duration::from_millis(150),
            || live_top_offset(st),
        );
        assert!(
            settled.converged,
            "{what}: the viewport was still moving after 8s (last offset \
             {:?}) — the precondition for reading a position was never established, \
             so this run can say nothing about drift either way",
            settled.value
        );
    }

    /// The preview's top line, read once the pane has stopped moving.
    ///
    /// **Not a fixed sleep** — a restore lands through `scroll_to_mark`, which GTK
    /// re-applies across successive line-height validation passes, so the position
    /// keeps changing for an unbounded number of frames after the switch returns; on an
    /// idle machine it settles in well under 250 ms, and under the load of a full suite
    /// run it does not (ScrAP-13/65, and the same wall-clock-on-a-shared-runner trap the
    /// register's flaky growth-ratio guards fell into).
    ///
    /// **And not a poll of this value either, which is the correction.** The version
    /// that replaced the fixed sleep polled `preview_top_line` for four equal readings —
    /// but that call prefers `restore_target_line`, which the restore path writes
    /// SYNCHRONOUSLY, so what it polled was a constant and it reached its bar on the
    /// minimum five turns every time. MEASURED on this fixture: `polls=5 stable=4
    /// converged=true` on every call, on a healthy run AND on a deliberately starved one
    /// that was reading line 0 off an unvalidated view. A wait that reports success
    /// without ever having observed the thing it waits for is not a wait, and the
    /// convergence flag it did compute was discarded on top. The settling now happens in
    /// [`settle`], against live geometry, and its failure is asserted; this function is
    /// left with the single deterministic read it always wanted.
    fn settled_top_line(st: &std::rc::Rc<crate::winstate::TabState>) -> i32 {
        settle(st, "the preview before reading its top line");
        st.split
            .preview_scroller()
            .and_then(|sw| crate::preview::preview_top_line(&sw))
            .expect("the preview never reported a top line at all")
    }

    /// **TDD 7.5: repeated view-mode round trips do not accumulate a drift.**
    ///
    /// The position used to cross the switch as a scroll fraction — a ratio of two
    /// view-specific content heights — so every crossing re-derived it from
    /// geometry and lost precision at both ends. MEASURED on the pre-fix binary
    /// with this exact fixture: the top line went 79 → 110 → 152 → 158 over four
    /// preview↔split trips and terminated CLAMPED at the end of the document.
    ///
    /// **Why this asserts over four trips and not one.** One round trip is passed
    /// by any conversion accurate to its own rounding, which the fraction was — the
    /// defect is only visible in the REPEAT. A single-trip assertion would have
    /// been green against the code this test exists to prevent coming back.
    ///
    /// **Why the reader is parked mid-document.** The pre-fix drift terminates by
    /// clamping at `upper`. A fixture parked near the bottom clamps on trip one and
    /// then reads as perfectly stable — a stationary measurement taken exactly
    /// where the defect is worst.
    #[gtktest::test]
    fn repeated_view_mode_round_trips_do_not_walk_the_reading_position() {
        let (window, st) = windowed("com.extollit.scribobulate.integrationtest.driftsplit");
        let sw = st.split.preview_scroller().expect("preview mounted");
        let adj = sw.vadjustment();
        crate::saferizer::scrollpos::jump(&adj, adj.upper() * 0.5);
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(300),
        );
        let start = settled_top_line(&st);
        // Captured WITH `start`, not after the trips: read later it reports whichever
        // mode the loop happened to end in, and a `page_size` of 0 from an unmounted
        // pane reads as a broken fixture rather than as a timing artefact of the probe.
        let geometry = fixture_geometry(&st);

        let mut seen = Vec::new();
        for _ in 0..4 {
            round_trip(&window, "split");
            seen.push(settled_top_line(&st));
        }

        assert_no_accumulation(&geometry, start, &seen, "preview↔split");
    }

    /// The contract TDD 7.5 actually states, asserted as two separate things.
    ///
    /// **Stability is the load-bearing half.** Crossing between the two panes costs
    /// a bounded, ONE-TIME settle: the panes hold different text and the render map
    /// resolves a position to the waypoint at or before it, so the first crossing
    /// lands on the block containing the reader rather than the exact pixel. That is
    /// "approximately the same relative position" and is fine. What is not fine is
    /// the same cost being paid AGAIN on every subsequent trip, because that is
    /// unbounded — the pre-fix hand-off walked a 40-section fixture to the end of the
    /// document and clamped there.
    ///
    /// So: every trip after the first must land in exactly the same place, and the
    /// one-time settle must be bounded. Asserting only a tolerance against `start`
    /// would pass a slow ratchet for as long as it stayed inside the tolerance,
    /// which is the defect wearing a smaller step.
    /// The fixture geometry both assertions below are read against, carried into their
    /// failure messages.
    ///
    /// **Because a failure of these guards is usually read from a CI log by someone who
    /// cannot rerun it.** The macOS red that put these guards on the register reported
    /// `start` and the sequence and nothing else, and deciding whether that was a timing
    /// artefact or a genuinely different document meant getting a second machine to
    /// print the content height by hand. Three numbers in the message would have settled
    /// it from the log: they turned out to be IDENTICAL on the runner, on the macOS box
    /// and here (`total=159 upper=2858 page=507 start=79`), which is what ruled out
    /// font metrics and left timing as the only variable.
    fn fixture_geometry(st: &std::rc::Rc<crate::winstate::TabState>) -> String {
        let Some(sw) = st.split.preview_scroller() else {
            return "no preview scroller".to_string();
        };
        let adj = sw.vadjustment();
        let total = sw
            .child()
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .map_or(-1, |v| v.buffer().line_count());
        format!(
            "total_lines={total} upper={} page_size={}",
            adj.upper(),
            adj.page_size()
        )
    }

    fn assert_no_accumulation(geometry: &str, start: i32, seen: &[i32], what: &str) {
        let lo = *seen.iter().min().expect("at least one round trip");
        let hi = *seen.iter().max().expect("at least one round trip");

        // PUBLISHED ON A GREEN RUN, deliberately — an assertion message only reaches the
        // reader when the guard fails, and the number needed to tighten the bound below
        // is the one from a run that PASSED on the environment that used to fail. This
        // costs two lines per suite run in the `gtk_suite` target (whose `main` does not
        // capture) and it is the only way a hosted runner's cost is ever readable.
        eprintln!(
            "scrollsync {what}: start={start} seen={seen:?} \
             one_time_cost={} band={} {geometry}",
            (seen[0] - start).abs(),
            hi - lo
        );

        // THE load-bearing assertion: the readings stay inside a narrow band, so
        // repeating the trip does not walk the reader anywhere. A ratchet of even one
        // block per trip opens the band by roughly a block per trip and fails here;
        // the pre-fix hand-off spread these readings over 50 lines on its way to
        // clamping at the end of the document.
        //
        // A band rather than equality, because the crossing is not bit-exact and was
        // never going to be: `scroll_to_mark` re-applies across validation passes and
        // the render map resolves to block waypoints, so consecutive trips can settle
        // a line or two apart. Demanding equality made this guard fail about one run
        // in three on jitter of ±2 — a flaky gate, which is worse than none.
        const JITTER: i32 = 6;
        assert!(
            hi - lo <= JITTER,
            "{what}: the reading position moved across {} lines over the round trips \
             (sequence {seen:?}, started at {start}; {geometry}) — that is a ratchet, \
             not jitter",
            hi - lo
        );

        // And the one-time cost of crossing between two panes that hold different
        // text is bounded: it may settle onto the block containing the reader, not
        // several blocks away.
        //
        // **FIVE blocks, and the five is headroom that is on its way out.** MEASURED
        // once the pane is genuinely settled, this cost is 1 on ALL THREE platforms with
        // zero variance — Linux 3/3, macOS 10/10 per guard, Windows 10/10 per guard.
        // Before the settle fix it WANDERED with host speed: 11 on Linux; 9/11/13/17/19
        // across ten macOS runs; 11-or-17 across twenty Windows runs, moving back and
        // forth rather than stepping once. All against a bound of 20 — which is how the
        // hosted runner reached 25 and failed this line. The bound was never
        // miscalibrated — it was absorbing an unsettled read, on one line of margin, on
        // boxes that had passed fifteen and twenty times in a row.
        //
        // **Why the multiplier is still 5 when three platforms measure 1.** One
        // environment has not been measured post-fix and it is the one that actually
        // went red: the HOSTED macOS runner, which is slower than any desk here and is
        // where 25 came from. Tightening to one block on three desktops that all read 1
        // would calibrate the bound everywhere except where the defect appeared — the
        // original mistake at a smaller radius. The line published above prints the cost
        // on a PASSING run, so the next CI run supplies that number; tighten the
        // multiplier to 1 then, and argue it from the runner's own figure.
        const SETTLE_LIMIT: i32 = FIXTURE_LINES_PER_SECTION * 5;
        let first = seen[0];
        assert!(
            (first - start).abs() <= SETTLE_LIMIT,
            "{what}: the one-time settle moved the reader from line {start} to \
             {first}, further than the block-granularity crossing should cost \
             (sequence {seen:?}; {geometry}). MEASURED cost with a settled pane is 1 on \
             every platform — a number well above that is an unsettled read, not a \
             wider block"
        );
    }

    /// The same guarantee for the edit round trip, which drifted the OPPOSITE way
    /// before the fix — so a repair that assumed one direction would have moved
    /// this one further rather than fixing it.
    #[gtktest::test]
    fn repeated_edit_round_trips_do_not_walk_the_reading_position_either() {
        let (window, st) = windowed("com.extollit.scribobulate.integrationtest.driftedit");
        let sw = st.split.preview_scroller().expect("preview mounted");
        let adj = sw.vadjustment();
        crate::saferizer::scrollpos::jump(&adj, adj.upper() * 0.5);
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(300),
        );
        let start = settled_top_line(&st);
        // Captured WITH `start`, not after the trips: read later it reports whichever
        // mode the loop happened to end in, and a `page_size` of 0 from an unmounted
        // pane reads as a broken fixture rather than as a timing artefact of the probe.
        let geometry = fixture_geometry(&st);

        let mut seen = Vec::new();
        for _ in 0..4 {
            round_trip(&window, "edit");
            seen.push(settled_top_line(&st));
        }

        assert_no_accumulation(&geometry, start, &seen, "preview↔edit");
    }
}
