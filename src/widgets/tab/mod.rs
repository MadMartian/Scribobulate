//! `TabBar`/`TabView` — a self-contained, gtk-rs-native replacement for the
//! `GtkNotebook`-based tab strip. Kills the GTK4Rs/AP-60 crash
//! class by construction (no `GtkNotebook` anywhere, so its unguarded
//! `dnd_finished_cb` NULL deref can never fire) and adds the two features
//! `GtkNotebook` could never host cleanly: a per-tab `×` close button (N1)
//! and a per-tab right-click context menu (N2).
//!
//! This module is the living record of a design doc that no longer exists. The
//! "the retired tab-widget plan §A/§B/§E/§G / gotcha #N" citations below name
//! sections of that retired text and cannot be followed; the design and rationale
//! they refer to are captured here in this module doc and in `sdd/TECH.md`'s
//! tab-widget row.
//!
//! ## File layout (decomposed from the former monolithic `window/tabwidget.rs`)
//!
//! - [`imp`] — the `GObject` subclass: the imp struct, the `GtkScrollable`
//!   property overrides, and the `measure`/`size_allocate`/`snapshot` vfuncs.
//! - [`layout`] — the pure, GTK-free geometry/decision arithmetic (gutter
//!   reservation, hit-testing, reorder index, scroll reveal, easing), unit-tested
//!   without a display and IN the coverage gate — the same split as
//!   `widgets::table::{mod,layout}`.
//! - [`bar`] — [`TabBar`] construction, tab bookkeeping, and callback registration.
//! - [`ops`] — [`TabBar`]'s add/remove/switch/reorder mutations and the
//!   sibling-slide animation.
//! - [`view`] — the [`TabView`] façade (strip + `GtkStack`) and its weak twin.
//!
//! `TabBar` is a genuine `GtkWidget` subclass (the strip: one handle per tab
//! PLUS the prev/next overflow chevrons, all laid out and animated in its own
//! `measure`/`size_allocate`, mirroring `ScribTableWidget`'s subclassing
//! pattern — see `widgets/table`) that ALSO implements `GtkScrollable` — the
//! horizontal scroll position is a genuine `GtkAdjustment`, not a hand-rolled
//! `Cell<f64>` offset. `TabBar` creates and owns this `GtkAdjustment` itself
//! (`TabBar::new`, self-assigned via `ScrollableExt::set_hadjustment` so it
//! flows through the same property path an external `GtkScrolledWindow`
//! would use) and is placed as a PLAIN child of `TabView`'s outer `GtkBox` —
//! deliberately **not** wrapped in a real `GtkScrolledWindow`. See the
//! "GtkScrolledWindow wrap: tried and reverted" paragraph below for why.
//! `TabView` is a plain Rust façade pairing the strip with a `GtkStack` (tab
//! bodies), exposing a `GtkNotebook`-shaped API (`append_page`, `n_pages`,
//! `set_current_page`, …) so `window/tabs/`'s call sites translate
//! mechanically (the retired tab-widget plan §"API surface").
//!
//! **`GtkScrolledWindow` wrap: tried and reverted.** the retired tab-widget plan §G's
//! literal recipe wraps the `Scrollable` child in a real `GtkScrolledWindow`.
//! Implemented and live-tested; researcher-confirmed (GTK 4.6.9 source,
//! `gtkscrolledwindow.c`) findings on why it made things WORSE, not better:
//! (1) `gtk_scrolled_window_measure` unconditionally measures BOTH internal
//! scrollbars before the policy check that hides them (:1559-1568) — even
//! under `PolicyType::External` the internal `GtkScrollbar`'s `(slider)`
//! gizmo is measured every layout pass, and its CSS node's negative slider
//! margins make that measurement legitimately go negative, logging
//! `Gtk-WARNING: GtkGizmo ... (slider) reported min width -2` continuously
//! (benign — `gtksizerequest.c` clamps it to 0 — but noisy and a red flag
//! that the widget tree underneath is being re-walked far more than
//! expected). (2) Live testing showed clicks on tab handles stopped
//! registering and the `GtkOverlay`-snapshot-without-allocation warning
//! (GTK4Rs/AP-104) started firing continuously (every few seconds, with
//! NO user interaction at all) instead of only on a scroll-triggering
//! switch — a real, easily-reproduced regression, not a rare edge case.
//! Wrapping in a `GtkScrolledWindow` interposes a `GtkViewport` and hands
//! `TabBar` viewport-driven allocation/pick semantics it doesn't need (this
//! widget already does its own overflow clipping, its own chevron-based
//! affordance, and was going to drive its own wheel handling regardless) —
//! all of Problem 1 and 2's machinery evaporates by simply not adding the
//! `GtkScrolledWindow`. `TabBar` still implements `gtk::Scrollable` properly
//! (so a future caller COULD embed it in a real `GtkScrolledWindow` without
//! code changes), it just isn't asked to live inside one itself.
//!
//! One deliberate simplification versus the plan's literal recipe (see
//! `sdd/ANTI-PATTERNS.md` for the full writeup): **no raw `gdk_drag_begin`.**
//! In-strip reorder and cross-window drag both ride the SAME
//! `GtkDragSource`/`GtkDropTarget` pair (attached once to the whole bar, in
//! `window/tabs/`'s `wire_tab_bar_dnd`), reusing the exact mechanism the
//! shipped `GtkNotebook` hybrid already proved correct (u64 tab-id payload,
//! `NoTarget` → spawn a new window). The "reorder vs. detach" distinction the
//! plan gets from geometry (`check_dnd_threshold`) falls out for free: while
//! the drag hovers over ITS OWN bar, `connect_motion` drives the live reorder
//! preview ([`TabBar::preview_reorder`]); once it leaves (or drops
//! elsewhere), the existing drop/cancel handlers take over. There is no
//! second competing gesture to arbitrate against (the original GTK4Rs/AP-60-hybrid's
//! whole problem), so the old Shift-gate is retired outright.
//!
//! **The prev/next chevrons are `TabBar`'s own children, allocated in its own
//! `size_allocate`, permanently GTK-`visible`, always at their true natural
//! size, and "hidden" purely by being moved outside `[0, width)` where
//! `set_overflow(Hidden)` clips them — the same mechanism a scrolled-off tab
//! handle already relies on.** This took several rounds to arrive at — the
//! full account (three earlier revisions, each a source-confirmed but
//! distinct crash) is GTK4Rs/AP-104. That entry also covers the residual
//! issue this module's `GtkScrollable` implementation exists to close: a
//! hand-rolled `Cell<f64>` scroll offset (this file's original design) could
//! make `switch_to_index`'s `queue_allocate()` land in the exact same
//! synchronous call as the `GtkStack` content swap it also triggers,
//! occasionally producing a real, operator-witnessed content blank. Driving
//! scroll from a genuine, self-owned `GtkAdjustment` (this revision) instead
//! of a hand-rolled `Cell<f64>` is the structural fix the retired tab-widget plan's
//! §G always called for — see the "GtkScrolledWindow wrap" paragraph above
//! for why the wrapping-widget half of that recipe was tried and reverted.
//!
//! Deferred (documented, not attempted): the plan's edge-autoscroll-during-
//! drag refinement (§G's "★ reconciled by geometry" paragraph) — a real
//! enhancement, but its precise geometry reconciliation was the single
//! highest-risk, lowest-value item to hand-implement without interactive
//! verification. Dragging a tab to the strip's edge does not yet auto-scroll;
//! the prev/next chevrons remain reachable during a drag as the fallback.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};

