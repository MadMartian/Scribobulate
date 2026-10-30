//! The gate every input passes before the decoder sees it: nothing that could reach
//! CVE-2026-96889 is ever decoded, on any runtime, the vulnerable Windows one included.
//!
//! The flaw needs an entity declaration, and every route to one (an internal subset, an
//! external DTD, a nested document carrying either) spells `DOCTYPE` or `ENTITY`
//! somewhere once its layers are undone. Testing the emitted bytes for those words is
//! not enough, because hiding markup under encodings is the whole point of the inputs.
//! So this is a deliberately LOOSE model of the decoder (approach 2 of the plan this
//! test implements, used only as a filter, never as the judge): from the input it
//! builds every reading reachable by undoing layers in any order — references, CSS
//! escapes, whitespace and markup removal, percent, base64 after any comma, gzip at any
//! signature, UTF-16 at either alignment — and refuses the input if ANY reading holds
//! either word in any case, or is in an encoding the model cannot read. Wrongly
//! dropping an input costs one test case; wrongly admitting one could corrupt memory on
//! the runner, so every bound here fails closed.
//!
//! Independent of the screen on purpose: the screen deciding what may reach the decoder
//! is the very thing under test.

use std::collections::HashSet;
use std::io::Read as _;

/// An input this gate admitted. Only [`admit`] makes one, and the oracle takes nothing
/// else, so no path hands the decoder bytes that skipped the check.
pub(super) struct Admitted(Vec<u8>);

