//! Table-cell helpers: collecting the selectable `GtkLabel` cells of the anchored
//! `ScribTableWidget`s, attaching each cell's per-cell copymap as qdata, and
//! enumerating per-cell find targets. Cells are selection islands (ScrAP-10)
//! whose text lives in `GtkLabel` children, not the buffer.

use super::qdata::scrib_render_data;
use crate::codeview::CodePreviewView;
use crate::copymap::CopyTree;
use crate::saferizer::qdata_key::QdataKey;
use crate::widgets::table::{link_cell_caption, ScribTableWidget};
use gtk::prelude::*;
use gtk::{Label, TextChildAnchor};
use std::rc::Rc;

/// Each table cell's own char-precise copymap, stored on its cell `GtkLabel`.
/// `None` for a label that was never tagged (e.g. a link-only cell). This key
/// lives here, not in `preview/qdata.rs`, because the payload is per-cell.
const CELL_COPYMAP: QdataKey<Rc<CopyTree>> = QdataKey::new("scrib-cell-copymap");

/// Collect every selectable `GtkLabel` cell from the anchored table widgets, so the
/// copy action can track per-cell selections (tables are selection islands — ScrAP-10).
/// Cells are direct children of each `ScribTableWidget`.
pub(super) fn collect_cell_labels(anchored: &[(TextChildAnchor, gtk::Widget)]) -> Vec<Label> {
    let mut labels = Vec::new();
    for (_, widget) in anchored {
        let Ok(table) = widget.clone().downcast::<ScribTableWidget>() else {
            continue;
        };
        let mut child = table.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            if let Ok(label) = c.downcast::<Label>() {
                labels.push(label);
            }
            child = next;
        }
    }
    labels
}

/// Attach each table cell's own copymap to its cell label as qdata, so the
/// copy-clipboard handler can resolve an in-cell selection to char-precise
/// Markdown (bold/italic/code/link preserved). `cell_maps` is one tree per
/// `TableCell` in document (row-major) order — the exact order the cell widgets
/// are parented, so the k-th direct child pairs with `cell_maps[k]` (link-only
/// `GtkLinkButton` cells consume an index but carry no label to tag).
pub(super) fn attach_cell_copymaps(
    anchored: &[(TextChildAnchor, gtk::Widget)],
    cell_maps: &[CopyTree],
) {
    let mut k = 0usize;
    for (_, widget) in anchored {
        let Ok(table) = widget.clone().downcast::<ScribTableWidget>() else {
            continue;
        };
        let mut child = table.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            if let Some(tree) = cell_maps.get(k) {
                if let Ok(label) = c.clone().downcast::<Label>() {
                    set_cell_copymap(&label, tree.clone());
                }
            }
            k += 1;
            child = next;
        }
    }
}

/// Store a cell's copymap on its label under the [`CELL_COPYMAP`] key.
fn set_cell_copymap(label: &Label, tree: CopyTree) {
    CELL_COPYMAP.set(label, Rc::new(tree));
}

// Mapping a cell's buffer-space Y (the ZZ/N seam): to place chrome (a marker chip,
// a scroll-nudge target) at a table cell's row, do NOT translate the cell `GtkLabel`
// into the *view* and add the scroll back — before the anchor line validates,
// `gtk_text_view_allocate_children` parks every anchored child at an off-screen
// placeholder (x=-w, y=-h with a correct SIZE but wrong POSITION, gtktextview.c:4442),
// so a cell→view translate reads a poisoned origin and yields a wrong-but-nonzero Y
// (bug C). Instead translate the cell INTO its `ScribTableWidget` ancestor (a local
// subtree transform, immune to the placeholder and to scroll) and add the table-top
// buffer-Y from the cache-free `line_yrange(table_anchor)` — never `get_iter_location`,
// which validates mid-snapshot (ScrAP-22). This is the shared `codeview.rs::cell_row_y_h`
// helper, used by BOTH `CodePreviewView::snapshot_layer` (marker chips) and
// `scroll_to_cell_offset` (find's cell scroll-to-hit).