mod bar;
mod imp;
mod layout;
mod ops;
mod view;

pub(crate) use view::TabView;

/// Horizontal gap (px) between adjacent tab handles in the strip.
const TAB_SPACING: f64 = 2.0;
/// CSS class carried by each tab handle's `×` close button. It has THREE
/// coupled roles that must never drift: `ops::add_tab` adds it to the button,
/// `bar::press_hit_close_button` hit-tests on it to suppress a tab-activate on a
/// close click (the GTK4Rs/AP-109 guard behind TDD 7.11), and `preview::css` styles the
/// hover-fade off it. Hoisted to one const so renaming it for a styling reason
/// can't silently break the behavioural guard with no compile error (QA L-1). The
/// CSS side embeds the literal in a string; `css`'s own test asserts the CSS
/// still mentions this const so the two can't diverge unnoticed.
pub(crate) const TAB_CLOSE_BTN_CLASS: &str = "tab-close-btn";
/// Time constant (s) of the exponential ease used for the sibling-slide
/// reorder animation — smaller is snappier.
const EASE_TAU: f64 = 0.055;

/// The natural (preferred) width of `w` in px. Names the 4-tuple `measure`
/// returns so no call site reaches for a positional `.1` tuple access (D1); the
/// strip's whole layout arithmetic is done in these natural handle widths.
fn natural_width(w: &impl IsA<gtk::Widget>) -> f64 {
    let (_min, natural, _min_baseline, _nat_baseline) = w.measure(gtk::Orientation::Horizontal, -1);
    natural as f64
}

glib::wrapper! {
    struct TabBar(ObjectSubclass<imp::TabBar>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}
