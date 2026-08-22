//! Preview zoom: the discrete zoom ladder and the `win.zoom-*` action logic.

use super::*;
const ZOOM_LADDER: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// The window-scoped zoom CSS rule. Scoping by a per-window class (not a bare
/// `textview.scrib-preview`) is load-bearing: the provider is registered on the
/// shared display, so an unscoped selector lets the most-recently-loaded window
/// dictate the font-size for EVERY window's preview (the multi-window zoom
/// regression). See GTK4Rs/AP-77. [`WindowId::of`] is the app's single
/// per-window identity derivation — the SAME value the `winstate` registry keys
/// on — so this CSS scope and the registry key can never diverge for a window
/// (a moved preview re-matches by tree position, so cross-window tab moves need
/// no reparenting).
/// Test-only alias, so `winstate::ids`' scope test can assert that the zoom rule and the
/// window's own CSS class land on the SAME selector — rather than rebuilding both strings
/// itself and comparing its own two copies, which is what it used to do.
#[cfg(test)]
pub(crate) fn zoom_css_rule_for_test(window: &ApplicationWindow, zoom: f64) -> String {
    zoom_css_rule(window, zoom)
}

pub(super) fn zoom_css_rule(window: &ApplicationWindow, zoom: f64) -> String {
    let id = winstate::WindowId::of(window).raw();
    format!(".scrib-win-{id} textview.scrib-preview {{ font-size: {zoom}em; }}")
}
pub(super) fn zoom_step_up(current: f64) -> f64 {
    ZOOM_LADDER
        .iter()
        .find(|&&z| z > current + f64::EPSILON)
        .copied()
        .unwrap_or(current)
}
pub(super) fn zoom_step_down(current: f64) -> f64 {
    ZOOM_LADDER
        .iter()
        .rev()
        .find(|&&z| z < current - f64::EPSILON)
        .copied()
        .unwrap_or(current)
}
/// Set the `win.zoom-in`, `win.zoom-out`, `win.zoom-reset` enabled states.
/// The active zoom factor for `window` (1.0 if it has no state yet). The single
/// reader for the `state → chrome → zoom_level` idiom that was open-coded ~14×
/// (QA L-5).
pub(super) fn zoom_of(window: &ApplicationWindow) -> f64 {
    winstate::state(window)
        .map(|st| st.chrome().zoom_level.get())
        .unwrap_or(1.0)
}

