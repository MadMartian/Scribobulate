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
        use crate::fold::FoldState;
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
            st.folds.borrow_mut().toggle(span.fold_key());
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

    /// **F-AP-B-105: a click that lands inside the debounce is a STATED no-op.**
    ///
    /// A `FoldKey` is baked into a toggle widget when it is built. In split mode the
    /// preview re-renders on a ~300 ms debounce, so a reader can click a control in the
    /// window between typing and the re-render — and that control's key names the
    /// PREVIOUS document, while the fold map has already been cleared on the keystroke.
    /// The click did nothing at all, silently, and `MANUAL-TEST.md` 2.26l asserted that
    /// it would toggle the block clicked.
    ///
    /// **The loss is not fixed, it is declared.** Making the key survive means
    /// re-deriving it against the new source, which is the "key per-fold state to
    /// something that survives arbitrary edits" problem `crate::fold` removes rather
    /// than solves. What must never happen is a DIFFERENT block toggling, and a stale
    /// key can land on another block's new start offset — so the stamp refuses rather
    /// than gambles.
    #[gtktest::test]
    fn a_toggle_minted_before_a_keystroke_is_refused_rather_than_applied() {
        use crate::fold::FoldState;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldepoch"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        const DOC: &str =
            "Lead paragraph.\n\n<details>\n<summary>One</summary>\n\nBody one.\n\n</details>\n";
        let window = crate::window::new_window(&app, "IT-foldepoch", DOC, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let st = state(&window).expect("state registered after new_window");

        let toggle = st
            .split
            .preview_scroller()
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .and_then(|v| crate::preview::scrib_render_data(&v))
            .map(|rd| rd.borrow().disclosure_lines[0].1.clone())
            .expect("the render emitted a control");

        // The reader types, above the block. Every offset below has moved and the map
        // has been cleared on the keystroke (2.26l).
        let mut at = st.editor_buf.start_iter();
        st.editor_buf.insert(&mut at, "x");
        assert_eq!(
            *st.folds.borrow(),
            FoldState::default(),
            "precondition: the keystroke cleared the map"
        );

        // ...and only THEN clicks the control the previous render built.
        toggle.set_active(!toggle.is_active());

        assert_eq!(
            *st.folds.borrow(),
            FoldState::default(),
            "the stale click changed no fold at all — it was refused, not applied to a \
             key that names the previous document"
        );
    }

    /// **F-AP-B-101: a VIEW-MODE switch is not an edit, and must not forget the folds.**
    ///
    /// The other half of 2.26l's contract, and the one it did not have. `set_source`
    /// cleared unconditionally, and every path leaving an editor-visible mode calls it
    /// with the editor's text whether or not anything was typed — so a reader who
    /// collapsed three blocks in Preview, glanced at Split and came back found them all
    /// open, with nothing having changed underneath them. Ctrl+S on a clean buffer and
    /// the zoom re-render took the same route.
    ///
    /// Two assertions, because the fix has two halves and either alone is a defect: the
    /// MODEL must keep the folds, and the pane the switch rebuilds must be RENDERED at
    /// them. A model that survives a switch onto a pane built at the document's own
    /// state is the same bug with a longer path.
    #[gtktest::test]
    fn switching_view_mode_keeps_the_reader_s_folds_and_renders_at_them() {
        use crate::fold::FoldState;
        use crate::winstate::ViewMode;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldacrossmode"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        // The body is long enough to outrun the collapsed preview's character limit, so
        // its absence means genuinely collapsed rather than merely truncated.
        let body = format!("Body one. {}MARKERONE", "filler ".repeat(20));
        let doc = format!(
            "Lead paragraph.\n\n<details open>\n<summary>One</summary>\n\n{body}\n\n</details>\n"
        );
        let window = crate::window::new_window(&app, "IT-foldmode", &doc, None);
        let st = state(&window).expect("state registered after new_window");

        let spans = crate::renderer::disclosure::scan_document(&doc);
        assert_eq!(spans.len(), 1, "fixture holds one disclosure");
        let key = spans[0].fold_key();

        let shown = || {
            let view = st
                .split
                .preview_scroller()
                .and_then(|sw| sw.child())
                .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
                .expect("a preview view in a preview-visible mode");
            let buf = view.buffer();
            buf.slice(&buf.start_iter(), &buf.end_iter(), true)
                .to_string()
        };

        assert!(
            shown().contains("MARKERONE"),
            "precondition: the document says `open`, so the block starts expanded"
        );

        // The reader closes it. Re-rendered directly rather than by driving the
        // toggle widget: this test is about what a MODE SWITCH does to the fold state,
        // and driving the control would make it a test of the splice as well.
        st.folds.borrow_mut().toggle(key);
        {
            let sw = st.split.preview_scroller().expect("a preview scroller");
            crate::preview::re_render(
                &sw,
                &doc,
                st.doc_dir().as_deref(),
                1.0,
                st.allow_unsafe_images.get(),
                &st.folds.borrow(),
                st.fold_epoch(),
            );
        }
        assert!(
            !shown().contains("MARKERONE"),
            "precondition: the reader's collapse took effect"
        );

        // Preview → Split → Preview. Nothing was typed.
        change_action_state(&window, "view-mode", &"split".to_variant());
        assert_eq!(st.view_mode.get(), ViewMode::Split, "the switch took");
        change_action_state(&window, "view-mode", &"preview".to_variant());

        assert_ne!(
            *st.folds.borrow(),
            FoldState::default(),
            "the MODEL kept the reader's fold across a switch that changed no text"
        );
        assert!(
            !shown().contains("MARKERONE"),
            "and the pane the switch REBUILT was rendered at it — a model that survives \
             onto a pane built at the document's own state is the same defect by a \
             longer route"
        );
    }
}
