//! CriticMarkup annotation markers for [`CodePreviewView`]: the
//! marker/edit data types, the right-margin comment popover UI, and the popover
//! lifecycle methods. Split out of the former monolithic `codeview.rs` (F-AP-002);
//! the chip *painting* stays in the view's `snapshot_layer` vfunc (in `mod.rs`).

use super::card::AnnotationCard;
use super::CodePreviewView;
use crate::widgets::comment_entry::CommentEntry;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene};

/// The byte-range fields that map a marker back to its construct in the source,
/// grouped so [`MarkerData`] stays focused on display concerns (F-AP-001). Every
/// range is original-source byte coordinates EXCEPT `cleaned_content`, which is the
/// delimiter-free cleaned-document range used to pair a marker with its table cell.
/// This is the sub-group the mutation path (Edit/Remove) and the cell-pairing pass
/// (`preview::cells`) consume; the display fields on `MarkerData` don't touch it.
#[derive(Clone)]
pub(crate) struct MarkerSource {
    /// The full construct in the ORIGINAL source, anchored to its own text
    /// (drives Remove and Edit).
    ///
    /// An [`AnchoredSpan`](crate::annotate::AnchoredSpan) rather than a bare
    /// range because a marker outlives the source it was scanned from: the card
    /// built from it can still be open after the user has typed, after an undo,
    /// or after the split-mode live re-render has re-scanned the document. Its
    /// `captured_at().start` remains the annotation's navigation identity.
    pub(crate) construct: crate::annotate::AnchoredSpan,
    /// The highlighted content's byte range in the ORIGINAL source, kept when the
    /// annotation is removed; `None` for a point comment (whole construct goes).
    pub(crate) src_content: Option<std::ops::Range<usize>>,
    /// Comment-body byte range in the ORIGINAL source (drives Edit).
    pub(crate) src_comment_body: std::ops::Range<usize>,
    /// Cleaned-document range of the claim (or point-comment anchor) — used to
    /// pair a marker with a table-cell label after the widgets exist (cell-marker pairing).
    pub(crate) cleaned_content: std::ops::Range<usize>,
}

/// One CriticMarkup comment marker to draw in the preview's right margin,
/// Google-Docs / GitHub-PR style. Carries the buffer offset it anchors to
/// (for vertical placement) and everything the popover needs to display and
/// mutate the annotation.
///
/// **Do NOT "upgrade" this to an inline anchored `GtkTextChildAnchor` marker.**
/// That was tried and rejected: a claim boundary is almost always *mid* pulldown
/// text-event, so the anchor char (`U+FFFC`) would land at a mid-event buffer
/// position and **shift every buffer-offset-based feature after it** — copymap,
/// sourcemap (scroll-sync), link ranges, heading offsets, outline scroll-spy,
/// find/replace, image-tint selection. The margin marker inserts no buffer char
/// at all, so all those maps stay pristine. Accepted trade-off: a self-drawn
/// glyph is not in the a11y tree, so `win.next-annotation`
/// (`open_next_marker_popover`) is the keyboard read path.
#[derive(Clone)]
pub(crate) struct MarkerData {
    /// Buffer char offset the marker sits beside (end of a highlighted claim, or
    /// a point comment's position). Drives the marker's vertical position.
    pub(crate) anchor: i32,
    /// The comment text to show in the popover.
    pub(crate) comment: String,
    /// The highlighted claim snippet (for the popover header); `None` for a
    /// point comment.
    pub(crate) claim: Option<String>,
    /// Byte-range mapping back to the construct in the source (Edit/Remove and the
    /// table-cell pairing). Grouped into [`MarkerSource`] (F-AP-001).
    pub(crate) source: MarkerSource,
    /// When the claim lives in a table cell, a weak ref to that cell's `GtkLabel`
    /// so `snapshot_layer` can refine the chip Y to the cell's row (cell-marker
    /// pairing; table cells). `None` for body-text annotations. `snapshot_layer`
    /// translates this label into its `ScribTableWidget` ancestor (a scroll- and
    /// placeholder-immune local transform, bug C) — no cached Y is needed.
    pub(crate) cell_widget: Option<glib::WeakRef<gtk::Label>>,
    /// For a cell marker, the TABLE's own `GtkTextChildAnchor` — its
    /// `line_yrange` is the table-top buffer-Y the cell→table offset (`cy`) is
    /// added to (bug C). This is NOT `anchor`: `anchor` is a cleaned-offset proxy
    /// that can resolve a line or two ABOVE the table (its `line_yrange` was 36px
    /// too high in testing), whereas `iter_at_child_anchor(this)` lands exactly on
    /// the table's line. Set together with `cell_widget`; `None` for body markers.
    pub(crate) cell_table_anchor: Option<gtk::TextChildAnchor>,
}

/// An annotation mutation requested from a marker's popover (or the selection
/// overlay). The view emits this; the **window layer** performs it on the
/// document source buffer (the view never touches `editor_buf`).
#[derive(Clone, Debug)]
pub(crate) enum AnnotationEdit {
    /// Replace a comment body with new text.
    ///
    /// Anchored on the WHOLE construct, not on the comment body alone: a body is
    /// often a handful of characters ("typo", "?") that occur all over a document,
    /// so it carries far too little identity to re-locate on its own. `body` is
    /// therefore a range RELATIVE to the construct's start, re-absolutised once the
    /// construct has been resolved against the live source.
    EditComment {
        construct: crate::annotate::AnchoredSpan,
        body: std::ops::Range<usize>,
        text: String,
    },
    /// Remove a construct, keeping the highlighted content (`keep_inner = Some`)
    /// or deleting it whole (a point comment, `None`).
    ///
    /// `keep_inner` is RELATIVE to the construct's start, for the same reason as
    /// `EditComment::body`.
    Remove {
        construct: crate::annotate::AnchoredSpan,
        keep_inner: Option<std::ops::Range<usize>>,
    },
    /// Create a new annotation (from the preview selection overlay, Part C).
    Create(CreateAnnotation),
    /// Flip a task-list checkbox's `[ ]`↔`[x]` state character. `span` is the
    /// marker's byte range in the ORIGINAL
    /// source (the editor buffer's raw text — the render records it in CLEANED
    /// bytes; the preview gesture translates it via `cleaned_to_original` before
    /// building this). NOT a CriticMarkup construct: this variant rides the same
    /// preview→editor sink purely to reuse its "edit the editor source, deferred,
    /// one user-action, free dirty + re-render" contract. The enum has thus
    /// outgrown its `Annotation` name (it is really the set of preview-initiated
    /// edits to the editor source buffer — annotations were the first, the task
    /// toggle the second); renaming it across the tree is out of Phase-3b scope
    /// (noted, not fixed).
    ToggleTask { span: std::ops::Range<usize> },
}

/// A new annotation to insert (Part C — selection overlay).
#[derive(Clone, Debug)]
pub(crate) enum CreateAnnotation {
    /// Wrap `range` as a highlight and attach `comment`.
    Highlight {
        range: std::ops::Range<usize>,
        comment: String,
    },
    /// A point comment at byte offset `at` (the cross-block fallback).
    Point { at: usize, comment: String },
}

/// The sink the window installs to perform an [`AnnotationEdit`] on the document.
pub(crate) type AnnotationSink = std::rc::Rc<dyn Fn(AnnotationEdit)>;

