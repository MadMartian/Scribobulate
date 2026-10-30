//! The generator: SVGs holding an XInclude under layers of encoding and nesting.
//!
//! A [`Case`] is a [`Core`] document and the [`Op`]s wrapped around it, innermost first,
//! so it can be rebuilt and shrunk op by op. Every parameter that varies is drawn here
//! and stored in the case; [`Case::build`] is a pure function of it.
//!
//! **What the core is, and why every carrier sees the same witness.** The innermost
//! `include` names a relative `href`, which librsvg cannot resolve with no base URL, so
//! its `fallback` runs. The fallback holds a group `m` and a gradient `p`; a stylesheet
//! paints both in the sentinel colour, but only through child selectors rooted at the
//! `svg` element (`svg>g>rect`, `svg>linearGradient>stop`). Processing the include puts
//! the fallback's children in its place, where those selectors match. Not processing it
//! leaves them under an unknown `include` element, which draws nothing and where the
//! selectors do not match — so a carrier that renders the whole document (`<image>`)
//! and one that only borrows an element by id (`<use>`, a CSS `url(…#p)` paint) both
//! show the sentinel exactly when the include was acted on. MEASURED on librsvg
//! 2.52.5: an id-only witness without the selectors drew the gradient and the group
//! with the namespace wrong, because librsvg builds the unknown element's subtree and
//! finds ids inside it.
//!
//! Nothing here declares an entity or a DOCTYPE; `super::safety` checks every input
//! anyway, on what it decodes to.

use std::fmt::Write as _;

use super::super::xinclude::MAX_DEPTH;

pub(super) const SVG_NS: &str = "http://www.w3.org/2000/svg";
pub(super) const XINCLUDE_NS: &str = "http://www.w3.org/2001/XInclude";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// The colour the core's fallback paints, and the oracle looks for.
pub(super) const SENTINEL: [u8; 3] = [0x1D, 0xE2, 0x5B];

/// Width and height of every generated document, in user units.
const SIDE: u32 = 8;

/// A small deterministic generator (SplitMix64): the pipeline run must cover the same
/// inputs on every platform, and a hand-rolled one is all that takes.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`; `n` must be non-zero.
    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub(super) fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    pub(super) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn sentinel_hex() -> String {
    let [r, g, b] = SENTINEL;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// How the core names the XInclude namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prefix {
    Xi,
    Other,
    Default,
}

/// The innermost document: an include of a relative `href` whose fallback paints the
/// sentinel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Core {
    pub(super) prefix: Prefix,
    /// Seed choosing which characters of the namespace URI are written as character
    /// references; zero writes it plainly.
    pub(super) ns_references: u64,
    /// Whether an `<?xml…?>` declaration leads the document.
    pub(super) declaration: bool,
}

impl Core {
    pub(super) const PLAIN: Self = Self {
        prefix: Prefix::Xi,
        ns_references: 0,
        declaration: false,
    };

    fn random(rng: &mut Rng) -> Self {
        Self {
            prefix: *rng.pick(&[Prefix::Xi, Prefix::Xi, Prefix::Other, Prefix::Default]),
            ns_references: if rng.one_in(4) { rng.next() | 1 } else { 0 },
            declaration: rng.one_in(3),
        }
    }

    pub(super) fn text(&self) -> String {
        self.with_namespace(XINCLUDE_NS)
    }

