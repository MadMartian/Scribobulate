//! Click-time dispatch for a link found in the preview: the SINGLE
//! choke point deciding whether a clicked URL is an external launch (governed by
//! `links::is_allowed_url`/`open_url`, unchanged) or in-app navigation to a local
//! Markdown sibling (governed by `links::resolve_doc_link`'s containment gate).
//! `preview/interactions.rs` calls in here for every non-anchor link click so the
//! two gates can never be asked the wrong question again — see `links.rs`'s
//! module doc for why that conflation was a root cause of bugs.

use crate::links::{self, LinkResolution};
use crate::preview::scroll_preview_to_fragment;
use crate::winstate::{TabState, ERROR_NOTICE_TIME};
use gtk::prelude::*;
use gtk::ApplicationWindow;
use std::path::Path;
use std::rc::Rc;

/// Activate a link clicked in `view`'s preview. `url` is whatever the renderer
/// captured as the link's href — still fully unvalidated (untrusted content,
/// TDD 2.7).
///
/// Three-way split, in order:
/// 1. A same-document `#fragment` never reaches here — `preview/interactions.rs`
///    handles that itself (needs the live `RenderData.heading_map`).
/// 2. An allowed external scheme (`http`/`https`/`mailto`) goes through the
///    EXISTING, unchanged `links::open_url` gate.
/// 3. Anything else with a scheme (`file://`, `smb://`, `javascript:`, …) is
///    refused — same policy as `is_allowed_url`, but now with a VISIBLE error
///    instead of only a WARN log line.
/// 4. A schemeless reference resolves against the CURRENT tab's document folder
///    via `links::resolve_doc_link`, applying that tab's own
///    `allow_outside_links` toggle.
pub(crate) fn activate_doc_link(view: &crate::codeview::CodePreviewView, url: &str) {
    let Some((window, tab)) = crate::winstate::tab_for_descendant(view) else {
        return;
    };

    if links::is_allowed_url(url) {
        links::open_url(url);
        return;
    }

    if let Some(scheme) = links::scheme_of(url) {
        log::warn!("refusing to open URL with disallowed scheme: {url}");
        show_link_error(
            &tab,
            format!("Blocked link — \"{scheme}:\" links are never opened from a document"),
        );
        return;
    }

    match links::resolve_doc_link(url, tab.doc_dir().as_deref(), tab.allow_outside_links.get()) {
        LinkResolution::Navigate(path) => {
            // `resolve_doc_link` itself drops the `#fragment` when deciding
            // WHICH file to open (it only affects the base path today) —
            // re-extract it here so `open_doc_link_target` can scroll the
            // target document to the matching heading once it's open.
            open_doc_link_target(&window, &path, links::doc_link_fragment(url).as_deref());
        }
        LinkResolution::NotMarkdown(path) => {
            show_link_error(&tab, format!("Not a Markdown document: {}", path.display()));
        }
        LinkResolution::Refused => show_link_error(
            &tab,
            "Blocked: link target is outside this document's folder \
             (File ▸ Load Unsafe Linked Documents to permit it)"
                .to_string(),
        ),
        LinkResolution::Missing => {
            show_link_error(&tab, format!("Link target not found: {url}"));
        }
    }
}

/// Push a transient error notice on `tab`'s status bar and self-clear it after
/// [`ERROR_NOTICE_TIME`] — mirrors `app/open.rs`'s "File deleted on disk"
/// notice so every "your click did nothing, and here is why" surfaces the same
/// way across the app.
fn show_link_error(tab: &Rc<TabState>, msg: String) {
    tab.chrome().push_timed_notice(&msg, ERROR_NOTICE_TIME);
}

