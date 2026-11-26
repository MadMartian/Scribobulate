//! **Every themed Pango span this application emits, in one place.**
//!
//! Pango markup is the project's *third* representation of a themed inline style,
//! beside the preview's `GtkTextTag`s and the HTML sink's CSS — and it is reached from
//! two unrelated directions. A table cell is a `GtkLabel` outside the buffer, so no
//! `GtkTextTag` can style it (ScrAP-36/GTK4Rs/AP-28) and the preview builds markup for it;
//! the PDF sink lays every run out through Pango and builds markup for that. Both were
//! building the same five spans, independently.
//!
//! # What that cost, measured rather than feared
//!
//! `renderer::mark_open` and `export::markup`'s `Inline::Highlight` arm were
//! byte-identical **except for `{fg}`** — the `mark_fg` projection. That single
//! difference is the whole mechanism behind `mark_fg` reaching the preview and the HTML
//! sink and never the page: not an oversight inside a shared function, but a second
//! copy nobody updated. `renderer::mod`'s own comment documented the reasoning for
//! adding `mark_fg` *to that copy*, with no reference to the twin one module over. The
//! other four pairs had not diverged, and that was luck rather than structure.
//!
//! There is a second axis, and it is why this module takes an explicit `&Theme`: the
//! renderer's copies resolved against the global `crate::theme::active()` while the
//! export copies took a `&Theme` (deliberately — the PDF resolves at System-light, TDD
//! 25.9). Two copies reading two different sources means a test on one says nothing
//! about the other, which is exactly what the parity test at the foot of this file
//! exists to close.
//!
//! # Why every span is a PAIR
//!
//! `strike` has to be, and the reason generalises: its plain form closes with `</s>`
//! and its themed form with `</span>`, so an open and a close chosen independently can
//! disagree — and a mismatched pair fails `pango_parse_markup`, which renders the whole
//! run **EMPTY**, with no warning (ScrAP-163). Any future themed variant of the other
//! four would acquire the same hazard the moment it changed a tag name. Handing out
//! both halves from one call makes that unrepresentable rather than remembered.

use crate::theme::Theme;
use std::fmt::Write;

/// The generic family code is set in where nothing names another.
pub(crate) const MONOSPACE: &str = "monospace";

/// One themed span: the opening tag, and the closing tag that pairs with it.
///
/// Deliberately not two functions. See the module header — the two halves are not
/// independent, and a mismatched pair renders the run empty rather than failing loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    pub open: String,
    pub close: &'static str,
}

impl Span {
    fn plain(open: &str, close: &'static str) -> Span {
        Span {
            open: open.to_string(),
            close,
        }
    }

    /// A `<span …>` with the given attribute text (which must begin with a space, or
    /// be empty).
    fn attrs(attrs: &str) -> Span {
        Span {
            open: format!("<span{attrs}>"),
            close: "</span>",
        }
    }

    /// Wrap `inner` — for a caller that has the whole run in hand.
    ///
    /// Test-only: production emitters push the two halves around a recursive walk
    /// (`export::markup::span`) rather than materialising the inner run first.
    #[cfg(test)]
    pub(crate) fn wrap(&self, inner: &str) -> String {
        format!("{}{inner}{}", self.open, self.close)
    }
}

/// A `==highlight==` (mark): its themed fill, and its themed ink where the theme
/// states one.
///
/// **Separate `bgalpha` rather than 8-digit hex**, for robust Pango compatibility;
/// `ThemeColor` owns that decomposition. The ink rides the SAME span as the fill,
/// because a cell is a `GtkLabel` outside the buffer and the body tag's foreground
/// cannot reach it — and is emitted only when the theme states one, so a theme without
/// the key produces the byte-identical span this path always did (TDD 18.2).
pub(crate) fn mark(t: &Theme) -> Span {
    let c = &t.mark_bg;
    let mut attrs = format!(" background=\"{}\" bgalpha=\"{}\"", c.hex(), c.alpha_pct());
    if let Some(fg) = t.mark_fg {
        let _ = write!(attrs, " foreground=\"{}\"", crate::palette::to_hex_rgba(fg));
    }
    Span::attrs(&attrs)
}

