//! Keep a dialog inside its parent's fullscreen Space instead of seizing one of
//! its own — the window classification GDK's Quartz backend does not make, applied
//! at the one instant it is still possible to make it.
//!
//! # What happens, traced
//!
//! With a document window in native fullscreen, opening any modal transient dialog
//! made the *dialog* go fullscreen into a Space of its own, and left the parent
//! unpainted (a black window at the right size and position) on the way back.
//!
//! The trigger is `-[NSWindow addChildWindow:ordered:]`, which GDK performs for a
//! transient toplevel in `_gdk_macos_toplevel_surface_attach_to_parent`
//! (`gdk/macos/gdkmacostoplevelsurface.c:688-700`). MEASURED — the live AppKit
//! stack, captured from an `NSWindowWillEnterFullScreenNotification` observer on
//! GTK 4.22.4 / macOS 26 / arm64:
//!
//! ```text
//! -[NSWindow(NSFullScreen) _willEnterFullScreen]
//! -[_NSEnterFullScreenTransitionController setupWindowForAfterFullScreenEnter]
//! -[_NSEnterFullScreenTransitionController start]
//! -[_NSFullScreenSpace(Transitions) startTransition:]
//! -[NSWindow _performSpecialWindowOrderingWasEffectivelyVisible:options:]
//! -[NSWindow _reallyDoOrderWindowAboveOrBelow:]
//! _NSWindowWalkOrderingGroupInternal
//! -[NSWindow _rebuildOrderingGroup:]
//! -[NSWindow addChildWindow:ordered:]            <- GDK
//! _gdk_macos_toplevel_surface_attach_to_parent
//! _gdk_macos_toplevel_surface_set_property
//! gdk_toplevel_set_transient_for
//! gtk_window_realize                              <- GTK
//! ```
//!
//! So attaching a child to a window that is in a fullscreen Space makes AppKit run
//! a genuine *enter full screen* transition on the **child**. Everything downstream
//! follows from that one transition: GDK's own delegate answers
//! `window:willUseFullScreenContentSize:` with `[[window screen] frame].size`
//! (`gdk/macos/GdkMacosWindow.c:648-651`), so the dialog is resized to the whole
//! screen; `_gdk_macos_surface_update_fullscreen_state`
//! (`gdk/macos/gdkmacossurface.c:672-690`) reads the resulting
//! `NSWindowStyleMaskFullScreen` back and synthesises `GDK_TOPLEVEL_STATE_FULLSCREEN`,
//! so GTK lays the dialog out as a fullscreen window; the dialog's own Space means
//! Escape and the close button never reach it; and returning from that Space leaves
//! the parent surface unpainted.
//!
//! # The correction, and why its TIMING is the whole fix
//!
//! AppKit decides whether that transition happens from the child's
//! `collectionBehavior`. GDK tags **every** toplevel it constructs as a candidate
//! *fullscreen* window —
//!
//! ```text
//! /* Allow NSWindow to go fullscreen */
//! [window setCollectionBehavior:NSWindowCollectionBehaviorFullScreenPrimary];
//! ```
//!
//! (`gdk/macos/gdkmacostoplevelsurface.c:643`, in the surface's `constructed`:
//! unconditional, with no dialog, modality or transient-parent test anywhere near
//! it, and no GDK API to change it afterwards.) AppKit documents the three
//! fullscreen bits as mutually exclusive roles, and a dialog wants the second:
//!
//! ```text
//! NSWindowCollectionBehaviorFullScreenPrimary   The frontmost window with this
//!     collection behavior will be the fullscreen window.
//! NSWindowCollectionBehaviorFullScreenAuxiliary Windows with this collection
//!     behavior can be shown with the fullscreen window.
//! ```
//!
//! (`AppKit.framework/Headers/NSWindow.h:117-118`, macOS 26 SDK.)
//!
//! **`Auxiliary` set at the obvious moment does nothing, and that near-miss is the
//! expensive part of this bug.** A `GtkWidget::realize` handler is the natural
//! place, and it is measurably too late: `realize` is `G_SIGNAL_RUN_FIRST`, so the
//! class closure — `gtk_window_realize`, which is the frame that calls
//! `gdk_toplevel_set_transient_for` and therefore `addChildWindow:` — has already
//! run by the time the handler fires. MEASURED: the write lands (`collectionBehavior
//! 0xc0 -> 0x140`, i.e. `IgnoresCycle|FullScreenPrimary` -> `IgnoresCycle|FullScreenAuxiliary`)
//! and the dialog is fullscreen anyway, because AppKit read the old value moments
//! earlier inside the same function. `FullScreenNone` and `MoveToActiveSpace` at
//! that point are equally inert — which reads as "collection behaviour is not the
//! lever" and sends the search somewhere else entirely.
//!
//! So the attachment has to be taken apart and re-done in the right order, and the
//! two halves land in the two handlers this module installs. [`detach`] takes the
//! transient parent off the window while it is still unrealized and holds it aside;
//! GTK then realizes the window at its own `present`/`show`, sees no transient
//! parent, and performs no `addChildWindow:` at all. The `realize` handler — which
//! runs *after* `gtk_window_realize`, which is the whole reason it is useless for
//! the classification on its own — is exactly the right place for the other half:
//! the `NSWindow` now exists and is still parentless, so [`classify`] can retag it
//! and [`restore`] can then put the parent back. GTK performs precisely the
//! `addChildWindow:` it would have, one moment later, against a window AppKit will
//! not try to make fullscreen.
//!
//! **Do not force the realize forward instead.** Realizing from the
//! `notify::transient-for` handler is the shorter-looking version of this and it
//! reaches the same `NSWindow` at the same point in the ordering — but it realizes
//! *every* transient window at the moment its parent is set, including ones nobody
//! is presenting. `GtkApplicationWindow::set_help_overlay` makes the keyboard-shortcuts
//! window modal and transient for its parent at window-construction time
//! (`gtk/gtkapplicationwindow.c`), so an eager realize creates that window's
//! `NSWindow` at startup — and on Quartz a realized toplevel is on screen: MEASURED, an
//! empty untitled ~100×100 window floating over the desktop from launch, for a window
//! GTK itself considers invisible (`gtk_widget_get_visible() == FALSE`). Deferring to
//! the window's own realize costs nothing and cannot manufacture a window.
//!
//! # Why this is a global hook, and what it does not touch
//!
//! There are three transient-window sites today (About, the modal confirmations,
//! the Insert/Go To Line input form) and nothing stops a fourth. Re-applied by hand
//! at every site this would be a latent regression — the next dialog would work
//! perfectly in a windowed session and be broken only in fullscreen, which no
//! functional test of *that* dialog looks at. So it subscribes once, at startup, to
//! the toplevel list GTK already maintains (`gtk_window_get_toplevels()`, which every
//! `GtkWindow` appends itself to in `gtk_window_constructed`, `gtk/gtkwindow.c:1934`):
//! there is no call site to forget, which is stronger than any convention. It
//! supplies a *classification* the toolkit omits and hands straight back to GTK,
//! owning no application behaviour and changing nothing on another platform;
//! document windows keep `FullScreenPrimary`, so the green button is untouched.
//!
//! # Measured, and how this must be verified
//!
//! Post-fix, GTK 4.22.4 / macOS 26 / arm64, driven through the `.app` bundle with the
//! document window in native fullscreen: the About dialog and the Save/Discard/Cancel
//! confirmation each open at their own size inside the parent's Space
//! (`AXFullScreen == false` on the dialog, `true` on the parent, one Space
//! throughout), `styleMask` carries no `NSWindowStyleMaskFullScreen` (`0x800f` /
//! `0xf`, against `0xc007` before), Escape dismisses both, all three confirmation
//! buttons act, and the parent repaints completely rather than black. The same run
//! confirms [`restore`]: `-[NSWindow parentWindow]` is non-NULL afterwards.
//!
//! **The fault reproduces only from inside the `.app` bundle**, which is what decides
//! how a regression can be caught: the identical binary run bare from a shell shows
//! the dialog windowed and correct even *without* this module (`styleMask=0x8007`),
//! and bisecting `Info.plist` down to three keys did not move that. The bundle is the
//! shipping configuration, so **a bare-binary run cannot verify this fix or catch its
//! regression** — `tests/MANUAL-TEST.md` §7.19m says so at the check, which is where
//! that requirement is enforceable.

