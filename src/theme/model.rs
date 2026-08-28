//! The **resolved** theme: the shape every consumer reads.
//!
//! A [`Theme`] is what links 1 and 2 of the resolution order produce — a value per
//! key, with every per-level and per-depth fallback already folded in, so a consumer
//! indexes rather than re-deriving the fold (POLICY "One theme key, every application
//! path"). Colours stay `Option` on purpose: `None` means "fall through to link 3",
//! the desktop probe, which only `palette` can perform at the GTK edge.
//!
//! Nothing here parses or decides anything; `resolve` fills these in and `spec` reads
//! the file. Keeping the shape apart from the filling is what lets a consumer read the
//! model without pulling the file format in behind it.

use super::keys::{BULLET_TIERS, HEADING_LEVELS};
use super::value::{CssSafeFontStack, LineStyle, MarkerGlyph};
use gtk::gdk;

/// Every list-marker glyph a theme may state, one per marker kind. Each is `None`
/// unless the theme set it AND [`MarkerGlyph::parse`] accepted it, so a rejected glyph
/// degrades to the drawn default — never a partial or broken marker.
///
/// The task marker gets TWO, because it has two states and they must stay tellable
/// apart; they resolve independently, so a theme may state either alone (a "tick or
/// nothing" look) as deliberately as it may state both.
///
/// The BULLET gets [`BULLET_TIERS`], by nesting depth (TDD 18.26) — already folded, so a
/// tier the theme left unset carries the next shallower tier's value and every consumer
/// indexes rather than re-deriving the fallback. Only the bullet: an ordered numeral at
/// depth 3 is still a numeral and a task box is still a box, so those stay single-valued.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ListGlyphs {
    pub bullet: [Option<MarkerGlyph>; BULLET_TIERS],
    pub ordered: Option<MarkerGlyph>,
    pub task: Option<MarkerGlyph>,
    pub task_checked: Option<MarkerGlyph>,
}

// ── the resolved theme ────────────────────────────────────────────────────────

/// A theme colour that carries its own alpha, resolved once and decomposed on
/// demand for whichever application path needs it. `annotation_hl` is the reason
/// this type exists: the tag path takes the RGBA directly, while a table cell is a
/// `GtkLabel` outside the buffer (ScrAP-36) and needs Pango markup with the alpha as
/// a separate attribute. One key, two decompositions — the generator owns the
/// split so the two paths cannot drift (TDD 18.6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThemeColor(pub(crate) gdk::RGBA);

impl ThemeColor {
    /// The whole colour, alpha included. Every other accessor here — and every
    /// caller — reads the wrapped value through this one rather than through the
    /// positional field, so the field has exactly one reader and the newtype can be
    /// reshaped without hunting `.0`s (project convention: destructure by name,
    /// never positionally).
    pub(crate) fn rgba(self) -> gdk::RGBA {
        self.0
    }
    /// `#rrggbb`, alpha dropped — for the paths that take colour and alpha apart.
    pub(crate) fn hex(self) -> String {
        crate::palette::to_hex_opaque(self.rgba())
    }
    /// Alpha as a Pango percentage attribute value, e.g. `38%`.
    pub(crate) fn alpha_pct(self) -> String {
        format!("{}%", (self.rgba().alpha() * 100.0).round() as i32)
    }
    /// The 16-bit-per-channel triple a `GtkLabel`'s Pango attribute list wants.
    pub(crate) fn u16_triple(self) -> (u16, u16, u16) {
        let ch = |x: f32| (x.clamp(0.0, 1.0) * 65535.0).round() as u16;
        let c = self.rgba();
        (ch(c.red()), ch(c.green()), ch(c.blue()))
    }
}

