//! The XInclude screen: refuse, ahead of gdk-pixbuf's loader chain, any content that
//! could make librsvg process an XInclude (CVE-2026-96889, fixed in librsvg 2.63.2).
//!
//! **Why bytes and not a parse.** The flaw is a use-after-free when an SVG's nested
//! `xi:include` (a `data:` URI suffices) redefines an XML entity. The loader chain
//! picks librsvg by CONTENT, so every hand-off of encoded bytes to it — not only a file
//! named `.svg` — is screened here first, and a refusal is final: the bytes never reach
//! the loader. Content the loader would have rendered is never re-examined by anything
//! downstream, so this screen errs towards refusing. A benign SVG refused here costs a
//! placeholder; a hostile one let through can corrupt memory.
//!
//! **What it refuses, and why each rule is there.** librsvg only acts on an
//! `include` element in the XInclude namespace, so refusing every document that NAMES
//! the namespace, under any prefix, closes the direct route. The remaining rules close
//! the ways the name could reach the parser without appearing in the bytes:
//!
//! | Route to a hidden namespace | Rule |
//! |---|---|
//! | `&#88;Include`, `&#x58;`, `&amp;`-style references | references are expanded before the search |
//! | an entity whose value spells the namespace | any `<!ENTITY` refuses |
//! | an internal DTD subset or an external DTD (which could declare entities) | any DOCTYPE with `[`, or naming a DTD outside `www.w3.org`, refuses |
//! | UTF-16 text (BOM or none), or a switch to UTF-16 mid-stream | content holding a NUL is also read as UTF-16, both byte orders, both alignments |
//! | UCS-4 or EBCDIC, which libxml2 detects from the first four bytes | those signatures refuse |
//! | any other declared encoding (UTF-7, ISO-2022, …) that can spell ASCII without ASCII bytes | an `encoding=` outside the ASCII-compatible allow-list refuses |
//! | gzip (`.svgz`), which librsvg inflates itself | inflated and screened in full |
//! | a nested document in a `data:` URI (`<image>`, `<use>`, CSS `url()`), base64 or percent-encoded | decoded and screened recursively; a `data:` no `,` follows before the next is no URL, and its text is screened with the one before it |
//! | CSS escapes (`data\3a`) and the URL parser dropping tabs and newlines | both are undone before the search |
//! | case | every search ignores ASCII case |
//!
//! Recursion is bounded in depth and in total bytes inspected; exceeding either
//! refuses. Relative and `file:` references are not followed: the loader is fed a
//! stream with no base URL, and librsvg resolves no non-`data:` reference without one.
//!
//! **Its history is ScrSEC-1 in `sdd/CVE.md`**: every leak review later found in this screen,
//! and the rule that closed it. Read it before simplifying anything here — each closed leak is
//! a shape a simplification reopens.
//!
//! Pure: no GTK, no I/O, no global state. The decision of WHETHER to screen
//! ([`enforced`]) and what a refusal does are `super`'s.

use std::io::Read;

/// Whether this process screens at all. Everywhere except a macOS bundle, which is
/// fixed instead by carrying librsvg 2.63.2 (`packaging/macos/bundle.sh`, held to that
/// floor by `verify-librsvg.sh`). Linux decodes through the distribution's librsvg,
/// Windows through the version gvsbuild pins, and a macOS binary run outside its bundle
/// through Homebrew's, none of which this project controls.
///
/// Decided at run time, not by `cfg`: the same macOS binary is one or the other
/// depending on where it runs from, and a compile-time exemption left `cargo run` and a
/// copied-out binary with neither the screen nor the bundle's gate.
pub(crate) fn enforced() -> bool {
    !crate::platform::svg_decoder_is_delivered_fixed()
}

/// How deep `data:` URIs and gzip layers may nest before the content is refused.
const MAX_DEPTH: usize = 4;

/// The total bytes one screen may inspect across every layer it decodes, outside the
/// whole-text readings. A multiple of the local image cap, so an honest `.svgz` with
/// embedded images fits and a gzip bomb or a quadratic tangle of nested `data:` URIs is
/// refused rather than scanned.
const SCAN_BUDGET: usize = 4 * crate::limits::MAX_LOCAL_IMAGE_BYTES as usize;

/// The total bytes the whole-text readings ([`screen_spliced`]) may inspect, every one
/// of them in one screen together, on top of [`SCAN_BUDGET`]. Their own pool because
/// they re-read what the cuts already read: on an honest cap-sized page of
/// percent-encoded icons they need about two and a half times the text, which on the
/// shared pool pushed the page past it. Sized as [`SCAN_BUDGET`] is, so the most one
/// screen can inspect is the two together, a fixed bound whatever the input.
const SPLICE_BUDGET: usize = SCAN_BUDGET;

const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];
const FORM_FEED: u8 = 0x0C;
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The first four bytes by which libxml2 recognises a UCS-4 or EBCDIC document — the
/// encodings in which ASCII markup carries no ASCII bytes and which this screen does
/// not decode.
const UNREADABLE_SIGNATURES: &[[u8; 4]] = &[
    [0x00, 0x00, 0x00, 0x3C],
    [0x3C, 0x00, 0x00, 0x00],
    [0x00, 0x00, 0x3C, 0x00],
    [0x00, 0x3C, 0x00, 0x00],
    [0x00, 0x00, 0xFE, 0xFF],
    [0xFF, 0xFE, 0x00, 0x00],
    [0x00, 0x00, 0xFF, 0xFE],
    [0xFE, 0xFF, 0x00, 0x00],
    [0x4C, 0x6F, 0xA7, 0x94],
];

/// Stands in for every non-ASCII character in the normalised text. Every pattern this
/// screen looks for is ASCII, so nothing is lost by collapsing the rest.
const NON_ASCII: u8 = 0x80;

