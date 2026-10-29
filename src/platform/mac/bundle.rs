//! Point GTK at the data staged inside a `.app`, when running from one.
//!
//! `packaging/macos/bundle.sh` copies the GTK runtime, the icon themes, the GSettings
//! schemas and the gdk-pixbuf loaders into the bundle so it needs no Homebrew. None of
//! that is found automatically: GTK looks in the prefix it was COMPILED against, which on
//! the recipient's machine does not exist. This module supplies the search paths, and
//! nothing else — it is a source of plumbing for machinery every platform already shares,
//! exactly as `appearance` supplies a light/dark signal without re-theming anything.
//!
//! **Env vars rather than a wrapper script**, following `GSK_RENDERER` and the Windows
//! `GTK_CSD`: set in-process before GTK initialises, so the behaviour does not depend on
//! how the binary was started. A wrapper would also break the `scribobulate` symlink into
//! `Contents/MacOS/`, which is what gives a terminal launch the bundle's Dock identity.
//!
//! **It is inert outside a bundle.** A `cargo run` and the test suites take an early
//! return, so a developer's Homebrew paths keep working and nothing here can affect them.

use std::path::{Path, PathBuf};

/// Where the loader cache is written at startup. Regenerated every launch rather than
/// cached, because it encodes the bundle's ABSOLUTE path and the bundle can be moved
/// between launches — a stale one would name a directory that is no longer there.
const LOADER_CACHE_NAME: &str = "scribobulate-loaders.cache";

/// The bundle root (`…/Scribobulate.app/Contents`) if this executable is running from
/// inside one.
///
/// The test is the layout Apple defines — the executable lives in `Contents/MacOS/` — and
/// not the presence of `Info.plist`, which a developer could plausibly have beside a
/// binary for other reasons.
///
/// **The path is canonicalised first, and that is what makes the test survive a symlink.**
/// `install.sh` puts `scribobulate` on `PATH` as a link into the bundle, and on macOS
/// `std::env::current_exe` reports the path the process was LAUNCHED by — the link — not
/// its target: `/opt/homebrew/bin/scribobulate`, whose parent is `bin`. Every check below
/// then fails, `configure_paths` takes its early return, and the app runs a bundle's
/// binary with none of the bundle's data. It starts and looks broadly right, which is why
/// this went unnoticed: what it actually loses is the staged gdk-pixbuf `loaders.cache`,
/// so image decodes return NULL and GTK walks on with it — the startup slough of
/// `GDK_IS_PIXBUF`/`GDK_IS_TEXTURE` assertion failures. A terminal launch through the link
/// is the normal way to run this app, so the failing spelling is the common one.
fn contents_dir() -> Option<PathBuf> {
    contents_for_exe(&std::env::current_exe().ok()?)
}

/// The librsvg `packaging/macos/bundle.sh` stages, relative to `Contents`, and which
/// `packaging/macos/verify-librsvg.sh` holds to the CVE-2026-96889 floor before the
/// bundle is delivered.
const STAGED_LIBRSVG: &str = "Frameworks/librsvg-2.2.dylib";

/// Whether this process runs from a bundle that carries its own librsvg — the one the
/// bundle's gate verified, so the XInclude screen may stand down. Outside a bundle
/// (`cargo run`, `./target/release/scribobulate`, a test binary) the SVG loader resolves
/// Homebrew's librsvg, whose version nothing checks, and this is `false`.
pub(crate) fn carries_verified_librsvg() -> bool {
    contents_dir().is_some_and(|contents| librsvg_staged_in(&contents))
}

/// [`carries_verified_librsvg`]'s decision over a `Contents` given rather than found.
fn librsvg_staged_in(contents: &Path) -> bool {
    contents.join(STAGED_LIBRSVG).is_file()
}

