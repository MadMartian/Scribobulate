//! Preview reading themes: the data model, the search path, and the resolution
//! order (POLICY "No hard-coded styling" / "One theme key, every application path").
//!
//! This module is the **engine**. It carries NO per-theme knowledge: no colour
//! constants, no `if theme == "sepia"` branches, no styling literals. Themes are
//! pure data in `data/themes.toml`, which is both installed alongside the app and
//! compiled in (`include_str!`) as the last-resort fallback — so "what does this
//! app hardcode?" is answerable by reading one data file, and a missing or
//! malformed themes file can never prevent startup (TDD 18.11/18.14).
//!
//! Three rules shape everything here:
//!
//! * **One key → one resolved value → every consumer.** A key that reached the
//!   `li-{depth}` tag but not the drawn marker gutter would strand every list
//!   marker — GTK4Rs/AP-96's exact failure mode. Resolution happens once, in
//!   [`Theme::resolve`]; consumers read the resolved [`Theme`], never the file.
//! * **The theme owns Pango SCALE, never CSS `font-size`.** Scale is a tag
//!   attribute GTK *multiplies* onto the CSS base (`gtktextattributes.c:349-351`),
//!   so it composes with zoom for free. `font-size` is a CSS longhand the zoom
//!   provider owns exclusively; a second provider writing the same lookup slot is
//!   arbitrated by provider ADD ORDER, not selector specificity (GTK4Rs/AP-101), which
//!   our `.scrib-win-<id>` scoping cannot arbitrate. There is no `font_size` key
//!   by decision, so the collision is impossible rather than managed.
//! * **Types over sanitisers.** Geometry is parsed to `i32` and clamped, so it
//!   *cannot* carry a `}` or `;` into generated CSS — injection is impossible by
//!   construction. Only the one genuinely free-form string (`font_family`) needs
//!   a sanitiser, and colours are re-emitted from parsed `RGBA`, never echoed.
//!
//! Pure and display-free end to end: parsing, merging, clamping, and resolution
//! are all unit-tested without a GTK display; the GTK probe stays at the edge in
//! `palette::Palette::resolve`.
//!
//! **Where the parts live.** The split is by *stage*, so each piece can be read —
//! and tested — without the rest:
//!
//! | Module | Owns |
//! |---|---|
//! | [`keys`] | The registry: every spelling a themes file may use, with its type, its slot count and whether it names a sprite |
//! | [`spec`] | One theme exactly as authored — a validated map, and the warning a key this build does not know earns |
//! | [`sources`] | The resolution walk: two authored themes read as one, identically for every kind and every levelling |
//! | [`value`] | The authored value types — colour, font stack, line style, glyph — each parsed or refused, never half-trusted |
//! | [`model`] | The resolved [`Theme`] every consumer reads, with every fallback already folded in |
//! | [`resolve`] | The floors, the clamp ranges, and the one function that applies them |
//! | this module | The search path, the built-in/user merge, and the active-theme cell |

pub(crate) mod keys;
mod model;
mod resolve;
mod sources;
mod spec;
mod value;

#[cfg(test)]
mod tests;

pub(crate) use keys::{BULLET_TIERS, HEADING_LEVELS};
pub(crate) use model::{ListGlyphs, Metrics, Sprites, Theme};
pub(crate) use spec::ThemeSpec;
pub(crate) use value::{
    depth_tier, parse_color, px, sanitize_font_family, CssSafeFontStack, LineStyle, MarkerGlyph,
};

use gtk::glib;
use std::collections::BTreeMap;

/// The shipped theme data, compiled in. This is the SAME file `install.sh`
/// installs, so the built-in fallback and the installed default can never drift.
const BUILTIN_THEMES_TOML: &str = include_str!("../../data/themes.toml");

/// The id of the base theme — the register of everything the app would otherwise
/// hardcode, and link 2 of the resolution order. Not privileged in any other way:
/// every key is available to every theme, and this one merely *happens* to hold
/// today's values.
pub(crate) const SYSTEM_ID: &str = "system";

