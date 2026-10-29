//! The byte-mutation stage: entity-free twins of the screen's past leaks, mutated one
//! byte or one layer at a time.
//!
//! Each twin keeps the encoding layers of a regression in `xinclude/tests.rs` around the
//! generator's core (an include of a relative `href` with a fallback, and no DOCTYPE).
//! The fixtures themselves are never seeds: they wrap the published trigger, an encoded
//! `<!DOCTYPE … <!ENTITY`, and a mutant the screen passes is handed to the decoder.

use super::grammar::{base64, gzip, outer, percent, utf16, Core, Op, Prefix, Rng};

/// A seed: what it is a twin of, and its bytes.
pub(super) struct Twin {
    pub(super) name: String,
    pub(super) bytes: Vec<u8>,
}

fn twin(name: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Twin {
    Twin {
        name: name.into(),
        bytes: bytes.into(),
    }
}

/// Every byte percent-encoded, so the payload holds no literal `data:` or comma.
fn percent_all(bytes: &[u8]) -> String {
    percent(bytes, true, false)
}

/// `text` as a quoted attribute value would carry it, its `#` percent-encoded so a
/// percent body is not cut at a fragment.
fn attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('#', "%23")
}

fn image(uri: &str) -> String {
    outer(&format!(r#"<image width="8" height="8" href="{uri}"/>"#))
}

fn styled(css: &str) -> String {
    outer(&format!(
        r#"<style>{css}</style><rect width="8" height="8"/>"#
    ))
}

fn stylesheet(sheet_uri: &str) -> String {
    format!(
        r#"<!-- <svg> --><?xml-stylesheet type="text/css" href="{sheet_uri}"?>{}"#,
        outer(r#"<rect width="8" height="8"/>"#)
    )
}

pub(super) fn twins() -> Vec<Twin> {
    let core = Core::PLAIN.text();
    let b64 = base64(core.as_bytes(), true);
    let mut out = vec![
        twin("the core alone", core.clone()),
        twin(
            "base64 under an image href",
            image(&format!("data:image/svg+xml;base64,{b64}")),
        ),
        twin(
            "percent under an image href",
            image(&format!("data:image/svg+xml,{}", attr(&core))),
        ),
        twin(
            "base64 under a use",
            outer(&format!(
                r#"<use href="data:image/svg+xml;base64,{b64}#m"/>"#
            )),
        ),
        twin(
            "a namespace spelled with references",
            core.replace("XInclude", "&#88;I&#x6e;clude"),
        ),
        twin(
            "a CSS-escaped scheme in a style element",
            styled(&format!(
                r"rect{{fill:url(data\3a image/svg+xml;base64,{b64}#p)}}"
            )),
        ),
    ];
    for header in ["; base64", ";BASE64", ";charset=utf-8;base64", ";base64 "] {
        out.push(twin(
            format!("base64 header {header:?}"),
            image(&format!("data:image/svg+xml{header},{b64}")),
        ));
    }
    // Headers only a reader of the decoded text takes for base64, over a percent body.
    for header in [
        ";base64%20",
        ";%20base64",
        "%3Bbase64",
        ";x=%3Bbase64",
        ";base%364",
        ";\\62 ase64",
    ] {
        out.push(twin(
            format!("header {header:?} over a percent body"),
            image(&format!("data:image/svg+xml{header},{}", attr(&core))),
        ));
    }
    // A comma an escape makes, which the decoder's header parser does not see.
    for param in [";a=\\2c", ";a=\\2C ", ";a=&#92;2c"] {
        out.push(twin(
            format!("escaped comma {param:?} before base64"),
            image(&format!("data:image/svg+xml{param};base64,{b64}")),
        ));
        out.push(twin(
            format!("escaped comma {param:?} before a gzip body"),
            image(&format!(
                "data:image/svg+xml{param},{}",
                percent_all(&gzip(core.as_bytes()))
            )),
        ));
    }
    let paint = format!("rect{{fill:url(data:image/svg+xml;base64,{b64}#p)}}");
    out.push(twin(
        "a later comma that &amp;base64 precedes, in a stylesheet instruction",
        stylesheet(&format!(
            "data:text/css;a=\\2c&amp;base64,{}",
            base64(paint.as_bytes(), true)
        )),
    ));
    for spelling in ["\\62\x0Case64", "bas\\\x0Ce64", "\\62\r\nase64"] {
        let css = format!("rect{{fill:url(data:image/svg+xml;{spelling},{b64}#p)}}");
        out.push(twin(
            format!("CSS whitespace {spelling:?} in a percent stylesheet"),
            stylesheet(&format!("data:text/css,{}", percent_all(css.as_bytes()))),
        ));
    }
    let (head, tail) = b64.split_at(64);
    for split in ["<!-- x -->", "<?p q?>", "<x/>", "<x>junk <y>more</y></x>"] {
        out.push(twin(
            format!("base64 split by {split:?} in a style element"),
            styled(&format!(
                "rect{{fill:url(data:image/svg+xml;base64,{head}{split}{tail}#p)}}"
            )),
        ));
    }
    out.push(twin(
        "base64 split across CDATA sections",
        styled(&format!(
            "<![CDATA[rect{{fill:url(data:image/svg+xml;base64,{head}]]><![CDATA[{tail}#p)}}]]>"
        )),
    ));
    for (opener, later) in [
        ("<!--", ""),
        ("<!--", "<!-- z -->"),
        ("<?", ""),
        ("<?", "<?z?>"),
    ] {
        let page = styled(&format!(
            "<![CDATA[/* {opener} */ rect{{fill:url(data:image/svg+xml;base64,{head}]]><![CDATA[{tail}#p)}}]]>"
        ));
        out.push(twin(
            format!("opener {opener:?} in CDATA, {later:?} after"),
            page.replace("</svg>", &format!("{later}</svg>")),
        ));
    }
    for opener in ["&lt;!--", "\\3c !--", "&lt;?", "\\3c ?"] {
        out.push(twin(
            format!("opener {opener:?} ahead of a split"),
            styled(&format!(
                "/* {opener} */ rect{{fill:url(data:image/svg+xml;base64,{head}<!-- x -->{tail}#p)}}"
            )),
        ));
    }
    let word = percent_all(b"XInclude");
    let split_word = percent_all(core.as_bytes())
        .replace(&word, &format!("{}<!---->{}", &word[..9], &word[9..]));
    out.push(twin(
        "a percent payload split inside the namespace",
        styled(&format!(
            "rect{{fill:url(data:image/svg+xml,{split_word}%23p)}}"
        )),
    ));
    let middle = image(&format!("data:image/svg+xml;base64,{b64}")).replace('"', "'");
    for (name, header) in [
        ("a literal inner data: under percent", ""),
        ("an inner data: under %3Bbase64", "%3Bbase64"),
    ] {
        let enc = percent(middle.as_bytes(), false, false).replace('\'', "%27");
        out.push(twin(
            name,
            image(&format!("data:image/svg+xml{header},{enc}")),
        ));
    }
    out.push(twin(
        "a reference layer between percent layers",
        image(&format!(
            "data:image/svg+xml,{}",
            percent_all(core.as_bytes()).replace('%', "&#37;")
        )),
    ));
    for (big_endian, bom) in [(false, false), (false, true), (true, false), (true, true)] {
        out.push(twin(
            format!("UTF-16 big_endian={big_endian} bom={bom}"),
            utf16(core.as_bytes(), big_endian, bom, true),
        ));
    }
    out.push(twin("gzip", gzip(core.as_bytes())));
    out.push(twin(
        "base64 of gzip under an image",
        image(&format!(
            "data:image/svg+xml;base64,{}",
            base64(&gzip(core.as_bytes()), true)
        )),
    ));
    out.push(twin(
        "base64 of UTF-16 under an image",
        image(&format!(
            "data:image/svg+xml;base64,{}",
            base64(&utf16(core.as_bytes(), false, true, false), true)
        )),
    ));
    out.push(twin(
        "the namespace as the default",
        Core {
            prefix: Prefix::Default,
            ..Core::PLAIN
        }
        .text(),
    ));
    out
}

/// Bytes a mutation favours: the delimiters and escape openers every past leak turned on.
const INTERESTING: &[u8] = b";,:%&#\\ \t\n\r\x0c<>\"'=/()[]!?-0123456789aAbBcCxX\x00";

/// One change to a seed.
#[derive(Clone, Debug)]
pub(super) enum Mutation {
    Replace { at: usize, byte: u8 },
    Insert { at: usize, byte: u8 },
    Delete { at: usize },
    Wrap(Op),
}

/// `seed` changed one byte or one layer.
pub(super) fn mutate(seed: &[u8], rng: &mut Rng) -> (Vec<u8>, Mutation) {
    let byte = if rng.one_in(4) {
        rng.next() as u8
    } else {
        *rng.pick(INTERESTING)
    };
    let mut at = rng.below(seed.len().max(1));
    // Half the time, move to the next delimiter: that is where readings diverge.
    if rng.one_in(2) {
        at += seed[at.min(seed.len())..]
            .iter()
            .position(|b| b";,:%&\\<>".contains(b))
            .unwrap_or(0);
    }
    let at = at.min(seed.len().saturating_sub(1));
    let mutation = match rng.below(7) {
        0..=2 => Mutation::Replace { at, byte },
        3 | 4 => Mutation::Insert { at, byte },
        5 => Mutation::Delete { at },
        _ => Mutation::Wrap(Op::random(rng)),
    };
    let mut out = seed.to_vec();
    match &mutation {
        Mutation::Replace { at, byte } => out[*at] = *byte,
        Mutation::Insert { at, byte } => out.insert(*at, *byte),
        Mutation::Delete { at } => {
            out.remove(*at);
        }
        Mutation::Wrap(op) => out = op.apply(&out),
    }
    (out, mutation)
}
