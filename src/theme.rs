//! Preview reading themes: the data model, the search path, and the resolution
//! order (POLICY "No hard-coded styling" / "One theme key, every application path").
//!
//! This module is the **engine**. It carries NO per-theme knowledge: no colour
//! constants, no `if theme == "sepia"` branches, no styling literals. Themes are
//! pure data in `data/themes.toml`, which is both installed alongside the app and
//! compiled in (`include_str!`) as the last-resort fallback — so "what does this
//! app hardcode?" is answerable by reading one data file, and a missing or
//! malformed themes file can never prevent startup (TDD 18.11/18.14).
//!
//! Three rules shape everything here:
//!
//! * **One key → one resolved value → every consumer.** A key that reached the
//!   `li-{depth}` tag but not the drawn marker gutter would strand every list
//!   marker — GTK4Rs/AP-96's exact failure mode. Resolution happens once, in
//!   [`Theme::resolve`]; consumers read the resolved [`Theme`], never the file.
//! * **The theme owns Pango SCALE, never CSS `font-size`.** Scale is a tag
//!   attribute GTK *multiplies* onto the CSS base (`gtktextattributes.c:349-351`),
//!   so it composes with zoom for free. `font-size` is a CSS longhand the zoom
//!   provider owns exclusively; a second provider writing the same lookup slot is
//!   arbitrated by provider ADD ORDER, not selector specificity (GTK4Rs/AP-101), which
//!   our `.scrib-win-<id>` scoping cannot arbitrate. There is no `font_size` key
//!   by decision, so the collision is impossible rather than managed.
//! * **Types over sanitisers.** Geometry is parsed to `i32` and clamped, so it
//!   *cannot* carry a `}` or `;` into generated CSS — injection is impossible by
//!   construction. Only the one genuinely free-form string (`font_family`) needs
//!   a sanitiser, and colours are re-emitted from parsed `RGBA`, never echoed.
//!
//! Pure and display-free end to end: parsing, merging, clamping, and resolution
//! are all unit-tested without a GTK display; the GTK probe stays at the edge in
//! `palette::Palette::resolve`.

use gtk::gdk;
use gtk::glib;
use std::collections::BTreeMap;

/// The shipped theme data, compiled in. This is the SAME file `install.sh`
/// installs, so the built-in fallback and the installed default can never drift.
const BUILTIN_THEMES_TOML: &str = include_str!("../data/themes.toml");

/// The id of the base theme — the register of everything the app would otherwise
/// hardcode, and link 2 of the resolution order. Not privileged in any other way:
/// every key is available to every theme, and this one merely *happens* to hold
/// today's values.
pub(crate) const SYSTEM_ID: &str = "system";

// ── the last-resort floor ─────────────────────────────────────────────────────
//
// Resolution must be TOTAL: every geometry/typography key has to produce a value
// even if `[themes.system]` somehow lacks it. These are that floor. They are NOT
// a second source of truth — `builtin_system_spec_matches_the_floor` asserts each
// one equals the shipped `data/themes.toml` `[themes.system]` value, so the data
// file stays the place a human reads and edits, and drift is a test failure.

const F_HEADING_SCALE: [f64; 5] = [2.2, 1.8, 1.48, 1.2, 1.0];
const F_HEADING_WEIGHT: i32 = 700;
const F_BOLD_WEIGHT: i32 = 700;
const F_SUPSUB_SCALE: f64 = 0.72;
const F_SUPERSCRIPT_RISE: i32 = 4;
const F_SUBSCRIPT_RISE: i32 = -2;
const F_HEADING_SPACE_BELOW: [i32; 5] = [4, 4, 2, 2, 2];
/// Zero, because the heading tags set no `pixels_above_lines` at all before this key
/// existed — the floor IS today's rendering, which is what keeps System byte-identical
/// (TDD 18.2). Not symmetric with the below-floor by accident: only space-below was
/// ever expressed.
const F_HEADING_SPACE_ABOVE: [i32; 5] = [0, 0, 0, 0, 0];
/// No heading carries a band until a theme states a fill for its level, so the radius
/// is only ever consulted for a band that exists.
const F_HEADING_BAND_RADIUS: i32 = 0;
/// No heading rule is drawn today, on either side.
const F_HEADING_OVERLINE: LineStyle = LineStyle::None;
const F_HEADING_UNDERLINE: LineStyle = LineStyle::None;
/// A body link has been underlined with a single line since before themes existed, so
/// unlike the heading rule's floor this one is NOT "none" — it is the shipped look, and
/// changing it would move System (TDD 18.2).
const F_LINK_UNDERLINE: LineStyle = LineStyle::Single;
const F_BQ_BAR_WIDTH: i32 = 3;
const F_BQ_TEXT_GAP: i32 = 10;
const F_LIST_STEP: i32 = 28;
const F_LIST_ITEM_GAP: i32 = 8;
const F_RULE_SPACE: i32 = 4;
const F_TABLE_CELL_PADDING_V: i32 = 4;
const F_TABLE_CELL_PADDING_H: i32 = 10;
const F_TABLE_BORDER_WIDTH: i32 = 1;
const F_TABLE_CELL_RADIUS: i32 = 0;

// ── clamp ranges ──────────────────────────────────────────────────────────────
//
// A malformed or hostile theme (`list_step = -5`, or `10000`) must not be able to
// break layout (TDD 18.11). Clamping — rather than rejecting — keeps a theme that
// is merely over-enthusiastic usable, and keeps resolution total.

const SCALE_RANGE: (f64, f64) = (0.25, 8.0);
const WEIGHT_RANGE: (i32, i32) = (100, 1000);
const RISE_RANGE: (i32, i32) = (-64, 64);
/// Decoration metrics: no negative sizes, and nothing wide enough to push the
/// text column off its own viewport.
const METRIC_RANGE: (i32, i32) = (0, 400);
/// A list step of 0 would stack every nesting depth in one column and bury the
/// drawn markers under the text, so this one has a positive floor.
const LIST_STEP_RANGE: (i32, i32) = (4, 400);

