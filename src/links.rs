//! Link, anchor, and resource-target handling for rendered Markdown: which URL
//! schemes are safe to launch, opening them, image-path containment, and the
//! GitHub-style heading-anchor slugs that drive same-document `#fragment` links.
//! Extracted from `renderer.rs` to keep that module focused on AST → buffer
//! rendering.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── heading anchor slugs (GitHub-style) ────────────────────────────────────────

/// Convert heading text to a GitHub-style anchor slug: lowercase, then keep only
/// letters, numbers, `_` and `-`, with spaces turned into `-`.  Matches the
/// github-slugger algorithm (lowercase → strip everything else → spaces to
/// hyphens); no hyphen/space collapsing or trimming.  `char::is_alphanumeric` is
/// Unicode-aware so non-ASCII headings slug like GitHub's (precomposed forms).
pub(crate) fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == ' ' {
            out.push('-');
        } else if ch == '-' || ch == '_' || ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        }
        // everything else (punctuation, symbols) is dropped
    }
    out
}

/// Disambiguate duplicate slugs the github-slugger way: the first occurrence
/// keeps the bare slug; later collisions get `-1`, `-2`, … (1-based, document
/// order).  Each produced slug is also recorded, so a literal heading that
/// already produced e.g. `notes-1` doesn't collide with a later duplicate.
pub(crate) fn unique_slug(base: &str, seen: &mut HashMap<String, u32>) -> String {
    let mut result = base.to_string();
    while seen.contains_key(&result) {
        let count = seen.entry(base.to_string()).or_insert(0);
        *count += 1;
        result = format!("{base}-{count}");
    }
    seen.insert(result.clone(), 0);
    result
}

/// Normalise a link target into a candidate anchor slug: a leading `#` is
/// stripped and the remainder percent-decoded (anchors with non-ASCII text may
/// arrive percent-encoded).  The result is compared *literally* against computed
/// heading slugs — it is NOT re-slugged (that would corrupt kept characters).
/// Returns `None` if the URL is not a bare same-document fragment.
pub(crate) fn anchor_target(url: &str) -> Option<String> {
    let frag = url.strip_prefix('#')?;
    Some(percent_decode(frag))
}

/// Extract and decode the `#fragment` suffix of a schemeless cross-document
/// link `href` (e.g. `TECH.md#module-map` → `Some("module-map")`), for the
/// target document's `heading_map` lookup once it is open
/// (`window::linknav::open_doc_link_target`). Returns `None` when `href`
/// carries no fragment at all, or an empty one (`TECH.md#`).
///
/// Shares [`percent_decode`] with [`anchor_target`] so a cross-document
/// fragment decodes exactly like a same-document one — the two must never
/// disagree about what a fragment slug is (same drift class the shared
/// `local_candidate` prevents between the link and image resolvers).
pub(crate) fn doc_link_fragment(href: &str) -> Option<String> {
    let (_, frag) = href.split_once('#')?;
    if frag.is_empty() {
        return None;
    }
    Some(percent_decode(frag))
}

/// Minimal percent-decoder (`%XX` → byte, UTF-8 reassembled; invalid sequences
/// pass through literally).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ── external link opening ──────────────────────────────────────────────────────

/// Schemes a document link is allowed to open via the default handler.  A
/// Markdown document is untrusted content (TDD 2.7): without this gate a link
/// like `file:///etc/passwd`, `smb://…`, or a desktop-action URI would be handed
/// straight to `launch_default_for_uri`.  Restrict to the web/mail schemes a
/// reader actually expects.  Scheme match is case-insensitive (RFC 3986 §3.1).
pub(crate) fn is_allowed_url(url: &str) -> bool {
    match scheme_of(url) {
        Some(scheme) => {
            matches!(
                scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "mailto"
            )
        }
        // No scheme (e.g. a bare relative anchor) — nothing safe to launch.
        None => false,
    }
}

/// The URL scheme of `url` when it is a genuine URL reference, or `None` when `url`
/// is really a **local filesystem path** (which must never be scheme-sniffed). Shared
/// by [`is_allowed_url`], [`resolve_image`], and the doc-link gate so "does this
/// reference have a scheme" is answered identically everywhere — the external-launch
/// gate and the internal-navigation gate must never disagree about it (exactly this
/// kind of drift — one gate silently being asked a question meant for the other — was
/// the root cause of a real bug).
///
/// A colon alone does NOT make a scheme. A naive "text before the first `:`" test
/// misclassifies every local path that happens to contain a colon as a URL and refuses
/// it: a Windows drive path (`C:\dir\img.png` → bogus scheme `"C"`), a relative path
/// with a colon in a segment (`assets/notes:v2.png`), or a same-directory file whose
/// name contains a colon (`report:draft.md`) — all paths, none URLs.
///
/// So a scheme is recognised per RFC 3986 §3.1 — `ALPHA *( ALPHA / DIGIT / "+" / "-" /
/// "." )` immediately followed by `:` — AND accepted only when it is either
/// **hierarchical** (`scheme://…`, e.g. `http`/`https`/`file`/`smb`) or the single
/// bare-colon scheme this app actually launches, `mailto:`. Every other bare-colon
/// form returns `None` and is treated as a local reference:
///  • a path with a colon (`report:draft.md`, `C:\…`) then resolves + containment-checks
///    as the local file it is — the bug this fixes;
///  • an unhandled dangerous bare-colon scheme (`javascript:`, `data:`) is *still* never
///    launched — with no `//` it is not a hierarchical URL, is not `mailto`, and so
///    falls through to local resolution where it fails to resolve and is rendered inert.
///    Security is preserved by fall-through, not by naming each dangerous scheme.
pub(crate) fn scheme_of(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once(':')?;
    // RFC 3986 scheme token: non-empty, first char ALPHA, remainder ALPHA/DIGIT/+/-/.
    let mut chars = scheme.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !first_ok || !rest_ok {
        return None;
    }
    // Accept only a hierarchical `scheme://…` or the bare-colon `mailto:` we launch.
    if rest.starts_with("//") || scheme.eq_ignore_ascii_case("mailto") {
        Some(scheme)
    } else {
        None
    }
}

