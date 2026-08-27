//! The **authored** value types: what a theme file's scalars mean, and the parsing
//! and sanitising that turns one into something safe to render with.
//!
//! Everything here is a pure function or a newtype over a validated string, and that
//! is the module's whole boundary: a value that has left this module has already been
//! clamped, parsed, or refused. A theme is data from disk, so the discipline is
//! uniform — **clamp or refuse, never reject the theme** — and the newtypes
//! ([`CssSafeFontStack`], [`MarkerGlyph`]) exist so a caller cannot forget which of
//! their strings went through the check.

use super::keys::BULLET_TIERS;
use gtk::gdk;

pub(super) fn clamp_i32(v: i32, (lo, hi): (i32, i32)) -> i32 {
    v.clamp(lo, hi)
}
pub(super) fn clamp_f64(v: f64, (lo, hi): (f64, f64)) -> f64 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        lo
    }
}

/// Scale a themed design-time metric to actual pixels at `zoom`.
///
/// Every pixel metric a theme states is a design-time value at zoom 1.0, and pixel
/// metrics are widget/Pango properties — they do NOT follow the CSS `font-size`
/// rule zoom rides on, so they must be scaled explicitly on every render/zoom. This
/// is that one conversion; theming swapped the *source* of the number, not the
/// scaling machinery. `tags.rs` applies the same `round(n * zoom)` inline for the
/// metrics it batches.
pub(crate) fn px(n: i32, zoom: f64) -> i32 {
    (n as f64 * zoom).round() as i32
}

// ── colour parsing ────────────────────────────────────────────────────────────

/// Parse a theme colour: `#rrggbb`, `#rrggbb_aa` (hex alpha byte — the form the
/// data file documents, e.g. `#FFD133_61` = 38%), or anything else GDK accepts.
///
/// The `_aa` split exists because the two application paths consume alpha
/// differently: the tag path takes an RGBA straight (`set_background_rgba`),
/// while the Pango cell path needs it decomposed into separate attributes
/// (`background="#FFD133" bgalpha="38%"`). One key, two decompositions — see
/// [`ThemeColor`].
pub(crate) fn parse_color(s: &str) -> Option<gdk::RGBA> {
    if let Some((rgb, alpha)) = s.split_once('_') {
        let a = u8::from_str_radix(alpha, 16).ok()?;
        let base: gdk::RGBA = rgb.parse().ok()?;
        return Some(gdk::RGBA::new(
            base.red(),
            base.green(),
            base.blue(),
            a as f32 / 255.0,
        ));
    }
    s.parse::<gdk::RGBA>().ok()
}

// ── font-family sanitising ────────────────────────────────────────────────────

/// The CSS generic families. A stack must end in one of these: fontconfig
/// resolves an unknown family to the SANS default, not to serif, so a stack
/// without a generic terminator silently lands on sans and defeats the theme
/// (`fc-match Charter` → Noto Sans on a stock box).
const GENERIC_FAMILIES: &[&str] = &[
    "serif",
    "sans-serif",
    "sans",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
];

/// The generic appended to a stack that lacks one. `serif` because this styles the
/// preview *reading* pane, where the terminator is only ever reached if none of the
/// theme's own families resolved — an already-broken theme, for which a readable
/// serif is the better landing.
const DEFAULT_GENERIC: &str = "serif";

/// A font stack proven safe to interpolate into a CSS rule: stripped of every
/// CSS-significant character and guaranteed to end in a generic family.
///
/// # Security contract
///
/// This is a *proof-of-sanitisation* newtype. Its inner `String` is private and
/// its ONLY constructor is [`sanitize_font_family`] — there is no other way to
/// obtain a `CssSafeFontStack`. A live [`Theme`]'s `font_family` / `heading_font`
/// therefore hold this type rather than a bare `String`, which turns the old
/// "already sanitised — safe to interpolate" doc-comment guarantee into a
/// compiler-enforced one: an unsanitised `String` simply cannot be assigned to the
/// field, and the consumer (`preview::css`) interpolates it straight into a
/// stylesheet — an injection boundary held, before this seam, only by prose.
///
/// The projections below ([`as_str`](Self::as_str), [`Display`], and
/// `Deref<Target = str>`) are all *read-only*: they hand out the already-sanitised
/// text but give no way to *construct* the type, so the boundary holds regardless
/// of how the value is read. `Deref` in particular lets the non-CSS Pango consumer
/// (`tags::set_family`) keep treating the value as a `&str` without weakening the
/// construction gate. Themes are untrusted disk data; this type is what makes
/// interpolating them injection-safe by type instead of by convention.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CssSafeFontStack(String);

