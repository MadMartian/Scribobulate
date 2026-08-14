//! Application wiring: the `GApplication` startup/activate/open handlers and the
//! app-level GActions (new / open / quit / about). Each handler is a free
//! function rather than an inline closure so the wiring reads top-down and the
//! individual pieces stay independently reviewable.

use super::appactions::{
    add_about_action, add_markdown_help_action, add_new_action, add_open_action,
    add_preview_theme_action, add_quit_action,
};
use super::commands::{EDIT_CMDS, FILE_CMDS, FORMAT_CMDS, VIEW_CMDS, WELCOME};
use crate::preview::{css, theme_css};
use crate::window::{connect_buf_to_copy_action, new_window, rerender_tab_preview_in_place};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};

/// Re-render every open window's preview and editor after a desktop dark↔light
/// theme switch — keeps both the GtkSourceView scheme and the preview colours in
/// step for EVERY tab, not just the active one.
pub(crate) fn re_render_all_windows(app: &Application) {
    // The desktop theme just changed; keep the editor's GtkSourceView scheme in step too
    // (the preview re-render below already re-resolves its theme colours). The editor
    // follows the DESKTOP, not the preview's reading theme (TDD 18.7), so this is the
    // desktop probe — `Palette::is_dark` is the PAGE's lightness and would flip the
    // editor to a light scheme whenever a light reading theme is selected.
    let dark = crate::palette::desktop_is_dark();
    // The desktop probe feeds resolution link 3, so a desktop change moves every
    // colour the active theme leaves unstated — regenerate the theme sheet before
    // the previews below re-render against it.
    reload_theme_css();
    for win in app.windows() {
        let Some(app_win) = win.downcast_ref::<ApplicationWindow>() else {
            continue;
        };
        // On Windows the caption is DWM's, not GTK's, and DWM remembers the last
        // value set for that HWND rather than following GtkSettings — so unlike
        // every re-theme below, nothing re-applies it for us. Per WINDOW, not per
        // tab: there is one caption.
        #[cfg(windows)]
        crate::platform::win32::sync_caption_theme(app_win);
        // Sweep EVERY tab in the window, not just the
        // active one — a background tab must not keep showing the old theme's
        // colours until the user happens to switch to it.
        for st in crate::winstate::tabs_for_window(app_win) {
            crate::window::apply_editor_style_scheme(&st.editor_buf, dark);
            // Re-render each tab's preview IN PLACE (reusing the SplitView's
            // persistent preview scroller) rather than rebuilding + reparenting.
            // This never touches the editor — the reused GtkSourceView is never
            // reparented (ScrAP-58) — and, unlike the
            // old whole-child swap, correctly leaves edit/split tabs in their mode
            // (the swap used to collapse them to preview-only) while keeping their
            // scroll-spy / split-sync handlers valid (same preview widget, no
            // rewire). A no-op for edit-mode tabs (no preview to re-theme).
            let mode = st.view_mode.get();
            if mode.is_preview_visible() {
                let zoom = st.chrome().zoom_level.get();
                let allow_unsafe = st.allow_unsafe_images.get();
                rerender_tab_preview_in_place(&st, mode, zoom, allow_unsafe);
            }
        }
        connect_buf_to_copy_action(app_win);
        // The preview buffer (and its heading offsets) was just rebuilt — refresh
        // the outline so its scroll targets point into the new buffer.
        crate::window::refresh_outline(app_win);
        // The annotations list is rebuilt from the same source (theme re-render).
        crate::window::refresh_annotations(app_win);
        // Each preview's buffer was just rebuilt (rerender_tab_preview_in_place →
        // set_buffer), dropping the find-match highlights — re-apply them for the
        // active tab if the find bar is open, the same derived-state re-sync outline
        // and annotations get above (GTK4Rs/AP-47/GTK4Rs/AP-47; no-op when find is closed).
        crate::window::refresh_preview_find_highlight(app_win);
        // Keep the theme-button label in step with the (possibly just-switched) theme.
        crate::window::refresh_theme_button(app_win);
    }
}

// ── application wiring ────────────────────────────────────────────────────────

pub(crate) fn setup_app(app: &Application) {
    app.connect_startup(on_startup);
    app.connect_activate(on_activate);
    app.connect_open(super::openbatch::on_open);
}