use gtk::glib::WeakRef;
use gtk::prelude::*;
use std::cell::RefCell;
use std::ffi::{c_char, c_void, CString};
use std::rc::Rc;

// ── Objective-C runtime + the one public GDK entry point ─────────────────────
//
// Declared by hand rather than by taking `objc2`/`gdk4-macos`: three symbols, all
// public and ABI-stable — the same bargain `appearance.rs` struck for
// CoreFoundation and `process.rs` for `libproc`.

type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "objc")]
unsafe extern "C" {
    fn sel_registerName(name: *const c_char) -> Sel;
    /// Untyped on purpose: `objc_msgSend`'s real signature is per-selector, and on
    /// `aarch64-apple-darwin` it must be *called* through a correctly typed pointer
    /// (the variadic and non-variadic calling conventions differ), never through a
    /// variadic declaration. Each use below transmutes it to the exact signature of
    /// the selector it is sending.
    fn objc_msgSend();
}

unsafe extern "C" {
    /// Public GDK API since 4.8 (`gdk/macos/gdkmacossurface.h`), returning the
    /// surface's `NSWindow*`. Resolved out of the already-linked `libgtk-4`, so
    /// this needs no `#[link]` of its own.
    fn gdk_macos_surface_get_native_window(surface: *mut c_void) -> *mut c_void;
}

