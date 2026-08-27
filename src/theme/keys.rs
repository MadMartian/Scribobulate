//! The theme key registry — the one enumeration of the vocabulary `themes.toml`
//! speaks, and the shape everything else in this module reads.
//!
//! **Every key exists exactly once, here.** The registry answers all four questions
//! the rest of the theme model used to answer separately, each with its own
//! hand-maintained list: what may a theme say (validation, and the warning for a key
//! this build does not know), what type does that value have, how many values does
//! the key carry (one, one per heading level, or one per bullet nesting depth), and
//! which keys name a sprite that load-time resolution must answer. A list that is
//! written once and read four times cannot drift from itself; four lists could, and
//! did — the shipped `list_marker` bug was a key present in the file model and absent
//! from the merge list, which compiled, passed every test, and silently dropped every
//! user override of that one key.
//!
//! The registry also makes the *level* dimension free. A heading key is declared
//! `Heading` and thereby exists in six spellings — the bare key plus `_h1`…`_h5` —
//! with no per-level field, no per-level merge entry and no per-level resolution
//! branch anywhere. Adding a heading key is one line here plus its use site.

/// What a key's value is. Decides how the authored TOML scalar is coerced, and
/// therefore what a wrong type in a theme file costs (a warning and that one key,
/// never the file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A plain string used as-is (`name`, `syntect_theme`).
    Text,
    /// A CSS font stack, sanitised and generic-terminated before it can be used.
    Font,
    /// An `RGBA` spelling — `#rrggbb`, `#rrggbb_aa`, or a CSS colour name.
    Color,
    /// A short grapheme run standing in for a drawn marker.
    Glyph,
    /// A decoration-line style (`none`/`single`/`double`/`wavy`).
    Line,
    /// A path naming an image, resolved against the stating file's origin.
    Sprite,
    /// A whole-number metric, clamped at its use site.
    Int,
    /// A fractional scale, clamped at its use site.
    Float,
}

/// How many values a key carries, and how the extra ones are spelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Levelling {
    /// One value, one spelling.
    Flat,
    /// One value per heading level: the bare key applies to every level, and
    /// `_h1`…`_h5` narrows it to one, overriding the bare form.
    Heading,
    /// One value per bullet nesting depth: the bare key is depth 1, `_2` and `_3`
    /// narrow it, and each falls back to the next *shallower* depth.
    Depth,
}

/// Heading levels a theme can address: h1 · h2 · h3 · h4 · h5-and-deeper. The
/// renderer maps h6 onto the h5 tag before a tag is ever chosen — on every surface,
/// preview and outline alike — so there is no `_h6` spelling and no theme can
/// differentiate the two.
pub(crate) const HEADING_LEVELS: usize = 5;

/// Bullet nesting-depth tiers: depth 1, depth 2, depth 3-and-deeper.
pub(crate) const BULLET_TIERS: usize = 3;

/// One key of the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    /// The bare spelling, as a theme file writes it.
    pub name: &'static str,
    pub kind: Kind,
    pub levelling: Levelling,
}

impl Key {
    /// How many values this key carries.
    pub(crate) const fn slots(&self) -> usize {
        match self.levelling {
            Levelling::Flat => 1,
            Levelling::Heading => HEADING_LEVELS,
            Levelling::Depth => BULLET_TIERS,
        }
    }

    /// The spelling this key wears at slot `idx` — the bare name for a flat key and
    /// for a depth key's first tier, `_hN` for a heading level, `_N` for a deeper
    /// nesting tier.
    pub(crate) fn spelling(&self, idx: usize) -> String {
        match self.levelling {
            Levelling::Flat => self.name.to_string(),
            Levelling::Heading => format!("{}_h{}", self.name, idx + 1),
            Levelling::Depth if idx == 0 => self.name.to_string(),
            Levelling::Depth => format!("{}_{}", self.name, idx + 1),
        }
    }

    /// Whether `name` is one of this key's spellings — the bare form, which every key
    /// has, or one of the narrowed forms its levelling adds.
    pub(crate) fn claims(&self, name: &str) -> bool {
        self.name == name || (0..self.slots()).any(|i| self.spelling(i) == name)
    }

    /// Every spelling to try for slot `idx`, **most specific first and always ending
    /// at the bare key** — the fallback chain, walked once per source.
    ///
    /// The two levellings differ in exactly one way, and it is the difference the
    /// vocabulary intends. A heading level falls back to the bare key directly,
    /// because the bare key means *every level*. A nesting depth falls back through
    /// each shallower tier first, because a bullet's depth keys describe a gradient
    /// down the nesting and an unstated depth 3 should look like depth 2 rather than
    /// like depth 1.
    pub(crate) fn fallbacks(&self, idx: usize) -> Vec<String> {
        match self.levelling {
            Levelling::Flat => vec![self.name.to_string()],
            Levelling::Heading if idx == 0 => vec![self.spelling(0), self.name.to_string()],
            Levelling::Heading => vec![self.spelling(idx), self.name.to_string()],
            Levelling::Depth => (0..=idx).rev().map(|i| self.spelling(i)).collect(),
        }
    }
}

