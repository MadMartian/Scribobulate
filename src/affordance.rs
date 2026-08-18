//! The display-free geometry of the preview's **drawn, clickable affordances** —
//! where one sits, and whether a pointer is on one.
//!
//! The preview draws its own controls rather than anchoring widgets (GTK4Rs/AP-23,
//! GTK4Rs/AP-189), so nothing in GTK answers "is the pointer on that control": the
//! paint records a rectangle and a later click is tested against it. Both halves are
//! plain arithmetic over plain numbers, so they live here and are settled by unit
//! test, leaving the GTK side ([`crate::codeview`]) with the paint, the recording and
//! the coordinate conversion — the same split [`crate::keynav`] makes with
//! `codeview::navkeys`.
//!
//! Coordinates throughout are the **buffer** space `snapshot_layer` paints in; the
//! view converts an incoming widget-space pointer position with GTK's own inverse
//! transform before asking anything here.

use gtk::graphene;

/// The copy-button square for a code block whose (viewport-clamped) card rectangle is
/// `card`.
///
/// `pad` is the block's inner padding in device px (`px(config().code.block_padding)`
/// — the very inset the `code-block` tag holds the text off the card edge by) and
/// `single_line_h` is one text row's height in the view's own CSS-zoomed font. The
/// side is their sum and the corner inset is half the padding, so the button is
/// square, proportional to the type it sits over, and symmetrically inset on both
/// axes. Both inputs arrive already zoom-scaled, so the button tracks zoom with no
/// metric of its own to keep in step.
///
/// **The vertical inset yields where the card is short**, rather than the button
/// spilling out of it or vanishing. A one-line block is only `pad + row + pad` tall,
/// and its rows are the *code* font's, which is shorter than the body row this size
/// derives from — so the full inset does not fit and the button centres in what there
/// is instead. Measured, not assumed: a one-line block card is 39 px against a 42 px
/// ideal at the default configuration, and the first spelling of this rule refused
/// every such block a button (the exact blocks a reader most wants to copy in one
/// gesture).
///
/// `card` is the **viewport-clamped** rectangle the card was painted with, which is
/// what makes the button sticky in a block taller than the pane: it rides the top of
/// whatever part of the block is on screen.
///
/// `None` only when there is genuinely nowhere to put it: a card narrower than the
/// button plus its side insets, or one clamped shorter than a single text row — the
/// sliver a block half-way off either end of the viewport leaves.
pub(crate) fn copy_button_rect(
    card: &graphene::Rect,
    pad: f32,
    single_line_h: f32,
) -> Option<graphene::Rect> {
    let inset = (pad / 2.0).round();
    // Never wider or taller than the card that holds it.
    let side = (single_line_h + pad)
        .round()
        .min(card.height())
        .min(card.width());
    if side < single_line_h || side <= 0.0 || card.width() < side + 2.0 * inset {
        return None;
    }
    let inset_y = ((card.height() - side) / 2.0).min(inset);
    Some(graphene::Rect::new(
        card.x() + card.width() - inset - side,
        card.y() + inset_y,
        side,
        side,
    ))
}

/// The first recorded rectangle containing `(x, y)`, and the index it carries.
///
/// The one hit test behind every drawn affordance in the preview — task checkboxes,
/// code-block cards and their copy buttons — so "what counts as being on it" cannot
/// fork per affordance. Edges are inside: a click on a button's own border activates
/// it. Later rectangles win nothing; each list is built in paint order and its
/// members do not overlap.
pub(crate) fn hit_rects(rects: &[(graphene::Rect, usize)], x: f32, y: f32) -> Option<usize> {
    rects.iter().find_map(|(r, idx)| {
        (x >= r.x() && x <= r.x() + r.width() && y >= r.y() && y <= r.y() + r.height())
            .then_some(*idx)
    })
}

#[cfg(test)]
mod copy_button_tests {
    use super::copy_button_rect;
    use gtk::graphene;

    /// A default-configuration block: 12 px code padding, an 18 px text row. The
    /// button is 30 px square and inset 6 px from the card's top-right corner, so
    /// its right edge sits 6 px inside the card — the same gap on both axes.
    #[test]
    fn button_sits_in_the_cards_top_right_corner() {
        let card = graphene::Rect::new(20.0, 100.0, 500.0, 90.0);
        let r = copy_button_rect(&card, 12.0, 18.0).expect("card is tall enough");
        assert_eq!(r.width(), 30.0);
        assert_eq!(r.height(), 30.0);
        assert_eq!(r.y(), 106.0);
        assert_eq!(r.x() + r.width(), 20.0 + 500.0 - 6.0);
    }

