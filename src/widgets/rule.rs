//! `SpriteRule` — the horizontal rule when a theme fills it with a tiled sprite
//! (TDD 18.31).
//!
//! # Why a widget at all
//!
//! The `---` rule is the one decoration in the theme vocabulary drawn as a real
//! anchored widget: a stock `GtkSeparator` recoloured through generated CSS
//! (`preview/css.rs`'s `separator.scrib-rule` rule — mechanism C in
//! [THEMING.md](../../sdd/THEMING.md)). Every other sprite-able decoration is either a
//! `GtkTextTag` or something `codeview` draws itself, so every other sprite reaches the
//! screen through `snapshot`/`push_repeat` already.
//!
//! CSS cannot carry this one. A GTK CSS `url()` needs a resource or file path, and a
//! built-in theme's sprite is **compiled into the binary** — `include_bytes!` bytes with
//! no path anywhere (ScrAP-324). Giving one a path again is the exact defect that entry
//! records: a decoration resolved against a runtime directory is absent everywhere that
//! directory is not, silently, because an unresolved sprite is inert by design. So the
//! tile has to be painted rather than declared, and `gtk_snapshot_push_repeat` +
//! `append_texture` is how this codebase already paints every other one.
//!
//! # Why a second widget rather than a smarter separator
//!
//! `gtk::Separator` is not subclassable from gtk4-rs — the bindings ship no
//! `SeparatorImpl`, so it cannot be a `ParentType`. Replacing the separator outright
//! for *every* theme would have meant reproducing, in our own `measure`, the 1px
//! `min-height` the desktop theme gives a `separator` node — a literal where a theme's
//! value used to be, and a System-parity regression (TDD 18.2) for the sake of a
//! decoration no shipped theme but one asks for.
//!
//! So the flat rule stays exactly the `GtkSeparator` it has always been, and this
//! widget is built **only** where `Theme::rule_decor` answered with a sprite that
//! decoded. "Unstated ⇒
//! byte-identical" is then true by construction rather than by care: the untouched path
//! is the untouched code.
//!
//! # Sizing
//!
//! `ConstantSize`, with a natural height of the texture's own height in device pixels —
//! the tile's NATURAL size, which is what "tiled at its natural size" means everywhere
//! else in this vocabulary and what keeps pixel art crisp (GSK 4.6's `append_texture`
//! offers no filter choice, so a texture drawn 1:1 is never resampled at all —
//! GTK4Rs/AP-114). It is deliberately **not** scaled by zoom: the tile is not a themed
//! pixel metric, it is an image, and the blockquote bar's tile is likewise unscaled
//! while only its clip box follows the zoom.
//!
//! Width is left at zero minimum so the view's own bound (`width_bounded` in
//! `renderer::events`, the GTK4Rs/AP-23a inset) is what decides it, exactly as it does
//! for the separator this stands in for.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::ObjectSubclassIsExt;

