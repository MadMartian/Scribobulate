//! Window title, View ▸ Documents submenu, and tab-strip label upkeep — all
//! the surfaces that must track a window's tab SET and each tab's display name
//! (operator decisions Q7/Q14; GTK4Rs/AP-76 for the deferred GMenu
//! rebuild). Split out of the former monolithic `window/tabs.rs`.

use super::super::*;

/// Window title (operator decisions Q7/Q14): the ACTIVE tab's filename, plus a
/// count of the window's other tabs once it has any ("notes.md (+2 documents) —
/// Scribobulate"). Also keeps every tab's own label text in sync, since it
/// depends on the same "how many tabs does this window have" fact — called on
/// every event that changes a window's tab count (create, close, and both sides
/// of a cross-window move).
///
/// A plain tab SWITCH changes no tab's label and no count, but it does change
/// which document the title names — that path calls [`retitle_window`] alone
/// (via `switch::refresh_tab_surfaces`), so a switch costs one `set_title` and
/// rebuilds nothing (GTK4Rs/AP-76).
pub(crate) fn update_window_title(window: &ApplicationWindow) {
    let tabs = winstate::tabs_for_window(window);
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    // Moving a window's only tab elsewhere would just leave an identical,
    // empty-of-purpose window behind — there's nothing useful for the
    // command to do, so it's disabled rather than a no-op / confusing prompt.
    set_action_enabled(window, "move-tab-new-window", tabs.len() > 1);
    retitle_window(window);
    for tab in &tabs {
        set_tab_label(&chrome, tab);
    }
    // The set of open tabs (and/or their filenames) may have just changed — open,
    // new, close, and cross-window move all funnel through here — so refresh this
    // window's View ▸ Documents list too.
    refresh_documents_menu(window);
}

/// Set `window`'s title from the one shared formula
/// ([`winstate::window_title_for_tabs`]) — the ACTIVE tab's filename plus a count
/// of the others.
///
/// Its own function, rather than a step inside [`update_window_title`], because
/// the title now has two distinct triggers with different costs: anything that
/// changes the tab SET or a tab's NAME (which must also relabel the strip and
/// rebuild the Documents menu) goes through `update_window_title`, while a plain
/// tab switch changes only *which* tab is named and comes straight here. Both
/// resolve the name the same way, so the two triggers cannot disagree about what
/// the window is called.
///
/// The active tab is read via `winstate::state` — the same accessor the toolbar's
/// documents combo uses (`documents_button_label`) — so title and combo name the
/// same document by construction. A window momentarily without an active tab (the
/// instant between removing the active tab and the notebook settling on its
/// successor) has no filename to show and falls back to the app name; the
/// switch-page that follows retitles it for real.
///
/// Visibility is deliberately `pub(super)`, not `pub(crate)`: the only legitimate
/// callers are this module and its sibling `switch`, and a title-only refresh
/// reachable from anywhere else is how a call site that *also* changed a tab's name
/// or the tab set comes to retitle the window without relabelling the strip or
/// rebuilding the Documents menu — a half-updated derived view (Derived-view CAM
/// row 4). Outside `window::tabs` the entry point is `update_window_title`, and the
/// compiler enforces that rather than a convention (GTK4Rs/AP-130).
pub(super) fn retitle_window(window: &ApplicationWindow) {
    let active_name = state(window).and_then(|tab| {
        tab.path
            .borrow()
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    });
    window.set_title(Some(&winstate::window_title_for_tabs(
        winstate::tab_count(window),
        active_name.as_deref(),
    )));
}

