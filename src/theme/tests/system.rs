//! TDD 18.2 — the regression bar: what the shipped `[themes.system]` block promises,
//! and what a hostile or malformed theme cannot do to it.

use super::super::model::*;
use super::super::resolve::*;
use super::super::*;

fn builtin_system() -> ThemeSpec {
    Themes::parse_compiled(BUILTIN_THEMES_TOML)
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
    assert_eq!(
        r.heading_rule.overline,
        [F_HEADING_OVERLINE; HEADING_LEVELS]
    );
    assert_eq!(
        r.heading_rule.underline,
        [F_HEADING_UNDERLINE; HEADING_LEVELS]
    );
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
    assert!(t.accent_color.is_none());
    assert!(t.font_family.is_none());
    assert!(t.syntect_theme.is_none());
    assert!(t.heading_color.is_none());
    assert!(t.list_marker_color.is_none());
    assert!(t.link_color.is_none());
    assert!(t.code_inline_bg.is_none());
    assert!(t.blockquote_bar_color.is_none());
}

/// TDD 18.11 — out-of-range geometry clamps rather than breaking layout.
#[test]
fn hostile_geometry_is_clamped() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.evil]\nlist_step = -5\nblockquote_bar_width = 10000\n\
                 heading_weight = 99999\nsupsub_scale = -3.0\nsuperscript_rise = 9999\n",
        )
        .unwrap(),
    );
    let t = themes.resolve("evil");
    assert_eq!(t.metrics.list_step, LIST_STEP_RANGE.0);
    assert_eq!(t.metrics.blockquote_bar_width, METRIC_RANGE.1);
    assert_eq!(
        t.typography.heading_weight,
        [WEIGHT_RANGE.1; HEADING_LEVELS]
    );
    assert_eq!(t.typography.supsub_scale, SCALE_RANGE.0);
    assert_eq!(t.typography.superscript_rise, RISE_RANGE.1);
}

/// A theme file cannot kill the app with a value of the wrong shape.
///
/// The vocabulary carries no arrays at all now, so the length hazard this once
/// guarded is gone by construction; what remains is the two ways a scalar can be
/// wrong. A value of the wrong TYPE costs its own key and nothing else — the file
/// still loads and every other key in that theme applies — and a non-finite float
/// is clamped rather than propagated into Pango.
#[test]
fn a_malformed_value_costs_its_own_key_and_never_the_theme() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.wrong]\nheading_scale_h1 = \"enormous\"\nheading_scale_h2 = 2.0\n\
                 [themes.nan]\nheading_scale_h1 = nan\nheading_scale_h2 = inf\n",
        )
        .unwrap(),
    );
    // The refused key falls back to its floor; its neighbour is untouched.
    let wrong = themes.resolve("wrong").typography.heading_scale;
    assert_eq!(wrong[0], F_HEADING_SCALE[0]);
    assert_eq!(wrong[1], 2.0);
    // Non-finite: clamped, never propagated into Pango.
    let n = themes.resolve("nan").typography.heading_scale;
    assert!(n.iter().all(|x| x.is_finite()));
    // A level nobody states keeps the system hierarchy, which is what the short
    // array used to buy by extending from the floor.
    assert_eq!(&n[2..], &F_HEADING_SCALE[2..]);
}

/// TDD 18.11 — a colour cannot escape a generated CSS rule, because it is
/// re-emitted from a parsed RGBA rather than echoed.
#[test]
fn a_hostile_colour_string_cannot_inject_css() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled("[themes.evil]\nbackground = \"#fff; } * { color: red; }\"\n")
            .unwrap(),
    );
    // Unparseable → falls through to the desktop probe; nothing is interpolated.
    assert!(themes.resolve("evil").background.is_none());
}

/// A stale persisted selection (a theme the user deleted) must degrade to the
/// default, not fail.
#[test]
fn an_unknown_theme_id_resolves_as_system() {
    let t = Themes::builtin().resolve("no-such-theme");
    assert!(t.background.is_none());
    assert_eq!(t.typography.heading_scale, F_HEADING_SCALE);
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
