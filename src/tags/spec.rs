//! What each `GtkTextTag` SAYS, decided from the theme alone — display-free.
//!
//! `setup_tags_with_theme` is a 365-line monolith that needs a live `GtkTextBuffer`, so
//! every theme→tag decision in it could only be asserted by standing up a view
//! (F-TAGS-001). The decisions are the part that can be *wrong*: the quote-ink floor's
//! condition, the band inset's per-level gate, and the arithmetic that turns a
//! design-time metric into a pixel at a zoom. They live here as pure values, and the
//! GTK half is a thin applier — the same split `codeview::gutter` already uses, which is
//! why that is the best-tested file in this area.
//!
//! **A spec is not every property.** Only what a theme can move: an unconditional
//! `set_scale` is arithmetic with one answer, while `Option`-shaped fields carry the
//! "set it only when the theme asks" rule that TDD 18.2 is about — calling a setter with
//! `None` still marks the property SET, which is a different tag from the one the preview
//! registered before the key existed.

use crate::palette::Palette;
use crate::theme::{px, CssSafeFontStack, LineStyle, Theme};
use gtk::gdk;

/// The ink a construct falls back to **because the quote panel would otherwise claim
/// it** — `None` when the theme states no `blockquote_fg`.
///
/// SCHEMA's `blockquote_fg` row exempts three constructs from the quote's re-inking: a
/// link, a heading and a `==mark==`. The link tag delivers that by setting its foreground
/// unconditionally; the other two set theirs only where the theme states one, so a theme
/// stating `blockquote_fg` and nothing else left NO tag above `blockquote-ink` on those
/// runs and the quote re-inked two of the three.
///
/// A FLOOR rather than an unconditional set, because two contracts pull against each
/// other and both are real: TDD 18.2 says a theme that states nothing leaves System's
/// tags byte-identical — not merely its pixels — so those tags must go on setting no
/// foreground when nothing asks. They only need one when something else is about to
/// claim the run, which is exactly when a quote ink is stated.
pub(super) fn ink_floor(theme: &Theme, palette: &Palette) -> Option<gdk::RGBA> {
    theme.blockquote_fg.map(|_| palette.body_fg)
}

/// The margins a banded heading's TEXT takes, in device pixels at `zoom`.
///
/// `None` for a level the theme does not band, and that conditionality — not the metric's
/// value — is what keeps System byte-identical: an unconditional heading margin would
/// re-indent every heading in every theme, System's included.
///
/// The band's RECT stays the full content column (that extent is what both export sinks
/// match against), so the text moves in through the tag's own margins — the identical
/// lever `code-block` and `blockquote` already use to sit their text inside the
/// decoration drawn behind it (GTK4Rs/AP-21: a drawn rect and a tag margin are the only
/// two halves available, and the rect cannot pad).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BandInset {
    pub(super) left: i32,
    pub(super) right: i32,
}

/// Everything one heading level's tag carries that a theme can move.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct HeadingSpec {
    pub(super) scale: f64,
    pub(super) weight: i32,
    /// Device pixels at this render's zoom, not the theme's design-time value.
    pub(super) space_above: i32,
    pub(super) space_below: i32,
    /// `None` for an unbanded level — see [`BandInset`].
    pub(super) band_inset: Option<BandInset>,
    pub(super) overline: LineStyle,
    pub(super) underline: LineStyle,
    /// Only ever consulted when `underline` is not `None`. The OVERLINE deliberately has
    /// no colour of its own: a run carrying a coloured overline AND a coloured underline
    /// double-frees inside GTK 4.6, and a link inside a heading is exactly such a run.
    pub(super) underline_color: Option<gdk::RGBA>,
    /// The level's own ink, already folded with the theme's bare `heading_color` by
    /// `Theme::resolve`, then floored against [`ink_floor`].
    pub(super) foreground: Option<gdk::RGBA>,
    pub(super) family: Option<CssSafeFontStack>,
}

/// Decide one heading level's tag.
///
/// `view_margins` is the view's own left/right margin in device pixels — the base a
/// band's padding is added to, passed in rather than read from the config so this stays
/// a function of its arguments.
pub(super) fn heading_spec(
    theme: &Theme,
    palette: &Palette,
    zoom: f64,
    level: usize,
    view_margins: (i32, i32),
) -> HeadingSpec {
    let (view_left, view_right) = view_margins;
    let metrics = &theme.metrics;
    let rule = &theme.heading_rule;
    // The engine's own gate, not a second reading of the fill key: a level banded only
    // by a SPRITE still needs its text inset by the band's padding, and the two used to
    // disagree about that.
    let band_inset = theme.heading_band_decor(level).is_present().then(|| {
        let pad = px(metrics.heading_band_padding[level], zoom);
        BandInset {
            left: view_left + pad,
            right: view_right + pad,
        }
    });
    HeadingSpec {
        scale: theme.typography.heading_scale[level],
        weight: theme.typography.heading_weight[level],
        space_above: px(metrics.heading_space_above[level], zoom),
        space_below: px(metrics.heading_space_below[level], zoom),
        band_inset,
        overline: rule.overline[level],
        underline: rule.underline[level],
        underline_color: rule.underline_color[level],
        foreground: theme.heading_colors[level].or(ink_floor(theme, palette)),
        family: theme.heading_fonts[level].clone(),
    }
}

