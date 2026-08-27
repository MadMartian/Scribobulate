//! The user-file merge, and the keys whose whole contract is "opt in, or inherit"
//! (TDD 18.13/18.14/18.23/18.29).

use super::super::resolve::*;
use super::super::value::*;
use super::super::*;

/// TDD 18.23 / 18.2 — the strike colour and the link-underline colour are absent
/// under System, and the link underline floors at the SINGLE line the app has always
/// drawn (not at "none", unlike the heading rule — that difference is the whole of
/// what keeps System's links looking as they did).
#[test]
fn strike_and_link_underline_default_to_todays_rendering() {
    let sys = Themes::builtin().resolve(SYSTEM_ID);
    assert!(sys.strikethrough_color.is_none());
    assert!(sys.link_underline_color.is_none());
    assert_eq!(sys.link_underline, LineStyle::Single);
    assert_eq!(
        sys.link_underline.underline(),
        gtk::pango::Underline::Single
    );
    // Sepia states none of them either, so it inherits the same.
    let sep = Themes::builtin().resolve("sepia");
    assert!(sep.strikethrough_color.is_none());
    assert_eq!(sep.link_underline, LineStyle::Single);
}

/// TDD 18.23 — both resolve, and both merge from a user file (`take!`-list guard).
#[test]
fn a_theme_states_the_strike_and_link_underline_colours_independently() {
    // Synthetic rather than a built-in theme's own content on purpose — content is
    // free to change, this contract is not.
    let mut synth = Themes::builtin();
    synth.merge_over(
        Themes::parse_compiled(
            "[themes.sepia]\nlink_color = \"#2de1ff\"\nstrikethrough_color = \"#ff3caf\"\n\
                 link_underline_color = \"#ff3caf\"\n",
        )
        .unwrap(),
    );
    let t = synth.resolve("sepia");
    assert_eq!(
        crate::palette::to_hex(t.strikethrough_color.expect("stated")),
        "#ff3caf"
    );
    // Stated independently of the link's own ink — that separation IS the key.
    assert_eq!(
        crate::palette::to_hex(t.link_underline_color.expect("stated")),
        "#ff3caf"
    );
    assert_ne!(
        crate::palette::to_hex(t.link_color.expect("theme sets a link colour")),
        crate::palette::to_hex(t.link_underline_color.unwrap())
    );

    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.sepia]\nstrikethrough_color = \"#654321\"\n\
                 link_underline = \"wavy\"\nlink_underline_color = \"#abcdef\"\n",
        )
        .unwrap(),
    );
    let sep = themes.resolve("sepia");
    assert_eq!(
        crate::palette::to_hex(sep.strikethrough_color.expect("merged")),
        "#654321"
    );
    assert_eq!(sep.link_underline, LineStyle::Wavy);
    assert_eq!(
        crate::palette::to_hex(sep.link_underline_color.expect("merged")),
        "#abcdef"
    );
    // A link with NO line at all is expressible, and is not the floor.
    let mut off = Themes::builtin();
    off.merge_over(Themes::parse_compiled("[themes.sepia]\nlink_underline = \"none\"\n").unwrap());
    assert_eq!(off.resolve("sepia").link_underline, LineStyle::None);
}

/// TDD 18.29 / 18.2 — the quote panel is opt-in, its two halves are independent of
/// each other, and BOTH are independent of the accent bar's own colour.
///
/// The independence is the rubric, not the parsing: `blockquote_bar` seeded a
/// blockquote's only themed colour until this pair existed, and a fold from it
/// would have made a themed bar silently panel the quote on every existing theme.
#[test]
fn the_quote_panel_is_opt_in_and_independent_of_the_bar() {
    let sys = Themes::builtin().resolve(SYSTEM_ID);
    assert!(sys.blockquote_bg.is_none());
    assert!(sys.blockquote_fg.is_none());

    // A bar colour alone panels nothing — every shipped theme before this pair is
    // in exactly this state.
    let mut bar_only = Themes::builtin();
    bar_only.merge_over(
        Themes::parse_compiled("[themes.sepia]\nblockquote_bar_color = \"#112233\"\n").unwrap(),
    );
    let t = bar_only.resolve("sepia");
    assert_eq!(
        crate::palette::to_hex(t.blockquote_bar_color.unwrap()),
        "#112233"
    );
    assert!(
        t.blockquote_bg.is_none(),
        "a bar colour must not seed a panel"
    );
    assert!(t.blockquote_fg.is_none(), "…nor an ink");

    // Either half alone resolves, leaving the other absent: a panel with the body
    // ink on it, or re-inked quoted text on the page.
    for (key, read) in [("blockquote_bg", true), ("blockquote_fg", false)] {
        let mut one = Themes::builtin();
        one.merge_over(
            Themes::parse_compiled(&format!("[themes.sepia]\n{key} = \"#ff00ff\"\n")).unwrap(),
        );
        let t = one.resolve("sepia");
        let (stated, other) = if read {
            (t.blockquote_bg, t.blockquote_fg)
        } else {
            (t.blockquote_fg, t.blockquote_bg)
        };
        assert_eq!(crate::palette::to_hex(stated.unwrap()), "#ff00ff", "{key}");
        assert!(other.is_none(), "{key} must not imply its counterpart");
        assert!(
            t.blockquote_bar_color.is_none(),
            "{key} must not disturb the bar, which stays whatever the theme said"
        );
    }
}

