//! The **pure-link table cell** — a cell whose entire content is a single link —
//! and the one way to read its caption back.
//!
//! A cell like `| [Handbook](https://example.com) |` renders as a `GtkLinkButton`
//! rather than a `GtkLabel` with `<a href>` markup, because Pango's link markup has
//! no hover cursor and activates on button-*press* (ScrAP-4). That choice puts the
//! caption **inside** the button instead of on a direct child of the table, which is
//! where the two halves below come from: a builder and its matching reader, defined
//! together so a consumer that walks a table's cells cannot see only the plain ones.
//!
//! That is not hypothetical — it is exactly what happened. Find-in-preview enumerates
//! table cells by downcasting each of the table's direct children to `GtkLabel`
//! (cell text lives in labels, not in the buffer — ScrAP-36), so a pure-link cell
//! matched nothing at all: the reader could see "Handbook" on the page and the find
//! bar reported "No matches", while an identical link *beside other text* in the next
//! cell (a mixed cell, which IS a label) matched normally. See ScrAP-250.

use gtk::prelude::*;
use gtk::{glib, Label, LinkButton};

/// Build a pure-link cell's button, captioned so that every consumer of a table's
/// cell text can treat its label exactly like a plain cell's.
///
/// The caption is installed as **escaped Pango markup**, not plain text. Every other
/// cell label in a table is a markup label, and find's cell path relies on that
/// uniformly: it forces an anchored child to re-snapshot by toggling a transient
/// no-attr `<span>` wrapper around the label's own markup (ScrAP-37/ScrAP-117), which
/// on a plain-text label would silently reinterpret the caption AS markup — a caption
/// like `R&D notes` or `<draft>` then fails `pango_parse_markup` and the label renders
/// EMPTY, with no crash and no compile error (ScrAP-163). One `set_markup` of an
/// escaped string is the whole fix, and it renders the identical glyphs.
///
/// Only the caption is set here. The `.cell` styling classes and the `activate-link`
/// containment gate belong to the render path that knows the cell's context, and stay
/// with it in `renderer::end`.
pub(crate) fn link_cell_button(url: &str, caption: &str) -> LinkButton {
    // This function IS the sanctioned route the clippy ban names.
    #[allow(clippy::disallowed_methods)]
    let btn = LinkButton::with_label(url, caption);
    match link_cell_caption(btn.upcast_ref::<gtk::Widget>()) {
        // `set_markup` sets the text and `use-markup` in one call, so the label is
        // never briefly holding an unescaped string with markup parsing already on.
        Some(label) => label.set_markup(&glib::markup_escape_text(caption)),
        // GtkButton has built its label child from `set_label` since GTK4's first
        // release; if that ever changes, the caption is simply unreachable to find,
        // which is the silent failure this module exists to end — so say so loudly.
        None => log::error!(
            "link cell: GtkLinkButton's caption child is not a GtkLabel, so its text \
             is invisible to find-in-preview and to every other cell-text consumer \
             (url={url})"
        ),
    }
    btn
}

/// The caption label of a pure-link cell, or `None` for any other cell widget.
///
/// The reader half of [`link_cell_button`]: a walk over a table's cells resolves each
/// child either as a plain cell `GtkLabel` or through this, and both answers are a
/// `GtkLabel` carrying the cell's on-screen text with markup semantics.
pub(crate) fn link_cell_caption(cell: &gtk::Widget) -> Option<Label> {
    cell.downcast_ref::<LinkButton>()?
        .child()?
        .downcast::<Label>()
        .ok()
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::{link_cell_button, link_cell_caption};
    use gtk::prelude::*;

    /// The builder and the reader agree: what goes in as a caption comes back out as
    /// the label's plain text, so a cell-text consumer reads the same characters the
    /// reader sees on screen.
    #[gtktest::test]
    fn a_link_cells_caption_is_readable_back_as_plain_text() {
        let btn = link_cell_button("https://example.com/h", "Handbook");
        let label = link_cell_caption(btn.upcast_ref::<gtk::Widget>())
            .expect("a pure-link cell exposes its caption label");
        assert_eq!(label.text(), "Handbook");
    }

    /// A caption containing markup metacharacters survives verbatim — it is escaped
    /// on the way in, so Pango renders the literal `&` and `<…>` (ScrAP-163). A plain
    /// `set_markup` of the raw caption would fail to parse and leave the label EMPTY,
    /// which is what the `text()` assertion below would catch.
    #[gtktest::test]
    fn a_caption_with_markup_metacharacters_renders_literally() {
        let btn = link_cell_button("https://example.com/h", "R&D <draft> notes");
        let label = link_cell_caption(btn.upcast_ref::<gtk::Widget>())
            .expect("a pure-link cell exposes its caption label");
        assert_eq!(
            label.text(),
            "R&D <draft> notes",
            "the caption must render as literal characters, not be parsed as markup"
        );
        assert!(
            label.uses_markup(),
            "the caption label must be a MARKUP label: find's repaint force wraps its \
             own markup in a transient <span>, which a plain-text label would then \
             show literally or fail to parse (ScrAP-37/ScrAP-163)"
        );
    }

    /// The reader answers `None` for a cell that is not a pure-link cell, so a walk
    /// cannot mistake a plain cell's label for a caption (or double-count it).
    #[gtktest::test]
    fn a_plain_cell_is_not_read_as_a_link_cell() {
        let label = gtk::Label::new(Some("plain"));
        assert!(link_cell_caption(label.upcast_ref::<gtk::Widget>()).is_none());
    }
}
