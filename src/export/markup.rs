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
use std::fmt::Write as _;

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
                // The theme's inline-code fill (TDD 18.7), on the code's own run —
                // which is what a `<code>` background is in every other medium too.
                // It reached the preview and the HTML sink and nothing here.
                let bg = theme
                    .code_inline_bg
                    .map(|c| format!(" background=\"{}\"", crate::palette::to_hex_rgba(c)))
                    .unwrap_or_default();
                out.push_str(&format!("<span font_family=\"monospace\"{bg}>"));
                out.push_str(&escape_pango(c));
                out.push_str("</span>");
            }
            Inline::Emphasis(v) => tag(out, "i", "", v, doc, theme),
            // Themed: `bold_weight` / `supsub_scale` + rise — the SAME `Typography`
            // methods the table cell uses (`renderer::bold_open` etc.), called
            // directly here because this sink resolves against an EXPLICIT `Theme`
            // (System-light for the PDF, TDD 25.9), never `crate::theme::active()`
            // (TDD 18.18 / plan constraint 1 — now a three-way parity, not two).
            Inline::Strong(v) => span(out, &crate::pangospan::bold(theme), v, doc, theme),
            // Themed: the strike colour — the same key the body tag and the table cell
            // read (TDD 18.23). Unset ⇒ the bare `<s>` this sink always emitted, which
            // is why the pair comes from one call (ScrAP-163).
            Inline::Strikethrough(v) => span(out, &crate::pangospan::strike(theme), v, doc, theme),
            Inline::Superscript(v) => {
                span(out, &crate::pangospan::superscript(theme), v, doc, theme)
            }
            Inline::Subscript(v) => span(out, &crate::pangospan::subscript(theme), v, doc, theme),
            // The SAME span the preview's table cell emits, from the one builder — it
            // was a second copy, byte-identical except for `mark_fg`, and that single
            // difference is the whole mechanism behind `mark_fg` never reaching the
            // page (POLICY "One theme key, every application path").
            Inline::Highlight(v) => span(out, &crate::pangospan::mark(theme), v, doc, theme),
            Inline::Claim(idx, v) => {
                span(
                    out,
                    &crate::pangospan::annotation_claim(theme),
                    v,
                    doc,
                    theme,
                );
                // The comment as a margin note beside its claim — the in-file review
                // loop is the product thesis, and an export that drops the review is
                // the wrong document (TDD 25.13).
                if let Some(ann) = doc.annotations.get(*idx) {
                    // TDD 18.19: the chip's colour keys, applied to the note's
                    // background/ink. Empty unless the theme sets at least one, so a
                    // theme that sets neither renders this BYTE-IDENTICAL to before
                    // the chip could be themed at all — a sprite has no expression
                    // here (Pango markup carries no inline image), which is a scope
                    // limit stated once here rather than silently absent.
                    let mut chip_attrs = String::new();
                    if let Some(bg) = theme.annotation_chip_bg {
                        let _ = write!(
                            chip_attrs,
                            " background=\"{}\"",
                            crate::palette::to_hex_rgba(bg)
                        );
                    }
                    if let Some(fg) = theme.annotation_chip_fg {
                        let _ = write!(
                            chip_attrs,
                            " foreground=\"{}\"",
                            crate::palette::to_hex_rgba(fg)
                        );
                    }
                    let _ = std::fmt::Write::write_fmt(
                        out,
                        format_args!(
                            " <span size=\"small\" style=\"italic\"{chip_attrs}>[{}]</span>",
                            escape_pango(&ann.comment)
                        ),
                    );
                }
            }
            Inline::Link { href, inner, .. } => {
                // Underlined in the theme's link colour, and the destination shown
                // where it differs from the text — a printed page cannot be clicked,
                // so a bare link label loses the only thing it carried.
                // The ink, the underline STYLE and its own optional colour, from the
                // one builder (TDD 18.23).
                span(out, &crate::pangospan::link(theme), inner, doc, theme);
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

/// The Pango span that carries a theme's heading rule (TDD 18.22), as an
/// `(open, close)` pair. `("", "")` when the theme draws no rule, so the heading run a
/// PDF measures and inks is byte-identical to before the key existed (TDD 18.2).
///
/// Wrapping is the whole mechanism: the rule is a text decoration over the heading's
/// glyph run, so it is spelled as attributes on a span around that run and never as a
/// separate drawn line. `overline`/`overline_color` reached Pango markup in 1.46; this
/// project's GTK 4.6 floor requires 1.50, so the attribute is always understood — which
/// matters more than it looks, because an attribute Pango does not recognise fails the
/// whole `pango_parse_markup` and renders the run EMPTY, silently (ScrAP-163).
pub(super) fn heading_span(theme: &Theme, level_index: usize) -> (String, &'static str) {
    let rule = &theme.heading_rule;
    let mut attrs = String::new();
    // **The heading's own INK.** This sink carried four of the five heading
    // decorations — scale, weight, band, rule — and not the colour, so a Synthwave
    // export printed banded, ruled, correctly-scaled headings in body black. It is
    // also what makes SCHEMA's `blockquote_fg` row true here: a heading inside a quote
    // keeps its own colour only if it carries a foreground span, and without one it
    // took the quote's cairo pen while a LINK in the same quote kept its colour.
    //
    // Per level and already folded (`Theme::resolve`), so an unstated level carries
    // the bare `heading_color` and this arm indexes rather than re-deriving.
    if let Some(c) = theme.heading_colors[level_index] {
        let _ = write!(attrs, " foreground=\"{}\"", crate::palette::to_hex_rgba(c));
    }
    if rule.is_absent_at(level_index) {
        return finish_heading_span(attrs);
    }
    let (overline, underline) = (rule.overline[level_index], rule.underline[level_index]);
    if !overline.is_none() {
        // Pango's overline vocabulary is none/single, which `LineStyle::overline`
        // already clamped to; spell what it resolved to, not what the theme typed.
        let spelled = match overline.overline() {
            gtk::pango::Overline::Single => "single",
            _ => "none",
        };
        // No `overline_color`: the overline takes the run's own ink on every path,
        // because the preview cannot colour it (see `theme::HeadingRule`) and an
        // artefact that coloured it would be showing something the reader's preview
        // does not (TDD 25.3).
        let _ = write!(attrs, " overline=\"{spelled}\"");
    }
    if !underline.is_none() {
        let _ = write!(attrs, " underline=\"{}\"", underline.pango_markup());
        if let Some(c) = rule.underline_color[level_index] {
            let _ = write!(
                attrs,
                " underline_color=\"{}\"",
                crate::palette::to_hex_rgba(c)
            );
        }
    }
    finish_heading_span(attrs)
}

/// Close [`heading_span`]'s two exits onto one shape: no attributes means no span at
/// all, so a theme that states nothing about a heading emits the byte-identical markup
/// this sink always did (TDD 18.2).
/// The Pango span carrying a theme's `disclosure_fg` (TDD 18.51), as an
/// `(open, close)` pair. `("", "")` where the theme states none, so the summary label
/// a PDF measures and inks is byte-identical to before the key existed (TDD 18.2).
///
/// **A span around the run, never a cairo pen on the line — and that is what makes a
/// QUOTED summary come out right.** `ink::draw_page` sets `blockquote_fg` as the cairo
/// source for every line inside a quote, so a summary label in one would print in the
/// quote's ink; a Pango `foreground` attribute overrides that source for the run it
/// covers, which is the same answer the preview gets from `disclosure-ink` being
/// registered after `blockquote-ink`. Markup also nests, so an inner
/// `<span foreground=…>` still wins over this outer one — which matters not for the
/// label as it renders today (one plain run, `Block::Disclosure::summary`) but for
/// whatever inline markup it may later carry.
pub(super) fn disclosure_span(theme: &Theme) -> (String, &'static str) {
    let mut attrs = String::new();
    if let Some(c) = theme.disclosure_fg {
        let _ = write!(attrs, " foreground=\"{}\"", crate::palette::to_hex_rgba(c));
    }
    finish_heading_span(attrs)
}

fn finish_heading_span(attrs: String) -> (String, &'static str) {
    if attrs.is_empty() {
        (String::new(), "")
    } else {
        (format!("<span{attrs}>"), "</span>")
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

/// Emit `v` wrapped in a themed [`crate::pangospan::Span`].
///
/// The pair comes from ONE value, so the open and the close cannot be chosen
/// independently — which they could when each arm formatted its own tag name, and a
/// mismatched pair renders the whole run EMPTY with no warning (ScrAP-163).
fn span(
    out: &mut String,
    s: &crate::pangospan::Span,
    v: &[Inline],
    doc: &ExportDoc,
    theme: &Theme,
) {
    out.push_str(&s.open);
    emit_markup(v, doc, theme, out);
    out.push_str(s.close);
}

/// Escape for Pango markup.
///
/// Switching a run to markup makes **every** interpolated string an injection and
/// breakage surface — an un-escaped metacharacter renders the whole run EMPTY, with no
/// crash and no warning (ScrAP-163). So nothing reaches a markup string un-escaped.
pub(crate) fn escape_pango(s: &str) -> String {
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
    use super::{escape_pango, inline_markup};
    use crate::export::{doc, Block, RenderOptions};

    fn themed_fixture() -> (crate::export::ExportDoc, crate::theme::Theme) {
        let mut theme = crate::theme::Themes::builtin().resolve("system");
        theme.typography.bold_weight = 650;
        theme.typography.supsub_scale = 0.75;
        theme.typography.superscript_rise = 4;
        theme.typography.subscript_rise = -2;
        let d = doc::build(
            "A **bold** word, H~2~O, and x^2^.\n",
            &RenderOptions::default(),
        );
        (d, theme)
    }

    /// TDD 18.19 / 25.13: an annotated claim's `[comment]` note carries the chip's
    /// colour keys when set. Sprites have no expression here — Pango markup carries
    /// no inline image — so this covers only the colour half by design.
    #[test]
    fn a_claims_comment_note_carries_the_themed_chip_colours() {
        let mut theme = crate::theme::Themes::builtin().resolve("system");
        theme.annotation_chip_bg = Some(gtk::gdk::RGBA::new(0.1, 0.2, 0.3, 1.0));
        theme.annotation_chip_fg = Some(gtk::gdk::RGBA::WHITE);
        let d = doc::build("{==claim==}{>>my note<<}\n", &RenderOptions::default());
        let Some(Block::Paragraph(inlines)) = d.blocks.first() else {
            panic!("expected one paragraph: {:?}", d.blocks);
        };
        let out = inline_markup(inlines, &d, &theme);
        assert!(out.contains("background=\"#1a334d\""), "{out}");
        assert!(out.contains("foreground=\"#ffffff\""), "{out}");
        assert!(out.contains("[my note]"), "{out}");
    }

    /// TDD 18.18: the PDF/HTML export markup sink is the THIRD representation of
    /// `bold_weight`/`supsub_scale` (body tag, table cell, this one) — resolved
    /// against the sink's own EXPLICIT `Theme`, never `crate::theme::active()`.
    #[test]
    fn bold_and_supsub_carry_the_themed_attributes_into_export_markup() {
        let (d, theme) = themed_fixture();
        let Some(Block::Paragraph(inlines)) = d.blocks.first() else {
            panic!("expected one paragraph: {:?}", d.blocks);
        };
        let out = inline_markup(inlines, &d, &theme);
        assert!(out.contains("weight=\"650\""), "{out}");
        assert!(out.contains("size=\"75%\""), "{out}");
        // Rise, in Pango units — same `value * pango::SCALE` the body tag and the
        // table cell both use, so the number here is comparable across all three.
        assert!(
            out.contains(&format!("rise=\"{}\"", 4 * gtk::pango::SCALE)),
            "superscript rise: {out}"
        );
        assert!(
            out.contains(&format!("rise=\"{}\"", -2 * gtk::pango::SCALE)),
            "subscript rise: {out}"
        );
    }

    /// TDD 18.22 / 25.3 — the heading rule reaches the PDF's Pango markup, and the
    /// markup it produces actually PARSES.
    ///
    /// The parse half is the load-bearing one. Pango rejects an attribute it does not
    /// know by failing `pango_parse_markup` outright, and this sink's answer to a failed
    /// parse is a run rendered EMPTY, silently (ScrAP-163) — so a spelling test that only
    /// compares strings would pass on markup that blanks every heading in the document.
    #[test]
    fn the_heading_rule_reaches_export_markup_and_the_markup_parses() {
        let mut theme = crate::theme::Themes::builtin().resolve("system");
        // Absent by default: a theme with no rule wraps nothing at all.
        let (open, close) = super::heading_span(&theme, 0);
        assert!(open.is_empty() && close.is_empty());

        theme.heading_rule.overline[0] = crate::theme::LineStyle::Single;
        theme.heading_rule.underline[0] = crate::theme::LineStyle::Wavy;
        theme.heading_rule.underline_color[0] = Some(gtk::gdk::RGBA::new(0.0, 0.0, 1.0, 1.0));
        let (open, close) = super::heading_span(&theme, 0);
        assert!(open.contains("overline=\"single\""), "{open}");
        // …and NEVER an `overline_color`: see `theme::HeadingRule`.
        assert!(!open.contains("overline_color"), "{open}");
        // `wavy` is Pango's ERROR underline — the theme's word, Pango's spelling.
        assert!(open.contains("underline=\"error\""), "{open}");
        assert!(open.contains("underline_color=\"#0000ff\""), "{open}");
        gtk::pango::parse_markup(&format!("{open}Heading{close}"), '\0')
            .expect("the heading-rule span must parse as Pango markup");
    }

    /// Every spelling in the vocabulary parses, not just the one a shipped theme uses —
    /// a value only reachable from a user file is exactly the one nobody renders before
    /// shipping, and its penalty here is a blank heading rather than an error.
    #[test]
    fn every_heading_rule_spelling_parses_as_pango_markup() {
        use crate::theme::LineStyle::{Double, None as NoLine, Single, Wavy};
        let mut theme = crate::theme::Themes::builtin().resolve("system");
        for over in [NoLine, Single, Double, Wavy] {
            for under in [NoLine, Single, Double, Wavy] {
                theme.heading_rule.overline[0] = over;
                theme.heading_rule.underline[0] = under;
                theme.heading_rule.underline_color[0] =
                    Some(gtk::gdk::RGBA::new(0.4, 0.5, 0.6, 1.0));
                let (open, close) = super::heading_span(&theme, 0);
                gtk::pango::parse_markup(&format!("{open}H{close}"), '\0')
                    .unwrap_or_else(|e| panic!("{over:?}/{under:?} → {open:?} failed: {e}"));
            }
        }
    }

    /// TDD 18.23 / 25.3 — the strike colour and the link underline reach the PDF's
    /// markup, and the markup PARSES (an attribute Pango does not know renders the whole
    /// run EMPTY — ScrAP-163 — so spelling alone is not evidence).
    #[test]
    fn the_strike_and_link_underline_reach_export_markup_and_parse() {
        let mut theme = crate::theme::Themes::builtin().resolve("system");
        let d = doc::build(
            "~~gone~~ and [a link](https://example.com/x)\n",
            &RenderOptions::default(),
        );
        let Some(Block::Paragraph(inlines)) = d.blocks.first() else {
            panic!("expected one paragraph: {:?}", d.blocks);
        };

        // Unset ⇒ byte-identical to what this sink always emitted.
        let plain = inline_markup(inlines, &d, &theme);
        assert!(plain.contains("<s>gone</s>"), "{plain}");
        assert!(plain.contains("underline=\"single\""), "{plain}");
        assert!(!plain.contains("strikethrough_color"), "{plain}");
        assert!(!plain.contains("underline_color"), "{plain}");

        theme.strikethrough_color = Some(gtk::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0));
        theme.link_underline = crate::theme::LineStyle::Wavy;
        theme.link_underline_color = Some(gtk::gdk::RGBA::new(0.0, 1.0, 0.0, 1.0));
        let themed = inline_markup(inlines, &d, &theme);
        assert!(
            themed.contains("strikethrough=\"true\" strikethrough_color=\"#ff0000\""),
            "{themed}"
        );
        assert!(themed.contains("underline=\"error\""), "{themed}");
        assert!(themed.contains("underline_color=\"#00ff00\""), "{themed}");
        gtk::pango::parse_markup(&themed, '\0').expect("themed inline markup must parse");
        gtk::pango::parse_markup(&plain, '\0').expect("plain inline markup must parse");
    }

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
