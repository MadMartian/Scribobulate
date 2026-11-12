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
                Some("Open Markdown files"),
                parent.as_ref(),
                FileChooserAction::Open,
                Some("Open"),
                Some("Cancel"),
            );
            // The reader may pick several documents at once. Nothing downstream is
            // new: one `open` invocation has always carried a whole batch (a
            // glob-expanded command line is the same shape), so the selection lands
            // as tabs of one window with the first rendered eagerly and the rest
            // deferred (TDD 1.2/1.7). Enabling it here is deliberately the ONLY
            // change to the open path — a second, multi-file-only route would be
            // the parallel path POLICY § Architecture rules forbids.
            dialog.set_select_multiple(true);
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
                    // `files()` covers both cases — a single-selection chooser
                    // answers with a one-item model — so there is one read here
                    // rather than a branch on how many the user picked.
                    if let Some(files) = super::openselection::chosen_files(&d.files()) {
                        // Any of them would do (a chooser selects within one
                        // folder), so the FIRST is taken for determinism rather
                        // than for a property of that file.
                        if let Some(path) = files[0].path() {
                            remember_dialog_dir(&path);
                        }
                        if let Some(a) = app_weak2.upgrade() {
                            // ONE invocation carrying the whole selection, never one
                            // per file. `openbatch` builds a batch in a single
                            // uninterrupted pass, so N separate invocations would be N
                            // batches racing each other for the same window — the
                            // interleaving that module exists to prevent (and its 1/A
                            // cell in the Deferred-operation CAM, where two overlapping
                            // opens can duplicate a tab).
                            //
                            // The "interactive" hint (GIO's free-form open()
                            // hint string) tells connect_open this came from
                            // File ▸ Open, not a CLI/D-Bus batch launch — see
                            // its own comment for why that changes reuse
                            // behavior (TDD 1.2).
                            a.open(&files, "interactive");
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
            // regardless of whether it starts with https:// (GTK4Rs/AP-50).
            // license_type stays on About — it is the only way to surface the
            // auto-rendered full Apache 2 text via the License button.
            let dialog = AboutDialog::builder()
                .program_name("Scribobulate")
                .version(env!("CARGO_PKG_VERSION"))
                // No platform names here, deliberately. A dialog that lists the
                // platforms it runs on is a claim that goes stale the moment another
                // one is supported, and it is the copy nobody remembers to revisit —
                // this text still said "Linux-first, also compatible with macOS" a
                // Windows port later. What the app IS does not vary by platform.
                .comments(
                    "A native Markdown viewer/editor, purpose-built for managing \
                     Markdown generated by clankers.\n\n\
                     Renders entirely on the CPU — no GPU, no WebEngine, no \
                     web stack. In an era where VRAM is precious real estate \
                     for running local models, Scribobulate stays out of the way.",
                )
                .license_type(gtk::License::Apache20)
                // The one clickable link a GtkAboutDialog offers, spent on the project
                // itself — where a reader goes for the source, the issue tracker and a
                // release. The publisher is named in the copyright line below; it does
                // not need the link. (A second link is not expressible here: the only
                // other link-bearing fields, `authors`/credits, route a `<...>` through
                // GTK's mailto: parser whatever its scheme — GTK4Rs/AP-50.)
                .website("https://github.com/MadMartian/Scribobulate")
                .website_label("Scribobulate on GitHub")
                .authors(vec!["© 2026 extollIT Enterprises".to_string()])
                .logo_icon_name(crate::icons::Icon::App.name())
                .system_information(system_info)
                .modal(true)
                .build();

            // Attribution for bundled third-party components, in three layers: the
            // syntax-highlighting grammars compiled in via `two-face` (bat's syntect
            // assets), every Rust crate statically linked into the binary, and on macOS
            // and Windows the GTK runtime the installer redistributes. All carry licences
            // that REQUIRE the copyright + licence notice to travel with a binary
            // distribution; the verbatim notices ship as THIRD-PARTY-LICENSES.md
            // (`build.rs` from `notices/`) and, for the runtime, as one directory per
            // library. This credit section is the in-app surface of that obligation, and
            // it NAMES WHERE THE FILES ARE on every platform: the 1.0.0 release dry run
            // found "in the distribution" told a macOS or Linux reader nothing they could
            // open (sdd/TDD.md 9.18). STRING LITERALS ONLY, one section, every platform's
            // path in one line: `cargo xtask lint-references` check 17 reads these
            // literals and requires the man page's CREDITS section to say the same, so a
            // per-platform `cfg!` would hide text from the gate. SHORT LINES: each line
            // is its own unwrapping GtkLabel, and one 200-character line grew the dialog
            // to 1282 pt and off the screen (Mac seat, 2026-11-12). Plain text only — a
            // `<...>` here would be mis-parsed as a mailto: link (GTK4Rs/AP-50).
            dialog.add_credit_section(
                "Bundled open-source components",
                &[
                    "Syntax-highlighting grammars — via two-face (bat / syntect assets),",
                    "licensed MIT, Apache-2.0, BSD-2-Clause and BSD-3-Clause",
                    "Every Rust crate statically linked into the program,",
                    "each with its own notice",
                    "The GTK runtime bundled on macOS and Windows",
                    "(GLib, cairo, Pango, librsvg and more), one notice folder per library:",
                    "macOS: Scribobulate.app/Contents/Resources/licenses",
                    "Windows: share/licenses in the installation folder",
                    "Full notices: THIRD-PARTY-LICENSES.md —",
                    "Linux: /usr/share/doc/scribobulate",
                    "macOS: Scribobulate.app/Contents/Resources (Show Package Contents)",
                    "Windows: the installation folder",
                ],
            );

            // Development-assistance attribution. Unlike the section above this is
            // NOT a licence obligation and no regulation compels it: the EU AI Act
            // (Reg. 2024/1689) Art. 50(2) machine-readable marking duty binds the
            // *provider* of the generating AI system, not a downstream project that
            // ships the resulting source, and its Art. 50(4) text-disclosure duty is
            // limited to deepfakes and public-interest publication (and exempts
            // content under human editorial responsibility, which this is). Reg.
            // 2026/1744 (Digital Omnibus on AI, applicable 27 Jul 2026) only relaxes
            // that regime. This section is a voluntary acknowledgement. Plain text
            // only — a `<...>` here would be mis-parsed as a mailto: link
            // (GTK4Rs/AP-50).
            dialog.add_credit_section(
                "Developed with AI assistance",
                &["Claude (Anthropic) — AI pair programming"],
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
/// warning a parent triggers on 4.6 (GTK4Rs/AP-99; see `open_url`'s own rationale).
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
    // — clicking a theme would otherwise "activate" File (GTK4Rs/AP-108). This
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

/// `app.play-animations` — the reader's Play Animations choice (TDD 27.5-27.7).
/// Nothing animates yet — this is only the control and the policy a later change
/// subscribes to.
///
/// **Process-wide, deliberately `app.`, not `win.`** — one stateful boolean `GAction`
/// (POLICY § Architecture rules: single source of truth) so toggling it in any window
/// pauses/resumes every animation in every window, and every window's View menu
/// mirrors the same tick, exactly like `app.preview-theme` above.
///
/// **A DIRECT View-menu item, not a nested submenu** — unlike `preview-theme` above,
/// this does NOT go through [`crate::window::nested_submenu_app_stateful_action`] /
/// `dismiss_stray_menubar_popovers`. That GTK 4.6-4.12 stray-popover bug
/// (GTK4Rs/AP-108) fires only when activating an item drawn in a nested submenu's OWN
/// popover; Play Animations sits directly in the View menu's own popover, in the same
/// section as Show Unsafe Images (`app/menubar.rs`'s `unsafe_images_section`) — one
/// level up from where that bug lives, the same shape as `win.show-unsafe-images`
/// itself, which carries no such wrapper either.
///
/// This action holds ONLY the reader's raw choice — never the effective (system
/// "reduce animations"-adjusted) play state. `animation::policy` is the sole place
/// that reconciles the two (TDD 27.7); this handler neither reads nor reacts to
/// `gtk-enable-animations`, so the two concerns can't tangle. It also never disables
/// itself: the toggle stays sensitive regardless of the system setting (operator
/// decision) — only its EFFECT changes.
pub(super) fn add_play_animations_action(app: &Application) {
    let initial = crate::session::load().play_animations;
    let action = SimpleAction::new_stateful(
        crate::animation::policy::ACTION_NAME,
        None,
        &initial.to_variant(),
    );
    action.connect_change_state(|act, value| {
        let Some(value) = value else { return };
        act.set_state(value);
    });
    app.add_action(&action);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;

    /// TDD 27.5/27.6: `app.play-animations` is a bare boolean toggle (no parameter
    /// type), stays sensitive (the toggle is never disabled — only its EFFECT
    /// changes with the system setting), and — on a fresh session (nothing saved
    /// yet) — defaults ON.
    #[gtktest::test]
    fn play_animations_action_is_a_stateful_bool_defaulting_on() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app =
                crate::window::testkit::test_app_suffixed("appactions.playanimations.default");
            add_play_animations_action(&app);
            let action = app
                .lookup_action("play-animations")
                .expect("add_play_animations_action must register it");
            assert!(
                action.is_enabled(),
                "stays sensitive regardless of the system setting"
            );
            assert_eq!(
                action.parameter_type(),
                None,
                "a bare boolean toggle takes no activation parameter"
            );
            assert_eq!(
                action.state().and_then(|v| v.get::<bool>()),
                Some(true),
                "first launch (no saved session) must default ON (TDD 27.6)"
            );
        });
    }

    /// TDD 27.6: a session that saved the choice OFF restores OFF on the next
    /// launch — "otherwise is whatever the reader last chose", not the default.
    #[gtktest::test]
    fn play_animations_action_restores_the_saved_choice() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            crate::session::save(&crate::session::Session {
                play_animations: false,
                ..crate::session::Session::default()
            });
            let app =
                crate::window::testkit::test_app_suffixed("appactions.playanimations.restore");
            add_play_animations_action(&app);
            let action = app.lookup_action("play-animations").unwrap();
            assert_eq!(
                action.state().and_then(|v| v.get::<bool>()),
                Some(false),
                "a saved OFF choice must restore OFF, not the ON default"
            );
        });
    }

    /// Toggling the action drives `change_state` → `set_state`, exactly like every
    /// other stateful checkbox action in this file — the View-menu tick moves, and
    /// (per `animation::policy`'s own gtktest coverage) `notify::state` fires so a
    /// `PolicyWatch` subscriber sees it.
    #[gtktest::test]
    fn play_animations_action_toggle_updates_state() {
        let dir = tempfile::tempdir().unwrap();
        crate::session::with_state_home_for_test(dir.path(), || {
            let app = crate::window::testkit::test_app_suffixed("appactions.playanimations.toggle");
            add_play_animations_action(&app);
            let action = app.lookup_action("play-animations").unwrap();
            // `change_state`, not `activate`: the action's PARAMETER type is `None`
            // (a bare checkbox, no target) while its STATE type is boolean — passing
            // a parameter to `activate` on a `None`-parameter-type action trips
            // GLib's own `g_simple_action_activate` assertion. `change_state` targets
            // the state directly, exactly what the real View-menu checkbox drives.
            action.change_state(&false.to_variant());
            assert_eq!(action.state().and_then(|v| v.get::<bool>()), Some(false));
        });
    }

    /// TDD 27.5: no accelerator is EVER registered for it. It is not wired through
    /// any `Cmd`/`FmtCmd`/`InlineCmd` descriptor (the tables `register_accelerators`
    /// reads), so there is no accel string anywhere for it to bind.
    #[gtktest::test]
    fn play_animations_action_has_no_accelerator() {
        let app = crate::window::testkit::test_app_suffixed("appactions.playanimations.accel");
        crate::app::register_accelerators(&app);
        assert!(
            app.accels_for_action("app.play-animations").is_empty(),
            "Play Animations must never gain a keyboard shortcut (TDD 27.5)"
        );
    }
}
