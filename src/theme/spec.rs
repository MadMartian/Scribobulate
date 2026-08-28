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
use crate::sprite::SpriteRef;
use std::collections::BTreeMap;

/// Stands in for a sprite path that is not valid UTF-8, so a diagnostic about one
/// names the problem rather than printing an empty string that reads as a real name.
const NON_UTF8: &str = "<non-UTF-8 path>";

/// One authored value, already coerced to the type its key declares.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    /// Every string-ish kind — the parse into a colour, a glyph or a line style
    /// happens at resolution, so a value a *later* link could still supply is not
    /// discarded here.
    Text(String),
    Int(i64),
    Float(f64),
    Sprite(SpriteRef),
}

impl Value {
    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The value as an `i32`, saturating rather than wrapping. A theme is data from
    /// disk, so `heading_weight = 99999999999` is a value to clamp, not to panic on.
    pub(super) fn int(&self) -> Option<i32> {
        match self {
            Value::Int(n) => Some((*n).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
            _ => None,
        }
    }

    pub(super) fn float(&self) -> Option<f64> {
        match self {
            Value::Float(x) => Some(*x),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    /// The value as a theme file spelled it, for a diagnostic that has to quote it
    /// back. `Debug` would print the Rust variant (`Text("wavy")`), which names this
    /// module's model rather than the line the author has to go and fix.
    pub(super) fn authored(&self) -> String {
        match self {
            Value::Text(s) => format!("{s:?}"),
            Value::Int(n) => n.to_string(),
            Value::Float(x) => x.to_string(),
            Value::Sprite(r) => format!("{:?}", r.name().unwrap_or(NON_UTF8)),
        }
    }

    pub(super) fn sprite(&self) -> Option<&SpriteRef> {
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
pub(super) struct RawSpec(BTreeMap<String, toml::Value>);

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
    pub(super) fn validate(id: &str, raw: RawSpec) -> ThemeSpec {
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
                    log::warn!(
                        "theme: sprite {:?} for {name:?} is unavailable",
                        r.name().unwrap_or(NON_UTF8)
                    );
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

    pub(super) fn get(&self, spelling: &str) -> Option<&Value> {
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

/// What a key of this kind takes, for the warning a refused value earns — both the
/// wrong-TOML-type refusal here and the value-level one `Sources::walk` reports, so
/// the two read in one voice.
pub(super) fn expected(kind: Kind) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
