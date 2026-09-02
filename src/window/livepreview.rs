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
        let Some(st) = state(&window) else { return };
        if st.loading.get() {
            return;
        }

        // The edit moved every source byte offset after it, and a `FoldKey` IS a source
        // byte offset — so this must happen HERE, on the keystroke, not inside the 300 ms
        // debounce below. A fold toggle clicked during that window reads the fresh editor
        // text against the stale map, which is the same wrong-block collapse by a shorter
        // route. `set_source` is the other caller; this path deliberately does not go
        // through it (the source and baseline are not touched by a live edit).
        st.note_source_offsets_moved();

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

            // Re-render the preview in-place: the GtkTextBuffer's CONTENT is
            // rebuilt (the buffer, the GtkScrolledWindow and its GtkAdjustment all
            // stay alive — replacing the buffer is fatal, see
            // `preview::build::build_render_products_into`). The rebuild triggers
            // GtkTextView's multi-pass height validation, during which the preview
            // adjustment's `upper` thrashes and it emits a storm of
            // notify::upper / value-changed.
            // rerender_split_preview_driven_by_editor forces the editor as the
            // sync driver so that noise can never drag the editor, and lets
            // the coalesced tick re-project editor→preview as the new height
            // settles (GTK4Rs/AP-16). No guard spanning validation — that is
            // unwinnable.
            rerender_split_preview_driven_by_editor(&window, &text);
        });
        pending.set(Some(id));
    });
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// TDD 2.26l — an edit forgets the reader's folds, and does so ON THE KEYSTROKE.
    ///
    /// Not decidable as pure data: the defect was a missing CALL on one path, and the
    /// fold model itself was always correct. Only a test that drives the editor buffer's
    /// own `changed` signal through the wiring this module installs can see it.
    ///
    /// Mutation-checked (POLICY § Typed GTK seams): removing the
    /// `note_source_offsets_moved()` call from the handler leaves the toggle in the map
    /// and fails the first assertion; moving it inside the 300 ms debounce leaves it
    /// there for the length of that window and fails it too, since nothing here pumps.
    #[gtktest::test]
    fn typing_in_split_mode_forgets_every_fold_on_the_keystroke() {
        use crate::fold::{FoldKey, FoldState};
        use gtk::prelude::TextBufferExt;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldinvalidation"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        const DOC: &str = "Lead paragraph.\n\n<details open>\n<summary>One</summary>\n\nBody one.\n\n</details>\n\n<details open>\n<summary>Two</summary>\n\nBody two.\n\n</details>\n";
        let window = crate::window::new_window(&app, "IT-folds", DOC, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let st = state(&window).expect("state registered after new_window");

        // Collapse both blocks, exactly as activating their summaries would.
        let spans = crate::renderer::disclosure::scan_document(DOC);
        assert_eq!(spans.len(), 2, "fixture holds two disclosures");
        for span in &spans {
            st.folds
                .borrow_mut()
                .toggle(FoldKey::from_source_offset(span.start));
        }
        assert_ne!(
            *st.folds.borrow(),
            FoldState::default(),
            "precondition: the reader has folds to lose"
        );

        // One character, typed at the very top — every offset below it, and so every
        // fold key, has just moved.
        let mut at = st.editor_buf.start_iter();
        st.editor_buf.insert(&mut at, "x");

        assert_eq!(
            *st.folds.borrow(),
            FoldState::default(),
            "the keystroke dropped every fold, without waiting for the debounced re-render"
        );
    }
}
