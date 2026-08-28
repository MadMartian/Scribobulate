//! List markers: the shared colour, the per-kind glyphs and sprites, and the
//! bullet’s nesting-depth tiers (TDD 18.15/18.24/18.26/18.27).

use super::super::model::*;
use super::super::*;

/// TDD 18.15 — a theme can colour the list-marker glyph (bullet/numeral/checkbox)
/// independently of the item text; omitted, `list_marker` stays `None` so markers
/// inherit the widget foreground (System byte-identical). One key, all three kinds.
#[test]
fn list_marker_is_opt_in() {
    assert!(Themes::builtin()
        .resolve(SYSTEM_ID)
        .list_marker_color
        .is_none());
    assert!(Themes::builtin()
        .resolve("sepia")
        .list_marker_color
        .is_none());
    let term = Themes::builtin().resolve("terminal");
    assert_eq!(
        crate::palette::to_hex_opaque(term.list_marker_color.expect("terminal sets it")),
        "#55ff55"
    );
    let sw = Themes::builtin().resolve("synthwave");
    assert_eq!(
        crate::palette::to_hex_opaque(sw.list_marker_color.expect("synthwave sets it")),
        "#ff3caf"
    );
}

/// TDD 18.24 / 18.2 — every marker key is absent under System, and resolves and
/// merges from a user file, eight keys' worth.
///
/// The merge half no longer guards a per-key merge LIST — `overlay` is one `extend`
/// over a spelling map and a key cannot be omitted from it. What it still proves is the
/// surviving obligation: that each key reaches the resolved `Theme` at all, which is
/// `F-SINK-001`'s hazard and is narrower than the one this docstring used to name.
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
    themes
        .merge_over(Themes::parse_compiled("[themes.sepia]\nlist_bullet_glyph = \"❧\"\n").unwrap());
    assert_eq!(
        themes.resolve("sepia").list_glyphs.bullet[0]
            .as_ref()
            .unwrap()
            .as_plain(),
        "❧"
    );
}

/// TDD 18.26 / 18.2 — with no depth key stated, every tier carries the un-suffixed
/// key's value, which is what makes the feature inert: a theme that says nothing
/// paints exactly as it did before the tiers existed.
#[test]
fn every_bullet_tier_inherits_the_unsuffixed_key_by_default() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.sepia]\nlist_marker_color = \"#112233\"\nlist_bullet_glyph = \"a\"\n",
        )
        .unwrap(),
    );
    let t = themes.resolve("sepia");
    for tier in 0..BULLET_TIERS {
        assert_eq!(
            crate::palette::to_hex_opaque(t.list_bullet_colors[tier].expect("inherited")),
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
        Themes::parse_compiled(
            "[themes.sepia]\nlist_marker_color = \"#111111\"\nlist_marker_color_2 = \"#222222\"\n\
                 list_bullet_glyph = \"a\"\nlist_bullet_glyph_2 = \"b\"\n",
        )
        .unwrap(),
    );
    let t = themes.resolve("sepia");
    let hex = |i: usize| crate::palette::to_hex_opaque(t.list_bullet_colors[i].unwrap());
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
        Themes::parse_compiled(
            "[themes.sepia]\nlist_marker_color = \"#111111\"\nlist_marker_color_3 = \"#333333\"\n",
        )
        .unwrap(),
    );
    let t3 = only3.resolve("sepia");
    assert_eq!(
        crate::palette::to_hex_opaque(t3.list_bullet_colors[0].unwrap()),
        "#111111"
    );
    assert_eq!(
        crate::palette::to_hex_opaque(t3.list_bullet_colors[1].unwrap()),
        "#111111"
    );
    assert_eq!(
        crate::palette::to_hex_opaque(t3.list_bullet_colors[2].unwrap()),
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
        Themes::parse_compiled(
            "[themes.sepia]\nlist_marker_color = \"#111111\"\nlist_marker_color_2 = \"#222222\"\n",
        )
        .unwrap(),
    );
    let t = themes.resolve("sepia");
    // **The markers themselves, at depth, not the bare key.** The only assertion here
    // used to be on `list_marker_color`, which the depth keys could not affect under
    // ANY implementation — so this test passed with the rule broken, and SCHEMA's Lists
    // ⚠️ callout is the one list contract a reader is most likely to get backwards.
    use crate::theme::MarkerKind;
    let hex = |kind: MarkerKind, depth: usize| {
        crate::palette::to_hex_opaque(t.marker_ink(kind, depth).expect("stated"))
    };
    for depth in [1usize, 2, 3, 9] {
        assert_eq!(
            hex(MarkerKind::Ordered, depth),
            "#111111",
            "an ordered numeral at depth {depth} must keep the shared colour"
        );
        assert_eq!(
            hex(MarkerKind::Task, depth),
            "#111111",
            "a task box at depth {depth} must keep the shared colour"
        );
        assert_eq!(hex(MarkerKind::TaskChecked, depth), "#111111");
    }
    // Anti-vacuity: the BULLET does move, so the assertions above are about which
    // kinds the depth key reaches and not about the key doing nothing at all.
    assert_eq!(hex(MarkerKind::Bullet, 1), "#111111");
    assert_eq!(hex(MarkerKind::Bullet, 2), "#222222");
    // And the shared key itself is untouched.
    assert_eq!(
        crate::palette::to_hex_opaque(t.list_marker_color.unwrap()),
        "#111111"
    );
}

