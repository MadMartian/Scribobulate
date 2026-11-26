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
    // A suggested edit shows its change where a claim shows its words, in the card's
    // fuller form: what goes, struck through, and what comes (TDD 17.72). It is the
    // card's subject rather than context for a comment, so it is not dimmed, and each
    // side is capped on its own, so a long old side cannot push "With:" out of sight.
    let quotes: Vec<gtk::Label> = match (&m.suggestion, &m.claim) {
        (Some(s), _) => crate::sidebarfilter::suggestion_card_lines(s)
            .iter()
            .map(|markup| {
                let l = gtk::Label::new(None);
                l.set_markup(markup);
                l
            })
            .collect(),
        (None, Some(claim)) => {
            let l = gtk::Label::new(Some(&format!("\u{201c}{claim}\u{201d}")));
            l.add_css_class("dim-label");
            vec![l]
        }
        (None, None) => Vec::new(),
    };
    for cl in quotes {
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
    // A suggested edit can also be applied: Accept edit keeps its new side, as Remove
    // keeps its old (TDD 17.73, 17.74) — either may be nothing. Both are removals that differ only in
    // which side survives.
    if m.suggestion.is_some() {
        let accept = gtk::Button::with_label("Accept edit");
        accept.add_css_class("suggested-action");
        wire_removal(&accept, card, &sink, m, m.source.src_content.clone());
        btns.append(&accept);
    }
    btns.append(&remove);
    // A follow-up answers a comment, so only an annotation that has one offers it. A bare
    // highlight's or note-less suggestion's comment "body" is the empty point just past
    // the construct, and a `{>>…<<}` written there would become its first comment —
    // Edit's job (TDD 17.75).
    if m.source.src_comment_body.end != m.source.construct.captured_at().end {
        let follow_up = gtk::Button::with_label("Add Follow-up");
        let card = card.clone();
        let sink = sink.clone();
        let construct = m.source.construct.clone();
        let comment = m.comment.clone();
        follow_up.connect_clicked(move |_| {
            // Deferred off the press gesture: the card's content is replaced (GTK4Rs/AP-30).
            let (card, sink, construct, comment) = (
                card.clone(),
                sink.clone(),
                construct.clone(),
                comment.clone(),
            );
            glib::idle_add_local_once(move || {
                show_follow_up_form(&card, sink, construct, &comment);
            });
        });
        btns.append(&follow_up);
    }

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
            let view = card.view();
            card.dismiss();
            let Some(body) = body_rel.clone() else { return };
            if !text.trim().is_empty() {
                // Defer the re-render off the active press gesture (see the Remove handler
                // below — avoids "Broken accounting of active state").
                let sink = sink.clone();
                let construct = construct.clone();
                glib::idle_add_local_once(move || {
                    // The construct's start does not move when its comment is rewritten,
                    // so it names the same annotation in the render the edit produces.
                    let start = construct.captured_at().start;
                    sink(AnnotationEdit::EditComment {
                        construct,
                        body,
                        text,
                    });
                    // Saving an edit leaves the card open on what was saved (TDD 17.7):
                    // the reader sees the new comment, and the controls a comment brings
                    // (Add Follow-up), without reopening. The render the edit triggered
                    // has already installed its markers, so the card is opened afresh on
                    // them in the same turn, before a frame can show it closed.
                    if let Some(view) = view {
                        view.reopen_marker_card_on(start);
                    }
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

    // A highlight keeps its claim; a suggested edit, rejected, keeps its old side;
    // a point comment keeps nothing.
    let kept = match &m.suggestion {
        Some(_) => m.source.src_old.clone(),
        None => m.source.src_content.clone(),
    };
    wire_removal(&remove, card, &sink, m, kept);
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
                crate::widgets::comment_entry::open_comment(&entry, &original);
                stack.set_visible_child_name("read");
            });
        });
    }
}