/// Open `url` in the user's default handler, after the [`is_allowed_url`] gate.
///
/// Goes through `gtk_show_uri_full` rather than `g_app_info_launch_default_for_uri`
/// **because only the former supplies a `GdkAppLaunchContext`** — and it is that
/// context's `get_startup_notify_id` vfunc that emits `DESKTOP_STARTUP_ID`. Called
/// with a NULL context (as this did), GIO delivers the URI perfectly but with **no
/// activation token**, so a window manager's focus-stealing prevention has nothing to
/// justify raising an already-running browser: the tab opens silently *behind* us.
/// Researcher-confirmed against GTK 4.6.9 (`gtkshow.c:120-121`,
/// `gdkapplaunchcontext-x11.c:362-368`) and measured — the same call with a context
/// yields a real `..._TIME<n>` token where the bare GIO call yields none.
///
/// **`timestamp` 0 is deliberate and correct — do NOT "fix" it by threading the click's
/// event time through.** On the *launch* path GDK substitutes 0 (`GDK_CURRENT_TIME`)
/// with `gdk_x11_display_get_user_time()` — the last **user-interaction** time, which
/// the click that got us here already seeded via `XI_ButtonPress`
/// (`gdkapplaunchcontext-x11.c:346-348`). This is the opposite of GTK4Rs/AP-62,
/// whose *focus* path (`gdksurface-x11.c:2220`) passes the timestamp **raw** with no
/// substitution — hence 0 failing there. Different code paths; GTK4Rs/AP-62 does not apply here.
///
/// **Parent is deliberately `None`.** It is not needed for the token (the launch
/// context comes from `gdk_display_get_default()` when parent is NULL —
/// `gtkshow.c:118`), and passing one would call `gtk_window_export_handle`, whose
/// unexport half has no X11 branch on 4.6 — logging
/// `Couldn't unexport handle for GdkX11Toplevel surface` on **every link click**, on
/// real X11, not just headless (fixed upstream in 4.8.0 by a1d03e69a4, never
/// backported to 4.6). The only thing a parent buys is `PARENT_WINDOW_ID`, which
/// parents a *portal* chooser dialog — and no chooser appears when a default handler
/// is set, which is the case a link click is.
///
/// `show_uri_full` (not the simpler `show_uri`) because `show_uri` swallows failure
/// into its own "Could not show link" modal; we want it in the log, not in the user's
/// face. It is NOT deprecated on our target — the deprecation is `v4_10`-gated and we
/// do not enable that feature (GTK4Rs/AP-114's version trap).
pub(crate) fn open_url(url: &str) {
    if !is_allowed_url(url) {
        log::warn!("refusing to open URL with disallowed scheme: {url}");
        return;
    }
    let url_owned = url.to_string();
    gtk::show_uri_full(
        None::<&gtk::Window>,
        url,
        0,
        None::<&gtk::gio::Cancellable>,
        move |res| {
            if let Err(e) = res {
                log::warn!("failed to open URL {url_owned}: {e}");
            }
        },
    );
}

/// Resolve an image `src` to the absolute path to load, **only if it is contained
/// within the document's directory tree** — the security gate for untrusted documents
/// (VULN-002 / TDD 2.7).  Returns `None` (→ render blank) otherwise.
///
/// `src` is resolved against `doc_dir` (a *relative* src is joined to it; an
/// *absolute* src is taken as-is), then **canonicalized** — which resolves `..` AND
/// symbolic links — and admitted only if the canonical target is at or beneath the
/// canonical `doc_dir`.  Canonicalize (not a lexical normalize) is load-bearing: it
/// is the only check that catches a symlink *under* the doc dir whose target escapes
/// it (researcher-sourced; see ANTI-PATTERNS).  A missing file canonicalizes to an
/// error → `None`; an untitled buffer (no `doc_dir`) resolves nothing.
///
/// `Path::starts_with` is component-wise on purpose — a string prefix would wrongly
/// admit `/home/u/doc-evil` as under `/home/u/doc`.  Relative joins go through
/// `doc_dir.join(src)` *before* canonicalize: canonicalizing a bare relative path
/// resolves against the process CWD (the original blank-image bug).
/// Expand a leading `~` against the user's home directory.
///
/// Only a bare `~` or a `~/…` prefix expands. `~user/…` is deliberately NOT
/// supported — guessing another user's home is a resolution this app has no
/// business making. A `~` anywhere but position 0 stays a literal path
/// component, so `./~/x.md` and a file literally named `~` behave as before.
///
/// **This must be applied to the RAW href, before any `is_absolute()`/join
/// decision.** `~/notes.md` expands to an absolute path *outside* the document's
/// folder, so expanding first is exactly what lets the containment gate see it
/// for what it is and refuse it unless the tab's opt-in is on. Expanding after
/// canonicalize — or special-casing `~` past the gate — would turn the tilde
/// into a way around containment.
fn expand_tilde(src: &str) -> PathBuf {
    if src == "~" {
        return glib::home_dir();
    }
    match src.strip_prefix("~/") {
        Some(rest) => glib::home_dir().join(rest),
        None => PathBuf::from(src),
    }
}

/// The single place a local (schemeless) path reference becomes a filesystem
/// candidate: tilde-expand, then resolve a still-relative path against
/// `doc_dir`. Shared by the link and image resolvers so the two content types
/// can never drift apart about what a path *means* — fixing one resolver alone
/// is what left `![](~/pic.png)` broken while `[x](~/notes.md)` worked.
///
/// Returns `None` only when a relative path has no `doc_dir` to resolve against
/// (an untitled buffer); a bare relative path handed to `canonicalize` would
/// otherwise resolve against the process CWD, which is never what we want.
fn local_candidate(src: &str, doc_dir: Option<&Path>) -> Option<PathBuf> {
    let expanded = expand_tilde(src);
    if expanded.is_absolute() {
        Some(expanded)
    } else {
        Some(doc_dir?.join(expanded))
    }
}