/// Pair each marker whose claim lives in a table cell with a weak ref to that
/// cell's `GtkLabel` (cell-marker pairing; table cells). `cell_src_spans` is
/// row-major per table-child (same order as [`attach_cell_copymaps`]); a marker
/// matches when its `cleaned_content` overlaps a span. Call AFTER the table
/// widgets are parented (post `install_products_into_view`); the Y refine itself
/// is a cell→table transform done at draw time in `snapshot_layer`.
pub(super) fn attach_cell_marker_widgets(
    markers: &mut [crate::codeview::MarkerData],
    anchored: &[(TextChildAnchor, gtk::Widget)],
    cell_src_spans: &[std::ops::Range<usize>],
) {
    if markers.is_empty() || cell_src_spans.is_empty() {
        return;
    }
    let mut k = 0usize;
    for (anchor, widget) in anchored {
        let Ok(table) = widget.clone().downcast::<ScribTableWidget>() else {
            continue;
        };
        let mut child = table.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            if let Some(span) = cell_src_spans.get(k) {
                if let Ok(label) = c.clone().downcast::<Label>() {
                    if !span.is_empty() {
                        for m in markers.iter_mut() {
                            // Overlap of cleaned claim with this cell's content span.
                            if m.source.cleaned_content.start < span.end
                                && span.start < m.source.cleaned_content.end
                            {
                                m.cell_widget = Some(label.downgrade());
                                // Record THIS table's anchor: snapshot_layer uses its
                                // line_yrange as the table-top base for the cell→table
                                // refine (bug C — `m.anchor` alone is a proxy that lands
                                // above the table).
                                m.cell_table_anchor = Some(anchor.clone());
                            }
                        }
                    }
                }
            }
            k += 1;
            child = next;
        }
    }
}

/// The per-cell copymap stored on a cell `label` by [`attach_cell_copymaps`], if
/// any. `None` for an untagged label.
pub(crate) fn cell_copymap(label: &Label) -> Option<Rc<CopyTree>> {
    CELL_COPYMAP.get(label)
}

/// Pair each rendered table widget with its `GtkTextChildAnchor` (a subset of
/// `anchored` — only the `ScribTableWidget`s). Document order is preserved (the
/// renderer pushes tables in source order). Used to drive find-in-preview's
/// cell-text search (cell text lives in `GtkLabel` children, NOT the buffer, so
/// `forward_search` cannot reach it — ScrAP-36).
pub(super) fn collect_table_anchors(
    anchored: &[(TextChildAnchor, gtk::Widget)],
) -> Vec<(TextChildAnchor, ScribTableWidget)> {
    anchored
        .iter()
        .filter_map(|(anchor, widget)| {
            widget
                .clone()
                .downcast::<ScribTableWidget>()
                .ok()
                .map(|table| (anchor.clone(), table))
        })
        .collect()
}

/// Per-cell find targets for the preview: `(table's buffer char offset, cell
/// label)` for every cell that carries on-screen text, in document order. Resolves
/// each table's anchor offset live (the read-only preview buffer never shifts offsets
/// after a render).
///
/// **Both cell shapes count.** A plain or mixed cell IS a `GtkLabel` child of the
/// table; a cell whose whole content is one link is a `GtkLinkButton` whose caption
/// label sits *inside* it (ScrAP-4), and is reached through the seam that built it
/// ([`link_cell_caption`]). Enumerating only the direct-child labels is what made find
/// silently blind to link captions — text plainly visible on the page, reported as
/// "No matches" (ScrAP-250).
///
/// Both shapes are equally navigable, so the count still equals what find can step to:
/// the highlight is a Pango attribute overlay on the label (a caption label takes one
/// exactly like a cell label), and the scroll resolves the row by transforming the
/// label into its `ScribTableWidget` ancestor, which is an ancestor of a caption label
/// too (ScrAP-109).
pub(crate) fn cell_search_targets(view: &CodePreviewView) -> Vec<(i32, Label)> {
    let Some(rd) = scrib_render_data(view) else {
        return Vec::new();
    };
    let buf = view.buffer();
    let mut out = Vec::new();
    for (anchor, table) in &rd.borrow().table_anchors {
        let off = buf.iter_at_child_anchor(anchor).offset();
        let mut child = table.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            if let Ok(label) = c.clone().downcast::<Label>() {
                out.push((off, label));
            } else if let Some(caption) = link_cell_caption(&c) {
                out.push((off, caption));
            }
            child = next;
        }
    }
    out
}
