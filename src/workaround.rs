/// Returns non-ASCII characters found inside `macro(...)` bindings across all
/// keyd config files in /etc/keyd/. These are the characters the user types via
/// macropad keys and therefore need compose sequences available in GTK's IM context.
pub(crate) fn keyd_macro_chars() -> std::collections::HashSet<String> {
    let mut chars = std::collections::HashSet::new();
    let Ok(dir) = std::fs::read_dir("/etc/keyd") else {
        return chars;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("conf") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        collect_macro_chars(&content, &mut chars);
    }
    chars
}

/// Collect the non-ASCII characters inside `macro(...)` bindings in one keyd
/// config's text into `chars`.  Split out from the directory walk so the
/// parsing logic is unit-testable without `/etc/keyd`.
fn collect_macro_chars(content: &str, chars: &mut std::collections::HashSet<String>) {
    let mut rest = content;
    while let Some(pos) = rest.find("macro(") {
        rest = &rest[pos + 6..];
        let Some(end) = rest.find(')') else {
            break;
        };
        for c in rest[..end].chars() {
            if !c.is_ascii() {
                let mut buf = [0u8; 4];
                chars.insert(c.encode_utf8(&mut buf).to_string());
            }
        }
        rest = &rest[end + 1..];
    }
}

/// Reads `xcompose_path` and returns the lines that produce one of the
/// characters in `needed`. Safe to call with the full keyd compose file —
/// it is scanned as plain text, never parsed into a compose table.
pub(crate) fn keyd_compose_lines(
    xcompose_path: &std::path::Path,
    needed: &std::collections::HashSet<String>,
) -> Vec<String> {
    if needed.is_empty() {
        return Vec::new();
    }
    let Ok(content) = std::fs::read_to_string(xcompose_path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in content.lines() {
        let Some(colon) = line.rfind(':') else {
            continue;
        };
        let rhs = line[colon + 1..].trim();
        if !rhs.starts_with('"') {
            continue;
        }
        let after_open = &rhs[1..];
        let Some(close) = after_open.find('"') else {
            continue;
        };
        if needed.contains(&after_open[..close]) {
            out.push(line.to_string());
        }
    }
    out
}

/// Create a fresh, private (mode 0700) directory to hold the redirected GTK config,
/// and return its path.
///
/// Security note (see sdd/ANTI-PATTERNS.md): the original implementation reused a
/// fixed, predictable path under the shared `std::env::temp_dir()` (`/tmp` on most
/// systems) and only symlinked real config files in when the destination didn't
/// already exist. A local attacker (any user on the box, or same-user malware) could
/// pre-create `.../gtk-4.0/settings.ini` with a malicious `gtk-modules` entry ahead
/// of time; because it "already existed", the real settings were never symlinked
/// over it and the planted file loaded — local code execution via `dlopen()`.
///
/// This version prefers `$XDG_RUNTIME_DIR` (per-user, mode 0700, torn down at
/// logout — not world-writable, so other local users can't plant files there) and
/// falls back to the shared temp dir only if `XDG_RUNTIME_DIR` is unset. In both
/// cases the directory name includes the PID and a nanosecond timestamp and is
/// created with `DirBuilder::mode(0o700)` + no-clobber semantics (fails with
/// `AlreadyExists` rather than reusing a pre-existing directory), so nothing can be
/// planted before the directory exists, and no pre-existing directory is ever
/// trusted. Never reused across invocations — each run gets its own directory, so
/// a previous run's symlinks/permissions can't be inherited either.
fn create_unique_secure_dir() -> Option<std::path::PathBuf> {
    use std::os::unix::fs::DirBuilderExt;

    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let pid = std::process::id();

    for attempt in 0..8u32 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let candidate = base.join(format!("scribobulate-gtk-config-{pid}-{nanos}-{attempt}"));
        match std::fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => return Some(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// The `mimeapps.list` filenames GIO consults under the user config dir, in GIO's own
/// precedence order: one `<desktop>-mimeapps.list` per colon-separated entry of
/// `XDG_CURRENT_DESKTOP` (lowercased — `KDE:X-Cinnamon` → `kde-`, `x-cinnamon-`),
/// then the unprefixed `mimeapps.list`.
///
/// Split out (and derived from the environment rather than hardcoded) so the
/// XDG_CONFIG_HOME redirect below can re-expose exactly the set GIO would have read,
/// and so the precedence rule is unit-testable without touching `$HOME`. Mirrors
/// `get_lowercase_current_desktops()` + the `desktop_file_dir_unindexed_init` scan in
/// `gdesktopappinfo.c` (glib 2.72.4).
fn mimeapps_list_names_for(current_desktop: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = current_desktop
        .unwrap_or("")
        .split(':')
        .filter(|d| is_valid_xdg_desktop(d))
        .map(|d| format!("{}-mimeapps.list", d.to_lowercase()))
        .collect();
    names.push("mimeapps.list".to_string());
    names
}

/// Mirror of GIO's `validate_xdg_desktop` (`gdesktopappinfo.c:315-328`): an
/// `XDG_CURRENT_DESKTOP` entry is valid only if it is non-empty and contains nothing
/// but ASCII alphanumerics, `-`, or `_`. GIO **drops** invalid entries, so it never
/// probes a `<entry>-mimeapps.list` for them — matching that exactly (rather than
/// merely splitting on `:`) keeps this a mirror of the consumer's lookup instead of an
/// approximation of it, which is the whole point of deriving the set at all.
fn is_valid_xdg_desktop(desktop: &str) -> bool {
    !desktop.is_empty()
        && desktop
            .bytes()
            .all(|b| b == b'-' || b == b'_' || b.is_ascii_alphanumeric())
}

/// [`mimeapps_list_names_for`] against the live `XDG_CURRENT_DESKTOP`.
fn mimeapps_list_names() -> Vec<String> {
    mimeapps_list_names_for(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref())
}

/// GTK 4.6 crashes when GtkIMContextSimple tries to parse a large XCompose file
/// (gtkcomposetable.c:981 assertion). Fixed in GTK 4.12.
///
/// GtkIMContextSimple reads compose sequences from the first of:
///   1. $XDG_CONFIG_HOME/gtk-4.0/Compose
///   2. ~/.XCompose
///   3. /usr/share/X11/locale/$locale/Compose
///
/// Workaround: redirect XDG_CONFIG_HOME for this process only to a temp
/// directory containing a small Compose file (system locale + any keyd macro
/// sequences found), with symlinks to all other real GTK4 config files so user
/// theme/settings survive. No-op when ~/.XCompose is absent or ≤ 8 KB.
/// See GTK4Rs/AP-3.
pub(crate) fn workaround_gtk46_compose_crash() {
    let minor = unsafe { gtk::ffi::gtk_get_minor_version() };
    if minor >= 12 {
        return;
    }

    let xdg_config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")));
    let Some(real_config) = xdg_config else {
        return;
    };
    let real_gtk4 = real_config.join("gtk-4.0");

    // Only needed if ~/.XCompose (or equiv) exists and is large enough to crash GTK 4.6.
    let compose_src = std::env::var_os("XCOMPOSEFILE")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".XCompose"))
        });
    let large_compose = compose_src
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .is_some_and(|m| m.len() > 8_192);
    if !large_compose {
        return;
    }

    let Some(tmp) = create_unique_secure_dir() else {
        return;
    };
    let tmp_gtk4 = tmp.join("gtk-4.0");
    if std::fs::create_dir(&tmp_gtk4).is_err() {
        return;
    }

    // Symlink every real GTK4 config file/dir except Compose into our temp dir.
    // `tmp` was just created exclusively above, so every destination is guaranteed
    // fresh — no `dest.exists()` guard needed (and none should be trusted: see
    // create_unique_secure_dir's doc comment for why).
    if let Ok(entries) = std::fs::read_dir(&real_gtk4) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name == "Compose" {
                continue;
            }
            let dest = tmp_gtk4.join(&name);
            let _ = std::os::unix::fs::symlink(entry.path(), &dest);
        }
    }

    // mimeapps.list lives at $XDG_CONFIG_HOME/mimeapps.list (not inside gtk-4.0/).
    // Without these symlinks, GIO MIME lookups bypass the user's default-app config
    // and fall through to system defaults (wrong browser, wrong PDF viewer, etc.) —
    // GIO's HIGHEST-priority association source is the user config dir, which this
    // function has just redirected out from under it (`gdesktopappinfo.c:1587-88`).
    //
    // Plain `mimeapps.list` is NOT enough: GIO probes the desktop-specific
    // `<desktop>-mimeapps.list` FIRST, once per entry in `XDG_CURRENT_DESKTOP`
    // (lowercased), before falling back to the unprefixed file
    // (`gdesktopappinfo.c:961-970`, researcher-sourced against glib 2.72.4). On a KDE
    // session that means `kde-mimeapps.list` outranks `mimeapps.list` — so linking
    // only the latter would still silently lose the user's real default for any type
    // KDE recorded in its own file. Mirror GIO's own lookup rather than hardcoding
    // "kde": derive the prefixes from the live `XDG_CURRENT_DESKTOP`.
    // ⚠️ These are FILE symlinks, and that is only safe because this app **never writes
    // app associations** — it has no `set_as_default_for_type`/`set_as_last_used_for_type`
    // call anywhere (verified; keep it that way, or fix this first). An atomic-rename
    // writer DEFEATS a file symlink by design: `rename(2)` replaces the **link itself**,
    // not its target, so the first write would swap this symlink for a real file inside
    // the temp dir and the user's default-app change would vanish on exit — silently.
    // The usual remedy (symlink the DIRECTORY, as done for `gtk-3.0` below) is NOT
    // available here: these files sit at the config ROOT, and symlinking `real_config`
    // itself would re-expose the real `gtk-4.0/Compose` and defeat this entire
    // workaround. So the invariant is pinned here instead of engineered away.
    for name in mimeapps_list_names() {
        let src = real_config.join(&name);
        if src.exists() {
            let _ = std::os::unix::fs::symlink(&src, tmp.join(&name));
        }
    }

    // GtkFileChooser's sidebar bookmarks live at `$XDG_CONFIG_HOME/`**`gtk-3.0`**`/bookmarks`
    // — GTK 4 deliberately still reads the GTK-3 path because the file format never
    // changed (`gtkbookmarksmanager.c:80`, with a comment saying exactly that). The
    // `gtk-4.0` symlinks above therefore CANNOT cover it: it is a different top-level
    // directory. Without this, a redirected process shows an empty bookmarks sidebar and
    // — worse — writes any bookmark the user adds into the temp dir, so it is silently
    // lost when the process exits. Symlink the DIRECTORY, not the file: GTK rewrites
    // `bookmarks` via write-temp-then-rename, which would replace a file symlink with a
    // real file in the temp dir (losing the write); with the directory linked, the rename
    // resolves inside the user's real `gtk-3.0/` and the bookmark persists.
    let src_gtk3 = real_config.join("gtk-3.0");
    if src_gtk3.is_dir() {
        let _ = std::os::unix::fs::symlink(&src_gtk3, tmp.join("gtk-3.0"));
    }

    // Build a compose file: system locale sequences + the specific sequences
    // needed for keyd macro() bindings (Cancel-prefixed, safe for GTK 4.6).
    let mut compose_content = String::from("include \"%L\"\n");
    if let Some(ref src) = compose_src {
        let chars = keyd_macro_chars();
        let lines = keyd_compose_lines(src, &chars);
        for line in &lines {
            compose_content.push_str(line);
            compose_content.push('\n');
        }
    }

    if std::fs::write(tmp_gtk4.join("Compose"), &compose_content).is_err() {
        return;
    }

    std::env::set_var("XDG_CONFIG_HOME", &tmp);
    eprintln!(
        "GTK 4.{minor}: per-process XDG_CONFIG_HOME set to prevent compose-table \
         crash from large ~/.XCompose. Upgrade to GTK ≥ 4.12 to remove this \
         workaround."
    );
}