// `NSWindowCollectionBehavior`, `AppKit.framework/Headers/NSWindow.h:140-142`.
// AppKit raises if more than one of the three is set, so the swap below clears the
// other two rather than OR-ing a bit in.
/// `NSWindowCollectionBehaviorFullScreenPrimary` — "may become *the* fullscreen
/// window", which is what GDK sets on everything.
const FULLSCREEN_PRIMARY: usize = 1 << 7;
/// `NSWindowCollectionBehaviorFullScreenAuxiliary` — "may be shown *with* the
/// fullscreen window", which is what a dialog wants.
const FULLSCREEN_AUXILIARY: usize = 1 << 8;
/// `NSWindowCollectionBehaviorFullScreenNone` — the third of the mutually
/// exclusive set. Nothing here sets it; it is cleared so the swap is total whatever
/// a future GDK writes.
const FULLSCREEN_NONE: usize = 1 << 9;

/// The transient parent taken off one window until it realizes, shared by that
/// window's two handlers.
///
/// **Weak, and that is load-bearing.** A parent frequently owns the very window
/// whose slot this is — `GtkApplicationWindow` holds a strong reference to its help
/// overlay — so a strong parent here would close `parent -> overlay -> handler ->
/// slot -> parent`, an uncollectable cycle that strands both windows on close
/// (ScrAP-60). The slot is normally emptied at realize; weakness is what keeps the
/// window that is *never* presented from leaking its parent anyway.
type HeldParent = Rc<RefCell<Option<WeakRef<gtk::Window>>>>;

