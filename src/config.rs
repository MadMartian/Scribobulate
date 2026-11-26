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
/// unreachable (GEP-42). The snapshot-before-redirect discipline above is unchanged
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
                // why (GEP-42). `get_or_init` runs once, so this cannot become
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

/// The platform fallback, read from the process environment.
fn config_home_fallback() -> Option<PathBuf> {
    config_home_fallback_in(|var| std::env::var_os(var))
}

/// POSIX fallback: `~/.config`, per the XDG Base Directory spec.
///
/// Takes the environment as a lookup rather than reading it, so a test can assert the
/// convention: `.cargo/config.toml` pins `XDG_CONFIG_HOME`, so under Cargo this path is
/// never reached and nothing else would ever exercise it.
#[cfg(unix)]
fn config_home_fallback_in(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    env("HOME").map(|h| PathBuf::from(h).join(".config"))
}

/// Windows fallback: **Roaming** AppData, deliberately not Local. Configuration is
/// the user's stated preferences — themes, view defaults, outline settings — which
/// should follow them between machines. Session *state* takes the opposite decision;
/// see `session::state_home_fallback_in`.
///
/// Hand-rolled from `std::env` rather than `glib::user_config_dir()` for the reason
/// in this module's doc comment above: that call is banned project-wide
/// (`clippy.toml` `disallowed-methods`) because its process-global cache is shared
/// with GTK's compose table. The ban is unconditional, and this branch keeps it that
/// way rather than carving out a platform exception.
#[cfg(windows)]
fn config_home_fallback_in(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    env("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Roaming")))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct Config {
    pub window: WindowConfig,
    pub view: ViewConfig,
    pub code: CodeConfig,
    pub outline: OutlineConfig,
    pub images: ImagesConfig,
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
    /// The longest the status bar writes the document's path, in characters, before
    /// its middle gives way to `...` (TDD 16.18); 0 never shortens it. 100 is what fits
    /// beside the indicators in a window of the default width.
    pub status_path_max: usize,
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

/// `[images]` — the two knobs `src/imagedecode` and `richimg` need from the
/// operator. Both are clamped after parsing
/// ([`Config::parse`]'s `clamped`), on the same "a malformed config must never
/// prevent startup" principle every other section here follows.
#[derive(serde::Deserialize)]
#[serde(default)]
pub(crate) struct ImagesConfig {
    /// Byte cap for a local image file, in MiB. Clamped to 1..=64. Default matches
    /// [`crate::limits::MAX_LOCAL_IMAGE_BYTES`]; see that constant's doc comment for
    /// how it was measured.
    pub local_file_limit_mib: u64,
    /// What a declared per-frame animation delay under richimg's fixed 20 ms
    /// threshold becomes. Clamped to 20..=1000. Default matches
    /// `richimg::Limits::default().short_delay_substitute`.
    pub short_frame_delay_ms: u64,
}

/// 1 MiB, spelled out once so [`ImagesConfig::clamp`] and the constant below don't
/// each restate the multiplication.
const BYTES_PER_MIB: u64 = 1024 * 1024;

/// [`ImagesConfig::local_file_limit_mib`]'s clamp range. The floor keeps the setting
/// meaningful (0 would refuse every local image); the ceiling matches
/// [`crate::limits::MAX_DOCUMENT_BYTES`] converted to MiB — a local image is never
/// allowed to cost more than a whole document.
const LOCAL_FILE_LIMIT_MIB_RANGE: std::ops::RangeInclusive<u64> = 1..=64;

/// [`ImagesConfig::short_frame_delay_ms`]'s clamp range: richimg's own fixed
/// short-delay THRESHOLD is 20 ms (`richimg::SHORT_DELAY_THRESHOLD`), so a substitute
/// below it would defeat its own purpose; 1000 ms is generously past any frame delay
/// a real animation declares.
const SHORT_FRAME_DELAY_MS_RANGE: std::ops::RangeInclusive<u64> = 20..=1000;

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            local_file_limit_mib: crate::limits::MAX_LOCAL_IMAGE_BYTES / BYTES_PER_MIB,
            short_frame_delay_ms: richimg::Limits::default()
                .short_delay_substitute
                .as_millis() as u64,
        }
    }
}