/// Typography — all Pango tag attributes, so all compose with zoom for free.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Typography {
    /// h1, h2, h3, h4, h5-and-deeper. FIVE entries, not six: `emit.rs` maps
    /// h6-and-deeper to the h5 tag before a tag is ever chosen, so no theme can
    /// differentiate h6 from h5 however it is keyed. Honest to the renderer — h6 is
    /// a deliberate fold-into-deepest on every surface.
    pub heading_scale: [f64; HEADING_LEVELS],
    pub heading_weight: [i32; HEADING_LEVELS],
    pub bold_weight: i32,
    pub supsub_scale: f64,
    /// Points, converted to Pango units at apply time.
    pub superscript_rise: i32,
    pub subscript_rise: i32,
}

impl Typography {
    /// Pango markup attribute fragment for themed bold — e.g. ` weight="600"`, leading
    /// space included so a caller can splice it straight into a `<span…>` open tag.
    ///
    /// Shared by every representation OUTSIDE the buffer (table-cell `GtkLabel`
    /// markup, PDF/HTML export markup) so `bold_weight` cannot silently apply on the
    /// body `GtkTextTag` alone and drift from a bold word in a table or an exported
    /// document (TDD 18.18 — the prerequisite the whole decoration plan is gated on,
    /// `sdd/PLAN.preview-decoration.md` constraint 1).
    pub(crate) fn bold_attr(&self) -> String {
        format!(" weight=\"{}\"", self.bold_weight)
    }

    /// Pango markup attribute fragment for themed super/subscript — `size` AND
    /// `rise` together, the same two properties `tags.rs` applies to the body tag via
    /// `set_scale`/`set_rise`. `superscript` selects which rise; `subscript_rise`'s
    /// own floor is already negative (`F_SUBSCRIPT_RISE = -2`), so no sign flip
    /// happens here — the raw theme value is exactly what `tags.rs` feeds `set_rise`,
    /// just re-expressed as Pango markup's `rise` (also Pango units) instead of the
    /// tag property.
    pub(crate) fn supsub_attr(&self, superscript: bool) -> String {
        let pct = (self.supsub_scale * 100.0).round().max(1.0) as i32;
        let rise = if superscript {
            self.superscript_rise
        } else {
            self.subscript_rise
        };
        format!(" size=\"{pct}%\" rise=\"{}\"", rise * gtk::pango::SCALE)
    }
}