/// Where an external DTD may live without refusal: the W3C's own, which declare no
/// entity that could spell anything.
const W3C_DTD_PREFIXES: &[&[u8]] = &[b"http://www.w3.org/", b"https://www.w3.org/"];

/// Encodings in which ASCII text is ASCII bytes (or, for UTF-16, is screened as such).
const ALLOWED_ENCODINGS: &[&[u8]] = &[
    b"utf-8",
    b"utf8",
    b"us-ascii",
    b"ascii",
    b"utf-16",
    b"utf-16le",
    b"utf-16be",
    b"ucs-2",
    b"iso-10646-ucs-2",
    b"latin1",
    b"latin-1",
];
const ALLOWED_ENCODING_FAMILIES: &[&[u8]] = &[
    b"iso-8859-",
    b"iso8859-",
    b"iso_8859-",
    b"windows-125",
    b"cp125",
];

/// The URL scheme whose payload is decoded and screened as a nested document.
const DATA_SCHEME: &[u8] = b"data:";

/// Why content was refused. Logged; the user is told only that the image was blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// It names the XInclude namespace.
    XInclude,
    /// It declares an entity, which could spell the namespace.
    EntityDeclaration,
    /// Its DOCTYPE has an internal subset or names a non-W3C DTD.
    Doctype,
    /// It is in, or declares, an encoding this screen cannot read.
    Encoding,
    /// A gzip layer or `data:` payload would not decode.
    Undecodable,
    /// Layers nest deeper than [`MAX_DEPTH`].
    TooDeep,
    /// Decoding its layers would exceed [`SCAN_BUDGET`] or [`SPLICE_BUDGET`].
    TooLarge,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::XInclude => "it names the XInclude namespace",
            Self::EntityDeclaration => "it declares an XML entity",
            Self::Doctype => "its DOCTYPE has an internal subset or a non-W3C DTD",
            Self::Encoding => "it uses a character encoding the screen cannot read",
            Self::Undecodable => "an embedded gzip or data: layer does not decode",
            Self::TooDeep => "its embedded layers nest too deeply",
            Self::TooLarge => "its embedded layers expand past the screening budget",
        })
    }
}

/// Screen `bytes`. `Ok` when nothing in them can make librsvg process an XInclude.
pub(crate) fn inspect(bytes: &[u8]) -> Result<(), Reason> {
    inspect_at(bytes, 0, &mut Budget::new(SCAN_BUDGET, SPLICE_BUDGET))
}

/// What is left to inspect: one pool for the whole-text readings and everything they
/// decode ([`SPLICE_BUDGET`]), one for the rest ([`SCAN_BUDGET`]). Every byte any layer
/// inspects is charged to the pool of the reading it is part of, so each pool bounds
/// its share and neither can be refilled from the other.
struct Budget {
    scan: usize,
    spliced: usize,
    splicing: bool,
}

impl Budget {
    fn new(scan: usize, spliced: usize) -> Self {
        Self {
            scan,
            spliced,
            splicing: false,
        }
    }

    fn pool(&mut self) -> &mut usize {
        if self.splicing {
            &mut self.spliced
        } else {
            &mut self.scan
        }
    }

    /// What the current reading may still inspect.
    fn left(&self) -> usize {
        if self.splicing {
            self.spliced
        } else {
            self.scan
        }
    }

    fn charge(&mut self, bytes: usize) -> Result<(), Reason> {
        let pool = self.pool();
        *pool = pool.checked_sub(bytes).ok_or(Reason::TooLarge)?;
        Ok(())
    }

    /// Run `read` charging the whole-text pool, then return to the pool it was on.
    fn splicing<T>(&mut self, read: impl FnOnce(&mut Self) -> T) -> T {
        let was = std::mem::replace(&mut self.splicing, true);
        let out = read(self);
        self.splicing = was;
        out
    }
}

fn inspect_at(bytes: &[u8], depth: usize, budget: &mut Budget) -> Result<(), Reason> {
    inspect_layer(bytes, depth, budget, Splice::Read)
}

/// Whether a text's `data:` cuts are also read whole ([`screen_spliced`]). Suppressed
/// only for the whole-tail reading itself: it already holds every later sibling URI,
/// and reading ITS tail whole again would nest each sibling one level deeper than the
/// last, refusing an honest page of icons as too deep.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Splice {
    Read,
    Skip,
}

fn inspect_layer(
    bytes: &[u8],
    depth: usize,
    budget: &mut Budget,
    splice: Splice,
) -> Result<(), Reason> {
    if depth > MAX_DEPTH {
        return Err(Reason::TooDeep);
    }
    budget.charge(bytes.len())?;
    if bytes.starts_with(GZIP_MAGIC) {
        let (inflated, tail) = gunzip(bytes, budget.left())?;
        inspect_at(&inflated, depth + 1, budget)?;
        return if tail.is_empty() {
            Ok(())
        } else {
            inspect_at(tail, depth + 1, budget)
        };
    }
    if starts_by_signature(bytes) {
        return Err(Reason::Encoding);
    }
    let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    inspect_chars(body.iter().map(|&b| char::from(b)), depth, budget, splice)?;
    // ASCII written as UTF-16 always has a zero byte beside it, so content with none
    // cannot spell anything when read that way. Each of these four readings is a full
    // pass of its own, so each is charged its code units before it runs: the budget
    // bounds work done, not bytes received.
    if bytes.contains(&0) {
        for offset in [0, 1] {
            let tail = bytes.get(offset..).unwrap_or_default();
            for big_endian in [false, true] {
                budget.charge(tail.len() / 2)?;
                inspect_chars(utf16_chars(tail, big_endian), depth, budget, splice)?;
            }
        }
    }
    Ok(())
}

