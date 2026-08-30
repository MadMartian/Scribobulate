//! The palette's own tests, split out at the 500-line soft limit.
//!
//! Split by AUDIENCE rather than by subject: everything here drives the module beside
//! it, and the alternative — splitting the production half by cause — would have moved
//! the GTK probe away from the derivations it feeds, which are read together.

use super::{
    contrast, luminance, mix_rgba, page_floor_for_ink, syntect_color_to_rgba, to_hex_opaque,
    walk_to_contrast, Palette, DARK_PAGE_FLOOR, LIGHT_PAGE_FLOOR, WCAG_AA_TEXT,
};
use gtk::gdk;

const BLACK: gdk::RGBA = gdk::RGBA::BLACK;
const WHITE: gdk::RGBA = gdk::RGBA::WHITE;

#[test]
fn syntect_color_maps_channels_and_forces_opaque() {
    let c = syntect::highlighting::Color {
        r: 255,
        g: 0,
        b: 128,
        a: 0,
    };
    let rgba = syntect_color_to_rgba(c);
    assert_eq!(rgba.red(), 1.0);
    assert_eq!(rgba.green(), 0.0);
    assert!((rgba.blue() - 128.0 / 255.0).abs() < 1e-6);
    // Alpha is forced opaque regardless of the source `a`.
    assert_eq!(rgba.alpha(), 1.0);
}

#[test]
fn to_hex_formats_channels() {
    assert_eq!(to_hex_opaque(BLACK), "#000000");
    assert_eq!(to_hex_opaque(WHITE), "#ffffff");
    assert_eq!(to_hex_opaque(gdk::RGBA::new(1.0, 0.0, 0.0, 1.0)), "#ff0000");
    // Out-of-range channels clamp rather than overflow.
    assert_eq!(
        to_hex_opaque(gdk::RGBA::new(2.0, -1.0, 0.0, 1.0)),
        "#ff0000"
    );
}

#[test]
fn luminance_endpoints() {
    assert!((luminance(BLACK) - 0.0).abs() < 1e-9);
    assert!((luminance(WHITE) - 1.0).abs() < 1e-9);
    // Mid-grey sits between the endpoints.
    let g = luminance(gdk::RGBA::new(0.5, 0.5, 0.5, 1.0));
    assert!(g > 0.0 && g < 1.0);
}

#[test]
fn contrast_black_on_white_is_maximal() {
    // WCAG contrast ratio of pure black vs pure white is 21:1.
    assert!((contrast(BLACK, WHITE) - 21.0).abs() < 1e-6);
    // Contrast is symmetric.
    assert!((contrast(WHITE, BLACK) - contrast(BLACK, WHITE)).abs() < 1e-9);
    // A color against itself has ratio 1.
    assert!((contrast(WHITE, WHITE) - 1.0).abs() < 1e-9);
}

#[test]
fn mix_rgba_interpolates() {
    // t=0 yields a, t=1 yields b.
    assert_eq!(to_hex_opaque(mix_rgba(BLACK, WHITE, 0.0)), "#000000");
    assert_eq!(to_hex_opaque(mix_rgba(BLACK, WHITE, 1.0)), "#ffffff");
    // Midpoint of black→white is mid-grey (0.5*255 = 127.5 → 128 = 0x80).
    assert_eq!(to_hex_opaque(mix_rgba(BLACK, WHITE, 0.5)), "#808080");
}

/// Selected text stays legible on every shipped theme's selection fill.
///
/// The bug this pins: styling the selection's background alone left its
/// foreground to the desktop GTK theme, which painted selected text `#000000`
/// at 2.1:1 on Bedtime's fill — measured on screen, not theorised. The
/// derivation picks between the page ink and the page itself, so the assertion
/// is on the RATIO rather than on a literal: a future theme is free to choose
/// any fill, and this fails if that choice would strand its selected text.
#[test]
fn selected_text_clears_the_legibility_floor_on_every_theme() {
    let themes = crate::theme::Themes::builtin();
    for crate::theme::ChooserEntry { id, .. } in themes.chooser_list() {
        let t = themes.resolve(&id);
        // Only a theme that states its own page emits a selection rule at all;
        // System defers the whole block to the desktop (TDD 18.2).
        let (Some(bg), Some(fg)) = (t.background, t.foreground) else {
            continue;
        };
        let p = Palette::from_base(bg, fg, fg, t.accent_color.unwrap_or(fg), &t);
        let c = contrast(p.selection_fg, p.selection_bg);
        assert!(
            c >= 4.5,
            "{id}: selected text {} on selection fill {} is {c:.2}:1",
            to_hex_opaque(p.selection_fg),
            to_hex_opaque(p.selection_bg)
        );
        assert_ne!(
            to_hex_opaque(p.selection_fg),
            to_hex_opaque(p.selection_bg),
            "{id}: selected text drawn in the fill's own colour"
        );
    }
}