/// Make every secondary window this process opens float over a fullscreen parent
/// rather than take a Space of its own. Call once, from `startup`, before any
/// window is built.
///
/// Named after the gap rather than the mechanism, like its neighbours: it is the
/// window *classification* GDK's Quartz backend does not make.
pub(crate) fn track_transient_windows() {
    let toplevels = gtk::Window::toplevels();

    // Windows that already exist. There are none at `startup`, but the sweep costs
    // nothing and keeps the function total rather than order-dependent — a caller
    // that moved it later would otherwise silently cover only what came after.
    for i in 0..toplevels.n_items() {
        if let Some(window) = toplevels.item(i).and_downcast::<gtk::Window>() {
            arm(&window);
        }
    }

    toplevels.connect_items_changed(|model, position, _removed, added| {
        for i in position..position.saturating_add(added) {
            if let Some(window) = model.item(i).and_downcast::<gtk::Window>() {
                arm(&window);
            }
        }
    });
}

/// Watch one window for the two moments the re-ordering needs.
///
/// A window joins the toplevel list in `gtk_window_constructed`, before any of its
/// properties are set, so nothing can be decided here — only subscribed to.
fn arm(window: &gtk::Window) {
    let held: HeldParent = Rc::default();

    // Half one: take the parent off before GTK can attach it. Fires whether the
    // parent came from the builder or a later `set_transient_for`.
    window.connect_transient_for_notify({
        let held = Rc::clone(&held);
        move |w| detach(w, &held)
    });
    // A window armed by the startup sweep rather than by `items_changed` may already
    // carry a parent, and no further notify is coming for it.
    detach(window, &held);

    // Half two: the `NSWindow` now exists and is still parentless. Classify it, then
    // hand the parent back for GTK to attach.
    //
    // This also covers the parentless-modal case (About raised from a process with
    // no active window), where nothing was ever detached and there is nothing to
    // restore — the classification alone is the whole fix there.
    window.connect_realize(move |w| {
        let parent = held.borrow_mut().take().and_then(|p| p.upgrade());
        if is_secondary(
            parent.is_some() || w.transient_for().is_some(),
            w.is_modal(),
        ) {
            classify(w);
        }
        restore(w, parent);
    });
}

/// Hold `window`'s transient parent aside so GTK realizes an unattached `NSWindow`.
///
/// A no-op once the window is realized: GTK has attached it already and the only
/// moment that mattered has passed. It is also a no-op on the `set_transient_for`
/// this function itself performs and on [`restore`]'s — both re-enter here through
/// `notify::transient-for`, and both are caught by the two guards below rather than
/// by a re-entrancy flag, so there is no flag that can be left set.
fn detach(window: &gtk::Window, held: &HeldParent) {
    if window.is_realized() {
        return;
    }
    let Some(parent) = window.transient_for() else {
        return;
    };
    // Written and dropped before the setter below, which re-enters this function
    // synchronously — a borrow still live across it aborts the process (ScrAP-53).
    held.replace(Some(parent.downgrade()));
    window.set_transient_for(None::<&gtk::Window>);
}

/// Give `window` its transient parent back, now that its `NSWindow` is classified.
///
/// GTK attaches it from here exactly as it would have from `gtk_window_realize`, so
/// the child-window relationship the dialog needs is the same one, made one moment
/// later. That is the load-bearing assumption of this whole module and it is
/// MEASURED rather than reasoned: with both windows realized, `-[NSWindow
/// parentWindow]` is non-NULL immediately after this call returns (4.22.4/Quartz),
/// so `gtk_window_set_transient_for` on an already-realized window does reach
/// `gdk_toplevel_set_transient_for` and is not silently dropped. The
/// `gtk_integration_tests` guard below re-asserts the toolkit-level half of it on
/// every run.
fn restore(window: &gtk::Window, parent: Option<gtk::Window>) {
    if let Some(parent) = parent {
        window.set_transient_for(Some(&parent));
    }
}

/// Whether `window` is a *secondary* window — one that belongs to another window
/// rather than standing on its own.
///
/// Either property alone is sufficient and neither implies the other: a transient
/// parent makes it secondary by construction, and modality makes it secondary by
/// behaviour even in the one case that carries no parent (About raised from a
/// process with no active window). Kept a free function over two booleans so the
/// rule is unit-testable without a display.
fn is_secondary(has_transient_parent: bool, is_modal: bool) -> bool {
    has_transient_parent || is_modal
}

