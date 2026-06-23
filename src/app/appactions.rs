//! App-level `GAction` builders: File ▸ New Document, File ▸ Open, Exit, and the
//! Help ▸ About dialog. Each is a `SimpleAction` added to the `GApplication` in
//! `setup::on_startup`. Kept separate from the lifecycle wiring so the (long)
//! About-dialog construction doesn't crowd out the startup/activate/open flow.

use super::commands::WELCOME;
use super::open::{dialog_dir_for, remember_dialog_dir};
use crate::window::new_window;
use crate::winstate::state;
use gtk::gio::SimpleAction;
use gtk::prelude::*;
use gtk::{
    AboutDialog, Application, ApplicationWindow, FileChooserAction, FileChooserNative, FileFilter,
    ResponseType,
};

/// File ▸ New Document (operator decision): adds a tab
/// to the CURRENT window (this is the single `app.new` call site — the
/// label/accel and this handler are the only two places that needed to change to
/// repurpose app.new). Falls back to opening a brand-new window if there is
/// somehow no active window (not normally reachable — closing the last window
/// quits the app).
pub(super) fn add_new_action(app: &Application) {
    let new_action = SimpleAction::new("new", None);
    new_action.connect_activate(glib::clone!(
        #[weak(rename_to = a)]
        app,
        move |_, _| {
            match a
                .active_window()
                .and_then(|w| w.downcast::<ApplicationWindow>().ok())
            {
                Some(win) => crate::window::add_new_document_tab(&win),
                None => {
                    new_window(&a, "Scribobulate", WELCOME, None);
                }
            }
        }
    ));
    app.add_action(&new_action);
}

pub(super) fn add_open_action(app: &Application) {
    let open_action = SimpleAction::new("open", None);
    open_action.connect_activate({
        let app_weak = app.downgrade();
        move |_, _| {
            let Some(a) = app_weak.upgrade() else { return };
            let parent = a.active_window();
            let dialog = FileChooserNative::new(
                Some("Open Markdown file"),
                parent.as_ref(),
                FileChooserAction::Open,
                Some("Open"),
                Some("Cancel"),
            );
            let filter = FileFilter::new();
            filter.add_pattern("*.md");
            filter.add_pattern("*.markdown");
            filter.set_name(Some("Markdown files"));
            dialog.add_filter(&filter);
            // Start in the active doc's directory, or the last-visited dialog dir.
            if let Some(dir) = dialog_dir_for(parent.as_ref().and_then(|w| w.downcast_ref())) {
                let _ = dialog.set_current_folder(Some(&gtk::gio::File::for_path(&dir)));
            }
            let app_weak2 = app_weak.clone();
            crate::saferizer::native_dialog::NativeDialogHolder::show(&dialog, move |d, resp| {
                if resp == ResponseType::Accept {
                    if let Some(file) = d.file() {
                        if let Some(path) = file.path() {
                            remember_dialog_dir(&path);
                        }
                        if let Some(a) = app_weak2.upgrade() {
                            // The "interactive" hint (GIO's free-form open()
                            // hint string) tells connect_open this came from
                            // File ▸ Open, not a CLI/D-Bus batch launch — see
                            // its own comment for why that changes reuse
                            // behavior (TDD 1.2).
                            a.open(&[file], "interactive");
                        }
                    }
                }
                d.destroy();
            });
        }
    });
    app.add_action(&open_action);
}

pub(super) fn add_quit_action(app: &Application) {
    let quit_action = SimpleAction::new("quit", None);
    quit_action.connect_activate(glib::clone!(
        #[weak(rename_to = a)]
        app,
        move |_, _| {
            // Snapshot ALL windows once (while every one is still alive) and
            // freeze session writes, THEN close each window via its
            // close-request so the unsaved-changes confirmation still fires
            // (`a.quit()` would bypass the prompt and silently discard edits).
            // Without the upfront snapshot+freeze the sequential closes would
            // each re-persist a shrinking window set, so only the last window
            // would survive a restart (TDD 15.10). The app exits once the last
            // window closes; a cancelled prompt leaves that window (and the
            // app) open and thaws persistence again.
            crate::window::quit_all_windows(&a);
        }
    ));
    app.add_action(&quit_action);
}

