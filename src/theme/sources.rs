//! Resolution's inner walk: two authored themes, read as one.
//!
//! [`Sources`] is where the resolution order becomes code. It answers one question —
//! *what does this key say at this slot?* — and answers it identically for every kind
//! and every levelling, which is what stops a colour and a metric from falling back
//! differently. `resolve` builds a `Theme` out of these answers; nothing else reads a
//! spec directly.

use super::keys::Key;
use super::spec::{expected, ThemeSpec, Value};
use super::{parse_color, sanitize_font_family};
use super::{CssSafeFontStack, LineStyle, MarkerGlyph};
use crate::sprite::SpriteRef;
use gtk::gdk;
use std::cell::RefCell;
use std::collections::HashSet;

/// The two links of the resolution order a `Theme` is resolved from: the selected
/// theme, then `[themes.system]`.
///
/// **Source outranks specificity, and specificity decides within a source.** The
/// selected theme's bare `heading_color` therefore beats `[themes.system]`'s
/// `heading_color_h1`: a theme that says "all my headings are this colour" has said
/// something about h1, and the base theme's narrower key is not a reason to ignore
/// it. Within one source the narrower spelling wins, which is what makes the bare
/// form a default rather than a competitor.
pub(crate) struct Sources<'a> {
    /// The selected theme's id — carried so a refusal can NAME the theme it came
    /// from. Without it every value-level refusal read "theme: unknown line style" and
    /// left the reader to find which of seven themes, and which of ~150 spellings,
    /// had said it (SCHEMA § Key resolution requires both).
    id: &'a str,
    selected: &'a ThemeSpec,
    system: &'a ThemeSpec,
    /// `(source index, spelling)` pairs the typed parse has already refused.
    ///
    /// A key's fallback chain revisits the same spelling once per slot — five times
    /// for a heading key, three for a depth key — so a single bad value used to be
    /// parsed, and reported, once per level. Recording the refusal makes the walk skip
    /// the re-parse entirely, which is both the diagnostic fix and the reason the
    /// answer is unchanged: `f` is pure over one `Value`, so a spelling that refused
    /// once refuses always.
    refused: RefCell<HashSet<(usize, String)>>,
}

impl<'a> Sources<'a> {
    pub(crate) fn new(id: &'a str, selected: &'a ThemeSpec, system: &'a ThemeSpec) -> Sources<'a> {
        Sources {
            id,
            selected,
            system,
            refused: RefCell::new(HashSet::new()),
        }
    }
}

impl Sources<'_> {
    /// Walk the resolution order for one slot of one key: each source in turn, and
    /// within it each spelling from the most specific to the bare form.
    ///
    /// A value `f` refuses — an unparseable colour, a glyph too long to be one, a
    /// line style nobody spells — is **skipped, not fatal**, and the walk continues.
    /// That is the same clamp-rather-than-reject discipline every metric follows
    /// (TDD 18.11): a theme with one bad colour renders with one colour inherited,
    /// not as a theme that failed to load.
    fn pick<T>(&self, key: &Key, idx: usize, f: impl Fn(&Value) -> Option<T>) -> Option<T> {
        self.walk(key, key.fallbacks(idx), f)
    }

    /// The key's **bare** spelling only, in each source in turn.
    ///
    /// This is what a surface that is not a heading level reads: the table header
    /// takes `heading_color`/`heading_font` when it states no ink of its own (TDD
    /// 18.30), and taking h1's narrowed colour there instead would make a theme that
    /// distinguishes its h1 silently re-ink a table header it said nothing about.
    fn bare<T>(&self, key: &Key, f: impl Fn(&Value) -> Option<T>) -> Option<T> {
        self.walk(key, vec![key.name.to_string()], f)
    }

    fn walk<T>(
        &self,
        key: &Key,
        spellings: Vec<String>,
        f: impl Fn(&Value) -> Option<T>,
    ) -> Option<T> {
        for (which, spec) in [self.selected, self.system].into_iter().enumerate() {
            for spelling in &spellings {
                let Some(value) = spec.get(spelling) else {
                    continue;
                };
                if self.already_refused(which, spelling) {
                    continue;
                }
                match f(value) {
                    Some(found) => return Some(found),
                    None => self.report_refusal(which, key, spelling, value),
                }
            }
        }
        None
    }

