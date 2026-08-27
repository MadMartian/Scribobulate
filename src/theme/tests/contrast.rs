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