/// Inflate every gzip member at the start of `bytes`, at most `budget` bytes in all, and
/// return what they inflate to with whatever follows the last member. A percent-encoded
/// `data:` payload is screened to the next URI or the end of the text, past the real end
/// of the payload, so a gzip one arrives with the rest of the document behind it: that
/// tail is returned for screening as it is rather than failing the inflate, which would
/// refuse every honest `.svgz` embedded that way.
fn gunzip(bytes: &[u8], budget: usize) -> Result<(Vec<u8>, &[u8]), Reason> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while rest.starts_with(GZIP_MAGIC) {
        let left = budget.saturating_sub(out.len());
        let mut member = flate2::bufread::GzDecoder::new(rest);
        (&mut member)
            .take(left as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|_| Reason::Undecodable)?;
        if out.len() > budget {
            return Err(Reason::TooLarge);
        }
        rest = member.into_inner();
    }
    Ok((out, rest))
}

fn utf16_chars(bytes: &[u8], big_endian: bool) -> impl Iterator<Item = char> + '_ {
    let units = bytes.chunks_exact(2).map(move |pair| {
        let pair = [pair[0], pair[1]];
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
}

/// Read decoded characters as text: as they stand, and, where markup inside a
/// `<style>` element splits its text, also as the stylesheet receives that text
/// ([`without_joining_markup`]). The markup is found in the text as written, before any
/// reference or escape is expanded, as the XML parser finds it; each reading is then
/// normalised for searching.
fn inspect_chars(
    chars: impl Iterator<Item = char>,
    depth: usize,
    budget: &mut Budget,
    splice: Splice,
) -> Result<(), Reason> {
    let ascii = to_ascii(chars);
    if let Some(joined) = without_joining_markup(&ascii) {
        budget.charge(joined.len())?;
        inspect_text(&normalise(joined), depth, budget, splice)?;
    }
    inspect_text(&normalise(ascii), depth, budget, splice)
}

/// ASCII kept, everything else [`NON_ASCII`].
fn to_ascii(chars: impl Iterator<Item = char>) -> Vec<u8> {
    chars
        .map(|c| if c.is_ascii() { c as u8 } else { NON_ASCII })
        .collect()
}

/// Reduce text to what a parser would see, for searching: XML character and
/// predefined-entity references expanded; CSS escapes expanded; tabs, newlines and form
/// feeds dropped, as a URL parser drops the first two and CSS reads the third as a
/// newline (dropping more can only find more).
fn normalise(ascii: Vec<u8>) -> Vec<u8> {
    let mut text = expand_xml_references(&ascii);
    text = expand_css_escapes(&text);
    text.retain(|&b| !matches!(b, b'\t' | b'\n' | b'\r' | FORM_FEED));
    text
}

fn code_point_byte(value: u32) -> u8 {
    match char::from_u32(value) {
        Some(c) if c.is_ascii() => c as u8,
        _ => NON_ASCII,
    }
}

fn expand_xml_references(text: &[u8]) -> Vec<u8> {
    const PREDEFINED: &[(&[u8], u8)] = &[
        (b"amp;", b'&'),
        (b"lt;", b'<'),
        (b"gt;", b'>'),
        (b"quot;", b'"'),
        (b"apos;", b'\''),
    ];
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'&' {
            let rest = &text[i + 1..];
            if let Some((byte, used)) = char_reference(rest) {
                out.push(byte);
                i += 1 + used;
                continue;
            }
            if let Some((name, byte)) = PREDEFINED.iter().find(|(name, _)| rest.starts_with(name)) {
                out.push(*byte);
                i += 1 + name.len();
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// `#123;` or `#x7B;` at the start of `rest` → the byte it stands for and the bytes
/// consumed.
fn char_reference(rest: &[u8]) -> Option<(u8, usize)> {
    let digits_from = |start: usize, radix: u32| -> Option<(u8, usize)> {
        let len = rest[start..]
            .iter()
            .take_while(|b| char::from(**b).is_digit(radix))
            .count();
        if len == 0 || rest.get(start + len) != Some(&b';') {
            return None;
        }
        let digits = std::str::from_utf8(&rest[start..start + len]).ok()?;
        let byte = u32::from_str_radix(digits, radix).map_or(NON_ASCII, code_point_byte);
        Some((byte, start + len + 1))
    };
    match rest {
        [b'#', b'x' | b'X', ..] => digits_from(2, 16),
        [b'#', ..] => digits_from(1, 10),
        _ => None,
    }
}

/// CSS escapes as CSS Syntax 3 reads them: `\` + up to six hex digits, then one
/// whitespace eaten (space, tab, line feed, carriage return, form feed, or CR LF);
/// `\` + a newline or form feed (an escaped newline in a string) stands for nothing; `\`
/// + any other character stands for itself.
fn expand_css_escapes(text: &[u8]) -> Vec<u8> {
    const MAX_HEX: usize = 6;
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] != b'\\' || i + 1 == text.len() {
            out.push(text[i]);
            i += 1;
            continue;
        }
        let rest = &text[i + 1..];
        let hex = rest
            .iter()
            .take(MAX_HEX)
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        if hex == 0 {
            match rest {
                [b'\r', b'\n', ..] => i += 3,
                [b'\n' | b'\r' | FORM_FEED, ..] => i += 2,
                [other, ..] => {
                    out.push(*other);
                    i += 2;
                }
                [] => unreachable!("a `\\` that ends the text is pushed above"),
            }
            continue;
        }
        let digits = std::str::from_utf8(&rest[..hex]).unwrap_or("0");
        out.push(u32::from_str_radix(digits, 16).map_or(NON_ASCII, code_point_byte));
        i += 1 + hex;
        match &text[i..] {
            [b'\r', b'\n', ..] => i += 2,
            [b' ' | b'\t' | b'\n' | b'\r' | FORM_FEED, ..] => i += 1,
            _ => {}
        }
    }
    out
}

fn inspect_text(
    text: &[u8],
    depth: usize,
    budget: &mut Budget,
    splice: Splice,
) -> Result<(), Reason> {
    #[cfg(test)]
    READINGS.with(|n| n.set(n.get() + 1));
    check_patterns(text, budget)?;
    let starts = delimited_starts(text, data_uri_starts(text));
    for (i, &at) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(text.len());
        for reading in data_uri_payload(&text[at + DATA_SCHEME.len()..end], budget)? {
            match reading {
                Payload::Base64(decoded) => inspect_at(&decoded, depth + 1, budget)?,
                Payload::Percent(decoded) => inspect_every_decoding(decoded, depth, budget)?,
            }
        }
    }
    if let ([first, _, ..], Splice::Read) = (&starts[..], splice) {
        screen_spliced(&text[first + DATA_SCHEME.len()..], depth, budget)?;
    }
    Ok(())
}

/// A `<style>` element's text as librsvg hands it to the stylesheet. The XML parser
/// drops comments and processing instructions and unwraps CDATA sections, and librsvg
/// keeps only the style element's own character data: a child element, and all it holds,
/// contributes nothing. So the text on either side of any of those is one run
/// (`url(data:…;base64,AAAA<x/>BBBB)` is one URI to the stylesheet).
///
/// `text` is as written, before references and escapes are expanded, because that is
/// where the parser finds markup: `&lt;!--` and `\3c !--` are text to it, and inside a
/// CDATA section only `]]>` is markup. Everything outside style elements is kept as it
/// is. A comment, instruction or tag with no end is kept as text (the parser fails on
/// it; dropping it could let this reading see less than the decoder might).
///
/// `None` unless something removed inside a style element has text before it and after
/// it in that element, so an honest page is not read twice. One pass, each byte once
/// (a search for an end that fails is not repeated), so linear.
fn without_joining_markup(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut unended = Unended::default();
    // Inside a style element: how deep in child elements, whether its own text has begun,
    // and whether something was removed since.
    let mut in_style = false;
    let mut depth = 0usize;
    let mut text_seen = false;
    let mut removed = false;
    let mut joins = false;
    let mut in_cdata = false;
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        // The extent of the markup at `i`, if any, and whether it is removed.
        let markup = if in_cdata {
            rest.starts_with(b"]]>").then(|| {
                in_cdata = false;
                i + 3
            })
        } else if rest.starts_with(b"<!--") {
            unended.find(text, i + 4, b"-->", 0)
        } else if rest.starts_with(b"<?") {
            unended.find(text, i + 2, b"?>", 1)
        } else if rest.starts_with(b"<![CDATA[") {
            in_cdata = true;
            Some(i + 9)
        } else if let Some(tag) = Tag::at(text, i, &mut unended) {
            if !in_style {
                if tag.is_style && !tag.closing && !tag.self_closing {
                    in_style = true;
                    (depth, text_seen, removed) = (0, false, false);
                }
                out.extend_from_slice(&text[i..tag.end]);
                i = tag.end;
                continue;
            }
            if depth == 0 && tag.closing && tag.is_style {
                in_style = false;
                out.extend_from_slice(&text[i..tag.end]);
                i = tag.end;
                continue;
            }
            if tag.closing {
                depth = depth.saturating_sub(1);
            } else if !tag.self_closing {
                depth += 1;
            }
            Some(tag.end)
        } else {
            None
        };
        match markup {
            Some(end) if in_style => {
                removed |= text_seen;
                i = end;
            }
            Some(end) => {
                out.extend_from_slice(&text[i..end]);
                i = end;
            }
            None => {
                let b = text[i];
                if !in_style || depth == 0 {
                    out.push(b);
                    if in_style && !b.is_ascii_whitespace() {
                        joins |= removed;
                        text_seen = true;
                        removed = false;
                    }
                }
                i += 1;
            }
        }
    }
    joins.then_some(out)
}