    fn already_refused(&self, which: usize, spelling: &str) -> bool {
        self.refused
            .borrow()
            .contains(&(which, spelling.to_string()))
    }

    /// Report one value-level refusal, ONCE, naming the theme and the spelling.
    ///
    /// SCHEMA § Key resolution's rationale for the unknown-key warning applies just as
    /// hard here: silence makes a key that never applied indistinguishable from one
    /// that applied and did nothing. `validate` already reports an unknown spelling and
    /// a wrong TOML type; this covers the third refusal class, a value of the right
    /// TOML type that the key's own parser will not take — which is where the colour
    /// keys live, and colours are half the vocabulary.
    fn report_refusal(&self, which: usize, key: &Key, spelling: &str, value: &Value) {
        self.refused
            .borrow_mut()
            .insert((which, spelling.to_string()));
        let source = if which == 0 {
            self.id
        } else {
            super::SYSTEM_ID
        };
        log::warn!(
            "theme {source:?}: {spelling} = {} is not {} — ignored, the key falls \
             through to the next source",
            value.authored(),
            expected(key.kind)
        );
    }

    /// One slot's authored value, before any typed parse — for the tests that assert
    /// across the whole key family, where the kinds differ but the walk does not.
    #[cfg(test)]
    fn raw_at(&self, key: &Key, idx: usize) -> Option<Value> {
        self.pick(key, idx, |v| Some(v.clone()))
    }

    /// Every slot of a key, resolved independently. `N` is the key's own slot count,
    /// so the caller's array shape and the registry's levelling cannot disagree.
    fn each<T, const N: usize>(
        &self,
        key: &Key,
        f: impl Fn(&Value) -> Option<T>,
    ) -> [Option<T>; N] {
        // UNCONDITIONAL, not a `debug_assert`. `N` comes from the DESTINATION field's
        // array length and nothing in the type system ties it to `key.levelling`, so
        // `src.colors::<BULLET_TIERS>(&keys::HEADING_COLOR)` compiles — and under a
        // `debug_assert` the shipped binary would resolve three of five heading levels
        // and silently drop h4/h5, at exactly the moment (wiring a new levelled key)
        // the mismatch gets introduced. It costs one comparison per key per resolve.
        assert_eq!(
            N,
            key.slots(),
            "{} is declared {:?} and so carries {} slots, but is being read into an \
             array of {N} — the registry and the model field disagree",
            key.name,
            key.levelling,
            key.slots()
        );
        std::array::from_fn(|i| self.pick(key, i, &f))
    }

    // ── the typed accessors ──────────────────────────────────────────────────
    //
    // Each comes in two forms, and the difference is which question it answers. The
    // singular form reads the key's BARE value — what a theme said about the whole
    // construct. The array form reads one value per level or depth, each already
    // folded down its own fallback chain, so every consumer indexes and none of them
    // re-derives the fold.
    //
    // **None of them takes a floor or a range.** Those are the key's own
    // ([`crate::theme::keys::Bound`]), read off the `Key` the caller already had to
    // hand. Passing them made the resolution site re-pair every key with its floor and
    // its clamp BY HAND, per key, across a 146-line struct literal — `METRIC_RANGE`
    // alone at 13 call sites — with nothing checking a pairing, so a key wired to the
    // wrong range compiled and passed.

    pub(crate) fn text(&self, key: &Key) -> Option<String> {
        self.bare(key, |v| v.text().map(str::to_string))
    }

    pub(crate) fn colors<const N: usize>(&self, key: &Key) -> [Option<gdk::RGBA>; N] {
        self.each(key, |v| v.text().and_then(parse_color))
    }

    pub(crate) fn color(&self, key: &Key) -> Option<gdk::RGBA> {
        self.bare(key, |v| v.text().and_then(parse_color))
    }

    pub(crate) fn fonts<const N: usize>(&self, key: &Key) -> [Option<CssSafeFontStack>; N] {
        self.each(key, |v| v.text().and_then(sanitize_font_family))
    }

