//! Window lifecycle: the close-request handler that persists the session state and
//! guards unsaved changes with a Save / Discard / Cancel prompt.
use super::*;

/// Wire the window's close-request: persist session geometry / view state and, for a
/// dirty document, prompt before closing.
pub(super) fn wire_close_request(window: &ApplicationWindow) {
    let force_close = Rc::new(Cell::new(false));
    window.connect_close_request(move |win| {
        // A close during a PDF export cancels the export and closes once it has
        // stopped, rather than destroying the window beneath it.
        let reclose = win.downgrade();
        if crate::window::defer_until_export_stops(win, move || {
            if let Some(win) = reclose.upgrade() {
                win.close();
            }
        }) {
            return glib::Propagation::Stop;
        }
        log::info!(
            "window close-request ({} tabs, forced: {})",
            winstate::tabs_for_window(win).len(),
            force_close.get()
        );
        // Persist EVERY currently-open window's geometry/zoom/tabs (TDD 7.2),
        // not just this closing one — otherwise closing
        // one window of several would overwrite the saved session with only
        // that window's state, losing every other still-open window entirely.
        // This handles a STANDALONE window close (WM close / Ctrl+W): the set is
        // this window plus whatever else is still open. A COORDINATED quit closes
        // windows sequentially, which would make each successive close persist a
        // shrinking set (the last one persisting only itself — TDD 15.10); that
        // path is handled by `quit_all_windows`, which snapshots all windows once
        // and FREEZES `session::save` for the duration, so this call no-ops then.
        persist_all_windows_session(win);

        if force_close.get() {
            return glib::Propagation::Proceed;
        }
        // Prompt when ANY of the window's tabs needs guarding — not just the
        // active one (a background at-risk tab must not be silently discarded
        // just because the tab the user happens to be looking at is clean).
        // `needs_close_prompt` covers both unsaved edits and a document whose
        // backing file was deleted on disk (TDD 15.22): an edited untitled doc
        // now confirms (Save As / Discard / Cancel) instead of silently
        // discarding the content, and a clean doc over a deleted file confirms
        // too — closing it without a Save would lose its only copy.
        let needs_prompt = winstate::tabs_for_window(win)
            .iter()
            .any(|t| t.needs_close_prompt());
        if !needs_prompt {
            return glib::Propagation::Proceed;
        }
        confirm_close(win, &force_close);
        glib::Propagation::Stop
    });
}

/// Coordinated app quit (File ▸ Exit / Ctrl+Q — `app.quit_action`): snapshot the
/// FULL multi-window session ONCE while every window is still alive, then close
/// each window so the per-window unsaved-changes prompt still fires. Freezing
/// session writes (`session::set_frozen`) after the upfront snapshot stops the
/// sequential per-window closes from re-persisting a SHRINKING window set — the
/// last window to close would otherwise overwrite the session with only itself,
/// losing every other window on restart (TDD 15.10). A cancelled close (the user
/// aborts quit) thaws again from `confirm_close`'s Cancel arm.
///
/// NOT `app.quit()`: that destroys windows without close-request, bypassing the
/// unsaved-changes prompt (the same reason the quit action already looped `close`).
pub(crate) fn quit_all_windows(app: &gtk::Application) {
    let windows = app.windows();
    if let Some(anchor) = windows
        .iter()
        .find_map(|w| w.clone().downcast::<ApplicationWindow>().ok())
    {
        persist_all_windows_session(&anchor);
    }
    crate::session::set_frozen(true);
    for w in &windows {
        w.close();
    }
}

