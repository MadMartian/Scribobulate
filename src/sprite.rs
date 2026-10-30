//! Sprite decoration: a theme naming an image to stand in for part of the closed
//! decoration vocabulary the engine already knows how to draw.
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
//! Three decoded forms are cached, all thread-local (GTK4 is single-threaded, so no
//! lock is needed and every unit test gets its own). **They live only while the
//! theme that named them is active:** [`clear_cache`] runs from [`crate::theme::set_active`],
//! so switching Pixel Quest → System drops every decoded raster rather than keeping
//! it for the rest of the process (TDD 18.58). The compiled-in PNG *bytes* stay in
//! the binary — that is the built-in-theme-with-nothing-on-disk promise, a few
//! kilobytes the OS can page out — and are not what this cache holds.
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
//! - [`surface`] — the sprite as a cairo image surface, for the PDF sink.
//!
//! **Admission happens twice, and the second time is on an open handle.** [`resolve`]
//! decides whether a reference may be read at all, once, when the theme loads;
//! [`open_checked`] re-establishes what it can at each read, because everything
//! between those two moments is represented by a `PathBuf`. [`admit_for_decode`] adds
//! the bound `resolve` cannot express from metadata — the size of the *decoded*
//! raster — and is the single place a decode failure is diagnosed, so a sprite that
//! is absent because it could not be produced is distinguishable in the log from a
//! theme that named none (ScrAP-324).

use gtk::{cairo, gdk};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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

/// The largest decoded raster this module will admit, in pixels.
///
/// [`MAX_SPRITE_BYTES`] bounds the *compressed* file and says nothing about what it
/// expands to. MEASURED on this project's Linux reference host: a 20000×20000
/// single-colour PNG compresses to 388 332 bytes — comfortably inside the byte cap —
/// and decoding it allocates ~1.2 GB, because neither `GdkTexture` nor `GdkPixbuf`
/// refuses on dimensions or offers a configurable limit. On the paint path that
/// decode happens inside GTK's `snapshot`, so the process dies with every unsaved
/// buffer in it.
///
/// **Why 4096×4096.** A sprite in this vocabulary is a tile, a bullet, a chip or a
/// band texture — the shipped ones run 16×16 to 420×56, the largest by area being
/// Candy's 320×320 quote tile, and the largest metric a theme can ask for is
/// clamped at 400 design-time px (×4 for zoom headroom is
/// 1600). 16.7 M pixels is ~10× past anything the vocabulary can display and ~24×
/// under the measured bomb, so no plausible sprite comes near it. Re-measure rather
/// than re-reason if this ever needs raising.
const MAX_SPRITE_PIXELS: i64 = 4096 * 4096;

/// Every sprite a **built-in** theme names, embedded at compile time.
///
/// Keyed by the theme-relative reference exactly as it appears in the compiled-in
/// `data/themes.toml`, so the file and this table are read as one pair; a key here
/// that no built-in theme names is dead weight, and a reference no key here matches
/// is a decoration that silently never appears.
/// `theme::tests::sprites::every_built_in_theme_sprite_reference_is_embedded` fails
/// on the second, which is the direction that costs something; its sibling
/// `every_sprite_on_disk_is_compiled_in_with_the_same_bytes` fails on a file added
/// under `data/sprites/` and never embedded — the direction the packaging scripts
/// open on their own. A row here that no theme names is dead weight and is
/// deliberately not guarded: it costs binary size and nothing else.
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
        "sprites/candy-sparkles-h1.png",
        include_bytes!("../data/sprites/candy-sparkles-h1.png"),
    ),
    (
        "sprites/candy-sparkles-h2.png",
        include_bytes!("../data/sprites/candy-sparkles-h2.png"),
    ),
    (
        "sprites/candy-stripes.png",
        include_bytes!("../data/sprites/candy-stripes.png"),
    ),
    (
        "sprites/chest-marker.png",
        include_bytes!("../data/sprites/chest-marker.png"),
    ),
    (
        "sprites/copper-plate.png",
        include_bytes!("../data/sprites/copper-plate.png"),
    ),
    (
        "sprites/flower-marker.png",
        include_bytes!("../data/sprites/flower-marker.png"),
    ),
    (
        "sprites/grass-platform.png",
        include_bytes!("../data/sprites/grass-platform.png"),
    ),
    (
        "sprites/scene-cave.png",
        include_bytes!("../data/sprites/scene-cave.png"),
    ),
    (
        "sprites/scene-knoll.png",
        include_bytes!("../data/sprites/scene-knoll.png"),
    ),
    (
        "sprites/scene-reef.png",
        include_bytes!("../data/sprites/scene-reef.png"),
    ),
    (
        "sprites/scene-ridge.png",
        include_bytes!("../data/sprites/scene-ridge.png"),
    ),
    (
        "sprites/scene-stage.png",
        include_bytes!("../data/sprites/scene-stage.png"),
    ),
    (
        "sprites/sign-down.png",
        include_bytes!("../data/sprites/sign-down.png"),
    ),
    (
        "sprites/sign-right.png",
        include_bytes!("../data/sprites/sign-right.png"),
    ),
    (
        "sprites/slime-marker.png",
        include_bytes!("../data/sprites/slime-marker.png"),
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
            // The ONE place a refusal is reported. `resolve` names the gate; the
            // wording is the `Refusal`'s, so a new gate arrives with its reason
            // attached rather than needing one remembered at the site.
            SpriteOrigin::Directory(dir) => match resolve(dir, rel) {
                Ok(p) => Some(SpriteRef::File(p)),
                Err(why) => {
                    log::warn!("theme: sprite {rel:?} {why} — ignored");
                    None
                }
            },
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
    /// compiled-in sprite. Used for diagnostics; never for opening anything.
    ///
    /// `None` where a file path is not valid UTF-8. Returned rather than spelled
    /// `""`, because the empty string is indistinguishable from a real answer and
    /// takes the extension, the MIME type and every diagnostic with it — a
    /// decoration that vanishes with no log, which is the failure mode this module
    /// exists to end.
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            SpriteRef::Named(rel) => Some(rel),
            SpriteRef::File(p) => p.to_str(),
            SpriteRef::Compiled(key) => Some(key),
        }
    }

    /// The reference's lowercased extension, or `None`. Both resolved variants carry
    /// one by construction — `resolve` allowlists it and every [`BUILTIN_SPRITES`]
    /// key is a filename — so a caller matching on it is matching on a closed set.
    ///
    /// A `File` is asked through its own [`Path`] rather than through [`name`], so a
    /// sprite whose *directory* is not UTF-8 keeps its extension.
    ///
    /// [`name`]: SpriteRef::name
    pub(crate) fn extension(&self) -> Option<String> {
        let ext = match self {
            SpriteRef::File(p) => p.extension()?,
            other => Path::new(other.name()?).extension()?,
        };
        Some(ext.to_str()?.to_ascii_lowercase())
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
        SpriteRef::File(p) => open_checked(p).map(std::borrow::Cow::Owned),
        SpriteRef::Compiled(key) => builtin_bytes(key).map(std::borrow::Cow::Borrowed),
        SpriteRef::Named(_) => {
            unresolved(r);
            None
        }
    }
}