/// An inline `code` run: the monospace face, and the chip's fill where the surface it
/// sits on has one.
///
/// **The one span here that takes a colour rather than a `&Theme`**, and deliberately:
/// the chip's fill is a function of the SURFACE behind the run, not of the theme
/// alone, so the caller — which is the only thing that knows where it is — resolves it
/// through `palette::CodeChips` and hands the answer down. A `&Theme` here would let
/// this module answer "the page's chip" for a run inside a heading band, which is the
/// defect ScrAP-356 records.
///
/// `None` ⇒ face only, for a surface whose colour is unknown (a tile). Absent, not
/// guessed.
pub(crate) fn code(fill: Option<gtk::gdk::RGBA>) -> Span {
    // The face with the script fallback after it ([`with_script_fallback`]); escaped,
    // because the fallback is the desktop's font name and lands inside an attribute.
    let mut attrs = format!(
        " font_family=\"{}\"",
        gtk::glib::markup_escape_text(&with_script_fallback(MONOSPACE))
    );
    if let Some(c) = fill {
        let _ = write!(attrs, " background=\"{}\"", crate::palette::to_hex_rgba(c));
    }
    Span::attrs(&attrs)
}

/// A CriticMarkup claim's highlight wash (TDD 18.5/18.6).
pub(crate) fn annotation_claim(t: &Theme) -> Span {
    let c = &t.annotation_hl_color;
    Span::attrs(&format!(
        " background=\"{}\" bgalpha=\"{}\"",
        c.hex(),
        c.alpha_pct()
    ))
}

/// One side of a suggested edit: its ink, its fill where the theme states one, and —
/// for what it would remove — the strike line, in the strike colour where one is set
/// (PLAN.suggested-edit-colours.md). The resolved `style` is the palette's, the same
/// value the preview's body tag reads.
pub(crate) fn suggestion(style: crate::palette::SuggestionStyle, struck: bool, t: &Theme) -> Span {
    let mut attrs = format!(
        " foreground=\"{}\"",
        crate::palette::to_hex_opaque(style.fg)
    );
    if let Some(bg) = style.bg {
        // Colour and alpha apart, as `annotation_claim` writes them, for Pango robustness.
        let _ = write!(
            attrs,
            " background=\"{}\" bgalpha=\"{}%\"",
            crate::palette::to_hex_opaque(bg),
            (bg.alpha() * 100.0).round() as i32
        );
    }
    if struck {
        attrs.push_str(" strikethrough=\"true\"");
        if let Some(c) = t.strikethrough_color {
            let _ = write!(
                attrs,
                " strikethrough_color=\"{}\"",
                crate::palette::to_hex_opaque(c)
            );
        }
    }
    Span::attrs(&attrs)
}

/// The span for a run of annotated text in `style`: the claim highlight, or a suggested
/// edit's addition or deletion colours.
pub(crate) fn claim(
    style: crate::annotate::ClaimStyle,
    t: &Theme,
    inks: crate::palette::SuggestionInks,
) -> Span {
    use crate::annotate::ClaimStyle;
    match style {
        ClaimStyle::Highlight => annotation_claim(t),
        ClaimStyle::Addition => suggestion(inks.addition, false, t),
        ClaimStyle::Deletion => suggestion(inks.deletion, true, t),
    }
}

/// `**bold**` at the theme's own weight (TDD 18.18).
pub(crate) fn bold(t: &Theme) -> Span {
    Span::attrs(&t.typography.bold_attr())
}

/// `~~strikethrough~~`, in the theme's strike colour where it states one (TDD 18.23).
///
/// The pair that proves why every span here is a pair: `("<s>", "</s>")` against
/// `("<span …>", "</span>")`.
pub(crate) fn strike(t: &Theme) -> Span {
    match t.strikethrough_color {
        None => Span::plain("<s>", "</s>"),
        Some(c) => Span::attrs(&format!(
            " strikethrough=\"true\" strikethrough_color=\"{}\"",
            crate::palette::to_hex_rgba(c)
        )),
    }
}

/// `^superscript^` at the theme's scale and rise (TDD 18.18).
pub(crate) fn superscript(t: &Theme) -> Span {
    Span::attrs(&t.typography.supsub_attr(true))
}

/// `~subscript~` at the theme's scale and rise (TDD 18.18).
pub(crate) fn subscript(t: &Theme) -> Span {
    Span::attrs(&t.typography.supsub_attr(false))
}

