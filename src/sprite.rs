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
//! Three decoded forms are cached, all thread-local (GTK4 is single-threaded, so no
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

use gtk::{cairo, gdk, glib};
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
/// band texture — the three shipped ones are 8×8 to 64×64, and the largest metric a
/// theme can ask for is clamped at 400 design-time px (×4 for zoom headroom is
/// 1600). 16.7 M pixels is ~10× past anything the vocabulary can display and ~24×
/// under the measured bomb, so no plausible sprite comes near it. Re-measure rather
/// than re-reason if this ever needs raising.
const MAX_SPRITE_PIXELS: i64 = 4096 * 4096;

/// How much of a sprite file the dimension probe feeds the loader before giving up.
/// Every format on [`ALLOWED_EXTENSIONS`] carries its dimensions in the first few
/// dozen bytes; this is generous enough for a JPEG whose SOF marker sits behind a
/// large EXIF block, and small enough that the probe is not a second full read.
const PROBE_CHUNK: usize = 64 * 1024;

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
                "not a regular file (a pipe, socket or device)".to_string()
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
                write!(f, "is not a regular file (a pipe, socket or device)")
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
/// without ever being expanded — see [`probe_pixel_size`] for the one spelling that
/// makes that true.
fn admit_for_decode(r: &SpriteRef) -> Option<std::borrow::Cow<'static, [u8]>> {
    let raw = bytes(r)?;
    match probe_pixel_size(raw.as_ref()) {
        Some((w, h)) if i64::from(w) * i64::from(h) > MAX_SPRITE_PIXELS => {
            log::warn!(
                "theme: sprite {r} decodes to {w}×{h} pixels (cap {MAX_SPRITE_PIXELS}) — ignored"
            );
            None
        }
        Some(_) => Some(raw),
        None => {
            log::warn!("theme: sprite {r} could not be read as an image — ignored");
            None
        }
    }
}

/// The natural pixel dimensions an image's header declares, without decoding it.
///
/// Feeds a `GdkPixbufLoader` in chunks only until `size-prepared` fires, then **aborts
/// the load from inside that handler with `set_size(0, 0)`**. Returns `None` for bytes
/// no installed loader recognises.
///
/// **`0, 0` is load-bearing and no other value substitutes for it.** A non-zero
/// `set_size` asks the loader to *scale*, which several image modules — the PNG one
/// among them — honour only after allocating the pixel buffer at the file's declared
/// size; zero is the sentinel `gdk_pixbuf_get_file_info` itself uses to make the
/// module bail out before allocating anything. MEASURED on this project's Linux
/// reference host against a 20000×20000 PNG: peak RSS 1163 MB with `set_size(1, 1)`
/// and 20 MB with `set_size(0, 0)`, against an 18 MB do-nothing baseline. The
/// intuitive spelling reports the right dimensions, refuses the sprite, passes the
/// test — and allocates the bomb anyway, which is the whole thing this gate exists to
/// prevent.
///
/// Deliberately a loader rather than `gdk_pixbuf_get_file_info`, which is equivalent
/// (18 MB, same measurement) but takes a **path**: a compiled-in sprite has none, and
/// re-opening a file the caller has already read and validated would reintroduce the
/// check-then-use seam [`open_checked`] closes.
fn probe_pixel_size(raw: &[u8]) -> Option<(i32, i32)> {
    use gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
    use std::cell::Cell;
    use std::rc::Rc;

    let seen: Rc<Cell<Option<(i32, i32)>>> = Rc::new(Cell::new(None));
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.connect_size_prepared({
        let seen = Rc::clone(&seen);
        move |l, w, h| {
            seen.set(Some((w, h)));
            l.set_size(0, 0);
        }
    });
    for chunk in raw.chunks(PROBE_CHUNK) {
        if seen.get().is_some() {
            break;
        }
        if loader.write(chunk).is_err() {
            break;
        }
    }
    // The loader is deliberately fed only part of the file, so `close` reports a
    // premature end of file. Closing it anyway is required — an unclosed loader
    // warns at finalize.
    let _ = loader.close();
    seen.get()
}

/// The decoded texture for a resolved sprite reference, at its natural size.
///
/// Both sources decode from bytes through `Texture::from_bytes` — the same
/// gdk-pixbuf loader chain `from_filename` uses, by a different door (GTK4Rs/AP-66),
/// and a 4.6 API at this project's floor rather than above it (GTK4Rs/AP-114). The
/// file arm went through `from_filename` until the read had to be bounded and
/// re-validated on an open handle ([`open_checked`]); one read path for both sources
/// is what keeps that guarantee from having two implementations.
pub(crate) fn texture(r: &SpriteRef) -> Option<gdk::Texture> {
    NATURAL.with(|c| {
        c.borrow_mut()
            .entry(r.clone())
            .or_insert_with(|| {
                let raw = admit_for_decode(r)?;
                match gdk::Texture::from_bytes(&glib::Bytes::from_owned(raw.into_owned())) {
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
                let raw = admit_for_decode(r)?;
                let stream = gtk::gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(
                    raw.into_owned(),
                ));
                let pb = match Pixbuf::from_stream(&stream, gtk::gio::Cancellable::NONE) {
                    Ok(pb) => pb,
                    Err(e) => {
                        log::warn!("theme: sprite {r} failed to decode: {e}");
                        return None;
                    }
                };
                let Some(resampled) = pb.scale_simple(w, h, InterpType::Nearest) else {
                    log::warn!("theme: sprite {r} could not be resampled to {w}×{h}");
                    return None;
                };
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
    SURFACES.with(|c| c.borrow_mut().clear());
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

    /// The verdict, not just the refusal — and it is the verdict that was platform-
    /// dependent. MEASURED on Windows before the `has_root` change at the gate: this
    /// exact case returned `Err(NotPlainRelative)`, because `/etc/passwd` is *rooted*
    /// but not *absolute* there (no drive prefix), so the gate above it did not fire.
    #[test]
    fn resolve_refuses_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(dir.path(), "/etc/passwd"), Err(Refusal::Absolute));
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
    /// as a pass (ScrAP-212).
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
}
