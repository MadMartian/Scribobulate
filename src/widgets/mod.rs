//! Custom GTK4 widgets built for Scribobulate.
//!
//! This is the home for every hand-written `GtkWidget` subclass (and the plain
//! Rust façades that pair with them). Each widget lives in its own submodule
//! directory so its GObject glue, its pure decision/layout arithmetic (a
//! GTK-free, unit-tested `layout` module — see POLICY §coverage gate), and any
//! façade split across focused files rather than one monolith:
//!
//! - [`table`] — `ScribTableWidget`, the churn-free anchored Markdown-table
//!   widget (a `ConstantSize` widget with cached cell rects; see GTK4Rs/AP-23).
//! - [`tab`] — the `GtkNotebook`-free tab strip (`TabBar` + the `TabView`
//!   façade); kills the GTK4Rs/AP-60 crash class and adds per-tab close/context-menu.
//! - [`comment_entry`] — `CommentEntry`, the single annotation comment
//!   entry + Save pair shared by all three annotation surfaces, wiring every
//!   commit route once.
//! - [`floating_card`] — `FloatingCard`, the in-window card every transient
//!   anchored card (Annotate action, formatting bar, annotation comment) floats as,
//!   in place of a `GtkPopover` the window manager can leave behind.
//! - [`rule`] — `SpriteRule`, the horizontal rule when a theme tiles a sprite across
//!   it (TDD 18.31). Built only where the theme states one; the flat rule stays the
//!   stock `GtkSeparator` it has always been.
//! - [`sprite_icon`] — `SpriteIcon`, a theme sprite drawn at a fixed square size (the
//!   disclosure indicator's sprite shape), so an animated one plays (TDD 27.9).
//! - [`textfield`] — the constructors every `GtkEntry`/`GtkSearchEntry` in the
//!   application comes from, so the two silent follow-ups a hand-built field owes
//!   (accessible name; macOS word navigation) cannot be forgotten one surface at a
//!   time.
//! - [`wrapbox`] — `ToolbarWrapBox`, a left-packed wrapping row: children keep
//!   their natural size and overflow onto a new row instead of being clipped or
//!   squeezed, with no shared column grid to misalign a later row against an
//!   earlier one's sizes (the toolbar's own use, `window::toolbar`).

use gtk::prelude::*;

pub(crate) mod comment_entry;
pub(crate) mod disclosure;
pub(crate) mod floating_card;
pub(crate) mod rule;
pub(crate) mod sprite_icon;
pub(crate) mod tab;
pub(crate) mod table;
pub(crate) mod textfield;
pub(crate) mod wrapbox;

/// How long after a popover closes before its surface may be released (unrealized or
/// unparented). It is measured against GTK's tooltip hover delay (`HOVER_TIMEOUT`, 500 ms
/// in gtktooltip.c at 4.6.9), plus slack, so a tooltip timeout armed by an enter into the
/// popover has fired while the surface still exists (GTK4Rs/AP-117). Waited by
/// `popoverrelease`, the toolbar menu's release (`window::toolbarmenu`), and the suite's
/// teardown (`testpump::destroy_after_tooltip_delay`).
pub(crate) const TOOLTIP_HOVER_DRAIN: std::time::Duration = std::time::Duration::from_millis(600);

