use super::*;
use std::io::Write;

/// The published trigger's shape: an `xi:include` of a `data:` URI.
const ATTACK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude" width="8" height="8"><xi:include href="data:text/xml,%3C!DOCTYPE%20a%5B%3C!ENTITY%20e%20'x'%3E%5D%3E%3Ca%3E%26e;%3C/a%3E" parse="xml"/></svg>"#;

/// A plain badge-style SVG with nothing to refuse.
const BENIGN: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="90" height="20">
  <style>.t { font: 11px sans-serif; fill: url(#g); }</style>
  <linearGradient id="g"><stop offset="0" stop-color="#bbb"/></linearGradient>
  <rect width="90" height="20" rx="3" fill="#555"/>
  <text x="45" y="14" class="t">build &amp; test &#169; &lt;ok&gt;</text>
  <image width="1" height="1" xlink:href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="/>
</svg>"##;

fn utf16(text: &str, big_endian: bool, bom: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let units = bom
        .then_some(0xFEFF_u16)
        .into_iter()
        .chain(text.encode_utf16());
    for unit in units {
        out.extend(if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        });
    }
    out
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).expect("gzip write");
    enc.finish().expect("gzip finish")
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &b)| acc | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn refused(bytes: impl AsRef<[u8]>) -> Option<Reason> {
    inspect(bytes.as_ref()).err()
}

#[test]
fn the_published_trigger_is_refused() {
    assert_eq!(refused(ATTACK), Some(Reason::XInclude));
}

#[test]
fn the_namespace_is_refused_under_any_prefix_or_as_the_default() {
    for svg in [
        r#"<svg xmlns:zz="http://www.w3.org/2001/XInclude"><zz:include href="a"/></svg>"#,
        r#"<svg><include xmlns="http://www.w3.org/2001/XInclude" href="a"/></svg>"#,
        r#"<svg xmlns:q='http://www.w3.org/2003/XInclude'/>"#,
        r#"<svg xmlns:q="HTTP://WWW.W3.ORG/2001/XINCLUDE"/>"#,
    ] {
        assert_eq!(refused(svg), Some(Reason::XInclude), "{svg}");
    }
}

#[test]
fn a_namespace_spelled_with_references_or_split_by_newlines_is_refused() {
    for svg in [
        r#"<svg xmlns:a="http://www.w3.org/2001/&#88;Include"/>"#,
        r#"<svg xmlns:a="http://www.w3.org/2001/&#x58;&#x49;nclude"/>"#,
        r#"<svg xmlns:a="http://www.w3.org/2001/XIn&#000099;lude"/>"#,
        "<svg xmlns:a=\"http://www.w3.org/2001/XIn\nclude\"/>",
    ] {
        assert_eq!(refused(svg), Some(Reason::XInclude), "{svg}");
    }
}

