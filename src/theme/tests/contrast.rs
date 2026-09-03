//! Legibility floors. Every shipped theme is held to WCAG here, because a theme is
//! content and content is where an unreadable pairing gets in (TDD 18.8/18.17).

use super::super::*;
use gtk::gdk;

/// TDD 18.8 — the legibility floor. Every theme that states its own page must
/// clear WCAG AA for body text; this is what stops a later "warm it up a bit"
/// tweak from quietly degrading readability.
#[test]
fn every_theme_body_contrast_clears_the_legibility_floor() {
    let themes = Themes::builtin();
    for ChooserEntry { id, .. } in themes.chooser_list() {
        let t = themes.resolve(&id);
        let (Some(bg), Some(fg)) = (t.background, t.foreground) else {
            continue; // derives from the desktop; the desktop owns its own contrast
        };
        let c = crate::palette::contrast(fg, bg);
        assert!(
            c >= TEXT,
            "theme {id}: body contrast {c:.2} is below WCAG AA"
        );
    }
}

/// TDD 18.8 / 18.17 — a decoration LINE (a heading rule, a link underline, a strike)
/// is a graphic, not text, so it answers to WCAG's 3:1 non-text floor rather than
/// body prose's 4.5:1. Stated separately from the ink check below because reading a
/// rule at the text floor would rule out every legitimate hairline accent, and a
/// rule nobody can see is the other failure.
#[test]
fn every_theme_decoration_line_clears_the_non_text_contrast_floor() {
    let themes = Themes::builtin();
    for ChooserEntry { id, .. } in themes.chooser_list() {
        let t = themes.resolve(&id);
        let Some(bg) = t.background else { continue };
        // Every decoration LINE a theme may colour. The heading rule's overline has
        // no colour key at all (see `HeadingRule`), so it is absent here by
        // construction — its ink is the heading's, already held to the stricter text
        // floor below.
        // The heading rule is per level, so every level's ink is checked — a theme
        // that rules one level in an unreadable colour must fail here whichever
        // level it chose.
        let mut inks: Vec<(String, Option<gdk::RGBA>)> = (0..HEADING_LEVELS)
            .map(|l| {
                (
                    format!("heading rule h{}", l + 1),
                    t.heading_rule.underline_color[l],
                )
            })
            .collect();
        inks.push(("link underline".to_string(), t.link_underline_color));
        inks.push(("strikethrough".to_string(), t.strikethrough_color));
        for (what, ink) in inks {
            let Some(ink) = ink else { continue };
            let c = crate::palette::contrast(ink, bg);
            assert!(
                c >= GRAPHIC,
                "theme {id}: {what} {} on page {} is {c:.2}:1, below the \
                     3:1 non-text floor",
                crate::palette::to_hex_opaque(ink),
                crate::palette::to_hex_opaque(bg)
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
    for ChooserEntry { id, .. } in themes.chooser_list() {
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
            //
            // **Every surface the band can actually be, not just its fill.** A band
            // has three appearances and this used to read `fills[level]` alone: a
            // theme with a dark fill and a PALE second stop passed on the fill and was
            // unreadable across the bottom half of its own band, and where a SPRITE
            // outranks the fill the gate was measuring a colour that is never painted.
            //
            // **Through the SHARED resolver**, not a second copy of the same match. The
            // two sweeps had one each and they had already drifted: this one read the
            // stops RAW while the disclosure sweep composited them over the page, so a
            // translucent band gave the same colour two different verdicts depending on
            // which decoration was asking (F-AP-B-303). One function, one answer.
            for behind in band_surfaces(&t.heading_band_decor(level), bg) {
                let c = crate::palette::contrast(*ink, behind);
                assert!(
                    c >= TEXT,
                    "theme {id}: h{} ink {} on {} is {c:.2}:1, below WCAG AA",
                    level + 1,
                    crate::palette::to_hex_opaque(*ink),
                    crate::palette::to_hex_opaque(behind)
                );
            }
        }
    }
}

#[test]
fn sepia_body_contrast_is_wcag_aaa() {
    let t = Themes::builtin().resolve("sepia");
    let c = crate::palette::contrast(t.foreground.unwrap(), t.background.unwrap());
    assert!(c >= 7.0, "sepia body contrast {c:.2} should be AAA");
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
    assert!(t.link_color.is_none());
    assert!(t.code_inline_bg.is_none());
    assert!(t.blockquote_bar_color.is_none());
}

// ── the ink table ─────────────────────────────────────────────────────────────
//
// The three gates above are hand-written lists, and between them they covered
// `background`/`foreground`, `heading_colors[]`, `heading_rule.underline_color[]`,
// `link_underline_color` and `strikethrough_color` — and NONE of `rule_color`,
// `mark_fg`, `table_head_fg`, `list_marker_color`, `annotation_chip_fg`, `link_color`
// or `blockquote_bar_color`. Six of those seven carried a hand-computed ratio in a
// `data/themes.toml` COMMENT and nothing else, which is a number a human wrote once
// and no gate has ever read.
//
// `link_color` is the sharpest: it is body-adjacent TEXT, Pixel Quest's is deliberately
// below the floor, and because nothing checked it a future theme's accidental
// low-contrast link would be indistinguishable from Pixel Quest's deliberate one.

/// One legibility pair: an ink, the surface it is actually read on, and the floor that
/// surface's role answers to.
struct Pair {
    what: String,
    ink: gdk::RGBA,
    surface: gdk::RGBA,
    floor: f64,
}

/// WCAG's non-text floor. A decoration is a **graphic**: a rule, a quote bar, a bullet
/// or a checkbox has to be *seen*, not *read*, and holding a hairline accent to the
/// text floor would rule out every legitimate one.
///
/// Local, unlike [`TEXT`] beside it, and that asymmetry is stated rather than tidied:
/// `palette` derives ink against the TEXT floor and has no use for the graphic one, so
/// a `pub(crate)` constant for it there would be dead code in the non-test build and
/// fail the `-D warnings` gate. It lives where its only consumer is.
const GRAPHIC: f64 = 3.0;
/// WCAG AA for body-sized text — **`palette`'s own constant**, not a copy of it.
///
/// The walk `palette` lifts an ink with and the floor this sweep holds a shipped theme
/// to must be the same number, or the derivation can satisfy itself while failing the
/// audit beside it.
const TEXT: f64 = crate::palette::WCAG_AA_TEXT;

/// The pairs that are **deliberately** below their floor, each with the reason.
///
/// A named allow-list rather than an omission: an exception that is merely unmeasured
/// is indistinguishable from a defect, which is this whole finding. Adding a row here
/// is a decision someone has to write down.
///
/// **Reviewed as a whole and ratified by the operator 2026-08-27** — recorded because a
/// list like this otherwise accretes one row at a time and no one ever reads it back.
/// The rows do NOT all mean the same thing, and the table cannot say so itself:
///
/// - **Terminal `rule_color`** — settled. The ANSI 8 reproduction is the point.
/// - **Bedtime `mark_fg`, `rule_color`** — settled *for this project*: that palette is
///   owned by a different operator and is not ours to retune.
///
/// That ratification also covered **Pixel Quest `link_color` and `list_task_color`**,
/// which were **postponed, not settled** — and both have since been RETIRED, by moving
/// that theme's page rather than by relaxing anything: the pale page cleared them at
/// 6.08:1 and 4.05:1 and the rows came out. Recorded here because the count is the part
/// a reader checks against, and this paragraph claimed five rows over a table of three
/// for as long as it stood. The mechanism is the standing lesson, not the two colours:
/// a licence over an open question is retired by closing the question.
const DELIBERATE: &[(&str, &str, &str)] = &[
    (
        "bedtime",
        "mark_fg",
        "chosen knowingly by the operator over a brighter band that measured 7.31:1, because the two-green pairing is the look wanted; it clears the 3:1 large-text floor. The reasoning is at data/themes.toml's `mark_fg`, which asks a future reader not to 'fix' it by accident",
    ),
    (
        "bedtime",
        "rule_color",
        "page FURNITURE, deliberately neutral because it belongs to the grey page rather than to the warm ink; a section rule that shouted would break the theme's whole premise",
    ),
    (
        "terminal",
        "rule_color",
        "ANSI 8 grey on true black is the period palette this theme reproduces — 2.82:1, a hair under the graphic floor, and any correction stops it being ANSI 8",
    ),
];

/// Composite `c` over `under`, so a translucent wash is measured as the colour a reader
/// actually sees rather than as its own unpainted value.
fn over(c: gdk::RGBA, under: gdk::RGBA) -> gdk::RGBA {
    crate::palette::mix_rgba(under, c, f64::from(c.alpha()))
}

/// Every ink a theme states, paired with the surface it is read on.
///
/// Driven off the MODEL rather than written out: a key added to `Theme` and not to this
/// function is a key with no legibility gate, and the point of the table is that the
/// list of gated inks and the list of stated inks are the same list.
fn ink_pairs(t: &Theme, page: gdk::RGBA) -> Vec<Pair> {
    let mut out: Vec<Pair> = Vec::new();
    let mut push = |what: &str, ink: Option<gdk::RGBA>, surface: gdk::RGBA, floor: f64| {
        if let Some(ink) = ink {
            out.push(Pair {
                what: what.to_string(),
                ink,
                surface,
                floor,
            });
        }
    };
    push("foreground", t.foreground, page, TEXT);
    push("link_color", t.link_color, page, TEXT);
    push(
        "link_underline_color",
        t.link_underline_color,
        page,
        GRAPHIC,
    );
    push("strikethrough_color", t.strikethrough_color, page, GRAPHIC);
    push("rule_color", t.rule_color, page, GRAPHIC);
    push(
        "blockquote_bar_color",
        t.blockquote_bar_color,
        page,
        GRAPHIC,
    );
    // A bullet, a numeral and a checkbox are drawn marks, held to the graphic floor.
    push("list_marker_color", t.list_marker_color, page, GRAPHIC);
    push("list_task_color", t.list_task_color, page, GRAPHIC);
    // Quoted text sits on the quote's own panel where the theme states one.
    let quote_surface = t.blockquote_bg.map(|bg| over(bg, page)).unwrap_or(page);
    push("blockquote_fg", t.blockquote_fg, quote_surface, TEXT);
    // A mark's ink is read on its own wash, composited over the page.
    push("mark_fg", t.mark_fg, over(t.mark_bg.rgba(), page), TEXT);
    // A table header's ink is read on the header fill.
    let head = t.table_head_bg.map(|bg| over(bg, page)).unwrap_or(page);
    push("table_head_fg", t.table_head_fg, head, TEXT);
    // The chip's numeral is read on the chip.
    let chip = t
        .annotation_chip_bg
        .map(|bg| over(bg, page))
        .unwrap_or(page);
    push("annotation_chip_fg", t.annotation_chip_fg, chip, TEXT);
    // ── the disclosure summary line ───────────────────────────────────────────
    //
    // All three of its inks — the label, the collapsed-body preview beside it and the
    // indicator in front of it — are read on the summary BAND wherever the theme states
    // one (TDD 18.48), not on the page. The same correction the heading sweep above
    // makes for a banded heading, and it matters in the same direction: a band picked to
    // carry a pale label is exactly the change a page-only check sails past. This ink
    // used to be measured against the page with a comment saying there was no panel
    // behind a summary line, which was true until the band key existed.
    for behind in summary_surfaces(t, page) {
        push("disclosure_fg", t.disclosure_fg, behind, TEXT);
        push(
            "disclosure_preview_fg",
            t.disclosure_preview_fg,
            behind,
            TEXT,
        );
        // An indicator is a drawn mark, held to the non-text floor like every other
        // marker here. Measured whatever the winning rung is: a sprite indicator ignores
        // this key, but the key still inks the GLYPH the sprite falls back to, so it is
        // read by somebody on every theme that states it.
        push(
            "disclosure_marker_color",
            t.disclosure_marker_color,
            behind,
            GRAPHIC,
        );
    }
    out
}

/// Every surface text drawn on `decor` can be read on.
///
/// The page where the theme bands nothing; the band's fill where it bands flat; BOTH
/// endpoints where it bands with a gradient, since a gradient is legible only if its
/// whole run is. Every one composited OVER the page, so a translucent wash is measured
/// as the colour a reader actually sees rather than as its own unpainted value.
///
/// **One resolver for both sweeps** — the heading band's and the disclosure summary's.
/// They were two copies of this match and had drifted on exactly that compositing step,
/// which meant one colour could pass one gate and fail the other (F-AP-B-303).
///
/// A **sprite** band yields the PAGE. Its own pixels are arbitrary and no ratio computed
/// over a fill says anything about reading text on one, so the band surface is not
/// measured — but a sprite that fails to decode degrades to the page
/// (`codeview::bandpaint`), so the page is the surface the ink must clear regardless.
/// Returning nothing instead dropped every ink on such a theme out of the sweep
/// entirely, which is a gate silently switching itself off (F-AP-B-302).
fn band_surfaces(decor: &crate::theme::Band<'_>, page: gdk::RGBA) -> Vec<gdk::RGBA> {
    if decor.sprite.is_some() {
        return vec![page];
    }
    match decor.without_sprite() {
        Some(crate::theme::BandPaint::Gradient { from, to }) => {
            vec![over(from, page), over(to, page)]
        }
        Some(crate::theme::BandPaint::Flat(c)) => vec![over(c, page)],
        None => vec![page],
    }
}

/// Every surface a disclosure's summary line can be read on — see [`band_surfaces`].
fn summary_surfaces(t: &Theme, page: gdk::RGBA) -> Vec<gdk::RGBA> {
    band_surfaces(&t.disclosure_band_decor(), page)
}

/// **Every ink a shipped theme states clears its floor on the surface it is read on.**
///
/// Table-driven, so a key that gains an ink gains a gate. The exceptions are named in
/// [`DELIBERATE`] and nowhere else.
#[test]
fn every_stated_ink_clears_its_floor_on_the_surface_it_is_read_on() {
    let themes = Themes::builtin();
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for ChooserEntry { id, .. } in themes.chooser_list() {
        let t = themes.resolve(&id);
        let Some(page) = t.background else {
            continue; // derives from the desktop; the desktop owns its own contrast
        };
        for pair in ink_pairs(&t, page) {
            if DELIBERATE
                .iter()
                .any(|(theme, what, _)| *theme == id.as_str() && *what == pair.what)
            {
                continue;
            }
            checked += 1;
            let c = crate::palette::contrast(pair.ink, pair.surface);
            if c < pair.floor {
                failures.push(format!(
                    "{id}: {} {} on {} is {c:.2}:1, below its {:.1}:1 floor",
                    pair.what,
                    crate::palette::to_hex_opaque(pair.ink),
                    crate::palette::to_hex_opaque(pair.surface),
                    pair.floor
                ));
            }
        }
    }
    assert!(
        checked > 20,
        "the sweep measured only {checked} pairs — a table that stopped finding inks \
         passes for the wrong reason"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every named exception is still real. A theme that no longer states the key, or that
/// has since been corrected, leaves a licence standing over nothing — and the next
/// theme to state that key inherits it silently.
#[test]
fn every_deliberate_exception_is_still_below_its_floor() {
    let themes = Themes::builtin();
    for (id, what, why) in DELIBERATE {
        let t = themes.resolve(id);
        let page = t
            .background
            .unwrap_or_else(|| panic!("{id}: an exception on a theme with no page of its own"));
        // **EVERY pair the key produces, not the first** (F-AP-B-306). One ink on a
        // GRADIENT band is two pairs — one per endpoint — and `find` inspected only the
        // one that happened to come first. A theme that has retuned the OTHER stop back
        // above its floor then keeps a licence standing over nothing, which is the exact
        // invisibility this guard exists to end, one endpoint over.
        let pairs: Vec<_> = ink_pairs(&t, page)
            .into_iter()
            .filter(|p| p.what == *what)
            .collect();
        assert!(
            !pairs.is_empty(),
            "{id}: states no {what}, so the exception is stale"
        );
        for pair in pairs {
            let c = crate::palette::contrast(pair.ink, pair.surface);
            assert!(
                c < pair.floor,
                "{id}: {what} on {} is now {c:.2}:1, at or above its {:.1}:1 floor — \
                 delete the exception rather than leaving a licence over nothing ({why})",
                crate::palette::to_hex_opaque(pair.surface),
                pair.floor
            );
        }
    }
}

/// **The surface resolver's own arms, including the two no shipped theme reaches.**
///
/// No theme in `themes.toml` states a band SPRITE, so the sweep above never enters
/// that arm — and an arm no fixture reaches is exactly where a gate switches itself off
/// unnoticed. It did: returning `Vec::new()` there dropped all three disclosure inks
/// out of the sweep for any theme that stated one (F-AP-B-302).
///
/// The compositing arms are pinned here for the same reason the resolver is shared at
/// all: the heading sweep read its stops RAW and the disclosure sweep composited them,
/// so one colour had two verdicts (F-AP-B-303).
#[test]
fn the_band_surface_resolver_answers_every_arm() {
    use crate::theme::{Band, BandPaint};
    let page = gdk::RGBA::new(0.0, 0.0, 0.0, 1.0);
    let opaque = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
    let half = gdk::RGBA::new(1.0, 1.0, 1.0, 0.5);

    // No band at all: the page is the surface.
    let none = Band {
        sprite: None,
        gradient: None,
        flat: None,
    };
    assert_eq!(band_surfaces(&none, page), vec![page], "no band");

    // A flat band, composited.
    let flat = Band {
        sprite: None,
        gradient: None,
        flat: Some(half),
    };
    assert_eq!(
        band_surfaces(&flat, page),
        vec![over(half, page)],
        "a translucent fill is measured as the colour a reader sees"
    );
    assert_ne!(
        band_surfaces(&flat, page),
        vec![half],
        "and NOT as its own unpainted value — which is the drift this resolver ended"
    );

    // A gradient: both endpoints, both composited.
    let grad = Band {
        sprite: None,
        gradient: Some((half, opaque)),
        flat: Some(half),
    };
    assert_eq!(
        band_surfaces(&grad, page),
        vec![over(half, page), over(opaque, page)],
        "a gradient is legible only if its whole run is"
    );
    assert!(matches!(
        grad.without_sprite(),
        Some(BandPaint::Gradient { .. })
    ));

    // A sprite: the band surface is not measured, but the PAGE still is.
    let sprite = crate::sprite::SpriteRef::Compiled("nothing-in-particular");
    let with_sprite = Band {
        sprite: Some(&sprite),
        gradient: Some((half, opaque)),
        flat: Some(half),
    };
    assert_eq!(
        band_surfaces(&with_sprite, page),
        vec![page],
        "a sprite band still gates its inks against the page it degrades to — an empty \
         list here is a gate switching itself off"
    );
}
