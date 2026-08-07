//! **A link in a table cell** — both widget shapes one renders in, the single
//! activation path they share with every other link in the document, and the one way
//! to read a cell's caption back.
//!
//! A cell whose *entire* content is one link (`| [Handbook](https://example.com) |`)
//! renders as a `GtkLinkButton`; a cell holding a link **and** anything else
//! (`| ☑ [#6378](…) |`) renders as a `GtkLabel` whose markup carries a Pango
//! `<a href>` (ScrAP-4). Both shapes exist because `GtkLinkButton` is a *widget* and
//! cannot be one word inside a wrapping sentence — but a reader cannot tell them
//! apart and must not be able to: same colour, same pointer cursor, same URL tooltip,
//! same activation, same Copy Link Location.
//!
//! Keeping that true is what this module is for. Two hazards make each shape fail
//! silently on its own, and both are sealed here rather than at the call site:
//!
//! * **The caption escapes the walkers.** A `GtkLinkButton` puts its caption in a
//!   label *inside* itself instead of on a direct child of the table. Find-in-preview
//!   enumerates cell text by downcasting each of a table's direct children to
//!   `GtkLabel` (cell text lives in labels, not the buffer — ScrAP-36), so a pure-link
//!   cell matched nothing: the reader could see "Handbook" on the page while the find
//!   bar reported "No matches", and an identical link *beside other text* in the next
//!   cell matched normally. Hence a builder and its matching reader, defined together
//!   (ScrAP-250).
//! * **The containment gate is bypassable.** *Both* shapes ship a default
//!   `activate-link` handler that calls `gtk_show_uri` with the raw href
//!   (`gtk_label_activate_link`, `gtktextview`-independent; `gtk_link_button_activate_link`),
//!   so a handler that forgets to return [`glib::Propagation::Stop`] hands
//!   `file:///etc/passwd` straight to the desktop. Hence one activation function both
//!   connections delegate to, which cannot return anything else (GTK4Rs/AP-239).

use crate::codeview::CodePreviewView;
use gtk::prelude::*;
use gtk::{glib, Label, LinkButton};