fn clamp_i32(v: i32, (lo, hi): (i32, i32)) -> i32 {
    v.clamp(lo, hi)
}
fn clamp_f64(v: f64, (lo, hi): (f64, f64)) -> f64 {
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
    fn parse(s: &str) -> Option<LineStyle> {
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
    fn parse(s: &str) -> Option<MarkerGlyph> {
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

/// How many nesting-depth tiers a BULLET's decoration is stated in: depth 1, depth 2,
/// and depth 3-and-deeper (TDD 18.26).
///
/// Three rather than "one per depth" because a list nests arbitrarily and a theme cannot
/// state a value for every depth it might meet — so the deepest tier is a catch-all, and
/// `depth_tier` is the one function that says so.
pub(crate) const BULLET_TIERS: usize = 3;

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

/// Every list-marker glyph a theme may state, one per marker kind. Each is `None`
/// unless the theme set it AND [`MarkerGlyph::parse`] accepted it, so a rejected glyph
/// degrades to the drawn default — never a partial or broken marker.
///
/// The task marker gets TWO, because it has two states and they must stay tellable
/// apart; they resolve independently, so a theme may state either alone (a "tick or
/// nothing" look) as deliberately as it may state both.
///
/// The BULLET gets [`BULLET_TIERS`], by nesting depth (TDD 18.26) — already folded, so a
/// tier the theme left unset carries the next shallower tier's value and every consumer
/// indexes rather than re-deriving the fallback. Only the bullet: an ordered numeral at
/// depth 3 is still a numeral and a task box is still a box, so those stay single-valued.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ListGlyphs {
    pub bullet: [Option<MarkerGlyph>; BULLET_TIERS],
    pub ordered: Option<MarkerGlyph>,
    pub task: Option<MarkerGlyph>,
    pub task_checked: Option<MarkerGlyph>,
}

// ── the file model ────────────────────────────────────────────────────────────

/// One theme exactly as authored. Every key is optional: a theme states only what
/// makes it distinctive, and anything it omits resolves through `[themes.system]`
/// and then the desktop GTK theme.
#[derive(serde::Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub(crate) struct ThemeSpec {
    /// Display name for the chooser. Falls back to the theme's id.
    pub name: Option<String>,
    /// Optional colour-emoji / unicode symbol shown to the LEFT of the theme's name in
    /// the picker (menu + toolbar). Pure decoration; omit for no symbol.
    pub symbol: Option<String>,

    // Base colours. A named theme injects these instead of probing the desktop;
    // every derived colour follows for free, because the derivation is already a
    // pure function of exactly these three (see `palette::Palette::from_base`).
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub accent: Option<String>,
    pub font_family: Option<String>,
    pub syntect_theme: Option<String>,

    // Derived colours — omit to derive, set to override.
    /// Heading foreground (h1–h6). Omit ⇒ headings inherit the body foreground
    /// (the default — the heading tags set only scale/weight). Set ⇒ all headings
    /// take this colour (a link inside a heading still wins, being higher priority).
    pub heading_color: Option<String>,
    /// Heading font family (h1–h6 + the table header). Omit ⇒ headings use the body
    /// `font_family`. Set ⇒ a distinct heading face (e.g. a display font like Orbitron)
    /// while the body keeps a readable one — the honest fix for "one family, two scales".
    /// A CSS stack, sanitised and generic-terminated exactly like `font_family`.
    pub heading_font: Option<String>,
    /// Per-level heading colours, h1 · h2 · h3 · h4 · h5-and-deeper. FIVE slots, the
    /// same shape and the same fold as `heading_scale` (h6 maps onto the h5 tag before
    /// a tag is ever chosen, so no theme can differentiate them). A slot left EMPTY
    /// (`""`) — or absent, because the array is short — falls back to the single
    /// `heading_color`, so a theme states only the levels it wants to distinguish.
    /// The table header is NOT a level and keeps reading `heading_color`.
    pub heading_colors: Option<Vec<String>>,
    /// Per-level heading font stacks, same five slots and same empty-means-inherit rule
    /// as `heading_colors`, falling back to `heading_font`. Each slot is sanitised and
    /// generic-terminated exactly like `font_family`.
    pub heading_fonts: Option<Vec<String>>,
    /// A rule ABOVE the heading text: `"none"` (the default — no rule, exactly as
    /// before this key existed) or `"single"`. Pango's overline has no other values, so
    /// `"double"`/`"wavy"` are accepted and clamped to a single line.
    ///
    /// ⚠️ **There is deliberately no `heading_overline_rgba` key**, so this rule always
    /// takes the heading's own ink. GTK 4.6 DOUBLE-FREES a text run that carries both a
    /// coloured overline and a coloured underline — see [`HeadingRule`] for the
    /// measurement and why the key's absence, rather than a rule about it, is the fix.
    pub heading_overline: Option<String>,
    /// A rule BELOW the heading text: `"none"` (the default), `"single"`, `"double"` or
    /// `"wavy"`. This is the text-decoration line under the glyph run, not a
    /// column-width divider.
    pub heading_underline: Option<String>,
    /// The underline's colour. Omitted ⇒ the line follows the heading's own foreground,
    /// which is what a `GtkTextTag` does when `underline-rgba` is never set.
    pub heading_underline_rgba: Option<String>,
    /// The BAND behind a heading's text, per level (h1 · h2 · h3 · h4 · h5-and-deeper),
    /// the same five slots and the same empty-means-unset rule as `heading_colors`. A
    /// level with no fill carries no band, which is every level of every theme until one
    /// asks — so the decoration is absent rather than defaulted (TDD 18.2).
    ///
    /// Per level because "band the h1 only" is the ordinary want; the band's SHAPE
    /// (radius, gradient, sprite) is one description shared by every level that has one.
    pub heading_band_bg: Option<Vec<String>>,
    /// A second stop. Stated, the band is a vertical linear gradient from the level's own
    /// fill down to this colour; omitted, it is a flat fill.
    pub heading_band_gradient_to: Option<String>,
    /// Corner radius of the band, design-time px at zoom 1.0.
    pub heading_band_radius: Option<i32>,
    /// A sprite TILED across the band, in place of its fill. Theme-relative and validated
    /// like every sprite key. Outranks the fill and the gradient, the same way a marker
    /// sprite outranks a marker glyph.
    pub sprite_heading_band: Option<String>,
    pub link: Option<String>,
    /// A link's underline style: `"single"` (the default, and what the app drew before
    /// this key existed), `"double"`, `"wavy"`, or `"none"` for a coloured link with no
    /// line at all.
    pub link_underline: Option<String>,
    /// The link underline's colour, independent of the link's ink. Omitted ⇒ the line
    /// follows the link colour, exactly as before this key existed.
    pub link_underline_rgba: Option<String>,
    pub code_inline_bg: Option<String>,
    pub code_block_bg: Option<String>,
    pub blockquote_bar: Option<String>,
    pub selection_bg: Option<String>,
    /// The ink SELECTED text is drawn in, over `selection_bg`. Omit ⇒ derived from the
    /// page and its ink (see `palette::Palette::from_base`), which is right often enough
    /// that no shipped theme but Bedtime states it. State it when the derived answer is
    /// merely *legible* rather than *good*: Bedtime's sand ink clears 5.3:1 on its violet
    /// selection and still looks wrong there, which is a judgement no contrast ratio makes.
    pub selection_fg: Option<String>,
    pub table_border: Option<String>,
    pub table_head_bg: Option<String>,
    pub rule: Option<String>,
    /// List-marker glyph colour — the unordered bullet dot, the ordered numeral, and
    /// the task checkbox outline+tick (all three drawn in the left gutter). Colours the
    /// MARKER ONLY; the item's text keeps the body foreground. Omit ⇒ markers inherit
    /// the widget foreground (the pre-theming default). One key for all three kinds.
    pub list_marker: Option<String>,
    /// The BULLET's colour at nesting depth 2, and at depth 3-and-deeper (TDD 18.26).
    /// Each optional; unset falls back to the next shallower tier, so an unstated
    /// `list_marker_3` takes `list_marker_2`'s value and an unstated `list_marker_2`
    /// takes `list_marker`'s — which is exactly today's behaviour when neither is set.
    ///
    /// ⚠️ **Bullet only**, unlike the un-suffixed `list_marker` beside them, which
    /// colours all three marker kinds. A nested ordered list's numeral and a nested task
    /// box keep the shared colour: they are the same marker at any depth, where a bullet
    /// dot is the one whose whole job is to say which level you are on.
    pub list_marker_2: Option<String>,
    pub list_marker_3: Option<String>,
    /// Glyph strings that stand in for the DRAWN list markers — the bullet dot, the
    /// ordered numeral, and the task checkbox in each of its two states. Validated by
    /// [`MarkerGlyph::parse`]; unset (or refused) ⇒ the drawn default, unchanged.
    ///
    /// Replacing the ordered numeral DISCARDS the ordinal, which is a deliberate theme
    /// choice rather than an oversight: the key is inert unless a theme asks for it, and
    /// a theme that wants numbers simply does not state it.
    pub list_bullet_glyph: Option<String>,
    /// The bullet glyph at depth 2 / depth 3-and-deeper, folding to the shallower tier
    /// exactly as `list_marker_2`/`_3` do (TDD 18.26).
    pub list_bullet_glyph_2: Option<String>,
    pub list_bullet_glyph_3: Option<String>,
    pub list_ordered_glyph: Option<String>,
    pub list_task_glyph: Option<String>,
    pub list_task_checked_glyph: Option<String>,
    /// Sprite files that stand in for the same four markers. Resolved and validated by
    /// `crate::sprite::resolve` at load time, exactly as `sprite_annotation_chip` is.
    /// A sprite WINS over a glyph for the same marker — it is the more specific and the
    /// dearer opt-in, so stating both is answered by the one the theme paid more for.
    pub sprite_list_bullet: Option<String>,
    /// The bullet sprite at depth 2 / depth 3-and-deeper, folding the same way.
    pub sprite_list_bullet_2: Option<String>,
    pub sprite_list_bullet_3: Option<String>,
    pub sprite_list_ordered: Option<String>,
    pub sprite_list_task: Option<String>,
    pub sprite_list_task_checked: Option<String>,

    /// The colour of the line struck through `~~text~~`. Omitted ⇒ the line follows the
    /// struck text's own foreground, which is what a `GtkTextTag` does when
    /// `strikethrough-rgba` is never set — and what every theme did before this key.
    pub strikethrough_rgba: Option<String>,

    /// Ink for `==marked==` text. Omit ⇒ the marked text keeps the body foreground and
    /// only its background changes, which is how a highlighter behaves on paper and is
    /// right for every theme whose `mark_bg` is a wash. State it when the band is opaque
    /// enough to need its own ink — it reaches BOTH the body tag and the table-cell
    /// Pango path, like `mark_bg` itself.
    pub mark_fg: Option<String>,

    // Overlay colours. Each of these reaches BOTH the body path and the
    // table-cell path (TDD 18.6) — the representations differ, the source is one key.
    pub annotation_hl: Option<String>,
    pub find_hl_all: Option<String>,
    pub find_hl_current: Option<String>,
    /// `==highlight==` (mark) background wash. Like the three above it reaches BOTH
    /// the body tag and the table-cell Pango span from this one key. Per-theme,
    /// because the right highlighter colour varies with the page (a warm wash on
    /// cream, a neon wash on a dark page) — themes that omit it take the floor.
    pub mark_bg: Option<String>,

    // Typography — Pango attributes, so all inherently zoom-safe.
    pub heading_scale: Option<Vec<f64>>,
    pub heading_weight: Option<i32>,
    pub bold_weight: Option<i32>,
    pub supsub_scale: Option<f64>,
    pub superscript_rise: Option<i32>,
    pub subscript_rise: Option<i32>,

    // Decoration geometry — design-time px at zoom 1.0, scaled on apply.
    pub heading_space_below: Option<Vec<i32>>,
    /// Space ABOVE each heading, h1..h5. The counterpart `heading_space_below` has
    /// always existed; this side never did, so its floor is `[0, 0, 0, 0, 0]` — the
    /// heading tags set no `pixels_above_lines` before this key.
    pub heading_space_above: Option<Vec<i32>>,
    pub blockquote_bar_width: Option<i32>,
    pub blockquote_text_gap: Option<i32>,
    pub list_step: Option<i32>,
    pub list_item_gap: Option<i32>,
    pub rule_space: Option<i32>,
    pub table_cell_padding_v: Option<i32>,
    pub table_cell_padding_h: Option<i32>,
    pub table_border_width: Option<i32>,
    pub table_cell_radius: Option<i32>,

    // Annotation chip — closes a hardcoded-styling deviation (`codeview/mod.rs`),
    // and the first entry in the closed decoration vocabulary. `None` on every
    // key ⇒ the chip stays exactly the hardcoded amber/white it always was
    // (TDD 18.2).
    /// Chip fill colour.
    pub annotation_chip_bg: Option<String>,
    /// Chip ink (the overflow count's numeral).
    pub annotation_chip_fg: Option<String>,
    /// A sprite drawn in place of the flat chip fill. Relative to this file's
    /// directory; resolved and validated by `crate::sprite::resolve` at load time
    /// (`sdd/PLAN.preview-decoration.md` — "a theme naming a FILE" is the dearest of
    /// the three untrusted-input classes, so this key alone does not loosen the
    /// "no icon, no arbitrary path" rule the rest of the theme model holds).
    pub sprite_annotation_chip: Option<String>,
}

#[derive(serde::Deserialize, Default, Debug)]
struct ThemesFile {
    #[serde(default)]
    themes: BTreeMap<String, ThemeSpec>,
}

/// Every installed theme, keyed by id.
#[derive(Debug, Clone)]
pub(crate) struct Themes {
    specs: BTreeMap<String, ThemeSpec>,
}

impl Themes {
    /// Parse a themes file. Pure — no filesystem, no environment, no display.
    /// A malformed file yields `None` so the caller can fall back rather than
    /// surface a broken app (the same discipline `Config::parse` follows).
    fn parse(text: &str) -> Option<BTreeMap<String, ThemeSpec>> {
        match toml::from_str::<ThemesFile>(text) {
            Ok(f) => Some(f.themes),
            Err(e) => {
                log::warn!("theme: themes.toml parse error: {e} — ignoring this file");
                None
            }
        }
    }

    /// The compiled-in themes. Total by construction: if the shipped data file
    /// ever failed to parse, every key would still resolve through the floor
    /// consts, so the app renders rather than dies. `builtin_parses` catches that
    /// at test time, where it belongs.
    pub(crate) fn builtin() -> Self {
        Themes {
            specs: Themes::parse(BUILTIN_THEMES_TOML).unwrap_or_default(),
        }
    }

    /// Merge a user file's themes over these, per theme id and per key: a user can
    /// override ONE key of a shipped theme without restating it, or add a whole new
    /// theme (TDD 18.13/18.14). Pure.
    fn merge_over(&mut self, user: BTreeMap<String, ThemeSpec>) {
        for (id, spec) in user {
            match self.specs.get_mut(&id) {
                Some(base) => base.overlay(spec),
                None => {
                    self.specs.insert(id, spec);
                }
            }
        }
    }

    /// Merge an inline themes-file fragment over these, as a user file would.
    /// Test-only seam so a test elsewhere in the crate can exercise a themed value
    /// end-to-end without writing a file or touching the search path.
    #[cfg(test)]
    pub(crate) fn merge_over_for_test(&mut self, toml_text: &str) {
        let user = Themes::parse(toml_text).expect("test theme fragment must parse");
        self.merge_over(user);
    }

    /// Theme ids paired with display names, in chooser order: the system theme
    /// first (it is the default), then every other theme by display name.
    /// Deterministic — TOML tables carry no authoring order we could honour.
    pub(crate) fn chooser_list(&self) -> Vec<(String, String, Option<String>)> {
        let entry = |id: &String, s: &ThemeSpec| {
            (
                id.clone(),
                s.name.clone().unwrap_or_else(|| id.clone()),
                s.symbol.clone(),
            )
        };
        let mut rest: Vec<(String, String, Option<String>)> = self
            .specs
            .iter()
            .filter(|(id, _)| id.as_str() != SYSTEM_ID)
            .map(|(id, s)| entry(id, s))
            .collect();
        rest.sort_by(|a, b| a.1.cmp(&b.1)); // by NAME (not the symbol-prefixed label)
        let mut out = Vec::with_capacity(rest.len() + 1);
        if let Some(sys) = self.specs.get(SYSTEM_ID) {
            out.push(entry(&SYSTEM_ID.to_string(), sys));
        }
        out.extend(rest);
        out
    }

    /// The picker label for a theme: `"<symbol>  <name>"` when it has a symbol, else
    /// just the name. Shared by both surfaces so they read identically.
    pub(crate) fn chooser_label(name: &str, symbol: Option<&str>) -> String {
        match symbol {
            Some(s) => format!("{s}\u{2002}{name}"),
            None => name.to_string(),
        }
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.specs.contains_key(id)
    }

    /// Resolve `id` against the resolution order's links 1 and 2. An unknown id
    /// resolves as the system theme, so a stale persisted selection (a theme the
    /// user deleted) degrades to the default instead of failing.
    pub(crate) fn resolve(&self, id: &str) -> Theme {
        let system = self.specs.get(SYSTEM_ID).cloned().unwrap_or_default();
        let selected = self
            .specs
            .get(id)
            .filter(|_| id != SYSTEM_ID)
            .cloned()
            .unwrap_or_default();
        Theme::resolve(id, &selected, &system)
    }
}

impl ThemeSpec {
    /// Overlay `other`'s set keys onto self, leaving self's value wherever `other`
    /// is silent. This is the per-key half of both the user-file merge and the
    /// selected→system resolution.
    fn overlay(&mut self, other: ThemeSpec) {
        macro_rules! take {
            ($($f:ident),+ $(,)?) => { $( if other.$f.is_some() { self.$f = other.$f; } )+ };
        }
        take!(
            name,
            symbol,
            background,
            foreground,
            accent,
            font_family,
            syntect_theme,
            heading_color,
            heading_font,
            heading_colors,
            heading_fonts,
            heading_overline,
            heading_underline,
            heading_underline_rgba,
            heading_band_bg,
            heading_band_gradient_to,
            heading_band_radius,
            sprite_heading_band,
            link,
            link_underline,
            link_underline_rgba,
            strikethrough_rgba,
            code_inline_bg,
            code_block_bg,
            blockquote_bar,
            selection_bg,
            selection_fg,
            table_border,
            table_head_bg,
            rule,
            list_marker,
            list_marker_2,
            list_marker_3,
            list_bullet_glyph,
            list_bullet_glyph_2,
            list_bullet_glyph_3,
            list_ordered_glyph,
            list_task_glyph,
            list_task_checked_glyph,
            sprite_list_bullet,
            sprite_list_bullet_2,
            sprite_list_bullet_3,
            sprite_list_ordered,
            sprite_list_task,
            sprite_list_task_checked,
            mark_fg,
            annotation_hl,
            find_hl_all,
            find_hl_current,
            mark_bg,
            heading_scale,
            heading_weight,
            bold_weight,
            supsub_scale,
            superscript_rise,
            subscript_rise,
            heading_space_below,
            heading_space_above,
            blockquote_bar_width,
            blockquote_text_gap,
            list_step,
            list_item_gap,
            rule_space,
            table_cell_padding_v,
            table_cell_padding_h,
            table_border_width,
            table_cell_radius,
            annotation_chip_bg,
            annotation_chip_fg,
            sprite_annotation_chip,
        );
    }
}

// ── the resolved theme ────────────────────────────────────────────────────────

/// A theme colour that carries its own alpha, resolved once and decomposed on
/// demand for whichever application path needs it. `annotation_hl` is the reason
/// this type exists: the tag path takes the RGBA directly, while a table cell is a
/// `GtkLabel` outside the buffer (ScrAP-36) and needs Pango markup with the alpha as
/// a separate attribute. One key, two decompositions — the generator owns the
/// split so the two paths cannot drift (TDD 18.6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThemeColor(pub(crate) gdk::RGBA);

impl ThemeColor {
    pub(crate) fn rgba(self) -> gdk::RGBA {
        self.0
    }
    /// `#rrggbb`, alpha dropped — for the paths that take colour and alpha apart.
    pub(crate) fn hex(self) -> String {
        crate::palette::to_hex(self.0)
    }
    /// Alpha as a Pango percentage attribute value, e.g. `38%`.
    pub(crate) fn alpha_pct(self) -> String {
        format!("{}%", (self.0.alpha() * 100.0).round() as i32)
    }
    /// The 16-bit-per-channel triple a `GtkLabel`'s Pango attribute list wants.
    pub(crate) fn u16_triple(self) -> (u16, u16, u16) {
        let ch = |x: f32| (x.clamp(0.0, 1.0) * 65535.0).round() as u16;
        (ch(self.0.red()), ch(self.0.green()), ch(self.0.blue()))
    }
}

/// Typography — all Pango tag attributes, so all compose with zoom for free.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Typography {
    /// h1, h2, h3, h4, h5-and-deeper. FIVE entries, not six: `emit.rs` maps
    /// h6-and-deeper to the h5 tag before a tag is ever chosen, so no theme can
    /// differentiate h6 from h5 however it is keyed. Honest to the renderer — h6 is
    /// a deliberate fold-into-deepest on every surface.
    pub heading_scale: [f64; 5],
    pub heading_weight: i32,
    pub bold_weight: i32,
    pub supsub_scale: f64,
    /// Points, converted to Pango units at apply time.
    pub superscript_rise: i32,
    pub subscript_rise: i32,
}

