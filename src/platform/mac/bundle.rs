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

/// The loader cache written at startup is `<temp dir>/<this><pid>-<n>.cache`. Regenerated
/// every launch rather than cached, because it encodes the bundle's ABSOLUTE path and the
/// bundle can be moved between launches — a stale one would name a directory that is no
/// longer there.
///
/// **One file per process, never a shared name.** A fixed name is rewritten by every
/// launch of every copy of the bundle, so the cache gdk-pixbuf reads at its first image
/// need not be the one [`carries_verified_librsvg`] checked at startup: another copy,
/// launched in between, puts its own loader paths there.
const LOADER_CACHE_PREFIX: &str = "scribobulate-loaders-";

/// The loader cache [`configure_paths`] wrote for this process, if it wrote one.
static INSTALLED_CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

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

/// The gdk-pixbuf SVG loader `bundle.sh` stages, relative to `Contents`: the module the
/// loader cache names for SVG, and the one object that links librsvg.
const STAGED_SVG_LOADER: &str = "Frameworks/libpixbufloader_svg.so";

/// Whether this process runs from a bundle whose SVG loader is bound to the bundle's own
/// librsvg, the copy the bundle's gate verified, so the XInclude screen may stand down.
/// Outside a bundle (`cargo run`, `./target/release/scribobulate`, a test binary) the SVG
/// loader resolves Homebrew's librsvg, whose version nothing checks, and this is `false`.
///
/// **Decided from what dyld loaded, not from which file is present.** The staged copy
/// being in `Frameworks/` does not make it the one in use: dyld resolves the loader's
/// `@rpath/librsvg-2.2.dylib` through the loader's own rpaths before the executable's,
/// and a bundle once carried a Homebrew rpath naming the directory that holds the `.app`.
/// A librsvg placed there was loaded instead of the staged one (measured;
/// `packaging/macos/rpath.sh`'s header). So the loader is loaded here, at the path
/// gdk-pixbuf will load it from, and every librsvg image in the process must then be the
/// staged file. The handle is kept: gdk-pixbuf loads the same module for the first SVG
/// anyway, and dyld hands it back the same image, already bound.
///
/// **And only when gdk-pixbuf will load that loader.** Which module gdk-pixbuf opens for
/// an SVG is whatever the module file it reads names, and [`configure_paths`] points it
/// at the bundle's cache only when writing that cache succeeds. When it does not,
/// gdk-pixbuf falls back to the cache it was compiled against, Homebrew's, whose SVG
/// loader binds Homebrew's librsvg, and the loader loaded here proves nothing about it.
/// So the module file checked is the one this process wrote, still the one
/// `GDK_PIXBUF_MODULE_FILE` (the setting that overrides the compiled default) names, and
/// every SVG module in it must be the staged loader. An inherited value is never trusted:
/// a module file this process did not write is not one it can vouch for.
///
/// Fails closed: no module file, one naming any other SVG loader, a loader that will not
/// load, or no librsvg image after it did, is `false`, and the screen stays up.
pub(crate) fn carries_verified_librsvg() -> bool {
    contents_dir().is_some_and(|contents| {
        let env = std::env::var_os("GDK_PIXBUF_MODULE_FILE");
        let module_file = module_file_in_use(INSTALLED_CACHE.get(), env.as_deref());
        verified(&contents, module_file, load_and_keep, loaded_images)
    })
}

/// The whole decision over the bundle at `contents`, with the module file gdk-pixbuf
/// reads, the loader load and dyld's image list given rather than discovered.
fn verified(
    contents: &Path,
    module_file: Option<&Path>,
    load: impl FnOnce(&Path) -> bool,
    images: impl FnOnce() -> Vec<PathBuf>,
) -> bool {
    svg_modules_are_staged(module_file, &contents.join(STAGED_SVG_LOADER))
        && loader_binds_staged_librsvg(contents, load, images)
}

/// The cache this process `installed`, if `env` (`GDK_PIXBUF_MODULE_FILE`) still names it.
fn module_file_in_use<'a>(
    installed: Option<&'a PathBuf>,
    env: Option<&std::ffi::OsStr>,
) -> Option<&'a Path> {
    installed
        .filter(|ours| env == Some(ours.as_os_str()))
        .map(PathBuf::as_path)
}

