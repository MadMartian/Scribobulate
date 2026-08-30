//! List markers: which key each kind reads, which of them vary by nesting depth, and
//! what happens when the winning value cannot be produced.
//!
//! These moved here from `codeview::gutter` with the dispatch itself. They were always
//! display-free — they only *happened* to live beside the paint code, which is the
//! argument that kept the PDF sink hand-rolling its own copies of the same tables.

use super::super::decor::{marker_choice, marker_glyph, marker_sprite, MarkerSubstitute};
use super::super::{ListGlyphs, MarkerKind, Sprites, Themes};
use crate::renderer::ListMarkerKind;

const ALL: [MarkerKind; 4] = [
    MarkerKind::Bullet,
    MarkerKind::Ordered,
    MarkerKind::Task,
    MarkerKind::TaskChecked,
];

/// The winner among the values the theme STATED — the first candidate, before any
/// question of whether it can be produced.
fn winner(t: &crate::theme::Theme, kind: MarkerKind, depth: usize) -> MarkerSubstitute<'_> {
    t.marker_decor(kind, depth).candidates()[0]
}

fn themed(spec: &str, id: &str) -> crate::theme::Theme {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(spec);
    themes.resolve(id)
}

/// TDD 18.24 / 18.2 — a theme that states nothing gets the drawn primitives, which
/// is what keeps System byte-identical.
#[test]
fn nothing_stated_means_the_drawn_marker() {
    let (g, s) = (ListGlyphs::default(), Sprites::default());
    for kind in ALL {
        assert_eq!(
            marker_choice(kind, 1, &g, &s).candidates()[0],
            MarkerSubstitute::Drawn
        );
    }
}

/// Each kind reads its OWN key — the failure this pins is a match arm that answers
/// one marker's question with another's, which renders plausibly and is wrong on
/// exactly the document that has more than one list kind in it.
#[test]
fn each_marker_kind_reads_its_own_glyph_and_the_task_states_are_separate() {
    let t = themed(
        "[themes.marks]\nlist_bullet_glyph = \"b\"\nlist_ordered_glyph = \"o\"\n\
         list_task_glyph = \"t\"\nlist_task_checked_glyph = \"c\"\n",
        "marks",
    );
    let plain = |k: MarkerKind| match winner(&t, k, 1) {
        MarkerSubstitute::Glyph(g) => g.as_plain().to_string(),
        other => panic!("expected a glyph, got {other:?}"),
    };
    assert_eq!(plain(MarkerKind::Bullet), "b");
    assert_eq!(plain(MarkerKind::Ordered), "o");
    assert_eq!(plain(MarkerKind::Task), "t");
    assert_eq!(plain(MarkerKind::TaskChecked), "c");
}

/// A theme may state only ONE task state; the other keeps its drawn box. Deliberate
/// and visible, rather than a rule that silently suppresses the glyph it was given.
#[test]
fn one_task_state_may_be_stated_alone() {
    let t = themed("[themes.half]\nlist_task_checked_glyph = \"✔\"\n", "half");
    assert!(matches!(
        winner(&t, MarkerKind::TaskChecked, 1),
        MarkerSubstitute::Glyph(_)
    ));
    assert_eq!(winner(&t, MarkerKind::Task, 1), MarkerSubstitute::Drawn);
}

/// TDD 18.26 — the bullet's glyph and sprite vary by nesting depth, and the tier a
/// depth reads is the shared `depth_tier`. Depth 3 and anything deeper share a tier.
#[test]
fn a_bullet_reads_its_depth_tier_and_deeper_levels_share_the_last_one() {
    let t = themed(
        "[themes.tiered]\nlist_bullet_glyph = \"1\"\nlist_bullet_glyph_2 = \"2\"\n\
         list_bullet_glyph_3 = \"3\"\n",
        "tiered",
    );
    let at = |depth: usize| match winner(&t, MarkerKind::Bullet, depth) {
        MarkerSubstitute::Glyph(g) => g.as_plain().to_string(),
        other => panic!("depth {depth}: expected a glyph, got {other:?}"),
    };
    assert_eq!(at(1), "1");
    assert_eq!(at(2), "2");
    assert_eq!(at(3), "3");
    // Three-and-deeper: a six-level list does not index past the last tier.
    assert_eq!(at(6), "3");
    assert_eq!(at(60), "3");
}