/// **Do not "fix" the case-sensitivity here.** `Path::starts_with` compares
/// case-*sensitively* while NTFS is case-*insensitive*, which reads as a Windows bug
/// that would refuse legitimate paths — and it is not one, because `canonicalize`
/// runs on **both** sides first and normalises each to the true on-disk casing. A
/// `doc_dir` handed in fully uppercased still admits its own contained files.
/// Lower-casing either side to "fix" this would replace a verified-correct comparison
/// with one that can be defeated by a crafted name, and no test would notice.
///
/// The whole adversarial set was measured on Win11/NTFS (17 forms, all correct), and
/// three results are worth knowing before changing anything here:
/// - **8.3 short names are neither a bypass nor a false negative.**
///   `dunce::canonicalize` expands them, so a short-name form of a *contained* file
///   is admitted and of an *outside* file is refused, both correctly.
/// - **The UNC admin-share form of a contained file is REFUSED** (`\\localhost\C$\…`),
///   even though it denotes the same file. That is fail-closed and intended, not a
///   gap to close.
/// - **Trailing-dot and trailing-space forms are ADMITTED**, because Windows strips
///   them, so they resolve to the same contained file. Also correct.
///
/// Alternate data streams (`in.png:hidden`) and drive-relative paths (`C:in.png`)
/// resolve to nothing and land on `Missing` rather than `Refused`.
pub(crate) fn resolve_contained_image(src: &str, doc_dir: Option<&Path>) -> Option<PathBuf> {
    let doc_dir = doc_dir?;
    let base = dunce::canonicalize(doc_dir).ok()?;
    let candidate = local_candidate(src, Some(doc_dir))?;
    let target = dunce::canonicalize(&candidate).ok()?;
    target.starts_with(&base).then_some(target)
}

/// The result of resolving an image `src` against the safety policy, used by
/// `renderer.rs` to decide what (if anything) to load or display.
#[derive(Debug)]
pub(crate) enum ImageResolution {
    /// An absolute local path that is safe to load — either it passed the
    /// containment gate (when `allow_unsafe_images` is false) or `allow_unsafe`
    /// is true and the path canonicalized successfully.
    Local(PathBuf),
    /// A remote http/https URI. Only produced when `allow_unsafe_images` is true.
    /// Load via `gdk::Texture::from_file(&gio::File::for_uri(uri))`.
    Remote(String),
    /// Blocked by the safety policy: a remote URL with `allow_unsafe_images`
    /// false, or a local path that escapes the document folder. The renderer
    /// shows a broken-image placeholder icon and suppresses the alt text.
    Refused,
    /// Cannot resolve: no `doc_dir` for a relative path, or the file simply
    /// does not exist (canonicalize error). The renderer shows a broken-image
    /// placeholder with an "Image not found" tooltip (not bare alt text) — see
    /// `renderer::image_placeholder_tooltip`.
    Missing,
}

/// Resolve an image `src` respecting the "Show Unsafe Images" opt-in toggle.
///
/// When `allow_unsafe_images` is **false** (the default, VULN-002), the existing
/// containment gate is enforced: local paths must stay at or beneath the document
/// folder (`resolve_contained_image`), and remote http/https URLs are `Refused`.
///
/// When `allow_unsafe_images` is **true**, the containment gate is lifted for
/// local paths (still canonicalized so symlinks resolve to their real target)
/// and remote http/https URLs are returned as `Remote` for the renderer to fetch
/// via `gio::File::for_uri`. All other URL schemes remain `Refused` regardless
/// of the flag (no ftp://, file://, smb://, etc.).
pub(crate) fn resolve_image(
    src: &str,
    doc_dir: Option<&Path>,
    allow_unsafe_images: bool,
) -> ImageResolution {
    // Detect URL scheme first — both for the remote case and to block schemes
    // that would be dangerous regardless of the unsafe flag (file://, smb://…).
    // Via the SHARED `scheme_of` (not an inlined `split_once(':')`) so a local
    // path containing a colon (`assets/notes:v2.png`, `C:\…`) is NOT misread as a
    // URL and wrongly refused — it has no scheme and falls through to the local
    // resolution below (ScrAP-151). A hierarchical `file://`/`smb://`
    // still resolves to a scheme here and is refused.
    if let Some(scheme) = scheme_of(src) {
        match scheme.to_ascii_lowercase().as_str() {
            "http" | "https" => {
                return if allow_unsafe_images {
                    ImageResolution::Remote(src.to_string())
                } else {
                    ImageResolution::Refused
                };
            }
            _ => {
                // Any other scheme (file://, smb://, …) is never handed to the
                // image loader regardless of the unsafe flag.
                return ImageResolution::Refused;
            }
        }
    }

    // Local path (no scheme).
    if allow_unsafe_images {
        // Containment gate lifted: canonicalize and admit any existing path.
        // `local_candidate` tilde-expands and supplies doc_dir as the base for a
        // still-relative path — the same preamble the link resolver uses, shared
        // so the two content types agree on what a path means.
        let Some(candidate) = local_candidate(src, doc_dir) else {
            return ImageResolution::Missing;
        };
        match dunce::canonicalize(&candidate) {
            Ok(abs) => ImageResolution::Local(abs),
            Err(_) => ImageResolution::Missing,
        }
    } else {
        // An untitled buffer (doc_dir is None) cannot resolve a relative path at
        // all — this is not a policy refusal but an inability to resolve, so it
        // is `Missing` ("Image not found" placeholder) rather than `Refused`
        // ("Blocked image" placeholder); the distinction drives the tooltip, not
        // whether an icon appears.  An absolute path with no doc_dir still goes
        // through the containment check (it will fail, yielding Refused — correct,
        // since we have no base to compare against).
        if doc_dir.is_none() && !Path::new(src).is_absolute() {
            return ImageResolution::Missing;
        }
        // Enforce the existing containment gate (VULN-002).
        match resolve_contained_image(src, doc_dir) {
            Some(path) => ImageResolution::Local(path),
            None => ImageResolution::Refused,
        }
    }
}