/// A link's ink and underline (TDD 18.23), and the destination the reader cannot click.
///
/// Only the PDF sink emits this today — a preview link is a `GtkTextTag` — but it lives
/// here because it reads the same three keys through the same grammar as its
/// neighbours, and a second Pango link span written elsewhere would be this module's
/// whole failure mode returning.
pub(crate) fn link(t: &Theme) -> Span {
    let mut attrs = format!(" underline=\"{}\"", t.link_underline.pango_markup());
    if let Some(c) = t.link_underline_color {
        let _ = write!(
            attrs,
            " underline_color=\"{}\"",
            crate::palette::to_hex_rgba(c)
        );
    }
    let colour = t
        .link_color
        .or(t.accent_color)
        .map(crate::palette::to_hex_rgba)
        .unwrap_or_default();
    if !colour.is_empty() {
        let _ = write!(attrs, " foreground=\"{colour}\"");
    }
    Span::attrs(&attrs)
}

/// The family code is set in, with the platform's script fallback after it: `family`
/// itself wherever the platform supplies none.
///
/// Every surface that sets code in a face goes through this — the inline-code span
/// above (prose in the PDF, table cells in the preview) and the preview's code tags,
/// while the PDF's code block composes the same through [`family_with_fallback`] —
/// because a code family REPLACES the paragraph's, so the
/// fallback the paragraph carries does not reach it. On Windows `monospace` falls back
/// to no Korean face a stock install has, and Hangul in code drew as hex boxes on both
/// surfaces while the prose around it drew (`platform::win32::script_fallback`).
pub(crate) fn with_script_fallback(family: &str) -> String {
    family_with_fallback(
        Some(family.to_string()),
        None,
        crate::platform::script_fallback_family().as_deref(),
    )
    .unwrap_or_else(|| family.to_string())
}

/// The family list a run is laid out in, with the platform's script fallback after it.
///
/// `requested` is the stack the run names (already in Pango's spelling), `default` the
/// family the Pango context lays an unnamed run out in, and `fallback` the face
/// [`crate::platform::script_fallback_family`] supplies. Where the platform supplies
/// none — every platform whose font map already falls back over all installed faces —
/// the request comes back untouched, so the run is described exactly as it was before
/// there was a fallback to add. Where it does, the fallback goes LAST: it only ever
/// answers for a character no face before it can draw, so the run's lead face, and with
/// it the page's look, is unchanged.
///
/// A request naming no face still needs spelling out once there is a fallback, because
/// a family that names only the fallback would replace the context's default face
/// rather than extend it.
pub(crate) fn family_with_fallback(
    requested: Option<String>,
    default: Option<&str>,
    fallback: Option<&str>,
) -> Option<String> {
    let Some(fallback) = fallback else {
        return requested;
    };
    match requested.as_deref().or(default) {
        Some(stack) => Some(format!("{stack},{fallback}")),
        None => Some(fallback.to_string()),
    }
}

// ── what a face draws, and the combining marks a run's own face lacks ───────────────

/// The size a coverage probe loads a face at, in points.
///
/// Coverage does not depend on it; it is stated because a description with no size has
/// taken Pango to a negative-size CRITICAL on one host (`host_has_a_face_for`).
const COVERAGE_PROBE_PT: i32 = 12;

/// The face Pango sets text named `family` in — the first family of the list the font
/// map has — or `None` where it loads nothing.
///
/// **The one coverage probe**, for every surface that must know whether a run's own face
/// draws a character: a themed disclosure glyph (`widgets::disclosure`) and a letter's
/// combining marks ([`mark_cover`]). `family` is spelled the way Pango splits a list,
/// names joined by a bare comma, as `CssSafeFontStack::pango_family` gives them and as GTK
/// hands a CSS stack to Pango. Spelled with a space after each comma, this probe answered
/// for the wrong face: Pango does not trim the names it splits out, the Win32 font map
/// matched ` Georgia` to nothing, and with every family but the first missing a themed
/// stack loaded `Sans` (Tahoma) and warned `couldn't load font`, while the preview drew
/// Georgia (MEASURED, Pango 1.57 / Windows 10).
pub(crate) fn lead_face(ctx: &gtk::pango::Context, family: &str) -> Option<gtk::pango::Font> {
    let mut desc = gtk::pango::FontDescription::new();
    desc.set_family(family);
    desc.set_size(COVERAGE_PROBE_PT * gtk::pango::SCALE);
    ctx.load_font(&desc)
}