/// Schedule a rebuild of `window`'s `View ▸ Documents` submenu to match its open
/// tabs. **Coalesced and deferred to idle** — this is load-bearing, not an
/// optimization: mutating a `GMenu` bound to a live (possibly OPEN)
/// `GtkPopoverMenuBar` is synchronous, and doing it from inside a menu item's own
/// activation frees the `GtkModelButton` mid-`clicked` → a use-after-free (our
/// prior crash class; researcher-confirmed against gtkmenusectionbox.c). Every
/// caller therefore only marks a dirty flag here and the actual `remove_all` +
/// re-append happens later at main-loop top level, out of any signal dispatch.
///
/// Call this on EVERY event that changes the tab SET or a tab's DISPLAY NAME:
///   • open a file / New Document — via `update_window_title`
///   • close a tab — via `update_window_title`
///   • move a tab to another window / pop out — via `update_window_title` on BOTH
///     the source and destination windows (`wire_tab_arrival`)
///   • in-window drag-reorder — via the drag source's `drag-end` (`wire_tab_bar_dnd`)
///   • rename (Save As adopts a path) — `window/save.rs::adopt_and_save`
/// A plain tab SWITCH is deliberately NOT in this list: it only re-points the
/// `select-tab` action state (`on_active_tab_changed`), rebuilding nothing.
/// See GTK4Rs/AP-76.
pub(crate) fn refresh_documents_menu(window: &ApplicationWindow) {
    // Deferred through the shared choke point rather than a private idle of its own.
    // This site had the mitigation first and the sibling `Format ▸` submenu did not;
    // routing both through one function is what stops the next live-bound menu being
    // the third site to decide for itself (GTK4Rs/AP-76).
    crate::app::defer_live_menu_mutation(
        window,
        |chrome| &chrome.documents_refresh_scheduled,
        rebuild_documents_menu,
    );
}

/// Rebuild `window`'s Documents submenu in place (`remove_all` + re-append). Runs
/// only from the coalesced idle in [`refresh_documents_menu`] — never call it
/// directly from a signal handler. Enumerates tabs in VISUAL STRIP order (via
/// `TabView::ordered_contents`, not registry order, which diverges after a
/// reorder — GTK4Rs/AP-74) so the list matches what the user sees, and re-asserts the
/// `select-tab` radio state so the active tab stays checked across the rebuild.
fn rebuild_documents_menu(window: &ApplicationWindow, chrome: &winstate::WindowChrome) {
    let menu = &chrome.documents_menu;
    menu.remove_all();
    for content in chrome.tabs.ordered_contents() {
        let Some(tab) = winstate::tab_by_content_box(&content) else {
            continue;
        };
        let item = gtk::gio::MenuItem::new(Some(&documents_item_label(&tab)), None);
        item.set_action_and_target_value(
            Some("win.select-tab"),
            Some(&tab.id.to_string().to_variant()),
        );
        menu.append_item(&item);
    }
    // Keep the active tab checked after the rebuild (the item set was just
    // replaced; the action state is unchanged but re-assert it defensively so a
    // freshly-appended item picks up the check immediately).
    if let Some(active) = state(window) {
        set_action_state(window, "select-tab", &active.id.to_string().to_variant());
    }
    // The toolbar combo box shares this menu's MODEL, so its item list already
    // tracked the rebuild — but its LABEL (the active document's name) is separate
    // presentational state, so nudge it here. This covers every set/name change
    // (open, close, cross-window move, and Save-As rename), which is exactly the
    // set of events that reach the rebuild; a plain tab SWITCH keeps it current via
    // `on_active_tab_changed` instead (no rebuild happens there — GTK4Rs/AP-76).
    refresh_documents_button(window);
}

/// Max glyphs shown in the open-documents combo box label before it is ellipsized,
/// so a long filename cannot stretch the toolbar. The full name stays visible in
/// the dropdown's own item and in the tab-strip tooltip.
const DOCUMENTS_BUTTON_MAX_CHARS: usize = 24;

/// Refresh the toolbar's open-documents combo box label to the active document's
/// name. Called wherever the active document — or its filename — changes: the
/// [`documents_menu`](winstate::WindowChrome::documents_menu) rebuild above
/// (open / close / move / rename) and every tab switch
/// (`window/tabs/switch::refresh_tab_surfaces`). The button's ITEM LIST tracks the
/// shared model automatically; only this label is per-window state that needs a
/// nudge — the same split the Reading Theme picker uses
/// (`window::refresh_theme_button`).
pub(crate) fn refresh_documents_button(window: &ApplicationWindow) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    chrome
        .documents_btn
        .set_label(&documents_button_label(window));
}

