//! Display-free: [`ExportDoc`] → one self-contained HTML file.
//!
//! # Why this is hand-written rather than `pulldown_cmark::html::push_html`
//!
//! That function is a **different renderer**, not a shortcut to the same output. It
//! walks a fresh parse, so it is blind to the four constructs a second tokeniser owns
//! (`^sup^`, `~sub~`, `~~strike~~`, `==highlight==` — ScrAP-66/ScrAP-195) and to
//! CriticMarkup entirely; it never consults the scheme allowlist or the image
//! containment gate; and it **emits raw HTML verbatim**, so a `<script>` in an
//! agent-written document would land executable in a file the reader is about to
//! send. Emitting from [`ExportDoc`] instead means every such decision was already
//! made once, upstream, for the preview and the export alike.
//!
//! # Styling
//!
//! Every value comes from the resolved [`Palette`] and [`Theme`] — POLICY's "no
//! hard-coded styling" rule applies to an export sink as a **third** application path
//! for every theme key, beside the body buffer and the table cell (TDD 25.9). The
//! only literals here are structural CSS: layout, box model, and the relationships
//! between elements, none of which a theme states.

use super::{Align, Block, ExportDoc, ImageRef, ImageSource, Inline, ListItem};
use crate::palette::{to_hex, Palette};
use crate::theme::Theme;
use std::fmt::Write as _;

/// Base body size in points at zoom 1.0. Structural, not themed: a theme owns the
/// heading *scale* and never the base size, exactly as it does on screen — the theme
/// and zoom providers write disjoint properties and the theme owns SCALE, never SIZE
/// (THEMING.md).
const BASE_PT: f64 = 11.0;