#[cfg(test)]
mod tests {
    use super::{collect_macro_chars, keyd_compose_lines, mimeapps_list_names_for};

    #[test]
    fn mimeapps_names_put_the_desktop_specific_file_first() {
        // GIO probes `<desktop>-mimeapps.list` BEFORE the unprefixed file, so the
        // symlink set must cover it or a KDE user's real default is silently lost.
        assert_eq!(
            mimeapps_list_names_for(Some("KDE")),
            vec!["kde-mimeapps.list", "mimeapps.list"]
        );
    }

    #[test]
    fn mimeapps_names_handle_multiple_lowercased_desktops_in_order() {
        assert_eq!(
            mimeapps_list_names_for(Some("KDE:X-Cinnamon")),
            vec![
                "kde-mimeapps.list",
                "x-cinnamon-mimeapps.list",
                "mimeapps.list"
            ]
        );
    }

    #[test]
    fn mimeapps_names_fall_back_to_the_bare_file_with_no_desktop_set() {
        assert_eq!(mimeapps_list_names_for(None), vec!["mimeapps.list"]);
        assert_eq!(mimeapps_list_names_for(Some("")), vec!["mimeapps.list"]);
        // A stray separator must not produce a bare "-mimeapps.list".
        assert_eq!(
            mimeapps_list_names_for(Some("KDE::")),
            vec!["kde-mimeapps.list", "mimeapps.list"]
        );
    }

