//! Link, anchor, and resource-target handling for rendered Markdown: which URL
//! schemes are safe to launch, opening them, image-path containment, and decoding
//! a `#fragment` (what it is matched against lives in `anchors.rs`).
//! Extracted from `renderer.rs` to keep that module focused on AST → buffer
//! rendering.

use std::path::{Path, PathBuf};

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

/// Percent-encode a local filesystem path so it is a valid Markdown destination.
///
/// **Only ever applied to a path we produced ourselves** — the Browse button's chosen
/// file (`path_for_insert`), never to whatever the user typed in the URL field.
/// That restriction is the whole design: the field takes a local path OR a real URL and
/// nothing can reliably tell them apart, so encoding a typed value would mangle the
/// `?`, `&` and `:` of a pasted URL, or double-encode one that was already escaped.
/// Browse has no such ambiguity — it hands us a `PathBuf`.
///
/// `/` is left alone (it is the separator, already normalised from `\` by the caller)
/// and so are non-ASCII bytes, which every consumer here handles raw and which would
/// otherwise turn a perfectly readable `café.png` into noise in the URL field.
/// **`%` itself is encoded**, which is what makes the round trip lossless: a file
/// genuinely named `100%.png` becomes `100%25.png` and decodes back to itself.
fn percent_encode_path(s: &str) -> String {
    // Assembled as BYTES, not by pushing `b as char`: that reads each byte as a
    // Unicode scalar, so a multi-byte character is re-encoded per byte and
    // `café.png` comes out `cafÃ©.png`. Untouched bytes are copied verbatim, so the
    // result is still valid UTF-8 by construction (the input was).
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for b in s.bytes() {
        // The set that breaks either Markdown's inline destination or URL syntax.
        // `(` and `)` are in it because they terminate `](…)`; `%` because of the
        // round trip above; the control range because it cannot survive a line of text.
        if matches!(
            b,
            b' ' | b'%'
                | b'#'
                | b'?'
                | b'('
                | b')'
                | b'<'
                | b'>'
                | b'"'
                | b'\\'
                | b'^'
                | b'`'
                | b'{'
                | b'}'
                | b'|'
        ) || b < 0x20
            || b == 0x7F
        {
            out.extend_from_slice(format!("%{b:02X}").as_bytes());
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// The gates' one filesystem resolution — every call that can reach a path, and so a
/// network share, goes through here. Counted in tests, so a test can assert that a
/// refusal touched the filesystem not at all, which holds on any host however fast
/// its network fails.
fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    #[cfg(test)]
    CANONICALIZE_CALLS.with(|n| n.set(n.get() + 1));
    dunce::canonicalize(path)
}

#[cfg(test)]
thread_local! {
    /// How many times [`canonicalize`] has run on this thread.
    static CANONICALIZE_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The canonicalized on-disk file a Markdown destination denotes, or `None` when
/// nothing is there. The single place a destination becomes a path, so the image gate,
/// the unsafe-images branch and the link resolver cannot drift on what one means.
///
/// **Decoded first, literal second.** A Markdown destination is a URL, so
/// `A%20file.svg` denotes `A file.svg` — that is what GitHub renders, and what this
/// app's own Insert dialog writes since Browse started encoding. The literal retry is
/// for the genuine collision: a file whose name really does contain something shaped
/// like an escape. Decoding cannot widen the containment gate, which canonicalizes and
/// re-checks `starts_with(base)` on whatever comes back — a `%2e%2e%2f` traversal
/// decodes to `../` and is refused exactly as the literal form is.
fn canonical_local_target(src: &str, doc_dir: Option<&Path>) -> Option<PathBuf> {
    // **The choke point every route passes through touches the network for a remote
    // namespace, so it refuses one itself**, from the text alone and whatever either
    // opt-in toggle says: canonicalizing `\\host\share\x` IS the SMB session (and NTLM
    // handshake) the refusal exists to prevent. Callers that report a verdict check
    // `reaches_a_foreign_remote_namespace` first, to say WHY; this is the backstop that
    // makes forgetting to impossible to turn into a connection.
    if reaches_a_foreign_remote_namespace(src, doc_dir) {
        return None;
    }
    let decoded = percent_decode(src);
    if decoded != src {
        if let Some(hit) = local_candidate(&decoded, doc_dir).and_then(|c| canonicalize(&c).ok()) {
            return Some(hit);
        }
    }
    local_candidate(src, doc_dir).and_then(|c| canonicalize(&c).ok())
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

/// Whether `href` may be written into an **exported artefact's** link destination.
///
/// The one gate every serialising sink asks, so the answer cannot differ between
/// them. Two ways to pass: an allowed external scheme, or a genuinely relative
/// reference carrying a fragment (`TECH.md#map`, `#section`) — the cross-document
/// and same-document links the artefact can still resolve for itself.
///
/// # Why this is not `is_allowed_url(h) || doc_link_fragment(h).is_some()`
///
/// That is what it used to be, and the second limb was not a gate. [`doc_link_fragment`]
/// documents a *schemeless* precondition and enforces nothing — it returns `Some` for
/// any href containing a non-empty `#`. `[click](javascript:alert(1)//#x)` satisfies
/// it: the `#x` that matched is commented out by the `//` in the payload itself. The
/// destination then reached `<a href>` intact, because the attribute escaper rewrites
/// `& < > " '` and that payload contains none of them — script execution in the
/// artefact's origin, which for a locally-opened export is `file://`.
///
/// The precondition is therefore enforced here, against [`has_uri_scheme`] rather than
/// [`scheme_of`]. See that function for why the stricter grammar is the right one on
/// this side of the boundary.
pub(crate) fn is_exportable_href(href: &str) -> bool {
    is_allowed_url(href) || (is_relative_reference(href) && doc_link_fragment(href).is_some())
}

/// Does `href` POSITIVELY look like a relative reference an artefact can resolve?
///
/// # This is an allowlist, and the previous version's failure is why
///
/// The second limb of [`is_exportable_href`] used to read `!has_uri_scheme(href)` —
/// "relative" inferred from "my scheme parser did not recognise this". That is a
/// rejection test standing in for an admission test, and it inherits every disagreement
/// between the two parsers involved. [`has_uri_scheme`] implements RFC 3986 §3.1, where
/// a scheme is `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`; the engine reading the
/// artefact implements the WHATWG URL Standard, which **strips leading C0 controls and
/// spaces, and removes every tab, LF and CR anywhere in the URL, before it parses the
/// scheme at all**. So a destination our parser rejects as malformed is one the engine
/// repairs and runs, and the fragment limb then admitted it:
///
/// ```text
/// [a](<{TAB}javascript:alert(1)//#x>)   // {TAB} is a literal U+0009
/// ```
///
/// Blacklisting tab, LF and CR would close the payloads that were measured and leave the
/// class open, because the class is *the two parsers disagree*, not *these characters*.
/// So this asks what a relative reference IS, and anything it cannot recognise loses its
/// destination and keeps its text — the direction that costs a link rather than the one
/// that emits script.
///
/// Admitted:
/// * a same-document fragment (`#section`);
/// * a path whose characters are drawn from the unreserved/sub-delims set plus `/`,
///   `.`, `%`, `#` and `?` — enough for `TECH.md#map`, `./sub/PLAN.md#caf%C3%A9` and a
///   query string, and nothing that can carry a scheme.
///
/// Refused, each deliberately:
/// * anything containing a colon, whatever surrounds it — a relative path does not need
///   one here, and it is the only character that can make a scheme;
/// * **a protocol-relative `//host/path`**, which is not a relative reference at all: it
///   inherits the artefact's scheme, so from a `file://` page it is a filesystem fetch,
///   and on Windows a `//host/share` is a UNC path reaching SMB;
/// * any C0 control, space or DEL, which is what the engine strips or rejects, so our
///   opinion of the rest is worthless once it has;
/// * any non-ASCII byte: an artefact link in this project is a document path, and
///   admitting the percent-encoded form only keeps this decidable.
fn is_relative_reference(href: &str) -> bool {
    if href.is_empty() || href.starts_with("//") {
        return false;
    }
    href.chars().all(|c| {
        matches!(c,
            'a'..='z' | 'A'..='Z' | '0'..='9'
            | '-' | '.' | '_' | '~'            // unreserved (RFC 3986 §2.3)
            | '!' | '$' | '&' | '\'' | '(' | ')'
            | '*' | '+' | ',' | ';' | '='      // sub-delims
            | '/' | '%' | '#' | '?' | '@'      // path, escape, fragment, query
        )
    })
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
///    falls through to local resolution where it fails to resolve and is rendered inert
///    **in this process**. Security is preserved by fall-through, not by naming each
///    dangerous scheme.
///
/// # That qualifier is load-bearing, and it was missing
///
/// "Rendered inert" is a property of **this** consumer, not of the value. The moment the
/// same href is written into an artefact — an exported HTML file — it is read by an
/// engine applying RFC 3986 §3.1, where a colon alone makes a scheme, and `javascript:`
/// executes. This comment previously closed on the unqualified claim, and it is the
/// comment a maintainer reads before touching the parser; the register entry that had
/// the lesson right (GTK4Rs/AP-247, *"no handler is registered for this scheme" is not a
/// safety property*) is not what gets read at the moment of the edit.
///
/// So anything bound for serialisation asks [`is_exportable_href`], which does NOT
/// consult this function at all. **Do not wire it back in.** The first fix did — it
/// admitted a destination when this parser's stricter twin found no scheme — and that
/// inherited every disagreement between an RFC 3986 parser and the WHATWG one the
/// reader actually runs, which repairs a malformed scheme before parsing it. The export
/// gate asks what a relative reference IS instead. Answering "not a scheme" is not the
/// same question as "safe to serialise", whichever parser answers it (GEP-82, GEP-53).
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
/// (`gdkapplaunchcontext-x11.c:346-348`). This is the opposite of GTK4Rs/AP-54,
/// whose *focus* path (`gdksurface-x11.c:2220`) passes the timestamp **raw** with no
/// substitution — hence 0 failing there. Different code paths; GTK4Rs/AP-54 does not apply here.
///
/// **Parent is deliberately `None`.** It is not needed for the token (the launch
/// context comes from `gdk_display_get_default()` when parent is NULL —
/// `gtkshow.c:118`), and passing one would call `gtk_window_export_handle`, whose
/// unexport half has no X11 branch on 4.6 — logging
/// `Couldn't unexport handle for GdkX11Toplevel surface` on **every link click**, on
/// real X11, not just headless (fixed upstream in GTK 4.8.0 by a1d03e69a4, never
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
    let base = canonicalize(doc_dir).ok()?;
    let target = canonical_local_target(src, Some(doc_dir))?;
    target.starts_with(&base).then_some(target)
}

/// The containment gate's verdict *with its reason*, which
/// [`resolve_contained_image`] cannot express: it answers `None` both for a real
/// file that escapes the document folder and for a path with no file behind it at
/// all, and those two are opposite things to tell the reader.
///
/// This is the enum split GTK4Rs/AP-34 names, applied to the whole gate rather than to
/// one corner of it. That entry's fix reached only the untitled-buffer case (no
/// `doc_dir`, relative `src`), so a **contained** reference whose file was simply
/// absent at render time still read as `Escapes` — an image sitting safely beside
/// its document was reported blocked by the safety policy, and the placeholder's own
/// tooltip then told the reader to lift that policy to see it. That is a lie about
/// where the boundary is, and it teaches exactly the wrong reflex (VULN-002's gate is
/// the thing being invited off). It bit hardest through live reload, where a document
/// and its images arrive together and the image can lose the race by a frame — the
/// gate is a pure function, so it cannot see that a file merely has not landed *yet*,
/// and only the reason it reports keeps that transient honest.
///
/// Both callers of the gate go through here so the two content types cannot drift on
/// this question the way they already did once — a link click has distinguished the
/// two since [`resolve_doc_link`] was written, and an image never did.
///
/// **The reason is not an existence oracle worth worrying about**: it is shown to the
/// user, who can already `stat` anything this process can, and never to the document —
/// with the gate on there is no network path for a `Missing`/`Refused` answer to leave
/// by. Refusal is unchanged in *effect* either way: nothing outside the folder is ever
/// loaded, whichever reason the placeholder gives.
enum Containment {
    /// At or beneath the document folder — the canonicalized target, safe to load.
    Inside(PathBuf),
    /// A real file, but outside the document folder. The safety policy refuses it.
    Escapes,
    /// Nothing on disk resolves here (or a relative `src` with no `doc_dir` to
    /// resolve it against) — unresolvable, not refused.
    Absent,
}

/// Run the containment gate and report *why* it answered as it did. See
/// [`Containment`].
fn containment_of(src: &str, doc_dir: Option<&Path>) -> Containment {
    // A foreign remote namespace never gets here as anything but `Absent`: the verdict
    // callers report it first (`NetworkShare`), and `canonical_local_target` refuses it
    // before any filesystem call.
    if let Some(target) = resolve_contained_image(src, doc_dir) {
        return Containment::Inside(target);
    }
    // Refused or unresolvable? Re-resolve the same candidate the gate just built —
    // through `canonical_local_target`, so the tilde expands and the destination
    // percent-decodes on the identical path the gate used (an unexpanded `~/x.png`
    // canonicalizes as `<doc_dir>/~/x.png`, fails, and would report a file that
    // exists as absent; an undecoded `A%20b.png` would do the same).
    match canonical_local_target(src, doc_dir) {
        Some(_) => Containment::Escapes,
        None => Containment::Absent,
    }
}

/// Whether `src` (in its literal or its percent-decoded form, the two forms
/// [`canonical_local_target`] tries) names a remote or device namespace — a Windows UNC
/// share, `\\?\UNC\…`, `\\.\…` or another verbatim non-disk prefix — that is not
/// lexically inside `doc_dir`.
///
/// **Why it must be lexical.** `dunce::canonicalize` on `\\host\share\x.png` calls
/// `CreateFileW`, which opens an SMB session to `host` and authenticates with the
/// user's NTLM credentials. That would happen before the gate's `starts_with` check
/// refuses the path, so merely rendering a hostile document would leak the hash with
/// no click, and an unreachable host would stall the main thread for the SMB timeout.
/// This check runs on the text alone and touches no filesystem.
///
/// **Why a document on a share still works.** A candidate lexically under the
/// document's own folder is left to the normal gate, because that only reaches the
/// server the document was opened from. `..` is folded lexically, so it cannot climb
/// out to another share without being caught here. Components are compared
/// case-insensitively. That only lets more through to the real canonical check, which
/// still decides.
///
/// **Then through symbolic links, read without being followed.** A path that is
/// lexically innocent can still lead to a share: `lnk/x.png`, where `lnk` is a symbolic
/// link in the document's folder whose target is `\\host\share`. `canonicalize` follows
/// it and reaches the host (MEASURED on Windows: a 22 s stall before "Image not found",
/// with no click). So each component is examined with `symlink_metadata`, which does
/// not follow a link, and a link's target is read with `read_link` and judged by the
/// same rule before anything traverses it. See [`links_reach_foreign`].
///
/// On Linux and macOS no path has a prefix component, so this is always false there.
fn reaches_a_foreign_remote_namespace(src: &str, doc_dir: Option<&Path>) -> bool {
    let is_foreign = |path: &Path| {
        is_remote_namespace(path) && !doc_dir.is_some_and(|dir| lexically_within(path, dir))
    };
    let decoded = percent_decode(src);
    // The literal form only when decoding changed it: walking the same path twice
    // doubles every `lstat` and automount question for the common, escape-free case.
    let literal = (decoded != src).then_some(src);
    let foreign = std::iter::once(decoded.as_str())
        .chain(literal)
        .any(|form| {
            local_candidate(form, doc_dir).is_some_and(|candidate| {
                is_foreign(&candidate)
                    || links_reach_foreign(
                        &candidate,
                        doc_dir,
                        &is_foreign,
                        &crate::platform::mounts_on_lookup,
                    )
            })
        });
    foreign
}

/// How many symbolic links one resolution follows before giving up. Giving up answers
/// "foreign" (refuse): Linux refuses a chain this long itself, but Windows follows up to
/// 63 reparse points, so a chain of 41–63 links ending at a share would otherwise pass
/// the walk and `canonicalize` would then reach the host. A local loop this long is
/// refused as a share rather than reported missing; the wording is the only cost.
const MAX_LINK_HOPS: usize = 40;

/// Whether resolving `candidate` would pass through a symbolic link whose target
/// `is_foreign`, resolving one component at a time without ever letting the
/// filesystem follow a link itself.
///
/// The walk starts at `doc_dir` when `candidate` is spelled under it (the document's
/// own folder is trusted, even on a share), else at the candidate's root. Each step
/// `symlink_metadata`s one component of an already resolved, link-free prefix, so no
/// call can traverse an unexamined link. A link's target replaces the walk's position
/// and its own components are walked in turn, so a chain of links is followed hop by
/// hop. A component that does not exist ends the walk: nothing further can be reached.
/// A link that cannot be read, or a chain past [`MAX_LINK_HOPS`], fails closed: the
/// walk cannot say where it leads, so it is treated as reaching a share.
///
/// **A step into an automounter's directory is a share too, and is refused without
/// looking.** `mounts_on_lookup` says whether the resolved prefix is one (macOS's
/// `/net -hosts` map is the case that matters: `/net/<host>` mounts that host's NFS
/// exports). There, the `symlink_metadata` of the next component is itself the network
/// contact: MEASURED on macOS 27 with `/net` enabled, a 6 s stall per unreachable host
/// before "Image not found", with no click. Asking about the prefix rather than the
/// next component is what keeps the question itself off the network.
fn links_reach_foreign(
    candidate: &Path,
    doc_dir: Option<&Path>,
    is_foreign: &dyn Fn(&Path) -> bool,
    mounts_on_lookup: &dyn Fn(&Path) -> bool,
) -> bool {
    use std::collections::VecDeque;
    use std::path::Component;

    let (mut current, remaining) = match doc_dir.and_then(|dir| {
        candidate
            .strip_prefix(dir)
            .ok()
            .map(|rest| (dir.to_path_buf(), rest))
    }) {
        Some((base, rest)) => (base, rest.to_path_buf()),
        None => {
            let root: PathBuf = candidate
                .components()
                .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
                .collect();
            let rest = candidate
                .strip_prefix(&root)
                .unwrap_or(candidate)
                .to_path_buf();
            (root, rest)
        }
    };
    let mut pending: VecDeque<std::ffi::OsString> = VecDeque::new();
    let push_back = |path: &Path, pending: &mut VecDeque<std::ffi::OsString>| {
        for component in path.components() {
            pending.push_back(component.as_os_str().to_os_string());
        }
    };
    push_back(&remaining, &mut pending);

    let mut hops = 0;
    while let Some(part) = pending.pop_front() {
        match Path::new(&part).components().next() {
            Some(Component::CurDir) | None => continue,
            Some(Component::ParentDir) => {
                current.pop();
                continue;
            }
            // APPENDED, never assigned: on Windows a target's root is two components,
            // the drive (`C:`) and then the root (`\`), and assigning the second
            // discarded the first, so a re-walked target continued from the current
            // drive's root, found nothing and let a two-hop link through (MEASURED on
            // Windows). `push` of a root onto a bare drive gives `C:\`.
            Some(Component::Prefix(_) | Component::RootDir) => {
                current.push(&part);
                continue;
            }
            Some(Component::Normal(_)) => {}
        }
        if mounts_on_lookup(&current) {
            return true;
        }
        let next = current.join(&part);
        if is_foreign(&next) {
            return true;
        }
        let Ok(meta) = std::fs::symlink_metadata(&next) else {
            return false;
        };
        if !meta.file_type().is_symlink() {
            current = next;
            continue;
        }
        hops += 1;
        if hops > MAX_LINK_HOPS {
            return true;
        }
        let Ok(target) = std::fs::read_link(&next) else {
            return true;
        };
        let resolved = if target.is_absolute() {
            target
        } else {
            current.join(target)
        };
        if is_foreign(&resolved) {
            return true;
        }
        // Walk the target's own components next, from its root, ahead of what is left.
        let mut ahead: VecDeque<std::ffi::OsString> = VecDeque::new();
        for component in resolved.components() {
            ahead.push_back(component.as_os_str().to_os_string());
        }
        current = PathBuf::new();
        while let Some(part) = ahead.pop_back() {
            pending.push_front(part);
        }
    }
    false
}

/// Whether `path` starts with a Windows prefix that reaches beyond the local disks.
fn is_remote_namespace(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    match path.components().next() {
        Some(Component::Prefix(prefix)) => matches!(
            prefix.kind(),
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..) | Prefix::Verbatim(..)
        ),
        _ => false,
    }
}

/// `path` with `.` dropped and `..` folded, using only its text. A `..` at the root
/// stays at the root.
fn lexically_normalized(path: &Path) -> Vec<std::ffi::OsString> {
    use std::path::Component;
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    let mut rooted = 0;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                out.push(component.as_os_str().to_ascii_lowercase());
                rooted = out.len();
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if out.len() > rooted {
                    out.pop();
                }
            }
            Component::Normal(name) => out.push(name.to_ascii_lowercase()),
        }
    }
    out
}

