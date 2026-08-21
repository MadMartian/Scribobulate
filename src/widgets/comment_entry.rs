//! [`CommentEntry`] — the one annotation comment entry, and the one place the routes
//! that commit a comment are wired.
//!
//! **Why this type exists.** "Commit an annotation comment" is one command
//! reachable from three surfaces — the editor-side Create card, the preview-side Create
//! card, and the marker-chip Edit popover. It was implemented three times, and the third
//! copy simply missed a line: it wired its commit to `save.connect_clicked` and never to
//! `entry.connect_activate`, so Enter was inert in the Edit popover while working in both
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
use std::rc::Rc;

/// A comment `GtkEntry` paired with its Save button, with every commit route wired once.
///
/// Both fields are public so a caller can pack them into its own container (a floating
/// overlay card, an inline popover row) and tweak presentation — the layout genuinely
/// differs per surface. What must not differ is the wiring, and that is not reachable
/// from outside.
pub(crate) struct CommentEntry {
    pub(crate) entry: gtk::Entry,
    pub(crate) save: gtk::Button,
}

impl CommentEntry {
    /// Build the entry + Save pair, wiring `commit` to **every** route that commits the
    /// comment: Enter in the entry, and the Save button. `commit` receives the entry's
    /// current text, so no caller re-reads it (three copies of `entry.text()` behind
    /// three weak upgrades was how the routes drifted in the first place).
    ///
    /// `initial` pre-fills the entry — the existing comment for an Edit, `""` for a fresh
    /// one. Callers that need an empty comment rejected must do it in `commit`; the
    /// decision differs per surface (the Create paths let `create_from_*` reject it and
    /// still dismiss the card; the Edit path dismisses and skips the write), so it is
    /// deliberately not baked in here.
    pub(crate) fn new(initial: &str, commit: impl Fn(&str) + 'static) -> Self {
        let entry = gtk::Entry::new();
        entry.set_text(initial);
        entry.set_hexpand(true);
        // Option+Left/Right word navigation (`macwordnav`). Wired here for exactly
        // the reason this type exists at all: a surface that had to remember it
        // would be the fourth surface that forgot.
        #[cfg(target_os = "macos")]
        crate::macwordnav::wire_field_word_navigation(&entry);
        crate::a11y::name_field(&entry, "Comment");
        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");

        // ONE closure, BOTH routes — the whole point of this type. The `Rc`
        // is what lets the two handlers share a single closure rather than each capturing
        // its own copy of the commit logic.
        let commit: Rc<dyn Fn(&str)> = Rc::new(commit);
        entry.connect_activate({
            let commit = commit.clone();
            // The handler's emitter argument, not a captured ref: a strong capture of the
            // entry in a closure the entry itself owns is an uncollectable cycle (ScrAP-60).
            move |e| commit(&e.text())
        });
        save.connect_clicked(glib::clone!(
            #[strong]
            commit,
            #[weak(rename_to = e)]
            entry,
            move |_| {
                commit(&e.text());
            }
        ));
        Self { entry, save }
    }

    /// Apply the floating-card presentation shared by both Create cards: a placeholder and
    /// a character-based width (the canonical `GtkEntry` sizing) wide enough for the
    /// placeholder and a real comment.
    ///
    /// The Edit popover skips this: it opens pre-filled (so a placeholder can never show)
    /// inside a popover that already sets its own width request, and a 34-char entry would
    /// fight that.
    pub(crate) fn style_as_card(&self) {
        self.entry
            .set_placeholder_text(Some("Add a comment\u{2026}"));
        self.entry.set_width_chars(34);
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

    /// **The regression this test guards against.** Enter in the entry must commit. This is the route the
    /// marker-chip Edit popover silently lacked: it wired only `save.connect_clicked`, so
    /// Enter was inert there while working in both Create cards — and a functional test
    /// that clicked Save passed the whole time. Asserting Enter *specifically* is the point.
    #[gtktest::test]
    fn enter_commits() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        ce.entry.set_text("typed by hand");
        ce.entry.emit_activate();
        assert_eq!(
            log.borrow().as_slice(),
            ["typed by hand"],
            "Enter must commit the entry's text"
        );
    }

    /// The Save button must commit — the route that already worked everywhere. Present so
    /// the pair is asserted together: the commit bug existed precisely because these two
    /// were wired in two places and one was forgotten.
    #[gtktest::test]
    fn save_commits() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        ce.entry.set_text("typed by hand");
        ce.save.emit_clicked();
        assert_eq!(
            log.borrow().as_slice(),
            ["typed by hand"],
            "the Save button must commit the entry's text"
        );
    }

    /// Both routes reach the SAME commit and see the SAME text. The two must be
    /// indistinguishable to the caller — that is what makes the surfaces unable to drift.
    #[gtktest::test]
    fn enter_and_save_are_the_same_commit() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("", sink);
        ce.entry.set_text("first");
        ce.entry.emit_activate();
        ce.entry.set_text("second");
        ce.save.emit_clicked();
        assert_eq!(
            log.borrow().as_slice(),
            ["first", "second"],
            "Enter and Save must dispatch the same commit with the live entry text"
        );
    }

    /// The Edit surface opens pre-filled with the existing comment, and committing without
    /// touching it must round-trip that text unchanged — not an empty string.
    #[gtktest::test]
    fn an_initial_comment_is_prefilled_and_round_trips() {
        let (log, sink) = recording();
        let ce = CommentEntry::new("the reviewer's earlier remark", sink);
        assert_eq!(ce.entry.text(), "the reviewer's earlier remark");
        ce.entry.emit_activate();
        assert_eq!(log.borrow().as_slice(), ["the reviewer's earlier remark"]);
    }
}
