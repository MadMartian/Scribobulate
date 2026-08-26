//! `Renderer::end_tag` — the block/inline CLOSE handler: heading slug + newline,
//! blockquote range/per-line tag, list renumber + hanging-indent, code-block flush,
//! the table-cell → link-button/label widget build and table anchoring, and the
//! inline-tag pops.

use super::Renderer;
use crate::links::{slugify, unique_slug};
use crate::widgets::table::{
    cell_markup_label, link_cell_button, ScribTableWidget, LINK_MARKUP_CLOSE,
};
use gtk::prelude::*;
use pulldown_cmark::TagEnd;

impl Renderer {
    pub(super) fn end_tag(&mut self, end: TagEnd) {
        match end {
            TagEnd::Heading(level) => {
                // Compute this heading's GitHub-style anchor slug and record its
                // position so `#slug` links can scroll here.
                let slug = unique_slug(&slugify(&self.heading_text), &mut self.slug_seen);
                self.headings.push((slug, self.heading_start));
                // …and its EXTENT, for the drawn band (TDD 18.25). Recorded BEFORE the
                // terminating newline, so the span covers the heading's content only —
                // the same content-not-separator discipline the blockquote range below
                // follows, and what keeps a band from reaching into the blank line after
                // the heading.
                self.heading_spans.push(crate::renderer::HeadingSpan {
                    span: crate::span::BufferSpan::new(self.heading_start, self.end_offset()),
                    // h5 and h6 share the deepest slot, the same fold `emit.rs` applies
                    // when it picks the heading's tag.
                    level_index: match level {
                        pulldown_cmark::HeadingLevel::H1 => 0,
                        pulldown_cmark::HeadingLevel::H2 => 1,
                        pulldown_cmark::HeadingLevel::H3 => 2,
                        pulldown_cmark::HeadingLevel::H4 => 3,
                        _ => 4,
                    },
                });
                self.newline();
                self.heading = None;
            }
            // Close any inline `<picture>` left open within this paragraph (its tags
            // arrive as separate InlineHtml events; a malformed/unclosed one is
            // flushed here so it can't swallow later content). No-op otherwise.
            TagEnd::Paragraph => self.flush_open_picture(),
            TagEnd::BlockQuote(_) => {
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                if self.blockquote_depth == 0 {
                    if let Some(start) = self.blockquote_start.take() {
                        // Close the top-level blockquote: apply the `blockquote`
                        // margin tag and record the range so the preview view draws
                        // the left accent bar over it in snapshot_layer (proven,
                        // never-flickers code-block pattern — no anchored widget). The
                        // text is already in the buffer with its inline tags, so it is
                        // selectable and its links work.
                        //
                        // Apply the tag to each logical line's CONTENT ONLY, EXCLUDING
                        // the terminating '\n's — NOT one continuous apply over the
                        // whole range. A continuous multi-paragraph tag leaves the
                        // MIDDLE lines with no tag toggle inside them, and
                        // GtkTextView's `one_style_cache` (gtktextlayout.c get_style)
                        // then reuses the previous line's style and DROPS the
                        // left-margin on those lines — a width-dependent layout
                        // artifact (researcher-sourced, GTK 4.6.9; GTK4Rs/AP-72).
                        // Leaving each '\n' untagged breaks the btree's same-tag
                        // coalescing, so every line gets its own on/off toggle and its
                        // margin is rebuilt. (The `\n`s carry no visible glyph, so
                        // excluding them costs nothing.)
                        let end = self.end_offset();
                        if end > start {
                            self.apply_tag_per_line(crate::tags::TagName::Blockquote, start, end);
                            self.blockquote_ranges
                                .push(crate::span::BufferSpan::new(start, end));
                        }
                    }
                }
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Item => {
                self.list_item_open = false;
                if let Some(Some(n)) = self.lists.last_mut() {
                    *n += 1;
                }
                // Apply the uniform per-level content-margin tags over the complete item
                // span, PER LINE (the '\n's left untagged, GTK4Rs/AP-72/GTK4Rs/AP-72). The first logical
                // line gets the inter-item gap; every later logical line (each hard-broken
                // source line — breaks are real newlines again — and any loose continuation
                // paragraph) shares the same absolute left-margin with indent=0. Depth is
                // self.lists.len() at this point (inner list already popped if this item
                // contained a nested list).
                if let Some(start) = self.item_starts.pop() {
                    let end = self.end_offset();
                    // An item that produced NO content — a bullet / number / checkbox with
                    // nothing after it (`- `, `1. `, `- [ ]`) — draws no gutter marker: the
                    // marker renders only when the item has text/content, exactly as an empty
                    // bullet or number shows nothing. Drop the marker this item pushed at
                    // Tag::Item. It is guaranteed to be `list_markers.last()`: an empty item
                    // (end == start) inserted no buffer text, so it can hold no surviving
                    // nested item's marker after its own (a non-empty descendant would have
                    // advanced the offset; an empty descendant already dropped its own marker).
                    // The `first_line` guard keeps the pop honest against that invariant.
                    if end == start {
                        if self
                            .list_markers
                            .last()
                            .is_some_and(|m| m.first_line == start)
                        {
                            self.list_markers.pop();
                        }
                    } else {
                        self.apply_list_item_per_line(self.lists.len(), start, end);
                    }
                }
            }
            TagEnd::CodeBlock => {
                if let Some((lang, text)) = self.code.take() {
                    self.insert_code_block(&lang, &text);
                }
            }
            TagEnd::TableCell => {
                if let Some(ts) = &mut self.table {
                    ts.in_cell = false;
                    // What the delimiter row said about THIS cell's column. Both cell
                    // shapes honour it, so `:---:` centres a pure-link cell exactly as
                    // it centres a text one.
                    let align = crate::mdtable::column_align(&ts.aligns, ts.col);
                    // Table-cell annotation: paint CriticMarkup claim highlights into the cell label.
                    if !self.ann_highlights.is_empty() {
                        ts.cell_markup = Self::finalize_cell_markup(
                            &ts.cell_markup,
                            &ts.cell_plain,
                            &ts.cell_content_evs,
                            &self.cleaned,
                            &self.ann_highlights,
                        );
                    }
                    let cell_widget: gtk::Widget =
                        if let (Some(url), false) = (ts.cell_sole_link.take(), ts.cell_mixed) {
                            // Pure-link cell: GtkLinkButton routes through xdg-open so
                            // browser-router picks the correct browser per its rules.
                            // Built through the cell seam, which also owns how the
                            // caption is installed and how it is read back — a walk
                            // over a table's cells must be able to reach this text
                            // (ScrAP-250).
                            // The cell seam takes the column's alignment and applies
                            // it to the CAPTION, leaving the button at its default
                            // `halign`/`valign` of Fill so its `.cell` border spans the
                            // whole grid slot — see `link_cell_button`. A non-Fill
                            // `halign` here instead shrink-wraps the border to the
                            // caption and breaks the column's rules row to row.
                            let btn = link_cell_button(&url, &ts.cell_plain, align);
                            btn.set_has_frame(false);
                            btn.add_css_class("cell");
                            if ts.in_head {
                                btn.add_css_class("cell-head");
                            }
                            btn.upcast()
                        } else {
                            // Plain or mixed cell: GtkLabel with Pango markup for
                            // bold/italic/links. Built through the cell seam, which owns
                            // the `activate-link` containment gate for BOTH cell shapes —
                            // a mixed cell's `<a href>` is a real link (GTK4Rs/AP-239) and its
                            // default handler would otherwise `gtk_show_uri` the raw href.
                            // Cells WRAP (WordChar breaks even a long token) and
                            // top-align; the custom ScribTableWidget measures and lays
                            // them out (it never re-measures at validation, so they never
                            // re-arm the GTK4Rs/AP-23 blank).
                            let label = cell_markup_label(&ts.cell_markup);
                            label.set_xalign(align.xalign());
                            // Fill (not Start) so the cell's CSS border spans the FULL
                            // row height even when a sibling cell wraps taller; yalign 0
                            // keeps this cell's text aligned to the top.
                            label.set_valign(gtk::Align::Fill);
                            label.set_yalign(0.0);
                            label.set_selectable(true);
                            label.set_wrap(true);
                            label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                            label.add_css_class("cell");
                            if ts.in_head {
                                label.add_css_class("cell-head");
                            }
                            label.upcast()
                        };
                    if let Some(row) = ts.rows.last_mut() {
                        row.push(cell_widget);
                    }
                    ts.cell_markup.clear();
                    ts.cell_plain.clear();
                    ts.cell_content_evs.clear();
                    ts.cell_off = 0;
                    ts.col += 1;
                }
            }
            TagEnd::TableHead => {}
            TagEnd::TableRow => {}
            TagEnd::Table => {
                if let Some(ts) = self.table.take() {
                    // Build the custom churn-free table widget from the collected rows
                    // and anchor it. It holds the real selectable label/link cells but,
                    // unlike a GtkGrid, reports a CACHED size independent of GTK's
                    // validation `for_size` and never re-measures its cells at a steady
                    // width — so the line-validator's re-measure on a far scroll can
                    // never re-arm the "snapshot without a current allocation" blank
                    // (GTK4Rs/AP-23, researcher-verified contract). Its bound width is set to
                    // the live viewport column by CodePreviewView (registered in
                    // `tables`); until then it is 0×0 and grows on the first allocation.
                    let table = ScribTableWidget::new(ts.rows);
                    // A table nested in a list item / blockquote starts at the enclosing
                    // block's indented content margin, not the view's content edge; record
                    // that inset so the view fits it into `content − inset` and it never
                    // goes over-wide → no spurious Automatic h-scrollbar (GTK4Rs/AP-23a).
                    // Zero for a top-level table.
                    table.set_bound_inset(self.block_inset());
                    let mut iter = self.buf.end_iter();
                    let anchor = self.buf.create_child_anchor(&mut iter);
                    self.tables.push(table.clone());
                    self.anchored.push((anchor, table.upcast()));
                    // Anchor char is not a newline; reset so block_sep inserts \n\n after.
                    self.trailing_newlines = 0;
                    self.at_start = false;
                }
            }
            TagEnd::Strong => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str(super::BOLD_CLOSE);
                    }
                } else {
                    self.inline_tags
                        .retain(|&t| t != crate::tags::TagName::Bold);
                }
            }
            TagEnd::Emphasis => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str("</i>");
                    }
                } else {
                    self.inline_tags
                        .retain(|&t| t != crate::tags::TagName::Italic);
                }
            }
            TagEnd::Strikethrough => {
                if self.in_table_cell() {
                    let (_open, close) = super::strike_tags();
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str(close);
                    }
                } else {
                    self.inline_tags
                        .retain(|&t| t != crate::tags::TagName::Strike);
                }
            }
            TagEnd::Superscript => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str(super::SUPERSCRIPT_CLOSE);
                    }
                } else {
                    self.inline_tags
                        .retain(|&t| t != crate::tags::TagName::Superscript);
                }
            }
            TagEnd::Subscript => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str(super::SUBSCRIPT_CLOSE);
                    }
                } else {
                    self.inline_tags
                        .retain(|&t| t != crate::tags::TagName::Subscript);
                }
            }
            TagEnd::Link => {
                if self.in_table_cell() {
                    if let Some(ts) = &mut self.table {
                        ts.cell_markup.push_str(LINK_MARKUP_CLOSE);
                        if let Some(url) = ts.in_link.take() {
                            // Promote to sole link only if no other content has appeared.
                            if !ts.cell_mixed && ts.cell_sole_link.is_none() {
                                ts.cell_sole_link = Some(url);
                            }
                        }
                    }
                } else {
                    if let Some((start_off, url)) = self.link_start.take() {
                        self.links.push((start_off, self.end_offset(), url));
                    }
                    self.inline_tags
                        .retain(|&t| t != crate::tags::TagName::Link);
                }
            }
            TagEnd::Image => {
                // End of an image: stop suppressing text (the alt text, if any, is done).
                self.suppress_image_alt = false;
            }
            // The raw HTML block is complete: feed its accumulated body (block HTML
            // arrives line-by-line in `html_acc`, events.rs) through the scanner,
            // rendering any `<picture>`/`<img>` images, else dropped (sanitize by
            // omission). Flush a `<picture>` left open by a malformed/unclosed block.
            // See ScrAP-147 / TDD 2.23.
            TagEnd::HtmlBlock if self.in_html_block => {
                self.in_html_block = false;
                let html = std::mem::take(&mut self.html_acc);
                self.feed_html(&html);
                self.flush_open_picture();
            }
            _ => {}
        }
    }
}

