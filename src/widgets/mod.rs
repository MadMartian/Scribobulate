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
//! - [`rule`] — `SpriteRule`, the horizontal rule when a theme tiles a sprite across
//!   it (TDD 18.31). Built only where the theme states one; the flat rule stays the
//!   stock `GtkSeparator` it has always been.
//! - [`textfield`] — the constructors every `GtkEntry`/`GtkSearchEntry` in the
//!   application comes from, so the two silent follow-ups a hand-built field owes
//!   (accessible name; macOS word navigation) cannot be forgotten one surface at a
//!   time.

use gtk::prelude::*;

pub(crate) mod comment_entry;
pub(crate) mod disclosure;
pub(crate) mod rule;
pub(crate) mod tab;
pub(crate) mod table;
pub(crate) mod textfield;

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
/// The tile grid is anchored at `(rect.x(), 0.0)` — x from the rect, y from the DOCUMENT
/// origin — and every tiling site in the tree wants that pair, so it is baked in rather
/// than chosen per call. It replaced a `TileOrigin` enum whose two answers were "the
/// rect's own top-left" and `(0, 0)`; both were wrong, in different ways, and the enum's
/// own documentation asserted the opposite of what each did.
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
/// **Why y must be the document origin, not the rect's top.** Inside a `GtkTextView`'s
/// `snapshot_layer` the current transform is already `translate(-xoffset, -yoffset)`
/// (`gtktextview.c:5871-5873`), and a pure translate bakes into the rects rather than
/// wrapping a transform node — so `y = 0` bakes to `-yoffset`, putting plate row
/// `yoffset % tile_h` at the top of the viewport, a phase that travels with the text.
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
/// the off-screen unvalidated-iter read ScrAP-22 bans. `0` is a coordinate, not an iter,
/// so this anchor needs no such read.
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
    let tile = gtk::graphene::Rect::new(rect.x(), 0.0, tw as f32, th as f32);
    snapshot.push_repeat(rect, Some(&tile));
    snapshot.append_texture(tex, &tile);
    snapshot.pop();
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
/// Returns `false` — painting nothing — when the rect is degenerate or the sprite will
/// not resample, which is this vocabulary's inert-by-default failure: the caller then
/// draws whatever the decoration would have been without a sprite.
pub(crate) fn draw_sprite_into(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    sprite: &crate::sprite::SpriteRef,
) -> bool {
    let w = rect.width().round() as i32;
    let h = rect.height().round() as i32;
    if w <= 0 || h <= 0 {
        return false;
    }
    let Some(tex) = crate::sprite::scaled(sprite, w, h) else {
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
    use super::tile_texture;
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

    /// **The tile grid is anchored at `(rect.x(), 0)`, and BOTH halves are load-bearing.**
    ///
    /// This test used to assert the opposite of the truth. It drove a `TileOrigin` enum
    /// and asserted that the `codeview` sites' choice — the rect's own top-left — was
    /// what made "the phase travel with the document". It does not:
    /// `codeview::geometry::span_card_y_extent` clamps a span's `top` to the viewport, so
    /// anchoring at the rect nails the pattern to the SCREEN. Measured on the blockquote
    /// bar as a bar column that stayed pixel-identical (AE=0) across a 176px scroll while
    /// the text scrolled under it. The enum is gone; this asserts the one anchor left.
    ///
    /// Two assertions, because the two axes fail differently and independently:
    /// y must be the document origin (what survives the viewport clamp and keeps the
    /// phase travelling with the text), and x must be the rect's own (a `0` there samples
    /// the tile at `rect.x() % tile_w` and slices the sprite horizontally at every
    /// decoration that does not begin on a tile boundary — the bug a fix for the y half
    /// alone introduces, and one that is visible in a screenshot).
    ///
    /// **Mutation check (both killed, singly):** `(0.0, 0.0)` fails the x assertion;
    /// `(rect.x(), rect.y())` fails the y assertion. Neither is visible in `RenderNode`'s
    /// `Debug`, which prints only the node's OWN bounds — identical under every anchor,
    /// so a formatted comparison would pass whatever this code did (ScrAP-325).
    #[gtktest::test]
    fn the_tile_grid_is_anchored_at_the_rects_x_and_the_document_origin() {
        let t = tex();
        // A rect at neither axis' origin, or an assertion below passes by coincidence.
        let rect = graphene::Rect::new(3.0, 7.0, 10.0, 10.0);
        let (x, y) = {
            use gtk::prelude::SnapshotExt;
            let snapshot = gtk::Snapshot::new();
            tile_texture(&snapshot, &rect, &t);
            let node = snapshot.to_node().expect("a real rect renders");
            let repeat = node
                .downcast::<gtk::gsk::RepeatNode>()
                .expect("tile_texture emits a repeat node");
            // The CHILD bounds of the repeat node, which is where the phase lives.
            let child = repeat.child_bounds();
            (child.x(), child.y())
        };
        assert_eq!(
            y, 0.0,
            "the tile grid must anchor y at the DOCUMENT origin: that is what survives \
             span_card_y_extent's viewport clamp and keeps the phase travelling with the \
             text as the reader scrolls (got {y})"
        );
        assert_eq!(
            x,
            rect.x(),
            "the tile grid must take x from the RECT: a 0 here samples the tile at \
             rect.x() % tile_w and slices the sprite down its left edge (got {x})"
        );
    }
}
