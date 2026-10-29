//! Window-side CriticMarkup annotation mutations. The preview
//! view emits an [`AnnotationEdit`] intent (from a marker popover's Edit/Remove or
//! the selection overlay's Annotate); this module performs it on the document's
//! **editor source buffer** — the same buffer any edit goes through, so the
//! existing dirty-tracking + debounced live re-render pick it up for free
//! (decision 3: reuse the edit path, never a second write path).

use super::*;
use crate::annotations::Direction;
use crate::codeview::{AnnotationEdit, CreateAnnotation};

/// Register the `win.annotate` action — a first-class command so annotation is
/// reachable from a keyboard accelerator (and, later, the menu bar and toolbar),
/// not only the preview selection popover (SSOT rule; cf. the
/// `win.format` precedent). Disabled until a preview selection exists (gated by
/// [`super::update_annotate_action_state`]). Activating it raises the same
/// comment-entry card the popover's Annotate button does, over the current preview
/// selection.
pub(super) fn register_annotate_action(window: &ApplicationWindow) {
    let action = SimpleAction::new("annotate", None);
    action.set_enabled(false);
    action.connect_activate({
        let win = window.downgrade();
        move |_, _| {
            // DEFER raising the card to an idle turn. When this action is activated
            // from the Edit menu, the GtkPopoverMenu is still popping down as the
            // handler runs; on pop-down GTK restores focus to the widget that held
            // it before the menu opened (the editor/preview). Raising the card
            // synchronously here calls `entry.grab_focus()` FIRST, then the menu's
            // focus-restore steals it back — the card's own focus-`leave` handler
            // then immediately hides it, so the card flashes and vanishes ("Annotate
            // does nothing" from the menu, while Ctrl+Alt+M — no menu — works). Idle
            // lets the pop-down + focus-restore fully settle, so our grab wins.
            let win = win.clone();
            glib::idle_add_local_once(move || {
                if let Some(w) = win.upgrade() {
                    trigger_annotate_in_active_pane(&w);
                }
            });
        }
    });
    window.add_action(&action);
}

/// Register `win.next-annotation` and `win.prev-annotation` — the **accessibility**
/// counterpart to `win.annotate`. The margin-marker design puts the comment on a
/// self-drawn margin marker rather than an anchored widget (an anchored `U+FFFC` would
/// shift every buffer-offset consumer), and the accepted cost was that a drawn glyph is
/// not in the a11y tree: a pointer click was the ONLY way to read a comment. These
/// actions are the parallel keyboard path D1 always owed — they walk to the neighbouring
/// annotation and open its popover, whose contents are ordinary accessible widgets.
///
/// **A pair, not a command with a direction.** Forward-only navigation is not a smaller
/// version of navigation: overshooting means cycling the entire document to come back
/// one step, which on a heavily reviewed file is not a shortcut a reader will use. They
/// are two `GAction`s because an accelerator addresses an action by name and GTK has no
/// notion of a parameter carried on a key press; the *behaviour* is one function with a
/// direction argument, which is where the two cannot drift.
///
/// Enabled only while the active document holds at least one annotation (TDD 17.55):
/// with none there is nowhere to walk to, and the greyed-out item says so. The state is
/// driven by [`update_annotation_step_state`] from the annotations-list rebuild.
pub(super) fn register_annotation_step_actions(window: &ApplicationWindow) {
    register_annotation_step(window, "next-annotation", Direction::Next);
    register_annotation_step(window, "prev-annotation", Direction::Previous);
}

/// Enable or disable both annotation-walk actions together — the single source of
/// truth for their menu items and accelerators.
pub(crate) fn update_annotation_step_state(window: &ApplicationWindow, has_annotations: bool) {
    set_action_enabled(window, "next-annotation", has_annotations);
    set_action_enabled(window, "prev-annotation", has_annotations);
}

fn register_annotation_step(window: &ApplicationWindow, name: &str, direction: Direction) {
    let action = SimpleAction::new(name, None);
    action.connect_activate({
        let win = window.downgrade();
        move |_, _| {
            let win = win.clone();
            // Same idle deferral as `win.annotate`, for the same reason: activated from
            // a menu, the GtkPopoverMenu is still popping down and would restore focus
            // over our popover.
            glib::idle_add_local_once(move || {
                if let Some(w) = win.upgrade() {
                    super::annotations_nav::step_annotation(&w, direction);
                }
            });
        }
    });
    window.add_action(&action);
}