/// Read a resolved sprite file, re-establishing the admission rules **on the open
/// handle** rather than trusting the ones [`resolve`] proved against a path.
///
/// [`resolve`] runs once, at `theme::load()`; this runs at first paint, at every
/// theme swap and at every export. Between them the guarantee was represented by a
/// `PathBuf` and nothing else — so a file that grew past the cap, or was replaced by
/// something that is not a regular file, was read whole regardless. Opening first and
/// asking [`std::fs::File::metadata`] closes that window for the file this call is
/// actually reading, and the `.take()` makes the cap **bound** the read rather than
/// predict it.
///
/// **Residual, stated rather than hidden.** Containment (`resolve`'s symlink check)
/// is *not* re-established here: proving it against an open handle needs
/// `openat2(RESOLVE_BENEATH)`, which is Linux-only, and this module is portable. A
/// directory component swapped for a symlink between resolve and read is therefore
/// followed. So is a path swapped for a FIFO — `open(2)` blocks before there is a
/// handle to interrogate. Both require write access to the theme's own directory,
/// which is the user's own configuration directory; `sdd/SCHEMA.md` § How a
/// `*_sprite` key resolves records the same limit for a theme author.
fn open_checked(p: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let f = match std::fs::File::open(p) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("theme: sprite {} could not be opened: {e}", p.display());
            return None;
        }
    };
    let meta = match f.metadata() {
        Ok(m) => m,
        Err(e) => {
            log::warn!("theme: sprite {} could not be measured: {e}", p.display());
            return None;
        }
    };
    if let Err(refusal) = crate::limits::is_regular_file_within(&meta, MAX_SPRITE_BYTES) {
        // `LoadRefusal`'s own `Display` is worded for a document and quotes the
        // document cap, so this caller words its own diagnostic.
        let why = match refusal {
            crate::limits::LoadRefusal::NotARegularFile => {
                "not a regular file (a directory, pipe, socket or device)".to_string()
            }
            crate::limits::LoadRefusal::TooLarge { bytes } => {
                format!("{bytes} bytes, over the {MAX_SPRITE_BYTES}-byte cap")
            }
        };
        log::warn!(
            "theme: sprite {} refused when opened: {why} — ignored",
            p.display()
        );
        return None;
    }
    let mut buf = Vec::new();
    if let Err(e) = f.take(MAX_SPRITE_BYTES + 1).read_to_end(&mut buf) {
        log::warn!("theme: sprite {} could not be read: {e}", p.display());
        return None;
    }
    if buf.len() as u64 > MAX_SPRITE_BYTES {
        log::warn!(
            "theme: sprite {} grew past the {MAX_SPRITE_BYTES}-byte cap after it was \
             validated — ignored",
            p.display()
        );
        return None;
    }
    Some(buf)
}

/// The table's bytes for a key a [`SpriteRef::Compiled`] already holds.
fn builtin_bytes(key: &str) -> Option<&'static [u8]> {
    BUILTIN_SPRITES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, b)| *b)
}

/// A reference that reached a read path still unresolved — a bug in the load step,
/// not in the theme. Logged at `error`, because the symptom otherwise is a
/// decoration that quietly is not there.
///
/// **Deliberately not a `debug_assert!(false, …)`.** This runs under a GTK
/// `snapshot` vfunc, and a panic there unwinds through C and aborts the process —
/// in a debug build, which is where every test and developer run lives. It would
/// also make the graceful degradation this function *documents* unassertable: the
/// path would be inert in release and fatal in debug, so no test could reach it.
fn unresolved(r: &SpriteRef) {
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
/// The test is `has_root`, not `is_absolute`, and the difference is a platform one:
/// on Windows a *rooted* reference with no drive prefix (`/etc/passwd`, `\Windows\
/// win.ini`) is **not** absolute, so `is_absolute` returned `false` for it and the
/// component check one gate below refused it as `NotPlainRelative` instead. The
/// reference was still refused either way — but a refusal's REASON is a cross-platform
/// verdict, asserted by name and reported to the user by name, and one that flips with
/// the host is the same class of drift every other rule in this module exists to
/// prevent. `has_root` is `is_absolute` on unix, so nothing changes there.
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
/// [`ALLOWED_EXTENSIONS`] **on the canonicalised target**, not on the name the theme
/// authored. Checking the authored name would let a symlink `chip.png → notes.txt`
/// inside the theme directory pass the allowlist while pointing the image loader at
/// arbitrary bytes — which is precisely what SCHEMA.md says the allowlist prevents.
///
/// **Regular file within the byte cap**, decided by
/// [`crate::limits::is_regular_file_within`] — the project's single admission test,
/// with this module's own cap. Both halves are required: a size test alone admits a
/// FIFO (whose reported length is zero), and the read then blocks the main thread
/// until a writer that never comes.
pub(crate) fn resolve(base: &Path, rel: &str) -> Result<PathBuf, Refusal> {
    let candidate = Path::new(rel);
    if candidate.has_root() {
        return Err(Refusal::Absolute);
    }
    if candidate
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(Refusal::NotPlainRelative);
    }
    let full = base.join(candidate);
    let Ok(real) = full.canonicalize() else {
        return Err(Refusal::NoSuchFile);
    };
    let Ok(real_base) = base.canonicalize() else {
        return Err(Refusal::UnreadableThemeDirectory);
    };
    if !real.starts_with(&real_base) {
        return Err(Refusal::OutsideThemeDirectory { resolved: real });
    }
    let ext_ok = real
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| ALLOWED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    if !ext_ok {
        return Err(Refusal::DisallowedExtension { resolved: real });
    }
    let meta =
        std::fs::metadata(&real).map_err(|e| Refusal::Unmeasurable { why: e.to_string() })?;
    match crate::limits::is_regular_file_within(&meta, MAX_SPRITE_BYTES) {
        Ok(()) => Ok(real),
        Err(crate::limits::LoadRefusal::NotARegularFile) => Err(Refusal::NotARegularFile),
        Err(crate::limits::LoadRefusal::TooLarge { bytes }) => Err(Refusal::TooLarge { bytes }),
    }
}