/// Whether `face` has a glyph of its own for every character of `text`.
pub(crate) fn face_covers(face: &gtk::pango::Font, text: &str) -> bool {
    use gtk::pango::prelude::FontExt;
    let coverage = face.coverage(&gtk::pango::Language::default());
    text.chars()
        .all(|c| coverage.get(c as i32) != gtk::pango::CoverageLevel::None)
}

/// Whether `c` is a combining mark: a character shaped against the letter before it.
fn is_mark(c: char) -> bool {
    // SAFETY: a pure lookup in GLib's Unicode tables, defined for every code point.
    unsafe { gtk::glib::ffi::g_unichar_ismark(u32::from(c)) != 0 }
}

/// Whether `text` carries a combining mark at all — the cheap test a caller makes before
/// asking [`mark_cover`] anything.
pub(crate) fn has_combining_mark(text: &str) -> bool {
    text.chars().any(is_mark)
}

/// The family the preview sets its prose in: the theme's stack, which the theme sheet
/// puts on the view, else the desktop's face, which GTK lays an unstyled view out in.
pub(crate) fn page_family(theme: &Theme) -> Option<String> {
    match &theme.font_family {
        Some(stack) => Some(stack.pango_family()),
        None => {
            if !gtk::is_initialized_main_thread() {
                return None;
            }
            let name = gtk::Settings::default()?.gtk_font_name()?;
            let family = gtk::pango::FontDescription::from_string(&name).family()?;
            Some(family.to_string())
        }
    }
}

/// The byte range of every cluster of `text` that carries a combining mark: a letter and
/// the marks after it. A mark with no letter before it opens no cluster.
fn mark_clusters(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    // The current letter's start, and the end of its last mark (equal while it has none).
    let mut open: Option<(usize, usize)> = None;
    for (at, c) in text.char_indices() {
        let end = at + c.len_utf8();
        match (is_mark(c), open.as_mut()) {
            (true, Some((_, marks_end))) => *marks_end = end,
            (true, None) => {}
            (false, _) => {
                out.extend(
                    open.filter(|(start, marks_end)| marks_end > start)
                        .map(|(s, e)| s..e),
                );
                open = Some((at, at));
            }
        }
    }
    out.extend(
        open.filter(|(start, marks_end)| marks_end > start)
            .map(|(s, e)| s..e),
    );
    out
}

/// The clusters of a run its own face would split, and the face that keeps each whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkCover {
    /// The platform's face for them ([`crate::platform::mark_fallback_family`]).
    pub(crate) family: &'static str,
    /// Byte ranges into the text asked about, in order, each a letter and its marks.
    pub(crate) clusters: Vec<std::ops::Range<usize>>,
}

/// **Where a letter's combining marks would draw detached, the face that keeps them on
/// it** — the one decision every surface applies, each in its own medium: a tag in the
/// preview's buffer, a span in a table cell's markup, an attribute in the PDF's layout.
///
/// The disclosure glyph's rule (move only what the run's own face cannot draw) applied
/// per CLUSTER. On Windows a mark the lead face lacks is itemized into a run of its own,
/// in whatever face Pango's alias table reaches, and HarfBuzz positions a mark only
/// against a base in the same run, so `n` + U+0303 under Georgia drew a tilde beside the
/// `n` (`platform::win32::script_fallback`). A cluster set in a face that has its letter
/// and every mark shapes in one run and composes. Only the cluster moves: Latin text
/// without marks keeps its face, and the text itself is untouched — `n` + U+0303 stays
/// two characters, never rewritten to `ñ` (TDD 25.18). (HarfBuzz may DRAW the pair as
/// the face's composed glyph; cairo then carries the two characters as its
/// `/ActualText`, so the PDF still extracts them as written.)
///
/// `lead_at(byte)` is the face the run is set in at the cluster starting at `byte`: the
/// face Pango put the letter in, where the caller has the laid-out runs, else
/// [`lead_face`] of the family the run names.
///
/// `None` — leave the text exactly as it is — where the platform names no face (Linux and
/// macOS: inert, so byte-identical), where `text` carries no combining mark, and where
/// every cluster's lead face draws all its marks, or cannot draw its letter either: a
/// letter its run's face lacks is the script fallback's to place, mark and all.
pub(crate) fn mark_cover(
    ctx: &gtk::pango::Context,
    text: &str,
    mut lead_at: impl FnMut(usize) -> Option<gtk::pango::Font>,
) -> Option<MarkCover> {
    let family = crate::platform::mark_fallback_family()?;
    if !has_combining_mark(text) {
        return None;
    }
    // Loaded at the first cluster that needs it, and only then.
    let mut cover: Option<Option<gtk::pango::Font>> = None;
    let mut clusters = Vec::new();
    for range in mark_clusters(text) {
        let cluster = &text[range.clone()];
        let Some(letter) = cluster.chars().next() else {
            continue;
        };
        let Some(lead) = lead_at(range.start) else {
            continue;
        };
        if !face_covers(&lead, letter.encode_utf8(&mut [0; 4])) || face_covers(&lead, cluster) {
            continue;
        }
        let cover = cover.get_or_insert_with(|| named_face(ctx, family));
        if cover
            .as_ref()
            .is_some_and(|face| face_covers(face, cluster))
        {
            clusters.push(range);
        }
    }
    (!clusters.is_empty()).then_some(MarkCover { family, clusters })
}

