//! One adjustment write per frame for a scroller that holds a virtualized list.
//!
//! # The defect this exists for
//!
//! A `GtkListView` keeps the heights of its rows in an RB-tree, and a scroll that
//! moves the tracked window BACKWARD (upward) splits that tree —
//! `gtk_list_item_manager_ensure_items` inserts a fresh node for everything above the
//! tracked range, and a fresh node is `g_slice_alloc0`'d, so it measures **0px** until
//! the next `size_allocate` repopulates it. Within one frame GTK reads that tree back
//! (`gtk_list_view_get_position_from_allocation` clamps against the tree's LIVE height
//! while the caller computed its `y` from the adjustment's still-correct `upper`), so
//! the second write of a frame is resolved against a collapsed tree: the position it
//! lands on is roughly *doubled*, and since it compounds per write it saturates at the
//! end of the list. `gtk_list_base_set_adjustment_values`'s
//! `value = MIN (value, size - page_size)` is what finally lands, which is why the
//! symptom is a snap to **exactly** `upper - page_size` — the bottom — whatever the
//! reader was doing.
//!
//! Upward only, because scrolling down merges rows into the existing run on a node
//! boundary and never splits. Rate-dependent, because the damage from one write is
//! only *read* by the next one, and `size_allocate` repairs the heights in between:
//! two writes in one frame is the whole precondition. Wheel scroll is the one input
//! that delivers it — GDK compresses pointer motion (so a scrollbar drag stays at one
//! write per frame; measured) but explicitly does NOT compress scroll events.
//!
//! Upstream: GNOME/gtk#2971, fixed by the `GtkListTile` rewrite (MR !5584, commit
//! `d949afb80e`) in **4.10.1** — and never backported to 4.6 or 4.8. Confirmed against
//! the 4.6.9 sources; `probes/listview-scroll-snap.c` reproduces it in ~40 lines of
//! plain GTK with a `GtkStringList` and uniform row heights, and also with the input
//! stack removed entirely (a `g_timeout` writing the adjustment every 8ms).
//!
//! # What this does
//!
//! Takes the wheel in the CAPTURE phase before `GtkScrolledWindow` sees it, accumulates
//! the pixels, and applies them **once** from a frame-clock tick. One write per frame
//! means the tree is always repaired by an allocation before the next read, so the
//! precondition never arises. The step replicates `gtkscrolledwindow.c`'s own
//! (`get_scroll_unit`, :1235) so the scroll feels identical.
//!
//! Inert on GTK >= 4.10.1, where the toolkit is correct and its own path — with the
//! platform tuning this deliberately does not reimplement — is left alone.
//!
//! Installed by [`super::sidebar::SidebarPane::new`] for every sidebar list pane, so a
//! future pane cannot forget it; that construction site is the enforcement mechanism
//! (POLICY "Typed GTK seams": no promotion without one).

use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// GTK's own per-event scroll step, in pixels (`gtkscrolledwindow.c:1235`).
///
/// `pow(page_size, 2/3)` everywhere except a Wayland smooth-scroll event, which GTK
/// gives a flat 25px unit. The macOS branch (unit 1) is deliberately absent: it cannot
/// be reached, because every macOS build is far past the version gate below.
pub(crate) fn wheel_step(page_size: f64, wayland_smooth: bool) -> f64 {
    if wayland_smooth {
        25.0
    } else {
        page_size.powf(2.0 / 3.0)
    }
}

/// Whether the running GTK still has the #2971 defect — i.e. is older than 4.10.1.
///
/// Asked of the RUNTIME, not the build: the crate's `v4_6` feature floor says what we
/// may call, not what is loaded (a distribution upgrade fixes this without a rebuild).
pub(crate) fn coalescing_needed() -> bool {
    defect_present(
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version(),
    )
}

/// The version comparison itself, separated from the runtime so it can be tested at
/// the boundary rather than only at whatever GTK this machine happens to load.
fn defect_present(major: u32, minor: u32, micro: u32) -> bool {
    major < 4 || (major == 4 && (minor < 10 || (minor == 10 && micro < 1)))
}

/// One scroller's accumulated-but-unapplied wheel travel, in pixels.
#[derive(Default)]
struct Pending {
    delta: Cell<f64>,
    /// The frame-clock callback that will apply it, while one is armed.
    tick: RefCell<Option<gtk::TickCallbackId>>,
}

