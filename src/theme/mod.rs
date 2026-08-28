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

mod decor;
pub(crate) mod keys;
mod model;
mod resolve;
mod sources;
mod spec;
mod value;

#[cfg(test)]
mod tests;

pub(crate) use decor::{
    marker_choice, marker_glyph, marker_sprite, Band, BandPaint, Fill, MarkerChoice, MarkerKind,
    MarkerSubstitute,
};
pub(crate) use keys::{heading_slot, BULLET_TIERS, HEADING_LEVELS};
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

/// One row of the theme picker: what to select, what to show, and the glyph to show
/// beside it.
///
/// A NAMED struct rather than the `(String, String, Option<String>)` this used to be.
/// The triple carried two indistinguishable `String`s, so every one of its eleven call
/// and test sites re-derived which was which from position — and a transposition
/// between the id and the label compiles, then selects a theme named after its own
/// display text. Field names make that unrepresentable (POLICY § Code style:
/// destructure by name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChooserEntry {
    /// The theme id — what `set_active` takes, never what a human reads.
    pub(crate) id: String,
    /// The theme's display name, with no symbol prefix. This is also the sort key, so
    /// a theme's glyph cannot decide where it appears in the list.
    pub(crate) label: String,
    /// The theme's own picker glyph, `None` where it states none. A theme's symbol is
    /// its OWN — it is never inherited from `[themes.system]`, or every unnamed theme
    /// wears the base theme's window glyph.
    pub(crate) symbol: Option<String>,
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
    pub(crate) fn chooser_list(&self) -> Vec<ChooserEntry> {
        let entry = |id: &str, s: &ThemeSpec| ChooserEntry {
            id: id.to_string(),
            label: s
                .own_text(&keys::NAME)
                .map(str::to_string)
                .unwrap_or_else(|| id.to_string()),
            symbol: s.own_text(&keys::SYMBOL).map(str::to_string),
        };
        let mut rest: Vec<ChooserEntry> = self
            .specs
            .iter()
            .filter(|(id, _)| id.as_str() != SYSTEM_ID)
            .map(|(id, s)| entry(id, s))
            .collect();
        // By NAME, not by the symbol-prefixed label — a theme's glyph must not decide
        // where it sorts.
        rest.sort_by(|a, b| a.label.cmp(&b.label));
        let mut out = Vec::with_capacity(rest.len() + 1);
        if let Some(sys) = self.specs.get(SYSTEM_ID) {
            out.push(entry(SYSTEM_ID, sys));
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
    /// **No carve-out for [`SYSTEM_ID`], deliberately.** This used to blank `selected`
    /// for that id, which bought nothing — `Sources::walk` reads `[selected, system]`
    /// and returns the first hit, so the walk is idempotent when the two are the same
    /// spec. Its only *effect* was to destroy the system theme's own display name,
    /// which then required a compensating branch in a **different file**
    /// (`Theme::resolve`'s `name`) to put back. Deleting either one alone made the
    /// system theme silently become `"system"` in every picker, nothing linked them,
    /// and no test covered the coupling — the same shape as ScrAP-324, where one
    /// origin got a step and the other did not. Both are gone; the module doc's claim
    /// that `SYSTEM_ID` is "not privileged in any other way" is true again.
    pub(crate) fn resolve(&self, id: &str) -> Theme {
        let system = self.specs.get(SYSTEM_ID).cloned().unwrap_or_default();
        let selected = self.specs.get(id).cloned().unwrap_or_default();
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

/// The directories the search path is derived from, snapshotted once.
///
/// **A data seam, not a trait.** `find_themes_file` used to read
/// `config::user_config_dir()`, `glib::user_data_dir()` and `glib::system_data_dirs()`
/// inline and take no parameters, so every rule SCHEMA states about the search path —
/// row ordering, `$XDG_DATA_DIRS` being *iterated* rather than hard-coded to
/// `/usr/share`, first-match-wins, the returned directory being the found file's own
/// parent (which is the sprite origin) — was untestable. The only way to reach it was
/// to mutate `std::env`, which is process-global and breaks `cargo test`'s
/// parallelism: a hazard, not a test. SCHEMA records that this is not hypothetical —
/// the ordering once made user theme overrides unreachable on Windows outright.
///
/// A trait object for one call site would be over-abstraction; the split that buys the
/// tests is between *"where would I look?"* (pure, over this struct) and *"read the
/// first one that exists"* (thin I/O).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SearchBases {
    /// The user's configuration directory — row 1, the override.
    pub config: Option<std::path::PathBuf>,
    /// The per-user data directory — row 2.
    pub data_home: std::path::PathBuf,
    /// The system data directories, **in order** — row 3, one candidate each.
    pub system_dirs: Vec<std::path::PathBuf>,
}

impl SearchBases {
    /// The bases this host actually offers.
    ///
    /// ⚠️ **`XDG_CONFIG_HOME` is snapshotted, never read through GLib** — see
    /// [`load`]'s note. `XDG_DATA_HOME`/`XDG_DATA_DIRS` are untouched by that
    /// redirect, so the GLib helpers are correct and order-independent for those.
    fn from_env() -> SearchBases {
        SearchBases {
            config: crate::config::user_config_dir(),
            data_home: glib::user_data_dir(),
            system_dirs: glib::system_data_dirs(),
        }
    }

    /// Every candidate path, **in search order**. Pure.
    ///
    /// The order is SCHEMA's three-row table: the user override first, then the
    /// per-user install, then each system install directory in the order the platform
    /// gave them. `system_dirs` is *iterated* and never hard-coded to `/usr/share` —
    /// on a KDE box its first entry is `/usr/share/plasma`, so a hard-coded path works
    /// on GNOME and fails there.
    pub(crate) fn candidates(&self) -> Vec<std::path::PathBuf> {
        let leaf = |dir: &std::path::Path| dir.join("scribobulate").join("themes.toml");
        let mut paths = Vec::with_capacity(2 + self.system_dirs.len());
        if let Some(dir) = &self.config {
            paths.push(leaf(dir));
        }
        paths.push(leaf(&self.data_home));
        paths.extend(self.system_dirs.iter().map(|d| leaf(d)));
        paths
    }
}

/// Returns the file's text and the directory it was read from — sprite references
/// resolve against that directory, never against the current working directory.
fn find_themes_file() -> Option<(String, std::path::PathBuf)> {
    read_first_existing(&SearchBases::from_env().candidates())
}

/// The thin I/O half: read the **first** candidate that exists, and report the
/// directory it came from.
///
/// First-match-wins is the contract, not an implementation detail — a later file does
/// NOT merge over an earlier one, so a system install cannot add keys to a user's own
/// themes file. The directory is the found file's own parent because that is the
/// sprite origin (`crate::sprite::SpriteOrigin::Directory`).
fn read_first_existing(paths: &[std::path::PathBuf]) -> Option<(String, std::path::PathBuf)> {
    for p in paths {
        if let Ok(text) = std::fs::read_to_string(p) {
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

/// Make an already-resolved [`Theme`] active **for the life of the returned guard**.
///
/// Test-only seam: it lets a test activate a theme built from an inline fragment (via
/// [`Themes::merge_over_for_test`]) without installing a file on the search path.
/// Production code always goes through [`set_active`], so the active theme can only
/// ever be one the registry actually holds.
///
/// Test-only, and NOT gated on the integration feature: a display-free test needs it
/// too. `pangospan`'s parity check asserts that the preview's wrappers — which resolve
/// against the app-wide active theme — emit the same span the export sink builds from
/// an explicit one, and there is no way to ask that question without setting the
/// active theme.
///
/// # Why a guard and not a setter
///
/// The active theme and the sprite caches are PROCESS-GLOBAL, and every `#[gtktest]`
/// body in the binary shares one thread — so a test that panicked mid-body used to
/// leave whichever theme it had installed active for whatever ran next, turning one
/// failure into a spurious failure, or a spurious PASS, somewhere unrelated. The
/// restore used to be a trailing `set_active(SYSTEM_ID)` on the happy path, which is
/// exactly the shape `sdd/POLICY.md` § Unit tests prescribes an RAII guard for: `Drop`
/// runs on the panic path too, and it restores the theme that was active BEFORE rather
/// than assuming it was System.
#[cfg(test)]
#[must_use = "the guard restores the previous active theme when it is dropped; \
              binding it to `_` drops it immediately and undoes the activation"]
pub(crate) fn activate_for_test(theme: Theme) -> ActiveThemeGuard {
    let previous = ACTIVE.with(|a| a.borrow_mut().take());
    // Sprite textures are cached by REFERENCE, not by theme, so a swap must clear them
    // on the way in as well as on the way out — otherwise a theme installed here can
    // be served a texture the previous one decoded.
    crate::sprite::clear_cache();
    ACTIVE.with(|a| *a.borrow_mut() = Some(std::rc::Rc::new(theme)));
    ActiveThemeGuard { previous }
}

/// Restores the process-global active theme and clears the sprite caches on `Drop` —
/// including the panic path, which is the whole point.
#[cfg(test)]
pub(crate) struct ActiveThemeGuard {
    /// What was active before, `None` when nothing had resolved a theme yet. Restoring
    /// `None` rather than System keeps the guard honest about the state it found: a
    /// test that runs first must not leave the process looking like one that had
    /// already resolved a theme.
    previous: Option<std::rc::Rc<Theme>>,
}

#[cfg(test)]
impl Drop for ActiveThemeGuard {
    fn drop(&mut self) {
        crate::sprite::clear_cache();
        ACTIVE.with(|a| *a.borrow_mut() = self.previous.take());
    }
}