/// Whether gdk-pixbuf, reading `module_file`, would load SVG through `staged_loader` and
/// nothing else: the file exists, names at least one SVG module, and every one it names
/// is `staged_loader`, by absolute path. `None`, gdk-pixbuf reading its compiled-in
/// default, is `false`.
///
/// An SVG module is one whose format is `svg` or which claims `image/svg+xml`: gdk-pixbuf
/// picks a loader by content sniff and MIME as well as by name, so a second module
/// claiming the type could take the decode. A relative module name is not the staged
/// loader: the dynamic loader resolves it by its own search.
fn svg_modules_are_staged(module_file: Option<&Path>, staged_loader: &Path) -> bool {
    let Some(text) = module_file.and_then(|f| std::fs::read_to_string(f).ok()) else {
        return false;
    };
    let Ok(staged) = std::fs::canonicalize(staged_loader) else {
        return false;
    };
    let svg_modules = loader_cache_modules(&text)
        .into_iter()
        .filter(|m| m.format == "svg" || m.mime_types.contains(&"image/svg+xml"))
        .collect::<Vec<_>>();
    !svg_modules.is_empty()
        && svg_modules.iter().all(|m| {
            let path = Path::new(m.path);
            path.is_absolute() && std::fs::canonicalize(path).is_ok_and(|p| p == staged)
        })
}

/// One module block of a gdk-pixbuf `loaders.cache`: the module line, then the format
/// line (`"svg" 6 "gdk-pixbuf" …`), then the MIME types line.
struct CacheModule<'a> {
    path: &'a str,
    format: &'a str,
    mime_types: Vec<&'a str>,
}

/// The module blocks of a `loaders.cache`. A block is the module line and the lines up
/// to the next blank one; comments are skipped.
fn loader_cache_modules(cache: &str) -> Vec<CacheModule<'_>> {
    let mut modules = Vec::new();
    let mut lines = cache.lines().filter(|l| !l.starts_with('#'));
    while let Some(line) = lines.next() {
        let Some(path) = module_path(line) else {
            continue;
        };
        let block: Vec<&str> = lines
            .by_ref()
            .take_while(|l| !l.trim().is_empty())
            .collect();
        let quoted = |i: usize| block.get(i).map(|l| quoted_fields(l)).unwrap_or_default();
        modules.push(CacheModule {
            path,
            format: quoted(0).first().copied().unwrap_or(""),
            mime_types: quoted(1).into_iter().filter(|t| !t.is_empty()).collect(),
        });
    }
    modules
}

/// The `"…"` fields of a cache line, in order.
fn quoted_fields(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// The path a module line names, verbatim, if it is a module line ([`module_basename`]).
fn module_path(line: &str) -> Option<&str> {
    module_basename(line)?;
    line.strip_prefix('"')?.strip_suffix('"')
}

/// Whether loading the staged SVG loader with `load` leaves `images` (dyld's image list,
/// read after the load) holding the staged librsvg and no other. Both are parameters so
/// the composed decision is testable without a real loader.
fn loader_binds_staged_librsvg(
    contents: &Path,
    load: impl FnOnce(&Path) -> bool,
    images: impl FnOnce() -> Vec<PathBuf>,
) -> bool {
    let Ok(staged) = std::fs::canonicalize(contents.join(STAGED_LIBRSVG)) else {
        return false;
    };
    if !load(&contents.join(STAGED_SVG_LOADER)) {
        return false;
    }
    only_the_staged_librsvg_is_loaded(&images(), &staged)
}

/// `dlopen` `path` and leak the handle. `false` when it does not load.
fn load_and_keep(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is NUL-terminated and outlives the call. The module is a gdk-pixbuf
    // loader with no initialisers of its own, which gdk-pixbuf would `dlopen` in this
    // process for the first SVG regardless.
    !unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) }.is_null()
}

// dyld's image list, declared by hand: `libc` deprecates its bindings in favour of the
// `mach2` crate, which this one pair of stable libSystem symbols does not justify taking.
unsafe extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(image_index: u32) -> *const std::ffi::c_char;
}

/// The path of every image dyld has loaded into this process.
fn loaded_images() -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: plain reads of dyld's image list. An index past an image unloaded meanwhile
    // yields NULL, which is skipped; a non-NULL name is a NUL-terminated string dyld owns.
    unsafe {
        (0.._dyld_image_count())
            .filter_map(|i| {
                let name = _dyld_get_image_name(i);
                (!name.is_null()).then(|| {
                    let bytes = std::ffi::CStr::from_ptr(name).to_bytes();
                    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
                })
            })
            .collect()
    }
}

