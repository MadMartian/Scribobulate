//! Build script: emit `rustc-env` variables for key dependency versions so the
//! About dialog can display them without hardcoding.  Reads `Cargo.lock` (which
//! is committed and always present) and re-runs only when it changes.  Also
//! compiles `data/resources.gresource.xml` (the bundled icons — see that file's
//! own header comment) into a GResource blob embedded
//! in the binary via `gio::resources_register_include!` at startup.  When the
//! TARGET is Windows it additionally embeds a Win32 resource section so the shell
//! can identify the executable — see `embed_windows_resources` below.

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");

    let lock = std::fs::read_to_string("Cargo.lock").expect("Cargo.lock not found");

    for (var, pkg) in &[
        ("SCRIB_GTK4_CRATE_VERSION", "gtk4"),
        ("SCRIB_SOURCEVIEW_CRATE_VERSION", "sourceview5"),
        ("SCRIB_PULLDOWN_CRATE_VERSION", "pulldown-cmark"),
    ] {
        if let Some(ver) = lock_version(&lock, pkg) {
            println!("cargo:rustc-env={var}={ver}");
        }
    }

    emit_git_commit();

    // One source dir: `data/icons` is a freedesktop icon tree holding every
    // bundled icon, the application icon included. Its layout mirrors the
    // install target exactly (`scalable/apps/…` here becomes
    // `hicolor/scalable/apps/…` under install.sh), so the same file serves the
    // GResource and the system install with no alias and no second copy.
    glib_build_tools::compile_resources(
        &["data/icons"],
        "data/resources.gresource.xml",
        "scribobulate.gresource",
    );

    // Gated on the TARGET, not the host. A build script is compiled for the host, so
    // `#[cfg(windows)]` here asks "am I running on Windows?" — which is the wrong
    // question in both directions: a Windows host cross-compiling elsewhere would
    // embed a Win32 section into a non-Windows binary, and a non-Windows host
    // targeting Windows would skip it silently, which is precisely the unbranded-exe
    // outcome `res.compile()`'s `expect` below exists to make impossible.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_resources();
    }
}

/// Emit `SCRIB_GIT_COMMIT` — the source revision a crash report is tied back to.
///
/// Deliberately the abbreviated hash and nothing else. A "-dirty" marker would need
/// `git status` over the whole tree on every build, and to be correct it would need
/// `rerun-if-changed` on every file — trading a real rebuild cost for a flag that
/// the crash report's *executable* fingerprint (size + mtime, see `identity.rs`)
/// already supplies in a form that cannot go stale.
///
/// Falls back to `unknown` rather than failing: the crate must build from a release
/// tarball, from a vendored copy, and inside a container with no `git` on PATH.
fn emit_git_commit() {
    // MEASURED (QA round 5, L-3): `.git/HEAD` alone is NOT enough, and the comment
    // that used to sit here claiming it "covers a commit landing on the current
    // branch" was wrong. On a branch, `.git/HEAD` holds `ref: refs/heads/<branch>` and
    // a new commit does not touch it — only the ref it points at moves. So the hash
    // baked into the binary went stale the moment anyone committed without switching
    // branches: the debug build reported a commit three behind HEAD, in the constant
    // whose entire job is tying a crash report back to a revision.
    //
    // `.git/logs/HEAD` is the reflog, appended on every HEAD movement — commit,
    // checkout, reset, merge — so it moves in exactly the cases a checkout-only watch
    // misses. Both are watched: `.git/HEAD` still catches a branch switch on a repo
    // with reflogs disabled, and a packed-refs-only repository re-resolves on the next
    // `cargo build` anyway.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=SCRIB_GIT_COMMIT={commit}");
}

