//! Insert Link's "Section" button: a heading picker, shaped like the outline sidebar,
//! that writes a same-document link (`#slug`) into the URL field.
//!
//! The rows, folding and filter come from the sidebar's own builders
//! (`outline_view::build_heading_picker`, `outline::filter::filter_outline`), so the
//! picker lists exactly the headings the outline lists, and a filter narrows it by the
//! same rule. The picker adds one thing the sidebar does not list: a row per bare named
//! anchor (`<a id="x"></a>` with no heading of its own). The fragment written is the name
//! the document gave the place, or a heading's canonical anchor
//! (`outline::section_entries`), so the link resolves on the page and on GitHub.

use crate::outline::expansion::{paths_in_document_order, HeadingPath};
use crate::outline::filter::{filter_outline, RowMark};
use crate::outline::{build_tree, Heading, SectionEntries};
use gtk::prelude::*;
use std::rc::Rc;

/// A "Section" picker on one field of an [`input_form`](super::input_form) (the Insert
/// Link URL): a menu button (mnemonic Alt+S) beside that field whose popover lists the
/// document's headings.
pub(in crate::window) struct SectionSpec {
    /// Index of the field whose row gets the Section button, and whose text a pick
    /// replaces.
    pub(in crate::window) field: usize,
    /// The picker's rows and links, read from the text the link is being inserted into.
    pub(in crate::window) entries: SectionEntries,
}

/// The picker's fixed viewport. Fixed rather than natural so filtering, which changes
/// how many rows there are, never asks a shown popover to change size — a shown popover
/// does not grow, and on GTK ≥ 4.14 one whose content outgrows it closes itself
/// (GTK4Rs/AP-86).
const PICKER_WIDTH: i32 = 320;
const PICKER_HEIGHT: i32 = 300;

/// What an empty document's picker says — the outline sidebar's own wording.
const NO_HEADINGS: &str = "No headings";

/// Everything one picker build needs, shared by the initial build and every filter
/// keystroke's rebuild.
struct Picker {
    headings: Vec<Heading>,
    paths: Vec<HeadingPath>,
    fragments: Vec<String>,
    url: gtk::glib::WeakRef<gtk::Entry>,
    popover: gtk::glib::WeakRef<gtk::Popover>,
}

impl Picker {
    /// Replace the URL field's text with the fragment for heading `doc_index`, and close.
    fn pick(&self, doc_index: usize) {
        let Some(fragment) = self.fragments.get(doc_index) else {
            return;
        };
        if let Some(url) = self.url.upgrade() {
            url.set_text(fragment);
        }
        if let Some(popover) = self.popover.upgrade() {
            popover.popdown();
        }
    }

    /// The list for `query` (all headings when `None`), plus the first matching heading —
    /// the one Enter in the filter box picks.
    fn build(self: &Rc<Self>, query: Option<&str>) -> (gtk::Widget, Option<usize>) {
        let me = Rc::downgrade(self);
        let on_pick: Rc<dyn Fn(usize)> = Rc::new(move |doc_index| {
            if let Some(me) = me.upgrade() {
                me.pick(doc_index);
            }
        });
        match query.and_then(crate::sidebarfilter::Query::parse) {
            None => (
                crate::outline_view::build_heading_picker(
                    &build_tree(&self.headings),
                    &self.paths,
                    None,
                    NO_HEADINGS,
                    on_pick,
                ),
                (!self.headings.is_empty()).then_some(0),
            ),
            Some(q) => {
                let f = filter_outline(&self.headings, &q);
                let first = f
                    .marks
                    .iter()
                    .find(|(_, mark)| matches!(mark, RowMark::Match(_)))
                    .map(|(&doc_index, _)| doc_index);
                let list = crate::outline_view::build_heading_picker(
                    &f.roots,
                    &self.paths,
                    Some(&f.marks),
                    crate::sidebarfilter::NO_MATCHING_HEADINGS,
                    on_pick,
                );
                (list, first)
            }
        }
    }
}