/// Tile `tex` across `rect` at the texture's NATURAL size, with the grid anchored so the
/// pattern travels with the document.
///
/// **One spelling of one operation.** The `push_repeat` / `append_texture` / `pop`
/// sequence was copy-pasted three times (the heading band, the blockquote bar, the rule
/// widget) with two undeclared variations between them: the two `codeview` sites anchored
/// the tile grid at the rect while the rule widget anchored at the origin, and only the
/// rule widget guarded against a zero or negative dimension. `sdd/THEMING.md` already
/// described the rule widget as using "the same `push_repeat`/`append_texture` pair every
/// other sprite in this vocabulary is painted with" — a claim about a shared seam that
/// did not exist until now.
///
/// # The anchor, and why it is not a parameter
///
/// The tile grid is anchored at `(rect.x(), N * tile_h)` — x from the rect, y at the
/// DOCUMENT-origin grid line at or above the rect — and every tiling site in the tree
/// wants that pair, so it is baked in rather than chosen per call. It replaced a
/// `TileOrigin` enum whose two answers were "the rect's own top-left" and `(0, 0)`; both
/// were wrong, in different ways, and the enum's own documentation asserted the opposite
/// of what each did.
///
/// **What the anchor actually means** (researcher-verified against GTK 4.6.9):
/// `gtk_snapshot_push_repeat` (`gtksnapshot.c:787-807`) `ensure_affine`-bakes the current
/// 2-D affine into *both* `bounds` and `child_bounds`, and the cairo repeat node
/// (`gskrendernodeimpl.c:3399-3432`) sets its source-surface matrix from
/// `-child_bounds.origin`. So `child_bounds` is simultaneously the sample window and the
/// phase anchor, absolute in that baked space: the phase at a point `P` is
/// `(P - child_origin) mod tile`. "Anchored relative to the decoration" is not a mode the
/// API offers — it is only what you get by choosing `child_origin == bounds.origin`.
///
/// **Why y must be the document's grid, not the rect's top.** Inside a `GtkTextView`'s
/// `snapshot_layer` the current transform is already `translate(-xoffset, -yoffset)`
/// (`gtktextview.c:5871-5873`), and a pure translate bakes into the rects rather than
/// wrapping a transform node — so a `y` congruent to `0` modulo `tile_h` bakes to
/// `-yoffset` modulo `tile_h`, putting plate row `yoffset % tile_h` at the top of the
/// viewport, a phase that travels with the text.
/// Anchoring at the rect cannot do this, because `codeview::geometry::span_card_y_extent`
/// returns `top = vtop` whenever a span begins above the visible range — the normal case
/// for anything taller than the pane. The anchor then *is* the viewport, and the pattern
/// is nailed to the screen while the text scrolls under it. MEASURED on the blockquote
/// bar before the fix: the bar column was pixel-identical (AE=0) across a 176px scroll —
/// 7.3 tile periods — while the text column at the same rows differed by over 22,000
/// pixels, absolute tile phase reading 6.34 in every frame. After: 6.34 / 19.34 / 8.34 /
/// 22.34 at scroll offsets 576 / 635 / 694 / 752, matching `(6.34 - Δscroll) mod 24`
/// exactly at all three steps.
///
/// **Why x must come from the rect.** The `(0, 0)` spelling samples the tile at
/// `rect.x() % tile_w`, slicing the sprite horizontally wherever a decoration does not
/// begin on a tile boundary — which a bar at the view's left margin generally does not.
/// The two halves are independent: fixing the scroll phase with `(0, 0)` trades a
/// vertical bug for a visible vertical seam down the bar's left edge. The rule widget's
/// rect is `(0, 0, w, h)`, so both spellings coincide there and its pixels are unchanged.
///
/// Grid alignment to each decoration's own top is deliberately NOT offered: it needs
/// `decoration_top % tile_h`, and for a viewport-clamped span that remainder is exactly
/// the off-screen unvalidated-iter read GTK4Rs/AP-22 bans. A grid line is a coordinate, not
/// an iter, so this anchor needs no such read.
///
/// **Why the anchor is the nearest grid line and not the literal `0`.** The phase half of
/// this anchor is GTK4Rs/AP-315, whose prescribed `child_origin = (rect.x(), 0.0)` is what
/// this tree shipped and what the ceiling below breaks; that entry does not yet carry the
/// ceiling, so nothing here cites it for one. Same grid — a
/// multiple of `tile_h` is congruent to `0`, so the phase is bit-identical — but the
/// literal `0` puts `child_bounds` an unbounded distance from `bounds`, and past
/// **32768 px of separation the Cairo renderer draws the repeat node as NOTHING**. The
/// decoration does not degrade to its flat sibling and emits no warning; it simply is
/// not there, on every tiled decoration at once, from one scroll position onward.
/// MEASURED (GTK 4.6.9, `GSK_RENDERER=cairo`, this tree): a bar at buffer y 32670 tiles,
/// the same bar at 32724 does not; `append_color`, `append_texture` and
/// `append_linear_gradient` at the same coordinates are all unaffected, so this is
/// specific to the repeat node. 32768 is `2^15` — the ceiling of pixman's 16.16
/// `pixman_fixed_t`, which the repeating source pattern's offset is converted to.
/// A rendered `sdd/TDD.md` is ~107,000 px tall at a 1000 px pane, so roughly two thirds
/// of it lost every sprite; the reported symptom was a blockquote plate that "stops
/// partway down the file", and the boundary moves with pane width and preview zoom
/// because those are what decide the buffer y of any given line.
///
/// `floor` and not `round`: the anchor must be at or ABOVE the rect, so the tile row the
/// rect's top samples is the one the grid actually places there.
///
/// Natural size, not stretched: 1:1 pixels need no filter, and GSK 4.6's
/// `append_texture` filters linearly with no choice (the variant that takes one is 4.10,
/// GTK4Rs/AP-114). Tiling also means one cached texture per reference instead of one per
/// decoration width, which a window resize would otherwise mint by the hundred.
///
/// The same tile rect goes to `push_repeat` and to `append_texture`, and must: they bake
/// through the same affine, and giving them different origins leaves the first
/// `child_origin.y - texture_origin.y` rows of every tile empty.
///
/// **The repeat covers more than the rect, and a clip cuts it back** (GTK4Rs/AP-380). From
/// GTK 4.22 the Cairo renderer draws a repeat node strip by strip and stretches each
/// strip to the bottom of whatever clip is in force (`gsk/gskrepeatnode.c`, `gsk_repeat_node_draw_repeat`,
/// `end_y = MAX (...)`). Inside the node's own bounds that clip is the view, so a short
/// bar that crosses a tile-grid line painted its stripes down to the bottom of the view
/// (MEASURED on GTK 4.22.4, Windows and macOS; GTK 4.6.9 fills exactly the node's bounds
/// and never showed it). A clip pushed to the rect is NOT enough on its own: GTK emits no
/// clip node when the clip contains the child's bounds (`gtksnapshot.c`, the
/// `graphene_rect_contains_rect` early return), and a repeat node's bounds are the rect
/// it was given. So the repeat is given the rect grown downward to the next grid line
/// past it — always past it, so it is always strictly taller and the clip is always
/// kept — and the clip is the rect. On GTK 4.6.9, where the fill is the repeat's own
/// bounds, the clip is just as load-bearing: without it the whole grown area would paint.
///
/// A zero or negative dimension — on either the rect or the texture — paints NOTHING
/// rather than asking GSK for a degenerate repeat node. Two of the three call sites had
/// no such guard; folding it in here is what makes the omission unrepresentable.
pub(crate) fn tile_texture(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    tex: &gtk::gdk::Texture,
) {
    use gtk::gdk::prelude::TextureExt;
    let (tw, th) = (tex.width(), tex.height());
    if rect.width() <= 0.0 || rect.height() <= 0.0 || tw <= 0 || th <= 0 {
        return;
    }
    // The DOCUMENT-origin grid line at or above the rect: congruent to 0 modulo the
    // tile height, so the phase is the document's, but never more than one tile from
    // `rect`. See "Why the anchor is the nearest grid line" above — the literal 0
    // silently paints nothing once the two are 32768 px apart.
    let anchor_y = (rect.y() / th as f32).floor() * th as f32;
    let tile = gtk::graphene::Rect::new(rect.x(), anchor_y, tw as f32, th as f32);
    // Grown to the first grid line strictly below the rect's bottom, so the clip below
    // never contains it (see "The repeat covers more than the rect" above). Starting on
    // the anchor keeps it within two tile rows of the rect, far from pixman's ceiling.
    let below = ((rect.y() + rect.height()) / th as f32).floor() * th as f32 + th as f32;
    let repeat = gtk::graphene::Rect::new(rect.x(), anchor_y, rect.width(), below - anchor_y);
    snapshot.push_clip(rect);
    snapshot.push_repeat(&repeat, Some(&tile));
    snapshot.append_texture(tex, &tile);
    snapshot.pop();
    snapshot.pop();
}