/// The left indent (device px at `zoom`) the `li-{depth}` tag applies for a list item at
/// `depth`, and the per-side indent the `bq-{depth}` tag applies for a blockquote at
/// `depth`.
///
/// They are separate functions because **the two multiply by depth on opposite sides of
/// the rounding**, and that asymmetry is the whole reason this pair exists rather than one
/// helper with a flag. `px` rounds, so `px(a + b) != px(a) + px(b)` and
/// `n * px(a) != px(n * a)`; a caller that scales a summed metric once lands up to a pixel
/// short per term. `codeview::gutter::list_content_margin_px` already carried that warning
/// for the list half ("do NOT fold this into one `round(base + depth*STEP*zoom)`") — the
/// quote half had no such home, so `Renderer::block_inset` computed it its own way and
/// drifted from the tag by exactly the rounding (a table inside a quote then overflowed
/// the viewport by 1px at zoom 1.5, summoning the Automatic h-scrollbar and re-arming the
/// GTK4Rs/AP-22/23 churn — ScrAP-23a's failure through a new door).
///
/// So this is the single supplier POLICY's "One theme key, every application path" asks
/// for, at the ARITHMETIC rather than at the key: every path that needs to know how far a
/// block's tag pushes its text reads it here.
pub(crate) fn list_indent_px(depth: i32, zoom: f64, m: &crate::theme::Metrics) -> i32 {
    // Depth multiplied BEFORE the scale — one accumulative tag carrying `depth * step`.
    px(depth * m.list_step, zoom)
}

