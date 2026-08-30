//! Remote image fetch — the HTTP(S) half of the "Show Unsafe Images" path.
//!
//! ## Why this exists rather than `gio::File::for_uri(url)`
//!
//! Handing an `https://` URI to GIO looks like it costs no dependency: `GFile`
//! takes a URI, `GdkTexture::from_file` takes a `GFile`, and on a GNOME desktop
//! it works. What it actually does is ask **GVfs** to resolve the scheme, and
//! the process that claims `http`/`https` is `gvfsd-http`, a separate daemon
//! that ships with the Linux desktop stack and **has no macOS build at all** —
//! Homebrew has no `gvfs` formula. So the same call that renders a README banner
//! on Linux answers `Operation not supported` on macOS, for every remote image,
//! with no log line (measured with GLib's own `gio info https://…`, i.e. with
//! this application out of the picture). It is not fixable by installing
//! anything (ScrAP-292).
//!
//! The scheme's availability is therefore a property of the *host's desktop
//! stack*, not of the toolkit the project depends on — the same class of hidden
//! platform requirement as the D-Bus session bus behind single-instance
//! activation, and answered the same way: supply the transport ourselves and
//! hand off to the machinery every platform already shares. An explicit HTTP GET
//! into a byte buffer plus `GdkTexture::from_bytes` reaches the identical
//! `GdkPixbuf` loader chain the file path would have (GTK4Rs/AP-66, ScrAP-146),
//! so *what decodes* is unchanged; only *who fetched the bytes* is.
//!
//! ## What this module does not change
//!
//! The fetch still runs synchronously on the GTK main thread, which is ScrAP-34's 34a half
//! and remains accepted for this opt-in path — but it is now **bounded**, where
//! GVfs offered no timeout at all: [`CONNECT_TIMEOUT`] and [`GLOBAL_TIMEOUT`]
//! cap the freeze, and [`limits::MAX_REMOTE_IMAGE_BYTES`] caps the allocation.
//! The policy decision of *whether* a URL may be fetched is not made here at
//! all: `links::resolve_image` makes it, and only ever produces
//! `ImageResolution::Remote` for an `http`/`https` URL with the toggle on. The
//! scheme test below is a seam guard against a future caller, not the gate.

use crate::limits;
use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

/// How long to wait for the TCP+TLS connection before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the whole request may take, connection included. This is the number
/// that bounds the main-thread freeze (ScrAP-34, its 34a half), so it is deliberately short
/// enough to be survivable rather than generous enough for a large download over
/// a bad link — an image that cannot arrive in this long renders as the ordinary
/// "Could not load image" placeholder, which is a better outcome than a window
/// that stops repainting.
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(15);

/// Redirects to follow. Badge and asset URLs in real documents redirect once or
/// twice (`github.com` → a CDN); a chain longer than this is a loop or a tarpit.
const MAX_REDIRECTS: u32 = 5;

/// Sent as `User-Agent`. Some CDNs answer a request with no product token with a
/// 403, so this is functional rather than cosmetic.
const USER_AGENT: &str = concat!("Scribobulate/", env!("CARGO_PKG_VERSION"));

/// The schemes this module will fetch. Kept as a list rather than an `is_https`
/// test so it says the same thing `links::resolve_image` does.
const FETCHABLE_SCHEMES: &[&str] = &["http", "https"];

/// Why a remote image did not produce bytes. Separate variants because they are
/// separate stories for whoever reads the log: a 404 is the document's fault, a
/// transport error is the network's, and `TooLarge` is a refusal *we* made.
#[derive(Debug)]
pub(crate) enum FetchError {
    /// The URI carries a scheme this module does not fetch.
    UnsupportedScheme,
    /// The connection, TLS handshake, or transfer failed (also: timed out).
    Transport(String),
    /// The server answered, with a status that is not a success.
    Status(u16),
    /// The response body reached [`limits::MAX_REMOTE_IMAGE_BYTES`] and was
    /// abandoned unread.
    TooLarge,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::UnsupportedScheme => write!(f, "unsupported URL scheme"),
            FetchError::Transport(msg) => write!(f, "transport error: {msg}"),
            FetchError::Status(code) => write!(f, "HTTP status {code}"),
            FetchError::TooLarge => write!(
                f,
                "image exceeds the {} MiB remote-image limit",
                limits::MAX_REMOTE_IMAGE_BYTES / (1024 * 1024)
            ),
        }
    }
}