/// Why [`resolve`] refused a reference — **one variant per gate**.
///
/// The gates used to log for themselves, six of the eight in one voice
/// (`if … { log::warn!; return None }`) and two written as a `let … else` and a match
/// arm that returned `None` in silence. Both silent ones are reachable in the field:
/// `base.canonicalize()` fails when the themes file's own directory was removed or
/// became unreadable between load and resolve, and `metadata` fails on a permissions
/// change — and in either case EVERY sprite in that file goes inert at once, which is
/// "my whole theme lost its decorations" with nothing in the log.
///
/// Making the refusal a value rather than a log line is what stops that recurring: a
/// gate cannot be added without naming a reason, the wording lives in one place
/// instead of eight, and the discipline is carried by the type rather than by the
/// shape of the code around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A theme may point inside its own directory and nowhere else.
    Absolute,
    /// A component that is not a plain name — `..`, a root, a prefix, a `.`. Refused
    /// outright rather than interpreted, so there is no traversal to reason about.
    NotPlainRelative,
    /// The candidate does not canonicalise: it names nothing, or a component of the
    /// path is unreadable.
    NoSuchFile,
    /// The **theme's own directory** does not canonicalise. Not the reference's fault
    /// and not per-reference — every sprite in that file is about to be refused too.
    UnreadableThemeDirectory,
    /// Canonicalised, the target is outside the theme directory — a symlink inside it
    /// pointing out.
    OutsideThemeDirectory {
        resolved: PathBuf,
    },
    /// Checked on the CANONICALISED target, never on the authored name: a symlink
    /// `chip.png → notes.txt` inside the theme directory must not pass the allowlist.
    DisallowedExtension {
        resolved: PathBuf,
    },
    /// The target exists and canonicalised, but its metadata could not be read.
    Unmeasurable {
        why: String,
    },
    /// A pipe, socket or device. A size test alone would admit a FIFO, whose reported
    /// length is zero and whose read never returns.
    NotARegularFile,
    TooLarge {
        bytes: u64,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Absolute => write!(f, "is an absolute path"),
            Refusal::NotPlainRelative => write!(f, "is not a plain relative path"),
            Refusal::NoSuchFile => write!(f, "does not resolve to a real file"),
            Refusal::UnreadableThemeDirectory => write!(
                f,
                "cannot be resolved because the themes file's own directory is \
                 unreadable — every sprite in that file is inert"
            ),
            Refusal::OutsideThemeDirectory { resolved } => write!(
                f,
                "resolves to {}, outside the theme directory",
                resolved.display()
            ),
            Refusal::DisallowedExtension { resolved } => write!(
                f,
                "resolves to {}, which is not one of {ALLOWED_EXTENSIONS:?}",
                resolved.display()
            ),
            Refusal::Unmeasurable { why } => write!(f, "could not be measured: {why}"),
            Refusal::NotARegularFile => {
                write!(
                    f,
                    "is not a regular file (a directory, pipe, socket or device)"
                )
            }
            Refusal::TooLarge { bytes } => {
                write!(f, "is {bytes} bytes (cap {MAX_SPRITE_BYTES})")
            }
        }
    }
}

/// A decoded sprite as the PDF sink wants it: a cairo surface plus its natural pixel
/// width and height, which the sink needs in order to scale the pattern.
pub(crate) type Raster = (cairo::ImageSurface, f64, f64);

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
    /// Reference → the sprite as a cairo image surface with its natural size, for the
    /// PDF sink. See [`surface`].
    static SURFACES: RefCell<HashMap<SpriteRef, Option<Raster>>> = RefCell::new(HashMap::new());
    /// Reference → the whole ENCODED file, but only for a sprite [`texture`] found to
    /// be an animated WebP/GIF/APNG — `None` for a still sprite (TDD 27.9:
    /// animated theme sprites play). Populated inside
    /// [`texture`]'s own decode, from the SAME `imagedecode::decode` call, never a
    /// second one: `DecodedImage::animation` already answers "is this animated" and
    /// already carries the bytes a `richimg::Animation` needs, so this is free
    /// bookkeeping rather than a second probe. This is what lets
    /// [`animated_bytes`] answer a STILL sprite with one cheap `HashMap` lookup and no
    /// disk I/O — the "no new work on that path" requirement a still sprite keeps.
    static ANIMATION_BYTES: RefCell<HashMap<SpriteRef, Option<std::sync::Arc<[u8]>>>> =
        RefCell::new(HashMap::new());
    /// Every reference [`admit_for_decode`] has refused since the cache was last
    /// cleared. The decoded caches above each memoise their OWN failure, so without
    /// this a refused sprite was read, probed, screened and logged once per cache it
    /// reached — and a list bullet reaches two in one paint (`Frames::scaled` asks
    /// [`texture`] for the animation check, then falls back to [`scaled`]), so every
    /// theme selection logged the same refusal twice. A refusal is now decided and
    /// logged once per reference until the theme is reloaded.
    static INADMISSIBLE: RefCell<HashSet<SpriteRef>> = RefCell::new(HashSet::new());
}

/// The sprite as a cairo image surface, with its natural pixel size.
///
/// **Cached, and that is the point rather than a bonus.** The PDF sink decodes a band
/// sprite per *heading* and a marker sprite per *list item*; before this cache each of
/// those re-read and re-decoded the same file, so a document of 200 list items paid
/// 200 admissions, 200 decodes and 200 retained surfaces. `ink.rs` had already
/// hoisted the quote-bar and rule decodes out of their loops by hand for exactly that
/// reason — a cache is that hoist made general, so the next per-item decode cannot
/// re-introduce the cost by being written in the obvious place.
///
/// It also dedupes the artefact: cairo emits one image object per distinct source
/// surface, so a shared surface embeds the picture once rather than once per heading.
///
/// Goes through [`admit_for_decode`], so this path carries the same pixel cap and the
/// same diagnostics as the two texture paths — the PDF sink's own `decode` remains for
/// **document** images, which are a different trust boundary with a different cap.
pub(crate) fn surface(r: &SpriteRef) -> Option<Raster> {
    SURFACES.with(|c| {
        c.borrow_mut()
            .entry(r.clone())
            .or_insert_with(|| {
                use gtk::gdk::prelude::{TextureExt, TextureExtManual};
                let t = texture(r)?;
                let (w, h) = (t.width(), t.height());
                if w <= 0 || h <= 0 {
                    return None;
                }
                let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).ok()?;
                let stride = surface.stride() as usize;
                {
                    let mut data = surface.data().ok()?;
                    t.download(&mut data, stride);
                }
                Some((surface, f64::from(w), f64::from(h)))
            })
            .clone()
    })
}

/// The bytes of a sprite that is safe to *decode*: [`bytes`] plus the decoded-raster
/// bound [`MAX_SPRITE_PIXELS`].
///
/// **The one decode gate, and the one place a decode failure is logged.** Both decode
/// paths ([`texture`] and [`scaled`]) come through here, so a third one cannot forget
/// either the pixel cap or the log line — the asymmetry this replaced had `texture`
/// logging and `scaled` returning `None` in silence, which made a bullet or chip that
/// failed to decode indistinguishable from a theme that named no sprite (ScrAP-324's
/// class).
///
/// The dimension probe runs **before** the real decode, so an image bomb is refused
/// without ever being expanded — see [`crate::imagedecode::probe_dimensions`] for the
/// one spelling that makes that true, content-aware so a WebP sprite is measured
/// correctly even on a host with no gdk-pixbuf WebP loader.
fn admit_for_decode(r: &SpriteRef) -> Option<std::borrow::Cow<'static, [u8]>> {
    if INADMISSIBLE.with(|c| c.borrow().contains(r)) {
        return None;
    }
    let admitted = admit_uncached(r);
    if admitted.is_none() {
        INADMISSIBLE.with(|c| c.borrow_mut().insert(r.clone()));
    }
    admitted
}

/// [`admit_for_decode`]'s decision, made afresh: read, probe, screen, and log a refusal.
fn admit_uncached(r: &SpriteRef) -> Option<std::borrow::Cow<'static, [u8]>> {
    let raw = bytes(r)?;
    match crate::imagedecode::probe_dimensions(raw.as_ref()) {
        Some((w, h)) if i64::from(w) * i64::from(h) > MAX_SPRITE_PIXELS => {
            log::warn!(
                "theme: sprite {r} decodes to {w}×{h} pixels (cap {MAX_SPRITE_PIXELS}) — ignored"
            );
            None
        }
        Some(_) => Some(raw),
        // The probe screens for XInclude without recording (it has no origin to name),
        // so ask the recording screen whether that is why: a theme image is shown by no
        // document, so its notice is the theme's, in the window in front (TDD 2.23c;
        // Status-notice CAM row 9), once per content however often the theme reloads.
        None if crate::imagedecode::screen_refuses(raw.as_ref(), &format!("sprite {r}")) => {
            crate::imagedecode::report_shown_refusal(
                crate::imagedecode::RefusalTarget::Theme,
                &format!("sprite {r}"),
            );
            None
        }
        None => {
            log::warn!("theme: sprite {r} could not be read as an image — ignored");
            None
        }
    }
}