    #[test]
    fn mimeapps_names_drop_entries_gio_would_reject() {
        // GIO's validate_xdg_desktop rejects anything outside [A-Za-z0-9_-], so it never
        // probes a file for such an entry — mirror that instead of over-approximating.
        assert_eq!(
            mimeapps_list_names_for(Some("KDE:not valid:GNOME")),
            vec!["kde-mimeapps.list", "gnome-mimeapps.list", "mimeapps.list"]
        );
        assert_eq!(
            mimeapps_list_names_for(Some("has.dot")),
            vec!["mimeapps.list"]
        );
        // Hyphen and underscore ARE valid.
        assert_eq!(
            mimeapps_list_names_for(Some("X-Cinnamon:a_b")),
            vec![
                "x-cinnamon-mimeapps.list",
                "a_b-mimeapps.list",
                "mimeapps.list"
            ]
        );
    }
    use std::collections::HashSet;
    use std::io::Write;

    #[test]
    fn collect_macro_chars_extracts_non_ascii_only() {
        let mut chars = HashSet::new();
        collect_macro_chars("key = macro(a é b ü)\nother = macro(z)", &mut chars);
        assert!(chars.contains("é"));
        assert!(chars.contains("ü"));
        assert!(!chars.contains("a")); // ASCII ignored
        assert_eq!(chars.len(), 2);
    }

