//! The swap file's frontmatter codec.
//!
//! ```text
//! +++scribobulate-swap 1          <- opening fence: magic + format version
//! doc_id = "3f2a…c91"             <- TOML
//! path = "/home/u/Documents/notes.md"
//! untitled = false
//! …
//! +++                             <- closing fence, alone on its line
//! <the buffer's bytes, verbatim, to EOF>
//! ```
//!
//! # Why frontmatter, and why this spelling
//!
//! The decisive argument is drift: nothing transactionally couples the swap directory to
//! the session file (see the module doc's *self-sufficiency* principle), so a swap file
//! has to be able to say what it is on its own. The second payoff is manual
//! recoverability — if the application will not start, or the recovery path itself is
//! what is broken, a swap file opens in any editor (including this one) and states its
//! provenance in the first few lines. A length-prefixed binary header is unambiguous to a
//! parser and a puzzle to a human holding the only remaining copy of their work.
//!
//! Three spelling choices, each load-bearing:
//!
//! - **`+++` fences, not `---`.** The payload is Markdown, where a `---` line is both a
//!   thematic break and a setext `<h2>` underline — and the payload may itself be a
//!   document with its own YAML frontmatter. Parsing is safe either way (see the
//!   invariant below), but `---` makes the file ambiguous to *other* tools and to a human
//!   skimming it, and a truncated header would leave a body that still looks
//!   frontmatter-fenced. `+++` is the Hugo/Zola convention for TOML frontmatter and is
//!   vanishingly rare in prose.
//! - **TOML inside, not YAML.** The *idiom* is what is valuable, not the language, and
//!   TOML costs no new dependency: `toml` and `serde` already carry `config.toml` and
//!   `session.toml`, so the header speaks the project's existing metadata language
//!   instead of introducing a second one.
//! - **The magic is on the opening fence.** `+++scribobulate-swap 1` puts file-type
//!   identification and format version on the line that would otherwise carry no
//!   information.
//!
//! # The one invariant a delimited format needs
//!
//! A length prefix gets this for free; a delimiter has to earn it. Two halves:
//!
//! 1. **The terminator search is bounded.** The closing fence is looked for only within
//!    the first [`MAX_HEADER_BYTES`] / [`MAX_HEADER_LINES`]. Past that the file is
//!    malformed — we do not scan 64 MiB of payload hunting a fence. Content *after* the
//!    terminator is then irrelevant by construction: the body may contain `+++`, `---`,
//!    or a whole nested frontmatter block, because the scan has already stopped.
//! 2. **No header value may serialise to a bare `+++` line.** The live hazard is `path`:
//!    on unix a filename may legally contain a newline, so a crafted path could otherwise
//!    inject a fence and truncate the recovered document.
//!
//!    **This is not something the TOML serialiser gives us, and assuming it did was
//!    measurably wrong (ScrAP-233).** The reasoning that fails is seductive: *TOML basic
//!    strings escape `\n`, so a correct serialiser upholds the invariant automatically.*
//!    That describes the TOML `1.0` spec accurately and describes the `toml` **crate**
//!    not at all — given a value containing a newline it selects a **multi-line** basic
//!    string, whose whole purpose is to reproduce those newlines verbatim:
//!
//!    ```text
//!    path = """
//!    /tmp/evil
//!    +++
//!    not-the-body.md"""
//!    ```
//!
//!    That is a forged closing fence, and everything after it — the user's actual
//!    unsaved work — is silently discarded on recovery. The crate picks between basic,
//!    literal and multi-line forms by a heuristic that is its business and not our
//!    contract, so **no** delegated-escaping argument is safe here, not just this one.
//!
//!    So the invariant is enforced twice, by construction and then by verification:
//!
//!    - [`to_wire`] escapes every line-break out of every externally-derived string
//!      field *before* the value ever reaches the serialiser, so a multi-line string
//!      cannot be selected. [`from_wire`] reverses it on the way back.
//!    - [`encode`] then **verifies** the serialised header contains no bare fence line
//!      and fails loudly if it does. With the escaping in place that check is
//!      unreachable — which is exactly why it is a real `return Err` and not a
//!      `debug_assert`: its job is to convert a future change in someone else's
//!      escaping heuristic into a visible failure rather than a truncated document.
//!
//! # A file that is not ours is not ours
//!
//! The state directory is a shared place. A file whose first line does not match the
//! magic is **ignored, logged, and never deleted** — this mechanism must not become a
//! file shredder for anything that happens to land there. A file that *is* ours but
//! whose header will not parse (a torn write) is likewise kept, not deleted: it may be
//! the only remaining copy of the user's work and a human can still read it.