/// GApplication `startup`: register the bundled icon GResource + CSS, wire the
/// desktop-theme-change signals, add the app-level actions, and register the
/// keyboard accelerators. Runs once, before any window is built.
fn on_startup(app: &Application) {
    init_resources_and_css();
    connect_theme_change(app);
    // Supplies the SOURCE for the channel just subscribed to: GTK never reads the
    // platform light/dark setting on macOS or Windows, so without this the app is
    // permanently light there — not merely stale until restart, as on KDE/X11. Must
    // run after `connect_theme_change` (so the initial write reaches the theme sheet)
    // and before the first window is built (so it opens in the right theme instead of
    // flashing light) — the same ordering constraint `add_preview_theme_action`
    // has below.
    #[cfg(target_os = "macos")]
    crate::platform::mac::appearance::track_system_dark_mode();
    // Classifies every secondary window as auxiliary as it is realized, so a dialog
    // raised over a natively-fullscreen window floats above it instead of seizing a
    // Space of its own (GDK tags every toplevel `FullScreenPrimary`). Subscribes to
    // GTK's toplevel list, so it must run before the first window is built — after
    // that, a window already constructed would never be seen.
    #[cfg(target_os = "macos")]
    crate::platform::mac::fullscreen::track_transient_windows();
    #[cfg(windows)]
    crate::platform::win32::track_system_dark_mode();
    add_new_action(app);
    add_open_action(app);
    add_quit_action(app);
    add_about_action(app);
    add_markdown_help_action(app);
    // Registers the reading-theme action AND applies the session's saved theme, so
    // the very first window builds against the right theme rather than flashing the
    // default and re-rendering (TDD 18.12).
    add_preview_theme_action(app);
    register_accelerators(app);

    // ── menu bar ─────────────────────────────────────────────────────────
    // The menubar model is built PER WINDOW (`build_menubar`) rather than set
    // once on the GApplication — because `View ▸ Documents` lists THAT window's
    // tabs, and a GMenuModel's content (labels + item count) can't vary per
    // window when the model is shared app-wide. `win.*` action state stays
    // per-window for free; only content had to be split out. Accelerators
    // (`set_accels_for_action`) remain app-level (see `register_accelerators`)
    // and are unaffected by the model split.
    //
    // WHERE that model is rendered is the platform's business, and there are two
    // answers. Linux and Windows pack a self-built `GtkPopoverMenuBar` into the
    // window (`window::build_chrome`); this file calls no `set_menubar` for them,
    // which is what keeps a global-menu shell from exporting a second copy of the
    // bar already on screen (GTK4Rs/AP-76).
    //
    // macOS is the opposite case, and reading that rule as universal is what left
    // it with TWO menu bars. GTK's Quartz backend installs a native `NSMenu`
    // unconditionally and falls back to a built-in stub when no model is set — so
    // withholding the model does not suppress the native bar, it only makes it
    // show GTK's placeholder next to our in-window one. There the model is
    // exported and no in-window bar is built at all; `platform::mac::menubar`
    // carries the full account. Subscribes to `notify::active-window`, so like
    // the fullscreen tracker above it must run before the first window is built.
    #[cfg(target_os = "macos")]
    crate::platform::mac::menubar::track_active_window(app);
}

/// Register the bundled-icon GResource (compiled by build.rs from
/// data/resources.gresource.xml) before any window is built, so
/// `Button::from_icon_name("copy-document-symbolic")` and friends resolve from
/// the very first toolbar, then install the application CSS. `add_resource_path`
/// makes GTK's icon-theme lookup treat the resource tree exactly like a
/// filesystem icon-theme directory, including automatic light/dark symbolic
/// recoloring.
fn init_resources_and_css() {
    gtk::gio::resources_register_include!("scribobulate.gresource")
        .expect("bundled icon GResource failed to register");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_resource_path("/com/extollit/scribobulate/icons");
    }

    let provider = gtk::CssProvider::new();
    provider.load_from_data(css());
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    // The reading theme's generated sheet goes in as a SECOND provider, at a
    // strictly higher priority than the static one above.
    //
    // The priority bump is load-bearing, not decoration. GTK's cascade is not web
    // CSS: providers are scanned highest-priority-first, the FIRST value found wins
    // permanently, and **specificity only breaks ties WITHIN one provider** — so two
    // providers writing the same property on the same node are arbitrated by
    // priority, and at EQUAL priority by ADD ORDER (`gtkstylecascade.c:314-319`;
    // GTK4Rs/AP-101). The theme sheet deliberately overrides a few of the static sheet's
    // card rules, so relying on its selectors being more specific would have made the
    // winner a construction-order accident. Priority makes it deterministic.
    //
    // Still below PRIORITY_USER (800), so a user's own gtk.css keeps the last word.
    reload_theme_css();
}