    #[test]
    fn collect_macro_chars_handles_no_macros_and_unclosed() {
        let mut chars = HashSet::new();
        collect_macro_chars("nothing here", &mut chars);
        assert!(chars.is_empty());
        // An unclosed macro( must not panic and yields nothing.
        collect_macro_chars("broken = macro(é", &mut chars);
        assert!(chars.is_empty());
    }

    #[test]
    fn keyd_compose_lines_selects_only_needed_sequences() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "<Multi_key> <a> : \"é\"   eacute").unwrap();
        writeln!(file, "<Multi_key> <b> : \"ü\"   udiaeresis").unwrap();
        writeln!(file, "# a comment line").unwrap();
        writeln!(file, "<Multi_key> <c> : \"ñ\"   ntilde").unwrap();

        let needed: HashSet<String> = ["é".to_string(), "ñ".to_string()].into_iter().collect();
        let lines = keyd_compose_lines(file.path(), &needed);

        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.contains("\"é\"")));
        assert!(lines.iter().any(|l| l.contains("\"ñ\"")));
        assert!(!lines.iter().any(|l| l.contains("\"ü\""))); // not requested
    }

    #[test]
    fn keyd_compose_lines_empty_when_nothing_needed() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let lines = keyd_compose_lines(file.path(), &HashSet::new());
        assert!(lines.is_empty());
    }

    #[test]
    fn keyd_compose_lines_skips_missing_file_and_malformed_lines() {
        let needed: HashSet<String> = ["é".to_string()].into_iter().collect();

        // A non-existent path returns empty even though `needed` is non-empty.
        let missing = std::path::Path::new("/nonexistent/scribobulate/xcompose");
        assert!(keyd_compose_lines(missing, &needed).is_empty());

        // Lines with no colon, an unquoted right-hand side, or an unterminated
        // quote are all skipped; only the well-formed matching line survives.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "a line with no colon").unwrap();
        writeln!(file, "<Multi_key> <a> : eacute").unwrap(); // rhs not quoted
        writeln!(file, "<Multi_key> <b> : \"é   unterminated").unwrap(); // no closing quote
        writeln!(file, "<Multi_key> <c> : \"é\"   eacute").unwrap(); // valid match
        let lines = keyd_compose_lines(file.path(), &needed);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("<c>"));
    }
}

