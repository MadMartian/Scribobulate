//! The annotation card's deferred-focus mechanism, in isolation.
//!
//! `src/codeview/markers.rs` opens a non-autohide `GtkPopover` (the annotation card) and,
//! one tick later via `glib::idle_add_local_once`, calls
//! `popover.child_focus(DirectionType::TabForward)` so Tab/Space/Escape reach the card's
//! Edit button from the moment it presents. The code comment there records why the defer
//! is load-bearing: a SAME-TICK `child_focus` right after `popup()` runs before the
//! popover's surface is mapped and silently no-ops — "real-keyboard confirmed on Quartz,
//! mac" — so this project already carries a claim that the fix was verified with a real
//! keyboard on this exact backend.
//!
//! What no automated check pinned down: whether that claim still holds, and whether it can
//! be pinned down WITHOUT a real keyboard at all. `#[gtk::test]` cannot run on macOS
//! (gtk4-rs dispatches test bodies onto a worker thread; GTK here requires main-thread
//! init), and a live GtkPopover surface can never become the AppKit key window regardless
//! of autohide (`GdkMacosWindow.c` `canBecomeKeyWindow` returns `GDK_IS_TOPLEVEL(...)`,
//! researcher-confirmed at GTK 4.22.4) — so no synthetic-keyboard test could observe this
//! mechanism on this platform even if `#[gtk::test]` worked here. The two layers are
//! independent: AppKit key-window status decides which `NSWindow` receives `keyDown:`,
//! GTK's OWN focus chain (`gtk_root_get_focus`) decides which widget a routed event is
//! propagated from — so the question "did GTK's internal focus land on Edit" needs no
//! window-manager cooperation and no keyboard input to answer. It needs exactly what this
//! target does: open the popover with the real mouse-driven mechanism already proven to
//! work, then read GTK's own focus state in-process.
//!
//! `harness = false`, per `tests/icon_resolution.rs`/`tests/macos_dark_mode.rs`: this owns
//! its own `main()` on the process main thread, so it runs everywhere `cargo test
//! --features gtk-integration-tests` runs a display, not just where a real keyboard does.
//! Self-contained rather than reaching into `src/codeview/` by path: the mechanism under
//! test is generic GtkPopover/focus-chain behaviour (the fix is a defer-to-idle pattern,
//! not anything specific to the annotation card's own wiring), and the card's actual
//! widget tree already has coverage from `find_close_button`'s unit test
//! (`the_corner_close_button_dismisses_without_stealing_focus`, gated behind
//! `#[gtk::test]` and so Linux-only) that the close button is excluded from the tab ring.
//! Reaching `AnnotationCard` itself would need `crate::codeview::CodePreviewView` and
//! `crate::preview::render`, well past what `#[path]` can pull in as a leaf. That
//! constraint has since been lifted for *new* work: a body written as
//! `#[gtktest::test]` inside `src/` reaches those types directly and runs on the main
//! thread via `src/gtk_suite.rs`, so a future card-specific assertion should be
//! written there rather than here. This target stays standalone because it measures a
//! focus curve from a known-clean window state, which a shared process cannot promise.
//!
//! Run: `cargo test --features gtk-integration-tests --test popover_deferred_focus`

use gtk::prelude::*;
use std::time::Duration;