    /// The same document with the include's namespace URI replaced: the oracle's
    /// negative control.
    pub(super) fn with_namespace(&self, namespace: &str) -> String {
        let ns = if self.ns_references == 0 {
            namespace.to_owned()
        } else {
            as_references(namespace, self.ns_references)
        };
        let declaration = if self.declaration {
            r#"<?xml version="1.0" encoding="UTF-8"?>"#
        } else {
            ""
        };
        let fill = sentinel_hex();
        let style = format!(
            "<style>svg>g>rect{{fill:{fill}}}svg>linearGradient>stop{{stop-color:{fill}}}</style>"
        );
        let body = format!(
            r#"<linearGradient xmlns="{SVG_NS}" id="p"><stop offset="0"/></linearGradient><g xmlns="{SVG_NS}" id="m"><rect width="{SIDE}" height="{SIDE}"/></g>"#
        );
        let (bind, include, fallback) = match self.prefix {
            Prefix::Xi => (format!(r#" xmlns:xi="{ns}""#), "xi:include", "xi:fallback"),
            Prefix::Other => (format!(r#" xmlns:q7="{ns}""#), "q7:include", "q7:fallback"),
            Prefix::Default => (String::new(), "include", "fallback"),
        };
        let default_ns = if self.prefix == Prefix::Default {
            format!(r#" xmlns="{ns}""#)
        } else {
            String::new()
        };
        format!(
            r#"{declaration}<svg xmlns="{SVG_NS}"{bind} width="{SIDE}" height="{SIDE}">{style}<{include}{default_ns} href="missing.svg"><{fallback}>{body}</{fallback}></{include}></svg>"#
        )
    }
}

/// Some characters of `text` as XML character references, chosen by `seed`, in every
/// spelling the parser accepts: decimal, hex in either case, leading zeros.
fn as_references(text: &str, seed: u64) -> String {
    let mut rng = Rng::new(seed);
    let mut out = String::new();
    for c in text.chars() {
        if rng.one_in(3) {
            out.push_str(&reference(c, &mut rng));
        } else {
            out.push(c);
        }
    }
    out
}

fn reference(c: char, rng: &mut Rng) -> String {
    let n = u32::from(c);
    match rng.below(4) {
        0 => format!("&#{n};"),
        1 => format!("&#000{n};"),
        2 => format!("&#x{n:x};"),
        _ => format!("&#x{n:X};"),
    }
}

/// A run of URI text: literal characters, which are escaped for their context, or
/// markup already written for it (a reference, an escape), which is emitted as is.
#[derive(Clone, Copy)]
enum Seg {
    Lit(&'static str),
    Raw(&'static str),
}

/// Where URI text is written, which decides what escapes it may carry and what it must.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    /// An XML attribute value: references expanded, then the URL parser.
    Attr,
    /// A `style` attribute: references expanded, then CSS.
    StyleAttr,
    /// A `<style>` element's text: references expanded, then CSS.
    StyleText,
    /// A CDATA section in a `<style>` element: CSS only.
    Cdata,
    /// A stylesheet read with no XML parse (a `data:text/css` body): CSS only.
    Css,
    /// An `<?xml-stylesheet?>` pseudo-attribute: nothing expanded.
    Pi,
}

impl Ctx {
    fn references(self) -> bool {
        matches!(self, Self::Attr | Self::StyleAttr | Self::StyleText)
    }

    fn css(self) -> bool {
        matches!(
            self,
            Self::StyleAttr | Self::StyleText | Self::Cdata | Self::Css
        )
    }

    /// What `c` must become to keep the surrounding XML well formed.
    fn mandatory(self, c: char) -> Option<&'static str> {
        match (self, c) {
            (Self::Attr | Self::StyleAttr | Self::StyleText, '&') => Some("&amp;"),
            (Self::Attr | Self::StyleAttr | Self::StyleText, '<') => Some("&lt;"),
            (Self::Attr | Self::StyleAttr, '"') => Some("&quot;"),
            _ => None,
        }
    }
}

/// One escaped character in this many, in a write that has a decoy to place, is that
/// decoy. A write places at most one: a decoy is a deliberate break in what the
/// decoder can read, and two make an input that nothing reads at all.
const DECOY_ODDS: usize = 8;

/// One escape of `c`. Honest, it is one `ctx` undoes back to `c`. A decoy is from a
/// family `ctx` does not undo, which the decoder reads literally while a careless reader
/// undoes it: a CSS escape in a plain attribute, a reference in CDATA or in an
/// instruction, whitespace inside a CSS `url(`.
fn escape(c: char, ctx: Ctx, decoy: bool, rng: &mut Rng) -> String {
    let plain = ctx
        .mandatory(c)
        .map_or_else(|| c.to_string(), str::to_owned);
    let attr = matches!(ctx, Ctx::Attr | Ctx::StyleAttr);
    // CSS eats one whitespace after a hex escape; raw whitespace in an attribute is
    // normalised to a space before CSS sees it.
    // A form feed is not a character XML allows, so only a stylesheet read with no XML
    // parse can carry one.
    let end = if attr {
        " "
    } else if ctx == Ctx::Css {
        *rng.pick(&[" ", "\t", "\n", "\x0c", "\r\n"])
    } else {
        *rng.pick(&[" ", "\t", "\n", "\r\n"])
    };
    let css_hex = format!("\\{:x}{end}", u32::from(c));
    let dropped = if ctx.references() {
        *rng.pick(&["&#9;", "&#10;", "&#13;"])
    } else {
        *rng.pick(&["\t", "\n"])
    };
    let mut options = Vec::new();
    if decoy {
        if !ctx.references() {
            options.push(reference(c, rng));
        }
        if !ctx.css() {
            options.push(css_hex);
        } else {
            options.push(format!("{dropped}{plain}"));
            let escaped = if ctx == Ctx::Css { "\\\x0c" } else { "\\\n" };
            options.push(format!("{escaped}{plain}"));
        }
    } else {
        if ctx.references() {
            options.push(reference(c, rng));
        }
        // The URL parser drops a tab or newline anywhere, where no CSS token ends at it.
        if ctx == Ctx::Attr {
            options.push(format!("{dropped}{plain}"));
        }
        if ctx.css() {
            options.push(css_hex);
            if !c.is_ascii_hexdigit() && c != '\n' {
                options.push(format!("\\{plain}"));
            }
        }
    }
    if options.is_empty() {
        plain
    } else {
        options.swap_remove(rng.below(options.len()))
    }
}

/// `segs` written for `ctx`, with roughly one literal character in `density` escaped
/// (none when `seed` is zero).
fn write_segs(segs: &[Seg], ctx: Ctx, seed: u64, density: usize) -> String {
    let mut rng = Rng::new(seed);
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::Raw(s) => out.push_str(s),
            Seg::Lit(s) => write_literal(&mut out, s, ctx, seed != 0, density, &mut rng),
        }
    }
    out
}

fn write_literal(
    out: &mut String,
    s: &str,
    ctx: Ctx,
    escapes: bool,
    density: usize,
    rng: &mut Rng,
) {
    let mut decoy = escapes && rng.one_in(2);
    for c in s.chars() {
        if escapes && rng.one_in(density.max(1)) {
            let this_one = decoy && rng.one_in(DECOY_ODDS);
            decoy &= !this_one;
            out.push_str(&escape(c, ctx, this_one, rng));
        } else if let Some(m) = ctx.mandatory(c) {
            out.push_str(m);
        } else {
            out.push(c);
        }
    }
}

/// Bytes a percent body may carry unencoded: none that ends a URI, quotes an
/// attribute, starts a fragment, or closes a CSS `url(`.
fn percent_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-._~!$*+,;=:@/?".contains(&b)
}

pub(super) fn percent(bytes: &[u8], all: bool, lower: bool) -> String {
    let mut out = String::new();
    for &b in bytes {
        if !all && percent_safe(b) {
            out.push(char::from(b));
        } else if lower {
            let _ = write!(out, "%{b:02x}");
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

pub(super) fn base64(bytes: &[u8], padded: bool) -> String {
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
            } else if padded {
                out.push('=');
            }
        }
    }
    out
}

pub(super) fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).expect("gzip into memory");
    enc.finish().expect("gzip into memory")
}