/// Whether `path` sits at or beneath `dir` once both are normalized lexically and
/// compared without regard to ASCII case.
fn lexically_within(path: &Path, dir: &Path) -> bool {
    lexically_normalized(path).starts_with(&lexically_normalized(dir))
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
    /// Fetched by `crate::imagefetch` and decoded with `gdk::Texture::from_bytes`
    /// — *not* through `gio::File::for_uri`, which needs a GVfs http backend that
    /// only the Linux desktop has (GTK4Rs/AP-292).
    Remote(String),
    /// Blocked by the safety policy: a remote URL with `allow_unsafe_images`
    /// false, or a local path that **exists** and escapes the document folder. The
    /// renderer shows a broken-image placeholder icon and suppresses the alt text.
    /// Reserved for a reference the "Show Unsafe Images" toggle could actually
    /// admit — the placeholder says so, so anything else here misdirects the reader
    /// into lifting a gate that was never what stopped them.
    Refused,
    /// A path into a remote or device namespace outside the document's folder — a
    /// Windows `\\host\share` — which is never loaded, whatever Show Unsafe Images says:
    /// reaching it opens an SMB session that authenticates with the user's credentials,
    /// which neither toggle describes consent to. Decided from the text alone, so
    /// nothing touched the network to answer.
    NetworkShare,
    /// Cannot resolve: no `doc_dir` for a relative path, or the file simply
    /// does not exist (canonicalize error) — including a path that would be
    /// perfectly *contained* if anything were there. The renderer shows a
    /// broken-image placeholder with an "Image not found" tooltip (not bare alt
    /// text) — see `renderer::image_placeholder_tooltip`.
    ///
    /// **By design, this answer is a point-in-time disk check and nothing watches
    /// the path afterwards.** An image file that lands *after* the render that
    /// referenced it therefore stays a placeholder until something re-renders the
    /// preview (reload, view-mode switch, zoom, theme change, the Show Unsafe Images
    /// toggle) — which is reachable in ordinary use, since a checkout, an `rsync` or
    /// a static-site generator can write the document a few milliseconds before its
    /// pictures, and the live-reload path re-renders the moment the *document*
    /// lands. Stated here because it is a deliberate limitation rather than an
    /// oversight: self-healing is a larger change (the renderer would have to report
    /// which local candidates resolved to nothing, the tab would hold a
    /// `gio::FileMonitor` per distinct absent path — GIO does monitor a path that
    /// does not exist yet and reports `Created` — and the handler would re-render in
    /// place), and it wants a TDD rubric of its own before it is built.
    Missing,
}

