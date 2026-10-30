//! `FloatingCard` — a small card that floats over a pane INSIDE the window, pointing at
//! something in that pane. It is the one way this application shows a transient,
//! selection- or chip-anchored card (the preview's Annotate action, the editor's
//! formatting bar, an annotation's comment card).
//!
//! # Why not a `GtkPopover`
//!
//! A `GtkPopover` is its own window-system surface, and on two of the three platforms
//! that surface is not tied to the window it belongs to:
//!
//!  * **X11 (GTK 4.6.9).** GDK creates every popup as an override-redirect child of the
//!    ROOT window (`gdksurface-x11.c`: `xparent = GDK_SCREEN_XROOTWIN`; `CWOverrideRedirect`
//!    set unconditionally for `GDK_SURFACE_POPUP`) and never sets `WM_TRANSIENT_FOR` on it.
//!    The window manager does not manage it at all, so minimizing the window, switching
//!    virtual desktop, or switching KDE activity leaves the card on screen where it was.
//!    Nothing GDK does on the toplevel's minimize or unmap reaches it, and there is no
//!    public or environment lever that changes this.
//!  * **macOS (GTK 4.22.4).** The popup is a child `NSWindow` of the toplevel. It follows
//!    the window between Spaces, but is left on screen, over other applications, when the
//!    window is minimized.
//!  * Win32 owns the popup `HWND` by the toplevel, which hides with it — the only one of
//!    the three that behaves.
//!
//! A child of the pane's `GtkOverlay` is part of the window's own surface, so nothing the
//! window manager does to the window can leave it behind: there is no second window to
//! leave. That is what the comment entry card has always been, and why it never showed
//! this defect. Measured on all three platforms before the change (GTK4Rs/AP-362).
//!
//! It drops two more popover hazards with it. An in-surface card receives keys through
//! the window, so the application's accelerators keep working while it holds focus
//! (GTK4Rs/AP-266 was a popover being its own `GtkNative`), and it is never placed by
//! screen coordinates, so an anchor off the monitor cannot trip GDK's monitor lookup
//! (GTK4Rs/AP-26).
//!
//! The cost: the card is clipped to the pane it floats over. [`place`] keeps it inside,
//! moving it to the other side of its anchor when there is no room on the preferred one.
//!
//! # The API is a popover's, deliberately
//!
//! `popup`, `popdown`, `set_pointing_to` (in the ANCHOR widget's coordinates, as a
//! popover's are in its parent's), a preferred side, and a `closed` signal emitted from the
//! `hide` vfunc exactly as `GtkPopover` emits it (GTK4Rs/AP-192), so every caller carried
//! over unchanged in meaning. The one addition is [`attach`](FloatingCardExt::attach): a
//! popover needs only a parent; this card needs a parent (the pane's overlay) AND an anchor
//! (the widget whose coordinates `pointing_to` is in), which are different widgets.
//!
//! # How it is placed
//!
//! Not by margins. The host overlay asks each child where it goes on every allocation
//! (`GtkOverlay::get-child-position`), and the card answers from its anchor rectangle
//! translated into the overlay at that moment. So a pane resize re-places it with no help,
//! and a new anchor needs only a `queue_resize` — not `queue_allocate`, which GTK 4.6 can
//! silently drop (GTK4Rs/AP-104). Reading geometry there is safe: it measures only the
//! card's own content and translates between two widgets the overlay is allocating; it
//! never asks a `GtkTextView` to validate anything (GTK4Rs/AP-22).
//!
//! # The pointer (TDD 17.64)
//!
//! A popover draws a pointer at what it is about; this card draws its own. The card widget
//! paints nothing itself: its allocation is the card BODY (which wears the card's CSS and
//! holds the content) plus a strip [`ARROW_DEPTH`] deep on the side facing the anchor. In
//! that strip it draws a small square styled like the body, turned 45° and clipped so only
//! the half that stands out shows — a widget, not a drawn triangle, because its fill and
//! border must be the body's own CSS colours and GTK 4.6 cannot read a node's background
//! colour back. [`place`] reports which edge ended up facing the anchor (it flips with the
//! card), and [`arrow_offset`] where along that edge the anchor's middle is.

use gtk::gdk;
use gtk::glib;
use gtk::gsk;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// Gap in px between the card and the rectangle it points at.
const GAP: i32 = 4;

/// Side in px of the square that, turned 45°, is the card's pointer.
const ARROW_SIDE: i32 = 10;