fn main() {
    gtk::init().expect("GTK init on the process main thread");

    let window = gtk::Window::new();
    window.set_default_size(300, 200);
    let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&parent));
    window.present();
    pump_for(Duration::from_millis(200));

    // The card's exact tab-order shape (`markers.rs::card_close_button` /
    // `open_marker_popover`): a non-focusable close button FIRST, then the first REAL
    // focusable control. Order matters — a focusable widget placed before Edit would
    // capture `TabForward` instead, which is precisely the defect the real close button's
    // `set_can_focus(false)` prevents (and which this target would catch if it regressed:
    // move `close` after `edit` below and the final assertion fails).
    let popover = gtk::Popover::new();
    popover.set_autohide(false);
    popover.set_parent(&parent);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.set_can_focus(false);
    vbox.append(&close);
    let edit = gtk::Button::with_label("Edit");
    vbox.append(&edit);
    let remove = gtk::Button::with_label("Remove");
    vbox.append(&remove);
    popover.set_child(Some(&vbox));

    popover.popup();

    // The exact call `markers.rs` makes: deferred one tick via `idle_add_local_once`, not
    // same-tick. Reproducing the defer (not just the `child_focus` call) is the point —
    // the code comment's claim is specifically that the ORDERING matters on Quartz.
    let popover_for_idle = popover.clone();
    glib::idle_add_local_once(move || {
        let _ = popover_for_idle.child_focus(gtk::DirectionType::TabForward);
    });

    // Sample across a spread of delays rather than asserting once at the end: a timing
    // regression (focus arriving late, or on a platform where the idle priority differs)
    // would pass a single late assertion and still describe a broken first-open. If this
    // target ever fails, the printed curve is itself the bug report.
    let mut samples: Vec<(u64, bool, bool)> = Vec::new(); // (ms elapsed, edit.has_focus, close.has_focus)
    let checkpoints = [0u64, 16, 50, 250];
    let mut elapsed = 0u64;
    for &target_ms in &checkpoints {
        if target_ms > elapsed {
            pump_for(Duration::from_millis(target_ms - elapsed));
            elapsed = target_ms;
        }
        samples.push((elapsed, edit.has_focus(), close.has_focus()));
    }

    println!("deferred child_focus(TabForward) after popup():");
    for (ms, edit_focused, close_focused) in &samples {
        println!(
            "  +{ms:>4}ms  edit.has_focus()={edit_focused:<5}  close.has_focus()={close_focused}"
        );
    }

    // Cross-check against GTK's own root-level focus record, not just the button's own
    // flag — this is `gtk_root_get_focus`, the exact accessor `handle_key_event` consults
    // to decide where a routed key event starts propagating (researcher, this thread).
    // `GtkPopover` is a `GtkNative` but NOT a `GtkRoot` (see the `AnnotationCard` wrapper's
    // `@implements` list) — its root is the WINDOW it is parented into — so this reads the
    // same root a real key event would.
    let root_focus_is_edit = edit
        .root()
        .and_then(|r| r.focus())
        .is_some_and(|w| w == edit.clone().upcast::<gtk::Widget>());
    println!("  gtk_root_get_focus(root) == edit  ->  {root_focus_is_edit}");

    let mut failures: Vec<String> = Vec::new();

    // THE DEFER ITSELF, and this assertion is the reason the target is named for it
    // (QA round 5, M-4). MUTATION-TESTED before this line existed: deleting the
    // `idle_add_local_once` and calling `child_focus` straight after `popup()` left every
    // assertion below GREEN — 184 lines of test that could not fail for the reason they
    // were written. Of course they could not: the *necessity* of the defer is a Quartz
    // property (a same-tick `child_focus` runs before the popover surface is mapped and
    // silently no-ops there), and on this backend the same-tick call works fine, so no
    // assertion about the OUTCOME can distinguish the two orderings here.
    //
    // What is observable on every backend is the defer's PRESENCE. The `+0ms` sample is
    // taken with no pumping at all, so no idle source has run yet: with the defer, focus
    // cannot have arrived; without it, `child_focus` already ran synchronously and Edit
    // is focused. MEASURED on this backend — `+0ms edit.has_focus()=false`, `+16ms true`.
    //
    // So this target pins the ordering rather than its consequence, and says so. The
    // consequence is only checkable with a real keyboard on Quartz, which is recorded in
    // the module header as a hand-verified claim; pretending an automated check covers it
    // is what the previous version did.
    if samples
        .first()
        .is_some_and(|&(_, edit_focused, _)| edit_focused)
    {
        failures.push(
            "Edit was already focused at +0ms, before any idle source ran — child_focus is \
             being called SYNCHRONOUSLY after popup(), not deferred. On Quartz that call \
             lands before the popover's surface is mapped and silently no-ops, leaving \
             Tab/Space/Escape dead in the card from the moment it opens. The defer is the \
             mechanism this target exists to pin."
                .to_string(),
        );
    }

    if !samples
        .last()
        .is_some_and(|&(_, edit_focused, _)| edit_focused)
    {
        failures.push(format!(
            "edit.has_focus() never became true within {}ms of popup() — the deferred \
             child_focus(TabForward) either did not fire or did not land on Edit. Tab/Space \
             would be dead in the card from the moment it opens, and (per the mechanism this \
             target isolates) so would Escape, which only sees the key event while a popover \
             descendant is actually focused.",
            checkpoints.last().unwrap()
        ));
    }
    if !root_focus_is_edit {
        failures.push(
            "gtk_root_get_focus(root) does not report the Edit button even though \
             edit.has_focus() may read true — a routed key event would not start propagation \
             where the widget-level flag suggests it would."
                .to_string(),
        );
    }
    if samples.iter().any(|&(_, _, close_focused)| close_focused) {
        failures.push(
            "the close button took focus at some sample point — it must stay out of the tab \
             ring (`set_can_focus(false)`) or a Space press one tick after open dismisses the \
             card instead of reaching Edit."
                .to_string(),
        );
    }

    window.destroy();

    if !failures.is_empty() {
        eprintln!("\nFAILED:");
        for f in &failures {
            eprintln!("  - {f}");
        }
        std::process::exit(1);
    }
    println!(
        "\nthe deferred child_focus(TabForward) mechanism lands on Edit, never on the close \
         button, on this backend."
    );
}

/// Pump the default main context for approximately `dur`, in small non-blocking slices —
/// never `iteration(true)`, which can block indefinitely if nothing schedules more work
/// (GTK4Rs/AP-79). A coarse wall-clock budget is the right shape here (not a frame-count bound):
/// the thing being measured is elapsed real time since `popup()`, matching how a human
/// perceives "focus arrived late".
fn pump_for(dur: Duration) {
    let ctx = glib::MainContext::default();
    let deadline = std::time::Instant::now() + dur;
    loop {
        while ctx.iteration(false) {}
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
