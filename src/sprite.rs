//! Sprite decoration: a theme naming an image to stand in for part of the closed
//! decoration vocabulary (`sdd/PLAN.preview-decoration.md`, ratified).
//!
//! **A sprite has two possible sources, and which one a reference resolves to is
//! decided by where its `themes.toml` came from** ([`SpriteOrigin`]). A file on disk
//! is read from `$XDG_CONFIG_HOME` (or an install directory) and is therefore
//! attacker-influenced input (`sdd/THEMING.md` § Untrusted input) — this module is
//! the one place such a path is turned into bytes, so every rule in [`resolve`]
//! exists to keep a hostile or careless theme file from reaching outside its own
//! directory or opening something enormous. A **built-in** theme's sprite, by
//! contrast, is compiled into the binary by [`BUILTIN_SPRITES`] and needs no file, no
//! install step and no validation: the bytes are this project's own, fixed at build
//! time.
//!
//! That second source is not an optimisation, it is the feature working at all. A
//! built-in theme is compiled in (`include_str!`) precisely so it renders on a fresh
//! install with nothing on disk, and a sprite it names has to keep that promise — a
//! reference resolved against a directory that only exists once someone runs an
//! installer is absent on every developer build, every `.app` bundle, and every host
//! whose packaging forgot the asset, silently, because an unresolved sprite is
//! indistinguishable from a theme that stated none.
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

use gtk::{gdk, glib};
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

/// Every sprite a **built-in** theme names, embedded at compile time.
///
/// Keyed by the theme-relative reference exactly as it appears in the compiled-in
/// `data/themes.toml`, so the file and this table are read as one pair; a key here
/// that no built-in theme names is dead weight, and a reference no key here matches
/// is a decoration that silently never appears. `theme.rs`'s
/// `every_built_in_theme_sprite_reference_is_embedded` fails on the second, which is
/// the direction that costs something.
///
/// `include_bytes!` for the same reason `BUILTIN_THEMES_TOML` is `include_str!`: the
/// asset is part of the binary, so it is present wherever the binary is — no install
/// step, no filesystem lookup, no search path, and nothing about it can differ
/// between a developer build and a packaged one. Deliberately NOT resolved against
/// `CARGO_MANIFEST_DIR` or any other checkout-relative path, which would work only
/// when the binary is run from the source tree and fail — silently, since an
/// unresolved sprite is inert — for every installed copy.
const BUILTIN_SPRITES: &[(&str, &[u8])] = &[
    (
        "sprites/copper-plate.png",
        include_bytes!("../data/sprites/copper-plate.png"),
    ),
    (
        "sprites/grass-platform.png",
        include_bytes!("../data/sprites/grass-platform.png"),
    ),
    (
        "sprites/sword.png",
        include_bytes!("../data/sprites/sword.png"),
    ),
];

/// Where a themes file's sprite references resolve from — the one decision that
/// separates the two sources, made once per file at parse time and never guessed at
/// afterwards.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SpriteOrigin<'a> {
    /// A `themes.toml` read from disk: a reference names a file inside **that file's
    /// own directory**, and goes through every check in [`resolve`].
    Directory(&'a Path),
    /// The compiled-in `themes.toml`: a reference names an entry in
    /// [`BUILTIN_SPRITES`]. No filesystem, so none of `resolve`'s checks apply or
    /// could — there is no path to contain and no file to cap.
    Compiled,
}

impl SpriteOrigin<'_> {
    /// Turn one authored reference into the source it names, or `None` if this origin
    /// cannot supply it. The single point where the two sources are told apart.
    pub(crate) fn resolve(&self, r: &SpriteRef) -> Option<SpriteRef> {
        let SpriteRef::Named(rel) = r else {
            // Already resolved. Unreachable through `Themes::parse`, which resolves
            // every slot of every spec it returns, but stated rather than asserted so
            // a second caller cannot make re-resolution mean something else.
            return Some(r.clone());
        };
        match self {
            SpriteOrigin::Directory(dir) => resolve(dir, rel).map(SpriteRef::File),
            SpriteOrigin::Compiled => builtin(rel),
        }
    }
}

/// A theme's sprite reference, in one of two resolved states — plus the authored
/// state it arrives in.
///
/// **`Named` exists only between deserialization and resolution.** `serde` has to
/// produce a value from the TOML string before anything knows which file that string
/// came from, so a freshly parsed spec carries `Named`; `theme::Themes::parse` is the
/// sole constructor of a `ThemeSpec` and resolves every sprite slot before returning,
/// so no renderer, export sink or cache ever sees one. The read paths below say so
/// out loud rather than failing quietly, because a sprite that silently does not
/// appear is exactly the defect this type was introduced to end.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SpriteRef {
    /// The reference exactly as the theme file wrote it — theme-relative and
    /// unvalidated. Never reaches a consumer; see the type docs.
    Named(String),
    /// A validated absolute path inside the theme file's own directory.
    File(PathBuf),
    /// A [`BUILTIN_SPRITES`] key, whose bytes are compiled into this binary.
    Compiled(&'static str),
}