/// How far in px the pointer stands out from the card's edge: half the turned square's
/// diagonal, `ARROW_SIDE / √2`, rounded. The card reserves a strip this deep on the side
/// facing what it points at, so the pointer is part of its own allocation and never
/// clipped by the host.
const ARROW_DEPTH: i32 = 7;

/// The closest in px the pointer's centre may come to either end of the edge it sits on.
/// Its half-width plus a margin that clears the card's rounded corner, so the pointer
/// always joins a straight stretch of the edge.
const ARROW_END_CLEARANCE: i32 = 16;

/// Which side of its anchor a card prefers. A closed set, so the placement arithmetic has
/// no "other" case to forget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// Above the anchor, centred on it; below when there is no room above.
    Top,
    /// Left of the anchor, centred on it vertically; right when there is no room left.
    Left,
}

impl Side {
    /// The axis the pointer strip is reserved on. Fixed by the preferred side, so it does
    /// not change when the card flips, and the card's size request does not either.
    fn arrow_axis(self) -> gtk::Orientation {
        match self {
            Side::Top => gtk::Orientation::Vertical,
            Side::Left => gtk::Orientation::Horizontal,
        }
    }

    /// The edge that faces the anchor when the card sits on this side.
    fn facing_edge(self) -> Edge {
        match self {
            Side::Top => Edge::Bottom,
            Side::Left => Edge::Right,
        }
    }
}

/// The edge of the card that faces what it points at, and so carries the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// An axis-aligned rectangle in the HOST overlay's coordinates — plain data, so the
/// placement rule is testable without a display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

/// Where a card goes: its top-left corner in the host, and the edge facing its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) edge: Edge,
}

/// Where a `card_w` × `card_h` card goes, pointing at `anchor`, inside a `host_w` ×
/// `host_h` overlay.
///
/// On the preferred side when it fits; on the opposite side when it does not; and in every
/// case clamped so no part of it is outside the host (the host clips it, so a card hanging
/// off an edge would simply be cut). A card larger than the host is pinned to the top-left,
/// which shows its start rather than its middle.
pub(crate) fn place(
    anchor: Rect,
    card_w: i32,
    card_h: i32,
    host_w: i32,
    host_h: i32,
    side: Side,
) -> Placement {
    let max_x = (host_w - card_w).max(0);
    let max_y = (host_h - card_h).max(0);
    let (x, y, edge) = match side {
        Side::Top => {
            let above = anchor.y - GAP - card_h;
            let (y, edge) = if above >= 0 {
                (above, Edge::Bottom)
            } else {
                (anchor.y + anchor.h + GAP, Edge::Top)
            };
            (anchor.x + anchor.w / 2 - card_w / 2, y, edge)
        }
        Side::Left => {
            let left = anchor.x - GAP - card_w;
            let (x, edge) = if left >= 0 {
                (left, Edge::Right)
            } else {
                (anchor.x + anchor.w + GAP, Edge::Left)
            };
            (x, anchor.y + anchor.h / 2 - card_h / 2, edge)
        }
    };
    Placement {
        x: x.clamp(0, max_x),
        y: y.clamp(0, max_y),
        edge,
    }
}

/// Where along `edge` of `card` the pointer's centre goes, measured from the card's
/// top-left along that edge, so it points at the middle of `anchor`.
///
/// `None` when the anchor is not beyond that edge — a card clamped inside a pane too small
/// for it can end up over its own anchor, and a pointer there would point at nothing.
/// Otherwise the centre is held [`ARROW_END_CLEARANCE`] from both ends, or on the middle of
/// an edge too short for that.
pub(crate) fn arrow_offset(anchor: Rect, card: Rect, edge: Edge) -> Option<i32> {
    let beyond = match edge {
        Edge::Bottom => anchor.y >= card.y + card.h - ARROW_DEPTH,
        Edge::Top => anchor.y + anchor.h <= card.y + ARROW_DEPTH,
        Edge::Right => anchor.x >= card.x + card.w - ARROW_DEPTH,
        Edge::Left => anchor.x + anchor.w <= card.x + ARROW_DEPTH,
    };
    if !beyond {
        return None;
    }
    let (centre, len) = match edge {
        Edge::Top | Edge::Bottom => (anchor.x + anchor.w / 2 - card.x, card.w),
        Edge::Left | Edge::Right => (anchor.y + anchor.h / 2 - card.y, card.h),
    };
    let (lo, hi) = (ARROW_END_CLEARANCE, len - ARROW_END_CLEARANCE);
    Some(if hi < lo {
        len / 2
    } else {
        centre.clamp(lo, hi)
    })
}