pub(super) fn add_about_action(app: &Application) {
    let about_action = SimpleAction::new("about", None);
    about_action.connect_activate(glib::clone!(
        #[weak(rename_to = a)]
        app,
        move |_, _| {
            // Resolve the focused window's document path for the System tab.
            let active_win = a
                .active_window()
                .and_then(|w| w.downcast::<ApplicationWindow>().ok());
            let doc_path = active_win
                .as_ref()
                .and_then(state)
                .and_then(|st| st.path.borrow().clone())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(welcome screen)".to_string());

            let gtk_ver = format!(
                "{}.{}.{}",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version(),
            );
            let system_info = format!(
                "Document:         {doc_path}\n\n\
                 GTK:              {gtk_ver}\n\
                 gtk4 bindings:    {}\n\
                 sourceview5:      {}\n\
                 pulldown-cmark:   {}",
                env!("SCRIB_GTK4_CRATE_VERSION"),
                env!("SCRIB_SOURCEVIEW_CRATE_VERSION"),
                env!("SCRIB_PULLDOWN_CRATE_VERSION"),
            );

            // .website() renders a GtkLinkButton on the About tab that opens the
            // default web browser.  Do NOT use "Name <url>" in .authors() for a
            // website URL — GTK treats the <...> portion as a mailto: address
            // regardless of whether it starts with https:// (ScrAP-40).
            // license_type stays on About — it is the only way to surface the
            // auto-rendered full Apache 2 text via the License button.
            let dialog = AboutDialog::builder()
                .program_name("Scribobulate")
                .version(env!("CARGO_PKG_VERSION"))
                .comments(
                    "A native, Linux-first Markdown viewer/editor — also \
                     compatible with macOS — purpose-built for managing \
                     Markdown generated by clankers.\n\n\
                     Renders entirely on the CPU — no GPU, no WebEngine, no \
                     web stack. In an era where VRAM is precious real estate \
                     for running local models, Scribobulate stays out of the way.",
                )
                .license_type(gtk::License::Apache20)
                .website("https://www.extollit.com")
                .website_label("extollIT Enterprises")
                .authors(vec!["© 2026 extollIT Enterprises".to_string()])
                .logo_icon_name(crate::icons::Icon::App.name())
                .system_information(system_info)
                .modal(true)
                .build();

            // Attribution for bundled third-party components. The syntax-highlighting
            // grammars are compiled into the binary via the `two-face` crate (bat's
            // syntect assets); their upstream licenses (MIT/Apache-2.0/BSD-2/3-Clause)
            // are all permissive but REQUIRE the copyright + licence notice to travel
            // with a binary distribution. The full verbatim notices ship as
            // THIRD-PARTY-LICENSES.md (generated from `two_face::acknowledgement`);
            // this credit section is the in-app surface of that obligation. Plain text
            // only — a `<...>` here would be mis-parsed as a mailto: link (ScrAP-40).
            dialog.add_credit_section(
                "Bundled open-source components",
                &[
                    "Syntax-highlighting grammars — via two-face (bat / syntect assets)",
                    "Licensed MIT, Apache-2.0, BSD-2-Clause, and BSD-3-Clause",
                    "Full notices: THIRD-PARTY-LICENSES.md (in the distribution)",
                ],
            );

            // Enable horizontal scroll on the System (and License) text panes.
            // GtkAboutDialog wraps system_information in a GtkTextView inside a
            // GtkScrolledWindow.  By default the TextView uses WrapMode::Word,
            // which suppresses horizontal overflow — long paths wrap rather than
            // scroll.  Walk the dialog's widget tree (built at construction time,
            // before present()), disable word-wrap on every TextView, and set
            // the nearest ScrolledWindow ancestor to PolicyType::Automatic so a
            // scrollbar appears whenever a line overflows the viewport.
            configure_about_text_areas(dialog.upcast_ref());

            dialog.set_transient_for(active_win.as_ref());
            // GtkWindow's default close-request handling only hides the window;
            // without an explicit destroy() each About invocation leaks a new
            // AboutDialog (and its GTK native peer) that is never freed.
            dialog.connect_close_request(|d| {
                d.destroy();
                gtk::glib::Propagation::Proceed
            });
            dialog.present();
        }
    ));
    app.add_action(&about_action);
}

/// The CommonMark reference the online help opens. The app renders CommonMark
/// (pulldown-cmark), so this is the syntax reference that actually matches what
/// it displays — not a generic cheat-sheet that might show constructs we don't
/// support.
const MARKDOWN_HELP_URL: &str = "https://commonmark.org/help/";

/// Help ▸ Markdown Reference — opens [`MARKDOWN_HELP_URL`] in the user's default
/// browser through [`crate::links::open_url`], the single sanctioned external-URI
/// launch path (`show_uri_full`, parent `None`, timestamp 0). Routing every launch
/// through it (rather than a bare `gtk::show_uri`) keeps launch failures in the log
/// instead of a per-call "Could not show link" modal, and avoids the X11 handle-unexport
/// warning a parent triggers on 4.6 (ScrAP-129; see `open_url`'s own rationale).
pub(super) fn add_markdown_help_action(app: &Application) {
    let help_action = SimpleAction::new("markdown-help", None);
    help_action.connect_activate(move |_, _| {
        crate::links::open_url(MARKDOWN_HELP_URL);
    });
    app.add_action(&help_action);
}