// ── local document-link navigation ────────────────────────────────

/// The result of resolving a schemeless, non-anchor link `href` found in a
/// rendered document, used by the click handler to decide what to do — mirrors
/// [`ImageResolution`]'s shape (a document link and an image `src` are the same
/// kind of untrusted-content decision, TDD 2.7).
#[derive(Debug)]
pub(crate) enum LinkResolution {
    /// A local `.md`/`.markdown` file, safe to navigate to — either it passed
    /// the containment gate, or containment was lifted for this tab by the
    /// "Load Unsafe Linked Documents" toggle.
    Navigate(PathBuf),
    /// Resolved to an existing local file that is not a Markdown document
    /// (extension check). Never opened externally — see ANTI-PATTERNS: falling
    /// through to the launcher here would be the exact `file://` leak the
    /// scheme gate exists to prevent, just reached through a different door.
    NotMarkdown(PathBuf),
    /// Resolved outside the document's folder and the per-tab "Load Unsafe
    /// Linked Documents" toggle is off.
    Refused,
    /// Could not resolve: no `doc_dir` (untitled buffer), an empty path, or the
    /// target simply does not exist (canonicalize error).
    Missing,
}

/// Resolve a schemeless link `href` (already established by the caller to have
/// no URL scheme and not be a bare `#fragment`, via [`scheme_of`] /
/// [`anchor_target`]) against the linking document's folder, applying the
/// containment gate.
///
/// A `./doc.md#section` cross-document fragment is accepted: the `#…` suffix is
/// stripped before resolving the file path. The fragment itself is not yet
/// carried through to a scroll-to-heading, so today it only affects which base
/// file opens, same as if the fragment were absent.
///
/// `allow_outside_links` is the CURRENT (linking) tab's own toggle — it is
/// deliberately never copied to the tab this navigates to (see the field's own
/// doc comment on [`crate::winstate::TabState::allow_outside_links`] for why:
/// permission re-roots at every hop instead of ratcheting outward).
///
/// Reuses [`resolve_contained_image`] verbatim for the containment check itself
/// (component-wise `Path::starts_with`, canonicalize-first) — same untrusted-
/// content policy, same escape vectors (`../secrets`, a `docs-evil/` sibling
/// masquerading as `docs/`, a symlink under the doc dir pointing outside).
pub(crate) fn resolve_doc_link(
    href: &str,
    doc_dir: Option<&Path>,
    allow_outside_links: bool,
) -> LinkResolution {
    let path_part = href.split('#').next().unwrap_or(href);
    if path_part.is_empty() {
        return LinkResolution::Missing;
    }
    let Some(doc_dir) = doc_dir else {
        return LinkResolution::Missing;
    };

    // Reuse the image gate's exact resolve-and-contain logic when containment
    // is enforced; when the toggle lifts it, still canonicalize (so a symlink
    // resolves to its real target and a genuinely-missing file is still
    // `Missing`, never a phantom `Navigate`) but skip the `starts_with` check.
    let target = if allow_outside_links {
        let Some(candidate) = local_candidate(path_part, Some(doc_dir)) else {
            return LinkResolution::Missing;
        };
        match dunce::canonicalize(&candidate) {
            Ok(t) => t,
            Err(_) => return LinkResolution::Missing,
        }
    } else {
        match resolve_contained_image(path_part, Some(doc_dir)) {
            Some(t) => t,
            None => {
                // Distinguish "exists but escapes the folder" (Refused) from
                // "doesn't exist at all" (Missing) — resolve_contained_image
                // collapses both to None, but the click handler needs to tell
                // the reader WHY nothing happened rather than leaving a click
                // looking broken. This is why `~/notes.md` must tilde-expand on
                // the same path the gate uses: unexpanded it canonicalizes as
                // `<doc_dir>/~/notes.md`, fails, and reports Missing — telling
                // the reader a file that exists does not exist.
                let Some(candidate) = local_candidate(path_part, Some(doc_dir)) else {
                    return LinkResolution::Missing;
                };
                return match dunce::canonicalize(&candidate) {
                    Ok(_) => LinkResolution::Refused,
                    Err(_) => LinkResolution::Missing,
                };
            }
        }
    };

    let is_md = target
        .extension()
        .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false);
    if is_md {
        LinkResolution::Navigate(target)
    } else {
        LinkResolution::NotMarkdown(target)
    }
}

