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
    // A suggested edit's controls come in two rows — what settles or rewrites the
    // suggestion, then what edits or answers its note — so the card does not widen to fit
    // five buttons in one. Any other annotation has one row.
    let btns = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let note_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let change_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let edit = gtk::Button::with_label(EDIT_NOTE);
    let remove = gtk::Button::with_label("Remove");
    remove.add_css_class("destructive-action");
    note_row.append(&edit);
    // A suggested edit can also be applied: Accept edit keeps its new side, as Remove
    // keeps its old (TDD 17.73, 17.74) — either may be nothing. Both are removals that differ only in
    // which side survives.
    if m.suggestion.is_some() {
        let accept = gtk::Button::with_label("Accept edit");
        accept.add_css_class("suggested-action");
        wire_removal(&accept, card, &sink, m, m.source.src_content.clone());
        change_row.append(&accept);
        change_row.append(&remove);
        btns.append(&change_row);
    } else {
        note_row.append(&remove);
    }
    btns.append(&note_row);
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
        note_row.append(&follow_up);
    }

    // The edit pages, built NOW (not lazily on click) so the stack can measure them from
    // the start. Both mutations address the construct through its anchor, and locate
    // their sub-range (comment body / added side / kept claim) RELATIVE to it — so the
    // pair travels as one unit and cannot be resolved against different positions.
    // `relative` returns `None` only if the scan produced a sub-range outside its own
    // construct, in which case the commit is left inert rather than wired to a mutation
    // nobody can place.
    let construct = m.source.construct.clone();
    let body_rel = construct.relative(m.source.src_comment_body.clone());

    // `vhomogeneous` (below) sizes EVERY page to the tallest, so the read page keeps its
    // comment and buttons together at the top rather than stretching them apart.
    let read_page = gtk::Box::new(gtk::Orientation::Vertical, 4);
    read_page.set_valign(gtk::Align::Start);
    read_page.append(comment);
    read_page.append(&btns);

    let stack = gtk::Stack::new();
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(true);
    stack.add_named(&read_page, Some(READ_PAGE));

    let note_field = add_edit_page(&stack, NOTE_PAGE, &m.comment, card, sink.clone(), {
        let construct = construct.clone();
        move |text: String| {
            let body = body_rel.clone()?;
            // An empty note writes nothing; Remove is how a comment goes.
            (!text.trim().is_empty()).then(|| AnnotationEdit::EditComment {
                construct: construct.clone(),
                body,
                text,
            })
        }
    });
    wire_open_page(&edit, &stack, NOTE_PAGE, &note_field);

    // A suggested addition or replacement can have what it adds rewritten (TDD 17.78),
    // pre-filled with the Markdown as written, not as the card quotes it, so formatting in
    // it survives an edit that does not touch it. A deletion adds nothing.
    let added = m
        .suggestion
        .as_ref()
        .and_then(|s| s.new.as_ref())
        .and(m.source.src_content.clone())
        .and_then(|side| construct.relative(side));
    if let Some(side) = added {
        let written = construct
            .captured_text(&side)
            .unwrap_or_default()
            .to_string();
        let edit_suggestion = gtk::Button::with_label(EDIT_SUGGESTION);
        // First in the suggestion's row: it changes the suggestion short of settling it.
        change_row.prepend(&edit_suggestion);
        let field = add_edit_page(&stack, SUGGESTION_PAGE, &written, card, sink.clone(), {
            let construct = construct.clone();
            move |text: String| {
                Some(AnnotationEdit::EditSuggestion {
                    construct: construct.clone(),
                    side: side.clone(),
                    text,
                })
            }
        });
        crate::a11y::name_field(&field, "Suggested text");
        wire_open_page(&edit_suggestion, &stack, SUGGESTION_PAGE, &field);
    }
    stack.set_visible_child_name(READ_PAGE);
    row.append(&stack);

    // A highlight keeps its claim; a suggested edit, rejected, keeps its old side;
    // a point comment keeps nothing.
    let kept = match &m.suggestion {
        Some(_) => m.source.src_old.clone(),
        None => m.source.src_content.clone(),
    };
    wire_removal(&remove, card, &sink, m, kept);
}