/// [`contents_dir`]'s decision, over an executable path given rather than discovered — the
/// whole of it, so the launch path is the only thing the caller supplies and the only
/// thing a test has to stand in for.
fn contents_for_exe(exe: &Path) -> Option<PathBuf> {
    // A path that cannot be canonicalised is still tested as it stands: only a symlink
    // needs resolving, and a failure here must not cost a direct launch its bundle.
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let macos = exe.parent()?;
    if macos.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    Some(contents.to_path_buf())
}

/// Rewrite a `loaders.cache` so every module is named by ABSOLUTE path.
///
/// The staged cache holds bare basenames, and a bare name is the one spelling that is
/// silently wrong here: gdk-pixbuf passes the string to `g_module_open` verbatim on a
/// non-relocatable build (Homebrew's is `-Drelocatable=false`), and a name with no slash
/// then falls through to the dynamic loader's own search. On the machine that built the
/// bundle that search finds Homebrew's copy and everything works; on the recipient's it
/// finds nothing and images silently fail to decode. Absolute is also the spelling that
/// stays correct if that build flag ever flips, since the relocatable branch passes an
/// absolute path through unchanged.
///
/// Module lines are the ones starting with `"` at column zero; every other line —
/// comments, the MIME/extension/signature blocks, blank lines — is copied through
/// untouched. `# LoaderDir = …` is a comment and inert, so it is deliberately not special
/// cased.
pub(crate) fn absolutize_loader_cache(cache: &str, loaders_dir: &Path) -> String {
    let mut out = String::with_capacity(cache.len() + 512);
    for line in cache.lines() {
        match module_basename(line) {
            Some(name) => {
                let full = loaders_dir.join(name);
                out.push('"');
                out.push_str(&full.to_string_lossy());
                out.push_str("\"\n");
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// The module basename a cache line names, if it is a module line.
///
/// A module line is a quoted path ALONE on the line. The blocks beneath one also begin
/// with a quote — `"image/png" ""` and so on — so quoting is not the discriminator and
/// treating it as one would rewrite MIME types into filesystem paths.
fn module_basename(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('"')?;
    let inner = rest.strip_suffix('"')?;
    if inner.is_empty() || inner.contains('"') {
        return None;
    }
    Some(inner.rsplit('/').next().unwrap_or(inner))
}

/// Set the search paths for a bundled run. A no-op anywhere else.
///
/// Call before GTK initialises. Every failure here is soft: the app still starts, with
/// whatever GTK finds by itself, because a missing icon theme is a degraded window and
/// refusing to launch over it would be worse than the defect.
pub(crate) fn configure_paths() {
    let Some(contents) = contents_dir() else {
        return;
    };
    let resources = contents.join("Resources");
    let share = resources.join("share");
    if !share.is_dir() {
        // A bundle without staged data — an older build, or one assembled by hand. Leave
        // the environment alone rather than pointing GTK at directories that do not exist.
        return;
    }

    // Icon themes, and anything else GTK resolves through the XDG data path.
    //
    // PREPENDED, AND THE FALLBACK IS NOT AN EMPTY STRING. Setting this variable REPLACES
    // the platform default rather than adding to it — measured by the Windows seat, whose
    // state E pointed it at one unrelated directory and broke a lookup that had been
    // working, because the default was what had been carrying it. So when the variable is
    // unset, the XDG default has to be written back explicitly or this function silently
    // takes /usr/local/share and /usr/share away from every other data lookup in the
    // process. A value already set is the operator's and outranks ours.
    //
    // The entry is the `share` ROOT, never a directory below it: consumers append their
    // own subdirectory (`gtksourceview-5/language-specs`, `icons/…`), so an entry one
    // level deeper looks correct in a listing and matches nothing.
    const XDG_DEFAULT: &str = "/usr/local/share:/usr/share";
    let existing = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let tail = if existing.is_empty() {
        XDG_DEFAULT
    } else {
        existing.as_str()
    };
    std::env::set_var(
        "XDG_DATA_DIRS",
        format!("{}:{tail}", share.to_string_lossy()),
    );

    let schemas = share.join("glib-2.0").join("schemas");
    if schemas.is_dir() {
        std::env::set_var("GSETTINGS_SCHEMA_DIR", &schemas);
    }

    // The loader cache has to be rewritten rather than shipped ready to use, because the
    // absolute paths inside it are only knowable once the bundle is somewhere.
    let staged = resources.join("loaders.cache");
    let loaders_dir = contents.join("Frameworks");
    if let Ok(template) = std::fs::read_to_string(&staged) {
        let rewritten = absolutize_loader_cache(&template, &loaders_dir);
        let target = std::env::temp_dir().join(LOADER_CACHE_NAME);
        if std::fs::write(&target, rewritten).is_ok() {
            std::env::set_var("GDK_PIXBUF_MODULE_FILE", &target);
        }
    }
}

/// Put the bundle's GtkSourceView validators ahead of the compile-time prefix.
///
/// **Why this is not an env var, and so cannot live in [`configure_paths`].** Loading a
/// `.lang` validates it against `language2.rng`, read from a real filesystem path —
/// libxml takes a filename, so the GResource the `.lang` itself comes from cannot satisfy
/// it. The path is the compile-time `PACKAGE_DATADIR`, the Homebrew Cellar, which a
/// recipient does not have. Validation fails, the language is DROPPED, and the editor
/// loses Markdown highlighting with nothing on screen to say why.
///
/// **`XDG_DATA_DIRS` does not reach it**, and the reason is search ORDER rather than the
/// variable being ignored. GtkSourceView walks: user data dir, compiled `DATADIR`, its own
/// GResource, then `XDG_DATA_DIRS`. On a machine that HAS the Cellar the walk stops at the
/// second entry and never reaches a staged copy. Prepending is the supported way to get in
/// front of `DATADIR`.
///
/// **Call before the first `language()` or `guess_language()`.** The resolved RNG path is
/// cached on first use, so a later call changes nothing.
#[cfg(target_os = "macos")]
pub(crate) fn configure_language_path() {
    let Some(contents) = contents_dir() else {
        return;
    };
    let specs = contents
        .join("Resources")
        .join("gtksourceview-5")
        .join("language-specs");
    if !specs.is_dir() {
        return;
    }
    // READ-PREPEND-SET rather than `prepend_search_path`, and the reason is other
    // people's builds. The binding gates `prepend_search_path` behind its `v5_4` feature,
    // and enabling that would raise the GtkSourceView floor for LINUX AND WINDOWS to
    // satisfy a macOS-only need — the one thing the cross-platform rule says not to do.
    // `search_path` and `set_search_path` are ungated, and reading the existing list
    // before setting it preserves every default entry, including the GResource the .lang
    // files themselves come from. Same effect, no floor moved.
    let manager = sourceview::LanguageManager::default();
    let existing = manager.search_path();
    let mut dirs: Vec<&str> = Vec::with_capacity(existing.len() + 1);
    let ours = specs.to_string_lossy().into_owned();
    dirs.push(&ours);
    dirs.extend(existing.iter().map(glib::GString::as_str));
    manager.set_search_path(&dirs);
}

#[cfg(test)]
mod tests {
    use super::{absolutize_loader_cache, contents_for_exe, librsvg_staged_in, module_basename};
    use std::path::Path;

    /// A `Scribobulate.app/Contents/MacOS/scribobulate` under `root`, returning the
    /// executable's path and the `Contents` its detection must report.
    fn staged_bundle(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let contents = root.join("Scribobulate.app").join("Contents");
        let macos = contents.join("MacOS");
        std::fs::create_dir_all(&macos).expect("stage a bundle");
        let exe = macos.join("scribobulate");
        std::fs::write(&exe, b"#!/bin/sh\n").expect("stage an executable");
        (exe, contents)
    }

    #[test]
    fn a_launch_through_the_path_symlink_still_finds_the_bundle() {
        // The shape `packaging/macos/install.sh` creates: the .app in ~/Applications and
        // a symlink to its executable in Homebrew's bin/. `current_exe` reports the LINK
        // on macOS, so a detection that does not resolve it runs a bundled binary with
        // none of the bundle's staged data — no `loaders.cache`, so image decodes hand
        // GTK a NULL pixbuf and the startup log fills with `GDK_IS_PIXBUF` /
        // `GDK_IS_TEXTURE` assertion failures.
        let dir = tempfile::tempdir().unwrap();
        let (exe, contents) = staged_bundle(dir.path());
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join("scribobulate");
        if crate::testsymlink::symlink_or_skip(
            &exe,
            &link,
            "macOS bundle detection through the PATH symlink",
        )
        .is_err()
        {
            return;
        }
        assert_eq!(
            contents_for_exe(&link),
            Some(std::fs::canonicalize(&contents).unwrap()),
            "a launch through the PATH symlink must resolve to the bundle it points into"
        );
    }

    #[test]
    fn a_direct_launch_out_of_the_bundle_finds_it_too() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, contents) = staged_bundle(dir.path());
        assert_eq!(
            contents_for_exe(&exe),
            Some(std::fs::canonicalize(&contents).unwrap())
        );
    }

    #[test]
    fn a_binary_outside_any_bundle_is_not_mistaken_for_one() {
        // `cargo run` and every test binary land here, and must leave the developer's
        // Homebrew search paths alone.
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("scribobulate");
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        assert_eq!(contents_for_exe(&exe), None);
        // A `MacOS` directory that is not under a `Contents` is not a bundle either.
        let stray = dir.path().join("MacOS");
        std::fs::create_dir_all(&stray).unwrap();
        let exe = stray.join("scribobulate");
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        assert_eq!(contents_for_exe(&exe), None);
    }

    /// The screen stands down only for a bundle whose staged librsvg is there: a bundle
    /// staged without one is screened like a binary outside any bundle.
    #[test]
    fn only_a_bundle_carrying_its_librsvg_counts_as_verified() {
        let dir = tempfile::tempdir().unwrap();
        let (_, contents) = staged_bundle(dir.path());
        assert!(!librsvg_staged_in(&contents));
        let frameworks = contents.join("Frameworks");
        std::fs::create_dir_all(&frameworks).unwrap();
        std::fs::write(frameworks.join("librsvg-2.2.dylib"), b"").unwrap();
        assert!(librsvg_staged_in(&contents));
        assert!(
            !super::carries_verified_librsvg(),
            "a test binary runs outside any bundle"
        );
    }

    #[test]
    fn a_module_line_is_rewritten_to_an_absolute_path() {
        let out =
            absolutize_loader_cache("\"libpixbufloader-png.so\"\n", Path::new("/A/Frameworks"));
        assert_eq!(out, "\"/A/Frameworks/libpixbufloader-png.so\"\n");
    }

    #[test]
    fn a_mime_block_is_not_mistaken_for_a_module() {
        // These sit under a module line and are also quoted. Rewriting one would turn a
        // MIME type into a filesystem path and lose the loader's format registration.
        for line in [
            "\"image/png\" \"\"",
            "\"png\" \"\"",
            "\" \\211PNG\\r\\n\" \"\" 100",
        ] {
            assert!(
                module_basename(line).is_none(),
                "{line} must not read as a module"
            );
        }
    }

    #[test]
    fn comments_and_blanks_survive_untouched() {
        let src = "# GdkPixbuf Image Loader Modules file\n# LoaderDir = /opt/homebrew/x\n\n";
        assert_eq!(absolutize_loader_cache(src, Path::new("/A")), src);
    }

    #[test]
    fn an_already_absolute_module_is_repointed_at_the_bundle() {
        // The staged cache is generated from Homebrew's originals, so a path may survive
        // the build-time strip. Taking the basename means the result names the STAGED
        // copy either way — a cache still naming /opt/homebrew is the exact "works here,
        // silent for the recipient" defect this function exists to prevent.
        let out = absolutize_loader_cache(
            "\"/opt/homebrew/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so\"\n",
            Path::new("/A/Frameworks"),
        );
        assert_eq!(out, "\"/A/Frameworks/libpixbufloader-svg.so\"\n");
    }
}