impl ImagesConfig {
    /// Clamp every field to its documented range — called once, from
    /// [`Config::parse`], so a config no human hand-edited still cannot push either
    /// value out of range.
    fn clamped(self) -> Self {
        Self {
            local_file_limit_mib: self.local_file_limit_mib.clamp(
                *LOCAL_FILE_LIMIT_MIB_RANGE.start(),
                *LOCAL_FILE_LIMIT_MIB_RANGE.end(),
            ),
            short_frame_delay_ms: self.short_frame_delay_ms.clamp(
                *SHORT_FRAME_DELAY_MS_RANGE.start(),
                *SHORT_FRAME_DELAY_MS_RANGE.end(),
            ),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 900,
            height: 720,
            status_path_max: 100,
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
        let cfg: Self = toml::from_str(text).unwrap_or_else(|e| {
            // `log`, not `eprintln!`: the release build is `windows_subsystem =
            // "windows"`, so it has no console attached and anything written to
            // stderr there reaches nobody at all — a malformed config would fall
            // back to defaults with no diagnostic anywhere. Through the facade it
            // reaches the forensic log and the breadcrumb ring on every platform,
            // and still reaches stderr wherever there is one (POLICY § Logging).
            log::error!("config parse error: {e} — using defaults");
            Self::default()
        });
        cfg.clamped()
    }

    /// Clamp every out-of-range field, the same "malformed input never prevents
    /// startup" principle the rest of this module follows. Split out so a future
    /// section's own range only has to be added here once.
    fn clamped(mut self) -> Self {
        self.images = self.images.clamped();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    /// The fallback follows the host's convention. Driven through a fake environment
    /// because the real one has `XDG_CONFIG_HOME` pinned, which makes the fallback
    /// unreachable from every other test — its coverage used to come from whichever
    /// host happened to run the suite. Each platform asserts its own convention.
    #[test]
    fn the_config_fallback_follows_the_platform_convention() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |var: &str| {
                vars.iter()
                    .find(|(k, _)| *k == var)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        let fallback = |vars| super::config_home_fallback_in(env(vars));

        assert_eq!(fallback(&[]), None, "no base variable, no directory");
        if cfg!(windows) {
            assert_eq!(
                fallback(&[("APPDATA", "roaming"), ("USERPROFILE", "profile")]),
                Some(PathBuf::from("roaming")),
                "APPDATA wins"
            );
            assert_eq!(
                fallback(&[("USERPROFILE", "profile")]),
                Some(PathBuf::from("profile").join("AppData").join("Roaming")),
                "without APPDATA, the profile's Roaming directory"
            );
        } else {
            assert_eq!(
                fallback(&[("HOME", "home")]),
                Some(PathBuf::from("home").join(".config"))
            );
        }
    }

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
        assert_eq!(c.images.local_file_limit_mib, 16);
        assert_eq!(c.images.short_frame_delay_ms, 50);
    }

    /// Absent-safe: a config file with no `[images]` section at all — every config
    /// this project shipped before that section existed — must still parse to the documented
    /// defaults rather than erroring or zeroing the section.
    #[test]
    fn a_config_with_no_images_section_gets_the_documented_defaults() {
        let c = Config::parse("[window]\nwidth = 1234\n");
        assert_eq!(c.images.local_file_limit_mib, 16);
        assert_eq!(c.images.short_frame_delay_ms, 50);
    }

    #[test]
    fn images_keys_parse_within_range() {
        let c = Config::parse("[images]\nlocal_file_limit_mib = 32\nshort_frame_delay_ms = 100\n");
        assert_eq!(c.images.local_file_limit_mib, 32);
        assert_eq!(c.images.short_frame_delay_ms, 100);
    }

    /// Out-of-range values are clamped, never rejected — the same "a malformed
    /// config must never prevent startup" principle every other section follows.
    #[test]
    fn images_keys_out_of_range_are_clamped_not_rejected() {
        let c = Config::parse("[images]\nlocal_file_limit_mib = 0\nshort_frame_delay_ms = 5\n");
        assert_eq!(
            c.images.local_file_limit_mib, 1,
            "clamped to the 1 MiB floor"
        );
        assert_eq!(
            c.images.short_frame_delay_ms, 20,
            "clamped to the 20 ms floor"
        );

        let c =
            Config::parse("[images]\nlocal_file_limit_mib = 999\nshort_frame_delay_ms = 999999\n");
        assert_eq!(
            c.images.local_file_limit_mib, 64,
            "clamped to the 64 MiB ceiling"
        );
        assert_eq!(
            c.images.short_frame_delay_ms, 1000,
            "clamped to the 1000 ms ceiling"
        );
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