/// Draw `sprite` ONCE into `rect`, fitted by HEIGHT and anchored to the right edge.
///
/// The third member of this vocabulary, beside [`tile_texture`] (a texture the
/// decoration is made OF) and [`draw_sprite_into`] (a picture resampled to fill a box
/// the layout chose). A **scene** is neither: it keeps its own aspect ratio, is drawn
/// once rather than repeated, and composites OVER whatever the band already painted.
///
/// **Right-anchored, and that is the whole design.** A heading band's width tracks the
/// content column and changes with the window; its height tracks the heading and does
/// not. So the right edge is the only part of the band whose position is stable
/// relative to its content, and the left is where the heading text sits. Anchoring left
/// would slide scenery under the text at every width; stretching to the width would
/// distort the scene as the window resized. Overflow therefore runs off the LEFT, where
/// a scene is expected to fade into the fill, and is clipped away.
///
/// Scaled by height with the width derived from the source's aspect, so the scene never
/// distorts, and resampled through `sprite::scaled` (cached per size, nearest-neighbour)
/// rather than handed to GSK at the wrong size — GSK 4.6's `append_texture` takes no
/// filter (GTK4Rs/AP-114), and letting it scale would also make the result depend on the
/// active renderer, which is the environment-dependence forcing `GSK_RENDERER=cairo`
/// exists to remove.
///
/// The clip is unconditional and costs nothing when it is unnecessary: `gtk_snapshot`
/// emits no clip node at all when the clip fully contains the child.
///
/// ⚠️ Sizes here are LOGICAL px, like every other sprite call site in this tree. On a
/// HiDPI surface `append_texture` bakes the scale factor into the node, so the
/// nearest-resampled texture is scaled a second time by GSK. That gap is not specific
/// to scenes — it applies to every `sprite::scaled` and `tile_texture` caller — and
/// fixing it belongs to one sweep of them all, not to this function.
///
/// `frames` supplies the pixels — an animated scene's current frame (TDD 27.9).
///
/// Returns `false` — painting nothing — when the rect is degenerate or the sprite will
/// not decode, this vocabulary's inert-by-default failure.
pub(crate) fn draw_scene_into(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    sprite: &crate::sprite::SpriteRef,
    frames: crate::animation::sprites::Frames<'_>,
) -> bool {
    use gtk::gdk::prelude::TextureExt;
    let h = rect.height().round() as i32;
    if h <= 0 || rect.width() <= 0.0 {
        return false;
    }
    let Some(natural) = crate::sprite::texture(sprite) else {
        return false;
    };
    let (nw, nh) = (natural.width(), natural.height());
    if nw <= 0 || nh <= 0 {
        return false;
    }
    let w = ((f64::from(nw) * f64::from(h) / f64::from(nh)).round() as i32).max(1);
    let Some(tex) = frames.scaled(sprite, w, h) else {
        return false;
    };
    let dst = gtk::graphene::Rect::new(
        rect.x() + rect.width() - w as f32,
        rect.y(),
        w as f32,
        h as f32,
    );
    snapshot.push_clip(rect);
    snapshot.append_texture(&tex, &dst);
    snapshot.pop();
    true
}