impl SpriteRef {
    /// The reference's own name — a filename for a file, the table key for a
    /// compiled-in sprite. Used for the extension (which decides a MIME type in the
    /// HTML sink) and for diagnostics; never for opening anything.
    pub(crate) fn name(&self) -> &str {
        match self {
            SpriteRef::Named(rel) => rel,
            SpriteRef::File(p) => p.to_str().unwrap_or_default(),
            SpriteRef::Compiled(key) => key,
        }
    }

    /// The reference's lowercased extension, or `None`. Both resolved variants carry
    /// one by construction — `resolve` allowlists it and every [`BUILTIN_SPRITES`]
    /// key is a filename — so a caller matching on it is matching on a closed set.
    pub(crate) fn extension(&self) -> Option<String> {
        Path::new(self.name())
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
    }
}

/// Deserializes to [`SpriteRef::Named`] and nothing else. There is deliberately no
/// way to author a `File` or a `Compiled` directly: which source a reference resolves
/// to is decided by the file it was read from ([`SpriteOrigin`]), never by the theme
/// stating it, so a theme cannot name a compiled-in sprite it did not ship with or a
/// path outside its own directory.
impl<'de> serde::Deserialize<'de> for SpriteRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(SpriteRef::Named(String::deserialize(d)?))
    }
}

impl std::fmt::Display for SpriteRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpriteRef::Named(rel) => write!(f, "{rel} (unresolved)"),
            SpriteRef::File(p) => write!(f, "{}", p.display()),
            SpriteRef::Compiled(key) => write!(f, "{key} (compiled in)"),
        }
    }
}

/// The compiled-in sprite a **built-in** theme's reference names, if the table
/// carries it. No validation, and none is possible or wanted: the bytes are this
/// project's own and were fixed when the binary was built.
pub(crate) fn builtin(rel: &str) -> Option<SpriteRef> {
    match BUILTIN_SPRITES.iter().find(|(key, _)| *key == rel) {
        Some((key, _)) => Some(SpriteRef::Compiled(key)),
        None => {
            log::warn!("theme: built-in sprite {rel:?} is not compiled into this binary — ignored");
            None
        }
    }
}

/// The bytes behind a resolved reference: read from disk for a file, handed straight
/// out for a compiled-in sprite.
///
/// **The one read path.** Every consumer that needs the raw image — both export sinks
/// — comes through here rather than calling `std::fs::read` on a path, which is what
/// lets a compiled-in sprite reach an artefact at all.
pub(crate) fn bytes(r: &SpriteRef) -> Option<std::borrow::Cow<'static, [u8]>> {
    match r {
        SpriteRef::File(p) => match std::fs::read(p) {
            Ok(b) => Some(std::borrow::Cow::Owned(b)),
            Err(e) => {
                log::warn!("theme: sprite {} could not be read: {e}", p.display());
                None
            }
        },
        SpriteRef::Compiled(key) => builtin_bytes(key).map(std::borrow::Cow::Borrowed),
        SpriteRef::Named(_) => {
            unresolved(r);
            None
        }
    }
}

/// The table's bytes for a key a [`SpriteRef::Compiled`] already holds.
fn builtin_bytes(key: &str) -> Option<&'static [u8]> {
    BUILTIN_SPRITES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, b)| *b)
}

/// A reference that reached a read path still unresolved — a bug in the load step,
/// not in the theme. Loud in a debug build and logged in a release one, because the
/// symptom otherwise is a decoration that quietly is not there.
fn unresolved(r: &SpriteRef) {
    debug_assert!(false, "sprite reference {r} reached a read path unresolved");
    log::error!("theme: sprite reference {r} reached a read path unresolved — ignored");
}

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
    /// Reference → decoded texture at its natural size, `None` for one that failed to
    /// decode. Cached so a broken sprite is not re-attempted on every paint. Keyed on
    /// the whole [`SpriteRef`], not on a path, so a file and a compiled-in sprite can
    /// never collide in it.
    static NATURAL: RefCell<HashMap<SpriteRef, Option<gdk::Texture>>> =
        RefCell::new(HashMap::new());
    /// (reference, w, h) → the sprite resampled to exactly that size with
    /// nearest-neighbour — see the module docs for why this exists at all.
    static RESAMPLED: RefCell<HashMap<(SpriteRef, i32, i32), Option<gdk::Texture>>> =
        RefCell::new(HashMap::new());
}