/// Whether `images` holds a librsvg and every librsvg in it is `staged` (canonical).
/// Two copies loaded, the staged one and another, is `false`: the loader is bound to one
/// of them, and nothing here says which.
fn only_the_staged_librsvg_is_loaded(images: &[PathBuf], staged: &Path) -> bool {
    let mut librsvgs = images
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "librsvg-2.2.dylib"))
        .peekable();
    librsvgs.peek().is_some()
        && librsvgs.all(|p| std::fs::canonicalize(p).is_ok_and(|p| p == staged))
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
    // absolute paths inside it are only knowable once the bundle is somewhere. A failure
    // leaves gdk-pixbuf on its compiled-in cache, and [`carries_verified_librsvg`] then
    // keeps the image screen up.
    let temp = std::env::temp_dir();
    sweep_dead_caches(&temp);
    if let Some(cache) = install_loader_cache(
        &resources.join("loaders.cache"),
        &contents.join("Frameworks"),
        &temp,
    ) {
        std::env::set_var("GDK_PIXBUF_MODULE_FILE", &cache);
        let _ = INSTALLED_CACHE.set(cache);
    }
}

/// Write the staged cache `template`, absolutized against `loaders_dir`, to a file of
/// this process's own in `dir`, and return its path; `None` when nothing was written.
///
/// Created exclusively (`O_EXCL`, mode 0600), so no other process's file is ever
/// overwritten or reused, and nothing else holds this one: the content read back is the
/// content written. That is also why it needs no write-then-rename: nobody but this
/// process reads it, and only after this returns. A name left by an earlier process
/// that had this pid is skipped for the next suffix.
fn install_loader_cache(template: &Path, loaders_dir: &Path, dir: &Path) -> Option<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let text = absolutize_loader_cache(&std::fs::read_to_string(template).ok()?, loaders_dir);
    let pid = std::process::id();
    for n in 0..16 {
        let target = dir.join(format!("{LOADER_CACHE_PREFIX}{pid}-{n}.cache"));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&target)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        };
        if file.write_all(text.as_bytes()).is_err() {
            let _ = std::fs::remove_file(&target);
            return None;
        }
        return Some(target);
    }
    None
}

