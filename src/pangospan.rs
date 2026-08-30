//! **Every themed Pango span this application emits, in one place.**
//!
//! Pango markup is the project's *third* representation of a themed inline style,
//! beside the preview's `GtkTextTag`s and the HTML sink's CSS — and it is reached from
//! two unrelated directions. A table cell is a `GtkLabel` outside the buffer, so no
//! `GtkTextTag` can style it (ScrAP-36/ScrAP-110) and the preview builds markup for it;
//! the PDF sink lays every run out through Pango and builds markup for that. Both were
//! building the same five spans, independently.
//!
//! # What that cost, measured rather than feared
//!
//! `renderer::mark_open` and `export::markup`'s `Inline::Highlight` arm were
//! byte-identical **except for `{fg}`** — the `mark_fg` projection. That single
//! difference is the whole mechanism behind `mark_fg` reaching the preview and the HTML
//! sink and never the page: not an oversight inside a shared function, but a second
//! copy nobody updated. `renderer::mod`'s own comment documented the reasoning for
//! adding `mark_fg` *to that copy*, with no reference to the twin one module over. The
//! other four pairs had not diverged, and that was luck rather than structure.
//!
//! There is a second axis, and it is why this module takes an explicit `&Theme`: the
//! renderer's copies resolved against the global `crate::theme::active()` while the
//! export copies took a `&Theme` (deliberately — the PDF resolves at System-light, TDD
//! 25.9). Two copies reading two different sources means a test on one says nothing
//! about the other, which is exactly what the parity test at the foot of this file
//! exists to close.
//!
//! # Why every span is a PAIR
//!
//! `strike` has to be, and the reason generalises: its plain form closes with `</s>`
//! and its themed form with `</span>`, so an open and a close chosen independently can
//! disagree — and a mismatched pair fails `pango_parse_markup`, which renders the whole
//! run **EMPTY**, with no warning (ScrAP-163). Any future themed variant of the other
//! four would acquire the same hazard the moment it changed a tag name. Handing out
//! both halves from one call makes that unrepresentable rather than remembered.

use crate::theme::Theme;
use std::fmt::Write;

/// One themed span: the opening tag, and the closing tag that pairs with it.
///
/// Deliberately not two functions. See the module header — the two halves are not
/// independent, and a mismatched pair renders the run empty rather than failing loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    pub open: String,
    pub close: &'static str,
}

impl Span {
    fn plain(open: &str, close: &'static str) -> Span {
        Span {
            open: open.to_string(),
            close,
        }
    }

    /// A `<span …>` with the given attribute text (which must begin with a space, or
    /// be empty).
    fn attrs(attrs: &str) -> Span {
        Span {
            open: format!("<span{attrs}>"),
            close: "</span>",
        }
    }

    /// Wrap `inner` — for a caller that has the whole run in hand.
    ///
    /// Test-only: production emitters push the two halves around a recursive walk
    /// (`export::markup::span`) rather than materialising the inner run first.
    #[cfg(test)]
    pub(crate) fn wrap(&self, inner: &str) -> String {
        format!("{}{inner}{}", self.open, self.close)
    }
}

/// A `==highlight==` (mark): its themed fill, and its themed ink where the theme
/// states one.
///
/// **Separate `bgalpha` rather than 8-digit hex**, for robust Pango compatibility;
/// `ThemeColor` owns that decomposition. The ink rides the SAME span as the fill,
/// because a cell is a `GtkLabel` outside the buffer and the body tag's foreground
/// cannot reach it — and is emitted only when the theme states one, so a theme without
/// the key produces the byte-identical span this path always did (TDD 18.2).
pub(crate) fn mark(t: &Theme) -> Span {
    let c = &t.mark_bg;
    let mut attrs = format!(" background=\"{}\" bgalpha=\"{}\"", c.hex(), c.alpha_pct());
    if let Some(fg) = t.mark_fg {
        let _ = write!(attrs, " foreground=\"{}\"", crate::palette::to_hex_rgba(fg));
    }
    Span::attrs(&attrs)
}

/// A CriticMarkup claim's highlight wash (TDD 18.5/18.6).
pub(crate) fn annotation_claim(t: &Theme) -> Span {
    let c = &t.annotation_hl_color;
    Span::attrs(&format!(
        " background=\"{}\" bgalpha=\"{}\"",
        c.hex(),
        c.alpha_pct()
    ))
}

/// `**bold**` at the theme's own weight (TDD 18.18).
pub(crate) fn bold(t: &Theme) -> Span {
    Span::attrs(&t.typography.bold_attr())
}

/// `~~strikethrough~~`, in the theme's strike colour where it states one (TDD 18.23).
///
/// The pair that proves why every span here is a pair: `("<s>", "</s>")` against
/// `("<span …>", "</span>")`.
pub(crate) fn strike(t: &Theme) -> Span {
    match t.strikethrough_color {
        None => Span::plain("<s>", "</s>"),
        Some(c) => Span::attrs(&format!(
            " strikethrough=\"true\" strikethrough_color=\"{}\"",
            crate::palette::to_hex_rgba(c)
        )),
    }
}

/// `^superscript^` at the theme's scale and rise (TDD 18.18).
pub(crate) fn superscript(t: &Theme) -> Span {
    Span::attrs(&t.typography.supsub_attr(true))
}