/// Depth is a BULLET question. A nested ordered numeral and a nested task box read
/// their own single-valued keys at every depth — the array is indexed only on the
/// arm that has one.
#[test]
fn depth_does_not_reach_the_ordered_or_task_markers() {
    let t = themed(
        "[themes.tiered]\nlist_bullet_glyph_2 = \"deep\"\nlist_ordered_glyph = \"o\"\n\
         list_task_glyph = \"t\"\n",
        "tiered",
    );
    for depth in [1usize, 2, 5] {
        let ordered = winner(&t, MarkerKind::Ordered, depth);
        let task = winner(&t, MarkerKind::Task, depth);
        assert!(
            matches!(ordered, MarkerSubstitute::Glyph(g) if g.as_plain() == "o"),
            "depth {depth}: {ordered:?}"
        );
        assert!(
            matches!(task, MarkerSubstitute::Glyph(g) if g.as_plain() == "t"),
            "depth {depth}: {task:?}"
        );
    }
}

/// TDD 18.26 — the marker's INK by depth, and the two properties every consumer of
/// this fold depends on: a bullet reads its tier, everything else reads the shared
/// key at every depth.
#[test]
fn marker_ink_follows_the_bullets_depth_and_nothing_elses() {
    let t = themed(
        "[themes.tiered]\nlist_marker_color = \"#111111\"\nlist_marker_color_2 = \"#222222\"\n",
        "tiered",
    );
    let hex = |kind: MarkerKind, depth: usize| {
        crate::palette::to_hex_opaque(t.marker_ink(kind, depth).unwrap())
    };
    assert_eq!(hex(MarkerKind::Bullet, 1), "#111111");
    assert_eq!(hex(MarkerKind::Bullet, 2), "#222222");
    // Depth 3 inherited depth 2, so the bullet stays on the deeper colour.
    assert_eq!(hex(MarkerKind::Bullet, 3), "#222222");
    // …while a numeral and a checkbox stay on the shared key wherever they sit.
    assert_eq!(hex(MarkerKind::Ordered, 2), "#111111");
    assert_eq!(hex(MarkerKind::TaskChecked, 3), "#111111");

    // A theme that states no marker colour at all resolves to `None`, which the
    // caller answers with the widget foreground — the pre-theming default.
    let bare = Themes::builtin().resolve(crate::theme::SYSTEM_ID);
    assert!(bare.marker_ink(MarkerKind::Bullet, 2).is_none());
}

/// TDD 18.27 — the task checkbox takes its own colour while bullets and numerals in
/// the same document keep `list_marker`, and BOTH task states take the same one: a
/// checked and an unchecked box are the same control in two positions.
#[test]
fn a_task_checkbox_takes_its_own_colour_and_both_states_share_it() {
    let t = themed(
        "[themes.split]\nlist_marker_color = \"#111111\"\nlist_task_marker_color = \"#ff00ff\"\n",
        "split",
    );
    let hex = |kind: MarkerKind| crate::palette::to_hex_opaque(t.marker_ink(kind, 1).unwrap());
    assert_eq!(hex(MarkerKind::Task), "#ff00ff");
    assert_eq!(hex(MarkerKind::TaskChecked), "#ff00ff");
    assert_eq!(hex(MarkerKind::Bullet), "#111111");
    assert_eq!(hex(MarkerKind::Ordered), "#111111");
    // Depth does not reach the task marker — it is one colour wherever it sits.
    assert_eq!(
        crate::palette::to_hex_opaque(t.marker_ink(MarkerKind::TaskChecked, 3).unwrap()),
        "#ff00ff"
    );
}

/// A SPRITE outranks a glyph for the same marker — stated once here so the gutter
/// and both export sinks cannot each invent their own precedence.
#[test]
fn a_sprite_outranks_a_glyph_for_the_same_marker() {
    let mut t = themed("[themes.both]\nlist_bullet_glyph = \"b\"\n", "both");
    // Set the resolved path directly: `resolve` never touches the filesystem, and
    // this test is about the PRECEDENCE, not about sprite validation (which
    // `sprite.rs` owns and `theme::tests::sprites` exercises across every sprite key).
    t.sprites.list_bullet[0] = Some(crate::sprite::SpriteRef::File(std::path::PathBuf::from(
        "/x/bullet.png",
    )));
    assert!(matches!(
        winner(&t, MarkerKind::Bullet, 1),
        MarkerSubstitute::Sprite(_)
    ));
    // …and the OTHER markers are untouched by it.
    assert_eq!(winner(&t, MarkerKind::Ordered, 1), MarkerSubstitute::Drawn);
}