#[test]
fn entity_declarations_and_foreign_dtds_are_refused() {
    assert_eq!(
        refused(r#"<!DOCTYPE svg [<!ENTITY ns "http://www.w3.org/2001/X&#37;49nclude">]><svg/>"#),
        Some(Reason::EntityDeclaration)
    );
    assert_eq!(
        refused(r#"<!DOCTYPE svg [ <!-- subset --> ]><svg/>"#),
        Some(Reason::Doctype)
    );
    assert_eq!(
        refused(r#"<!DOCTYPE svg SYSTEM "data:application/xml-dtd,x"><svg/>"#),
        Some(Reason::Doctype)
    );
    assert_eq!(
        refused(r#"<!DOCTYPE svg SYSTEM "evil.dtd"><svg/>"#),
        Some(Reason::Doctype)
    );
    assert_eq!(refused(r#"<!DOCTYPE svg"#), Some(Reason::Doctype));
}

#[test]
fn utf16_in_either_byte_order_with_or_without_a_bom_is_read() {
    for big_endian in [false, true] {
        for bom in [false, true] {
            assert_eq!(
                refused(utf16(ATTACK, big_endian, bom)),
                Some(Reason::XInclude),
                "big_endian={big_endian} bom={bom}"
            );
            assert_eq!(refused(utf16(BENIGN, big_endian, bom)), None);
        }
    }
}

#[test]
fn a_mid_stream_switch_to_utf16_at_an_odd_offset_is_read() {
    let mut bytes = b"<?xml version='1.0' encoding='UTF-16'?>x".to_vec();
    bytes.extend(utf16(ATTACK, false, false));
    assert_eq!(refused(bytes), Some(Reason::XInclude));
}

#[test]
fn unreadable_encodings_are_refused() {
    assert_eq!(
        refused([0x4C, 0x6F, 0xA7, 0x94, 0x40]),
        Some(Reason::Encoding)
    );
    assert_eq!(
        refused([0, 0, 0, 0x3C, 0, 0, 0, 0x73]),
        Some(Reason::Encoding)
    );
    for enc in ["UTF-7", "ISO-2022-JP", "IBM037", "UCS-4", "Shift_JIS"] {
        let svg = format!(r#"<?xml version="1.0" encoding="{enc}"?><svg/>"#);
        assert_eq!(refused(&svg), Some(Reason::Encoding), "{enc}");
    }
    assert_eq!(
        refused(r#"<?xml version="1.0" encoding=UTF-7?><svg/>"#),
        Some(Reason::Encoding),
        "an unquoted value is unreadable, so refused"
    );
    for separator in ["\n", "\t", "\r\n"] {
        let svg = format!(r#"<?xml{separator}version="1.0"{separator}encoding="UTF-7"?><svg/>"#);
        assert_eq!(refused(&svg), Some(Reason::Encoding), "{separator:?}");
    }
    for enc in ["utf-8", "ISO-8859-1", "windows-1252", "US-ASCII", "UTF-16"] {
        let svg = format!(r#"<?xml version="1.0" encoding = '{enc}' ?><svg/>"#);
        assert_eq!(refused(&svg), None, "{enc}");
    }
}

#[test]
fn a_gzipped_svgz_is_inflated_and_screened() {
    assert_eq!(refused(gzip(ATTACK.as_bytes())), Some(Reason::XInclude));
    assert_eq!(
        refused(gzip(&gzip(ATTACK.as_bytes()))),
        Some(Reason::XInclude)
    );
    assert_eq!(refused(gzip(BENIGN.as_bytes())), None);
    // What follows the last member is screened too, not dropped.
    let mut trailed = gzip(BENIGN.as_bytes());
    trailed.extend_from_slice(ATTACK.as_bytes());
    assert_eq!(refused(&trailed), Some(Reason::XInclude));
    let mut padded = gzip(BENIGN.as_bytes());
    padded.extend_from_slice(b"\n\n");
    assert_eq!(refused(&padded), None, "harmless trailing bytes pass");
    assert_eq!(
        refused([0x1f, 0x8b, 0x08, 0x00, 0xde, 0xad]),
        Some(Reason::Undecodable)
    );
}

#[test]
fn a_nested_document_in_a_data_uri_is_screened() {
    let base64_svg = format!(
        r#"<svg xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="data:image/svg+xml;base64,{}"/></svg>"#,
        base64(ATTACK.as_bytes())
    );
    assert_eq!(refused(&base64_svg), Some(Reason::XInclude));
    let gz_base64 = format!(
        r#"<svg><use href="data:image/svg+xml;base64,{}#x"/></svg>"#,
        base64(&gzip(ATTACK.as_bytes()))
    );
    assert_eq!(refused(&gz_base64), Some(Reason::XInclude));
    assert_eq!(
        refused(
            r#"<svg><image href="data:image/svg+xml,%3Csvg xmlns:a=%22http://www.w3.org/2001/%58Include%22/%3E"/></svg>"#
        ),
        Some(Reason::XInclude)
    );
    assert_eq!(
        refused(
            r#"<svg><style>rect { fill: url(data\3a image/svg+xml;base64,AAAA!) }</style></svg>"#
        ),
        None,
        "a payload that stops at a non-base64 character decodes the prefix"
    );
    assert_eq!(
        refused(r#"<svg><image href="data:image/svg+xml;base64,QUJD="/></svg>"#),
        Some(Reason::Undecodable),
        "padding in the wrong place does not decode, so refuses"
    );
}

#[test]
fn a_percent_encoded_payload_is_screened_past_any_literal_quote() {
    // Optimiser output: single-quoted attributes inside a double-quoted href.
    let svg = r#"<svg><image href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' xmlns:a='http://www.w3.org/2001/%58Include'/%3E"/></svg>"#;
    assert_eq!(refused(svg), Some(Reason::XInclude));
    let svg = r#"<svg><image href='data:image/svg+xml,%3Csvg xmlns:a="http://www.w3.org/2001/%58%49nclude"/%3E'/></svg>"#;
    assert_eq!(refused(svg), Some(Reason::XInclude));
}

#[test]
fn a_data_uri_header_that_cannot_be_delimited_or_bounded_still_screens() {
    let payload = base64(ATTACK.as_bytes());
    let long = format!(
        r#"<svg><image href="data:image/svg+xml;a={};base64,{payload}"/></svg>"#,
        "A".repeat(300)
    );
    assert_eq!(refused(&long), Some(Reason::XInclude), "a long header");
    let quoted = format!(r#"<svg><image href="data:image/svg+xml;q=';base64,{payload}"/></svg>"#);
    assert_eq!(
        refused(&quoted),
        Some(Reason::XInclude),
        "a quote in the header"
    );
    assert_eq!(
        refused(r#"<svg><text>data: none</text></svg>"#),
        None,
        "no comma ends a header, so there is no URI, only prose"
    );
}

/// R9-GTK2-06: prose holding `data:` is decided by what it says, not by whether a
/// comma happens to come later in the file; and a URI whose comma is encoded inside
/// an encoded payload is still decoded and screened, with that payload.
///
/// Mutations, each failing here: refusing a start no comma follows (the first two
/// asserts); dropping such a start without its text joining the segment before it
/// (the encoded-comma asserts).
#[test]
fn prose_holding_data_is_decided_without_regard_to_a_later_comma() {
    let prose = "<svg><title>Source data: World Bank</title><path d='M0 0 L1 1'/>";
    assert_eq!(refused(format!("{prose}</svg>")), None);
    assert_eq!(refused(format!("{prose}<desc>a, b</desc></svg>")), None);
    let after_an_icon = format!(
        r#"<svg><image href="data:image/svg+xml,%3Csvg/%3E"/>{}</svg>"#,
        "<title>Source data: World Bank</title>"
    );
    assert_eq!(refused(after_an_icon), None);
    let hostile = base64(ATTACK.as_bytes());
    for comma in ["%2C", "%252C"] {
        let encoded = format!(
            r#"<svg><image href="data:image/svg+xml,%3Csvg%3E%3Cimage href='data:image/svg+xml;base64{comma}{hostile}'/%3E%3C/svg%3E"/></svg>"#
        );
        assert_eq!(refused(&encoded), Some(Reason::XInclude), "{comma}");
    }
}

#[test]
fn a_base64_payload_is_not_cut_short_by_escaped_whitespace() {
    let payload = base64(ATTACK.as_bytes());
    let (head, tail) = payload.split_at(8);
    let svg = format!(
        r#"<svg><style>rect {{ fill: url(data:image/svg+xml;base64,{head}\c {tail}) }}</style></svg>"#
    );
    assert_eq!(refused(&svg), Some(Reason::XInclude));
}

#[test]
fn sibling_percent_encoded_uris_neither_nest_nor_compound() {
    let icon = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg'/%3E";
    let images: String = (0..=2 * MAX_DEPTH)
        .map(|_| format!(r#"<image href="{icon}"/>"#))
        .collect();
    assert_eq!(refused(format!("<svg>{images}</svg>")), None);
}

/// A literal `data:` inside a percent-encoded document must not split
/// one real payload so that its bytes go unscreened.
#[test]
fn a_literal_data_scheme_inside_an_encoded_document_hides_nothing() {
    // The cut after `a='` makes `xmlns:xi=…%58Include…` a "header" up to `b=','`.
    let header_hidden = r#"<svg><image href="data:image/svg+xml,%3Csvg a='data:' xmlns:xi='http://www.w3.org/2001/%58Include' b=','%3E%3Cxi:include href='data:text/plain,x'/%3E%3C/svg%3E"/></svg>"#;
    assert_eq!(refused(header_hidden), Some(Reason::XInclude));
    // A URI written literally inside an encoded one is decoded twice by a real
    // decoder: once with the outer layer, once as itself.
    let twice = r#"<svg><image href="data:image/svg+xml,%3Csvg%3E%3Cimage href='data:image/svg+xml,%253Csvg xmlns:a=%2522http://www.w3.org/2001/%2558Include%2522/%253E'/%3E%3C/svg%3E"/></svg>"#;
    assert_eq!(refused(twice), Some(Reason::XInclude));
    // An SVG nested in an encoded SVG and encoded again, its `:`, `,` and `'` left
    // literal, so the inner `data:` follows a quote and is a start of its own (one
    // after `%27` is read as the tail of a word and not cut at all): a real decoder
    // decodes its payload twice. As text, and as gzip.
    let enc = |b: &[u8]| -> String {
        b.iter()
            .map(|&c| {
                if c.is_ascii_alphanumeric() || matches!(c, b':' | b',' | b'/' | b'.' | b'\'') {
                    char::from(c).to_string()
                } else {
                    format!("%{c:02X}")
                }
            })
            .collect()
    };
    // Padded so deflate really compresses it: a short document is stored, and its
    // text would then be screened in the clear without the gzip layer ever opening.
    let padded = format!("{ATTACK}<!-- {} -->", "pad ".repeat(500));
    let packed = gzip(padded.as_bytes());
    assert!(
        find_ci(&packed, b"xinclude", 0).is_none(),
        "precondition: the namespace is not readable in the compressed bytes"
    );
    let double = |inner: &[u8]| {
        let middle = format!(
            "<svg><image href='data:image/svg+xml,{}'/></svg>",
            enc(inner)
        );
        format!(
            r#"<svg><image href="data:image/svg+xml,{}"/></svg>"#,
            enc(middle.as_bytes())
        )
    };
    for (what, inner) in [("text", ATTACK.as_bytes().to_vec()), ("gzip", packed)] {
        assert_eq!(
            refused(double(&inner)),
            Some(Reason::XInclude),
            "double-encoded {what}"
        );
    }
    // The benign twins, built the same way, pass: each refusal above is the
    // content's, not the wrapping's. The stored one carries `%41` in its compressed
    // bytes, which decoding the gzip level once more as text would corrupt into a
    // refusal (mutation-checked: dropping `inspect_every_decoding`'s gzip stop
    // fails it as `Undecodable`).
    let benign_padded = format!("{BENIGN}<!-- 100%41 {} -->", "pad ".repeat(500));
    let mut stored = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::none());
    stored
        .write_all(benign_padded.as_bytes())
        .expect("gzip write");
    let stored = stored.finish().expect("gzip finish");
    assert!(
        find_ci(&stored, b"%41", 0).is_some(),
        "precondition: the stored gzip carries a %XX-shaped run"
    );
    for (what, inner) in [
        ("text", BENIGN.as_bytes().to_vec()),
        ("gzip", gzip(benign_padded.as_bytes())),
        ("stored gzip", stored),
    ] {
        assert_eq!(
            refused(double(&inner)),
            None,
            "double-encoded benign {what}"
        );
    }
    // `%da` decodes to one byte, so this `data:` never existed and is not a start.
    // The comma after the namespace would make the false start a URL, its "header"
    // holding the namespace. With the rule this is one segment, refused whole; with
    // only the rule dropped the whole-tail pass still refuses it; with both dropped
    // it passes (mutation-checked, all three).
    let escaped_start = r#"<svg><image href="data:image/svg+xml,%3Csvg x='%data:' y='%5data:' xmlns:a='http://www.w3.org/2001/%58Include' b=','/%3E"/></svg>"#;
    assert_eq!(refused(escaped_start), Some(Reason::XInclude));
    // Many honest siblings, some with literal `<` and `,`, still pass.
    let icon = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0,0,8,8'/>";
    let images: String = (0..=2 * MAX_DEPTH)
        .map(|_| format!(r#"<image href="{icon}"/>"#))
        .collect();
    assert_eq!(refused(format!("<svg>{images}</svg>")), None);
}

/// R10-DRY1-01: between two percent layers a real decoder parses the nested
/// document, expanding its references, so `&#37;1F` is one layer from 0x1F. A
/// gzipped XInclude SVG written that way inside an encoded document, its inner
/// `data:` literal, must be inflated and refused; and an honest `.svgz` whose
/// compressed bytes happen to read as `%41` must not be "decoded" into a refusal.
///
/// Two mechanisms each refuse the reference case on their own (GTK4Rs/AP-254's
/// shape): the per-segment walk through [`next_layer`], and the whole-tail reading,
/// whose full inspection expands references before it cuts. Mutation-checked as a
/// pair: with the whole-tail reading off this still passes, and with the reference
/// layer also dropped it fails. Dropping the gzip stop fails the `.svgz` half.
#[test]
fn a_reference_layer_between_percent_layers_hides_nothing() {
    let literal = |b: &[u8], escape: &dyn Fn(u8) -> String| -> String {
        b.iter()
            .map(|&c| {
                if c.is_ascii_alphanumeric() || matches!(c, b':' | b',' | b'/' | b'.' | b'\'') {
                    char::from(c).to_string()
                } else {
                    escape(c)
                }
            })
            .collect()
    };
    let pct = |c: u8| format!("%{c:02X}");
    let pad = |doc: &str| format!("{doc}<!-- {} -->", "pad ".repeat(500));
    // Each document, gzipped and plain, written through the reference layer: the
    // hostile one must be refused for naming the namespace, and its benign twin, built
    // the same way, must pass, so the refusal is the content's and not the route's.
    for (form, reference) in [("decimal", "&#37;"), ("hex", "&#x25;")] {
        let route = |doc: &[u8]| {
            let inner = literal(doc, &|c| format!("{reference}{c:02X}"));
            let middle = format!("<svg><image href='data:image/svg+xml,{inner}'/></svg>");
            format!(
                r#"<svg><image href="data:image/svg+xml,{}"/></svg>"#,
                literal(middle.as_bytes(), &pct)
            )
        };
        for (doc, expect) in [(ATTACK, Some(Reason::XInclude)), (BENIGN, None)] {
            let padded = pad(doc);
            assert_eq!(
                refused(route(&gzip(padded.as_bytes()))),
                expect,
                "{form} reference layer, gzipped"
            );
            assert_eq!(
                refused(route(padded.as_bytes())),
                expect,
                "{form} reference layer, plain"
            );
        }
    }

    let mut stored = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::none());
    stored
        .write_all(b"<svg xmlns='http://www.w3.org/2000/svg'><desc>100%41 sure</desc></svg>")
        .expect("gzip write");
    let stored = stored.finish().expect("gzip finish");
    assert!(
        find_ci(&stored, b"%41", 0).is_some(),
        "precondition: the stored gzip carries a %XX-shaped run"
    );
    let svgz = format!(
        r#"<svg><image href="data:image/svg+xml,{}"/><image href="data:,x"/></svg>"#,
        literal(&stored, &pct)
    );
    assert_eq!(
        refused(&svgz),
        None,
        "an honest svgz is inflated, not text-decoded"
    );
}

/// R10-AP2-01 / R10-AP2-02: the header is read the way the decoder reads it, so
/// every spelling of `;base64` that a decoder accepts gets the base64 reading — and
/// a hostile payload under each is refused while its benign twin passes.
#[test]
fn every_spelling_of_a_base64_header_is_read_as_base64() {
    let hostile = base64(ATTACK.as_bytes());
    let benign = base64(BENIGN.as_bytes());
    for header in [
        "image/svg+xml; base64",
        "image/svg+xml;BASE64",
        "image/svg+xml;charset=utf-8;base64",
        // These two reach the header with the tab or newline already gone (the
        // screen's normalisation drops both), so they pin that normalisation, not
        // the header rule; `decoder_reads_base64` is held to its own filter below.
        "image/svg+xml;\tbase64",
        "image/svg+xml;\nbase64",
        "image/svg+xml%3Bbase64",
        "image/svg+xml;base64 ",
    ] {
        let wrap = |b64: &str| format!(r#"<svg><image href="data:{header},{b64}"/></svg>"#);
        assert_eq!(
            refused(wrap(&hostile)),
            Some(Reason::XInclude),
            "hostile under {header:?}"
        );
        assert_eq!(refused(wrap(&benign)), None, "benign under {header:?}");
    }
    // The decoder's rule itself, on headers the normalisation never hands it: the URL
    // parser drops a tab or newline anywhere in the header.
    for header in [
        &b"image/svg+xml;\tbase64"[..],
        b"image/svg+xml;\nbase64",
        b"x;ba\r\nse64",
    ] {
        assert!(
            decoder_reads_base64(header),
            "{}",
            String::from_utf8_lossy(header)
        );
    }
    // R10-AP2-01: a literal inner `data:` inside a percent-encoded document, its
    // header written `%3Bbase64`.
    let enc = |b: &[u8]| -> String {
        b.iter()
            .map(|&c| {
                if c.is_ascii_alphanumeric() || matches!(c, b':' | b',' | b'/' | b'.' | b'\'') {
                    char::from(c).to_string()
                } else {
                    format!("%{c:02X}")
                }
            })
            .collect()
    };
    for (what, payload, expect) in [
        ("hostile", &hostile, Some(Reason::XInclude)),
        ("benign", &benign, None),
    ] {
        let middle = format!("<svg><image href='data:image/svg+xml;base64,{payload}'/></svg>");
        let outer = format!(
            r#"<svg><image href="data:image/svg+xml,{}"/></svg>"#,
            enc(middle.as_bytes())
        );
        assert_eq!(refused(&outer), expect, "{what} %3Bbase64 inner");
    }
}

/// The converse of the matrix above: a header that only LOOKS like `;base64` once
/// something decodes it is percent to the decoder, which reads the raw header. Each
/// carries a percent-hostile body that a base64 reading empties (it stops at the first
/// `&`), so a screen that took the base64 reading alone would let the nested document
/// through.
#[test]
fn a_header_that_spells_base64_only_once_decoded_is_also_read_as_percent() {
    let attr = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    // No `data:` of its own: a second start would bring in the spliced reading, which
    // is not the reading under test.
    let hostile = attr(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/X%49nclude"><xi:include href="x.xml" parse="xml"/></svg>"#,
    );
    let benign = attr(BENIGN);
    for header in [
        "image/svg+xml;base64%20",
        "image/svg+xml;%20base64",
        "image/svg+xml%3Bbase64",
        "image/svg+xml;x=%3Bbase64",
        "image/svg+xml;base%364",
        "image/svg+xml;\\62 ase64",
    ] {
        let wrap = |body: &str| format!(r#"<svg><image href="data:{header},{body}"/></svg>"#);
        assert_eq!(
            refused(wrap(&hostile)),
            Some(Reason::XInclude),
            "percent-hostile under {header:?}"
        );
        assert_eq!(refused(wrap(&benign)), None, "benign under {header:?}");
    }
    // In a stylesheet the decoder DOES undo CSS escapes, and `\c ` is a form feed, which
    // is not one of the spaces the base64 rule allows.
    let css = |body: &str| {
        format!(
            r#"<svg><style>rect {{ fill: url(data:image/svg+xml;\c base64,{body}) }}</style></svg>"#
        )
    };
    assert_eq!(
        refused(css(&hostile)),
        Some(Reason::XInclude),
        "percent-hostile under a CSS form feed"
    );
    assert_eq!(refused(css(&benign)), None, "benign under a CSS form feed");
}

/// A header that names base64 where the decoder's rule does not look for it (a
/// parameter after it, or inside another word) is percent to the decoder, and the
/// screen reads it both ways: a base64 body that hides the namespace is refused, as it
/// would be by a decoder that took the looser reading, and a benign one passes.
/// Mutation-checked: reading such a header as percent alone leaves the hostile body
/// unread.
#[test]
fn a_header_naming_base64_off_the_end_is_read_both_ways() {
    let hostile = base64(ATTACK.as_bytes());
    let benign = base64(BENIGN.as_bytes());
    for header in [
        "image/svg+xml;base64;charset=utf-8",
        "image/svg+xml;x-base64",
    ] {
        assert!(!decoder_reads_base64(header.as_bytes()), "{header:?}");
        let wrap = |b64: &str| format!(r#"<svg><image href="data:{header},{b64}"/></svg>"#);
        assert_eq!(
            refused(wrap(&hostile)),
            Some(Reason::XInclude),
            "hostile under {header:?}"
        );
        assert_eq!(refused(wrap(&benign)), None, "benign under {header:?}");
    }
}

/// A layer holding a NUL is read four more times, as UTF-16 at both offsets in both
/// byte orders, and each reading is charged its code units: one byte short of the
/// total refuses. Mutation-checked: dropping that charge passes the short budget.
#[test]
fn every_utf16_reading_is_charged() {
    let bytes = b"<svg/>\0x";
    let readings = 2 * (bytes.len() / 2) + 2 * ((bytes.len() - 1) / 2);
    let exact = bytes.len() + readings;
    let run = |scan| inspect_at(bytes, 0, &mut Budget::new(scan, 0));
    assert_eq!(run(exact), Ok(()));
    assert_eq!(run(exact - 1), Err(Reason::TooLarge));
}

#[test]
fn a_data_scheme_spelled_with_css_escapes_or_references_is_still_found() {
    let hidden = format!(
        r#"<svg><style>rect {{ fill: url(da\74 a:image/svg+xml;base64,{}) }}</style><image href="&#100;ata:image/svg+xml;base64,{}"/></svg>"#,
        base64(ATTACK.as_bytes()),
        base64(ATTACK.as_bytes())
    );
    assert_eq!(refused(&hidden), Some(Reason::XInclude));
}

#[test]
fn nesting_past_the_depth_limit_is_refused() {
    let mut bytes = b"<svg/>".to_vec();
    for _ in 0..=MAX_DEPTH {
        bytes = gzip(&bytes);
    }
    assert_eq!(refused(bytes), Some(Reason::TooDeep));
}

/// A percent payload still decoding after [`MAX_DEPTH`] levels is refused as too
/// deep, and one that settles at the last level is not. Mutation-checked: returning
/// `Ok` in place of `inspect_every_decoding`'s final `TooDeep` fails here.
#[test]
fn a_percent_payload_decoding_past_the_depth_limit_is_refused() {
    // `data:` decodes the payload once; each `25` after the `%` is one more level.
    let chain = |levels: usize| {
        format!(
            r#"<svg><image href="data:,%{}41"/></svg>"#,
            "25".repeat(levels)
        )
    };
    assert_eq!(refused(chain(MAX_DEPTH)), None);
    assert_eq!(refused(chain(MAX_DEPTH + 1)), Some(Reason::TooDeep));
}

/// The budget bounds what one screen inspects, layer by layer: a plain document is
/// charged its length, a gzip layer may inflate only to what is left, and a nested
/// `data:` payload is charged against the same budget as the document around it.
///
/// Mutations, each failing here: `saturating_sub` for `checked_sub`; dropping the
/// `+ 1` from `gunzip`'s take (a payload cut at the budget would then be screened
/// truncated rather than refused); dropping `gunzip`'s length check; handing the
/// nested payload a fresh budget, or a copy of what is left.
#[test]
fn the_scan_budget_bounds_every_layer_and_refuses_past_it() {
    let scan_only = |scan: usize| Budget::new(scan, 0);
    let plain = b"<svg/>";
    assert_eq!(inspect_at(plain, 0, &mut scan_only(plain.len())), Ok(()));
    assert_eq!(
        inspect_at(plain, 0, &mut scan_only(plain.len() - 1)),
        Err(Reason::TooLarge)
    );

    let bomb = gzip(&[0; 4096]);
    assert!(bomb.len() < 1024, "the bomb is small before inflating");
    assert_eq!(
        inspect_at(&bomb, 0, &mut scan_only(1024)),
        Err(Reason::TooLarge)
    );
    let packed = gzip(&[b' '; 100]);
    assert_eq!(
        gunzip(&packed, 100).map(|(out, tail)| (out.len(), tail.len())),
        Ok((100, 0))
    );
    assert_eq!(gunzip(&packed, 99), Err(Reason::TooLarge));

    // Nothing in this text decodes, so its whole-text reading charges nothing.
    let outer = siblings("<svg/>");
    let exact = outer.len() + payload_lengths(&outer).iter().sum::<usize>();
    assert_eq!(
        inspect_at(outer.as_bytes(), 0, &mut scan_only(exact)),
        Ok(())
    );
    assert_eq!(
        inspect_at(outer.as_bytes(), 0, &mut scan_only(exact - 1)),
        Err(Reason::TooLarge),
        "each nested payload is charged to the one budget, its siblings' charges kept"
    );
}

/// Two sibling URIs, so the text after the first is also read whole.
fn siblings(inner: &str) -> String {
    let image = format!(r#"<image href="data:image/svg+xml,{inner}"/>"#);
    format!("<svg>{image}{image}</svg>")
}

/// The length of each `data:` payload in `text` once percent-decoded. A
/// percent-encoded payload runs to the next `data:` URI or the end of the text (see
/// `data_uri_payload`), so each is charged that extent.
fn payload_lengths(text: &str) -> Vec<usize> {
    let starts: Vec<usize> = text.match_indices("data:").map(|(at, _)| at).collect();
    let ends = starts.iter().skip(1).copied().chain([text.len()]);
    starts
        .iter()
        .zip(ends)
        .map(|(&at, end)| {
            let comma = at + text[at..].find(',').expect("a header comma");
            percent_decode(&text.as_bytes()[comma + 1..end]).len()
        })
        .collect()
}

/// The whole-text reading is charged to its own pool, every layer it takes and every
/// payload it cuts from one, and the cuts of the text itself to the other: each
/// pool, one byte short, refuses. Mutations, each failing here: inspecting the
/// reading's layers on a fresh budget; charging them to the shared pool (running
/// the walk outside `Budget::splicing`); `saturating_sub` in `Budget::charge`.
#[test]
fn the_whole_text_reading_is_charged_to_its_own_pool() {
    let outer = siblings("<svg a='%41'/>");
    let scan = outer.len() + payload_lengths(&outer).iter().sum::<usize>();
    let first = outer.find(',').expect("first comma") + 1;
    let tail = &outer.as_bytes()[first..];
    let layer = String::from_utf8(next_layer(tail)).expect("ASCII");
    assert_ne!(layer.as_bytes(), tail, "the tail decodes");
    assert_eq!(next_layer(layer.as_bytes()), layer.as_bytes(), "once");
    let spliced = layer.len() + payload_lengths(&layer).iter().sum::<usize>();
    let run = |scan, spliced| inspect_at(outer.as_bytes(), 0, &mut Budget::new(scan, spliced));
    assert_eq!(run(scan, spliced), Ok(()));
    assert_eq!(
        run(scan - 1, spliced),
        Err(Reason::TooLarge),
        "the cuts' pool"
    );
    assert_eq!(
        run(scan, spliced - 1),
        Err(Reason::TooLarge),
        "the reading's pool"
    );
}

/// What each pool is charged for `text` with no limit on either: (cuts, whole-text).
fn charges(text: &[u8]) -> (usize, usize) {
    let mut budget = Budget::new(usize::MAX, usize::MAX);
    assert_eq!(
        inspect_at(text, 0, &mut budget),
        Ok(()),
        "unbounded, it passes"
    );
    (usize::MAX - budget.scan, usize::MAX - budget.spliced)
}

/// A page of icons at the image cap. Each icon is fully percent-encoded and holds a
/// reference, so it decodes twice, and the cuts and the whole-text reading each
/// re-read the page: together that is more than [`SCAN_BUDGET`], which refused the
/// page while both shared one pool. It passes now. The same page with each payload a
/// `%25` chain two levels deep (within [`MAX_DEPTH`]) still refuses, on the whole-text
/// pool: the cuts fit theirs.
///
/// Run at a sixty-fourth of the cap against pools a sixty-fourth of theirs. Both
/// pools are multiples of the cap and every charge is linear in the page, so the
/// proportions are the cap's; the full-size page takes half a minute unoptimised.
/// Mutation-checked: running the whole-text walk outside `Budget::splicing` (one
/// shared pool, as before) refuses the honest page.
#[test]
fn a_cap_sized_page_of_encoded_icons_passes_and_a_chain_of_it_refuses() {
    const SCALE: usize = 64;
    let size = crate::limits::MAX_LOCAL_IMAGE_BYTES as usize / SCALE;
    let screen = |text: &str| {
        let mut budget = Budget::new(SCAN_BUDGET / SCALE, SPLICE_BUDGET / SCALE);
        inspect_at(text.as_bytes(), 0, &mut budget).err()
    };
    let encode = |s: &str| -> String {
        s.bytes()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    char::from(c).to_string()
                } else {
                    format!("%{c:02X}")
                }
            })
            .collect()
    };
    let page = |uri: &str| {
        let image = format!(r#"<image width="16" height="16" href="data:image/svg+xml,{uri}"/>"#);
        let svg = |body: &str| format!(r#"<svg xmlns="http://www.w3.org/2000/svg">{body}</svg>"#);
        let room = size - svg("").len();
        svg(&image.repeat(room / image.len()))
    };
    let icon = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><title>A &amp; B</title><path fill="#3a7" d="M1 1h14v14H1z"/></svg>"##;
    let honest = page(&encode(icon));
    assert!(
        honest.len() > size - 1024 && honest.len() <= size,
        "at the size"
    );
    let (cuts, whole) = charges(honest.as_bytes());
    assert!(
        cuts + whole > SCAN_BUDGET / SCALE,
        "precondition: more than one pool holds"
    );
    assert_eq!(screen(&honest), None);

    let chain = page(&format!("{}%252541", "x".repeat(40)));
    let mut cuts_only = Budget::new(SCAN_BUDGET / SCALE, usize::MAX);
    assert_eq!(
        inspect_at(chain.as_bytes(), 0, &mut cuts_only),
        Ok(()),
        "the cuts fit"
    );
    assert_eq!(screen(&chain), Some(Reason::TooLarge));
}

/// The screen's oracle over its own encodings, composed: a hostile document refuses
/// under every carrier (gzip, a base64, percent-encoded or CSS-escaped `data:` URI)
/// wrapped around every encoding (those carriers again, UTF-16 in both byte orders
/// with and without a BOM, a character reference), while the benign document under
/// the same wrapping still passes, so each refusal is the content's and not the
/// wrapper's.
#[test]
fn every_wrapping_of_a_hostile_document_refuses_and_of_a_benign_one_passes() {
    type Wrap = fn(&[u8]) -> Vec<u8>;
    fn data_base64(b: &[u8]) -> Vec<u8> {
        format!(
            r#"<svg><image href="data:image/svg+xml;base64,{}"/></svg>"#,
            base64(b)
        )
        .into_bytes()
    }
    fn data_percent(b: &[u8]) -> Vec<u8> {
        let body: String = b.iter().map(|byte| format!("%{byte:02X}")).collect();
        format!(r#"<svg><image href="data:image/svg+xml,{body}"/></svg>"#).into_bytes()
    }
    fn data_css(b: &[u8]) -> Vec<u8> {
        format!(
            r#"<svg><style>rect {{ fill: url(data\3a image/svg+xml;base64,{}) }}</style></svg>"#,
            base64(b)
        )
        .into_bytes()
    }
    fn text(b: &[u8]) -> String {
        b.iter().map(|&byte| char::from(byte)).collect()
    }
    fn utf16_le(b: &[u8]) -> Vec<u8> {
        utf16(&text(b), false, false)
    }
    fn utf16_be_bom(b: &[u8]) -> Vec<u8> {
        utf16(&text(b), true, true)
    }
    fn char_ref(b: &[u8]) -> Vec<u8> {
        text(b).replace("XInclude", "&#88;Include").into_bytes()
    }
    let carriers: &[(&str, Wrap)] = &[
        ("gzip", gzip),
        ("base64 data:", data_base64),
        ("percent data:", data_percent),
        ("CSS-escaped data:", data_css),
    ];
    let encodings: &[(&str, Wrap)] = &[
        ("plain", <[u8]>::to_vec),
        ("gzip", gzip),
        ("base64 data:", data_base64),
        ("percent data:", data_percent),
        ("CSS-escaped data:", data_css),
        ("UTF-16LE", utf16_le),
        ("UTF-16BE with BOM", utf16_be_bom),
        ("character reference", char_ref),
    ];
    for (outer, carry) in carriers {
        for (inner, encode) in encodings {
            let hostile = carry(&encode(ATTACK.as_bytes()));
            assert_eq!(
                refused(&hostile),
                Some(Reason::XInclude),
                "{inner} inside {outer}"
            );
            let benign = carry(&encode(BENIGN.as_bytes()));
            assert_eq!(refused(&benign), None, "benign {inner} inside {outer}");
        }
    }
}

#[test]
fn benign_content_passes() {
    assert_eq!(refused(BENIGN), None);
    assert_eq!(refused(format!("\u{FEFF}{BENIGN}")), None);
    assert_eq!(refused(r#"<?xml-stylesheet href="s.css"?><svg/>"#), None);
    assert_eq!(refused(r#"<svg><text>metadata: none</text></svg>"#), None);
    // A still PNG's real header and IDAT, which carry zero bytes.
    let png = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/wide.png"
    ));
    assert_eq!(refused(png), None);
    let svg = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/diagram.svg"
    ));
    assert_eq!(refused(svg), None);
}

/// The manual-test fixtures (MANUAL-TEST 2.23c) are what this screen says they
/// are, so a placeholder seen there is the screen's doing.
#[test]
fn the_manual_test_fixtures_are_refused_except_the_plain_one() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xinclude");
    for name in [
        "prefixed.svg",
        "hidden.svg",
        "utf16.svg",
        "compressed.svgz",
        "nested.svg",
    ] {
        let bytes = std::fs::read(dir.join(name)).expect("fixture");
        assert_eq!(refused(bytes), Some(Reason::XInclude), "{name}");
    }
    let plain = std::fs::read(dir.join("plain.svg")).expect("fixture");
    assert_eq!(refused(plain), None);
}

#[test]
fn the_helpers_decode_what_they_claim() {
    assert_eq!(base64_decode(b"aGVs bG8=").as_deref(), Some(&b"hello"[..]));
    assert_eq!(base64_decode(b"aGVsbG8").as_deref(), Some(&b"hello"[..]));
    assert_eq!(base64_decode(b"a"), None);
    assert_eq!(percent_decode(b"%3Cx%zz%4"), b"<x%zz%4");
    assert_eq!(find_ci(b"abXINCLUDE", b"xinclude", 0), Some(2));
    assert_eq!(find_ci(b"ab", b"xinclude", 0), None);
}

/// Overlapping declarations made two checks quadratic outside any budget: `<!doctype `
/// repeated before one `>`, and `<?xml` repeated with no `?>` (each rescanned to the
/// end). At the image cap that hung the main thread for good. Each shape, at the cap,
/// must finish in seconds; the old code needed on the order of 10^13 steps.
#[test]
fn cap_sized_runs_of_declarations_are_screened_in_bounded_time() {
    let cap = crate::limits::MAX_LOCAL_IMAGE_BYTES as usize;
    let shapes: [(&str, String); 4] = [
        (
            "doctypes before one `>`",
            "<!doctype ".repeat(cap / 10) + ">",
        ),
        ("xml declarations with no `?>`", "<?xml ".repeat(cap / 6)),
        (
            "unclosed encoding values",
            "<?xml encoding=\"".repeat(cap / 16),
        ),
        (
            "declarations sharing one `?>`",
            "<?xml encoding=\"utf-8\" ".repeat(cap / 23) + "?>",
        ),
    ];
    for (what, text) in shapes {
        let started = std::time::Instant::now();
        let _ = refused(&text);
        let took = started.elapsed();
        assert!(
            took < std::time::Duration::from_secs(20),
            "{what}: {took:?}"
        );
    }
}

/// The decoder ends the header at the first LITERAL comma, and the screen used to cut
/// at the first comma of its normalised text, where an escape (`\2c`, or `&#92;2c`,
/// which the XML parser turns into `\2c`) can produce one earlier. The decoder then
/// reads `…;base64` and a base64 body; the screen read a short header and percent text.
#[test]
fn a_comma_produced_by_an_escape_does_not_move_the_header_boundary() {
    let hostile = base64(ATTACK.as_bytes());
    let benign = base64(BENIGN.as_bytes());
    for header in [
        "image/svg+xml;a=\\2c;base64",
        "image/svg+xml;a=\\2C ;base64",
        "image/svg+xml;a=&#92;2c;base64",
    ] {
        let wrap = |b64: &str| format!(r#"<svg><image href="data:{header},{b64}"/></svg>"#);
        assert_eq!(
            refused(wrap(&hostile)),
            Some(Reason::XInclude),
            "hostile under {header:?}"
        );
        assert_eq!(refused(wrap(&benign)), None, "benign under {header:?}");
    }
}

/// Every byte percent-encoded, so the payload holds no literal `data:` or comma.
fn percent_all(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("%{b:02X}")).collect()
}

/// The same moved boundary with no `;base64`: the decoder's percent body starts at the
/// literal comma, so it begins with gzip's or UCS-4's first bytes, while the screen's
/// first-cut reading begins one comma earlier and recognised neither.
#[test]
fn a_comma_produced_by_an_escape_does_not_hide_a_payload_known_by_its_first_bytes() {
    let mut ucs4 = vec![0, 0, 0, b'<'];
    ucs4.extend_from_slice(&[0, 0, 0, b's']);
    for header in ["image/svg+xml;a=\\2c", "image/svg+xml;a=&#92;2c"] {
        let wrap = |body: &str| format!(r#"<svg><image href="data:{header},{body}"/></svg>"#);
        assert_eq!(
            refused(wrap(&percent_all(&gzip(ATTACK.as_bytes())))),
            Some(Reason::XInclude),
            "gzip under {header:?}"
        );
        assert_eq!(
            refused(wrap(&percent_all(&gzip(BENIGN.as_bytes())))),
            None,
            "benign gzip under {header:?}"
        );
        assert_eq!(
            refused(wrap(&percent_all(&ucs4))),
            Some(Reason::Encoding),
            "UCS-4 under {header:?}"
        );
    }
}

/// Each later comma that leads a signature gets a whole reading of its own, so a text
/// of many of them must be refused by the budget, not screened in quadratic time.
#[test]
fn many_commas_leading_a_signature_are_bounded_by_the_budget() {
    let cap = crate::limits::MAX_LOCAL_IMAGE_BYTES as usize;
    let text = format!(
        r#"<svg><image href="data:a\2c,{}"/></svg>"#,
        ",%1F%8B".repeat(cap / 7)
    );
    let started = std::time::Instant::now();
    assert!(refused(&text).is_some());
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

/// A context that expands no references (an `<?xml-stylesheet?>` pseudo-attribute)
/// leaves `&amp;` to the decoder, where it ends in the `;` its base64 rule asks for,
/// while the screen's normalised `&base64` has none.
#[test]
fn a_later_comma_that_base64_precedes_is_read_as_base64_whatever_introduces_it() {
    let wrap = |doc: &str| {
        format!(
            r#"<?xml-stylesheet href="data:text/css;a=\2c&amp;base64,{}"?><svg/>"#,
            base64(doc.as_bytes())
        )
    };
    assert_eq!(refused(wrap(ATTACK)), Some(Reason::XInclude));
    assert_eq!(refused(wrap(BENIGN)), None);
}

/// CSS eats any one whitespace after a hex escape, form feed included, and an escaped
/// newline or form feed stands for nothing; the screen kept the form feed, which split
/// `base64` in a stylesheet read with no XML parse (a percent-encoded `data:text/css`).
#[test]
fn a_form_feed_after_a_css_escape_does_not_split_base64() {
    for spelling in ["\\62\x0Case64", "bas\\\x0Ce64", "\\62\r\nase64"] {
        let wrap = |doc: &str| {
            let css = format!(
                "a{{b:url(data:image/svg+xml;{spelling},{})}}",
                base64(doc.as_bytes())
            );
            format!(
                r#"<svg><image href="data:text/css,{}"/></svg>"#,
                percent_all(css.as_bytes())
            )
        };
        assert_eq!(
            refused(wrap(ATTACK)),
            Some(Reason::XInclude),
            "hostile under {spelling:?}"
        );
        assert_eq!(refused(wrap(BENIGN)), None, "benign under {spelling:?}");
    }
}

/// The XML parser hands a `<style>` element's text to the stylesheet with comments and
/// processing instructions dropped and CDATA unwrapped, so markup between two halves
/// of a payload joins them.
#[test]
fn markup_splitting_a_payload_in_element_text_hides_nothing() {
    let splits = ["<!-- x -->", "<?p q?>", "]]><![CDATA["];
    for split in splits {
        let wrap = |doc: &str| {
            let b64 = base64(doc.as_bytes());
            // Past the benign twin's declarations, so its first half reads honestly.
            let (head, tail) = b64.split_at(192);
            format!(
                "<svg><style><![CDATA[a{{b:url(data:image/svg+xml;base64,{head}{split}{tail})}}]]></style></svg>"
            )
        };
        assert_eq!(
            refused(wrap(ATTACK)),
            Some(Reason::XInclude),
            "base64 split by {split:?}"
        );
        assert_eq!(refused(wrap(BENIGN)), None, "benign split by {split:?}");
    }
    let word = percent_all(b"XInclude");
    let inner = percent_all(
        br#"<svg xmlns:xi="http://www.w3.org/2001/XInclude"><xi:include href="x"/></svg>"#,
    )
    .replace(&word, &format!("{}<!---->{}", &word[..9], &word[9..]));
    let page = format!("<svg><style>a{{b:url(data:image/svg+xml,{inner})}}</style></svg>");
    assert_eq!(refused(page), Some(Reason::XInclude), "percent split");
}

/// An honest page's comments and declarations sit between tags, so it is read once.
#[test]
fn markup_between_tags_does_not_join_anything() {
    assert!(without_joining_markup(BENIGN.as_bytes()).is_none());
    assert!(
        without_joining_markup(b"<svg><style><![CDATA[ .a{} ]]></style> <!-- c --> </svg>")
            .is_none()
    );
    assert!(without_joining_markup(b"<s>ab<!-- c -->cd</s>").is_some());
}

/// An honest page near the cap holding one base64 raster is screened in full and passes.
/// Without [`inert_prefix`] the percent reading kept under a base64 verdict re-reads the
/// whole raster, and the same page exceeds the budget: that is the precondition, so the
/// test cannot pass by the page being too small to matter.
#[test]
fn an_honest_raster_near_the_cap_is_not_refused_for_its_cost() {
    let cap = crate::limits::MAX_LOCAL_IMAGE_BYTES as usize;
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let raw: Vec<u8> = (0..(cap - 256) / 4 * 3)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect();
    let page = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="data:image/png;base64,{}"/></svg>"#,
        base64(&raw)
    );
    assert!(page.len() <= cap);
    NO_INERT_PREFIX.with(|off| off.set(true));
    let without = refused(&page);
    NO_INERT_PREFIX.with(|off| off.set(false));
    assert_eq!(without, Some(Reason::TooLarge), "precondition");
    assert_eq!(refused(&page), None);
}