/// Which ends [`without_joining_markup`] has already failed to find: once one is not
/// found, none is found later, so the search is not repeated.
#[derive(Default)]
struct Unended([bool; 3]);

impl Unended {
    const TAG: usize = 2;

    /// The index just past the first `close` at or after `from`, unless an earlier
    /// search for the same kind failed.
    fn find(&mut self, text: &[u8], from: usize, close: &[u8], kind: usize) -> Option<usize> {
        if self.0[kind] {
            return None;
        }
        let found = find_ci(text, close, from).map(|at| at + close.len());
        self.0[kind] = found.is_none();
        found
    }
}

/// A start or end tag as the XML parser reads one.
struct Tag {
    end: usize,
    closing: bool,
    self_closing: bool,
    /// Its local name (any prefix dropped) is `style`, in any case.
    is_style: bool,
}

impl Tag {
    fn at(text: &[u8], i: usize, unended: &mut Unended) -> Option<Self> {
        if text.get(i) != Some(&b'<') {
            return None;
        }
        let closing = text.get(i + 1) == Some(&b'/');
        let name_from = i + 1 + usize::from(closing);
        let first = *text.get(name_from)?;
        if !(first.is_ascii_alphabetic() || matches!(first, b'_' | b':')) {
            return None;
        }
        let name_len = text[name_from..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-'))
            .count();
        let name = &text[name_from..name_from + name_len];
        let local = name.rsplit(|&b| b == b':').next().unwrap_or(name);
        // The end is the first `>` outside a quoted attribute value.
        if unended.0[Unended::TAG] {
            return None;
        }
        let mut quote = None;
        let mut at = name_from + name_len;
        let end = loop {
            let Some(&b) = text.get(at) else {
                unended.0[Unended::TAG] = true;
                return None;
            };
            match (quote, b) {
                (None, b'"' | b'\'') => quote = Some(b),
                (Some(q), _) if b == q => quote = None,
                (None, b'>') => break at + 1,
                _ => {}
            }
            at += 1;
        };
        Some(Self {
            end,
            closing,
            self_closing: !closing && text[end - 2] == b'/',
            is_style: local.eq_ignore_ascii_case(b"style"),
        })
    }
}

/// What a real decoder makes of one decoded `data:` payload on the way to the next: the
/// payload is a document whose parser expands XML character and entity references (and,
/// in a stylesheet, CSS escapes) before the URL inside it is percent-decoded. So
/// `&#37;1F` is one layer from the byte 0x1F, as `%1F` is. The ONE step both the
/// per-segment loop and the whole-tail reading take, so neither can skip a layer the
/// other models.
fn next_layer(level: &[u8]) -> Vec<u8> {
    percent_decode(&expand_css_escapes(&expand_xml_references(level)))
}

/// Inspect a percent-encoded payload at every level of decoding it has, not just the
/// first. A URI written literally inside an encoded document is the cut's own segment,
/// but a real decoder reaches it through the outer layer first, so its payload is
/// decoded once more than the segment decodes it — a double-encoded gzip or SVG would
/// otherwise reach the screen still encoded. Each level that differs from the last is
/// inspected in full (gzip, UTF-16, nested URIs); one still changing after
/// [`MAX_DEPTH`] levels is refused, as nesting that deep is anyway. A level that is
/// gzip ends the walk: inflating it is how its content is read, and "decoding" the
/// compressed bytes as text would corrupt an honest `.svgz` into a refusal.
fn inspect_every_decoding(
    mut level: Vec<u8>,
    depth: usize,
    budget: &mut Budget,
) -> Result<(), Reason> {
    inspect_at(&level, depth + 1, budget)?;
    for _ in 0..MAX_DEPTH {
        if level.starts_with(GZIP_MAGIC) {
            return Ok(());
        }
        let next = next_layer(&level);
        if next == level {
            return Ok(());
        }
        inspect_at(&next, depth + 1, budget)?;
        level = next;
    }
    if level.starts_with(GZIP_MAGIC) || next_layer(&level) == level {
        Ok(())
    } else {
        Err(Reason::TooDeep)
    }
}

/// The rules that read the text as it stands, with no layer decoded.
fn check_patterns(text: &[u8], budget: &mut Budget) -> Result<(), Reason> {
    if find_ci(text, b"xinclude", 0).is_some() {
        return Err(Reason::XInclude);
    }
    if find_ci(text, b"<!entity", 0).is_some() {
        return Err(Reason::EntityDeclaration);
    }
    check_doctypes(text, budget)?;
    check_encodings(text)
}

/// The cut at each `data:` is a guess at where one URI ends and the next begins, and a
/// wrong guess must not hide anything. A literal `data:` can sit inside a
/// percent-encoded document — an attribute value, or a nested URI written unencoded —
/// and then the cut splits ONE real payload in two: what follows the cut up to its
/// first comma is screened as nobody's text (a "header"), and a URI written literally
/// inside an encoded one is decoded once by its segment where a real decoder, decoding
/// the outer layer first, decodes it twice.
///
/// So the text after the first URI's comma is also read whole, the way a decoder reads
/// the one URI it may all belong to: every real payload is a contiguous run of it (a
/// payload ends before a later byte, never mid-escape, since a comma is never part of
/// one). It is taken a [`next_layer`] at a time until nothing more decodes, and every
/// layer is inspected in full — gzip inflated, UTF-16 read, nested URIs cut and decoded
/// — though not read whole again ([`Splice::Skip`]). Each layer is charged to the
/// whole-text pool ([`SPLICE_BUDGET`]) with all it decodes, so a chain of `%25` cannot
/// spin it, and a gzip layer ends the walk as in [`inspect_every_decoding`].
fn screen_spliced(after_scheme: &[u8], depth: usize, budget: &mut Budget) -> Result<(), Reason> {
    let comma = after_scheme
        .iter()
        .position(|&b| b == b',')
        .ok_or(Reason::Undecodable)?;
    let mut layer = after_scheme[comma + 1..].to_vec();
    budget.splicing(|budget| loop {
        let next = next_layer(&layer);
        if next == layer {
            return Ok(());
        }
        inspect_layer(&next, depth + 1, budget, Splice::Skip)?;
        if next.starts_with(GZIP_MAGIC) {
            return Ok(());
        }
        layer = next;
    })
}

/// The `starts` that a comma follows before the next one, the only ones that can begin
/// a `data:` URL in this text: a reader takes the header to the FIRST comma, and with
/// none there is no URL (WHATWG's `data:` URL processor fails, and librsvg's `data-url`
/// with it). Every other start is dropped, and its text joins the segment before it.
///
/// Dropping one hides nothing. If a comma follows it further on, past later starts, its
/// header runs to that comma and its payload is the payload of the last start before
/// that comma, which is kept and screened alike: that header ends in that start's
/// header, so `;base64` reads the same in both, because `;base64` holds no `:` to straddle
/// the `data:` between. If a comma never follows, it is no URL. And a `data:` written
/// inside an encoded payload, its comma encoded (`data:x;base64%2C…`), is part of the
/// segment before it, decoded with that payload, where its comma is literal and it is a
/// start once more.
///
/// So prose such as `<title>Source data: World Bank</title>` is screened as text whether
/// or not a comma happens to come later in the file.
fn delimited_starts(text: &[u8], starts: Vec<usize>) -> Vec<usize> {
    let ends = starts.iter().skip(1).copied().chain([text.len()]);
    starts
        .iter()
        .zip(ends)
        .filter(|&(&at, end)| text[at + DATA_SCHEME.len()..end].contains(&b','))
        .map(|(&at, _)| at)
        .collect()
}

/// Where each `data:` that could begin a URL starts. One preceded by a scheme
/// character is the tail of a longer word (`metadata:`), which no URL parser reads as
/// the `data` scheme, since a scheme is the whole run of scheme characters before `:`.
/// One whose `d` or `da` completes a `%XX` escape (`%da`, `%5da`) is not there at all
/// once the escape is decoded.
fn data_uri_starts(text: &[u8]) -> Vec<usize> {
    let is_scheme_char = |b: &u8| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.');
    let escaped = |at: usize| {
        let before = |n: usize| at.checked_sub(n).and_then(|i| text.get(i)).copied();
        before(1) == Some(b'%')
            || (before(2) == Some(b'%') && before(1).is_some_and(|b| b.is_ascii_hexdigit()))
    };
    let mut starts = Vec::new();
    let mut from = 0;
    while let Some(at) = find_ci(text, DATA_SCHEME, from) {
        from = at + 1;
        let in_word = at
            .checked_sub(1)
            .and_then(|before| text.get(before))
            .is_some_and(is_scheme_char);
        if !in_word && !escaped(at) {
            starts.push(at);
        }
    }
    starts
}

/// Every `<!doctype` is examined from its own start, even one inside an earlier
/// declaration's quoted literal: skipping past each declaration would leave a real one
/// unexamined behind a decoy whose literal swallows it. So the scans can overlap, and
/// `<!doctype ` repeated to the image cap before a single `>` is quadratic. Each scan is
/// therefore charged to the inspection budget, byte for byte, and a text that exhausts
/// it is refused as too large rather than hanging the main thread.
fn check_doctypes(text: &[u8], budget: &mut Budget) -> Result<(), Reason> {
    let mut from = 0;
    while let Some(at) = find_ci(text, b"<!doctype", from) {
        from = at + 1;
        let mut scanned = 0;
        let mut quote = None;
        let mut literals: Vec<&[u8]> = Vec::new();
        let mut literal_start = 0;
        let mut closed = false;
        for (i, &b) in text.iter().enumerate().skip(at) {
            scanned += 1;
            match quote {
                Some(q) if b == q => {
                    literals.push(&text[literal_start..i]);
                    quote = None;
                }
                Some(_) => {}
                None if b == b'"' || b == b'\'' => {
                    quote = Some(b);
                    literal_start = i + 1;
                }
                None if b == b'[' => return Err(Reason::Doctype),
                None if b == b'>' => {
                    closed = true;
                    break;
                }
                None => {}
            }
        }
        budget.charge(scanned)?;
        if !closed {
            return Err(Reason::Doctype);
        }
        if let Some(system) = literals.last() {
            if !W3C_DTD_PREFIXES.iter().any(|p| starts_with_ci(system, p)) {
                return Err(Reason::Doctype);
            }
        }
    }
    Ok(())
}

fn check_encodings(text: &[u8]) -> Result<(), Reason> {
    let mut from = 0;
    // The first `?>` at or after the current declaration. Declarations are visited in
    // order, so it only ever moves forward: one search per `?>` in the text, where a
    // search per `<?xml` made `<?xml` repeated with no `?>` quadratic.
    let mut close: Option<usize> = None;
    let mut searched_from = 0;
    let mut judged_to = 0;
    while let Some(at) = find_ci(text, b"<?xml", from) {
        from = at + 1;
        let after = at + b"<?xml".len();
        // Every `<?xml…?>` is examined, `<?xml-stylesheet …?>` and friends included:
        // normalising drops the tab or newline that may separate `<?xml` from
        // `version`, so what follows `<?xml` cannot tell the declaration apart.
        if close.map_or(searched_from <= after, |c| c < after) {
            close = find_ci(text, b"?>", after);
            searched_from = close.map_or(text.len() + 1, |c| c);
        }
        let end = close.unwrap_or(text.len());
        // A key is judged once. Every declaration that holds it ends at the same `?>`
        // (the first after it), so its verdict cannot differ between them, and
        // overlapping unterminated declarations would otherwise re-judge every key.
        let mut key_from = after.max(judged_to);
        while let Some(key) = find_ci(&text[..end], b"encoding", key_from) {
            key_from = key + 1;
            let rest = &text[key + b"encoding".len()..end];
            // `encoding` not followed by `=` is a word inside some other value.
            if !trim_start(rest).starts_with(b"=") {
                continue;
            }
            let name = declared_value(rest).ok_or(Reason::Encoding)?;
            let allowed = ALLOWED_ENCODINGS
                .iter()
                .any(|e| name.eq_ignore_ascii_case(e))
                || ALLOWED_ENCODING_FAMILIES
                    .iter()
                    .any(|f| starts_with_ci(name, f));
            if !allowed {
                return Err(Reason::Encoding);
            }
        }
        // A key that straddles `end` is not in this declaration but may be in a later
        // one that reaches further.
        judged_to = judged_to.max(key_from.max(end.saturating_sub(b"encoding".len() - 1)));
    }
    Ok(())
}

/// `= "value"` (whitespace allowed around `=`) → `value`.
fn declared_value(rest: &[u8]) -> Option<&[u8]> {
    let rest = trim_start(rest).strip_prefix(b"=")?;
    let rest = trim_start(rest);
    let (&quote, rest) = rest.split_first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let len = rest.iter().position(|&b| b == quote)?;
    Some(&rest[..len])
}

fn trim_start(bytes: &[u8]) -> &[u8] {
    let skip = bytes.iter().take_while(|b| b.is_ascii_whitespace()).count();
    &bytes[skip..]
}

/// The decoded payload of the `data:` URI whose text follows `data:` in `rest`, which
/// runs to the next `data:` URI or the end of the text. Fails closed: with no `,` to end
/// the header the URI cannot be delimited, so it is refused rather than passed
/// unscreened ([`delimited_starts`] hands it only starts a comma follows, so this guards
/// a future caller). The extent is over-approximated — a percent-encoded payload is
/// taken to run to the end of `rest`, past any quote, since the quote may belong to the
/// nested document rather than close the attribute — and screening more text than the
/// URI holds can only refuse more.
fn data_uri_payload(rest: &[u8], budget: &mut Budget) -> Result<Vec<Payload>, Reason> {
    let comma = rest
        .iter()
        .position(|&b| b == b',')
        .ok_or(Reason::Undecodable)?;
    let body = &rest[comma + 1..];
    let base64_body = || base64_reading(body);
    let mut readings = first_cut_readings(rest, comma, body, base64_body)?;
    // The cut above is the first comma of the NORMALISED text, and an escape can make
    // one (`\2c`, or `&#92;2c`, which the XML parser turns into `\2c`) where the
    // decoder, which ends the header at the first LITERAL comma, sees none. The decoder
    // may then end the header at any later comma, and read a base64 body there if what
    // precedes it ends in its base64 rule. That rule asks for a `;`, which this
    // normalised text may hold where the decoder's does not, and lack where the
    // decoder's holds one (an `&amp;` the decoder's context leaves unexpanded ends in
    // `;`). So every later comma that `base64` precedes, however it is introduced, gets
    // a base64 reading too. The percent reading of the first cut holds every byte after a later
    // comma, so as TEXT it is screened already, but not as a payload that STARTS there:
    // gzip and the unreadable encodings are recognised by their first bytes, and behind
    // the bytes before the comma neither is seen. So a later comma whose percent-decoded
    // payload begins with one gets a percent reading of its own. Only its first bytes are
    // decoded to ask, so asking every comma stays linear, and a reading is charged before
    // it is built, so a text of many such commas refuses rather than grow quadratically.
    for (at, _) in rest
        .iter()
        .enumerate()
        .skip(comma + 1)
        .filter(|&(_, &b)| b == b',')
    {
        let tail = &rest[at + 1..];
        if ends_in_base64_word(&rest[..at]) {
            readings.extend(base64_reading(tail).map(Payload::Base64));
        }
        if starts_by_signature(&percent_decode(
            &tail[..tail.len().min(SIGNATURE_RAW_BYTES)],
        )) {
            budget.charge(tail.len())?;
            readings.push(Payload::Percent(percent_decode(tail)));
        }
    }
    Ok(readings)
}

/// The most raw bytes the longest start signature ([`UNREADABLE_SIGNATURES`]) can take
/// to write percent-encoded: three per byte.
const SIGNATURE_RAW_BYTES: usize = 3 * 4;

/// Whether `bytes` begins with what [`inspect_layer`] recognises by a document's first
/// bytes alone: gzip, or an encoding this screen does not decode.
fn starts_by_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(GZIP_MAGIC)
        || bytes
            .get(..4)
            .is_some_and(|head| UNREADABLE_SIGNATURES.iter().any(|sig| head == sig))
}