/// See [`list_indent_px`] — the quote's per-side indent, depth multiplied AFTER the scale.
///
/// One `bq-{depth}` tag carries its level's FULL indent on both sides, built from a
/// per-level step that is itself rounded (`tags.rs`: `bq_step = px(bar + gap)`, then
/// `bq_step * depth`). A blockquote therefore narrows the usable column by TWICE this.
pub(crate) fn quote_indent_px(depth: i32, zoom: f64, m: &crate::theme::Metrics) -> i32 {
    px(m.blockquote_bar_width + m.blockquote_text_gap, zoom) * depth
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two indents round at DIFFERENT steps, and that is the contract — not an
    /// implementation detail. A caller that scales a summed metric once (what
    /// `Renderer::block_inset` used to do) disagrees with the tags by the rounding, and
    /// under-reserves the anchored-child bound by up to a pixel per level.
    ///
    /// Default metrics: `list_step = 28`, `bar + gap = 3 + 10 = 13`. At zoom 1.5 the
    /// quote step is `round(19.5) = 20` per level, so a depth-1 quote costs 40 across the
    /// pair, while a single scaling of the summed pair gives `round(26 * 1.5) = 39`.
    /// That 1px is the whole defect.
    #[test]
    fn the_two_indents_round_where_their_tags_round() {
        let theme = themed("background = \"#ffffff\"\nforeground = \"#000000\"\n");
        let m = &theme.metrics;
        assert_eq!(m.list_step, 28, "fixture assumption");
        assert_eq!(
            m.blockquote_bar_width + m.blockquote_text_gap,
            13,
            "fixture assumption"
        );

        // Quote: depth multiplied AFTER the scale.
        assert_eq!(quote_indent_px(1, 1.5, m), 20);
        assert_eq!(quote_indent_px(2, 1.5, m), 40);
        assert_ne!(
            2 * quote_indent_px(1, 1.5, m),
            crate::theme::px(2 * (m.blockquote_bar_width + m.blockquote_text_gap), 1.5),
            "scaling the summed pair once must NOT equal the tags' arithmetic — if these \
             ever agree the regression this guards is unreachable and the test is vacuous"
        );

        // List: depth multiplied BEFORE the scale.
        assert_eq!(list_indent_px(2, 1.5, m), crate::theme::px(56, 1.5));

        // Zoom 1.0 is the case that hides the defect: everything is integral, so both
        // spellings agree and only a non-integral zoom can discriminate.
        assert_eq!(quote_indent_px(2, 1.0, m), 26);
    }

    use crate::theme::Themes;

    fn palette_for(theme: &Theme) -> Palette {
        Palette::for_paper(theme)
    }

    fn themed(fragment: &str) -> Theme {
        let mut themes = Themes::builtin();
        themes.merge_over_for_test(&format!("[themes.probe]\n{fragment}"));
        themes.resolve("probe")
    }

    /// **The quote-ink floor exists only when a quote ink does** — the condition, not
    /// the colour, is the whole decision.
    ///
    /// Unconditionally flooring would also satisfy SCHEMA's exemption and would silently
    /// make every theme's heading tag a different tag from the one the preview registered
    /// before any of this existed (TDD 18.2 is a claim about the TAG).
    #[test]
    fn the_ink_floor_appears_only_when_the_theme_states_a_quote_ink() {
        let bare = themed("background = \"#ffffff\"\nforeground = \"#000000\"\n");
        assert_eq!(ink_floor(&bare, &palette_for(&bare)), None);

        let quoted = themed(
            "background = \"#ffffff\"\nforeground = \"#123456\"\nblockquote_fg = \"#ffffff\"\n",
        );
        let p = palette_for(&quoted);
        assert_eq!(
            ink_floor(&quoted, &p),
            Some(p.body_fg),
            "the floor is the resolved BODY ink — what the run would have inherited \
             from the page if the quote's ink were not underneath it"
        );
    }

    /// A heading's foreground takes the floor only where the LEVEL states no ink of its
    /// own, and a stated level ink outranks it.
    #[test]
    fn a_levels_own_ink_outranks_the_quote_ink_floor() {
        let t = themed(
            "background = \"#ffffff\"\nforeground = \"#000000\"\n\
             blockquote_fg = \"#ffffff\"\nheading_color_h1 = \"#ff0000\"\n",
        );
        let p = palette_for(&t);
        let h1 = heading_spec(&t, &p, 1.0, 0, (10, 10));
        let h2 = heading_spec(&t, &p, 1.0, 1, (10, 10));
        assert_eq!(
            h1.foreground.map(crate::palette::to_hex_opaque),
            Some("#ff0000".to_string())
        );
        assert_eq!(
            h2.foreground,
            Some(p.body_fg),
            "a level stating no ink takes the floor, not the quote's ink"
        );
    }

    /// **Only a BANDED level is inset**, and a level banded by a sprite ALONE is banded.
    ///
    /// The gate is the engine's `is_present()` rather than a second reading of the fill
    /// key, because those two disagreed: a sprite-only band left its text flush against
    /// the band's edge while the band itself drew.
    #[test]
    fn only_a_banded_level_is_inset_and_a_sprite_alone_counts_as_banded() {
        let t = themed(
            "background = \"#ffffff\"\nforeground = \"#000000\"\n\
             heading_band_color_h1 = \"#334455\"\nheading_band_padding = 12\n",
        );
        let p = palette_for(&t);
        assert_eq!(
            heading_spec(&t, &p, 1.0, 0, (24, 24)).band_inset,
            Some(BandInset {
                left: 36,
                right: 36
            })
        );
        assert_eq!(
            heading_spec(&t, &p, 1.0, 1, (24, 24)).band_inset,
            None,
            "an unbanded level must set NO margin — not the view's own value, which \
             would still be a different tag"
        );

        let sprited = {
            let mut t = themed("background = \"#ffffff\"\nforeground = \"#000000\"\n");
            t.metrics.heading_band_padding = [12; crate::theme::HEADING_LEVELS];
            t.sprites.heading_band[0] = Some(crate::sprite::SpriteRef::Compiled(
                "sprites/copper-plate.png",
            ));
            t
        };
        let p = palette_for(&sprited);
        assert_eq!(
            heading_spec(&sprited, &p, 1.0, 0, (24, 24)).band_inset,
            Some(BandInset {
                left: 36,
                right: 36
            }),
            "a level banded by a sprite ALONE still insets its text"
        );
    }

    /// The inset is in DEVICE pixels: the band's padding is a design-time value and the
    /// view's margin is already scaled, so only the padding is multiplied.
    #[test]
    fn the_band_inset_scales_the_padding_and_not_the_view_margin() {
        let t = themed(
            "background = \"#ffffff\"\nforeground = \"#000000\"\n\
             heading_band_color_h1 = \"#334455\"\nheading_band_padding = 14\n",
        );
        let p = palette_for(&t);
        // The caller passes the view's margin ALREADY at this zoom, so 48 stays 48 and
        // only the themed 14 becomes 28.
        assert_eq!(
            heading_spec(&t, &p, 2.0, 0, (48, 48)).band_inset,
            Some(BandInset {
                left: 76,
                right: 76
            })
        );
    }

    /// The CSS quoting the sanitiser adds is stripped for Pango, and nothing else about
    /// the stack is touched — the ORDER is the fallback chain.
    #[test]
    fn a_font_stack_reaches_pango_unquoted_and_in_order() {
        let stack = crate::theme::sanitize_font_family("Liberation Serif, Georgia, serif")
            .expect("a safe stack");
        assert_eq!(
            stack.pango_family(),
            "Liberation Serif, Georgia, serif",
            "a quoted multi-word family breaks Pango's own list parsing, so the whole \
             stack silently drops to the default sans"
        );
    }
}