/// The decoded texture for a resolved sprite reference, at its natural size.
///
/// A file keeps GDK's own filename route rather than being read and re-decoded
/// through bytes: that path is unchanged from before compiled-in sprites existed, so
/// nothing about a user theme's sprite moved. A compiled-in sprite goes through
/// `Texture::from_bytes`, which is the same gdk-pixbuf loader chain by a different
/// door (GTK4Rs/AP-66) and is a 4.6 API, at this project's floor rather than above it
/// (GTK4Rs/AP-114).
pub(crate) fn texture(r: &SpriteRef) -> Option<gdk::Texture> {
    NATURAL.with(|c| {
        c.borrow_mut()
            .entry(r.clone())
            .or_insert_with(|| {
                let decoded = match r {
                    SpriteRef::File(p) => gdk::Texture::from_filename(p),
                    SpriteRef::Compiled(key) => {
                        let raw = builtin_bytes(key)?;
                        gdk::Texture::from_bytes(&glib::Bytes::from_static(raw))
                    }
                    SpriteRef::Named(_) => {
                        unresolved(r);
                        return None;
                    }
                };
                match decoded {
                    Ok(t) => Some(t),
                    Err(e) => {
                        log::warn!("theme: sprite {r} failed to decode: {e}");
                        None
                    }
                }
            })
            .clone()
    })
}

/// The sprite resampled to exactly `w × h` with nearest-neighbour filtering. `w`/`h`
/// must both be positive; anything else is not a size and returns `None`.
pub(crate) fn scaled(r: &SpriteRef, w: i32, h: i32) -> Option<gdk::Texture> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let key = (r.clone(), w, h);
    RESAMPLED.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                use gtk::gdk_pixbuf::{InterpType, Pixbuf};
                // Same split as `texture`: a file keeps `Pixbuf::from_file`, a
                // compiled-in sprite decodes its static bytes through a memory
                // stream. Both land on the same `scale_simple`, so the resample —
                // the part that decides whether pixel art survives zoom — is one
                // implementation, not one per source.
                let pb = match r {
                    SpriteRef::File(p) => Pixbuf::from_file(p).ok()?,
                    SpriteRef::Compiled(k) => {
                        let stream = gtk::gio::MemoryInputStream::from_bytes(
                            &glib::Bytes::from_static(builtin_bytes(k)?),
                        );
                        Pixbuf::from_stream(&stream, gtk::gio::Cancellable::NONE).ok()?
                    }
                    SpriteRef::Named(_) => {
                        unresolved(r);
                        return None;
                    }
                };
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
        let resolved = SpriteRef::File(resolve(dir.path(), "chip.png").expect("resolves"));
        let tex = texture(&resolved).expect("decodes");
        assert_eq!((tex.width(), tex.height()), (1, 1));
        let big = scaled(&resolved, 32, 32).expect("resamples");
        assert_eq!((big.width(), big.height()), (32, 32));
    }

    /// The compiled-in source, at the level the file-backed one is tested: a
    /// reference that IS in the table resolves and decodes, one that is not is refused
    /// like any other bad reference. Both halves matter — a lookup that answered
    /// `Some` for an absent key would hand a texture path a name with no bytes behind
    /// it, and one that answered `None` for a present key is the defect this whole
    /// source exists to fix.
    #[test]
    fn a_compiled_in_sprite_resolves_and_decodes_with_no_filesystem() {
        let (key, raw) = BUILTIN_SPRITES[0];
        let r = builtin(key).expect("a table key must resolve");
        assert_eq!(r, SpriteRef::Compiled(key));
        assert_eq!(bytes(&r).expect("bytes").as_ref(), raw);
        let tex = texture(&r).expect("a compiled-in sprite must decode");
        assert!(tex.width() > 0 && tex.height() > 0);
        // The resample path is the one the drawn gutter and the drawn chip take, so it
        // is not covered by the natural-size decode above.
        let small = scaled(&r, 8, 8).expect("a compiled-in sprite must resample");
        assert_eq!((small.width(), small.height()), (8, 8));
        assert!(builtin("sprites/no-such-sprite.png").is_none());
    }

    /// The origin is what tells the two sources apart, so it is tested as the single
    /// decision it is rather than only through its two callers.
    #[test]
    fn an_origin_decides_which_source_a_reference_resolves_from() {
        let dir = tempfile::tempdir().unwrap();
        write_test_png(&dir.path().join("chip.png"));
        let authored = SpriteRef::Named("chip.png".to_string());
        let from_dir = SpriteOrigin::Directory(dir.path())
            .resolve(&authored)
            .expect("a contained file resolves against its directory");
        assert!(matches!(from_dir, SpriteRef::File(_)));
        // The SAME reference under the compiled origin names no table entry, so it is
        // refused rather than reaching for a file the binary knows nothing about.
        assert!(SpriteOrigin::Compiled.resolve(&authored).is_none());

        let key = BUILTIN_SPRITES[0].0;
        let table = SpriteRef::Named(key.to_string());
        assert_eq!(
            SpriteOrigin::Compiled.resolve(&table),
            Some(SpriteRef::Compiled(key))
        );
        // And a table key is NOT a licence to read a file of that name out of a user
        // theme's directory: the directory origin answers only from disk.
        assert!(SpriteOrigin::Directory(dir.path())
            .resolve(&table)
            .is_none());
    }

    #[test]
    fn scaled_refuses_a_non_positive_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        write_test_png(&path);
        let resolved = SpriteRef::File(resolve(dir.path(), "chip.png").unwrap());
        assert!(scaled(&resolved, 0, 10).is_none());
        assert!(scaled(&resolved, 10, -1).is_none());
    }
}