/// Snapshot every currently-open, registered window (`closing` included — it is
/// still fully alive at this point in `close-request`, before `destroy`) into a
/// [`crate::session::Session`] and persist it (TDD 7.2).
///
/// Every window-scoped value — geometry, zoom, and chrome visibility — is read
/// from the window it belongs to, inside the loop below. `closing` is used ONLY
/// to reach the `GtkApplication` (and hence the full window set); it has no
/// privileged say over any value. It used to supply the chrome for the whole
/// session under a "last window to touch it wins" rule, which silently discarded
/// a toggle made in any OTHER window: hide the toolbar in one window, then close
/// a different window whose toolbar was showing, and the session recorded
/// "showing" for everyone.
fn persist_all_windows_session(closing: &ApplicationWindow) {
    let Some(app) = closing.application() else {
        return;
    };
    let windows: Vec<crate::session::WindowSession> = app
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<ApplicationWindow>().ok())
        .filter_map(|w| {
            let chrome = winstate::chrome(&w)?;
            let tabs_state = winstate::tabs_for_window(&w);
            if tabs_state.is_empty() {
                return None;
            }
            let active_id = state(&w).map(|st| st.id);
            let active_tab = tabs_state
                .iter()
                .position(|t| Some(t.id) == active_id)
                .unwrap_or(0);
            Some(crate::session::WindowSession {
                place: window_place(&w),
                zoom_level: chrome.zoom_level.get(),
                active_tab,
                // THIS window's own chrome, off its own `win.*` toggles — the
                // same reader `window::inherit_from` seeds a new window with, so
                // the value that gets persisted and the value that gets
                // inherited can never drift apart.
                chrome: crate::window::read_window_chrome(&w),
                tabs: tabs_state
                    .iter()
                    .map(|t| crate::session::TabSession {
                        path: t.path.borrow().clone(),
                        // Persisted so a restored tab can be matched to the swap file
                        // holding its unsaved content. Advisory only: the swap file's
                        // own header is authoritative, so a session that loses this
                        // costs a recovered tab its placement, never its content
                        // (`swapfile`'s self-sufficiency principle).
                        doc_id: Some(t.doc_id().as_str().to_string()),
                        view_mode: t.view_mode.get(),
                        show_unsafe_images: t.allow_unsafe_images.get(),
                        find_history: t.find_history.borrow().clone(),
                        replace_history: t.replace_history.borrow().clone(),
                        find_options: t.find_options.get(),
                    })
                    .collect(),
            })
        })
        .collect();

    // App-wide, off the `app.split-*` actions' own state — one value, no "which
    // window's?" question (TDD 7.3).
    let crate::window::arrangement::SplitArrangement { swapped, vertical } =
        crate::window::arrangement::current(&app);
    // App-wide for the same reason and on the same mechanism — off the
    // `app.show-toolbar` / `app.show-tbtn-<id>` states (TDD 9.22).
    let crate::window::toolbarchrome::ToolbarLayout {
        shown: toolbar_shown,
        sections: toolbar_sections,
    } = crate::window::toolbarchrome::current(&app);
    crate::session::save(&crate::session::Session {
        split_swap: swapped,
        split_vertical: vertical,
        show_toolbar: toolbar_shown,
        toolbar_sections,
        // Genuinely app-wide, and read straight off the live active theme rather
        // than off any window's action state: the theme is one app-wide CSS
        // provider, so there is exactly one value and no "which window's?"
        // question to answer (TDD 18.12).
        preview_theme: crate::theme::active().id.clone(),
        // The READER'S raw choice off `app.play-animations`'s own state — never the
        // effective (system "reduce animations"-adjusted) value, which is
        // recomputed live from `gtk-enable-animations` on every launch instead of
        // being persisted (TDD 27.7). `app` here is the same `GtkApplication` every
        // window shares, so — like `preview_theme` above — there is exactly one
        // value and no "which window's?" question to answer.
        play_animations: crate::animation::policy::reader_choice(&app),
        windows,
    });
}