/// The body's rectangle inside a `w` × `h` card whose pointer strip is on `edge`.
fn body_rect(w: i32, h: i32, edge: Edge) -> Rect {
    let d = ARROW_DEPTH;
    match edge {
        Edge::Bottom => Rect {
            x: 0,
            y: 0,
            w,
            h: (h - d).max(0),
        },
        Edge::Top => Rect {
            x: 0,
            y: d,
            w,
            h: (h - d).max(0),
        },
        Edge::Right => Rect {
            x: 0,
            y: 0,
            w: (w - d).max(0),
            h,
        },
        Edge::Left => Rect {
            x: d,
            y: 0,
            w: (w - d).max(0),
            h,
        },
    }
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use gtk::graphene;
    use std::cell::{Cell, RefCell};
    use std::sync::OnceLock;

    pub(crate) struct FloatingCard {
        /// The widget `pointing_to` is measured in. Weak: the anchor owns the content the
        /// card is about, never the other way round.
        pub(super) anchor: glib::WeakRef<gtk::Widget>,
        /// The overlay the card is a child of, and our `get-child-position` handler on it.
        pub(super) host: RefCell<Option<(glib::WeakRef<gtk::Overlay>, glib::SignalHandlerId)>>,
        /// `(x, y, width, height)` in the anchor's coordinates.
        pub(super) pointing_to: Cell<Option<(i32, i32, i32, i32)>>,
        pub(super) side: Cell<Side>,
        /// The card's content, held so it can be replaced.
        pub(super) child: RefCell<Option<gtk::Widget>>,
        /// The card proper: it wears the card's CSS and holds the content. The card widget
        /// itself draws nothing of its own, so its allocation can include the pointer strip.
        pub(super) body: gtk::Box,
        /// The pointer: a square styled like the body, drawn turned 45° and clipped to the
        /// strip outside the body, so only the half that stands out shows. A widget rather
        /// than a drawn shape because its fill and border must be the body's own CSS colours,
        /// and GTK 4.6 offers no way to read a node's background colour back.
        pub(super) arrow: gtk::Box,
        /// The edge the pointer is on, from the last placement.
        pub(super) edge: Cell<Edge>,
        /// The edge the body was last ALLOCATED clear of. Differs from `edge` after a flip
        /// GTK has not re-allocated the card for yet (`set_arrow`).
        pub(super) body_edge: Cell<Edge>,
        /// Where along that edge the pointer's centre is; `None` hides it.
        pub(super) arrow_at: Cell<Option<i32>>,
    }

    impl Default for FloatingCard {
        fn default() -> Self {
            Self {
                anchor: Default::default(),
                host: Default::default(),
                pointing_to: Default::default(),
                side: Cell::new(Side::Top),
                child: Default::default(),
                body: gtk::Box::new(gtk::Orientation::Vertical, 0),
                arrow: gtk::Box::new(gtk::Orientation::Vertical, 0),
                edge: Cell::new(Side::Top.facing_edge()),
                body_edge: Cell::new(Side::Top.facing_edge()),
                arrow_at: Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FloatingCard {
        const NAME: &'static str = "ScribFloatingCard";
        type Type = super::FloatingCard;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("floatingcard");
        }
    }

    impl ObjectImpl for FloatingCard {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("closed").build()])
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            self.body.add_css_class(BODY_CLASS);
            self.body.set_parent(&*obj);
            self.arrow.add_css_class(ARROW_CLASS);
            self.arrow.set_size_request(ARROW_SIDE, ARROW_SIDE);
            // Clicks on the pointer fall through to the card's edge or the page under it.
            self.arrow.set_can_target(false);
            self.arrow.set_parent(&*obj);
            // Positioned by the host's `get-child-position`; the alignment only matters
            // before the first anchor arrives, and START keeps an unanchored card from
            // filling the pane.
            obj.set_halign(gtk::Align::Start);
            obj.set_valign(gtk::Align::Start);
            obj.set_visible(false);
        }

        fn dispose(&self) {
            self.obj().disconnect_host();
            self.child.borrow_mut().take();
            self.body.unparent();
            self.arrow.unparent();
        }
    }

    impl WidgetImpl for FloatingCard {
        /// `closed` from `hide`, as `GtkPopover` does — every hide emits it, a caller's own
        /// included (GTK4Rs/AP-192).
        fn hide(&self) {
            self.parent_hide();
            self.obj().emit_by_name::<()>("closed", &[]);
        }

        /// The body's. Without this a widget with no layout manager is CONSTANT_SIZE, and
        /// GTK then measures it with `for_size` forced to -1, so content that wraps (the
        /// formatting bar in a narrow pane) is sized at its one-row height and its second
        /// row hangs outside the card (GTK4Rs/AP-366).
        fn request_mode(&self) -> gtk::SizeRequestMode {
            self.body.request_mode()
        }

        /// The body's size plus the pointer strip on the preferred side's axis.
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == self.side.get().arrow_axis() {
                let (min, nat, _, _) = self.body.measure(orientation, for_size);
                (min + ARROW_DEPTH, nat + ARROW_DEPTH, -1, -1)
            } else {
                let for_size = if for_size < 0 {
                    for_size
                } else {
                    (for_size - ARROW_DEPTH).max(0)
                };
                let (min, nat, _, _) = self.body.measure(orientation, for_size);
                (min, nat, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let edge = self.edge.get();
            self.body_edge.set(edge);
            let Rect { x, y, w, h } = body_rect(width, height, edge);
            self.body.allocate(
                w,
                h,
                -1,
                Some(gsk::Transform::new().translate(&graphene::Point::new(x as f32, y as f32))),
            );
            // Measured as GTK requires before every allocation; placed in `snapshot`, which
            // reads the latest pointer position even when GTK skips an unchanged allocation.
            self.arrow.measure(gtk::Orientation::Horizontal, -1);
            self.arrow.allocate(ARROW_SIDE, ARROW_SIDE, -1, None);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            obj.snapshot_child(&self.body, snapshot);
            let Some(at) = self.arrow_at.get() else {
                return;
            };
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let (d, at) = (ARROW_DEPTH as f32, at as f32);
            // The pointer's centre sits one pixel inside the body's edge, on its border
            // line, so the clip below keeps that one pixel and the pointer's fill covers
            // the body's border where the two join.
            let (centre, clip) = match self.edge.get() {
                Edge::Bottom => (
                    graphene::Point::new(at, h - d - 1.0),
                    graphene::Rect::new(0.0, h - d - 1.0, w, d + 1.0),
                ),
                Edge::Top => (
                    graphene::Point::new(at, d + 1.0),
                    graphene::Rect::new(0.0, 0.0, w, d + 1.0),
                ),
                Edge::Right => (
                    graphene::Point::new(w - d - 1.0, at),
                    graphene::Rect::new(w - d - 1.0, 0.0, d + 1.0, h),
                ),
                Edge::Left => (
                    graphene::Point::new(d + 1.0, at),
                    graphene::Rect::new(0.0, 0.0, d + 1.0, h),
                ),
            };
            let half = ARROW_SIDE as f32 / 2.0;
            snapshot.push_clip(&clip);
            snapshot.save();
            snapshot.translate(&centre);
            snapshot.rotate(45.0);
            snapshot.translate(&graphene::Point::new(-half, -half));
            obj.snapshot_child(&self.arrow, snapshot);
            snapshot.restore();
            snapshot.pop();
        }
    }
}