/// The process-wide agent. Built once: a `rustls` client configuration parses the
/// bundled root store, which is not work to repeat per image in a document that
/// carries twenty of them. `ureq::Agent` is internally shared and its connection
/// pool is the reason a document's second image from the same host is faster.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(GLOBAL_TIMEOUT))
            .max_redirects(MAX_REDIRECTS)
            .user_agent(USER_AGENT)
            // Verify against the MACHINE's trust store, not the bundled Mozilla
            // roots this crate defaults to. Both routes replaced here already did:
            // Linux fetched through glib-networking/GnuTLS, Windows through GLib's
            // in-process `GWinHttpVfs` on WinHTTP. Defaulting to `WebPki` would
            // therefore have broken remote images on every desktop behind a
            // TLS-inspecting proxy or carrying a private root — machines where the
            // feature worked before this change — which is a regression on two
            // platforms delivered by a fix for a third, and the hardest kind to
            // attribute afterwards. The proxy half is the `win-system-proxy`
            // feature (env vars first, then the per-user Windows settings).
            //
            // One deliberate BEHAVIOUR CHANGE, not a preservation: measured on
            // Windows, the route this replaces ignored `HTTPS_PROXY`/`ALL_PROXY`
            // entirely (WinHTTP reads system configuration only), so a machine
            // that sets those for other tooling will now route image fetches
            // through them. Kept, because one proxy rule across all three
            // platforms is worth more than bug-for-bug fidelity to a backend
            // that is being removed.
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent()
    })
}

/// Whether this module will fetch `uri` — a scheme test over the *shared*
/// `links::scheme_of`, so a path that merely contains a colon is not mistaken
/// for a URL here either (ScrAP-151).
pub(crate) fn is_fetchable(uri: &str) -> bool {
    crate::links::scheme_of(uri)
        .is_some_and(|scheme| FETCHABLE_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()))
}

/// GET `uri` and return the response body, or the reason there is none.
///
/// **Blocks the calling thread** for up to [`GLOBAL_TIMEOUT`]; see the module
/// comment for why that is accepted here and what bounds it.
pub(crate) fn fetch_image_bytes(uri: &str) -> Result<Vec<u8>, FetchError> {
    fetch_capped(uri, limits::MAX_REMOTE_IMAGE_BYTES)
}

/// [`fetch_image_bytes`] with the byte cap supplied rather than read from
/// [`limits`] — the production caller passes the limit, and a test passes a small
/// one so the refusal boundary is exercised over a real HTTP response instead of
/// 16 MiB of loopback traffic.
fn fetch_capped(uri: &str, cap: usize) -> Result<Vec<u8>, FetchError> {
    if !is_fetchable(uri) {
        return Err(FetchError::UnsupportedScheme);
    }
    let mut response = agent().get(uri).call().map_err(|err| match err {
        // `ureq` reports a non-success status as an error by default; keep it a
        // status, because "the server said 404" and "the server never answered"
        // are different things to anyone reading the log.
        ureq::Error::StatusCode(code) => FetchError::Status(code),
        other => FetchError::Transport(other.to_string()),
    })?;
    read_capped(&mut response.body_mut().as_reader(), cap)
}

/// Read at most `cap` bytes, refusing anything longer.
///
/// Reads `cap + 1` so that "exactly at the cap" is admitted and the first byte
/// past it is a refusal — a `take(cap)` alone cannot tell a body that ended from
/// one that was truncated, and would hand a half-image to the decoder as if it
/// were whole. Generic over the reader (rather than taking `ureq`'s) so the
/// boundary behaviour is testable with no network.
fn read_capped<R: Read>(reader: &mut R, cap: usize) -> Result<Vec<u8>, FetchError> {
    let mut buf = Vec::new();
    reader
        .take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|err| FetchError::Transport(err.to_string()))?;
    if buf.len() > cap {
        return Err(FetchError::TooLarge);
    }
    Ok(buf)
}

