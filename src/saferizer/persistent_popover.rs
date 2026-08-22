//! `PersistentPopover` — own a `GtkPopover`'s parenting lifecycle so the ScrAP-144
//! order bug (unparent-while-open) is unrepresentable through the handle.

use gtk::prelude::*;

/// A `GtkPopover` whose *parenting* is yours to manage (GTK4Rs/AP-80): GTK does not
/// auto-unparent a `set_parent`-attached popover, so its owner must — and teardown
/// of a *possibly-open* popover must run the close path first, i.e. `popdown()`
/// **before** `unparent()` (ScrAP-144). `unparent()` unrealizes the popover's surface;
/// doing that while the popover still holds its (autohide) seat grab strands the
/// grab, leaving the app dead to clicks and keys while hover still works (GTK4Rs/AP-83,
/// real-compositor-only). Reuse the instance — never destroy per use (GTK4Rs/AP-117).
///
/// This handle exists so that order cannot be re-introduced wrongly at a call site:
/// its [`teardown`](Self::teardown) and [`reparent`](Self::reparent) always
/// `popdown()` first, and callers hold a `PersistentPopover` rather than a raw
/// `gtk::Popover` for the lifecycle operations. (It intentionally owns *only* the
/// parenting order; a lag-free "is one open" flag or a pre-warm one-shot, where a
/// specific popover needs them, stay with that popover's own state.)
///
/// Hold it where the parent's `dispose` — or the owning chrome's teardown — can
/// reach it and call [`teardown`](Self::teardown). Rust `Drop` is deliberately
/// **not** used for teardown: it fires at GObject *finalize*, after `dispose`, when
/// the parent may already be gone — too late to unparent safely.
pub(crate) struct PersistentPopover {
    popover: gtk::Popover,
}

impl PersistentPopover {
    /// Adopt an already-built popover (its child content, `autohide`, and signal
    /// wiring remain the caller's; this handle owns only its *parenting*). The
    /// popover may be unparented — parented later via [`reparent`](Self::reparent) —
    /// or already parented.
    pub(crate) fn adopt(popover: gtk::Popover) -> Self {
        Self { popover }
    }

    /// The wrapped popover, for content / pointing-to / signal wiring. Use the
    /// handle's lifecycle methods rather than raw `set_parent`/`unparent` on this —
    /// those carry the ScrAP-144 order.
    pub(crate) fn popover(&self) -> &gtk::Popover {
        &self.popover
    }

    /// Present the popover.
    pub(crate) fn popup(&self) {
        self.popover.popup();
    }

    /// Dismiss the popover (runs the close path; a no-op if already closed).
    pub(crate) fn popdown(&self) {
        self.popover.popdown();
    }

    /// Whether the popover is currently visible.
    ///
    /// This does **not** lag. The long-standing note here said `popdown` was animated so
    /// `is_visible()` trailed the close; measured against 4.6.9 that is false —
    /// `gtk_popover_popdown()` is `gtk_widget_hide()` plus a cascade that early-returns
    /// for a non-autohide popover, there is no transition, and the flag flips
    /// synchronously (ScrAP-192).
    ///
    /// A caller may still want its own flag, for a reason that survives the correction:
    /// *visible* and *open* are different questions once a popover hides itself while its
    /// anchor is off screen. Track state when you mean state; this answers pixels.
    pub(crate) fn is_visible(&self) -> bool {
        self.popover.is_visible()
    }

    /// The popover's current parent, if any (for a caller's "already on this editor?"
    /// idempotence check that [`reparent`](Self::reparent) does not cover).
    pub(crate) fn parent(&self) -> Option<gtk::Widget> {
        self.popover.parent()
    }

    /// Tear the popover down for its owner's `dispose`/teardown: **`popdown()` then
    /// `unparent()`** (ScrAP-144 order), the unparent guarded on an actual parent
    /// (GTK4Rs/AP-80 — a `set_parent`'d popover left parented at teardown floods
    /// "not a child of" warnings). Idempotent: `popdown` no-ops when closed, the
    /// unparent is skipped when already unparented.
    pub(crate) fn teardown(&self) {
        self.popover.popdown();
        if self.popover.parent().is_some() {
            self.popover.unparent();
        }
    }