/// How the scheme is spelled. The escaped ones mean `data:` only where their escape is
/// undone; elsewhere they are decoys.
const SCHEMES: &[&[Seg]] = &[
    &[Seg::Lit("data:")],
    &[Seg::Lit("DATA:")],
    &[Seg::Lit("data"), Seg::Raw("\\3a ")],
    &[Seg::Lit("data"), Seg::Raw("\\:")],
    &[Seg::Lit("data"), Seg::Raw("&#58;")],
];

const MEDIA: &[&str] = &["image/svg+xml", "text/xml", "application/xml", ""];

/// Parameters ahead of the encoding. The escaped commas are a comma only to a reader
/// that undoes the escape, which the decoder's header parser does not.
const PARAMS: &[&[Seg]] = &[
    &[],
    &[Seg::Lit(";charset=utf-8")],
    &[Seg::Lit(";a=b")],
    &[Seg::Lit(";a="), Seg::Raw("\\2c")],
    &[Seg::Lit(";a="), Seg::Raw("\\2C ")],
    &[Seg::Lit(";a="), Seg::Raw("&#92;2c")],
];

/// `;base64` spelled every way the decoder accepts, and ways it rejects that a looser
/// reader might not.
const BASE64_HEADERS: &[&str] = &[
    ";base64",
    "; base64",
    ";BASE64",
    ";Base64",
    ";base64 ",
    ";\tbase64",
    ";\nbase64",
    "%3Bbase64",
    ";base64%20",
    ";%20base64",
    ";base%364",
    ";\\62 ase64",
    ";\\c base64",
    ";\\62\x0case64",
    ";bas\\\x0ce64",
    ";base64;x=y",
    ";x-base64",
    "&amp;base64",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Body {
    Percent {
        all: bool,
        lower: bool,
        twice: bool,
    },
    Base64 {
        header: usize,
        wrapped: bool,
        padded: bool,
    },
}

impl Body {
    fn random(rng: &mut Rng) -> Self {
        if rng.one_in(2) {
            Self::Percent {
                all: rng.one_in(3),
                lower: rng.one_in(2),
                twice: rng.one_in(8),
            }
        } else {
            Self::Base64 {
                header: if rng.one_in(2) {
                    0
                } else {
                    rng.below(BASE64_HEADERS.len())
                },
                wrapped: rng.one_in(4),
                padded: !rng.one_in(4),
            }
        }
    }

    fn header(&self) -> Option<&'static str> {
        match self {
            Self::Percent { .. } => None,
            Self::Base64 { header, .. } => Some(BASE64_HEADERS[*header]),
        }
    }

    fn encode(&self, doc: &[u8]) -> String {
        match *self {
            Self::Percent { all, lower, twice } => {
                let once = percent(doc, all, lower);
                if twice {
                    percent(once.as_bytes(), false, lower)
                } else {
                    once
                }
            }
            Self::Base64 {
                wrapped, padded, ..
            } => {
                let b64 = base64(doc, padded);
                if !wrapped {
                    return b64;
                }
                let mut out = String::new();
                for (i, c) in b64.chars().enumerate() {
                    if i > 0 && i % 60 == 0 {
                        out.push('\n');
                    }
                    out.push(c);
                }
                out
            }
        }
    }
}