/// The base64 body that starts `body`, decoded. The scan admits all the whitespace
/// `base64_decode` skips, so a CSS-escaped form feed cannot end the payload early.
fn base64_reading(body: &[u8]) -> Option<Vec<u8>> {
    let len = body
        .iter()
        .take_while(|b| {
            b.is_ascii_alphanumeric()
                || b.is_ascii_whitespace()
                || matches!(b, b'+' | b'/' | b'=' | b'%')
        })
        .count();
    base64_decode(&percent_decode(&body[..len]))
}

/// Whether `prefix`, trailing whitespace trimmed, ends in `base64` in any case: a
/// superset of the decoder's rule ([`ends_in_base64_rule`]), in constant time.
fn ends_in_base64_word(prefix: &[u8]) -> bool {
    let h = prefix.trim_ascii_end();
    h.len()
        .checked_sub(b"base64".len())
        .is_some_and(|cut| h[cut..].eq_ignore_ascii_case(b"base64"))
}

/// The decoder's base64 rule (see [`decoder_reads_base64`]) tested on the END of
/// `header` only, in constant time, so every comma of a long text can be asked. The
/// text is normalised, so tabs and newlines are already gone.
fn ends_in_base64_rule(header: &[u8]) -> bool {
    let h = header.trim_ascii_end();
    let Some(cut) = h.len().checked_sub(b"base64".len()) else {
        return false;
    };
    if !h[cut..].eq_ignore_ascii_case(b"base64") {
        return false;
    }
    let before = &h[..cut];
    let spaces = before.iter().rev().take_while(|&&b| b == b' ').count();
    before[..before.len() - spaces].ends_with(b";")
}