/// **The single WCAG walk** lifts an ink to the AA floor against either polarity of
/// fill, and leaves an ink that already clears the floor alone.
///
/// Both derived inks in this module — the link colour and the selection ink — go
/// through this one function now, so this is the guard for both. The direction
/// assertions are what make it a guard rather than a smoke test: a walk that moved
/// toward the wrong endpoint would still terminate, just never legibly.
#[test]
fn the_contrast_walk_lifts_ink_to_the_aa_floor_from_either_polarity() {
    // Mid-grey on white is ~2.85:1 — below AA, and walked DOWN toward black.
    let dim = gdk::RGBA::new(0.6, 0.6, 0.6, 1.0);
    assert!(contrast(dim, WHITE) < WCAG_AA_TEXT);
    let lifted = walk_to_contrast(dim, WHITE);
    assert!(contrast(lifted, WHITE) >= WCAG_AA_TEXT);
    assert!(
        luminance(lifted) < luminance(dim),
        "walked toward white on a light fill"
    );

    // Dark grey on black is ~1.66:1 — below AA, and walked UP toward white.
    let murky = gdk::RGBA::new(0.2, 0.2, 0.2, 1.0);
    assert!(contrast(murky, BLACK) < WCAG_AA_TEXT);
    let lifted = walk_to_contrast(murky, BLACK);
    assert!(contrast(lifted, BLACK) >= WCAG_AA_TEXT);
    assert!(
        luminance(lifted) > luminance(murky),
        "walked toward black on a dark fill"
    );

    // An ink that ALREADY clears the floor is returned untouched — the walk stops
    // at the floor rather than running its full count, which would darken a
    // perfectly legible ink for nothing.
    let legible = gdk::RGBA::new(0.3, 0.3, 0.3, 1.0); // 8.5:1 on white
    assert_eq!(
        to_hex_opaque(walk_to_contrast(legible, WHITE)),
        to_hex_opaque(legible)
    );
}

/// A desktop that names an ink but no page still yields a legible pair: the page
/// floor is the ink's opposite, never a fixed colour.
#[test]
fn the_page_floor_opposes_the_ink_the_probe_answered_with() {
    assert_eq!(page_floor_for_ink(WHITE), DARK_PAGE_FLOOR);
    assert_eq!(page_floor_for_ink(BLACK), LIGHT_PAGE_FLOOR);
}

/// `probe_named`'s two rules, against a real GTK style context — the chain is
/// ordered, and the floor is what "none of them names a colour" resolves to.
#[cfg(feature = "gtk-integration-tests")]
mod probe {
    use super::super::{probe_named, BODY_INK_NAMES};
    use gtk::gdk;

    /// A display-wide CSS provider, removed again when this value drops. A provider
    /// on the display is PROCESS-global state and libtest runs the whole suite in
    /// one process, so it must come off even on a panic (POLICY § Unit tests).
    struct DisplayCss(gtk::CssProvider);

    impl DisplayCss {
        fn install(css: &str) -> Self {
            let display = gdk::Display::default().expect("these tests need a display");
            let provider = gtk::CssProvider::new();
            provider.load_from_data(css);
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
            Self(provider)
        }
    }

    impl Drop for DisplayCss {
        fn drop(&mut self) {
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &self.0);
            }
        }
    }

    /// A colour no probe chain in this module can reach, so an answer of this value
    /// can only have come from the fall-through.
    const FLOOR: gdk::RGBA = gdk::RGBA::new(0.25, 0.5, 0.75, 1.0);

    /// The FIRST name the theme defines wins, and a name it does not define is
    /// skipped rather than ending the walk.
    #[gtktest::test]
    fn probe_named_takes_the_first_name_the_theme_defines() {
        let _css = DisplayCss::install(
            "@define-color scribo_probe_a #ff0000; @define-color scribo_probe_b #00ff00;",
        );
        assert_eq!(
            probe_named(&["scribo_probe_a", "scribo_probe_b"], FLOOR),
            gdk::RGBA::RED
        );
        assert_eq!(
            probe_named(&["scribo_probe_absent", "scribo_probe_b"], FLOOR),
            gdk::RGBA::GREEN
        );
    }

    /// The "none of them names a colour" contract this seam owns — with a positive
    /// control, because a probe that answered with the floor unconditionally would
    /// satisfy the fall-through assertions on its own.
    #[gtktest::test]
    fn probe_named_answers_with_its_floor_when_nothing_names_a_colour() {
        assert_eq!(probe_named(&["scribo_probe_absent"], FLOOR), FLOOR);
        assert_eq!(probe_named(&[], FLOOR), FLOOR);
        let _css = DisplayCss::install("@define-color theme_text_color #ff0000;");
        assert_eq!(probe_named(BODY_INK_NAMES, FLOOR), gdk::RGBA::RED);
    }
}

/// TDD 25.9 — **paper has no dark mode**, on the fall-through branch specifically.
///
/// `Palette::for_paper` honours a theme's own stated page and forces the *unstated*
/// case light; the honoured half is pinned beside the PDF sink, and this is the other
/// half. Asserted as EXACT equality with the paper floor rather than as "light
/// enough", because the branch's whole content is that it takes the constant instead
/// of asking the desktop probe — and a probe answer can be light too, so a luminance
/// threshold passes on a build that consults it.
///
/// Driven from a theme that states a dark INK and no page at all: `from_base` then
/// has an ink whose own polarity would pull a screen palette dark, so the assertion
/// is about `for_paper`'s choice and not about the fixture being colourless.
#[test]
fn an_unstated_page_resolves_to_paper_and_never_to_the_desktops() {
    let mut themes = crate::theme::Themes::builtin();
    themes.merge_over_for_test("[themes.inkonly]\nforeground = \"#101014\"\n");
    let t = themes.resolve("inkonly");
    assert!(
        t.background.is_none(),
        "the fixture must state no page; that premise is the test's subject"
    );
    let paper = Palette::for_paper(&t);
    assert_eq!(
        to_hex_opaque(paper.page_bg),
        to_hex_opaque(LIGHT_PAGE_FLOOR),
        "an unstated page must resolve to the paper floor, not to whatever the \
         desktop probe answers"
    );
    assert!(
        contrast(paper.body_fg, paper.page_bg) >= WCAG_AA_TEXT,
        "the theme's own dark ink must still clear AA on that paper"
    );
}