/// The collection behaviour a secondary window should carry, given what it has.
///
/// Split out from the `NSWindow` traffic because it is the part that can be wrong
/// without anything crashing, and the part a test can reach without a display.
fn auxiliary(current: usize) -> usize {
    (current & !(FULLSCREEN_PRIMARY | FULLSCREEN_NONE)) | FULLSCREEN_AUXILIARY
}

/// Replace `FullScreenPrimary` with `FullScreenAuxiliary` on a realized window's
/// `NSWindow`, leaving every other collection-behaviour bit as GDK left it.
fn classify(window: &gtk::Window) {
    let Some(surface) = window.surface() else {
        return;
    };
    // A `GdkSurface` is only a `GdkMacosSurface` on the Quartz backend. This module
    // compiles nowhere else, but the check is cheap and the alternative — handing a
    // foreign instance to `gdk_macos_surface_get_native_window` — is undefined
    // behaviour rather than an error return.
    let Some(macos_surface) = glib::Type::from_name("GdkMacosSurface") else {
        return;
    };
    if !surface.type_().is_a(macos_surface) {
        return;
    }

    // SAFETY: `surface` is a live `GdkMacosSurface` per the type check above, and
    // the pointer is only used for the duration of this call while that binding
    // holds a reference.
    let ns_window = unsafe { gdk_macos_surface_get_native_window(surface.as_ptr().cast()) };
    if ns_window.is_null() {
        return;
    }

    let (Some(getter), Some(setter)) = (
        selector("collectionBehavior"),
        selector("setCollectionBehavior:"),
    ) else {
        return;
    };

    // SAFETY: `objc_msgSend` is being called through pointers carrying the exact
    // signatures of `-[NSWindow collectionBehavior]` (returns `NSUInteger`) and
    // `-[NSWindow setCollectionBehavior:]` (takes `NSUInteger`), which is the
    // required form on aarch64. `ns_window` is a live `NSWindow` GDK owns, and both
    // selectors are `NSWindow`'s own.
    unsafe {
        let get: extern "C" fn(Id, Sel) -> usize = std::mem::transmute(objc_msgSend as *const ());
        let set: extern "C" fn(Id, Sel, usize) = std::mem::transmute(objc_msgSend as *const ());

        let current = get(ns_window, getter);
        let wanted = auxiliary(current);
        if current != wanted {
            set(ns_window, setter, wanted);
            log::debug!(
                "macOS fullscreen: \"{}\" classified auxiliary (collectionBehavior {current:#x} -> {wanted:#x})",
                window.title().unwrap_or_default()
            );
        }
    }
}

/// Look a selector up by name. `None` only if the name is not a valid C string,
/// which for the two literals above cannot happen — expressed as an `Option` rather
/// than an `expect` so a startup path can never panic on it.
fn selector(name: &str) -> Option<Sel> {
    let name = CString::new(name).ok()?;
    // SAFETY: `name` is a live NUL-terminated C string for the duration of the call.
    // The returned selector is a process-lifetime interned pointer.
    let sel = unsafe { sel_registerName(name.as_ptr()) };
    (!sel.is_null()).then_some(sel)
}