/// The optional rule a theme may draw above and/or below a heading's text (TDD 18.22).
///
/// Both sides default to [`LineStyle::None`], so a theme that states neither leaves the
/// heading tags byte-identical to before this decoration existed (TDD 18.2). A colour
/// left `None` means "do not set the property", which is how a `GtkTextTag` line follows
/// the run's own foreground — NOT a derived default we would have to keep in step.
///
/// # Why only ONE of the two sides carries a colour
///
/// **GTK 4.6.9 double-frees a text run that carries a coloured overline AND a coloured
/// underline.** MEASURED here, minimal: build a `GtkTextTag`, set `overline-rgba` and
/// `underline-rgba`, drop it — valgrind reports `Invalid free()` of a 16-byte block (a
/// `GdkRGBA`) freed twice inside GTK's own finalize path, and a few repetitions poison
/// the heap until an unrelated `gtk::Box::new` SIGSEGVs somewhere else entirely.
/// Characterised against a positive control: either colour ALONE avoids the invalid
/// free; the same colour on both still corrupts; every other `*-rgba` pair (foreground,
/// strikethrough) is clean; and **splitting the two across two tags applied to the same
/// range does not escape it** — the invariant is per RUN, not per tag. That last part is
/// what decides the design: a link inside a heading carries the heading's tag and the
/// link's tag at once, and the link tag colours an underline.
///
/// **ROOT CAUSE, confirmed against `gtk 4.6.9-5-g492b44f20c` source
/// (`gtk/gtktextattributes.c`, `gtk_text_attributes_unref`) — NOT an aliasing bug, a
/// one-line copy-paste typo in the destructor**:
/// ```c
/// if (values->appearance.underline_rgba)
///     gdk_rgba_free (values->appearance.underline_rgba);
/// if (values->appearance.overline_rgba)
///     gdk_rgba_free (values->appearance.underline_rgba);   /* guard says overline, free says underline */
/// ```
/// Every copy path (`copy_values`, the run-merge in `_gtk_text_attributes_fill_from_tags`)
/// deep-copies each field independently — no pointer is ever shared. The corruption is
/// this one destructor: with both fields set, `underline_rgba` is freed twice and
/// `overline_rgba` leaks; with only `overline_rgba` set, nothing is double-freed but that
/// 16 B still leaks on every tag/attributes destruction, which is why the theme vocabulary
/// has no `heading_overline_rgba` key at all rather than merely warning against combining
/// it with `heading_underline_rgba`. Fixed upstream by commit
/// `86e962929bf2be13a721053141b33e4381f0312` ("gtktextattributes: Make sure to free the
/// right color", found by Coverity CID 1621077, GitLab MR !8137) in GTK **4.16.13** and
/// **4.18.0**; never backported to any earlier stable branch, so nothing short of raising
/// this project's floor past 4.16.13 makes the key safe.
///
/// So the project sets `underline-rgba` and **never** `overline-rgba`, anywhere — a
/// `clippy.toml` ban makes that a build failure rather than a thing to remember. The
/// overline is expressible, and takes the heading's ink; the theme vocabulary simply has
/// no key that could ask for the combination GTK cannot survive. `paragraph-background-rgba`
/// is unaffected (its guard/free pair is correctly matched at every call site audited) and
/// safe to combine with anything, including this pair, for a future heading band (TDD
/// 18.25). When the toolkit floor moves past 4.16.13, `heading_overline_rgba` can be added
/// and nothing else changes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct HeadingRule {
    /// The rule above the text, per level. Always drawn in the heading's own ink —
    /// see the type docs; there is no colour key for this side at any level, and
    /// adding one is a heap bug.
    pub overline: [LineStyle; HEADING_LEVELS],
    pub underline: [LineStyle; HEADING_LEVELS],
    pub underline_color: [Option<gdk::RGBA>; HEADING_LEVELS],
}

impl HeadingRule {
    /// Whether the heading at `level` carries a rule — the one gate every consumer
    /// asks before emitting one, so "absent" is a single decision rather than four.
    ///
    /// Per level rather than per theme: each side is stated per level (TDD 18.32), so
    /// a theme that rules its h1 alone must not make every other level emit an empty
    /// decoration.
    ///
    /// `level` is a **slot**, and [`crate::theme::heading_slot`] is its only legal
    /// producer — it is the one definition of the h6→h5 fold and clamps to
    /// `HEADING_LEVELS - 1`, so every per-level array in this module is indexed
    /// in range by construction rather than by a bounds check here. That is the
    /// contract for every `[…; HEADING_LEVELS]` field below, not just this one.
    pub(crate) fn is_absent_at(&self, level: usize) -> bool {
        self.overline[level].is_none() && self.underline[level].is_none()
    }
}

/// The band drawn behind a heading's text (TDD 18.25) — the plan's marquee decoration,
/// and the first entry in the vocabulary that is a genuinely NEW drawn thing rather than
/// a property of something already painted.
///
/// Absent by default on every level: `fills` is all-`None` until a theme states one, so a
/// theme that says nothing leaves the paint path byte-identical to before the decoration
/// existed. `Theme::bands_nothing` is the one gate every consumer asks, so "no band" is
/// one decision
/// rather than five.
///
/// **The band spans the CONTENT COLUMN**, the same extent the code-block card uses — not
/// the text column a `paragraph_background_rgba` tag would pin it to, and not the widget
/// edge. Two reasons, and the second is the load-bearing one: a tag band follows the
/// *tag's* margins, so a heading inside a blockquote or a list item would carry a band of
/// a different width from its siblings; and the content column is the one extent all
/// three renderings can agree on (the HTML sink's `<h1>` fills its own column, the PDF's
/// printable width is its column), which is what keeps 25.3's "as the preview shows it"
/// true rather than nearly true.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct HeadingBand {
    /// Per level, h1 · h2 · h3 · h4 · h5-and-deeper. `None` ⇒ that level carries no band.
    pub fills: [Option<gdk::RGBA>; HEADING_LEVELS],
    /// A second stop, making the band a vertical gradient from the level's fill.
    pub gradient_to: [Option<gdk::RGBA>; HEADING_LEVELS],
}

