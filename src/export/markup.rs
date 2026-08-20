//! Pango markup for an [`ExportDoc`]'s inline runs.
//!
//! Spelling, not rendering: every construct was already identified upstream, so
//! nothing here decides what anything *is* — it chooses how each one is written for
//! Pango. Separate from [`super::pdf`] because measuring and drawing is a different
//! job from serialising, and the two grew past one file.
//!
//! **Everything from the document is escaped.** Switching a run to markup makes every
//! interpolated string an injection and breakage surface, and a Pango parse failure
//! renders the whole run **empty** — silently, with no crash and no warning
//! (ScrAP-163). So an un-escaped string here is not a cosmetic defect; it is a blank
//! page.

use super::{ExportDoc, ImageSource, Inline};
use crate::theme::Theme;

/// Pango markup for an inline run, every string escaped.
///
/// Markup, not a second renderer: the constructs were already identified upstream, so
/// this only chooses how each one is *spelled* for Pango.
pub(super) fn inline_markup(inlines: &[Inline], doc: &ExportDoc, theme: &Theme) -> String {
    let mut out = String::new();
    emit_markup(inlines, doc, theme, &mut out);
    out
}

fn emit_markup(inlines: &[Inline], doc: &ExportDoc, theme: &Theme, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text { text, .. } => out.push_str(&escape_pango(text)),
            Inline::Code(c) => {
                out.push_str("<span font_family=\"monospace\">");
                out.push_str(&escape_pango(c));
                out.push_str("</span>");
            }
            Inline::Emphasis(v) => tag(out, "i", "", v, doc, theme),
            Inline::Strong(v) => tag(out, "b", "", v, doc, theme),
            Inline::Strikethrough(v) => tag(out, "s", "", v, doc, theme),
            Inline::Superscript(v) => tag(out, "sup", "", v, doc, theme),
            Inline::Subscript(v) => tag(out, "sub", "", v, doc, theme),
            Inline::Highlight(v) => {
                // The SAME theme key the body tag and the table cell read, in this
                // path's own representation — one source, three spellings (POLICY
                // "One theme key, every application path").
                let open = format!(
                    "<span background=\"{}\" bgalpha=\"{}\">",
                    theme.mark_bg.hex(),
                    theme.mark_bg.alpha_pct()
                );
                out.push_str(&open);
                emit_markup(v, doc, theme, out);
                out.push_str("</span>");
            }
            Inline::Claim(idx, v) => {
                let open = format!(
                    "<span background=\"{}\" bgalpha=\"{}\">",
                    theme.annotation_hl.hex(),
                    theme.annotation_hl.alpha_pct()
                );
                out.push_str(&open);
                emit_markup(v, doc, theme, out);
                out.push_str("</span>");
                // The comment as a margin note beside its claim — the in-file review
                // loop is the product thesis, and an export that drops the review is
                // the wrong document (TDD 25.13).
                if let Some(ann) = doc.annotations.get(*idx) {
                    let _ = std::fmt::Write::write_fmt(
                        out,
                        format_args!(
                            " <span size=\"small\" style=\"italic\">[{}]</span>",
                            escape_pango(&ann.comment)
                        ),
                    );
                }
            }
            Inline::Link { href, inner, .. } => {
                // Underlined in the theme's link colour, and the destination shown
                // where it differs from the text — a printed page cannot be clicked,
                // so a bare link label loses the only thing it carried.
                let colour = theme.link.map(crate::palette::to_hex).unwrap_or_else(|| {
                    theme.accent.map(crate::palette::to_hex).unwrap_or_default()
                });
                let attr = if colour.is_empty() {
                    " underline=\"single\"".to_string()
                } else {
                    format!(" underline=\"single\" foreground=\"{colour}\"")
                };
                tag(out, "span", &attr, inner, doc, theme);
                let label = super::plain_text(inner);
                if !href.is_empty() && *href != label {
                    out.push_str(&escape_pango(&format!(" ({href})")));
                }
            }
            Inline::Image(img) => {
                // A PDF cannot reference a URL the way HTML can, so a remote image is
                // named rather than silently absent (TDD 25.12).
                let note = match &img.source {
                    ImageSource::Remote(url) => format!("[image: {url}]"),
                    ImageSource::Missing(reason) => format!("[{reason}]"),
                    ImageSource::Embedded { .. } if img.alt.is_empty() => "[image]".to_string(),
                    ImageSource::Embedded { .. } => format!("[image: {}]", img.alt),
                };
                out.push_str("<span style=\"italic\">");
                out.push_str(&escape_pango(&note));
                out.push_str("</span>");
            }
            Inline::Break => out.push('\n'),
        }
    }
}

fn tag(out: &mut String, name: &str, attrs: &str, v: &[Inline], doc: &ExportDoc, theme: &Theme) {
    out.push('<');
    out.push_str(name);
    out.push_str(attrs);
    out.push('>');
    emit_markup(v, doc, theme, out);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// Escape for Pango markup.
///
/// Switching a run to markup makes **every** interpolated string an injection and
/// breakage surface — an un-escaped metacharacter renders the whole run EMPTY, with no
/// crash and no warning (ScrAP-163). So nothing reaches a markup string un-escaped.
pub(super) fn escape_pango(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod markup_tests {
    use super::escape_pango;

    #[test]
    fn every_pango_metacharacter_is_escaped() {
        // ScrAP-163: an un-escaped metacharacter renders the whole run EMPTY, with no
        // crash and no warning, so this is the difference between a page and a blank.
        assert_eq!(
            escape_pango("a & b < c > d ' e \" f"),
            "a &amp; b &lt; c &gt; d &apos; e &quot; f"
        );
    }

    #[test]
    fn markup_from_an_untrusted_document_cannot_open_a_span() {
        let hostile = "<span foreground=\"red\">not markup</span>";
        let escaped = escape_pango(hostile);
        assert!(!escaped.contains('<'), "{escaped}");
        assert!(!escaped.contains('>'), "{escaped}");
    }
}