/// Open `path` (already confirmed to be a Markdown file within the allowed
/// containment) as a new tab in `window` — or focus it if it is already open
/// somewhere (TDD 8.2/15.16, `app::find_open_tab_for_path`, which the link path
/// inherits for free rather than duplicating). New tabs opened this way are
/// switched to immediately (not deferred) since the reader explicitly navigated
/// here, mirroring interactive File ▸ Open's `defer = false`.
///
/// `fragment` is the decoded `#section` slug from the ORIGINAL link href, if
/// any (already split off by `resolve_doc_link` for the file-resolution step
/// and re-extracted by the caller via `links::doc_link_fragment`). After the
/// target tab is open/focused, its own `heading_map` is consulted and, on a
/// match, scrolled to — the same map a same-document `#anchor` click already
/// reads (`preview/interactions.rs`), just against the OTHER tab's render
/// state. An unmatched fragment is silently ignored: a same-document anchor
/// click that slugs to nothing is already silent today (see
/// `interactions.rs`'s link-click handler), and a cross-document fragment is
/// the same kind of "heading not found" outcome — the document still opened
/// correctly, which is the part that matters; two different UX treatments for
/// the same failure shape would be the inconsistency, not the silence.
///
/// Deliberately does NOT copy `allow_outside_links` from the SOURCE tab onto the
/// destination — see that field's doc comment on `TabState` for why permission
/// re-roots at every hop instead of ratcheting outward. `allow_unsafe_images`
/// (the unrelated image-rendering toggle) DOES carry forward from the window's
/// active tab, matching the existing glob-open convention in `app/setup.rs`.
fn open_doc_link_target(window: &ApplicationWindow, path: &Path, fragment: Option<&str>) {
    let Some(app) = window.application() else {
        return;
    };
    if let Some((win, tab)) = crate::app::find_open_tab_for_path(&app, path) {
        crate::app::focus_tab(&win, &tab);
        // MEASURED, not assumed: `focus_tab` → `TabView::focus_page` →
        // `set_current_page` → `switch_to_index` (`widgets/tab/ops.rs`) invokes
        // the switch-page callback SYNCHRONOUSLY — which runs
        // `on_active_tab_changed` → `materialize_deferred_preview`
        // (`window/tabs/switch.rs`) before `switch_to_index` returns. So even a
        // tab that was added in the background and never yet visited already
        // has a rendered preview — and a populated `heading_map` — by the time
        // control returns here; no extra "wait for it to finish" step is
        // needed. A tab with no preview at all (Edit view mode) simply has
        // nothing to scroll — `preview_scroller()` returns `None` for it below.
        if let Some(fragment) = fragment {
            if let Some(sw) = tab.split.preview_scroller() {
                // The arrival carries the heading (TDD 23.11), so Forward lands
                // back on it rather than on wherever that tab happens to sit.
                // Recorded against the window the tab is now in, which is `win`
                // and not necessarily the one the link was clicked in.
                record_fragment_arrival(&win, &tab, fragment);
                scroll_preview_to_fragment(&sw, fragment);
            }
        }
        return;
    }

    // The read leaves the main thread (`docio`), so the tab is built when it comes
    // back rather than in this call. Everything after the `await` is the ORIGINAL
    // body, unchanged and still one uninterrupted synchronous block — which is the
    // property that matters: tab creation, monitor attachment and the fragment
    // scroll cannot be interleaved with another navigation's, because the main loop
    // never runs between them.
    //
    // The window is re-resolved through a weak reference afterwards: a read that
    // takes any real time is a window the user can close in the meantime, and a
    // strong capture here would keep the whole widget subtree alive past its own
    // teardown and then build a tab into it (ScrAP-152 / GTK4Rs/AP-161).
    let win_weak = window.downgrade();
    let path = path.to_path_buf();
    let fragment = fragment.map(str::to_owned);
    // Following a link is a gesture with no other feedback until the tab appears, so a
    // slow target document would otherwise look like a click that did nothing.
    let busy = crate::winstate::chrome(window)
        .map(|chrome| crate::winstate::BusyNotice::arm(&chrome, "Opening…"));
    gtk::glib::MainContext::default().spawn_local(async move {
        let _busy = busy;
        let doc = crate::docio::read_document(Some(&path)).await;
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        // `doc.title` unused directly — `create_tab_in_window` already relabels the
        // window via `update_window_title` (every tab's filename in the strip,
        // active tab's in the titlebar), same as every other tab-adding call site.
        let allow_unsafe_images = crate::winstate::state(&window)
            .map(|st| st.allow_unsafe_images.get())
            .unwrap_or(false);
        let Some(tab_id) = crate::window::create_tab_in_window(
            &window,
            &doc.source,
            doc.backing.as_deref(),
            allow_unsafe_images,
            false,
        ) else {
            return;
        };
        let new_tab = crate::winstate::tab_by_id(tab_id);
        if let (Some(p), Some(t)) = (doc.backing, new_tab.as_ref()) {
            crate::app::attach_file_backing(&window, t, p);
        }
        // MEASURED, not assumed: this call site passes `defer = false`, and
        // `create_tab_in_window` renders that tab's preview SYNCHRONOUSLY when
        // `defer` is false — `render_and_wire_preview` → `preview::render`
        // builds and stores `RenderData` (including `heading_map`) before
        // returning; `preview/render.rs` has no `idle_add`/`timeout_add`/async
        // anywhere in that path. So the new tab's `heading_map` is already
        // populated here — the "hook that fires after tab creation completes"
        // is simply the point right after this call returns, not a mechanism
        // that needs to be built.
        if let (Some(fragment), Some(t)) = (fragment, new_tab) {
            if let Some(sw) = t.split.preview_scroller() {
                record_fragment_arrival(&window, &t, &fragment);
                scroll_preview_to_fragment(&sw, &fragment);
            }
        }
    });
}