    pub(crate) fn font(&self, key: &Key) -> Option<CssSafeFontStack> {
        self.bare(key, |v| v.text().and_then(sanitize_font_family))
    }

    pub(crate) fn glyphs<const N: usize>(&self, key: &Key) -> [Option<MarkerGlyph>; N] {
        self.each(key, |v| v.text().and_then(MarkerGlyph::parse))
    }

    pub(crate) fn glyph(&self, key: &Key) -> Option<MarkerGlyph> {
        self.bare(key, |v| v.text().and_then(MarkerGlyph::parse))
    }

    pub(crate) fn sprites<const N: usize>(&self, key: &Key) -> [Option<SpriteRef>; N] {
        self.each(key, |v| v.sprite().cloned())
    }

    pub(crate) fn sprite(&self, key: &Key) -> Option<SpriteRef> {
        self.bare(key, |v| v.sprite().cloned())
    }

    /// A colour that must always resolve: the two sources, then the key's own
    /// last-resort literal.
    ///
    /// Only the overlay washes carry one — they have no surface to inherit from, so
    /// an unresolved one would be a wash of nothing. Every other colour is
    /// [`Bound::Inherited`] and stays `Option` all the way to its consumer.
    pub(crate) fn color_floored(&self, key: &Key) -> gdk::RGBA {
        let floor = key
            .bound
            .color_floor()
            .expect("color_floored reads a key declared Bound::Color");
        self.color(key)
            .or_else(|| parse_color(floor))
            .unwrap_or(gdk::RGBA::BLACK)
    }

    pub(crate) fn lines<const N: usize>(&self, key: &Key) -> [LineStyle; N] {
        let floor = key.bound.line_floor();
        let found: [Option<LineStyle>; N] = self.each(key, |v| v.text().and_then(LineStyle::parse));
        found.map(|s| s.unwrap_or(floor))
    }

    pub(crate) fn line(&self, key: &Key) -> LineStyle {
        self.bare(key, |v| v.text().and_then(LineStyle::parse))
            .unwrap_or_else(|| key.bound.line_floor())
    }

    pub(crate) fn ints<const N: usize>(&self, key: &Key) -> [i32; N] {
        let range = key.bound.int_range();
        let found: [Option<i32>; N] = self.each(key, |v| v.int());
        std::array::from_fn(|i| {
            found[i]
                .map(|n| range.apply(n))
                .unwrap_or_else(|| key.bound.int_floor(i))
        })
    }

    pub(crate) fn int(&self, key: &Key) -> i32 {
        self.bare(key, |v| v.int())
            .map(|n| key.bound.int_range().apply(n))
            .unwrap_or_else(|| key.bound.int_floor(0))
    }

    pub(crate) fn floats<const N: usize>(&self, key: &Key) -> [f64; N] {
        let range = key.bound.float_range();
        let found: [Option<f64>; N] = self.each(key, |v| v.float());
        std::array::from_fn(|i| {
            found[i]
                .map(|x| range.apply(x))
                .unwrap_or_else(|| key.bound.float_floor(i))
        })
    }

    pub(crate) fn float(&self, key: &Key) -> f64 {
        self.bare(key, |v| v.float())
            .map(|x| key.bound.float_range().apply(x))
            .unwrap_or_else(|| key.bound.float_floor(0))
    }
}

#[cfg(test)]
mod tests {
    use super::super::keys::{self, Kind};
    use super::super::spec::RawSpec;
    use super::*;

    /// A pair of authored values for a key of this kind: one for the bare spelling,
    /// one for a narrowed one, chosen so the two are distinguishable.
    fn sample(kind: Kind) -> (&'static str, &'static str) {
        match kind {
            Kind::Color => ("\"#111111\"", "\"#222222\""),
            Kind::Font => ("\"Georgia, serif\"", "\"Verdana, sans-serif\""),
            Kind::Line => ("\"single\"", "\"wavy\""),
            Kind::Glyph | Kind::Text => ("\"a\"", "\"b\""),
            Kind::Sprite => ("\"one.png\"", "\"two.png\""),
            Kind::Int => ("3", "7"),
            Kind::Float => ("1.25", "2.5"),
        }
    }