impl Typography {
    /// Pango markup attribute fragment for themed bold — e.g. ` weight="600"`, leading
    /// space included so a caller can splice it straight into a `<span…>` open tag.
    ///
    /// Shared by every representation OUTSIDE the buffer (table-cell `GtkLabel`
    /// markup, PDF/HTML export markup) so `bold_weight` cannot silently apply on the
    /// body `GtkTextTag` alone and drift from a bold word in a table or an exported
    /// document (TDD 18.18 — the prerequisite the whole decoration plan is gated on,
    /// `sdd/PLAN.preview-decoration.md` constraint 1).
    pub(crate) fn bold_attr(&self) -> String {
        format!(" weight=\"{}\"", self.bold_weight)
    }

    /// Pango markup attribute fragment for themed super/subscript — `size` AND
    /// `rise` together, the same two properties `tags.rs` applies to the body tag via
    /// `set_scale`/`set_rise`. `superscript` selects which rise; `subscript_rise`'s
    /// own floor is already negative (`F_SUBSCRIPT_RISE = -2`), so no sign flip
    /// happens here — the raw theme value is exactly what `tags.rs` feeds `set_rise`,
    /// just re-expressed as Pango markup's `rise` (also Pango units) instead of the
    /// tag property.
    pub(crate) fn supsub_attr(&self, superscript: bool) -> String {
        let pct = (self.supsub_scale * 100.0).round().max(1.0) as i32;
        let rise = if superscript {
            self.superscript_rise
        } else {
            self.subscript_rise
        };
        format!(" size=\"{pct}%\" rise=\"{}\"", rise * gtk::pango::SCALE)
    }
}

/// The optional rule a theme may draw above and/or below a heading's text (TDD 18.22).
///
/// Both sides default to [`LineStyle::None`], so a theme that states neither leaves the
/// heading tags byte-identical to before this decoration existed (TDD 18.2). A colour
/// left `None` means "do not set the property", which is how a `GtkTextTag` line follows
/// the run's own foreground — NOT a derived default we would have to keep in step.
///
/// # Why only ONE of the two sides carries a colour
///
/// **GTK 4.6.9 double-frees a text run that carries a coloured overline AND a coloured
/// underline.** MEASURED here, minimal: build a `GtkTextTag`, set `overline-rgba` and
/// `underline-rgba`, drop it — valgrind reports `Invalid free()` of a 16-byte block (a
/// `GdkRGBA`) freed twice inside GTK's own finalize path, and a few repetitions poison
/// the heap until an unrelated `gtk::Box::new` SIGSEGVs somewhere else entirely.
/// Characterised against a positive control: either colour ALONE avoids the invalid
/// free; the same colour on both still corrupts; every other `*-rgba` pair (foreground,
/// strikethrough) is clean; and **splitting the two across two tags applied to the same
/// range does not escape it** — the invariant is per RUN, not per tag. That last part is
/// what decides the design: a link inside a heading carries the heading's tag and the
/// link's tag at once, and the link tag colours an underline.
///
/// **ROOT CAUSE, confirmed against `gtk 4.6.9-5-g492b44f20c` source
/// (`gtk/gtktextattributes.c`, `gtk_text_attributes_unref`) — NOT an aliasing bug, a
/// one-line copy-paste typo in the destructor**:
/// ```c
/// if (values->appearance.underline_rgba)
///     gdk_rgba_free (values->appearance.underline_rgba);
/// if (values->appearance.overline_rgba)
///     gdk_rgba_free (values->appearance.underline_rgba);   /* guard says overline, free says underline */
/// ```
/// Every copy path (`copy_values`, the run-merge in `_gtk_text_attributes_fill_from_tags`)
/// deep-copies each field independently — no pointer is ever shared. The corruption is
/// this one destructor: with both fields set, `underline_rgba` is freed twice and
/// `overline_rgba` leaks; with only `overline_rgba` set, nothing is double-freed but that
/// 16 B still leaks on every tag/attributes destruction, which is why the theme vocabulary
/// has no `heading_overline_rgba` key at all rather than merely warning against combining
/// it with `heading_underline_rgba`. Fixed upstream by commit
/// `86e962929bf2be13a721053141b33e4381f0312` ("gtktextattributes: Make sure to free the
/// right color", found by Coverity CID 1621077, GitLab MR !8137) in GTK **4.16.13** and
/// **4.18.0**; never backported to any earlier stable branch, so nothing short of raising
/// this project's floor past 4.16.13 makes the key safe.
///
/// So the project sets `underline-rgba` and **never** `overline-rgba`, anywhere — a
/// `clippy.toml` ban makes that a build failure rather than a thing to remember. The
/// overline is expressible, and takes the heading's ink; the theme vocabulary simply has
/// no key that could ask for the combination GTK cannot survive. `paragraph-background-rgba`
/// is unaffected (its guard/free pair is correctly matched at every call site audited) and
/// safe to combine with anything, including this pair, for a future heading band (TDD
/// 18.25). When the toolkit floor moves past 4.16.13, `heading_overline_rgba` can be added
/// and nothing else changes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct HeadingRule {
    /// The rule above the text. Always drawn in the heading's own ink — see the type
    /// docs; there is no colour key for this side and adding one is a heap bug.
    pub overline: LineStyle,
    pub underline: LineStyle,
    pub underline_rgba: Option<gdk::RGBA>,
}

impl HeadingRule {
    /// Whether this theme draws any heading rule at all — the one gate every consumer
    /// asks before emitting a rule, so "absent" is one decision rather than four.
    pub(crate) fn is_absent(&self) -> bool {
        self.overline.is_none() && self.underline.is_none()
    }
}

/// The band drawn behind a heading's text (TDD 18.25) — the plan's marquee decoration,
/// and the first entry in the vocabulary that is a genuinely NEW drawn thing rather than
/// a property of something already painted.
///
/// Absent by default on every level: `fills` is all-`None` until a theme states one, so a
/// theme that says nothing leaves the paint path byte-identical to before the decoration
/// existed. `is_absent` is the one gate every consumer asks, so "no band" is one decision
/// rather than five.
///
/// **The band spans the CONTENT COLUMN**, the same extent the code-block card uses — not
/// the text column a `paragraph_background_rgba` tag would pin it to, and not the widget
/// edge. Two reasons, and the second is the load-bearing one: a tag band follows the
/// *tag's* margins, so a heading inside a blockquote or a list item would carry a band of
/// a different width from its siblings; and the content column is the one extent all
/// three renderings can agree on (the HTML sink's `<h1>` fills its own column, the PDF's
/// printable width is its column), which is what keeps 25.3's "as the preview shows it"
/// true rather than nearly true.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct HeadingBand {
    /// Per level, h1 · h2 · h3 · h4 · h5-and-deeper. `None` ⇒ that level carries no band.
    pub fills: [Option<gdk::RGBA>; 5],
    /// A second stop, making the band a vertical gradient from the level's fill.
    pub gradient_to: Option<gdk::RGBA>,
}

impl HeadingBand {
    /// Whether any level carries a band at all — the single gate, so the paint path, the
    /// span scan and both export sinks ask one question rather than five.
    ///
    /// Keyed on the FILLS alone, deliberately: a sprite or a gradient describes what a
    /// band looks like and cannot conjure one, so a theme that states a sprite and no
    /// fill has stated the shape of a decoration it never asked for.
    pub(crate) fn is_absent(&self) -> bool {
        self.fills.iter().all(Option::is_none)
    }
}

/// Decoration metrics: design-time px at zoom 1.0. Every consumer scales these
/// through the existing `px(n) = round(n * zoom)` path; a theme never expresses
/// pixels at the current zoom.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Metrics {
    pub heading_space_below: [i32; 5],
    /// Space above each heading. Zero on every level until a theme says otherwise, so
    /// the heading tag's `pixels_above_lines` stays at the view default (TDD 18.2).
    pub heading_space_above: [i32; 5],
    /// Corner radius of the heading band. Only consulted where a band exists.
    pub heading_band_radius: i32,
    pub blockquote_bar_width: i32,
    pub blockquote_text_gap: i32,
    /// The ONE definition both the `li-{depth}` tag's `left_margin` and the drawn
    /// marker gutter's x read. A value that reached one but not the other would
    /// strand every list marker — GTK4Rs/AP-96.
    pub list_step: i32,
    pub list_item_gap: i32,
    pub rule_space: i32,
    pub table_cell_padding_v: i32,
    pub table_cell_padding_h: i32,
    pub table_border_width: i32,
    pub table_cell_radius: i32,
}

/// A theme with links 1 and 2 of the resolution order already applied. Colours
/// stay `Option` on purpose: `None` means "fall through to link 3", the desktop
/// GTK probe + derivation, which only `palette` (at the GTK edge) can perform.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub id: String,
    pub name: String,
    /// Optional picker symbol (emoji); `None` ⇒ just the name.
    pub symbol: Option<String>,
    pub background: Option<gdk::RGBA>,
    pub foreground: Option<gdk::RGBA>,
    pub accent: Option<gdk::RGBA>,
    /// The body font stack. A [`CssSafeFontStack`], so the type itself proves it was
    /// sanitised and generic-terminated — safe to interpolate into CSS. `None` ⇒
    /// fall through to the system font.
    pub font_family: Option<CssSafeFontStack>,
    pub syntect_theme: Option<String>,
    /// Heading foreground; `None` ⇒ inherit the body foreground.
    pub heading_color: Option<gdk::RGBA>,
    /// Heading font family (a [`CssSafeFontStack`], sanitised + generic-terminated by
    /// construction); `None` ⇒ headings use the body font.
    pub heading_font: Option<CssSafeFontStack>,
    /// Per-level heading colours (h1 · h2 · h3 · h4 · h5-and-deeper), already folded
    /// with the singular `heading_color`: a slot the theme left unset carries that
    /// fallback here, so every consumer indexes and no consumer re-implements the
    /// fold. `None` in a slot still means "inherit the body foreground".
    pub heading_colors: [Option<gdk::RGBA>; 5],
    /// Per-level heading font stacks, folded with `heading_font` the same way.
    pub heading_fonts: [Option<CssSafeFontStack>; 5],
    /// The rule drawn above and/or below a heading; absent unless a theme asks for it.
    pub heading_rule: HeadingRule,
    /// The band behind a heading's text; absent unless a theme states a fill for a level.
    pub heading_band: HeadingBand,
    pub link: Option<gdk::RGBA>,
    /// A link's underline style; `LineStyle::Single` unless a theme says otherwise,
    /// which is the line the app has always drawn.
    pub link_underline: LineStyle,
    /// The link underline's colour; `None` ⇒ it follows the link's own ink.
    pub link_underline_rgba: Option<gdk::RGBA>,
    /// The strike line's colour; `None` ⇒ it follows the struck text's own foreground.
    pub strikethrough_rgba: Option<gdk::RGBA>,
    pub code_inline_bg: Option<gdk::RGBA>,
    pub code_block_bg: Option<gdk::RGBA>,
    pub blockquote_bar: Option<gdk::RGBA>,
    pub selection_bg: Option<gdk::RGBA>,
    /// Selected-text ink; `None` ⇒ `palette` derives it from the page and its ink.
    pub selection_fg: Option<gdk::RGBA>,
    pub table_border: Option<gdk::RGBA>,
    pub table_head_bg: Option<gdk::RGBA>,
    pub rule: Option<gdk::RGBA>,
    /// List-marker glyph colour (bullet/numeral/checkbox); `None` ⇒ inherit the widget
    /// foreground. Marker glyph only — never the item text.
    pub list_marker: Option<gdk::RGBA>,
    /// The BULLET's colour by nesting-depth tier (TDD 18.26), already folded with
    /// `list_marker` — so slot 0 IS `list_marker` unless a theme says otherwise, and a
    /// consumer indexes rather than re-deriving the fallback. Bullet only: the ordered
    /// numeral and the task box read `list_marker` at every depth.
    pub list_bullet_colors: [Option<gdk::RGBA>; BULLET_TIERS],
    /// Glyphs standing in for the drawn list markers; each `None` ⇒ that marker is
    /// drawn as it always was. A sprite for the same marker outranks the glyph.
    pub list_glyphs: ListGlyphs,
    /// Ink for `==marked==` text, over `mark_bg`; `None` ⇒ the marked text keeps the
    /// body foreground, which is what every theme did before this key existed.
    pub mark_fg: Option<gdk::RGBA>,
    pub annotation_hl: ThemeColor,
    pub find_hl_all: ThemeColor,
    pub find_hl_current: ThemeColor,
    pub mark_bg: ThemeColor,
    pub typography: Typography,
    pub metrics: Metrics,
    /// Annotation chip fill; `None` ⇒ the hardcoded amber, exactly as before themes
    /// could touch it.
    pub annotation_chip_bg: Option<gdk::RGBA>,
    /// Annotation chip ink; `None` ⇒ the hardcoded white.
    pub annotation_chip_fg: Option<gdk::RGBA>,
    /// Every sprite this theme names, already validated to an absolute, existing,
    /// contained path (`crate::sprite::resolve` ran at load time — see `load()`).
    /// Empty for every shipped theme: the vocabulary is opt-in per theme, not a
    /// feature that appears unasked.
    pub sprites: Sprites,
}