/// `family`'s own face, or `None` where the font map would substitute another for it: a
/// cover face this host does not have covers nothing.
fn named_face(ctx: &gtk::pango::Context, family: &str) -> Option<gtk::pango::Font> {
    use gtk::pango::prelude::FontExt;
    lead_face(ctx, family).filter(|face| {
        face.describe()
            .family()
            .is_some_and(|got| got.eq_ignore_ascii_case(family))
    })
}

/// A context on the font map GTK draws widgets with, for a caller that has none of its
/// own — the preview's renderer, which builds buffer text and cell markup, not widgets.
pub(crate) fn widget_font_context() -> gtk::pango::Context {
    use pangocairo::prelude::FontMapExt;
    pangocairo::FontMap::default().create_context()
}

/// `text` escaped as Pango markup, with each cluster [`mark_cover`] answers for wrapped in
/// a span of the face that keeps its marks on it — for a run laid out in `family` whose
/// markup is built as text (a preview table cell). Off Windows, for a run whose family is
/// unknown, and for any text without a mark, exactly `glib::markup_escape_text`.
pub(crate) fn escape_covering_marks(text: &str, family: Option<&str>) -> String {
    let escape = |s: &str| gtk::glib::markup_escape_text(s).to_string();
    // Before a context is made, so the platform without a face pays nothing.
    let family = match family {
        Some(family)
            if crate::platform::mark_fallback_family().is_some() && has_combining_mark(text) =>
        {
            family
        }
        _ => return escape(text),
    };
    let ctx = widget_font_context();
    let lead = lead_face(&ctx, family);
    let Some(cover) = mark_cover(&ctx, text, |_| lead.clone()) else {
        return escape(text);
    };
    let mut out = String::with_capacity(text.len() + 48 * cover.clusters.len());
    let mut at = 0;
    for range in &cover.clusters {
        let _ = write!(
            out,
            "{}<span font_family=\"{}\">{}</span>",
            escape(&text[at..range.start]),
            escape(cover.family),
            escape(&text[range.clone()])
        );
        at = range.end;
    }
    out.push_str(&escape(&text[at..]));
    out
}

/// Set each cluster of an already-built `layout` that [`mark_cover`] answers for in the
/// face that keeps its marks on it, as a family attribute over the cluster alone — for
/// the PDF sink, whose layouts are itemized, so a cluster's lead face is the face Pango
/// actually put its letter in, whatever span chose it. Off Windows, and for a layout
/// without a mark, nothing is touched and nothing is re-laid out.
pub(crate) fn cover_marks_in_layout(layout: &gtk::pango::Layout) {
    if crate::platform::mark_fallback_family().is_none() {
        return;
    }
    let text = layout.text();
    if !has_combining_mark(&text) {
        return;
    }
    let runs = laid_runs(layout);
    let lead_at = |at: usize| {
        runs.iter()
            .find(|(range, _)| range.contains(&at))
            .map(|(_, face)| face.clone())
    };
    let Some(cover) = mark_cover(&layout.context(), &text, lead_at) else {
        return;
    };
    // A copy, not the layout's own list mutated in place: `set_attributes` with an equal
    // list keeps the stale itemization.
    let attrs = layout
        .attributes()
        .and_then(|own| own.copy())
        .unwrap_or_default();
    for range in &cover.clusters {
        let (Ok(start), Ok(end)) = (u32::try_from(range.start), u32::try_from(range.end)) else {
            continue;
        };
        // Inserted after every attribute starting at or before it, so it is the family
        // that wins over the cluster, a code span's included.
        let mut family = gtk::pango::AttrString::new_family(cover.family);
        family.set_start_index(start);
        family.set_end_index(end);
        attrs.insert(family);
    }
    layout.set_attributes(Some(&attrs));
}

