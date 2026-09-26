//! [`CommentEntry`] — the one annotation comment entry, and the one place the routes
//! that commit a comment are wired.
//!
//! **Why this type exists.** "Commit an annotation comment" is one command
//! reachable from three surfaces — the editor-side Create card, the preview-side Create
//! card, and the marker-chip Edit popover. It was implemented three times, and the third
//! copy simply missed a line: it wired its commit to `save.connect_clicked` and never to
//! the entry's Enter, so Enter was inert in the Edit popover while working in both
//! Create cards.
//!
//! That is the failure mode POLICY's Action CAM and the single-source-of-truth rule exist
//! to prevent (ScrAP-9), and it is the skill's standing lesson that an unenforced
//! per-call-site obligation regresses at the next call site. The tell is what the bug
//! survives: **a functional test of the Edit popover that only clicks Save passes while
//! Enter stays broken** — the effect works, only the sibling route is dead, so the obvious
//! test never fires. The fix is not to add the missing line (that leaves the fourth
//! surface free to forget it too) but to make the wiring unforgettable: a surface gets
//! Enter and Save together from [`CommentEntry::new`], or it gets neither.
//!
//! The pure "what does this comment mean" logic stays where it is — `annotate::scan`
//! parses, `annotate::mutate` writes, and each caller's `commit` closure maps its own
//! selection to an `AnnotationEdit`. This type owns only the widgets and their wiring.

use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// The CSS class every comment field carries, so a window-level predicate can recognise
/// one without knowing which surface built it (`window::actions::focus_in_text_entry`).
pub(crate) const COMMENT_FIELD_CLASS: &str = "comment-field";

/// How many text rows a comment field shows before it scrolls. Fixed rather than grown
/// with the text, because one of the three hosts is a shown `GtkPopover`, and a popover
/// whose child outgrows it after `popup()` is clipped on GTK 4.6 and dismissed outright
/// on GTK 4.14+ (GTK4Rs/AP-86).
const VISIBLE_ROWS: i32 = 3;

/// The name of the field's Enter-commits key controller — the one way a test finds it
/// among the field's other capture-phase controllers (macOS word navigation is one).
const COMMIT_KEYS_NAME: &str = "comment-commit-keys";

/// A wrapping comment field paired with its Save button, with every commit route wired
/// once.
///
/// The fields are public so a caller can pack them into its own container (a floating
/// overlay card, a popover page) — the layout genuinely differs per surface. What must not
/// differ is the wiring, and that is not reachable from outside. Pack [`area`](Self::area),
/// which holds [`field`](Self::field); the field is exposed for focus and text only.
///
/// **A comment is one line of text, displayed wrapped.** It is stored inline in the
/// document (`{>>comment<<}`), and a line break there would end a table row or split a
/// list item. So Enter commits rather than breaking the line, and [`comment_text`]
/// flattens any break a paste brought in. The flattening happens at the READ, never in an
/// `insert-text` handler: GTK replays undo through the public insert, so a rewriting
/// handler would corrupt the field's own undo (GTK4Rs/AP-303).
pub(crate) struct CommentEntry {
    pub(crate) area: gtk::Overlay,
    pub(crate) field: sourceview::View,
    pub(crate) save: gtk::Button,
    /// The hint shown over an empty field. A `GtkTextView` has no placeholder of its own,
    /// so this is one, laid over the field and never a target for the pointer. Empty
    /// until [`style_as_card`](Self::style_as_card) gives it text.
    placeholder: gtk::Label,
}

