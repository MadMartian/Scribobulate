//! Sprite decoration: a theme naming an image file to stand in for part of the
//! closed decoration vocabulary (`sdd/PLAN.preview-decoration.md`, ratified). A
//! theme's `themes.toml` is read from `$XDG_CONFIG_HOME` and is therefore
//! attacker-influenced input (`sdd/THEMING.md` § Untrusted input) — this module is
//! the one place a path from that file is turned into bytes on disk, so every rule
//! below exists to keep a hostile or careless theme file from reaching outside its
//! own directory or opening something enormous.
//!
//! Two decoded forms are cached, both thread-local (GTK4 is single-threaded, so no
//! lock is needed and every unit test gets its own):
//! - [`texture`] — the sprite at its natural size, for a caller that scales it itself
//!   (the HTML export sink, which lets CSS do the scaling).
//! - [`scaled`] — the sprite pre-resampled to an exact pixel size with
//!   nearest-neighbour filtering. **Required** for anything drawn through GSK at this
//!   project's floor (`gtk4 0.10.3`, feature `v4_6`): `gtk_snapshot_append_texture`
//!   filters *linearly* with no filter choice — the nearest-neighbour variant,
//!   `append_scaled_texture`, is 4.10. Handing GSK an already-correctly-sized texture
//!   sidesteps the filter entirely, which is the only way pixel art stays crisp at
//!   any zoom on this floor (GTK4Rs skill; a `4.10`+ wrapper compiles and fails at
//!   link/runtime — GTK4Rs/AP-114).

use gtk::gdk;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Extensions a sprite may have. An allowlist, not a sniff: the point is to keep a
/// theme file from steering the image loader at arbitrary bytes on disk by extension
/// alone, not to validate the bytes themselves (the loader already refuses whatever
/// it cannot decode).
const ALLOWED_EXTENSIONS: [&str; 3] = ["png", "webp", "jpg"];

/// The largest sprite file this module will read. A theme file is untrusted input,
/// so an unbounded sprite is an unbounded decode — and in the HTML export sink it is
/// also an unbounded base64 embed, inflating by roughly a third on top.
const MAX_SPRITE_BYTES: u64 = 512 * 1024;

/// Resolve one theme-supplied sprite reference against the directory its
/// `themes.toml` was read from, returning an absolute path if — and only if — it
/// passes every check below. Any failure logs and returns `None`, so a broken or
/// hostile sprite reference degrades to "this decoration is absent" (the same
/// inert-by-default rule every other decoration in the vocabulary follows), never a
/// crash and never a path outside the theme's own directory.
///
/// **Relative only.** `rel` must not be an absolute path — a theme cannot point
/// anywhere on the filesystem, only within its own directory tree.
///
/// **No parent traversal.** Every path component must be a plain name; `..` (or a
/// root, prefix, or current-dir component) is refused outright rather than
/// interpreted, so there is no `../../etc/…` to reason about.
///
/// **Symlink-contained.** The candidate is canonicalised and checked to still start
/// with the canonicalised base directory — a symlink inside the theme directory
/// cannot point outside it. This is also what proves the file exists.
///
/// **Allowlisted extension**, checked case-insensitively against
/// [`ALLOWED_EXTENSIONS`], and **size-capped** at [`MAX_SPRITE_BYTES`] — checked here
/// via a metadata read, before anything decodes the file.
pub(crate) fn resolve(base: &Path, rel: &str) -> Option<PathBuf> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        log::warn!("theme: sprite {rel:?} is an absolute path — ignored");
        return None;
    }
    if candidate
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        log::warn!("theme: sprite {rel:?} is not a plain relative path — ignored");
        return None;
    }
    let ext_ok = candidate
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| ALLOWED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    if !ext_ok {
        log::warn!("theme: sprite {rel:?} is not one of {ALLOWED_EXTENSIONS:?} — ignored");
        return None;
    }
    let full = base.join(candidate);
    let Ok(real) = full.canonicalize() else {
        log::warn!("theme: sprite {rel:?} does not resolve to a real file — ignored");
        return None;
    };
    let Ok(real_base) = base.canonicalize() else {
        return None;
    };
    if !real.starts_with(&real_base) {
        log::warn!("theme: sprite {rel:?} resolves outside the theme directory — ignored");
        return None;
    }
    match std::fs::metadata(&real) {
        Ok(m) if m.len() <= MAX_SPRITE_BYTES => Some(real),
        Ok(m) => {
            log::warn!(
                "theme: sprite {rel:?} is {} bytes (cap {MAX_SPRITE_BYTES}) — ignored",
                m.len()
            );
            None
        }
        Err(_) => None,
    }
}