// ── the file model ──────────────────────────────────────────────────────────
//
// One theme exactly as authored lives in `spec::ThemeSpec`, and the vocabulary it
// may state lives in `keys`. Neither is a struct with a field per key: the key set
// is data, so the merge, the sprite walk and the per-level fallback are each one
// piece of code rather than one line per key.

/// Every installed theme, keyed by id.
#[derive(Debug, Clone)]
pub(crate) struct Themes {
    specs: BTreeMap<String, ThemeSpec>,
}

impl Themes {
    /// Parse a themes file, validating every key and resolving every sprite
    /// reference in it against `origin`. A malformed *file* yields `None` so the
    /// caller can fall back rather than surface a broken app (the same discipline
    /// `Config::parse` follows); a malformed *key* costs only itself.
    ///
    /// **Validation and sprite resolution happen HERE, not in the caller, and that
    /// is what makes them total.** This is the sole constructor of a `ThemeSpec`, so
    /// a spec that has escaped this function states only keys this build knows and
    /// has had every sprite reference answered — no `SpriteRef::Named` survives.
    /// `Theme::resolve` therefore stays pure (no filesystem) while still seeing real
    /// sources, and the built-in and user-file paths cannot diverge on when either
    /// step runs: they call the same function.
    ///
    /// The origin is a parameter rather than a property of the text because the two
    /// callers genuinely differ: a file on disk resolves against its own directory, a
    /// compiled-in file against the compiled-in table (`crate::sprite`).
    fn parse(
        text: &str,
        origin: crate::sprite::SpriteOrigin<'_>,
    ) -> Option<BTreeMap<String, ThemeSpec>> {
        ThemeSpec::parse_file(text, origin)
    }

    /// The compiled-in themes. Total by construction: if the shipped data file
    /// ever failed to parse, every key would still resolve through the floor
    /// consts, so the app renders rather than dies. `builtin_parses` catches that
    /// at test time, where it belongs.
    ///
    /// Its sprites resolve against the **compiled-in table**, not against any
    /// directory. A built-in theme is compiled in so that it renders with nothing on
    /// disk; resolving its sprites against a themes-file directory would have made
    /// each of them contingent on an installed asset instead — present on a packaged
    /// host, absent on every fresh install, developer build and `.app` bundle, and
    /// silent either way, because an unresolved sprite is inert by design.
    pub(crate) fn builtin() -> Self {
        Themes {
            specs: Themes::parse(BUILTIN_THEMES_TOML, crate::sprite::SpriteOrigin::Compiled)
                .unwrap_or_default(),
        }
    }

    /// Merge a user file's themes over these, per theme id and per key: a user can
    /// override ONE key of a shipped theme without restating it, or add a whole new
    /// theme (TDD 18.13/18.14). Pure.
    fn merge_over(&mut self, user: BTreeMap<String, ThemeSpec>) {
        for (id, spec) in user {
            match self.specs.get_mut(&id) {
                Some(base) => base.overlay(spec),
                None => {
                    self.specs.insert(id, spec);
                }
            }
        }
    }

    /// Parse a fragment the way the compiled-in file is parsed. Test-only convenience
    /// so the many parse-and-resolve tests below state only the TOML they are about —
    /// and `Compiled` rather than a directory because a fragment written inline has no
    /// directory to resolve against, while the compiled-in sprite table is reachable
    /// from any test without a fixture on disk.
    #[cfg(test)]
    fn parse_compiled(text: &str) -> Option<BTreeMap<String, ThemeSpec>> {
        Themes::parse(text, crate::sprite::SpriteOrigin::Compiled)
    }