/// Raise the comment-entry card for the active pane's selection. Dispatches to the
/// editor OR the preview: prefer the pane that currently holds focus
/// (`WindowChrome::focused_pane`), falling back to the other if the focused one has no
/// usable selection. The editor card writes into the raw source buffer directly; the
/// preview card resolves the selection through the render maps. A no-op when neither pane
/// has an annotatable selection — but the action is gated to exactly that condition
/// (`update_annotate_action_state`), so an enabled activation always finds a target.
fn trigger_annotate_in_active_pane(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let editor_sel = current_mode(window).is_editor_visible() && st.editor_buf.has_selection();
    let preview = preview_text_view(window)
        .and_then(|tv| tv.downcast::<crate::codeview::CodePreviewView>().ok())
        .filter(|v| {
            if v.annotation_sink().is_none() {
                return false;
            }
            // Buffer selection OR a table-cell label selection (table-cell annotation).
            v.buffer().has_selection()
                || crate::preview::scrib_labels(v).is_some_and(|labels| {
                    labels
                        .borrow()
                        .iter()
                        .any(|l| l.selection_bounds().is_some())
                })
        });
    let prefer_editor = winstate::chrome(window)
        .map(|ch| ch.focused_pane.get() == winstate::FocusedPane::Editor)
        .unwrap_or(true);
    // `&&`/`||` short-circuit: fire the preferred pane if it has a usable selection,
    // otherwise the other; each trigger returns whether it actually ran.
    let _fired = if prefer_editor {
        (editor_sel && st.split.trigger_editor_annotate())
            || preview.as_ref().is_some_and(|v| v.trigger_annotate())
    } else {
        preview.as_ref().is_some_and(|v| v.trigger_annotate())
            || (editor_sel && st.split.trigger_editor_annotate())
    };
}

/// Apply an [`AnnotationEdit`] to the editor source buffer. The edit's byte ranges
/// are offsets into the current source (which the editor buffer holds verbatim),
/// so each mutation is a pure `String -> String` transform ([`crate::annotate`]);
/// we then splice only the *minimal changed region* back into the buffer so the
/// caret, selection, and undo history outside that region are preserved.
/// The live document source of `window`'s active tab — the editor buffer's RAW Markdown,
/// which is the canonical source every annotation mutation is computed against
/// (`apply_annotation_edit` reads exactly this).
///
/// Exposed because the preview's comment card holds only the *cleaned* document
/// plus the shift map, but deciding which existing comments a selection will merge is a
/// question about the source. Going through the same buffer the mutation will read keeps
/// the card's preview of the merge and the merge itself looking at one text.
pub(crate) fn document_source(window: &ApplicationWindow) -> Option<String> {
    let st = state(window)?;
    let buf = &st.editor_buf;
    Some(crate::saferizer::BufferText::of(buf).into_string())
}

