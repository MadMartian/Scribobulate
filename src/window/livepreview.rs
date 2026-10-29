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
        // The tab whose buffer fired, which is not necessarily the window's active tab:
        // a session restore or a swap recovery fills a background tab's buffer too.
        let Some(tab) = cb
            .upgrade()
            .and_then(|content_box| crate::winstate::tab_by_content_box(content_box.upcast_ref()))
        else {
            return;
        };

        // The edit moved every source byte offset after it, and a `FoldKey` IS an offset
        // into the text the preview renders from — which in split mode is THIS buffer
        // (`TabState::previewed_source`). So this must happen HERE, on the keystroke,
        // not inside the 300 ms debounce below: a fold toggle clicked during that window
        // reads the fresh editor text against the stale map, which is the same
        // wrong-block collapse by a shorter route.
        //
        // **Above the `loading` guard, deliberately.** A programmatic replacement — an
        // external reload, a session restore, a swap-file recovery — moves this text as
        // surely as typing does, and `set_source`'s own clearing does NOT cover it here
        // (in split mode that field is not the text the keys index, so the flush leaves
        // the map alone by design). Below the guard, a reloaded document kept the
        // previous one's fold keys.
        //
        // On the FIRING tab, by its own mode: the window's active tab is a different
        // document whenever a background tab's buffer is filled.
        if tab.view_mode.get() == crate::winstate::ViewMode::Split {
            tab.note_source_offsets_moved();
        }

        let Some(window) = resolve_tab_window(&cb) else {
            return;
        };
        // Guard: live re-render/outline-refresh only matter in editor-backed
        // modes (the preview re-render is split-only; the outline tracks edits
        // in both edit and split).
        if !current_mode(&window).is_editor_visible() {
            return;
        }
        // The re-render and outline refresh below are the ACTIVE tab's; a background
        // tab is rendered when it is shown.
        let Some(st) = state(&window).filter(|active| active.id == tab.id) else {
            return;
        };

        // Ignore programmatic buffer replacement (load / external reload) beyond that —
        // those re-render the preview and outline themselves.
        if st.loading.get() {
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
        let spans = crate::renderer::disclosure::scan_document(
            DOC,
            crate::renderer::frontmatter::Show::AsDisclosure,
        );
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

    /// **TDD 2.26n — a control follows the block it names across an edit that moved it.**
    ///
    /// The predecessor of this test asserted the opposite (F-AP-B-105): a control minted
    /// before a keystroke was REFUSED, because the only thing it carried was an offset
    /// and a generation stamp, and a bare offset cannot be checked. It now carries the
    /// block's own opening delimiter, so "moved" and "gone" are different answers.
    ///
    /// Mutation-checked (POLICY § Typed GTK seams): drop the `set_reference` call from
    /// `preview::render::anchor_disclosure_control` and the control resolves nothing,
    /// re-derives, and toggles no block — this fails on the first assertion.
    #[gtktest::test]
    fn a_control_minted_before_a_keystroke_follows_its_block_rather_than_refusing() {
        use crate::fold::FoldState;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldfollow"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        const DOC: &str =
            "Lead paragraph.\n\n<details>\n<summary>One</summary>\n\nBody one.\n\n</details>\n";
        let window = crate::window::new_window(&app, "IT-foldfollow", DOC, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let st = state(&window).expect("state registered after new_window");

        let toggle = control(&st).expect("the render emitted a control");

        // The reader types, ABOVE the block: every offset below it has moved, and the
        // map has been cleared on the keystroke (2.26l).
        let mut at = st.editor_buf.start_iter();
        st.editor_buf.insert(&mut at, "x");
        assert_eq!(
            *st.folds.borrow(),
            FoldState::default(),
            "precondition: the keystroke cleared the map"
        );

        // ...and only THEN clicks the control the previous render built.
        toggle.set_active(!toggle.is_active());

        let moved_key = crate::renderer::disclosure::scan_document(
            &st.editor_text(),
            crate::renderer::frontmatter::Show::AsDisclosure,
        )[0]
        .fold_key();
        // The toggle lands in the idle that splices it (`window::defer_with_tab`).
        crate::testpump::until(crate::testpump::Clock::Idle, "the toggle's idle", || {
            !st.folds.borrow().is_collapsed(moved_key, false)
        });
        let folds = st.folds.borrow();
        assert!(
            !folds.is_collapsed(moved_key, false),
            "the click toggled the block at its NEW offset — the control resolved its \
             own text rather than refusing, and rather than acting on the offset the \
             block used to be at"
        );
    }

    /// **TDD 2.26n's ambiguity arm — a repeated identity toggles NO block.**
    ///
    /// Two disclosures whose opening delimiters are identical, plus an edit above them:
    /// nothing in the document distinguishes them any more, and proximity is not
    /// consulted for an identity that is not distinctive (`docref::Ambiguity::Unique`).
    /// A wrong-block toggle is the one outcome this feature promises never happens, so
    /// the answer is neither, plus a re-render so the reader's next click lands.
    ///
    /// Mutation-checked: capture the reference with `Ambiguity::Nearest` instead and
    /// one of the two blocks toggles, failing this.
    #[gtktest::test]
    fn a_control_whose_identity_the_document_repeats_toggles_neither_block() {
        use crate::fold::FoldState;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldambiguous"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        // Identical summaries, far enough apart that an edit larger than half their
        // separation makes the second one's new position nearer the first one's old
        // offset — the shape that resolves to the WRONG block under nearest-match.
        const BLOCK: &str = "<details>\n<summary>Example</summary>\n\nBody.\n\n</details>\n";
        let doc = format!("Lead.\n\n{BLOCK}\n{BLOCK}");
        let window = crate::window::new_window(&app, "IT-foldambig", &doc, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let st = state(&window).expect("state registered after new_window");

        let toggle = control(&st).expect("the render emitted a control");

        let mut at = st.editor_buf.start_iter();
        st.editor_buf.insert(&mut at, &"pad ".repeat(30));
        toggle.set_active(!toggle.is_active());
        // Past the re-render the unresolvable click schedules, so a toggle deferred to
        // an idle would have had its chance to land.
        crate::testpump::drain_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_millis(300),
        );

        assert_eq!(
            *st.folds.borrow(),
            FoldState::default(),
            "neither block was toggled: an identity the document repeats names no place, \
             and guessing between the two is the failure the policy exists to refuse"
        );
    }

    /// **TDD 2.26m / 2.26o — every command that moves the source leaves the controls
    /// acting.**
    ///
    /// The gate this plan was written for, and it is a TABLE rather than six tests on
    /// purpose: a command added to the application adds a row here, where a new test is
    /// something nobody writes. Each row drives a real command in split mode after an
    /// edit and then asks the only question a reader can ask — does the control on
    /// screen still act?
    ///
    /// The defect it holds shut: the disclosure control carried a source offset plus a
    /// generation stamp, and `save_window`'s flush of editor→source bumped that stamp
    /// without re-minting a single control. Every disclosure in the pane went dead,
    /// silently, until the reader typed again — invisible where it was caused and
    /// invisible where it was felt.
    ///
    /// Mutation-checked, and the three mutations land on three different tests, which
    /// is what says each is pulling its own weight: withholding the control's reference
    /// (`preview::render::anchor_disclosure_control`) fails every row here; restoring
    /// the unconditional `note_source_offsets_moved()` in `TabState::set_source` fails
    /// `a_save_in_split_mode_keeps_every_collapsed_block` and no row here, because a
    /// control still ACTS against a map that has been emptied; swapping the ambiguity
    /// policy fails only the repeated-identity test.
    #[gtktest::test]
    fn every_command_that_moves_the_source_leaves_the_disclosure_controls_acting() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldcommands"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        type Drive = fn(&gtk::ApplicationWindow, &Rc<TabState>);
        let commands: &[(&str, Drive)] = &[
            // Ctrl+S: `save_window`'s flush of the editor's text into the tab's source,
            // which is the whole of what a successful save does to in-memory state that
            // a control can see. The write itself is asynchronous and needs a file; it
            // changes nothing here.
            ("save", |_, st| st.set_source(&st.editor_text())),
            // An annotation mutation and a zoom step both take the in-place refresh,
            // which reinstalls the render's maps while KEEPING the widget tree (2.26o).
            ("annotation refresh", |w, _| {
                crate::window::rerender_preview_from_live_edit(w)
            }),
            ("zoom in", |w, _| {
                gtk::prelude::ActionGroupExt::activate_action(w, "zoom-in", None);
            }),
            ("view-mode switch out and back", |w, _| {
                change_action_state(w, "view-mode", &"preview".to_variant());
                change_action_state(w, "view-mode", &"split".to_variant());
            }),
            // A wholesale replacement: the reader's folds are forgotten here by design
            // (the document IS different), so this row asserts only that the controls
            // the reload put on screen act.
            ("external reload", |w, st| {
                crate::window::apply_external_reload(w, &st.editor_text())
            }),
        ];

        for (name, drive) in commands {
            const DOC: &str =
                "Lead paragraph.\n\n<details>\n<summary>One</summary>\n\nBody one.\n\n</details>\n";
            let window = crate::window::new_window(&app, "IT-foldcmd", DOC, None);
            change_action_state(&window, "view-mode", &"split".to_variant());
            let st = state(&window).expect("state registered after new_window");

            // The reader types, then the debounce fires (driven directly rather than
            // waited out — nothing here pumps the main loop).
            let mut at = st.editor_buf.start_iter();
            st.editor_buf.insert(&mut at, "x");
            let text = st.editor_text();
            rerender_split_preview_driven_by_editor(&window, &text);

            drive(&window, &st);

            // Fetched AFTER the command: a mode switch and a reload both rebuild the
            // pane, and the control the reader can click is the one on screen now.
            let toggle = control(&st).unwrap_or_else(|| panic!("{name}: a control"));
            let before = st.folds.borrow().clone();
            toggle.set_active(!toggle.is_active());
            // The toggle lands in the idle that splices it (`window::defer_with_tab`).
            crate::testpump::until_or_for(
                crate::testpump::Clock::Idle,
                std::time::Duration::from_secs(5),
                || *st.folds.borrow() != before,
            );
            assert_ne!(
                *st.folds.borrow(),
                before,
                "{name}: the control the CURRENT render put on screen must act on the \
                 click that follows"
            );
        }
    }

    /// **TDD 2.26m's second half — a save keeps the reader's collapsed blocks.**
    ///
    /// Separate from the table above because it asserts about state the reader can see
    /// rather than about a control acting, and because it is the half that fails
    /// silently: a save that forgets the folds looks like nothing at all until the next
    /// re-render pops every block open.
    #[gtktest::test]
    fn a_save_in_split_mode_keeps_every_collapsed_block() {
        use crate::fold::FoldState;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldsave"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        const DOC: &str =
            "Lead paragraph.\n\n<details open>\n<summary>One</summary>\n\nBody one.\n\n</details>\n";
        let window = crate::window::new_window(&app, "IT-foldsave", DOC, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let st = state(&window).expect("state registered after new_window");

        // The reader types, the debounce re-renders, and then they collapse a block.
        let mut at = st.editor_buf.start_iter();
        st.editor_buf.insert(&mut at, "x");
        let text = st.editor_text();
        rerender_split_preview_driven_by_editor(&window, &text);
        let toggle = control(&st).expect("the render emitted a control");
        toggle.set_active(!toggle.is_active());
        crate::testpump::until(crate::testpump::Clock::Idle, "the toggle's idle", || {
            *st.folds.borrow() != FoldState::default()
        });
        let collapsed = st.folds.borrow().clone();
        assert_ne!(
            collapsed,
            FoldState::default(),
            "precondition: the reader has a collapsed block to lose"
        );

        // Ctrl+S. In split mode the editor buffer is what the preview renders from and
        // what a fold key indexes, so flushing it into `tab.source` moves nothing.
        st.set_source(&st.editor_text());

        assert_eq!(
            *st.folds.borrow(),
            collapsed,
            "a flush that changed nothing the preview is rendered from kept the \
             reader's collapsed blocks"
        );
    }

    /// The disclosure control the pane is showing, in document order.
    fn control(st: &Rc<TabState>) -> Option<gtk::ToggleButton> {
        st.split
            .preview_scroller()
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .and_then(|v| crate::preview::scrib_render_data(&v))
            .and_then(|rd| rd.borrow().disclosure_lines.first().map(|(_, t)| t.clone()))
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

        let spans = crate::renderer::disclosure::scan_document(
            &doc,
            crate::renderer::frontmatter::Show::AsDisclosure,
        );
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
                None,
                &sw,
                &doc,
                st.doc_dir().as_deref(),
                1.0,
                st.allow_unsafe_images.get(),
                &st.folds.borrow(),
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

    /// A programmatic fill of a BACKGROUND tab's buffer (a session restore, a swap
    /// recovery) forgets that tab's folds and leaves the active tab's alone. The
    /// handler acted on the window's active tab, so it cleared the wrong map.
    ///
    /// Mutation: resolve the tab through `state(&window)` again and the active tab's
    /// folds are lost while the background tab keeps its stale ones.
    #[gtktest::test]
    fn filling_a_background_tab_forgets_its_folds_not_the_active_ones() {
        use crate::fold::FoldState;
        use gtk::prelude::TextBufferExt;

        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.foldbackground"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        const DOC: &str = "<details open>\n<summary>One</summary>\n\nBody.\n\n</details>\n";
        let window = crate::window::new_window(&app, "IT-foldbg", DOC, None);
        change_action_state(&window, "view-mode", &"split".to_variant());
        let active = state(&window).expect("the window's tab");
        let background = crate::window::create_tab_in_window(&window, DOC, None, false, true)
            .and_then(crate::winstate::tab_by_id)
            .expect("a background tab");
        assert_eq!(
            state(&window).map(|s| s.id),
            Some(active.id),
            "precondition"
        );
        background.view_mode.set(crate::winstate::ViewMode::Split);
        let key = crate::renderer::disclosure::scan_document(
            DOC,
            crate::renderer::frontmatter::Show::AsDisclosure,
        )[0]
        .fold_key();
        active.folds.borrow_mut().toggle(key);
        background.folds.borrow_mut().toggle(key);

        // A programmatic replacement of the background tab's text.
        background.loading.set(true);
        background.editor_buf.set_text(&format!("x{DOC}"));
        background.loading.set(false);

        assert_eq!(
            *background.folds.borrow(),
            FoldState::default(),
            "the tab whose text moved must forget its folds"
        );
        assert_ne!(
            *active.folds.borrow(),
            FoldState::default(),
            "the active tab's folds name an unmoved document and must survive"
        );
    }
}