use super::{SwapHeader, DOC_ID_HEX_LEN};

/// Opening-fence prefix. The format version follows it on the same line.
const MAGIC: &str = "+++scribobulate-swap ";
/// The format version this build writes.
const FORMAT_VERSION: u32 = 1;
/// The closing fence, alone on its line.
const CLOSE_FENCE: &str = "+++";
/// Bound on the terminator search — see the module doc's invariant.
const MAX_HEADER_BYTES: usize = 8 * 1024;
/// Line bound on the terminator search, applied alongside [`MAX_HEADER_BYTES`].
const MAX_HEADER_LINES: usize = 64;

/// Why a swap file could not be decoded.
///
/// The distinction between the first variant and the rest is the whole point: `NotOurs`
/// means *leave this file completely alone*, while the others mean *this is our file and
/// it is damaged*. Collapsing them would make the recovery scan either a shredder or a
/// no-op.
#[derive(Debug, PartialEq)]
pub(crate) enum SwapDecodeError {
    /// The first line is not our magic. Somebody else's file, or not a file at all.
    NotOurs,
    /// Ours, but written by a format version this build does not understand.
    UnsupportedVersion(u32),
    /// Ours, but structurally damaged — no closing fence within the bound, unparseable
    /// TOML, or a body that is not valid UTF-8. Carries a reason for the log.
    Malformed(String),
}

impl std::fmt::Display for SwapDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOurs => write!(f, "not a Scribobulate swap file"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported swap format version {v}"),
            Self::Malformed(why) => write!(f, "malformed swap file: {why}"),
        }
    }
}