/// The app-wide provider carrying the reading theme's generated CSS. Installed on
/// the display once, on first use, then reloaded in place on every theme/desktop
/// change — reloading a provider re-cascades every widget already using it, so no
/// window needs to know it exists.
fn theme_provider() -> gtk::CssProvider {
    thread_local! {
        static PROVIDER: gtk::CssProvider = {
            let p = gtk::CssProvider::new();
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &p,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
                );
            }
            p
        };
    }
    PROVIDER.with(|p| p.clone())
}

/// Regenerate the reading theme's CSS for the ACTIVE theme and reload it into the
/// app-wide provider. Call after any change to the active theme or the desktop
/// theme; every preview re-cascades without being touched individually.
pub(crate) fn reload_theme_css() {
    let theme = crate::theme::active();
    let palette = crate::palette::Palette::for_theme(&theme);
    theme_provider().load_from_data(&theme_css(&theme, &palette));
}

/// Re-render all windows when the desktop switches dark↔light theme.
fn connect_theme_change(app: &Application) {
    if let Some(settings) = gtk::Settings::default() {
        settings.connect_gtk_application_prefer_dark_theme_notify(glib::clone!(
            #[weak(rename_to = a)]
            app,
            move |_| {
                re_render_all_windows(&a);
            }
        ));
        settings.connect_gtk_theme_name_notify(glib::clone!(
            #[weak(rename_to = a)]
            app,
            move |_| {
                re_render_all_windows(&a);
            }
        ));
    }
}

/// Every `(detailed action name, accelerator)` pair the command descriptors declare —
/// the one enumeration of "what is bound to what".
///
/// Extracted so [`register_accelerators`] is not the only thing that can answer it.
/// A second consumer exists because a focused popover with its own GDK surface never
/// reaches the window's accelerator machinery, and has to re-offer the same set locally
/// (`codeview::card`); re-listing the tables there would be a second copy of exactly the
/// thing every one of these tables exists to prevent (POLICY, "Keyboard accelerators are
/// part of the single-source-of-truth contract").
pub(crate) fn accelerator_bindings() -> Vec<(String, String)> {
    accelerator_bindings_for(crate::accel::host())
}

/// [`accelerator_bindings`] for an explicitly named platform.
///
/// The platform is a parameter rather than a `cfg!` read so the whole binding
/// set can be enumerated for *both* platforms from either one — which is what
/// lets `accel::tests::bindings_are_unique_on_every_platform` prove that the
/// macOS re-spelling introduces no collision without a macOS host, and what
/// keeps that check a plain `cargo test` unit test rather than a `#[cfg]`'d
/// (i.e. deleted, POLICY § Unit tests) one.
pub(crate) fn accelerator_bindings_for(platform: crate::accel::Platform) -> Vec<(String, String)> {
    // Every accelerator leaves this function already in the host's spelling, so
    // no consumer downstream has to know a platform difference exists — the
    // single transform point POLICY's accelerator-SSOT rule requires.
    let spell = |accel: &str| crate::accel::map(accel, platform).into_owned();
    let mut out: Vec<(String, String)> = Vec::new();
    for cmd in FILE_CMDS.iter().chain(EDIT_CMDS.iter()) {
        if !cmd.accel.is_empty() {
            out.push((cmd.action.to_string(), spell(cmd.accel)));
        }
    }
    // View-mode accelerators use the detailed action name syntax so GTK
    // activates the correct string-variant target (D2, VIEW_CMDS descriptor).
    for cmd in VIEW_CMDS.iter().filter(|c| !c.accel.is_empty()) {
        out.push((
            format!("win.view-mode::{}", cmd.action_target),
            spell(cmd.accel),
        ));
    }
    // Every command with no Cmd-table row — tab/window navigation, zoom, the
    // outline toggle, Go To Line, and the F1/Ctrl+? shortcuts-help overlay — is
    // bound from the single INLINE_ACCEL_CMDS table (QA M-4), the same table the
    // menu hints, the shortcuts window, and the toolbar tooltips read, so binding
    // and display can never drift. `app.about` intentionally has NO accelerator.
    // These `win.*` actions route to whichever window is focused.
    for cmd in crate::app::INLINE_ACCEL_CMDS {
        for accel in cmd.accels {
            out.push((cmd.action.to_string(), spell(accel)));
        }
    }
    // Format accelerators — detailed win.format::<target> syntax, same as
    // view-mode.  Headings bind to Shift+F1..F6 (chosen to avoid the existing
    // Alt+Shift+2 "Side by Side" accelerator — used by formatters).
    for cmd in FORMAT_CMDS.iter().filter(|c| !c.accel.is_empty()) {
        out.push((format!("win.format::{}", cmd.target), spell(cmd.accel)));
    }
    for (n, accel) in HEADING_ACCELS.iter().enumerate() {
        out.push((format!("win.format::h{}", n + 1), spell(accel)));
    }
    out
}