/// The one hop in the link-containment gate that static review could not settle.
///
/// A pure-link table cell is rendered as a `GtkLinkButton` (built through
/// [`link_cell_button`]) from a **raw
/// document href** (`end_tag`, `TagEnd::TableCell`). Its `activate-link` handler
/// routes through [`open_url`], which refuses anything outside `http`/`https`/
/// `mailto` — so `file:///etc/passwd` in a table cell is refused *there*. But
/// `GtkLinkButton`'s own **default handler** calls `gtk_show_uri` with the raw
/// URI, gate and all bypassed. Whether that default handler runs is decided
/// entirely by our handler returning [`glib::Propagation::Stop`].
///
/// A code reviewer cannot settle that from this repository: it is a claim about
/// GObject emission order and GTK's signal flags, not about our code. These two
/// tests settle it by measurement, using the fact that the default handler is
/// also the only thing that sets `:visited` — so `visited` is a *proxy for the
/// default handler having run*, and by extension for `gtk_show_uri` having been
/// reached. The control test is what makes the proxy trustworthy: it proves
/// `visited` DOES flip when emission is allowed to continue, so a `false` in the
/// guard means "halted", not "this oracle never fires".
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::LinkButton;

    /// A string that is deliberately **not a valid URI**, so that when the default
    /// handler DOES run it is refused inside GTK and no launch ever reaches the
    /// platform. The control test below lets the default handler run on purpose,
    /// and must not have side effects off this process.
    ///
    /// The refusal is an explicit early return, not a hope about what an unhandled
    /// scheme does: `gtk_link_button_activate_link` hands the URI to
    /// `gtk_uri_launcher_launch`, whose second statement is
    ///
    /// ```text
    /// if (!g_uri_is_valid (self->uri, G_URI_FLAGS_NONE, &error))
    ///   { g_task_return_new_error (...); g_object_unref (task); return; }
    /// ```
    ///
    /// (MEASURED, gtk-4.22.4 `gtk/gtkurilauncher.c:333`) — it returns before
    /// `gtk_show_uri_full`, so nothing is handed to the desktop, the shell or
    /// LaunchServices. `gtk_link_button_set_visited (…, TRUE)` runs unconditionally
    /// afterwards, so the oracle this pair depends on is unaffected. This string has
    /// no `:` at all, hence no scheme, which is the first thing `g_uri_is_valid`
    /// rejects.
    ///
    /// # Why not "a scheme nobody has registered"
    ///
    /// That was the previous constant, and its safety claim was **false on
    /// Windows**. The shell answers an *unregistered* scheme by presenting a modal
    /// "You'll need a new app to open this" chooser, hosted by a different process,
    /// so it outlived the test binary, held foreground focus (silently swallowing
    /// keystrokes aimed at any later driven UI run), ignored `WM_CLOSE` and Escape,
    /// and offered OK beside an already-ticked "Always use this app" — one stray
    /// click from writing a scheme handler into `HKCU`. It was invisible from Linux
    /// because the failure mode there is a `g_warning` and nothing else.
    ///
    /// The lesson generalises past this constant: "no handler is registered for X"
    /// is a claim about every machine the tests will ever run on, and no test can
    /// hold it. "GTK refuses to launch X" is a claim about GTK, which is in the
    /// repository's own dependency set — and [`the_probe_uri_is_one_gtk_refuses_to_launch`]
    /// asserts it on every platform, every run, so the property cannot rot back into
    /// a comment.
    const INERT_URI: &str = "scribobulate-probe/deliberately-not-a-uri";

    /// The safety property [`INERT_URI`] rests on, asserted rather than asserted-in-prose.
    ///
    /// `gtk_uri_launcher_launch` refuses an invalid URI before it reaches any
    /// platform launcher, so an invalid probe cannot open anything on the machine
    /// running the tests. This is the enforcement mechanism for that claim (POLICY
    /// § Typed GTK seams — "no promotion without its enforcement mechanism"): if a
    /// later edit makes the constant a *real* URI, this fails here instead of on
    /// somebody's desktop.
    #[gtktest::test]
    fn the_probe_uri_is_one_gtk_refuses_to_launch() {
        assert!(
            glib::Uri::is_valid(INERT_URI, glib::UriFlags::NONE).is_err(),
            "INERT_URI must be a URI that `g_uri_is_valid` REJECTS — that refusal, \
             inside gtk_uri_launcher_launch, is the only thing stopping the control \
             test below from handing {INERT_URI} to the desktop/shell of whatever \
             machine is running these tests"
        );
    }

    /// CONTROL — the oracle discriminates.
    ///
    /// With a handler that returns `Proceed`, emission reaches `GtkLinkButton`'s
    /// default handler and `:visited` flips to true. Without this, the guard
    /// below would pass just as happily against a `visited` that never changes.
    #[gtktest::test]
    fn the_default_activate_link_handler_runs_when_emission_is_not_halted() {
        // A BARE button on purpose: the subject here is GTK's own emission behaviour,
        // not our cell — routing through `link_cell_button` would put this test's
        // oracle behind the very seam it exists to justify. Declared in clippy.toml.
        #[allow(clippy::disallowed_methods)]
        let btn = LinkButton::with_label(INERT_URI, "cell");
        assert!(
            !btn.is_visited(),
            "precondition: a fresh button is unvisited"
        );

        btn.connect_activate_link(|_| glib::Propagation::Proceed);
        let _handled: bool = btn.emit_by_name("activate-link", &[]);

        assert!(
            btn.is_visited(),
            "CONTROL FAILED: `visited` did not flip even with emission allowed to \
             continue, so it is not a usable proxy for the default handler running. \
             The guard test below proves nothing until this passes — do not trust it."
        );
    }

    /// GUARD — `Propagation::Stop` halts emission before the default handler.
    ///
    /// This is the property the pure-link table cell's containment depends on.
    /// If it ever fails, `open_url`'s scheme gate is bypassable by putting a
    /// `file://` link alone in a table cell.
    #[gtktest::test]
    fn returning_stop_from_activate_link_prevents_gtks_own_show_uri() {
        // Bare on purpose — see the control test above. Declared in clippy.toml.
        #[allow(clippy::disallowed_methods)]
        let btn = LinkButton::with_label(INERT_URI, "cell");
        assert!(
            !btn.is_visited(),
            "precondition: a fresh button is unvisited"
        );

        // Exactly what `end_tag` wires for a pure-link cell.
        btn.connect_activate_link(|_| glib::Propagation::Stop);
        let handled: bool = btn.emit_by_name("activate-link", &[]);

        assert!(handled, "our handler's Stop is the emission's return value");
        assert!(
            !btn.is_visited(),
            "GtkLinkButton's default handler ran despite our handler returning \
             Stop — it calls gtk_show_uri with the RAW href, so the pure-link \
             table cell bypasses open_url's scheme gate entirely"
        );
    }

    /// CONTROL for the `GtkLabel` shape — the oracle discriminates.
    ///
    /// A mixed cell is a `GtkLabel`, and its `activate-link` has the same bypassable
    /// default handler, but no `:visited` property to watch. The oracle instead is the
    /// **emission's own return value**, which works because `gtk_label_activate_link`
    /// declines outright when the label is not inside a `GtkWindow`
    /// (`gtklabel.c:2087-2088`, an early `return FALSE`) — as it is not here.
    ///
    /// So on a bare label with no handler of ours, emission reaches that default and
    /// returns FALSE. That is what makes the TRUE in the guard below evidence of
    /// anything: the accumulator is `_gtk_boolean_handled_accumulator`
    /// (`gtklabel.c:2274`), which halts emission at the first TRUE, so a TRUE can only
    /// have come from a handler that ran *before* the default and stopped it.
    #[gtktest::test]
    fn a_bare_labels_default_activate_link_handler_declines_outside_a_window() {
        let label = gtk::Label::new(None);
        let handled: bool = label.emit_by_name("activate-link", &[&INERT_URI]);
        assert!(
            !handled,
            "CONTROL FAILED: an unparented GtkLabel's default activate-link handler \
             reported the link as handled, so a `true` from the guard below would say \
             nothing about whose handler produced it. Do not trust that test until \
             this one passes."
        );
    }

    /// GUARD — a mixed cell's `<a href>` cannot reach `gtk_show_uri` either.
    ///
    /// The `GtkLabel` twin of the pure-link cell guard above, and it earns its own
    /// test rather than an argument by analogy: the two shapes are different widgets
    /// with different default handlers, and the cell seam is the only thing making
    /// them agree. Without it, `[x](file:///etc/passwd)` written *beside other text*
    /// in a table cell would bypass `open_url`'s scheme gate that the same link alone
    /// in a cell is stopped by (GTK4Rs/AP-239).
    #[gtktest::test]
    fn a_mixed_cell_labels_link_never_reaches_gtks_own_show_uri() {
        let label = crate::widgets::table::cell_markup_label("cell");
        let handled: bool = label.emit_by_name("activate-link", &[&INERT_URI]);
        assert!(
            handled,
            "the cell seam's handler did not halt emission, so GtkLabel's default \
             handler got the RAW href — inside a window that is gtk_show_uri, and \
             the containment gate is bypassed"
        );
    }
}