/// `~subscript~` at the theme's scale and rise (TDD 18.18).
pub(crate) fn subscript(t: &Theme) -> Span {
    Span::attrs(&t.typography.supsub_attr(false))
}

/// A link's ink and underline (TDD 18.23), and the destination the reader cannot click.
///
/// Only the PDF sink emits this today — a preview link is a `GtkTextTag` — but it lives
/// here because it reads the same three keys through the same grammar as its
/// neighbours, and a second Pango link span written elsewhere would be this module's
/// whole failure mode returning.
pub(crate) fn link(t: &Theme) -> Span {
    let mut attrs = format!(" underline=\"{}\"", t.link_underline.pango_markup());
    if let Some(c) = t.link_underline_color {
        let _ = write!(
            attrs,
            " underline_color=\"{}\"",
            crate::palette::to_hex_rgba(c)
        );
    }
    let colour = t
        .link_color
        .or(t.accent_color)
        .map(crate::palette::to_hex_rgba)
        .unwrap_or_default();
    if !colour.is_empty() {
        let _ = write!(attrs, " foreground=\"{colour}\"");
    }
    Span::attrs(&attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themed(spec: &str) -> Theme {
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(spec);
        themes.resolve("t")
    }

    /// **Every span parses as Pango markup, on a theme that states every key.**
    ///
    /// The pair is the hazard: an open and a close that disagree fail
    /// `pango_parse_markup` and render the run EMPTY with no warning (ScrAP-163), so a
    /// test that merely string-matched the open would pass on exactly the broken case.
    #[test]
    fn every_span_parses_as_markup_around_real_content() {
        let t = themed(
            "[themes.t]\nmark_bg = \"#fff59d_88\"\nmark_fg = \"#402000\"\n\
             strikethrough_color = \"#aa0000\"\nlink_color = \"#0055cc\"\n\
             link_underline = \"double\"\nlink_underline_color = \"#00aa55\"\n\
             annotation_hl_color = \"#FFD133_61\"\nbold_weight = 800\n",
        );
        for (name, span) in named_spans(&t) {
            let markup = span.wrap("content &amp; more");
            let parsed = gtk::pango::parse_markup(&markup, '\u{0}');
            assert!(
                parsed.is_ok(),
                "{name} does not parse as Pango markup: {markup:?}"
            );
            let (_attrs, text, _accel) = parsed.unwrap();
            assert_eq!(
                text, "content & more",
                "{name} must wrap its content, not replace it"
            );
        }
    }

    /// A theme that states NOTHING still produces parseable, inert spans — the
    /// byte-identical case (TDD 18.2), which is the one a themed variant is most
    /// likely to break by assuming its key is present.
    #[test]
    fn an_unstated_theme_still_produces_parseable_spans() {
        let t = crate::theme::Themes::builtin().resolve(crate::theme::SYSTEM_ID);
        for (name, span) in named_spans(&t) {
            let markup = span.wrap("x");
            assert!(
                gtk::pango::parse_markup(&markup, '\u{0}').is_ok(),
                "{name} does not parse on an unstated theme: {markup:?}"
            );
        }
        // The strike pair is the one that changes SHAPE between the two cases.
        assert_eq!(strike(&t), Span::plain("<s>", "</s>"));
    }

    fn named_spans(t: &Theme) -> Vec<(&'static str, Span)> {
        vec![
            ("mark", mark(t)),
            ("annotation_claim", annotation_claim(t)),
            ("bold", bold(t)),
            ("strike", strike(t)),
            ("superscript", superscript(t)),
            ("subscript", subscript(t)),
            ("link", link(t)),
        ]
    }

    /// **The preview's wrappers and the export sink emit the SAME span for the same
    /// theme.**
    ///
    /// This is the assertion that could not exist while there were two copies: they
    /// read two different sources, so a test on one said nothing about the other. There
    /// is no method to ban here, so the enforcement ladder stops at the test rung — and
    /// this is that rung.
    ///
    /// **No `activate_for_test` any more.** The preview wrappers used to read the
    /// process-global active theme, so proving the parity meant installing one; both
    /// sides now take an explicit `&Theme` (F-BUILDPRODUCTS-001), which is a strictly
    /// stronger test — it can no longer pass because both sides happened to read the
    /// same global.
    #[test]
    fn the_preview_wrappers_and_the_export_sink_emit_one_span() {
        let t = themed(
            "[themes.t]\nmark_bg = \"#fff59d_88\"\nmark_fg = \"#402000\"\n\
             strikethrough_color = \"#aa0000\"\nannotation_hl_color = \"#FFD133_61\"\n\
             bold_weight = 800\nsupsub_scale = 0.6\nsuperscript_rise = 9\n",
        );
        assert_eq!(crate::renderer::mark_open(&t), mark(&t).open);
        assert_eq!(crate::renderer::ann_hl_open(&t), annotation_claim(&t).open);
        assert_eq!(crate::renderer::bold_open(&t), bold(&t).open);
        assert_eq!(crate::renderer::superscript_open(&t), superscript(&t).open);
        assert_eq!(crate::renderer::subscript_open(&t), subscript(&t).open);
        let (open, close) = crate::renderer::strike_tags(&t);
        assert_eq!((open, close), (strike(&t).open, strike(&t).close));
    }
}