/// The CSS class of the card's body, which carries the card's look.
pub(crate) const BODY_CLASS: &str = "card-body";
/// The CSS class of the card's pointer, styled like the body.
pub(crate) const ARROW_CLASS: &str = "card-arrow";

glib::wrapper! {
    pub(crate) struct FloatingCard(ObjectSubclass<imp::FloatingCard>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// Subclassing hook: a widget that IS a floating card (the annotation comment card).
pub(crate) trait FloatingCardImpl:
    WidgetImpl + ObjectSubclass<Type: IsA<FloatingCard>>
{
}

// SAFETY: `FloatingCard` adds no class struct or vfuncs of its own beyond `GtkWidget`'s,
// so a subclass needs nothing initialised beyond what `WidgetImpl` already sets up.
unsafe impl<T: FloatingCardImpl> IsSubclassable<T> for FloatingCard {}

impl Default for FloatingCard {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingCard {
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    fn disconnect_host(&self) {
        if let Some((weak, id)) = self.imp().host.borrow_mut().take() {
            if let Some(host) = weak.upgrade() {
                host.disconnect(id);
            }
        }
    }

    /// The card's rectangle in `host`, answered from the host's own allocation pass.
    /// `None` (the overlay's default placement) when there is nothing to point at.
    fn position_in(&self, host: &gtk::Overlay) -> Option<gdk::Rectangle> {
        let (ax, ay, aw, ah) = self.imp().pointing_to.get()?;
        let anchor = self.imp().anchor.upgrade()?;
        let (hx, hy) = anchor.translate_coordinates(host, f64::from(ax), f64::from(ay))?;
        let (width, height) = self.size_within(host.width(), host.height());
        let anchor = Rect {
            x: hx as i32,
            y: hy as i32,
            w: aw,
            h: ah,
        };
        let Placement { x, y, edge } = place(
            anchor,
            width,
            height,
            host.width(),
            host.height(),
            self.imp().side.get(),
        );
        let card = Rect {
            x,
            y,
            w: width,
            h: height,
        };
        self.set_arrow(edge, arrow_offset(anchor, card, edge));
        Some(gdk::Rectangle::new(x, y, width, height))
    }

    /// Record where the pointer goes. Repaints when it moved: GTK skips the card's
    /// allocation when its rectangle is unchanged, which a card clamped against the pane's
    /// edge keeps while its anchor moves along it. A draw request is safe here, inside the
    /// host's allocation pass, where a resize request would be dropped (GTK4Rs/AP-29).
    ///
    /// A flip to the other edge also moves the BODY, which `size_allocate` places. A flip
    /// that follows `set_pointing_to`, `set_side` or `popup` gets that allocation from
    /// their `queue_resize`; a flip caused only by the host's own re-allocation (the pane
    /// resized under an open card) leaves the card's size unchanged, so GTK skips its
    /// allocation and the body stays on the old side with the pointer drawn over it. So a
    /// flip asks for the allocation itself — from an idle, after this pass, since a
    /// request made inside it would be dropped.
    fn set_arrow(&self, edge: Edge, at: Option<i32>) {
        let imp = self.imp();
        let edge_moved = imp.edge.replace(edge) != edge;
        let arrow_moved = imp.arrow_at.replace(at) != at;
        if edge_moved || arrow_moved {
            self.queue_draw();
        }
        if edge_moved && imp.body_edge.get() != edge {
            let weak = self.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(card) = weak.upgrade() {
                    if card.imp().body_edge.get() != card.imp().edge.get() {
                        card.queue_resize();
                    }
                }
            });
        }
    }

    /// The card's size inside a host `host_w` × `host_h`: its natural width when that
    /// fits, else the host's width, with the height its content needs at that width when
    /// that fits, else the host's height — never below its own minimum on either axis. So
    /// content that can wrap, wraps, and content that can scroll, scrolls, rather than
    /// being cut off by the host's clip with controls past its edge.
    fn size_within(&self, host_w: i32, host_h: i32) -> (i32, i32) {
        let (min_w, nat_w, _, _) = self.measure(gtk::Orientation::Horizontal, -1);
        let width = nat_w.min(host_w).max(min_w);
        let (min_h, nat_h, _, _) = self.measure(gtk::Orientation::Vertical, width);
        (width, nat_h.min(host_h).max(min_h))
    }
}

/// The card's operations, on the card and on every subclass of it.
pub(crate) trait FloatingCardExt: IsA<FloatingCard> + 'static {
    /// Put the card's content in. Replaces any previous content.
    fn set_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        let card = self.as_ref();
        let body = &card.imp().body;
        if let Some(old) = card.imp().child.borrow_mut().take() {
            body.remove(&old);
        }
        if let Some(child) = child {
            let child = child.as_ref().clone();
            body.append(&child);
            card.imp().child.replace(Some(child));
        }
    }

    /// Float the card over `host`, pointing in `anchor`'s coordinates. Idempotent for the
    /// same host; moving to another host removes it from the previous one first (a card
    /// that follows the active tab is re-attached on every tab switch).
    fn attach(&self, host: &gtk::Overlay, anchor: &impl IsA<gtk::Widget>) {
        let card = self.as_ref();
        card.imp().anchor.set(Some(anchor.as_ref()));
        if card.host().as_ref() == Some(host) {
            return;
        }
        card.detach();
        host.add_overlay(card);
        // Clipped to the pane, and never part of its size request: a card must not be able
        // to grow the pane it floats over.
        host.set_clip_overlay(card, true);
        host.set_measure_overlay(card, false);
        let weak = card.downgrade();
        let id = host.connect_get_child_position(move |host, child| {
            let card = weak.upgrade()?;
            if child != card.upcast_ref::<gtk::Widget>() {
                return None;
            }
            card.position_in(host)
        });
        card.imp().host.replace(Some((host.downgrade(), id)));
    }

    /// Take the card off its host, closing it first. A no-op when it has none.
    fn detach(&self) {
        let card = self.as_ref();
        card.popdown();
        let host = card.host();
        card.disconnect_host();
        if let Some(host) = host {
            host.remove_overlay(card);
        }
    }

    /// The overlay the card floats over, if attached.
    fn host(&self) -> Option<gtk::Overlay> {
        self.as_ref()
            .imp()
            .host
            .borrow()
            .as_ref()
            .and_then(|(weak, _)| weak.upgrade())
    }

    /// The widget `pointing_to` is measured in, if attached and alive.
    fn anchor(&self) -> Option<gtk::Widget> {
        self.as_ref().imp().anchor.upgrade()
    }

    /// Point at `rect`, in the anchor's coordinates; `None` clears it.
    fn set_pointing_to(&self, rect: Option<&gdk::Rectangle>) {
        let card = self.as_ref();
        card.imp()
            .pointing_to
            .set(rect.map(|r| (r.x(), r.y(), r.width(), r.height())));
        card.queue_resize();
    }

    /// What the card is pointing at, in the anchor's coordinates, or `None` when unset.
    fn pointing_to(&self) -> Option<gdk::Rectangle> {
        self.as_ref()
            .imp()
            .pointing_to
            .get()
            .map(|(x, y, w, h)| gdk::Rectangle::new(x, y, w, h))
    }

    /// Which side of its anchor the card prefers.
    fn set_side(&self, side: Side) {
        let imp = self.as_ref().imp();
        imp.side.set(side);
        // Until the next placement says otherwise, the pointer is on the edge this side
        // faces — the strip `measure` reserves is on that axis.
        imp.edge.set(side.facing_edge());
        self.as_ref().queue_resize();
    }

    /// Show the card. A no-op when it is not attached, so a card whose pane has gone cannot
    /// be shown into nothing.
    fn popup(&self) {
        let card = self.as_ref();
        if card.host().is_none() {
            return;
        }
        card.set_visible(true);
        card.queue_resize();
    }

    /// Hide the card; emits `closed`.
    fn popdown(&self) {
        self.as_ref().set_visible(false);
    }

    /// Run `f` whenever the card hides, for whatever reason.
    fn connect_closed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        // Weak: the handler lives on the card itself, so a strong capture would be a cycle.
        let weak = self.downgrade();
        self.as_ref().connect_local("closed", false, move |_| {
            if let Some(this) = weak.upgrade() {
                f(&this);
            }
            None
        })
    }

    /// Whether the press `gesture` is handling right now, at `(x, y)` in the gesture's
    /// widget, landed on this card. For an outside-click dismissal on an ancestor: a press
    /// on the card's own buttons reaches an ancestor's capture-phase gesture first, and must
    /// not count as "outside".
    fn contains_press(&self, gesture: &impl IsA<gtk::Gesture>, x: f64, y: f64) -> bool {
        let card: &gtk::Widget = self.as_ref().upcast_ref();
        let Some(widget) = gesture.as_ref().widget() else {
            return false;
        };
        widget
            .pick(x, y, gtk::PickFlags::DEFAULT)
            .is_some_and(|hit| &hit == card || hit.is_ancestor(card))
    }
}