/// A user file's depth override must merge over a shipped theme, six keys' worth.
///
/// Not a merge-LIST guard any more: `overlay` extends a spelling map, so the per-key
/// omission this once watched for is unrepresentable. The live claim is that a NARROWED
/// depth spelling survives the merge and lands on the tier it names.
#[test]
fn a_user_file_can_override_a_bullet_depth_key() {
    let mut themes = Themes::builtin();
    themes.merge_over(
        Themes::parse_compiled(
            "[themes.terminal]\nlist_marker_color_2 = \"#abcdef\"\n\
                 list_bullet_glyph_2 = \"·\"\nlist_bullet_glyph_3 = \"‧\"\n",
        )
        .unwrap(),
    );
    let t = themes.resolve("terminal");
    assert_eq!(
        crate::palette::to_hex_opaque(t.list_bullet_colors[1].expect("merged")),
        "#abcdef"
    );
    assert_eq!(t.list_glyphs.bullet[1].as_ref().unwrap().as_plain(), "·");
    assert_eq!(t.list_glyphs.bullet[2].as_ref().unwrap().as_plain(), "‧");
    // Terminal's own depth-1 glyph survives the override of the deeper tiers.
    assert_eq!(t.list_glyphs.bullet[0].as_ref().unwrap().as_plain(), "▸");
}

/// TDD 18.27 / 18.2 — the task colour is opt-in and folds to `list_marker`, so a
/// theme that states neither leaves it `None` and the marker takes the widget
/// foreground exactly as before the key existed.
#[test]
fn the_task_marker_colour_is_opt_in_and_folds_to_the_shared_key() {
    let sys = Themes::builtin().resolve(SYSTEM_ID);
    assert!(sys.list_task_color.is_none());
    assert!(sys.list_marker_color.is_none());

    // Stated `list_marker` alone: the task marker follows it, which is today's
    // behaviour and what makes the new key inert until asked for.
    let mut shared = Themes::builtin();
    shared.merge_over(
        Themes::parse_compiled("[themes.sepia]\nlist_marker_color = \"#111111\"\n").unwrap(),
    );
    assert_eq!(
        crate::palette::to_hex_opaque(shared.resolve("sepia").list_task_color.unwrap()),
        "#111111"
    );

    // Stated separately: the task marker leaves the shared key behind, and the
    // shared key is untouched — that independence IS the rubric.
    let mut split = Themes::builtin();
    split.merge_over(
            Themes::parse_compiled(
                "[themes.sepia]\nlist_marker_color = \"#111111\"\nlist_task_marker_color = \"#ff00ff\"\n",
            )
            .unwrap(),
        );
    let t = split.resolve("sepia");
    assert_eq!(
        crate::palette::to_hex_opaque(t.list_task_color.unwrap()),
        "#ff00ff"
    );
    assert_eq!(
        crate::palette::to_hex_opaque(t.list_marker_color.unwrap()),
        "#111111"
    );
    // …and the BULLET tiers keep reading the shared key, not the task one.
    assert_eq!(
        crate::palette::to_hex_opaque(t.list_bullet_colors[0].unwrap()),
        "#111111"
    );
}

/// HISTORY, kept as a named regression rather than as a live mechanism: `list_marker`
/// was once omitted from `overlay`'s hand-written per-key merge list, so every user
/// override of that one key was silently dropped while every built-in theme kept
/// working. That list no longer exists — the registry made the omission
/// unrepresentable — so this test now pins the OUTCOME (a user file overrides a key its
/// theme never states) rather than the mechanism that used to break it.
#[test]
fn a_user_file_can_override_list_marker() {
    let mut themes = Themes::builtin();
    // Sepia ships no list_marker (stays None); a user file adds one.
    themes.merge_over(
        Themes::parse_compiled("[themes.sepia]\nlist_marker_color = \"#abcdef\"\n").unwrap(),
    );
    assert_eq!(
        crate::palette::to_hex_opaque(themes.resolve("sepia").list_marker_color.expect("merged")),
        "#abcdef"
    );
}