/// Resolve an image `src` respecting the "Show Unsafe Images" opt-in toggle.
///
/// When `allow_unsafe_images` is **false** (the default, VULN-002), the existing
/// containment gate is enforced: local paths must stay at or beneath the document
/// folder (`containment_of`), and remote http/https URLs are `Refused`. A local path
/// the gate turns down is `Refused` only when a file is really there to refuse;
/// otherwise it is `Missing` (GTK4Rs/AP-34).
///
/// When `allow_unsafe_images` is **true**, the containment gate is lifted for
/// local paths (still canonicalized so symlinks resolve to their real target)
/// and remote http/https URLs are returned as `Remote` for the renderer to fetch
/// over HTTP (`crate::imagefetch`). All other URL schemes remain `Refused`
/// regardless of the flag (no ftp://, file://, smb://, etc.).
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
    // resolution below (GEP-46). A hierarchical `file://`/`smb://`
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

    // Local path (no scheme). A remote namespace is refused before either branch, so
    // the opt-in cannot reach it (`ImageResolution::NetworkShare`).
    if reaches_a_foreign_remote_namespace(src, doc_dir) {
        return ImageResolution::NetworkShare;
    }
    if allow_unsafe_images {
        // Containment gate lifted: canonicalize and admit any existing path.
        // `local_candidate` tilde-expands and supplies doc_dir as the base for a
        // still-relative path — the same preamble the link resolver uses, shared
        // so the two content types agree on what a path means.
        match canonical_local_target(src, doc_dir) {
            Some(abs) => ImageResolution::Local(abs),
            None => ImageResolution::Missing,
        }
    } else {
        // Enforce the containment gate (VULN-002), and report the reason it gave.
        // "Refused" and "unresolvable" are different answers to the reader — the
        // first says the safety policy stopped this, the second says there is
        // nothing here — and the distinction drives the tooltip, not whether an icon
        // appears (§14.9). `containment_of` owns it for both content types; see its
        // doc comment for what collapsing them cost (GTK4Rs/AP-34).
        //
        // Every case the old special-casing here handled falls out of that verdict:
        // an untitled buffer (no `doc_dir`) has nothing to resolve a relative `src`
        // against, and an absolute path with no `doc_dir` has no base to be inside
        // of, so it can only ever be `Escapes` or `Absent` — never wrongly admitted.
        match containment_of(src, doc_dir) {
            Containment::Inside(path) => ImageResolution::Local(path),
            Containment::Escapes => ImageResolution::Refused,
            Containment::Absent => ImageResolution::Missing,
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
    /// A path into a remote or device namespace outside the document's folder, never
    /// opened whatever the toggle says. See [`ImageResolution::NetworkShare`].
    NetworkShare,
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
/// Reuses [`containment_of`] — and through it [`resolve_contained_image`] verbatim —
/// for the containment check itself (component-wise `Path::starts_with`,
/// canonicalize-first): same untrusted-content policy, same escape vectors
/// (`../secrets`, a `docs-evil/` sibling masquerading as `docs/`, a symlink under the
/// doc dir pointing outside), and the same refused-vs-unresolvable verdict an image
/// gets.
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
    if reaches_a_foreign_remote_namespace(path_part, Some(doc_dir)) {
        return LinkResolution::NetworkShare;
    }

    // Reuse the image gate's exact resolve-and-contain logic when containment
    // is enforced; when the toggle lifts it, still canonicalize (so a symlink
    // resolves to its real target and a genuinely-missing file is still
    // `Missing`, never a phantom `Navigate`) but skip the `starts_with` check.
    let target = if allow_outside_links {
        match canonical_local_target(path_part, Some(doc_dir)) {
            Some(t) => t,
            None => return LinkResolution::Missing,
        }
    } else {
        // "Exists but escapes the folder" (Refused) and "doesn't exist at all"
        // (Missing) are different things to tell the reader — the click handler has
        // to say WHY nothing happened rather than leave a click looking broken.
        // `containment_of` owns that distinction for links and images alike (it was
        // hand-rolled here and absent there, which is how the image side came to
        // report a safe-but-not-yet-present file as blocked by the safety policy).
        match containment_of(path_part, Some(doc_dir)) {
            Containment::Inside(t) => t,
            Containment::Escapes => return LinkResolution::Refused,
            Containment::Absent => return LinkResolution::Missing,
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

/// The reference to insert for a chosen local file (the Insert Link / Image Browse
/// buttons) — ONE function whatever the document's state, because the two concerns
/// in it are independent and must not be coupled by a caller's branch:
///
/// * **Where it is relative to.** With a `base` (the document's folder), the
///   `pathdiff::diff_paths` of the canonicalized target against the canonicalized
///   base, with forward slashes for portability; the absolute path when there is no
///   relative route (different roots) or no base at all (an untitled buffer). A
///   result that escapes the base (`../…`) is returned as-is — it simply won't pass
///   the image containment gate, which is the intended security behaviour.
/// * **Encoding.** Always, on the finished path, so the result is a valid Markdown
///   destination rather than one this app writes and then cannot read back. The
///   encoding once lived only on the based branch, so an untitled buffer's Browse
///   wrote `/path/with spaces.svg` raw — the space ends a destination and the image
///   rendered as literal text.
pub(crate) fn path_for_insert(target: &Path, base: Option<&Path>) -> String {
    let t = dunce::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let rel = base.and_then(|base| {
        let b = dunce::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
        pathdiff::diff_paths(&t, &b).map(|rel| rel.to_string_lossy().replace('\\', "/"))
    });
    let path = rel.unwrap_or_else(|| t.to_string_lossy().into_owned());
    percent_encode_path(&path)
}

#[cfg(test)]
mod tests {
    use super::{
        anchor_target, doc_link_fragment, is_allowed_url, is_exportable_href, lexically_within,
        path_for_insert, percent_decode, percent_encode_path, reaches_a_foreign_remote_namespace,
        resolve_contained_image, resolve_doc_link, resolve_image, scheme_of, ImageResolution,
        LinkResolution,
    };
    use std::path::Path;

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

        // Local paths that merely CONTAIN a colon are NOT schemes (GEP-46).
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

    /// **The regression the operator reproduced.** Browse used to hand its chosen file
    /// into the URL field verbatim, so a name with a space produced
    /// `![alt](/some/path/with spaces.svg)` — the space ends a Markdown destination, so
    /// the app rendered its own insertion as literal text. Mutation: dropping the
    /// `percent_encode_path` call in `path_for_insert` fails this.
    #[test]
    fn browse_inserts_a_destination_markdown_can_actually_parse() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let spaced = base.join("A file.svg");
        fs::write(&spaced, b"x").unwrap();

        let inserted = path_for_insert(&spaced, Some(base));
        assert_eq!(inserted, "A%20file.svg");
        assert!(
            !inserted.contains(' '),
            "a destination with a raw space does not parse as a link: {inserted}"
        );
        // And the round trip closes: what Browse writes is what the resolver reads.
        assert!(
            resolve_contained_image(&inserted, Some(base)).is_some(),
            "the app must be able to read back the destination it just wrote"
        );
    }

    /// The same guarantee with NO base — an untitled buffer, where Browse inserts the
    /// absolute path. Encoding once lived only on the based branch, so this case wrote
    /// the path raw and the inserted image rendered as literal text. The destination
    /// must carry no raw space and must decode back to the chosen file.
    #[test]
    fn browse_encodes_the_absolute_path_of_an_untitled_buffer() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Trust Pod");
        fs::create_dir(&folder).unwrap();
        let spaced = folder.join("A file.svg");
        fs::write(&spaced, b"x").unwrap();

        let inserted = path_for_insert(&spaced, None);
        assert!(!inserted.contains(' '), "raw space in {inserted}");
        assert!(
            inserted.ends_with("Trust%20Pod/A%20file.svg") || cfg!(windows),
            "{inserted}"
        );
        match resolve_image(&inserted, None, true) {
            ImageResolution::Local(p) => {
                assert_eq!(p, dunce::canonicalize(&spaced).unwrap())
            }
            other => panic!("{inserted} must resolve to the chosen file, got {other:?}"),
        }
    }

    /// The encoder touches what breaks Markdown or URL syntax and nothing else — the
    /// separator and non-ASCII stay readable, so the URL field does not fill with noise.
    #[test]
    fn the_insert_encoder_is_narrow_and_round_trips() {
        assert_eq!(percent_encode_path("sub/dir/a b.png"), "sub/dir/a%20b.png");
        assert_eq!(percent_encode_path("caf\u{e9}.png"), "caf\u{e9}.png");
        assert_eq!(percent_encode_path("a(b).png"), "a%28b%29.png");
        // `%` is encoded, which is what makes the round trip lossless.
        assert_eq!(percent_encode_path("100%.png"), "100%25.png");
        for name in [
            "A file.svg",
            "100%.png",
            "a(b) [c].png",
            "caf\u{e9} \u{2014} notes.png",
            "with#hash?and.png",
        ] {
            assert_eq!(
                percent_decode(&percent_encode_path(name)),
                name,
                "encode/decode must round trip: {name}"
            );
        }
    }

    /// A hand-written or GitHub-authored `%20` destination resolves, and so does the
    /// pathological file whose name really contains the escape text — the decoded
    /// candidate is tried first, the literal second.
    #[test]
    fn a_percent_encoded_destination_resolves_and_so_does_a_literal_one() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        fs::write(base.join("A file.svg"), b"x").unwrap();
        assert!(
            resolve_contained_image("A%20file.svg", Some(base)).is_some(),
            "an encoded destination must resolve to the spaced file"
        );

        // A file whose NAME contains the escape text, with no spaced twin: the literal
        // retry is the only thing that finds it.
        fs::write(base.join("B%20file.svg"), b"x").unwrap();
        assert!(
            resolve_contained_image("B%20file.svg", Some(base)).is_some(),
            "a file literally named with an escape sequence must still resolve"
        );
    }

    /// **Decoding must not widen the containment gate.** An encoded traversal decodes
    /// to `../` and is refused exactly as the raw form is — the gate canonicalizes
    /// whatever the decode produced and re-checks it, so escaping is not reachable by
    /// spelling the path differently.
    #[test]
    fn an_encoded_traversal_is_refused_like_a_raw_one() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        fs::write(outer.path().join("secret.png"), b"x").unwrap();

        assert!(
            resolve_contained_image("../secret.png", Some(&base)).is_none(),
            "raw traversal must be refused (control)"
        );
        assert!(
            resolve_contained_image("%2e%2e%2fsecret.png", Some(&base)).is_none(),
            "an encoded traversal must be refused too, or decoding is a gate bypass"
        );
        assert!(
            resolve_contained_image("..%2fsecret.png", Some(&base)).is_none(),
            "a partially encoded traversal must be refused as well"
        );
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

    /// A link UNDER the doc dir pointing OUTSIDE must be rejected: canonicalize
    /// resolves it to its outside target, which fails containment.
    ///
    /// The fixture is whatever reparse point this host allows — a file symlink, or an
    /// NTFS directory junction on a Windows box without Developer Mode, where the
    /// symlink arm cannot run at all. The limb is named for the GUARANTEE rather than
    /// for the mechanism, because the mechanism now varies by host and a skip line
    /// saying "symlink" would misdescribe the box it ran on.
    #[test]
    fn resolve_rejects_symlink_escape() {
        use crate::testsymlink::escaping_reference_or_skip;
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let secret = outer.path().join("secret.png");
        fs::write(&secret, b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        let Ok(escaping) = escaping_reference_or_skip(&base, &secret, "TDD 2.7 image containment")
        else {
            return;
        };
        assert!(
            resolve_contained_image(&escaping, Some(&base)).is_none(),
            "{escaping:?} reads through to a file outside the document folder and must \
             not resolve"
        );
        // Anti-vacuity: an ordinary contained image in the same folder still resolves,
        // so the refusal is about containment and not about the reference's shape.
        fs::write(base.join("ok.png"), b"x").unwrap();
        assert!(resolve_contained_image("ok.png", Some(&base)).is_some());
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
        // A same-directory file whose NAME contains a colon — the case that was
        // reported broken.
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
        // A real file the `../` traversal reaches — `Refused` is a statement about a
        // file that IS there, so the fixture has to put one there (before, this
        // asserted `Refused` over a path with nothing behind it, and so would have
        // passed just as well if the gate had refused everything unconditionally).
        fs::write(outer.path().join("secret.png"), b"x").unwrap();
        assert!(matches!(
            resolve_image("../secret.png", Some(&base), false),
            ImageResolution::Refused
        ));
    }

    /// GTK4Rs/AP-34, the part its original fix did not reach: with the containment gate
    /// ON, a reference that is perfectly *contained* but has no file behind it yet
    /// must read as **not found**, never as blocked by the safety policy.
    ///
    /// The live-reload path is how this surfaced. A document and its images arrive
    /// together (a checkout, a sync, a generator, or a paste-then-copy), the reload
    /// re-renders the instant the document lands, and an image that loses that race by
    /// a frame was reported blocked — with a tooltip inviting the reader to turn the
    /// safety gate off to "fix" an image the gate had never objected to. The render is
    /// a snapshot either way; the reason it gives is the only part that can be honest
    /// about a file that simply has not landed yet.
    #[test]
    fn resolve_image_reports_a_contained_absent_file_as_missing_not_refused() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        for src in ["late.png", "./late.png", "sub/late.png"] {
            let verdict = resolve_image(src, Some(dir.path()), false);
            assert!(
                matches!(verdict, ImageResolution::Missing),
                "a contained but absent `{src}` must be Missing (\"Image not found\"), \
                 not Refused (\"enable Show Unsafe Images\"), got {verdict:?}"
            );
        }

        // And the same reference resolves once the file lands — the verdict tracks
        // the disk, so it is the render that is a snapshot, not the gate that is stuck.
        fs::write(dir.path().join("late.png"), b"x").unwrap();
        assert!(matches!(
            resolve_image("late.png", Some(dir.path()), false),
            ImageResolution::Local(_)
        ));
    }

    /// The companion to the escape test above: an escaping path with **nothing** at the
    /// end of it is unresolvable, not refused. Enabling "Show Unsafe Images" would not
    /// make it appear, so the placeholder must not send the reader to that toggle.
    #[test]
    fn resolve_image_reports_an_escaping_absent_path_as_missing() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        let verdict = resolve_image("../nothing-here.png", Some(&base), false);
        assert!(
            matches!(verdict, ImageResolution::Missing),
            "an escaping path with no file behind it is unresolvable, got {verdict:?}"
        );
    }

    /// A UNC image path in a document is refused from its text alone, so it never
    /// opens an SMB session (NTLM leak, main-thread stall). 203.0.113.0/24 is
    /// TEST-NET-3, unroutable: a canonicalize call on it would hang for the redirector
    /// timeout, so the elapsed time is the witness that no filesystem call was made.
    #[test]
    fn resolve_image_refuses_a_unc_path_without_touching_the_network() {
        if !cfg!(windows) {
            println!("SKIPPED [TDD 2.7]: UNC prefixes exist only in the Windows path grammar");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for src in [
            r"\\203.0.113.1\s\x.png",
            "//203.0.113.1/s/x.png",
            "%5C%5C203.0.113.1%5Cs%5Cx.png",
            r"\\?\UNC\203.0.113.1\s\x.png",
            r"\\203.0.113.1@SSL@443\DavWWWRoot\x.png",
        ] {
            let started = std::time::Instant::now();
            let calls_before = super::CANONICALIZE_CALLS.with(std::cell::Cell::get);
            // With each opt-in OFF and ON: neither toggle is consent to a network share.
            for unsafe_on in [false, true] {
                let verdict = resolve_image(src, Some(dir.path()), unsafe_on);
                assert!(
                    matches!(verdict, ImageResolution::NetworkShare),
                    "{src} (unsafe images {unsafe_on}): {verdict:?}"
                );
                let link = resolve_doc_link(src, Some(dir.path()), unsafe_on);
                assert!(
                    matches!(link, LinkResolution::NetworkShare),
                    "{src} (outside links {unsafe_on}): {link:?}"
                );
            }
            assert_eq!(
                super::CANONICALIZE_CALLS.with(std::cell::Cell::get),
                calls_before,
                "{src}: the refusal must not resolve anything on the filesystem"
            );
            assert!(
                started.elapsed() < std::time::Duration::from_secs(1),
                "{src} took {:?}, so something reached for the network",
                started.elapsed()
            );
        }
    }

    /// A document opened from a share keeps its own images: a candidate lexically
    /// under the document's folder is not a foreign namespace, and `..` cannot fold
    /// a candidate out to a sibling share without being caught.
    #[test]
    fn a_share_hosted_document_is_not_foreign_to_itself() {
        if !cfg!(windows) {
            println!("SKIPPED [TDD 2.7]: UNC prefixes exist only in the Windows path grammar");
            return;
        }
        let dir = Path::new(r"\\files\team\docs");
        assert!(!reaches_a_foreign_remote_namespace("img/x.png", Some(dir)));
        assert!(!reaches_a_foreign_remote_namespace(
            r"\\FILES\Team\docs\x.png",
            Some(dir)
        ));
        assert!(reaches_a_foreign_remote_namespace(
            r"..\..\other\x.png",
            Some(dir)
        ));
        assert!(reaches_a_foreign_remote_namespace(
            r"\\evil\s\x.png",
            Some(dir)
        ));
        assert!(reaches_a_foreign_remote_namespace(r"\\evil\s\x.png", None));
    }

    /// The lexical fold the remote check relies on, on every platform's own grammar.
    #[test]
    fn lexical_containment_folds_dots_and_ignores_ascii_case() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("Doc");
        assert!(lexically_within(
            &base.join("a").join("..").join("x.png"),
            &base
        ));
        assert!(lexically_within(
            &base.join(".").join("x.png"),
            &dir.path().join("doc")
        ));
        assert!(!lexically_within(&base.join("..").join("x.png"), &base));
        assert!(!lexically_within(
            &dir.path().join("Doc-evil").join("x.png"),
            &base
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
        assert_eq!(path_for_insert(&target, Some(base)), "sub/y.png");
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
    use crate::testsymlink::escaping_reference_or_skip;

    #[test]
    fn doc_link_refuses_symlink_escape_when_toggle_off() {
        use std::fs;
        let outer = tempfile::tempdir().unwrap();
        let secrets = outer.path().join("secrets.md");
        fs::write(&secrets, b"x").unwrap();
        let base = outer.path().join("doc");
        fs::create_dir(&base).unwrap();
        let Ok(escaping) = escaping_reference_or_skip(&base, &secrets, "TDD 19.2 link containment")
        else {
            return;
        };
        assert!(
            matches!(
                resolve_doc_link(&escaping, Some(&base), false),
                LinkResolution::Refused
            ),
            "a link under the document folder pointing outside it must be refused on \
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
        let Ok(escaping) = escaping_reference_or_skip(&base, &target, "TDD 19.3 link resolution")
        else {
            return;
        };
        let want = dunce::canonicalize(&target).unwrap();
        assert!(
            matches!(
                resolve_doc_link(&escaping, Some(&base), true),
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

    /// `doc_link_fragment` documents a schemeless precondition and does not enforce
    /// one — this pins that, so a future reader does not mistake it for a gate and
    /// rebuild the hole `is_exportable_href` closes.
    #[test]
    fn doc_link_fragment_does_not_enforce_its_own_schemeless_precondition() {
        assert_eq!(
            doc_link_fragment("javascript:alert(1)//#x").as_deref(),
            Some("x")
        );
    }

    // ── export href gate ────────────────────────────────────────────────────────

    #[test]
    fn is_exportable_href_refuses_a_scheme_the_two_parsers_disagree_about() {
        // Every one of these is malformed by RFC 3986 and REPAIRED by the
        // WHATWG URL Standard, which strips leading C0/space and removes every tab, LF
        // and CR anywhere in the URL before parsing the scheme. Inferring "relative"
        // from "my scheme parser did not recognise this" admitted all of them.
        for href in [
            "\tjavascript:alert(1)//#x",
            " javascript:alert(1)//#x",
            "\njavascript:alert(1)//#x",
            "\rjavascript:alert(1)//#x",
            "\u{0}javascript:alert(1)//#x",
            "java\tscript:alert(1)//#x",
            "java\nscript:alert(1)//#x",
            "jav\u{0}ascript:alert(1)//#x",
            "javascript\t:alert(1)//#x",
            " \t javascript:alert(1)#x",
        ] {
            assert!(
                !is_exportable_href(href),
                "a destination the reader's parser repairs into a scheme: {href:?}"
            );
        }
    }

    /// A protocol-relative destination is not a relative reference: it inherits the
    /// artefact's scheme. From a `file://` page that is a filesystem fetch, and on
    /// Windows `//host/share` is a UNC path reaching SMB.
    #[test]
    fn is_exportable_href_refuses_a_protocol_relative_destination() {
        assert!(!is_exportable_href("//evil.example/#x"));
        assert!(!is_exportable_href("//evil.example/path#frag"));
        assert!(!is_exportable_href("//#x"));
    }

    #[test]
    fn is_exportable_href_refuses_a_hostile_scheme_that_carries_a_fragment() {
        // The vulnerability, at the unit that decides it: the `//` comments out the
        // `#x` for the engine, but the fragment limb matched on it.
        assert!(!is_exportable_href("javascript:alert(1)//#x"));
        assert!(!is_exportable_href("javascript:alert(1)#x"));
        assert!(!is_exportable_href("JaVaScRiPt:alert(1)#x"));
        assert!(!is_exportable_href(
            "data:text/html,<script>alert(1)</script>#x"
        ));
        assert!(!is_exportable_href("file:///etc/passwd#x"));
        assert!(!is_exportable_href("smb://host/share#x"));
    }

    #[test]
    fn is_exportable_href_keeps_the_links_an_artefact_can_still_resolve() {
        assert!(is_exportable_href("https://example.com"));
        assert!(is_exportable_href("https://example.com/page#frag"));
        assert!(is_exportable_href("HTTPS://example.com"));
        assert!(is_exportable_href("mailto:user@example.com"));
        assert!(is_exportable_href("TECH.md#module-map"));
        assert!(is_exportable_href("./sub/PLAN.md#caf%C3%A9"));
        assert!(is_exportable_href("#section"));
        // Unchanged from before the gate: a relative link with no fragment has never
        // been emitted with a destination.
        assert!(!is_exportable_href("TECH.md"));
        assert!(!is_exportable_href("TECH.md#"));
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

    /// A symbolic link inside the document folder whose target is foreign is caught
    /// from the link's text, before anything follows it — directly, through a chain of
    /// links, and not for a link to an ordinary local folder. The foreign predicate is
    /// a stand-in path here, so the walk is exercised on every platform; on Windows the
    /// real predicate is a UNC prefix.
    ///
    /// Mutation: follow the link with `metadata` instead of reading it, or skip the
    /// target check, and the first two assertions fail.
    #[test]
    fn a_link_to_a_foreign_target_is_caught_without_following_it() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("doc");
        std::fs::create_dir(&doc).unwrap();
        let local = dir.path().join("local");
        std::fs::create_dir(&local).unwrap();
        let fake_remote = Path::new("/scribo-fake-remote-share");
        // Matched by component, not prefix: on Windows the driveless stand-in is not
        // absolute, so the walk joins it onto the current drive (`C:\scribo-…`), and a
        // prefix test would only match while that drive was being wrongly dropped.
        let is_foreign = |p: &Path| {
            p.components()
                .any(|c| c.as_os_str() == "scribo-fake-remote-share")
        };
        #[cfg(unix)]
        let link = |target: &Path, at: &Path| std::os::unix::fs::symlink(target, at);
        #[cfg(windows)]
        let link = |target: &Path, at: &Path| std::os::windows::fs::symlink_dir(target, at);
        if link(fake_remote, &doc.join("lnk")).is_err() {
            println!("SKIPPED [TDD 2.7]: this host cannot create a symbolic link here");
            return;
        }
        link(Path::new("lnk"), &doc.join("chain")).unwrap();
        // An ABSOLUTE intermediate: its re-walk must keep the target's drive on Windows.
        link(&doc.join("lnk"), &doc.join("chain_abs")).unwrap();
        link(&local, &doc.join("near")).unwrap();
        let reaches = |rel: &str| {
            super::links_reach_foreign(&doc.join(rel), Some(&doc), &is_foreign, &|_: &Path| false)
        };
        assert!(
            reaches("lnk/x.png"),
            "a link straight to the foreign target"
        );
        assert!(reaches("chain/x.png"), "a link to a link to it");
        assert!(
            reaches("chain_abs/x.png"),
            "a link to a link to it, by absolute path"
        );
        assert!(!reaches("near/x.png"), "a link to an ordinary local folder");
        assert!(!reaches("absent/x.png"), "a path that does not exist");
    }

    /// A chain longer than the walk will follow is refused, not let through: Windows
    /// follows up to 63 reparse points, so one ending at a share would otherwise pass
    /// the walk and be reached by `canonicalize`. Exactly [`super::MAX_LINK_HOPS`] links
    /// to a local folder still resolve as local.
    ///
    /// Mutation: return `false` past the hop limit and the first assertion fails.
    #[test]
    fn a_link_chain_past_the_hop_limit_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        // CANONICAL, because every hop re-walks its resolved target from the root and
        // counts each link on the way, as the OS does: on macOS the temp dir is under
        // `/var`, a link to `/private/var`, which added hops the count below does not
        // expect (MEASURED on macOS).
        let root = dunce::canonicalize(dir.path()).unwrap();
        let doc = root.join("doc");
        std::fs::create_dir(&doc).unwrap();
        let local = root.join("local");
        std::fs::create_dir(&local).unwrap();
        let never_foreign = |_: &Path| false;
        #[cfg(unix)]
        let link = |target: &Path, at: &Path| std::os::unix::fs::symlink(target, at);
        #[cfg(windows)]
        let link = |target: &Path, at: &Path| std::os::windows::fs::symlink_dir(target, at);
        // l0 -> local, l<n> -> l<n-1>: following l<n> takes n + 1 hops.
        if link(&local, &doc.join("l0")).is_err() {
            println!("SKIPPED [TDD 2.7]: this host cannot create a symbolic link here");
            return;
        }
        for n in 1..=super::MAX_LINK_HOPS {
            link(
                Path::new(&format!("l{}", n - 1)),
                &doc.join(format!("l{n}")),
            )
            .unwrap();
        }
        let reaches = |rel: &str| {
            super::links_reach_foreign(&doc.join(rel), Some(&doc), &never_foreign, &|_: &Path| {
                false
            })
        };
        let over = format!("l{}/x.png", super::MAX_LINK_HOPS);
        let at = format!("l{}/x.png", super::MAX_LINK_HOPS - 1);
        assert!(reaches(&over), "one link past the limit is refused");
        assert!(
            !reaches(&at),
            "a chain at the limit to a local folder is local"
        );
    }

    /// A path through an automounter's directory is refused before the walk looks up
    /// anything inside it: there, the lookup itself contacts the host (macOS `/net`).
    /// The host component does not exist here, so a walk that looked would find nothing
    /// and answer "local"; only the refusal-before-lookup answers "foreign".
    ///
    /// Mutation: drop the `mounts_on_lookup` check and both assertions fail.
    #[test]
    fn a_path_into_an_automount_directory_is_refused_without_looking_inside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let doc = root.join("doc");
        std::fs::create_dir(&doc).unwrap();
        let net = root.join("net");
        std::fs::create_dir(&net).unwrap();
        let never_foreign = |_: &Path| false;
        let is_net = |p: &Path| p == net.as_path();
        assert!(
            super::links_reach_foreign(
                &net.join("host/x.png"),
                Some(&doc),
                &never_foreign,
                &is_net
            ),
            "an absolute path into the automount directory is refused"
        );
        #[cfg(unix)]
        let link = |target: &Path, at: &Path| std::os::unix::fs::symlink(target, at);
        #[cfg(windows)]
        let link = |target: &Path, at: &Path| std::os::windows::fs::symlink_dir(target, at);
        if link(&net.join("host"), &doc.join("lnk")).is_err() {
            println!("SKIPPED [TDD 2.7]: this host cannot create a symbolic link here");
            return;
        }
        assert!(
            super::links_reach_foreign(&doc.join("lnk/x.png"), Some(&doc), &never_foreign, &is_net),
            "a link in the document's folder into the automount directory is refused"
        );
        assert!(
            !super::links_reach_foreign(&doc.join("x.png"), Some(&doc), &never_foreign, &is_net),
            "control: the document's own folder is not affected"
        );
    }
}
