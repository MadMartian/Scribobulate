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
//!
//! **A key's [`Bound`] — its clamp range and its last-resort floor — lives here too,
//! and that is the half the registry originally left out.** `sdd/SCHEMA.md` states a
//! key's four properties uniformly (type, default, clamp range, optional narrowing);
//! the registry expressed two, and the other two were open-coded in `resolve.rs` as 22
//! `F_*` constants and 5 range constants, re-paired with their key **by hand, per key**,
//! inside a 146-line struct literal. `METRIC_RANGE` alone was passed at 13 call sites.
//! Nothing checked a pairing, so a key wired to the wrong range or the wrong floor
//! compiled and passed. Adding a key is now one line here and one field on the model.

use super::value::Clamp;

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
    /// A whole-number metric, clamped to its key's [`Bound`].
    Int,
    /// A fractional scale, clamped to its key's [`Bound`].
    Float,
}

/// A key's **clamp range and last-resort floor** — the two properties SCHEMA states
/// for every key and the registry used not to carry.
///
/// A malformed or hostile theme (`list_step = -5`, or `10000`) must not be able to
/// break layout (TDD 18.11), and resolution must be TOTAL: every geometry, typography
/// and overlay key has to land on a value even where `[themes.system]` says nothing.
/// Clamping rather than rejecting keeps a merely over-enthusiastic theme usable.
///
/// **The floors are not a second source of truth.** Each equals the shipped
/// `data/themes.toml` `[themes.system]` value, so the data file stays the place a
/// human reads and edits — and `theme::tests::system` proves it for *every* key at
/// once rather than by a hand-written list that could omit one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Bound {
    /// No numeric bound and no floor: the value is `Option`-shaped all the way to the
    /// consumer, which decides what an absent one means (every colour a surface may
    /// legitimately inherit, plus text, fonts, glyphs and sprites).
    Inherited,
    /// A colour that must always resolve, so it walks past both sources to a literal.
    /// The overlay washes — annotation, find, `==mark==` — which have no surface to
    /// inherit from.
    Color { floor: &'static str },
    /// A decoration line style. `LineStyle` has no `Option` at the consumer, so an
    /// unrecognised or absent spelling lands here.
    Line { floor: crate::theme::LineStyle },
    /// A whole-number metric. `floor` is broadcast when it holds one value and read
    /// per slot when it holds the key's full width — `[4, 4, 2, 2, 2]` is five
    /// distinct floors, `[12]` is one floor at every level.
    Int {
        floor: &'static [i32],
        range: Clamp<i32>,
    },
    /// A fractional scale, with the same broadcast rule as [`Bound::Int`].
    Float {
        floor: &'static [f64],
        range: Clamp<f64>,
    },
}

impl Bound {
    /// This key's integer floor at slot `idx`, broadcasting a single-value floor.
    ///
    /// Panics for a key whose bound is not [`Bound::Int`] — which is a registry
    /// declaration error, not a theme-file one, and is caught by
    /// `every_key_carries_the_bound_its_kind_requires` rather than by a user.
    pub(crate) fn int_floor(&self, idx: usize) -> i32 {
        match self {
            Bound::Int { floor, .. } => floor[idx.min(floor.len() - 1)],
            _ => unreachable!("an Int accessor read a key whose bound is {self:?}"),
        }
    }

    /// This key's integer clamp range.
    pub(crate) fn int_range(&self) -> Clamp<i32> {
        match self {
            Bound::Int { range, .. } => *range,
            _ => unreachable!("an Int accessor read a key whose bound is {self:?}"),
        }
    }

    /// This key's float floor at slot `idx`, broadcasting a single-value floor.
    pub(crate) fn float_floor(&self, idx: usize) -> f64 {
        match self {
            Bound::Float { floor, .. } => floor[idx.min(floor.len() - 1)],
            _ => unreachable!("a Float accessor read a key whose bound is {self:?}"),
        }
    }

    /// This key's float clamp range.
    pub(crate) fn float_range(&self) -> Clamp<f64> {
        match self {
            Bound::Float { range, .. } => *range,
            _ => unreachable!("a Float accessor read a key whose bound is {self:?}"),
        }
    }

    /// This key's line-style floor.
    pub(crate) fn line_floor(&self) -> crate::theme::LineStyle {
        match self {
            Bound::Line { floor } => *floor,
            _ => unreachable!("a Line accessor read a key whose bound is {self:?}"),
        }
    }

    /// This key's last-resort colour literal, for the overlay washes that must always
    /// resolve. `None` for every colour a surface may legitimately inherit.
    pub(crate) fn color_floor(&self) -> Option<&'static str> {
        match self {
            Bound::Color { floor } => Some(floor),
            _ => None,
        }
    }
}