/// Every sprite a theme may name, one field per decoration. A decoration's sprite lives
/// HERE rather than inside that decoration's own struct, so "what files does this theme
/// name?" is one question with one answer — which is also what `rewrite_sprite_paths`
/// iterates, and therefore what keeps a new sprite key from being validated nowhere. Each is `None` unless
/// the theme both set the key AND `crate::sprite::resolve` accepted it — a theme
/// that sets a broken reference gets the SAME "decoration absent" fallback as a
/// theme that sets nothing, never a partial or broken render.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Sprites {
    pub annotation_chip: Option<std::path::PathBuf>,
    /// The bullet's sprite by nesting-depth tier (TDD 18.26), already folded — a tier
    /// the theme left unset carries the next shallower tier's file.
    pub list_bullet: [Option<std::path::PathBuf>; BULLET_TIERS],
    pub list_ordered: Option<std::path::PathBuf>,
    pub list_task: Option<std::path::PathBuf>,
    pub list_task_checked: Option<std::path::PathBuf>,
    pub heading_band: Option<std::path::PathBuf>,
}

impl Theme {
    /// Apply resolution links 1 (selected) and 2 (`[themes.system]`), clamping and
    /// sanitising as it goes. Pure and total: every geometry/typography key lands
    /// on a value, and any colour still unresolved is left `None` for link 3.
    fn resolve(id: &str, selected: &ThemeSpec, system: &ThemeSpec) -> Theme {
        // Link 1, then link 2 — for each key independently.
        let pick = |a: &Option<String>, b: &Option<String>| a.clone().or_else(|| b.clone());
        let color =
            |a: &Option<String>, b: &Option<String>| pick(a, b).as_deref().and_then(parse_color);
        // An overlay colour must always resolve, so it walks all the way to the floor.
        let overlay = |a: &Option<String>, b: &Option<String>, floor: &str| {
            ThemeColor(
                color(a, b).unwrap_or_else(|| parse_color(floor).unwrap_or(gdk::RGBA::BLACK)),
            )
        };

        let heading_scale = selected
            .heading_scale
            .clone()
            .or_else(|| system.heading_scale.clone())
            .map(|v| fit5_f64(&v, F_HEADING_SCALE, SCALE_RANGE))
            .unwrap_or(F_HEADING_SCALE);
        let heading_space_below = selected
            .heading_space_below
            .clone()
            .or_else(|| system.heading_space_below.clone())
            .map(|v| fit5_i32(&v, F_HEADING_SPACE_BELOW, METRIC_RANGE))
            .unwrap_or(F_HEADING_SPACE_BELOW);
        let heading_space_above = selected
            .heading_space_above
            .clone()
            .or_else(|| system.heading_space_above.clone())
            .map(|v| fit5_i32(&v, F_HEADING_SPACE_ABOVE, METRIC_RANGE))
            .unwrap_or(F_HEADING_SPACE_ABOVE);

        // Per-level heading colour/face (18.21). The array comes from ONE link — the
        // selected theme's if it states one, else `[themes.system]`'s — exactly as
        // `heading_scale` does; then each slot folds down to the singular key. The fold
        // happens HERE, once, so `tags.rs`, the table header and the export sinks all
        // index a value that is already correct rather than each re-deriving the
        // fallback (POLICY "One theme key, every application path").
        //
        // An empty or unparseable slot IS the "unset" spelling: TOML arrays cannot hold
        // a null, and a colour that fails to parse must fall back rather than reject the
        // theme (TDD 18.11's clamp-don't-reject discipline, applied to a slot).
        let heading_color = color(&selected.heading_color, &system.heading_color);
        let heading_font = pick(&selected.heading_font, &system.heading_font)
            .as_deref()
            .and_then(sanitize_font_family);
        let level_slots = |sel: &Option<Vec<String>>, sys: &Option<Vec<String>>| {
            let authored = sel.clone().or_else(|| sys.clone()).unwrap_or_default();
            let mut out: [Option<String>; 5] = Default::default();
            for (slot, s) in out.iter_mut().zip(authored.iter()) {
                if !s.trim().is_empty() {
                    *slot = Some(s.clone());
                }
            }
            out
        };
        let heading_colors = level_slots(&selected.heading_colors, &system.heading_colors)
            .map(|s| s.as_deref().and_then(parse_color).or(heading_color));
        let heading_fonts = level_slots(&selected.heading_fonts, &system.heading_fonts).map(|s| {
            s.as_deref()
                .and_then(sanitize_font_family)
                .or_else(|| heading_font.clone())
        });

        // The heading rule (18.22). An unrecognised style falls back to the floor —
        // "no rule" — rather than failing the theme, and each colour left unset stays
        // `None` so the tag never sets `*-rgba` at all and the line follows the
        // heading's own ink.
        let style = |a: &Option<String>, b: &Option<String>, floor: LineStyle| {
            pick(a, b)
                .as_deref()
                .and_then(LineStyle::parse)
                .unwrap_or(floor)
        };
        let glyph = |a: &Option<String>, b: &Option<String>| {
            pick(a, b).as_deref().and_then(MarkerGlyph::parse)
        };
        // Already-validated absolute paths: `rewrite_sprite_paths` ran in `load()` before
        // this function ever sees the spec, so `Theme::resolve` itself stays pure (no
        // filesystem) — matching every other field here.
        let sprite =
            |a: &Option<String>, b: &Option<String>| pick(a, b).map(std::path::PathBuf::from);

        // The BULLET's three nesting-depth tiers (TDD 18.26), folded HERE so every
        // consumer indexes and none of them re-derives the fallback — the same discipline
        // the per-level heading fold above follows, and for the same reason: three
        // consumers each spelling `tier_2.or(tier_1)` is three chances to spell it
        // differently. Each tier falls back to the next SHALLOWER one, so an unstated
        // depth-3 takes depth 2's value and an unstated depth-2 takes depth 1's — which
        // is the un-suffixed key, i.e. exactly today's behaviour when neither is stated.
        let list_marker = color(&selected.list_marker, &system.list_marker);
        let tier2 = color(&selected.list_marker_2, &system.list_marker_2);
        let tier3 = color(&selected.list_marker_3, &system.list_marker_3);
        let list_bullet_colors = [
            list_marker,
            tier2.or(list_marker),
            tier3.or(tier2).or(list_marker),
        ];
        let g1 = glyph(&selected.list_bullet_glyph, &system.list_bullet_glyph);
        let g2 = glyph(&selected.list_bullet_glyph_2, &system.list_bullet_glyph_2);
        let g3 = glyph(&selected.list_bullet_glyph_3, &system.list_bullet_glyph_3);
        let bullet_glyphs = [
            g1.clone(),
            g2.clone().or_else(|| g1.clone()),
            g3.or(g2).or(g1),
        ];
        let s1 = sprite(&selected.sprite_list_bullet, &system.sprite_list_bullet);
        let s2 = sprite(&selected.sprite_list_bullet_2, &system.sprite_list_bullet_2);
        let s3 = sprite(&selected.sprite_list_bullet_3, &system.sprite_list_bullet_3);
        let bullet_sprites = [
            s1.clone(),
            s2.clone().or_else(|| s1.clone()),
            s3.or(s2).or(s1),
        ];

        let heading_band = HeadingBand {
            fills: level_slots(&selected.heading_band_bg, &system.heading_band_bg)
                .map(|slot| slot.as_deref().and_then(parse_color)),
            gradient_to: color(
                &selected.heading_band_gradient_to,
                &system.heading_band_gradient_to,
            ),
        };

        let heading_rule = HeadingRule {
            overline: style(
                &selected.heading_overline,
                &system.heading_overline,
                F_HEADING_OVERLINE,
            ),
            underline: style(
                &selected.heading_underline,
                &system.heading_underline,
                F_HEADING_UNDERLINE,
            ),
            underline_rgba: color(
                &selected.heading_underline_rgba,
                &system.heading_underline_rgba,
            ),
        };

        macro_rules! num {
            ($f:ident, $floor:expr, $range:expr, $clamp:ident) => {
                $clamp(selected.$f.or(system.$f).unwrap_or($floor), $range)
            };
        }

        Theme {
            id: id.to_string(),
            symbol: pick(&selected.symbol, &system.symbol),
            name: selected
                .name
                .clone()
                .or_else(|| {
                    if id == SYSTEM_ID {
                        system.name.clone()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| id.to_string()),
            background: color(&selected.background, &system.background),
            foreground: color(&selected.foreground, &system.foreground),
            accent: color(&selected.accent, &system.accent),
            font_family: pick(&selected.font_family, &system.font_family)
                .as_deref()
                .and_then(sanitize_font_family),
            syntect_theme: pick(&selected.syntect_theme, &system.syntect_theme),
            heading_color,
            heading_font,
            heading_colors,
            heading_fonts,
            heading_rule,
            heading_band,
            link: color(&selected.link, &system.link),
            link_underline: style(
                &selected.link_underline,
                &system.link_underline,
                F_LINK_UNDERLINE,
            ),
            link_underline_rgba: color(&selected.link_underline_rgba, &system.link_underline_rgba),
            strikethrough_rgba: color(&selected.strikethrough_rgba, &system.strikethrough_rgba),
            code_inline_bg: color(&selected.code_inline_bg, &system.code_inline_bg),
            code_block_bg: color(&selected.code_block_bg, &system.code_block_bg),
            blockquote_bar: color(&selected.blockquote_bar, &system.blockquote_bar),
            selection_bg: color(&selected.selection_bg, &system.selection_bg),
            selection_fg: color(&selected.selection_fg, &system.selection_fg),
            table_border: color(&selected.table_border, &system.table_border),
            table_head_bg: color(&selected.table_head_bg, &system.table_head_bg),
            rule: color(&selected.rule, &system.rule),
            list_marker,
            list_bullet_colors,
            list_glyphs: ListGlyphs {
                bullet: bullet_glyphs,
                ordered: glyph(&selected.list_ordered_glyph, &system.list_ordered_glyph),
                task: glyph(&selected.list_task_glyph, &system.list_task_glyph),
                task_checked: glyph(
                    &selected.list_task_checked_glyph,
                    &system.list_task_checked_glyph,
                ),
            },
            mark_fg: color(&selected.mark_fg, &system.mark_fg),
            annotation_hl: overlay(&selected.annotation_hl, &system.annotation_hl, "#FFD133_61"),
            find_hl_all: overlay(&selected.find_hl_all, &system.find_hl_all, "#f6d32d"),
            find_hl_current: overlay(
                &selected.find_hl_current,
                &system.find_hl_current,
                "#ff7800",
            ),
            // Neutral highlighter yellow as the last-resort floor; each bundled
            // theme overrides it with a page-appropriate wash (data/themes.toml).
            mark_bg: overlay(&selected.mark_bg, &system.mark_bg, "#fff59d_88"),
            typography: Typography {
                heading_scale,
                heading_weight: num!(heading_weight, F_HEADING_WEIGHT, WEIGHT_RANGE, clamp_i32),
                bold_weight: num!(bold_weight, F_BOLD_WEIGHT, WEIGHT_RANGE, clamp_i32),
                supsub_scale: num!(supsub_scale, F_SUPSUB_SCALE, SCALE_RANGE, clamp_f64),
                superscript_rise: num!(superscript_rise, F_SUPERSCRIPT_RISE, RISE_RANGE, clamp_i32),
                subscript_rise: num!(subscript_rise, F_SUBSCRIPT_RISE, RISE_RANGE, clamp_i32),
            },
            metrics: Metrics {
                heading_space_below,
                heading_space_above,
                heading_band_radius: num!(
                    heading_band_radius,
                    F_HEADING_BAND_RADIUS,
                    METRIC_RANGE,
                    clamp_i32
                ),
                blockquote_bar_width: num!(
                    blockquote_bar_width,
                    F_BQ_BAR_WIDTH,
                    METRIC_RANGE,
                    clamp_i32
                ),
                blockquote_text_gap: num!(
                    blockquote_text_gap,
                    F_BQ_TEXT_GAP,
                    METRIC_RANGE,
                    clamp_i32
                ),
                list_step: num!(list_step, F_LIST_STEP, LIST_STEP_RANGE, clamp_i32),
                list_item_gap: num!(list_item_gap, F_LIST_ITEM_GAP, METRIC_RANGE, clamp_i32),
                rule_space: num!(rule_space, F_RULE_SPACE, METRIC_RANGE, clamp_i32),
                table_cell_padding_v: num!(
                    table_cell_padding_v,
                    F_TABLE_CELL_PADDING_V,
                    METRIC_RANGE,
                    clamp_i32
                ),
                table_cell_padding_h: num!(
                    table_cell_padding_h,
                    F_TABLE_CELL_PADDING_H,
                    METRIC_RANGE,
                    clamp_i32
                ),
                table_border_width: num!(
                    table_border_width,
                    F_TABLE_BORDER_WIDTH,
                    METRIC_RANGE,
                    clamp_i32
                ),
                table_cell_radius: num!(
                    table_cell_radius,
                    F_TABLE_CELL_RADIUS,
                    METRIC_RANGE,
                    clamp_i32
                ),
            },
            annotation_chip_bg: color(&selected.annotation_chip_bg, &system.annotation_chip_bg),
            annotation_chip_fg: color(&selected.annotation_chip_fg, &system.annotation_chip_fg),
            // Already-validated absolute paths: `rewrite_sprite_paths` ran in `load()`
            // before this function ever sees the spec, so `Theme::resolve` itself stays
            // pure (no filesystem) — matching every other field here.
            sprites: Sprites {
                annotation_chip: sprite(
                    &selected.sprite_annotation_chip,
                    &system.sprite_annotation_chip,
                ),
                list_bullet: bullet_sprites,
                list_ordered: sprite(&selected.sprite_list_ordered, &system.sprite_list_ordered),
                list_task: sprite(&selected.sprite_list_task, &system.sprite_list_task),
                list_task_checked: sprite(
                    &selected.sprite_list_task_checked,
                    &system.sprite_list_task_checked,
                ),
                heading_band: sprite(&selected.sprite_heading_band, &system.sprite_heading_band),
            },
        }
    }
}

/// Coerce an authored array to exactly 5 entries: short arrays extend from the
/// floor, long ones truncate. A theme is data from disk — the length is no more
/// trustworthy than the values, and a resolution that panicked on a 3-entry array
/// would let a theme file kill the app.
fn fit5_f64(v: &[f64], floor: [f64; 5], range: (f64, f64)) -> [f64; 5] {
    let mut out = floor;
    // `zip` gives both bounds for free: a short array leaves the floor's remaining
    // entries in place, a long one stops at 5.
    for (slot, &x) in out.iter_mut().zip(v.iter()) {
        *slot = clamp_f64(x, range);
    }
    out
}
fn fit5_i32(v: &[i32], floor: [i32; 5], range: (i32, i32)) -> [i32; 5] {
    let mut out = floor;
    for (slot, &x) in out.iter_mut().zip(v.iter()) {
        *slot = clamp_i32(x, range);
    }
    out
}

// ── loading (the only impure part) ────────────────────────────────────────────

/// Load every installed theme: the compiled-in data, with the first themes file
/// found on the search path merged over it.
///
/// ⚠️ **`XDG_CONFIG_HOME` is snapshotted, never read through GLib.** This process
/// redirects `XDG_CONFIG_HOME` to a temp dir at startup to prevent the GTK 4.6
/// compose-table crash (`workaround.rs`), and `g_get_user_config_dir()` caches its
/// answer in a global static FOREVER on first call — a global GTK 4.6's compose
/// table reads from too. So calling `glib::user_config_dir()` here would either
/// resolve into the temp dir (silently losing the user's override) or, if forced
/// to resolve early, re-arm the crash. There is no ordering that gives both.
/// `config::config()` hand-rolls the same snapshot for the same reason — that is
/// FORCED BY THE WORKAROUND, not needless XDG re-implementation. Do not "clean it
/// up" into `glib::user_config_dir()`.
///
/// `XDG_DATA_HOME`/`XDG_DATA_DIRS` are untouched by the redirect, so the GLib
/// helpers are correct and order-independent for those.
fn load() -> Themes {
    let mut themes = Themes::builtin();
    if let Some((text, dir)) = find_themes_file() {
        if let Some(mut user) = Themes::parse(&text) {
            // The one place sprite paths touch the filesystem: rewrite each spec's
            // theme-relative sprite reference to a validated absolute path (or drop
            // it) BEFORE `Theme::resolve` ever runs, so `resolve` itself stays pure —
            // matching every other field it produces.
            for spec in user.values_mut() {
                rewrite_sprite_paths(spec, &dir);
            }
            themes.merge_over(user);
        }
    }
    themes
}

/// Rewrite one spec's sprite keys from theme-relative to validated absolute,
/// dropping (to `None`) any `crate::sprite::resolve` refuses.
fn rewrite_sprite_paths(spec: &mut ThemeSpec, dir: &std::path::Path) {
    // Every sprite key goes through this ONE loop rather than a line each: a new sprite
    // key that is added to the spec and forgotten here compiles, works for a built-in
    // theme (which states no sprite), and silently ignores every user reference — the
    // `take!` failure mode one layer over.
    for slot in [
        &mut spec.sprite_annotation_chip,
        &mut spec.sprite_list_bullet,
        &mut spec.sprite_list_bullet_2,
        &mut spec.sprite_list_bullet_3,
        &mut spec.sprite_list_ordered,
        &mut spec.sprite_list_task,
        &mut spec.sprite_list_task_checked,
        &mut spec.sprite_heading_band,
    ] {
        *slot = slot.as_deref().and_then(|rel| {
            crate::sprite::resolve(dir, rel).map(|p| p.to_string_lossy().into_owned())
        });
    }
}

/// Returns the file's text and the directory it was read from — sprite references
/// resolve against that directory, never against the current working directory.
fn find_themes_file() -> Option<(String, std::path::PathBuf)> {
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    // 1. user override — from the config path snapshotted before the redirect.
    if let Some(dir) = crate::config::user_config_dir() {
        paths.push(dir.join("scribobulate").join("themes.toml"));
    }
    // 2. per-user install, then 3. system install. Iterate `system_data_dirs()`;
    // never hard-code `/usr/share` — on a KDE box its first entry is
    // `/usr/share/plasma`, and a hard-coded path would work on GNOME and fail here.
    paths.push(
        glib::user_data_dir()
            .join("scribobulate")
            .join("themes.toml"),
    );
    for dir in glib::system_data_dirs() {
        paths.push(dir.join("scribobulate").join("themes.toml"));
    }
    for p in paths {
        if let Ok(text) = std::fs::read_to_string(&p) {
            log::debug!("theme: loaded themes from {}", p.display());
            let dir = p
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            return Some((text, dir));
        }
    }
    None
}

// ── the active theme ──────────────────────────────────────────────────────────
//
// App-wide, not per-window (sdd/THEMING.md): the theme's CSS properties
// (`font-family`, `color`, `background-color`) are DISJOINT from zoom's
// (`font-size`), so the theme can be one app-wide provider with unscoped rules and
// zoom's per-window provider is untouched. A per-window theme would force both
// into per-window generated CSS scoped by `.scrib-win-<id>` for no benefit.
//
// GTK4 is single-threaded, so a thread-local holds this without a lock; unit tests
// each get their own, which keeps them isolated.

thread_local! {
    static THEMES: std::cell::OnceCell<Themes> = const { std::cell::OnceCell::new() };
    static ACTIVE: std::cell::RefCell<Option<std::rc::Rc<Theme>>> =
        const { std::cell::RefCell::new(None) };
}

/// Every installed theme, loaded once.
pub(crate) fn themes() -> Themes {
    THEMES.with(|t| t.get_or_init(load).clone())
}

/// The active resolved theme. Defaults to the system theme, so every consumer can
/// read a theme unconditionally without an "is theming on yet?" branch.
pub(crate) fn active() -> std::rc::Rc<Theme> {
    ACTIVE.with(|a| {
        let mut slot = a.borrow_mut();
        if slot.is_none() {
            *slot = Some(std::rc::Rc::new(themes().resolve(SYSTEM_ID)));
        }
        slot.clone().expect("just initialised")
    })
}

/// Select `id` as the active theme, re-resolving it. Returns the new theme.
/// An unknown id resolves as the system theme (see [`Themes::resolve`]).
pub(crate) fn set_active(id: &str) -> std::rc::Rc<Theme> {
    let resolved = std::rc::Rc::new(themes().resolve(id));
    // Decoded sprites are cached by PATH, not by theme id, so a swap away from a
    // sprite-using theme would otherwise keep its textures resident for the rest of
    // the process — and a path a new theme no longer names could, in principle, be
    // served stale to a caller asking about the new one.
    crate::sprite::clear_cache();
    ACTIVE.with(|a| *a.borrow_mut() = Some(resolved.clone()));
    resolved
}

/// Make an already-resolved [`Theme`] active. Test-only seam: it lets a test
/// activate a theme built from an inline fragment (via
/// [`Themes::merge_over_for_test`]) without installing a file on the search path.
/// Production code always goes through [`set_active`], so the active theme can only
/// ever be one the registry actually holds.
///
/// Gated on the integration feature because only the realized-view tests need it —
/// the pure tests resolve a `Theme` and assert on it directly, without ever making
/// it the app-wide active one.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn set_active_for_test(theme: Theme) {
    ACTIVE.with(|a| *a.borrow_mut() = Some(std::rc::Rc::new(theme)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_attr_carries_the_themed_weight() {
        let typo = Typography {
            heading_scale: [1.0; 5],
            heading_weight: 700,
            bold_weight: 650,
            supsub_scale: 0.75,
            superscript_rise: 4,
            subscript_rise: -2,
        };
        assert_eq!(typo.bold_attr(), " weight=\"650\"");
    }

    #[test]
    fn supsub_attr_selects_the_matching_rise_and_shares_the_scale() {
        let typo = Typography {
            heading_scale: [1.0; 5],
            heading_weight: 700,
            bold_weight: 600,
            supsub_scale: 0.75,
            superscript_rise: 4,
            subscript_rise: -2,
        };
        // 0.75 -> "75%"; rise is in Pango units, the SAME `value * pango::SCALE`
        // `tags.rs` feeds `set_rise` — one raw theme number, two representations.
        assert_eq!(
            typo.supsub_attr(true),
            format!(" size=\"75%\" rise=\"{}\"", 4 * gtk::pango::SCALE)
        );
        // subscript_rise's own floor is already negative — no sign flip here.
        assert_eq!(
            typo.supsub_attr(false),
            format!(" size=\"75%\" rise=\"{}\"", -2 * gtk::pango::SCALE)
        );
    }

    fn builtin_system() -> ThemeSpec {
        Themes::parse(BUILTIN_THEMES_TOML)
            .expect("shipped themes.toml must parse")
            .get(SYSTEM_ID)
            .cloned()
            .expect("shipped themes.toml must define [themes.system]")
    }

    #[test]
    fn builtin_parses_and_ships_the_two_themes() {
        let t = Themes::builtin();
        assert!(t.contains("system"));
        assert!(t.contains("sepia"));
    }

    /// The floor consts exist only to keep resolution TOTAL; the data file is the
    /// source of truth a human reads. This asserts they say the same thing, so the
    /// floor can never quietly become a second, divergent set of defaults.
    #[test]
    fn builtin_system_spec_matches_the_floor() {
        let sys = builtin_system();
        let r = Theme::resolve(SYSTEM_ID, &ThemeSpec::default(), &sys);
        assert_eq!(r.typography.heading_scale, F_HEADING_SCALE);
        assert_eq!(r.typography.heading_weight, F_HEADING_WEIGHT);
        assert_eq!(r.typography.bold_weight, F_BOLD_WEIGHT);
        assert_eq!(r.typography.supsub_scale, F_SUPSUB_SCALE);
        assert_eq!(r.typography.superscript_rise, F_SUPERSCRIPT_RISE);
        assert_eq!(r.typography.subscript_rise, F_SUBSCRIPT_RISE);
        assert_eq!(r.metrics.heading_space_below, F_HEADING_SPACE_BELOW);
        assert_eq!(r.metrics.heading_space_above, F_HEADING_SPACE_ABOVE);
        assert_eq!(r.heading_rule.overline, F_HEADING_OVERLINE);
        assert_eq!(r.heading_rule.underline, F_HEADING_UNDERLINE);
        assert_eq!(r.metrics.blockquote_bar_width, F_BQ_BAR_WIDTH);
        assert_eq!(r.metrics.blockquote_text_gap, F_BQ_TEXT_GAP);
        assert_eq!(r.metrics.list_step, F_LIST_STEP);
        assert_eq!(r.metrics.list_item_gap, F_LIST_ITEM_GAP);
        assert_eq!(r.metrics.rule_space, F_RULE_SPACE);
        assert_eq!(r.metrics.table_cell_padding_v, F_TABLE_CELL_PADDING_V);
        assert_eq!(r.metrics.table_cell_padding_h, F_TABLE_CELL_PADDING_H);
        assert_eq!(r.metrics.table_border_width, F_TABLE_BORDER_WIDTH);
        assert_eq!(r.metrics.table_cell_radius, F_TABLE_CELL_RADIUS);
    }

    /// TDD 18.2 — the regression bar. System must inject NO base colour, so every
    /// one of them falls through to the desktop probe exactly as before theming.
    #[test]
    fn system_theme_injects_no_base_colour_and_no_font() {
        let t = Themes::builtin().resolve(SYSTEM_ID);
        assert_eq!(t.id, SYSTEM_ID);
        assert!(t.background.is_none());
        assert!(t.foreground.is_none());
        assert!(t.accent.is_none());
        assert!(t.font_family.is_none());
        assert!(t.syntect_theme.is_none());
        assert!(t.heading_color.is_none());
        assert!(t.list_marker.is_none());
        assert!(t.link.is_none());
        assert!(t.code_inline_bg.is_none());
        assert!(t.blockquote_bar.is_none());
    }

    /// A theme can colour its headings; omitted, `heading_color` stays `None` so
    /// headings inherit the body foreground (the default).
    #[test]
    fn heading_color_is_opt_in() {
        assert!(Themes::builtin().resolve(SYSTEM_ID).heading_color.is_none());
        assert!(Themes::builtin().resolve("sepia").heading_color.is_none());
        let sw = Themes::builtin().resolve("synthwave");
        assert_eq!(
            crate::palette::to_hex(sw.heading_color.expect("synthwave sets it")),
            "#ffc21e"
        );
    }

    /// TDD 18.15 — a theme can colour the list-marker glyph (bullet/numeral/checkbox)
    /// independently of the item text; omitted, `list_marker` stays `None` so markers
    /// inherit the widget foreground (System byte-identical). One key, all three kinds.
    #[test]
    fn list_marker_is_opt_in() {
        assert!(Themes::builtin().resolve(SYSTEM_ID).list_marker.is_none());
        assert!(Themes::builtin().resolve("sepia").list_marker.is_none());
        let term = Themes::builtin().resolve("terminal");
        assert_eq!(
            crate::palette::to_hex(term.list_marker.expect("terminal sets it")),
            "#55ff55"
        );
        let sw = Themes::builtin().resolve("synthwave");
        assert_eq!(
            crate::palette::to_hex(sw.list_marker.expect("synthwave sets it")),
            "#ff3caf"
        );
    }

    /// A theme can give its headings a distinct FONT; omitted, `heading_font` is `None`
    /// so headings use the body font. When set it is sanitised + generic-terminated.
    #[test]
    fn heading_font_is_opt_in_and_sanitised() {
        assert!(Themes::builtin().resolve(SYSTEM_ID).heading_font.is_none());
        assert!(Themes::builtin().resolve("sepia").heading_font.is_none());
        let hf = Themes::builtin()
            .resolve("synthwave")
            .heading_font
            .expect("synthwave sets it");
        assert!(hf.contains("Orbitron") && hf.ends_with("sans-serif"));
    }

    /// TDD 18.3 — Sepia is book-like: warm page, serif face, soft-brown text.
    #[test]
    fn sepia_supplies_a_warm_page_and_a_serif_stack() {
        let t = Themes::builtin().resolve("sepia");
        let bg = t.background.expect("sepia sets a page background");
        let fg = t.foreground.expect("sepia sets a body foreground");
        // Off-white and yellowish: bright, and warmer than it is blue.
        assert!(crate::palette::luminance(bg) > 0.6);
        assert!(bg.red() > bg.blue());
        // Soft brown body text: dark, and warmer than it is blue.
        assert!(crate::palette::luminance(fg) < 0.2);
        assert!(fg.red() > fg.blue());
        assert!(t.font_family.as_deref().unwrap().ends_with("serif"));
        // Derived keys stay unset so they follow background/foreground/accent.
        assert!(t.link.is_none());
        assert!(t.code_inline_bg.is_none());
        assert!(t.blockquote_bar.is_none());
    }

    /// TDD 18.21 — per-level heading colour/face. Three claims in one place, because
    /// they are one contract: a stated slot wins, an EMPTY or absent slot falls back to
    /// the theme's singular key, and the array merges from a user file.
    ///
    /// The merge half is not decoration. A new key has to reach `overlay`'s `take!`
    /// list, and omitting it compiles, leaves every built-in theme working, and silently
    /// drops EVERY user override — the shipped `list_marker` bug, pinned below.
    #[test]
    fn per_level_heading_colour_and_face_fall_back_and_merge() {
        let themes = Themes::builtin();

        // System states neither, so every level is `None` — the tag sets no foreground
        // and headings inherit the page's `color`, exactly as before 18.21 (TDD 18.2).
        let sys = themes.resolve(SYSTEM_ID);
        assert!(sys.heading_colors.iter().all(Option::is_none));
        assert!(sys.heading_fonts.iter().all(Option::is_none));

        // A synthetic theme states h1 only; h2..h5 fall back to its singular keys.
        // Synthetic rather than a built-in theme's own content on purpose — content
        // (which theme demonstrates which key) is free to change, this contract is not.
        let mut synth = Themes::builtin();
        synth.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_color = \"#334455\"\nheading_font = \"Georgia, serif\"\n\
                 heading_colors = [\"#ff3caf\"]\n\
                 heading_fonts = [\"Michroma, sans-serif\"]\n",
            )
            .unwrap(),
        );
        let t = synth.resolve("sepia");
        assert_eq!(
            crate::palette::to_hex(t.heading_colors[0].expect("h1 is stated")),
            "#ff3caf"
        );
        let base = crate::palette::to_hex(t.heading_color.expect("theme sets one"));
        for level in 1..5 {
            assert_eq!(
                crate::palette::to_hex(t.heading_colors[level].expect("falls back")),
                base,
                "h{} did not fall back to heading_color",
                level + 1
            );
        }
        assert!(t.heading_fonts[0]
            .as_ref()
            .expect("h1 face is stated")
            .as_str()
            .starts_with("\"Michroma\""));
        for level in 1..5 {
            assert_eq!(
                t.heading_fonts[level].as_ref().map(|f| f.as_str()),
                t.heading_font.as_ref().map(|f| f.as_str()),
                "h{} did not fall back to heading_font",
                level + 1
            );
        }

        // A theme that states NEITHER the array nor the singular leaves the level unset.
        assert!(themes
            .resolve("sepia")
            .heading_colors
            .iter()
            .all(Option::is_none));

        // The `take!`-list guard: a user override of a theme that ships no array.
        let mut user = Themes::builtin();
        user.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_colors = [\"\", \"#123456\"]\n\
                 heading_fonts = [\"\", \"Georgia, serif\"]\n",
            )
            .unwrap(),
        );
        let sep = user.resolve("sepia");
        assert_eq!(
            crate::palette::to_hex(sep.heading_colors[1].expect("h2 override merged")),
            "#123456"
        );
        assert_eq!(
            sep.heading_fonts[1].as_ref().map(|f| f.as_str()),
            Some("\"Georgia\", serif")
        );
        // An empty slot and a slot past the array's end both stay unset (sepia states
        // no singular heading colour either).
        assert!(sep.heading_colors[0].is_none());
        assert!(sep.heading_colors[4].is_none());
    }

