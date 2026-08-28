//! The outline sidebar *widget* — a collapsible heading tree.
//!
//! Builds a virtualized `GtkListView` over a `GtkTreeListModel` whose rows are
//! `GtkTreeExpander`s (disclosure chevrons + depth indentation for free), driven
//! by a small `HeadingObject` GObject wrapping each `outline::HeadingNode`.
//! Activating a row calls back with the heading's document-order index and source
//! byte offset; `window.rs` turns that into a scroll of the preview (or editor).
//!
//! This module is GTK-construction code (custom GObject + signal factory), not
//! headlessly testable, so it sits outside the unit-test coverage gate alongside
//! `codeview.rs` — the pure model and tree-folding it consumes live in
//! `outline.rs`, where they are unit-tested.

use crate::outline::HeadingNode;
use crate::span::OriginalByteOffset;
use gtk::prelude::*;
use gtk::{gio, glib, pango};
use std::rc::Rc;

mod imp {
    use super::*;
    use glib::subclass::prelude::*;
    use std::cell::{Cell, RefCell};

    pub(crate) struct HeadingObject {
        pub(crate) level: Cell<u8>,
        pub(crate) doc_index: Cell<usize>,
        pub(crate) src_offset: Cell<OriginalByteOffset>,
        pub(crate) title: RefCell<String>,
        /// Child headings nested under this one (empty for a leaf).
        pub(crate) children: gio::ListStore,
    }