/// The natural pixel dimensions an image's header declares, without decoding it.
///
/// **Moved to [`crate::imagedecode::probe_pixel_size`]** — the same probe now
/// serves the document/remote-image path as well as the theme-sprite path, so the two
/// cannot disagree about what an image's header says (GTK4Rs/AP-311). Re-exported here so
/// `preview::build`'s pinned test (`the_shared_byte_probe_and_the_cap_agree_about_an_oversized_image`)
/// and this module's own dimension-probe test keep resolving `sprite::probe_pixel_size`
/// — `admit_for_decode` itself now calls the content-aware
/// [`crate::imagedecode::probe_dimensions`] instead, which is why this specific name has
/// no production caller left in THIS module.
#[allow(unused_imports)]
// kept resolvable for preview::build's pinned test and this module's own
pub(crate) use crate::imagedecode::probe_pixel_size;

/// The decoded texture for a resolved sprite reference, at its natural size.
///
/// **Decodes through [`crate::imagedecode::decode`]**, the application's one
/// choke point: content-sniffed, so a WebP sprite routes to `richimg` rather than the
/// gdk-pixbuf loader chain `Texture::from_bytes` used to reach (the same leak
/// `renderer::start`'s document-image path had — ScrAP-351). The file arm went
/// through `from_filename` until the read had to be bounded and re-validated on an
/// open handle ([`open_checked`]); one read path for both sources is what keeps that
/// guarantee from having two implementations. Animated sprites are a later work
/// package — only the first frame is kept here.
pub(crate) fn texture(r: &SpriteRef) -> Option<gdk::Texture> {
    NATURAL.with(|c| {
        c.borrow_mut()
            .entry(r.clone())
            .or_insert_with(|| {
                let raw = admit_for_decode(r)?;
                let origin = format!("sprite {r}");
                let decoded = crate::imagedecode::decode(raw.as_ref(), &origin)?;
                // Stash whether this decode came back animated, and its whole
                // encoded file if so — see `ANIMATION_BYTES`'s own doc comment for why
                // this rides the SAME decode rather than a second one.
                ANIMATION_BYTES.with(|a| {
                    a.borrow_mut()
                        .insert(r.clone(), decoded.animation.map(|anim| anim.bytes));
                });
                Some(decoded.texture)
            })
            .clone()
    })
}

/// The whole encoded file behind `r`, if [`texture`] has already decoded it and found
/// it to be an animated WebP/GIF/APNG — `None` for a still sprite, for one that failed
/// to decode, or for one [`texture`] has never been asked about yet (every current
/// caller asks `texture` for the natural/frame-0 texture before ever asking this, so in
/// practice the answer is always populated by the time it matters).
///
/// **One `HashMap` lookup, nothing else** — no disk read, no `richimg` call — which is
/// what keeps a still sprite's paint doing no new work at all (a still sprite must
/// render byte-identically to before animation existed): [`crate::animation::sprites::Frames`] calls this first and
/// returns the still texture verbatim on `None`.
pub(crate) fn animated_bytes(r: &SpriteRef) -> Option<std::sync::Arc<[u8]>> {
    ANIMATION_BYTES.with(|c| c.borrow().get(r).cloned().flatten())
}

/// The sprite resampled to exactly `w × h` with nearest-neighbour filtering. `w`/`h`
/// must both be positive; anything else is not a size and returns `None`.
///
/// Decodes through [`crate::imagedecode::decode_pixbuf`] — the `Pixbuf` shape
/// [`gtk::gdk_pixbuf::Pixbuf::scale_simple`] needs, content-sniffed the same way
/// [`texture`] is, so a WebP sprite resamples through `richimg` too rather than the
/// leaking `Pixbuf::from_stream` chain.
pub(crate) fn scaled(r: &SpriteRef, w: i32, h: i32) -> Option<gdk::Texture> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let key = (r.clone(), w, h);
    RESAMPLED.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let raw = admit_for_decode(r)?;
                let origin = format!("sprite {r}");
                let pb = crate::imagedecode::decode_pixbuf(raw.as_ref(), &origin)?;
                let resampled = crate::imagedecode::resample_nearest(&pb, w, h);
                if resampled.is_none() {
                    log::warn!("theme: sprite {r} could not be resampled to {w}×{h}");
                }
                resampled
            })
            .clone()
    })
}

/// Drop every decoded and resampled sprite. Called when the active theme changes so
/// a theme swap does not keep the previous theme's sprites resident for the rest of
/// the process, and so a texture keyed on a path a theme no longer resolves to
/// cannot be served to a caller that thinks it is asking about the new one.
///
/// **Necessary, not sufficient.** Widgets that cloned a texture out of the cache —
/// a heading-marker paintable in the buffer, a [`crate::widgets::rule::SpriteRule`],
/// an animated sprite's current frame — keep that GObject alive until the theme-change
/// re-render destroys them. The cache going empty is the first half; those holders
/// dropping is the second, and both are asserted.
pub(crate) fn clear_cache() {
    NATURAL.with(|c| c.borrow_mut().clear());
    RESAMPLED.with(|c| c.borrow_mut().clear());
    SURFACES.with(|c| c.borrow_mut().clear());
    ANIMATION_BYTES.with(|c| c.borrow_mut().clear());
    INADMISSIBLE.with(|c| c.borrow_mut().clear());
}

