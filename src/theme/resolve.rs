//! Resolution: the selected theme, then `[themes.system]`, then the key's own floor.
//!
//! **This module used to hold the floors and the clamp ranges; it holds neither now.**
//! They are properties of a key, so they live on the key ([`super::keys::Bound`]) and
//! the accessors read them off the `Key` they are already handed. What is left here is
//! the one function that walks the resolution order and lands each answer on the model
//! — no `F_*` constant, no `*_RANGE` constant, and no per-key re-pairing of the two.
//!
//! The floors are still NOT a second source of truth: `theme::tests::system` resolves
//! `[themes.system]` twice — once against the shipped `data/themes.toml` and once
//! against an EMPTY spec, where every value can only be its floor — and asserts the
//! two are equal. The data file stays the place a human reads and edits, and drift is
//! a test failure for every key at once rather than for the ones somebody remembered
//! to list.

use super::keys::{self, HEADING_LEVELS};
use super::model::{
    HeadingBand, HeadingRule, ListGlyphs, Metrics, Sprites, Theme, ThemeColor, Typography,
};
use super::sources::Sources;
use super::spec::ThemeSpec;

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
        let src = Sources::new(id, selected, system);

        // The bare heading ink and face: what the theme said about headings as a
        // whole, which is what the table header reads when it states nothing of its
        // own (TDD 18.30). The per-level arrays beside them have already folded each
        // level down to these.
        let heading_color = src.color(&keys::HEADING_COLOR);
        let heading_font = src.font(&keys::HEADING_FONT);

        let list_marker_color = src.color(&keys::LIST_MARKER_COLOR);

        // A gradient's second stop with no first one is a stated key that renders
        // nothing. SCHEMA's `heading_band_gradient_to_color` row says so — "Ignored
        // where the level states no fill" — but the code said it in silence, which is
        // ScrAP-324's class: "the key resolved fine and was then discarded for want of
        // another" looks exactly like "the theme stated nothing". The SPRITE's version
        // of this is gone (a sprite alone is now a band); the gradient's is real and is
        // therefore diagnosed rather than removed.
        for level in 0..HEADING_LEVELS {
            let key = keys::HEADING_BAND_GRADIENT_TO_COLOR.spelling(level);
            // **Only a gradient STATED AT THIS LEVEL is a mistake**, which is what the
            // paragraph above already says and what this loop did not do. A theme may
            // state ONE bare `heading_band_gradient_to_color` and band only its top two
            // levels — a perfectly ordinary shape, and both `synthwave` and `candy` ship
            // it. The bare key is a BROADCAST, and a broadcast that does not apply
            // everywhere it reaches is not an authoring error; the resolved per-level
            // value cannot tell the two apart, so asking it produced three warnings per
            // theme on shipped defaults, forever, for nothing the author did wrong.
            // That matters beyond tidiness: several manual checks verify a theme by
            // running with warnings on and expecting SILENCE, so noise here does not
            // merely annoy, it spends the signal those checks read.
            let stated_here = src.stated(&key);
            if stated_here
                && src.colors::<HEADING_LEVELS>(&keys::HEADING_BAND_COLOR)[level].is_none()
            {
                log::warn!(
                    "theme {id:?}: {key} is ignored — a gradient is a second stop, and \
                     this level states no heading_band_color for it to start from"
                );
            }
        }
        // The summary band's own version of the same discard, diagnosed for the same
        // reason: silence makes "the key resolved fine and was then dropped for want
        // of another" indistinguishable from "the theme stated nothing" (ScrAP-324).
        if src
            .color(&keys::DISCLOSURE_BAND_GRADIENT_TO_COLOR)
            .is_some()
            && src.color(&keys::DISCLOSURE_BAND_COLOR).is_none()
        {
            log::warn!(
                "theme {id:?}: disclosure_band_gradient_to_color is ignored — a gradient \
                 is a second stop, and this theme states no disclosure_band_color for it \
                 to start from"
            );
        }

        Theme {
            id: id.to_string(),
            // **A theme's name and symbol are its OWN**, so both read `own_text`
            // rather than the two-source walk — otherwise every theme that states
            // neither is labelled "System" in the picker, wearing the base theme's
            // window glyph. `symbol` used to take the walk while `name` beside it did
            // not, which diverged the two chooser surfaces: `chooser_list` builds its
            // label from `own_text` (no symbol) while `window::actions` reads the
            // resolved `Theme::symbol` (the inherited one), and TDD 18.1 says both
            // always show the same choice. Latent only because all seven shipped
            // themes state a symbol — i.e. it bit exactly the case TDD 18.14 is
            // about, a theme added as data.
            //
            // There is no `id == SYSTEM_ID` branch here any more, and `Themes::resolve`
            // no longer blanks `selected` for that id. The two were one carve-out split
            // across two files: the filter's only EFFECT was to destroy the system
            // theme's own display name, which this branch then had to put back. With
            // the filter gone, `selected` IS the system spec for that id and
            // `own_text(&NAME)` finds "System" directly.
            name: selected
                .own_text(&keys::NAME)
                .map(str::to_string)
                .unwrap_or_else(|| id.to_string()),
            symbol: selected.own_text(&keys::SYMBOL).map(str::to_string),
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
                overline: src.lines(&keys::HEADING_OVERLINE),
                underline: src.lines(&keys::HEADING_UNDERLINE),
                underline_color: src.colors(&keys::HEADING_UNDERLINE_COLOR),
            },
            heading_band: HeadingBand {
                fills: src.colors(&keys::HEADING_BAND_COLOR),
                gradient_to: src.colors(&keys::HEADING_BAND_GRADIENT_TO_COLOR),
            },
            link_color: src.color(&keys::LINK_COLOR),
            link_underline: src.line(&keys::LINK_UNDERLINE),
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
            disclosure_glyphs: crate::theme::model::DisclosureGlyphs {
                collapsed: src.glyph(&keys::DISCLOSURE_GLYPH),
                expanded: src.glyph(&keys::DISCLOSURE_EXPANDED_GLYPH),
            },
            // Folded down to the page's own ink, the way `table_head_fg` folds to the
            // heading's — and for a sharper reason than tidiness. An unstated marker
            // colour left the indicator on the DESKTOP theme's ink, which is a colour
            // the reading theme does not own: wrong on a themed page even focused, and
            // not stable when the window loses focus, since the desktop states an
            // unfocused ink for the node the mark is drawn on (TDD 18.52). This is the
            // rule the drawn list markers already follow at their own paint site —
            // marker ink is the body's until the theme says otherwise — so the fold
            // makes the two agree rather than introducing a new one.
            //
            // System states no foreground, so this stays `None` there and the theme
            // sheet emits no rule at all: the stock chevron on the desktop's ink,
            // byte-identical to what it has always drawn (TDD 18.2).
            disclosure_marker_color: src
                .color(&keys::DISCLOSURE_MARKER_COLOR)
                .or_else(|| src.color(&keys::FOREGROUND)),
            disclosure_preview_fg: src.color(&keys::DISCLOSURE_PREVIEW_FG),
            disclosure_band_color: src.color(&keys::DISCLOSURE_BAND_COLOR),
            disclosure_band_gradient_to: src.color(&keys::DISCLOSURE_BAND_GRADIENT_TO_COLOR),
            disclosure_fg: src.color(&keys::DISCLOSURE_FG),
            mark_fg: src.color(&keys::MARK_FG),
            annotation_hl_color: ThemeColor(src.color_floored(&keys::ANNOTATION_HL_COLOR)),
            find_hl_all_color: ThemeColor(src.color_floored(&keys::FIND_HL_ALL_COLOR)),
            find_hl_current_color: ThemeColor(src.color_floored(&keys::FIND_HL_CURRENT_COLOR)),
            mark_bg: ThemeColor(src.color_floored(&keys::MARK_BG)),
            typography: Typography {
                heading_scale: src.floats(&keys::HEADING_SCALE),
                heading_weight: src.ints(&keys::HEADING_WEIGHT),
                bold_weight: src.int(&keys::BOLD_WEIGHT),
                supsub_scale: src.float(&keys::SUPSUB_SCALE),
                superscript_rise: src.int(&keys::SUPERSCRIPT_RISE),
                subscript_rise: src.int(&keys::SUBSCRIPT_RISE),
            },
            metrics: Metrics {
                heading_space_below: src.ints(&keys::HEADING_SPACE_BELOW),
                heading_space_above: src.ints(&keys::HEADING_SPACE_ABOVE),
                heading_band_radius: src.ints(&keys::HEADING_BAND_RADIUS),
                heading_band_padding: src.ints(&keys::HEADING_BAND_PADDING),
                rule_thickness: src.int(&keys::RULE_THICKNESS),
                blockquote_bar_width: src.int(&keys::BLOCKQUOTE_BAR_WIDTH),
                blockquote_text_gap: src.int(&keys::BLOCKQUOTE_TEXT_GAP),
                list_step: src.int(&keys::LIST_STEP),
                list_item_gap: src.int(&keys::LIST_ITEM_GAP),
                rule_space: src.int(&keys::RULE_SPACE),
                table_cell_padding_v: src.int(&keys::TABLE_CELL_PADDING_V),
                table_cell_padding_h: src.int(&keys::TABLE_CELL_PADDING_H),
                table_border_width: src.int(&keys::TABLE_BORDER_WIDTH),
                table_cell_radius: src.int(&keys::TABLE_CELL_RADIUS),
                disclosure_marker_size: src.int(&keys::DISCLOSURE_MARKER_SIZE),
                disclosure_band_radius: src.int(&keys::DISCLOSURE_BAND_RADIUS),
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
                disclosure: src.sprite(&keys::DISCLOSURE_SPRITE),
                disclosure_expanded: src.sprite(&keys::DISCLOSURE_EXPANDED_SPRITE),
                disclosure_band: src.sprite(&keys::DISCLOSURE_BAND_SPRITE),
            },
        }
    }
}