mod imp {
    use super::*;
    use gtk::gdk;
    use gtk::graphene;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub(crate) struct SpriteRule {
        pub(crate) tile: RefCell<Option<gdk::Texture>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SpriteRule {
        const NAME: &'static str = "ScribSpriteRule";
        type Type = super::SpriteRule;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SpriteRule {}

    impl WidgetImpl for SpriteRule {
        /// Height never depends on width, so the anchored-child re-measure loop
        /// GTK4Rs/AP-23 warns about cannot start here.
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                // The caller bounds the width; asking for none of our own is what lets
                // it, and is what a horizontal `GtkSeparator` does too.
                return (0, 0, -1, -1);
            }
            let h = self
                .tile
                .borrow()
                .as_ref()
                .map(|t| t.height())
                .unwrap_or(0)
                .max(1);
            (h, h, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let tile = self.tile.borrow();
            let Some(tex) = tile.as_ref() else {
                return;
            };
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            // The SAME seam the heading band and the blockquote bar tile through, which
            // is what `sdd/THEMING.md` has claimed all along. The zero-dimension guard
            // this site used to carry alone now lives inside it, so the two `codeview`
            // sites cannot go on lacking it. Clipped to the widget, so a tile taller
            // than the rule shows a slice of itself rather than overflowing — the
            // documented consequence the bar's `blockquote_bar_width` has, here decided
            // by the tile itself.
            let rect = graphene::Rect::new(0.0, 0.0, w, h);
            crate::widgets::tile_texture(
                snapshot, &rect,
                // A standalone widget: its rect IS its own coordinate space, so the
                // origin and the rect coincide and the phase is fixed either way.
                tex,
            );
        }
    }
}

glib::wrapper! {
    pub(crate) struct SpriteRule(ObjectSubclass<imp::SpriteRule>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SpriteRule {
    /// A rule filled with `tile`, tiled at the texture's natural size.
    ///
    /// Takes the decoded texture rather than the `SpriteRef` so the decision "does this
    /// theme have a rule sprite, and did it decode?" stays at the ONE call site that
    /// chooses between this widget and a `GtkSeparator` — a constructor that could
    /// return an undecodable rule would put a second, silent way to get a blank one.
    pub(crate) fn new(tile: gtk::gdk::Texture) -> Self {
        let obj: Self = glib::Object::new();
        // A rule is decoration: it carries no text and names nothing, so it announces
        // as a separator exactly as the `GtkSeparator` it stands in for does.
        obj.set_accessible_role(gtk::AccessibleRole::Separator);
        obj.imp().tile.replace(Some(tile));
        obj
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// An opaque magenta 2×3 texture, built from raw bytes rather than a file.
    ///
    /// Non-square deliberately: a fixture whose width equals its height cannot tell a
    /// transposed `measure` from a correct one.
    fn tile() -> gtk::gdk::Texture {
        const W: i32 = 2;
        const H: i32 = 3;
        let pixels: Vec<u8> = [0xffu8, 0x00, 0xff, 0xff].repeat((W * H) as usize);
        gtk::gdk::MemoryTexture::new(
            W,
            H,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(pixels),
            (W * 4) as usize,
        )
        .upcast()
    }

    /// TDD 18.31 — the rule takes the TILE's natural height, on the vertical axis only.
    ///
    /// The horizontal half is the load-bearing one: a rule that requested its tile's
    /// width would stop the view's own width bound from making it span the column, and
    /// on a nested rule would overflow it (GTK4Rs/AP-23a).
    #[gtktest::test]
    fn a_sprite_rule_measures_the_tiles_height_and_asks_for_no_width() {
        let rule = SpriteRule::new(tile());
        let (min_h, nat_h, _, _) = WidgetExt::measure(&rule, gtk::Orientation::Vertical, -1);
        assert_eq!((min_h, nat_h), (3, 3), "the tile's own height, unscaled");
        let (min_w, nat_w, _, _) = WidgetExt::measure(&rule, gtk::Orientation::Horizontal, -1);
        assert_eq!(
            (min_w, nat_w),
            (0, 0),
            "a rule must claim no width of its own — the view bounds it"
        );
    }

    /// TDD 18.31 — the tile actually reaches the framebuffer, REPEATED.
    ///
    /// A `snapshot` that produced no node at all would leave a rule-shaped gap and pass
    /// every measurement assertion above, so this renders the widget and counts the
    /// tile's colour: at 2px wide, a 40px rule can only be that many pixels of magenta
    /// if the node repeated.
    #[gtktest::test]
    fn a_sprite_rule_tiles_across_its_whole_width() {
        let rule = SpriteRule::new(tile());
        rule.set_size_request(40, -1);
        let win = gtk::Window::new();
        win.set_child(Some(&rule));
        win.present();
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the rule to be allocated its requested width",
            || rule.width() >= 40,
        );

        // The widget's own `snapshot` vfunc — GTK exposes no public "render this widget"
        // call, and going through the imp is what lets the assertion be about THIS
        // widget's node rather than a parent's composite.
        let snapshot = gtk::Snapshot::new();
        gtk::subclass::prelude::WidgetImpl::snapshot(rule.imp(), &snapshot);
        let node = snapshot.to_node().expect("the rule drew nothing at all");
        let renderer = gtk::gsk::CairoRenderer::new();
        renderer
            .realize(None::<&gtk::gdk::Surface>)
            .expect("realize");
        let texture = renderer.render_texture(&node, None);
        renderer.unrealize();
        win.destroy();

        let (w, h) = (texture.width() as usize, texture.height() as usize);
        let mut bytes = vec![0u8; w * h * 4];
        texture.download(&mut bytes, w * 4);
        let magenta = bytes
            .chunks_exact(4)
            .filter(|px| px[3] == 0xff && px[2] > 0x80 && px[0] > 0x80 && px[1] < 0x60)
            .count();
        assert!(
            magenta >= 40 * 3,
            "the tile covered {magenta} pixels of a {w}×{h} rule — a single \
             un-repeated node covers only its own {}",
            2 * 3
        );
    }
}
