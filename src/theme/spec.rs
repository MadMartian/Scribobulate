//! The file model: one theme exactly as authored, and the resolution that reads two
//! of them at once.
//!
//! A spec is a **map from spelling to value**, not a struct with a field per key.
//! That is the whole reason the vocabulary can carry ~150 spellings without ~150 of
//! anything: the merge is a map merge, sprite resolution is a walk over the entries
//! the registry calls sprites, and per-level resolution is a walk over one fallback
//! chain. The alternative — the shape this replaced — needed a field, a merge-list
//! entry and a resolution branch per key, all three hand-maintained, and dropping any
//! one of them compiled cleanly and failed silently.
//!
//! What the map costs is the compile-time check that a key exists. That is bought
//! back at the boundary instead: every value goes in through [`ThemeSpec::validate`],
//! which admits only spellings [`super::keys`] declares, and every value comes out
//! through a [`Key`] constant rather than a string literal, so a typo at a use site
//! does not compile either.

use super::keys::{self, Key, Kind};
use super::{clamp_f64, clamp_i32, parse_color, sanitize_font_family};
use super::{CssSafeFontStack, LineStyle, MarkerGlyph};
use crate::sprite::SpriteRef;
use gtk::gdk;
use std::collections::BTreeMap;

/// One authored value, already coerced to the type its key declares.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    /// Every string-ish kind — the parse into a colour, a glyph or a line style
    /// happens at resolution, so a value a *later* link could still supply is not
    /// discarded here.
    Text(String),
    Int(i64),
    Float(f64),
    Sprite(SpriteRef),
}