thread_local! {
    /// Path → decoded texture at its natural size, `None` for a path that failed to
    /// decode. Cached so a broken sprite is not re-attempted on every paint.
    static NATURAL: RefCell<HashMap<PathBuf, Option<gdk::Texture>>> =
        RefCell::new(HashMap::new());
    /// (path, w, h) → the sprite resampled to exactly that size with
    /// nearest-neighbour — see the module docs for why this exists at all.
    static RESAMPLED: RefCell<HashMap<(PathBuf, i32, i32), Option<gdk::Texture>>> =
        RefCell::new(HashMap::new());
}

/// The decoded texture for an already-[`resolve`]d sprite path, at its natural size.
pub(crate) fn texture(path: &Path) -> Option<gdk::Texture> {
    NATURAL.with(|c| {
        c.borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| match gdk::Texture::from_filename(path) {
                Ok(t) => Some(t),
                Err(e) => {
                    log::warn!("theme: sprite {} failed to decode: {e}", path.display());
                    None
                }
            })
            .clone()
    })
}

/// The sprite resampled to exactly `w × h` with nearest-neighbour filtering. `w`/`h`
/// must both be positive; anything else is not a size and returns `None`.
pub(crate) fn scaled(path: &Path, w: i32, h: i32) -> Option<gdk::Texture> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let key = (path.to_path_buf(), w, h);
    RESAMPLED.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                use gtk::gdk_pixbuf::{InterpType, Pixbuf};
                let pb = Pixbuf::from_file(path).ok()?;
                let resampled = pb.scale_simple(w, h, InterpType::Nearest)?;
                Some(gdk::Texture::for_pixbuf(&resampled))
            })
            .clone()
    })
}

/// Drop every decoded and resampled sprite. Called when the active theme changes so
/// a theme swap does not keep the previous theme's sprites resident for the rest of
/// the process, and so a texture keyed on a path a theme no longer resolves to
/// cannot be served to a caller that thinks it is asking about the new one.
pub(crate) fn clear_cache() {
    NATURAL.with(|c| c.borrow_mut().clear());
    RESAMPLED.with(|c| c.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::prelude::TextureExt;
    use std::io::Write;

    /// Writes a minimal valid 1×1 PNG (the smallest fixture that actually decodes,
    /// so `texture`/`scaled` tests exercise the real loader rather than a byte blob
    /// that merely passes the size/extension gate).
    fn write_test_png(path: &Path) {
        // A 1x1 white PNG, the standard minimal fixture (67 bytes).
        const PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59,
            0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(PNG).unwrap();
    }

    #[test]
    fn resolve_admits_a_contained_relative_sprite() {
        let dir = tempfile::tempdir().unwrap();
        write_test_png(&dir.path().join("chip.png"));
        let got = resolve(dir.path(), "chip.png").expect("should resolve");
        assert_eq!(got, dir.path().join("chip.png").canonicalize().unwrap());
    }

    #[test]
    fn resolve_refuses_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve(dir.path(), "/etc/passwd").is_none());
    }

    #[test]
    fn resolve_refuses_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        // A file that genuinely exists one level up — proves the refusal is about
        // the `..` component, not merely a missing target.
        write_test_png(&dir.path().join("../escaped.png"));
        assert!(resolve(dir.path(), "../escaped.png").is_none());
        std::fs::remove_file(dir.path().join("../escaped.png")).ok();
    }

    #[test]
    fn resolve_refuses_an_unlisted_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("chip.svg"), b"<svg/>").unwrap();
        assert!(resolve(dir.path(), "chip.svg").is_none());
    }

    #[test]
    fn resolve_refuses_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve(dir.path(), "nope.png").is_none());
    }

    #[test]
    fn resolve_refuses_a_file_over_the_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.png");
        // Bytes past the cap alone are enough to refuse — it need not even be a
        // valid PNG, because the size check runs before any decode is attempted.
        std::fs::write(&path, vec![0u8; (MAX_SPRITE_BYTES + 1) as usize]).unwrap();
        assert!(resolve(dir.path(), "huge.png").is_none());
    }

    #[test]
    fn resolve_refuses_a_symlink_that_escapes_the_theme_directory() {
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            write_test_png(&outside.path().join("real.png"));
            let theme_dir = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(
                outside.path().join("real.png"),
                theme_dir.path().join("link.png"),
            )
            .unwrap();
            assert!(resolve(theme_dir.path(), "link.png").is_none());
        }
    }

    #[test]
    fn texture_and_scaled_decode_a_resolved_sprite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        write_test_png(&path);
        let resolved = resolve(dir.path(), "chip.png").expect("resolves");
        let tex = texture(&resolved).expect("decodes");
        assert_eq!((tex.width(), tex.height()), (1, 1));
        let big = scaled(&resolved, 32, 32).expect("resamples");
        assert_eq!((big.width(), big.height()), (32, 32));
    }

    #[test]
    fn scaled_refuses_a_non_positive_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        write_test_png(&path);
        let resolved = resolve(dir.path(), "chip.png").unwrap();
        assert!(scaled(&resolved, 0, 10).is_none());
        assert!(scaled(&resolved, 10, -1).is_none());
    }
}
