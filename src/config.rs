use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();
static USER_CONFIG_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Returns the application config, loading it on first call.
///
/// Must be called before XDG_CONFIG_HOME is redirected — i.e. as the first
/// expression in main(), before `workaround::workaround_gtk46_compose_crash()`.
/// The real reason is stronger than "config must be read first": **no read of the
/// user config dir may happen before the redirect, or the workaround breaks.** See
/// [`user_config_dir`], which this call is what snapshots.
pub(crate) fn config() -> &'static Config {
    CONFIG.get_or_init(Config::load)
}

/// The user's REAL config directory, snapshotted from the environment before
/// `workaround::workaround_gtk46_compose_crash()` redirects `XDG_CONFIG_HOME` to a
/// temp dir. Every consumer that wants a user config path must come through here.
///
/// ⚠️ **Never replace this with `glib::user_config_dir()`.** This is not a needless
/// re-implementation of XDG — it is forced by the workaround, and the two cannot
/// coexist:
///
/// * `g_get_user_config_dir()` reads the environment EXACTLY ONCE and caches the
///   answer in a global static forever (`gutils.c:1865-1878`); whoever calls first
///   wins, permanently.
/// * GTK 4.6's compose table reads that same cached global
///   (`gtkimcontextsimple.c:278`) — it is the very mechanism the workaround relies
///   on to avoid the crash.
///
/// So a GLib read AFTER the redirect resolves into the temp dir (silently losing
/// the user's config and theme overrides), and a GLib read BEFORE it caches the
/// real dir and re-arms the crash. There is no ordering that gives both, because
/// it is one global cache shared with GTK. Hence: snapshot from `std::env`, and
/// keep GLib out of the config path entirely. (`XDG_DATA_HOME`/`XDG_DATA_DIRS` are
/// untouched by the redirect, so `glib::user_data_dir()` and friends are fine.)
///
/// Only the FALLBACK is platform-specific: `HOME` is a POSIX convention Windows does
/// not set, so assuming it there made this return `None` forever and the user's
/// `config.toml` — and their `themes.toml` overrides, which come through here — were
/// unreachable (ScrAP-167). The snapshot-before-redirect discipline above is unchanged
/// and still applies on every platform; the Windows branch simply has no redirect to
/// race, since `workaround.rs` is `#[cfg(unix)]`.
pub(crate) fn user_config_dir() -> Option<PathBuf> {
    USER_CONFIG_DIR
        .get_or_init(|| {
            let dir = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(config_home_fallback);
            if dir.is_none() {
                // This went unnoticed through an entire platform port because it
                // returned None in silence: indistinguishable from "the user has
                // no config file", so the app just used defaults and never said
                // why (ScrAP-167). `get_or_init` runs once, so this cannot become
                // log spam.
                log::warn!(
                    "no user config directory could be located \
                     (checked XDG_CONFIG_HOME, then the platform fallback); \
                     config.toml and user theme overrides will be ignored"
                );
            }
            dir
        })
        .clone()
}

/// POSIX fallback: `~/.config`, per the XDG Base Directory spec.
#[cfg(unix)]
fn config_home_fallback() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
}

/// Windows fallback: **Roaming** AppData, deliberately not Local. Configuration is
/// the user's stated preferences — themes, view defaults, outline settings — which
/// should follow them between machines. Session *state* takes the opposite decision;
/// see `session::state_home_fallback`.
///
/// Hand-rolled from `std::env` rather than `glib::user_config_dir()` for the reason
/// in this module's doc comment above: that call is banned project-wide
/// (`clippy.toml` `disallowed-methods`) because its process-global cache is shared
/// with GTK's compose table. The ban is unconditional, and this branch keeps it that
/// way rather than carving out a platform exception.
#[cfg(windows)]
fn config_home_fallback() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Roaming"))
    })
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct Config {
    pub window: WindowConfig,
    pub view: ViewConfig,
    pub code: CodeConfig,
    pub outline: OutlineConfig,
}

// NOTE: there is deliberately no `[colors]` section. It was exactly "override
// these derived colours", which is now what a user `themes.toml` override of
// `[themes.system]` does — strictly more capable, and it reaches the typography
// and geometry the app hardcodes as well. Keeping both would have left two
// override layers with unspecified precedence. See `src/theme/`.

#[derive(serde::Deserialize)]
#[serde(default)]
pub(crate) struct OutlineConfig {
    /// Default width (px) of the outline sidebar pane.
    pub width: i32,
}

impl Default for OutlineConfig {
    fn default() -> Self {
        Self { width: 240 }
    }
}

#[derive(serde::Deserialize)]
#[serde(default)]
pub(crate) struct WindowConfig {
    pub width: i32,
    pub height: i32,
}

#[derive(serde::Deserialize)]
#[serde(default)]
pub(crate) struct ViewConfig {
    pub left_margin: i32,
    pub right_margin: i32,
    pub top_margin: i32,
    pub bottom_margin: i32,
}