/// Declare the vocabulary: a constant per key plus the [`KEYS`] table, from one line
/// each. The trailing levelling word is optional and defaults to [`Levelling::Flat`].
macro_rules! keys {
    (@lev) => { Levelling::Flat };
    (@lev $l:ident) => { Levelling::$l };
    ($( $konst:ident = $name:literal : $kind:ident $($lev:ident)? ; )+) => {
        $(
            pub(crate) const $konst: Key = Key {
                name: $name,
                kind: Kind::$kind,
                levelling: keys!(@lev $($lev)?),
            };
        )+
        /// Every key this build knows, in the order the schema documents them.
        pub(crate) const KEYS: &[Key] = &[ $($konst),+ ];
    };
}

keys! {
    // ── identity and the base colours ────────────────────────────────────────
    NAME                   = "name"                    : Text;
    SYMBOL                 = "symbol"                  : Text;
    BACKGROUND             = "background"              : Color;
    FOREGROUND             = "foreground"              : Color;
    ACCENT_COLOR           = "accent_color"            : Color;
    FONT_FAMILY            = "font_family"             : Font;
    SYNTECT_THEME          = "syntect_theme"           : Text;

    // ── headings (every one of these also takes an `_h1`…`_h5` spelling) ─────
    HEADING_COLOR          = "heading_color"           : Color  Heading;
    HEADING_FONT           = "heading_font"            : Font   Heading;
    HEADING_SCALE          = "heading_scale"           : Float  Heading;
    HEADING_WEIGHT         = "heading_weight"          : Int    Heading;
    HEADING_OVERLINE       = "heading_overline"        : Line   Heading;
    HEADING_UNDERLINE      = "heading_underline"       : Line   Heading;
    HEADING_UNDERLINE_COLOR = "heading_underline_color" : Color Heading;
    HEADING_BAND_COLOR     = "heading_band_color"      : Color  Heading;
    HEADING_BAND_GRADIENT_TO_COLOR
                           = "heading_band_gradient_to_color" : Color Heading;
    HEADING_BAND_SPRITE    = "heading_band_sprite"     : Sprite Heading;
    HEADING_BAND_RADIUS    = "heading_band_radius"     : Int    Heading;
    HEADING_BAND_PADDING   = "heading_band_padding"    : Int    Heading;
    HEADING_SPACE_ABOVE    = "heading_space_above"     : Int    Heading;
    HEADING_SPACE_BELOW    = "heading_space_below"     : Int    Heading;

    // ── body and inline text ─────────────────────────────────────────────────
    BOLD_WEIGHT            = "bold_weight"             : Int;
    SUPSUB_SCALE           = "supsub_scale"            : Float;
    SUPERSCRIPT_RISE       = "superscript_rise"        : Int;
    SUBSCRIPT_RISE         = "subscript_rise"          : Int;
    STRIKETHROUGH_COLOR    = "strikethrough_color"     : Color;
    MARK_BG                = "mark_bg"                 : Color;
    MARK_FG                = "mark_fg"                 : Color;
    CODE_INLINE_BG         = "code_inline_bg"          : Color;
    CODE_BLOCK_BG          = "code_block_bg"           : Color;

    // ── links ────────────────────────────────────────────────────────────────
    LINK_COLOR             = "link_color"              : Color;
    LINK_UNDERLINE         = "link_underline"          : Line;
    LINK_UNDERLINE_COLOR   = "link_underline_color"    : Color;

    // ── lists (the ⓷ keys also take `_2` and `_3` spellings) ─────────────────
    LIST_MARKER_COLOR      = "list_marker_color"       : Color  Depth;
    LIST_TASK_MARKER_COLOR = "list_task_marker_color"  : Color;
    LIST_BULLET_GLYPH      = "list_bullet_glyph"       : Glyph  Depth;
    LIST_ORDERED_GLYPH     = "list_ordered_glyph"      : Glyph;
    LIST_TASK_GLYPH        = "list_task_glyph"         : Glyph;
    LIST_TASK_CHECKED_GLYPH = "list_task_checked_glyph" : Glyph;
    LIST_BULLET_SPRITE     = "list_bullet_sprite"      : Sprite Depth;
    LIST_ORDERED_SPRITE    = "list_ordered_sprite"     : Sprite;
    LIST_TASK_SPRITE       = "list_task_sprite"        : Sprite;
    LIST_TASK_CHECKED_SPRITE = "list_task_checked_sprite" : Sprite;
    LIST_STEP              = "list_step"               : Int;
    LIST_ITEM_GAP          = "list_item_gap"           : Int;

    // ── blockquote ───────────────────────────────────────────────────────────
    BLOCKQUOTE_BAR_COLOR   = "blockquote_bar_color"    : Color;
    BLOCKQUOTE_BAR_SPRITE  = "blockquote_bar_sprite"   : Sprite;
    BLOCKQUOTE_BAR_WIDTH   = "blockquote_bar_width"    : Int;
    BLOCKQUOTE_TEXT_GAP    = "blockquote_text_gap"     : Int;
    BLOCKQUOTE_BG          = "blockquote_bg"           : Color;
    BLOCKQUOTE_FG          = "blockquote_fg"           : Color;

    // ── table ────────────────────────────────────────────────────────────────
    TABLE_BORDER_COLOR     = "table_border_color"      : Color;
    TABLE_BORDER_WIDTH     = "table_border_width"      : Int;
    TABLE_HEAD_BG          = "table_head_bg"           : Color;
    TABLE_HEAD_FG          = "table_head_fg"           : Color;
    TABLE_CELL_PADDING_V   = "table_cell_padding_v"    : Int;
    TABLE_CELL_PADDING_H   = "table_cell_padding_h"    : Int;
    TABLE_CELL_RADIUS      = "table_cell_radius"       : Int;

    // ── horizontal rule ──────────────────────────────────────────────────────
    RULE_COLOR             = "rule_color"              : Color;
    RULE_SPRITE            = "rule_sprite"             : Sprite;
    RULE_SPACE             = "rule_space"              : Int;

    // ── selection ────────────────────────────────────────────────────────────
    SELECTION_BG           = "selection_bg"            : Color;
    SELECTION_FG           = "selection_fg"            : Color;

    // ── annotations and find ─────────────────────────────────────────────────
    ANNOTATION_HL_COLOR    = "annotation_hl_color"     : Color;
    ANNOTATION_CHIP_BG     = "annotation_chip_bg"      : Color;
    ANNOTATION_CHIP_FG     = "annotation_chip_fg"      : Color;
    ANNOTATION_CHIP_SPRITE = "annotation_chip_sprite"  : Sprite;
    FIND_HL_ALL_COLOR      = "find_hl_all_color"       : Color;
    FIND_HL_CURRENT_COLOR  = "find_hl_current_color"   : Color;
}