/// Record a cross-document fragment link's ARRIVAL as a within-document place
/// (TDD 23.11), so Forward re-lands on the heading instead of on whatever
/// position the target tab was left at.
///
/// Passes `from: None` deliberately: the departure was a *different* document, so
/// there is nothing in this tab for the reader to have moved off, and
/// `record_jump` only ever stamps the entry it is leaving when that entry names
/// this same tab. The tab switch itself was already recorded by the switch-page
/// choke point before we get here, so this appends the position within it.
fn record_fragment_arrival(
    window: &ApplicationWindow,
    tab: &std::rc::Rc<crate::winstate::TabState>,
    fragment: &str,
) {
    crate::window::record_in_document_jump(
        window,
        tab,
        crate::winstate::NavSpot::Heading(fragment.to_string()),
    );
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::codeview::CodePreviewView;

    /// A fresh temporary directory, returned with its path already canonical.
    ///
    /// Every test below builds an expected path from this directory and compares it
    /// against the path a tab actually recorded — and the tab's path went through the
    /// resolver, which canonicalises. On macOS `$TMPDIR` lives under `/var`, which is a
    /// symlink to `/private/var`, so the raw `tempfile` path and the resolved one are
    /// two spellings of the same file: every such comparison fails there while passing
    /// everywhere else, and it fails by reporting that the wrong tab backs the target —
    /// blaming the navigation for a path-spelling mismatch.
    ///
    /// Deliberately NOT `#[cfg(target_os = "macos")]`-gated: it is a no-op wherever the
    /// temp dir is not reached through a symlink, and a comparison that only holds on
    /// one platform is the defect, not the platform.
    ///
    /// The `TempDir` guard is returned alongside the path because dropping it deletes
    /// the directory — bind it, never discard it.
    fn canonical_tempdir() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dunce::canonicalize(dir.path()).expect("the temp dir canonicalises");
        (dir, path)
    }
    use crate::window::new_window;
    use crate::winstate::state;
    use gtk::gio::ApplicationFlags;

    fn make_app(name: &str) -> gtk::Application {
        let app = gtk::Application::new(Some(name), ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        app
    }

    /// `tab`'s currently-mounted preview `CodePreviewView`, or panics — every
    /// tab built by these tests stays in the default Preview mode, so this is
    /// always `Some` unless the render wiring itself is broken (which the test
    /// calling this wants to fail loudly on, not silently skip).
    fn preview_view_of(tab: &TabState) -> CodePreviewView {
        tab.split
            .preview_scroller()
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("a Preview-mode tab has a mounted CodePreviewView")
    }

    /// Drain the default `MainContext`'s pending sources (non-blocking
    /// iterations only — never waits for new work). `scroll_to_buffer_offset`
    /// (`codeview/geometry.rs`) defers its actual `scroll_to_mark` call to an
    /// `idle_add_local_once` on this SAME shared default context; every
    /// `#[gtktest::test]` in the binary runs on one thread against that one
    /// context (GTK4Rs/AP-71), so a source left pending when a test's window is
    /// destroyed would otherwise fire LATER, inside a completely unrelated
    /// test's own main-loop pump, against an already-destroyed view — the
    /// `gtk_widget_realize`/`gdk_surface_new_popup` crash this call prevents.
    /// Called before every `window.destroy()` below that may have triggered a
    /// scroll (mirrors `preview/scroll.rs`'s own `pump_until` teardown
    /// discipline for the same idle).
    fn drain_pending_idle() {
        let ctx = gtk::glib::MainContext::default();
        while ctx.pending() {
            ctx.iteration(false);
        }
    }

    /// Pump the main loop until `window` holds `n` tabs, or panic.
    ///
    /// Following a document link now reads the target off the main thread
    /// (`crate::docio`), so the new tab appears when that read comes back rather
    /// than before `activate_doc_link` returns — a `gio::spawn_blocking` completion
    /// posted to this context, hence `Clock::Worker`.
    ///
    /// `crate::testpump::until` (M31). The pre-migration version here had NO watchdog
    /// timeout source, only a 2000-iteration count while blocking on `iteration(true)`
    /// — which is exactly the GTK4Rs/AP-79 hazard the rest of this crate's copies
    /// guard against: with nothing ever becoming ready, the very first `iteration(true)`
    /// blocks forever and the "budget" of 2000 is never reached. `until` closes that
    /// gap with a real timeout source before the loop.
    fn pump_until_tabs(window: &ApplicationWindow, n: usize) {
        crate::testpump::until(
            crate::testpump::Clock::Worker,
            &format!("the link navigation to produce {n} tabs"),
            || crate::winstate::tabs_for_window(window).len() >= n,
        );
    }

    /// The buffer offset `view`'s "scrib-outline-target" mark currently sits
    /// at, or `None` if `scroll_to_buffer_offset` was never called on `view`.
    /// `CodePreviewView::scroll_to_buffer_offset` (`codeview/geometry.rs`)
    /// installs this mark SYNCHRONOUSLY — only the actual `scroll_to_mark`
    /// call is deferred to an idle — so this is provable right after a call
    /// returns, with no main-loop pump needed.
    fn scroll_target_offset(view: &CodePreviewView) -> Option<i32> {
        let buffer = view.buffer();
        let mark = buffer.mark("scrib-outline-target")?;
        Some(buffer.iter_at_mark(&mark).offset())
    }

    /// `TARGET.md`'s own "section-two" heading's buffer offset, read from its
    /// preview's `heading_map` — the value every assertion below compares the
    /// scroll target against, computed independently of what
    /// `open_doc_link_target` is being tested for.
    fn heading_map_offset(view: &CodePreviewView, slug: &str) -> i32 {
        let rd =
            crate::preview::scrib_render_data(view).expect("a rendered preview stores RenderData");
        let rd = rd.borrow();
        *rd.heading_map
            .get(slug)
            .unwrap_or_else(|| panic!("no heading slugs to \"{slug}\" in this document"))
    }

    const TARGET_MD: &str =
        "# Section One\n\nFiller paragraph so Section Two isn't at offset 0.\n\n\
         # Section Two\n\nThe paragraph the fragment must land on.\n";

    /// TDD 19.10 — new-tab case: a cross-document
    /// `doc.md#fragment` link must not just open the right document, it must
    /// land scrolled to the matching heading, not the top.
    #[gtktest::test]
    fn fragment_link_to_a_new_tab_scrolls_to_the_matching_heading() {
        let (_dir, dir) = canonical_tempdir();
        let target_path = dir.join("TARGET.md");
        std::fs::write(&target_path, TARGET_MD).unwrap();
        let source_md = "# Source\n\n[link](TARGET.md#section-two)\n";
        let source_path = dir.join("SOURCE.md");
        std::fs::write(&source_path, source_md).unwrap();

        let app = make_app("com.extollit.scribobulate.integrationtest.linknav.newtab");
        let window = new_window(&app, "IT", source_md, Some(&source_path));
        let source_tab = state(&window).expect("state registered after new_window");
        let source_view = preview_view_of(&source_tab);

        activate_doc_link(&source_view, "TARGET.md#section-two");
        pump_until_tabs(&window, 2);

        let tabs = crate::winstate::tabs_for_window(&window);
        assert_eq!(tabs.len(), 2, "the link must open the target as a new tab");
        let target_tab = tabs
            .iter()
            .find(|t| t.path.borrow().as_deref() == Some(target_path.as_path()))
            .expect("one of the two tabs backs TARGET.md");
        let target_view = preview_view_of(target_tab);

        let expected = heading_map_offset(&target_view, "section-two");
        assert_ne!(
            expected, 0,
            "sanity: Section Two is not the document's first offset"
        );
        assert_eq!(
            scroll_target_offset(&target_view),
            Some(expected),
            "the new tab must be scrolled to the Section Two heading, not left at the top"
        );

        drain_pending_idle();
        window.destroy();
    }

    /// The already-open-tab case: a fragment link to a document that is
    /// ALREADY open in another tab must focus that tab AND scroll it — not
    /// just switch to it and leave it wherever it happened to be.
    #[gtktest::test]
    fn fragment_link_to_an_already_open_tab_focuses_and_scrolls_it() {
        let (_dir, dir) = canonical_tempdir();
        let target_path = dir.join("TARGET.md");
        std::fs::write(&target_path, TARGET_MD).unwrap();
        let source_md = "# Source\n\n[link](TARGET.md#section-two)\n";
        let source_path = dir.join("SOURCE.md");
        std::fs::write(&source_path, source_md).unwrap();

        let app = make_app("com.extollit.scribobulate.integrationtest.linknav.reuse");
        let window = new_window(&app, "IT", source_md, Some(&source_path));
        let source_tab = state(&window).expect("state registered after new_window");

        // Open TARGET.md as a second tab up front (simulating "already open"),
        // then switch back to the source tab — mirroring a reader who has both
        // documents open and clicks the link from the source tab.
        // Fixture setup, not the reader under test: read it directly rather than
        // driving `docio`'s async path just to seed a second tab.
        let target_md = std::fs::read_to_string(&target_path).unwrap();
        let target_backing = Some(target_path.clone());
        let target_tab_id = crate::window::create_tab_in_window(
            &window,
            &target_md,
            target_backing.as_deref(),
            false,
            false,
        )
        .expect("create_tab_in_window returns the new tab's id");
        let target_tab = crate::winstate::tab_by_id(target_tab_id).expect("target tab registered");
        *target_tab.path.borrow_mut() = target_backing.clone();
        let target_view = preview_view_of(&target_tab);
        assert_eq!(
            scroll_target_offset(&target_view),
            None,
            "sanity: the freshly-opened target tab starts with no scroll target"
        );

        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        chrome.tabs.focus_page(&source_tab.content_box);
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(source_tab.id),
            "sanity: the source tab is active before the click"
        );
        let source_view = preview_view_of(&source_tab);

        activate_doc_link(&source_view, "TARGET.md#section-two");

        assert_eq!(
            state(&window).map(|t| t.id),
            Some(target_tab.id),
            "the click must focus the already-open target tab, not open a duplicate"
        );
        let tabs = crate::winstate::tabs_for_window(&window);
        assert_eq!(
            tabs.len(),
            2,
            "no duplicate tab was created for the already-open target"
        );

        let expected = heading_map_offset(&target_view, "section-two");
        assert_eq!(
            scroll_target_offset(&target_view),
            Some(expected),
            "focusing the already-open tab must also scroll it to the fragment's heading"
        );

        drain_pending_idle();
        window.destroy();
    }

    /// TDD 19 follow-up — an unmatched fragment must not crash and must not
    /// block the document from opening; per `open_doc_link_target`'s decision,
    /// it is also silent (no status-bar notice), matching a same-document
    /// anchor click that slugs to nothing.
    #[gtktest::test]
    fn unknown_fragment_still_opens_the_document_without_scrolling_or_erroring() {
        let (_dir, dir) = canonical_tempdir();
        let target_path = dir.join("TARGET.md");
        std::fs::write(&target_path, TARGET_MD).unwrap();
        let source_md = "# Source\n\n[link](TARGET.md#no-such-heading)\n";
        let source_path = dir.join("SOURCE.md");
        std::fs::write(&source_path, source_md).unwrap();

        let app = make_app("com.extollit.scribobulate.integrationtest.linknav.unknown");
        let window = new_window(&app, "IT", source_md, Some(&source_path));
        let source_tab = state(&window).expect("state registered after new_window");
        let source_view = preview_view_of(&source_tab);

        activate_doc_link(&source_view, "TARGET.md#no-such-heading");
        pump_until_tabs(&window, 2);

        let tabs = crate::winstate::tabs_for_window(&window);
        assert_eq!(
            tabs.len(),
            2,
            "the document must still open despite the unknown fragment"
        );
        let target_tab = tabs
            .iter()
            .find(|t| t.path.borrow().as_deref() == Some(target_path.as_path()))
            .expect("TARGET.md opened even though its fragment matched nothing");
        let target_view = preview_view_of(target_tab);
        assert_eq!(
            scroll_target_offset(&target_view),
            None,
            "an unmatched fragment must not install a scroll target (silent, not an error)"
        );
        assert!(
            target_tab.chrome().status.borrow().is_empty(),
            "an unmatched fragment must not raise a status-bar notice either"
        );

        drain_pending_idle();
        window.destroy();
    }

    /// Regression guard — no fragment at all must behave exactly as before
    /// this fix: the target opens as a new tab and nothing scrolls it.
    #[gtktest::test]
    fn a_fragmentless_link_still_just_opens_the_target_with_no_scroll() {
        let (_dir, dir) = canonical_tempdir();
        let target_path = dir.join("TARGET.md");
        std::fs::write(&target_path, TARGET_MD).unwrap();
        let source_md = "# Source\n\n[link](TARGET.md)\n";
        let source_path = dir.join("SOURCE.md");
        std::fs::write(&source_path, source_md).unwrap();

        let app = make_app("com.extollit.scribobulate.integrationtest.linknav.nofragment");
        let window = new_window(&app, "IT", source_md, Some(&source_path));
        let source_tab = state(&window).expect("state registered after new_window");
        let source_view = preview_view_of(&source_tab);

        activate_doc_link(&source_view, "TARGET.md");
        pump_until_tabs(&window, 2);

        let tabs = crate::winstate::tabs_for_window(&window);
        assert_eq!(
            tabs.len(),
            2,
            "a fragmentless link must still open the target as a new tab"
        );
        let target_tab = tabs
            .iter()
            .find(|t| t.path.borrow().as_deref() == Some(target_path.as_path()))
            .expect("one of the two tabs backs TARGET.md");
        let target_view = preview_view_of(target_tab);
        assert_eq!(
            scroll_target_offset(&target_view),
            None,
            "no fragment means no scroll target — unchanged from before this fix"
        );

        drain_pending_idle();
        window.destroy();
    }

    /// TDD 23.11's cross-document clause walked end to end: following
    /// `TARGET.md#section-two` records the tab switch *and* the heading it
    /// arrived at, so Back retraces both — first back up the target document,
    /// then across to the source — and Forward is the inverse of each.
    ///
    /// The middle stop is the one worth having, and it found a second defect. The
    /// arrival stamps the just-created target tab's entry with the position it was
    /// at *before* the fragment scrolled it, which for a brand-new tab is **line
    /// 0** — so this crosses ScrAP-262's path in the cross-document direction,
    /// where the reader never chose the departure at all; it is simply where a
    /// fresh document starts. But that position is read in the same synchronous
    /// turn the tab is built in, before GTK has allocated the view, and
    /// `line_at_y` answers an unallocated view with the buffer's **last** line
    /// (ScrAP-263) — so the stamp was the end of the document, and one Back threw
    /// the reader there.
    ///
    /// Mutation-checked against both mechanisms, which fail with different
    /// signatures: dropping the viewport gate in `saferizer::viewport` lands the
    /// first Back on line 6 (the target's last line), and restoring the
    /// `line <= 0` return in `restore_preview_scroll_to_line` lands it on line 4
    /// (section two — Back did nothing at all).
    #[gtktest::test]
    fn a_fragment_link_to_another_document_walks_back_across_both_stops() {
        let (_dir, dir) = canonical_tempdir();
        let target_path = dir.join("TARGET.md");
        std::fs::write(&target_path, TARGET_MD).unwrap();
        let source_md = "# Source\n\n[link](TARGET.md#section-two)\n";
        let source_path = dir.join("SOURCE.md");
        std::fs::write(&source_path, source_md).unwrap();

        let app = make_app("com.extollit.scribobulate.integrationtest.linknav.navwalk");
        let window = new_window(&app, "IT", source_md, Some(&source_path));
        let source_tab = state(&window).expect("state registered after new_window");
        let source_id = source_tab.id;
        let source_view = preview_view_of(&source_tab);

        activate_doc_link(&source_view, "TARGET.md#section-two");
        pump_until_tabs(&window, 2);

        let target_tab = crate::winstate::tabs_for_window(&window)
            .into_iter()
            .find(|t| t.path.borrow().as_deref() == Some(target_path.as_path()))
            .expect("one of the two tabs backs TARGET.md");
        let target_view = preview_view_of(&target_tab);
        let section_two = target_view
            .buffer()
            .iter_at_offset(heading_map_offset(&target_view, "section-two"))
            .line();
        assert_eq!(
            target_view.reading_line(),
            section_two,
            "precondition: the link landed on the heading in the new document"
        );

        // Stop 1 — still in the target document, back at the top it opened on.
        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(target_tab.id),
            "the first Back retraces the jump WITHIN the document that was opened"
        );
        assert_eq!(
            target_view.reading_line(),
            0,
            "and returns it to where it was before the fragment scrolled it"
        );

        // Stop 2 — across the document boundary, back to where the link was.
        WidgetExt::activate_action(&window, "win.nav-back", None).expect("nav-back activates");
        assert_eq!(
            state(&window).map(|t| t.id),
            Some(source_id),
            "only the second Back crosses back to the document holding the link"
        );

        // Forward retraces both, in order.
        WidgetExt::activate_action(&window, "win.nav-forward", None).expect("activates");
        assert_eq!(state(&window).map(|t| t.id), Some(target_tab.id));
        WidgetExt::activate_action(&window, "win.nav-forward", None).expect("activates");
        assert_eq!(
            target_view.reading_line(),
            section_two,
            "the last Forward returns to the heading the link named"
        );

        drain_pending_idle();
        window.destroy();
    }
}