/// **Which rendering surfaces a key must reach.**
///
/// The registry closed the drift hole for *parsing* — an unknown key now warns (TDD
/// 18.33) — and left it wide open for *consumption*. A key declared here but never read
/// by a sink is **worse** than an unknown one: `ThemeSpec::validate` admits it WITHOUT
/// a warning, because `keys::lookup` claims it, so it is accepted, SCHEMA-documented
/// and completely inert with no log line at all. Nothing asserts a key is *used*, so it
/// was a completeness obligation on the author until this table existed — and eleven keys
/// duly reached two surfaces of three.
///
/// An excluded surface carries its reason, so an exception is *stated* rather than
/// merely unmeasured. `theme::tests::sinks` sweeps the registry against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reach {
    /// The reading preview: its generated CSS, its `GtkTextTag` set, and the resolved
    /// decorations `snapshot_layer` paints from.
    pub preview: bool,
    /// The HTML export sink's whole artefact.
    pub html: bool,
    /// The PDF export sink's laid-out page.
    pub pdf: bool,
    /// Why a surface is excluded. Required whenever one is — an unexplained `false` is
    /// indistinguishable from a key somebody forgot to wire.
    pub why: &'static str,
    /// TOML a probe must state **alongside** this key for it to express anything.
    ///
    /// Some keys are gated on another: a band's radius and padding need a band, a
    /// gradient's second stop needs a first, a heading rule's colour needs a rule. That
    /// dependency is real, and the sweep has to know it — but writing it here also
    /// makes it **declared**, which is the half that was missing. F-BAND-001 was
    /// exactly an undeclared one, and it read as "the theme stated nothing".
    pub needs: &'static str,
}

impl Reach {
    /// Every rendering surface. The default, and what a new key should almost always be.
    const ALL: Reach = Reach {
        preview: true,
        html: true,
        pdf: true,
        why: "",
        needs: "",
    };

    /// Every surface, but only once `needs` is stated too.
    const fn gated_on(needs: &'static str) -> Reach {
        Reach {
            needs,
            ..Reach::ALL
        }
    }

    /// The preview only, with the reason the artefacts are excluded.
    const fn preview_only(why: &'static str) -> Reach {
        Reach {
            preview: true,
            html: false,
            pdf: false,
            why,
            needs: "",
        }
    }

    /// Reaches no rendering surface at all — a key about the theme itself rather than
    /// about how a document looks.
    const fn none(why: &'static str) -> Reach {
        Reach {
            preview: false,
            html: false,
            pdf: false,
            why,
            needs: "",
        }
    }

    /// Every surface but the page.
    const fn not_on_paper(why: &'static str, needs: &'static str) -> Reach {
        Reach {
            preview: true,
            html: true,
            pdf: false,
            why,
            needs,
        }
    }
}

/// The metric clamp shared by every decoration metric: no negative sizes, and nothing
/// wide enough to push the text column off its own viewport.
const METRIC: Clamp<i32> = Clamp { min: 0, max: 400 };
/// A list step of 0 would stack every nesting depth in one column and bury the drawn
/// markers under the text, so this one has a positive floor.
const LIST_STEP_R: Clamp<i32> = Clamp { min: 4, max: 400 };
const SCALE: Clamp<f64> = Clamp {
    min: 0.25,
    max: 8.0,
};
const WEIGHT: Clamp<i32> = Clamp {
    min: 100,
    max: 1000,
};
const RISE: Clamp<i32> = Clamp { min: -64, max: 64 };