/// Escape line breaks (and the escape character itself) out of a string, so it cannot
/// select a multi-line TOML string and cannot carry a bare fence line.
///
/// The mapping is the conventional one — `\` → `\\`, newline → `\n`, carriage return →
/// `\r` — and it is applied to every string field the *outside world* supplies. Ours
/// (`doc_id`, `baseline_digest`, `app_version`) cannot contain a line break by
/// construction, but they cost nothing to route through the same function and doing so
/// removes the judgement call from the next person to add a field.
///
/// **A Windows path comes out with doubled backslashes** (`C:\\Users\\u\\notes.md`).
/// That is correct, round-trips exactly, and is *not* a bug to fix: the alternative — a
/// conditional escaping scheme that leaves backslashes alone unless a newline is present
/// — trades a cosmetic wart for a second code path through the one invariant that must
/// never have two.
fn escape_lines(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

/// Reverse [`escape_lines`]. An unrecognised escape is passed through verbatim rather
/// than rejected: a header we can *mostly* read is more useful than one we refuse, and
/// the only thing at stake is a filename's cosmetic accuracy.
fn unescape_lines(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The on-the-wire form of a header: every externally-derived string escaped so no raw
/// line break reaches the serialiser.
///
/// Written as an explicit field-by-field transform rather than a blanket one so that
/// adding a string field to [`SwapHeader`] does not compile until it has been considered
/// here.
fn to_wire(header: &SwapHeader) -> SwapHeader {
    let SwapHeader {
        doc_id,
        path,
        untitled,
        baseline_digest,
        written_at,
        owner_pid,
        app_version,
    } = header;
    SwapHeader {
        doc_id: doc_id.clone(),
        path: path.as_deref().map(escape_lines),
        untitled: *untitled,
        baseline_digest: escape_lines(baseline_digest),
        written_at: *written_at,
        owner_pid: *owner_pid,
        app_version: escape_lines(app_version),
    }
}

/// Reverse [`to_wire`].
fn from_wire(header: SwapHeader) -> SwapHeader {
    let SwapHeader {
        doc_id,
        path,
        untitled,
        baseline_digest,
        written_at,
        owner_pid,
        app_version,
    } = header;
    SwapHeader {
        doc_id,
        path: path.as_deref().map(unescape_lines),
        untitled,
        baseline_digest: unescape_lines(&baseline_digest),
        written_at,
        owner_pid,
        app_version: unescape_lines(&app_version),
    }
}

/// Serialise a header and body into a complete swap file.
pub(crate) fn encode(header: &SwapHeader, body: &str) -> Result<Vec<u8>, String> {
    let toml = toml::to_string(&to_wire(header)).map_err(|e| e.to_string())?;
    // The backstop half of the codec's one invariant — see the module doc. Unreachable
    // while `to_wire` holds, and a real error rather than a `debug_assert` precisely
    // because its purpose is to catch the day that stops being true, in a release build,
    // rather than to document an assumption.
    if toml.lines().any(|line| line.trim_end() == CLOSE_FENCE) {
        return Err(format!(
            "refusing to write a swap header whose serialised form contains a bare \
             `{CLOSE_FENCE}` line — it would forge the closing fence and truncate the \
             recovered document (ScrAP-233)"
        ));
    }
    let mut out = String::with_capacity(toml.len() + body.len() + MAGIC.len() + 16);
    out.push_str(MAGIC);
    out.push_str(&FORMAT_VERSION.to_string());
    out.push('\n');
    out.push_str(&toml);
    if !toml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(CLOSE_FENCE);
    out.push('\n');
    out.push_str(body);
    Ok(out.into_bytes())
}

/// Parse a complete swap file back into its header and body.
///
/// The body is returned verbatim — byte-identical to what [`encode`] was handed, which
/// is what makes a round trip lossless even when the document itself contains fences.
pub(crate) fn decode(bytes: &[u8]) -> Result<(SwapHeader, String), SwapDecodeError> {
    let text = std::str::from_utf8(bytes).map_err(|_| SwapDecodeError::NotOurs)?;

    let (first_line, rest) = split_line(text).ok_or(SwapDecodeError::NotOurs)?;
    let version_text = first_line
        .strip_prefix(MAGIC)
        .ok_or(SwapDecodeError::NotOurs)?;
    let version: u32 = version_text
        .trim()
        .parse()
        .map_err(|_| SwapDecodeError::NotOurs)?;
    if version != FORMAT_VERSION {
        // A FUTURE version is ours and readable by a later build — never delete it, and
        // never guess at its shape. An older build encountering a newer swap file must
        // leave the user's work for the build that understands it.
        return Err(SwapDecodeError::UnsupportedVersion(version));
    }

    let (header_toml, body) = split_at_close_fence(rest)?;
    let header = from_wire(
        toml::from_str(header_toml).map_err(|e| SwapDecodeError::Malformed(e.to_string()))?,
    );

    if header.doc_id.as_str().len() != DOC_ID_HEX_LEN {
        return Err(SwapDecodeError::Malformed(
            "header carries a malformed document id".to_string(),
        ));
    }

    Ok((header, body.to_string()))
}

/// Split off the first line (without its newline) and everything after it.
fn split_line(text: &str) -> Option<(&str, &str)> {
    match text.find('\n') {
        Some(idx) => Some((text[..idx].trim_end_matches('\r'), &text[idx + 1..])),
        // A file consisting of one unterminated line has no header body to follow.
        None => Some((text, "")),
    }
}

/// Find the closing fence within the bound and split the header from the body.
fn split_at_close_fence(rest: &str) -> Result<(&str, &str), SwapDecodeError> {
    let mut offset = 0usize;
    for (lines_seen, line) in rest.split_inclusive('\n').enumerate() {
        if lines_seen >= MAX_HEADER_LINES || offset >= MAX_HEADER_BYTES {
            break;
        }
        if line.trim_end_matches(['\n', '\r']) == CLOSE_FENCE {
            return Ok((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    Err(SwapDecodeError::Malformed(format!(
        "no closing `{CLOSE_FENCE}` fence within the first {MAX_HEADER_LINES} lines / \
         {MAX_HEADER_BYTES} bytes"
    )))
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, SwapDecodeError, MAX_HEADER_LINES};
    use crate::swapfile::{DocId, SwapHeader};

    fn header() -> SwapHeader {
        SwapHeader {
            doc_id: DocId::from_hex("3f2ac91b4d5e6f708192a3b4c5d6e7f8").expect("valid"),
            path: Some("/home/u/Documents/notes.md".to_string()),
            untitled: false,
            baseline_digest: "abc-3".to_string(),
            written_at: 1_754_000_000,
            owner_pid: 4242,
            app_version: "0.1.0".to_string(),
        }
    }

    #[test]
    fn a_header_and_body_round_trip() {
        let body = "# Notes\n\nSome unsaved work.\n";
        let bytes = encode(&header(), body).expect("encodes");
        let (got_header, got_body) = decode(&bytes).expect("decodes");
        assert_eq!(got_header, header());
        assert_eq!(got_body, body);
    }

    #[test]
    fn the_file_opens_with_the_magic_line_a_human_can_read() {
        let bytes = encode(&header(), "x").expect("encodes");
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(
            text.starts_with("+++scribobulate-swap 1\n"),
            "the first line identifies the file and its format version: {:?}",
            text.lines().next()
        );
    }

    #[test]
    fn a_body_containing_fences_round_trips_untouched() {
        // The scan stops at the FIRST closing fence, so everything after it — including
        // a whole nested frontmatter block — is body.
        let body = "+++\ntitle = \"a document with its own frontmatter\"\n+++\n\n---\n\nBody.\n";
        let bytes = encode(&header(), body).expect("encodes");
        let (_, got) = decode(&bytes).expect("decodes");
        assert_eq!(got, body);
    }

    #[test]
    fn a_path_containing_a_newline_and_a_fence_round_trips_byte_identically() {
        // THE invariant test, and it FAILED when first written — the `toml` crate emits
        // a MULTI-LINE basic string for a value containing a newline, reproducing the
        // `+++` verbatim on its own line and truncating the recovered document. See
        // ScrAP-233; the escaping in `to_wire` is what makes this pass.
        let mut h = header();
        h.path = Some("/tmp/evil\n+++\nnot-the-body.md".to_string());
        let body = "The user's actual work.\n";
        let bytes = encode(&h, body).expect("encodes");
        let (got_header, got_body) = decode(&bytes).expect("decodes");
        assert_eq!(got_header.path, h.path, "the path survives verbatim");
        assert_eq!(
            got_body, body,
            "the body was not truncated by the injection"
        );
    }

    #[test]
    fn no_serialised_header_line_can_forge_the_closing_fence() {
        // The mechanism behind the test above, asserted directly rather than only
        // through its consequence: a hostile path must not put a bare `+++` in the
        // header at all. Stated separately because the round-trip test would still pass
        // if a future change made the body-splitting accidentally tolerant.
        let mut h = header();
        h.path = Some("/tmp/a\n+++\nb.md".to_string());
        let text = String::from_utf8(encode(&h, "work").expect("encodes")).expect("utf-8");
        let header_lines: Vec<&str> = text
            .lines()
            .skip(1) // the magic line
            .take_while(|line| *line != "+++")
            .collect();
        assert!(
            !header_lines.iter().any(|line| line.trim_end() == "+++"),
            "a header line forged the fence: {header_lines:?}"
        );
    }

    #[test]
    fn a_windows_path_round_trips_through_the_escaping() {
        // Backslashes are the common case on one of the three target platforms, so the
        // escaping must be exactly reversible rather than merely newline-correct.
        let mut h = header();
        h.path = Some(r"C:\Users\u\Documents\notes.md".to_string());
        let bytes = encode(&h, "body").expect("encodes");
        let (got, _) = decode(&bytes).expect("decodes");
        assert_eq!(got.path, h.path);
    }

    #[test]
    fn a_path_containing_a_carriage_return_round_trips() {
        let mut h = header();
        h.path = Some("/tmp/a\r\n+++\rb.md".to_string());
        let bytes = encode(&h, "body").expect("encodes");
        let (got, body) = decode(&bytes).expect("decodes");
        assert_eq!(got.path, h.path);
        assert_eq!(body, "body");
    }

    #[test]
    fn an_ordinary_path_is_not_disfigured_by_the_escaping() {
        // Human recoverability is a stated goal of the format: the common case must
        // still read as itself in a text editor.
        let text = String::from_utf8(encode(&header(), "body").expect("encodes")).expect("utf-8");
        assert!(
            text.contains(r#"path = "/home/u/Documents/notes.md""#),
            "an ordinary path should appear verbatim: {text}"
        );
    }

    #[test]
    fn an_untitled_header_omits_the_path_and_says_so() {
        let mut h = header();
        h.path = None;
        h.untitled = true;
        let bytes = encode(&h, "typed but never saved").expect("encodes");
        let (got, body) = decode(&bytes).expect("decodes");
        assert!(got.untitled);
        assert_eq!(got.path, None);
        assert_eq!(body, "typed but never saved");
    }

    #[test]
    fn a_foreign_file_is_reported_as_not_ours() {
        // The recovery scan must be able to tell "somebody else's file" from "our
        // damaged file", because only the second is worth logging as a problem and
        // NEITHER is ever deleted.
        assert_eq!(
            decode(b"# just a markdown file\n"),
            Err(SwapDecodeError::NotOurs)
        );
        assert_eq!(
            decode(b"---\ntitle = 'x'\n---\n"),
            Err(SwapDecodeError::NotOurs)
        );
        assert_eq!(decode(b""), Err(SwapDecodeError::NotOurs));
        assert_eq!(decode(&[0xff, 0xfe, 0x00]), Err(SwapDecodeError::NotOurs));
    }

    #[test]
    fn a_future_format_version_is_ours_but_refused() {
        let bytes = b"+++scribobulate-swap 99\ndoc_id = \"x\"\n+++\nbody";
        assert_eq!(decode(bytes), Err(SwapDecodeError::UnsupportedVersion(99)));
    }

    #[test]
    fn a_truncated_header_is_malformed_not_foreign() {
        // A torn write: our magic, no closing fence. Detectable precisely BECAUSE the
        // fence is missing — which is the property that makes a torn file better than a
        // deleted one.
        let bytes = b"+++scribobulate-swap 1\ndoc_id = \"3f2ac91b4d5e6f708192a3b4c5d6e7f8\"\n";
        assert!(matches!(decode(bytes), Err(SwapDecodeError::Malformed(_))));
    }

    #[test]
    fn the_terminator_search_is_bounded() {
        // A fence far past the bound must NOT be found: otherwise a large document
        // whose body happens to contain `+++` would be scanned end to end and then
        // mis-split.
        let filler = "x = 1\n".repeat(MAX_HEADER_LINES + 10);
        let text = format!("+++scribobulate-swap 1\n{filler}+++\nbody");
        assert!(matches!(
            decode(text.as_bytes()),
            Err(SwapDecodeError::Malformed(_))
        ));
    }

    #[test]
    fn an_empty_body_round_trips() {
        // Reachable: a user selects all and deletes, leaving a dirty empty buffer.
        let bytes = encode(&header(), "").expect("encodes");
        let (_, body) = decode(&bytes).expect("decodes");
        assert_eq!(body, "");
    }

    #[test]
    fn a_body_with_no_trailing_newline_round_trips() {
        let bytes = encode(&header(), "no trailing newline").expect("encodes");
        let (_, body) = decode(&bytes).expect("decodes");
        assert_eq!(body, "no trailing newline");
    }

    #[test]
    fn a_header_carrying_a_malformed_document_id_is_refused() {
        // Defends the filename interpolation even if a header is hand-edited.
        let bytes = b"+++scribobulate-swap 1\ndoc_id = \"../../etc\"\nuntitled = false\n\
                      baseline_digest = \"x\"\nwritten_at = 0\nowner_pid = 1\n\
                      app_version = \"0\"\n+++\nbody";
        assert!(matches!(decode(bytes), Err(SwapDecodeError::Malformed(_))));
    }
}