impl CssSafeFontStack {
    /// The sanitised, generic-terminated CSS font stack, ready to interpolate.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CssSafeFontStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::ops::Deref for CssSafeFontStack {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

/// Make a theme's font stack safe to interpolate into a CSS rule, and guarantee it
/// ends in a generic family. The `Some` return is a [`CssSafeFontStack`] — this
/// function is that type's sole constructor, so its output is the *only* value that
/// can ever reach `Theme::font_family` / `Theme::heading_font` and thence the CSS.
///
/// Themes are data from disk — untrusted input, not literals. A stray `}` or `;`
/// in a value would escape the rule and inject arbitrary CSS, so any entry
/// carrying CSS-significant punctuation is dropped rather than escaped (a font
/// family has no legitimate use for it). Returns `None` when nothing usable
/// survives, which falls the key through to the system font.
pub(crate) fn sanitize_font_family(s: &str) -> Option<CssSafeFontStack> {
    let mut families: Vec<String> = Vec::new();
    for raw in s.split(',') {
        let f = raw.trim().trim_matches(['"', '\'']).trim();
        if f.is_empty() {
            continue;
        }
        // Reject anything that could escape the rule, open a comment, or smuggle
        // an escape sequence. Font families are letters, digits, spaces, and hyphens.
        if !f
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.')
        {
            log::warn!("theme: dropping unsafe font family {f:?}");
            continue;
        }
        families.push(f.to_string());
    }
    if families.is_empty() {
        return None;
    }
    let ends_generic = families
        .last()
        .is_some_and(|f| GENERIC_FAMILIES.contains(&f.to_ascii_lowercase().as_str()));
    if !ends_generic {
        log::warn!(
            "theme: font stack {s:?} does not end in a generic family; \
             appending {DEFAULT_GENERIC:?} so an unresolved stack cannot fall back to sans"
        );
        families.push(DEFAULT_GENERIC.to_string());
    }
    // Re-emit from the parsed families rather than echoing the input: quote every
    // non-generic name (an unquoted multi-word IDENT run parses identically but is
    // ambiguous), leave generics bare so GTK treats them as generics.
    Some(CssSafeFontStack(
        families
            .iter()
            .map(|f| {
                if GENERIC_FAMILIES.contains(&f.to_ascii_lowercase().as_str()) {
                    f.clone()
                } else {
                    format!("\"{f}\"")
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

// ── decoration lines ──────────────────────────────────────────────────────────

/// The line styles a theme may state for a **decoration line** — a heading's rule, a
/// link's underline. A closed vocabulary parsed ONCE at the file boundary, so no
/// consumer ever matches on a string (POLICY "No magic numbers or magic strings"): the
/// tag path, the generated CSS and the export sinks each ask this type for their own
/// spelling of the same choice.
///
/// Four values, because that is what the *underline* attribute can express. Pango's
/// OVERLINE has only none/single, so [`LineStyle::overline`] clamps `Double`/`Wavy` down
/// to a single line rather than rejecting the theme — the same clamp-don't-reject
/// discipline every geometry key follows (TDD 18.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LineStyle {
    #[default]
    None,
    Single,
    Double,
    /// Pango spells this one `ERROR` (it is the spell-checker squiggle), which is a
    /// name about its *origin* rather than its appearance — a theme says `wavy`.
    Wavy,
}

impl LineStyle {
    /// Parse a theme's spelling. `None` (the `Option`) for anything unrecognised, so an
    /// unknown value falls back to the key's floor instead of failing the theme.
    pub(super) fn parse(s: &str) -> Option<LineStyle> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(LineStyle::None),
            "single" => Some(LineStyle::Single),
            "double" => Some(LineStyle::Double),
            "wavy" => Some(LineStyle::Wavy),
            _ => {
                log::warn!("theme: unknown line style {s:?} — falling back to the default");
                None
            }
        }
    }

    pub(crate) fn is_none(self) -> bool {
        self == LineStyle::None
    }

    /// The `GtkTextTag` underline attribute.
    pub(crate) fn underline(self) -> gtk::pango::Underline {
        match self {
            LineStyle::None => gtk::pango::Underline::None,
            LineStyle::Single => gtk::pango::Underline::Single,
            LineStyle::Double => gtk::pango::Underline::Double,
            LineStyle::Wavy => gtk::pango::Underline::Error,
        }
    }

    /// The `GtkTextTag` overline attribute — CLAMPED, see the type docs.
    pub(crate) fn overline(self) -> gtk::pango::Overline {
        match self {
            LineStyle::None => gtk::pango::Overline::None,
            _ => gtk::pango::Overline::Single,
        }
    }

    /// The Pango **markup** value, for the export sinks that spell attributes rather
    /// than set properties.
    pub(crate) fn pango_markup(self) -> &'static str {
        match self {
            LineStyle::None => "none",
            LineStyle::Single => "single",
            LineStyle::Double => "double",
            LineStyle::Wavy => "error",
        }
    }

    /// The CSS `text-decoration-style` value, or `None` where the line is absent (the
    /// caller then states `text-decoration-line: none` instead — CSS separates the two
    /// where Pango does not).
    pub(crate) fn css_style(self) -> Option<&'static str> {
        match self {
            LineStyle::None => None,
            LineStyle::Single => Some("solid"),
            LineStyle::Double => Some("double"),
            LineStyle::Wavy => Some("wavy"),
        }
    }
}

// ── theme-supplied glyphs ─────────────────────────────────────────────────────

/// The most characters a theme's marker glyph may carry. Generous enough for a
/// composed emoji (a ZWJ family sequence with variation selectors runs to seven), tight
/// enough that a "glyph" cannot be a paragraph — an unbounded string here is an
/// unbounded Pango layout in the paint path, on every list item, every frame.
const MAX_GLYPH_CHARS: usize = 8;

/// A theme-supplied decoration glyph, validated at the file boundary and — this is the
/// point of the type — reachable only through a projection **named for the grammar it
/// is going into**.
///
/// A glyph is the first theme-supplied TEXT to reach an exported artefact, and it
/// arrives in three different grammars: a plain `PangoLayout` in the drawn gutter, a
/// Pango *markup* string in the PDF sink, and HTML in the export sink. A single escape
/// is not sufficient for all three and a raw interpolation is wrong in two of them — an
/// un-escaped `&` fails `pango_parse_markup`, which renders the whole run EMPTY with no
/// warning (ScrAP-163), and an un-escaped `<` in HTML is an injection into a file this
/// project hands to a browser it does not control (TDD §25's untrusted-content rule,
/// which is stricter here, never looser).
///
/// So the inner string is private, and the only ways out are the three below. There is
/// no `Display`, no `Deref`, and no constructor but [`MarkerGlyph::parse`] — the same
/// proof-of-sanitisation shape [`CssSafeFontStack`] uses for the other free-form key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkerGlyph(String);

impl MarkerGlyph {
    /// Validate one authored glyph. `None` — meaning "this theme states no glyph" — for
    /// anything empty, over-long, or carrying a control character.
    ///
    /// **Over-long is REFUSED, never truncated.** Cutting a string at N `char`s can slice
    /// a grapheme cluster in half and leave a lone combining mark or half a ZWJ sequence,
    /// which is a worse rendering than the default marker the theme was trying to
    /// replace — and it would make the clamp's correctness depend on a grapheme
    /// segmenter this project does not carry. Refusing falls back to the drawn default,
    /// which is the same inert-by-default answer every other unset key gets.
    pub(super) fn parse(s: &str) -> Option<MarkerGlyph> {
        let g = s.trim();
        if g.is_empty() {
            return None;
        }
        if g.chars().any(|c| c.is_control()) {
            log::warn!("theme: marker glyph {s:?} carries a control character — ignored");
            return None;
        }
        let n = g.chars().count();
        if n > MAX_GLYPH_CHARS {
            log::warn!(
                "theme: marker glyph {s:?} is {n} chars (cap {MAX_GLYPH_CHARS}) — ignored \
                 rather than cut, which could split a grapheme cluster"
            );
            return None;
        }
        Some(MarkerGlyph(g.to_string()))
    }

    /// The glyph as PLAIN TEXT.
    ///
    /// Legitimate destinations are exactly two, and both are why this projection exists
    /// rather than a `Deref`: a plain-text API that performs no markup parsing
    /// (`GtkTextView::create_pango_layout`, the drawn gutter), and a caller that escapes
    /// the string it builds through its own sink-wide funnel (`export/pdf`'s marker,
    /// which `measure.rs` hands to `escape_pango` along with everything else it
    /// assembles). Anything else wants one of the two projections below.
    pub(crate) fn as_plain(&self) -> &str {
        &self.0
    }

    /// The glyph escaped for a **Pango markup** string.
    pub(crate) fn escaped_for_pango_markup(&self) -> String {
        glib::markup_escape_text(&self.0).to_string()
    }

    /// The glyph escaped for **HTML**, through the export sink's own escaper — so this
    /// project has one HTML escaper rather than one plus a copy that drifts.
    pub(crate) fn escaped_for_html(&self) -> String {
        crate::export::html::escape(&self.0)
    }
}

/// The tier a 1-based list nesting `depth` reads: depth 1 → 0, depth 2 → 1, depth 3 and
/// anything deeper → 2.
///
/// **The single definition of "which tier is this?"**, called by the drawn gutter, the
/// HTML sink and the PDF sink alike. A depth of 0 cannot arise from the renderer (the
/// outermost list is depth 1) but is answered anyway rather than underflowing, on the
/// same reasoning as `heading_scale_index`: a caller contract enforced by a clamp
/// somewhere else is exactly the arrangement that fails when the somewhere else moves.
pub(crate) fn depth_tier(depth: usize) -> usize {
    depth.saturating_sub(1).min(BULLET_TIERS - 1)
}

#[cfg(test)]
mod tests {
    use super::super::Themes;
    use super::*;

    /// TDD 18.24 — a marker glyph is validated at the file boundary, and everything it
    /// refuses falls back to the drawn default rather than failing the theme.
    #[test]
    fn a_marker_glyph_is_validated_and_refuses_rather_than_truncates() {
        assert_eq!(MarkerGlyph::parse("▸").unwrap().as_plain(), "▸");
        // Trimmed, because a TOML author's trailing space is not part of the glyph.
        assert_eq!(MarkerGlyph::parse("  ✓ ").unwrap().as_plain(), "✓");
        // A composed emoji is inside the cap on purpose — this is the case the cap
        // exists to admit, not the one it exists to reject.
        assert!(MarkerGlyph::parse("👨\u{200d}👩\u{200d}👧").is_some());
        // Empty / whitespace-only IS "unset".
        assert!(MarkerGlyph::parse("").is_none());
        assert!(MarkerGlyph::parse("   ").is_none());
        // A control character would break the layout it is dropped into.
        assert!(MarkerGlyph::parse("a\nb").is_none());
        assert!(MarkerGlyph::parse("\u{0007}").is_none());
        // Over-long is REFUSED, not cut: truncating at a char boundary can split a
        // grapheme cluster and leave a lone combining mark, which renders worse than
        // the default the theme was replacing.
        assert!(MarkerGlyph::parse("123456789").is_none());
        assert!(MarkerGlyph::parse("12345678").is_some());
    }

    /// TDD 18.24 — the escaping seam. ONE validated glyph, THREE grammars, and the
    /// projections are the only way out of the type.
    ///
    /// A single `markup_escape_text` is not sufficient once both export sinks are
    /// involved (`sdd/PLAN.preview-decoration.md` constraint 2): an un-escaped `&`
    /// fails `pango_parse_markup` and renders the whole run EMPTY with no warning
    /// (ScrAP-163), and an un-escaped `<` in HTML is an injection into a file this
    /// project hands to a browser. The plain projection is deliberately NOT escaped —
    /// it goes to a plain-text API — which is exactly why it has its own name.
    #[test]
    fn a_hostile_glyph_is_inert_in_every_grammar_it_reaches() {
        let g = MarkerGlyph::parse("<&\"x\"").expect("within the cap, no control chars");
        assert_eq!(g.as_plain(), "<&\"x\"");
        let pango = g.escaped_for_pango_markup();
        assert!(!pango.contains('<'), "{pango}");
        assert!(!pango.contains('&') || pango.contains("&amp;"), "{pango}");
        gtk::pango::parse_markup(&format!("<span>{pango}</span>"), '\0')
            .expect("an escaped glyph must not break the markup it lands in");
        let html = g.escaped_for_html();
        assert!(!html.contains('<'), "{html}");
        assert!(html.contains("&amp;"), "{html}");
    }

    /// TDD 18.26 — the tier map. A depth of 0 cannot arise from the renderer (the
    /// outermost list is depth 1) and is answered anyway rather than underflowing: a
    /// caller contract enforced by a clamp somewhere else is the arrangement that fails
    /// when the somewhere else moves.
    #[test]
    fn a_nesting_depth_maps_to_its_bullet_tier() {
        assert_eq!(depth_tier(1), 0);
        assert_eq!(depth_tier(2), 1);
        assert_eq!(depth_tier(3), 2);
        // Three-AND-DEEPER: every deeper level shares the last tier rather than
        // indexing past it.
        assert_eq!(depth_tier(4), 2);
        assert_eq!(depth_tier(99), 2);
        assert_eq!(depth_tier(usize::MAX), 2);
        // Total for the depth that cannot happen.
        assert_eq!(depth_tier(0), 0);
    }

    #[test]
    fn color_parses_plain_hex_and_the_alpha_suffix() {
        let c = parse_color("#FFD133_61").unwrap();
        assert_eq!(crate::palette::to_hex(c), "#ffd133");
        assert!((c.alpha() - 97.0 / 255.0).abs() < 1e-6);
        let p = parse_color("#f6d32d").unwrap();
        assert_eq!(p.alpha(), 1.0);
        assert!(parse_color("not a colour").is_none());
    }

    #[test]
    fn a_hostile_font_family_cannot_inject_css() {
        // The whole stack is punctuation-bearing → nothing usable survives.
        assert!(sanitize_font_family("Georgia; } * { color: red; }").is_none());
        // A hostile entry beside a safe one drops only the hostile entry…
        let s = sanitize_font_family("Georgia, \"}; evil {\", serif").unwrap();
        assert!(!s.contains('}') && !s.contains(';') && !s.contains('{'));
        assert!(s.contains("Georgia"));
        assert!(s.ends_with("serif"));

        // The type IS the guarantee: `sanitize_font_family` returns a
        // `CssSafeFontStack`, of which it is the sole constructor, and that is the
        // only value that can reach `Theme::font_family` / `Theme::heading_font`.
        // A raw, unsanitised String therefore cannot be assigned to the field —
        // the following would NOT compile (mutation-sanity, enforced by rustc):
        //
        //     let mut t = Themes::builtin().resolve("sepia");
        //     t.font_family = Some(String::from("Evil; } * { color: red; }"));
        //     //             ^ expected `Option<CssSafeFontStack>`, found `Option<String>`
        //
        // so an unsanitised value can never reach the CSS interpolation in preview::css.
        let field: Option<CssSafeFontStack> = sanitize_font_family("Georgia, serif");
        assert!(field.is_some());
    }

    /// The fontconfig trap: an unknown family resolves to SANS, not serif, so a
    /// stack without a generic terminator silently defeats the theme.
    #[test]
    fn a_font_stack_is_always_generic_terminated() {
        let s = sanitize_font_family("Charter, Georgia").unwrap();
        assert!(s.ends_with(DEFAULT_GENERIC));
        // An already-terminated stack is left alone (no double terminator).
        let t = sanitize_font_family("Charter, serif").unwrap();
        assert_eq!(t.as_str(), "\"Charter\", serif");
        // Generics stay bare; named families get quoted.
        let u = sanitize_font_family("Liberation Serif, monospace").unwrap();
        assert_eq!(u.as_str(), "\"Liberation Serif\", monospace");
    }

    #[test]
    fn the_shipped_sepia_stack_survives_sanitising_intact() {
        let t = Themes::builtin().resolve("sepia");
        let f = t.font_family.unwrap();
        for want in ["Charter", "Georgia", "Liberation Serif", "Noto Serif"] {
            assert!(f.contains(want), "sepia stack lost {want}: {f}");
        }
        assert!(f.ends_with("serif"));
    }
}
