//! Heading keys: the ink, the face, the rule and the band — each stated bare for
//! every level or narrowed to one (TDD 18.21/18.22/18.25/18.32).

use super::super::resolve::*;
use super::super::value::*;
use super::super::*;

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
        Themes::parse_compiled(
            "[themes.sepia]\nheading_color = \"#334455\"\nheading_font = \"Georgia, serif\"\n\
                 heading_color_h1 = \"#ff3caf\"\n\
                 heading_font_h1 = \"Michroma, sans-serif\"\n",
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
        Themes::parse_compiled(
            "[themes.sepia]\nheading_color_h2 = \"#123456\"\n\
                 heading_font_h2 = \"Georgia, serif\"\n",
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
    // A level the user narrowed nothing for stays unset (sepia states no bare
    // heading colour either).
    assert!(sep.heading_colors[0].is_none());
    assert!(sep.heading_colors[4].is_none());
}

/// A level a theme fills with nonsense must FALL BACK, never reject the theme —
/// the same clamp-don't-reject discipline every geometry key follows (TDD 18.11).
#[test]
fn an_unparseable_heading_level_falls_back_to_the_bare_key() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.synthwave]\nheading_color_h1 = \"not a colour\"\n\
                 heading_font_h1 = \"}} * {{ color: red; }}\"\n",
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

/// TDD 18.22 / 18.2 — the heading rule is INERT until a theme asks for it, and the
/// space above it is zero, so System registers exactly the heading tag it always did.
#[test]
fn the_heading_rule_and_space_above_are_absent_under_system() {
    let sys = Themes::builtin().resolve(SYSTEM_ID);
    for level in 0..HEADING_LEVELS {
        assert!(sys.heading_rule.is_absent_at(level));
    }
    assert!(sys.heading_rule.underline_color.iter().all(Option::is_none));
    assert_eq!(sys.metrics.heading_space_above, [0; HEADING_LEVELS]);
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
        Themes::parse_compiled(
            "[themes.sepia]\nheading_underline = \"single\"\n\
                 heading_underline_color = \"#3e6fa0\"\n\
                 heading_space_above_h1 = 16\nheading_space_above_h2 = 12\n\
                 heading_space_above_h3 = 8\nheading_space_above_h4 = 6\n\
                 heading_space_above_h5 = 6\n",
        )
        .unwrap(),
    );
    let t = synth.resolve("sepia");
    // The bare key reaches every level (TDD 18.32), so all five carry the rule.
    assert_eq!(
        t.heading_rule.underline,
        [LineStyle::Single; HEADING_LEVELS]
    );
    assert_eq!(
        crate::palette::to_hex(t.heading_rule.underline_color[0].expect("stated")),
        "#3e6fa0"
    );
    // This theme states no overline, so that side stays off.
    assert_eq!(t.heading_rule.overline, [LineStyle::None; HEADING_LEVELS]);
    assert_eq!(t.metrics.heading_space_above, [16, 12, 8, 6, 6]);

    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.sepia]\nheading_overline = \"double\"\n\
                 heading_underline = \"wavy\"\n\
                 heading_underline_color = \"#222222\"\nheading_space_above_h1 = 7\n",
        )
        .unwrap(),
    );
    let sep = themes.resolve("sepia");
    // The overline CLAMPS: Pango's attribute has only none/single, so a theme asking
    // for a double rule above gets a single one rather than a rejected theme.
    assert_eq!(
        sep.heading_rule.overline,
        [LineStyle::Double; HEADING_LEVELS]
    );
    assert_eq!(
        sep.heading_rule.overline[0].overline(),
        gtk::pango::Overline::Single
    );
    assert_eq!(
        sep.heading_rule.underline,
        [LineStyle::Wavy; HEADING_LEVELS]
    );
    assert_eq!(
        sep.heading_rule.underline[0].underline(),
        gtk::pango::Underline::Error
    );
    assert_eq!(
        crate::palette::to_hex(sep.heading_rule.underline_color[0].expect("merged")),
        "#222222"
    );
    // A level the theme narrows nothing for keeps the key's own floor (TDD 18.32),
    // which is what a short array used to buy by extending from it.
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
    themes.merge_over(
        Themes::parse_compiled("[themes.sepia]\nheading_underline = \"zigzag\"\n").unwrap(),
    );
    assert_eq!(
        themes.resolve("sepia").heading_rule.underline,
        [F_HEADING_UNDERLINE; HEADING_LEVELS]
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
    assert!(sys.sprites.heading_band.iter().all(Option::is_none));

    let mut shape_only = Themes::builtin();
    shape_only.merge_over(
        Themes::parse_compiled(
            "[themes.sepia]\nheading_band_radius = 12\n\
                 heading_band_gradient_to_color = \"#ffffff\"\n",
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
        Themes::parse_compiled(
            "[themes.sepia]\nheading_band_color_h1 = \"#6c2a92\"\n\
                 heading_band_color_h2 = \"#9e1449\"\n\
                 heading_band_gradient_to_color = \"#101a4d\"\nheading_band_radius = 8\n",
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
    // The gradient stop is stated bare, so it reaches every level.
    assert!(t.heading_band.gradient_to.iter().all(Option::is_some));
    assert_eq!(t.metrics.heading_band_radius, [8; HEADING_LEVELS]);

    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.sepia]\nheading_band_color_h2 = \"#abcdef\"\n\
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
    assert_eq!(
        sep.metrics.heading_band_radius,
        [METRIC_RANGE.1; HEADING_LEVELS]
    );
}