pub(crate) fn apply_annotation_edit(buf: &gtk::TextBuffer, edit: AnnotationEdit) {
    let old = crate::saferizer::BufferText::of(buf).into_string();
    let new = match edit {
        // Both anchored mutations RE-LOCATE their construct in the live source
        // before touching it. The card that issued this edit was built against an
        // earlier state of the document — the user can type, undo, or trip the
        // 300 ms split-mode re-render while it sits open — so the offsets it
        // captured are a hint about where to look, never an instruction about
        // where to cut. `None` means the construct is gone; fall through to a
        // clean no-op rather than splice at coordinates that now address
        // something else (ScrAP-187: a stale range destroyed unrelated text, and
        // out of bounds it panicked the process).
        AnnotationEdit::EditComment {
            construct,
            body,
            text,
        } => {
            let Some(at) = construct.resolve(&old) else {
                return;
            };
            let body = crate::docref::AnchoredSpan::absolute(&at, &body);
            match crate::annotate::edit_comment(&old, body, &text) {
                Some(new) => new,
                None => return,
            }
        }
        AnnotationEdit::Remove {
            construct,
            keep_inner,
        } => {
            let Some(at) = construct.resolve(&old) else {
                return;
            };
            let keep = keep_inner.map(|r| crate::docref::AnchoredSpan::absolute(&at, &r));
            match crate::annotate::remove_annotation(&old, at, keep) {
                Some(new) => new,
                None => return,
            }
        }
        AnnotationEdit::Create(CreateAnnotation::Highlight { target, comment }) => {
            // Re-located in the live source first, exactly as the two mutations above
            // are: the card was raised against an earlier state of the document and has
            // been open ever since.
            let Some(range) = target.resolve(&old) else {
                return;
            };
            // Never written into front matter, code, an HTML block or math (TDD 17.59).
            match crate::annotate::landing::for_highlight(&old, range) {
                // Extend/edit an intersecting highlight rather than nesting a new one.
                crate::annotate::Landing::Highlight(range) => {
                    crate::annotate::insert_or_extend_highlight(&old, range, &comment)
                }
                crate::annotate::Landing::Point(slot) => {
                    crate::annotate::insert_landed_point(&old, slot, &comment)
                }
            }
        }
        AnnotationEdit::Create(CreateAnnotation::Point { target, comment }) => {
            let Some(at) = target.resolve(&old).map(|r| r.end) else {
                return;
            };
            crate::annotate::insert_landed_point(
                &old,
                crate::annotate::landing::for_point(&old, at),
                &comment,
            )
        }
        // Task checkbox toggle: a single `[ ]`↔`[x]`
        // state-char flip located in the LIVE source (`old`). Pure `&str -> String`
        // like the annotation transforms; `None` (no well-formed marker at the span)
        // yields `new == old` below → a clean no-op. `splice_minimal` then makes the
        // one-char replace a single grouped, undoable user-action, exactly as for an
        // annotation edit.
        AnnotationEdit::ToggleTask { span } => {
            crate::tasklist::toggle_task_marker(&old, span).unwrap_or_else(|| old.clone())
        }
    };
    if new == old {
        return;
    }
    splice_minimal(buf, &old, &new);
}

/// Re-render the preview so a just-applied annotation (create / edit / remove)
/// shows immediately. The mutation edits the editor buffer in place — typically in
/// preview-only mode, where the split-only live-preview debounce never fires — so
/// the preview's highlights and markers would otherwise stay stale until a manual
/// reload (the sub-agent's §17.5–17.8 finding). Routes through the same in-place
/// re-render the zoom/reload paths use, reading the LIVE buffer.
pub(crate) fn refresh_preview_after_annotation(view: &crate::codeview::CodePreviewView) {
    if let Some(window) = super::host_window(view) {
        super::rerender_preview_from_live_edit(&window);
    }
}