/// How many decoded forms the caches currently hold, including memoised failures.
///
/// Test seam for TDD 18.58: a theme switch that claims to unload sprites is only as
/// good as this going to zero (and a WeakRef proving the GObject actually finalized,
/// rather than remaining reachable from a widget the cache no longer names).
///
/// **All FOUR caches, and it used to be three.** `ANIMATION_BYTES` was missing, and it
/// is the one that matters most: the other three hold decoded forms of a single frame,
/// while this holds the whole encoded file an animation replays from — typically the
/// largest thing any of them retains. An oracle for "the caches emptied" that cannot
/// see the biggest cache reports zero while the bytes are still held, which is the
/// direction that reads as success. `clear_cache` has always cleared all four; only
/// the measurement was short. It also counts the refusal memo, `INADMISSIBLE`, so a
/// theme switch is seen to forget a refusal as well as a raster.
#[cfg(test)]
pub(crate) fn occupancy() -> usize {
    NATURAL.with(|c| c.borrow().len())
        + RESAMPLED.with(|c| c.borrow().len())
        + SURFACES.with(|c| c.borrow().len())
        + ANIMATION_BYTES.with(|c| c.borrow().len())
        + INADMISSIBLE.with(|c| c.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::glib;
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

    /// A square PNG of deterministic pseudo-random pixels, so its compressed stream
    /// is long enough to corrupt *inside*. A flat-colour fixture compresses to ~100
    /// bytes, which leaves no room between "the header parsed" and "the file ended".
    fn write_noise_png(path: &Path, side: i32) {
        use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
        let stride = (side * 3) as usize;
        let mut data = vec![0u8; stride * side as usize];
        // A plain LCG: reproducible, so a failure is reproducible too.
        let mut state: u32 = 0x1234_5678;
        for b in data.iter_mut() {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *b = (state >> 16) as u8;
        }
        let pb = Pixbuf::from_bytes(
            &glib::Bytes::from_owned(data),
            Colorspace::Rgb,
            false,
            8,
            side,
            side,
            stride as i32,
        );
        pb.savev(path, "png", &[]).expect("encode fixture");
    }

    #[test]
    fn resolve_admits_a_contained_relative_sprite() {
        let dir = tempfile::tempdir().unwrap();
        write_test_png(&dir.path().join("chip.png"));
        let got = resolve(dir.path(), "chip.png").expect("should resolve");
        assert_eq!(got, dir.path().join("chip.png").canonicalize().unwrap());
    }

    /// References that leave the theme directory by being **root-anchored**, in the
    /// grammar of the platform the test is running on. Index 0 is that platform's
    /// canonical spelling.
    ///
    /// **A POSIX literal is not a portable fixture, it is a different test on each
    /// platform wearing one spelling.** `"/etc/passwd"` is an *absolute* path on unix
    /// and a *rooted, drive-less* one on Windows — so a corpus of only that literal
    /// left the Windows absolute limb untested with any actual Windows absolute path,
    /// on the platform whose sprite search path starts at the user-writable
    /// `%APPDATA%`. That is a coverage hole wearing a passing test: the same species
    /// as the `#[cfg(unix)]`-inside-the-body one this module already carried, reached
    /// by a string constant instead of an attribute.
    ///
    /// The cases cannot simply be unioned across platforms, which is why this is a
    /// fixture and not a flat array: `C:\Windows\win.ini` is one ordinary relative
    /// component on unix (a backslash is not a separator there), so asserting it as
    /// absolute would fail for a reason that says nothing about containment. Each
    /// platform states its own grammar; the *rule* under test is identical on both.
    fn root_anchored_references() -> &'static [&'static str] {
        #[cfg(windows)]
        {
            &[
                // Drive-qualified: `is_absolute()` and `has_root()` both true.
                r"C:\Windows\win.ini",
                // UNC. Absolute, and the form that reaches another host entirely.
                r"\\server\share\evil.png",
                // Rooted with no drive prefix, in both separators. `is_absolute()` is
                // FALSE for these on Windows and `has_root()` is true -- the pair that
                // motivated the gate's predicate.
                r"\Windows\win.ini",
                "/etc/passwd",
            ]
        }
        #[cfg(not(windows))]
        {
            &["/etc/passwd"]
        }
    }

    /// The verdict, not just the refusal. MEASURED on Windows before the `has_root`
    /// change at the gate: `/etc/passwd` returned `Err(NotPlainRelative)` there,
    /// because it is *rooted* but not *absolute* (no drive prefix), so the gate above
    /// it did not fire. Both spellings are refused at the first gate now, on both
    /// platforms, which is what makes the reason a cross-platform verdict.
    #[test]
    fn resolve_refuses_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        for rel in root_anchored_references() {
            assert_eq!(
                resolve(dir.path(), rel),
                Err(Refusal::Absolute),
                "{rel:?} must be refused at the root-anchored gate"
            );
        }
        // The control: the same directory admits an ordinary contained reference, so
        // the loop above is not passing because every call refuses.
        write_test_png(&dir.path().join("chip.png"));
        assert!(resolve(dir.path(), "chip.png").is_ok());
    }

    #[test]
    fn resolve_refuses_parent_traversal() {
        // NESTED, not `dir/..`. The escape target has to genuinely exist — otherwise
        // the does-this-resolve gate refuses it and the test passes with traversal
        // checking deleted — but writing it one level ABOVE a `tempfile::tempdir()`
        // puts a FIXED filename in the shared temp directory, where two concurrent test
        // binaries collide and one deletes the other's fixture mid-run. A second temp
        // level keeps the escape real and the fixture private.
        let root = tempfile::tempdir().unwrap();
        write_test_png(&root.path().join("escaped.png"));
        let theme_dir = root.path().join("theme");
        std::fs::create_dir(&theme_dir).unwrap();
        assert_eq!(
            resolve(&theme_dir, "../escaped.png"),
            Err(Refusal::NotPlainRelative)
        );
    }

    #[test]
    fn resolve_refuses_an_unlisted_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("chip.svg"), b"<svg/>").unwrap();
        assert!(matches!(
            resolve(dir.path(), "chip.svg"),
            Err(Refusal::DisallowedExtension { .. })
        ));
    }

    #[test]
    fn resolve_refuses_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(dir.path(), "nope.png"), Err(Refusal::NoSuchFile));
    }

    #[test]
    fn resolve_refuses_a_file_over_the_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.png");
        // Bytes past the cap alone are enough to refuse — it need not even be a
        // valid PNG, because the size check runs before any decode is attempted.
        std::fs::write(&path, vec![0u8; (MAX_SPRITE_BYTES + 1) as usize]).unwrap();
        assert_eq!(
            resolve(dir.path(), "huge.png"),
            Err(Refusal::TooLarge {
                bytes: MAX_SPRITE_BYTES + 1
            })
        );
    }

    /// Containment is the one admission rule that cannot be decided from the
    /// reference string alone, so it is the one that most needs a real link.
    ///
    /// The `#[cfg(unix)]` used to sit **inside** this body, which on Windows compiled
    /// it to an empty function that ran, asserted nothing and reported green — and
    /// Windows is where the user-writable `%APPDATA%` row of SCHEMA's search-path
    /// table lives. It now goes through the shared runtime skip, so a host that
    /// genuinely cannot make a symlink says so in the run output instead of counting
    /// as a pass (GEP-4).
    ///
    /// **And on Windows it no longer skips at all.** Containment is a property of
    /// *reparse points*, not of symlinks specifically, so the fixture comes from
    /// `escaping_reference_or_skip`, which falls back to an NTFS directory junction —
    /// creatable without Developer Mode or elevation. MEASURED on an ordinary
    /// unelevated box: the symlink arm fails with os error 1314 and the junction arm
    /// then drives this assertion for real. The reference it hands back is
    /// `junction/real.png` there and `real.png` on unix, which is why the caller must
    /// use the returned string rather than assume either shape.
    #[test]
    fn resolve_refuses_a_symlink_that_escapes_the_theme_directory() {
        use crate::testsymlink::escaping_reference_or_skip;
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("real.png");
        write_test_png(&target);
        let theme_dir = tempfile::tempdir().unwrap();
        let Ok(escaping) =
            escaping_reference_or_skip(theme_dir.path(), &target, "SCHEMA sprite containment")
        else {
            return;
        };
        assert!(
            matches!(
                resolve(theme_dir.path(), &escaping),
                Err(Refusal::OutsideThemeDirectory { .. })
            ),
            "{escaping:?} must not resolve: it reads through to a file outside the \
             theme directory"
        );
        // Anti-vacuity: the identical fixture INSIDE the directory resolves, so the
        // refusal above is about containment and not about symlinks, or PNGs, or this
        // temp directory.
        write_test_png(&theme_dir.path().join("inside.png"));
        assert!(resolve(theme_dir.path(), "inside.png").is_ok());
    }

    /// The string-shape refusals, which every platform decides identically and which
    /// no `#[cfg]` may therefore gate away. `sub/../../x.png` is the one a reader
    /// most expects to be *normalised*; it is refused as a component, unnormalised.
    #[test]
    fn resolve_refuses_every_reference_shape_that_leaves_the_theme_directory() {
        let dir = tempfile::tempdir().unwrap();
        write_test_png(&dir.path().join("chip.png"));
        for hostile in [
            "sub/../../escaped.png",
            "./chip.png",
            "C:evil.png",
            "chip.png:stream",
            "",
        ] {
            assert!(
                resolve(dir.path(), hostile).is_err(),
                "{hostile:?} must not resolve"
            );
        }
        // The control: the same directory does resolve an ordinary reference, so the
        // loop above is not passing because every call refuses.
        assert!(resolve(dir.path(), "chip.png").is_ok());
    }

    /// **Every refusal reaches the log, through the one caller that reports them.**
    ///
    /// Six of the eight gates used to log for themselves and two did not — written as
    /// a `let … else` and a match arm rather than as an `if … { warn; return }`, so the
    /// discipline was applied by SHAPE rather than by rule and the two odd ones out
    /// went silent. Reporting at the single caller is what makes that unrepresentable;
    /// this drives the reference through `SpriteOrigin::resolve`, which is the path a
    /// themes file actually takes.
    #[test]
    fn a_refused_reference_is_reported_by_the_one_caller_that_reports_refusals() {
        let dir = tempfile::tempdir().unwrap();
        write_test_png(&dir.path().join("chip.png"));
        std::fs::write(dir.path().join("notes.txt"), b"not an image").unwrap();
        let origin = SpriteOrigin::Directory(dir.path());

        for (rel, fragment) in [
            ("/etc/passwd", "is an absolute path"),
            ("../escape.png", "is not a plain relative path"),
            ("nope.png", "does not resolve to a real file"),
            ("notes.txt", "which is not one of"),
        ] {
            let cap = crate::testlog::capture();
            assert!(
                origin.resolve(&SpriteRef::Named(rel.to_string())).is_none(),
                "{rel:?} must be refused"
            );
            assert!(
                cap.logged(log::Level::Warn, fragment),
                "{rel:?} was refused without saying why: {:?}",
                cap.records()
            );
            assert!(
                cap.logged(log::Level::Warn, rel),
                "the refusal must name the reference: {:?}",
                cap.records()
            );
        }

        // The control: a reference that resolves earns no complaint, so the loop above
        // is not passing because every call logs.
        let cap = crate::testlog::capture();
        assert!(origin
            .resolve(&SpriteRef::Named("chip.png".to_string()))
            .is_some());
        assert!(
            cap.records().is_empty(),
            "a reference that resolved logged anyway: {:?}",
            cap.records()
        );
    }

    /// Every gate carries a reason, and no two read alike.
    ///
    /// The two silent gates are the two this list would have caught: both are TOCTOU
    /// races (the theme directory, or the target's metadata, changing between one
    /// syscall and the next) and neither is reachable from a test, so the ONLY thing
    /// that can hold them is that the enum makes a reasonless gate impossible to write.
    /// This asserts the other half — that a variant cannot ship with an empty or
    /// duplicated message.
    #[test]
    fn every_refusal_variant_states_a_distinct_reason() {
        let all = [
            Refusal::Absolute,
            Refusal::NotPlainRelative,
            Refusal::NoSuchFile,
            Refusal::UnreadableThemeDirectory,
            Refusal::OutsideThemeDirectory {
                resolved: PathBuf::from("/elsewhere/x.png"),
            },
            Refusal::DisallowedExtension {
                resolved: PathBuf::from("/theme/x.txt"),
            },
            Refusal::Unmeasurable {
                why: "permission denied".to_string(),
            },
            Refusal::NotARegularFile,
            Refusal::TooLarge { bytes: 1 },
        ];
        let mut seen: Vec<String> = Vec::new();
        for r in &all {
            let said = r.to_string();
            assert!(!said.trim().is_empty(), "{r:?} states no reason");
            assert!(
                !seen.contains(&said),
                "{r:?} reads the same as another gate"
            );
            seen.push(said);
        }
    }

    /// A FIFO reports `len() == 0`, so a size test alone admits it and the read then
    /// blocks on `open(2)` until a writer that never comes. `limits`'s admission test
    /// asks the type first, which is why `resolve` calls it rather than comparing
    /// against the cap itself (`sdd/POLICY.md` § Input limits).
    #[test]
    fn resolve_refuses_a_fifo_even_though_it_reports_zero_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pipe.png");
        if !make_fifo(&path) {
            crate::testsymlink::skipped(
                "POLICY input limits: sprite FIFO admission",
                "this host cannot create a FIFO",
            );
            return;
        }
        // The premise, asserted rather than assumed: a size-only gate WOULD admit it.
        let meta = std::fs::metadata(&path).expect("the fifo we just created must stat");
        assert_eq!(meta.len(), 0, "a FIFO must report zero length");
        assert!(!meta.is_file(), "a FIFO must not be a regular file");
        assert_eq!(
            resolve(dir.path(), "pipe.png"),
            Err(Refusal::NotARegularFile)
        );
    }

    /// `Ok` when a FIFO now exists at `path`. Split by target family rather than
    /// `#[cfg]`-ing the caller away, for the reason `testsymlink` gives.
    fn make_fifo(path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: `c` is a valid NUL-terminated path for the duration of the call.
            unsafe { libc::mkfifo(c.as_ptr(), 0o600) == 0 }
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            false
        }
    }

    /// SCHEMA says the extension allowlist exists to "keep a theme file from steering
    /// the image loader at arbitrary bytes on disk by extension alone". Checked on the
    /// authored name it did not mean that: a link `chip.png → notes.txt` inside the
    /// theme's own directory passed while the loader was aimed at `notes.txt`.
    #[test]
    fn the_extension_allowlist_is_checked_on_the_link_target_not_the_authored_name() {
        use crate::testsymlink::symlink_or_skip;
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("notes.txt");
        std::fs::write(&secret, b"not an image").unwrap();
        let link = dir.path().join("chip.png");
        if symlink_or_skip(&secret, &link, "SCHEMA sprite extension allowlist").is_err() {
            return;
        }
        assert!(
            resolve(dir.path(), "chip.png").is_err(),
            "a contained link to a non-allowlisted target must be refused"
        );
        // Anti-vacuity: an in-directory link to an allowlisted target still resolves,
        // so the refusal is about the target's extension and not about links.
        write_test_png(&dir.path().join("real.png"));
        let ok_link = dir.path().join("alias.png");
        if symlink_or_skip(
            &dir.path().join("real.png"),
            &ok_link,
            "SCHEMA sprite alias",
        )
        .is_err()
        {
            return;
        }
        assert!(resolve(dir.path(), "alias.png").is_ok());
    }

    /// `MAX_SPRITE_BYTES` bounds the compressed file and says nothing about the
    /// raster it expands to. This fixture is well inside the byte cap and well past
    /// the pixel cap — the shape of a decompression bomb.
    #[test]
    fn a_sprite_that_decodes_past_the_pixel_cap_is_refused_and_diagnosed() {
        use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bomb.png");
        // 5000×5000 = 25 M pixels, past MAX_SPRITE_PIXELS (16.7 M). A single flat
        // colour compresses to a few tens of KiB, well under MAX_SPRITE_BYTES.
        let pb = Pixbuf::new(Colorspace::Rgb, false, 8, 5000, 5000).expect("allocate fixture");
        pb.fill(0x336699ff);
        pb.savev(&path, "png", &[]).expect("encode fixture");
        let bytes_on_disk = std::fs::metadata(&path).unwrap().len();
        assert!(
            bytes_on_disk <= MAX_SPRITE_BYTES,
            "the fixture must pass the BYTE cap ({bytes_on_disk} bytes) or it proves \
             the wrong gate"
        );
        let r = SpriteRef::File(resolve(dir.path(), "bomb.png").expect("passes admission"));

        let cap = crate::testlog::capture();
        assert!(texture(&r).is_none(), "the natural decode must refuse");
        clear_cache();
        assert!(scaled(&r, 16, 16).is_none(), "the resample must refuse too");
        assert!(
            cap.logged(log::Level::Warn, "decodes to 5000×5000 pixels"),
            "a refusal that is not logged is indistinguishable from a theme that \
             stated no sprite: {:?}",
            cap.records()
        );
    }

    /// TDD 2.23c — a theme sprite the XInclude screen refuses is reported as the
    /// theme's, once however often the theme reloads it. Named `.png`: a sprite must
    /// carry a raster extension, but its CONTENT picks the decoder, so SVG content under
    /// that name still reaches the loader the screen guards.
    #[test]
    fn a_sprite_refused_for_xinclude_is_reported_to_the_theme() {
        if !crate::imagedecode::screens_xinclude() {
            println!("SKIPPED [2.23c]: this platform does not screen SVG XInclude");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("hostile.png"),
            br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude"/>"#,
        )
        .expect("write fixture");
        let r = SpriteRef::File(resolve(dir.path(), "hostile.png").expect("passes admission"));
        let origin = format!("sprite {r}");
        // A list bullet's paint asks both caches in one pass (`Frames::scaled` asks
        // `texture` first, then falls back to `scaled`); that used to log it twice.
        let cap = crate::testlog::capture();
        assert!(texture(&r).is_none(), "the screen refuses it");
        assert!(scaled(&r, 16, 16).is_none(), "the resample is refused too");
        let refusals = |cap: &crate::testlog::Capture| {
            cap.records()
                .iter()
                .filter(|rec| rec.message.contains("refused before decoding"))
                .count()
        };
        assert_eq!(refusals(&cap), 1, "logged once per theme selection");
        clear_cache();
        assert!(scaled(&r, 16, 16).is_none(), "a reload refuses it again");
        assert_eq!(refusals(&cap), 2, "and a reload logs it again");
        drop(cap);
        assert_eq!(
            crate::imagedecode::told_for_test(&origin),
            [crate::imagedecode::RefusalTarget::Theme],
            "reported once, as the theme's"
        );
        crate::imagedecode::forget_for_test(&origin);
        clear_cache();
    }

    /// `scaled` used to swallow every failure while `texture` logged, so a bullet or
    /// checkbox whose file cleared admission but would not decode produced no sprite,
    /// no glyph, no fallback and no log line anywhere in the process — and the
    /// `RESAMPLED` cache memoised the silence for the rest of the run.
    ///
    /// **Two fixtures, because two independent mechanisms diagnose this** and either
    /// alone masks the other's absence (GTK4Rs/AP-254). Bytes no loader recognises
    /// never reach a decoder — `admit_for_decode`'s probe refuses them. A file whose
    /// *header* parses and whose data does not is the case that reaches the decoder,
    /// and it is the only one that can prove the decoder's own arm logs.
    #[test]
    fn an_admissible_but_undecodable_sprite_is_diagnosed_on_both_decode_paths() {
        let dir = tempfile::tempdir().unwrap();
        // Unrecognisable to every loader: refused by the header probe.
        std::fs::write(dir.path().join("broken.png"), b"this is not a PNG").unwrap();
        // A real PNG header over a corrupt compressed stream: the probe reads 64×64
        // and the decoder then fails. MEASURED that truncation does NOT produce this
        // window — libpng returns a partly-filled pixbuf once any IDAT has arrived,
        // so a truncated file decodes "successfully" and would leave this arm
        // unreached; corrupting bytes inside the stream is what reaches it.
        write_noise_png(&dir.path().join("whole.png"), 64);
        let mut raw = std::fs::read(dir.path().join("whole.png")).unwrap();
        for b in raw.iter_mut().take(260).skip(200) {
            *b ^= 0xFF;
        }
        std::fs::write(dir.path().join("corrupt.png"), &raw).unwrap();

        for (name, why) in [
            ("broken.png", "bytes no loader recognises"),
            (
                "corrupt.png",
                "a header that parses over data that does not",
            ),
        ] {
            let r = SpriteRef::File(resolve(dir.path(), name).expect("passes admission"));
            for path in ["scaled", "texture"] {
                clear_cache();
                let cap = crate::testlog::capture();
                let got = if path == "scaled" {
                    scaled(&r, 16, 16)
                } else {
                    texture(&r)
                };
                assert!(
                    got.is_none(),
                    "{name} must not produce a texture via {path}"
                );
                assert!(
                    cap.logged(log::Level::Warn, name),
                    "{path} must diagnose {name} ({why}); a refusal that is not \
                     logged is indistinguishable from a theme that stated no \
                     sprite. Records: {:?}",
                    cap.records()
                );
            }
        }
    }

    /// The probe answers the header's own dimensions and produces no pixbuf — it is a
    /// measurement, not a decode. Pinned separately because both callers only ever see
    /// its verdict, so a probe that silently started decoding would look identical
    /// from `admit_for_decode`.
    #[test]
    fn the_dimension_probe_reads_the_header_and_decodes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        write_test_png(&path);
        let raw = std::fs::read(&path).unwrap();
        assert_eq!(probe_pixel_size(&raw), Some((1, 1)));
        assert_eq!(probe_pixel_size(b"not an image at all"), None);
        assert_eq!(probe_pixel_size(&[]), None);
    }

    /// The byte cap must **bound** the read, not predict it: `resolve` measures the
    /// file once at load and the read happens at every paint and every export, so a
    /// file that grew in between was previously read whole.
    #[test]
    fn a_sprite_that_grew_after_admission_is_refused_at_read_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        write_test_png(&path);
        let r = SpriteRef::File(resolve(dir.path(), "chip.png").expect("passes admission"));
        assert!(bytes(&r).is_some(), "it reads before it grows");

        std::fs::write(&path, vec![0u8; (MAX_SPRITE_BYTES + 1) as usize]).unwrap();
        let cap = crate::testlog::capture();
        assert!(
            bytes(&r).is_none(),
            "the cap must be re-established on the open handle"
        );
        assert!(
            cap.logged(log::Level::Warn, "chip.png"),
            "and the refusal must be diagnosed: {:?}",
            cap.records()
        );
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

    /// A WebP sprite (`ALLOWED_EXTENSIONS` has always named it) decodes through
    /// [`crate::imagedecode`]'s choke point too — both the natural-size texture
    /// and the resample, at the seam this module owns. Never exercised by a WebP
    /// fixture before this: the pixel cap probe, the decode, and the resample all had
    /// zero coverage over this extension until now.
    #[test]
    fn a_webp_sprite_decodes_through_the_choke_point_natural_and_resampled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.webp");
        std::fs::write(
            &path,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/anim.webp"
            )),
        )
        .unwrap();
        let resolved = SpriteRef::File(resolve(dir.path(), "chip.webp").expect("resolves"));
        let tex = texture(&resolved).expect("richimg decodes the WebP sprite");
        assert_eq!((tex.width(), tex.height()), (480, 270));
        let small = scaled(&resolved, 16, 9).expect("richimg-backed resample");
        assert_eq!((small.width(), small.height()), (16, 9));
    }

    /// [`animated_bytes`] answers `Some` for an animated sprite and carries the
    /// whole encoded file — the exact input a `richimg::Animation` needs — but ONLY
    /// after [`texture`] has actually decoded it (never a fresh disk read of its own).
    #[test]
    fn animated_bytes_answers_the_encoded_file_for_an_animated_sprite_once_decoded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.webp");
        let webp = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/anim.webp"
        ));
        std::fs::write(&path, webp).unwrap();
        let resolved = SpriteRef::File(resolve(dir.path(), "chip.webp").expect("resolves"));

        assert!(
            animated_bytes(&resolved).is_none(),
            "before `texture` has ever decoded it, there is nothing to answer"
        );
        texture(&resolved).expect("richimg decodes the animated WebP");
        let bytes = animated_bytes(&resolved).expect("an animated sprite carries its bytes");
        assert_eq!(&*bytes, webp, "the whole encoded file, byte for byte");
    }

    /// A STILL sprite must answer `None` — a caller falls back to `texture`'s own
    /// result unchanged, and never attempts to build a `richimg::Animation` over a
    /// format `richimg` never decoded.
    #[test]
    fn animated_bytes_answers_none_for_a_still_sprite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.png");
        write_test_png(&path);
        let resolved = SpriteRef::File(resolve(dir.path(), "chip.png").expect("resolves"));
        texture(&resolved).expect("decodes");
        assert!(
            animated_bytes(&resolved).is_none(),
            "a still PNG must never be reported as animated"
        );
    }

    /// [`clear_cache`] drops the animation-bytes cache alongside the other three, so a
    /// theme switch cannot leave a previous theme's animated sprite bytes resident
    /// (TDD 18.58's rule, extended to the fourth cache).
    #[test]
    fn clear_cache_drops_animation_bytes_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.webp");
        std::fs::write(
            &path,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/anim.webp"
            )),
        )
        .unwrap();
        let resolved = SpriteRef::File(resolve(dir.path(), "chip.webp").expect("resolves"));
        texture(&resolved).expect("decodes");
        assert!(animated_bytes(&resolved).is_some());
        clear_cache();
        assert!(
            animated_bytes(&resolved).is_none(),
            "clear_cache must drop the animation-bytes cache too"
        );
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

        let (key, _bytes) = BUILTIN_SPRITES[0];
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

    /// TDD 18.58 — switching the active theme drops every decoded form, and
    /// selecting the sprite theme again still decodes. The occupancy going to
    /// zero is the cache half; the WeakRef test below is the GObject half.
    #[test]
    fn switching_the_active_theme_drops_decoded_sprites_and_they_reload() {
        clear_cache();
        let (key, _) = BUILTIN_SPRITES[0];
        let r = SpriteRef::Compiled(key);
        assert!(texture(&r).is_some());
        assert!(scaled(&r, 8, 8).is_some());
        assert!(surface(&r).is_some());
        let n = occupancy();
        assert!(
            n >= 3,
            "positive control: a decode of all three forms must occupy the cache, got {n}"
        );

        crate::theme::set_active(crate::theme::SYSTEM_ID);
        assert_eq!(
            occupancy(),
            0,
            "System names no sprite; the previous theme's rasters must not stay resident"
        );

        crate::theme::set_active("pixelquest");
        assert!(
            texture(&r).is_some(),
            "unload is not a one-way loss: the same sprite must decode again"
        );
        crate::theme::set_active(crate::theme::SYSTEM_ID);
        assert_eq!(occupancy(), 0);
    }

    /// Clearing the cache is what DROPS the GObject, not merely what forgets the
    /// key. Occupancy going to zero while a WeakRef still upgrades would mean the
    /// raster is still resident under a different name.
    #[test]
    fn clearing_the_cache_finalizes_a_texture_the_cache_alone_held() {
        use gtk::glib::clone::Downgrade;
        clear_cache();
        let (key, _) = BUILTIN_SPRITES[0];
        let r = SpriteRef::Compiled(key);
        let tex = texture(&r).expect("decodes");
        let weak = tex.downgrade();
        drop(tex);
        assert!(
            weak.upgrade().is_some(),
            "positive control: the cache is still holding the texture"
        );
        clear_cache();
        assert!(
            weak.upgrade().is_none(),
            "the cache was the last holder; clearing it must finalize the GObject"
        );
    }
}