    fn spec(id: &str, body: &str) -> ThemeSpec {
        let raw: RawSpec = toml::from_str(body).expect("test fixture parses");
        ThemeSpec::validate(id, raw)
    }

    #[test]
    fn a_heading_level_overrides_the_bare_key_and_an_unstated_one_takes_it() {
        let selected = spec(
            "t",
            "heading_color = \"#112233\"\nheading_color_h2 = \"#445566\"\n",
        );
        let system = ThemeSpec::default();
        let src = Sources::new("t", &selected, &system);
        let levels = src.colors::<{ keys::HEADING_LEVELS }>(&keys::HEADING_COLOR);
        assert_eq!(levels[1], parse_color("#445566"));
        for level in [0, 2, 3, 4] {
            assert_eq!(levels[level], parse_color("#112233"), "level {level}");
        }
    }

    /// TDD 18.34 — source order decides between two themes, narrowing decides only
    /// within one.
    #[test]
    fn a_selected_themes_bare_key_outranks_the_system_themes_narrowed_one() {
        let selected = spec("t", "heading_color = \"#112233\"\n");
        let system = spec("system", "heading_color_h1 = \"#ff0000\"\n");
        let src = Sources::new("t", &selected, &system);
        assert_eq!(
            src.colors::<{ keys::HEADING_LEVELS }>(&keys::HEADING_COLOR)[0],
            parse_color("#112233")
        );

        let silent = ThemeSpec::default();
        let src = Sources::new("t", &silent, &system);
        assert_eq!(
            src.colors::<{ keys::HEADING_LEVELS }>(&keys::HEADING_COLOR)[0],
            parse_color("#ff0000"),
            "with the selected theme silent, the system theme's narrowed key applies"
        );
    }

    #[test]
    fn a_depth_key_falls_back_through_each_shallower_tier() {
        let selected = spec(
            "t",
            "list_marker_color = \"#111111\"\nlist_marker_color_2 = \"#222222\"\n",
        );
        let system = ThemeSpec::default();
        let src = Sources::new("t", &selected, &system);
        let tiers = src.colors::<{ keys::BULLET_TIERS }>(&keys::LIST_MARKER_COLOR);
        assert_eq!(tiers[0], parse_color("#111111"));
        assert_eq!(tiers[1], parse_color("#222222"));
        assert_eq!(tiers[2], parse_color("#222222"), "depth 3 takes depth 2");
    }

    /// A colour the parser refuses is skipped, and the walk continues — the theme
    /// renders with that one value inherited rather than failing to load.
    #[test]
    fn an_unparseable_value_is_skipped_and_the_chain_continues() {
        let selected = spec(
            "t",
            "heading_color = \"#112233\"\nheading_color_h1 = \"not a colour\"\n",
        );
        let system = ThemeSpec::default();
        let src = Sources::new("t", &selected, &system);
        assert_eq!(
            src.colors::<{ keys::HEADING_LEVELS }>(&keys::HEADING_COLOR)[0],
            parse_color("#112233")
        );
    }

    #[test]
    fn overlay_replaces_only_the_keys_the_other_states() {
        let mut base = spec(
            "t",
            "heading_color = \"#111111\"\nlink_color = \"#222222\"\n",
        );
        base.overlay(spec("t", "link_color = \"#333333\"\n"));
        let system = ThemeSpec::default();
        let src = Sources::new("t", &base, &system);
        assert_eq!(src.color(&keys::HEADING_COLOR), parse_color("#111111"));
        assert_eq!(src.color(&keys::LINK_COLOR), parse_color("#333333"));
    }

    /// A number out of `i32`'s range is a value to clamp, like every other hostile
    /// metric — never a panic and never a wrap.
    #[test]
    fn an_out_of_range_number_saturates_before_it_is_clamped() {
        let selected = spec("t", "list_step = 99999999999999\n");
        let system = ThemeSpec::default();
        let src = Sources::new("t", &selected, &system);
        assert_eq!(
            src.int(&keys::LIST_STEP),
            keys::LIST_STEP.bound.int_range().max
        );
    }

