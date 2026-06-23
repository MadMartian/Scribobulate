//! Split-mode live-preview debounce: a 300 ms coalesced re-render of the preview
//! pane (and outline refresh) driven by editor buffer edits. Wired once per tab;
//! the closure is a no-op outside edit/split mode.
use super::*;

/// Wire the editor `buffer`'s change signal to a debounced preview / outline
/// refresh. Takes `content_box` (this tab's own, stable across a
/// cross-window move), not `window` -- QA round-1 H2: a captured
/// window keeps re-rendering the ORIGIN window's active tab after a Move Tab
/// to New Window / cross-window drag, so the moved tab's split preview never
/// re-renders on edit again. Resolving fresh via `tabs::lifecycle::resolve_tab_window`
/// on every fire self-heals across the move.
pub(super) fn wire_live_preview(content_box: &gtk::Box, buffer: &sourceview::Buffer) {
    let sv_buffer = buffer;
    let pending: Rc<Cell<Option<glib::SourceId>>> = Rc::new(Cell::new(None));
    let cb = content_box.downgrade();

    sv_buffer.connect_changed(move |_| {
        let Some(window) = resolve_tab_window(&cb) else {
            return;
        };

        // Guard: live re-render/outline-refresh only matter in editor-backed
        // modes (the preview re-render is split-only; the outline tracks edits
        // in both edit and split).
        if !current_mode(&window).is_editor_visible() {
            return;
        }
        // Ignore programmatic buffer replacement (load / external reload) —
        // those re-render the preview and outline themselves.
        if state(&window).map(|st| st.loading.get()).unwrap_or(false) {
            return;
        }

        // Cancel any already-pending re-render.
        if let Some(id) = pending.take() {
            id.remove();
        }

        // Schedule a fresh 300 ms re-render.
        let pending_c = Rc::clone(&pending);
        let cb_c = cb.clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            pending_c.set(None); // mark this timeout as consumed
            let Some(window) = resolve_tab_window(&cb_c) else {
                return;
            };

            // Re-check mode: user may have left edit/split during the 300 ms.
            let mode = current_mode(&window);
            if !mode.is_editor_visible() {
                return;
            }
            let Some(st) = state(&window) else { return };

            // The outline follows the live buffer in both editor modes.
            refresh_outline(&window);
            // The annotations list follows it too — a comment typed/edited/deleted in
            // the editor gains/updates/drops its row after this debounce (TDD 20.15).
            refresh_annotations(&window);
            // After the rebuild, restore the spy's viewport-based selection
            // (refresh_outline re-selects the last user-activated heading, which
            // may differ from the section currently at the top of the viewport).
            apply_scroll_spy(&window);

            // Only split mode shows a live preview to re-render.
            if mode != ViewMode::Split {
                return;
            }

            // Read live buffer text; the source/baseline are NOT touched.
            let text = st.editor_text();

            // Re-render the preview in-place: only the GtkTextBuffer is
            // replaced; the GtkScrolledWindow and its GtkAdjustment stay
            // alive. set_buffer() triggers GtkTextView's multi-pass height
            // validation, during which the preview adjustment's `upper`
            // thrashes and it emits a storm of notify::upper / value-changed.
            // rerender_split_preview_driven_by_editor forces the editor as the
            // sync driver so that noise can never drag the editor, and lets
            // the coalesced tick re-project editor→preview as the new height
            // settles (ScrAP-16). No guard spanning validation — that is
            // unwinnable.
            rerender_split_preview_driven_by_editor(&window, &text);
        });
        pending.set(Some(id));
    });
}