/// The one check here that needs a live `GdkMacosSurface` rather than a decision
/// table — it realizes real toplevels, so it carries the integration-test gate its
/// callers do (POLICY § GTK-object integration tests).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    /// **The ordering IS the fix, so this is the guard that has to exist.**
    ///
    /// Both unit tests below can pass with the whole mechanism inert — they judge two
    /// pure functions, and neither can see *when* the classification happens, which is
    /// the only thing that was ever wrong. This asserts the two states the re-ordering
    /// is made of, as states rather than as schedulings: a window must reach
    /// `gtk_window_realize` carrying **no** transient parent (or GTK attaches it with
    /// `addChildWindow:` while it is still `FullScreenPrimary`), and must be carrying
    /// it again by the time realize returns (or the dialog is never a child window at
    /// all). Mutation-checked in both directions — neutering `detach` fails the first
    /// assertion only, `restore` the second only, so neither masks the other
    /// (ScrAP-254).
    ///
    /// It cannot judge the *symptom*: a Space transition needs a natively-fullscreen
    /// parent, and reproduces only from inside the `.app` bundle. That half is
    /// `tests/MANUAL-TEST.md` §7.19m.
    #[gtktest::test]
    fn the_transient_parent_is_withheld_until_the_window_has_realized() {
        use gtk::prelude::*;

        let parent = gtk::Window::new();
        // Disambiguated: `NativeExt` and `WidgetExt` both offer `realize`, and only
        // the widget one creates the surface.
        gtk::prelude::WidgetExt::realize(&parent);

        let dialog = gtk::Window::new();
        // Armed directly rather than through `track_transient_windows`, which
        // subscribes to the process-wide toplevel list — permanent state this
        // shared-process suite would carry into every later test.
        super::arm(&dialog);

        dialog.set_transient_for(Some(&parent));
        assert!(
            dialog.transient_for().is_none(),
            "an unrealized window must be holding NO transient parent, so GTK creates \
             its NSWindow unattached"
        );

        gtk::prelude::WidgetExt::realize(&dialog);
        assert_eq!(
            dialog.transient_for().as_ref(),
            Some(&parent),
            "the parent must be back by the end of realize, so GTK still makes the \
             child-window relationship a dialog depends on"
        );

        dialog.destroy();
        parent.destroy();
    }
}

#[cfg(test)]
mod tests {
    /// Pins the two-property rule, which is the piece here that can be wrong
    /// without anything failing to compile or crash: narrowing it to "has a
    /// transient parent" leaves a parentless modal seizing a Space, and widening it
    /// to "every window" would take fullscreen away from the document windows
    /// themselves.
    #[test]
    fn only_a_secondary_window_is_reclassified() {
        assert!(
            super::is_secondary(true, true),
            "the ordinary dialog: modal AND parented"
        );
        assert!(
            super::is_secondary(true, false),
            "a parented non-modal window is still secondary"
        );
        assert!(
            super::is_secondary(false, true),
            "a modal window with no parent is still secondary"
        );
        assert!(
            !super::is_secondary(false, false),
            "a document window must keep FullScreenPrimary, or the green button stops working"
        );
    }

    /// The three fullscreen bits are one mutually exclusive field in AppKit, so the
    /// swap must CLEAR the other two rather than OR one in — setting more than one
    /// raises.
    #[test]
    fn the_swap_leaves_exactly_one_fullscreen_bit() {
        // What GDK actually leaves on a toplevel (gdkmacostoplevelsurface.c:643).
        assert_eq!(
            super::auxiliary(super::FULLSCREEN_PRIMARY),
            super::FULLSCREEN_AUXILIARY
        );
        // Unrelated bits survive: `NSWindowCollectionBehaviorManaged` (1 << 2).
        let managed = 1 << 2;
        assert_eq!(
            super::auxiliary(super::FULLSCREEN_PRIMARY | managed),
            super::FULLSCREEN_AUXILIARY | managed
        );
        // Idempotent, so a re-realize cannot accumulate anything.
        assert_eq!(
            super::auxiliary(super::FULLSCREEN_AUXILIARY),
            super::FULLSCREEN_AUXILIARY
        );
        for start in [0, super::FULLSCREEN_PRIMARY, super::FULLSCREEN_NONE] {
            let fullscreen_bits = super::auxiliary(start)
                & (super::FULLSCREEN_PRIMARY
                    | super::FULLSCREEN_AUXILIARY
                    | super::FULLSCREEN_NONE);
            assert_eq!(
                fullscreen_bits,
                super::FULLSCREEN_AUXILIARY,
                "exactly one of the three mutually exclusive bits survives"
            );
        }
    }
}
