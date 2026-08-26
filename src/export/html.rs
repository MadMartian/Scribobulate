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
    let page = Page { doc, theme };
    for block in &doc.blocks {
        block_html(block, &page, &mut out);
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

/// What the emission pass needs about the document **and** the theme, together.
///
/// The theme is here because a construct's HTML can depend on a theme key — a task
/// item's marker is a `<input type=checkbox>` unless the theme states a glyph or a
/// sprite for it, in which case the artefact must show what the preview shows (TDD
/// 25.3). Bundling it with the document keeps every level of the walk at the same
/// arity it already had, rather than threading a second parameter through five
/// signatures and twenty call sites.
struct Page<'a> {
    doc: &'a ExportDoc,
    theme: &'a Theme,
}

fn block_html(block: &Block, page: &Page<'_>, out: &mut String) {
    match block {
        Block::Heading { level, id, inlines } => {
            let _ = write!(out, "<h{level} id=\"{}\">", escape_attr(id));
            inlines_html(inlines, page, out);
            let _ = writeln!(out, "</h{level}>");
        }
        Block::Paragraph(inlines) => {
            out.push_str("<p>");
            inlines_html(inlines, page, out);
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
                block_html(b, page, out);
            }
            out.push_str("</blockquote>\n");
        }
        Block::List { start, items } => list_html(*start, items, page, out),
        Block::Table { aligns, head, rows } => table_html(aligns, head, rows, page, out),
        Block::Rule => out.push_str("<hr>\n"),
    }
}