/// All three are disabled in edit mode (no preview to zoom). In preview/split,
/// enabled state tracks position on the discrete zoom ladder.
pub(super) fn update_zoom_action_state(window: &ApplicationWindow) {
    let mode = current_mode(window);
    let preview_visible = mode.is_preview_visible();
    let zoom = zoom_of(window);
    set_action_enabled(
        window,
        "zoom-in",
        preview_visible && zoom < 3.0 - f64::EPSILON,
    );
    set_action_enabled(
        window,
        "zoom-out",
        preview_visible && zoom > 0.5 + f64::EPSILON,
    );
    set_action_enabled(
        window,
        "zoom-reset",
        preview_visible && (zoom - 1.0).abs() > f64::EPSILON,
    );
}
/// The preview `ScrolledWindow` for the current mode, or `None` in edit-only mode.
///
/// QA round-1 L8: delegates to [`tab_preview_sw`] (the active tab, `current_mode`)
/// instead of re-deriving the same swap-aware split-pane rule against a second
/// swap source (`bool_action_state(window, "split-swap", ...)`, the window-level
/// GAction, vs. `tab_preview_sw`'s per-tab `TabState.split_swap` Cell). The two
/// sources are kept in sync for the active tab by `on_active_tab_changed`'s
/// resync, so this was never an observed bug — but a future split-layout change
/// updating only one copy would have been a silent wrong-pane zoom/scroll bug,
/// not a compile error. One rule, one implementation.
pub(super) fn get_preview_sw(window: &ApplicationWindow) -> Option<gtk::ScrolledWindow> {
    let st = winstate::state(window)?;
    tab_preview_sw(&st, current_mode(window))
}
/// Re-render `tab`'s preview into `preview_sw` in place, preserving the
/// reading position across the buffer swap (ScrAP-14/GTK4Rs/AP-15). Reads
/// `mode`-appropriate source text straight off `tab` — in split mode
/// `tab.source` is stale until a mode-switch flush (D7), so the live editor
/// buffer is read instead; in preview mode `tab.source` is authoritative
/// (ScrAP-35's match arm, now with exactly one copy). In split mode also forces
/// the editor as scroll-sync driver before the swap, so the preview's
/// post-render validation noise can never drag the editor (GTK4Rs/AP-16).
///
/// Shared core of [`rerender_preview_in_place`] (the active tab, which also
/// queues a coalesced scroll-sync re-projection afterward) and
/// [`rerender_tab_preview_in_place`] (any background tab during the zoom
/// sweep, which cannot — that queue resolves `state(window)`, the active tab
/// only) — QA round-1 L2: previously a ~10-line capture/force/render/restore
/// sequence hand-duplicated between the two, each keeping its own
/// legitimately-different tail.
fn rerender_and_restore_scroll(
    tab: &Rc<TabState>,
    preview_sw: &gtk::ScrolledWindow,
    mode: ViewMode,
    zoom: f64,
    allow_unsafe: bool,
) {
    let md = match mode {
        ViewMode::Split => tab.editor_text(),
        ViewMode::Preview | ViewMode::Edit => tab.source().clone(),
    };

    // Capture the reading position as a buffer LINE before the swap — this is a
    // SAME-buffer re-render (identical rendered content, only rescaled by the new
    // zoom), so an exact line anchor lands the same line back at the top, with
    // none of the upward drift a pixel fraction suffers here (a fraction mixes
    // tall heading lines and short blank lines, so `fraction × line_count` ≠ the
    // captured line). ScrAP-65.
    let top_line = preview_top_line(preview_sw).unwrap_or(0);

    // In split mode, force the editor as scroll driver so the preview's validation
    // noise during the buffer swap doesn't drive the editor (GTK4Rs/AP-16).
    if mode == ViewMode::Split {
        tab.scroll.driver.set(ScrollDriver::Editor);
        tab.scroll.pv_last.set((-1.0, -1.0));
    }

    re_render(
        preview_sw,
        &md,
        tab.doc_dir().as_deref(),
        zoom,
        allow_unsafe,
    );

    // Restore to the captured line — validation-safe and input-wedge-proof. The
    // sole validation force is `scroll_to_mark`'s internal `flush_scroll` inside
    // the restore idle (`gtk_text_view_get_line_yrange` validates nothing on any
    // GTK 4.6.9 path — the former pre-read primer was vestigial and is gone;
    // ANTI-PATTERNS deferred-work meta-pattern, myth-bust #1)
    // (ScrAP-14/GTK4Rs/AP-15/ScrAP-65).
    restore_preview_scroll_to_line(preview_sw, top_line);
}

/// Re-render the current preview pane in place. The caller only needs to have
/// already updated whichever of zoom/unsafe-images it's toggling; current
/// zoom and unsafe-image setting are read straight off `TabState`. Queues one
/// coalesced scroll-sync re-projection afterward in split mode, ensuring at
/// least one projection fires as the new heights settle.
pub(super) fn rerender_preview_in_place(window: &ApplicationWindow, mode: ViewMode) {
    let Some(st) = winstate::state(window) else {
        return;
    };
    let Some(preview_sw) = get_preview_sw(window) else {
        return;
    };

    let zoom = st.chrome().zoom_level.get();
    let allow_unsafe = st.allow_unsafe_images.get();
    rerender_and_restore_scroll(&st, &preview_sw, mode, zoom, allow_unsafe);

    // In split mode the coalesced tick also re-projects editor→preview as the new
    // heights settle — queuing it here ensures at least one projection fires.
    if mode == ViewMode::Split {
        queue_scroll_sync(window);
    }
}

