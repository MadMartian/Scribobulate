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
fn contents_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
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

    // Icon themes, and anything else GTK resolves through the XDG data path. Prepended
    // rather than replacing: a value already set is the operator's and outranks ours.
    let existing = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let combined = if existing.is_empty() {
        share.to_string_lossy().into_owned()
    } else {
        format!("{}:{existing}", share.to_string_lossy())
    };
    std::env::set_var("XDG_DATA_DIRS", combined);

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

#[cfg(test)]
mod tests {
    use super::{absolutize_loader_cache, module_basename};
    use std::path::Path;

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