    /// Both inputs arrive already zoom-scaled (the caller runs them through the same
    /// `px()`/Pango path the tag margins and the gutter use), so the button doubles
    /// with the document rather than needing a zoom factor of its own.
    #[test]
    fn button_tracks_zoomed_inputs() {
        let card = graphene::Rect::new(40.0, 0.0, 1000.0, 200.0);
        let r = copy_button_rect(&card, 24.0, 36.0).expect("card is tall enough");
        assert_eq!(r.width(), 60.0);
        assert_eq!(r.y(), 12.0);
        assert_eq!(r.x() + r.width(), 40.0 + 1000.0 - 12.0);
    }

    /// A card clamped shorter than one text row — a long block whose top has only
    /// just scrolled into view — yields no button at all, rather than one floating
    /// free of the block it belongs to.
    #[test]
    fn no_button_when_the_clamped_card_cannot_hold_it() {
        let card = graphene::Rect::new(20.0, 0.0, 500.0, 12.0);
        assert!(copy_button_rect(&card, 12.0, 18.0).is_none());
        // Narrow cards are refused on the same rule, so a pathologically thin pane
        // never draws a button wider than the block.
        let narrow = graphene::Rect::new(20.0, 0.0, 30.0, 200.0);
        assert!(copy_button_rect(&narrow, 12.0, 18.0).is_none());
    }

    /// A ONE-LINE block still gets a button. Its card is `pad + code row + pad`, and
    /// the code row is shorter than the body row the size derives from (39 px against
    /// a 42 px ideal at the default configuration, MEASURED), so the button keeps its
    /// size and the vertical inset yields — centring it in the card rather than
    /// spilling past the edges. Delete the `inset_y` clamp and this returns `None`,
    /// which is precisely the regression it guards: every one-line block silently
    /// loses its copy button while multi-line blocks keep theirs.
    #[test]
    fn a_one_line_block_keeps_its_button_by_centring_it() {
        let card = graphene::Rect::new(20.0, 100.0, 500.0, 39.0);
        let r = copy_button_rect(&card, 12.0, 18.0).expect("a one-line block has a button");
        assert_eq!(r.width(), 30.0);
        assert_eq!(r.y(), 104.5);
        // Symmetric in the card it could not take the full inset from.
        assert_eq!(
            r.y() - card.y(),
            card.y() + card.height() - (r.y() + r.height())
        );
    }

    /// Shorter still and the button shrinks to the card rather than overhanging it,
    /// down to the one-text-row floor the refusal above starts at.
    #[test]
    fn a_card_shorter_than_the_button_shrinks_it_to_fit() {
        let card = graphene::Rect::new(20.0, 0.0, 500.0, 24.0);
        let r = copy_button_rect(&card, 12.0, 18.0).expect("still taller than a row");
        assert_eq!(r.height(), 24.0);
        assert_eq!(r.y(), 0.0);
    }
}

#[cfg(test)]
mod hit_rect_tests {
    use super::hit_rects;
    use gtk::graphene::Rect;

    #[test]
    fn a_point_inside_a_rect_returns_its_index() {
        let rects = vec![(Rect::new(10.0, 20.0, 30.0, 40.0), 7)];
        assert_eq!(hit_rects(&rects, 12.0, 22.0), Some(7));
        // Edges are inside — a click on a button's own border must activate it.
        assert_eq!(hit_rects(&rects, 10.0, 20.0), Some(7));
        assert_eq!(hit_rects(&rects, 40.0, 60.0), Some(7));
    }

    #[test]
    fn a_point_outside_every_rect_returns_none() {
        let rects = vec![(Rect::new(10.0, 20.0, 30.0, 40.0), 7)];
        assert_eq!(hit_rects(&rects, 9.0, 22.0), None);
        assert_eq!(hit_rects(&rects, 12.0, 61.0), None);
        assert_eq!(hit_rects(&[], 12.0, 22.0), None);
    }

    #[test]
    fn each_rect_carries_its_own_index() {
        let rects = vec![
            (Rect::new(0.0, 0.0, 10.0, 10.0), 0),
            (Rect::new(0.0, 20.0, 10.0, 10.0), 3),
        ];
        assert_eq!(hit_rects(&rects, 5.0, 5.0), Some(0));
        assert_eq!(hit_rects(&rects, 5.0, 25.0), Some(3));
        assert_eq!(hit_rects(&rects, 5.0, 15.0), None);
    }
}