#[cfg(test)]
mod xdg_redirect_tests {
    /// The redirect must move `XDG_CONFIG_HOME` and NOTHING ELSE.
    ///
    /// This is a two-sided guard, and both sides are easy to break with an innocent
    /// refactor that looks like a cleanup:
    ///
    ///  * If a GLib config-dir read (`glib::user_config_dir()`) is ever introduced
    ///    BEFORE the redirect — ours or a dependency's — GLib caches the real dir in
    ///    a global static forever, GTK 4.6's compose table reads that same global,
    ///    and the crash this workaround prevents comes back silently.
    ///  * If the redirect is ever widened to touch `XDG_DATA_HOME`/`XDG_DATA_DIRS`,
    ///    the themes search path (which deliberately uses the GLib data-dir helpers,
    ///    because those ARE order-independent) would resolve into a throwaway temp
    ///    dir and every installed theme would vanish with no error.
    ///
    /// Both failures are silent, which is exactly why they get a test.
    #[test]
    fn the_redirect_touches_config_home_only() {
        // The data dirs the themes search path reads must be untouched by name.
        let src = include_str!("workaround.rs");
        let sets: Vec<&str> = src
            .lines()
            .filter(|l| l.contains("set_var("))
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect();
        for line in &sets {
            assert!(
                !line.contains("XDG_DATA_HOME") && !line.contains("XDG_DATA_DIRS"),
                "the redirect must not touch the data dirs the theme search path uses: {line}"
            );
        }
        assert!(
            sets.iter().any(|l| l.contains("XDG_CONFIG_HOME")),
            "the workaround is supposed to redirect XDG_CONFIG_HOME"
        );
    }

    // The "config path must never be resolved through GLib" invariant is now
    // enforced crate-wide by `clippy.toml` (a `disallowed-methods` ban on
    // `glib::user_config_dir`), replacing the former hardcoded-4-file self-scanning
    // test — which was itself the ScrAP-132 scope-gap species (a "crate-wide" guard
    // that only scanned a fixed file list). Use `config::user_config_dir`, which
    // snapshots `std::env` before the `XDG_CONFIG_HOME` redirect.
}