/// Delete the loader caches in `dir` left by processes that have exited, so one file per
/// launch does not accumulate. A file whose pid is alive, or that is not ours, is left.
fn sweep_dead_caches(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix(LOADER_CACHE_PREFIX))
            .and_then(|rest| rest.split_once('-'))
            .and_then(|(pid, _)| pid.parse::<libc::pid_t>().ok())
        else {
            continue;
        };
        if !process_is_alive(pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Whether `pid` names a live process. Only `ESRCH` is "dead": `EPERM` means it exists.
fn process_is_alive(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 sends nothing; it only checks that the process exists.
    pid > 0
        && (unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH))
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
    use super::{
        absolutize_loader_cache, contents_for_exe, install_loader_cache, load_and_keep,
        loaded_images, loader_binds_staged_librsvg, module_basename, module_file_in_use,
        only_the_staged_librsvg_is_loaded, svg_modules_are_staged, sweep_dead_caches, verified,
    };
    use std::path::Path;
    use std::path::PathBuf;

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

    /// The screen stands down only when the librsvg loaded is the staged one: a bundle
    /// with no librsvg, or whose loader will not load, is screened like a binary outside
    /// any bundle.
    #[test]
    fn only_a_bundle_carrying_its_librsvg_counts_as_verified() {
        let dir = tempfile::tempdir().unwrap();
        let (_, contents) = staged_bundle(dir.path());
        assert!(
            !loader_binds_staged_librsvg(&contents, load_and_keep, loaded_images),
            "no librsvg staged"
        );
        let frameworks = contents.join("Frameworks");
        std::fs::create_dir_all(&frameworks).unwrap();
        std::fs::write(frameworks.join("librsvg-2.2.dylib"), b"").unwrap();
        std::fs::write(frameworks.join("libpixbufloader_svg.so"), b"not a Mach-O").unwrap();
        assert!(
            !loader_binds_staged_librsvg(&contents, load_and_keep, loaded_images),
            "a staged librsvg whose loader does not load is not proof of anything"
        );
        assert!(
            !super::carries_verified_librsvg(),
            "a test binary runs outside any bundle"
        );
    }

    /// The composed decision: it is the image list read AFTER the loader loads that
    /// decides, not the load succeeding. Mutation: returning `true` once `load` succeeds
    /// passes the second and third cases.
    #[test]
    fn a_loader_that_loads_counts_only_if_the_staged_librsvg_is_what_it_bound() {
        let dir = tempfile::tempdir().unwrap();
        let (_, contents) = staged_bundle(dir.path());
        std::fs::create_dir_all(contents.join("Frameworks")).unwrap();
        let staged = contents.join("Frameworks/librsvg-2.2.dylib");
        std::fs::write(&staged, b"").unwrap();
        let homebrew = PathBuf::from("/opt/homebrew/opt/librsvg/lib/librsvg-2.2.dylib");
        let loaded = |_: &Path| true;
        let decide = |load: &dyn Fn(&Path) -> bool, images: Vec<PathBuf>| {
            loader_binds_staged_librsvg(&contents, load, || images)
        };

        assert!(
            decide(&loaded, vec![staged.clone()]),
            "anti-vacuity: the staged copy alone"
        );
        assert!(
            !decide(&loaded, vec![homebrew.clone()]),
            "the loader bound another librsvg"
        );
        assert!(
            !decide(&loaded, vec![]),
            "the loader bound no librsvg at all"
        );
        assert!(!decide(&|_| false, vec![staged]), "the loader did not load");
    }

    /// The decision over dyld's image list: the staged copy alone passes; a librsvg from
    /// anywhere else, alone or beside the staged one, or none at all, fails. Mutation:
    /// keying on presence (`staged.is_file()`) passes the sibling case.
    #[test]
    fn only_the_staged_librsvg_being_loaded_counts() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("S.app/Contents");
        std::fs::create_dir_all(app.join("Frameworks")).unwrap();
        std::fs::create_dir_all(app.join("MacOS")).unwrap();
        let sibling = dir.path().join("librsvg-2.2.dylib");
        std::fs::write(app.join("Frameworks/librsvg-2.2.dylib"), b"").unwrap();
        std::fs::write(&sibling, b"").unwrap();
        let staged = std::fs::canonicalize(app.join("Frameworks/librsvg-2.2.dylib")).unwrap();
        let other = PathBuf::from("/usr/lib/libSystem.B.dylib");
        // Spelled the way dyld reports an @rpath resolution, not canonical.
        let via_rpath = app.join("MacOS/../Frameworks/librsvg-2.2.dylib");

        let ok = only_the_staged_librsvg_is_loaded;
        assert!(ok(&[other.clone(), via_rpath.clone()], &staged));
        assert!(!ok(&[other.clone(), sibling.clone()], &staged));
        assert!(!ok(&[via_rpath, sibling], &staged));
        assert!(!ok(&[other], &staged));
    }

    /// A bundle staging only its SVG loader, and the template cache `bundle.sh` stages
    /// beside it, as `(contents, template)`.
    fn staged_cache(root: &Path) -> (PathBuf, PathBuf) {
        let (_, contents) = staged_bundle(root);
        std::fs::create_dir_all(contents.join("Frameworks")).unwrap();
        std::fs::create_dir_all(contents.join("Resources")).unwrap();
        std::fs::write(contents.join("Frameworks/libpixbufloader_svg.so"), b"").unwrap();
        let template = contents.join("Resources/loaders.cache");
        std::fs::write(
            &template,
            "# GdkPixbuf Image Loader Modules file\n\n\
             \"libpixbufloader-png.so\"\n\
             \"png\" 5 \"gdk-pixbuf\" \"PNG\" \"LGPL\"\n\
             \"image/png\" \"\"\n\"png\" \"\"\n\n\
             \"libpixbufloader_svg.so\"\n\
             \"svg\" 6 \"gdk-pixbuf\" \"Scalable Vector Graphics\" \"LGPL\"\n\
             \"image/svg+xml\" \"image/svg\" \"\"\n\"svg\" \"svgz\" \"\"\n\n",
        )
        .unwrap();
        (contents, template)
    }

    /// The whole decision, over a staged bundle whose loader loads and binds the staged
    /// librsvg: only the module file decides. Mutation: dropping the module-file conjunct
    /// from `verified` passes the first two cases.
    #[test]
    fn the_screen_stays_up_unless_the_bundles_cache_is_the_one_gdk_pixbuf_reads() {
        let dir = tempfile::tempdir().unwrap();
        let (contents, template) = staged_cache(dir.path());
        let frameworks = contents.join("Frameworks");
        let staged = frameworks.join("librsvg-2.2.dylib");
        std::fs::write(&staged, b"").unwrap();
        let decide = |module_file: Option<&Path>| {
            verified(&contents, module_file, |_| true, || vec![staged.clone()])
        };

        // The write fails: no cache, so gdk-pixbuf reads its compiled-in one.
        let unwritable = dir.path().join("no-such-dir");
        let installed = install_loader_cache(&template, &frameworks, &unwritable);
        assert_eq!(installed, None, "a failed write must not report a cache");
        assert!(!decide(None), "no module file of ours");
        // A module file naming another SVG loader.
        let foreign = dir.path().join("foreign.cache");
        std::fs::write(&foreign, std::fs::read_to_string(&template).unwrap()).unwrap();
        assert!(
            !decide(Some(&foreign)),
            "a cache with a bare, unrewritten loader name"
        );

        // ANTI-VACUITY: the cache this process wrote stands the screen down.
        let installed = install_loader_cache(&template, &frameworks, dir.path())
            .expect("a writable directory takes the cache");
        assert!(decide(Some(&installed)));
    }

    /// The check reads the cache this process wrote, and only while the environment still
    /// points gdk-pixbuf at it. Mutation: trusting the inherited variable passes the
    /// second case.
    #[test]
    fn only_the_cache_this_process_wrote_and_still_names_is_checked() {
        let ours = PathBuf::from("/T/scribobulate-loaders-1-0.cache");
        let theirs = std::ffi::OsStr::new("/T/somebody-elses.cache");
        assert_eq!(module_file_in_use(None, Some(ours.as_os_str())), None);
        assert_eq!(module_file_in_use(Some(&ours), Some(theirs)), None);
        assert_eq!(module_file_in_use(Some(&ours), None), None);
        assert_eq!(
            module_file_in_use(Some(&ours), Some(ours.as_os_str())),
            Some(ours.as_path())
        );
    }

    /// Two launches never share a cache file, and neither overwrites the other's.
    /// Mutation: a fixed name makes the second install return the first's path.
    #[test]
    fn every_install_gets_a_file_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let (contents, template) = staged_cache(dir.path());
        let frameworks = contents.join("Frameworks");
        let first = install_loader_cache(&template, &frameworks, dir.path()).unwrap();
        std::fs::write(&first, b"left alone").unwrap();
        let second = install_loader_cache(&template, &frameworks, dir.path()).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first).unwrap(), b"left alone");
    }

    /// A dead process's cache is swept; a live one's, and anything not ours, is kept.
    #[test]
    fn only_the_caches_of_exited_processes_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        let dead_file = dir
            .path()
            .join(format!("scribobulate-loaders-{dead}-0.cache"));
        let live_file = dir.path().join(format!(
            "scribobulate-loaders-{}-0.cache",
            std::process::id()
        ));
        let other = dir.path().join("scribobulate-loaders.cache");
        for f in [&dead_file, &live_file, &other] {
            std::fs::write(f, b"").unwrap();
        }
        sweep_dead_caches(dir.path());
        assert!(!dead_file.exists(), "an exited process's cache");
        assert!(live_file.exists(), "this process's cache");
        assert!(other.exists(), "a name that is not a per-process cache");
    }

    /// Every module claiming SVG must be the staged loader, named absolutely. Mutation:
    /// accepting the first SVG module passes a cache where a second one claims the type.
    #[test]
    fn a_module_file_naming_any_other_svg_loader_is_not_the_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let (contents, template) = staged_cache(dir.path());
        let loader = contents.join("Frameworks/libpixbufloader_svg.so");
        let homebrew = dir.path().join("libpixbufloader_svg.so");
        std::fs::write(&homebrew, b"").unwrap();
        let check = |cache: &str| {
            let f = dir.path().join("cache");
            std::fs::write(&f, cache).unwrap();
            svg_modules_are_staged(Some(&f), &loader)
        };
        let block = |path: &str, format: &str, mime: &str| {
            format!("\"{path}\"\n\"{format}\" 6 \"gdk-pixbuf\" \"X\" \"LGPL\"\n\"{mime}\" \"\"\n\n")
        };
        let ours = block(&loader.to_string_lossy(), "svg", "image/svg+xml");
        let theirs = block(&homebrew.to_string_lossy(), "svg", "image/svg+xml");
        let claimant = block(&homebrew.to_string_lossy(), "rsvg2", "image/svg+xml");
        let png = block(&homebrew.to_string_lossy(), "png", "image/png");

        assert!(check(&format!("{png}{ours}")));
        assert!(!check(&theirs), "Homebrew's SVG loader");
        assert!(
            !check(&format!("{ours}{claimant}")),
            "a second module claiming the MIME type"
        );
        assert!(!check(&png), "no SVG module at all");
        assert!(
            !check(&std::fs::read_to_string(&template).unwrap()),
            "a bare, unrewritten name"
        );
        assert!(!svg_modules_are_staged(
            Some(&dir.path().join("absent")),
            &loader
        ));
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