impl CommentEntry {
    /// Build the field + Save pair, wiring `commit` to **every** route that commits the
    /// comment: Enter in the field, and the Save button. `commit` receives the field's
    /// current text, flattened to one line, so no caller re-reads it (three copies of the
    /// read behind three weak upgrades was how the routes drifted in the first place).
    ///
    /// `initial` pre-fills the field — the existing comment for an Edit, `""` for a fresh
    /// one. Callers that need an empty comment rejected must do it in `commit`; the
    /// decision differs per surface (the Create paths let `create_from_*` reject it and
    /// still dismiss the card; the Edit path dismisses and skips the write), so it is
    /// deliberately not baked in here.
    pub(crate) fn new(initial: &str, commit: impl Fn(&str) + 'static) -> Self {
        // Built through the shared constructor, which owns the name, clipboard and macOS
        // word-navigation follow-ups a text field owes.
        let field = crate::widgets::textfield::named_text_area("Comment", initial);
        field.add_css_class(COMMENT_FIELD_CLASS);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        scroller.set_has_frame(true);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        // Never propagate: the field must not grow as the user types (see VISIBLE_ROWS).
        scroller.set_propagate_natural_height(false);
        scroller.set_min_content_height(rows_px(&field, VISIBLE_ROWS));
        scroller.set_child(Some(&field));

        let placeholder = gtk::Label::new(None);
        placeholder.add_css_class("dim-label");
        placeholder.set_halign(gtk::Align::Start);
        placeholder.set_valign(gtk::Align::Start);
        // Sit where the field's first glyph does: its own inset plus the frame's pixel.
        placeholder.set_margin_start(field.left_margin() + 1);
        placeholder.set_margin_top(field.top_margin() + 1);
        placeholder.set_can_target(false);
        placeholder.set_can_focus(false);
        placeholder.set_visible(initial.is_empty());
        field.buffer().connect_changed(glib::clone!(
            #[weak]
            placeholder,
            move |buf| placeholder.set_visible(buf.char_count() == 0)
        ));

        let area = gtk::Overlay::new();
        area.set_child(Some(&scroller));
        area.add_overlay(&placeholder);

        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");

        // ONE closure, BOTH routes — the whole point of this type.
        let commit: Rc<dyn Fn(&str)> = Rc::new(commit);

        // Enter commits, from CAPTURE so it runs before GtkTextView's own key handler
        // inserts the newline. Except mid-composition: an input method confirms its
        // preedit with Enter, and taking the key then would commit a half-typed word.
        let composing = Rc::new(Cell::new(false));
        field.connect_preedit_changed({
            let composing = composing.clone();
            move |_, preedit| composing.set(!preedit.is_empty())
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.set_name(Some(COMMIT_KEYS_NAME));
        keys.connect_key_pressed({
            let commit = commit.clone();
            move |ctl, keyval, _code, _mods| {
                if !is_enter(keyval) || composing.get() {
                    return glib::Propagation::Proceed;
                }
                // The controller's own widget, not a captured ref: a strong capture of
                // the field in a closure the field owns is a cycle (GTK4Rs/AP-63).
                if let Some(view) = ctl.widget().and_downcast::<sourceview::View>() {
                    commit(&comment_text(&view));
                }
                glib::Propagation::Stop
            }
        });
        field.add_controller(keys);

        save.connect_clicked(glib::clone!(
            #[strong]
            commit,
            #[weak]
            field,
            move |_| {
                commit(&comment_text(&field));
            }
        ));
        Self {
            area,
            field,
            save,
            placeholder,
        }
    }

    /// Apply the floating-card presentation shared by both Create cards: a placeholder,
    /// and a width in characters wide enough for it and a real comment.
    ///
    /// The Edit popover skips this: it opens pre-filled (so a placeholder can never show)
    /// inside a popover that sets its own width request, which a fixed width would fight.
    pub(crate) fn style_as_card(&self) {
        self.placeholder.set_label("Add a comment\u{2026}");
        // Beside a field three rows tall, a Save that filled the row's height would read
        // as a second field; it keeps its own height at the top instead.
        self.save.set_valign(gtk::Align::Start);
        let metrics = self.field.pango_context().metrics(None, None);
        let char_px = metrics.approximate_char_width() / gtk::pango::SCALE;
        self.field.set_size_request(char_px.max(1) * 34, -1);
    }

    /// Wire Escape anywhere in `root`'s subtree to `cancel`.
    ///
    /// For the in-surface cards only. They are `GtkOverlay` children rather than popovers
    /// (GTK4Rs/AP-83 — a popover hosting a typing entry is unwinnable on X11), so they gave up
    /// the popover's built-in Escape-to-dismiss and must wire it explicitly. The marker
    /// Edit popover is a real `GtkPopover` and GTK already dismisses it on Escape, so it
    /// does not call this — that asymmetry is GTK's, not ours.
    ///
    /// `cancel` owns the whole dismissal, including handing focus back to the pane: which
    /// widget that is differs per surface, and is not this type's business.
    pub(crate) fn wire_escape(&self, root: &impl IsA<gtk::Widget>, cancel: impl Fn() + 'static) {
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _code, _mods| {
            if keyval == gtk::gdk::Key::Escape {
                cancel();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        root.as_ref().add_controller(key);
    }
}

/// Is `keyval` one of the keys a user presses to mean "done" — Return, keypad Enter, or
/// the ISO Enter some layouts produce?
fn is_enter(keyval: gtk::gdk::Key) -> bool {
    matches!(
        keyval,
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter | gtk::gdk::Key::ISO_Enter
    )
}

/// One line of text from a comment `text` that may carry line breaks — a paste is the
/// only way one gets in, since Enter commits. Each break becomes one space.
pub(crate) fn flatten_comment(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

/// The comment in `field`, flattened to one line ([`flatten_comment`]).
pub(crate) fn comment_text(field: &sourceview::View) -> String {
    flatten_comment(crate::saferizer::BufferText::of(&field.buffer()).as_str())
}

/// Replace the comment in `field` with `text`.
pub(crate) fn set_comment_text(field: &sourceview::View, text: &str) {
    field.buffer().set_text(text);
}

/// Focus `field` with the caret at the end, so typing APPENDS: a pre-filled field holds
/// a comment the user is amending (an Edit, or the comments a merge will fold in), and
/// must never open with its text selected for replacement.
pub(crate) fn focus_at_end(field: &sourceview::View) {
    field.grab_focus();
    let buf = field.buffer();
    buf.place_cursor(&buf.end_iter());
    field.scroll_mark_onscreen(&buf.get_insert());
}

/// The height of `rows` text rows in `field`'s own font, plus its vertical margins.
fn rows_px(field: &sourceview::View, rows: i32) -> i32 {
    let (_, row) = field.create_pango_layout(Some("0")).pixel_size();
    row.max(1) * rows + field.top_margin() + field.bottom_margin()
}

/// Press `keyval` on a comment `field` the way GTK delivers it: through the field's own
/// capture-phase key controller, the one [`CommentEntry::new`] installs. Returns whether
/// that controller claimed the key. For tests of every surface that hosts a field, so
/// each drives Enter through the real route rather than a stand-in.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn press_key(field: &sourceview::View, keyval: gtk::gdk::Key) -> bool {
    let controllers = field.observe_controllers();
    let key = (0..controllers.n_items())
        .filter_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .find(|c| c.name().as_deref() == Some(COMMIT_KEYS_NAME))
        .expect("CommentEntry::new installs its Enter-commits key controller");
    key.emit_by_name(
        "key-pressed",
        &[&keyval, &0u32, &gtk::gdk::ModifierType::empty()],
    )
}

/// The commit-route contract, tested once here because it now holds once.
///
/// These build GTK objects, so they need `gtk::init` and are gated with the rest of the
/// GTK-object tests: `cargo test --features gtk-integration-tests`, under Xvfb if
/// headless. `#[gtktest::test]` rather than `#[test]` — a plain `#[test]` calling `gtk::init`
/// panics on the second test in the binary when the thread changes (GTK4Rs/AP-71).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Record every commit the entry dispatches, so a route that never fires is visible as
    /// an empty log rather than as an absence nobody asserts on.
    fn recording() -> (Rc<RefCell<Vec<String>>>, impl Fn(&str) + 'static) {
        let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = {
            let log = log.clone();
            move |text: &str| log.borrow_mut().push(text.to_string())
        };
        (log, sink)
    }

    fn press(ce: &CommentEntry, keyval: gtk::gdk::Key) -> bool {
        press_key(&ce.field, keyval)
    }

    fn text_of(ce: &CommentEntry) -> String {
        crate::saferizer::BufferText::of(&ce.field.buffer()).into_string()
    }

    /// **The regression this test guards against.** Enter in the field must commit. This
    /// is the route the marker-chip Edit popover once silently lacked: it wired only the
    /// Save button, so Enter was inert there while working in both Create cards — and a
    /// functional test that clicked Save passed the whole time. Asserting Enter
    /// *specifically* is the point. It must also be CLAIMED, or the text view's own
    /// handler inserts a line break into a comment that cannot hold one.
    #[gtktest::test]
    fn enter_commits_and_inserts_no_line_break() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        set_comment_text(&ce.field, "typed by hand");
        assert!(press(&ce, gtk::gdk::Key::Return), "Enter must be claimed");
        assert!(
            press(&ce, gtk::gdk::Key::KP_Enter),
            "keypad Enter must be claimed"
        );
        assert_eq!(
            log.borrow().as_slice(),
            ["typed by hand", "typed by hand"],
            "Enter must commit the field's text"
        );
        assert_eq!(
            text_of(&ce),
            "typed by hand",
            "no line break may reach the field"
        );
    }

    /// Mid-composition, Enter belongs to the input method (it confirms the preedit), so
    /// the field must neither commit nor claim it. Any other key is never claimed.
    #[gtktest::test]
    fn enter_during_composition_and_other_keys_are_not_taken() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        ce.field.emit_by_name::<()>("preedit-changed", &[&"ka"]);
        assert!(
            !press(&ce, gtk::gdk::Key::Return),
            "Enter confirms the preedit"
        );
        ce.field.emit_by_name::<()>("preedit-changed", &[&""]);
        assert!(
            !press(&ce, gtk::gdk::Key::a),
            "an ordinary key is the field's own"
        );
        assert!(log.borrow().is_empty(), "nothing may have been committed");
        assert!(
            press(&ce, gtk::gdk::Key::Return),
            "composition over, Enter commits"
        );
        assert_eq!(log.borrow().len(), 1);
    }

    /// The Save button must commit — the route that already worked everywhere. Present so
    /// the pair is asserted together: the commit bug existed precisely because these two
    /// were wired in two places and one was forgotten.
    #[gtktest::test]
    fn save_commits() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        set_comment_text(&ce.field, "typed by hand");
        ce.save.emit_clicked();
        assert_eq!(
            log.borrow().as_slice(),
            ["typed by hand"],
            "the Save button must commit the field's text"
        );
    }

