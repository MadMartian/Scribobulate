//! Per-render state ([`RenderData`]) and the typed GLib-qdata accessors that
//! store/read it (plus the table-label and anchor-widget lists) on the preview view.
//!
//! `render()` stores 3 `Rc<RefCell<_>>` values as GLib qdata on the preview's
//! `CodePreviewView` (a `TextView` subclass) so later closures — and other
//! modules reading through the plain `TextView`/`gtk::Widget` supertype — can
//! get at live, in-place-mutable render state without a widget subclass field.
//! Every read/write MUST use the exact same concrete type at a given key or it
//! is instant UB. Each key is a [`QdataKey<T>`] const that binds the name to its
//! one concrete type, so the accessors below carry no `unsafe` and no turbofish
//! — the mismatch is unrepresentable, not merely documented.

use crate::saferizer::qdata_key::QdataKey;
use crate::widgets::table::ScribTableWidget;
use gtk::prelude::*;
use gtk::{Label, TextChildAnchor};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Per-render data shared by live closures on the preview `TextView`.  Stored
/// as `Rc<RefCell<RenderData>>` qdata under `"scrib-render-data"` so that
/// `re_render` can update it in-place without rewiring any signal handlers.
pub(crate) struct RenderData {
    pub source_map: Vec<(i32, usize)>,
    /// The inverse of `source_map`: `(source_byte_offset, buffer_char_offset)` sorted
    /// by source byte offset, so the split scroll-sync can binary-search
    /// source→buffer (the forward map is sorted by buffer offset, and the source
    /// column is only near-monotonic, so it needs its own sorted index). Built with
    /// [`invert_source_map`](super::invert_source_map) whenever `source_map` is (re)built.
    pub source_map_inv: Vec<(usize, i32)>,
    /// Character-precise copy-as-Markdown tree (TDD 2.8),
    /// consumed by `connect_copy_clipboard`'s spanning-selection branch. A pure
    /// function of the rendered source, rebuilt wholesale by every `re_render`
    /// (never incrementally mutated) so it is always consistent with the buffer.
    pub copymap: crate::copymap::CopyTree,
    pub md_owned: String,
    pub links: Vec<(i32, i32, String)>,
    pub heading_map: HashMap<String, i32>,
    /// Where each heading the SOURCE declares is reachable in this render, in
    /// document order and indexed by `outline::HeadingNode::doc_index` — the outline
    /// sidebar's scroll targets. One entry per source heading, including the ones a
    /// collapsed disclosure is hiding; see [`crate::outline::HeadingSite`] for why
    /// the rendered list cannot be used directly.
    ///
    /// Each entry carries the heading's anchor slug as well. `heading_map` answers
    /// slug→offset; this answers the inverse the Back/Forward history needs, so an
    /// outline activation can record the *slug* it navigated to (a reference that
    /// survives an edit) rather than the positional index it was handed (the weakest
    /// reference there is — Document-Reference CAM).
    pub heading_sites: Vec<crate::outline::HeadingSite>,
    /// Every disclosure this render drew COLLAPSED, in document order. Find reads it
    /// to answer "does a hidden body hold the query?" — the body is in no buffer, so
    /// the question can only be asked of the source (`renderer::CollapsedBlock`).
    pub collapsed_blocks: Vec<crate::renderer::CollapsedBlock>,
    /// Where every disclosure this render DREW sits in the buffer, in document order
    /// — see [`crate::renderer::DisclosureExtent`].
    ///
    /// Distinct from `collapsed_blocks`, which holds only the folded ones and answers
    /// a question about the SOURCE they withheld. This answers where a block's
    /// rendered content is, which is what a toggle must delete or write into, and it
    /// covers expanded blocks too — the ones a collapse has to find.
    pub disclosure_extents: Vec<crate::renderer::DisclosureExtent>,
    /// Each disclosure summary's buffer LINE and the toggle that sits on it, so a
    /// click anywhere along that line reaches the control. The arrow is ~16px and is
    /// meant to be — it reads as an indicator in prose — but that makes it a poor
    /// thing to aim at, which is why the hit target is the line and not the glyph.
    pub disclosure_lines: Vec<(i32, gtk::ToggleButton)>,
    /// (anchor, tint widget) for every rendered image: the click-through overlay box
    /// shown when the image is inside the buffer selection (`connect_image_tints`).
    pub image_tints: Vec<(TextChildAnchor, gtk::Widget)>,
    /// (anchor, table widget) for every rendered table, document order — lets
    /// find-in-preview search the cell `GtkLabel`s (whose text is not in the
    /// buffer) and order cell matches against body-text matches (ScrAP-36).
    pub table_anchors: Vec<(TextChildAnchor, ScribTableWidget)>,
    /// Cleaned→original byte-offset shift table (CriticMarkup extraction) so the
    /// annotation create overlay can map a preview selection back to the editor's
    /// original source (the CriticMarkup shift table). Identity `[(0,0)]` when un-annotated.
    pub shifts: Vec<(usize, usize)>,
    /// The **original** (pre-extraction) source the editor holds, for the
    /// scroll-sync char↔byte conversions across the shift table (Fork 2-B). Equals
    /// `md_owned` when the document has no CriticMarkup.
    pub original_owned: String,
}