/// Wire `button` to remove `m`'s construct from the document, keeping the ORIGINAL-source
/// range `kept` (`None` keeps nothing). Remove and Accept edit are this one mutation.
fn wire_removal(
    button: &gtk::Button,
    card: &AnnotationCard,
    sink: &AnnotationSink,
    m: &MarkerData,
    kept: Option<std::ops::Range<usize>>,
) {
    let sink = sink.clone();
    let construct = m.source.construct.clone();
    // The text to keep, relative to the construct, so the pair travels as one unit and
    // cannot be resolved against different positions. A point comment has no kept
    // content, which is `None` for a different reason than a failed `relative` — but
    // both mean "keep nothing", so the flatten is faithful: text that could not be
    // placed inside its own construct must not be spliced in from wherever it
    // nominally pointed.
    let keep_rel = kept.and_then(|c| construct.relative(c));
    let card = card.clone();
    button.connect_clicked(move |_| {
        card.dismiss();
        // Defer the sink: it re-renders the preview (rebuilds the widget tree), which
        // must not run inside this button's active press gesture or GTK reports
        // "Broken accounting of active state" up the ancestor chain (GTK4Rs/AP-30).
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

/// Turn the card over to a new-comment form for a follow-up to `comment`, the comment of
/// the annotation `construct` (TDD 17.76). Save writes the follow-up straight after that
/// annotation and brings the card back with it listed beneath; Cancel, or a Save with
/// nothing typed, brings the card back as it was; Escape and the corner × close the card,
/// as they do on an Edit. A typed draft keeps the card up against an outside click, as any
/// comment field in it does (TDD 17.70).
fn show_follow_up_form(
    card: &AnnotationCard,
    sink: AnnotationSink,
    construct: crate::docref::AnchoredSpan,
    comment: &str,
) {
    let Some(view) = card.view() else { return };
    // The annotation answered keeps its place in the document, so its start names it in
    // whatever render the follow-up produces.
    let start = construct.captured_at().start;
    let back = {
        let card = card.clone();
        let view = view.clone();
        move || {
            card.dismiss();
            view.reopen_marker_card_on(start);
        }
    };
    let ce = CommentEntry::new("", {
        let card = card.clone();
        let back = back.clone();
        move |text: &str| {
            if text.trim().is_empty() {
                back();
                return;
            }
            let text = text.to_string();
            card.dismiss();
            let (sink, construct, view) = (sink.clone(), construct.clone(), view.clone());
            // Deferred off the commit's key or press (GTK4Rs/AP-30): the sink re-renders.
            glib::idle_add_local_once(move || {
                sink(AnnotationEdit::FollowUp { construct, text });
                view.reopen_marker_card_on(start);
            });
        }
    });
    ce.style_as_card();
    let cancel = gtk::Button::with_label("Cancel");
    cancel.connect_clicked(move |_| {
        let back = back.clone();
        glib::idle_add_local_once(back);
    });

    let heading = gtk::Label::new(Some("Follow-up to"));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    let quoted = gtk::Label::new(Some(&format!("\u{201c}{comment}\u{201d}")));
    quoted.add_css_class("dim-label");
    quoted.set_xalign(0.0);
    quoted.set_wrap(true);
    quoted.set_max_width_chars(34);
    quoted.set_lines(cardscroll::CLAIM_MAX_LINES);
    quoted.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let btns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    btns.append(&ce.save);
    btns.append(&cancel);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    vbox.set_margin_top(2);
    vbox.set_margin_bottom(8);
    vbox.set_margin_start(8);
    vbox.set_margin_end(8);
    vbox.set_width_request(280);
    vbox.append(&card_close_button(card));
    vbox.append(&heading);
    vbox.append(&quoted);
    vbox.append(&ce.area);
    vbox.append(&btns);
    crate::widgets::floating_card::FloatingCardExt::set_child(card, Some(&vbox));
    // Re-presented so the card re-derives its place for its new size.
    card.present();
    let field = ce.field.clone();
    glib::idle_add_local_once(move || crate::widgets::comment_entry::focus_at_end(&field));
}