impl<T: IsA<FloatingCard>> FloatingCardExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn at(x: i32, y: i32, edge: Edge) -> Placement {
        Placement { x, y, edge }
    }

    #[test]
    fn top_sits_above_the_anchor_centred_on_it() {
        assert_eq!(
            place(r(100, 200, 40, 20), 60, 30, 800, 600, Side::Top),
            at(90, 200 - GAP - 30, Edge::Bottom)
        );
    }

    #[test]
    fn top_moves_below_when_there_is_no_room_above() {
        assert_eq!(
            place(r(100, 10, 40, 20), 60, 30, 800, 600, Side::Top),
            at(90, 10 + 20 + GAP, Edge::Top)
        );
    }

    #[test]
    fn left_sits_left_of_the_anchor_centred_on_it() {
        assert_eq!(
            place(r(300, 200, 20, 20), 100, 40, 800, 600, Side::Left),
            at(300 - GAP - 100, 190, Edge::Right)
        );
    }

    #[test]
    fn left_moves_right_when_there_is_no_room_left() {
        assert_eq!(
            place(r(50, 200, 20, 20), 100, 40, 800, 600, Side::Left),
            at(50 + 20 + GAP, 190, Edge::Left)
        );
    }

    #[test]
    fn a_card_never_hangs_off_any_edge() {
        // Anchor at the far right and bottom: clamped back inside on both axes.
        let Placement { x, y, .. } = place(r(790, 590, 10, 10), 100, 50, 800, 600, Side::Top);
        assert!(
            x + 100 <= 800 && y + 50 <= 600,
            "({x}, {y}) leaves the host"
        );
        // Anchor off the top-left (negative coordinates): clamped to the origin.
        let Placement { x, y, .. } = place(r(-50, -80, 10, 10), 100, 50, 800, 600, Side::Top);
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn a_card_larger_than_the_host_shows_its_start() {
        let Placement { x, y, .. } = place(r(100, 100, 10, 10), 900, 700, 800, 600, Side::Top);
        assert_eq!((x, y), (0, 0));
    }

    /// The pointer is on the middle of the anchor, whichever edge it is on.
    #[test]
    fn the_pointer_points_at_the_middle_of_its_anchor() {
        // A card above, its anchor a 1 px caret at x = 130: the pointer is 30 px in.
        let card = r(100, 100, 200, 40);
        assert_eq!(
            arrow_offset(r(130, 150, 1, 16), card, Edge::Bottom),
            Some(30)
        );
        // The same card flipped below its anchor.
        assert_eq!(arrow_offset(r(130, 80, 1, 16), card, Edge::Top), Some(30));
        // A card to the left of a 20 px chip centred at y = 150: 50 px down its edge.
        let side = r(100, 100, 80, 120);
        assert_eq!(
            arrow_offset(r(190, 140, 20, 20), side, Edge::Right),
            Some(50)
        );
        assert_eq!(arrow_offset(r(70, 140, 20, 20), side, Edge::Left), Some(50));
    }

    /// Mutation: drop the clamp in `arrow_offset` and the pointer lands on the rounded
    /// corner or past the card's end.
    #[test]
    fn the_pointer_stays_clear_of_the_corners() {
        let card = r(100, 100, 200, 40);
        let far_left = arrow_offset(r(0, 150, 1, 16), card, Edge::Bottom);
        let far_right = arrow_offset(r(900, 150, 1, 16), card, Edge::Bottom);
        assert_eq!(far_left, Some(ARROW_END_CLEARANCE));
        assert_eq!(far_right, Some(200 - ARROW_END_CLEARANCE));
        // An edge too short for both clearances takes its middle.
        assert_eq!(
            arrow_offset(r(0, 150, 1, 16), r(100, 100, 20, 40), Edge::Bottom),
            Some(10)
        );
    }

    /// A card clamped over its own anchor shows no pointer: there is nothing beyond the
    /// edge for it to point at.
    #[test]
    fn a_card_over_its_own_anchor_has_no_pointer() {
        let card = r(0, 0, 200, 100);
        assert_eq!(arrow_offset(r(50, 40, 1, 16), card, Edge::Bottom), None);
        assert_eq!(arrow_offset(r(50, 40, 1, 16), card, Edge::Top), None);
        assert_eq!(arrow_offset(r(50, 40, 10, 10), card, Edge::Right), None);
        assert_eq!(arrow_offset(r(50, 40, 10, 10), card, Edge::Left), None);
    }

    /// The body leaves the pointer strip free on exactly the pointer's edge.
    #[test]
    fn the_body_leaves_the_pointer_strip_free() {
        let d = ARROW_DEPTH;
        assert_eq!(body_rect(200, 50, Edge::Bottom), r(0, 0, 200, 50 - d));
        assert_eq!(body_rect(200, 50, Edge::Top), r(0, d, 200, 50 - d));
        assert_eq!(body_rect(200, 50, Edge::Right), r(0, 0, 200 - d, 50));
        assert_eq!(body_rect(200, 50, Edge::Left), r(d, 0, 200 - d, 50));
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::testpump::{destroy_after_tooltip_delay, until, Clock};

    /// **A card wider than its pane wraps to fit it rather than being cut off.** The pane
    /// clips the card, so a one-row card wider than the pane would leave its last buttons
    /// unreachable — measured on Windows with the editor's formatting bar in a split.
    /// Mutation: size the card at its natural width in `size_within` and this fails.
    #[gtktest::test]
    fn a_card_wider_than_its_pane_wraps_inside_it() {
        let win = gtk::Window::new();
        let host = gtk::Overlay::new();
        let anchor = gtk::Box::new(gtk::Orientation::Vertical, 0);
        host.set_child(Some(&anchor));
        host.set_size_request(300, 300);
        win.set_child(Some(&host));
        win.set_default_size(300, 300);
        win.set_resizable(false);

        let row = crate::widgets::wrapbox::ToolbarWrapBox::new(2, 2);
        let buttons: Vec<gtk::Button> = (0..20)
            .map(|i| gtk::Button::with_label(&format!("B{i}")))
            .collect();
        for b in &buttons {
            row.append(b);
        }
        let card = FloatingCard::new();
        card.set_child(Some(&row));
        card.attach(&host, &anchor);
        card.set_pointing_to(Some(&gdk::Rectangle::new(150, 200, 1, 16)));
        win.present();
        card.popup();
        until(Clock::Frame, "the card to be allocated", || {
            card.is_mapped() && card.width() > 0
        });
        assert!(
            row.preferred_size().1.width() > host.width(),
            "precondition: the card's one-row width must exceed the pane, or nothing is \
             tested"
        );
        let bounds = card
            .compute_bounds(&host)
            .expect("the card is inside its pane");
        assert!(
            bounds.x() >= 0.0 && bounds.x() + bounds.width() <= host.width() as f32,
            "the card must fit inside its pane: {bounds:?} in a {} px pane",
            host.width()
        );
        for b in &buttons {
            let bb = b.compute_bounds(&host).expect("each button is laid out");
            assert!(
                bb.x() + bb.width() <= host.width() as f32,
                "every button must be inside the pane"
            );
            // And inside the card: a card sized at its one-row height leaves the wrapped
            // row drawn over the page below it (request mode lost, measured on Linux).
            assert!(
                bb.y() >= bounds.y() && bb.y() + bb.height() <= bounds.y() + bounds.height(),
                "every button must be inside the card: {bb:?} in {bounds:?}"
            );
        }
        destroy_after_tooltip_delay(&win, || card.popdown());
    }

    /// **A card the pane's own resize flips to the other side moves its body with the
    /// pointer.** The anchor sits at the pane's bottom; laying the pane out again with the
    /// anchor at its top takes away the room above it, so the card flips below without
    /// changing size, as a window resize under an open card does: the pane's layout runs,
    /// the card is asked where it goes, and nothing else tells it. GTK skips the
    /// allocation of a widget whose size is unchanged, and the body is placed only there,
    /// so without the flip's own re-allocation the body stays at the top of the card and
    /// the pointer is drawn over its first rows (TDD 17.64).
    #[gtktest::test]
    fn a_card_flipped_by_a_pane_resize_moves_its_body_clear_of_the_pointer() {
        let win = gtk::Window::new();
        let host = gtk::Overlay::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let above = gtk::Box::new(gtk::Orientation::Vertical, 0);
        above.set_size_request(300, 240);
        let anchor = gtk::Box::new(gtk::Orientation::Vertical, 0);
        anchor.set_size_request(300, 60);
        content.append(&above);
        content.append(&anchor);
        host.set_child(Some(&content));
        host.set_size_request(300, 300);
        win.set_child(Some(&host));
        win.set_resizable(false);

        let card = FloatingCard::new();
        card.set_child(Some(&gtk::Label::new(Some("card"))));
        card.attach(&host, &anchor);
        card.set_pointing_to(Some(&gdk::Rectangle::new(150, 10, 1, 16)));
        win.present();
        card.popup();
        until(
            Clock::Frame,
            "the card to be laid out above its anchor",
            || {
                card.is_mapped()
                    && card.height() > 0
                    && card.imp().edge.get() == Edge::Bottom
                    && card.imp().body_edge.get() == Edge::Bottom
            },
        );
        let size = crate::testpump::until_stable(
            Clock::Frame,
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(200),
            || (card.width(), card.height()),
        );
        assert!(size.converged, "the card's size never settled");
        let size = size.value.expect("a sampled size");

        // The pane lays its content out again with the anchor at its top: no room above,
        // so the card flips below. Nothing tells the card; only the host re-allocates.
        above.set_size_request(300, 0);
        until(
            Clock::Frame,
            "the anchor to move up and the card to flip",
            || card.imp().edge.get() == Edge::Top,
        );
        assert_eq!(
            (card.width(), card.height()),
            size,
            "precondition: the flip must leave the card's size unchanged, or GTK \
             re-allocates it anyway and nothing is tested"
        );
        crate::testpump::until_or_for(Clock::Frame, std::time::Duration::from_secs(2), || {
            card.imp().body_edge.get() == Edge::Top
        });
        let body = card
            .imp()
            .body
            .compute_bounds(&card)
            .expect("the body is inside the card");
        assert_eq!(
            body.y() as i32,
            ARROW_DEPTH,
            "the body stayed where it was before the flip, under the pointer: {body:?}"
        );
        destroy_after_tooltip_delay(&win, || card.popdown());
    }
}