// Constructors, so a table row reads as data rather than as a struct literal. `const
// fn` rather than macro arms: the table's `|` tail is parsed as one `expr`, which is
// what keeps the grammar unambiguous when a floor is itself a bracketed list.
const fn int(floor: &'static [i32], range: Clamp<i32>) -> Bound {
    Bound::Int { floor, range }
}
const fn float(floor: &'static [f64], range: Clamp<f64>) -> Bound {
    Bound::Float { floor, range }
}
const fn line(floor: crate::theme::LineStyle) -> Bound {
    Bound::Line { floor }
}
const fn color(floor: &'static str) -> Bound {
    Bound::Color { floor }
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

/// The slot a Markdown heading of `level` occupies — the h6→h5 fold, defined once.
///
/// **This is the whole fold, and it is the only copy.** It was written out by hand in
/// five places (`renderer/emit.rs`'s tag choice, `renderer/end.rs`'s `level_index`,
/// `outline_view.rs`'s row class, `export/pdf/decide.rs`'s scale index and
/// `export/html.rs`'s stylesheet loop), plus a sixth defensive re-clamp in
/// `codeview/mod.rs` — and two of them had already drifted: three hardcoded `4` or
/// `H5` while one derived its bound from [`HEADING_LEVELS`], so a change to that
/// constant would have moved some surfaces and not others. `renderer/mod.rs` even
/// documented `HeadingSpan::level_index` as *"computed once here so the paint path
/// indexes rather than re-deriving a fold that would then have two definitions to
/// disagree"* while being the second of five.
///
/// `saturating_sub` rather than `- 1`: `level` arrives from `pulldown_cmark` where it
/// is 1-based, but a `0` would otherwise wrap to `usize::MAX` and index nothing.
///
/// SCHEMA.md § Heading keys are per level states the rule this implements: there are
/// five levels, not six, and the fold happens *before a tag is ever chosen*.
pub(crate) fn heading_slot(level: u8) -> usize {
    (level as usize).saturating_sub(1).min(HEADING_LEVELS - 1)
}

#[cfg(test)]
mod slot_tests {
    use super::{heading_slot, HEADING_LEVELS};

    /// Every level Markdown can express, plus the two out-of-range values that reach
    /// this function only through a bug — pinned so the fold cannot quietly change
    /// shape on one surface, which is the drift it was extracted to end.
    #[test]
    fn every_heading_level_folds_into_a_slot_the_arrays_can_index() {
        let got: Vec<usize> = (0..=8u8).map(heading_slot).collect();
        assert_eq!(got, vec![0, 0, 1, 2, 3, 4, 4, 4, 4]);
        // The bound is derived from the constant, not written as a literal: three of
        // the five sites this replaced hardcoded `4`, so raising HEADING_LEVELS would
        // have moved some surfaces and left the others behind.
        assert!(got.iter().all(|&i| i < HEADING_LEVELS));
    }
}

/// Bullet nesting-depth tiers: depth 1, depth 2, depth 3-and-deeper.
pub(crate) const BULLET_TIERS: usize = 3;

/// One key of the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Key {
    /// The bare spelling, as a theme file writes it.
    pub name: &'static str,
    pub kind: Kind,
    pub levelling: Levelling,
    pub bound: Bound,
    pub reach: Reach,
    /// **A consumer that reads this key's BARE spelling directly, outside the
    /// per-level walk** — named, so the exception is stated rather than inferred.
    /// Empty for every key whose only reader is its own levelling, which is almost
    /// all of them.
    ///
    /// It exists because the two facts it reconciles live in different files. A
    /// levelled key's bare form is normally reachable only as the tail of a fallback
    /// chain, so a theme that states every narrowed spelling has made the bare one
    /// unreachable — which [`Key::bare_shadow`] reports. Three keys break that: the
    /// table header takes bare `heading_color`/`heading_font` when it states no ink or
    /// face of its own (TDD 18.30), and the task marker takes bare `list_marker_color`
    /// when it states no colour of its own. For those, a fully levelled theme's bare
    /// key still applies, and reporting it would be a false positive — the failure
    /// mode a diagnostic never recovers from, because it teaches its reader to ignore
    /// it.
    ///
    /// **Declared, not remembered:** [`super::sources::Sources::bare`] asserts that
    /// the key it is reading is either flat or names its reader here, so a new bare
    /// read of a levelled key cannot be added without this line.
    pub bare_reader: &'static str,
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

    /// **The spellings that shadow this key's bare form at every slot it could reach**
    /// — or `None` where the bare form still applies somewhere, which includes the
    /// case where it is not stated at all.
    ///
    /// `stated` answers whether one spelling is present in the theme being checked.
    /// The caller supplies a single theme's merged spec, and that is the whole of the
    /// question: **within** a source a narrower spelling wins, **between** sources the
    /// source wins (SCHEMA § Key resolution), so no other theme can shadow this one's
    /// bare key and none can un-shadow it. A key that loses only to the *selected*
    /// theme has not stopped applying — it applies whenever a different theme is
    /// selected — and reporting that would be the false positive this returns `None`
    /// for.
    ///
    /// **Precision is the whole design.** Three conditions each return `None`, and
    /// each is a case where the bare key does something: it is not stated; some slot's
    /// chain reaches it before any narrower spelling the theme states; or a consumer
    /// reads it bare regardless of the levelling ([`Key::bare_reader`]). Only "stated,
    /// and beaten at every level, with nothing else reading it" is reportable — a bare
    /// key that loses at h1-h3 and wins at h4-h5 is doing its job.
    ///
    /// Written over the registry rather than over one key family, because the registry
    /// is where levelling lives and a per-family predicate would be a second list to
    /// keep in step. It is deliberately not restricted to [`Levelling::Heading`] even
    /// though that is the only levelling it can fire for today: a `Flat` key and a
    /// `Depth` key both reach their bare form at slot 0 by construction
    /// ([`Key::fallbacks`]), so they fall out `None` from the same arithmetic rather
    /// than from a special case that would have to be revisited if a chain ever
    /// changed shape.
    pub(crate) fn bare_shadow(&self, stated: impl Fn(&str) -> bool) -> Option<Vec<String>> {
        if !stated(self.name) || !self.bare_reader.is_empty() {
            return None;
        }
        let mut shadows: Vec<String> = Vec::new();
        for idx in 0..self.slots() {
            // Every chain ends at the bare name (`every_chain_ends_at_the_bare_key`),
            // which `stated` has just answered for, so a winner always exists.
            let winner = self.fallbacks(idx).into_iter().find(|s| stated(s))?;
            if winner == self.name {
                return None;
            }
            if !shadows.contains(&winner) {
                shadows.push(winner);
            }
        }
        Some(shadows)
    }
}