impl Admitted {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// The most distinct readings, and bytes across them, one input may produce. Past
/// either, the input is dropped rather than half-checked.
const MAX_READINGS: usize = 4096;
const MAX_READING_BYTES: usize = 64 << 20;
/// The shortest run after a comma worth a base64 reading.
const MIN_BASE64_RUN: usize = 8;

/// Words whose presence in any reading drops the input.
const FORBIDDEN: &[&[u8]] = &[b"doctype", b"entity"];

/// First bytes of encodings the model does not read (UCS-4 in its four byte orders,
/// EBCDIC), which libxml2 would.
const UNREADABLE_SIGNATURES: &[&[u8]] = &[
    &[0, 0, 0, b'<'],
    &[b'<', 0, 0, 0],
    &[0, 0, b'<', 0],
    &[0, b'<', 0, 0],
    &[0, 0, 0xFE, 0xFF],
    &[0xFF, 0xFE, 0, 0],
    &[0x4C, 0x6F, 0xA7, 0x94],
];

/// Declared encodings the model reads: ASCII-compatible ones read as bytes, and UTF-16,
/// which it decodes.
const READABLE_ENCODINGS: &[&str] = &[
    "utf-8",
    "utf8",
    "us-ascii",
    "ascii",
    "iso-8859-1",
    "latin1",
    "utf-16",
    "utf-16le",
    "utf-16be",
];

/// `bytes` admitted for the decoder, or `None` when any reading of them could carry an
/// entity declaration.
pub(super) fn admit(bytes: &[u8]) -> Option<Admitted> {
    safe(bytes).then(|| Admitted(bytes.to_vec()))
}

fn safe(bytes: &[u8]) -> bool {
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut frontier = vec![bytes.to_vec()];
    let mut total = 0usize;
    while let Some(reading) = frontier.pop() {
        if !seen.insert(reading.clone()) {
            continue;
        }
        total += reading.len();
        if seen.len() > MAX_READINGS || total > MAX_READING_BYTES || forbidden(&reading) {
            return false;
        }
        if could_hold_markup(&reading) {
            frontier.extend(next_readings(&reading));
        }
    }
    true
}

/// Whether undoing more layers of `reading` could still find markup in it: it is mostly
/// text, mostly UTF-16 text, or holds a gzip signature. A base64 reading of text that
/// was never base64 is noise, and expanding noise is what made this model slow; it is
/// still checked for the words, only not read further. A single mutated base64
/// character garbles three bytes, which leaves a text payload well inside the bound.
fn could_hold_markup(reading: &[u8]) -> bool {
    let text = reading
        .iter()
        .filter(|b| b.is_ascii_graphic() || b.is_ascii_whitespace() || **b >= 0x80)
        .count();
    let nul = reading.iter().filter(|&&b| b == 0).count();
    (text + nul) * 4 >= reading.len() * 3 || reading.windows(2).any(|w| w == [0x1F, 0x8B])
}

fn forbidden(reading: &[u8]) -> bool {
    FORBIDDEN
        .iter()
        .any(|word| find_ci(reading, word, 0).is_some())
        || UNREADABLE_SIGNATURES
            .iter()
            .any(|sig| reading.starts_with(sig))
        || declares_unreadable_encoding(reading)
}

fn declares_unreadable_encoding(reading: &[u8]) -> bool {
    let mut from = 0;
    while let Some(at) = find_ci(reading, b"encoding", from) {
        from = at + 1;
        let rest = &reading[at + b"encoding".len()..];
        let rest = trim(rest);
        let Some(rest) = rest.strip_prefix(b"=") else {
            continue;
        };
        let rest = trim(rest);
        let Some((&quote, rest)) = rest.split_first() else {
            continue;
        };
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        let value = rest.split(|&b| b == quote).next().unwrap_or_default();
        let value = String::from_utf8_lossy(value).to_ascii_lowercase();
        if !READABLE_ENCODINGS.contains(&value.as_str()) {
            return true;
        }
    }
    false
}

fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

/// Every reading one undone layer away from `reading`.
fn next_readings(reading: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![normalised(reading), percent_decoded(reading)];
    out.extend(base64_runs(reading));
    out.extend(gunzipped(reading));
    if reading.contains(&0) {
        for big_endian in [false, true] {
            for offset in [0, 1] {
                out.push(utf16_as_ascii(
                    &reading[offset.min(reading.len())..],
                    big_endian,
                ));
            }
        }
    }
    out.retain(|next| next != reading);
    out
}

/// References expanded, CSS escapes undone, whitespace and control bytes removed, and
/// markup removed, repeated until nothing changes: every order in which an XML parser,
/// a CSS parser and a URL parser could each undo one of these.
fn normalised(reading: &[u8]) -> Vec<u8> {
    let mut text = reading.to_vec();
    for _ in 0..8 {
        let next = without_markup(&without_whitespace(&css_unescaped(&references_expanded(
            &text,
        ))));
        if next == text {
            break;
        }
        text = next;
    }
    text
}

fn as_byte(code_point: u32) -> u8 {
    u8::try_from(code_point)
        .ok()
        .filter(u8::is_ascii)
        .unwrap_or(b'?')
}

fn references_expanded(text: &[u8]) -> Vec<u8> {
    const NAMED: &[(&[u8], u8)] = &[
        (b"&lt;", b'<'),
        (b"&gt;", b'>'),
        (b"&amp;", b'&'),
        (b"&quot;", b'"'),
        (b"&apos;", b'\''),
    ];
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'&' {
            if let Some((byte, len)) = NAMED
                .iter()
                .find(|(name, _)| {
                    text[i..].len() >= name.len()
                        && text[i..i + name.len()].eq_ignore_ascii_case(name)
                })
                .map(|(name, b)| (*b, name.len()))
                .or_else(|| numeric_reference(&text[i..]))
            {
                out.push(byte);
                i += len;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

fn numeric_reference(rest: &[u8]) -> Option<(u8, usize)> {
    let body = rest.strip_prefix(b"&#")?;
    let (radix, digits_from) = match body.first() {
        Some(b'x' | b'X') => (16, 1),
        _ => (10, 0),
    };
    let digits = &body[digits_from..];
    let len = digits.iter().take_while(|b| b.is_ascii_hexdigit()).count();
    if len == 0 || digits.get(len) != Some(&b';') {
        return None;
    }
    let value = u32::from_str_radix(std::str::from_utf8(&digits[..len]).ok()?, radix).ok()?;
    Some((as_byte(value), 2 + digits_from + len + 1))
}

fn css_unescaped(text: &[u8]) -> Vec<u8> {
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
            .take(6)
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        if hex > 0 {
            let value = u32::from_str_radix(std::str::from_utf8(&rest[..hex]).unwrap_or("0"), 16)
                .unwrap_or(0);
            out.push(as_byte(value));
            i += 1 + hex;
            if text[i..].starts_with(b"\r\n") {
                i += 2;
            } else if text.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
                i += 1;
            }
        } else if matches!(rest[0], b'\n' | b'\r' | 0x0C) {
            i += 2;
        } else {
            out.push(rest[0]);
            i += 2;
        }
    }
    out
}

fn without_whitespace(text: &[u8]) -> Vec<u8> {
    text.iter()
        .copied()
        .filter(|&b| b > b' ' && b != 0x7F)
        .collect()
}

/// Comments, processing instructions, CDATA delimiters and tags removed — what joins a
/// payload split across a `<style>` element's children.
fn without_markup(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let end = if rest.starts_with(b"<!--") {
            find(rest, b"-->").map(|e| e + 3)
        } else if rest.starts_with(b"<?") {
            find(rest, b"?>").map(|e| e + 2)
        } else if rest.starts_with(b"<![CDATA[") {
            Some(9)
        } else if rest.starts_with(b"]]>") {
            Some(3)
        } else if rest.len() > 1
            && rest[0] == b'<'
            && (rest[1].is_ascii_alphabetic() || rest[1] == b'/')
        {
            find(rest, b">").map(|e| e + 1)
        } else {
            None
        };
        match end {
            Some(len) => i += len,
            None => {
                out.push(text[i]);
                i += 1;
            }
        }
    }
    out
}

fn percent_decoded(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'%' && i + 2 < text.len() {
            let digits = std::str::from_utf8(&text[i + 1..i + 3]).unwrap_or("zz");
            if let Ok(byte) = u8::from_str_radix(digits, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// A forgiving base64 decode of the run after each comma, where every `data:` body
/// starts (an escaped comma is a real one in the normalised reading),
/// skipping whitespace and stopping at the first byte outside the alphabet.
fn base64_runs(text: &[u8]) -> Vec<Vec<u8>> {
    text.iter()
        .enumerate()
        .filter(|(_, &b)| b == b',')
        .map(|(i, _)| i + 1)
        .filter_map(|start| {
            let decoded = base64_decode(&text[start..]);
            (decoded.len() * 4 / 3 >= MIN_BASE64_RUN).then_some(decoded)
        })
        .collect()
}

fn base64_value(b: u8) -> Option<u32> {
    Some(u32::from(match b {
        b'A'..=b'Z' => b - b'A',
        b'a'..=b'z' => b - b'a' + 26,
        b'0'..=b'9' => b - b'0' + 52,
        b'+' | b'-' => 62,
        b'/' | b'_' => 63,
        _ => return None,
    }))
}

fn base64_decode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in text {
        if b.is_ascii_whitespace() || b == b'=' {
            continue;
        }
        let Some(value) = base64_value(b) else {
            break;
        };
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// What inflates from every gzip signature in `text`, as far as it inflates.
fn gunzipped(text: &[u8]) -> Vec<Vec<u8>> {
    (0..text.len().saturating_sub(1))
        .filter(|&i| text[i] == 0x1F && text[i + 1] == 0x8B)
        .map(|i| {
            let mut out = Vec::new();
            let _ = flate2::read::MultiGzDecoder::new(&text[i..])
                .take(MAX_READING_BYTES as u64)
                .read_to_end(&mut out);
            out
        })
        .filter(|out| !out.is_empty())
        .collect()
}

fn utf16_as_ascii(bytes: &[u8], big_endian: bool) -> Vec<u8> {
    let units = bytes.chunks_exact(2).map(|pair| {
        if big_endian {
            u16::from_be_bytes([pair[0], pair[1]])
        } else {
            u16::from_le_bytes([pair[0], pair[1]])
        }
    });
    char::decode_utf16(units)
        .map(|c| c.map_or(b'?', |c| as_byte(u32::from(c))))
        .collect()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn find_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|at| at + from)
}

#[cfg(test)]
mod tests {
    use super::super::grammar::{base64, gzip, percent, utf16, Core};
    use super::*;

    /// Gate: an entity or DOCTYPE hidden under each layer the generator composes, and
    /// under two of them, is dropped; the core it wraps, under the same layers, is not.
    /// Both halves matter — a model that dropped everything would pass the first alone.
    #[test]
    fn no_input_that_decodes_to_an_entity_is_admitted() {
        let core = Core::PLAIN.text();
        let planted = core.replacen("<svg", "<!DOCTYPE svg [<!ENTITY e 'x'>]><svg", 1);
        type Layer = (&'static str, fn(&str) -> String);
        let layers: &[Layer] = &[
            ("plain", |d| d.to_owned()),
            ("references", |d| {
                d.replace('E', "&#69;").replace('D', "&#x44;")
            }),
            ("css escapes", |d| {
                d.replace('E', "\\45 ").replace('D', "\\44\x0c")
            }),
            ("percent", |d| percent(d.as_bytes(), true, false)),
            ("reference then percent", |d| {
                percent(d.as_bytes(), true, false).replace('%', "&#37;")
            }),
            ("base64", |d| {
                format!(
                    "<image href=\"data:x;base64,{}\"/>",
                    base64(d.as_bytes(), true)
                )
            }),
            ("split by a comment", |d| {
                d.replace("ENTITY", "EN<!-- x -->TITY")
                    .replace("DOCTYPE", "DOC<x/>TYPE")
            }),
            ("split by a tab", |d| {
                d.replace("ENTITY", "EN&#9;TITY")
                    .replace("DOCTYPE", "DOC\nTYPE")
            }),
            ("base64 of percent", |d| {
                format!(
                    ",{}",
                    base64(percent(d.as_bytes(), true, true).as_bytes(), false)
                )
            }),
        ];
        for (name, layer) in layers {
            assert!(
                admit(layer(&planted).as_bytes()).is_none(),
                "{name}: entity admitted"
            );
            assert!(
                admit(layer(&core).as_bytes()).is_some(),
                "{name}: the core was dropped"
            );
        }
        for (name, wrap) in [
            ("gzip", gzip as fn(&[u8]) -> Vec<u8>),
            ("utf-16", |d: &[u8]| utf16(d, false, false, false)),
            ("utf-16 be, odd", |d: &[u8]| {
                [&[b' '][..], &utf16(d, true, true, true)].concat()
            }),
            ("gzip in base64", |d: &[u8]| {
                format!(",{}", base64(&gzip(d), true)).into_bytes()
            }),
        ] {
            assert!(
                admit(&wrap(planted.as_bytes())).is_none(),
                "{name}: entity admitted"
            );
            assert!(
                admit(&wrap(core.as_bytes())).is_some(),
                "{name}: the core was dropped"
            );
        }
        for unreadable in [
            r#"<?xml version="1.0" encoding="UTF-7"?><svg/>"#.as_bytes(),
            &[0, 0, 0, b'<', 0, 0, 0, b's'],
        ] {
            assert!(admit(unreadable).is_none(), "{unreadable:?}");
        }
    }
}