    /// A slot a theme fills with nonsense must FALL BACK, never reject the theme —
    /// the same clamp-don't-reject discipline every geometry key follows (TDD 18.11).
    #[test]
    fn an_unparseable_heading_level_slot_falls_back_to_the_singular_key() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.synthwave]\nheading_colors = [\"not a colour\"]\n\
                 heading_fonts = [\"}} * {{ color: red; }}\"]\n",
            )
            .unwrap(),
        );
        let sw = themes.resolve("synthwave");
        assert_eq!(
            crate::palette::to_hex(sw.heading_colors[0].expect("fell back")),
            crate::palette::to_hex(sw.heading_color.unwrap())
        );
        assert_eq!(
            sw.heading_fonts[0].as_ref().map(|f| f.as_str()),
            sw.heading_font.as_ref().map(|f| f.as_str())
        );
    }

    /// TDD 18.8 — the legibility floor. Every theme that states its own page must
    /// clear WCAG AA for body text; this is what stops a later "warm it up a bit"
    /// tweak from quietly degrading readability.
    #[test]
    fn every_theme_body_contrast_clears_the_legibility_floor() {
        let themes = Themes::builtin();
        for (id, _name, _sym) in themes.chooser_list() {
            let t = themes.resolve(&id);
            let (Some(bg), Some(fg)) = (t.background, t.foreground) else {
                continue; // derives from the desktop; the desktop owns its own contrast
            };
            let c = crate::palette::contrast(fg, bg);
            assert!(
                c >= 4.5,
                "theme {id}: body contrast {c:.2} is below WCAG AA"
            );
        }
    }

    /// TDD 18.22 / 18.2 — the heading rule is INERT until a theme asks for it, and the
    /// space above it is zero, so System registers exactly the heading tag it always did.
    #[test]
    fn the_heading_rule_and_space_above_are_absent_under_system() {
        let sys = Themes::builtin().resolve(SYSTEM_ID);
        assert!(sys.heading_rule.is_absent());
        assert!(sys.heading_rule.underline_rgba.is_none());
        assert_eq!(sys.metrics.heading_space_above, [0; 5]);
    }

    /// TDD 18.22 — both sides resolve independently, each with its own colour, and both
    /// merge from a user file (the `take!`-list guard again — four new keys, four ways
    /// to silently drop every user override).
    #[test]
    fn a_theme_states_each_heading_rule_side_independently_and_merges() {
        // Synthetic rather than a built-in theme's own content on purpose — content is
        // free to change, this contract is not.
        let mut synth = Themes::builtin();
        synth.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_underline = \"single\"\n\
                 heading_underline_rgba = \"#3e6fa0\"\n\
                 heading_space_above = [16, 12, 8, 6, 6]\n",
            )
            .unwrap(),
        );
        let t = synth.resolve("sepia");
        assert_eq!(t.heading_rule.underline, LineStyle::Single);
        assert_eq!(
            crate::palette::to_hex(t.heading_rule.underline_rgba.expect("stated")),
            "#3e6fa0"
        );
        // This theme states no overline, so that side stays off.
        assert_eq!(t.heading_rule.overline, LineStyle::None);
        assert_eq!(t.metrics.heading_space_above, [16, 12, 8, 6, 6]);

        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_overline = \"double\"\n\
                 heading_underline = \"wavy\"\n\
                 heading_underline_rgba = \"#222222\"\nheading_space_above = [7]\n",
            )
            .unwrap(),
        );
        let sep = themes.resolve("sepia");
        // The overline CLAMPS: Pango's attribute has only none/single, so a theme asking
        // for a double rule above gets a single one rather than a rejected theme.
        assert_eq!(sep.heading_rule.overline, LineStyle::Double);
        assert_eq!(
            sep.heading_rule.overline.overline(),
            gtk::pango::Overline::Single
        );
        assert_eq!(sep.heading_rule.underline, LineStyle::Wavy);
        assert_eq!(
            sep.heading_rule.underline.underline(),
            gtk::pango::Underline::Error
        );
        assert_eq!(
            crate::palette::to_hex(sep.heading_rule.underline_rgba.expect("merged")),
            "#222222"
        );
        // A short array extends from the floor rather than panicking (TDD 18.11).
        assert_eq!(sep.metrics.heading_space_above, [7, 0, 0, 0, 0]);
    }

    /// TDD 18.11 — an unknown line style falls back to the key's floor. A theme file is
    /// data from disk: a typo must cost the decoration, never the theme.
    #[test]
    fn an_unknown_line_style_falls_back_to_the_floor() {
        assert_eq!(LineStyle::parse("wavy"), Some(LineStyle::Wavy));
        assert_eq!(LineStyle::parse("  SINGLE "), Some(LineStyle::Single));
        assert_eq!(LineStyle::parse("squiggle"), None);
        let mut themes = Themes::builtin();
        themes
            .merge_over(Themes::parse("[themes.sepia]\nheading_underline = \"zigzag\"\n").unwrap());
        assert_eq!(
            themes.resolve("sepia").heading_rule.underline,
            F_HEADING_UNDERLINE
        );
    }

    /// TDD 18.23 / 18.2 — the strike colour and the link-underline colour are absent
    /// under System, and the link underline floors at the SINGLE line the app has always
    /// drawn (not at "none", unlike the heading rule — that difference is the whole of
    /// what keeps System's links looking as they did).
    #[test]
    fn strike_and_link_underline_default_to_todays_rendering() {
        let sys = Themes::builtin().resolve(SYSTEM_ID);
        assert!(sys.strikethrough_rgba.is_none());
        assert!(sys.link_underline_rgba.is_none());
        assert_eq!(sys.link_underline, LineStyle::Single);
        assert_eq!(
            sys.link_underline.underline(),
            gtk::pango::Underline::Single
        );
        // Sepia states none of them either, so it inherits the same.
        let sep = Themes::builtin().resolve("sepia");
        assert!(sep.strikethrough_rgba.is_none());
        assert_eq!(sep.link_underline, LineStyle::Single);
    }

    /// TDD 18.23 — both resolve, and both merge from a user file (`take!`-list guard).
    #[test]
    fn a_theme_states_the_strike_and_link_underline_colours_independently() {
        let sw = Themes::builtin().resolve("synthwave");
        assert_eq!(
            crate::palette::to_hex(sw.strikethrough_rgba.expect("stated")),
            "#ff3caf"
        );
        // Stated independently of the link's own ink — that separation IS the key.
        assert_eq!(
            crate::palette::to_hex(sw.link_underline_rgba.expect("stated")),
            "#ff3caf"
        );
        assert_ne!(
            crate::palette::to_hex(sw.link.expect("synthwave sets a link colour")),
            crate::palette::to_hex(sw.link_underline_rgba.unwrap())
        );

        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.sepia]\nstrikethrough_rgba = \"#654321\"\n\
                 link_underline = \"wavy\"\nlink_underline_rgba = \"#abcdef\"\n",
            )
            .unwrap(),
        );
        let sep = themes.resolve("sepia");
        assert_eq!(
            crate::palette::to_hex(sep.strikethrough_rgba.expect("merged")),
            "#654321"
        );
        assert_eq!(sep.link_underline, LineStyle::Wavy);
        assert_eq!(
            crate::palette::to_hex(sep.link_underline_rgba.expect("merged")),
            "#abcdef"
        );
        // A link with NO line at all is expressible, and is not the floor.
        let mut off = Themes::builtin();
        off.merge_over(Themes::parse("[themes.sepia]\nlink_underline = \"none\"\n").unwrap());
        assert_eq!(off.resolve("sepia").link_underline, LineStyle::None);
    }

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

    /// TDD 18.24 / 18.2 — every marker key is absent under System, and resolves and
    /// merges from a user file (the `take!`-list guard, eight keys' worth).
    #[test]
    fn list_marker_glyphs_and_sprites_are_opt_in_and_merge() {
        let sys = Themes::builtin().resolve(SYSTEM_ID);
        assert_eq!(sys.list_glyphs, ListGlyphs::default());
        assert!(sys.sprites.list_bullet.iter().all(Option::is_none));
        assert!(sys.sprites.list_ordered.is_none());
        assert!(sys.sprites.list_task.is_none());
        assert!(sys.sprites.list_task_checked.is_none());

        // Terminal states all four glyphs — including both task states, so a ticked
        // glyph never sits beside a drawn box.
        let term = Themes::builtin().resolve("terminal");
        assert_eq!(term.list_glyphs.bullet[0].as_ref().unwrap().as_plain(), "▸");
        assert_eq!(term.list_glyphs.ordered.as_ref().unwrap().as_plain(), "$");
        assert_eq!(term.list_glyphs.task.as_ref().unwrap().as_plain(), "[ ]");
        assert_eq!(
            term.list_glyphs.task_checked.as_ref().unwrap().as_plain(),
            "[x]"
        );

        let mut themes = Themes::builtin();
        themes.merge_over(Themes::parse("[themes.sepia]\nlist_bullet_glyph = \"❧\"\n").unwrap());
        assert_eq!(
            themes.resolve("sepia").list_glyphs.bullet[0]
                .as_ref()
                .unwrap()
                .as_plain(),
            "❧"
        );
    }

    /// TDD 18.25 / 18.2 — the heading band is absent on every level under System, and
    /// `is_absent` keys on the FILLS: a theme that describes a band's shape without
    /// stating a fill has described a decoration it never asked for.
    #[test]
    fn the_heading_band_is_absent_until_a_theme_states_a_fill() {
        let sys = Themes::builtin().resolve(SYSTEM_ID);
        assert!(sys.heading_band.is_absent());
        assert_eq!(sys.metrics.heading_band_radius, F_HEADING_BAND_RADIUS);
        assert!(sys.sprites.heading_band.is_none());

        let mut shape_only = Themes::builtin();
        shape_only.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_band_radius = 12\n\
                 heading_band_gradient_to = \"#ffffff\"\n",
            )
            .unwrap(),
        );
        assert!(shape_only.resolve("sepia").heading_band.is_absent());
    }

    /// TDD 18.25 — per-level fills, a gradient stop and a radius all resolve and merge
    /// (the `take!`-list guard once more), and an unstated level carries no band.
    #[test]
    fn a_theme_bands_the_levels_it_names_and_no_others() {
        // Synthetic rather than a built-in theme's own content on purpose — content is
        // free to change, this contract is not.
        let mut synth = Themes::builtin();
        synth.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_band_bg = [\"#6c2a92\", \"#9e1449\"]\n\
                 heading_band_gradient_to = \"#101a4d\"\nheading_band_radius = 8\n",
            )
            .unwrap(),
        );
        let t = synth.resolve("sepia");
        assert!(!t.heading_band.is_absent());
        assert_eq!(
            crate::palette::to_hex(t.heading_band.fills[0].expect("h1 is banded")),
            "#6c2a92"
        );
        assert_eq!(
            crate::palette::to_hex(t.heading_band.fills[1].expect("h2 is banded")),
            "#9e1449"
        );
        // h3..h5 are left empty on purpose — banding every level is a stack of stripes.
        assert!(t.heading_band.fills[2].is_none());
        assert!(t.heading_band.fills[4].is_none());
        assert!(t.heading_band.gradient_to.is_some());
        assert_eq!(t.metrics.heading_band_radius, 8);

        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.sepia]\nheading_band_bg = [\"\", \"#abcdef\"]\n\
                 heading_band_radius = 999\n",
            )
            .unwrap(),
        );
        let sep = themes.resolve("sepia");
        assert!(sep.heading_band.fills[0].is_none());
        assert_eq!(
            crate::palette::to_hex(sep.heading_band.fills[1].expect("merged")),
            "#abcdef"
        );
        // A hostile radius is CLAMPED into the metric range, never rejected (TDD 18.11).
        assert_eq!(sep.metrics.heading_band_radius, METRIC_RANGE.1);
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

    /// TDD 18.26 / 18.2 — with no depth key stated, every tier carries the un-suffixed
    /// key's value, which is what makes the feature inert: a theme that says nothing
    /// paints exactly as it did before the tiers existed.
    #[test]
    fn every_bullet_tier_inherits_the_unsuffixed_key_by_default() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse("[themes.sepia]\nlist_marker = \"#112233\"\nlist_bullet_glyph = \"a\"\n")
                .unwrap(),
        );
        let t = themes.resolve("sepia");
        for tier in 0..BULLET_TIERS {
            assert_eq!(
                crate::palette::to_hex(t.list_bullet_colors[tier].expect("inherited")),
                "#112233",
                "tier {tier}"
            );
            assert_eq!(
                t.list_glyphs.bullet[tier].as_ref().map(|g| g.as_plain()),
                Some("a"),
                "tier {tier}"
            );
        }
        // System states none of them at all, so every tier is None and the drawn
        // default stands.
        let sys = Themes::builtin().resolve(SYSTEM_ID);
        assert!(sys.list_bullet_colors.iter().all(Option::is_none));
        assert!(sys.list_glyphs.bullet.iter().all(Option::is_none));
    }

    /// TDD 18.26 — each tier falls back to the next SHALLOWER one, not to the base and
    /// not to the deepest. The half-stated case is the one that distinguishes a real
    /// cascade from a two-way `or`: with depth 2 stated and depth 3 unset, depth 3 must
    /// take depth 2's value, NOT the un-suffixed key's.
    #[test]
    fn an_unstated_tier_falls_back_to_the_next_shallower_one() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.sepia]\nlist_marker = \"#111111\"\nlist_marker_2 = \"#222222\"\n\
                 list_bullet_glyph = \"a\"\nlist_bullet_glyph_2 = \"b\"\n",
            )
            .unwrap(),
        );
        let t = themes.resolve("sepia");
        let hex = |i: usize| crate::palette::to_hex(t.list_bullet_colors[i].unwrap());
        assert_eq!(hex(0), "#111111");
        assert_eq!(hex(1), "#222222");
        assert_eq!(
            hex(2),
            "#222222",
            "depth 3 must inherit depth 2, not depth 1"
        );
        let g = |i: usize| t.list_glyphs.bullet[i].as_ref().unwrap().as_plain();
        assert_eq!(g(0), "a");
        assert_eq!(g(1), "b");
        assert_eq!(g(2), "b");

        // And a theme that states ONLY the deepest tier leaves the two above it on the
        // base — the fallback runs one way, downward.
        let mut only3 = Themes::builtin();
        only3.merge_over(
            Themes::parse(
                "[themes.sepia]\nlist_marker = \"#111111\"\nlist_marker_3 = \"#333333\"\n",
            )
            .unwrap(),
        );
        let t3 = only3.resolve("sepia");
        assert_eq!(
            crate::palette::to_hex(t3.list_bullet_colors[0].unwrap()),
            "#111111"
        );
        assert_eq!(
            crate::palette::to_hex(t3.list_bullet_colors[1].unwrap()),
            "#111111"
        );
        assert_eq!(
            crate::palette::to_hex(t3.list_bullet_colors[2].unwrap()),
            "#333333"
        );
    }

    /// TDD 18.26 — the depth keys are BULLET-only. A nested ordered numeral and a nested
    /// task box keep the shared `list_marker`, which is the asymmetry the un-suffixed
    /// key's kind-blindness makes easy to get wrong in the other direction.
    #[test]
    fn the_depth_keys_do_not_reach_the_ordered_or_task_markers() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.sepia]\nlist_marker = \"#111111\"\nlist_marker_2 = \"#222222\"\n",
            )
            .unwrap(),
        );
        let t = themes.resolve("sepia");
        // The shared key is untouched by the depth keys — every non-bullet marker reads
        // it at every depth.
        assert_eq!(crate::palette::to_hex(t.list_marker.unwrap()), "#111111");
    }

    /// The `take!`-list guard, six keys' worth: a user file's depth override must merge
    /// over a shipped theme. Omitting a key there compiles, leaves every built-in theme
    /// working, and silently drops EVERY user override.
    #[test]
    fn a_user_file_can_override_a_bullet_depth_key() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.terminal]\nlist_marker_2 = \"#abcdef\"\n\
                 list_bullet_glyph_2 = \"·\"\nlist_bullet_glyph_3 = \"‧\"\n",
            )
            .unwrap(),
        );
        let t = themes.resolve("terminal");
        assert_eq!(
            crate::palette::to_hex(t.list_bullet_colors[1].expect("merged")),
            "#abcdef"
        );
        assert_eq!(t.list_glyphs.bullet[1].as_ref().unwrap().as_plain(), "·");
        assert_eq!(t.list_glyphs.bullet[2].as_ref().unwrap().as_plain(), "‧");
        // Terminal's own depth-1 glyph survives the override of the deeper tiers.
        assert_eq!(t.list_glyphs.bullet[0].as_ref().unwrap().as_plain(), "▸");
    }

    /// TDD 18.8 / 18.17 — a decoration LINE (a heading rule, a link underline, a strike)
    /// is a graphic, not text, so it answers to WCAG's 3:1 non-text floor rather than
    /// body prose's 4.5:1. Stated separately from the ink check below because reading a
    /// rule at the text floor would rule out every legitimate hairline accent, and a
    /// rule nobody can see is the other failure.
    #[test]
    fn every_theme_decoration_line_clears_the_non_text_contrast_floor() {
        let themes = Themes::builtin();
        for (id, _name, _sym) in themes.chooser_list() {
            let t = themes.resolve(&id);
            let Some(bg) = t.background else { continue };
            // Every decoration LINE a theme may colour. The heading rule's overline has
            // no colour key at all (see `HeadingRule`), so it is absent here by
            // construction — its ink is the heading's, already held to the stricter text
            // floor below.
            for (what, ink) in [
                ("heading rule", t.heading_rule.underline_rgba),
                ("link underline", t.link_underline_rgba),
                ("strikethrough", t.strikethrough_rgba),
            ] {
                let Some(ink) = ink else { continue };
                let c = crate::palette::contrast(ink, bg);
                assert!(
                    c >= 3.0,
                    "theme {id}: {what} {} on page {} is {c:.2}:1, below the \
                     3:1 non-text floor",
                    crate::palette::to_hex(ink),
                    crate::palette::to_hex(bg)
                );
            }
        }
    }

    /// TDD 18.8 / 18.21 — the legibility floor reaches HEADING ink, per level.
    ///
    /// A heading is text, so it takes the same 4.5:1 floor body prose does, and the
    /// resolved per-level array is what the tag, the table header and the HTML sink all
    /// read — so asserting on it covers the singular `heading_color` too (a level the
    /// theme left unset carries that value here). Before 18.21 nothing checked heading
    /// ink at all: `heading_color` could be set to anything and only the body pair was
    /// gated, which is exactly the "warm it up a bit" hole 18.8 exists to close.
    #[test]
    fn every_theme_heading_contrast_clears_the_legibility_floor() {
        let themes = Themes::builtin();
        for (id, _name, _sym) in themes.chooser_list() {
            let t = themes.resolve(&id);
            let Some(bg) = t.background else {
                continue; // derives from the desktop; the desktop owns its own contrast
            };
            for (level, ink) in t.heading_colors.iter().enumerate() {
                // Unset ⇒ the heading inherits the body foreground, which the body
                // check above already gates.
                let Some(ink) = ink else { continue };
                // Heading text sits ON its band where the theme states one (TDD 18.25),
                // so the pair that decides legibility is ink-on-BAND, not ink-on-page —
                // a band dark enough to look good behind a pale heading is exactly the
                // kind of change that would sail past a page-only check.
                let behind = t.heading_band.fills[level].unwrap_or(bg);
                let c = crate::palette::contrast(*ink, behind);
                assert!(
                    c >= 4.5,
                    "theme {id}: h{} ink {} on {} is {c:.2}:1, below WCAG AA",
                    level + 1,
                    crate::palette::to_hex(*ink),
                    crate::palette::to_hex(behind)
                );
            }
        }
    }

    #[test]
    fn sepia_body_contrast_is_wcag_aaa() {
        let t = Themes::builtin().resolve("sepia");
        let c = crate::palette::contrast(t.foreground.unwrap(), t.background.unwrap());
        assert!(c >= 7.0, "sepia body contrast {c:.2} should be AAA");
    }

    /// TDD 18.1 — System leads the chooser; the rest follow by display name.
    #[test]
    fn chooser_lists_system_first() {
        let list = Themes::builtin().chooser_list();
        assert_eq!(list[0].0, SYSTEM_ID);
        assert_eq!(list[0].1, "System");
        assert!(list
            .iter()
            .any(|(id, name, _sym)| id == "sepia" && name == "Sepia"));
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

    /// One key, two decompositions — the split the body and cell paths need.
    #[test]
    fn theme_color_decomposes_for_both_application_paths() {
        let c = ThemeColor(parse_color("#FFD133_61").unwrap());
        assert_eq!(c.hex(), "#ffd133");
        assert_eq!(c.alpha_pct(), "38%");
        assert_eq!(c.u16_triple().0, 0xffff);
        assert_eq!(c.rgba().alpha(), 97.0 / 255.0);
    }

    /// TDD 18.6 — the same key feeds the body tag and the table-cell markup, so a
    /// theme's overlay colours can never differ between the two.
    #[test]
    fn overlay_colours_resolve_per_theme_and_are_never_none() {
        let themes = Themes::builtin();
        let sys = themes.resolve(SYSTEM_ID);
        assert_eq!(sys.annotation_hl.hex(), "#ffd133");
        assert_eq!(sys.find_hl_all.hex(), "#f6d32d");
        assert_eq!(sys.find_hl_current.hex(), "#ff7800");
        assert_eq!(sys.mark_bg.hex(), "#fff59d");
        // Sepia replaces all three, because the system yellows wash out on cream.
        let sep = themes.resolve("sepia");
        assert_ne!(sep.annotation_hl.hex(), sys.annotation_hl.hex());
        assert_ne!(sep.find_hl_all.hex(), sys.find_hl_all.hex());
        assert_ne!(sep.find_hl_current.hex(), sys.find_hl_current.hex());
        assert_ne!(sep.mark_bg.hex(), sys.mark_bg.hex());
        // Synthwave's highlight is the radioactive toxic green — a deliberate,
        // theme-specific mark colour, distinct from the neutral yellow floor.
        let synth = themes.resolve("synthwave");
        assert_eq!(synth.mark_bg.hex(), "#39ff14");
        assert_ne!(synth.mark_bg.hex(), sys.mark_bg.hex());
        // …and keeps the system's alpha semantics for the wash.
        assert_eq!(sep.annotation_hl.alpha_pct(), "38%");
    }

    /// TDD 18.14 — a new theme is data. Nothing about adding one touches code.
    #[test]
    fn a_user_file_can_add_a_whole_new_theme() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse("[themes.slate]\nname = \"Slate\"\nbackground = \"#222222\"\nforeground = \"#dddddd\"\n")
                .unwrap(),
        );
        assert!(themes.contains("slate"));
        let t = themes.resolve("slate");
        assert_eq!(t.name, "Slate");
        assert_eq!(crate::palette::to_hex(t.background.unwrap()), "#222222");
        // It inherits [themes.system]'s typography/geometry without restating them.
        assert_eq!(t.typography.heading_scale, F_HEADING_SCALE);
        assert_eq!(t.metrics.list_step, F_LIST_STEP);
        // …and appears in the chooser after System.
        let list = themes.chooser_list();
        assert_eq!(list[0].0, SYSTEM_ID);
        assert!(list.iter().any(|(id, _, _)| id == "slate"));
    }

    /// TDD 18.13 — a user overrides ONE key without restating the theme.
    #[test]
    fn a_user_file_overrides_one_key_of_a_shipped_theme() {
        let mut themes = Themes::builtin();
        themes.merge_over(Themes::parse("[themes.sepia]\nbackground = \"#fffbe6\"\n").unwrap());
        let t = themes.resolve("sepia");
        assert_eq!(crate::palette::to_hex(t.background.unwrap()), "#fffbe6");
        // Every other Sepia key survives the override.
        assert_eq!(t.name, "Sepia");
        assert_eq!(crate::palette::to_hex(t.foreground.unwrap()), "#5b4636");
        assert_eq!(t.syntect_theme.as_deref(), Some("Solarized (light)"));
    }

    /// TDD 18.17 — `selection_fg` is opt-in: stated, it wins; omitted, it stays `None`
    /// so `palette` derives the selected-text ink from the page and its own ink.
    ///
    /// The merge half is asserted here on purpose. A new colour key has to be added in
    /// FOUR places (the spec, the resolved struct, `overlay`'s `take!` list, and
    /// `resolve`), and missing the `take!` list is invisible for built-in themes —
    /// `resolve()`'s per-key path masks it — while silently dropping every user
    /// override. That is exactly what happened to `list_marker` (test below).
    #[test]
    fn selection_fg_is_opt_in_and_merges() {
        assert!(Themes::builtin().resolve(SYSTEM_ID).selection_fg.is_none());
        assert!(Themes::builtin().resolve("sepia").selection_fg.is_none());
        let bed = Themes::builtin().resolve("bedtime");
        assert_eq!(
            crate::palette::to_hex(bed.selection_fg.expect("bedtime states it")),
            "#e6e4e9"
        );

        // The `take!`-list guard: a user override of a theme that ships no value.
        let mut themes = Themes::builtin();
        themes.merge_over(Themes::parse("[themes.sepia]\nselection_fg = \"#abcdef\"\n").unwrap());
        assert_eq!(
            crate::palette::to_hex(themes.resolve("sepia").selection_fg.expect("merged")),
            "#abcdef"
        );
    }

    /// TDD 10.17 — `mark_fg` is opt-in, and merges. Omitted, marked text keeps the body
    /// foreground (every theme's behaviour before the key existed); stated, it reaches
    /// both the body tag and the cell span. Same four-place / `take!`-list guard as
    /// [`selection_fg_is_opt_in_and_merges`].
    #[test]
    fn mark_fg_is_opt_in_and_merges() {
        assert!(Themes::builtin().resolve(SYSTEM_ID).mark_fg.is_none());
        assert!(Themes::builtin().resolve("synthwave").mark_fg.is_none());
        let bed = Themes::builtin().resolve("bedtime");
        assert_eq!(
            crate::palette::to_hex(bed.mark_fg.expect("bedtime states it")),
            "#a9ce99"
        );

        let mut themes = Themes::builtin();
        themes.merge_over(Themes::parse("[themes.sepia]\nmark_fg = \"#123456\"\n").unwrap());
        assert_eq!(
            crate::palette::to_hex(themes.resolve("sepia").mark_fg.expect("merged")),
            "#123456"
        );
    }

    /// Regression: `list_marker` must merge through a user-file override like every
    /// other colour. It was omitted from the `overlay` `take!` list, so a user
    /// override was silently dropped — `resolve()`'s own per-key path masked the gap
    /// for built-in themes, but a user file goes through `merge_over` → `overlay`.
    #[test]
    fn a_user_file_can_override_list_marker() {
        let mut themes = Themes::builtin();
        // Sepia ships no list_marker (stays None); a user file adds one.
        themes.merge_over(Themes::parse("[themes.sepia]\nlist_marker = \"#abcdef\"\n").unwrap());
        assert_eq!(
            crate::palette::to_hex(themes.resolve("sepia").list_marker.expect("merged")),
            "#abcdef"
        );
    }

    /// TDD 18.19 / 18.2 — the new chip keys default to absent, so the hardcoded
    /// amber/white fallback at the draw site is unaffected until a theme opts in.
    #[test]
    fn annotation_chip_keys_default_to_absent() {
        let system = Themes::builtin().resolve(SYSTEM_ID);
        assert_eq!(system.annotation_chip_bg, None);
        assert_eq!(system.annotation_chip_fg, None);
        assert_eq!(system.sprites.annotation_chip, None);
    }

    #[test]
    fn a_user_file_can_theme_the_annotation_chip_colours() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.system]\nannotation_chip_bg = \"#112233\"\nannotation_chip_fg = \"#ffffff\"\n",
            )
            .unwrap(),
        );
        let sys = themes.resolve(SYSTEM_ID);
        assert_eq!(
            crate::palette::to_hex(sys.annotation_chip_bg.expect("set")),
            "#112233"
        );
        assert_eq!(
            crate::palette::to_hex(sys.annotation_chip_fg.expect("set")),
            "#ffffff"
        );
    }

    /// `rewrite_sprite_paths` is the ONE filesystem-touching step for a sprite key —
    /// proves it accepts a contained relative reference and drops one that fails
    /// `crate::sprite::resolve`'s checks, independent of the XDG search path.
    #[test]
    fn rewrite_sprite_paths_resolves_a_contained_reference_and_drops_a_bad_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("chip.png"), b"not a real png, just bytes").unwrap();

        let mut good = ThemeSpec {
            sprite_annotation_chip: Some("chip.png".to_string()),
            ..Default::default()
        };
        rewrite_sprite_paths(&mut good, dir.path());
        // `resolve` only checks extension/containment/size, not that the bytes
        // decode — decoding is `sprite::texture`'s job, exercised in `sprite.rs`.
        let got = good.sprite_annotation_chip.expect("resolved");
        assert!(std::path::Path::new(&got).is_absolute());
        assert!(got.ends_with("chip.png"));

        let mut bad = ThemeSpec {
            sprite_annotation_chip: Some("../escape.png".to_string()),
            ..Default::default()
        };
        rewrite_sprite_paths(&mut bad, dir.path());
        assert_eq!(bad.sprite_annotation_chip, None);
    }

    /// A user may also override what the app hardcodes, by overriding
    /// [themes.system] — this is what retired config.toml's `[colors]` section.
    #[test]
    fn a_user_file_overrides_the_system_theme_itself() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse("[themes.system]\nfind_hl_all = \"#00ff00\"\nlist_step = 40\n").unwrap(),
        );
        let sys = themes.resolve(SYSTEM_ID);
        assert_eq!(sys.find_hl_all.hex(), "#00ff00");
        assert_eq!(sys.metrics.list_step, 40);
        // …and it reaches every theme that doesn't state its own, per link 2.
        assert_eq!(themes.resolve("sepia").metrics.list_step, 40);
    }

    /// TDD 18.11 — a malformed file is ignored, not fatal.
    #[test]
    fn a_malformed_user_file_is_ignored_and_the_builtin_survives() {
        assert!(Themes::parse("this is not = = valid toml").is_none());
        let mut themes = Themes::builtin();
        if let Some(user) = Themes::parse("this is not = = valid toml") {
            themes.merge_over(user);
        }
        assert!(themes.contains("sepia"));
        assert_eq!(themes.resolve(SYSTEM_ID).metrics.list_step, F_LIST_STEP);
    }

    /// TDD 18.11 — out-of-range geometry clamps rather than breaking layout.
    #[test]
    fn hostile_geometry_is_clamped() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.evil]\nlist_step = -5\nblockquote_bar_width = 10000\n\
                 heading_weight = 99999\nsupsub_scale = -3.0\nsuperscript_rise = 9999\n",
            )
            .unwrap(),
        );
        let t = themes.resolve("evil");
        assert_eq!(t.metrics.list_step, LIST_STEP_RANGE.0);
        assert_eq!(t.metrics.blockquote_bar_width, METRIC_RANGE.1);
        assert_eq!(t.typography.heading_weight, WEIGHT_RANGE.1);
        assert_eq!(t.typography.supsub_scale, SCALE_RANGE.0);
        assert_eq!(t.typography.superscript_rise, RISE_RANGE.1);
    }

    /// A theme file cannot kill the app with a wrong-length or non-finite array.
    #[test]
    fn malformed_arrays_fit_to_five_entries() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse(
                "[themes.short]\nheading_scale = [3.0]\n\
                 [themes.long]\nheading_space_below = [1, 2, 3, 4, 5, 6]\n\
                 [themes.nan]\nheading_scale = [nan, inf, 1.0, 1.0]\n",
            )
            .unwrap(),
        );
        // Short: stated entries apply, the rest keep the system hierarchy.
        let s = themes.resolve("short").typography.heading_scale;
        assert_eq!(s[0], 3.0);
        assert_eq!(&s[1..], &F_HEADING_SCALE[1..]);
        // Long: truncated to the five tags the renderer actually has.
        assert_eq!(
            themes.resolve("long").metrics.heading_space_below,
            [1, 2, 3, 4, 5]
        );
        // Non-finite: clamped, never propagated into Pango.
        let n = themes.resolve("nan").typography.heading_scale;
        assert!(n.iter().all(|x| x.is_finite()));
    }

    /// TDD 18.11 — a colour cannot escape a generated CSS rule, because it is
    /// re-emitted from a parsed RGBA rather than echoed.
    #[test]
    fn a_hostile_colour_string_cannot_inject_css() {
        let mut themes = Themes::builtin();
        themes.merge_over(
            Themes::parse("[themes.evil]\nbackground = \"#fff; } * { color: red; }\"\n").unwrap(),
        );
        // Unparseable → falls through to the desktop probe; nothing is interpolated.
        assert!(themes.resolve("evil").background.is_none());
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

    /// A stale persisted selection (a theme the user deleted) must degrade to the
    /// default, not fail.
    #[test]
    fn an_unknown_theme_id_resolves_as_system() {
        let t = Themes::builtin().resolve("no-such-theme");
        assert!(t.background.is_none());
        assert_eq!(t.typography.heading_scale, F_HEADING_SCALE);
    }
}