/// Holders OUTSIDE the cache — a heading-marker paintable, a `SpriteRule` tile —
/// keep a texture alive after [`clear_cache`]. The theme-change re-render is what
/// drops those, and these tests pin that the holders actually release rather than
/// leaking the raster for the rest of the process (TDD 18.58, second half).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use gtk::glib;
    use gtk::prelude::*;

    fn compiled_texture() -> (gdk::Texture, glib::WeakRef<gdk::Texture>) {
        clear_cache();
        let (key, _) = BUILTIN_SPRITES[0];
        let tex = texture(&SpriteRef::Compiled(key)).expect("decodes");
        // `ObjectExt` vs `clone::Downgrade` both apply to `Texture` once gtk's
        // prelude is in scope; name the GObject one, which is what `WeakRef` is.
        let weak = gtk::glib::object::ObjectExt::downgrade(&tex);
        (tex, weak)
    }

    /// A heading marker is `insert_paintable` of a resampled texture. Clearing the
    /// cache must NOT finalize it while the run is still in the buffer — and
    /// deleting the run must. No selection: GtkTextBufferContent leaks a buffer
    /// ref per select→deselect (GTK4Rs/AP-318), which would pin the texture and
    /// make this look like our holder.
    #[gtktest::test]
    fn a_buffer_paintable_releases_its_texture_when_the_run_is_deleted() {
        let (tex, weak) = compiled_texture();
        let buf = gtk::TextBuffer::new(None);
        let mut iter = buf.start_iter();
        buf.insert_paintable(&mut iter, &tex);
        drop(tex);
        clear_cache();
        assert!(
            weak.upgrade().is_some(),
            "the buffer is still holding the paintable after the cache dropped its clone"
        );
        buf.delete(&mut buf.start_iter(), &mut buf.end_iter());
        assert!(
            weak.upgrade().is_none(),
            "deleting the paintable run must finalize the texture — this is the \
             drop the theme-change re-render relies on"
        );
    }

    /// `SpriteRule` clones the tile into its own cell. Dropping the widget is
    /// what the theme-change sweep does when it replaces a tiled rule with a
    /// `GtkSeparator` (or with nothing, on a theme that states no sprite).
    #[gtktest::test]
    fn a_sprite_rule_releases_its_tile_when_the_widget_is_dropped() {
        let (tex, weak) = compiled_texture();
        let rule = crate::widgets::rule::SpriteRule::new(tex.clone(), None);
        drop(tex);
        clear_cache();
        assert!(
            weak.upgrade().is_some(),
            "the rule widget is still holding the tile after the cache dropped its clone"
        );
        drop(rule);
        assert!(
            weak.upgrade().is_none(),
            "destroying the rule widget must finalize the tile"
        );
    }
}