/// Draw `sprite` ONCE in one of `rect`'s CORNERS, at its natural size scaled by
/// `zoom`, clipped to `rect`.
///
/// The corner-anchored member of this vocabulary, beside [`draw_scene_into`] (fitted to
/// an edge) and [`tile_texture`] (repeated). Which corner is the caller's — a quote
/// panel's scene sits on its floor, a heading band's sparkles wherever the theme hangs
/// them — and the four are one arithmetic over [`SceneAnchor::fractions`] rather than
/// four functions, because the only thing that varies between them is a multiplier.
/// The distinction from the other two is not decoration: a
/// heading band's height is fixed by its heading, so a scene can be fitted to it, while
/// a quote panel's height is however long the quote is. Fitting to that would balloon
/// the scene on a long quotation. Drawn at a fixed size on the panel's floor instead, so
/// the panel reveals MORE of the scene as it grows rather than magnifying it — a short
/// quote shows the seabed, a long one brings what swims above it into view.
///
/// Scaled by `zoom` and not by the rect, because at a fixed size it is a themed pixel
/// metric like any other and must track the page's zoom or it shrinks to nothing as the
/// reader zooms in. Resampled through `sprite::scaled` (cached, nearest-neighbour) for
/// the reason every sprite here is: GSK 4.6's `append_texture` takes no filter
/// (GTK4Rs/AP-114), and letting it scale would make the result depend on the active
/// renderer — the environment-dependence `GSK_RENDERER=cairo` exists to remove.
///
/// ⚠️ The caller owns the decision that whichever edges its anchor hangs on are REAL.
/// Paint-path extents
/// in this project are viewport-clamped (GTK4Rs/AP-22 — never measure an off-screen,
/// unvalidated iter), so a rect whose bottom was clamped would anchor this to the
/// viewport and slide it as the reader scrolls, which is GTK4Rs/AP-315's shape. See
/// `codeview::quotes::draw_panel_scene`.
///
/// `frames` supplies the pixels — an animated scene's current frame (TDD 27.9).
///
/// Returns `false` — painting nothing — when the rect is degenerate or the sprite will
/// not resample.
pub(crate) fn draw_scene_corner(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    sprite: &crate::sprite::SpriteRef,
    anchor: crate::theme::SceneAnchor,
    zoom: f64,
    frames: crate::animation::sprites::Frames<'_>,
) -> bool {
    use gtk::gdk::prelude::TextureExt;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return false;
    }
    let Some(natural) = crate::sprite::texture(sprite) else {
        return false;
    };
    let (nw, nh) = (natural.width(), natural.height());
    if nw <= 0 || nh <= 0 {
        return false;
    }
    let w = ((f64::from(nw) * zoom).round() as i32).max(1);
    let h = ((f64::from(nh) * zoom).round() as i32).max(1);
    let Some(tex) = frames.scaled(sprite, w, h) else {
        return false;
    };
    // Placement is `SceneAnchor::offset`'s, shared with the PDF sink so the two cannot
    // put one theme's scene in different corners.
    let (dx, dy) = anchor.offset(
        f64::from(rect.width() - w as f32),
        f64::from(rect.height() - h as f32),
    );
    let dst = gtk::graphene::Rect::new(
        rect.x() + dx as f32,
        rect.y() + dy as f32,
        w as f32,
        h as f32,
    );
    snapshot.push_clip(rect);
    snapshot.append_texture(&tex, &dst);
    snapshot.pop();
    true
}

