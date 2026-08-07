//! Split-pane scroll synchronisation and cross-mode scroll-position preservation.

use super::*;
use crate::preview::scrib_render_data;
use crate::span::CleanedByteOffset;
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
                let top_char = view_top_offset(&st.editor);
                let orig_byte = char_to_byte(original, top_char as usize);
                let cleaned_byte = crate::annotate::original_to_cleaned(
                    &rd.shifts,
                    crate::span::OriginalByteOffset::new(orig_byte),
                )
                .raw();
                let buf_char = src_byte_to_buf_char(&rd.source_map_inv, cleaned_byte);
                (preview_sw.clone(), preview_view.clone().upcast(), buf_char)
            }
            // Preview drives: preview top buffer char -> CLEANED byte (forward map)
            // -> ORIGINAL byte -> editor char (the editor buffer IS the original).
            ScrollDriver::Preview => {
                let top_char = view_top_offset(&preview_view);
                let cleaned_byte = buf_char_to_src_byte(&rd.source_map, top_char);
                let orig_byte = crate::annotate::cleaned_to_original(
                    &rd.shifts,
                    CleanedByteOffset::new(cleaned_byte),
                )
                .raw();
                let ed_char = byte_to_char(original, orig_byte);
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

/// Byte offset in `text` of the char at `char_off` (clamped to the end).
fn char_to_byte(text: &str, char_off: usize) -> usize {
    text.char_indices()
        .nth(char_off)
        .map_or(text.len(), |(b, _)| b)
}

/// Char offset in `text` of the char containing `byte_off`.
///
/// Delegates to the shared seam (QA round 3, P-1). This used to slice raw, which
/// PANICKED — i.e. aborted the process, from a tick callback on a C trampoline —
/// on a byte offset that landed inside a multi-byte character. Byte offsets here
/// come from the shift table, which does arithmetic and proves nothing about
/// character boundaries.
fn byte_to_char(text: &str, byte_off: usize) -> i32 {
    crate::saferizer::buffer_text::char_offset_at_byte(text, byte_off)
}

/// Forward map: preview buffer char offset -> source byte offset. Largest waypoint
/// whose buffer offset is <= `buf_char` (the map is sorted by buffer offset).
fn buf_char_to_src_byte(map: &[(i32, usize)], buf_char: i32) -> usize {
    let i = map.partition_point(|&(bc, _)| bc <= buf_char);
    i.checked_sub(1).map_or(0, |j| map[j].1)
}

/// Inverse map: source byte offset -> preview buffer char offset. Binary-searches
/// the render's `source_map_inv` (sorted by source byte) for the waypoint with the
/// largest source byte offset <= `src_byte`.
fn src_byte_to_buf_char(inv: &[(usize, i32)], src_byte: usize) -> i32 {
    let i = inv.partition_point(|&(sb, _)| sb <= src_byte);
    i.checked_sub(1).map_or(0, |j| inv[j].1)
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
    re_render(&preview_sw, md, st.doc_dir().as_deref(), zoom, allow_unsafe);
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

/// Fraction scrolled of whatever scroller `content_box` currently shows — the
/// preview, the editor, or (in split) the editor pane.  Used to carry the reading
/// position across a view-mode switch.
pub(super) fn content_scroll_fraction(window: &ApplicationWindow) -> f64 {
    let Some(st) = state(window) else { return 0.0 };
    // In editor-visible modes the editor is the reading position that matters
    // (and, in split, the scroll-sync driver); in pure preview it's the preview.
    if st.view_mode.get().is_editor_visible() {
        preview_scroll_fraction(&st.split.editor_scroller())
    } else {
        st.split
            .preview_scroller()
            .map(|sw| preview_scroll_fraction(&sw))
            .unwrap_or(0.0)
    }
}
/// Restore a scroll `fraction` onto the freshly built content for `mode`, so the
/// reading position is preserved across a view-mode switch.  Different content
/// (source vs rendered) has different heights, hence a fraction rather than an
/// absolute offset. Applies to whichever panes are visible in `mode` (both in
/// split, where the editor drives and the preview follows).
pub(super) fn apply_content_scroll(window: &ApplicationWindow, mode: ViewMode, fraction: f64) {
    let Some(st) = state(window) else { return };
    if mode.is_editor_visible() {
        restore_preview_scroll(&st.split.editor_scroller(), fraction);
    }
    if mode.is_preview_visible() {
        if let Some(preview) = st.split.preview_scroller() {
            restore_preview_scroll(&preview, fraction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{byte_to_char, char_to_byte};

    /// QA round 3, P-1: the shift table hands `byte_to_char` offsets that are
    /// byte ARITHMETIC, not proven character boundaries. Pinned here at the call
    /// site as well as in the seam's own tests, because what matters locally is
    /// that THIS function delegates — an inlined re-implementation would pass
    /// the seam's tests while re-arming the abort here.
    #[test]
    fn a_byte_offset_inside_a_multibyte_char_floors_instead_of_panicking() {
        let text = "aé→😀b";
        assert_eq!(byte_to_char(text, 2), 1, "inside 'é'");
        assert_eq!(byte_to_char(text, 5), 2, "inside '→'");
        assert_eq!(byte_to_char(text, 8), 3, "inside '😀'");
        assert_eq!(byte_to_char(text, 9_999), 5, "past the end clamps");
    }

    /// The paired direction was already total (`nth` yields `None` past the
    /// end); pinned so the pair keeps agreeing at the boundaries.
    #[test]
    fn char_to_byte_round_trips_with_byte_to_char_on_boundaries() {
        let text = "aé→😀b";
        for c in 0..=5 {
            let b = char_to_byte(text, c);
            assert_eq!(byte_to_char(text, b), c as i32, "round trip at char {c}");
        }
    }
}