/// The card's read page, and its two in-place editors: the note, and what a suggestion
/// adds.
const READ_PAGE: &str = "read";
const NOTE_PAGE: &str = "edit";
const SUGGESTION_PAGE: &str = "edit-suggestion";
/// The label of the control that edits an annotation's comment — "Note", not a bare
/// "Edit", since a suggested edit's card also offers to edit the suggestion itself.
pub(crate) const EDIT_NOTE: &str = "Edit Note";
/// The label of the control that rewrites what a suggested addition or replacement adds.
pub(crate) const EDIT_SUGGESTION: &str = "Edit Suggestion";

/// Add an in-place editor page named `name` to `stack`, pre-filled with `initial`: the
/// field with Save / Cancel under it. `build` turns the typed text into the edit to make,
/// or `None` to write nothing. Returns the field, for the control that opens the
/// page to focus.
///
/// Save (or Enter — the shared `CommentEntry` wires both, so neither can be forgotten)
/// closes the card, performs the edit, and leaves the card open on what was saved (TDD
/// 17.7): the render the edit triggered has already installed its markers, so the card is
/// opened afresh on them in the same turn, before a frame can show it closed. The edit is
/// deferred off the active press gesture (GTK4Rs/AP-30). The construct's start does not
/// move when its comment or added side is rewritten, so it names the same annotation in
/// the new render.
///
/// No `style_as_card()`: the field opens pre-filled inside a popover that sets its own
/// width request, so the card's placeholder and 34-char width don't apply.
///
/// Cancel ABORTS THE EDIT, it does not close the card: the stack goes back to the read
/// page and the card stays open on the annotation the user was looking at. Dismissing
/// there conflated "I don't want this edit" with "I'm done with this annotation", and
/// answered both with the more destructive one. Escape and the corner × remain the ways
/// to say the other thing. The field is restored to `initial` on the way out, so an
/// abandoned draft does not greet the user the next time they open the page — that is
/// what makes this an abort rather than a postponement.
fn add_edit_page(
    stack: &gtk::Stack,
    name: &str,
    initial: &str,
    card: &AnnotationCard,
    sink: AnnotationSink,
    build: impl Fn(String) -> Option<AnnotationEdit> + 'static,
) -> sourceview::View {
    let ce = CommentEntry::new(initial, {
        let card = card.clone();
        move |text: &str| {
            let view = card.view();
            card.dismiss();
            let Some(edit) = build(text.to_string()) else {
                return;
            };
            let start = match &edit {
                AnnotationEdit::EditComment { construct, .. }
                | AnnotationEdit::EditSuggestion { construct, .. } => construct.captured_at().start,
                _ => return,
            };
            let sink = sink.clone();
            glib::idle_add_local_once(move || {
                sink(edit);
                if let Some(view) = view {
                    view.reopen_marker_card_on(start);
                }
            });
        }
    });
    let cancel = gtk::Button::with_label("Cancel");
    let ebtns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    ebtns.append(&ce.save);
    ebtns.append(&cancel);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 4);
    page.append(&ce.area);
    page.append(&ebtns);
    stack.add_named(&page, Some(name));
    {
        let stack = stack.clone();
        let entry = ce.field.clone();
        let original = initial.to_string();
        cancel.connect_clicked(move |_| {
            // Deferred (GTK4Rs/AP-30): flipping the stack mutates this button's own
            // ancestor while its press gesture is still active, which GTK reports as
            // "Broken accounting of active state".
            let stack = stack.clone();
            let entry = entry.clone();
            let original = original.clone();
            glib::idle_add_local_once(move || {
                crate::widgets::comment_entry::open_comment(&entry, &original);
                stack.set_visible_child_name(READ_PAGE);
            });
        });
    }
    ce.field
}

/// Wire `button` to show the editor page `name` and focus its `field`, so typing (and the
/// popover's Escape controller) reach it. Deferred off the active press gesture
/// (GTK4Rs/AP-30): flipping the stack mutates this button's own ancestor mid-click.
fn wire_open_page(
    button: &gtk::Button,
    stack: &gtk::Stack,
    name: &'static str,
    field: &sourceview::View,
) {
    let stack = stack.clone();
    let field = field.clone();
    button.connect_clicked(move |_| {
        let stack = stack.clone();
        let field = field.clone();
        glib::idle_add_local_once(move || {
            stack.set_visible_child_name(name);
            crate::widgets::comment_entry::focus_at_end(&field);
        });
    });
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