/// TDD 18.6 — the same key feeds the body tag and the table-cell markup, so a
/// theme's overlay colours can never differ between the two.
#[test]
fn overlay_colours_resolve_per_theme_and_are_never_none() {
    let themes = Themes::builtin();
    let sys = themes.resolve(SYSTEM_ID);
    assert_eq!(sys.annotation_hl_color.hex(), "#ffd133");
    assert_eq!(sys.find_hl_all_color.hex(), "#f6d32d");
    assert_eq!(sys.find_hl_current_color.hex(), "#ff7800");
    assert_eq!(sys.mark_bg.hex(), "#fff59d");
    // Sepia replaces all three, because the system yellows wash out on cream.
    let sep = themes.resolve("sepia");
    assert_ne!(sep.annotation_hl_color.hex(), sys.annotation_hl_color.hex());
    assert_ne!(sep.find_hl_all_color.hex(), sys.find_hl_all_color.hex());
    assert_ne!(
        sep.find_hl_current_color.hex(),
        sys.find_hl_current_color.hex()
    );
    assert_ne!(sep.mark_bg.hex(), sys.mark_bg.hex());
    // Synthwave's highlight is the radioactive toxic green — a deliberate,
    // theme-specific mark colour, distinct from the neutral yellow floor.
    let synth = themes.resolve("synthwave");
    assert_eq!(synth.mark_bg.hex(), "#39ff14");
    assert_ne!(synth.mark_bg.hex(), sys.mark_bg.hex());
    // …and keeps the system's alpha semantics for the wash.
    assert_eq!(sep.annotation_hl_color.alpha_pct(), "38%");
}

/// TDD 18.14 — a new theme is data. Nothing about adding one touches code.
#[test]
fn a_user_file_can_add_a_whole_new_theme() {
    let mut themes = Themes::builtin();
    themes.merge_over(
            Themes::parse_compiled("[themes.slate]\nname = \"Slate\"\nbackground = \"#222222\"\nforeground = \"#dddddd\"\n")
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
    themes
        .merge_over(Themes::parse_compiled("[themes.sepia]\nbackground = \"#fffbe6\"\n").unwrap());
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
    themes.merge_over(
        Themes::parse_compiled("[themes.sepia]\nselection_fg = \"#abcdef\"\n").unwrap(),
    );
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
    themes.merge_over(Themes::parse_compiled("[themes.sepia]\nmark_fg = \"#123456\"\n").unwrap());
    assert_eq!(
        crate::palette::to_hex(themes.resolve("sepia").mark_fg.expect("merged")),
        "#123456"
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
        Themes::parse_compiled(
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

/// A user may also override what the app hardcodes, by overriding
/// [themes.system] — this is what retired config.toml's `[colors]` section.
#[test]
fn a_user_file_overrides_the_system_theme_itself() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.system]\nfind_hl_all_color = \"#00ff00\"\nlist_step = 40\n",
        )
        .unwrap(),
    );
    let sys = themes.resolve(SYSTEM_ID);
    assert_eq!(sys.find_hl_all_color.hex(), "#00ff00");
    assert_eq!(sys.metrics.list_step, 40);
    // …and it reaches every theme that doesn't state its own, per link 2.
    assert_eq!(themes.resolve("sepia").metrics.list_step, 40);
}

/// TDD 18.11 — a malformed file is ignored, not fatal.
#[test]
fn a_malformed_user_file_is_ignored_and_the_builtin_survives() {
    assert!(Themes::parse_compiled("this is not = = valid toml").is_none());
    let mut themes = Themes::builtin();
    if let Some(user) = Themes::parse_compiled("this is not = = valid toml") {
        themes.merge_over(user);
    }
    assert!(themes.contains("sepia"));
    assert_eq!(themes.resolve(SYSTEM_ID).metrics.list_step, F_LIST_STEP);
}