/// The active document's toolbar-combo label: its filename, or "Untitled" for a
/// pathless buffer, ellipsized to [`DOCUMENTS_BUTTON_MAX_CHARS`]. Unlike the
/// menu-item label ([`documents_item_label`]) this is deliberately NOT `_`-doubled
/// — a `GtkMenuButton`'s plain text label is not a mnemonic context, so an
/// underscore already renders literally.
fn documents_button_label(window: &ApplicationWindow) -> String {
    let name = state(window)
        .and_then(|tab| {
            tab.path
                .borrow()
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Untitled".to_string());
    ellipsize(&name, DOCUMENTS_BUTTON_MAX_CHARS)
}

/// Truncate `s` to at most `max` characters, appending a single-glyph ellipsis
/// when it is cut. Counts and slices by `char`, never bytes, so it can never split
/// a multi-byte UTF-8 sequence in a non-ASCII filename.
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// A tab's label for the Documents menu: its filename, or "Untitled" for a
/// pathless buffer. Underscores are DOUBLED because GMenu model labels honor `_`
/// as a mnemonic marker (a filename like `my_notes.md` would otherwise render as
/// `mynotes.md` with a hidden accelerator) — no dirty "•" marker here (that would
/// force a rebuild on every dirty toggle; the strip already shows dirtiness).
fn documents_item_label(tab: &TabState) -> String {
    let name = tab
        .path
        .borrow()
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string());
    crate::app::escape_mnemonic(&name)
}

/// Refresh one tab's own tab-strip label text (filename + a "•" dirty marker,
/// operator decision Q7) without touching the window title — the cheap path
/// called on every dirty-state change.
pub(crate) fn refresh_active_tab_label(window: &ApplicationWindow) {
    let (Some(chrome), Some(tab)) = (winstate::chrome(window), state(window)) else {
        return;
    };
    set_tab_label(&chrome, &tab);
}

/// Refresh a SPECIFIC tab's own tab-strip label directly from the tab itself —
/// unlike [`refresh_active_tab_label`], this does not require the tab to be
/// the active one. Used to badge a background tab whose own backing file
/// changed on disk (TDD 15.13), via `Rc<TabState>::chrome()`,
/// which resolves this tab's CURRENT window/strip regardless of which tab
/// that window happens to have on screen.
pub(crate) fn badge_tab_label(tab: &TabState) {
    set_tab_label(&tab.chrome(), tab);
}

fn set_tab_label(chrome: &winstate::WindowChrome, tab: &TabState) {
    chrome
        .tabs
        .set_tab_markup(&tab.content_box, &tab_display_markup(tab));
    chrome
        .tabs
        .set_tab_tooltip(&tab.content_box, &tab_tooltip_text(tab));
}

/// The amber the tab-strip "⚠" deleted-backing badge is drawn in — Adwaita
/// "yellow 5" (`#e5a50a`), chosen to stay legible on both a light and a dark
/// tab strip. The tab strip wears the desktop GTK theme, not the preview's
/// reading theme (TECH "the reading theme is preview-only"), so this is a fixed
/// app constant rather than a theme key.
const BACKING_MISSING_BADGE_COLOR: &str = "#e5a50a";

/// A tab's hover tooltip: the full absolute file path of its backing file
/// (verbatim, as stored — the same string the copy-path action yields), or
/// "Unsaved" for a buffer that has never been saved to a path. Unlike the
/// strip label (filename only) and the Documents-menu label, this is the whole
/// path so the user can disambiguate same-named files in different directories.
fn tab_tooltip_text(tab: &TabState) -> String {
    tooltip_for_path(tab.path.borrow().as_deref())
}

/// Pure core of [`tab_tooltip_text`] (display-free, unit-tested): the stored path
/// verbatim, or "Unsaved" when there is none.
fn tooltip_for_path(path: Option<&std::path::Path>) -> String {
    path.map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unsaved".to_string())
}

fn tab_display_markup(tab: &TabState) -> String {
    let name = tab
        .path
        .borrow()
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string());
    // The label is Pango markup (so the ⚠ badge can be coloured), so the
    // filename — which can legitimately contain markup metacharacters (& < >) —
    // MUST be escaped before it is interpolated, or such a name produces
    // malformed markup GTK rejects.
    let name = gtk::glib::markup_escape_text(&name);
    winstate::tab_label_markup(
        name.as_str(),
        winstate::TabBadgeState {
            dirty: tab.is_dirty(),
            pending_external: tab.pending_external.get(),
            backing_missing: tab.backing_missing.get(),
        },
        BACKING_MISSING_BADGE_COLOR,
    )
}