/// Every run of `layout` as its byte range and the face Pango itemized it in.
fn laid_runs(layout: &gtk::pango::Layout) -> Vec<(std::ops::Range<usize>, gtk::pango::Font)> {
    let mut runs = Vec::new();
    let mut iter = layout.iter();
    loop {
        if let Some(run) = iter.run_readonly() {
            let item = run.item();
            if let (Ok(start), Ok(len)) = (
                usize::try_from(item.offset()),
                usize::try_from(item.length()),
            ) {
                runs.push((start..start + len, item.analysis().font()));
            }
        }
        if !iter.next_run() {
            break;
        }
    }
    runs
}

#[cfg(test)]
mod mark_tests {
    use super::mark_clusters;

    /// A cluster is a letter and the marks after it — two marks stay one cluster, a mark
    /// at the start (no letter) opens none, and a letter with no mark is not one.
    #[test]
    fn a_cluster_is_a_letter_and_the_marks_after_it() {
        let text = "\u{303}an\u{303}o q\u{323}\u{301}!";
        let got: Vec<&str> = mark_clusters(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(got, ["n\u{303}", "q\u{323}\u{301}"]);
        assert!(mark_clusters("plain Latin, ñ precomposed").is_empty());
    }

    /// Off Windows the platform names no face, so nothing is ever covered — the text is
    /// laid out exactly as it was (the inert, byte-identical half of the contract).
    #[test]
    fn no_platform_face_covers_nothing_and_escapes_as_before() {
        if crate::platform::mark_fallback_family().is_some() {
            println!(
                "SKIPPED [TDD 25.18]: this platform names a mark face; the GTK tests cover it"
            );
            return;
        }
        let text = "a<n\u{303}>&";
        assert_eq!(
            super::escape_covering_marks(text, Some("serif")),
            gtk::glib::markup_escape_text(text).as_str()
        );
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::family_with_fallback;

    /// Where the platform supplies no fallback, nothing changes — including a run with
    /// no face of its own, which must stay unnamed so the context's default still lays
    /// it out exactly as before.
    #[test]
    fn no_fallback_leaves_the_request_untouched() {
        assert_eq!(
            family_with_fallback(Some("Georgia,serif".into()), Some("serif"), None),
            Some("Georgia,serif".into())
        );
        assert_eq!(family_with_fallback(None, Some("serif"), None), None);
    }

    /// The fallback goes after the stack, never before: the lead face is the theme's.
    #[test]
    fn the_fallback_follows_the_named_stack() {
        assert_eq!(
            family_with_fallback(
                Some("Georgia,serif".into()),
                Some("serif"),
                Some("Segoe UI")
            ),
            Some("Georgia,serif,Segoe UI".into())
        );
    }

    /// A run naming no face extends the context's default rather than being replaced
    /// by the fallback, which would change every unthemed run's face.
    #[test]
    fn an_unnamed_run_extends_the_context_default() {
        assert_eq!(
            family_with_fallback(None, Some("serif"), Some("Segoe UI")),
            Some("serif,Segoe UI".into())
        );
        assert_eq!(
            family_with_fallback(None, None, Some("Segoe UI")),
            Some("Segoe UI".into())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themed(spec: &str) -> Theme {
        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(spec);
        themes.resolve("t")
    }

    /// **Every span parses as Pango markup, on a theme that states every key.**
    ///
    /// The pair is the hazard: an open and a close that disagree fail
    /// `pango_parse_markup` and render the run EMPTY with no warning (ScrAP-163), so a
    /// test that merely string-matched the open would pass on exactly the broken case.
    #[test]
    fn every_span_parses_as_markup_around_real_content() {
        let t = themed(
            "[themes.t]\nmark_bg = \"#fff59d_88\"\nmark_fg = \"#402000\"\n\
             strikethrough_color = \"#aa0000\"\nlink_color = \"#0055cc\"\n\
             link_underline = \"double\"\nlink_underline_color = \"#00aa55\"\n\
             annotation_hl_color = \"#FFD133_61\"\nbold_weight = 800\n",
        );
        for (name, span) in named_spans(&t) {
            let markup = span.wrap("content &amp; more");
            let parsed = gtk::pango::parse_markup(&markup, '\u{0}');
            assert!(
                parsed.is_ok(),
                "{name} does not parse as Pango markup: {markup:?}"
            );
            let (_attrs, text, _accel) = parsed.unwrap();
            assert_eq!(
                text, "content & more",
                "{name} must wrap its content, not replace it"
            );
        }
    }

    /// A theme that states NOTHING still produces parseable, inert spans — the
    /// byte-identical case (TDD 18.2), which is the one a themed variant is most
    /// likely to break by assuming its key is present.
    #[test]
    fn an_unstated_theme_still_produces_parseable_spans() {
        let t = crate::theme::Themes::builtin().resolve(crate::theme::SYSTEM_ID);
        for (name, span) in named_spans(&t) {
            let markup = span.wrap("x");
            assert!(
                gtk::pango::parse_markup(&markup, '\u{0}').is_ok(),
                "{name} does not parse on an unstated theme: {markup:?}"
            );
        }
        // The strike pair is the one that changes SHAPE between the two cases.
        assert_eq!(strike(&t), Span::plain("<s>", "</s>"));
    }

    fn named_spans(t: &Theme) -> Vec<(&'static str, Span)> {
        vec![
            ("mark", mark(t)),
            ("annotation_claim", annotation_claim(t)),
            ("bold", bold(t)),
            ("strike", strike(t)),
            ("superscript", superscript(t)),
            ("subscript", subscript(t)),
            ("link", link(t)),
        ]
    }

    /// **The preview's wrappers and the export sink emit the SAME span for the same
    /// theme.**
    ///
    /// This is the assertion that could not exist while there were two copies: they
    /// read two different sources, so a test on one said nothing about the other. There
    /// is no method to ban here, so the enforcement ladder stops at the test rung — and
    /// this is that rung.
    ///
    /// **No `activate_for_test` any more.** The preview wrappers used to read the
    /// process-global active theme, so proving the parity meant installing one; both
    /// sides now take an explicit `&Theme` (F-BUILDPRODUCTS-001), which is a strictly
    /// stronger test — it can no longer pass because both sides happened to read the
    /// same global.
    #[test]
    fn the_preview_wrappers_and_the_export_sink_emit_one_span() {
        let t = themed(
            "[themes.t]\nmark_bg = \"#fff59d_88\"\nmark_fg = \"#402000\"\n\
             strikethrough_color = \"#aa0000\"\nannotation_hl_color = \"#FFD133_61\"\n\
             bold_weight = 800\nsupsub_scale = 0.6\nsuperscript_rise = 9\n",
        );
        assert_eq!(crate::renderer::mark_open(&t), mark(&t).open);
        assert_eq!(crate::renderer::ann_hl_open(&t), annotation_claim(&t).open);
        assert_eq!(crate::renderer::bold_open(&t), bold(&t).open);
        assert_eq!(crate::renderer::superscript_open(&t), superscript(&t).open);
        assert_eq!(crate::renderer::subscript_open(&t), subscript(&t).open);
        let (open, close) = crate::renderer::strike_tags(&t);
        assert_eq!((open, close), (strike(&t).open, strike(&t).close));
    }
}

/// Whether any face installed on this host covers `c`: a host with none can only draw
/// tofu, so a script test skips rather than asks it. One copy for every such test.
///
/// Every probe carries a size. Without one, some face on the GitHub Linux runner took
/// Pango to `pango_font_description_set_size` with a negative size — a Pango-CRITICAL the
/// suite's fatal-criticals turns into an abort, twice, once from each of two copies of
/// this probe.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn host_has_a_face_for(c: char) -> bool {
    use gtk::pango;
    use gtk::pango::prelude::{FontExt, FontFamilyExt, FontMapExt};
    let map = pangocairo::FontMap::default();
    let ctx = map.create_context();
    let language = pango::Language::default();
    map.list_families().iter().any(|family| {
        let mut desc = pango::FontDescription::new();
        desc.set_family(&family.name());
        desc.set_size(12 * pango::SCALE);
        ctx.load_font(&desc).is_some_and(|font| {
            font.coverage(&language).get(c as i32) != pango::CoverageLevel::None
        })
    })
}