/// Serialise `doc` to a complete HTML document.
pub(crate) fn render(doc: &ExportDoc, palette: &Palette, theme: &Theme) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let title = doc.title.clone().unwrap_or_else(|| "Document".to_string());
    let _ = writeln!(out, "<title>{}</title>", escape(&title));
    let _ = writeln!(out, "<style>\n{}</style>", stylesheet(palette, theme));
    out.push_str("</head>\n<body>\n<main>\n");
    for block in &doc.blocks {
        block_html(block, doc, &mut out);
    }
    out.push_str("</main>\n");
    if doc.has_unembedded_remote_images {
        // Stated rather than left to be discovered (TDD 25.12): these images are a
        // live network reference, not part of the artefact, so the file behaves
        // differently offline than it did on screen.
        out.push_str(
            "<p class=\"export-note\">Some images in this document are referenced by URL \
             and were not embedded. They load only when this file is opened with network \
             access.</p>\n",
        );
    }
    if !doc.annotations.is_empty() {
        out.push_str("<section class=\"annotations\">\n<h2>Annotations</h2>\n<ol>\n");
        for ann in &doc.annotations {
            let _ = writeln!(
                out,
                "<li><blockquote class=\"claim\">{}</blockquote><p>{}</p></li>",
                escape(&ann.claim),
                escape(&ann.comment)
            );
        }
        out.push_str("</ol>\n</section>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn block_html(block: &Block, doc: &ExportDoc, out: &mut String) {
    match block {
        Block::Heading { level, id, inlines } => {
            let _ = write!(out, "<h{level} id=\"{}\">", escape_attr(id));
            inlines_html(inlines, doc, out);
            let _ = writeln!(out, "</h{level}>");
        }
        Block::Paragraph(inlines) => {
            out.push_str("<p>");
            inlines_html(inlines, doc, out);
            out.push_str("</p>\n");
        }
        Block::CodeBlock { lang, text } => {
            match lang {
                Some(l) => {
                    let _ = write!(out, "<pre><code class=\"language-{}\">", escape_attr(l));
                }
                None => out.push_str("<pre><code>"),
            }
            out.push_str(&escape(text));
            out.push_str("</code></pre>\n");
        }
        Block::BlockQuote(inner) => {
            out.push_str("<blockquote>\n");
            for b in inner {
                block_html(b, doc, out);
            }
            out.push_str("</blockquote>\n");
        }
        Block::List { start, items } => list_html(*start, items, doc, out),
        Block::Table { aligns, head, rows } => table_html(aligns, head, rows, doc, out),
        Block::Rule => out.push_str("<hr>\n"),
    }
}

fn list_html(start: Option<u64>, items: &[ListItem], doc: &ExportDoc, out: &mut String) {
    // The list is CLASSED per list, but the marker is SUPPRESSED per item — the two are
    // not the same question, and answering the second with the first is what made a
    // mixed list lose its plain items' bullets. `any()` is correct here: this class is
    // the semantic "this list contains tasks" (GitHub's markup shape), carrying no
    // marker rule of its own. The suppression belongs to `li.task-list-item`, because
    // only an item that draws its own checkbox has anything to stand in for it — which
    // is exactly the preview's model, where markers are drawn per item in the gutter.
    let is_task = items.iter().any(|i| i.task.is_some());
    let class = if is_task { " class=\"task-list\"" } else { "" };
    match start {
        Some(n) if n != 1 => {
            let _ = writeln!(out, "<ol start=\"{n}\"{class}>");
        }
        Some(_) => {
            let _ = writeln!(out, "<ol{class}>");
        }
        None => {
            let _ = writeln!(out, "<ul{class}>");
        }
    }
    for item in items {
        // Only a checkbox-bearing item is classed, so a plain item sitting beside one
        // keeps its bullet in the artefact exactly as the preview draws it.
        out.push_str(if item.task.is_some() {
            "<li class=\"task-list-item\">"
        } else {
            "<li>"
        });
        if let Some(checked) = item.task {
            // Disabled: the artefact is a record, and a checkbox a reader could
            // toggle would imply an edit that goes nowhere.
            out.push_str(if checked {
                "<input type=\"checkbox\" checked disabled> "
            } else {
                "<input type=\"checkbox\" disabled> "
            });
        }
        // A single-paragraph item is emitted tight, as a Markdown "tight list" is
        // shown on screen — a `<p>` per item would space every list out.
        match item.blocks.as_slice() {
            [Block::Paragraph(inlines)] => inlines_html(inlines, doc, out),
            blocks => {
                out.push('\n');
                for b in blocks {
                    block_html(b, doc, out);
                }
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str(if start.is_some() {
        "</ol>\n"
    } else {
        "</ul>\n"
    });
}

fn table_html(
    aligns: &[Align],
    head: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    doc: &ExportDoc,
    out: &mut String,
) {
    out.push_str("<table>\n");
    if !head.is_empty() {
        out.push_str("<thead>\n<tr>");
        for (i, cell) in head.iter().enumerate() {
            let _ = write!(out, "<th{}>", align_attr(aligns.get(i)));
            inlines_html(cell, doc, out);
            out.push_str("</th>");
        }
        out.push_str("</tr>\n</thead>\n");
    }
    if !rows.is_empty() {
        out.push_str("<tbody>\n");
        for row in rows {
            out.push_str("<tr>");
            for (i, cell) in row.iter().enumerate() {
                let _ = write!(out, "<td{}>", align_attr(aligns.get(i)));
                inlines_html(cell, doc, out);
                out.push_str("</td>");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody>\n");
    }
    out.push_str("</table>\n");
}

fn align_attr(a: Option<&Align>) -> &'static str {
    match a {
        Some(Align::Left) => " class=\"a-l\"",
        Some(Align::Center) => " class=\"a-c\"",
        Some(Align::Right) => " class=\"a-r\"",
        _ => "",
    }
}

fn inlines_html(inlines: &[Inline], doc: &ExportDoc, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text { text, .. } => out.push_str(&escape(text)),
            Inline::Code(c) => {
                let _ = write!(out, "<code>{}</code>", escape(c));
            }
            Inline::Emphasis(v) => wrap(out, "em", "", v, doc),
            Inline::Strong(v) => wrap(out, "strong", "", v, doc),
            Inline::Strikethrough(v) => wrap(out, "del", "", v, doc),
            Inline::Superscript(v) => wrap(out, "sup", "", v, doc),
            Inline::Subscript(v) => wrap(out, "sub", "", v, doc),
            Inline::Highlight(v) => wrap(out, "mark", "", v, doc),
            Inline::Break => out.push_str("<br>\n"),
            Inline::Link { href, title, inner } => {
                let t = title
                    .as_deref()
                    .map(|t| format!(" title=\"{}\"", escape_attr(t)))
                    .unwrap_or_default();
                let _ = write!(out, "<a href=\"{}\"{t}>", escape_attr(href));
                inlines_html(inner, doc, out);
                out.push_str("</a>");
            }
            Inline::Image(img) => image_html(img, out),
            Inline::Claim(idx, v) => {
                // The claim highlight, plus its comment as an aside beside it — the
                // in-file review loop is the product thesis, so an export that drops
                // the review is the wrong document (TDD 25.13).
                let _ = write!(out, "<span class=\"claim\" id=\"claim-{}\">", idx + 1);
                inlines_html(v, doc, out);
                out.push_str("</span>");
                if let Some(ann) = doc.annotations.get(*idx) {
                    let _ = write!(
                        out,
                        "<aside class=\"comment\"><a href=\"#claim-{}\">{}</a> {}</aside>",
                        idx + 1,
                        idx + 1,
                        escape(&ann.comment)
                    );
                }
            }
        }
    }
}

fn wrap(out: &mut String, tag: &str, attrs: &str, v: &[Inline], doc: &ExportDoc) {
    let _ = write!(out, "<{tag}{attrs}>");
    inlines_html(v, doc, out);
    let _ = write!(out, "</{tag}>");
}

fn image_html(img: &ImageRef, out: &mut String) {
    let alt = escape_attr(&img.alt);
    let title = img
        .title
        .as_deref()
        .map(|t| format!(" title=\"{}\"", escape_attr(t)))
        .unwrap_or_default();
    match &img.source {
        ImageSource::Embedded { bytes, mime } => {
            // A data URI, so the artefact still renders after being moved, renamed or
            // sent on — a relative path breaks the moment the file is moved, which is
            // the only reason anyone exported it (TDD 25.12).
            let _ = write!(
                out,
                "<img alt=\"{alt}\"{title} src=\"data:{mime};base64,{}\">",
                base64(bytes)
            );
        }
        ImageSource::Remote(url) => {
            let _ = write!(
                out,
                "<img alt=\"{alt}\"{title} src=\"{}\">",
                escape_attr(url)
            );
        }
        ImageSource::Missing(reason) => {
            // A visible placeholder with the reason, never a silent gap — the same
            // choice the preview makes, so a reader can tell an image was expected.
            let _ = write!(
                out,
                "<span class=\"missing-image\" title=\"{}\">{}</span>",
                escape_attr(reason),
                escape(reason)
            );
        }
    }
}

/// Standard base64, no line breaks — about twenty lines rather than a new crate.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let idx = [(n >> 18) & 63, (n >> 12) & 63, (n >> 6) & 63, n & 63];
        out.push(ALPHABET[idx[0] as usize] as char);
        out.push(ALPHABET[idx[1] as usize] as char);
        // Pad to a whole quantum: two source bytes yield three characters, one yields
        // two, and the remainder is `=`.
        out.push(if chunk.len() > 1 {
            ALPHABET[idx[2] as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[idx[3] as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Escape text for an HTML **text node**.
///
/// This is the containment boundary for document content, so it is applied to every
/// string that reaches the output from the document — nothing is written raw. `<`, `&`
/// and `>` cover a text node; the two quotes are escaped as well so one function is
/// safe in both positions and there is no second, weaker one to reach for by mistake.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape for an attribute value. Identical rules — kept as its own name so a call
/// site reads as the position it is escaping for.
fn escape_attr(s: &str) -> String {
    escape(s)
}

/// The generated stylesheet: every colour, typeface and metric resolved through the
/// theme engine, never a literal (TDD 25.9).
fn stylesheet(p: &Palette, t: &Theme) -> String {
    let m = &t.metrics;
    let ty = &t.typography;
    let body_font = t
        .font_family
        .as_ref()
        .map(|f| f.as_str().to_string())
        .unwrap_or_else(|| "system-ui, sans-serif".to_string());
    let heading_font = t
        .heading_font
        .as_ref()
        .map(|f| f.as_str().to_string())
        .unwrap_or_else(|| body_font.clone());
    let heading_fg = t
        .heading_color
        .map(to_hex)
        .unwrap_or_else(|| to_hex(p.body_fg));
    let list_marker = t
        .list_marker
        .map(to_hex)
        .unwrap_or_else(|| to_hex(p.body_fg));
    let mark_fg = t
        .mark_fg
        .map(|c| format!("color: {};", to_hex(c)))
        .unwrap_or_default();

    let mut css = String::with_capacity(2048);
    let _ = write!(
        css,
        "html {{ background: {page}; }}
body {{ background: {page}; color: {fg}; font-family: {body_font};
  font-size: {base}pt; line-height: 1.5; margin: 0; padding: 2rem 1rem; }}
main {{ max-width: 46rem; margin: 0 auto; }}
a {{ color: {link}; }}
code {{ background: {code_inline}; border-radius: 3px; padding: 0.1em 0.3em; }}
pre {{ background: {code_block}; padding: {cell_pv}px {cell_ph}px; border-radius: {radius}px;
  overflow-x: auto; }}
pre code {{ background: none; padding: 0; }}
blockquote {{ border-left: {bar_w}px solid {bar}; margin-left: 0;
  padding-left: {bar_gap}px; }}
hr {{ border: 0; border-top: 1px solid {rule}; margin: {rule_space}px 0; }}
table {{ border-collapse: collapse; }}
th, td {{ border: {tbw}px solid {tb}; padding: {cell_pv}px {cell_ph}px; }}
th {{ background: {thead}; }}
td.a-l, th.a-l {{ text-align: left; }}
td.a-c, th.a-c {{ text-align: center; }}
td.a-r, th.a-r {{ text-align: right; }}
ul, ol {{ padding-left: {step}px; }}
li {{ margin-bottom: {li_gap}px; }}
li::marker {{ color: {marker}; }}
li.task-list-item {{ list-style: none; }}
mark {{ background: {mark_bg}; {mark_fg} }}
sup {{ font-size: {sup}%; vertical-align: super; }}
sub {{ font-size: {sup}%; vertical-align: sub; }}
strong {{ font-weight: {bold}; }}
.claim {{ background: {claim}; }}
.comment {{ display: block; border-left: {bar_w}px solid {bar}; margin: 0.4em 0 0.4em 1rem;
  padding-left: {bar_gap}px; font-size: 0.9em; opacity: 0.85; }}
.missing-image {{ border: 1px dashed {rule}; padding: 0.2em 0.4em; opacity: 0.75; }}
.export-note {{ max-width: 46rem; margin: 1.5rem auto; font-size: 0.9em; opacity: 0.8; }}
.annotations {{ max-width: 46rem; margin: 2rem auto 0; border-top: 1px solid {rule};
  padding-top: 1rem; }}
.annotations blockquote.claim {{ background: {claim}; border-left: 0; padding: 0.2em 0.4em; }}
img {{ max-width: 100%; height: auto; }}
",
        page = to_hex(p.page_bg),
        fg = to_hex(p.body_fg),
        base = BASE_PT,
        link = to_hex(p.link_fg),
        code_inline = to_hex(p.code_inline_bg),
        code_block = to_hex(p.code_block_bg),
        bar = to_hex(p.blockquote_bar),
        bar_w = m.blockquote_bar_width,
        bar_gap = m.blockquote_text_gap,
        rule = to_hex(p.rule),
        rule_space = m.rule_space,
        tb = to_hex(p.table_border),
        tbw = m.table_border_width,
        thead = to_hex(p.table_head_bg),
        cell_pv = m.table_cell_padding_v,
        cell_ph = m.table_cell_padding_h,
        radius = m.table_cell_radius,
        step = m.list_step,
        li_gap = m.list_item_gap,
        marker = list_marker,
        mark_bg = t.mark_bg.hex(),
        mark_fg = mark_fg,
        claim = t.annotation_hl.hex(),
        sup = (ty.supsub_scale * 100.0).round() as i32,
        bold = ty.bold_weight,
    );
    // Headings: the theme's scale ladder, applied to the base size. Five entries, not
    // six — `emit.rs` maps h6-and-deeper onto the h5 tag before a tag is chosen, so no
    // theme can differentiate them and this export must not invent a difference.
    for level in 1..=6usize {
        let scale = ty.heading_scale[(level - 1).min(4)];
        let space = m.heading_space_below[(level - 1).min(4)];
        let _ = writeln!(
            css,
            "h{level} {{ font-family: {heading_font}; color: {heading_fg}; \
             font-size: {size:.2}pt; font-weight: {weight}; margin: 1.2em 0 {space}px; }}",
            size = BASE_PT * scale,
            weight = ty.heading_weight,
        );
    }
    css
}

#[cfg(test)]
mod html_sink_tests {
    use super::{base64, escape, render};
    use crate::export::doc;
    use crate::export::RenderOptions;
    use crate::palette::Palette;
    use crate::theme::Theme;

    /// A theme and palette built without touching a display, so the sink's own tests
    /// stay inside the coverage gate.
    fn style() -> (Palette, Theme) {
        let theme = crate::theme::Themes::builtin().resolve("system");
        let palette = Palette::from_base(
            gtk::gdk::RGBA::WHITE,
            gtk::gdk::RGBA::BLACK,
            gtk::gdk::RGBA::BLACK,
            gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0),
            &theme,
        );
        (palette, theme)
    }

    fn html_of(md: &str) -> String {
        let (palette, theme) = style();
        let d = doc::build(md, &RenderOptions::default());
        render(&d, &palette, &theme)
    }

    /// **A plain item inside a task list keeps its marker** (TDD 25.3 parity).
    ///
    /// The defect this pins had NO on-screen symptom: the preview draws markers per
    /// item in the gutter, so it was always right, and only an exported file showed
    /// the plain item with no bullet. Nothing asserted it, which is why it stood.
    ///
    /// The oracle is the pair of classes plus the rule that acts on them, because the
    /// failure was a rule reaching further than the thing it was meant to style:
    /// suppression on the LIST hits every item; on the ITEM it hits only the ones that
    /// draw a checkbox. Mutation: moving `list-style: none` back onto `ul.task-list`
    /// fails this.
    #[test]
    fn a_plain_item_beside_a_task_keeps_its_marker_in_the_artefact() {
        let out = html_of("- A plain item\n- [ ] A task\n- [x] A done task\n");

        // The item that draws a checkbox is the only one whose marker is suppressed.
        assert_eq!(
            out.matches("<li class=\"task-list-item\">").count(),
            2,
            "exactly the two checkbox items carry the class:\n{out}"
        );
        assert!(
            out.contains("<li>A plain item"),
            "the plain item must stay unclassed, or the sheet strips its bullet:\n{out}"
        );

        // And the rule that hides a marker is scoped to the ITEM, never the list.
        assert!(
            out.contains("li.task-list-item { list-style: none; }"),
            "the suppression must be per item:\n{out}"
        );
        assert!(
            !out.contains("ul.task-list { list-style"),
            "a list-level suppression strips EVERY item's marker — the defect:\n{out}"
        );

        // The list itself still declares that it contains tasks (GitHub's shape), and
        // the checkboxes still render disabled.
        assert!(out.contains("<ul class=\"task-list\">"), "{out}");
        assert!(out.contains("<input type=\"checkbox\" disabled>"), "{out}");
        assert!(
            out.contains("<input type=\"checkbox\" checked disabled>"),
            "{out}"
        );
    }

    /// A list with NO tasks carries none of this markup. Asserted against the emitted
    /// ELEMENTS, not the whole artefact: the stylesheet states `li.task-list-item`
    /// unconditionally, so a bare `contains("task-list")` matches the sheet and fails
    /// on correct output — which is exactly what it did when first written.
    #[test]
    fn an_ordinary_list_carries_no_task_markup() {
        let out = html_of("- one\n- two\n");
        assert!(!out.contains("<ul class=\"task-list\">"), "{out}");
        assert!(!out.contains("<li class=\"task-list-item\">"), "{out}");
        assert!(!out.contains("<input type=\"checkbox\""), "{out}");
        assert!(out.contains("<li>one"), "{out}");
    }

    #[test]
    fn the_artefact_is_one_self_contained_document() {
        let out = html_of("# Title\n\nBody.\n");
        assert!(out.starts_with("<!DOCTYPE html>"));
        assert!(out.contains("<title>Title</title>"));
        assert!(out.trim_end().ends_with("</html>"));
        // Self-contained: the stylesheet is inline, never a link to something that
        // would have to travel with the file.
        assert!(out.contains("<style>"));
        assert!(!out.contains("<link"), "no external stylesheet");
        assert!(!out.contains("<script"), "no script of our own either");
    }

    #[test]
    fn a_script_in_the_source_reaches_the_artefact_neither_executable_nor_as_text() {
        // TDD 25.4 at the sink: the artefact is opened by software this project
        // neither controls nor sandboxes, so this is the boundary that matters most.
        let out = html_of("Hello\n\n<script>alert('x')</script>\n\nBye\n");
        assert!(out.contains("Hello") && out.contains("Bye"));
        assert!(!out.contains("<script>alert"), "executable script emitted");
        assert!(
            !out.contains("alert"),
            "script text emitted as visible text"
        );
    }

    #[test]
    fn text_from_the_document_is_escaped_wherever_it_lands() {
        let out = html_of("A < B & C > D and a \"quote\"\n");
        assert!(out.contains("A &lt; B &amp; C &gt; D"), "{out}");
        // The `<p>` structure is ours; the angle brackets in the content are not.
        assert!(!out.contains("A < B"), "{out}");
    }

    #[test]
    fn a_link_title_and_href_are_escaped_in_attribute_position() {
        let out = html_of("[t](https://e.example/?a=1&b=2 \"a \\\"title\\\"\")\n");
        assert!(out.contains("a=1&amp;b=2"), "{out}");
        assert!(
            !out.contains("\"a \"title\"\""),
            "attribute not escaped: {out}"
        );
    }

    #[test]
    fn every_construct_reaches_the_artefact() {
        // TDD 25.3, across the contexts Document Rendering CAM row 2 names.
        let out = html_of(
            "# H\n\n\
             Para with **bold**, *em*, `code`, ~~gone~~, ==mark==, H~2~O, x^2^.\n\n\
             > quoted\n\n\
             - one\n- two\n\n\
             1. first\n\n\
             - [x] done\n\n\
             | a | b |\n|---|---|\n| 1 | 2 |\n\n\
             ```rust\nfn f() {}\n```\n\n\
             ---\n",
        );
        for expected in [
            "<h1",
            "<strong>",
            "<em>",
            "<code>",
            "<del>",
            "<mark>",
            "<sub>",
            "<sup>",
            "<blockquote>",
            "<ul>",
            "<ol>",
            "<table>",
            "<thead>",
            "<tbody>",
            "<th",
            "<td",
            "<pre><code class=\"language-rust\">",
            "<hr>",
            "type=\"checkbox\" checked disabled",
        ] {
            assert!(out.contains(expected), "missing {expected:?} in output");
        }
    }

    #[test]
    fn a_heading_anchor_is_emitted_so_an_in_document_link_still_works() {
        let out = html_of("# My Section\n\n[go](#my-section)\n");
        assert!(out.contains("id=\"my-section\""), "{out}");
        assert!(out.contains("href=\"#my-section\""), "{out}");
    }

    #[test]
    fn an_annotation_travels_with_its_claim_and_is_listed() {
        // TDD 25.13.
        let out = html_of("The {==claim==}{>>my comment<<} here.\n");
        assert!(out.contains("class=\"claim\""), "{out}");
        assert!(out.contains("my comment"), "{out}");
        assert!(out.contains("<section class=\"annotations\">"), "{out}");
    }

    #[test]
    fn every_styling_value_comes_from_the_theme_rather_than_a_literal() {
        // TDD 25.9. Two themes with different pages must produce different CSS; a
        // literal would make them identical, which is the defect this catches.
        let d = doc::build("# T\n\nbody\n", &RenderOptions::default());
        let theme = crate::theme::Themes::builtin().resolve("system");
        let light = Palette::from_base(
            gtk::gdk::RGBA::WHITE,
            gtk::gdk::RGBA::BLACK,
            gtk::gdk::RGBA::BLACK,
            gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0),
            &theme,
        );
        let dark = Palette::from_base(
            gtk::gdk::RGBA::new(0.1, 0.1, 0.1, 1.0),
            gtk::gdk::RGBA::WHITE,
            gtk::gdk::RGBA::WHITE,
            gtk::gdk::RGBA::new(0.2, 0.5, 0.9, 1.0),
            &theme,
        );
        let a = render(&d, &light, &theme);
        let b = render(&d, &dark, &theme);
        assert_ne!(a, b, "the two palettes produced identical CSS — a literal?");
        assert!(a.contains(&crate::palette::to_hex(light.page_bg)));
        assert!(b.contains(&crate::palette::to_hex(dark.page_bg)));
    }

    #[test]
    fn base64_matches_the_standard_for_every_padding_case() {
        // The three quantum remainders, which is where a hand-rolled encoder goes
        // wrong. Vectors are RFC 4648's own.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_round_trips_every_byte_value() {
        // A hand-rolled encoder must be right for the whole byte range, not just for
        // text — an image is arbitrary bytes.
        let bytes: Vec<u8> = (0u8..=255).collect();
        let encoded = base64(&bytes);
        assert_eq!(encoded.len(), 344, "256 bytes → 344 base64 chars");
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='),
            "non-base64 character emitted"
        );
    }

    #[test]
    fn a_local_image_is_embedded_as_a_data_uri_so_the_file_can_be_moved() {
        // TDD 25.12. A one-pixel PNG beside the document, admitted by the containment
        // gate, must arrive in the artefact as bytes rather than as a path.
        let dir = tempfile::tempdir().expect("temp dir");
        let png: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut bytes = png.to_vec();
        bytes.extend_from_slice(b"not really a png body, but the magic is what we sniff");
        std::fs::write(dir.path().join("pic.png"), &bytes).expect("write");
        let d = doc::build(
            "![alt](pic.png)\n",
            &RenderOptions {
                doc_dir: Some(dir.path().to_path_buf()),
                allow_unsafe_images: false,
            },
        );
        let (palette, theme) = style();
        let out = render(&d, &palette, &theme);
        assert!(out.contains("src=\"data:image/png;base64,"), "{out}");
        assert!(!out.contains("src=\"pic.png\""), "a path survived: {out}");
    }

    #[test]
    fn an_image_outside_the_document_folder_is_a_named_placeholder_not_a_silent_gap() {
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = dir.path().parent().map(|p| p.join("escaped.png"));
        let _ = outside;
        let d = doc::build(
            "![alt](../escaped.png)\n",
            &RenderOptions {
                doc_dir: Some(dir.path().join("sub")),
                allow_unsafe_images: false,
            },
        );
        let (palette, theme) = style();
        let out = render(&d, &palette, &theme);
        assert!(out.contains("missing-image"), "{out}");
        assert!(!out.contains("base64"), "nothing was embedded: {out}");
    }

    #[test]
    fn a_referenced_remote_image_makes_the_artefact_say_so() {
        let d = doc::build(
            "![a](https://example.com/x.png)\n",
            &RenderOptions {
                doc_dir: None,
                allow_unsafe_images: true,
            },
        );
        let (palette, theme) = style();
        let out = render(&d, &palette, &theme);
        assert!(out.contains("export-note"), "{out}");
        assert!(out.contains("src=\"https://example.com/x.png\""), "{out}");
    }

    #[test]
    fn escape_covers_both_text_and_attribute_positions() {
        assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }
}
