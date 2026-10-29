//! The annotation card's row building: the corner close button, one annotation's
//! row (claim snippet and comment), and the in-place Edit / Remove controls that
//! mutate the document through the installed sink. Called only from
//! `markercard`'s `open_marker_popover`, which assembles the rows into the card.

use super::card::AnnotationCard;
use super::cardscroll;
use super::markers::{AnnotationEdit, AnnotationSink, MarkerData};
use crate::widgets::comment_entry::CommentEntry;
use gtk::glib;
use gtk::prelude::*;

/// The card's corner **×** — the pointer affordance for dismissing it, in the top-right
/// corner where a tool window puts its close control.
///
/// Same chrome as the sidebar panes' in-pane × (`window::sidebar`): the audited
/// [`Icon::WindowClose`] name rather than a raw string, so `tests/icon_resolution.rs`
/// catches it if the name ever stops resolving in the active theme (a GNOME-only name
/// silently renders the ⚠ placeholder elsewhere — GTK4Rs/AP-48), and `.flat` so it reads as
/// chrome rather than as a third action alongside Edit / Remove.
///
/// **Deliberately not focusable.** The card focuses its first focusable descendant one
/// tick after presenting, so that Tab/Space reach Edit and Remove and Escape is live from
/// the moment it opens. A focusable × placed first in the tree would capture that focus
/// instead, and the card would open with "close" selected — one Space from dismissing
/// itself. Keyboard dismissal is Escape, which the card wires in the capture phase and
/// which this button only duplicates for the pointer, so nothing is lost by keeping it out
/// of the tab ring; it stays in the accessibility tree either way.
///
/// [`Icon::WindowClose`]: crate::icons::Icon::WindowClose
pub(super) fn card_close_button(card: &AnnotationCard) -> gtk::Button {
    let close = gtk::Button::from_icon_name(crate::icons::Icon::WindowClose.name());
    close.add_css_class("flat");
    close.set_halign(gtk::Align::End);
    close.set_can_focus(false);
    crate::a11y::name_with_tooltip(&close, "Close annotation", "Close (Esc)");
    close.connect_clicked(glib::clone!(
        #[weak]
        card,
        move |_| card.dismiss()
    ));
    close
}

/// Build one annotation's popover row: the claim snippet (if any), the comment
/// text, and — when a mutation `sink` is installed — Edit / Remove controls.
pub(super) fn build_annotation_row(
    m: &MarkerData,
    card: &AnnotationCard,
    sink: Option<AnnotationSink>,
    wheel: &cardscroll::RowWheel,
) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    if let Some(claim) = &m.claim {
        let cl = gtk::Label::new(Some(&format!("\u{201c}{claim}\u{201d}")));
        cl.add_css_class("dim-label");
        cl.set_xalign(0.0);
        cl.set_wrap(true);
        cl.set_max_width_chars(34);
        row.append(&cardscroll::bounded_text(
            &cl,
            cardscroll::CLAIM_MAX_LINES,
            wheel,
        ));
    }
    let comment_label = gtk::Label::new(Some(&m.comment));
    comment_label.set_xalign(0.0);
    comment_label.set_wrap(true);
    comment_label.set_max_width_chars(34);
    // NOT selectable: a selectable GtkLabel selects ALL its text the moment it
    // receives focus. The popover is no longer autohide, so it no longer auto-focuses
    // its first focusable descendant on open — `open_marker_popover` now moves focus to
    // the first BUTTON explicitly, not this label — so the old "comment opens
    // pre-highlighted" case is gone; keep this non-selectable anyway, for consistency
    // with the claim quote and so a future focus-order change can't reintroduce it. The
    // claim quote above is non-selectable too; the comment text lives verbatim in the
    // source document if a user wants to copy it.

    let comment = cardscroll::bounded_text(&comment_label, cardscroll::COMMENT_MAX_LINES, wheel);
    let Some(sink) = sink else {
        row.append(&comment);
        return row.upcast();
    };
    append_edit_remove_controls(m, &row, &comment, card, sink);
    row.upcast()
}