/// Embed the Win32 resource section (icon + VERSIONINFO) into `scribobulate.exe`.
///
/// The Windows shell identifies a program from the file on disk, never by asking
/// the running process — so everything GTK sets at runtime (`icon_name`, the
/// window title) is invisible to it.  Without this section the shell has only the
/// file name to work with, which is why "Open with", Settings ▸ Default apps,
/// Task Manager and Properties ▸ Details all showed a bare `scribobulate.exe`
/// beside a generic icon.
///
/// Two resources carry the whole fix:
///
/// * **`RT_GROUP_ICON`** — the shell draws icon index 0 of the executable named
///   by a ProgID's `shell\open\command`.  It does *not* fall back to that
///   ProgID's `DefaultIcon`; that key is the *document* icon, a different thing.
/// * **`FileDescription`** — the friendly name.  The shell's lookup order is
///   ProgID `FriendlyAppName` → the executable's `FileDescription` → the file
///   name, and the installer now also sets the first (see `scribobulate.iss`),
///   so the name is right whether the app was installed or is being run straight
///   out of `target\release`.
///
/// `FileDescription` is set explicitly because `winresource` otherwise defaults
/// it to `CARGO_PKG_DESCRIPTION` — a full sentence that names Linux, which the
/// shell would then use verbatim as the app's name in every one of those menus.
/// Targeting Windows from a host that cannot do it. `winresource` is a
/// **host**-gated build dependency (`Cargo.toml`, `[target.'cfg(windows)'
/// .build-dependencies]`) because it needs the Windows SDK's `rc.exe`, so it is not
/// even linked here — there is nothing to call. Failing loudly is the whole point:
/// a binary with no resource section runs perfectly and shows a generic icon and a
/// bare `scribobulate.exe` in every shell surface, so nothing downstream would catch
/// it. The documented pipeline is a native MSVC build (packaging/windows/README.md).
#[cfg(not(windows))]
fn embed_windows_resources() -> ! {
    panic!(
        "cross-compiling to Windows from a non-Windows host: the Win32 resource \
         section (icon + VERSIONINFO) cannot be embedded, because `winresource` is a \
         host-gated build dependency needing the Windows SDK's rc.exe. Build on \
         Windows — see packaging/windows/README.md."
    );
}

#[cfg(windows)]
fn embed_windows_resources() {
    // The same art as the GResource app icon and install.sh's hicolor drop, but
    // pre-rendered: `rc.exe` takes .ico only, and PE icons are raster by nature.
    println!("cargo:rerun-if-changed=packaging/windows/scribobulate.ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon("packaging/windows/scribobulate.ico");
    res.set("FileDescription", "Scribobulate");
    res.set("ProductName", "Scribobulate");
    res.set("CompanyName", "Extollit");
    res.set("InternalName", "scribobulate");
    res.set("OriginalFilename", "scribobulate.exe");
    // Fail the build rather than ship a silently unbranded binary: a missing
    // resource section produces a perfectly working exe, so nothing downstream
    // would catch it until a user opened a context menu.
    res.compile().expect(
        "failed to embed the Windows resource section (is the Windows SDK's rc.exe on PATH?)",
    );
}

/// Return the first `version` value for `package_name` in a Cargo.lock file.
/// Parses the `[[package]]` / `name` / `version` block structure line-by-line
/// without pulling in a TOML parser.
fn lock_version(lock: &str, package_name: &str) -> Option<String> {
    let mut in_block = false;
    let mut matched_name = false;

    for line in lock.lines() {
        if line == "[[package]]" {
            in_block = true;
            matched_name = false;
            continue;
        }
        if !in_block {
            continue;
        }
        // A blank line ends the current [[package]] block.
        if line.is_empty() {
            in_block = false;
            matched_name = false;
            continue;
        }
        if let Some(rest) = line.strip_prefix("name = ") {
            matched_name = rest.trim_matches('"') == package_name;
            continue;
        }
        if matched_name {
            if let Some(rest) = line.strip_prefix("version = ") {
                return Some(rest.trim_matches('"').to_string());
            }
        }
    }
    None
}