    /// Merge an inline themes-file fragment over these, as a user file would.
    /// Test-only seam so a test elsewhere in the crate can exercise a themed value
    /// end-to-end without writing a file or touching the search path.
    #[cfg(test)]
    pub(crate) fn merge_over_for_test(&mut self, toml_text: &str) {
        let user = Themes::parse(toml_text, crate::sprite::SpriteOrigin::Compiled)
            .expect("test theme fragment must parse");
        self.merge_over(user);
    }

    /// Theme ids paired with display names, in chooser order: the system theme
    /// first (it is the default), then every other theme by display name.
    /// Deterministic — TOML tables carry no authoring order we could honour.
    pub(crate) fn chooser_list(&self) -> Vec<(String, String, Option<String>)> {
        let entry = |id: &String, s: &ThemeSpec| {
            (
                id.clone(),
                s.own_text(&keys::NAME)
                    .map(str::to_string)
                    .unwrap_or_else(|| id.clone()),
                s.own_text(&keys::SYMBOL).map(str::to_string),
            )
        };
        let mut rest: Vec<(String, String, Option<String>)> = self
            .specs
            .iter()
            .filter(|(id, _)| id.as_str() != SYSTEM_ID)
            .map(|(id, s)| entry(id, s))
            .collect();
        rest.sort_by(|a, b| a.1.cmp(&b.1)); // by NAME (not the symbol-prefixed label)
        let mut out = Vec::with_capacity(rest.len() + 1);
        if let Some(sys) = self.specs.get(SYSTEM_ID) {
            out.push(entry(&SYSTEM_ID.to_string(), sys));
        }
        out.extend(rest);
        out
    }

    /// The picker label for a theme: `"<symbol>  <name>"` when it has a symbol, else
    /// just the name. Shared by both surfaces so they read identically.
    pub(crate) fn chooser_label(name: &str, symbol: Option<&str>) -> String {
        match symbol {
            Some(s) => format!("{s}\u{2002}{name}"),
            None => name.to_string(),
        }
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.specs.contains_key(id)
    }

    /// Resolve `id` against the resolution order's links 1 and 2. An unknown id
    /// resolves as the system theme, so a stale persisted selection (a theme the
    /// user deleted) degrades to the default instead of failing.
    pub(crate) fn resolve(&self, id: &str) -> Theme {
        let system = self.specs.get(SYSTEM_ID).cloned().unwrap_or_default();
        let selected = self
            .specs
            .get(id)
            .filter(|_| id != SYSTEM_ID)
            .cloned()
            .unwrap_or_default();
        Theme::resolve(id, &selected, &system)
    }
}

// ── loading (the only impure part) ────────────────────────────────────────────

/// Load every installed theme: the compiled-in data, with the first themes file
/// found on the search path merged over it.
///
/// ⚠️ **`XDG_CONFIG_HOME` is snapshotted, never read through GLib.** This process
/// redirects `XDG_CONFIG_HOME` to a temp dir at startup to prevent the GTK 4.6
/// compose-table crash (`workaround.rs`), and `g_get_user_config_dir()` caches its
/// answer in a global static FOREVER on first call — a global GTK 4.6's compose
/// table reads from too. So calling `glib::user_config_dir()` here would either
/// resolve into the temp dir (silently losing the user's override) or, if forced
/// to resolve early, re-arm the crash. There is no ordering that gives both.
/// `config::config()` hand-rolls the same snapshot for the same reason — that is
/// FORCED BY THE WORKAROUND, not needless XDG re-implementation. Do not "clean it
/// up" into `glib::user_config_dir()`.
///
/// `XDG_DATA_HOME`/`XDG_DATA_DIRS` are untouched by the redirect, so the GLib
/// helpers are correct and order-independent for those.
fn load() -> Themes {
    let mut themes = Themes::builtin();
    if let Some((text, dir)) = find_themes_file() {
        // A file on disk resolves its sprites against its OWN directory — never the
        // current working directory, and never the compiled-in table, which would let
        // any theme file on the search path claim this project's own artwork.
        if let Some(user) = Themes::parse(&text, crate::sprite::SpriteOrigin::Directory(&dir)) {
            themes.merge_over(user);
        }
    }
    themes
}