/// `window`'s place on the desktop as it is NOW, for the session (`winplace`): the
/// size it returns to rather than its allocation, its states, and on X11 where it is.
/// A window with no usable size yet (never mapped) records the configured default.
fn window_place(window: &ApplicationWindow) -> winplace::Place {
    let mut place = winplace::capture(window);
    if place.width <= 0 {
        place.width = config().window.width;
    }
    if place.height <= 0 {
        place.height = config().window.height;
    }
    place
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::animation::policy::EnableAnimationsGuard;

    /// TDD 27.7's persistence half, pinned at the actual write site: a close/quit
    /// must save the READER'S raw `app.play-animations` choice, never the effective
    /// (system "reduce animations"-adjusted) value. Forcing the two to genuinely
    /// disagree — reader ON, system setting forcing OFF — is the only condition
    /// that can catch a save site that persists the wrong one (a mutation that
    /// swaps `reader_choice` for `current` at the call site above passes every
    /// other test in this file, since they never diverge).
    #[gtktest::test]
    fn persisted_choice_is_the_readers_raw_choice_not_the_effective_state() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let _settings = EnableAnimationsGuard::set(false); // reduce-animations ON

            let app = gtk::Application::new(
                Some("com.extollit.scribobulate.integrationtest.lifecycle.persistchoice"),
                gtk::gio::ApplicationFlags::NON_UNIQUE,
            );
            app.register(gtk::gio::Cancellable::NONE)
                .expect("register before building a window");
            // The same shape `app::appactions::add_play_animations_action` registers
            // (that function is `pub(super)` to `app` and reads the real session — this
            // mirrors its shape without either dependency, matching `animation::policy`'s
            // own `add_bare_action` test helper).
            let action = gtk::gio::SimpleAction::new_stateful(
                crate::animation::policy::ACTION_NAME,
                None,
                &true.to_variant(), // the reader chose ON
            );
            action.connect_change_state(|act, value| {
                let Some(value) = value else { return };
                act.set_state(value);
            });
            app.add_action(&action);

            let window = crate::window::new_window(&app, "IT", "alpha", None);
            persist_all_windows_session(&window);

            assert!(
                crate::session::load().play_animations,
                "the saved field must be the reader's ON choice, not the effective OFF state \
                 the system setting is currently forcing"
            );

            window.destroy();
        });
    }

    /// TDD 7.3's persistence half, at the write site: a close saves the app-wide
    /// split arrangement once, at the top level, whichever window set it.
    #[gtktest::test]
    fn persisted_split_arrangement_is_the_app_wide_value() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("persistarrangement");
            let setter = crate::window::new_window(&app, "setter", "# A\n", None);
            let closing = crate::window::new_window(&app, "closing", "# B\n", None);
            change_action_state(&setter, "split-swap", &true.to_variant());
            change_action_state(&setter, "split-orientation", &true.to_variant());

            persist_all_windows_session(&closing);

            let saved = crate::session::load();
            assert!(saved.split_swap && saved.split_vertical);
            setter.destroy();
            closing.destroy();
        });
    }

    /// A window restored into a place, for the place tests below: the shared
    /// construction every window takes, with a place as session restore passes one.
    ///
    /// The init's own size is deliberately one no place here uses, so a size the
    /// tests read back can only have come from the place (`winplace::present`).
    fn window_in_place(app: &gtk::Application, place: winplace::Place) -> ApplicationWindow {
        let init = WindowInit {
            width: 300,
            height: 200,
            place: Some(place),
            ..WindowInit::default()
        };
        build_window(app, "IT", "# A\n", None, &init)
    }

    /// What the session saved for its first window.
    fn saved_place() -> winplace::Place {
        crate::session::load().windows[0].place.clone()
    }

    /// TDD 15.10b: a maximized window saves the size it returns to — its default size —
    /// never its maximized allocation; a save that reads `width()`/`height()` fails here.
    /// With no window manager on a test display nothing grows a maximized window, so the
    /// test grows it the way one would; elsewhere the real one does.
    #[gtktest::test]
    fn a_maximized_window_saves_the_size_it_returns_to_not_its_maximized_size() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("placemaximized");
            let window = window_in_place(
                &app,
                winplace::Place {
                    width: 700,
                    height: 500,
                    maximized: true,
                    ..winplace::Place::default()
                },
            );
            crate::testpump::until(
                crate::testpump::Clock::Frame,
                "the window to maximize",
                || window.is_maximized(),
            );
            winplace::testing::resize_behind_gtk(&window, 1200, 900);
            crate::testpump::until(crate::testpump::Clock::Frame, "the maximized size", || {
                window.width() > 700 && window.height() > 500
            });

            persist_all_windows_session(&window);

            let saved = saved_place();
            assert!(saved.maximized, "the maximized state is saved");
            assert_eq!(
                (saved.width, saved.height),
                (700, 500),
                "the saved size is the one the window returns to, not its {}x{} allocation",
                window.width(),
                window.height()
            );
            window.destroy();
        });
    }

    /// TDD 15.10b: a window restored maximized goes back to its saved size when it leaves
    /// maximized through its frame's restore button. On the Windows native frame a window
    /// maximized by `gtk_window_maximize` grew to the work area instead, past the taskbar
    /// (`winplace/src/win32.rs`'s header).
    #[gtktest::test]
    fn a_window_restored_maximized_returns_to_its_saved_size_when_un_maximized() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("placeunmaximize");
            let window = window_in_place(
                &app,
                winplace::Place {
                    width: 700,
                    height: 500,
                    maximized: true,
                    ..winplace::Place::default()
                },
            );
            assert!(
                settled_state(&window, |w| w.is_maximized()),
                "precondition: the window is restored maximized"
            );
            if !winplace::testing::restore_natively(&window) {
                eprintln!(
                    "SKIPPED [TDD 15.10b un-maximize]: needs the native Windows frame \
                     (GTK_CSD=0)"
                );
                window.destroy();
                return;
            }
            let settled = crate::testpump::until_stable(
                crate::testpump::Clock::Frame,
                std::time::Duration::from_secs(3),
                std::time::Duration::from_millis(300),
                || (window.is_maximized(), window.width(), window.height()),
            );
            assert!(settled.converged, "the un-maximized size never settled");
            assert_eq!(
                settled.value,
                Some((false, 700, 500)),
                "leaving maximized returns to the saved size"
            );
            window.destroy();
        });
    }

    /// The settled reading of `state` for a restored `window`: after it is mapped, and
    /// once it has held still. Read any sooner and it reports what the window ASKED
    /// for — GTK sets the flag on an unmapped window directly — rather than what the
    /// platform did; on macOS that answer arrives up to ~1.6 s after map.
    fn settled_state(window: &ApplicationWindow, state: fn(&ApplicationWindow) -> bool) -> bool {
        crate::testpump::until(crate::testpump::Clock::Frame, "the window to map", || {
            window.is_mapped()
        });
        let settled = crate::testpump::until_stable(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_secs(20),
            std::time::Duration::from_millis(2500),
            || state(window),
        );
        assert!(settled.converged, "the window state never settled");
        settled.value.unwrap_or(false)
    }

    /// Destroy `window`, taking it out of full screen first and waiting for that to
    /// settle. On macOS, AppKit runs its own exit-full-screen transition on a window
    /// closed while full screen and calls back into GDK's surface after GDK has
    /// destroyed it. In a running app the surface outlives that callback; in
    /// this suite the next case starts at once, and on CI the callback has landed on a
    /// FREED surface and crashed the suite. Waiting here keeps that callback inside the
    /// case that caused it, while the surface is still alive.
    fn destroy_settled(window: &ApplicationWindow) {
        if window.is_fullscreen() {
            window.unfullscreen();
            let settled = crate::testpump::until_stable(
                crate::testpump::Clock::Frame,
                std::time::Duration::from_secs(20),
                std::time::Duration::from_millis(2500),
                || window.is_fullscreen(),
            );
            assert!(settled.converged, "leaving full screen never settled");
        }
        window.destroy();
    }

    /// One restored window, in `place`, and its settled state. One window per test:
    /// on macOS a full-screen window's Space makes a maximized sibling read as not
    /// maximized, so restoring the two together measures that instead.
    fn restored_state(
        suffix: &str,
        place: winplace::Place,
        state: fn(&ApplicationWindow) -> bool,
    ) -> bool {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed(suffix);
            let window = window_in_place(&app, place);
            let on = settled_state(&window, state);
            destroy_settled(&window);
            on
        })
    }

    fn place_640x480() -> winplace::Place {
        winplace::Place {
            width: 640,
            height: 480,
            ..winplace::Place::default()
        }
    }

    /// TDD 15.10b: a window restored maximized comes back maximized.
    #[gtktest::test]
    fn a_window_restored_maximized_comes_back_maximized() {
        let place = winplace::Place {
            maximized: true,
            ..place_640x480()
        };
        assert!(restored_state("placemax", place, |w| w.is_maximized()));
    }

    /// TDD 15.10b: a window restored full screen comes back full screen.
    #[gtktest::test]
    fn a_window_restored_full_screen_comes_back_full_screen() {
        let place = winplace::Place {
            fullscreen: true,
            ..place_640x480()
        };
        assert!(restored_state("placefull", place, |w| w.is_fullscreen()));
    }

    /// TDD 15.10b: a window restored in neither state comes back in neither, at its
    /// size.
    #[gtktest::test]
    fn a_window_restored_plain_comes_back_plain_at_its_size() {
        assert!(!restored_state("placeplain", place_640x480(), |w| {
            w.is_maximized() || w.is_fullscreen()
        }));
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("placeplainsize");
            let window = window_in_place(&app, place_640x480());
            assert_eq!(window.default_size(), (640, 480));
            window.destroy();
        });
    }

    /// A maximized window and a full-screen sibling, restored together, both settled.
    /// On macOS the sibling's Space is then current, and GDK reads the maximized window
    /// as not maximized while it still is (`winplace/src/macos.rs`).
    fn maximized_beside_full_screen(
        app: &gtk::Application,
    ) -> (ApplicationWindow, ApplicationWindow) {
        let maximized = window_in_place(
            app,
            winplace::Place {
                maximized: true,
                ..place_640x480()
            },
        );
        let fullscreen = window_in_place(
            app,
            winplace::Place {
                fullscreen: true,
                ..place_640x480()
            },
        );
        crate::testpump::until(crate::testpump::Clock::Frame, "both windows to map", || {
            maximized.is_mapped() && fullscreen.is_mapped()
        });
        let settled = crate::testpump::until_stable(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_secs(20),
            std::time::Duration::from_millis(2500),
            || (maximized.is_maximized(), fullscreen.is_fullscreen()),
        );
        assert!(settled.converged, "the two windows' states never settled");
        (maximized, fullscreen)
    }

    /// TDD 15.10c: a window that is maximized at quit is saved maximized, even when
    /// another window's full-screen Space is current then.
    ///
    /// Only macOS misreports the maximized window here, so only there does this
    /// exercise the correction (`winplace/src/macos.rs`), and there it first checks that
    /// GDK did misreport — otherwise it would pass with the correction deleted. On
    /// every other platform GTK reads the window maximized itself and this is a no-op
    /// guard that capture does not lose that.
    #[gtktest::test]
    fn a_maximized_window_is_saved_maximized_while_a_sibling_is_full_screen() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("placemaxbesidefull");
            let (maximized, fullscreen) = maximized_beside_full_screen(&app);
            assert!(
                fullscreen.is_fullscreen(),
                "precondition: the sibling is full screen"
            );
            // GTK's own bit for a full-screen window varies with what ran before it in the
            // process; what is pinned is that capture adds nothing to it.
            assert_eq!(
                winplace::capture(&fullscreen).maximized,
                fullscreen.is_maximized(),
                "capture does not make the full-screen sibling maximized"
            );
            if cfg!(target_os = "macos") {
                assert!(
                    !maximized.is_maximized(),
                    "positive control: GDK reads the zoomed window as not maximized while \
                     the sibling's Space is current; if it no longer does, the correction \
                     this test guards is not being exercised"
                );
            }
            assert!(
                winplace::capture(&maximized).maximized,
                "the maximized window is captured maximized (GTK reads is_maximized = {})",
                maximized.is_maximized()
            );
            maximized.destroy();
            destroy_settled(&fullscreen);
        });
    }

    /// TDD 15.10c: a window maximized and then taken out of maximized is saved plain
    /// when a sibling's full-screen Space is current at quit. The correction above
    /// remembers the frame a window had while zoomed; this is the guard that a window
    /// which has since left that frame does not still read as zoomed. (It is taken out
    /// of maximized before the sibling goes full screen: on macOS a window in another
    /// Space cannot be unzoomed — GDK sends no unzoom for a window it already reads as
    /// unzoomed, and the window is not on screen for a person to unzoom.)
    #[gtktest::test]
    fn a_window_maximized_then_unmaximized_is_saved_plain_beside_a_full_screen_sibling() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("placeunmaxbesidefull");
            let window = window_in_place(
                &app,
                winplace::Place {
                    maximized: true,
                    ..place_640x480()
                },
            );
            assert!(
                settled_state(&window, |w| w.is_maximized()),
                "precondition: the window is restored maximized"
            );
            window.unmaximize();
            if settled_state(&window, |w| w.is_maximized()) {
                eprintln!(
                    "SKIPPED [TDD 15.10c un-maximize]: nothing on this display takes a \
                     window out of maximized"
                );
                window.destroy();
                return;
            }
            let sibling = window_in_place(
                &app,
                winplace::Place {
                    fullscreen: true,
                    ..place_640x480()
                },
            );
            assert!(
                settled_state(&sibling, |w| w.is_fullscreen()),
                "precondition: the sibling is full screen"
            );
            assert!(
                !winplace::capture(&window).maximized,
                "a window no longer maximized is captured plain ({}x{})",
                window.width(),
                window.height()
            );
            window.destroy();
            destroy_settled(&sibling);
        });
    }

    /// TDD 15.10b: a size saved, restored and saved again is the same size — repeated
    /// quit-and-relaunch cycles do not drift it.
    #[gtktest::test]
    fn a_windows_size_does_not_drift_across_save_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("placedrift");
            let mut place = winplace::Place {
                width: 913,
                height: 577,
                ..winplace::Place::default()
            };
            for cycle in 0..3 {
                let window = window_in_place(&app, place.clone());
                crate::testpump::until(crate::testpump::Clock::Frame, "the window to map", || {
                    window.is_mapped()
                });
                persist_all_windows_session(&window);
                let saved = saved_place();
                assert_eq!(
                    (saved.width, saved.height),
                    (913, 577),
                    "cycle {cycle} drifted the size"
                );
                place = saved;
                window.destroy();
            }
        });
    }

    /// TDD 15.10e: only session restore carries a place. Every other new window —
    /// File ▸ New Window, a tab moved out, an open batch — is built from
    /// `inherit_from`, and opens where the window manager puts it.
    #[gtktest::test]
    fn only_session_restore_carries_a_place() {
        let app = crate::window::testkit::test_app_suffixed("placeonlyrestore");
        let source = crate::window::new_window(&app, "IT", "# A\n", None);
        assert!(WindowInit::default().place.is_none());
        assert!(inherit_from(None).place.is_none());
        assert!(inherit_from(Some(&source)).place.is_none());
        source.destroy();
    }
}