/// Append the comment, with its Edit / Remove controls, to a popover `row` and wire their
/// mutation handlers to `sink`.
///
/// **Edit happens IN PLACE.** The read page is the comment text with Edit / Remove under
/// it; the edit page is the comment field, pre-filled, in the same spot, with Save /
/// Cancel under it. The comment is on one page or the other, never both — an Edit that
/// left the fixed text showing and added a field beneath it showed the same comment twice
/// and moved the thing being edited away from where the reader was looking. Split out of [`build_annotation_row`] (F-DRY-002) so the
/// read-only layout above stays free of the nested GTK closure-capture ceremony the
/// buttons need — each `move` closure (and each deferred `idle_add_local_once`)
/// requires its own `sink`/range clone; the re-renders are deferred off the active
/// press gesture to avoid GTK's "Broken accounting of active state" warning (GTK4Rs/AP-30).
///
/// **Both pages — the read buttons and the edit entry+Save/Cancel — are real `GtkStack`
/// pages, built up front and switched by NAME; never lazily constructed on click, never
/// `set_visible`-toggled siblings (GTK4Rs/AP-86).** A shown `GtkPopover` sizes its
/// surface ONCE, at `popup()`. The original code hid the read widgets and appended the
/// taller edit entry AFTER `popup()`: the row's natural height momentarily collapsed and
/// GTK auto-hid the popover on the focus move into that just-appended child — the whole
/// popover vanished instead of showing the edit UI, even though Edit's own
/// `connect_clicked` fired every time (log-confirmed on Win32 and macOS/Quartz; the X11
/// autohide-off fix does not touch this, so it is a distinct, cross-platform bug).
/// Building both pages BEFORE `popup()` with `hhomogeneous` + `vhomogeneous` makes the
/// stack — and therefore the popover — request room for its LARGEST page from first show,
/// so switching pages later never resizes the surface and never appends a child
/// post-`popup`. Per-row (not whole-popover) so each annotation's edit state stays
/// independent when a marker groups several onto one line.
fn append_edit_remove_controls(
    m: &MarkerData,
    row: &gtk::Box,
    comment: &gtk::ScrolledWindow,
    card: &AnnotationCard,
    sink: AnnotationSink,
) {
    let btns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let edit = gtk::Button::with_label("Edit");
    let remove = gtk::Button::with_label("Remove");
    remove.add_css_class("destructive-action");
    btns.append(&edit);
    btns.append(&remove);

    // The edit page, built NOW (not lazily on click) so the stack can measure it from the
    // start. The shared `CommentEntry` wires the commit to Enter AND Save — hand-rolling it
    // only into `save.connect_clicked` was the original "Enter is inert here" bug, and
    // taking the wiring from the shared constructor makes that omission unrepresentable. No
    // `style_as_card()`: this entry opens pre-filled inside a popover that sets its own
    // width request, so the card's placeholder and 34-char width don't apply.
    // Both mutations address the construct through its anchor, and locate their
    // sub-range (comment body / kept claim) RELATIVE to it — so the pair travels as
    // one unit and cannot be resolved against different positions. `relative`
    // returns `None` only if the scan produced a sub-range outside its own
    // construct, in which case the button is left inert rather than wired to a
    // mutation nobody can place.
    let construct = m.source.construct.clone();
    let body_rel = construct.relative(m.source.src_comment_body.clone());
    let ce = CommentEntry::new(&m.comment, {
        let sink = sink.clone();
        let construct = construct.clone();
        let body_rel = body_rel.clone();
        let card = card.clone();
        move |text: &str| {
            let text = text.to_string();
            card.dismiss();
            let Some(body) = body_rel.clone() else { return };
            if !text.trim().is_empty() {
                // Defer the re-render off the active press gesture (see the Remove handler
                // below — avoids "Broken accounting of active state").
                let sink = sink.clone();
                let construct = construct.clone();
                glib::idle_add_local_once(move || {
                    sink(AnnotationEdit::EditComment {
                        construct,
                        body,
                        text,
                    });
                });
            }
        }
    });
    let cancel = gtk::Button::with_label("Cancel");
    let ebtns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    ebtns.append(&ce.save);
    ebtns.append(&cancel);
    let edit_page = gtk::Box::new(gtk::Orientation::Vertical, 4);
    edit_page.append(&ce.area);
    edit_page.append(&ebtns);

    // `vhomogeneous` (below) sizes BOTH pages to the taller of the two, so the read page
    // keeps its comment and buttons together at the top rather than stretching them apart.
    let read_page = gtk::Box::new(gtk::Orientation::Vertical, 4);
    read_page.set_valign(gtk::Align::Start);
    read_page.append(comment);
    read_page.append(&btns);

    let stack = gtk::Stack::new();
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(true);
    stack.add_named(&read_page, Some("read"));
    stack.add_named(&edit_page, Some("edit"));
    stack.set_visible_child_name("read");
    row.append(&stack);

    {
        let sink = sink.clone();
        let construct = construct.clone();
        // The claim to keep, relative to the construct (see the note above). A
        // point comment has no kept content, which is `None` for a different
        // reason than a failed `relative` — but both mean "keep nothing", so the
        // flatten is faithful: a claim that could not be placed inside its own
        // construct must not be spliced in from wherever it nominally pointed.
        let keep_rel = m
            .source
            .src_content
            .clone()
            .map(|c| construct.relative(c))
            .unwrap_or(None);
        let card = card.clone();
        remove.connect_clicked(move |_| {
            card.dismiss();
            // Defer the sink: it re-renders the preview (rebuilds the widget tree),
            // which must not run inside this button's active press gesture or GTK
            // reports "Broken accounting of active state" up the ancestor chain.
            let sink = sink.clone();
            let construct = construct.clone();
            let keep_inner = keep_rel.clone();
            glib::idle_add_local_once(move || {
                sink(AnnotationEdit::Remove {
                    construct,
                    keep_inner,
                });
            });
        });
    }
    {
        let stack = stack.clone();
        let entry = ce.field.clone();
        edit.connect_clicked(move |_| {
            // Defer off the active press gesture (GTK4Rs/AP-30): flipping the stack still mutates
            // this button's own ancestor mid-click. Focus the entry so typing (and the
            // popover's Escape controller) reach it.
            let stack = stack.clone();
            let entry = entry.clone();
            glib::idle_add_local_once(move || {
                stack.set_visible_child_name("edit");
                crate::widgets::comment_entry::focus_at_end(&entry);
            });
        });
    }
    {
        // Cancel ABORTS THE EDIT, it does not close the card: the stack goes back to the
        // read page and the card stays open on the annotation the user was looking at.
        // Dismissing here conflated two different intentions — "I don't want this edit"
        // and "I'm done with this annotation" — and answered both with the more
        // destructive one, so backing out of an accidental Edit cost the reader their
        // place and a second click to get back to it. Escape and the corner × remain the
        // ways to say the other thing.
        //
        // The entry is restored to the stored comment on the way out, so the abandoned
        // draft does not survive to greet the user the next time they press Edit — that is
        // what makes this an abort rather than a postponement.
        let stack = stack.clone();
        let entry = ce.field.clone();
        let original = m.comment.clone();
        cancel.connect_clicked(move |_| {
            // Deferred for the same reason the Edit handler defers (GTK4Rs/AP-30): flipping the
            // stack mutates this button's own ancestor while its press gesture is still
            // active, which GTK reports as "Broken accounting of active state".
            let stack = stack.clone();
            let entry = entry.clone();
            let original = original.clone();
            glib::idle_add_local_once(move || {
                crate::widgets::comment_entry::set_comment_text(&entry, &original);
                stack.set_visible_child_name("read");
            });
        });
    }
}