/// Build the Section menu button for `url`, the field a pick replaces.
pub(super) fn section_button(spec: SectionSpec, url: &gtk::Entry) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    // A menu button draws its own drop-down arrow beside a label (GTK4Rs/AP-335), so the
    // label carries no ellipsis or glyph of its own.
    button.set_label("_Section");
    button.set_use_underline(true);

    let popover = gtk::Popover::new();
    let filter = crate::widgets::textfield::named_search_entry("Filter headings");
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .width_request(PICKER_WIDTH)
        .height_request(PICKER_HEIGHT)
        .build();
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    vbox.append(&filter);
    vbox.append(&scroller);
    popover.set_child(Some(&vbox));
    button.set_popover(Some(&popover));

    let paths = paths_in_document_order(&build_tree(&spec.entries.headings));
    let picker = Rc::new(Picker {
        fragments: spec.entries.fragments,
        headings: spec.entries.headings,
        paths,
        url: url.downgrade(),
        popover: popover.downgrade(),
    });
    // The heading Enter in the filter box picks: the first match under the current
    // filter, or the first heading when there is none.
    let first = Rc::new(std::cell::Cell::new(None));
    let show = {
        let picker = picker.clone();
        let first = first.clone();
        let scroller = scroller.downgrade();
        move |query: Option<&str>| {
            let (list, top) = picker.build(query);
            first.set(top);
            if let Some(scroller) = scroller.upgrade() {
                scroller.set_child(Some(&list));
            }
        }
    };
    show(None);
    filter.connect_search_changed(move |entry| show(Some(entry.text().as_str())));
    {
        let picker = Rc::downgrade(&picker);
        filter.connect_activate(move |_| {
            if let (Some(picker), Some(doc_index)) = (picker.upgrade(), first.get()) {
                picker.pick(doc_index);
            }
        });
    }
    // A focused search entry swallows Escape as `stop-search` (GTK4Rs/AP-53), so the
    // popover never sees it: clear a filter first, close on the next Escape.
    {
        let popover = popover.downgrade();
        filter.connect_stop_search(move |entry| {
            if !entry.text().is_empty() {
                entry.set_text("");
            } else if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        });
    }
    button
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_tests {
    use crate::window::testkit::test_app;
    use gtk::prelude::*;

    /// Every widget of type `T` in `root`'s subtree, popovers included (a menu button's
    /// popover is its child in the widget tree).
    fn all_of<T: IsA<gtk::Widget>>(root: &gtk::Widget) -> Vec<T> {
        let mut out = Vec::new();
        if let Some(w) = root.downcast_ref::<T>() {
            out.push(w.clone());
        }
        let mut child = root.first_child();
        while let Some(c) = child {
            out.extend(all_of::<T>(&c));
            child = c.next_sibling();
        }
        out
    }

    fn insert_link_dialog(md: &str, app_id: &str) -> gtk::Window {
        let app = test_app(app_id);
        let window = crate::window::new_window(&app, "IT", md, None);
        crate::window::state(&window)
            .expect("tab state")
            .editor_buf
            .set_text(md);
        super::super::insert_link(&window);
        let toplevels = gtk::Window::toplevels();
        (0..toplevels.n_items())
            .filter_map(|i| toplevels.item(i).and_downcast::<gtk::Window>())
            .find(|w| w.title().is_some_and(|t| t == "Insert Link"))
            .expect("the Insert Link dialog is open")
    }

    /// The URL field, which the dialog builds second.
    fn url_field(dialog: &gtk::Window) -> gtk::Entry {
        all_of::<gtk::Entry>(dialog.upcast_ref())
            .into_iter()
            .nth(1)
            .expect("the dialog has a URL field")
    }

    fn picker_list(dialog: &gtk::Window) -> gtk::ListView {
        all_of::<gtk::ListView>(dialog.upcast_ref())
            .into_iter()
            .next()
            .expect("the Section popover holds a heading list")
    }

    /// Picking a heading REPLACES whatever the URL field held with that heading's
    /// `#slug`, numbered as the page numbers it (a repeated title is `-1`).
    #[gtktest::test]
    fn picking_a_section_replaces_the_url_with_its_anchor() {
        let dialog = insert_link_dialog(
            "# Intro\n\n## Notes\n\n## Notes\n",
            "com.extollit.scribobulate.integrationtest.sectionpick",
        );
        let buttons = all_of::<gtk::MenuButton>(dialog.upcast_ref());
        assert_eq!(buttons.len(), 1, "one Section button");
        assert!(
            all_of::<gtk::Button>(dialog.upcast_ref())
                .iter()
                .any(|b| b.label().is_some_and(|l| l == "B_rowse…")),
            "Browse sits beside it, unchanged"
        );
        let url = url_field(&dialog);
        url.set_text("old/path.md");
        // Row 2 is the second "Notes" while the tree is fully open.
        picker_list(&dialog).emit_by_name::<()>("activate", &[&2u32]);
        assert_eq!(url.text(), "#notes-1");
        dialog.destroy();
    }

    /// Typing in the filter narrows the list, and Enter picks the first match.
    #[gtktest::test]
    fn enter_in_the_filter_picks_the_first_match() {
        let dialog = insert_link_dialog(
            "# Intro\n\n## Getting started\n\n## Reference\n",
            "com.extollit.scribobulate.integrationtest.sectionfilter",
        );
        let filter = all_of::<gtk::SearchEntry>(dialog.upcast_ref())
            .into_iter()
            .next()
            .expect("the picker has a filter box");
        assert!(crate::a11y::has_name(&filter), "the filter box is named");
        filter.set_text("refer");
        // `search-changed` is debounced; emit it rather than wait out the delay.
        filter.emit_by_name::<()>("search-changed", &[]);
        filter.emit_by_name::<()>("activate", &[]);
        assert_eq!(url_field(&dialog).text(), "#reference");
        dialog.destroy();
    }
}