/// How much of `body`'s start a percent reading may skip: its run of base64 characters
/// containing no `%`, less the last [`STRADDLE`] bytes. See the base64 arm of
/// [`first_cut_readings`] for why that run is inert.
fn inert_prefix(body: &[u8]) -> usize {
    /// Longer than any word the checks search for, so one straddling the cut is kept.
    const STRADDLE: usize = 16;
    #[cfg(test)]
    if NO_INERT_PREFIX.with(std::cell::Cell::get) {
        return 0;
    }
    let run = body
        .iter()
        .take_while(|b| {
            b.is_ascii_alphanumeric() || b.is_ascii_whitespace() || matches!(b, b'+' | b'/' | b'=')
        })
        .count();
    run.saturating_sub(STRADDLE)
}

/// The readings at the first comma, by what its header says.
fn first_cut_readings(
    rest: &[u8],
    comma: usize,
    body: &[u8],
    base64_body: impl Fn() -> Option<Vec<u8>>,
) -> Result<Vec<Payload>, Reason> {
    Ok(match header_encoding(&rest[..comma]) {
        // The percent reading too, even here. The header this sees has already had the
        // screen's own normalisation applied (CSS escapes undone everywhere, references
        // expanded even in CDATA), which a decoder applies only in some contexts, so a
        // header that reads as base64 HERE can be percent to the decoder (`;\62 ase64`
        // in a plain attribute). Its leading run of base64 characters with no `%` is
        // skipped, all but its last few bytes: percent-decoding leaves that run as it is,
        // and text with no `<`, `:` or `.` can hold no markup and no namespace URL, so
        // reading it twice only doubled what an honest base64 raster costs the budget
        // (enough to refuse an honest page near the cap). The bytes kept let a word that
        // straddles the run's end still be found. A base64 run can spell `xinclude` by
        // chance and be refused for it: astronomically unlikely, and the safe direction.
        HeaderEncoding::Base64 => vec![
            Payload::Percent(percent_decode(&body[inert_prefix(body)..])),
            Payload::Base64(base64_body().ok_or(Reason::Undecodable)?),
        ],
        HeaderEncoding::Percent => vec![Payload::Percent(percent_decode(body))],
        HeaderEncoding::Either => {
            let mut readings = vec![Payload::Percent(percent_decode(body))];
            readings.extend(base64_body().map(Payload::Base64));
            readings
        }
    })
}