#[cfg(test)]
mod imagefetch_tests {
    use super::{fetch_capped, fetch_image_bytes, is_fetchable, read_capped, FetchError};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serve one canned HTTP response on a loopback port and return its URL.
    ///
    /// A real server rather than a mock because the thing worth testing is that
    /// *this application's own HTTP client* reaches an image, which is the whole
    /// point of ScrAP-292 — a mock would re-test the mock. Loopback keeps it
    /// deterministic and offline: the suite must never depend on a host being up.
    fn serve_once(status_line: &str, body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let head = format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n",
            body.len()
        );
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Drain the request line/headers so the client is not writing
                // into a closed socket while we answer.
                let mut scratch = [0u8; 2048];
                let _ = stream.read(&mut scratch);
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        format!("http://{addr}/image.png")
    }

    #[test]
    fn fetches_a_body_over_http() {
        let url = serve_once("200 OK", b"\x89PNG\r\n\x1a\nnot-really".to_vec());
        assert_eq!(
            fetch_image_bytes(&url).expect("fetch"),
            b"\x89PNG\r\n\x1a\nnot-really"
        );
    }

    #[test]
    fn a_non_success_status_is_reported_as_a_status() {
        // Not folded into `Transport`: "the server said 404" and "the server
        // never answered" are different stories in the log.
        let url = serve_once("404 Not Found", b"nope".to_vec());
        assert!(matches!(
            fetch_image_bytes(&url),
            Err(FetchError::Status(404))
        ));
    }

    #[test]
    fn a_response_past_the_cap_is_refused_rather_than_truncated() {
        let url = serve_once("200 OK", vec![b'x'; 64]);
        assert!(matches!(fetch_capped(&url, 16), Err(FetchError::TooLarge)));
    }

    #[test]
    fn a_refused_connection_is_a_transport_error() {
        // Bind then drop, so the port is real, unused and refuses immediately.
        let addr = TcpListener::bind("127.0.0.1:0")
            .expect("bind")
            .local_addr()
            .expect("addr");
        let url = format!("http://{addr}/image.png");
        assert!(matches!(
            fetch_image_bytes(&url),
            Err(FetchError::Transport(_))
        ));
    }

    #[test]
    fn a_non_http_scheme_never_reaches_the_network() {
        assert!(matches!(
            fetch_image_bytes("file:///etc/passwd"),
            Err(FetchError::UnsupportedScheme)
        ));
    }

    #[test]
    fn the_agent_verifies_against_the_platform_trust_store() {
        // Asserted on the CONFIGURATION, not on a successful fetch, because no
        // reachable host can tell the two root stores apart — a public
        // certificate chains in both, so every live check (and every screenshot)
        // is satisfied whichever one answered. Delete the explicit selection and
        // `ureq` silently reverts to its bundled Mozilla roots, breaking only the
        // machines behind a TLS-inspecting proxy that nobody here has. So this is
        // the only guard that can see the property at all.
        assert!(matches!(
            super::agent().config().tls_config().root_certs(),
            ureq::tls::RootCerts::PlatformVerifier
        ));
    }

    #[test]
    fn every_failure_says_what_happened() {
        assert_eq!(
            FetchError::UnsupportedScheme.to_string(),
            "unsupported URL scheme"
        );
        assert_eq!(
            FetchError::Transport("connection refused".into()).to_string(),
            "transport error: connection refused"
        );
        assert_eq!(FetchError::Status(503).to_string(), "HTTP status 503");
    }

    #[test]
    fn reads_a_body_under_the_cap_whole() {
        let mut src = &b"abcdef"[..];
        assert_eq!(read_capped(&mut src, 16).unwrap(), b"abcdef");
    }

    #[test]
    fn admits_a_body_of_exactly_the_cap() {
        // The boundary the `cap + 1` read exists for: at the cap is fine, one
        // byte past it is not — an off-by-one here silently truncates or refuses
        // a legitimate image.
        let mut src = &b"abcdef"[..];
        assert_eq!(read_capped(&mut src, 6).unwrap(), b"abcdef");
    }

    #[test]
    fn refuses_a_body_one_byte_over_the_cap() {
        let mut src = &b"abcdef"[..];
        assert!(matches!(
            read_capped(&mut src, 5),
            Err(FetchError::TooLarge)
        ));
    }

    #[test]
    fn only_http_and_https_are_fetchable() {
        assert!(is_fetchable("http://example.com/i.png"));
        assert!(is_fetchable("https://example.com/i.png"));
        // Case-insensitive per RFC 3986.
        assert!(is_fetchable("HTTPS://example.com/i.png"));
        // Everything else, including the schemes `resolve_image` refuses.
        assert!(!is_fetchable("file:///etc/passwd"));
        assert!(!is_fetchable("smb://host/share/i.png"));
        assert!(!is_fetchable("ftp://host/i.png"));
        // A local path that merely contains a colon is not a URL (ScrAP-151).
        assert!(!is_fetchable("assets/notes:v2.png"));
        assert!(!is_fetchable("/home/user/i.png"));
    }

    #[test]
    fn too_large_names_the_limit_in_mib() {
        assert_eq!(
            FetchError::TooLarge.to_string(),
            "image exceeds the 16 MiB remote-image limit"
        );
    }
}