/// Decoration metrics: design-time px at zoom 1.0. Every consumer scales these
/// through the existing `px(n) = round(n * zoom)` path; a theme never expresses
/// pixels at the current zoom.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Metrics {
    pub heading_space_below: [i32; HEADING_LEVELS],
    /// Space above each heading. Zero on every level until a theme says otherwise, so
    /// the heading tag's `pixels_above_lines` stays at the view default (TDD 18.2).
    pub heading_space_above: [i32; HEADING_LEVELS],
    /// Corner radius of the heading band, per level. Only consulted where a band
    /// exists.
    pub heading_band_radius: [i32; HEADING_LEVELS],
    /// The band's internal horizontal padding: the heading TEXT is inset from the band's
    /// edge by this much on each side, while the band itself keeps the content column it
    /// shares with both export sinks. Only consulted where a band exists.
    pub heading_band_padding: [i32; HEADING_LEVELS],
    pub blockquote_bar_width: i32,
    pub blockquote_text_gap: i32,
    /// The ONE definition both the `li-{depth}` tag's `left_margin` and the drawn
    /// marker gutter's x read. A value that reached one but not the other would
    /// strand every list marker — GTK4Rs/AP-96.
    pub list_step: i32,
    pub list_item_gap: i32,
    pub rule_space: i32,
    pub table_cell_padding_v: i32,
    pub table_cell_padding_h: i32,
    pub table_border_width: i32,
    pub table_cell_radius: i32,
}