/// Draw `sprite` filling `rect` exactly, resampled to that size with nearest-neighbour.
///
/// The twin of [`tile_texture`], and the other half of what "paint a themed sprite"
/// means in this project: a decoration whose size is decided by the LAYOUT (a marker
/// box, an annotation chip) resamples to fit, where one whose size is its own (a band,
/// a bar, a rule) tiles at natural size. Both sequences were open-coded per site, and
/// which of the two a decoration takes is a real decision that now has two named answers
/// instead of two idioms.
///
/// Resampled through `sprite::scaled` rather than handed to GSK at the wrong size: GSK
/// 4.6's `append_texture` filters linearly with no filter choice (the variant that takes
/// one is 4.10, above this project's floor and a link/runtime failure if reached —
/// GTK4Rs/AP-114), so pre-resampling with nearest-neighbour is the only way pixel art
/// stays crisp at any zoom. `sprite::scaled` caches per size and diagnoses its own
/// refusals.
///
/// `frames` supplies the pixels — an animated sprite's current frame (TDD 27.9).
///
/// Returns `false` — painting nothing — when the rect is degenerate or the sprite will
/// not resample, which is this vocabulary's inert-by-default failure: the caller then
/// draws whatever the decoration would have been without a sprite.
pub(crate) fn draw_sprite_into(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    sprite: &crate::sprite::SpriteRef,
    frames: crate::animation::sprites::Frames<'_>,
) -> bool {
    let w = rect.width().round() as i32;
    let h = rect.height().round() as i32;
    if w <= 0 || h <= 0 {
        return false;
    }
    let Some(tex) = frames.scaled(sprite, w, h) else {
        return false;
    };
    snapshot.append_texture(&tex, rect);
    true
}

/// Unparent every child of a custom `GtkWidget` subclass.
///
/// **Contract:** a custom widget that parents its children with `set_parent`
/// (rather than delegating to a layout manager / container that owns them) must
/// unparent every child in its `dispose`. GTK does **not** do this automatically
/// for custom widget subclasses; skipping it leaks the children and emits
/// finalize-time warnings. Call this once from `dispose`.
/// **Paint one band into `rect`** — the sprite → gradient → flat precedence, the
/// rounded clip, and the scene that composites over whichever of those painted.
///
/// The rect-level core of the band, sited here beside [`tile_texture`] and
/// [`draw_scene_into`] rather than in `codeview` because it now has a caller that is
/// not a text-view pass at all: a table's header row is a band drawn by an anchored
/// WIDGET (`widgets::table`), while a heading's and a disclosure summary's are drawn
/// by `snapshot_layer`. Everything above this line differs between those callers (what
/// is iterated, how the extent is measured, how the decoration is resolved); everything
/// below it is identical, and was already once maintained in two copies that diverged
/// (`codeview::bandpaint`'s own header records what that cost).
///
/// `radius` is a FINAL pixel radius — already scaled by zoom and clamped to the rect by
/// `decorplan::band_corner_radius`. It is taken rather than derived so this function
/// stays display-free arithmetic over a rect, and so a caller whose radius comes from a
/// different key (a table cell's, not a band's) is not forced through a heading's.
///
/// `frames` supplies the tile's and the scene's pixels — the current frame of an animated
/// one (TDD 27.9). A still sprite resolves through `sprite::texture`'s cache, so a caller
/// with many bands still decodes one texture for the whole document.
pub(crate) fn paint_band_into(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    decor: &crate::theme::Band<'_>,
    radius: f32,
    zoom: f64,
    frames: crate::animation::sprites::Frames<'_>,
) {
    use gtk::gsk;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    if radius > 0.0 {
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(*rect, radius));
    }
    // The sprite first, then whatever the band would have been without it. A sprite
    // that will not decode therefore falls through to the gradient, then to the flat
    // fill — degrading rather than erasing the band, the same rule every other
    // decoration in this vocabulary follows. An explicit branch rather than painting
    // the fill under the tile: an opaque tile hides the difference and a transparent
    // one lets the colour bleed through (SCHEMA § Key naming).
    let tiled = decor.sprite.and_then(|r| frames.natural(r));
    match tiled {
        Some(tex) => tile_texture(snapshot, rect, &tex),
        None => match decor.without_sprite() {
            Some(crate::theme::BandPaint::Gradient { from, to }) => snapshot
                .append_linear_gradient(
                    rect,
                    &gtk::graphene::Point::new(rect.x(), rect.y()),
                    &gtk::graphene::Point::new(rect.x(), rect.y() + rect.height()),
                    &[gsk::ColorStop::new(0.0, from), gsk::ColorStop::new(1.0, to)],
                ),
            Some(crate::theme::BandPaint::Flat(fill)) => snapshot.append_color(&fill, rect),
            None => {}
        },
    }
    // The SCENE rides on top of whichever of those painted, because it composites
    // rather than replaces (`theme::Band::scene`). Deliberately outside the match: a
    // theme may state a scene with a flat fill, with a gradient, with a tiled sprite, or
    // with nothing at all — "a scene alone is a band" for the same reason a sprite alone
    // is (`Band::is_present`), and each of those four combinations must paint it exactly
    // once.
    //
    // Inside the rounded clip pushed above, so a scene cannot square off the band's
    // corners — the failure a caller drawing it after the `pop` would ship.
    //
    // WHERE it hangs is the theme's (`SceneAnchor`), and the anchor decides the size
    // rule with it: an edge anchor fits the scene to the band's height, a corner anchor
    // draws it at its own size so a cluster keeps the spread the theme drew it with
    // instead of being rescaled by whatever height the heading happens to have.
    if let Some(scene) = decor.scene {
        match decor.scene_anchor {
            Some(corner) => draw_scene_corner(snapshot, rect, scene, corner, zoom, frames),
            None => draw_scene_into(snapshot, rect, scene, frames),
        };
    }
    if radius > 0.0 {
        snapshot.pop();
    }
}