/// How a `data:` header says its payload is encoded.
///
/// **Base64 only on the decoder's own rule, applied to the header before any
/// percent-decoding** (it is the screen's normalised text, so CSS escapes and references are
/// already undone; [`data_uri_payload`] covers what that changes). WHATWG's
/// `data:` processor (librsvg decodes with the `data-url` crate, which implements it)
/// never percent-decodes the media type: after the URL parser drops tabs and newlines
/// and the trailing whitespace is stripped, the header must end in `;`, any spaces, and
/// a case-insensitive `base64`. So `; base64` and `;BASE64` are base64, and `%3Bbase64`,
/// `;base64%20` or `;base%364` are percent text that only LOOKS like base64 once
/// decoded. Reading the decoded header there gave the base64 reading alone to a body
/// the decoder reads as percent, and a base64 scan empties a percent body at its first
/// `&` or `<`: a bypass with a single URI. Even a base64 verdict keeps the percent
/// reading ([`data_uri_payload`] says why).
///
/// **Every other header that could spell `base64` through some decoding** — it holds
/// `base`, or any byte that begins an encoding (`%`, a CSS `\`, an XML `&`) — gets both
/// readings, and refuses if either does. A header with none of those is percent.
///
/// **Redundant for safety today, and kept on purpose.** Since a base64 verdict keeps the
/// percent reading as well, reverting this to the decoded-header rule leaves every test
/// green (measured). It stays because it is the decoder's own rule, and it decides the
/// one thing the percent reading cannot: when an undecodable base64 body refuses.
fn header_encoding(header: &[u8]) -> HeaderEncoding {
    if decoder_reads_base64(header) {
        HeaderEncoding::Base64
    } else if spells_base(header) || header.iter().any(|b| matches!(b, b'%' | b'\\' | b'&')) {
        HeaderEncoding::Either
    } else {
        HeaderEncoding::Percent
    }
}