/// Declare the vocabulary: a constant per key plus the [`KEYS`] table, from one line
/// each.
///
/// Four optional tails, in order: the **levelling** word (default
/// [`Levelling::Flat`]); after an `@`, the key's **[`Key::bare_reader`]** (default
/// none); after a `|`, the key's **[`Bound`]** (default [`Bound::Inherited`]); and
/// after a `,`, its **[`Reach`]** (default [`Reach::ALL`]). A key's
/// floor and clamp range therefore sit on the same line as its name and type, where
/// SCHEMA already puts them — rather than in a separate constant that has to be
/// re-paired with the key by hand at the resolution site.
///
/// The bare-reader tail is spelled `@` and sits **before** the bound rather than after
/// the reach, where it would read more naturally, because `macro_rules!` permits only
/// `=>`, `,` or `;` after an `expr` fragment — so nothing can follow the `|` bound but
/// the `,` reach.
macro_rules! keys {
    (@lev) => { Levelling::Flat };
    (@lev $l:ident) => { Levelling::$l };
    (@bare) => { "" };
    (@bare $b:literal) => { $b };
    (@bound) => { Bound::Inherited };
    (@bound $b:expr) => { $b };
    (@reach) => { Reach::ALL };
    (@reach $r:expr) => { $r };
    ($(
        $konst:ident = $name:literal : $kind:ident $($lev:ident)? $(@ $bare:literal)? $(| $bound:expr)? $(, $reach:expr)? ;
    )+) => {
        $(
            pub(crate) const $konst: Key = Key {
                name: $name,
                kind: Kind::$kind,
                levelling: keys!(@lev $($lev)?),
                bound: keys!(@bound $($bound)?),
                reach: keys!(@reach $($reach)?),
                bare_reader: keys!(@bare $($bare)?),
            };
        )+
        /// Every key this build knows, in the order the schema documents them.
        pub(crate) const KEYS: &[Key] = &[ $($konst),+ ];
    };
}