thread_local! {
    /// Every installed scroller's pending travel, so a programmatic scroll can drop it
    /// (see [`cancel_pending`]). Weak, and pruned on every lookup: a strong reference
    /// here would outlive the window and strand its whole subtree (ScrAP-60).
    static PENDING: RefCell<Vec<(glib::WeakRef<gtk::ScrolledWindow>, Rc<Pending>)>> =
        const { RefCell::new(Vec::new()) };
}

fn pending_for(scroller: &gtk::ScrolledWindow) -> Option<Rc<Pending>> {
    PENDING.with(|reg| {
        let mut reg = reg.borrow_mut();
        reg.retain(|(weak, _)| weak.upgrade().is_some());
        reg.iter()
            .find(|(weak, _)| weak.upgrade().is_some_and(|s| &s == scroller))
            .map(|(_, pending)| pending.clone())
    })
}

/// Coalesce wheel scrolling on `scroller` to one adjustment write per frame.
///
/// A no-op on GTK >= 4.10.1. Only vertical, unmodified wheel scrolling is taken:
/// Shift-scroll is GTK's axis swap and is left to GTK, as is everything that is not a
/// scroll event (a scrollbar drag, a keyboard scroll, a touch drag) — none of those
/// deliver two adjustment writes in a frame.
pub(crate) fn install(scroller: &gtk::ScrolledWindow) {
    if !coalescing_needed() {
        return;
    }
    let pending = Rc::new(Pending::default());
    PENDING.with(|reg| {
        let weak = glib::WeakRef::new();
        weak.set(Some(scroller));
        reg.borrow_mut().push((weak, pending.clone()));
    });

    let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    // CAPTURE, and it must also beat GtkScrolledWindow's OWN capture-phase scroll
    // controller (gtkscrolledwindow.c:2155), which stops a smooth-scroll sequence
    // itself — a touchpad would otherwise never reach us. It does: `add_controller`
    // PREPENDS (gtkwidget.c:11461) and `gtk_widget_run_controllers` walks the list
    // head-first (:4523), so the last controller added in a phase runs first. Adding
    // this after the widget is built is therefore load-bearing, not incidental.
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.connect_scroll(glib::clone!(
        #[weak]
        scroller,
        #[strong]
        pending,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |controller, _dx, dy| {
            if controller
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK)
            {
                return glib::Propagation::Proceed; // GTK's axis swap — not ours to take
            }
            let wayland_smooth = is_wayland(&scroller)
                && controller
                    .current_event()
                    .and_then(|event| event.downcast::<gdk::ScrollEvent>().ok())
                    .is_some_and(|event| event.direction() == gdk::ScrollDirection::Smooth);
            let step = wheel_step(scroller.vadjustment().page_size(), wayland_smooth);
            pending.delta.set(pending.delta.get() + dy * step);
            arm(&scroller, &pending);
            glib::Propagation::Stop
        }
    ));
    scroller.add_controller(controller);
}

/// Discard travel accumulated but not yet applied on `scroller`.
///
/// Called before a *programmatic* scroll of the same scroller, for both of the reasons
/// that matter: the reader's pending wheel travel is superseded by wherever the app is
/// about to put them, and — the load-bearing one — applying it afterwards would put a
/// second write in that frame, which is the very condition the module exists to avoid.
pub(crate) fn cancel_pending(scroller: &gtk::ScrolledWindow) {
    if let Some(pending) = pending_for(scroller) {
        pending.delta.set(0.0);
        if let Some(id) = pending.tick.borrow_mut().take() {
            id.remove();
        }
    }
}

/// Arm the frame-clock callback that applies the accumulated travel, unless one is
/// already armed for this scroller.
fn arm(scroller: &gtk::ScrolledWindow, pending: &Rc<Pending>) {
    if pending.tick.borrow().is_some() {
        return;
    }
    let id = scroller.add_tick_callback(glib::clone!(
        #[strong]
        pending,
        move |scroller, _clock| {
            pending.tick.replace(None);
            let delta = pending.delta.replace(0.0);
            if delta != 0.0 {
                let vadj = scroller.vadjustment();
                // The one write. `jump` supersedes rather than races, which is right
                // here: this IS the position the reader just asked for.
                crate::saferizer::scrollpos::jump(&vadj, vadj.value() + delta);
            }
            glib::ControlFlow::Break
        }
    ));
    pending.tick.replace(Some(id));
}