/// A closure that reveals the preview's comment-entry card over the CURRENT preview
/// selection. The create overlay ([`crate::preview::annotate`]) stores it so the
/// `win.annotate` action can raise the very same card the selection popover's
/// **Annotate** button does — one code path behind both surfaces (SSOT).
pub(crate) type AnnotateTrigger = std::rc::Rc<dyn Fn()>;

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
fn card_close_button(card: &AnnotationCard) -> gtk::Button {
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
fn build_annotation_row(
    m: &MarkerData,
    card: &AnnotationCard,
    sink: Option<AnnotationSink>,
) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    if let Some(claim) = &m.claim {
        let cl = gtk::Label::new(Some(&format!("\u{201c}{claim}\u{201d}")));
        cl.add_css_class("dim-label");
        cl.set_xalign(0.0);
        cl.set_wrap(true);
        cl.set_max_width_chars(34);
        row.append(&cl);
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
    row.append(&comment_label);

    let Some(sink) = sink else {
        return row.upcast();
    };
    append_edit_remove_controls(m, &row, card, sink);
    row.upcast()
}

/// Append the Edit / Remove controls to a popover `row` and wire their mutation
/// handlers to `sink`. Split out of [`build_annotation_row`] (F-DRY-002) so the
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
    card: &AnnotationCard,
    sink: AnnotationSink,
) {
    let btns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    // `vhomogeneous` (below) sizes BOTH pages to the taller edit page's height, so this
    // shorter read page would otherwise stretch its buttons to fill it — center them at
    // their natural size instead.
    btns.set_valign(gtk::Align::Center);
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
    edit_page.append(&ce.entry);
    edit_page.append(&ebtns);

    let stack = gtk::Stack::new();
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(true);
    stack.add_named(&btns, Some("read"));
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
        let entry = ce.entry.clone();
        edit.connect_clicked(move |_| {
            // Defer off the active press gesture (GTK4Rs/AP-30): flipping the stack still mutates
            // this button's own ancestor mid-click. Focus the entry so typing (and the
            // popover's Escape controller) reach it.
            let stack = stack.clone();
            let entry = entry.clone();
            glib::idle_add_local_once(move || {
                stack.set_visible_child_name("edit");
                let _ = entry.grab_focus();
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
        let entry = ce.entry.clone();
        let original = m.comment.clone();
        cancel.connect_clicked(move |_| {
            // Deferred for the same reason the Edit handler defers (GTK4Rs/AP-30): flipping the
            // stack mutates this button's own ancestor while its press gesture is still
            // active, which GTK reports as "Broken accounting of active state".
            let stack = stack.clone();
            let entry = entry.clone();
            let original = original.clone();
            glib::idle_add_local_once(move || {
                entry.set_text(&original);
                stack.set_visible_child_name("read");
            });
        });
    }
}

/// Group marker indices that share a visual line (identical top-`y`), preserving
/// first-seen order — so multiple annotations on one line collapse to a single
/// margin marker (with a count). Pure; unit-tested without a display.
/// A wall-clock instant to give up at, as a `glib::monotonic_time` (µs) stamp.
///
/// A named type rather than a bare `i64`, because the ONE thing both bounds in this file
/// got wrong was the *shape* of the quantity, not its value: a frame count
/// converts to wall-clock only by assuming a refresh rate the app does not control.
/// Making the shape a type puts it at a choke point — every bound here is constructed by
/// [`Deadline::in_us`] and tested by [`Deadline::expired`], so a future "just count
/// ticks" cannot slip back in unnoticed, and the two sites cannot drift apart again.
///
/// **This is an absolute stamp, deliberately — never an accumulated per-frame delta.**
/// `TabBar::animate_tick` (`widgets/tab/ops.rs`) rightly accumulates a *clamped* frame
/// `dt`, but it is solving a different problem: it eases a position, so a long stall must
/// not jump the ease the whole distance in one step. A deadline has no such need and the
/// clamp would actively harm it — clamping a stall would UNDER-count elapsed time and
/// silently push the give-up past its wall-clock intent, reintroducing exactly the
/// frame-delivery dependence described above. Differencing an absolute clock cannot drift:
/// across a suspend the deadline has simply passed, which is the correct answer.
#[derive(Clone, Copy)]
pub(crate) struct Deadline(i64);

impl Deadline {
    /// A deadline `budget_us` microseconds from now.
    pub(crate) fn in_us(budget_us: i64) -> Self {
        Self(glib::monotonic_time() + budget_us)
    }
    /// Has the wall-clock passed this deadline?
    pub(crate) fn expired(self) -> bool {
        glib::monotonic_time() >= self.0
    }
}

/// An armed request to open marker `target`'s popover as soon as its chip is painted,
/// abandoned once `deadline` passes. Held in the view's `pending_marker_open` and fired
/// by `snapshot_layer`.
pub(crate) struct PendingMarkerOpen {
    pub(crate) target: usize,
    /// Buffer offset of the target's anchor, recorded at arm time.
    ///
    /// Carried on the request rather than re-derived from `markers[target]` at
    /// dispatch so the "has the scroll arrived?" test in `snapshot_layer` costs no
    /// second lookup and cannot go stale if the marker list is rebuilt mid-flight.
    pub(crate) anchor: i32,
    pub(crate) deadline: Deadline,
}

/// Wall-clock budget for one programmatic marker navigation.
///
/// **Wall-clock, not a frame count.** Everything this navigation waits on is measured
/// in TIME — GTK's scroll animation is a fixed 200 ms and is *distance-independent*
/// (`#define ANIMATION_DURATION 200`, gtkscrolledwindow.c:196; armed by the
/// ScrolledWindow, not the view, at :2318) and lazy line validation runs on the clock
/// too. A tick callback, by contrast, fires once per FRAME at a refresh rate the app
/// does not control, so a frame count converts to wall-clock only by assuming a rate.
/// The bound this replaced (`MAX_FRAMES = 45`, "~0.75s at 60Hz") was ~3× the animation
/// at 60Hz but **187 ms at 240Hz — shorter than the 200 ms animation it existed to
/// cover**, so on a high-refresh panel the poll expired before the scroll had even
/// finished and the silent give-up branch fired on every navigation. The same constant
/// fails the opposite way when `gtk-enable-animations` is FALSE (gtkscrolledwindow.c:3601
/// → duration 0 → the value is set instantly): 45 frames is then wildly over-generous.
/// A constant that is both too small and too large depending on the environment is the
/// definition of a wrong-shaped bound; a duration is correct in both.
const NAV_BUDGET_US: i64 = 2_000_000; // 2s — generous; the happy path exits on convergence.

/// Wall-clock budget for the GTK4Rs/AP-118 re-pin guard, replacing a `ticks >= 48` ("~0.8s
/// @60fps") count that inherited exactly the same flaw: at 240Hz it disarmed at
/// ~200 ms, i.e. right at the animation boundary, so a DEFERRED validation scroll
/// landing after that was no longer re-pinned and GTK4Rs/AP-118's original symptom returned
/// (the view jumps to the table top and the click is dropped). Safe to keep armed this
/// long precisely because a genuine user scroll disarms it at once (`user_scrolling`).
const REPIN_GUARD_US: i64 = 1_500_000; // 1.5s

pub(crate) fn group_by_line(ys: &[i32]) -> Vec<(i32, Vec<usize>)> {
    let mut out: Vec<(i32, Vec<usize>)> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        if let Some(g) = out.iter_mut().find(|(gy, _)| *gy == y) {
            g.1.push(i);
        } else {
            out.push((y, vec![i]));
        }
    }
    out
}

/// Index of the marker to visit next from buffer char offset `from`: the smallest
/// anchor **strictly after** `from`, wrapping to the document's smallest anchor when
/// `from` sits at or past the last one. `None` only when there are no markers.
///
/// Strictly-after is what makes repeated activations advance rather than re-open the
/// annotation already under the caret; the wrap is what makes the command a cycle
/// instead of a dead end at the last annotation. Selection is by anchor, never by
/// position in `markers` — the two coincide today (the renderer emits in document
/// order) but nothing enforces that, and a silent dependency on it would surface as
/// "Next Annotation visits them out of order" long after the render changed. Ties
/// resolve to the lowest index, matching the order `group_by_line` packs co-located
/// markers into their shared hit-box.
///
/// Pure, so the part of the a11y path that can be wrong *without any visible symptom*
/// is unit-tested without a display. What the caller does with the index — scroll,
/// await the paint, open the popover — is the same shell the pointer path already
/// exercises.
pub(crate) fn next_marker_index(anchors: &[i32], from: i32) -> Option<usize> {
    let after = anchors
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, anchor)| anchor > from)
        .min_by_key(|&(_, anchor)| anchor)
        .map(|(index, _)| index);
    after.or_else(|| {
        anchors
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|&(_, anchor)| anchor)
            .map(|(index, _)| index)
    })
}

impl CodePreviewView {
    /// Set this render's CriticMarkup comment markers and
    /// repaint. Positions are measured live in `snapshot_layer`, so a redraw is
    /// all that is needed.
    pub(crate) fn set_markers(&self, markers: Vec<MarkerData>) {
        use gtk::subclass::prelude::*;
        // Cell markers refine their row Y via a cell→table transform (bug C), which is
        // unavailable until the table has allocated its cells (incremental anchored-child
        // allocation). Nudge one more paint on idle — after the layout pass — so the chip
        // snaps from the table-top fallback to its exact row without needing a mouse-move.
        let has_cell_marker = markers.iter().any(|m| m.cell_widget.is_some());
        self.imp().markers.replace(markers);
        self.queue_draw();
        if has_cell_marker {
            gtk::glib::idle_add_local_once(glib::clone!(
                #[weak(rename_to = v)]
                self,
                move || {
                    v.queue_draw();
                }
            ));
        }
    }

    /// Install the sink the window uses to mutate the document source when a
    /// popover's Edit/Remove (or the selection overlay's Annotate) fires. Until
    /// set, popovers are read-only.
    pub(crate) fn set_annotation_sink(&self, sink: AnnotationSink) {
        use gtk::subclass::prelude::*;
        self.imp().annotation_sink.replace(Some(sink));
    }

    /// The installed annotation sink, if any (the create overlay reads it at Save).
    pub(crate) fn annotation_sink(&self) -> Option<AnnotationSink> {
        use gtk::subclass::prelude::*;
        self.imp().annotation_sink.borrow().clone()
    }

    /// Store the closure that raises the comment-entry card over the current
    /// selection (installed by the create overlay). Lets the `win.annotate` action
    /// share the popover Annotate button's exact code path.
    pub(crate) fn set_annotate_trigger(&self, trigger: AnnotateTrigger) {
        use gtk::subclass::prelude::*;
        self.imp().annotate_trigger.replace(Some(trigger));
    }

    /// Raise the comment-entry card over the current selection via the stored
    /// trigger. Returns `false` (a no-op) when no trigger is installed (e.g. before
    /// the create overlay wired this render) — the caller can then do nothing.
    pub(crate) fn trigger_annotate(&self) -> bool {
        use gtk::subclass::prelude::*;
        let trigger = self.imp().annotate_trigger.borrow().clone();
        if let Some(trigger) = trigger {
            trigger();
            true
        } else {
            false
        }
    }

    /// Remember the selection action popover so `dispose` can unparent it (GTK does
    /// not auto-unparent `set_parent`-attached native popovers — GTK4Rs/AP-80).
    pub(crate) fn store_overlay_popover(&self, pop: &gtk::Popover) {
        use gtk::subclass::prelude::*;
        // Held as a PersistentPopover so `dispose` tears it down through the one
        // popdown→unparent order (ScrAP-144); the create overlay is non-autohide.
        self.imp()
            .overlay_popover
            .replace(Some(crate::saferizer::PersistentPopover::adopt(
                pop.clone(),
            )));
    }

    /// Store the create overlay's primary-clipboard `changed` handler (table-cell annotation) so
    /// the view's `dispose` disconnects it from the long-lived display clipboard.
    /// Replaces any previous handler (disconnecting it first) so re-wiring never
    /// stacks handlers on the shared clipboard.
    pub(crate) fn store_primary_sel_handler(
        &self,
        clip: &gdk::Clipboard,
        id: glib::SignalHandlerId,
    ) {
        use gtk::subclass::prelude::*;
        if let Some((old_clip, old_id)) = self.imp().primary_sel_handler.borrow_mut().take() {
            old_clip.disconnect(old_id);
        }
        self.imp()
            .primary_sel_handler
            .replace(Some((clip.clone(), id)));
    }