/// The key a theme file's `name` belongs to, and which of its slots it spells.
///
/// This is the whole of unknown-key detection: a name no key claims is a name this
/// build does not know, whether it is a misspelling, a retired spelling from before
/// the vocabulary was regularised, or a key from a later version.
pub(crate) fn lookup(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.claims(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two keys claiming one spelling would make `lookup` order-dependent and make a
    /// merge silently pick a side. The registry is hand-written, so this is the check
    /// that it stays a *set*.
    #[test]
    fn no_two_keys_claim_the_same_spelling() {
        let mut seen = std::collections::BTreeSet::new();
        for key in KEYS {
            for spelling in std::iter::once(key.name.to_string())
                .chain((0..key.slots()).map(|i| key.spelling(i)))
                .collect::<std::collections::BTreeSet<_>>()
            {
                assert!(
                    seen.insert(spelling.clone()),
                    "two keys claim the spelling {spelling:?}"
                );
            }
        }
    }

    #[test]
    fn a_heading_key_falls_back_to_its_bare_form_and_a_depth_key_walks_shallower() {
        assert_eq!(
            HEADING_COLOR.fallbacks(2),
            vec!["heading_color_h3", "heading_color"]
        );
        assert_eq!(
            HEADING_COLOR.fallbacks(0),
            vec!["heading_color_h1", "heading_color"]
        );
        assert_eq!(
            LIST_MARKER_COLOR.fallbacks(2),
            vec![
                "list_marker_color_3",
                "list_marker_color_2",
                "list_marker_color"
            ]
        );
        assert_eq!(LIST_MARKER_COLOR.fallbacks(0), vec!["list_marker_color"]);
        assert_eq!(LINK_COLOR.fallbacks(0), vec!["link_color"]);
    }

    /// Every fallback chain must end at the bare key, or a key stated once in its bare
    /// form would fail to reach some level — the exact defect the levelling exists to
    /// prevent.
    #[test]
    fn every_chain_ends_at_the_bare_key() {
        for key in KEYS {
            for idx in 0..key.slots() {
                assert_eq!(
                    key.fallbacks(idx).last().map(String::as_str),
                    Some(key.name),
                    "{}[{idx}] does not fall back to its bare form",
                    key.name
                );
            }
        }
    }

    /// A retired spelling must not resolve. Each of these was legal before the
    /// vocabulary was regularised, which is exactly why silence would be wrong (TDD
    /// 18.35).
    #[test]
    fn a_retired_spelling_is_not_a_key() {
        for retired in [
            "sprite_rule",
            "sprite_annotation_chip",
            "heading_colors",
            "heading_fonts",
            "heading_band_bg",
            "link",
            "rule",
            "accent",
            "table_border",
            "list_marker",
            "list_marker_2",
            "strikethrough_rgba",
            "link_underline_rgba",
            "heading_underline_rgba",
            "annotation_hl",
            "find_hl_all",
            "heading_color_h6",
        ] {
            assert!(lookup(retired).is_none(), "{retired} still resolves");
        }
    }

    #[test]
    fn every_documented_spelling_resolves_to_its_key() {
        assert_eq!(lookup("heading_color_h4"), Some(&HEADING_COLOR));
        assert_eq!(lookup("heading_color"), Some(&HEADING_COLOR));
        assert_eq!(lookup("list_bullet_sprite_3"), Some(&LIST_BULLET_SPRITE));
        assert_eq!(lookup("rule_sprite"), Some(&RULE_SPRITE));
    }
}
