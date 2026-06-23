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
//!   marker — ScrAP-121's exact failure mode. Resolution happens once, in
//!   [`Theme::resolve`]; consumers read the resolved [`Theme`], never the file.
//! * **The theme owns Pango SCALE, never CSS `font-size`.** Scale is a tag
//!   attribute GTK *multiplies* onto the CSS base (`gtktextattributes.c:349-351`),
//!   so it composes with zoom for free. `font-size` is a CSS longhand the zoom
//!   provider owns exclusively; a second provider writing the same lookup slot is
//!   arbitrated by provider ADD ORDER, not selector specificity (ScrAP-127), which
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
    pub link: Option<String>,
    pub code_inline_bg: Option<String>,
    pub code_block_bg: Option<String>,
    pub blockquote_bar: Option<String>,
    pub selection_bg: Option<String>,
    pub table_border: Option<String>,
    pub table_head_bg: Option<String>,
    pub rule: Option<String>,
    /// List-marker glyph colour — the unordered bullet dot, the ordered numeral, and
    /// the task checkbox outline+tick (all three drawn in the left gutter). Colours the
    /// MARKER ONLY; the item's text keeps the body foreground. Omit ⇒ markers inherit
    /// the widget foreground (the pre-theming default). One key for all three kinds.
    pub list_marker: Option<String>,

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
    pub blockquote_bar_width: Option<i32>,
    pub blockquote_text_gap: Option<i32>,
    pub list_step: Option<i32>,
    pub list_item_gap: Option<i32>,
    pub rule_space: Option<i32>,
    pub table_cell_padding_v: Option<i32>,
    pub table_cell_padding_h: Option<i32>,
    pub table_border_width: Option<i32>,
    pub table_cell_radius: Option<i32>,
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
            link,
            code_inline_bg,
            code_block_bg,
            blockquote_bar,
            selection_bg,
            table_border,
            table_head_bg,
            rule,
            list_marker,
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
            blockquote_bar_width,
            blockquote_text_gap,
            list_step,
            list_item_gap,
            rule_space,
            table_cell_padding_v,
            table_cell_padding_h,
            table_border_width,
            table_cell_radius,
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

/// Decoration metrics: design-time px at zoom 1.0. Every consumer scales these
/// through the existing `px(n) = round(n * zoom)` path; a theme never expresses
/// pixels at the current zoom.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Metrics {
    pub heading_space_below: [i32; 5],
    pub blockquote_bar_width: i32,
    pub blockquote_text_gap: i32,
    /// The ONE definition both the `li-{depth}` tag's `left_margin` and the drawn
    /// marker gutter's x read. A value that reached one but not the other would
    /// strand every list marker — ScrAP-121.
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
    pub link: Option<gdk::RGBA>,
    pub code_inline_bg: Option<gdk::RGBA>,
    pub code_block_bg: Option<gdk::RGBA>,
    pub blockquote_bar: Option<gdk::RGBA>,
    pub selection_bg: Option<gdk::RGBA>,
    pub table_border: Option<gdk::RGBA>,
    pub table_head_bg: Option<gdk::RGBA>,
    pub rule: Option<gdk::RGBA>,
    /// List-marker glyph colour (bullet/numeral/checkbox); `None` ⇒ inherit the widget
    /// foreground. Marker glyph only — never the item text.
    pub list_marker: Option<gdk::RGBA>,
    pub annotation_hl: ThemeColor,
    pub find_hl_all: ThemeColor,
    pub find_hl_current: ThemeColor,
    pub mark_bg: ThemeColor,
    pub typography: Typography,
    pub metrics: Metrics,
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
            heading_color: color(&selected.heading_color, &system.heading_color),
            heading_font: pick(&selected.heading_font, &system.heading_font)
                .as_deref()
                .and_then(sanitize_font_family),
            link: color(&selected.link, &system.link),
            code_inline_bg: color(&selected.code_inline_bg, &system.code_inline_bg),
            code_block_bg: color(&selected.code_block_bg, &system.code_block_bg),
            blockquote_bar: color(&selected.blockquote_bar, &system.blockquote_bar),
            selection_bg: color(&selected.selection_bg, &system.selection_bg),
            table_border: color(&selected.table_border, &system.table_border),
            table_head_bg: color(&selected.table_head_bg, &system.table_head_bg),
            rule: color(&selected.rule, &system.rule),
            list_marker: color(&selected.list_marker, &system.list_marker),
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
    if let Some(text) = find_themes_file() {
        if let Some(user) = Themes::parse(&text) {
            themes.merge_over(user);
        }
    }
    themes
}

fn find_themes_file() -> Option<String> {
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
            return Some(text);
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