    /// Whether the annotation card is currently showing an annotation. The
    /// create-selection overlay consults this so it doesn't re-show the Annotate popover
    /// over a table-cell selection that is still live while the user is reading/editing an
    /// existing marker (table-cell annotation — otherwise the two popovers stack, since a
    /// cell selection is not cleared by a margin-marker click).
    ///
    /// "Open" is the card's own lag-free flag, NOT slot presence (the card is a persistent
    /// instance, so the slot stays `Some` for the view's life) and NOT `is_visible()`
    /// (GTK4Rs/AP-117) — which now differs from "open" for a second reason too: the card hides
    /// itself while its chip is scrolled off the viewport, without ending its session.
    /// **No production caller by design** — the two visual-collision gates that used to
    /// call it now (correctly) ask [`marker_card_is_showing`] instead. It survives as the
    /// predicate the *teardown* tests need: they assert a reload ENDS the session, and
    /// asking "is it on screen" there would pass for a card that is merely hidden.
    ///
    /// [`marker_card_is_showing`]: Self::marker_card_is_showing
    #[cfg_attr(not(all(test, feature = "gtk-integration-tests")), allow(dead_code))]
    pub(crate) fn has_open_marker_popover(&self) -> bool {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_card
            .borrow()
            .as_ref()
            .is_some_and(|c| c.is_open())
    }

    /// Whether the annotation card is **on screen right now**.
    ///
    /// The predicate for "don't raise another surface over it" — distinct from
    /// [`has_open_marker_popover`], which answers the *session* question and stays true
    /// while the card is hidden because its chip scrolled off the viewport. Gating a
    /// visual-collision decision on the session would suppress the create-Annotate
    /// affordance against a card the user cannot see, with nothing on screen to explain
    /// why (a regression introduced the moment the card gained a hidden state, and the
    /// reason the two questions now have two answers).
    ///
    /// [`has_open_marker_popover`]: Self::has_open_marker_popover
    pub(crate) fn marker_card_is_showing(&self) -> bool {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_card
            .borrow()
            .as_ref()
            .is_some_and(|c| c.is_showing())
    }

    /// Dismiss the annotation card if one is showing.
    ///
    /// For callers about to destroy this view: the card is parented to it, so the view
    /// cannot be torn down out from under it (see `dispose`). `dispose` dismisses too, but
    /// a caller that KNOWS the preview is being replaced should say so here rather than
    /// rely on teardown ordering to rescue it.
    ///
    /// Clones the card OUT and drops the slot borrow before dismissing: the `closed`
    /// handler only touches a `Cell`, but holding a `RefCell` borrow across a signal
    /// emission is the GTK4Rs/AP-61 re-entrant-abort shape, and it costs nothing to not do it.
    /// Reads the slot directly rather than `marker_card()`, which would create a card just
    /// to close it.
    pub(crate) fn popdown_marker_popover(&self) {
        use gtk::subclass::prelude::*;
        let card = self.imp().marker_card.borrow().clone();
        if let Some(card) = card {
            card.dismiss();
        }
    }