    /// TDD 18.32, across BOTH levelled families rather than one key of one of them.
    ///
    /// Every levelled key must narrow at its own slot, and every levelled key must
    /// fall back the way its `Levelling` says it does — asserted by walking the
    /// registry rather than by naming keys, so a key added later is covered the moment
    /// it is declared. A per-key test would have proved this of `heading_color` and
    /// said nothing about the sixteen keys beside it.
    ///
    /// **Parameterised on the levelling, not cloned per levelling.** `Heading` had this
    /// sweep and `Depth` had one hand-picked key — and `Depth`'s rule is the MORE
    /// unusual of the two (each tier falls back to the next SHALLOWER tier, not to the
    /// bare key), so `list_bullet_sprite`'s shallower chain was exercised nowhere. One
    /// test over `Levelling` also means a third levelling added later is covered by
    /// construction.
    ///
    /// Compared as authored values rather than resolved ones because the kinds differ
    /// and the walk does not: what is under test is the fallback chain, which is one
    /// piece of code for all of them.
    #[test]
    fn every_levelled_key_narrows_at_its_slot_and_falls_back_as_its_levelling_says() {
        use keys::Levelling;
        let mut levellings_seen = 0usize;
        for levelling in [Levelling::Heading, Levelling::Depth] {
            let family = || keys::KEYS.iter().filter(|k| k.levelling == levelling);
            assert!(
                family().count() > 1,
                "{levelling:?} is not a family — this sweep would be near-vacuous"
            );
            levellings_seen += 1;
            for key in family() {
                let (bare, narrowed) = sample(key.kind);
                let s = spec(
                    "t",
                    &format!("{} = {bare}\n{} = {narrowed}\n", key.name, key.spelling(1)),
                );
                let system = ThemeSpec::default();
                let src = Sources::new("t", &s, &system);
                let narrowed_value = src.raw_at(key, 1).unwrap_or_else(|| {
                    panic!("{}: the narrowed spelling resolved to nothing", key.name)
                });
                for slot in 0..key.slots() {
                    let got = src
                        .raw_at(key, slot)
                        .unwrap_or_else(|| panic!("{}: slot {slot} resolved to nothing", key.name));
                    // The one line where the two levellings differ, and it is the
                    // difference the vocabulary intends: a heading level that states
                    // nothing takes the BARE key, a nesting tier takes the next
                    // SHALLOWER one — so slot 2 is narrowed for `Depth` and bare for
                    // `Heading`.
                    let takes_the_narrowed_value = match levelling {
                        Levelling::Flat => unreachable!("a flat key has one slot"),
                        Levelling::Heading => slot == 1,
                        Levelling::Depth => slot >= 1,
                    };
                    if takes_the_narrowed_value {
                        assert_eq!(
                            got, narrowed_value,
                            "{} slot {slot} lost the value its levelling gives it",
                            key.name
                        );
                    } else {
                        assert_ne!(
                            got, narrowed_value,
                            "{} slot {slot} took a narrowing that is not its own",
                            key.name
                        );
                    }
                }
            }
        }
        assert_eq!(
            levellings_seen + 1,
            3,
            "a levelling was added to the registry and not to this sweep"
        );
    }

    /// **The array-shape check is a hard error, and this proves it fires.**
    ///
    /// `N` comes from the destination field's length and nothing in the type system
    /// ties it to `key.levelling`, so this call compiles. Under the `debug_assert` it
    /// replaced, the SHIPPED binary would have resolved three of five heading levels
    /// and silently dropped h4/h5 — the failure arriving in release only, at exactly
    /// the moment a new levelled key is wired.
    #[test]
    #[should_panic(expected = "the registry and the model field disagree")]
    fn reading_a_levelled_key_into_the_wrong_array_shape_is_a_hard_error() {
        let selected = spec("t", "heading_color = \"#112233\"\n");
        let system = ThemeSpec::default();
        let src = Sources::new("t", &selected, &system);
        let _ = src.colors::<{ keys::BULLET_TIERS }>(&keys::HEADING_COLOR);
    }
}
