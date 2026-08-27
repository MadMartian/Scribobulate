//! Resolution: the selected theme, then `[themes.system]`, then a floor.
//!
//! This module holds the two things that make resolution **total** — the last-resort
//! floor for every value that must have one, and the clamp range for every value that
//! could be hostile — plus the one function that applies them.
//!
//! The floors are NOT a second source of truth. `builtin_system_spec_matches_the_floor`
//! asserts each one equals the shipped `data/themes.toml` `[themes.system]` value, so
//! the data file stays the place a human reads and edits, and drift is a test failure.

use super::keys::{self, HEADING_LEVELS};
use super::model::{
    HeadingBand, HeadingRule, ListGlyphs, Metrics, Sprites, Theme, ThemeColor, Typography,
};
use super::sources::Sources;
use super::spec::ThemeSpec;
use super::SYSTEM_ID;
use super::{parse_color, LineStyle};
use gtk::gdk;

// ── clamp ranges ──────────────────────────────────────────────────────────────
//
// A malformed or hostile theme (`list_step = -5`, or `10000`) must not be able to
// break layout (TDD 18.11). Clamping — rather than rejecting — keeps a theme that
// is merely over-enthusiastic usable, and keeps resolution total.

pub(super) const SCALE_RANGE: (f64, f64) = (0.25, 8.0);
pub(super) const WEIGHT_RANGE: (i32, i32) = (100, 1000);
pub(super) const RISE_RANGE: (i32, i32) = (-64, 64);
/// Decoration metrics: no negative sizes, and nothing wide enough to push the
/// text column off its own viewport.
pub(super) const METRIC_RANGE: (i32, i32) = (0, 400);
/// A list step of 0 would stack every nesting depth in one column and bury the
/// drawn markers under the text, so this one has a positive floor.
pub(super) const LIST_STEP_RANGE: (i32, i32) = (4, 400);

// ── the last-resort floor ─────────────────────────────────────────────────────
//
// Resolution must be TOTAL: every geometry/typography key has to produce a value
// even if `[themes.system]` somehow lacks it. These are that floor. They are NOT
// a second source of truth — `builtin_system_spec_matches_the_floor` asserts each
// one equals the shipped `data/themes.toml` `[themes.system]` value, so the data
// file stays the place a human reads and edits, and drift is a test failure.

pub(super) const F_HEADING_SCALE: [f64; HEADING_LEVELS] = [2.2, 1.8, 1.48, 1.2, 1.0];
pub(super) const F_HEADING_WEIGHT: [i32; HEADING_LEVELS] = [700; HEADING_LEVELS];
pub(super) const F_BOLD_WEIGHT: i32 = 700;
pub(super) const F_SUPSUB_SCALE: f64 = 0.72;
pub(super) const F_SUPERSCRIPT_RISE: i32 = 4;
pub(super) const F_SUBSCRIPT_RISE: i32 = -2;
pub(super) const F_HEADING_SPACE_BELOW: [i32; HEADING_LEVELS] = [4, 4, 2, 2, 2];
/// Zero, because the heading tags set no `pixels_above_lines` at all before this key
/// existed — the floor IS today's rendering, which is what keeps System byte-identical
/// (TDD 18.2). Not symmetric with the below-floor by accident: only space-below was
/// ever expressed.
pub(super) const F_HEADING_SPACE_ABOVE: [i32; HEADING_LEVELS] = [0, 0, 0, 0, 0];
/// No heading carries a band until a theme states a fill for its level, so the radius
/// is only ever consulted for a band that exists.
pub(super) const F_HEADING_BAND_RADIUS: [i32; HEADING_LEVELS] = [0; HEADING_LEVELS];
/// NON-ZERO, unlike every other decoration default here, and deliberately so: a band's
/// padding is not an opt-in flourish but part of drawing a band correctly. It is inert
/// anyway on a theme that bands nothing, because the inset is applied per level and only
/// where that level HAS a band — the gate, not the value, is what keeps System
/// byte-identical (TDD 18.2), and every theme that already ships a band gets the fix
/// with no content edit.
pub(super) const F_HEADING_BAND_PADDING: [i32; HEADING_LEVELS] = [12; HEADING_LEVELS];
/// No heading rule is drawn today, on either side.
pub(super) const F_HEADING_OVERLINE: LineStyle = LineStyle::None;
pub(super) const F_HEADING_UNDERLINE: LineStyle = LineStyle::None;
/// A body link has been underlined with a single line since before themes existed, so
/// unlike the heading rule's floor this one is NOT "none" — it is the shipped look, and
/// changing it would move System (TDD 18.2).
pub(super) const F_LINK_UNDERLINE: LineStyle = LineStyle::Single;
pub(super) const F_BQ_BAR_WIDTH: i32 = 3;
pub(super) const F_BQ_TEXT_GAP: i32 = 10;
pub(super) const F_LIST_STEP: i32 = 28;
pub(super) const F_LIST_ITEM_GAP: i32 = 8;
pub(super) const F_RULE_SPACE: i32 = 4;
pub(super) const F_TABLE_CELL_PADDING_V: i32 = 4;
pub(super) const F_TABLE_CELL_PADDING_H: i32 = 10;
pub(super) const F_TABLE_BORDER_WIDTH: i32 = 1;
pub(super) const F_TABLE_CELL_RADIUS: i32 = 0;