/// Whether the widget is displayed by the Wayland backend, which GTK gives a different
/// smooth-scroll unit. Asked by display type name because the `gdk4-wayland` crate is
/// not a dependency and the constant is not worth one.
fn is_wayland(widget: &impl IsA<gtk::Widget>) -> bool {
    use glib::prelude::ObjectExt;
    widget.as_ref().display().type_().name() == "GdkWaylandDisplay"
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Every sidebar pane must carry the coalescing controller, because the defect is a
    /// property of the widget it hosts, not of what any one pane does with it.
    ///
    /// This asserts the wiring at the ENFORCEMENT point (`SidebarPane::new`) rather than
    /// on a scroller this test built itself: the failure being guarded against is a
    /// future pane, or a refactor of the pane constructor, that quietly stops
    /// installing it — which a test of `install` alone would never see. Mutation-tested:
    /// deleting the `wheelcoalesce::install` call in `SidebarPane::new` fails it.
    #[gtktest::test]
    fn a_sidebar_pane_scroller_carries_the_capture_phase_coalescer() {
        let pane = super::super::sidebar::SidebarPane::new(
            "Outline",
            "win.outline",
            "Hide outline",
            &[],
            180,
        );
        let controllers = pane.scroller.observe_controllers();
        // Selected by FLAGS, not merely by phase: GtkScrolledWindow installs a
        // capture-phase scroll controller of its own (gtkscrolledwindow.c:2155,
        // BOTH_AXES|KINETIC), so a phase-only count finds two and says nothing about
        // ours. VERTICAL alone is this module's signature.
        let capture_scrollers = (0..controllers.n_items())
            .filter_map(|i| {
                controllers
                    .item(i)
                    .and_downcast::<gtk::EventControllerScroll>()
            })
            .filter(|c| {
                c.propagation_phase() == gtk::PropagationPhase::Capture
                    && c.flags() == gtk::EventControllerScrollFlags::VERTICAL
            })
            .count();

        if coalescing_needed() {
            assert_eq!(
                capture_scrollers,
                1,
                "on GTK {}.{}.{} the pane must take the wheel in the capture phase — \
                 without it a fast scroll up snaps the list to its end (GNOME/gtk#2971)",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version(),
            );
        } else {
            assert_eq!(
                capture_scrollers, 0,
                "on a fixed GTK the toolkit's own scroll path must be left alone"
            );
        }
    }

    /// Cancelling is what keeps a programmatic reveal from becoming the second write of
    /// a frame; it must also be safe on a scroller that never accumulated anything, and
    /// on one that was never installed at all (both happen on the reveal path).
    #[gtktest::test]
    fn cancelling_is_safe_on_an_idle_and_on_an_uninstalled_scroller() {
        let installed = gtk::ScrolledWindow::new();
        install(&installed);
        cancel_pending(&installed);
        cancel_pending(&gtk::ScrolledWindow::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The step must be GTK's own, or a coalesced scroll travels a different distance
    /// per click than an uncoalesced one and the fix is felt as a regression.
    /// `gtkscrolledwindow.c:1235`: `scroll_unit = pow (page_size, 2.0 / 3.0)`.
    #[test]
    fn the_step_is_gtks_own_two_thirds_power_of_the_page() {
        // The measured case: page 800 gives GTK's 86px-per-click.
        assert!((wheel_step(800.0, false) - 86.17).abs() < 0.01);
        assert!((wheel_step(583.0, false) - 69.7).abs() < 0.1);
    }

    /// A Wayland smooth-scroll event takes GTK's flat unit instead, so a touchpad on
    /// that backend does not scroll by a page-derived step.
    #[test]
    fn a_wayland_smooth_event_takes_gtks_flat_unit() {
        assert_eq!(wheel_step(800.0, true), 25.0);
    }

    /// The gate is a runtime version comparison including the MICRO component: 4.10.0
    /// still has the defect and 4.10.1 is the release that fixed it, so a
    /// minor-only comparison would silently stop coalescing one release early.
    #[test]
    fn the_gate_turns_off_at_exactly_4_10_1() {
        assert!(
            defect_present(4, 6, 9),
            "4.6.9 — this project's floor — is affected"
        );
        assert!(
            defect_present(4, 8, 3),
            "the fix was never backported to 4.8 either"
        );
        assert!(defect_present(4, 10, 0), "4.10.0 predates the fix");
        assert!(
            !defect_present(4, 10, 1),
            "4.10.1 carries the GtkListTile rewrite"
        );
        assert!(!defect_present(4, 22, 4), "and everything after it");
    }
}