#[cfg(test)]
mod tests {
    use super::{ellipsize, tooltip_for_path, DOCUMENTS_BUTTON_MAX_CHARS};
    use std::path::Path;

    #[test]
    fn ellipsize_leaves_a_short_name_untouched() {
        assert_eq!(ellipsize("notes.md", 24), "notes.md");
    }

    #[test]
    fn ellipsize_at_exactly_the_limit_is_untouched() {
        let s = "a".repeat(DOCUMENTS_BUTTON_MAX_CHARS);
        assert_eq!(ellipsize(&s, DOCUMENTS_BUTTON_MAX_CHARS), s);
    }

    #[test]
    fn ellipsize_cuts_an_over_long_name_to_the_limit_with_an_ellipsis() {
        let out = ellipsize("a_very_long_document_filename.md", 10);
        // 9 kept glyphs + the single ellipsis glyph == the limit.
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('…'));
        assert_eq!(out, "a_very_lo…");
    }

    #[test]
    fn ellipsize_counts_glyphs_not_bytes_for_non_ascii() {
        // Five 2-byte glyphs; a byte-based slice would panic or split a sequence.
        let out = ellipsize("ééééé.md", 4);
        assert_eq!(out.chars().count(), 4);
        assert_eq!(out, "ééé…");
    }

    #[test]
    fn tooltip_is_the_full_path_when_saved() {
        assert_eq!(
            tooltip_for_path(Some(Path::new("/home/u/notes/todo.md"))),
            "/home/u/notes/todo.md"
        );
    }

    #[test]
    fn tooltip_is_unsaved_when_pathless() {
        assert_eq!(tooltip_for_path(None), "Unsaved");
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::super::super::*;
    use gtk::prelude::Cast;
    use std::path::Path;

    /// The toolbar's open-documents combo box must (a) share the ONE per-window
    /// `documents_menu` GMenu with the menubar's View ▸ Documents submenu — so the
    /// two surfaces can never list different documents — and (b) show the ACTIVE
    /// document's name as its label, tracking it across a tab switch. Both are the
    /// point of the feature: a second surface for the same fast-switch command, not
    /// a parallel one that can drift.
    #[gtktest::test]
    fn documents_combo_shares_the_menu_model_and_labels_the_active_document() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.docscombo"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        // A window whose sole tab has a backing path, so the combo shows a real
        // filename rather than "Untitled". `new_window` does not read the file (the
        // Markdown is passed in), so a non-existent path is fine for this test.
        let window = crate::window::new_window(
            &app,
            "IT",
            "# Alpha",
            Some(Path::new("/tmp/scrib-it/alpha.md")),
        );
        let chrome = winstate::chrome(&window).expect("chrome registered");

        // (a) ONE model, two surfaces: the combo's popup binds the very same GMenu
        // object the menubar submenu does — identity, not just an equal copy.
        let bound = chrome
            .documents_btn
            .menu_model()
            .expect("the combo's menu-model is bound after build_window");
        assert_eq!(
            bound.as_ptr() as *const (),
            chrome
                .documents_menu
                .upcast_ref::<gtk::gio::MenuModel>()
                .as_ptr() as *const (),
            "the combo must reuse the window's documents_menu, not a separate model"
        );

        // (b) The label reflects the active document. The initial rebuild is
        // deferred to idle (GTK4Rs/AP-76), so drive the same refresh synchronously here —
        // this is exactly what the idle would call.
        super::refresh_documents_button(&window);
        assert_eq!(
            chrome.documents_btn.label().map(|s| s.to_string()),
            Some("alpha.md".to_string()),
            "the combo labels the sole (active) document"
        );

        // Add a second document and switch to it (defer = false → switch-page fires
        // synchronously → `refresh_tab_surfaces` updates the label). The combo must
        // now name the newly-active document.
        crate::window::create_tab_in_window(
            &window,
            "# Beta",
            Some(Path::new("/tmp/scrib-it/beta.md")),
            false,
            false,
        )
        .expect("second tab created");
        assert_eq!(
            chrome.documents_btn.label().map(|s| s.to_string()),
            Some("beta.md".to_string()),
            "switching to the new tab retargets the combo label"
        );

        // Switch back to the first document: the label follows the active tab.
        let first = winstate::tabs_for_window(&window)
            .into_iter()
            .find(|t| {
                t.path
                    .borrow()
                    .as_deref()
                    .and_then(Path::file_name)
                    .is_some_and(|n| n == "alpha.md")
            })
            .expect("first tab still present");
        chrome.tabs.focus_page(&first.content_box);
        assert_eq!(
            chrome.documents_btn.label().map(|s| s.to_string()),
            Some("alpha.md".to_string()),
            "switching back retargets the combo label to the first document"
        );

        window.destroy();
    }

    /// **The window title names the ACTIVE document and counts the others (TDD
    /// 15.7).** Three claims, none implied by the others:
    ///
    /// 1. A lone tab is named with no parenthetical at all.
    /// 2. Opening a second document adds `(+1 document)` — singular — to the title.
    /// 3. A plain tab SWITCH re-aims the title at the newly active document. This is
    ///    the one the old count-only title had no way to get wrong, and the reason
    ///    `refresh_tab_surfaces` now retitles: nothing else fires on a switch, so
    ///    without that call the title would name whichever document happened to be
    ///    active when the tab SET last changed and then quietly stop tracking.
    ///
    /// Asserted against `window.title()` — the property the desktop actually reads —
    /// and against literal strings rather than a second call to the formula: a test
    /// that recomputes the formula agrees with any change to it, including a wrong
    /// one (GTK4Rs/AP-160's "makes the test agree with itself"). The one-shared-formula
    /// claim is `save.rs`'s to make; this test's job is the *content*.
    #[gtktest::test]
    fn window_title_names_the_active_document_and_counts_the_others() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.wintitle"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");

        let window = crate::window::new_window(
            &app,
            "IT",
            "# Alpha",
            Some(Path::new("/tmp/scrib-it/alpha.md")),
        );
        assert_eq!(
            window.title().map(|s| s.to_string()),
            Some("alpha.md — Scribobulate".to_string()),
            "a lone document is named on its own — no sibling count"
        );

        // A second document, switched to on creation (defer = false → switch-page
        // fires synchronously). Singular: one other document, not "(+1 documents)".
        crate::window::create_tab_in_window(
            &window,
            "# Beta",
            Some(Path::new("/tmp/scrib-it/beta.md")),
            false,
            false,
        )
        .expect("second tab created");
        assert_eq!(
            window.title().map(|s| s.to_string()),
            Some("beta.md (+1 document) — Scribobulate".to_string()),
            "the title names the newly active document and counts the other one"
        );

        // A third, then switch back to the first: the title must follow the ACTIVE
        // tab, and the count must follow the tab SET independently of it.
        crate::window::create_tab_in_window(
            &window,
            "# Gamma",
            Some(Path::new("/tmp/scrib-it/gamma.md")),
            false,
            false,
        )
        .expect("third tab created");
        assert_eq!(
            window.title().map(|s| s.to_string()),
            Some("gamma.md (+2 documents) — Scribobulate".to_string()),
            "two other documents pluralise the count"
        );

        let chrome = winstate::chrome(&window).expect("chrome registered");
        let first = winstate::tabs_for_window(&window)
            .into_iter()
            .find(|t| {
                t.path
                    .borrow()
                    .as_deref()
                    .and_then(Path::file_name)
                    .is_some_and(|n| n == "alpha.md")
            })
            .expect("first tab still present");
        chrome.tabs.focus_page(&first.content_box);
        assert_eq!(
            window.title().map(|s| s.to_string()),
            Some("alpha.md (+2 documents) — Scribobulate".to_string()),
            "a plain tab switch re-aims the title at the now-active document, \
             keeping the same sibling count"
        );

        window.destroy();
    }
}