impl RenderData {
    /// The FIRST render's state: a render's maps, plus the two widget-keyed lists
    /// that route owns outright (there is nothing yet to merge them with).
    ///
    /// Exhaustively destructures [`RenderMaps`](crate::preview::build::RenderMaps),
    /// exactly as [`Self::adopt_maps`] does, so a map added there is a compile error
    /// at both — which is the whole point of the type. `disclosure_lines` starts
    /// empty: it is filled when the toggles are wired, after the view exists.
    pub(super) fn new(
        maps: crate::preview::build::RenderMaps,
        image_tints: Vec<(TextChildAnchor, gtk::Widget)>,
        table_anchors: Vec<(TextChildAnchor, ScribTableWidget)>,
    ) -> Self {
        let crate::preview::build::RenderMaps {
            source_map,
            copymap,
            md_owned,
            links,
            heading_sites,
            heading_map,
            collapsed_blocks,
            disclosure_extents,
            shifts,
            original_owned,
        } = maps;
        Self {
            source_map_inv: super::sourcemap::invert_source_map(&source_map),
            source_map,
            copymap,
            md_owned,
            links,
            heading_map,
            heading_sites,
            collapsed_blocks,
            disclosure_extents,
            disclosure_lines: Vec::new(),
            image_tints,
            table_anchors,
            shifts,
            original_owned,
        }
    }

    /// Adopt a render's buffer-keyed maps WHOLESALE — the one way any route installs
    /// them, so a map added to [`RenderMaps`](crate::preview::build::RenderMaps)
    /// reaches every route or none.
    ///
    /// `source_map_inv` is derived here, and only here: it is a pure function of
    /// `source_map`, so no caller can install one without the other or leave the two
    /// describing different renders.
    ///
    /// Deliberately does NOT touch `image_tints`, `table_anchors` or
    /// `disclosure_lines` — those reference live anchor WIDGETS, and each route
    /// answers for them differently (a full render replaces, the splice merges
    /// survivors, the annotation refresh leaves them alone because the buffer was not
    /// swapped).
    pub(super) fn adopt_maps(&mut self, maps: crate::preview::build::RenderMaps) {
        let crate::preview::build::RenderMaps {
            source_map,
            copymap,
            md_owned,
            links,
            heading_sites,
            heading_map,
            collapsed_blocks,
            disclosure_extents,
            shifts,
            original_owned,
        } = maps;
        self.source_map_inv = super::sourcemap::invert_source_map(&source_map);
        self.source_map = source_map;
        self.copymap = copymap;
        self.md_owned = md_owned;
        self.links = links;
        self.heading_sites = heading_sites;
        self.heading_map = heading_map;
        self.collapsed_blocks = collapsed_blocks;
        self.disclosure_extents = disclosure_extents;
        self.shifts = shifts;
        self.original_owned = original_owned;
    }
}

/// The three render-state qdata keys, each binding its name to its one concrete
/// type. `render()` is the sole writer; `re_render()` mutates the pointed-to
/// cells in place rather than re-storing, so a clone taken once by a live
/// closure keeps seeing fresh data across every subsequent re-render.
const RENDER_DATA: QdataKey<Rc<RefCell<RenderData>>> = QdataKey::new("scrib-render-data");
const LABELS: QdataKey<Rc<RefCell<Vec<Label>>>> = QdataKey::new("scrib-labels");
const ANCHOR_WIDGETS: QdataKey<Rc<RefCell<Vec<gtk::Widget>>>> =
    QdataKey::new("scrib-anchor-widgets");

/// The `Rc<RefCell<RenderData>>` `render()` stores. `None` before the first
/// `render()`.
pub(crate) fn scrib_render_data<T: IsA<gtk::glib::Object>>(
    view: &T,
) -> Option<Rc<RefCell<RenderData>>> {
    RENDER_DATA.get(view)
}

/// The `Rc<RefCell<Vec<Label>>>` `render()` stores — the live list of selectable
/// table-cell `GtkLabel`s, rebuilt in place by every `re_render()`. `None`
/// before the first `render()`.
pub(crate) fn scrib_labels<T: IsA<gtk::glib::Object>>(view: &T) -> Option<Rc<RefCell<Vec<Label>>>> {
    LABELS.get(view)
}

/// The `Rc<RefCell<Vec<gtk::Widget>>>` `render()` stores — the anchored children
/// (tables, images) of the current buffer, unparented by `re_render()` just
/// before the buffer's content is cleared. `None` before the first `render()`.
pub(crate) fn scrib_anchor_widgets<T: IsA<gtk::glib::Object>>(
    view: &T,
) -> Option<Rc<RefCell<Vec<gtk::Widget>>>> {
    ANCHOR_WIDGETS.get(view)
}

/// Store the 3 render-state qdata values on `view`. Called exactly once, by
/// `render()`, right after building them; `re_render()` mutates the same cells
/// in place rather than calling this again.
pub(super) fn set_scrib_render_state(
    view: &impl IsA<gtk::glib::Object>,
    render_data: &Rc<RefCell<RenderData>>,
    table_labels: &Rc<RefCell<Vec<Label>>>,
    anchor_widgets: &Rc<RefCell<Vec<gtk::Widget>>>,
) {
    RENDER_DATA.set(view, Rc::clone(render_data));
    LABELS.set(view, Rc::clone(table_labels));
    ANCHOR_WIDGETS.set(view, Rc::clone(anchor_widgets));
}