/// The document-relative reference to insert for a chosen local file (the Insert
/// Link / Image Browse buttons): `pathdiff::diff_paths` of the canonicalized target
/// against the canonicalized base, with forward slashes for portability.  Falls back
/// to the absolute path when there is no relative route (different roots).  A result
/// that escapes the base (`../…`) is returned as-is — it simply won't pass the image
/// containment gate, which is the intended security behaviour.
pub(crate) fn relativize_for_insert(target: &Path, base: &Path) -> String {
    let t = dunce::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let b = dunce::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    match pathdiff::diff_paths(&t, &b) {
        Some(rel) => rel.to_string_lossy().replace('\\', "/"),
        None => t.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        anchor_target, doc_link_fragment, is_allowed_url, relativize_for_insert,
        resolve_contained_image, resolve_doc_link, resolve_image, scheme_of, slugify, unique_slug,
        ImageResolution, LinkResolution,
    };
    use std::collections::HashMap;

    #[test]
    fn slugify_matches_github() {
        assert_eq!(slugify("2. Skill Loading"), "2-skill-loading");
        assert_eq!(slugify("Anti-patterns to avoid"), "anti-patterns-to-avoid");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("C++ Guide"), "c-guide");
        // underscores and existing hyphens survive; no collapsing of repeats.
        assert_eq!(slugify("a_b"), "a_b");
        assert_eq!(slugify("a  b"), "a--b");
    }

    #[test]
    fn unique_slug_suffixes_duplicates() {
        let mut seen = HashMap::new();
        assert_eq!(unique_slug("notes", &mut seen), "notes");
        assert_eq!(unique_slug("notes", &mut seen), "notes-1");
        assert_eq!(unique_slug("notes", &mut seen), "notes-2");
        // a different slug is unaffected
        assert_eq!(unique_slug("intro", &mut seen), "intro");
    }

    #[test]
    fn unique_slug_avoids_literal_collision() {
        // A literal heading that already produced "notes-1" must not collide with
        // a later duplicate of "notes".
        let mut seen = HashMap::new();
        assert_eq!(unique_slug("notes-1", &mut seen), "notes-1");
        assert_eq!(unique_slug("notes", &mut seen), "notes");
        assert_eq!(unique_slug("notes", &mut seen), "notes-2"); // skips the taken notes-1
    }

    #[test]
    fn anchor_target_strips_and_decodes() {
        assert_eq!(anchor_target("#intro").as_deref(), Some("intro"));
        assert_eq!(anchor_target("#caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(anchor_target("https://x.com").as_deref(), None);
        assert_eq!(anchor_target("relative").as_deref(), None);
    }

    #[test]
    fn anchor_target_passes_through_invalid_and_incomplete_escapes() {
        // Non-hex digits after `%` → the escape is left literal.
        assert_eq!(anchor_target("#a%ZZb").as_deref(), Some("a%ZZb"));
        // A `%` with fewer than two following bytes → literal.
        assert_eq!(anchor_target("#end%").as_deref(), Some("end%"));
        assert_eq!(anchor_target("#end%4").as_deref(), Some("end%4"));
        // A valid escape still decodes even when an invalid one follows.
        assert_eq!(anchor_target("#%41%GG").as_deref(), Some("A%GG"));
    }

    #[test]
    fn allows_web_and_mail_schemes() {
        assert!(is_allowed_url("http://example.com"));
        assert!(is_allowed_url("https://example.com/page"));
        assert!(is_allowed_url("mailto:user@example.com"));
        // Case-insensitive per RFC 3986.
        assert!(is_allowed_url("HTTPS://example.com"));
    }

    #[test]
    fn rejects_dangerous_and_schemeless_urls() {
        assert!(!is_allowed_url("file:///etc/passwd"));
        assert!(!is_allowed_url("smb://host/share"));
        assert!(!is_allowed_url("javascript:alert(1)"));
        assert!(!is_allowed_url("/etc/passwd")); // no scheme
        assert!(!is_allowed_url("relative/anchor")); // no scheme
    }

    #[test]
    fn scheme_of_recognizes_urls_but_not_colon_bearing_paths() {
        // Genuine URLs: hierarchical `scheme://…` and the bare-colon `mailto:`.
        assert_eq!(scheme_of("http://example.com"), Some("http"));
        assert_eq!(scheme_of("https://example.com/p"), Some("https"));
        assert_eq!(scheme_of("file:///etc/passwd"), Some("file"));
        assert_eq!(scheme_of("smb://host/share"), Some("smb"));
        assert_eq!(scheme_of("mailto:user@example.com"), Some("mailto"));
        assert_eq!(scheme_of("HTTPS://x"), Some("HTTPS")); // case preserved; caller lowercases

        // Local paths that merely CONTAIN a colon are NOT schemes (ScrAP-151).
        assert_eq!(scheme_of("C:\\Users\\me\\img.png"), None); // Windows drive letter
        assert_eq!(scheme_of("C:/Users/me/img.png"), None); // ditto, forward slashes
        assert_eq!(scheme_of("assets/notes:v2.png"), None); // colon in a later segment
        assert_eq!(scheme_of("report:draft.md"), None); // colon in the first segment
        assert_eq!(scheme_of("relative/path.md"), None); // no colon at all
        assert_eq!(scheme_of("#section"), None); // bare fragment

        // Dangerous BARE-colon schemes are not recognised as launchable schemes, but
        // that is safe: with no `//` they fall through to (failing) local resolution
        // and are never launched — security is preserved by fall-through.
        assert_eq!(scheme_of("javascript:alert(1)"), None);
        assert_eq!(scheme_of("data:text/html,<script>"), None);

        // A colon-bearing path is therefore treated as a schemeless local reference,
        // not blocked as a "disallowed scheme".
        assert!(!is_allowed_url("report:draft.md"));
        assert!(!is_allowed_url("C:/Users/me/img.png"));
    }

    #[test]
    fn resolve_admits_files_under_the_doc_dir() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("img.png"), b"x").unwrap();
        fs::create_dir(base.join("sub")).unwrap();
        fs::write(base.join("sub").join("y.png"), b"x").unwrap();
        // Relative src under the doc dir → resolves to an absolute path.
        assert!(resolve_contained_image("img.png", Some(base)).is_some());
        assert!(resolve_contained_image("sub/y.png", Some(base)).is_some());
        // An absolute src that lands under the doc dir is also admitted.
        let abs = base.join("img.png");
        assert!(resolve_contained_image(abs.to_str().unwrap(), Some(base)).is_some());
        // No document directory (untitled) → nothing resolves.
        assert!(resolve_contained_image("img.png", None).is_none());
    }

    #[test]
    fn resolve_rejects_escapes_and_missing_files() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secret.png"), b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        fs::write(base.join("img.png"), b"x").unwrap();
        // `..` traversal escaping the doc dir.
        assert!(resolve_contained_image("../secret.png", Some(&base)).is_none());
        // An absolute path outside the doc dir.
        let outside = outer.path().join("secret.png");
        assert!(resolve_contained_image(outside.to_str().unwrap(), Some(&base)).is_none());
        // A file that does not exist (canonicalize errors).
        assert!(resolve_contained_image("nope.png", Some(&base)).is_none());
    }

    #[test]
    fn resolve_rejects_symlink_escape() {
        use crate::testsymlink::symlink_or_skip;
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secret.png"), b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        // A symlink UNDER the doc dir pointing OUTSIDE must be rejected: canonicalize
        // resolves the link to its outside target, which fails containment.
        let link = base.join("link.png");
        if symlink_or_skip(&outer.path().join("secret.png"), &link, "TDD 2.7 symlink").is_err() {
            return;
        }
        assert!(resolve_contained_image("link.png", Some(&base)).is_none());
    }

    // ── resolve_image tests ─────────────────────────────────────────────────────

    #[test]
    fn resolve_image_admits_contained_local_path() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("img.png"), b"x").unwrap();
        // Safe local path → Local regardless of allow_unsafe flag.
        assert!(matches!(
            resolve_image("img.png", Some(base), false),
            ImageResolution::Local(_)
        ));
        assert!(matches!(
            resolve_image("img.png", Some(base), true),
            ImageResolution::Local(_)
        ));
    }

    #[test]
    #[cfg(unix)] // a colon is a valid filename char on Unix; invalid on Windows (NTFS ADS)
    fn resolve_image_admits_contained_local_path_with_a_colon_in_its_name() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        // A same-directory file whose NAME contains a colon — the exact ISSUES case.
        // Before the fix, `resolve_image`'s inlined `split_once(':')` read "notes" (or
        // "assets/notes") as a URL scheme and returned Refused; now it has no scheme
        // and resolves as the contained local file it is.
        fs::create_dir(base.join("assets")).unwrap();
        fs::write(base.join("assets").join("notes:v2.png"), b"x").unwrap();
        assert!(
            matches!(
                resolve_image("assets/notes:v2.png", Some(base), false),
                ImageResolution::Local(_)
            ),
            "a contained local image whose name contains a colon must render, not be refused"
        );
        // And a first-segment colon (`report:draft.png`) too.
        fs::write(base.join("report:draft.png"), b"x").unwrap();
        assert!(matches!(
            resolve_image("report:draft.png", Some(base), false),
            ImageResolution::Local(_)
        ));
    }

    #[test]
    fn resolve_image_still_refuses_hierarchical_nonweb_schemes() {
        // The security gate is unchanged for real URL schemes: file://, smb:// are
        // Refused regardless of the unsafe flag (they resolve to a scheme via `://`).
        for unsafe_on in [false, true] {
            assert!(matches!(
                resolve_image("file:///etc/passwd", None, unsafe_on),
                ImageResolution::Refused
            ));
            assert!(matches!(
                resolve_image("smb://host/share/x.png", None, unsafe_on),
                ImageResolution::Refused
            ));
        }
    }

    #[test]
    fn resolve_image_refuses_remote_when_unsafe_off() {
        assert!(matches!(
            resolve_image("https://example.com/img.png", None, false),
            ImageResolution::Refused
        ));
        assert!(matches!(
            resolve_image("http://example.com/img.png", None, false),
            ImageResolution::Refused
        ));
    }

    #[test]
    fn resolve_image_allows_remote_when_unsafe_on() {
        // Remote returns a Remote variant (actual network fetch deferred to renderer).
        let res = resolve_image("https://example.com/img.png", None, true);
        assert!(
            matches!(res, ImageResolution::Remote(ref s) if s == "https://example.com/img.png")
        );
    }

    #[test]
    fn resolve_image_refuses_escaping_local_path_when_unsafe_off() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        // `../` traversal refused when allow_unsafe is false.
        assert!(matches!(
            resolve_image("../secret.png", Some(&base), false),
            ImageResolution::Refused
        ));
    }

    #[test]
    fn resolve_image_admits_any_existing_local_path_when_unsafe_on() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secret.png"), b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        // Escaping path allowed when allow_unsafe is true and file exists.
        assert!(matches!(
            resolve_image("../secret.png", Some(&base), true),
            ImageResolution::Local(_)
        ));
    }

    #[test]
    fn resolve_image_returns_missing_for_nonexistent_file_when_unsafe_on() {
        let dir = tempfile::tempdir().unwrap();
        // File doesn't exist → Missing (can't canonicalize).
        assert!(matches!(
            resolve_image("nonexistent.png", Some(dir.path()), true),
            ImageResolution::Missing
        ));
    }

    #[test]
    fn resolve_image_returns_missing_without_doc_dir_for_relative_src() {
        // Relative path with no doc_dir is unresolvable — Missing ("not found" placeholder)
        // in both unsafe modes, to match current behaviour for untitled buffers.
        assert!(matches!(
            resolve_image("relative.png", None, true),
            ImageResolution::Missing
        ));
        assert!(matches!(
            resolve_image("relative.png", None, false),
            ImageResolution::Missing
        ));
    }

    #[test]
    fn resolve_image_refuses_non_http_schemes_even_when_unsafe_on() {
        // file://, smb://, javascript: are always refused regardless of flag.
        assert!(matches!(
            resolve_image("file:///etc/passwd", None, true),
            ImageResolution::Refused
        ));
        assert!(matches!(
            resolve_image("smb://host/img.png", None, true),
            ImageResolution::Refused
        ));
    }

    #[test]
    fn relativize_yields_doc_relative_paths() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::create_dir(base.join("sub")).unwrap();
        let target = base.join("sub").join("y.png");
        fs::write(&target, b"x").unwrap();
        assert_eq!(relativize_for_insert(&target, base), "sub/y.png");
    }

    // ── leading-`~` expansion ───────────────────────────────────────────────────
    //
    // The bug these pin: unexpanded, `~/notes.md` is not `is_absolute()`, so it was
    // joined onto the doc dir as `<doc_dir>/~/notes.md`, canonicalize failed, and the
    // reader was told "Link target not found" about a file that exists — a silent
    // WRONG ANSWER, not an error.

    /// The regression this fix must never reintroduce: expanding the tilde must not
    /// smuggle a path past containment. Expanded, `~/x.md` is an absolute path OUTSIDE
    /// the document's folder, so with the opt-in OFF the correct verdict is **Refused**
    /// — not Missing (the old bug), and emphatically not Navigate (the bypass).
    #[test]
    fn tilde_path_is_refused_not_missing_when_containment_is_on() {
        use std::fs;
        let home = glib::home_dir();
        let dir = tempfile::tempdir().unwrap();
        let probe = home.join("scribobulate-rrr-probe.md");
        if fs::write(&probe, b"# Probe").is_err() {
            return; // unwritable HOME in a sandbox — nothing to assert
        }
        let verdict = resolve_doc_link("~/scribobulate-rrr-probe.md", Some(dir.path()), false);
        let _ = fs::remove_file(&probe);
        assert!(
            matches!(verdict, LinkResolution::Refused),
            "expanded tilde must reach the containment gate and be Refused, got {verdict:?}"
        );
    }

    /// With the per-tab opt-in ON, the same path navigates — proving the expansion is
    /// wired into the toggle-on branch too, not just the gate.
    #[test]
    fn tilde_path_navigates_when_the_opt_in_is_on() {
        use std::fs;
        let home = glib::home_dir();
        let dir = tempfile::tempdir().unwrap();
        let probe = home.join("scribobulate-rrr-probe-on.md");
        if fs::write(&probe, b"# Probe").is_err() {
            return;
        }
        let verdict = resolve_doc_link("~/scribobulate-rrr-probe-on.md", Some(dir.path()), true);
        let _ = fs::remove_file(&probe);
        assert!(
            matches!(verdict, LinkResolution::Navigate(_)),
            "opt-in on must navigate an expanded tilde path, got {verdict:?}"
        );
    }

    /// Scope guard: a `~` anywhere but position 0 stays a literal path component.
    /// `./~/x.md` names a real subdirectory called `~`, and must not touch $HOME.
    #[test]
    fn a_tilde_that_is_not_a_leading_component_stays_literal() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::create_dir(base.join("~")).unwrap();
        fs::write(base.join("~").join("x.md"), b"# Literal").unwrap();
        // Resolves inside the doc folder, so containment admits it with the toggle OFF.
        assert!(
            matches!(
                resolve_doc_link("./~/x.md", Some(base), false),
                LinkResolution::Navigate(_)
            ),
            "a literal `~` directory component must not be expanded"
        );
    }

    /// Links and images must agree about what a path means — fixing only the link
    /// resolver would leave `![](~/pic.png)` broken and the two content types
    /// disagreeing, which is the inconsistency the shared `local_candidate` prevents.
    #[test]
    fn the_image_resolver_expands_the_tilde_on_the_same_terms_as_links() {
        use std::fs;
        let home = glib::home_dir();
        let dir = tempfile::tempdir().unwrap();
        let probe = home.join("scribobulate-rrr-probe.png");
        if fs::write(&probe, b"\x89PNG").is_err() {
            return;
        }
        // Containment on: outside the doc folder, so refused (no phantom Missing).
        let contained = resolve_contained_image("~/scribobulate-rrr-probe.png", Some(dir.path()));
        // Containment lifted: the same path resolves to the real file.
        let unsafe_verdict = resolve_image("~/scribobulate-rrr-probe.png", Some(dir.path()), true);
        let _ = fs::remove_file(&probe);
        assert!(
            contained.is_none(),
            "an expanded tilde image outside the folder must not pass containment"
        );
        assert!(
            matches!(unsafe_verdict, ImageResolution::Local(_)),
            "unsafe-images on must resolve an expanded tilde image, got {unsafe_verdict:?}"
        );
    }

    // ── resolve_doc_link tests ─────────────────────────────────────

    #[test]
    fn doc_link_navigates_to_contained_markdown_sibling() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("TECH.md"), b"# Tech").unwrap();
        assert!(matches!(
            resolve_doc_link("TECH.md", Some(base), false),
            LinkResolution::Navigate(_)
        ));
        // Explicit `./` prefix and a nested sibling both resolve too.
        fs::create_dir(base.join("sdd")).unwrap();
        fs::write(base.join("sdd").join("TDD.md"), b"# TDD").unwrap();
        assert!(matches!(
            resolve_doc_link("./TECH.md", Some(base), false),
            LinkResolution::Navigate(_)
        ));
        assert!(matches!(
            resolve_doc_link("sdd/TDD.md", Some(base), false),
            LinkResolution::Navigate(_)
        ));
    }

    #[test]
    fn doc_link_drops_a_trailing_fragment_before_resolving() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("TECH.md"), b"# Tech").unwrap();
        assert!(matches!(
            resolve_doc_link("TECH.md#architecture", Some(base), false),
            LinkResolution::Navigate(_)
        ));
    }

    #[test]
    fn doc_link_refuses_dotdot_escape_when_toggle_off() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secrets.md"), b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        assert!(matches!(
            resolve_doc_link("../secrets.md", Some(&base), false),
            LinkResolution::Refused
        ));
    }

    #[test]
    fn doc_link_permits_dotdot_escape_when_toggle_on() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secrets.md"), b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        assert!(matches!(
            resolve_doc_link("../secrets.md", Some(&base), true),
            LinkResolution::Navigate(_)
        ));
    }

    #[test]
    fn doc_link_refuses_sibling_folder_masquerade() {
        // `docs-evil/x.md` must not be admitted as if it were under `docs/` —
        // a string-prefix check would wrongly accept this (formerly the image
        // gate's own regression risk; the same containment fn is reused here).
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let docs = outer.path().join("docs");
        fs::create_dir(&docs).unwrap();
        let docs_evil = outer.path().join("docs-evil");
        fs::create_dir(&docs_evil).unwrap();
        fs::write(docs_evil.join("x.md"), b"x").unwrap();
        // A link found inside a document living in `docs/` pointing at
        // `../docs-evil/x.md` must be refused, not silently admitted because
        // `docs-evil` starts with the string `docs`.
        assert!(matches!(
            resolve_doc_link("../docs-evil/x.md", Some(&docs), false),
            LinkResolution::Refused
        ));
    }

    /// The symlink setup these two tests need now lives in [`crate::testsymlink`],
    /// hoisted there so the other tests over the same subject could reach it — it was
    /// applied at two sites out of five while it sat in this file.
    ///
    /// The history it was written for is specific to this rubric and stays here: the
    /// symlink half of TDD 19.2 had no coverage at all on Windows, because the
    /// automated test was `#[cfg(unix)]`-deleted AND the checked-in fixture
    /// (`tests/fixtures/escapes-via-symlink.md`, git mode 120000) materialises on a
    /// `core.symlinks=false` checkout as an ordinary 32-byte text file holding the
    /// target path — so the manual check silently exercised the INVERSE of its intent,
    /// navigating where it must refuse. Neither absence announced itself, and each
    /// made the other harder to see.
    use crate::testsymlink::symlink_or_skip;

    #[test]
    fn doc_link_refuses_symlink_escape_when_toggle_off() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        fs::write(outer.path().join("secrets.md"), b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        let link = base.join("link.md");
        if symlink_or_skip(&outer.path().join("secrets.md"), &link, "TDD 19.2 symlink").is_err() {
            return;
        }
        assert!(
            matches!(
                resolve_doc_link("link.md", Some(&base), false),
                LinkResolution::Refused
            ),
            "a symlink under the document folder pointing outside it must be refused on \
             the RESOLVED path; deciding on the link's own location admits it (TDD 19.2)"
        );
    }

    /// The other half of the same guarantee, and the one that proves the refusal above
    /// is decided on the resolved target rather than on the link's own location: with
    /// containment lifted, the SAME link resolves and navigates to the real file
    /// outside the folder (TDD 19.3). Without this, `Refused` above would be equally
    /// consistent with a resolver that simply never followed the link.
    #[test]
    fn doc_link_symlink_resolves_to_its_real_target_when_toggle_on() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let target = outer.path().join("secrets.md");
        fs::write(&target, b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        let link = base.join("link.md");
        if symlink_or_skip(&target, &link, "TDD 19.3 symlink").is_err() {
            return;
        }
        let want = dunce::canonicalize(&target).unwrap();
        assert!(
            matches!(
                resolve_doc_link("link.md", Some(&base), true),
                LinkResolution::Navigate(ref p) if *p == want
            ),
            "with containment lifted the link must resolve to its real target, proving \
             the refusal in the sibling test is decided on the resolved path (TDD 19.3)"
        );
    }

    #[test]
    fn doc_link_refuses_non_markdown_target_even_when_contained() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("data.csv"), b"a,b\n").unwrap();
        assert!(matches!(
            resolve_doc_link("data.csv", Some(base), false),
            LinkResolution::NotMarkdown(_)
        ));
        // Still refused as non-Markdown even with the containment toggle on —
        // it must never fall through to the external launcher (that would be
        // the same file:// leak the scheme gate exists to prevent, reached
        // through a different door).
        assert!(matches!(
            resolve_doc_link("data.csv", Some(base), true),
            LinkResolution::NotMarkdown(_)
        ));
    }

    #[test]
    fn doc_link_missing_target_is_distinguished_from_refused() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        // Does not exist at all → Missing, not Refused — the click handler
        // must tell these apart to show the right error (formerly, silence
        // made "refused" and "not found" both look like nothing
        // happened).
        assert!(matches!(
            resolve_doc_link("nope.md", Some(base), false),
            LinkResolution::Missing
        ));
        assert!(matches!(
            resolve_doc_link("nope.md", Some(base), true),
            LinkResolution::Missing
        ));
    }

    #[test]
    fn doc_link_missing_without_doc_dir() {
        // Untitled buffer (no doc_dir) — nothing resolves, regardless of toggle.
        assert!(matches!(
            resolve_doc_link("TECH.md", None, false),
            LinkResolution::Missing
        ));
        assert!(matches!(
            resolve_doc_link("TECH.md", None, true),
            LinkResolution::Missing
        ));
    }

    // ── doc_link_fragment tests ─────────────────────────────────────────────────

    #[test]
    fn doc_link_fragment_extracts_and_decodes() {
        assert_eq!(
            doc_link_fragment("TECH.md#module-map").as_deref(),
            Some("module-map")
        );
        assert_eq!(
            doc_link_fragment("./sub/PLAN.md#caf%C3%A9").as_deref(),
            Some("café")
        );
    }

    #[test]
    fn doc_link_fragment_is_none_without_a_fragment_or_with_an_empty_one() {
        assert_eq!(doc_link_fragment("TECH.md").as_deref(), None);
        // Trailing bare `#` (no text after it) — nothing to look up.
        assert_eq!(doc_link_fragment("TECH.md#").as_deref(), None);
    }

    #[test]
    fn doc_link_empty_href_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            resolve_doc_link("", Some(dir.path()), false),
            LinkResolution::Missing
        ));
        // A bare fragment-only href reaching this function (should normally be
        // caught upstream by `anchor_target`, but defend anyway).
        assert!(matches!(
            resolve_doc_link("#section", Some(dir.path()), false),
            LinkResolution::Missing
        ));
    }
}