pub(crate) fn unparent_all_children(widget: &impl IsA<gtk::Widget>) {
    while let Some(child) = widget.first_child() {
        child.unparent();
    }
}

/// Gated on the integration feature: `gtk::Snapshot::new()` needs a live GTK, so these
/// bodies cannot run under a plain `cargo test`. `#[gtktest::test]`, never `#[gtk::test]`
/// — the latter is rejected by `cargo xtask lint-references` check 5 and would leave the
/// bodies absent from the portable main-thread run (`sdd/POLICY.md`).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tile_tests {
    use super::{paint_band_into, tile_texture};
    use gtk::graphene;

    /// A 2×2 texture, built from bytes rather than decoded — no display, no loader.
    fn tex() -> gtk::gdk::Texture {
        use gtk::glib::object::Cast;
        let bytes = gtk::glib::Bytes::from_owned(vec![0xffu8; 2 * 2 * 4]);
        gtk::gdk::MemoryTexture::new(2, 2, gtk::gdk::MemoryFormat::R8g8b8a8, &bytes, 2 * 4)
            .upcast::<gtk::gdk::Texture>()
    }

    /// **A degenerate rect or texture paints NOTHING.**
    ///
    /// Two of the three call sites this seam replaced had no such guard, so folding it
    /// in here is the whole point of there being a seam. Asserted on the produced render
    /// node: an empty snapshot yields `None`, and a repeat node over a zero-area rect is
    /// exactly the shape that reads as "the decoration is missing" with no warning.
    #[gtktest::test]
    fn a_zero_dimension_produces_no_node_at_all() {
        use gtk::prelude::SnapshotExt;
        let t = tex();
        // Zero only: `graphene::Rect::new` NORMALISES a negative extent (a
        // `(0, 0, -4, 10)` rect comes back as `(-4, 0, 4, 10)`), so a negative width
        // cannot reach the guard through this constructor and asserting it would be
        // asserting graphene's behaviour, not ours. The guard still tests `<= 0.0`,
        // because a rect built some other way is not this constructor's promise.
        for rect in [
            graphene::Rect::new(0.0, 0.0, 0.0, 10.0),
            graphene::Rect::new(0.0, 0.0, 10.0, 0.0),
        ] {
            let snapshot = gtk::Snapshot::new();
            tile_texture(&snapshot, &rect, &t);
            assert!(
                snapshot.to_node().is_none(),
                "a {rect:?} tile must paint nothing"
            );
        }
        // The control: a real rect DOES produce a node, so the assertions above are
        // about the guard and not about the seam painting nothing ever.
        let snapshot = gtk::Snapshot::new();
        tile_texture(&snapshot, &graphene::Rect::new(0.0, 0.0, 10.0, 10.0), &t);
        assert!(snapshot.to_node().is_some());
    }

    /// **The tile grid is anchored at `(rect.x(), a document grid line)`, and BOTH halves
    /// are load-bearing.**
    ///
    /// This test used to assert the opposite of the truth. It drove a `TileOrigin` enum
    /// and asserted that the `codeview` sites' choice — the rect's own top-left — was
    /// what made "the phase travel with the document". It does not:
    /// `codeview::geometry::span_card_y_extent` clamps a span's `top` to the viewport, so
    /// anchoring at the rect nails the pattern to the SCREEN. Measured on the blockquote
    /// bar as a bar column that stayed pixel-identical (AE=0) across a 176px scroll while
    /// the text scrolled under it. The enum is gone; this asserts the one anchor left.
    ///
    /// Three assertions, because the axes fail differently and independently:
    /// y must be ON the document's own grid (a multiple of the tile height — what
    /// survives the viewport clamp and keeps the phase travelling with the text), y must
    /// also be WITHIN one tile of the rect (the literal `0` is on the grid but paints
    /// nothing once it is 32768 px away — see `tile_texture`'s docs), and x must be the
    /// rect's own (a `0` there samples the tile at `rect.x() % tile_w` and slices the
    /// sprite horizontally at every decoration that does not begin on a tile boundary —
    /// the bug a fix for the y half alone introduces, and one that is visible in a
    /// screenshot).
    ///
    /// **Mutation check (all killed, singly):** `(0.0, 0.0)` fails the x assertion;
    /// `(rect.x(), rect.y())` fails the on-grid assertion; `(rect.x(), 0.0)` fails the
    /// proximity assertion. None is visible in `RenderNode`'s `Debug`, which prints only
    /// the node's OWN bounds — identical under every anchor, so a formatted comparison
    /// would pass whatever this code did (GEP-10).
    #[gtktest::test]
    fn the_tile_grid_is_anchored_at_the_rects_x_and_a_document_grid_line() {
        let t = tex();
        let tile_h = {
            use gtk::gdk::prelude::TextureExt;
            t.height() as f32
        };
        // A rect at neither axis' origin, and a y far enough down that the on-grid and
        // proximity assertions cannot both be satisfied by the literal 0.
        let rect = graphene::Rect::new(3.0, 40007.0, 10.0, 10.0);
        let (x, y) = {
            use gtk::prelude::SnapshotExt;
            let snapshot = gtk::Snapshot::new();
            tile_texture(&snapshot, &rect, &t);
            let node = snapshot.to_node().expect("a real rect renders");
            let repeat = node
                .downcast::<gtk::gsk::ClipNode>()
                .expect("tile_texture emits a clip over the repeat")
                .child()
                .downcast::<gtk::gsk::RepeatNode>()
                .expect("tile_texture emits a repeat node inside the clip");
            // The CHILD bounds of the repeat node, which is where the phase lives.
            let child = repeat.child_bounds();
            (child.x(), child.y())
        };
        assert_eq!(
            y % tile_h,
            0.0,
            "the tile grid must anchor y on the DOCUMENT's own grid — a multiple of the \
             tile height: that is what survives span_card_y_extent's viewport clamp and \
             keeps the phase travelling with the text as the reader scrolls (got {y})"
        );
        assert!(
            y <= rect.y() && rect.y() - y < tile_h,
            "the tile grid must anchor y WITHIN ONE TILE at or above the rect: the Cairo \
             renderer draws a repeat node as nothing once its child bounds sit 32768 px \
             from its bounds, so a decoration far down a long document vanishes with no \
             warning (got {y} for a rect at {})",
            rect.y()
        );
        assert_eq!(
            x,
            rect.x(),
            "the tile grid must take x from the RECT: a 0 here samples the tile at \
             rect.x() % tile_w and slices the sprite down its left edge (got {x})"
        );
    }

    /// **The tiles paint the rect and nothing outside it** (`tile_texture`'s docs, "The
    /// repeat covers more than the rect").
    ///
    /// Drawn through Cairo directly (`RenderNode::draw`), the path the app's
    /// `GSK_RENDERER=cairo` takes, onto a surface much taller than the rect, so the clip in
    /// force around the node is the surface: the condition under which GTK 4.22's Cairo
    /// repeat stretched each strip to the bottom of the view. The rect crosses a grid
    /// line, which is what made the second strip.
    ///
    /// Mutation checks: without the clip, this fails on GTK 4.6.9 (Linux), which fills
    /// the whole grown repeat. With the old body (a repeat over the rect alone, no clip)
    /// it is expected to fail on GTK 4.22.4 (the Windows and macOS legs) and passes on
    /// GTK 4.6.9, whose Cairo repeat never painted outside its own bounds.
    #[gtktest::test]
    fn the_tiles_paint_the_rect_and_nothing_outside_it() {
        use gtk::prelude::SnapshotExt;
        let bytes = gtk::glib::Bytes::from_owned(vec![0xffu8; 8 * 8 * 4]);
        let t: gtk::gdk::Texture = gtk::glib::object::Cast::upcast(gtk::gdk::MemoryTexture::new(
            8,
            8,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            8 * 4,
        ));
        // Rows 5..11 cross the grid line at 8.
        let rect = graphene::Rect::new(4.0, 5.0, 20.0, 6.0);
        let snapshot = gtk::Snapshot::new();
        tile_texture(&snapshot, &rect, &t);
        let node = snapshot.to_node().expect("a real rect renders");

        let (w, h) = (32, 48);
        let mut surface =
            gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h).unwrap();
        {
            let cr = gtk::cairo::Context::new(&surface).unwrap();
            node.draw(&cr);
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let alpha = |x: i32, y: i32| data[y as usize * stride + x as usize * 4 + 3];
        let mut outside = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let inside = (4..24).contains(&x) && (5..11).contains(&y);
                if inside {
                    assert_eq!(alpha(x, y), 0xff, "the rect is painted at ({x}, {y})");
                } else if alpha(x, y) != 0 {
                    outside.push((x, y));
                }
            }
        }
        assert!(
            outside.is_empty(),
            "nothing is painted outside the rect; {} pixels were, first {:?}",
            outside.len(),
            outside.first()
        );
    }

    /// **A corner-anchored scene lands in the corner the theme named, at its own size.**
    ///
    /// Driven through `paint_band_into` rather than `draw_scene_corner` directly, because
    /// the thing worth pinning is the DISPATCH: a band whose theme states an anchor must
    /// take the corner path, and one that states none must keep the fitted path. Asserted
    /// on the produced texture node's bounds, which is where the placement actually
    /// lands — the arithmetic itself is unit-tested display-free in `theme::value`.
    ///
    /// ⚠️ The unanchored control is the load-bearing half: without it, a painter that
    /// pinned EVERY scene to a corner would satisfy all four corner assertions and break
    /// every theme shipped before this key (TDD 18.2).
    #[gtktest::test]
    fn a_scene_takes_the_corner_its_anchor_names_and_none_without_one() {
        use crate::theme::SceneAnchor;
        use gtk::prelude::SnapshotExt;

        // A COMPILED-IN sprite, so the pixels come from this binary rather than from a
        // file an admission check would have to let through first — the same route the
        // sink sweep takes for exactly that reason (`theme::tests::sinks`).
        let sprite = crate::sprite::SpriteRef::Compiled("sprites/copper-plate.png");
        let (sw, sh) = (24.0, 24.0); // copper-plate's own size; drawn at zoom 1.0
        let rect = graphene::Rect::new(100.0, 200.0, 400.0, 40.0);
        let bounds_for = |anchor: Option<SceneAnchor>| {
            let decor = crate::theme::Band {
                sprite: None,
                scene: Some(&sprite),
                scene_anchor: anchor,
                gradient: None,
                flat: None,
            };
            let snapshot = gtk::Snapshot::new();
            paint_band_into(
                &snapshot,
                &rect,
                &decor,
                0.0,
                1.0,
                crate::animation::sprites::Frames::still(),
            );
            let node = snapshot.to_node().expect("a scene paints a node");
            node.bounds()
        };
        for (anchor, want) in [
            (SceneAnchor::TopLeft, (rect.x(), rect.y())),
            (
                SceneAnchor::TopRight,
                (rect.x() + rect.width() - sw, rect.y()),
            ),
            (
                SceneAnchor::BottomLeft,
                (rect.x(), rect.y() + rect.height() - sh),
            ),
            (
                SceneAnchor::BottomRight,
                (rect.x() + rect.width() - sw, rect.y() + rect.height() - sh),
            ),
        ] {
            let b = bounds_for(Some(anchor));
            assert_eq!((b.x(), b.y()), want, "{anchor:?} landed at {b:?}");
        }
        // Unanchored: FITTED to the band's height, so the box is as tall as the rect —
        // a different rendering, not a different position.
        let fitted = bounds_for(None);
        assert_eq!(
            fitted.height(),
            rect.height(),
            "an unanchored scene is fitted to the band's height: {fitted:?}"
        );
    }
}