keys! {
    // ── identity and the base colours ────────────────────────────────────────
    NAME                   = "name"                    : Text,
                             Reach::none("the theme's own label, in the picker — not a document style");
    SYMBOL                 = "symbol"                  : Text,
                             Reach::none("the theme's picker glyph — not a document style");
    BACKGROUND             = "background"              : Color;
    FOREGROUND             = "foreground"              : Color;
    ACCENT_COLOR           = "accent_color"            : Color;
    FONT_FAMILY            = "font_family"             : Font;
    SYNTECT_THEME          = "syntect_theme"           : Text,
                             Reach::preview_only("names a syntect scheme for the LIVE code highlighter; \
                                                  neither sink runs syntect — an exported code block is \
                                                  plain monospace on both");

    // ── headings (every one of these also takes an `_h1`…`_h5` spelling) ─────
    HEADING_COLOR          = "heading_color"           : Color  Heading
                             @ "the table header's ink, where the theme states no table_head_fg (TDD 18.30)";
    HEADING_FONT           = "heading_font"            : Font   Heading
                             @ "the table header's face, where the theme states no face of its own (TDD 18.30)";
    HEADING_SCALE          = "heading_scale"           : Float  Heading
                             | float(&[2.2, 1.8, 1.48, 1.2, 1.0], SCALE);
    HEADING_WEIGHT         = "heading_weight"          : Int    Heading | int(&[700], WEIGHT);
    // No heading rule is drawn today, on either side.
    HEADING_OVERLINE       = "heading_overline"        : Line   Heading | line(crate::theme::LineStyle::None);
    HEADING_UNDERLINE      = "heading_underline"       : Line   Heading | line(crate::theme::LineStyle::None);
    HEADING_UNDERLINE_COLOR = "heading_underline_color" : Color Heading,
                             Reach::gated_on("heading_underline = \"single\"\n");
    HEADING_BAND_COLOR     = "heading_band_color"      : Color  Heading;
    HEADING_BAND_GRADIENT_TO_COLOR
                           = "heading_band_gradient_to_color" : Color Heading,
                             Reach::gated_on("heading_band_color = \"#123456\"\n");
    HEADING_BAND_SPRITE    = "heading_band_sprite"     : Sprite Heading;
    // No heading carries a band until a theme states a fill for its level, so the
    // radius is only ever consulted for a band that exists.
    HEADING_BAND_RADIUS    = "heading_band_radius"     : Int    Heading | int(&[0], METRIC),
                             Reach::not_on_paper("the page draws line by line, so a wrapped band is \
                                                  several abutting rects and rounding each would pinch \
                                                  it at every interior join — SCHEMA states the limit",
                                                 "heading_band_color = \"#123456\"\n");
    // NON-ZERO, unlike every other decoration default here, and deliberately so: a
    // band's padding is not an opt-in flourish but part of drawing a band correctly.
    // It is inert anyway on a theme that bands nothing, because the inset is applied
    // per level and only where that level HAS a band — the gate, not the value, is
    // what keeps System byte-identical (TDD 18.2), and every theme that already ships
    // a band gets the fix with no content edit.
    HEADING_BAND_PADDING   = "heading_band_padding"    : Int    Heading | int(&[12], METRIC),
                             Reach::gated_on("heading_band_color = \"#123456\"\n");
    // Zero, because the heading tags set no `pixels_above_lines` at all before this
    // key existed — the floor IS today's rendering, which is what keeps System
    // byte-identical (TDD 18.2). Not symmetric with the below-floor by accident: only
    // space-below was ever expressed.
    HEADING_SPACE_ABOVE    = "heading_space_above"     : Int    Heading | int(&[0], METRIC);
    HEADING_SPACE_BELOW    = "heading_space_below"     : Int    Heading
                             | int(&[4, 4, 2, 2, 2], METRIC);

    // ── body and inline text ─────────────────────────────────────────────────
    BOLD_WEIGHT            = "bold_weight"             : Int    | int(&[700], WEIGHT);
    SUPSUB_SCALE           = "supsub_scale"            : Float  | float(&[0.72], SCALE);
    SUPERSCRIPT_RISE       = "superscript_rise"        : Int    | int(&[4], RISE);
    SUBSCRIPT_RISE         = "subscript_rise"          : Int    | int(&[-2], RISE);
    STRIKETHROUGH_COLOR    = "strikethrough_color"     : Color;
    // Neutral highlighter yellow as the last-resort floor; each bundled theme
    // overrides it with a page-appropriate wash (data/themes.toml).
    MARK_BG                = "mark_bg"                 : Color  | color("#fff59d_88");
    MARK_FG                = "mark_fg"                 : Color;
    CODE_INLINE_BG         = "code_inline_bg"          : Color;
    CODE_BLOCK_BG          = "code_block_bg"           : Color;

    // ── links ────────────────────────────────────────────────────────────────
    LINK_COLOR             = "link_color"              : Color;
    // A body link has been underlined with a single line since before themes existed,
    // so unlike the heading rule's floor this one is NOT "none" — it is the shipped
    // look, and changing it would move System (TDD 18.2).
    LINK_UNDERLINE         = "link_underline"          : Line   | line(crate::theme::LineStyle::Single);
    LINK_UNDERLINE_COLOR   = "link_underline_color"    : Color;

    // ── lists (the ⓷ keys also take `_2` and `_3` spellings) ─────────────────
    LIST_MARKER_COLOR      = "list_marker_color"       : Color  Depth
                             @ "the task marker's ink, where the theme states no list_task_marker_color";
    LIST_TASK_MARKER_COLOR = "list_task_marker_color"  : Color;
    LIST_BULLET_GLYPH      = "list_bullet_glyph"       : Glyph  Depth;
    LIST_ORDERED_GLYPH     = "list_ordered_glyph"      : Glyph;
    LIST_TASK_GLYPH        = "list_task_glyph"         : Glyph;
    LIST_TASK_CHECKED_GLYPH = "list_task_checked_glyph" : Glyph;
    LIST_BULLET_SPRITE     = "list_bullet_sprite"      : Sprite Depth;
    LIST_ORDERED_SPRITE    = "list_ordered_sprite"     : Sprite;
    LIST_TASK_SPRITE       = "list_task_sprite"        : Sprite;
    LIST_TASK_CHECKED_SPRITE = "list_task_checked_sprite" : Sprite;
    LIST_STEP              = "list_step"               : Int    | int(&[28], LIST_STEP_R);
    LIST_ITEM_GAP          = "list_item_gap"           : Int    | int(&[8], METRIC);

    // ── blockquote ───────────────────────────────────────────────────────────
    BLOCKQUOTE_BAR_COLOR   = "blockquote_bar_color"    : Color;
    BLOCKQUOTE_BAR_SPRITE  = "blockquote_bar_sprite"   : Sprite;
    BLOCKQUOTE_BAR_WIDTH   = "blockquote_bar_width"    : Int    | int(&[3], METRIC);
    BLOCKQUOTE_TEXT_GAP    = "blockquote_text_gap"     : Int    | int(&[10], METRIC);
    BLOCKQUOTE_BG          = "blockquote_bg"           : Color;
    BLOCKQUOTE_FG          = "blockquote_fg"           : Color;

    // ── table ────────────────────────────────────────────────────────────────
    TABLE_BORDER_COLOR     = "table_border_color"      : Color;
    TABLE_BORDER_WIDTH     = "table_border_width"      : Int    | int(&[1], METRIC);
    TABLE_HEAD_BG          = "table_head_bg"           : Color;
    TABLE_HEAD_FG          = "table_head_fg"           : Color;
    TABLE_CELL_PADDING_V   = "table_cell_padding_v"    : Int    | int(&[4], METRIC);
    TABLE_CELL_PADDING_H   = "table_cell_padding_h"    : Int    | int(&[10], METRIC);
    TABLE_CELL_RADIUS      = "table_cell_radius"       : Int    | int(&[0], METRIC),
                             Reach::not_on_paper("the page draws line by line, so a wrapped cell is \
                                                  several abutting rects and rounding each would pinch \
                                                  the box at every interior join — SCHEMA states the \
                                                  limit", "");

    // ── horizontal rule ──────────────────────────────────────────────────────
    RULE_COLOR             = "rule_color"              : Color;
    RULE_SPRITE            = "rule_sprite"             : Sprite;
    RULE_SPACE             = "rule_space"              : Int    | int(&[4], METRIC);
    RULE_THICKNESS         = "rule_thickness"          : Int    | int(&[1], METRIC);

    // ── selection ────────────────────────────────────────────────────────────
    SELECTION_BG           = "selection_bg"            : Color,
                             Reach::preview_only("an artefact has no selection to fill");
    SELECTION_FG           = "selection_fg"            : Color,
                             Reach::preview_only("an artefact has no selection to ink");

    // ── annotations and find ─────────────────────────────────────────────────
    // The overlay washes: no surface to inherit from, so each walks to a literal.
    ANNOTATION_HL_COLOR    = "annotation_hl_color"     : Color  | color("#FFD133_61");
    ANNOTATION_CHIP_BG     = "annotation_chip_bg"      : Color;
    ANNOTATION_CHIP_FG     = "annotation_chip_fg"      : Color;
    ANNOTATION_CHIP_SPRITE = "annotation_chip_sprite"  : Sprite;
    FIND_HL_ALL_COLOR      = "find_hl_all_color"       : Color  | color("#f6d32d"),
                             Reach::preview_only("find is a live feature; an artefact carries no matches");
    FIND_HL_CURRENT_COLOR  = "find_hl_current_color"   : Color  | color("#ff7800"),
                             Reach::preview_only("find is a live feature; an artefact carries no matches");
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

    /// **A key's [`Kind`] and its [`Bound`] must agree, for every key.**
    ///
    /// The two are declared on the same line but by different words, so nothing else
    /// stops a `Float` key being given an `int(...)` bound — which compiles, and then
    /// panics the first time a theme states that key, because the accessor reads the
    /// wrong arm. This is the check that the registry's two halves describe the same
    /// key.
    ///
    /// It also pins the shape of the floor: a per-level floor is either one value
    /// (broadcast to every level) or exactly the key's own slot count. Anything
    /// between the two silently gives some level the wrong default, since
    /// [`Bound::int_floor`] clamps the index rather than panicking.
    #[test]
    fn every_key_carries_the_bound_its_kind_requires() {
        for key in KEYS {
            let ok = match (key.kind, key.bound) {
                // A numeric key MUST carry a numeric bound: resolution is total, so
                // there is nowhere else for its default to come from.
                (Kind::Int, Bound::Int { .. }) | (Kind::Float, Bound::Float { .. }) => true,
                (Kind::Line, Bound::Line { .. }) => true,
                // A colour is the one kind that legitimately takes either: most
                // inherit from the surface under them, and only the overlay washes
                // (which have no such surface) carry a literal floor.
                (Kind::Color, Bound::Color { .. } | Bound::Inherited) => true,
                (Kind::Text | Kind::Font | Kind::Glyph | Kind::Sprite, Bound::Inherited) => true,
                _ => false,
            };
            assert!(
                ok,
                "{}: kind {:?} cannot carry bound {:?}",
                key.name, key.kind, key.bound
            );
            let width = match key.bound {
                Bound::Int { floor, .. } => Some(floor.len()),
                Bound::Float { floor, .. } => Some(floor.len()),
                _ => None,
            };
            if let Some(n) = width {
                assert!(
                    n == 1 || n == key.slots(),
                    "{}: a floor of {n} values fits neither one-value broadcast nor \
                     this key's {} slots",
                    key.name,
                    key.slots()
                );
            }
        }
    }

    /// Every declared exception carries a reason. An unexplained `false` is
    /// indistinguishable from a key somebody forgot to wire, which is the whole
    /// condition `theme::tests::sinks` exists to detect.
    #[test]
    fn every_unreached_surface_states_why() {
        for key in KEYS {
            let complete = key.reach.preview && key.reach.html && key.reach.pdf;
            assert_eq!(
                complete,
                key.reach.why.is_empty(),
                "{}: a key that reaches every surface needs no reason, and one that \
                 does not needs one (reach = {:?})",
                key.name,
                key.reach
            );
        }
    }

    /// The registry's own constructors build the shapes they are named for.
    ///
    /// They are `const fn`s evaluated inside the `keys!` table, so nothing calls them
    /// at run time and the coverage instrument scores every line of them zero — which
    /// distorts the gate downward for code that is in fact exercised at every build.
    /// This calls them, and asserts the shape: a `preview_only` that quietly set
    /// `pdf: true` would silently licence a key on a surface it never reaches.
    #[test]
    fn every_reach_constructor_builds_the_shape_it_names() {
        assert_eq!(
            Reach::ALL,
            Reach {
                preview: true,
                html: true,
                pdf: true,
                why: "",
                needs: ""
            }
        );
        let gated = Reach::gated_on("k = 1\n");
        assert!(gated.preview && gated.html && gated.pdf);
        assert_eq!(gated.needs, "k = 1\n");
        assert!(gated.why.is_empty(), "a fully reaching key needs no reason");

        let preview = Reach::preview_only("why");
        assert_eq!(
            (preview.preview, preview.html, preview.pdf, preview.why),
            (true, false, false, "why")
        );
        let none = Reach::none("why");
        assert_eq!(
            (none.preview, none.html, none.pdf, none.why),
            (false, false, false, "why")
        );
        let paper = Reach::not_on_paper("why", "k = 1\n");
        assert_eq!(
            (paper.preview, paper.html, paper.pdf, paper.why, paper.needs),
            (true, true, false, "why", "k = 1\n")
        );
    }

    /// The bound constructors, likewise — and the broadcast rule, which is the one
    /// piece of behaviour in them.
    #[test]
    fn every_bound_constructor_builds_the_shape_it_names() {
        let i = int(&[4, 5, 6], Clamp { min: 0, max: 9 });
        assert_eq!(i.int_range(), Clamp { min: 0, max: 9 });
        assert_eq!((i.int_floor(0), i.int_floor(1), i.int_floor(2)), (4, 5, 6));
        // A single-value floor BROADCASTS; an index past the end clamps to the last.
        assert_eq!(int(&[7], Clamp { min: 0, max: 9 }).int_floor(4), 7);
        assert_eq!(i.int_floor(99), 6);

        let f = float(&[1.0, 2.0], Clamp { min: 0.0, max: 9.0 });
        assert_eq!(f.float_range(), Clamp { min: 0.0, max: 9.0 });
        assert_eq!((f.float_floor(0), f.float_floor(1)), (1.0, 2.0));

        assert_eq!(
            line(crate::theme::LineStyle::Double).line_floor(),
            crate::theme::LineStyle::Double
        );
        assert_eq!(color("#abcdef").color_floor(), Some("#abcdef"));
        assert_eq!(Bound::Inherited.color_floor(), None);
    }

    /// Reading a key through the WRONG accessor is a registry declaration error, and
    /// it fails loudly rather than answering something plausible.
    ///
    /// `every_key_carries_the_bound_its_kind_requires` is what stops it reaching a
    /// user; this pins that the fallback is a panic and not a silent default.
    #[test]
    #[should_panic(expected = "an Int accessor read a key whose bound is")]
    fn reading_an_inherited_key_as_an_int_panics() {
        let _ = Bound::Inherited.int_floor(0);
    }

    #[test]
    #[should_panic(expected = "a Float accessor read a key whose bound is")]
    fn reading_an_int_key_as_a_float_panics() {
        let _ = int(&[1], Clamp { min: 0, max: 2 }).float_range();
    }

    #[test]
    #[should_panic(expected = "a Line accessor read a key whose bound is")]
    fn reading_a_colour_key_as_a_line_panics() {
        let _ = color("#000000").line_floor();
    }

    /// A clamp range that does not contain its own floor would make an unstated key
    /// resolve to a value a STATED one could never take — a default outside the
    /// vocabulary's own bounds, which no consumer would expect and no theme could
    /// reproduce.
    #[test]
    fn every_floor_lies_inside_its_own_clamp_range() {
        for key in KEYS {
            match key.bound {
                Bound::Int { floor, range } => {
                    let Clamp { min: lo, max: hi } = range;
                    for (i, &f) in floor.iter().enumerate() {
                        assert!(
                            (lo..=hi).contains(&f),
                            "{}[{i}]: floor {f} is outside its clamp range {lo}..={hi}",
                            key.name
                        );
                    }
                }
                Bound::Float { floor, range } => {
                    let Clamp { min: lo, max: hi } = range;
                    for (i, &f) in floor.iter().enumerate() {
                        assert!(
                            (lo..=hi).contains(&f),
                            "{}[{i}]: floor {f} is outside its clamp range {lo}..={hi}",
                            key.name
                        );
                    }
                }
                _ => {}
            }
        }
    }

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
            // …and the shipped themes file must not TEACH one either. It is the
            // primary documentation a theme author reads, and a comment quoting a
            // dead spelling costs that author a `warn` and a decoration that does
            // nothing. Backticked, because several of these words ("link", "rule",
            // "accent") are also ordinary English in the same prose — the backticks
            // are what make it a citation of a key rather than a sentence.
            assert!(
                !crate::theme::BUILTIN_THEMES_TOML.contains(&format!("`{retired}`")),
                "data/themes.toml quotes the retired spelling `{retired}` — use the \
                 live key, or the comment teaches a key that warns and does nothing"
            );
        }
    }

    /// **A bare key is reported shadowed only when it can apply NOWHERE.**
    ///
    /// The three `None` conditions, each a case where the key does something, and each
    /// the difference between a diagnostic people read and one they filter out.
    #[test]
    fn a_bare_key_is_shadowed_only_when_every_level_is_narrowed() {
        let stated = |set: &'static [&'static str]| move |s: &str| set.contains(&s);

        // Not stated at all: nothing to report.
        assert_eq!(
            HEADING_SPACE_ABOVE.bare_shadow(stated(&["heading_space_above_h1"])),
            None
        );
        // Narrowed at four of five levels — h5 still takes the bare key.
        assert_eq!(
            HEADING_SPACE_ABOVE.bare_shadow(stated(&[
                "heading_space_above",
                "heading_space_above_h1",
                "heading_space_above_h2",
                "heading_space_above_h3",
                "heading_space_above_h4",
            ])),
            None
        );
        // All five: the bare key can reach no level, and the report names each winner.
        assert_eq!(
            HEADING_SPACE_ABOVE.bare_shadow(stated(&[
                "heading_space_above",
                "heading_space_above_h1",
                "heading_space_above_h2",
                "heading_space_above_h3",
                "heading_space_above_h4",
                "heading_space_above_h5",
            ])),
            Some(vec![
                "heading_space_above_h1".to_string(),
                "heading_space_above_h2".to_string(),
                "heading_space_above_h3".to_string(),
                "heading_space_above_h4".to_string(),
                "heading_space_above_h5".to_string(),
            ])
        );
        // …and a key with a consumer that reads it bare is never shadowed, however
        // completely a theme narrows it.
        assert_eq!(
            HEADING_COLOR.bare_shadow(stated(&[
                "heading_color",
                "heading_color_h1",
                "heading_color_h2",
                "heading_color_h3",
                "heading_color_h4",
                "heading_color_h5",
            ])),
            None
        );
    }

    /// **No `Flat` or `Depth` key can be reported shadowed, whatever a theme states.**
    ///
    /// Not an exception in the predicate — it falls out of the chains. A flat key's
    /// chain is its bare form, and a depth key's tier-1 chain is too
    /// (`a_heading_key_falls_back_to_its_bare_form_and_a_depth_key_walks_shallower`),
    /// so both reach the bare form at slot 0. Swept over the registry rather than
    /// argued, so a chain that ever changed shape would be caught here rather than by a
    /// user reading a warning about a key that applies.
    #[test]
    fn only_a_heading_key_can_ever_be_shadowed() {
        // The most hostile input available: a theme stating EVERY spelling of every key.
        let everything = |_: &str| true;
        for key in KEYS {
            let shadowed = key.bare_shadow(everything).is_some();
            let reportable = key.levelling == Levelling::Heading && key.bare_reader.is_empty();
            assert_eq!(
                shadowed, reportable,
                "{} is {:?} with bare_reader {:?} — shadowed reported as {shadowed}",
                key.name, key.levelling, key.bare_reader
            );
        }
    }

    /// **A declared bare reader belongs to a LEVELLED key**, or it says nothing.
    ///
    /// Every reader of a flat key reads it bare, so declaring one there would be noise
    /// that reads like a load-bearing exception — and the field's whole job is to be
    /// the one place a genuine exception is stated.
    #[test]
    fn only_a_levelled_key_declares_a_bare_reader() {
        for key in KEYS {
            assert!(
                key.bare_reader.is_empty() || key.levelling != Levelling::Flat,
                "{} is Flat and declares a bare_reader — every reader of a flat key \
                 reads it bare, so the declaration means nothing",
                key.name
            );
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
