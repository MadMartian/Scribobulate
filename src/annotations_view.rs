//! The annotations sidebar *widget* — a flat, navigable list of the document's
//! comment-bearing annotations.
//!
//! Builds a virtualized `GtkListView` over a flat `GtkListStore` of `AnnotationObject`
//! GObjects (one per `annotations::AnnotationEntry`). Deliberately **flat**, not a
//! `GtkTreeListModel` — annotations have no hierarchy — which sidesteps every
//! autoexpand/collapse-destroys-subtree hazard the outline's tree must guard against
//! (ScrAP-84 does not apply here; do not introduce a tree without re-reading it).
//!
//! Activating a row calls back with the annotation's `src_span` START byte — its
//! **identity**, never a row index (the list is a filtered subsequence of all
//! CriticMarkup constructs, so position ≠ identity).
//! `window` turns that into a scroll + marker-popover open (preview/split) or a
//! caret move (edit).
//!
//! This is GTK-construction code (custom GObject + signal factory), not headlessly
//! testable, so it sits outside the unit-test coverage gate alongside `outline_view.rs`
//! — the pure model it consumes lives in `annotations.rs`, where it is unit-tested.

use crate::annotations::AnnotationEntry;
use crate::span::OriginalByteOffset;
use gtk::prelude::*;
use gtk::{gio, glib, pango};
use std::rc::Rc;

mod imp {
    use super::*;
    use glib::subclass::prelude::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub(crate) struct AnnotationObject {
        /// The annotation's `src_span` START byte — its identity key for navigation
        /// and for selection preservation across a rebuild.
        pub(crate) src_start: Cell<OriginalByteOffset>,
        pub(crate) comment: RefCell<String>,
        /// The highlighted claim snippet (secondary, dimmed); empty for a point comment.
        pub(crate) claim: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AnnotationObject {
        const NAME: &'static str = "ScribAnnotationObject";
        type Type = super::AnnotationObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for AnnotationObject {}
}

glib::wrapper! {
    pub(crate) struct AnnotationObject(ObjectSubclass<imp::AnnotationObject>);
}

impl AnnotationObject {
    fn new(entry: &AnnotationEntry) -> Self {
        use glib::subclass::prelude::*;
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        imp.src_start.set(entry.src_span.start);
        *imp.comment.borrow_mut() = entry.comment.clone();
        *imp.claim.borrow_mut() = entry.claim.clone().unwrap_or_default();
        obj
    }

    pub(crate) fn src_start(&self) -> OriginalByteOffset {
        use glib::subclass::prelude::*;
        self.imp().src_start.get()
    }
    fn comment(&self) -> String {
        use glib::subclass::prelude::*;
        self.imp().comment.borrow().clone()
    }
    fn claim(&self) -> String {
        use glib::subclass::prelude::*;
        self.imp().claim.borrow().clone()
    }
}

/// Build the annotations content widget for `entries`.
///
/// Returns a `GtkListView` (flat, single-selection) when there are annotations, or a
/// muted "No annotations" placeholder otherwise — never an error (TDD 20.3).
///
/// `on_activate` receives the activated annotation's `src_span` START byte (identity); a
/// single click or keyboard move of the `SingleSelection` fires it, navigating to the
/// annotation and opening its comment card.
///
/// `initial_selected`, when `Some(src_start)`, pre-selects the row for that annotation
/// *before* the selection-change handler is connected — so restoring the selection across a
/// rebuild does not re-fire navigation (TDD 20.11).
pub(crate) fn build_annotations_content(
    entries: &[AnnotationEntry],
    on_activate: Rc<dyn Fn(OriginalByteOffset)>,
    initial_selected: Option<OriginalByteOffset>,
) -> gtk::Widget {
    if entries.is_empty() {
        let placeholder = gtk::Label::builder()
            .label("No annotations")
            .xalign(0.0)
            .margin_start(12)
            .margin_top(12)
            .valign(gtk::Align::Start)
            .build();
        placeholder.add_css_class("dim-label");
        return placeholder.upcast();
    }

    let store = gio::ListStore::new::<AnnotationObject>();
    for e in entries {
        store.append(&AnnotationObject::new(e));
    }

    let selection = gtk::SingleSelection::builder()
        .model(&store)
        .autoselect(false)
        .can_unselect(true)
        .build();

    // Re-select the previously activated annotation by IDENTITY (its src_start),
    // BEFORE wiring the navigation handler, so restoring the selection across a
    // rebuild doesn't trigger a spurious navigation. A row's list POSITION is not its
    // identity (the list is a filtered subsequence), so the match is on src_start.
    if let Some(target) = initial_selected {
        let n = store.n_items();
        for i in 0..n {
            let is_match = store
                .item(i)
                .and_downcast::<AnnotationObject>()
                .is_some_and(|a| a.src_start() == target);
            if is_match {
                selection.set_selected(i);
                break;
            }
        }
    }

    // Navigate on selection change (not `activate`): a single click AND arrow-key movement
    // both change the SingleSelection, giving single-click + keyboard navigation for free
    // (TDD 20.5). Fires with None when a rebuild clears the selection — guarded. No
    // scroll-spy writes this selection (Q3), so unlike the outline this handler needs no
    // spy-origin suppression: every emission here is user-driven.
    selection.connect_selected_item_notify(move |sel| {
        let Some(item) = sel.selected_item() else {
            return;
        };
        if let Some(ann) = item.downcast_ref::<AnnotationObject>() {
            on_activate(ann.src_start());
        }
    });

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        row.set_margin_start(6);
        row.set_margin_end(6);
        let comment = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        comment.add_css_class("annotation-comment");
        let claim = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        claim.add_css_class("dim-label");
        claim.add_css_class("annotation-claim");
        row.append(&comment);
        row.append(&claim);
        item.set_child(Some(&row));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let Some(ann) = item.item().and_downcast::<AnnotationObject>() else {
            return;
        };
        let Some(row) = item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let Some(comment) = row.first_child().and_downcast::<gtk::Label>() else {
            return;
        };
        comment.set_text(&ann.comment());
        // The claim label is the second child; a point comment has no claim, so hide it.
        if let Some(claim) = comment.next_sibling().and_downcast::<gtk::Label>() {
            let text = ann.claim();
            claim.set_visible(!text.is_empty());
            // Quote the claim so it reads as "the text this note is about".
            claim.set_text(&if text.is_empty() {
                String::new()
            } else {
                format!("\u{201c}{text}\u{201d}")
            });
        }
    });

    let list_view = gtk::ListView::new(Some(selection), Some(factory));
    list_view.add_css_class("annotations");
    list_view.upcast()
}