/// Returns the file's text and the directory it was read from — sprite references
/// resolve against that directory, never against the current working directory.
fn find_themes_file() -> Option<(String, std::path::PathBuf)> {
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    // 1. user override — from the config path snapshotted before the redirect.
    if let Some(dir) = crate::config::user_config_dir() {
        paths.push(dir.join("scribobulate").join("themes.toml"));
    }
    // 2. per-user install, then 3. system install. Iterate `system_data_dirs()`;
    // never hard-code `/usr/share` — on a KDE box its first entry is
    // `/usr/share/plasma`, and a hard-coded path would work on GNOME and fail here.
    paths.push(
        glib::user_data_dir()
            .join("scribobulate")
            .join("themes.toml"),
    );
    for dir in glib::system_data_dirs() {
        paths.push(dir.join("scribobulate").join("themes.toml"));
    }
    for p in paths {
        if let Ok(text) = std::fs::read_to_string(&p) {
            log::debug!("theme: loaded themes from {}", p.display());
            let dir = p
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            return Some((text, dir));
        }
    }
    None
}

// ── the active theme ──────────────────────────────────────────────────────────
//
// App-wide, not per-window (sdd/THEMING.md): the theme's CSS properties
// (`font-family`, `color`, `background-color`) are DISJOINT from zoom's
// (`font-size`), so the theme can be one app-wide provider with unscoped rules and
// zoom's per-window provider is untouched. A per-window theme would force both
// into per-window generated CSS scoped by `.scrib-win-<id>` for no benefit.
//
// GTK4 is single-threaded, so a thread-local holds this without a lock; unit tests
// each get their own, which keeps them isolated.

thread_local! {
    static THEMES: std::cell::OnceCell<Themes> = const { std::cell::OnceCell::new() };
    static ACTIVE: std::cell::RefCell<Option<std::rc::Rc<Theme>>> =
        const { std::cell::RefCell::new(None) };
}

/// Every installed theme, loaded once.
pub(crate) fn themes() -> Themes {
    THEMES.with(|t| t.get_or_init(load).clone())
}

/// The active resolved theme. Defaults to the system theme, so every consumer can
/// read a theme unconditionally without an "is theming on yet?" branch.
pub(crate) fn active() -> std::rc::Rc<Theme> {
    ACTIVE.with(|a| {
        let mut slot = a.borrow_mut();
        if slot.is_none() {
            *slot = Some(std::rc::Rc::new(themes().resolve(SYSTEM_ID)));
        }
        slot.clone().expect("just initialised")
    })
}

/// Select `id` as the active theme, re-resolving it. Returns the new theme.
/// An unknown id resolves as the system theme (see [`Themes::resolve`]).
pub(crate) fn set_active(id: &str) -> std::rc::Rc<Theme> {
    let resolved = std::rc::Rc::new(themes().resolve(id));
    // Decoded sprites are cached by PATH, not by theme id, so a swap away from a
    // sprite-using theme would otherwise keep its textures resident for the rest of
    // the process — and a path a new theme no longer names could, in principle, be
    // served stale to a caller asking about the new one.
    crate::sprite::clear_cache();
    ACTIVE.with(|a| *a.borrow_mut() = Some(resolved.clone()));
    resolved
}

/// Make an already-resolved [`Theme`] active. Test-only seam: it lets a test
/// activate a theme built from an inline fragment (via
/// [`Themes::merge_over_for_test`]) without installing a file on the search path.
/// Production code always goes through [`set_active`], so the active theme can only
/// ever be one the registry actually holds.
///
/// Gated on the integration feature because only the realized-view tests need it —
/// the pure tests resolve a `Theme` and assert on it directly, without ever making
/// it the app-wide active one.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn set_active_for_test(theme: Theme) {
    ACTIVE.with(|a| *a.borrow_mut() = Some(std::rc::Rc::new(theme)));
}