/// Walk a GtkAboutDialog's widget tree, disabling word-wrap on every GtkTextView
/// and switching the nearest GtkScrolledWindow ancestor to `Automatic` so long
/// lines (e.g. document paths on the System tab) scroll horizontally rather than
/// wrap. See `add_about_action` for the rationale.
fn configure_about_text_areas(w: &gtk::Widget) {
    if let Some(tv) = w.downcast_ref::<gtk::TextView>() {
        tv.set_wrap_mode(gtk::WrapMode::None);
        let mut p = w.parent();
        while let Some(node) = p {
            if let Some(sw) = node.downcast_ref::<gtk::ScrolledWindow>() {
                sw.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
                break;
            }
            p = node.parent();
        }
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        configure_about_text_areas(&c);
        child = c.next_sibling();
    }
}

/// `app.preview-theme` (+ its toolbar shim `app.pick-preview-theme`) — the reading-theme
/// command. Every switch funnels through `preview-theme`'s change_state, so the surfaces
/// cannot diverge — satisfying the single-command rule at the level that matters (one
/// logic path) even though PRESENTATION needs two GActions (see below).
///
/// **`app.`, not `win.`** — deliberately. The theme is APP-WIDE (one CSS provider,
/// unscoped rules — sdd/THEMING.md), and the action map must match that
/// scope: a `win.` action would exist once per window, so selecting Sepia in one
/// window would not reach the others — reintroducing exactly the per-surface
/// divergence the single-command rule exists to prevent.
///
/// **Two GActions, one command.** `preview-theme` is STATEFUL (state = active theme id)
/// so the View ▸ Reading Theme menu renders RADIOs with a tick, consistent with the
/// other menubar radio menus. `pick-preview-theme` is a STATELESS shim the TOOLBAR
/// combobox targets so its items render PLAIN, consistent with the Heading picker; it
/// carries no logic, only forwarding its target to the stateful action's state.
pub(super) fn add_preview_theme_action(app: &Application) {
    let initial = crate::session::load().preview_theme;
    // A session naming a theme the user has since deleted must not leave the menu
    // ticking nothing: resolve it back to the base theme up front, so the state and
    // what is actually rendered always agree (`Themes::resolve` degrades the same way).
    let themes = crate::theme::themes();
    let initial = if themes.contains(&initial) {
        initial
    } else {
        crate::theme::SYSTEM_ID.to_string()
    };
    crate::theme::set_active(&initial);
    // Regenerate the theme sheet for the just-restored theme. Registration order in
    // `on_startup` must not decide whether the first window paints in the saved
    // theme or flashes the default, so this makes the function self-consistent
    // rather than relying on it running before `init_resources_and_css` (TDD 18.12).
    crate::app::reload_theme_css();

    // STATEFUL string action (state = active theme id). The View ▸ Reading Theme menu
    // items target it, and menu items on a stateful action render as RADIOs with a tick
    // on the active theme — consistent with the other menubar radio menus (view-mode).
    // View ▸ Reading Theme is a NESTED submenu, so activating an item hits the GTK
    // 4.6 GtkPopoverMenuBar bug that leaves a stray top-level menubar popover mapped
    // — clicking a theme would otherwise "activate" File (ScrAP-116). This
    // action is `app.`-scoped, so the dismissal routes to the active window (the one
    // the user clicked in); both that and `set_state` are carried by the
    // `nested_submenu_app_stateful_action` choke point rather than hand-wired here.
    let action = crate::window::nested_submenu_app_stateful_action(
        app,
        "preview-theme",
        Some(&gtk::glib::VariantType::new("s").unwrap()),
        &initial.to_variant(),
        move |app, value| {
            let Some(id) = value.str().map(str::to_owned) else {
                return;
            };
            crate::theme::set_active(&id);
            // Re-generate the theme's CSS, then re-render every window's preview through
            // the SAME sweep a desktop theme change already uses — a theme switch is the
            // same event as far as the app is concerned, so it needs no path of its own
            // (and inherits that path's in-place per-tab re-render, which preserves each
            // tab's mode and reading position, and refreshes each window's theme-button
            // label to the newly active theme).
            crate::app::re_render_all_windows(app);
        },
    );
    app.add_action(&action);

    // `app.pick-preview-theme` — a STATELESS presentation shim for the TOOLBAR picker.
    // The toolbar combobox must show PLAIN items (consistent with the Heading picker),
    // but menu items render as radios whenever they target a STATEFUL action — so the
    // toolbar targets THIS stateless action, which merely forwards its target to
    // `preview-theme`'s state. ALL switch logic stays in `preview-theme`'s change_state
    // (one source of truth — this shim carries none), so the plain toolbar and the radio
    // menu can never diverge: two presentations, one command.
    let pick = SimpleAction::new(
        "pick-preview-theme",
        Some(&gtk::glib::VariantType::new("s").unwrap()),
    );
    pick.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_pick, target| {
            let Some(target) = target else {
                return;
            };
            app.change_action_state("preview-theme", target);
        }
    ));
    app.add_action(&pick);
}