impl Theme {
    /// Apply resolution links 1 (selected) and 2 (`[themes.system]`), clamping and
    /// sanitising as it goes. Pure and total: every geometry/typography key lands
    /// on a value, and any colour still unresolved is left `None` for link 3.
    ///
    /// Every read goes through a [`keys`] constant rather than a field or a string,
    /// and every per-level value is folded HERE — so `tags.rs`, the table header and
    /// both export sinks index a value that is already correct instead of each
    /// re-deriving the fallback (POLICY "One theme key, every application path").
    pub(super) fn resolve(id: &str, selected: &ThemeSpec, system: &ThemeSpec) -> Theme {
        let src = Sources { selected, system };

        // An overlay colour must always resolve, so it walks all the way to a floor.
        let overlay = |key: &keys::Key, floor: &str| {
            ThemeColor(
                src.color(key)
                    .unwrap_or_else(|| parse_color(floor).unwrap_or(gdk::RGBA::BLACK)),
            )
        };

        // The bare heading ink and face: what the theme said about headings as a
        // whole, which is what the table header reads when it states nothing of its
        // own (TDD 18.30). The per-level arrays beside them have already folded each
        // level down to these.
        let heading_color = src.color(&keys::HEADING_COLOR);
        let heading_font = src.font(&keys::HEADING_FONT);

        let list_marker_color = src.color(&keys::LIST_MARKER_COLOR);

        Theme {
            id: id.to_string(),
            // A theme's name and symbol are its own: `own_text` rather than the
            // two-source walk, or every unnamed theme would be called "System".
            name: selected
                .own_text(&keys::NAME)
                .or_else(|| {
                    (id == SYSTEM_ID)
                        .then(|| system.own_text(&keys::NAME))
                        .flatten()
                })
                .map(str::to_string)
                .unwrap_or_else(|| id.to_string()),
            symbol: src.text(&keys::SYMBOL),
            background: src.color(&keys::BACKGROUND),
            foreground: src.color(&keys::FOREGROUND),
            accent_color: src.color(&keys::ACCENT_COLOR),
            font_family: src.font(&keys::FONT_FAMILY),
            syntect_theme: src.text(&keys::SYNTECT_THEME),
            heading_color,
            heading_font,
            heading_colors: src.colors(&keys::HEADING_COLOR),
            heading_fonts: src.fonts(&keys::HEADING_FONT),
            heading_rule: HeadingRule {
                overline: src.lines(&keys::HEADING_OVERLINE, F_HEADING_OVERLINE),
                underline: src.lines(&keys::HEADING_UNDERLINE, F_HEADING_UNDERLINE),
                underline_color: src.colors(&keys::HEADING_UNDERLINE_COLOR),
            },
            heading_band: HeadingBand {
                fills: src.colors(&keys::HEADING_BAND_COLOR),
                gradient_to: src.colors(&keys::HEADING_BAND_GRADIENT_TO_COLOR),
            },
            link_color: src.color(&keys::LINK_COLOR),
            link_underline: src.line(&keys::LINK_UNDERLINE, F_LINK_UNDERLINE),
            link_underline_color: src.color(&keys::LINK_UNDERLINE_COLOR),
            strikethrough_color: src.color(&keys::STRIKETHROUGH_COLOR),
            code_inline_bg: src.color(&keys::CODE_INLINE_BG),
            code_block_bg: src.color(&keys::CODE_BLOCK_BG),
            blockquote_bar_color: src.color(&keys::BLOCKQUOTE_BAR_COLOR),
            blockquote_bg: src.color(&keys::BLOCKQUOTE_BG),
            blockquote_fg: src.color(&keys::BLOCKQUOTE_FG),
            selection_bg: src.color(&keys::SELECTION_BG),
            selection_fg: src.color(&keys::SELECTION_FG),
            table_border_color: src.color(&keys::TABLE_BORDER_COLOR),
            table_head_bg: src.color(&keys::TABLE_HEAD_BG),
            // Folded once here, like every other fallback: the header's ink is the
            // heading's until the theme says otherwise.
            table_head_fg: src.color(&keys::TABLE_HEAD_FG).or(heading_color),
            rule_color: src.color(&keys::RULE_COLOR),
            list_marker_color,
            list_bullet_colors: src.colors(&keys::LIST_MARKER_COLOR),
            list_task_color: src
                .color(&keys::LIST_TASK_MARKER_COLOR)
                .or(list_marker_color),
            list_glyphs: ListGlyphs {
                bullet: src.glyphs(&keys::LIST_BULLET_GLYPH),
                ordered: src.glyph(&keys::LIST_ORDERED_GLYPH),
                task: src.glyph(&keys::LIST_TASK_GLYPH),
                task_checked: src.glyph(&keys::LIST_TASK_CHECKED_GLYPH),
            },
            mark_fg: src.color(&keys::MARK_FG),
            annotation_hl_color: overlay(&keys::ANNOTATION_HL_COLOR, "#FFD133_61"),
            find_hl_all_color: overlay(&keys::FIND_HL_ALL_COLOR, "#f6d32d"),
            find_hl_current_color: overlay(&keys::FIND_HL_CURRENT_COLOR, "#ff7800"),
            // Neutral highlighter yellow as the last-resort floor; each bundled
            // theme overrides it with a page-appropriate wash (data/themes.toml).
            mark_bg: overlay(&keys::MARK_BG, "#fff59d_88"),
            typography: Typography {
                heading_scale: src.floats(&keys::HEADING_SCALE, F_HEADING_SCALE, SCALE_RANGE),
                heading_weight: src.ints(&keys::HEADING_WEIGHT, F_HEADING_WEIGHT, WEIGHT_RANGE),
                bold_weight: src.int(&keys::BOLD_WEIGHT, F_BOLD_WEIGHT, WEIGHT_RANGE),
                supsub_scale: src.float(&keys::SUPSUB_SCALE, F_SUPSUB_SCALE, SCALE_RANGE),
                superscript_rise: src.int(&keys::SUPERSCRIPT_RISE, F_SUPERSCRIPT_RISE, RISE_RANGE),
                subscript_rise: src.int(&keys::SUBSCRIPT_RISE, F_SUBSCRIPT_RISE, RISE_RANGE),
            },
            metrics: Metrics {
                heading_space_below: src.ints(
                    &keys::HEADING_SPACE_BELOW,
                    F_HEADING_SPACE_BELOW,
                    METRIC_RANGE,
                ),
                heading_space_above: src.ints(
                    &keys::HEADING_SPACE_ABOVE,
                    F_HEADING_SPACE_ABOVE,
                    METRIC_RANGE,
                ),
                heading_band_radius: src.ints(
                    &keys::HEADING_BAND_RADIUS,
                    F_HEADING_BAND_RADIUS,
                    METRIC_RANGE,
                ),
                heading_band_padding: src.ints(
                    &keys::HEADING_BAND_PADDING,
                    F_HEADING_BAND_PADDING,
                    METRIC_RANGE,
                ),
                blockquote_bar_width: src.int(
                    &keys::BLOCKQUOTE_BAR_WIDTH,
                    F_BQ_BAR_WIDTH,
                    METRIC_RANGE,
                ),
                blockquote_text_gap: src.int(
                    &keys::BLOCKQUOTE_TEXT_GAP,
                    F_BQ_TEXT_GAP,
                    METRIC_RANGE,
                ),
                list_step: src.int(&keys::LIST_STEP, F_LIST_STEP, LIST_STEP_RANGE),
                list_item_gap: src.int(&keys::LIST_ITEM_GAP, F_LIST_ITEM_GAP, METRIC_RANGE),
                rule_space: src.int(&keys::RULE_SPACE, F_RULE_SPACE, METRIC_RANGE),
                table_cell_padding_v: src.int(
                    &keys::TABLE_CELL_PADDING_V,
                    F_TABLE_CELL_PADDING_V,
                    METRIC_RANGE,
                ),
                table_cell_padding_h: src.int(
                    &keys::TABLE_CELL_PADDING_H,
                    F_TABLE_CELL_PADDING_H,
                    METRIC_RANGE,
                ),
                table_border_width: src.int(
                    &keys::TABLE_BORDER_WIDTH,
                    F_TABLE_BORDER_WIDTH,
                    METRIC_RANGE,
                ),
                table_cell_radius: src.int(
                    &keys::TABLE_CELL_RADIUS,
                    F_TABLE_CELL_RADIUS,
                    METRIC_RANGE,
                ),
            },
            annotation_chip_bg: src.color(&keys::ANNOTATION_CHIP_BG),
            annotation_chip_fg: src.color(&keys::ANNOTATION_CHIP_FG),
            // Already-resolved sources: `Themes::parse` answered every sprite
            // reference against its file's origin before this function ever sees the
            // spec, so `Theme::resolve` itself stays pure (no filesystem).
            sprites: Sprites {
                annotation_chip: src.sprite(&keys::ANNOTATION_CHIP_SPRITE),
                list_bullet: src.sprites(&keys::LIST_BULLET_SPRITE),
                list_ordered: src.sprite(&keys::LIST_ORDERED_SPRITE),
                list_task: src.sprite(&keys::LIST_TASK_SPRITE),
                list_task_checked: src.sprite(&keys::LIST_TASK_CHECKED_SPRITE),
                heading_band: src.sprites(&keys::HEADING_BAND_SPRITE),
                blockquote_bar: src.sprite(&keys::BLOCKQUOTE_BAR_SPRITE),
                rule: src.sprite(&keys::RULE_SPRITE),
            },
        }
    }
}