#[derive(serde::Deserialize)]
#[serde(default)]
pub(crate) struct CodeConfig {
    /// CSS font-family string for inline code and code blocks.
    pub font: String,
    /// Internal left/right padding inside fenced code blocks (px).
    pub block_padding: i32,
    /// Syntect theme name for light desktop scheme (must be in ThemeSet::load_defaults()).
    /// Available light themes: "InspiredGitHub", "Solarized (light)", "base16-ocean.light"
    pub light_theme: String,
    /// Syntect theme name for dark desktop scheme.
    /// Available dark themes: "base16-ocean.dark", "base16-eighties.dark", "Solarized (dark)"
    pub dark_theme: String,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 900,
            height: 720,
        }
    }
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            left_margin: 20,
            right_margin: 20,
            top_margin: 16,
            bottom_margin: 16,
        }
    }
}

impl Default for CodeConfig {
    fn default() -> Self {
        Self {
            font: "monospace".to_string(),
            block_padding: 12,
            light_theme: "InspiredGitHub".to_string(),
            dark_theme: "base16-ocean.dark".to_string(),
        }
    }
}

impl Config {
    fn load() -> Self {
        // The twin of `session::save`'s guard, for the same failure and with the same
        // reasoning: reached in a test with no override, this reads the developer's real
        // config.toml, so the suite exercises different code depending on whose machine
        // it runs on. That is not hypothetical — it moved the coverage figure between two
        // hosts of the same platform, which is how it was found (ScrAP-123).
        // Unlike the session leak it is not destructive, only silently non-reproducible,
        // which is why it survived so much longer.
        //
        // Not platform-gated, because the leak is not platform-specific:
        // `user_config_dir` reads `XDG_CONFIG_HOME` on EVERY platform before it reaches
        // `config_home_fallback`, and `.cargo/config.toml`'s `[env]` table sets that
        // variable unconditionally. So the pin already covers Windows, and `APPDATA` is
        // never consulted under Cargo — the earlier `unix` gate reasoned from the
        // fallback rather than from the lookup order and left this dead there.
        // MEASURED on Windows/MSVC: a test binary launched by `cargo test` sees
        // `XDG_CONFIG_HOME = <repo>\target/test-config`, and inverting this predicate
        // makes 96 lib cases and the main-thread suite fail — so the assertion is
        // reached on this platform, not merely compiled into it.
        // This is an assertion inside production code, not a `#[cfg]`'d-out test —
        // POLICY's "never `#[cfg(platform)]` a test" is about tests that silently cease
        // to exist, and removing a platform gate is the direction that rule points.
        #[cfg(test)]
        assert!(
            std::env::var_os("XDG_CONFIG_HOME").is_some(),
            "Config::load reached in a test with no XDG_CONFIG_HOME override — the suite \
             would read the developer's real config.toml and its coverage would follow \
             the host. Run it through Cargo so .cargo/config.toml's [env] applies."
        );

        // Snapshots the real config dir on first call — which is why this must run
        // before the XDG_CONFIG_HOME redirect. See `user_config_dir`.
        let path = match user_config_dir() {
            Some(base) => base.join("scribobulate").join("config.toml"),
            None => return Self::default(),
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(), // missing file is not an error
        }
    }

    /// Parse config TOML, falling back to defaults on a parse error (a malformed
    /// config must never prevent the app from starting).  Split out from `load`
    /// so the parse + default-merge behaviour is unit-testable without touching
    /// the filesystem or environment.
    fn parse(text: &str) -> Self {
        toml::from_str(text).unwrap_or_else(|e| {
            // `log`, not `eprintln!`: the release build is `windows_subsystem =
            // "windows"`, so it has no console attached and anything written to
            // stderr there reaches nobody at all — a malformed config would fall
            // back to defaults with no diagnostic anywhere. Through the facade it
            // reaches the forensic log and the breadcrumb ring on every platform,
            // and still reaches stderr wherever there is one (POLICY § Logging).
            log::error!("config parse error: {e} — using defaults");
            Self::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.window.width, 900);
        assert_eq!(c.window.height, 720);
        assert_eq!(c.view.left_margin, 20);
        assert_eq!(c.code.font, "monospace");
        assert_eq!(c.code.block_padding, 12);
        assert_eq!(c.code.light_theme, "InspiredGitHub");
        assert_eq!(c.code.dark_theme, "base16-ocean.dark");
    }

    #[test]
    fn partial_toml_overrides_only_named_fields() {
        // A field set in one section overrides only itself; everything else —
        // including unset fields in the same section — keeps its default.
        let c = Config::parse("[window]\nwidth = 1234\n");
        assert_eq!(c.window.width, 1234);
        assert_eq!(c.window.height, 720); // serde(default) fills the rest
        assert_eq!(c.view.top_margin, 16); // untouched section stays default
    }

    /// `[colors]` was retired in favour of a `themes.toml` override of
    /// `[themes.system]`. A config still carrying it must be IGNORED, not rejected
    /// — an unknown section is not an error, so an old config keeps working (with
    /// its colours simply no longer applying) rather than failing the whole parse
    /// and silently reverting every other setting to a default.
    #[test]
    fn a_retired_colors_section_is_ignored_not_fatal() {
        let c = Config::parse("[colors]\nlink = \"#ff8800\"\n[window]\nwidth = 1234\n");
        assert_eq!(c.window.width, 1234);
    }

    #[test]
    fn malformed_toml_falls_back_to_defaults() {
        let c = Config::parse("this is not valid toml = = =");
        assert_eq!(c.window.width, 900);
    }
}