/// Build a pure-link cell's button, captioned so that every consumer of a table's
/// cell text can treat its label exactly like a plain cell's.
///
/// The caption is installed as **escaped Pango markup**, not plain text. Every other
/// cell label in a table is a markup label, and find's cell path relies on that
/// uniformly: it forces an anchored child to re-snapshot by toggling a transient
/// no-attr `<span>` wrapper around the label's own markup (GTK4Rs/AP-45/GTK4Rs/AP-92), which
/// on a plain-text label would silently reinterpret the caption AS markup — a caption
/// like `R&D notes` or `<draft>` then fails `pango_parse_markup` and the label renders
/// EMPTY, with no crash and no compile error (ScrAP-163). One `set_markup` of an
/// escaped string is the whole fix, and it renders the identical glyphs.
///
/// Activation is wired **here**, not at the call site: the default handler this
/// overrides bypasses the containment gate, so the one thing that must never be
/// forgotten is the one thing a caller cannot be trusted to remember. Only the
/// `.cell` styling classes stay with the render path that knows the cell's context.
pub(crate) fn link_cell_button(url: &str, caption: &str) -> LinkButton {
    // This function IS the sanctioned route the clippy ban names.
    #[allow(clippy::disallowed_methods)]
    let btn = LinkButton::with_label(url, caption);
    btn.connect_activate_link(|btn| activate_cell_link(btn.upcast_ref(), &btn.uri()));
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

/// The Pango markup that opens an inline link inside a cell's label, and its closing
/// tag [`LINK_MARKUP_CLOSE`]. Everything between them is the link's caption, which the
/// renderer emits exactly as it emits any other cell text.
///
/// **The `title` is escaped twice, and that is not a mistake.** It is what gives a
/// cell link the same hover tooltip a body link gets, and the two escapes undo two
/// different parsers: Pango unescapes the attribute value when it parses this markup,
/// then `gtk_label_query_tooltip` hands the *result* to `gtk_tooltip_set_markup`,
/// which parses it **again** (`gtklabel.c:1757`). A URL is the one string here that
/// routinely contains `&` (`?a=1&b=2`), so a single escape yields a tooltip that fails
/// to parse and silently shows nothing — the ScrAP-163 blank-label failure one layer
/// out. The `href` is escaped once: nothing parses it a second time.
pub(crate) fn link_markup_open(url: &str) -> String {
    let href = glib::markup_escape_text(url);
    let title = glib::markup_escape_text(&href);
    format!("<a href=\"{href}\" title=\"{title}\">")
}

/// Closes [`link_markup_open`].
pub(crate) const LINK_MARKUP_CLOSE: &str = "</a>";

/// Build a table cell's `GtkLabel` from its finished Pango `markup`, with link
/// activation already wired.
///
/// Every cell that is not a pure-link cell comes through here — including cells with
/// no link at all — because "does this cell contain a link?" is a question about the
/// markup string, and answering it at the call site is exactly how one shape of cell
/// ends up with the gate and the other without it. Wiring `activate-link`
/// unconditionally costs one signal connection on a label that will never emit it.
///
/// The caller owns the rest: `.cell` classes, wrapping, selection, alignment — the
/// cell's *context*, which this module has no view of.
pub(crate) fn cell_markup_label(markup: &str) -> Label {
    let label = Label::builder().label(markup).use_markup(true).build();
    label.connect_activate_link(|label, uri| activate_cell_link(label.upcast_ref(), uri));
    label
}

/// Open `url`, clicked on a link inside table cell `cell` — the single activation both
/// cell shapes delegate to, and the reason neither can diverge from a body link.
///
/// It resolves the preview view from the cell's own ancestry rather than taking one as
/// an argument, because a cell is built by the renderer *before* it is anchored: there
/// is no view to capture at construction time, and capturing one later would strand a
/// stale reference across a re-render.
///
/// **Always returns [`glib::Propagation::Stop`], and that return is the security
/// property.** Both `GtkLabel` and `GtkLinkButton` ship a default `activate-link`
/// handler that calls `gtk_show_uri` with the raw href (`gtklabel.c:2081`,
/// `gtklinkbutton.c:483`), gate and all bypassed — so `Proceed` here would hand
/// `file:///etc/passwd` in a table cell straight to the desktop. Pinned by the
/// measured tests in `renderer::end`, which use `:visited` as a proxy for the default
/// handler having run.
fn activate_cell_link(cell: &gtk::Widget, url: &str) -> glib::Propagation {
    match cell
        .ancestor(CodePreviewView::static_type())
        .and_then(|a| a.downcast::<CodePreviewView>().ok())
    {
        Some(view) => crate::preview::activate_link_url(&view, url),
        // A cell not (yet) inside a preview — a unit-test fixture, or a widget already
        // detached by a re-render. There is no document to resolve a relative
        // reference against and no heading map to scroll, so fall back to the external
        // gate, which admits http/https/mailto and refuses everything else.
        None => crate::links::open_url(url),
    }
    glib::Propagation::Stop
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

/// Display-free tests of the cell-link markup. `link_markup_open` builds a string and
/// Pango parses one; neither needs a GDK display, so these run in the default
/// `cargo test` rather than behind `gtk-integration-tests`.
#[cfg(test)]
mod tests {
    use super::{link_markup_open, LINK_MARKUP_CLOSE};
    use gtk::pango;

    /// A URL with an `&` in its query string — `?a=1&b=2` is the shape that breaks a
    /// single-escaped tooltip, and the shape half the URLs in a real document have.
    const AMPED: &str = "https://example.com/s?a=1&b=2";

    /// `<a href>` is **not** Pango markup — it is `GtkLabel` markup, and the
    /// distinction decides where this fragment can be validated.
    ///
    /// `GtkLabel` runs its own `GMarkupParser` over the string first, lifting out the
    /// `a` elements and recording their `href`/`title` (`gtklabel.c:3376`,
    /// `parse_uri_markup` `:3542`), and only hands what remains to
    /// `pango_parse_markup`. So Pango alone rejects this fragment, and asserting
    /// otherwise would be asserting against the wrong parser. That the whole thing
    /// renders is pinned where it can be — on a real label, in the integration tests
    /// below.
    #[test]
    fn cell_link_markup_is_gtk_label_markup_not_bare_pango() {
        let markup = format!("{}caption{LINK_MARKUP_CLOSE}", link_markup_open(AMPED));
        assert!(
            pango::parse_markup(&markup, '\0').is_err(),
            "if Pango has learned to parse `<a href>` on its own, the two-parser \
             reasoning behind the double-escaped title needs re-deriving: {markup}"
        );
    }

    /// The `title` survives **two** parsers, which is why it is escaped twice.
    ///
    /// Pango unescapes the attribute value when it parses the markup above; GTK then
    /// hands that result to `gtk_tooltip_set_markup`, which parses it AGAIN
    /// (`gtklabel.c:1757`). This asserts the intermediate — what Pango yields — is
    /// itself valid markup, and (the mutation the second escape exists to prevent)
    /// that the single-escaped form is NOT: without it a URL with `&` produces a
    /// tooltip that fails to parse and silently shows nothing.
    #[test]
    fn a_cell_links_tooltip_title_survives_both_parsers() {
        let once = gtk::glib::markup_escape_text(AMPED);
        assert!(
            link_markup_open(AMPED).contains(&format!("title=\"{}\"", once.replace('&', "&amp;"))),
            "the title must be escaped twice: {}",
            link_markup_open(AMPED)
        );
        assert!(
            pango::parse_markup(&once, '\0').is_ok(),
            "what Pango hands to gtk_tooltip_set_markup must itself be valid markup"
        );
        assert!(
            pango::parse_markup(AMPED, '\0').is_err(),
            "MUTATION CHECK: the raw URL must NOT be valid markup, or a single escape \
             would pass this file's tooltip contract and the second escape would be \
             dead weight"
        );
    }

    /// The `href` is escaped exactly ONCE — nothing parses it a second time, so a
    /// double escape there would hand `&amp;` to the browser as a literal.
    #[test]
    fn a_cell_links_href_is_escaped_exactly_once() {
        assert!(
            link_markup_open(AMPED).contains("href=\"https://example.com/s?a=1&amp;b=2\""),
            "{}",
            link_markup_open(AMPED)
        );
    }
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
             show literally or fail to parse (GTK4Rs/AP-45/ScrAP-163)"
        );
    }

    /// The reader answers `None` for a cell that is not a pure-link cell, so a walk
    /// cannot mistake a plain cell's label for a caption (or double-count it).
    #[gtktest::test]
    fn a_plain_cell_is_not_read_as_a_link_cell() {
        let label = gtk::Label::new(Some("plain"));
        assert!(link_cell_caption(label.upcast_ref::<gtk::Widget>()).is_none());
    }

    /// A mixed cell's markup is **consumed by `GtkLabel`'s link parser**, which is
    /// what makes the caption a link rather than decorated text.
    ///
    /// The rendered text is the whole oracle, and it discriminates because of what
    /// the unit tests above establish: `<a href>` is not Pango markup. Only GtkLabel's
    /// own `<a>` handler (`gtklabel.c:3376`) can turn this string into
    /// `☑ #6378 tracked` — hand the same string to Pango and the parse FAILS, leaving
    /// the label EMPTY (the ScrAP-163 blank cell). So a non-empty, tag-free text here
    /// means the `a` element was lifted out and recorded as a link.
    ///
    /// No display geometry is involved: the link table is built when the markup is
    /// parsed, not when the label is drawn. The URL carries an `&`, so a title that
    /// only survives one escape shows up here as a parse failure and a blank cell.
    #[gtktest::test]
    fn a_mixed_cells_markup_is_parsed_as_a_link_not_as_text() {
        use super::{cell_markup_label, link_markup_open, LINK_MARKUP_CLOSE};
        const URL: &str = "https://example.com/i?a=1&b=2";
        let label = cell_markup_label(&format!(
            "\u{2611} {}#6378{LINK_MARKUP_CLOSE} tracked",
            link_markup_open(URL)
        ));
        assert_eq!(
            label.text(),
            "☑ #6378 tracked",
            "EMPTY means the markup failed to parse and the cell renders blank \
             (ScrAP-163); text still containing `<a href` means the link markup was \
             never emitted and the caption is inert text — the defect GTK4Rs/AP-239 fixed"
        );
    }
}