/// **The candidates are ORDERED and complete, so a paint site can walk past a winner
/// it cannot produce.**
///
/// Precedence between two stated values and "what if the winner cannot be produced"
/// are different questions. Collapsing them made the list marker the one renderer
/// where a decode failure ERASED the decoration: a theme stating both a sprite and a
/// glyph lost the glyph too, and a task checkbox that failed to resample became an
/// invisible-but-still-clickable hit box. Every sibling degrades — the band to its
/// gradient then its fill, the bar to `blockquote_bar_color`, the rule to a stock
/// separator, the chip to its flat fill.
#[test]
fn a_marker_offers_every_candidate_in_order_and_always_ends_at_the_drawn_one() {
    let mut t = themed("[themes.both]\nlist_bullet_glyph = \"b\"\n", "both");
    t.sprites.list_bullet[0] = Some(crate::sprite::SpriteRef::File(std::path::PathBuf::from(
        "/x/bullet.png",
    )));
    let got = t.marker_decor(MarkerKind::Bullet, 1).candidates();
    assert!(matches!(got[0], MarkerSubstitute::Sprite(_)));
    assert!(
        matches!(got[1], MarkerSubstitute::Glyph(g) if g.as_plain() == "b"),
        "the theme's glyph must survive a sprite it cannot decode: {got:?}"
    );
    assert_eq!(got[2], MarkerSubstitute::Drawn);

    // Every kind, stated or not, ends at `Drawn` — which is what makes the walk total:
    // there is always something to paint.
    let bare = Themes::builtin().resolve(crate::theme::SYSTEM_ID);
    for kind in ALL {
        let candidates = bare.marker_decor(kind, 1).candidates();
        assert_eq!(candidates, vec![MarkerSubstitute::Drawn], "{kind:?}");
        assert_eq!(
            *t.marker_decor(kind, 1).candidates().last().unwrap(),
            MarkerSubstitute::Drawn,
            "{kind:?}"
        );
    }
}

/// **The two normalisations agree.** The preview arrives at a kind from a
/// `ListMarkerKind` and the PDF sink from a `(task, start)` pair; if the two disagree
/// about any row, the artefact wears a different marker from the screen — including
/// the one row a reader is most likely to get backwards, a task item inside an
/// ordered list.
#[test]
fn the_preview_and_the_pdf_sink_normalise_to_the_same_kind() {
    let cases: [(ListMarkerKind, Option<bool>, Option<u64>); 5] = [
        (ListMarkerKind::Bullet, None, None),
        (ListMarkerKind::Ordered(3), None, Some(1)),
        (
            ListMarkerKind::Task {
                checked: false,
                src: 0..1,
            },
            Some(false),
            None,
        ),
        (
            ListMarkerKind::Task {
                checked: true,
                src: 0..1,
            },
            Some(true),
            None,
        ),
        // A task item INSIDE an ordered list is still a checkbox: the task arms come
        // first, on both routes.
        (
            ListMarkerKind::Task {
                checked: true,
                src: 0..1,
            },
            Some(true),
            Some(4),
        ),
    ];
    for (live, task, start) in cases {
        assert_eq!(
            live.theme_kind(),
            MarkerKind::from_task_and_start(task, start),
            "{live:?} vs ({task:?}, {start:?})"
        );
    }
}

/// The two half-projections read the same table as the whole one — the PDF sink asks
/// for the sprite and the glyph separately, and a divergence there would put a
/// picture on the page beside the numeral it was meant to replace.
#[test]
fn the_sprite_and_glyph_halves_agree_with_the_whole_choice() {
    let mut t = themed(
        "[themes.mix]\nlist_bullet_glyph = \"b\"\nlist_ordered_glyph = \"o\"\n\
         list_task_glyph = \"t\"\nlist_task_checked_glyph = \"c\"\n",
        "mix",
    );
    t.sprites.list_ordered = Some(crate::sprite::SpriteRef::File(std::path::PathBuf::from(
        "/x/ordered.png",
    )));
    for kind in ALL {
        for depth in [1usize, 2, 4] {
            let whole = t.marker_decor(kind, depth);
            assert_eq!(
                marker_sprite(kind, depth, &t.sprites),
                whole.sprite,
                "{kind:?} @ {depth}"
            );
            assert_eq!(
                marker_glyph(kind, depth, &t.list_glyphs),
                whole.glyph,
                "{kind:?} @ {depth}"
            );
        }
    }
}