/// A theme with links 1 and 2 of the resolution order already applied. Colours
/// stay `Option` on purpose: `None` means "fall through to link 3", the desktop
/// GTK probe + derivation, which only `palette` (at the GTK edge) can perform.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub id: String,
    pub name: String,
    /// Optional picker symbol (emoji); `None` ⇒ just the name.
    pub symbol: Option<String>,
    pub background: Option<gdk::RGBA>,
    pub foreground: Option<gdk::RGBA>,
    pub accent_color: Option<gdk::RGBA>,
    /// The body font stack. A [`CssSafeFontStack`], so the type itself proves it was
    /// sanitised and generic-terminated — safe to interpolate into CSS. `None` ⇒
    /// fall through to the system font.
    pub font_family: Option<CssSafeFontStack>,
    pub syntect_theme: Option<String>,
    /// Heading foreground; `None` ⇒ inherit the body foreground.
    pub heading_color: Option<gdk::RGBA>,
    /// Heading font family (a [`CssSafeFontStack`], sanitised + generic-terminated by
    /// construction); `None` ⇒ headings use the body font.
    pub heading_font: Option<CssSafeFontStack>,
    /// Per-level heading colours (h1 · h2 · h3 · h4 · h5-and-deeper), already folded
    /// with the singular `heading_color`: a slot the theme left unset carries that
    /// fallback here, so every consumer indexes and no consumer re-implements the
    /// fold. `None` in a slot still means "inherit the body foreground".
    pub heading_colors: [Option<gdk::RGBA>; HEADING_LEVELS],
    /// Per-level heading font stacks, folded with `heading_font` the same way.
    pub heading_fonts: [Option<CssSafeFontStack>; HEADING_LEVELS],
    /// The rule drawn above and/or below a heading; absent unless a theme asks for it.
    pub heading_rule: HeadingRule,
    /// The band behind a heading's text; absent unless a theme states a fill for a level.
    pub heading_band: HeadingBand,
    pub link_color: Option<gdk::RGBA>,
    /// A link's underline style; `LineStyle::Single` unless a theme says otherwise,
    /// which is the line the app has always drawn.
    pub link_underline: LineStyle,
    /// The link underline's colour; `None` ⇒ it follows the link's own ink.
    pub link_underline_color: Option<gdk::RGBA>,
    /// The strike line's colour; `None` ⇒ it follows the struck text's own foreground.
    pub strikethrough_color: Option<gdk::RGBA>,
    pub code_inline_bg: Option<gdk::RGBA>,
    pub code_block_bg: Option<gdk::RGBA>,
    pub blockquote_bar_color: Option<gdk::RGBA>,
    /// The quote panel's fill and the ink on it (TDD 18.29); each `None` ⇒ that half is
    /// absent and quoted text keeps the page background / the body foreground, exactly
    /// as before these keys existed. Independent of `blockquote_bar` in both directions.
    pub blockquote_bg: Option<gdk::RGBA>,
    pub blockquote_fg: Option<gdk::RGBA>,
    pub selection_bg: Option<gdk::RGBA>,
    /// Selected-text ink; `None` ⇒ `palette` derives it from the page and its ink.
    pub selection_fg: Option<gdk::RGBA>,
    pub table_border_color: Option<gdk::RGBA>,
    pub table_head_bg: Option<gdk::RGBA>,
    /// The table header row's ink (TDD 18.30), already folded with `heading_color` — so
    /// this slot IS `heading_color` unless the theme says otherwise, and a consumer
    /// indexes rather than re-deriving the fallback (the same discipline
    /// `heading_colors` and `list_task_color` follow). `None` ⇒ the header text inherits
    /// the body foreground, which is what a theme stating neither key always did.
    pub table_head_fg: Option<gdk::RGBA>,
    pub rule_color: Option<gdk::RGBA>,
    /// List-marker glyph colour (bullet/numeral/checkbox); `None` ⇒ inherit the widget
    /// foreground. Marker glyph only — never the item text.
    pub list_marker_color: Option<gdk::RGBA>,
    /// The BULLET's colour by nesting-depth tier (TDD 18.26), already folded with
    /// `list_marker` — so slot 0 IS `list_marker` unless a theme says otherwise, and a
    /// consumer indexes rather than re-deriving the fallback. Bullet only: the ordered
    /// numeral and the task box read `list_marker` at every depth.
    pub list_bullet_colors: [Option<gdk::RGBA>; BULLET_TIERS],
    /// The TASK checkbox's colour, both states (TDD 18.27), already folded with
    /// `list_marker` — so a theme that states neither leaves this `None` and the marker
    /// takes the widget foreground, exactly as before.
    pub list_task_color: Option<gdk::RGBA>,
    /// Glyphs standing in for the drawn list markers; each `None` ⇒ that marker is
    /// drawn as it always was. A sprite for the same marker outranks the glyph.
    pub list_glyphs: ListGlyphs,
    /// Ink for `==marked==` text, over `mark_bg`; `None` ⇒ the marked text keeps the
    /// body foreground, which is what every theme did before this key existed.
    pub mark_fg: Option<gdk::RGBA>,
    pub annotation_hl_color: ThemeColor,
    pub find_hl_all_color: ThemeColor,
    pub find_hl_current_color: ThemeColor,
    pub mark_bg: ThemeColor,
    pub typography: Typography,
    pub metrics: Metrics,
    /// Annotation chip fill; `None` ⇒ the hardcoded amber, exactly as before themes
    /// could touch it.
    pub annotation_chip_bg: Option<gdk::RGBA>,
    /// Annotation chip ink; `None` ⇒ the hardcoded white.
    pub annotation_chip_fg: Option<gdk::RGBA>,
    /// Every sprite this theme names, already resolved to the source it comes from —
    /// a validated, contained file for a theme read off disk, compiled-in bytes for a
    /// built-in one (`Themes::parse` resolved it against that file's origin). Opt-in
    /// per theme: a decoration the theme did not ask for is absent, never guessed.
    pub sprites: Sprites,
}