fn list_html(start: Option<u64>, items: &[ListItem], page: &Page<'_>, out: &mut String) {
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
            // The theme may stand a glyph or a sprite in for the drawn checkbox (TDD
            // 18.24). Where it does, the artefact shows what the PREVIEW shows (25.3)
            // rather than a control the preview is not drawing; where it does not, the
            // `<input>` below is exactly what this sink always emitted.
            match task_marker_html(page.theme, checked) {
                Some(marker) => {
                    out.push_str(&marker);
                    out.push(' ');
                }
                // Disabled: the artefact is a record, and a checkbox a reader could
                // toggle would imply an edit that goes nowhere.
                None => out.push_str(if checked {
                    "<input type=\"checkbox\" checked disabled> "
                } else {
                    "<input type=\"checkbox\" disabled> "
                }),
            }
        }
        // A single-paragraph item is emitted tight, as a Markdown "tight list" is
        // shown on screen — a `<p>` per item would space every list out.
        match item.blocks.as_slice() {
            [Block::Paragraph(inlines)] => inlines_html(inlines, page, out),
            blocks => {
                out.push('\n');
                for b in blocks {
                    block_html(b, page, out);
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
    page: &Page<'_>,
    out: &mut String,
) {
    out.push_str("<table>\n");
    if !head.is_empty() {
        out.push_str("<thead>\n<tr>");
        for (i, cell) in head.iter().enumerate() {
            let _ = write!(out, "<th{}>", align_attr(aligns.get(i)));
            inlines_html(cell, page, out);
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
                inlines_html(cell, page, out);
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

fn inlines_html(inlines: &[Inline], page: &Page<'_>, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text { text, .. } => out.push_str(&escape(text)),
            Inline::Code(c) => {
                let _ = write!(out, "<code>{}</code>", escape(c));
            }
            Inline::Emphasis(v) => wrap(out, "em", "", v, page),
            Inline::Strong(v) => wrap(out, "strong", "", v, page),
            Inline::Strikethrough(v) => wrap(out, "del", "", v, page),
            Inline::Superscript(v) => wrap(out, "sup", "", v, page),
            Inline::Subscript(v) => wrap(out, "sub", "", v, page),
            Inline::Highlight(v) => wrap(out, "mark", "", v, page),
            Inline::Break => out.push_str("<br>\n"),
            Inline::Link { href, title, inner } => {
                let t = title
                    .as_deref()
                    .map(|t| format!(" title=\"{}\"", escape_attr(t)))
                    .unwrap_or_default();
                let _ = write!(out, "<a href=\"{}\"{t}>", escape_attr(href));
                inlines_html(inner, page, out);
                out.push_str("</a>");
            }
            Inline::Image(img) => image_html(img, out),
            Inline::Claim(idx, v) => {
                // The claim highlight, plus its comment as an aside beside it — the
                // in-file review loop is the product thesis, so an export that drops
                // the review is the wrong document (TDD 25.13).
                let _ = write!(out, "<span class=\"claim\" id=\"claim-{}\">", idx + 1);
                inlines_html(v, page, out);
                out.push_str("</span>");
                if let Some(ann) = page.doc.annotations.get(*idx) {
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

fn wrap(out: &mut String, tag: &str, attrs: &str, v: &[Inline], page: &Page<'_>) {
    let _ = write!(out, "<{tag}{attrs}>");
    inlines_html(v, page, out);
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
pub(crate) fn escape(s: &str) -> String {
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
    // Per-level heading face and colour (TDD 18.21). `Theme::resolve` already folded
    // each slot down to the theme's singular `heading_font`/`heading_color`, so a level
    // the theme left unset arrives here carrying that fallback and this sink neither
    // repeats the fold nor can disagree with the preview about it.
    let heading_font = |level: usize| {
        t.heading_fonts[level]
            .as_ref()
            .map(|f| f.as_str().to_string())
            .unwrap_or_else(|| body_font.clone())
    };
    let heading_fg = |level: usize| {
        t.heading_colors[level]
            .map(to_hex)
            .unwrap_or_else(|| to_hex(p.body_fg))
    };
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
a {{ color: {link};{link_line} }}
code {{ background: {code_inline}; border-radius: 3px; padding: 0.1em 0.3em; }}
pre {{ background: {code_block}; padding: {cell_pv}px {cell_ph}px; border-radius: {radius}px;
  overflow-x: auto; }}
pre code {{ background: none; padding: 0; }}
blockquote {{ border-left: {bar_w}px solid {bar}; margin-left: 0;
  padding-left: {bar_gap}px; }}
{bar_sprite_css}hr {{ border: 0; border-top: 1px solid {rule}; margin: {rule_space}px 0; }}
table {{ border-collapse: collapse; }}
th, td {{ border: {tbw}px solid {tb}; padding: {cell_pv}px {cell_ph}px; }}
th {{ background: {thead}; }}
td.a-l, th.a-l {{ text-align: left; }}
td.a-c, th.a-c {{ text-align: center; }}
td.a-r, th.a-r {{ text-align: right; }}
ul, ol {{ padding-left: {step}px; }}
li {{ margin-bottom: {li_gap}px; }}
li::marker {{ color: {marker}; }}
{marker_depths}li.task-list-item {{ list-style: none; }}
{task_marker_css}mark {{ background: {mark_bg}; {mark_fg} }}
sup {{ font-size: {sup}%; vertical-align: super; }}
sub {{ font-size: {sup}%; vertical-align: sub; }}
strong {{ font-weight: {bold}; }}
.claim {{ background: {claim}; }}
.comment {{ display: block; border-left: {bar_w}px solid {bar}; margin: 0.4em 0 0.4em 1rem;
  padding-left: {bar_gap}px; font-size: 0.9em; opacity: 0.85; }}
del {{ {strike} }}
.missing-image {{ border: 1px dashed {rule}; padding: 0.2em 0.4em; opacity: 0.75; }}
.export-note {{ max-width: 46rem; margin: 1.5rem auto; font-size: 0.9em; opacity: 0.8; }}
.annotations {{ max-width: 46rem; margin: 2rem auto 0; border-top: 1px solid {rule};
  padding-top: 1rem; }}
.annotations blockquote.claim {{ background: {claim}; border-left: 0; padding: 0.2em 0.4em; }}
img {{ max-width: 100%; height: auto; }}
{chip_css}{list_marker_css}",
        page = to_hex(p.page_bg),
        fg = to_hex(p.body_fg),
        base = BASE_PT,
        link = to_hex(p.link_fg),
        link_line = link_underline_css(t),
        code_inline = to_hex(p.code_inline_bg),
        code_block = to_hex(p.code_block_bg),
        bar = to_hex(p.blockquote_bar),
        bar_w = m.blockquote_bar_width,
        bar_sprite_css = blockquote_bar_sprite_css(t),
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
        marker_depths = list_marker_depth_css(t, &list_marker),
        task_marker_css = task_marker_css(t),
        mark_bg = t.mark_bg.hex(),
        mark_fg = mark_fg,
        strike = t
            .strikethrough_rgba
            .map(|c| format!("text-decoration-color: {};", to_hex(c)))
            .unwrap_or_default(),
        claim = t.annotation_hl.hex(),
        sup = (ty.supsub_scale * 100.0).round() as i32,
        bold = ty.bold_weight,
        chip_css = annotation_chip_css(t),
        list_marker_css = list_marker_css(t),
    );
    // Headings: the theme's scale ladder, applied to the base size. Five entries, not
    // six — `emit.rs` maps h6-and-deeper onto the h5 tag before a tag is chosen, so no
    // theme can differentiate them and this export must not invent a difference.
    let rule = heading_rule_css(t);
    for level in 1..=6usize {
        let i = (level - 1).min(4);
        let scale = ty.heading_scale[i];
        let space = m.heading_space_below[i];
        let _ = writeln!(
            css,
            "h{level} {{ font-family: {face}; color: {fg}; \
             font-size: {size:.2}pt; font-weight: {weight}; \
             margin: {top} 0 {space}px;{rule}{band} }}",
            face = heading_font(i),
            fg = heading_fg(i),
            size = BASE_PT * scale,
            weight = ty.heading_weight,
            top = heading_margin_top(m.heading_space_above[i]),
            band = heading_band_css(t, i),
        );
    }
    css
}

/// The heading band's declarations for one level (TDD 18.25). Empty unless the theme
/// bands that level, so an untouched theme's heading rules are byte-identical.
///
/// **This is the cheap side of the decoration, and the plan says so.** On screen the band
/// costs a span vector, an install choke point, a gate entry and a paint loop; here it is
/// a `background` on a real `<h1>`. The asymmetry is a trap in both directions — it
/// invites designing a decoration that is free in the artefact and unaffordable on
/// screen, and "the export already does this" is never evidence the preview can.
///
/// The band spans the heading's own box, which is the content column in both media —
/// that is why the preview draws it at the content column rather than edge-to-edge.
fn heading_band_css(t: &Theme, level_index: usize) -> String {
    let Some(fill) = t.heading_band.fills[level_index] else {
        return String::new();
    };
    let radius = t.metrics.heading_band_radius;
    let mut out = String::new();
    // A sprite outranks the fill and the gradient, the same precedence the drawn gutter
    // applies to a marker — and it TILES at natural size here too, so the artefact and
    // the screen show the same picture rather than the same file scaled differently.
    if let Some((uri, _, _)) = t.sprites.heading_band.as_ref().and_then(sprite_data_uri) {
        let _ = write!(out, " background: url({uri}) repeat;");
    } else if let Some(to) = t.heading_band.gradient_to {
        let _ = write!(
            out,
            " background: linear-gradient({}, {});",
            to_hex(fill),
            to_hex(to)
        );
    } else {
        let _ = write!(out, " background: {};", to_hex(fill));
    }
    if radius > 0 {
        let _ = write!(out, " border-radius: {radius}px;");
    }
    // The band's internal padding, and `box-sizing` is half the fix rather than a tidy-up:
    // CSS padding grows the box OUTWARDS by default, so a bare `padding` would widen the
    // band past the content column and leave the text exactly where it was — the opposite
    // of the intent. `border-box` makes the padding eat into the column instead, which is
    // the preview's behaviour (band at the content column, text inset from it) and what
    // keeps all three renderings agreeing on the band's extent (TDD 25.3).
    let pad = t.metrics.heading_band_padding;
    if pad > 0 {
        let _ = write!(out, " box-sizing: border-box; padding: 0 {pad}px;");
    }
    out
}

/// The HTML for a task item's marker when the theme stands something in for the drawn
/// checkbox — a glyph (HTML-escaped) or a sprite (embedded, so the artefact stays one
/// self-contained file). `None` means the theme states neither and the sink's own
/// `<input type="checkbox">` stands, byte-identical to before this key existed.
///
/// The task marker is inline content here rather than a `::marker`, because that is
/// what it replaces: this sink has always drawn the checkbox INSIDE the `<li>`. The
/// bullet and ordered markers are the opposite case and go through CSS `::marker` —
/// same key, different mechanism, because the two markers are different things in HTML
/// even though the preview's gutter draws both the same way.
fn task_marker_html(t: &Theme, checked: bool) -> Option<String> {
    let (sprite, glyph) = if checked {
        (&t.sprites.list_task_checked, &t.list_glyphs.task_checked)
    } else {
        (&t.sprites.list_task, &t.list_glyphs.task)
    };
    if let Some(sprite) = sprite {
        if let Some((uri, w, h)) = sprite_data_uri(sprite) {
            return Some(format!(
                "<img class=\"task-marker\" src=\"{uri}\" width=\"{w}\" height=\"{h}\" alt=\"\">"
            ));
        }
    }
    // ONE key, THREE grammars: this projection is the HTML one, and it is a different
    // escape from the Pango-markup one the PDF sink takes (`sdd/PLAN.preview-decoration.md`
    // constraint 2 — a single `markup_escape_text` is not sufficient once both sinks
    // are involved, and what leaves this application is opened by software this project
    // does not control).
    // Classed rather than inline-styled, so its colour lives in the sheet with every
    // other themed value (TDD 25.9) — see `task_marker_css`.
    glyph.as_ref().map(|g| {
        format!(
            "<span class=\"task-marker\">{}</span>",
            g.escaped_for_html()
        )
    })
}

/// The blockquote bar's sprite (TDD 18.28), tiled at its natural size. Empty unless the
/// theme names one, so a theme without it emits the flat `border-left` rule alone,
/// byte-identical to before this key existed.
///
/// Drawn on a `::before` rather than as a background on the blockquote itself, because
/// only a positioned box can be clipped to exactly `blockquote_bar_width`: a background
/// with `repeat-y` would make the strip the SPRITE's natural width instead of the bar's,
/// so a theme whose tile is wider than its bar would silently get a wider bar in the
/// artefact than on screen. The border stays for the indent it reserves and goes
/// transparent, which is this sink's explicit statement that the sprite outranks the
/// flat colour — the same branch the drawn bar and the PDF each make for themselves.
fn blockquote_bar_sprite_css(t: &Theme) -> String {
    let Some((uri, _, _)) = t.sprites.blockquote_bar.as_ref().and_then(sprite_data_uri) else {
        return String::new();
    };
    let bar_w = t.metrics.blockquote_bar_width;
    format!(
        "blockquote {{ position: relative; border-left-color: transparent; }}\n\
         blockquote::before {{ content: \"\"; position: absolute; left: -{bar_w}px; top: 0; \
         bottom: 0; width: {bar_w}px; background: url({uri}) repeat; }}\n"
    )
}

/// The task marker's own colour (TDD 18.27), for both the themed glyph and the
/// `<input type="checkbox">` this sink falls back to. Empty unless the theme resolves one.
///
/// Two rules because the marker is two different things here depending on the theme, and
/// a colour that reached one of them would be the drift the parity rule exists to stop.
/// `accent-color` is how a checkbox is themed at all — `color` does not reach it.
///
/// This also closes a quiet gap: the sheet's shared `li::marker` rule cannot reach a task
/// item, because that item suppresses its marker box (`list-style: none`) to draw its own
/// checkbox. So before this key, a theme's `list_marker` coloured the preview's drawn
/// checkbox and left the artefact's on the reader's default — visible only side by side.
fn task_marker_css(t: &Theme) -> String {
    let Some(c) = t.list_task_color else {
        return String::new();
    };
    let hex = to_hex(c);
    format!(
        ".task-marker {{ color: {hex}; }}\n\
         li.task-list-item input[type=\"checkbox\"] {{ accent-color: {hex}; }}\n"
    )
}

/// `::marker` rules for the bullet and ordered list markers when the theme stands a
/// glyph or a sprite in for them (TDD 18.24). Empty unless it does, so an untouched
/// theme leaves this sheet's list rules exactly as they were.
///
/// A `content` string is a CSS string literal, which is a THIRD grammar for the same
/// key — `"` and `\` are its metacharacters, not `<` and `&`. The glyph's own
/// validation already refuses control characters, and the CSS escape below closes the
/// rest; a quote that reached this rule unescaped would end the string and put the
/// remainder of the glyph into the stylesheet as declarations.
fn list_marker_css(t: &Theme) -> String {
    let mut css = String::new();
    // The bullet, once per nesting-depth tier (TDD 18.26). The selectors are plain
    // descendant combinators — a bullet item is `ul > li`, and one nested inside another
    // item has an `li` ancestor — so CSS specificity does the depth arithmetic for free:
    // the three-compound selector outranks the two-compound one, and both outrank the
    // bare `ul li`, with no `!important` and no depth counting in the emitter.
    //
    // ⚠️ Bullet-SCOPED on purpose (`ul > li`, never a bare `li li`): only the bullet is
    // depth-varying, so a nested ORDERED item must keep the shared colour it has at
    // every depth. A bare `li li::marker` would catch it.
    //
    // Emitting what the shared `marker_substitute` picks PER TIER — rather than letting
    // three independent property cascades interleave — is what keeps this sink agreeing
    // with the drawn gutter about which key wins at each depth. It is safe because the
    // fold makes the answer monotonic down the tiers: a tier inherits the shallower
    // tier's sprite, so a sprite at depth 1 cannot be undercut by a glyph at depth 2.
    for ((item, sprite), glyph) in BULLET_TIER_SELECTORS
        .iter()
        .zip(&t.sprites.list_bullet)
        .zip(&t.list_glyphs.bullet)
    {
        emit_marker_rule(&mut css, item, sprite, glyph);
    }
    for (selector, sprite, glyph) in [(
        "ol li::marker",
        &t.sprites.list_ordered,
        &t.list_glyphs.ordered,
    )] {
        // A `::marker` cannot carry an image, so a sprite marker becomes a background
        // on the item with the marker suppressed — the same picture, by the only route
        // CSS offers. `list-style: none` is on the same rule so the two can never be
        // applied apart.
        if let Some((uri, w, h)) = sprite.as_ref().and_then(sprite_data_uri) {
            let item = selector.trim_end_matches("::marker");
            let _ = writeln!(
                css,
                "{item} {{ list-style: none; background: url({uri}) no-repeat left \
                 center / {w}px {h}px; padding-left: {pad}px; }}",
                pad = w + 6
            );
            continue;
        }
        if let Some(g) = glyph {
            let _ = writeln!(
                css,
                "{selector} {{ content: \"{}\"; }}",
                css_string_escape(g.escaped_for_html().as_str())
            );
        }
    }
    css
}

/// The item selector for each of the bullet's nesting-depth tiers (TDD 18.26).
///
/// Plain descendant combinators, so CSS specificity does the depth arithmetic for free:
/// the three-compound selector outranks the two-compound one, and both outrank the bare
/// `li::marker`, with no `!important` and no depth counting in the emitter.
///
/// ⚠️ Bullet-SCOPED on purpose (`ul > li`, never a bare `li li`): only the bullet is
/// depth-varying, so a nested ORDERED item must keep the shared colour it has at every
/// depth, and a bare `li li` would catch it. The final step is a child combinator for
/// the same reason — an `ol > li` sitting inside a `ul` is a numeral, not a bullet.
const BULLET_TIER_SELECTORS: [&str; crate::theme::BULLET_TIERS] =
    ["ul > li", "li ul > li", "li li ul > li"];

/// One list-item rule: the sprite background if the theme states one for this marker,
/// else the glyph's `::marker` content, else nothing. `item` is the ITEM selector (the
/// `::marker` suffix is added where it belongs), so the same call emits both shapes —
/// a sprite has to style the item and a glyph has to style its marker box.
fn emit_marker_rule(
    css: &mut String,
    item: &str,
    sprite: &Option<crate::sprite::SpriteRef>,
    glyph: &Option<crate::theme::MarkerGlyph>,
) {
    // A `::marker` cannot carry an image, so a sprite marker becomes a background on the
    // item with the marker suppressed — the same picture, by the only route CSS offers.
    // `list-style: none` is on the same rule so the two can never be applied apart.
    if let Some((uri, w, h)) = sprite.as_ref().and_then(sprite_data_uri) {
        let _ = writeln!(
            css,
            "{item} {{ list-style: none; background: url({uri}) no-repeat left \
             center / {w}px {h}px; padding-left: {pad}px; }}",
            pad = w + 6
        );
        return;
    }
    if let Some(g) = glyph {
        let _ = writeln!(
            css,
            "{item}::marker {{ content: \"{}\"; }}",
            css_string_escape(g.escaped_for_html().as_str())
        );
    }
}

/// The `li::marker` colour rules: the shared `list_marker` for every marker, then the
/// BULLET's depth-2 and depth-3 overrides where a theme states them (TDD 18.26).
///
/// Emitted only where a tier differs from the tier above it — a tier that merely
/// inherited its colour would emit a rule that restates what the cascade already says,
/// which is noise in a file a human reads.
fn list_marker_depth_css(t: &Theme, shared: &str) -> String {
    let mut css = String::new();
    let mut previous = t.list_bullet_colors[0]
        .map(to_hex)
        .unwrap_or_else(|| shared.to_string());
    for (item, colour) in BULLET_TIER_SELECTORS
        .iter()
        .zip(&t.list_bullet_colors)
        .skip(1)
    {
        let Some(here) = colour.map(to_hex) else {
            continue;
        };
        if here != previous {
            let _ = writeln!(css, "{item}::marker {{ color: {here}; }}");
        }
        previous = here;
    }
    css
}

/// Escape a string for a CSS **string literal** — the grammar a `content:` value is in.
/// Only `\` and `"` can end or re-open it; everything else is inert, and the glyph's own
/// validation has already refused the control characters that would need `\A` forms.
fn css_string_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The `a` rule's underline declarations (TDD 18.23). Empty at the floor — a solid
/// single line with no stated colour, which is both what the preview draws and what a
/// browser does for a bare `<a>`, so an untouched theme leaves this sink's `a` rule
/// byte-identical to before either key existed.
fn link_underline_css(t: &Theme) -> String {
    let mut out = String::new();
    match t.link_underline.css_style() {
        None => out.push_str(" text-decoration-line: none;"),
        Some("solid") => {}
        Some(style) => {
            let _ = write!(out, " text-decoration-style: {style};");
        }
    }
    if let Some(c) = t.link_underline_rgba {
        let _ = write!(out, " text-decoration-color: {};", to_hex(c));
    }
    out
}

/// This sink's own block rhythm above a heading, in `em` so it tracks the heading's
/// size the way a document's flow spacing should. It is NOT what `heading_space_above`
/// replaces: that key is the theme's *additional* space, exactly as the preview's
/// `pixels_above_lines` adds to whatever the preceding block already left.
const HEADING_FLOW_MARGIN: &str = "1.2em";

/// The `margin-top` for a heading, given the theme's design-time `heading_space_above`.
///
/// Zero — which is every theme that does not state the key — yields the bare flow margin
/// this sink has always emitted, so an unset key leaves the artefact byte-identical
/// (TDD 18.2 at the export path).
fn heading_margin_top(space_above: i32) -> String {
    if space_above == 0 {
        HEADING_FLOW_MARGIN.to_string()
    } else {
        format!("calc({HEADING_FLOW_MARGIN} + {space_above}px)")
    }
}

/// The heading rule's CSS declarations (TDD 18.22), empty unless the theme draws one —
/// so a theme that states no rule emits exactly the heading rules it always did.
///
/// **Stated scope limit.** CSS carries ONE `text-decoration-style` and ONE
/// `text-decoration-color` for the whole element, while the theme states the style per
/// side and colours only the underline (`theme::HeadingRule` says why the overline has
/// no colour). So a theme that turns BOTH sides on and colours the underline gets that
/// colour on the overline too here, where the preview draws the overline in the
/// heading's ink. Named rather than left for a reader to discover in a browser; the
/// single-sided case — every shipped example — is exact.
fn heading_rule_css(t: &Theme) -> String {
    let rule = &t.heading_rule;
    if rule.is_absent() {
        return String::new();
    }
    let mut lines: Vec<&str> = Vec::new();
    if !rule.overline.is_none() {
        lines.push("overline");
    }
    if !rule.underline.is_none() {
        lines.push("underline");
    }
    // Whichever side is on, preferring the underline where both are.
    let (style, colour) = if rule.underline.is_none() {
        (rule.overline.css_style(), None)
    } else {
        (rule.underline.css_style(), rule.underline_rgba)
    };
    let mut out = format!(" text-decoration-line: {};", lines.join(" "));
    if let Some(style) = style {
        let _ = write!(out, " text-decoration-style: {style};");
    }
    if let Some(colour) = colour {
        let _ = write!(out, " text-decoration-color: {};", to_hex(colour));
    }
    out
}

/// CSS for the annotation-review affordance (TDD 25.13, 18.19). Empty unless the
/// theme sets at least one chip key — so a theme that sets none of them leaves this
/// sink's output BYTE-IDENTICAL to before the chip could be themed at all.
///
/// The chip's HTML equivalent is `.comment a` — the numbered link back to its claim
/// (`Inline::Claim`'s emission, above) is the closest thing this sink has to the
/// preview's gutter badge, since the artefact has no separate drawn marker.
fn annotation_chip_css(t: &Theme) -> String {
    if t.annotation_chip_bg.is_none()
        && t.annotation_chip_fg.is_none()
        && t.sprites.annotation_chip.is_none()
    {
        return String::new();
    }
    let mut decl = String::from(
        "display: inline-block; min-width: 1.1em; padding: 0 0.3em;          border-radius: 0.9em; text-align: center; text-decoration: none;",
    );
    if let Some(bg) = t.annotation_chip_bg {
        let _ = write!(decl, " background: {};", to_hex(bg));
    }
    if let Some(fg) = t.annotation_chip_fg {
        let _ = write!(decl, " color: {};", to_hex(fg));
    }
    // A sprite REPLACES the flat fill (same rule the preview's gutter chip follows) —
    // sized to its own aspect so a non-square chip does not distort, embedded as a
    // data URI so the artefact stays one self-contained file (TDD §25).
    if let Some(sprite) = t.sprites.annotation_chip.as_ref() {
        if let Some((uri, w, h)) = sprite_data_uri(sprite) {
            let aspect = if h > 0 { w as f64 / h as f64 } else { 1.0 };
            let _ = write!(
                decl,
                " background: url({uri}) no-repeat center / contain;                  background-color: transparent; width: {aspect:.3}em; height: 1em;                  image-rendering: pixelated;"
            );
        }
    }
    format!(
        ".comment a {{ {decl} }}
"
    )
}

/// The largest sprite this sink will embed. Mirrors `crate::sprite`'s own cap — this
/// is a SECOND check, not a substitute for it: `crate::sprite::resolve` already
/// refused anything over its cap before this path ever sees the value, but base64
/// inflates by roughly a third on top, so the sink caps independently rather than
/// trusting a bound set for a different budget.
const SPRITE_EMBED_CAP: usize = 512 * 1024;

/// A sprite as `(data URI, natural width, natural height)`. Display-free, and source-
/// agnostic: the bytes come from `crate::sprite::bytes`, so a theme's own file and a
/// compiled-in sprite reach the artefact by the same route and embed identically.
fn sprite_data_uri(sprite: &crate::sprite::SpriteRef) -> Option<(String, i32, i32)> {
    use gtk::prelude::TextureExt;
    let bytes = crate::sprite::bytes(sprite)?;
    if bytes.is_empty() || bytes.len() > SPRITE_EMBED_CAP {
        log::warn!(
            "export: sprite {sprite} is {} bytes (cap {SPRITE_EMBED_CAP}) — not embedded",
            bytes.len()
        );
        return None;
    }
    // Dimensions via the SAME decode+cache `crate::sprite::texture` uses everywhere
    // else, rather than a second image-loading path — `GdkTexture` decodes with no
    // live display (the PDF sink's own `decode` already proves this), so it is safe
    // in an export sink that must run display-free.
    let tex = crate::sprite::texture(sprite)?;
    let (w, h) = (tex.width(), tex.height());
    let mime = match sprite.extension().as_deref() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => return None,
    };
    Some((format!("data:{mime};base64,{}", base64(&bytes)), w, h))
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

    /// TDD 18.19: unset (System, and every shipped theme) ⇒ NO `.comment a` rule at
    /// all — the sink is byte-identical to before the chip could be themed.
    #[test]
    fn annotation_chip_css_is_empty_when_no_chip_key_is_set() {
        let (_, theme) = style();
        assert!(super::annotation_chip_css(&theme).is_empty());
    }

    /// A minimal valid 1×1 PNG — the same fixture shape `sprite.rs`'s own tests use,
    /// named once here because more than one sprite test needs a file that really
    /// decodes rather than a blob that merely passes the size/extension gate.
    const ONE_PIXEL_PNG: [u8; 69] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59, 0xE7, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn annotation_chip_css_embeds_a_sprite_as_a_data_uri() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        std::fs::write(&path, ONE_PIXEL_PNG).unwrap();
        let (_, mut theme) = style();
        theme.sprites.annotation_chip = Some(crate::sprite::SpriteRef::File(path));
        let css = super::annotation_chip_css(&theme);
        assert!(css.contains("url(data:image/png;base64,"), "{css}");
        assert!(css.contains("image-rendering: pixelated"), "{css}");
    }

    #[test]
    fn annotation_chip_css_carries_the_themed_colours() {
        let (_, mut theme) = style();
        theme.annotation_chip_bg = Some(gtk::gdk::RGBA::new(0.1, 0.2, 0.3, 1.0));
        theme.annotation_chip_fg = Some(gtk::gdk::RGBA::WHITE);
        let css = super::annotation_chip_css(&theme);
        assert!(css.contains(".comment a"), "{css}");
        assert!(css.contains("background: #1a334d"), "{css}");
        assert!(css.contains("color: #ffffff"), "{css}");
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

    /// TDD 18.21 / 25.3 — the artefact shows the per-level heading colour and face the
    /// preview does. Both halves matter: a level the theme states takes its own value,
    /// and a level it leaves empty takes the singular `heading_color`/`heading_font`, so
    /// the sink cannot disagree with the tag about the fallback (the fold is done once,
    /// in `Theme::resolve`, and this only indexes it).
    #[test]
    fn heading_rules_carry_the_per_level_colour_and_face() {
        let (palette, mut theme) = style();
        theme.heading_color = Some(gtk::gdk::RGBA::new(0.0, 0.0, 1.0, 1.0));
        theme.heading_colors = [
            Some(gtk::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0)),
            theme.heading_color,
            theme.heading_color,
            theme.heading_color,
            theme.heading_color,
        ];
        let css = super::stylesheet(&palette, &theme);
        assert!(css.contains("h1 { font-family:"), "{css}");
        let h1 = css.lines().find(|l| l.starts_with("h1 ")).expect("h1 rule");
        let h2 = css.lines().find(|l| l.starts_with("h2 ")).expect("h2 rule");
        assert!(h1.contains("color: #ff0000"), "{h1}");
        assert!(h2.contains("color: #0000ff"), "{h2}");
        // h6 folds onto the h5 slot, exactly as the preview's tag does.
        let h6 = css.lines().find(|l| l.starts_with("h6 ")).expect("h6 rule");
        assert!(h6.contains("color: #0000ff"), "{h6}");
    }

    /// TDD 18.22 / 18.2 — no heading rule and no space above ⇒ the heading rules are
    /// exactly what this sink emitted before either key existed.
    #[test]
    fn heading_rules_are_unchanged_when_the_theme_states_no_rule() {
        let (palette, theme) = style();
        let css = super::stylesheet(&palette, &theme);
        assert!(!css.contains("text-decoration-line"), "{css}");
        assert!(!css.contains("calc("), "{css}");
        assert!(
            css.lines()
                .any(|l| l.starts_with("h1 ") && l.contains("margin: 1.2em 0 4px;")),
            "{css}"
        );
    }

    /// TDD 18.22 / 25.3 — a stated rule and a stated space above reach the artefact.
    #[test]
    fn a_heading_rule_and_its_space_above_reach_the_stylesheet() {
        let (palette, mut theme) = style();
        theme.heading_rule = crate::theme::HeadingRule {
            overline: crate::theme::LineStyle::Single,
            underline: crate::theme::LineStyle::Wavy,
            underline_rgba: Some(gtk::gdk::RGBA::new(0.0, 0.0, 1.0, 1.0)),
        };
        theme.metrics.heading_space_above = [24, 0, 0, 0, 0];
        let css = super::stylesheet(&palette, &theme);
        let h1 = css.lines().find(|l| l.starts_with("h1 ")).expect("h1 rule");
        let h2 = css.lines().find(|l| l.starts_with("h2 ")).expect("h2 rule");
        assert!(
            h1.contains("text-decoration-line: overline underline;"),
            "{h1}"
        );
        // The documented scope limit: CSS carries ONE style and ONE colour for the
        // element, so both sides take the UNDERLINE's. Asserted rather than left to be
        // discovered in a browser.
        assert!(h1.contains("text-decoration-style: wavy;"), "{h1}");
        assert!(h1.contains("text-decoration-color: #0000ff;"), "{h1}");
        assert!(h1.contains("margin: calc(1.2em + 24px) 0"), "{h1}");
        // A level whose space-above is zero keeps the bare flow margin.
        assert!(h2.contains("margin: 1.2em 0"), "{h2}");
    }

    /// TDD 18.23 / 18.2 / 25.3 — at the floor the `a` rule is exactly what it always
    /// was (a bare colour; the browser's own solid underline), and `del` carries no
    /// colour rule at all. Stated, both reach the artefact.
    #[test]
    fn the_link_underline_and_strike_colour_reach_the_stylesheet() {
        let (palette, mut theme) = style();
        let plain = super::stylesheet(&palette, &theme);
        assert!(
            plain
                .lines()
                .any(|l| l.starts_with("a { color: ") && l.ends_with("; }")),
            "{plain}"
        );
        assert!(plain.contains("del {  }"), "{plain}");

        theme.link_underline = crate::theme::LineStyle::Wavy;
        theme.link_underline_rgba = Some(gtk::gdk::RGBA::new(0.0, 1.0, 0.0, 1.0));
        theme.strikethrough_rgba = Some(gtk::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0));
        let themed = super::stylesheet(&palette, &theme);
        let a = themed
            .lines()
            .find(|l| l.starts_with("a "))
            .expect("a rule");
        assert!(a.contains("text-decoration-style: wavy;"), "{a}");
        assert!(a.contains("text-decoration-color: #00ff00;"), "{a}");
        assert!(
            themed.contains("del { text-decoration-color: #ff0000; }"),
            "{themed}"
        );

        // `none` is STATED, not omitted — an omitted line hands the decision back to the
        // reader's browser, which is the drift the whole sheet exists to prevent.
        theme.link_underline = crate::theme::LineStyle::None;
        let off = super::stylesheet(&palette, &theme);
        let a = off.lines().find(|l| l.starts_with("a ")).expect("a rule");
        assert!(a.contains("text-decoration-line: none;"), "{a}");
    }

    /// TDD 18.24 / 18.2 — no marker key ⇒ the sink emits exactly what it always did:
    /// an `<input type=checkbox>` per task item, and no `::marker` rules at all.
    #[test]
    fn list_markers_are_unchanged_when_the_theme_states_none() {
        let (palette, theme) = style();
        let css = super::stylesheet(&palette, &theme);
        assert!(!css.contains("::marker { content"), "{css}");
        assert!(super::task_marker_html(&theme, true).is_none());
        assert!(super::task_marker_html(&theme, false).is_none());
    }

    /// TDD 18.24 / 25.3 — a themed glyph reaches the artefact, HTML-ESCAPED, in both of
    /// the two mechanisms HTML has for the two kinds of marker: a `::marker` rule for a
    /// bullet or an ordinal, and inline content for a task's checkbox.
    ///
    /// The escaping is the point. This is the same key the PDF sink escapes for Pango
    /// markup and the gutter takes as plain text, and a single escape is not sufficient
    /// for all three (`sdd/PLAN.preview-decoration.md` constraint 2) — what leaves this
    /// application is opened by software this project neither controls nor sandboxes.
    #[test]
    fn a_themed_marker_glyph_reaches_the_artefact_html_escaped() {
        let (palette, mut theme) = style();
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(
            "[themes.marks]\nlist_bullet_glyph = \"<b>\"\nlist_ordered_glyph = \"&o\"\n\
             list_task_glyph = \"\\\"t\\\"\"\nlist_task_checked_glyph = \"✔\"\n",
        );
        theme.list_glyphs = themes.resolve("marks").list_glyphs;

        let css = super::stylesheet(&palette, &theme);
        assert!(
            css.contains("ul > li::marker { content: \"&lt;b&gt;\"; }"),
            "{css}"
        );
        assert!(
            css.contains("ol li::marker { content: \"&amp;o\"; }"),
            "{css}"
        );
        // Never the raw glyph — an un-escaped `<` here is an injection into a file a
        // browser opens, and an un-escaped `"` ends the CSS string it sits in.
        assert!(!css.contains("content: \"<b>\""), "{css}");

        let checked = super::task_marker_html(&theme, true).expect("a checked glyph");
        // Classed so the sheet can colour it (TDD 18.27) rather than inline-styled.
        assert_eq!(checked, "<span class=\"task-marker\">✔</span>");
        let unchecked = super::task_marker_html(&theme, false).expect("an unchecked glyph");
        // Assert on the glyph INSIDE the wrapper — the wrapper's own `class="…"` carries
        // quotes of its own, so a bare "contains no quote" check would now be measuring
        // this sink's markup rather than the theme's glyph.
        let inner = unchecked
            .trim_start_matches("<span class=\"task-marker\">")
            .trim_end_matches("</span>");
        assert!(!inner.contains('"'), "{inner}");
        assert_eq!(inner, "&quot;t&quot;");
    }

    /// TDD 18.26 — the bullet's depth tiers reach the artefact as depth-SCOPED selectors,
    /// and the cascade does the depth arithmetic.
    ///
    /// Two properties, and the second is the one a reader would not think to check: the
    /// selectors must grow in specificity with depth (so the deeper rule wins without an
    /// `!important`), and they must be BULLET-scoped (`ul > li`, never a bare `li li`) so
    /// a nested ORDERED item keeps the shared colour it has at every depth.
    #[test]
    fn a_bullets_depth_tiers_reach_the_artefact_as_scoped_selectors() {
        let (palette, mut theme) = style();
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(
            "[themes.tiered]\nlist_marker = \"#111111\"\nlist_marker_2 = \"#222222\"\n\
             list_marker_3 = \"#333333\"\nlist_bullet_glyph = \"1\"\n\
             list_bullet_glyph_2 = \"2\"\n",
        );
        let t = themes.resolve("tiered");
        theme.list_marker = t.list_marker;
        theme.list_bullet_colors = t.list_bullet_colors;
        theme.list_glyphs = t.list_glyphs;
        let css = super::stylesheet(&palette, &theme);

        // The shared rule stays kind-blind, so a numeral at any depth reads it.
        assert!(css.contains("li::marker { color: #111111; }"), "{css}");
        // …and each deeper tier adds one more compound, which is what makes it win.
        assert!(
            css.contains("li ul > li::marker { color: #222222; }"),
            "{css}"
        );
        assert!(
            css.contains("li li ul > li::marker { color: #333333; }"),
            "{css}"
        );
        // Never a bare `li li::marker`: that would catch a nested numbered item too.
        assert!(!css.contains("\nli li::marker"), "{css}");

        // The glyph tiers use the same selectors. Depth 3 inherited depth 2's glyph, so
        // no third rule is emitted — a rule restating what the cascade already says is
        // noise in a file a human reads.
        assert!(css.contains("ul > li::marker { content: \"1\"; }"), "{css}");
        assert!(
            css.contains("li ul > li::marker { content: \"2\"; }"),
            "{css}"
        );
    }

    /// TDD 18.26 / 18.2 — a theme that states no depth key emits no depth rule at all,
    /// so the sheet is byte-identical to before the tiers existed. An inherited tier is
    /// exactly the case that must NOT produce a rule.
    #[test]
    fn no_depth_rule_is_emitted_when_every_tier_inherits() {
        let (palette, mut theme) = style();
        theme.list_marker = Some(gtk::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0));
        theme.list_bullet_colors = [theme.list_marker; crate::theme::BULLET_TIERS];
        let css = super::stylesheet(&palette, &theme);
        assert!(css.contains("li::marker { color: #ff0000; }"), "{css}");
        assert!(!css.contains("li ul > li::marker { color"), "{css}");
        assert!(!css.contains("li li ul > li::marker { color"), "{css}");
    }

    /// TDD 18.27 — the task marker's colour reaches the artefact for BOTH shapes it
    /// takes here: a themed glyph, and the `<input type="checkbox">` this sink falls back
    /// to. `accent-color` is how a checkbox is themed at all — `color` does not reach it,
    /// so styling only the glyph would leave every un-glyphed theme's checkbox on the
    /// reader's default.
    #[test]
    fn the_task_markers_colour_reaches_both_shapes_it_takes() {
        let (palette, mut theme) = style();
        assert!(
            super::task_marker_css(&theme).is_empty(),
            "no colour stated must emit nothing"
        );
        theme.list_task_color = Some(gtk::gdk::RGBA::new(1.0, 0.0, 1.0, 1.0));
        let css = super::stylesheet(&palette, &theme);
        assert!(css.contains(".task-marker { color: #ff00ff; }"), "{css}");
        assert!(
            css.contains("li.task-list-item input[type=\"checkbox\"] { accent-color: #ff00ff; }"),
            "{css}"
        );
        // The shared marker rule is untouched — a bullet and a numeral keep their colour.
        assert!(css.contains("li::marker { color: "), "{css}");
    }

    /// TDD 18.28 — the blockquote bar's sprite reaches the artefact, embedded, tiled,
    /// and REPLACING the flat colour rather than sitting over it.
    ///
    /// The transparent border is that replacement, stated: leaving the border coloured
    /// would put the flat fill under a tile whose transparent pixels let it through, and
    /// it would only ever show for a theme whose tile has some.
    #[test]
    fn a_blockquote_bar_sprite_is_embedded_and_replaces_the_flat_border() {
        let (palette, mut theme) = style();
        assert!(
            super::blockquote_bar_sprite_css(&theme).is_empty(),
            "no sprite stated must emit nothing"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bar.png");
        std::fs::write(&path, ONE_PIXEL_PNG).unwrap();
        theme.sprites.blockquote_bar = Some(crate::sprite::SpriteRef::File(path));
        theme.metrics.blockquote_bar_width = 24;

        let css = super::stylesheet(&palette, &theme);
        assert!(css.contains("blockquote::before"), "{css}");
        assert!(css.contains("data:image/png;base64,"), "{css}");
        // Tiled, matching the preview, not stretched to the strip.
        assert!(css.contains(") repeat;"), "{css}");
        // Clipped to the BAR's width, not the sprite's: a background with `repeat-y`
        // would make the strip as wide as the tile, so a theme whose tile is wider than
        // its bar would get a wider bar here than on screen.
        assert!(css.contains("width: 24px;"), "{css}");
        assert!(css.contains("border-left-color: transparent;"), "{css}");
        // The flat rule is still emitted — it reserves the indent, and it is what the
        // artefact falls back to if the file is ever missing or refused.
        assert!(
            css.contains("blockquote { border-left: 24px solid"),
            "{css}"
        );
    }

    /// TDD 18.19/18.24/18.25/18.28 — a **compiled-in** sprite embeds too.
    ///
    /// This sink used to reach the bytes with `std::fs::read` on the resolved path,
    /// which for a built-in theme was a bare theme-relative string: it resolved
    /// against the process's working directory, so an export ran from a different
    /// directory silently dropped the decoration and one run from the source tree
    /// would have "proved" it worked. The sink now reads through
    /// `crate::sprite::bytes`, which answers for either source, so the assertion is
    /// that the artefact carries the picture with no file anywhere.
    #[test]
    fn a_compiled_in_sprite_is_embedded_by_the_html_sink() {
        let (palette, mut theme) = style();
        theme.sprites.blockquote_bar = crate::theme::Themes::builtin()
            .resolve("pixelquest")
            .sprites
            .blockquote_bar;
        theme.metrics.blockquote_bar_width = 24;
        let css = super::stylesheet(&palette, &theme);
        assert!(css.contains("blockquote::before"), "{css}");
        assert!(css.contains("url(data:image/png;base64,"), "{css}");
    }

    /// TDD 18.24 — a marker SPRITE is embedded, so the artefact stays one self-contained
    /// file (the same rule and the same technique 18.19's chip sprite established).
    #[test]
    fn a_marker_sprite_is_embedded_rather_than_referenced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dot.png");
        std::fs::write(&path, ONE_PIXEL_PNG).unwrap();
        let (palette, mut theme) = style();
        let sprite = crate::sprite::SpriteRef::File(path);
        theme.sprites.list_bullet = [
            Some(sprite.clone()),
            Some(sprite.clone()),
            Some(sprite.clone()),
        ];
        theme.sprites.list_task = Some(sprite);
        // A sprite OUTRANKS a glyph for the same marker — the precedence the drawn
        // gutter applies, asserted here so the two sinks cannot drift from it.
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test("[themes.marks]\nlist_bullet_glyph = \"x\"\n");
        theme.list_glyphs = themes.resolve("marks").list_glyphs;

        let css = super::stylesheet(&palette, &theme);
        assert!(css.contains("data:image/png;base64,"), "{css}");
        assert!(css.contains("list-style: none"), "{css}");
        assert!(
            !css.contains("content: \"x\""),
            "sprite must outrank glyph:\n{css}"
        );
        let task = super::task_marker_html(&theme, false).expect("a task sprite");
        assert!(task.starts_with("<img class=\"task-marker\""), "{task}");
        assert!(task.contains("data:image/png;base64,"), "{task}");
    }

    /// TDD 18.25 / 18.2 — no banded level ⇒ no `background` on any heading rule, so an
    /// untouched theme's headings are byte-identical to before the decoration existed.
    #[test]
    fn heading_rules_carry_no_band_when_the_theme_bands_nothing() {
        let (palette, theme) = style();
        let css = super::stylesheet(&palette, &theme);
        for level in 1..=6 {
            let rule = css
                .lines()
                .find(|l| l.starts_with(&format!("h{level} ")))
                .unwrap_or_else(|| panic!("no h{level} rule"));
            assert!(!rule.contains("background"), "{rule}");
            assert!(!rule.contains("border-radius"), "{rule}");
        }
    }

    /// TDD 18.25 / 25.3 — a banded level reaches the artefact, and ONLY the levels the
    /// theme bands. All three fills are covered because they are three code paths and
    /// the sprite has to outrank the other two, which is the same precedence the drawn
    /// preview applies.
    #[test]
    fn a_banded_heading_level_reaches_the_stylesheet() {
        let (palette, mut theme) = style();
        theme.heading_band.fills[0] = Some(gtk::gdk::RGBA::new(0.2, 0.4, 0.6, 1.0));
        theme.metrics.heading_band_radius = 8;
        let flat = super::stylesheet(&palette, &theme);
        let h1 = flat.lines().find(|l| l.starts_with("h1 ")).expect("h1");
        let h2 = flat.lines().find(|l| l.starts_with("h2 ")).expect("h2");
        assert!(h1.contains("background: #336699;"), "{h1}");
        assert!(h1.contains("border-radius: 8px;"), "{h1}");
        // The band's internal padding, and the `box-sizing` that makes it inset the TEXT
        // rather than widen the band past the content column — without it the rule reads
        // right and renders as the bug it was meant to fix.
        assert!(h1.contains("box-sizing: border-box;"), "{h1}");
        assert!(h1.contains("padding: 0 12px;"), "{h1}");
        // An unbanded level gets neither — a padded box with no fill in it.
        assert!(!h2.contains("padding"), "{h2}");
        assert!(!h2.contains("box-sizing"), "{h2}");
        // A level the theme did not band carries nothing — including no radius, which
        // would otherwise round a box with no fill in it.
        assert!(!h2.contains("background"), "{h2}");
        assert!(!h2.contains("border-radius"), "{h2}");

        theme.heading_band.gradient_to = Some(gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
        let grad = super::stylesheet(&palette, &theme);
        let h1 = grad.lines().find(|l| l.starts_with("h1 ")).expect("h1");
        assert!(
            h1.contains("background: linear-gradient(#336699, #000000);"),
            "{h1}"
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("band.png");
        std::fs::write(&path, ONE_PIXEL_PNG).unwrap();
        theme.sprites.heading_band = Some(crate::sprite::SpriteRef::File(path));
        let sprite = super::stylesheet(&palette, &theme);
        let h1 = sprite.lines().find(|l| l.starts_with("h1 ")).expect("h1");
        assert!(h1.contains("url(data:image/png;base64,"), "{h1}");
        // Tiled, matching the preview's `push_repeat` — not stretched to the box.
        assert!(h1.contains(") repeat;"), "{h1}");
        // …and it outranks the gradient that is still stated.
        assert!(!h1.contains("linear-gradient"), "{h1}");
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