/// Replace only the shortest differing middle segment of the buffer, computed by
/// common char prefix/suffix — one grouped, undoable edit that leaves everything
/// outside the changed region (and the caret there) untouched.
fn splice_minimal(buf: &gtk::TextBuffer, old: &str, new: &str) {
    // Streamed rather than collected into a `Vec<char>` (U-4: the whole-document
    // `Vec<char>` of BOTH `old` and `new` — ~4 bytes/char, doubled — ran on every
    // annotation edit and every task-checkbox toggle, most of which change a
    // handful of characters out of a document that can be many megabytes).
    let p = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let old_len = old.chars().count();
    let new_len = new.chars().count();
    let old_tail = old_len - p;
    let new_tail = new_len - p;
    let s = old
        .chars()
        .rev()
        .zip(new.chars().rev())
        .take(old_tail.min(new_tail))
        .take_while(|(a, b)| a == b)
        .count();
    let start = p as i32;
    let end = (old_len - s) as i32;
    let insertion: String = new.chars().skip(p).take(new_tail - s).collect();

    // One discrete undo step, with the redo-merge barrier flushed before it — see
    // `window::undo::UndoGroup`. Held to the end of scope so the edit below is
    // recorded inside its user action; `Drop` closes it.
    let _grp = crate::window::undo::UndoGroup::new(buf);
    let mut a = buf.iter_at_offset(start);
    let mut b = buf.iter_at_offset(end);
    buf.delete(&mut a, &mut b);
    if !insertion.is_empty() {
        let mut at = buf.iter_at_offset(start);
        buf.insert(&mut at, &insertion);
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn buf_with(text: &str) -> gtk::TextBuffer {
        let b = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        b.set_text(text);
        b
    }
    fn text_of(b: &gtk::TextBuffer) -> String {
        crate::saferizer::BufferText::of(b).into_string()
    }

    /// U-4: `splice_minimal` streams its common-prefix/suffix scan directly over
    /// `old`/`new` (rather than collecting each into a `Vec<char>`) — multi-byte
    /// chars are exactly where a char-boundary slip would hide, so this exercises
    /// a change inside, and straddling, multi-byte runs.
    #[gtktest::test]
    fn splice_minimal_is_char_exact_across_multibyte_runs() {
        for (old, new) in [
            // Change entirely inside a run of multi-byte chars.
            ("café ☕ déjà vu", "café 🍵 déjà vu"),
            // Common prefix/suffix themselves end in multi-byte chars.
            ("préfixe→milieu←suffixe", "préfixe→AUTRE←suffixe"),
            // One side is a strict prefix of the other (empty common suffix).
            ("emoji: 😀", "emoji: 😀😀😀"),
            // No common material at all.
            ("完全に違う", "differ entirely"),
            // Identical strings but for a single leading multi-byte char.
            ("🔥sparkles", "✨sparkles"),
        ] {
            let buf = buf_with(old);
            splice_minimal(&buf, old, new);
            assert_eq!(text_of(&buf), new, "old={old:?} new={new:?}");
        }
    }

    fn undo_enabled_buf(text: &str) -> gtk::TextBuffer {
        let b = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        b.set_enable_undo(true);
        b.begin_irreversible_action();
        b.set_text(text);
        b.end_irreversible_action();
        b
    }
    /// The Create edit a comment card issues: the target anchored to its own text,
    /// exactly as `preview::annotate::capture_selection` builds it.
    fn highlight_in(source: &str, range: std::ops::Range<usize>, comment: &str) -> AnnotationEdit {
        AnnotationEdit::Create(CreateAnnotation::Highlight {
            target: crate::docref::AnchoredSpan::capture_in_context(
                source,
                range,
                crate::docref::Ambiguity::Nearest,
            )
            .expect("a valid range over the source"),
            comment: comment.into(),
        })
    }

    /// TDD 17.59 at the choke point every annotate surface writes through (the preview's
    /// card and the editor pane's card alike): neither a highlight nor a point comment is
    /// ever written into a block other tools read verbatim — front matter here; the
    /// landing module's own tests cover each kind. Mutation note: route either
    /// create arm of `apply_annotation_edit` past the landing module and its half fails.
    #[gtktest::test]
    fn no_annotation_is_ever_written_into_front_matter() {
        let fm = "---\ntitle: Hello\n---\n\nBody.\n";
        let after = "---\ntitle: Hello\n---\n{>>note<<}\n\nBody.\n";

        let buf = undo_enabled_buf(fm);
        apply_annotation_edit(
            &buf,
            highlight_in(fm, 0..fm.find("\n\n").unwrap() + 1, "note"),
        );
        assert_eq!(
            text_of(&buf),
            after,
            "a highlight over the block lands after it"
        );

        let buf = undo_enabled_buf(fm);
        let at = fm.find("Hello").unwrap();
        apply_annotation_edit(
            &buf,
            AnnotationEdit::Create(CreateAnnotation::Point {
                target: crate::docref::AnchoredSpan::capture_in_context(
                    fm,
                    at..at + 5,
                    crate::docref::Ambiguity::Nearest,
                )
                .expect("a valid range over the source"),
                comment: "note".into(),
            }),
        );
        assert_eq!(
            text_of(&buf),
            after,
            "a point comment inside the block lands after it"
        );
    }

    fn action_enabled(window: &ApplicationWindow, name: &str) -> bool {
        use gtk::prelude::*;
        window
            .lookup_action(name)
            .unwrap_or_else(|| panic!("{name} action missing"))
            .is_enabled()
    }

    /// Row count of the sidebar annotations viewer (its `GtkListView` model's
    /// `n_items`), or 0 when the viewer shows the "no annotations" placeholder.
    fn annotations_viewer_count(window: &ApplicationWindow) -> usize {
        use gtk::prelude::*;
        state(window)
            .and_then(|st| st.chrome().annotations_scroller.child())
            .and_then(|c| c.downcast::<gtk::ListView>().ok())
            .and_then(|lv| lv.model())
            .map(|m| m.n_items() as usize)
            .unwrap_or(0)
    }

    /// Adding an annotation in PREVIEW-ONLY mode refreshes the sidebar annotations
    /// viewer immediately — the mutation's re-render choke point
    /// (`rerender_preview_from_live_edit`) must rebuild the flat list, not only the
    /// preview pane (TDD 20.17; POLICY Derived-view CAM row 3 column A). Before the fix
    /// the viewer stayed stale until a mode/tab switch or reload forced a rebuild.
    /// Mutation note: drop the `refresh_annotations` call from the choke point and the
    /// post-edit count stays 1.
    #[gtktest::test]
    fn annotating_in_preview_only_mode_refreshes_the_sidebar_viewer() {
        use gtk::prelude::*;
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.previewannrefresh"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");

        // A window opens in Preview mode. Start with exactly one annotation so the
        // viewer is a populated ListView (not the placeholder).
        let window = crate::window::new_window(&app, "IT", "a {==claim==}{>>note<<} b gamma", None);
        assert_eq!(
            annotations_viewer_count(&window),
            1,
            "precondition: the viewer shows the one existing annotation"
        );

        // Add a SECOND annotation the way a preview-only annotate does: mutate the
        // editor source buffer, then run the shared re-render choke point.
        let st = state(&window).expect("state");
        let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
        let src = text_of(&buf);
        let g = src.find("gamma").expect("gamma present");
        apply_annotation_edit(&buf, highlight_in(&src, g..g + "gamma".len(), "second"));
        crate::window::rerender_preview_from_live_edit(&window);

        assert_eq!(
            annotations_viewer_count(&window),
            2,
            "annotating in preview-only mode must refresh the sidebar viewer immediately \
             (TDD 20.17 / Derived-view CAM), not leave it stale at 1"
        );

        window.destroy();
    }

    /// TDD 17.55: Next/Previous Annotation are enabled iff the document has an
    /// annotation, and follow it as annotations come and go (annotate, undo, redo).
    #[gtktest::test]
    fn annotation_walk_is_enabled_only_while_the_document_has_an_annotation() {
        use gtk::prelude::*;
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.annotationwalkgate"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        let walk_enabled = |w: &ApplicationWindow| {
            let next = action_enabled(w, "next-annotation");
            assert_eq!(
                next,
                action_enabled(w, "prev-annotation"),
                "Next and Previous Annotation must share one enabled state"
            );
            next
        };

        let annotated =
            crate::window::new_window(&app, "IT", "{==alpha==}{>>a comment<<} beta", None);
        assert!(
            walk_enabled(&annotated),
            "a document opened with an annotation enables the walk"
        );
        annotated.destroy();

        let window = crate::window::new_window(&app, "IT", "alpha beta", None);
        let st = state(&window).expect("state after new_window");
        assert!(
            !walk_enabled(&window),
            "a document with no annotations disables the walk"
        );

        // Annotate the way the preview sink does: edit the source, then refresh.
        let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
        apply_annotation_edit(&buf, highlight_in(&text_of(&buf), 0..5, "one"));
        crate::window::rerender_preview_from_live_edit(&window);
        assert!(walk_enabled(&window), "annotating enables the walk");

        window.lookup_action("undo").unwrap().activate(None);
        assert!(
            !walk_enabled(&window),
            "undoing the only annotation disables the walk"
        );
        window.lookup_action("redo").unwrap().activate(None);
        assert!(walk_enabled(&window), "redoing it enables the walk again");

        window.destroy();
    }

    #[gtktest::test]
    fn undo_redo_are_enabled_in_preview_mode_after_annotating() {
        use gtk::prelude::*;
        // A window opens in Preview mode (editor not visible). Annotations are the
        // one document mutation reachable there (preview selection → Annotate), so
        // Undo/Redo MUST be usable in preview or an annotation can't be undone where
        // it was made — the reported "annotations don't respect undo" bug.
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.undopreview"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        let window = crate::window::new_window(&app, "IT", "alpha beta", None);
        let st = state(&window).expect("state after new_window");

        // Precondition: fresh document, nothing to undo → Undo disabled.
        assert!(
            !action_enabled(&window, "undo"),
            "no history yet → undo disabled"
        );

        // Annotate directly on the editor source buffer (what the preview Annotate
        // flow does) while still in Preview mode.
        let buf: gtk::TextBuffer = st.editor_buf.clone().upcast();
        apply_annotation_edit(&buf, highlight_in(&text_of(&buf), 0..5, "one"));

        assert!(
            action_enabled(&window, "undo"),
            "undo must be enabled in preview mode once an annotation exists"
        );

        window.lookup_action("undo").unwrap().activate(None);
        assert!(
            action_enabled(&window, "redo"),
            "redo must be enabled in preview mode after undoing the annotation"
        );

        window.destroy();
    }

    #[gtktest::test]
    fn redo_then_new_annotation_are_two_independent_undo_steps() {
        // Repro of the reported bug: annotate, undo, redo, annotate again — then a
        // single Undo must revert ONLY the second annotation, not both.
        let b = undo_enabled_buf("alpha beta");
        let base = text_of(&b);

        // 1. annotate "alpha" (0..5)
        apply_annotation_edit(&b, highlight_in(&text_of(&b), 0..5, "one"));
        let after_a = text_of(&b);
        assert_ne!(after_a, base);

        // 2. undo it, 3. redo it
        b.undo();
        assert_eq!(text_of(&b), base);
        b.redo();
        assert_eq!(text_of(&b), after_a);

        // 4. annotate "beta" — its offset shifted by the first annotation's markup.
        let beta_start = after_a.find("beta").unwrap();
        apply_annotation_edit(
            &b,
            highlight_in(&after_a, beta_start..beta_start + "beta".len(), "two"),
        );
        let after_b = text_of(&b);
        assert_ne!(after_b, after_a);

        // 5. one Undo must revert ONLY the second annotation, back to `after_a`.
        b.undo();
        assert_eq!(
            text_of(&b),
            after_a,
            "one Undo after redo+annotate reverted more than the last annotation"
        );
    }

    #[gtktest::test]
    fn an_annotation_edit_is_a_single_undoable_step() {
        // Mirror the real editor buffer: undo enabled, initial text loaded as an
        // irreversible baseline (as build_tab_editor does), so only the annotation
        // edit is on the stack.
        let b = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        b.set_enable_undo(true);
        b.begin_irreversible_action();
        b.set_text("the earth is flat here");
        b.end_irreversible_action();

        apply_annotation_edit(&b, highlight_in(&text_of(&b), 13..17, "citation needed"));
        assert_eq!(
            text_of(&b),
            "the earth is {==flat==}{>>citation needed<<} here"
        );
        assert!(b.can_undo(), "the annotation edit must be undoable");

        b.undo();
        assert_eq!(
            text_of(&b),
            "the earth is flat here",
            "one undo must fully revert the annotation (delete+insert grouped)"
        );
        assert!(b.can_redo(), "the reverted annotation must be redoable");

        b.redo();
        assert_eq!(
            text_of(&b),
            "the earth is {==flat==}{>>citation needed<<} here"
        );
    }

    #[gtktest::test]
    fn create_highlight_wraps_the_selected_source_range() {
        let b = buf_with("the earth is flat here");
        apply_annotation_edit(&b, highlight_in(&text_of(&b), 13..17, "citation needed"));
        assert_eq!(
            text_of(&b),
            "the earth is {==flat==}{>>citation needed<<} here"
        );
    }

    /// A comment aimed at a word whose neighbouring text was edited while the card was
    /// open writes NOTHING, never a highlight on another copy of the same word: the
    /// write path is where a wrong guess would damage the document.
    ///
    /// Mutation: let a context-held target fall back to the unfiltered search, and the
    /// first `the` gets the comment.
    #[gtktest::test]
    fn a_comment_whose_target_lost_its_neighbours_writes_nothing() {
        let b = buf_with("the cat. the dog.");
        let edit = highlight_in(&text_of(&b), 9..12, "which one?");
        b.set_text("the cat. the cow.");
        apply_annotation_edit(&b, edit);
        assert_eq!(
            text_of(&b),
            "the cat. the cow.",
            "a target that no longer resolves must change nothing"
        );
    }

    /// Build the pair a card carries: the construct anchored to its own text, and
    /// a sub-range expressed relative to it. Mirrors exactly what
    /// `build_annotation_row` captures, so these tests exercise the real shape.
    fn anchored(src: &str) -> (crate::docref::AnchoredSpan, std::ops::Range<usize>) {
        let at = src.find("{==").unwrap()..src.rfind("<<}").unwrap() + 3;
        let a = crate::docref::AnchoredSpan::capture(src, at).expect("a valid construct");
        let inner_start = src.find("{==").unwrap() + 3;
        let inner = inner_start..src.find("==}").unwrap();
        let rel = a
            .relative(inner)
            .expect("the claim is inside its construct");
        (a, rel)
    }

    #[gtktest::test]
    fn remove_then_edit_round_trip_via_the_buffer() {
        let b = buf_with("a {==claim==}{>>old note<<} b");
        let cur = text_of(&b);
        let (construct, _) = anchored(&cur);
        // Edit the comment body, located relative to the construct.
        let body_abs = cur.find("{>>").unwrap() + 3..cur.rfind("<<}").unwrap();
        let body = construct.relative(body_abs).expect("body is inside");
        apply_annotation_edit(
            &b,
            AnnotationEdit::EditComment {
                construct,
                body,
                text: "new note".into(),
            },
        );
        assert_eq!(text_of(&b), "a {==claim==}{>>new note<<} b");
        // Remove keeps the claim.
        let cur = text_of(&b);
        let (construct, keep) = anchored(&cur);
        apply_annotation_edit(
            &b,
            AnnotationEdit::Remove {
                construct,
                keep_inner: Some(keep),
            },
        );
        assert_eq!(text_of(&b), "a claim b");
    }

    /// ScrAP-187. A card is built against one state of the document and clicked
    /// against another — the user types while it is open, or the split-mode live
    /// re-render re-scans underneath it. The captured offsets must be treated as a
    /// hint about where to look, never as an instruction about where to cut.
    ///
    /// Before the fix this produced `"EDITEDpha note<<} gamma"` from the input
    /// below: the annotated word gone, three characters of an unrelated word gone,
    /// and the CriticMarkup left half-open.
    #[gtktest::test]
    fn remove_after_an_edit_elsewhere_still_removes_the_right_construct() {
        let b = buf_with("alpha {==beta==}{>>note<<} gamma");
        // The card captures the construct as it stands now.
        let (construct, keep) = anchored(&text_of(&b));
        // The user then types at the very top of the document, shifting every
        // offset the card is holding.
        b.insert(&mut b.start_iter(), "EDITED ");
        assert_eq!(text_of(&b), "EDITED alpha {==beta==}{>>note<<} gamma");
        // Clicking Remove on that still-open card must remove the annotation and
        // nothing else.
        apply_annotation_edit(
            &b,
            AnnotationEdit::Remove {
                construct,
                keep_inner: Some(keep),
            },
        );
        assert_eq!(text_of(&b), "EDITED alpha beta gamma");
    }

    /// The other half of ScrAP-187: the document shrank past the captured range.
    /// Bare slicing panicked here ("start byte index 26 is out of bounds"), and a
    /// panic inside a GTK signal handler aborts the process. The contract is a
    /// clean no-op — the buffer must be left exactly as it was.
    #[gtktest::test]
    fn remove_whose_construct_is_gone_is_a_no_op_not_a_panic() {
        let b = buf_with("alpha {==beta==}{>>note<<} gamma");
        let (construct, keep) = anchored(&text_of(&b));
        // Someone deletes the annotation by hand (or an undo takes it away).
        b.set_text("alpha");
        apply_annotation_edit(
            &b,
            AnnotationEdit::Remove {
                construct,
                keep_inner: Some(keep),
            },
        );
        assert_eq!(
            text_of(&b),
            "alpha",
            "a vanished construct must change nothing"
        );
    }

    /// Editing a comment is exposed to exactly the same drift as removing one, and
    /// is anchored on the whole construct rather than the body: a body like "note"
    /// occurs all over a real document and could not be re-located on its own.
    #[gtktest::test]
    fn edit_after_an_edit_elsewhere_still_targets_the_right_comment() {
        let b = buf_with("alpha {==beta==}{>>note<<} gamma {>>note<<}");
        let cur = text_of(&b);
        let at = cur.find("{==").unwrap()..cur.find("<<}").unwrap() + 3;
        let construct = crate::docref::AnchoredSpan::capture(&cur, at).unwrap();
        let body_abs = cur.find("{>>").unwrap() + 3..cur.find("<<}").unwrap();
        let body = construct.relative(body_abs).unwrap();
        b.insert(&mut b.start_iter(), "EDITED ");
        apply_annotation_edit(
            &b,
            AnnotationEdit::EditComment {
                construct,
                body,
                text: "revised".into(),
            },
        );
        // The FIRST comment changed; the identical trailing point comment did not.
        assert_eq!(
            text_of(&b),
            "EDITED alpha {==beta==}{>>revised<<} gamma {>>note<<}"
        );
    }
}