    impl Default for HeadingObject {
        fn default() -> Self {
            Self {
                level: Cell::new(1),
                doc_index: Cell::new(0),
                src_offset: Cell::new(OriginalByteOffset::new(0)),
                title: RefCell::new(String::new()),
                children: gio::ListStore::new::<super::HeadingObject>(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HeadingObject {
        const NAME: &'static str = "ScribHeadingObject";
        type Type = super::HeadingObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for HeadingObject {}
}

glib::wrapper! {
    pub(crate) struct HeadingObject(ObjectSubclass<imp::HeadingObject>);
}

impl HeadingObject {
    /// Recursively wrap a `HeadingNode` (and its subtree) as GObjects.
    fn new(node: &HeadingNode) -> Self {
        use glib::subclass::prelude::*;
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        imp.level.set(node.level);
        imp.doc_index.set(node.doc_index);
        imp.src_offset.set(node.src_offset);
        *imp.title.borrow_mut() = node.text.clone();
        for child in &node.children {
            imp.children.append(&HeadingObject::new(child));
        }
        obj
    }

    fn title(&self) -> String {
        use glib::subclass::prelude::*;
        self.imp().title.borrow().clone()
    }
    fn level(&self) -> u8 {
        use glib::subclass::prelude::*;
        self.imp().level.get()
    }
    pub(crate) fn doc_index(&self) -> usize {
        use glib::subclass::prelude::*;
        self.imp().doc_index.get()
    }
    fn src_offset(&self) -> OriginalByteOffset {
        use glib::subclass::prelude::*;
        self.imp().src_offset.get()
    }
    fn children(&self) -> gio::ListStore {
        use glib::subclass::prelude::*;
        self.imp().children.clone()
    }
}

/// CSS class applied to each row label so headings read hierarchically (H1
/// boldest, deeper levels lighter/smaller). Defined in `preview::css`.
///
/// h6-and-deeper fold onto `outline-h5`, the deepest tier — matching the preview's
/// h6→h5 fold, so no surface distinguishes h6 from h5. See TECH.md § "Heading
/// levels (h1–h5; h6 folds to h5)".
fn level_class(level: u8) -> &'static str {
    const CLASSES: [&str; crate::theme::HEADING_LEVELS] = [
        "outline-h1",
        "outline-h2",
        "outline-h3",
        "outline-h4",
        "outline-h5",
    ];
    CLASSES[crate::theme::heading_slot(level)]
}

/// Expand every heading row of a `GtkTreeListModel` that was built
/// `autoexpand=false` (GTK4Rs/AP-111 — required so a later true-recursive Collapse all is
/// possible). The single, subtle forward walk shared by the build-time full-open
/// (`build_outline_content`) and the runtime "Expand all" button
/// (`window::outline_nav::outline_expand_all`) — kept in one place so the two
/// can't drift (QA M-3), since the walk is load-bearing and must stay identical.
///
/// A `GtkTreeListRow::set_expanded(true)` that actually expands inserts that row's
/// DIRECT children contiguously right after it, synchronously
/// (`gtk_tree_list_row_set_expanded` → `expand_node` → `items_changed(pos+1, 0, n)`,
/// source-confirmed `gtktreelistmodel.c:1216`). Because the model is
/// `autoexpand=false`, each step opens exactly one level; re-reading `n_items()`
/// each step and advancing the index one at a time therefore descends into each
/// freshly-revealed child in turn, reaching full depth one level per step.
/// Already-expanded rows are a cheap no-op. See GTK4Rs/AP-111.
pub(crate) fn expand_all_rows(model: &gtk::TreeListModel) {
    let mut i = 0;
    while i < model.n_items() {
        if let Some(row) = model.item(i).and_downcast::<gtk::TreeListRow>() {
            row.set_expanded(true);
        }
        i += 1;
    }
}

/// Build the outline content widget for `roots`.
///
/// Returns a `GtkListView` (collapsible heading tree) when there are headings,
/// or a muted "No headings" placeholder otherwise — never an error. `on_activate`
/// receives the activated heading's `(doc_index, src_offset)`.
///
/// `initial_selected`, when `Some(doc_index)`, pre-selects the row for that heading
/// *before* the selection-change navigation handler is connected — so restoring the
/// selection across a rebuild does not re-fire a scroll.
pub(crate) fn build_outline_content(
    roots: &[HeadingNode],
    on_activate: Rc<dyn Fn(usize, OriginalByteOffset)>,
    initial_selected: Option<usize>,
) -> gtk::Widget {
    if roots.is_empty() {
        let placeholder = gtk::Label::builder()
            .label("No headings")
            .xalign(0.0)
            .margin_start(12)
            .margin_top(12)
            .valign(gtk::Align::Start)
            .build();
        placeholder.add_css_class("dim-label");
        return placeholder.upcast();
    }

    let root_store = gio::ListStore::new::<HeadingObject>();
    for r in roots {
        root_store.append(&HeadingObject::new(r));
    }

    // passthrough=false ⇒ rows are GtkTreeListRow (required to drive a
    // TreeExpander; get the real data via row.item()).
    //
    // autoexpand=FALSE — deliberately, so that JetBrains-style *true recursive*
    // Collapse all is possible (TDD 12.17, GTK4Rs/AP-111). Under autoexpand=true a
    // programmatic `set_expanded(true)` on any row recursively re-opens its WHOLE
    // subtree synchronously (init_node → expand_node per child → recursion), so
    // "Collapse all, then expand one root to see its children collapsed" is
    // impossible — expanding a root would spring the entire subtree open again.
    // With autoexpand=false, collapsing a node DESTROYS its descendant subtree
    // (collapse_node → clear_node_children frees node->model + the child rbtree,
    // source-confirmed `gtktreelistmodel.c`), and re-expanding recreates only its
    // direct children, collapsed. The cost is that the outline no longer opens
    // fully on build for free — so we run an explicit expand-all pass just below.
    let tree_model = gtk::TreeListModel::new(root_store, false, false, |item| {
        let heading = item.downcast_ref::<HeadingObject>()?;
        let children = heading.children();
        if children.n_items() > 0 {
            Some(children.upcast())
        } else {
            None
        }
    });

    // Default outline is fully open (TDD 12.17). autoexpand=false builds only the
    // root rows, so open every level explicitly here — the same forward walk the
    // runtime "Expand all" button uses, shared as `expand_all_rows` (QA M-3).
    expand_all_rows(&tree_model);

    let selection = gtk::SingleSelection::builder()
        .model(&tree_model)
        .autoselect(false)
        .can_unselect(true)
        .build();

    // Re-select the previously activated heading BEFORE wiring the navigation
    // handler below, so restoring the selection across a rebuild doesn't trigger a
    // spurious scroll. The build-time expand-all pass above materialised a row for
    // every heading in the flattened TreeListModel, so a linear scan finds it by
    // doc_index. (This holds only at build time — after a user Collapse all,
    // collapsed headings have no row; the scroll-spy handles that miss gracefully.)
    if let Some(target) = initial_selected {
        let n = tree_model.n_items();
        for i in 0..n {
            let is_match = tree_model
                .item(i)
                .and_downcast::<gtk::TreeListRow>()
                .and_then(|row| row.item())
                .and_downcast::<HeadingObject>()
                .is_some_and(|h| h.doc_index() == target);
            if is_match {
                selection.set_selected(i);
                break;
            }
        }
    }

    // Navigate on selection change rather than on `activate`: a single click (and
    // arrow-key movement) changes the SingleSelection, so this gives single-click
    // navigation AND keyboard navigation for free — `activate` would require a
    // double-click / Enter. selected_item() is the GtkTreeListRow (passthrough=false);
    // the heading is one hop in via row.item(). Fires with None when a rebuild
    // clears the selection — guarded.
    selection.connect_selected_item_notify(move |sel| {
        let Some(item) = sel.selected_item() else {
            return;
        };
        let Some(row) = item.downcast_ref::<gtk::TreeListRow>() else {
            return;
        };
        if let Some(heading) = row.item().and_downcast::<HeadingObject>() {
            on_activate(heading.doc_index(), heading.src_offset());
        }
    });

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let expander = gtk::TreeExpander::new();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        expander.set_child(Some(&label));
        item.set_child(Some(&expander));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        // GOTCHA: with passthrough=false, item.item() is a GtkTreeListRow, not
        // the HeadingObject — the data is one hop further in (row.item()).
        let Some(row) = item.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() else {
            return;
        };
        let Some(label) = expander.child().and_downcast::<gtk::Label>() else {
            return;
        };
        // set_list_row gives the disclosure triangle + depth indentation for free.
        expander.set_list_row(Some(&row));
        if let Some(heading) = row.item().and_downcast::<HeadingObject>() {
            label.set_text(&heading.title());
            label.set_css_classes(&[level_class(heading.level())]);
        }
    });

    let list_view = gtk::ListView::new(Some(selection), Some(factory));
    list_view.add_css_class("outline");
    list_view.upcast()
}