impl Value {
    fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The value as an `i32`, saturating rather than wrapping. A theme is data from
    /// disk, so `heading_weight = 99999999999` is a value to clamp, not to panic on.
    fn int(&self) -> Option<i32> {
        match self {
            Value::Int(n) => Some((*n).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
            _ => None,
        }
    }

    fn float(&self) -> Option<f64> {
        match self {
            Value::Float(x) => Some(*x),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    fn sprite(&self) -> Option<&SpriteRef> {
        match self {
            Value::Sprite(r) => Some(r),
            _ => None,
        }
    }
}

/// A theme's `[themes.<id>]` block exactly as TOML gave it, before the registry has
/// seen it. Only [`ThemeSpec::validate`] constructs a [`ThemeSpec`] from one, so an
/// unvalidated spelling cannot exist further in.
#[derive(serde::Deserialize, Default, Debug)]
#[serde(transparent)]
pub(crate) struct RawSpec(BTreeMap<String, toml::Value>);

/// One theme exactly as authored: every key it stated, and nothing it did not.
///
/// Every entry's spelling is one the registry declares and every value carries the
/// type that key declares — both established once, by `validate`.
#[derive(Default, Clone, Debug, PartialEq)]
pub(crate) struct ThemeSpec {
    vals: BTreeMap<String, Value>,
}

impl ThemeSpec {
    /// Admit the keys this build knows, and report the rest (TDD 18.33).
    ///
    /// Three things are dropped, and the difference between them is deliberate. An
    /// **unknown spelling** is reported: a themes file is hand-written, so a typo is
    /// the ordinary failure, and a key that silently did nothing is indistinguishable
    /// from one that applied and had no effect. A **wrong type** is reported for the
    /// same reason. An **empty string** is not reported and not an error — it is the
    /// spelling of "unset", carried over from the arrays this vocabulary replaced,
    /// where an empty slot was the only way to say "inherit this one".
    ///
    /// Nothing here rejects the theme, or the file. One bad key costs that key.
    fn validate(id: &str, raw: RawSpec) -> ThemeSpec {
        let mut vals = BTreeMap::new();
        for (name, raw_value) in raw.0 {
            let Some(key) = keys::lookup(&name) else {
                log::warn!("theme {id:?}: unknown key {name:?} — ignored");
                continue;
            };
            match coerce(key.kind, &raw_value) {
                Coerced::Ok(v) => {
                    vals.insert(name, v);
                }
                Coerced::Unset => {}
                Coerced::WrongType => {
                    log::warn!(
                        "theme {id:?}: key {name:?} expects {} — ignored",
                        expected(key.kind)
                    );
                }
            }
        }
        ThemeSpec { vals }
    }

    /// Parse one themes file into its themes, resolving every sprite reference
    /// against `origin` on the way through.
    pub(crate) fn parse_file(
        text: &str,
        origin: crate::sprite::SpriteOrigin<'_>,
    ) -> Option<BTreeMap<String, ThemeSpec>> {
        #[derive(serde::Deserialize, Default, Debug)]
        struct ThemesFile {
            #[serde(default)]
            themes: BTreeMap<String, RawSpec>,
        }
        match toml::from_str::<ThemesFile>(text) {
            Ok(f) => Some(
                f.themes
                    .into_iter()
                    .map(|(id, raw)| {
                        let mut spec = ThemeSpec::validate(&id, raw);
                        spec.resolve_sprites(origin);
                        (id, spec)
                    })
                    .collect(),
            ),
            Err(e) => {
                log::warn!("theme: themes.toml parse error: {e} — ignoring this file");
                None
            }
        }
    }

    /// Answer every sprite reference against the file's own origin, dropping any this
    /// origin cannot supply.
    ///
    /// **Which entries are sprites is the registry's answer, not a second list.** The
    /// list this replaced had to be extended by hand for each new sprite key, and a
    /// key missing from it was a reference that reached a consumer unvalidated.
    fn resolve_sprites(&mut self, origin: crate::sprite::SpriteOrigin<'_>) {
        self.vals.retain(|name, value| {
            let Value::Sprite(r) = value else { return true };
            match origin.resolve(r) {
                Some(resolved) => {
                    *value = Value::Sprite(resolved);
                    true
                }
                None => {
                    log::warn!("theme: sprite {:?} for {name:?} is unavailable", r.name());
                    false
                }
            }
        });
    }

    /// Overlay `other`'s keys onto self, leaving self's value wherever `other` is
    /// silent — the per-key half of both the user-file merge and the selected→system
    /// resolution.
    ///
    /// One line, and that is the point: the hand-written merge list this replaced had
    /// one entry per key, and a key omitted from it compiled, passed, and silently
    /// dropped every user override of that key.
    pub(crate) fn overlay(&mut self, other: ThemeSpec) {
        self.vals.extend(other.vals);
    }

    fn get(&self, spelling: &str) -> Option<&Value> {
        self.vals.get(spelling)
    }

    /// This spec's OWN bare value for `key`, with no fallback to any other source.
    ///
    /// The one place resolution is not a two-source walk: a theme's display name and
    /// picker symbol belong to that theme, so inheriting `[themes.system]`'s name
    /// would label every unnamed theme "System".
    pub(crate) fn own_text(&self, key: &Key) -> Option<&str> {
        self.get(key.name).and_then(Value::text)
    }

    /// The spellings this theme states, for tests that assert on what a file carried.
    #[cfg(test)]
    pub(crate) fn spellings(&self) -> Vec<&str> {
        self.vals.keys().map(String::as_str).collect()
    }
}

enum Coerced {
    Ok(Value),
    /// An empty string: the authored spelling of "leave this one to the next link".
    Unset,
    WrongType,
}

/// Coerce one authored TOML scalar to the type its key declares.
fn coerce(kind: Kind, raw: &toml::Value) -> Coerced {
    match kind {
        Kind::Int => match raw.as_integer() {
            Some(n) => Coerced::Ok(Value::Int(n)),
            None => Coerced::WrongType,
        },
        Kind::Float => match raw
            .as_float()
            .or_else(|| raw.as_integer().map(|n| n as f64))
        {
            Some(x) => Coerced::Ok(Value::Float(x)),
            None => Coerced::WrongType,
        },
        Kind::Sprite => match raw.as_str() {
            Some(s) if s.trim().is_empty() => Coerced::Unset,
            Some(s) => Coerced::Ok(Value::Sprite(SpriteRef::Named(s.to_string()))),
            None => Coerced::WrongType,
        },
        Kind::Text | Kind::Font | Kind::Color | Kind::Glyph | Kind::Line => match raw.as_str() {
            Some(s) if s.trim().is_empty() => Coerced::Unset,
            Some(s) => Coerced::Ok(Value::Text(s.to_string())),
            None => Coerced::WrongType,
        },
    }
}

/// What a key of this kind takes, for the warning a wrong-typed value earns.
fn expected(kind: Kind) -> &'static str {
    match kind {
        Kind::Int => "a whole number",
        Kind::Float => "a number",
        Kind::Sprite => "a string naming an image",
        Kind::Color => "a colour string",
        Kind::Font => "a font-stack string",
        Kind::Glyph => "a string",
        Kind::Line => "a line-style string",
        Kind::Text => "a string",
    }
}

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
    pub(crate) selected: &'a ThemeSpec,
    pub(crate) system: &'a ThemeSpec,
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
        self.walk(key.fallbacks(idx), f)
    }