/// Re-render the active tab's preview from the **live editor buffer** (not the
/// saved baseline `tab.source`), preserving the reading position, in every
/// preview/split mode. Used after a CriticMarkup annotation mutation
/// after a CriticMarkup annotation mutation: the create overlay / marker Edit-Remove edit the editor
/// buffer in place — usually in preview-only mode, where the standard live-preview
/// debounce (`wire_live_preview`) does not fire and would read the stale baseline
/// anyway — so the preview's highlights/markers must be refreshed explicitly here.
pub(crate) fn rerender_preview_from_live_edit(window: &ApplicationWindow) {
    let Some(st) = winstate::state(window) else {
        return;
    };
    let mode = current_mode(window);
    let Some(preview_sw) = get_preview_sw(window) else {
        return; // edit-only mode has no preview (and no annotate overlay)
    };
    let zoom = st.chrome().zoom_level.get();
    let allow_unsafe = st.allow_unsafe_images.get();
    let md = st.editor_text();

    // Persist the annotation into the baseline source. The annotation
    // mutation lives ONLY in the live editor buffer; in preview-only mode the mode
    // switch never flushes editor→source (D7 flushes only when LEAVING an
    // editor-visible mode), so a subsequent fresh render (`render_and_wire_preview`
    // in the view-mode handler reads `st.source`) would rebuild WITHOUT the
    // annotation — it vanished on Preview→Split until a second toggle flushed it
    // back. Flushing here keeps `st.source` == the live buffer at every annotation
    // edit, so any later fresh render reproduces the annotation.
    st.set_source(&md);

    // Refresh the sidebar annotations viewer (the flat list) at this same choke point.
    // Every preview-side annotation CRUD routes through here (the preview sink via
    // `refresh_preview_after_annotation`, and Undo/Redo via `refresh_preview_after_undo_redo`),
    // but this function previously refreshed only the preview PANE — never the sidebar
    // viewer, so in preview-only mode the list stayed stale until some other trigger
    // (mode/tab switch, reload) rebuilt it, violating the POLICY Derived-view CAM
    // (annotations viewer, row 3 column A — in-session mutation must refresh immediately).
    // In split mode the live-preview debounce also rebuilds it, but doing it here too is a
    // harmless, immediate re-run — the POLICY Derived-view CAM requires mode-agnostic,
    // immediate (not self-healing) propagation. `refresh_annotations` reads the source we
    // just flushed (Preview) or the live editor buffer (Split), so it sees the mutation.
    refresh_annotations(window);

    // Fast path: an annotation add/remove leaves the RENDERED text
    // byte-identical (CriticMarkup is stripped; the highlighted words were already plain
    // text), so update only the highlight tags / margin markers / offset maps in place —
    // no `set_buffer`, so the scroll position, anchored children, and paint all stay put.
    // A full `set_buffer` re-render would instead repaint the whole document and reset the
    // scroll to the top before restoring it, making the pane visibly JUMP. Falls through to
    // the full re-render below only if the text somehow changed (the guard inside).
    if refresh_annotations_in_place(
        &preview_sw,
        &md,
        st.doc_dir().as_deref(),
        zoom,
        allow_unsafe,
    ) {
        if mode == ViewMode::Split {
            // The buffer didn't move, but the editor source shifted by the CriticMarkup, so
            // re-project the coalesced split-scroll sync against the new maps.
            queue_scroll_sync(window);
        }
        return;
    }

    // Full re-render fallback (the rendered text DID change — e.g. a CriticMarkup
    // edit shifted a table cell): reuse the shared capture→force→render→restore tail
    // instead of hand-duplicating it (F-DRY-001). `st.source` was just flushed to
    // `md` above, so `rerender_and_restore_scroll` reads back the same text for
    // either mode (Split → live editor, Preview → the just-flushed `st.source`).
    rerender_and_restore_scroll(&st, &preview_sw, mode, zoom, allow_unsafe);
    if mode == ViewMode::Split {
        queue_scroll_sync(window);
    }
}