    /// Both routes reach the SAME commit and see the SAME text. The two must be
    /// indistinguishable to the caller — that is what makes the surfaces unable to drift.
    #[gtktest::test]
    fn enter_and_save_are_the_same_commit() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        set_comment_text(&ce.field, "first");
        press(&ce, gtk::gdk::Key::Return);
        set_comment_text(&ce.field, "second");
        ce.save.emit_clicked();
        assert_eq!(
            log.borrow().as_slice(),
            ["first", "second"],
            "Enter and Save must dispatch the same commit with the live field text"
        );
    }

    /// A pasted line break cannot survive into the document: both routes commit one line.
    #[gtktest::test]
    fn a_pasted_line_break_is_committed_as_a_space() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        set_comment_text(&ce.field, "one\ntwo\r\nthree");
        ce.save.emit_clicked();
        press(&ce, gtk::gdk::Key::Return);
        assert_eq!(log.borrow().as_slice(), ["one two three", "one two three"]);
    }

    /// The Edit surface opens pre-filled with the existing comment, and committing without
    /// touching it must round-trip that text unchanged — not an empty string.
    #[gtktest::test]
    fn an_initial_comment_is_prefilled_and_round_trips() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("the reviewer's earlier remark", sink);
        assert_eq!(text_of(&ce), "the reviewer's earlier remark");
        press(&ce, gtk::gdk::Key::Return);
        assert_eq!(log.borrow().as_slice(), ["the reviewer's earlier remark"]);
    }

    /// The field is a text field like every other: named, so a screen reader announces it.
    #[gtktest::test]
    fn the_field_is_named_and_wraps() {
        let ce = CommentEntry::new("", |_| {});
        assert!(crate::a11y::has_name(&ce.field));
        assert_eq!(ce.field.wrap_mode(), gtk::WrapMode::Word);
        let scroller = ce
            .area
            .child()
            .and_downcast::<gtk::ScrolledWindow>()
            .expect("the area holds the field's scroller");
        assert!(
            !scroller.propagates_natural_height(),
            "the field must not grow as typed"
        );
    }

    /// The Create cards' hint shows over an EMPTY field only: typing hides it, clearing
    /// brings it back, and a pre-filled field (the merged comments) opens without it.
    #[gtktest::test]
    fn the_placeholder_shows_only_over_an_empty_field() {
        let ce = CommentEntry::new("", |_| {});
        ce.style_as_card();
        assert!(ce.placeholder.is_visible() && !ce.placeholder.label().is_empty());
        assert!(
            !ce.placeholder.can_target(),
            "the hint must never take a click"
        );
        set_comment_text(&ce.field, "typed");
        assert!(!ce.placeholder.is_visible(), "typing hides the hint");
        set_comment_text(&ce.field, "");
        assert!(ce.placeholder.is_visible(), "clearing brings it back");
        let prefilled = CommentEntry::new("merged", |_| {});
        assert!(!prefilled.placeholder.is_visible());
    }
}

#[cfg(test)]
mod pure_tests {
    use super::flatten_comment;

    #[test]
    fn flattening_turns_every_line_break_into_one_space() {
        assert_eq!(flatten_comment("a\nb"), "a b");
        assert_eq!(flatten_comment("a\r\nb"), "a b");
        assert_eq!(flatten_comment("a\rb"), "a b");
        assert_eq!(flatten_comment("no breaks"), "no breaks");
    }
}