    /// The key's **bare** spelling only, in each source in turn.
    ///
    /// This is what a surface that is not a heading level reads: the table header
    /// takes `heading_color`/`heading_font` when it states no ink of its own (TDD
    /// 18.30), and taking h1's narrowed colour there instead would make a theme that
    /// distinguishes its h1 silently re-ink a table header it said nothing about.
    fn bare<T>(&self, key: &Key, f: impl Fn(&Value) -> Option<T>) -> Option<T> {
        self.walk(vec![key.name.to_string()], f)
    }

    fn walk<T>(&self, spellings: Vec<String>, f: impl Fn(&Value) -> Option<T>) -> Option<T> {
        for spec in [self.selected, self.system] {
            for spelling in &spellings {
                if let Some(found) = spec.get(spelling).and_then(&f) {
                    return Some(found);
                }
            }
        }
        None
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
        debug_assert_eq!(N, key.slots(), "{} has {} slots", key.name, key.slots());
        std::array::from_fn(|i| self.pick(key, i, &f))
    }

    // ── the typed accessors ──────────────────────────────────────────────────
    //
    // Each comes in two forms, and the difference is which question it answers. The
    // singular form reads the key's BARE value — what a theme said about the whole
    // construct. The array form reads one value per level or depth, each already
    // folded down its own fallback chain, so every consumer indexes and none of them
    // re-derives the fold.

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

    pub(crate) fn lines<const N: usize>(&self, key: &Key, floor: LineStyle) -> [LineStyle; N] {
        let found: [Option<LineStyle>; N] = self.each(key, |v| v.text().and_then(LineStyle::parse));
        found.map(|s| s.unwrap_or(floor))
    }

    pub(crate) fn line(&self, key: &Key, floor: LineStyle) -> LineStyle {
        self.bare(key, |v| v.text().and_then(LineStyle::parse))
            .unwrap_or(floor)
    }

    pub(crate) fn ints<const N: usize>(
        &self,
        key: &Key,
        floor: [i32; N],
        range: (i32, i32),
    ) -> [i32; N] {
        let found: [Option<i32>; N] = self.each(key, |v| v.int());
        std::array::from_fn(|i| found[i].map(|n| clamp_i32(n, range)).unwrap_or(floor[i]))
    }

    pub(crate) fn int(&self, key: &Key, floor: i32, range: (i32, i32)) -> i32 {
        self.bare(key, |v| v.int())
            .map(|n| clamp_i32(n, range))
            .unwrap_or(floor)
    }

    pub(crate) fn floats<const N: usize>(
        &self,
        key: &Key,
        floor: [f64; N],
        range: (f64, f64),
    ) -> [f64; N] {
        let found: [Option<f64>; N] = self.each(key, |v| v.float());
        std::array::from_fn(|i| found[i].map(|x| clamp_f64(x, range)).unwrap_or(floor[i]))
    }

    pub(crate) fn float(&self, key: &Key, floor: f64, range: (f64, f64)) -> f64 {
        self.bare(key, |v| v.float())
            .map(|x| clamp_f64(x, range))
            .unwrap_or(floor)
    }
}

#[cfg(test)]
mod tests {
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
    fn an_unknown_key_is_dropped_and_every_recognised_key_survives() {
        let s = spec(
            "t",
            "heading_color = \"#ff0000\"\nheading_colour = \"#00ff00\"\nlink_color = \"#0000ff\"\n",
        );
        assert_eq!(s.spellings(), vec!["heading_color", "link_color"]);
    }