/// The preview `ScrolledWindow` inside `tab`'s OWN `content_box`, for `mode`
/// (the caller passes `tab.view_mode.get()` — not necessarily the window's
/// current mode). The tab-scoped sibling of `get_preview_sw`/`split_scrollers`,
/// which both implicitly resolve "whichever tab is active" — needed because the
/// zoom sweep must reach EVERY tab in a window, not just
/// the active one.
fn tab_preview_sw(tab: &Rc<TabState>, mode: ViewMode) -> Option<gtk::ScrolledWindow> {
    // The preview scroller is a distinct persistent widget on the tab's
    // SplitView, independent of pane order — so this is swap-agnostic (unlike the
    // old `GtkPaned` start/end lookup, which had to consult `tab.split_swap`).
    // `None` in edit mode (the preview is freed there).
    match mode {
        ViewMode::Split | ViewMode::Preview => tab.split.preview_scroller(),
        ViewMode::Edit => None,
    }
}

/// Re-render `tab`'s own preview pane, preserving its reading position — the
/// per-tab sibling of `rerender_preview_in_place`, which resolves `state
/// (window)` (the ACTIVE tab only) and so cannot be reused for a background
/// tab during the zoom sweep below. Also the re-render primitive `app.rs`'s
/// theme-change sweep (`re_render_all_windows`) uses per tab: an IN-PLACE
/// `re_render` (reusing the SplitView's persistent preview scroller) leaves the
/// editor untouched and keeps scroll-spy / split-sync handlers valid
/// (no rewire), and is a no-op in edit mode (no preview).
pub(crate) fn rerender_tab_preview_in_place(
    tab: &Rc<TabState>,
    mode: ViewMode,
    zoom: f64,
    allow_unsafe: bool,
) {
    let Some(preview_sw) = tab_preview_sw(tab, mode) else {
        return;
    };
    rerender_and_restore_scroll(tab, &preview_sw, mode, zoom, allow_unsafe);
    // No queue_scroll_sync here: it resolves state(window) (the active tab
    // only). Harmless to skip for a background tab — wire_scroll_spy's own
    // idle re-check re-aligns things the moment this tab becomes active.
}

/// Apply a new zoom level to the current window: update CSS, re-render the preview
/// buffer with scaled pixel tags and margins, then restore the scroll position.
/// A no-op in edit mode (zoom actions are disabled there).
pub(super) fn apply_zoom(window: &ApplicationWindow, new_zoom: f64) {
    let Some(st) = winstate::state(window) else {
        return;
    };
    if get_preview_sw(window).is_none() {
        return; // edit mode: nothing to zoom
    }

    // Update the stored zoom level.
    st.chrome().zoom_level.set(new_zoom);

    // Update the per-window CSS provider. load_from_data() triggers a re-evaluation
    // of all matching widgets immediately — no add/remove cycle needed (plan §6).
    // The rule is scoped to THIS window's `.scrib-win-<id>` class so it cannot
    // dictate the font-size of another window's preview (GTK4Rs/AP-77).
    st.chrome()
        .zoom_css_provider
        .load_from_data(&zoom_css_rule(window, new_zoom));

    // Re-render: swap the TextBuffer (new setup_tags pixel values) and update the
    // view's own pixel margins to match the new zoom.
    let mode = current_mode(window);
    rerender_preview_in_place(window, mode);

    // Zoom is window-scoped (operator decision), but each
    // OTHER tab in the window must be re-rendered too — the CSS font-size rule
    // above already reflows every tab's preview for free (it's a shared class),
    // but the pixel-margin-based scaling (heading scale, code-block padding) is
    // set in Rust at render time and does not move on its own. Without this a
    // background tab would show CSS-scaled text next to stale pixel margins the
    // moment the user switched to it.
    for tab in winstate::tabs_for_window(window) {
        if tab.id == st.id {
            continue; // already handled by rerender_preview_in_place above
        }
        let tab_mode = tab.view_mode.get();
        if tab_mode.is_preview_visible() {
            rerender_tab_preview_in_place(&tab, tab_mode, new_zoom, tab.allow_unsafe_images.get());
        }
    }

    // Sync all three zoom action states (ladder boundaries may have changed).
    update_zoom_action_state(window);
}