    /// The chip rectangle for marker `index`, in this view's WIDGET coordinates,
    /// **re-derived from the marker's own anchor at the moment of the call**.
    ///
    /// This is the anti-stale-rect primitive. Nothing caches its result: a widget
    /// rectangle is only meaningful at the scroll offset it was read at, and every one of
    /// the card's three anchoring defects was a rectangle outliving its offset. It is called
    /// on the way
    /// through its `show` vfunc and again on every scroll, so it always points at where
    /// the chip *is*, never where it *was*.
    ///
    /// Shares [`marker_row_y_h`] and [`chip_rect`] with the painter, so the anchor and the
    /// drawn chip cannot drift apart (GTK4Rs/AP-78/GTK4Rs/AP-127). Every read here is cache-free and
    /// side-effect-free — `line_yrange` is a btree height read, and
    /// `buffer_to_window_coords` is a bare `y -= yoffset` — so this forces no layout
    /// validation and is safe to call from a scroll handler (GTK4Rs/AP-22). Returns `None` for an
    /// index this render's `markers` no longer has.
    pub(crate) fn marker_chip_rect(&self, index: usize) -> Option<gdk::Rectangle> {
        use gtk::subclass::prelude::*;
        let markers = self.imp().markers.borrow();
        let m = markers.get(index)?;
        let buffer = self.buffer();
        let (row_y, row_h) = crate::codeview::geometry::marker_row_y_h(self, &buffer, m);
        let (x, buf_y, w, h) = crate::codeview::geometry::chip_rect(
            self.width() as f32,
            self.right_margin() as f32,
            row_y,
            row_h,
        );
        let (_, widget_y) =
            self.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, buf_y as i32);
        Some(gdk::Rectangle::new(x as i32, widget_y, w as i32, h as i32))
    }

    /// The stable identity of the annotation a marker group is anchored to: the
    /// `src_span.start` of its first member — the same key the annotations viewer
    /// navigates by, and the key [`marker_index_for_src`] resolves back to an index.
    ///
    /// The card stores THIS, never the index: `markers` is rebuilt on every render, so an
    /// index is a bet that the document has not changed underneath it (GTK4Rs/AP-74, and the
    /// ScrAP-187 family). An identity that stops resolving is the well-defined "this
    /// annotation is gone" answer, which is exactly when the card should stop pointing.
    ///
    /// [`marker_index_for_src`]: Self::marker_index_for_src
    fn marker_anchor_identity(&self, idxs: &[usize]) -> Option<usize> {
        use gtk::subclass::prelude::*;
        let markers = self.imp().markers.borrow();
        let first = idxs.first()?;
        Some(markers.get(*first)?.source.construct.captured_at().start)
    }

    /// Whether widget-coordinate `(x, y)` is over a comment marker's hit-box — so
    /// the hover cursor can switch to a pointer, signalling the marker is clickable.
    pub(crate) fn is_over_marker(&self, x: f32, y: f32) -> bool {
        use gtk::subclass::prelude::*;
        self.imp().marker_hitboxes.borrow().iter().any(|(r, _)| {
            x >= r.x() && x <= r.x() + r.width() && y >= r.y() && y <= r.y() + r.height()
        })
    }

    /// The marker group whose hit-box contains `(x, y)` (widget coords), or `None`.
    ///
    /// Returns the group's annotation indices — the marker's *identity* — because that
    /// is what a complete click compares: `saferizer::ClickActivation` opens the
    /// popover only when a press and its release both land on the **same** marker, so a
    /// hit-test answering a bare `bool` (as this one's caller once did on release
    /// alone) cannot express the question (GTK4Rs/AP-169).
    ///
    /// The hit-box answers WHICH marker was clicked; it does not supply the card's
    /// anchor. The card re-derives that itself, so the rect stops here.
    pub(super) fn marker_group_at(&self, x: f64, y: f64) -> Option<Vec<usize>> {
        use gtk::subclass::prelude::*;
        let (px, py) = (x as f32, y as f32);
        self.imp()
            .marker_hitboxes
            .borrow()
            .iter()
            .find(|(r, _)| {
                px >= r.x() && px <= r.x() + r.width() && py >= r.y() && py <= r.y() + r.height()
            })
            .map(|(_, idxs)| idxs.clone())
    }

    /// Open the comment popover for the FIRST annotation anchored strictly after
    /// `from` (buffer char offset), wrapping to the first annotation in the document
    /// when there is none. Returns `false` only when the document has no annotations.
    ///
    /// **This is the accessibility path.** The margin marker is self-drawn in
    /// `snapshot_layer`, so it is not a widget and therefore not in the a11y tree —
    /// a pointer click was the ONLY way to read a comment, which left keyboard and
    /// screen-reader users with no route to them at all (the drawn marker is
    /// deliberate: an anchored `U+FFFC` marker would shift every buffer-offset
    /// consumer — see [`MarkerData`]). Once the popover opens its contents are
    /// ordinary widgets (labels + Edit/Remove buttons), so it is accessible from
    /// there; this method only supplies the missing *reach*.
    ///
    /// **Two-phase by necessity (GTK4Rs/AP-97).** `marker_hitboxes` is repopulated on
    /// each `snapshot_layer(AboveText)` paint and therefore only ever holds the
    /// **visible** markers — an off-screen target has no hit-box to open against. So:
    /// arm a `pending_marker_open` request, converge-scroll to the anchor
    /// (`converge_and_scroll_to_offset`), and let `snapshot_layer` fire the request the
    /// instant the chip paints. When the target is already on screen the fast path opens
    /// it immediately, so the common case does not flash.
    ///
    /// The whole navigation is bounded by `NAV_BUDGET_US` — a **wall-clock** deadline,
    /// not a frame count; on expiry the document is left scrolled to the
    /// target rather than silently doing nothing.
    pub(crate) fn open_next_marker_popover(&self, from: i32) -> bool {
        use gtk::subclass::prelude::*;
        // Pick the target by ANCHOR (buffer offset), not by hit-box: the target may be
        // off-screen and thus absent from `marker_hitboxes` entirely. The choice is
        // delegated to the pure `next_marker_index` so it can be tested headlessly, then
        // handed to the shared index-addressed primitive below.
        let target = {
            let markers = self.imp().markers.borrow();
            let anchors: Vec<i32> = markers.iter().map(|m| m.anchor).collect();
            match next_marker_index(&anchors, from) {
                Some(index) => index,
                None => return false, // no annotations in this document
            }
        };
        self.open_marker_popover_at(target)
    }

    /// Open the comment popover for the marker at `index` in this render's `markers`
    /// (document order). Returns `false` only when `index` is out of range.
    ///
    /// **The single index-addressed navigation primitive.** Both the accessibility
    /// `win.next-annotation` path (via [`open_next_marker_popover`]) and the
    /// annotations viewer (via [`marker_index_for_src`]) route through here, so the two
    /// callers exercise ONE code path and cannot drift.
    ///
    /// **Two-phase by necessity (GTK4Rs/AP-97).** `marker_hitboxes` is repopulated
    /// on each `snapshot_layer(AboveText)` paint and therefore only ever holds the
    /// **visible** markers — an off-screen target has no hit-box to open against. So:
    /// arm a `pending_marker_open` request, converge-scroll to the anchor
    /// (`converge_and_scroll_to_offset`), and let `snapshot_layer` fire the request the
    /// instant the chip paints. When the target is already on screen the fast path opens
    /// it immediately, so the common case does not flash. The whole navigation is bounded
    /// by `NAV_BUDGET_US` — a **wall-clock** deadline, not a frame count; on expiry the
    /// document is left scrolled to the target rather than silently doing nothing.
    ///
    /// [`open_next_marker_popover`]: Self::open_next_marker_popover
    /// [`marker_index_for_src`]: Self::marker_index_for_src
    pub(crate) fn open_marker_popover_at(&self, index: usize) -> bool {
        use gtk::subclass::prelude::*;
        // A new navigation supersedes any converge-scroll still running from a previous
        // one: bump the token so older tick(s) self-cancel instead of fighting this
        // navigation over the shared vadjustment (see `nav_generation`). Bumped BEFORE the
        // fast path too, so even an immediate on-screen open cancels a prior converge
        // scroll that would otherwise keep pulling the view back to its stale target.
        let generation = self.imp().nav_generation.get().wrapping_add(1);
        self.imp().nav_generation.set(generation);

        let anchor = {
            let markers = self.imp().markers.borrow();
            match markers.get(index) {
                Some(m) => m.anchor,
                None => return false, // out of range (stale render / no such marker)
            }
        };
        // Fast path: already painted this frame ⇒ open without scrolling.
        if let Some((_rect, idxs)) = self.marker_hitbox_containing(index) {
            self.open_marker_popover(&idxs);
            return true;
        }
        // Arm the open-request BEFORE scrolling: `snapshot_layer` fires it the instant the
        // target's hit-box exists (see `pending_marker_open`). This replaces a frame-count
        // poll that re-checked the hit-box every tick and hoped — the request is now an
        // EVENT delivered at exactly the goal state.
        let deadline = Deadline::in_us(NAV_BUDGET_US);
        self.imp()
            .pending_marker_open
            .replace(Some(PendingMarkerOpen {
                target: index,
                anchor,
                deadline,
            }));
        self.converge_and_scroll_to_offset(anchor, deadline, generation);
        true
    }

    /// The index in this render's `markers` of the chip whose annotation begins at
    /// source byte `start` — the annotations viewer's **identity → index** resolver.
    ///
    /// Navigation keys on the annotation's `src_span.start` (its stable identity in the
    /// source), never on a positional row index: the viewer's list is a *filtered
    /// subsequence* of all CriticMarkup constructs, so a row's list position is not its
    /// chip's position (GTK4Rs/AP-74). `None` is the
    /// well-defined "no chip for this annotation" answer — surfaced by the type rather
    /// than by opening a wrong popover — reached when the annotation was filtered out of
    /// `markers` or the render is momentarily stale.
    pub(crate) fn marker_index_for_src(&self, start: usize) -> Option<usize> {
        use gtk::subclass::prelude::*;
        self.imp()
            .markers
            .borrow()
            .iter()
            // Identity is the construct's captured START offset, which is exactly
            // what the viewer's rows key on. `captured_at` (not a live resolve) is
            // right here: both sides derive from the SAME scan of the same source,
            // so they agree by construction — and a resolve would make the lookup
            // depend on the live buffer, which the viewer's key does not.
            .position(|m| m.source.construct.captured_at().start == start)
    }

    /// Scroll to buffer `offset` for a PROGRAMMATIC navigation: converge on the
    /// vadjustment's `upper`, landing with non-animated `set_value`.
    ///
    /// **Why not `scroll_to_mark`** (which `scroll_to_buffer_offset` rightly still uses
    /// for the outline and every user-driven case — ScrAP-14)? For programmatic navigation
    /// to a possibly-fresh, possibly-far target it is the wrong primitive, three ways:
    ///  * It **animates** (the fixed 200 ms above), putting a wait into the budget that
    ///    this path does not need at all.
    ///  * Its pending scroll is a **one-shot** (gtktextview.c:2836/2868) that does not
    ///    re-track as lazy validation changes line heights beneath it, and `flush_scroll`
    ///    validates only ±2× the widget height (:2848-2851) while first-paragraph pinning
    ///    (:4874-4935) holds the view near the top as heights grow. For a fresh + far
    ///    target in a long document it can therefore land near the top and **never**
    ///    converge — the second, independent failure of a frame-count bound, which no
    ///    frame count could fix.
    ///  * Being animated, it **loses to any `set_value`** — including our own GTK4Rs/AP-118
    ///    re-pin guard, since `set_value` calls `end_updating` and cancels an in-flight
    ///    animation (gtkadjustment.c:532). There is one animation slot per adjustment.
    ///
    /// So use the primitive that wins outright, and re-aim it every frame as heights
    /// validate (the GTK4Rs/AP-115 progressive restore). Each tick re-reads `line_yrange` — a
    /// pure cached btree read with NO validation side-effect, which is exactly why it is
    /// safe to call here and exactly why it returns an *estimate* until the line
    /// validates. Re-reading is what converges it.
    ///
    /// Re-aiming every tick does three jobs at once: it drives validation toward the
    /// target, it converges as estimates become real heights, and — because the last aim
    /// simply **stays applied** — it makes the give-up visible: on
    /// timeout the document is left scrolled at the target instead of the old code's
    /// nothing-at-all, where the user pressed the accelerator and saw no change whatever.
    fn converge_and_scroll_to_offset(&self, offset: i32, deadline: Deadline, generation: u64) {
        use gtk::subclass::prelude::*;
        // A programmatic scroll takes over from any hand-scrolling, and caches the target
        // as the re-anchor line (ScrAP-65 bookkeeping, matching `scroll_to_buffer_offset`).
        self.imp().user_scrolling.set(false);
        self.imp()
            .restore_target_line
            .set(Some(self.buffer().iter_at_offset(offset).line()));

        // `add_tick_callback` takes an `Fn`, not an `FnMut`, so the convergence state
        // needs interior mutability (Cell — GTK main thread only). The callback's first
        // argument IS the widget, so nothing is captured strongly (ScrAP-60: a tick callback
        // strong-capturing its own widget is an uncollectable cycle).
        let last_upper = std::cell::Cell::new(f64::NAN);
        let stable = std::cell::Cell::new(0u32);
        self.add_tick_callback(move |view, _| {
            // Superseded by a NEWER navigation → stop at once, and DON'T touch
            // `pending_marker_open`: that request now belongs to the newer navigation,
            // whose own tick (or `snapshot_layer` dispatch) will service it. Continuing
            // would re-aim the shared vadjustment at this navigation's now-stale target
            // and fight the newer one — the rapid-activation scroll/selection desync
            // (see `nav_generation`).
            if view.imp().nav_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            // The request is gone — `snapshot_layer` either dispatched it (the chip
            // painted) or it expired. If it was dispatched, `open_marker_popover`'s GTK4Rs/AP-118
            // re-pin guard now owns this adjustment, and our re-aiming would be a SECOND
            // `set_value` writer fighting it over the same slot. Stop.
            if view.imp().pending_marker_open.borrow().is_none() {
                return glib::ControlFlow::Break;
            }
            // Never fight a human: the user grabbing the scrollbar/wheel/keys ends the
            // navigation (`user_scrolling` is set by every hand-scroll input source).
            if view.imp().user_scrolling.get() {
                view.imp().pending_marker_open.replace(None);
                return glib::ControlFlow::Break;
            }
            let Some(vadj) = view.vadjustment() else {
                view.imp().pending_marker_open.replace(None);
                return glib::ControlFlow::Break;
            };
            // Re-aim at the target's current best-known y. yalign 0.0 — top of viewport,
            // matching `scroll_to_buffer_offset`.
            let it = view.buffer().iter_at_offset(offset);
            let (y, _h) = view.line_yrange(&it);
            let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
            crate::saferizer::scrollpos::jump(&vadj, (y as f64).clamp(vadj.lower(), max));

            let upper = vadj.upper();
            if upper == last_upper.get() {
                stable.set(stable.get() + 1);
            } else {
                stable.set(0);
                last_upper.set(upper);
            }
            // Converged: `upper` stopped growing for 2 consecutive frames, so
            // `line_yrange(target)` is a real height now and the aim just applied is the
            // final, correct one (F4 — reading it BEFORE convergence yields a stale/0 y and
            // silently computes off a wrong origin). Leave the request armed: the paint
            // this scroll causes is what dispatches it.
            if stable.get() >= 2 {
                return glib::ControlFlow::Break;
            }
            if deadline.expired() {
                // Visible give-up. The last aim stays applied, so the document is
                // left scrolled at the target even though the chip never painted. Disarm,
                // so no popover can open at a stale rect later.
                view.imp().pending_marker_open.replace(None);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    /// The painted hit-box whose group contains marker `idx`, if it is on screen.
    /// Markers on one visual line share ONE grouped hit-box (`group_by_line`), so the
    /// lookup is by membership, not equality.
    fn marker_hitbox_containing(&self, idx: usize) -> Option<(graphene::Rect, Vec<usize>)> {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_hitboxes
            .borrow()
            .iter()
            .find(|(_, idxs)| idxs.contains(&idx))
            .map(|(r, idxs)| (*r, idxs.clone()))
    }

    /// Build and pop up the comment popover for the annotations on one line.
    /// `pub(super)` so the view's own `snapshot_layer` can dispatch a pending open.
    pub(super) fn open_marker_popover(&self, idxs: &[usize]) {
        use gtk::subclass::prelude::*;
        // CHOKE-POINT crash guard (ScrAP-152/GTK4Rs/AP-128/GTK4Rs/AP-63). Every path that opens
        // a marker popover funnels through here, and it ends in `popup()`, which realizes
        // the popover's surface. On an UNREALIZED view that surface's parent is NULL →
        // `gdk_surface_new_popup` `GDK_IS_SURFACE` assert → NULL deref SIGSEGV
        // (gtkpopover.c:944-947 / gdksurface.c:885). The one deferred caller
        // (`snapshot_layer`'s idle dispatch) already weak-captures to de-pin a torn-down
        // view; this guard is the second, choke-point half — a realized-but-unmapped
        // fresh tab is still realized, so a legitimate open is never wrongly skipped.
        if !self.is_realized() {
            return;
        }
        // Suppress a redundant re-open of the marker that is ALREADY showing. This function
        // rebuilds the popover child via `set_child` below, so a stray re-dispatch of the SAME
        // marker — a snapshot/idle re-fire (this function is dispatched from `snapshot_layer`,
        // mod.rs), or a mis-routed pointer echo landing back on the marker hitbox on Win32/
        // Quartz — would rebuild the read view over whatever is in the popover and WIPE it.
        // Most visibly it wipes Edit's inline entry the instant it appears, so Edit reads as
        // "does nothing" (mac/Windows, cross-platform). Skipping the rebuild here preserves the
        // in-progress state. A switch to a DIFFERENT marker has different `idxs` and still
        // rebuilds; `open_next_marker_popover` advances to a strictly-different marker, so the
        // advance/switch navigation is untouched. The card's own `is_open` (never
        // `is_visible`) is the authoritative flag (GTK4Rs/AP-117).
        let card = self.marker_card();
        if card.is_open() && card.shows_exactly(idxs) {
            return;
        }
        let markers = self.imp().markers.borrow();
        let sink = self.imp().annotation_sink.borrow().clone();

        // Pop down the selection CREATE popover (Annotate) if it is showing.
        // Table-cell annotation: a table-cell selection auto-shows the create popover (driven by the
        // primary clipboard), and clicking a cell's margin marker does NOT clear that
        // label selection — so without this the create popover and the marker's comment
        // popover would coexist (two popovers over the same table). Opening a marker means
        // the user is acting on an EXISTING annotation, so the create affordance dismisses.
        if let Some(create) = self.imp().overlay_popover.borrow().as_ref() {
            create.popdown();
        }
        // NO `set_pointing_to` here, deliberately, and no rectangle reached this function
        // to supply one. Anchoring is the card's own job and happens inside its `show`
        // vfunc, on the way through every presentation — see `codeview::card`. That is
        // what makes the stale anchor unrepresentable rather than merely fixed: this path used to
        // point the card at a rect captured before the navigation's converge-scroll, and
        // there is now no rect to capture and no call site that could pass a stale one.
        let popover: gtk::Popover = card.clone().upcast();
        // Rebuild only the CHILD content for THIS marker (GTK unparents the old child on
        // set_child), then re-show the same card instance.
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
        // Top margin is smaller than the others because the close button below supplies
        // its own visual breathing room; without the reduction the card reads top-heavy.
        vbox.set_margin_top(2);
        vbox.set_margin_bottom(8);
        vbox.set_margin_start(8);
        vbox.set_margin_end(8);
        vbox.set_width_request(280);
        vbox.append(&card_close_button(&card));
        for (n, &idx) in idxs.iter().enumerate() {
            let Some(m) = markers.get(idx) else { continue };
            if n > 0 {
                vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            vbox.append(&build_annotation_row(m, &card, sink.clone()));
        }
        popover.set_child(Some(&vbox));

        // Bug B: keep the pane from jerking (and the popover from missing) on a marker
        // click. Two independent scroll triggers, two defenses:
        //  • FIRST-realization (the very first `set_parent`+`popup` of the view-parented
        //    popover forces the view's first full validation) → handled by the one-shot
        //    PRE-WARM in `prewarm_marker_popover` (at load, scroll 0).
        //  • RE-VALIDATION mid-session: an in-place annotation re-render leaves the tall
        //    table's layout dirty; a later `popup()` flushes it → `validate_onscreen` re-
        //    anchors on the first onscreen paragraph — and a table is ONE `GtkTextChildAnchor`
        //    = one paragraph, so when the viewport sits MID-table the "first onscreen
        //    paragraph" is the table's own line (its top) → a jump to the table top
        //    (gtktextview.c:4707/4746 "odd side effect of triggering a scroll").
        // A one-shot idle restore LOSES that race, and `scroll_to_mark` can't restore a
        // mid-table position (the whole table is one mark → it only lands at the table top).
        // So HOLD the saved scroll value across the popup's settle: re-pin it on each
        // `value-changed`. CRITICAL (operator repro: scroll up, scroll back down, THEN click
        // — only the first time): the validation scroll is DEFERRED, firing several frames
        // after `popup()`, so a short quiet-tick disconnect bails BEFORE it fires and misses
        // it. Keep the guard armed for a generous window and disconnect the instant the USER
        // scrolls — distinguished by `user_scrolling`, which every hand-scroll input source
        // sets but a programmatic validation scroll does NOT. Reset it false right before
        // `popup()` so the popup's own validation scroll is (correctly) treated as
        // non-user and re-pinned. `grab_focus()` is a harmless complement (does not scroll,
        // gtktextview.c:5704).
        let saved_scroll = self.vadjustment().map(|a| a.value());
        let _ = self.grab_focus();
        self.imp().user_scrolling.set(false);
        // Hand the card what it is showing and — crucially — the annotation IDENTITY it is
        // anchored to, not a position. From here on the card owns its own geometry: it
        // re-derives the chip rectangle whenever it is shown and on every scroll, which is
        // K and M both. `drop(markers)` first: `marker_anchor_identity` borrows the same
        // `RefCell`, and holding a borrow across a re-entrant borrow aborts (GTK4Rs/AP-61).
        drop(markers);
        // No identity means no annotation to point at — nothing to open.
        let Some(anchor_src) = self.marker_anchor_identity(idxs) else {
            return;
        };
        card.adopt(idxs, anchor_src);
        // The navigation this card belongs to. If a NEWER navigation starts (which bumps
        // `nav_generation`), this card's re-pin guard must stop at once — otherwise it
        // keeps forcing the scroll back to THIS card's position and fights the new
        // navigation's converge-scroll over the shared adjustment. That is the second half
        // of the rapid-activation desync (the first being the converge ticks themselves);
        // `user_scrolling` cannot catch it, because a programmatic navigation deliberately
        // sets `user_scrolling = false`. See `nav_generation`.
        let open_gen = self.imp().nav_generation.get();
        // `present()`, not `popup()`: the card arms its scroll-tracking (K) and its
        // outside-click dismissal (L) and then shows itself, which routes through its
        // `show` vfunc and therefore recomputes its anchor (M).
        card.present();
        // Autohide used to move focus into the popover on open (which is why the comment
        // label above is non-selectable); non-autohide does not, so do it explicitly — this
        // restores keyboard access (Tab/Space to Edit/Remove, and Escape via the popover's
        // CAPTURE-phase controller, which only sees the key event if a popover descendant is
        // actually focused, i.e. the popover is in the event's ancestor chain).
        //
        // DEFERRED one tick, deliberately: a same-tick `child_focus` right after `popup()`
        // runs BEFORE the popover's surface is mapped and silently no-ops (real-keyboard
        // confirmed on Quartz, mac) — focus never enters the popover, so Tab/Space AND Escape
        // are dead until the user clicks inside. The idle runs after the surface is mapped, so
        // Edit is focused when the popover first presents. Same idle-defer family as the Edit
        // handler; focusing a button does not scroll, so it doesn't disturb the Bug-B re-pin.
        glib::idle_add_local_once(glib::clone!(
            #[weak]
            popover,
            move || {
                let _ = popover.child_focus(gtk::DirectionType::TabForward);
            }
        ));
        if let (Some(vadj), Some(saved)) = (self.vadjustment(), saved_scroll) {
            // Re-pin `saved` on each `value-changed` UNLESS the user has taken over scrolling
            // OR a newer navigation has superseded this card.
            // Edge-triggered + self-healing (researcher, gtktextview.c:2890/4897/8340/8395).
            let reentrant = std::rc::Rc::new(std::cell::Cell::new(false));
            let re = reentrant.clone();
            let id = vadj.connect_value_changed(glib::clone!(
                #[weak(rename_to = v)]
                self,
                move |adj| {
                    if re.get() {
                        return;
                    }
                    // Stop re-pinning if the user grabbed the scrollbar/wheel/keys
                    // (`user_scrolling`) OR a newer navigation superseded this card (`open_gen`
                    // stale) — in either case this guard must not fight the current scroll.
                    if v.imp().user_scrolling.get() || v.imp().nav_generation.get() != open_gen {
                        return;
                    }
                    if (adj.value() - saved).abs() > 0.5 {
                        re.set(true);
                        crate::saferizer::scrollpos::jump(adj, saved);
                        re.set(false);
                    }
                }
            ));
            // Disconnect the guard the moment the user scrolls or a newer navigation
            // supersedes it, else after a generous WALL-CLOCK window (`REPIN_GUARD_US`) that
            // safely spans a DEFERRED validation scroll. This was a `ticks >= 48` frame
            // count; see `REPIN_GUARD_US` for why a count is the wrong shape — it is the same
            // defect as the navigation bound, in the opposite direction.
            let hid = std::rc::Rc::new(std::cell::RefCell::new(Some(id)));
            let vadj_t = vadj.clone();
            let weak_t = self.downgrade();
            let guard_deadline = Deadline::in_us(REPIN_GUARD_US);
            self.add_tick_callback(move |_, _| {
                let done = weak_t.upgrade().is_none_or(|v| {
                    v.imp().user_scrolling.get() || v.imp().nav_generation.get() != open_gen
                });
                if done || guard_deadline.expired() {
                    if let Some(id) = hid.borrow_mut().take() {
                        vadj_t.disconnect(id);
                    }
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        }
    }

    /// Get or create the single PERSISTENT annotation card. Bug D / GTK4Rs/AP-117: create it
    /// once and only show/hide it — NEVER unparent/destroy per use (an unrealized popover
    /// leaves a stale tooltip timer pointing at a NULL surface → `GDK_IS_SURFACE`
    /// criticals; `GtkPopover` never calls `gtk_tooltip_unset_surface`). Reuse is also what
    /// upstream does — GtkSourceView's assistants are parented once and hidden/shown
    /// repeatedly — so this is the intended lifecycle, not a workaround.
    /// `dispose()` tears the persistent instance down.
    ///
    /// Everything the popover used to be configured with at this call site — non-autohide,
    /// the parent, the position, the Escape controller, the `closed` bookkeeping — now
    /// lives in [`AnnotationCard::new`], along with the anchoring and dismissal this view
    /// used to have to remember to drive. See [`crate::codeview::card`].
    fn marker_card(&self) -> AnnotationCard {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_card
            .borrow_mut()
            .get_or_insert_with(|| AnnotationCard::new(self))
            .clone()
    }

    /// Bug B PRE-WARM (call once, from the view's first `map`, while scroll is at 0):
    /// create + `set_parent` + a single `popup()`/`popdown()` of the persistent marker
    /// popover so the FIRST real marker click is never the first realization. That first
    /// `set_parent`+`popup` forces the view's first full validation of a tall anchored
    /// table (vadjustment change + `validate_onscreen` re-anchor = a one-shot scroll that
    /// also shifts the clicked chip's hitbox out from under the grab, so the click both
    /// scrolls AND sometimes fails to present). Doing it at scroll 0 makes the re-anchor a
    /// no-op (first onscreen paragraph is already the top → no visible jump), and every
    /// later click hits a warm, already-validated popover. Researcher-sourced (GTK 4.6.9).
    pub(crate) fn prewarm_marker_popover(&self) {
        use gtk::subclass::prelude::*;
        // Crash guard (ScrAP-152/GTK4Rs/AP-128): pre-warm ends in `popup()`, which
        // realizes the popover surface — fatal on an unrealized view (NULL parent
        // surface). Checked BEFORE the one-shot flag is consumed, so an (unexpected)
        // unrealized call is retried on the next `map`, not silently burned.
        if !self.is_realized() {
            return;
        }
        if self.imp().marker_popover_warmed.replace(true) {
            return; // already warmed
        }
        let card = self.marker_card();
        // NEVER warm over a live card. The pre-warm drives the SAME persistent instance a
        // real annotation session uses, so its `popdown()` would dismiss a card the user
        // already has open — and it runs from a deferred first-`map` idle, so "the user
        // cannot have opened one yet" is an assumption about idle ordering, not a fact.
        // It is not one: a programmatic navigation (`win.next-annotation`, or the
        // annotations viewer) can open a card in the same turn the view first maps, and
        // then this fires and closes it. Caught by the card's own regression test, where
        // the pre-warm idle landed after the open and silently ended the session.
        //
        // Returning AFTER consuming the one-shot flag is deliberate: a card is open, which
        // means the popover has already been realized and popped up — the first-realization
        // validation this exists to absorb has therefore already happened, and there is
        // nothing left to warm.
        if card.is_open() {
            return;
        }
        // A minimal, on-viewport pointing rect + empty child; popup then immediately
        // popdown so it never actually paints a visible frame, but the present→size_allocate
        // →validate cycle (the scroll-causing part) runs now, at scroll 0.
        //
        // Driven through the RAW popover API rather than `present()`/`dismiss()` on
        // purpose: this is not a presentation, it is a one-shot realization, and the card
        // has no annotation to anchor to yet. Going through `present()` would arm the
        // scroll and outside-click wiring for a session that does not exist, and mark the
        // card open. The `show` vfunc still runs and still asks for an anchor — it finds
        // none (`anchor_src` is `None`) and simply leaves `pointing_to` as set here, which
        // is exactly the intended no-op.
        let popover: gtk::Popover = card.upcast();
        popover.set_pointing_to(Some(&gdk::Rectangle::new(0, 0, 1, 1)));
        popover.popup();
        popover.popdown();
    }
}

#[cfg(test)]
mod next_marker_tests {
    use super::next_marker_index;

    #[test]
    fn no_markers_yields_no_target() {
        // The one case `open_next_marker_popover` reports as `false`: a document with
        // no annotations. Every other input must produce a target, because the action
        // is deliberately always-enabled (a greyed-out Next Annotation would be
        // indistinguishable from the feature being broken).
        assert_eq!(next_marker_index(&[], 0), None);
        assert_eq!(next_marker_index(&[], 999), None);
    }

    #[test]
    fn picks_the_nearest_anchor_after_the_caret() {
        let anchors = [10, 40, 90];
        assert_eq!(next_marker_index(&anchors, 0), Some(0));
        assert_eq!(next_marker_index(&anchors, 11), Some(1));
        assert_eq!(next_marker_index(&anchors, 40), Some(2));
    }

    #[test]
    fn a_caret_exactly_on_a_marker_advances_past_it() {
        // Strictly-after. Activating the action with the caret already at an
        // annotation must move ON, not re-open the same popover forever — the
        // difference between a working "next" and one that appears frozen.
        assert_eq!(next_marker_index(&[10, 40, 90], 10), Some(1));
    }

    #[test]
    fn past_the_last_marker_wraps_to_the_lowest_anchor() {
        // Wrap targets the smallest ANCHOR, not index 0 — distinct only when `markers`
        // is not in anchor order, which is why the next test exists.
        assert_eq!(next_marker_index(&[10, 40, 90], 90), Some(0));
        assert_eq!(next_marker_index(&[10, 40, 90], 10_000), Some(0));
    }

    #[test]
    fn anchor_order_wins_over_vec_order() {
        // `markers` happens to arrive in document order today; this pins the contract
        // that `next_marker_index` does not rely on it. Anchors descend by index here,
        // so any implementation that walked the Vec would answer 0/2 instead of 2/1.
        let anchors = [90, 40, 10];
        assert_eq!(
            next_marker_index(&anchors, 0),
            Some(2),
            "nearest after 0 is 10"
        );
        assert_eq!(next_marker_index(&anchors, 10), Some(1), "then 40");
        assert_eq!(next_marker_index(&anchors, 40), Some(0), "then 90");
        assert_eq!(next_marker_index(&anchors, 90), Some(2), "wrap back to 10");
    }

    #[test]
    fn co_located_markers_resolve_to_the_lowest_index() {
        // Several annotations sharing one anchor collapse into a single grouped
        // hit-box (`group_by_line`); the popover lists the whole group, so which
        // member is named as the target only has to be stable and in document order.
        assert_eq!(next_marker_index(&[10, 40, 40, 90], 10), Some(1));
        assert_eq!(next_marker_index(&[40, 40, 40], 90), Some(0), "wrap, tied");
    }

    #[test]
    fn a_lone_marker_cycles_back_to_itself() {
        // Degenerate but reachable: one annotation, caret past it. Wrapping to itself
        // is correct — it re-opens the only comment there is.
        assert_eq!(next_marker_index(&[10], 0), Some(0));
        assert_eq!(next_marker_index(&[10], 10), Some(0));
        assert_eq!(next_marker_index(&[10], 50), Some(0));
    }

    #[test]
    fn a_marker_at_offset_zero_is_reachable_by_wrapping() {
        // An annotation on the document's very first character can never be the
        // "strictly after" answer for any caret >= 0, so the wrap is its ONLY route.
        // Without the wrap branch this marker would be permanently unreachable by
        // keyboard — precisely the gap this action exists to close.
        assert_eq!(next_marker_index(&[0, 40], 0), Some(1));
        assert_eq!(next_marker_index(&[0, 40], 40), Some(0));
    }
}

#[cfg(test)]
mod marker_tests {
    use super::group_by_line;

    #[test]
    fn distinct_lines_stay_separate_in_order() {
        assert_eq!(
            group_by_line(&[10, 30, 50]),
            vec![(10, vec![0]), (30, vec![1]), (50, vec![2])]
        );
    }

    #[test]
    fn same_line_annotations_collapse_into_one_group() {
        // Three markers on y=30 and one on y=10, interleaved: the y=30 group holds
        // all three (in first-seen order), y=10 holds one.
        assert_eq!(
            group_by_line(&[30, 10, 30, 30]),
            vec![(30, vec![0, 2, 3]), (10, vec![1])]
        );
    }

    #[test]
    fn empty_input_yields_no_groups() {
        assert!(group_by_line(&[]).is_empty());
    }
}

/// Live-display tests for the annotation **accessibility** path
/// (`open_next_marker_popover`). Excluded from the default `cargo test` (they need a
/// real display and a real paint); run with `cargo test --features
/// gtk-integration-tests`, under Xvfb if headless.
///
/// These exist because the interesting half of that method is unreachable from a pure
/// test: `marker_hitboxes` is repopulated only by `snapshot_layer`, so nothing about
/// the scroll→await-paint→open sequence can be exercised without an actual frame.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod a11y_integration_tests {
    use super::*;
    use crate::codeview::CodePreviewView;

    /// Two annotations, far enough apart that the second starts off-screen in a short
    /// window — so the scroll-then-await-paint branch (not just the fast path) runs.
    const MD: &str = "Intro paragraph here.\n\n\
        The {==first claim==}{>>first note<<} sits near the top.\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        The {==second claim==}{>>second note<<} sits far below.\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\n";

    fn view_of(pane: gtk::Widget) -> CodePreviewView {
        pane.downcast::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("overlay wraps the scroller")
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("scroller holds the CodePreviewView")
    }

    /// Pump the main loop until `done` reports true, or `budget` iterations elapse.
    /// Returns whether it converged. A paint is frame-clock driven, so this cannot be
    /// a single drain of pending events.
    fn pump_until(budget: u32, done: impl Fn() -> bool) -> bool {
        let ctx = glib::MainContext::default();
        for _ in 0..budget {
            if done() {
                return true;
            }
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        done()
    }

    fn hitbox_count(view: &CodePreviewView) -> usize {
        use gtk::subclass::prelude::*;
        view.imp().marker_hitboxes.borrow().len()
    }

    /// Depth-first search for the first descendant of type `T`. Production code must never
    /// re-discover built objects by walking the tree (ScrAP-10 — pass the list forward), but a
    /// test driving the real UI has no forwarded handle by design: walking is how it stays
    /// an honest end-to-end test of what the user's pointer actually reaches.
    fn find_descendant<T: IsA<gtk::Widget>>(root: &impl IsA<gtk::Widget>) -> Option<T> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(t) = c.clone().downcast::<T>() {
                return Some(t);
            }
            if let Some(found) = find_descendant::<T>(&c) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// The first descendant `GtkButton` whose label is `label`.
    fn find_button(root: &impl IsA<gtk::Widget>, label: &str) -> Option<gtk::Button> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(b) = c.clone().downcast::<gtk::Button>() {
                if b.label().as_deref() == Some(label) {
                    return Some(b);
                }
            }
            if let Some(found) = find_button(&c, label) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// **End to end, on the surface that was actually broken.**
    ///
    /// Drives the real marker-chip popover: open it, click Edit, type, press Enter — and
    /// assert the annotation was committed. Before the shared `CommentEntry`, this surface
    /// wired its commit only to `save.connect_clicked`, so Enter did nothing here while
    /// working in both Create cards.
    ///
    /// The `CommentEntry` unit tests prove the wiring; this proves the surface is really
    /// wired *through* it. That distinction is the whole lesson — the Enter-commit bug
    /// survived because the obvious test clicked Save, and clicking Save passed throughout.
    #[gtktest::test]
    fn enter_commits_in_the_marker_edit_popover() {
        use gtk::subclass::prelude::*;
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );

        // Edit/Remove controls only appear when a mutation sink is installed.
        let edits: std::rc::Rc<std::cell::RefCell<Vec<AnnotationEdit>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        view.set_annotation_sink({
            let edits = edits.clone();
            std::rc::Rc::new(move |e: AnnotationEdit| edits.borrow_mut().push(e))
        });

        assert!(
            view.open_next_marker_popover(0),
            "the fixture has annotations"
        );
        assert!(
            pump_until(200, || view.has_open_marker_popover()),
            "precondition: the marker popover opened"
        );
        let popover: gtk::Popover = view
            .imp()
            .marker_card
            .borrow()
            .clone()
            .expect("the card exists once opened")
            .upcast();

        let edit = find_button(&popover, "Edit").expect("the popover offers an Edit button");
        edit.emit_clicked();

        let entry = find_descendant::<gtk::Entry>(&popover)
            .expect("clicking Edit swaps the comment label for an entry");
        entry.set_text("amended via Enter");

        // The route under test. NOT `save.emit_clicked()` — that passed all along.
        entry.emit_activate();

        // The sink is deferred to an idle (GTK4Rs/AP-30: never re-render inside the gesture).
        assert!(
            pump_until(200, || !edits.borrow().is_empty()),
            "Enter in the Edit popover's entry must commit the annotation — it did nothing \
             before the shared CommentEntry, while the Save button worked"
        );
        match &edits.borrow()[0] {
            AnnotationEdit::EditComment { text, .. } => {
                assert_eq!(
                    text, "amended via Enter",
                    "Enter must commit the typed text"
                );
            }
            other => panic!("expected an EditComment, got {other:?}"),
        }
        win.destroy();
    }

    /// The precondition every other a11y assertion rests on: a presented window paints
    /// the margin chips, so `marker_hitboxes` is populated. If this fails, no popover
    /// can ever open and the whole keyboard path is dead — which is exactly the
    /// symptom seen when driving the app by hand (no chip ever observed).
    #[gtktest::test]
    fn a_presented_preview_paints_marker_hitboxes() {
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();

        let painted = pump_until(200, || hitbox_count(&view) > 0);
        assert!(
            painted,
            "a presented preview must paint its margin chips; \
             marker_hitboxes is still empty after pumping the loop"
        );
        win.destroy();
    }

    /// The fast path: the target chip is already painted, so the popover opens on the
    /// spot with no scroll. This is the common case (annotate, then read it back).
    #[gtktest::test]
    fn next_annotation_opens_the_popover_for_an_on_screen_marker() {
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );

        assert!(
            view.open_next_marker_popover(0),
            "a document WITH annotations must report a target"
        );
        assert!(
            pump_until(200, || view.has_open_marker_popover()),
            "the first annotation is on screen, so its popover must open"
        );
        win.destroy();
    }

    /// The branch a pure test cannot reach and the pointer path never runs: the target
    /// is BELOW the viewport, so there is no hit-box to open against until a scroll and
    /// a repaint have happened. A single `idle_add_local_once` here resolves against the
    /// PRE-scroll paint and silently finds nothing (observed live: the view scrolled to
    /// the annotation and no popover opened) — hence the bounded tick callback. This
    /// test fails if that regresses to an idle.
    #[gtktest::test]
    fn next_annotation_scrolls_to_an_off_screen_marker_and_opens_it() {
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );

        // Target the LAST annotation from a caret just past the first one. In a 400px
        // window the second claim is well below the fold.
        let anchors: Vec<i32> = {
            use gtk::subclass::prelude::*;
            view.imp()
                .markers
                .borrow()
                .iter()
                .map(|m| m.anchor)
                .collect()
        };
        assert_eq!(anchors.len(), 2, "fixture has exactly two annotations");
        let first = anchors.iter().copied().min().expect("two anchors");

        // The precondition MUST be about the target itself, not "some marker is
        // off-screen": if the target were already painted, `open_next_marker_popover`
        // would take the fast path, the test would pass, and it would prove nothing
        // about the scroll branch it exists to cover.
        let target = next_marker_index(&anchors, first).expect("a second annotation follows");
        assert!(
            view.marker_hitbox_containing(target).is_none(),
            "precondition: the TARGET marker must be off-screen, else this exercises \
             the fast path and the scroll branch stays untested"
        );

        assert!(view.open_next_marker_popover(first));
        assert!(
            pump_until(400, || view.has_open_marker_popover()),
            "the off-screen annotation must be scrolled to and its popover opened              once the paint repopulates the hit-boxes"
        );
        win.destroy();
    }

    /// The **visible give-up**. A navigation must leave the document AT the
    /// target whether or not the popover ever opens. The old code's give-up branch did
    /// nothing at all: the user pressed the accelerator, no popover appeared, and the
    /// view had not moved either — a completely silent no-op. The converge loop's last
    /// aim simply stays applied, so this contract holds on the success path and the
    /// timeout path alike, which is why it can be asserted without forcing a timeout.
    #[gtktest::test]
    fn a_navigation_leaves_the_document_scrolled_to_the_target() {
        use gtk::subclass::prelude::*;
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );

        let anchors: Vec<i32> = view
            .imp()
            .markers
            .borrow()
            .iter()
            .map(|m| m.anchor)
            .collect();
        let first = anchors.iter().copied().min().expect("two anchors");
        let target = next_marker_index(&anchors, first).expect("a second annotation follows");
        assert!(
            view.marker_hitbox_containing(target).is_none(),
            "precondition: the target must be off-screen, else no scroll is needed"
        );

        assert!(view.open_next_marker_popover(first));
        assert!(
            pump_until(400, || view.has_open_marker_popover()),
            "precondition: the navigation completed"
        );

        // Where the target's line actually sits, read AFTER convergence (F4: read before
        // and `line_yrange` hands back a stale/0 y).
        let vadj = view
            .vadjustment()
            .expect("scrollable view has a vadjustment");
        let it = view.buffer().iter_at_offset(anchors[target]);
        let (target_y, _h) = view.line_yrange(&it);
        let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
        // The fixture must leave a full viewport of content BELOW the target, or this
        // test proves nothing: with `target_y > max` the clamp below pins `want` to
        // `max`, and a view scrolled merely to its own end satisfies the assertion —
        // both sides collapse onto the same bound and a "scroll to end" stub passes.
        // That is precisely how this guard sat green here while failing on two other
        // platforms, so assert the fixture's adequacy LOUDLY rather than letting a
        // degenerate one quietly disarm the check — a guard whose observable is
        // constant across the behaviours it claims to discriminate is not weak, it is
        // vacuous, and it reports success under every implementation.
        assert!(
            (target_y as f64) <= max,
            "DEGENERATE FIXTURE, not a product failure: the target line sits at {target_y} \
             but the view can scroll no further than {max} (upper={}, page_size={}). Add \
             trailing content after the last annotation so the target can reach the top \
             of the viewport; until then this assertion cannot discriminate.",
            vadj.upper(),
            vadj.page_size(),
        );
        let want = (target_y as f64).clamp(vadj.lower(), max);
        assert!(
            (vadj.value() - want).abs() < 4.0,
            "the document must be left scrolled to the target: vadj.value()={} want={}",
            vadj.value(),
            want
        );
        win.destroy();
    }

    /// The GTK4Rs/AP-118 re-pin guard must hold the
    /// **post-scroll** position, never a pre-scroll snapshot.
    ///
    /// There is one animation slot per adjustment, and `set_value` beats an in-flight
    /// animation outright (it calls `end_updating` — gtkadjustment.c:532). So a guard
    /// that bracketed the whole navigation, or reused the pre-warm's pre-scroll snapshot,
    /// would re-pin the view straight back to where the navigation started — silently
    /// undoing it while every other assertion still passed. The plan calls this "the
    /// single easiest way to get this wrong".
    ///
    /// This has to be asserted deliberately. The guard only acts on `value-changed`, and
    /// this short fixture has no tall table and so no deferred validation scroll to fire
    /// one — mutation-checked (GTK4Rs/AP-78): seeding the guard with a pre-scroll `0.0` does NOT
    /// fail `a_navigation_leaves_the_document_scrolled_to_the_target`, because there the
    /// guard simply never runs. So nudge the adjustment the way a deferred validation
    /// scroll would, and assert where the guard puts it back.
    #[gtktest::test]
    fn the_repin_guard_holds_the_post_scroll_position_not_the_pre_scroll_one() {
        use gtk::subclass::prelude::*;
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );

        let anchors: Vec<i32> = view
            .imp()
            .markers
            .borrow()
            .iter()
            .map(|m| m.anchor)
            .collect();
        let first = anchors.iter().copied().min().expect("two anchors");
        assert!(view.open_next_marker_popover(first));
        assert!(
            pump_until(400, || view.has_open_marker_popover()),
            "precondition: the navigation completed and armed the guard"
        );

        let vadj = view
            .vadjustment()
            .expect("scrollable view has a vadjustment");
        let settled = vadj.value();
        assert!(
            settled > 0.0,
            "precondition: the navigation actually scrolled away from the top, else a \
             pre-scroll snapshot and a post-scroll one are indistinguishable"
        );

        // Exactly what GTK4Rs/AP-118's deferred validation scroll does: re-target the adjustment
        // to the top some frames after `popup()`. The guard runs synchronously inside this
        // emission and must put it straight back.
        crate::saferizer::scrollpos::jump(&vadj, 0.0);
        assert!(
            (vadj.value() - settled).abs() < 4.0,
            "the re-pin guard must restore the POST-scroll position ({settled}), not snap \
             back to a pre-scroll one; landed at {}",
            vadj.value()
        );
        win.destroy();
    }

    /// The bound is a **wall-clock deadline**, and it is enforced. An
    /// already-expired request must be dropped on the next paint even though its chip IS
    /// painted and could be opened: expiry wins over a late hit, so a long-dead
    /// navigation can never throw a surprise popover at a rect the user has scrolled away
    /// from.
    ///
    /// This is also the guard against regressing to a frame count. A past
    /// `glib::monotonic_time()` stamp is expired at *every* refresh rate, so this test
    /// states the contract in the units that matter. Mutation-checked (GTK4Rs/AP-78): reverting
    /// the expiry arm to the old `hit.is_some() || expired` shape — which returns the hit
    /// even when expired — fails this on the popover assertion.
    #[gtktest::test]
    fn an_expired_pending_open_is_discarded_without_opening_a_popover() {
        use gtk::subclass::prelude::*;
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );

        // Marker 0 is on screen and painted, so a hit-box for it exists RIGHT NOW. The
        // only thing that may stop it opening is the expiry — which is exactly what makes
        // this a test of the deadline and not of a missing chip.
        assert!(
            view.marker_hitbox_containing(0).is_some(),
            "precondition: the target's chip is painted, so only expiry can prevent the open"
        );
        view.imp()
            .pending_marker_open
            .replace(Some(PendingMarkerOpen {
                target: 0,
                anchor: 0,
                deadline: Deadline::in_us(-1), // already in the past
            }));
        view.queue_draw();

        // Pump for the popover, NOT for the request to clear. The dispatch is a paint
        // followed by an `idle_add_local_once`, so stopping the moment the request clears
        // would assert before that idle has had a chance to run — the negative would pass
        // vacuously. Pumping the full budget on the popover predicate gives a wrong
        // implementation every opportunity to open it, and only then concludes it didn't.
        let opened = pump_until(200, || view.has_open_marker_popover());
        assert!(
            !opened,
            "an EXPIRED request must not open a popover even though its chip is painted"
        );
        assert!(
            view.imp().pending_marker_open.borrow().is_none(),
            "an expired request must be discarded by the paint, not left armed forever"
        );
        win.destroy();
    }

    /// A document with no annotations reports no target and opens nothing. The action is
    /// deliberately always-enabled, so this is the one activation that must no-op
    /// cleanly rather than pop an empty popover.
    #[gtktest::test]
    fn next_annotation_is_a_clean_no_op_without_annotations() {
        let pane = crate::preview::render("Just prose, no annotations.\n", None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        pump_until(60, || false);

        assert!(
            !view.open_next_marker_popover(0),
            "no annotations => no target"
        );
        assert!(!view.has_open_marker_popover(), "and nothing may be shown");
        win.destroy();
    }

    /// `dispose` must `popdown()` an open marker popover BEFORE `unparent()`ing it.
    ///
    /// HISTORY: the marker popover used to be the app's only autohide popover, so the only
    /// one holding a real X11 seat grab (GTK4Rs/AP-83). `unparent()` unrealizes its surface, and
    /// doing that while the grab was live stranded it — the app stopped accepting clicks and
    /// keys while hover feedback kept working, unusable until restarted. It is now
    /// `set_autohide(false)` like every other popover (to fix GTK4Rs/AP-83 click-misrouting on its
    /// own buttons), so there is NO live seat grab and that specific failure can no longer
    /// occur. The popdown-before-unparent ordering is retained deliberately anyway: it is
    /// correct hygiene, and the `unparent` itself is still mandatory (GTK4Rs/AP-80/112).
    ///
    /// **This test cannot observe a seat grab** — there no longer is one, and Xvfb never had a
    /// real one either (GTK4Rs/AP-83, real-compositor-only). It observes the popover's own
    /// `closed` signal, which fires only on a real popdown, and so still pins the *ordering
    /// contract* that regresses silently under refactoring.
    ///
    /// This is the `dispose` half — the last-resort guarantee covering EVERY teardown
    /// path. The reload-layer popdown is guarded separately in `window::reload`, whose
    /// test deliberately holds a strong ref so `dispose` cannot mask it. Neither test
    /// covers the other's fix.
    #[gtktest::test]
    fn dispose_popdowns_the_marker_popover_before_unparenting_it() {
        use gtk::subclass::prelude::*;
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || hitbox_count(&view) > 0),
            "precondition: chips painted"
        );
        assert!(
            view.open_next_marker_popover(0),
            "precondition: the fixture has annotations"
        );
        assert!(
            pump_until(200, || view.has_open_marker_popover()),
            "precondition: the marker popover is open going into teardown"
        );

        // `closed` fires only on a genuine popdown — it is not emitted by `unparent()`.
        // That makes it the exact discriminator between the fix and the bug.
        let popover: gtk::Popover = view
            .imp()
            .marker_card
            .borrow()
            .clone()
            .expect("the card exists once opened")
            .upcast();
        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        popover.connect_closed({
            let c = closed.clone();
            move |_| c.set(true)
        });

        // Force dispose: detach the pane and drop every strong ref this test holds.
        win.set_child(gtk::Widget::NONE);
        drop(view);
        drop(pane);
        pump_until(200, || closed.get());

        assert!(
            closed.get(),
            "dispose must popdown the marker popover before unparenting it. Historically this \
             prevented a stranded X11 seat grab (an autohide popover unrealized mid-grab left \
             the app dead to clicks/keys, GTK4Rs/AP-83); the popover is non-autohide now so no \
             grab exists, but popdown-before-unparent is retained hygiene and the unparent \
             itself is still mandatory (GTK4Rs/AP-80) and must not become a per-use destroy \
             (GTK4Rs/AP-117)"
        );

        win.destroy();
    }

    /// The `open_marker_popover` choke-point crash guard (ScrAP-152/GTK4Rs/AP-128).
    /// Opening a marker popover ends in `popup()`, which realizes the popover surface;
    /// on an UNREALIZED view that surface's parent is NULL → `GDK_IS_SURFACE` assert →
    /// SIGSEGV. A view that is rendered but NEVER presented is unrealized, so a direct
    /// open here must be a clean no-op — reaching the assertion without aborting IS the
    /// pass (mirrors the ScrAP-152 scroll-idle crash test). Mutation note: remove the
    /// `is_realized()` guard at the top of `open_marker_popover` and this aborts (the
    /// `popup()` fires against a NULL parent surface). This is the choke-point half of
    /// the fix; the deferred `snapshot_layer` dispatch also weak-captures to de-pin a
    /// torn-down view — defense-in-depth, either alone prevents the crash.
    #[gtktest::test]
    fn open_marker_popover_is_a_no_op_on_an_unrealized_view() {
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane);
        assert!(
            !view.is_realized(),
            "precondition: a rendered-but-never-presented view is unrealized"
        );

        // Directly attempt an open, bypassing the deferred dispatch. The guard must
        // return before any `popup()`.
        view.open_marker_popover(&[0]);

        assert!(
            !view.has_open_marker_popover(),
            "the is_realized() guard must skip the open (no popup on an unrealized view)"
        );
    }
}