/// The six heading accelerators, in `h1..h6` order. A `const` rather than a formatted
/// `<Shift>F{n}` because the set is closed: there are exactly six heading levels.
const HEADING_ACCELS: [&str; 6] = [
    "<Shift>F1",
    "<Shift>F2",
    "<Shift>F3",
    "<Shift>F4",
    "<Shift>F5",
    "<Shift>F6",
];

/// Register every keyboard accelerator from the command descriptors so bindings
/// can never drift from the accel strings stored there. `pub(crate)` only so the
/// `gtk-integration-tests` guard in `shortcuts.rs` can assert that what this binds
/// matches what the shortcuts window / tooltips display (QA M-4).
pub(crate) fn register_accelerators(app: &Application) {
    // One `set_accels_for_action` per action, carrying ALL of its accelerators:
    // the call REPLACES an action's accel list rather than adding to it, so an
    // action with two bindings (Ctrl++ and Ctrl+= — see `INLINE_ACCEL_CMDS`) must
    // be registered in a single call or the second silently unbinds the first.
    let mut by_action: Vec<(String, Vec<String>)> = Vec::new();
    for (action, accel) in accelerator_bindings() {
        match by_action.iter_mut().find(|(name, _)| *name == action) {
            Some((_, accels)) => accels.push(accel),
            None => by_action.push((action, vec![accel])),
        }
    }
    for (action, accels) in &by_action {
        let accels: Vec<&str> = accels.iter().map(String::as_str).collect();
        app.set_accels_for_action(action, &accels);
    }
}

/// GApplication `activate`: a bare launch/re-activation with no file arguments.
fn on_activate(app: &Application) {
    // A bare re-activation of an ALREADY-running instance (single-instance
    // hand-off with no file args) — just open a fresh blank document rather
    // than replaying the saved session again (`restore_session` is only for
    // the very first activation of a brand-new process, detected by "no
    // windows exist yet": TDD 8.3 means a running process can never reach
    // zero windows, so this is an unambiguous signal).
    if !app.windows().is_empty() {
        new_window(app, "Scribobulate", WELCOME, None);
        return;
    }
    // Rebuild every window (and its tabs) from the
    // last saved session; fall back to today's single blank window when
    // there is no saved session yet (fresh install, or a corrupt file).
    // new_window/restore_window both self-register their TabState(s) (and
    // unregister on destroy).
    //
    // Restore reads every tab's document off the main thread, so the fallback
    // decision and the crash-recovery pass that follows it move into the same
    // async block — keeping them in exactly the order they have always run in,
    // which is load-bearing: recovery must see the restored tabs (it matches a
    // snapshot to the tab that owns it) and must run before the pre-render pump
    // warms them.
    //
    // The hold guard is not optional. A `GApplication` releases its own reference
    // the moment `activate` returns, and on a bare cold launch there is no window
    // yet at that point — so without it the use count hits zero and the process
    // quits before the session it is restoring ever appears.
    let hold = app.hold();
    let app = app.clone();
    gtk::glib::MainContext::default().spawn_local(async move {
        let _hold = hold;
        if !crate::window::restore_session(&app).await {
            new_window(&app, "Scribobulate", WELCOME, None);
        }
        recover_if_cold_start(&app, true).await;
    });
}

/// Run the crash-recovery pass, if this launch is the process's first activation.
///
/// **Every GApplication entry point that can start a cold process must call this**, and
/// there are two: [`on_activate`] (a bare launch) and [`on_open`] (a launch carrying file
/// arguments). Missing one is not a partial failure, it is a total one *for that route* —
/// and it shipped exactly that way: recovery lived only in `on_activate`, so
/// `scribobulate notes.md`, an Explorer double-click, a `.desktop` association and
/// `xdg-open` — i.e. the ordinary ways a user reopens the document they just lost — all
/// silently skipped the offer. The work was never lost (the snapshot survives untouched
/// and a later bare launch still finds it), but the offer did not appear at the moment the
/// user expected it, which for a recovery feature is most of its value.
///
/// `cold_start` must be evaluated **before** the caller creates any window, since the
/// signal is "no windows existed yet".
///
/// Ordering: after the windows and tabs exist, because a recovered document usually
/// belongs in a tab that already does; and before the deferred pre-render pump, so
/// applying content never races a tab mid-render.
pub(super) async fn recover_if_cold_start(app: &Application, cold_start: bool) {
    if !cold_start {
        return;
    }
    crate::window::recover_after_restore(app).await;
}