/// Whether `header` holds `base` once every control byte and space is removed, so no
/// whitespace a decoder might drop (a form feed in CSS, say) can split the word.
fn spells_base(header: &[u8]) -> bool {
    let kept: Vec<u8> = header.iter().copied().filter(|&b| b > b' ').collect();
    find_ci(&kept, b"base", 0).is_some()
}

/// The WHATWG `data:` rule for a base64 body, on the header exactly as the decoder
/// receives it: tabs and newlines removed (the URL parser drops them anywhere), trailing
/// whitespace stripped, then `;`, any number of spaces, and `base64` in any case.
fn decoder_reads_base64(header: &[u8]) -> bool {
    let kept: Vec<u8> = header
        .iter()
        .copied()
        .filter(|b| !matches!(b, b'\t' | b'\n' | b'\r'))
        .collect();
    ends_in_base64_rule(&kept)
}

enum HeaderEncoding {
    Base64,
    Percent,
    Either,
}

/// A `data:` payload, decoded once, and how it was encoded.
enum Payload {
    Base64(Vec<u8>),
    Percent(Vec<u8>),
}

fn percent_decode(text: &[u8]) -> Vec<u8> {
    let hex = |b: u8| char::from(b).to_digit(16);
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'%' {
            if let (Some(hi), Some(lo)) = (
                text.get(i + 1).copied().and_then(hex),
                text.get(i + 2).copied().and_then(hex),
            ) {
                // Two hex digits always fit a byte.
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// Forgiving base64, as a `data:` URL decoder reads it: ASCII whitespace ignored, up to
/// two `=` of padding, which may be absent. `None` for anything else.
fn base64_decode(text: &[u8]) -> Option<Vec<u8>> {
    const BITS_PER_SYMBOL: u32 = 6;
    let symbol = |b: u8| -> Option<u32> {
        Some(match b {
            b'A'..=b'Z' => u32::from(b - b'A'),
            b'a'..=b'z' => u32::from(b - b'a') + 26,
            b'0'..=b'9' => u32::from(b - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut clean: Vec<u8> = text
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.len().is_multiple_of(4) {
        for _ in 0..2 {
            if clean.last() == Some(&b'=') {
                clean.pop();
            }
        }
    }
    if clean.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for b in clean {
        acc = (acc << BITS_PER_SYMBOL) | symbol(b)?;
        bits += BITS_PER_SYMBOL;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn starts_with_ci(hay: &[u8], prefix: &[u8]) -> bool {
    hay.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// The first case-insensitive occurrence of ASCII `needle` in `hay` at or after `from`.
fn find_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let first = needle.first()?.to_ascii_lowercase();
    let last_start = hay.len().checked_sub(needle.len())?;
    (from..=last_start).find(|&i| {
        hay[i].to_ascii_lowercase() == first
            && hay[i..i + needle.len()].eq_ignore_ascii_case(needle)
    })
}

#[cfg(test)]
thread_local! {
    /// How many texts [`inspect_reading`] has read, so a test can bound the joined readings.
    static READINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Lets a test measure what an honest raster would cost without [`inert_prefix`].
    static NO_INERT_PREFIX: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests;
