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

/// The inclusive range a key's authored value is clamped into.
///
/// A NAMED pair rather than `(T, T)`. The two ends are the same type, so a bare tuple
/// admits a transposition that compiles, passes, and silently changes what a theme is
/// allowed to state — `(400, 0)` clamps every metric to 400. Field names make that
/// unrepresentable, and every read site says which end it wanted instead of `.0`/`.1`
/// (POLICY § Code style: destructure by name).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Clamp<T> {
    pub(crate) min: T,
    pub(crate) max: T,
}

impl Clamp<i32> {
    pub(super) fn apply(self, v: i32) -> i32 {
        v.clamp(self.min, self.max)
    }
}

impl Clamp<f64> {
    /// A non-finite value has no meaningful place in the range, so it lands on the
    /// floor rather than propagating a `NaN` into layout arithmetic.
    pub(super) fn apply(self, v: f64) -> f64 {
        if v.is_finite() {
            v.clamp(self.min, self.max)
        } else {
            self.min
        }
    }
}

/// Scale a themed design-time metric to actual pixels at `zoom`.
///
/// Every pixel metric a theme states is a design-time value at zoom 1.0, and pixel
/// metrics are widget/Pango properties — they do NOT follow the CSS `font-size`
/// rule zoom rides on, so they must be scaled explicitly on every render/zoom.
///
/// **This is the ONE conversion, and every consumer calls it.** Four call sites used
/// to re-declare it — `tags.rs`, `codeview/gutter.rs`, `codeview/mod.rs` and
/// `preview/build.rs` — and two of them had already drifted onto different rounding
/// semantics (`as i32`, which truncates toward zero, against `round()`), so the same
/// themed metric landed a pixel apart on the tag and on the marker drawn beside it.
/// That is exactly the tag-versus-marker drift POLICY's "One theme key, every
/// application path" rule exists to prevent, arriving through the scaling step rather
/// than through the key.
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

    /// The same stack spelled the way a **Pango font description** wants it — the CSS
    /// quoting removed.
    ///
    /// `FontDescription::set_family` is not CSS: Pango takes a plain comma-separated
    /// list and does its own ordered fallback across it, but the double quotes
    /// [`sanitize_font_family`] adds around a multi-word name break that parsing, and a
    /// stack Pango cannot parse falls through to its generic terminator. That failure is
    /// **silent and flattering** — `serif` is a plausible-looking answer for a theme that
    /// asked for `"DejaVu Serif"`, so an assertion of "not the default font" passes on
    /// a completely broken sink. Assert the resolved face by name.
    ///
    /// **One projection, two consumers**, and that is the whole point of it living on
    /// the type rather than beside either of them: the preview's tag sink
    /// (`tags::spec`) and the PDF sink (`export::pdf::measure`) both feed Pango, and
    /// this was `pub(super)` inside `tags/` while the PDF sink used the CSS spelling
    /// verbatim — one de-quoting projection that only one of the two callers could
    /// reach. The sanitiser has already made the value injection-safe and
    /// generic-terminated; only the quoting goes.
    pub(crate) fn pango_family(&self) -> String {
        self.0.replace('"', "")
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
        // Reject anything that could escape the rule, open a comment, or smuggle an
        // escape sequence.
        //
        // The admitted set, stated exactly: `char::is_alphanumeric` — which is
        // **Unicode-wide**, not ASCII: every Unicode Letter and Number, so a Cyrillic,
        // CJK or Devanagari family name is admitted, as it must be — plus space, `-`,
        // `_` and `.`.
        //
        // That is wider than "letters, digits, spaces and hyphens" and is still sound,
        // for a reason worth writing down rather than re-deriving: **no character that
        // can terminate a CSS string or begin a comment is alphanumeric.** The whole
        // hostile set — `"` `'` `\` `}` `{` `;` `:` `(` `)` `/` `*` `<` `>` `@` and
        // every control character, newline included — is punctuation or a control in
        // Unicode's general categories, none of which `is_alphanumeric` admits; and the
        // four explicit additions are each inert inside a quoted CSS string. So the
        // gate is a whitelist of two categories rather than of an alphabet, and it does
        // not narrow as new scripts are added to Unicode.
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
    /// assembles). Anything else wants one of the three projections below — **four
    /// grammars now, not three**: a marker glyph also reaches a CSS `content:` string,
    /// which is a fourth boundary with a fourth escape.
    pub(crate) fn as_plain(&self) -> &str {
        &self.0
    }

    /// The glyph escaped for a **Pango markup** string, through the export sink's own
    /// escaper — so this project has ONE Pango escaper rather than one plus a copy that
    /// drifts. Same reasoning as [`escaped_for_html`](Self::escaped_for_html), which was
    /// already held to it while this one was not.
    pub(crate) fn escaped_for_pango_markup(&self) -> String {
        crate::export::markup::escape_pango(&self.0)
    }

    /// The glyph escaped for **HTML**, through the export sink's own escaper — so this
    /// project has one HTML escaper rather than one plus a copy that drifts.
    pub(crate) fn escaped_for_html(&self) -> String {
        crate::export::html::escape(&self.0)
    }

    /// The glyph escaped for a **CSS string literal** — the grammar a `content:` value
    /// is in, and the fourth this one validated value reaches.
    ///
    /// TWO escapes composed, in this order and not the other: the HTML one first,
    /// because the marker rule lands in a `<style>` element inside an HTML document, and
    /// then the CSS-string one, because `\` and `"` are what can end or re-open the
    /// literal. Composing them at the call site is how the order gets reversed, which
    /// re-opens the boundary while looking escaped.
    ///
    /// **What the HTML pass buys here is narrower than "HTML escaping", and it is worth
    /// being exact about.** `<style>` is a *raw-text* element: an HTML parser decodes no
    /// character references inside it, so the entities this pass emits are NOT decoded
    /// back — a glyph of `&` reaches the page as the five characters `&amp;`, which is a
    /// display wart. What the pass is load-bearing for is the `<`: it is the only thing
    /// stopping a glyph from spelling `</style>` and closing the element out from under
    /// the sheet. So the pass stays until something else neutralises `<`; dropping it in
    /// favour of `css_string_escape(as_plain())` would fix the wart and open that.
    pub(crate) fn escaped_for_css_string(&self) -> String {
        crate::export::html::css_string_escape(&self.escaped_for_html())
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
    /// involved: an un-escaped `&` fails `pango_parse_markup` and renders the whole run
    /// EMPTY with no warning (ScrAP-163), and an un-escaped `<` in HTML is an injection
    /// into a file this project hands to a browser. The plain projection is deliberately NOT escaped —
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

        // FOUR grammars, not three. A marker glyph also reaches a CSS `content:`
        // string, and neither the HTML escape nor the Pango one closes that literal:
        // only `\\` and `"` can, and the projection has to compose the two escapes in
        // the right order — HTML first (the rule lands in a `<style>` element), then
        // the CSS-string one.
        let css = g.escaped_for_css_string();
        assert!(
            !css.contains('"') || css.contains("\\\""),
            "an unescaped quote ends the content string: {css}"
        );
        assert!(!css.contains('<'), "{css}");
        // The composition is not commutative and the order is load-bearing: escaping
        // for CSS first would turn `"` into `\"` and the HTML pass would then leave the
        // backslash while re-escaping nothing useful.
        assert_eq!(
            css,
            crate::export::html::css_string_escape(&g.escaped_for_html())
        );
    }

    /// ONE Pango escaper, not two. `escaped_for_pango_markup` used to call
    /// `glib::markup_escape_text` directly while `export::markup::escape_pango` sat
    /// beside it doing the same job — two implementations of one grammar, either of
    /// which could be corrected without the other.
    ///
    /// **This cannot mutation-test the delegation and does not pretend to** (GTK4Rs/AP-254's
    /// shape): once the projection calls `escape_pango`, both sides of any equality
    /// against `escape_pango` move together. What it CAN hold — and what makes deleting
    /// the second implementation safe — is that the surviving escaper agrees with
    /// GLib's canonical one over the whole alphabet a glyph can carry, and that its
    /// output parses.
    #[test]
    fn the_one_pango_escaper_agrees_with_glibs_over_a_glyphs_alphabet() {
        for raw in [
            "a", "<", "&", "\"", "'", ">", "<&>\"'", "\u{2739}", "a&b<c>d",
        ] {
            assert_eq!(
                crate::export::markup::escape_pango(raw),
                glib::markup_escape_text(raw).to_string(),
                "{raw:?}: this project's Pango escaper has diverged from GLib's"
            );
            let Some(g) = MarkerGlyph::parse(raw) else {
                continue;
            };
            assert_eq!(
                g.escaped_for_pango_markup(),
                crate::export::markup::escape_pango(raw)
            );
            // …and the result still parses as markup, which is the property the escape
            // exists for (ScrAP-163: an un-escaped `&` renders the whole run EMPTY).
            gtk::pango::parse_markup(
                &format!("<span>{}</span>", g.escaped_for_pango_markup()),
                '\0',
            )
            .unwrap_or_else(|e| panic!("{raw:?} broke the markup it landed in: {e}"));
        }
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

    /// **The one design-time→pixel conversion, pinned at its edges.**
    ///
    /// It had no unit test at all despite being the arithmetic applied to every themed
    /// metric on every render and every zoom — and four consumers spelled it themselves,
    /// two of them in `f32`, which is a different rounding of the same number. The
    /// cases below are exactly where the spellings could disagree.
    #[test]
    fn px_rounds_half_away_from_zero_and_is_the_identity_at_zoom_one() {
        // Zoom 1.0 must be the identity, or an unzoomed preview differs from the
        // design-time value the theme author wrote.
        for n in [0, 1, 3, 12, 400] {
            assert_eq!(px(n, 1.0), n, "zoom 1.0 must not move {n}");
        }
        // Exact halves round AWAY from zero (Rust's `f64::round`), which is what every
        // site must agree on — a truncating `as i32` would answer 5 and 3 here.
        assert_eq!(px(11, 0.5), 6);
        assert_eq!(px(7, 0.5), 4);
        assert_eq!(px(-7, 0.5), -4);
        // Below half rounds down, so a small metric at a small zoom can legitimately
        // vanish — the caller decides whether zero is acceptable, not this function.
        assert_eq!(px(1, 0.25), 0);
        assert_eq!(px(3, 0.5), 2);
        // Ordinary magnification.
        assert_eq!(px(12, 2.0), 24);
        assert_eq!(px(12, 1.75), 21);
        assert_eq!(px(13, 1.1), 14);
        // f32 and f64 must not be a choice a caller gets to make: this is the f64
        // answer, and `codeview::gutter` used to compute the same metric in f32.
        assert_eq!(px(400, 3.0), 1200);
    }

    /// SCHEMA § Key naming names FOUR colour spellings; all four are asserted here.
    ///
    /// The two this crate delegates to GDK — `#RRGGBBAA` and a CSS colour name —
    /// were the two nothing pinned, which is the wrong way round: a delegated
    /// spelling is exactly the one whose behaviour can change without this crate
    /// being touched.
    #[test]
    fn color_parses_every_documented_spelling() {
        // `#RRGGBB_AA` — this crate's own split, alpha as a hex byte.
        let c = parse_color("#FFD133_61").unwrap();
        assert_eq!(crate::palette::to_hex_opaque(c), "#ffd133");
        assert!((c.alpha() - 97.0 / 255.0).abs() < 1e-6);
        // `#RRGGBB` — opaque.
        let p = parse_color("#f6d32d").unwrap();
        assert_eq!(p.alpha(), 1.0);
        // `#RRGGBBAA` — GDK's own alpha spelling, the same colour as the underscored
        // form above, which is the claim SCHEMA makes about the pair.
        let packed = parse_color("#FFD13361").unwrap();
        assert_eq!(crate::palette::to_hex_opaque(packed), "#ffd133");
        assert!((packed.alpha() - c.alpha()).abs() < 1e-6);
        // A CSS colour name.
        let named = parse_color("rebeccapurple").unwrap();
        assert_eq!(crate::palette::to_hex_opaque(named), "#663399");
        assert_eq!(named.alpha(), 1.0);
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
        for want in [
            "Tiempos",
            "Charter",
            "Georgia",
            "Liberation Serif",
            "Noto Serif",
        ] {
            assert!(f.contains(want), "sepia stack lost {want}: {f}");
        }
        assert!(f.ends_with("serif"));
    }
}