    /// Re-target the popover onto `new_parent`: **popdown → unparent → set_parent**,
    /// so a mapped popover is never structurally moved (ScrAP-144). Idempotent — a
    /// no-op when already parented to `new_parent`, so it never needlessly pops down
    /// a popover already on the right parent.
    pub(crate) fn reparent(&self, new_parent: &impl IsA<gtk::Widget>) {
        let new_w = new_parent.upcast_ref::<gtk::Widget>();
        if self.popover.parent().as_ref() == Some(new_w) {
            return;
        }
        self.popover.popdown();
        if self.popover.parent().is_some() {
            self.popover.unparent();
        }
        self.popover.set_parent(new_parent);
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Pump the main loop until `cond` holds or `budget` turns' worth of wall-clock
    /// A popover open/close is frame-clock/animation driven, so this cannot be a single
    /// event drain — `crate::testpump::until` under `Clock::Frame`, which blocks on a
    /// real timeout SOURCE (GTK4Rs/AP-79).
    ///
    /// **No turn budget.** This took one and multiplied it by 4 to "keep the same
    /// worst-case ceiling", which assumes a turn costs 4 ms. GTK4Rs/AP-261 says turns
    /// are not time, and the macOS seat MEASURED the spread: the same 200 turns cost
    /// 74 ms with one live toplevel and 610 ms with three more alive, so the true figure
    /// moves ~8x with load and no constant is the right one. Every timeout here is a
    /// bug, so the wait is predicate-driven with a generous failure bound instead.
    fn settle(what: &str, cond: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Frame, what, cond);
    }

    /// A presented window hosting a child that a popover can be parented to. The
    /// window is returned so the caller keeps it alive (and can `destroy` it).
    fn host() -> (gtk::Window, gtk::Box) {
        let win = gtk::Window::new();
        let anchor = gtk::Box::new(gtk::Orientation::Vertical, 0);
        win.set_child(Some(&anchor));
        win.set_default_size(200, 200);
        win.present();
        (win, anchor)
    }

    /// `teardown()` must run the close path (`popdown`) BEFORE `unparent()` and end
    /// unparented. Proven headlessly the same way the codeview dispose test proves
    /// it: `closed` fires only on a genuine popdown, never on `unparent()` — so a
    /// fired `closed` discriminates popdown-first from unparent-while-open, and a
    /// `None` parent proves the unparent still happened. Mutation-guards both halves
    /// (drop the popdown → `closed` never fires; drop the unparent → parent stays
    /// `Some`), per GTK4Rs/AP-78.
    #[gtktest::test]
    fn teardown_popdowns_before_unparenting_and_ends_unparented() {
        let (win, anchor) = host();
        let pop = gtk::Popover::new();
        pop.set_autohide(false);
        pop.set_parent(&anchor);
        let handle = PersistentPopover::adopt(pop.clone());

        handle.popup();
        settle("the popover to open (precondition)", || pop.is_visible());

        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        pop.connect_closed({
            let c = closed.clone();
            move |_| c.set(true)
        });

        handle.teardown();
        settle("teardown to fire the popover's closed signal", || {
            closed.get()
        });

        assert!(
            closed.get(),
            "teardown must popdown (close path) before unparenting — `closed` fires \
             on a real popdown, never on unparent-while-open (ScrAP-144)"
        );
        assert!(
            handle.parent().is_none(),
            "teardown must also unparent the popover (GTK4Rs/AP-80)"
        );
        win.destroy();
    }

    /// `reparent()` moves the popover to the new parent, and is a no-op when already
    /// on that parent — proven by opening the popover and re-parenting it to the
    /// SAME parent: the early-return means it stays open (a missing early-return
    /// would popdown → close it). Mutation-guards the idempotence branch.
    #[gtktest::test]
    fn reparent_moves_and_is_idempotent() {
        let (win, root) = host();
        let a = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&a);
        root.append(&b);

        let pop = gtk::Popover::new();
        pop.set_autohide(false);
        let handle = PersistentPopover::adopt(pop.clone());

        handle.reparent(&a);
        assert_eq!(
            handle.parent().as_ref(),
            Some(a.upcast_ref::<gtk::Widget>()),
            "reparent parents an unparented popover onto the target"
        );

        handle.reparent(&b);
        assert_eq!(
            handle.parent().as_ref(),
            Some(b.upcast_ref::<gtk::Widget>()),
            "reparent moves the popover to a new parent"
        );

        handle.popup();
        settle("the popover to open on parent b (precondition)", || {
            pop.is_visible()
        });
        handle.reparent(&b); // already on b → must be a no-op, must NOT popdown
        assert!(
            pop.is_visible(),
            "reparent onto the current parent is a no-op — it must not popdown an \
             already-correctly-parented open popover"
        );

        win.destroy();
    }
}