    #[test]
    fn a_wrong_typed_value_costs_its_own_key_and_nothing_else() {
        let s = spec("t", "list_step = \"wide\"\nlist_item_gap = 12\n");
        assert_eq!(s.spellings(), vec!["list_item_gap"]);
    }

    /// An empty string is how the arrays this vocabulary replaced spelled "inherit
    /// this one", and it still means that — silently, because it is not a mistake.
    #[test]
    fn an_empty_string_is_unset_rather_than_a_value() {
        let s = spec(
            "t",
            "heading_color_h2 = \"\"\nheading_color_h3 = \"#123456\"\n",
        );
        assert_eq!(s.spellings(), vec!["heading_color_h3"]);
    }

    #[test]
    fn a_heading_level_overrides_the_bare_key_and_an_unstated_one_takes_it() {
        let selected = spec(
            "t",
            "heading_color = \"#112233\"\nheading_color_h2 = \"#445566\"\n",
        );
        let system = ThemeSpec::default();
        let src = Sources {
            selected: &selected,
            system: &system,
        };
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
        let src = Sources {
            selected: &selected,
            system: &system,
        };
        assert_eq!(
            src.colors::<{ keys::HEADING_LEVELS }>(&keys::HEADING_COLOR)[0],
            parse_color("#112233")
        );

        let silent = ThemeSpec::default();
        let src = Sources {
            selected: &silent,
            system: &system,
        };
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
        let src = Sources {
            selected: &selected,
            system: &system,
        };
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
        let src = Sources {
            selected: &selected,
            system: &system,
        };
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
        let src = Sources {
            selected: &base,
            system: &system,
        };
        assert_eq!(src.color(&keys::HEADING_COLOR), parse_color("#111111"));
        assert_eq!(src.color(&keys::LINK_COLOR), parse_color("#333333"));
    }

    /// A number out of `i32`'s range is a value to clamp, like every other hostile
    /// metric — never a panic and never a wrap.
    #[test]
    fn an_out_of_range_number_saturates_before_it_is_clamped() {
        let selected = spec("t", "list_step = 99999999999999\n");
        let system = ThemeSpec::default();
        let src = Sources {
            selected: &selected,
            system: &system,
        };
        assert_eq!(src.int(&keys::LIST_STEP, 28, (4, 400)), 400);
    }

    /// TDD 18.32, across the WHOLE family rather than one key of it.
    ///
    /// Every heading key must narrow, and every heading key must fall back — asserted
    /// by walking the registry rather than by naming keys, so a heading key added
    /// later is covered the moment it is declared. A per-key test would have proved
    /// this of `heading_color` and said nothing about the twelve keys beside it.
    ///
    /// Compared as authored values rather than resolved ones because the kinds differ
    /// and the walk does not: what is under test is the fallback chain, which is one
    /// piece of code for all of them.
    #[test]
    fn every_heading_key_narrows_to_a_level_and_falls_back_to_its_bare_form() {
        let heading_keys = || {
            keys::KEYS
                .iter()
                .filter(|k| k.levelling == keys::Levelling::Heading)
        };
        assert!(heading_keys().count() > 1, "the family is not a family");
        for key in heading_keys() {
            let (bare, narrowed) = sample(key.kind);
            let s = spec(
                "t",
                &format!("{} = {bare}\n{} = {narrowed}\n", key.name, key.spelling(1)),
            );
            let system = ThemeSpec::default();
            let src = Sources {
                selected: &s,
                system: &system,
            };
            let narrowed_value = src.raw_at(key, 1).unwrap_or_else(|| {
                panic!("{}: the narrowed spelling resolved to nothing", key.name)
            });
            for level in 0..keys::HEADING_LEVELS {
                let got = src
                    .raw_at(key, level)
                    .unwrap_or_else(|| panic!("{}: level {level} resolved to nothing", key.name));
                if level == 1 {
                    assert_eq!(got, narrowed_value, "{}: h2 lost its own value", key.name);
                } else {
                    assert_ne!(
                        got,
                        narrowed_value,
                        "{}: h{} took h2's narrowed value",
                        key.name,
                        level + 1
                    );
                }
            }
        }
    }
}