/// Every sprite a theme may name, one field per decoration. A decoration's sprite lives
/// HERE rather than inside that decoration's own struct, so "what files does this theme
/// name?" is one question with one answer. What keeps a new sprite key from being
/// validated nowhere is no longer a hand-written walk over these fields:
/// `ThemeSpec::resolve_sprites` retains over the entries the REGISTRY types as sprites,
/// so a key is validated by having been declared. Each is `None` unless
/// the theme both set the key AND `crate::sprite::resolve` accepted it — a theme
/// that sets a broken reference gets the SAME "decoration absent" fallback as a
/// theme that sets nothing, never a partial or broken render.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Sprites {
    pub annotation_chip: Option<crate::sprite::SpriteRef>,
    /// The bullet's sprite by nesting-depth tier (TDD 18.26), already folded — a tier
    /// the theme left unset carries the next shallower tier's sprite.
    pub list_bullet: [Option<crate::sprite::SpriteRef>; BULLET_TIERS],
    pub list_ordered: Option<crate::sprite::SpriteRef>,
    pub list_task: Option<crate::sprite::SpriteRef>,
    pub list_task_checked: Option<crate::sprite::SpriteRef>,
    pub heading_band: [Option<crate::sprite::SpriteRef>; HEADING_LEVELS],
    pub blockquote_bar: Option<crate::sprite::SpriteRef>,
    /// The horizontal rule's tile (TDD 18.31). Unlike every other entry here, this one
    /// is read by a WIDGET rather than by a drawing pass or an export sink alone — see
    /// `crate::widgets::rule`.
    pub rule: Option<crate::sprite::SpriteRef>,
}

#[cfg(test)]
mod tests {
    use super::super::parse_color;
    use super::*;

    #[test]
    fn bold_attr_carries_the_themed_weight() {
        let typo = Typography {
            heading_scale: [1.0; HEADING_LEVELS],
            heading_weight: [700; HEADING_LEVELS],
            bold_weight: 650,
            supsub_scale: 0.75,
            superscript_rise: 4,
            subscript_rise: -2,
        };
        assert_eq!(typo.bold_attr(), " weight=\"650\"");
    }

    #[test]
    fn supsub_attr_selects_the_matching_rise_and_shares_the_scale() {
        let typo = Typography {
            heading_scale: [1.0; HEADING_LEVELS],
            heading_weight: [700; HEADING_LEVELS],
            bold_weight: 600,
            supsub_scale: 0.75,
            superscript_rise: 4,
            subscript_rise: -2,
        };
        // 0.75 -> "75%"; rise is in Pango units, the SAME `value * pango::SCALE`
        // `tags.rs` feeds `set_rise` — one raw theme number, two representations.
        assert_eq!(
            typo.supsub_attr(true),
            format!(" size=\"75%\" rise=\"{}\"", 4 * gtk::pango::SCALE)
        );
        // subscript_rise's own floor is already negative — no sign flip here.
        assert_eq!(
            typo.supsub_attr(false),
            format!(" size=\"75%\" rise=\"{}\"", -2 * gtk::pango::SCALE)
        );
    }

    /// One key, two decompositions — the split the body and cell paths need.
    #[test]
    fn theme_color_decomposes_for_both_application_paths() {
        let c = ThemeColor(parse_color("#FFD133_61").unwrap());
        assert_eq!(c.hex(), "#ffd133");
        assert_eq!(c.alpha_pct(), "38%");
        let (red, _green, _blue) = c.u16_triple();
        assert_eq!(red, 0xffff);
        assert_eq!(c.rgba().alpha(), 97.0 / 255.0);
    }
}