/// The element or construct that carries a nested document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Carrier {
    ImageHref,
    ImageXlink,
    UseHref,
    StyleAttr,
    StyleElement,
    StyleCdata,
    XmlStylesheet,
    CssImport,
}

pub(super) const CARRIERS: &[Carrier] = &[
    Carrier::ImageHref,
    Carrier::ImageXlink,
    Carrier::UseHref,
    Carrier::StyleAttr,
    Carrier::StyleElement,
    Carrier::StyleCdata,
    Carrier::XmlStylesheet,
    Carrier::CssImport,
];

/// Markup splitting a payload inside a `<style>` element, a decoy opener ahead of it,
/// and whether a closer for that opener follows the element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Split {
    /// Where, in thousandths of the URI text.
    at: u16,
    splitter: usize,
    decoy: Option<usize>,
    closer: bool,
}

const ELEMENT_SPLITTERS: &[&str] = &["<!-- x -->", "<?p q?>", "<x/>", "<x>junk <y>more</y></x>"];
const CDATA_SPLITTER: &str = "]]><![CDATA[";
const ELEMENT_DECOYS: &[&str] = &[
    "/* &lt;!-- */",
    "/* \\3c !-- */",
    "/* &lt;? */",
    "/* \\3c ? */",
];
const CDATA_DECOYS: &[&str] = &["/* <!-- */", "/* <? */"];
const CLOSERS: &[&str] = &["<!-- z -->", "<?z?>"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Embed {
    pub(super) carrier: Carrier,
    scheme: usize,
    media: usize,
    param: usize,
    body: Body,
    /// Seed for the escapes written into the URI text; zero writes none.
    escapes: u64,
    density: usize,
    split: Option<Split>,
}

impl Embed {
    /// The plainest embedding in `carrier`: what a shrink reduces an embed to.
    pub(super) fn plain(carrier: Carrier) -> Self {
        Self {
            carrier,
            scheme: 0,
            media: 0,
            param: 0,
            body: Body::Base64 {
                header: 0,
                wrapped: false,
                padded: true,
            },
            escapes: 0,
            density: 1,
            split: None,
        }
    }

    pub(super) fn random(rng: &mut Rng) -> Self {
        let carrier = *rng.pick(CARRIERS);
        let splittable = matches!(carrier, Carrier::StyleElement | Carrier::StyleCdata);
        let split = (splittable && rng.one_in(2)).then(|| Split {
            at: rng.below(1000) as u16,
            splitter: rng.below(ELEMENT_SPLITTERS.len()),
            decoy: rng.one_in(2).then(|| rng.below(ELEMENT_DECOYS.len())),
            closer: rng.one_in(2),
        });
        Self {
            carrier,
            scheme: if rng.one_in(2) {
                0
            } else {
                rng.below(SCHEMES.len())
            },
            media: if rng.one_in(2) {
                0
            } else {
                rng.below(MEDIA.len())
            },
            param: if rng.one_in(2) {
                0
            } else {
                rng.below(PARAMS.len())
            },
            body: Body::random(rng),
            escapes: if rng.one_in(3) { rng.next() | 1 } else { 0 },
            density: 4 + rng.below(60),
            split,
        }
    }

    /// The simpler variants of this embed a shrink tries, simplest first.
    pub(super) fn simpler(&self) -> Vec<Self> {
        let mut out = vec![Self::plain(self.carrier)];
        let mut each = |change: &dyn Fn(&mut Self)| {
            let mut s = self.clone();
            change(&mut s);
            if s != *self {
                out.push(s);
            }
        };
        each(&|s| s.escapes = 0);
        each(&|s| s.split = None);
        each(&|s| s.scheme = 0);
        each(&|s| s.media = 0);
        each(&|s| s.param = 0);
        each(&|s| {
            if let Body::Base64 {
                wrapped, padded, ..
            } = &mut s.body
            {
                *wrapped = false;
                *padded = true;
            }
        });
        each(&|s| {
            if let Body::Percent { all, lower, twice } = &mut s.body {
                *all = false;
                *lower = false;
                *twice = false;
            }
        });
        out
    }

    /// The `data:` URI of `doc`, with `fragment`, written for `ctx`.
    fn uri(&self, doc: &[u8], media: &'static str, fragment: &'static str, ctx: Ctx) -> String {
        let body = self.body.encode(doc);
        let mut segs: Vec<Seg> = SCHEMES[self.scheme].to_vec();
        segs.push(Seg::Lit(media));
        segs.extend_from_slice(PARAMS[self.param]);
        let mut out = write_segs(&segs, ctx, self.escapes, self.density);
        if let Some(header) = self.body.header() {
            out.push_str(header);
        }
        // The body and fragment are escaped with a seed of their own, so a shrink that
        // drops a header escape does not reshuffle the body's.
        let tail = format!(",{body}{fragment}");
        out.push_str(&write_owned(
            &tail,
            ctx,
            self.escapes.rotate_left(17),
            self.density,
        ));
        out
    }

    fn uri_in(&self, doc: &[u8], fragment: &'static str, ctx: Ctx) -> String {
        self.uri(doc, MEDIA[self.media], fragment, ctx)
    }

    /// The outer document carrying `doc`.
    pub(super) fn wrap(&self, doc: &[u8]) -> Vec<u8> {
        let rect = format!(r#"<rect width="{SIDE}" height="{SIDE}"/>"#);
        let text = match self.carrier {
            Carrier::ImageHref | Carrier::ImageXlink => {
                let attr = if self.carrier == Carrier::ImageHref {
                    "href"
                } else {
                    "xlink:href"
                };
                let uri = self.uri_in(doc, "", Ctx::Attr);
                outer(&format!(
                    r#"<image width="{SIDE}" height="{SIDE}" {attr}="{uri}"/>"#
                ))
            }
            Carrier::UseHref => {
                let uri = self.uri_in(doc, "#m", Ctx::Attr);
                outer(&format!(r#"<use href="{uri}"/>"#))
            }
            Carrier::StyleAttr => {
                let uri = self.uri_in(doc, "#p", Ctx::StyleAttr);
                outer(&format!(
                    r#"<rect width="{SIDE}" height="{SIDE}" style="fill:url({uri})"/>"#
                ))
            }
            Carrier::StyleElement | Carrier::StyleCdata => self.style_element(doc, &rect),
            Carrier::XmlStylesheet => {
                let css = format!("rect{{fill:url({})}}", self.uri_in(doc, "#p", Ctx::Css));
                let sheet = format!("data:text/css,{}", percent(css.as_bytes(), false, false));
                // gdk-pixbuf picks a loader by content, and a long processing instruction
                // pushes the root element past what it sniffs; the comment keeps it SVG.
                format!(
                    r#"<!-- <svg> --><?xml-stylesheet type="text/css" href="{}"?>{}"#,
                    write_owned(&sheet, Ctx::Pi, 0, 1),
                    outer(&rect)
                )
            }
            Carrier::CssImport => {
                let css = format!("rect{{fill:url({})}}", self.uri_in(doc, "#p", Ctx::Css));
                let sheet = format!("data:text/css,{}", percent(css.as_bytes(), false, false));
                outer(&format!(
                    "<style>@import url({});</style>{rect}",
                    write_owned(&sheet, Ctx::StyleText, 0, 1)
                ))
            }
        };
        text.into_bytes()
    }

    fn style_element(&self, doc: &[u8], rect: &str) -> String {
        let cdata = self.carrier == Carrier::StyleCdata;
        let ctx = if cdata { Ctx::Cdata } else { Ctx::StyleText };
        let mut uri = self.uri_in(doc, "#p", ctx);
        let mut decoy = "";
        let mut closer = "";
        if let Some(split) = &self.split {
            let splitter = if cdata {
                CDATA_SPLITTER
            } else {
                ELEMENT_SPLITTERS[split.splitter % ELEMENT_SPLITTERS.len()]
            };
            let mut at = uri.len() * usize::from(split.at) / 1000;
            while !uri.is_char_boundary(at) {
                at += 1;
            }
            uri.insert_str(at, splitter);
            if let Some(d) = split.decoy {
                decoy = if cdata {
                    CDATA_DECOYS[d % CDATA_DECOYS.len()]
                } else {
                    ELEMENT_DECOYS[d % ELEMENT_DECOYS.len()]
                };
            }
            if split.closer {
                closer = CLOSERS[split.splitter % CLOSERS.len()];
            }
        }
        let css = format!("{decoy}rect{{fill:url({uri})}}");
        let sheet = if cdata {
            format!("<![CDATA[{css}]]>")
        } else {
            css
        };
        outer(&format!("<style>{sheet}</style>{rect}{closer}"))
    }
}

/// `text` written for `ctx`, with escapes drawn from `seed` (none when it is zero).
fn write_owned(text: &str, ctx: Ctx, seed: u64, density: usize) -> String {
    let mut rng = Rng::new(seed);
    let mut out = String::new();
    write_literal(&mut out, text, ctx, seed != 0, density, &mut rng);
    out
}

/// A document carrying `body`, grouped as `m` so a `<use>` one layer out can borrow it.
pub(super) fn outer(body: &str) -> String {
    format!(
        r#"<svg xmlns="{SVG_NS}" xmlns:xlink="{XLINK_NS}" width="{SIDE}" height="{SIDE}"><g id="m">{body}</g></svg>"#
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Op {
    Embed(Embed),
    Gzip,
    Utf16 {
        big_endian: bool,
        bom: bool,
        declare: bool,
    },
}

impl Op {
    pub(super) fn random(rng: &mut Rng) -> Self {
        match rng.below(10) {
            0 => Self::Gzip,
            1 => Self::Utf16 {
                big_endian: rng.one_in(2),
                bom: rng.one_in(2),
                declare: rng.one_in(2),
            },
            _ => Self::Embed(Embed::random(rng)),
        }
    }

    pub(super) fn apply(&self, doc: &[u8]) -> Vec<u8> {
        match self {
            Self::Embed(embed) => embed.wrap(doc),
            Self::Gzip => gzip(doc),
            Self::Utf16 {
                big_endian,
                bom,
                declare,
            } => utf16(doc, *big_endian, *bom, *declare),
        }
    }
}

/// `doc` re-encoded as UTF-16, its own `<?xml…?>` declaration replaced. Bytes that are
/// not UTF-8 text (a gzip layer) are left as they are.
pub(super) fn utf16(doc: &[u8], big_endian: bool, bom: bool, declare: bool) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(doc) else {
        return doc.to_vec();
    };
    let body = match text.strip_prefix("<?xml") {
        Some(rest) => rest.split_once("?>").map_or(text, |(_, after)| after),
        None => text,
    };
    let declaration = if declare {
        r#"<?xml version="1.0" encoding="UTF-16"?>"#
    } else {
        ""
    };
    let units = bom
        .then_some(0xFEFF_u16)
        .into_iter()
        .chain(declaration.encode_utf16())
        .chain(body.encode_utf16());
    let mut out = Vec::new();
    for unit in units {
        out.extend(if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        });
    }
    out
}

/// One generated input: a core and the ops around it, innermost first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Case {
    pub(super) core: Core,
    pub(super) ops: Vec<Op>,
}

impl Case {
    /// The most `data:` layers a case nests: one past the screen's limit, so a layer
    /// the screen undoes once and the decoder twice is reachable.
    pub(super) const MAX_EMBEDS: usize = MAX_DEPTH + 1;

    /// The largest input a case may build. Percent-encoding every byte triples a layer,
    /// so five such layers reach hundreds of KiB and cost a debug-build run seconds each
    /// in the entity check and the screen, while the shapes under test are about how a
    /// layer is spelled, not how big it is. A case over the bound is drawn again.
    pub(super) const MAX_BYTES: usize = 64 << 10;

    /// A random case and what it builds.
    pub(super) fn random(rng: &mut Rng) -> (Self, Vec<u8>) {
        loop {
            let case = Self::random_unbounded(rng);
            let bytes = case.build();
            if bytes.len() <= Self::MAX_BYTES {
                return (case, bytes);
            }
        }
    }

    fn random_unbounded(rng: &mut Rng) -> Self {
        let core = Core::random(rng);
        let mut ops = Vec::new();
        let mut embeds = 0;
        // Mostly shallow, since each layer is another chance to build something inert,
        // and a third of the time as deep as the grammar goes.
        let count = if rng.one_in(3) {
            1 + rng.below(Self::MAX_EMBEDS + 2)
        } else {
            1 + rng.below(3)
        };
        for _ in 0..count {
            let op = Op::random(rng);
            if matches!(op, Op::Embed(_)) {
                if embeds == Self::MAX_EMBEDS {
                    continue;
                }
                embeds += 1;
            }
            ops.push(op);
        }
        Self { core, ops }
    }

    pub(super) fn build(&self) -> Vec<u8> {
        self.ops
            .iter()
            .fold(self.core.text().into_bytes(), |doc, op| op.apply(&doc))
    }

    pub(super) fn embeds(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| matches!(op, Op::Embed(_)))
            .count()
    }
}
