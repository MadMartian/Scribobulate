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
pub(crate) mod rule;
pub(crate) mod tab;
pub(crate) mod table;
pub(crate) mod textfield;

/// Where a tiled sprite's grid is anchored — the decision the three former copies of
/// this sequence made differently, and none of them stated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TileOrigin {
    /// Anchor at the rect's own top-left. Correct for a decoration whose rect is
    /// already in the coordinate space being painted (a standalone widget), and it keeps
    /// the tile phase fixed relative to the decoration.
    Rect,
    /// Anchor at the WIDGET's origin. Correct inside a `GtkTextView`'s `snapshot_layer`,
    /// where the rect is in BUFFER coordinates: anchoring at the rect would hold the
    /// phase to the document, so the pattern would shift under the decoration as the
    /// view scrolls.
    Widget,
}

/// Tile `tex` across `rect` at the texture's NATURAL size.
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
/// Natural size, not stretched: 1:1 pixels need no filter, and GSK 4.6's
/// `append_texture` filters linearly with no choice (the variant that takes one is 4.10,
/// GTK4Rs/AP-114). Tiling also means one cached texture per reference instead of one per
/// decoration width, which a window resize would otherwise mint by the hundred.
///
/// A zero or negative dimension — on either the rect or the texture — paints NOTHING
/// rather than asking GSK for a degenerate repeat node. Two of the three call sites had
/// no such guard; folding it in here is what makes the omission unrepresentable.
pub(crate) fn tile_texture(
    snapshot: &gtk::Snapshot,
    rect: &gtk::graphene::Rect,
    origin: TileOrigin,
    tex: &gtk::gdk::Texture,
) {
    use gtk::gdk::prelude::TextureExt;
    let (tw, th) = (tex.width(), tex.height());
    if rect.width() <= 0.0 || rect.height() <= 0.0 || tw <= 0 || th <= 0 {
        return;
    }
    let (ox, oy) = match origin {
        TileOrigin::Rect => (rect.x(), rect.y()),
        TileOrigin::Widget => (0.0, 0.0),
    };
    let tile = gtk::graphene::Rect::new(ox, oy, tw as f32, th as f32);
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
    use super::{tile_texture, TileOrigin};
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
            tile_texture(&snapshot, &rect, TileOrigin::Rect, &t);
            assert!(
                snapshot.to_node().is_none(),
                "a {rect:?} tile must paint nothing"
            );
        }
        // The control: a real rect DOES produce a node, so the assertions above are
        // about the guard and not about the seam painting nothing ever.
        let snapshot = gtk::Snapshot::new();
        tile_texture(
            &snapshot,
            &graphene::Rect::new(0.0, 0.0, 10.0, 10.0),
            TileOrigin::Rect,
            &t,
        );
        assert!(snapshot.to_node().is_some());
    }

    /// **The origin is a stated decision, and the two answers differ.**
    ///
    /// The three copies this seam replaced made it differently and none of them said so:
    /// the band and the bar anchored at the rect (buffer coordinates, so the phase
    /// travels with the document) while the rule widget anchored at its own origin. A
    /// test that only drove `TileOrigin::Rect` would leave the parameter looking
    /// decorative.
    #[gtktest::test]
    fn the_two_origins_produce_different_tile_phases() {
        let t = tex();
        // A rect NOT at the origin, or the two answers coincide and this proves nothing.
        let rect = graphene::Rect::new(3.0, 7.0, 10.0, 10.0);
        // The CHILD bounds of the repeat node, which is where the phase lives.
        // `RenderNode`'s `Debug` prints only the node's OWN bounds — identical for both
        // origins — so a formatted comparison passes whatever the parameter does, which
        // is the assertion this test exists to avoid.
        let phase = |origin| {
            use gtk::prelude::SnapshotExt;
            let snapshot = gtk::Snapshot::new();
            tile_texture(&snapshot, &rect, origin, &t);
            let node = snapshot.to_node().expect("a real rect renders");
            let repeat = node
                .downcast::<gtk::gsk::RepeatNode>()
                .expect("tile_texture emits a repeat node");
            let child = repeat.child_bounds();
            (child.x(), child.y())
        };
        assert_eq!(phase(TileOrigin::Rect), (3.0, 7.0));
        assert_eq!(phase(TileOrigin::Widget), (0.0, 0.0));
    }
}
