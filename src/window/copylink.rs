//! Edit ▸ Copy Link Location — the `win.copy-link-location` action and the one
//! place its enabled state is computed.
//!
//! The command copies a link's destination, and it has **two ways to know which
//! link the reader means**, because its surfaces do:
//!
//! * **The pointer**, when the command is reached from a right-click — the reader
//!   pointed at one particular link, exactly as they would in a browser. Stored
//!   for the life of that one popover in `WindowChrome::ctx_link`.
//! * **The editor caret**, for every surface that has no pointer: the menu bar,
//!   the toolbar, and a right-click that landed on ordinary text.
//!
//! Those are two *inputs to one gate*, not two gates: there is still one
//! `SimpleAction`, one function that computes its `enabled`, and every surface —
//! the context-menu row included — merely reads `is_enabled()` (POLICY
//! single-source-of-truth; ScrAP-9). A per-surface sensitivity check would be the
//! forbidden shape, and would also be the one that drifts.
//!
//! **The pointer input is what makes the preview pane's row live at all.** The
//! preview is read-only and has no caret; gating the command on the editor caret
//! alone left its row permanently greyed — present, and never usable from the
//! surface it appeared on. In preview-only mode there is no editor pane on screen,
//! so the caret half is stood down (like every editor-only Edit command) while the
//! pointer half stays available: right-clicking a rendered link there is a reading
//! gesture, not an editing one.
//!
//! Each input resolves through the seam that already owns its question — the
//! source scan [`format::link_target_at`] for the editor, and the preview's single
//! link hit-test (`preview::link_url_at`, the same one behind the hover cursor,
//! the hover tooltip and click activation) for the rendered page — so "what counts
//! as a link" cannot fork per feature. The activation **re-resolves** the caret
//! rather than reusing whatever the gate last saw (Document-Reference CAM: a
//! target captured at gate time is a bet on the buffer not having moved, and the
//! buffer moves on every keystroke); the pointer target is captured deliberately,
//! because the thing it names — which link was clicked — cannot be re-derived once
//! the pointer has moved on.

use super::*;

/// Register `win.copy-link-location` on `window`, disabled until
/// [`update_copy_link_action_state`] says otherwise.
pub(super) fn register_copy_link_action(window: &ApplicationWindow) {
    let action = SimpleAction::new("copy-link-location", None);
    action.set_enabled(false);
    action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            let Some(url) = copy_target(&w) else { return };
            w.clipboard().set_text(&url);
        }
    ));
    window.add_action(&action);
}

/// The URL this command would copy right now, or `None` when there is nothing to
/// copy.
///
/// The right-clicked link wins over the caret's: when both exist, the reader
/// pointed at one of them, and that is the more specific answer. The caret half is
/// **re-resolved here**, never carried from the gate (see the module doc).
///
/// Its own function rather than a closure body so the precedence is testable
/// without a clipboard round-trip — which headlessly is an async read whose result
/// would say nothing about which of the two inputs was chosen.
fn copy_target(window: &ApplicationWindow) -> Option<String> {
    context_link(window).or_else(|| state(window).and_then(|st| caret_link_target(&st.editor_buf)))
}

/// The link the open context menu was invoked on, if any.
fn context_link(window: &ApplicationWindow) -> Option<String> {
    winstate::chrome(window)?.ctx_link.borrow().clone()
}

/// Record the link `view`'s right-click at `(x, y)` (WIDGET coords) landed on — or
/// clear it with `None` — and re-gate the action, so the context menu built next is
/// already showing the right state.
///
/// Called from both panes' context-menu gesture: at press (to arm) and from the
/// popover's `closed` (to disarm). Disarming matters as much as arming — the
/// pointer target belongs to one popover, and left standing it would keep the
/// menu-bar and toolbar surfaces enabled for a link the reader is no longer
/// pointing at (the stale-held-reference shape the Document-Reference CAM is
/// about, on the shortest possible timescale).
pub(super) fn set_context_link(window: &ApplicationWindow, url: Option<String>) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    *chrome.ctx_link.borrow_mut() = url;
    update_copy_link_action_state(window);
}

/// The link under `(x, y)` (WIDGET coords) in `view`, by whichever route that view
/// stores links:
///
/// * a **preview** pane — the rendered link spans, through the preview's one hit-test;
/// * a **table cell**, in either of the two widget shapes a cell link renders in —
///   neither holds a buffer span at all (cell text lives in widgets,
///   ScrAP-36/ScrAP-250) — read off the picked widget, so a link in a table behaves
///   like a link in the body (Document Rendering CAM row 11, interaction parity in
///   container contexts);
/// * an **editor** pane — the Markdown source under the pointer, through the same
///   scanner the caret gate uses.
///
/// `iter_at_location` is the correct read for the editor case for the reason it is
/// correct in `preview::link_url_at`: this is an over-a-glyph hit-test, so a `None`
/// (the margin, or past the end of a short line) is a truthful "not over a link"
/// rather than the failure it would be when locating a line (GTK4Rs/AP-15).
pub(super) fn link_at_pointer(view: &gtk::TextView, x: f64, y: f64) -> Option<String> {
    if let Some(pv) = view.downcast_ref::<CodePreviewView>() {
        return crate::preview::link_url_at(pv, x, y).or_else(|| link_in_widget_at(view, x, y));
    }
    let buf = view.buffer();
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let hit = view.iter_at_location(bx, by)?;
    link_target_in_line(&buf, &hit)
}

/// The URL of the link in the **widget** under `(x, y)` — a table cell, whose link
/// holds the URL that no buffer span does.
///
/// Both cell shapes answer here, because a reader cannot tell them apart: a cell that
/// is *nothing but* a link is a `GtkLinkButton` carrying the URL as a property, and a
/// cell holding a link *plus* other content is a `GtkLabel` whose markup carries a
/// Pango `<a href>` (`widgets::table::linkcell`). Missing either one produces the same
/// bug — a right-click on a visible, working link whose Copy Link Location row is
/// greyed out, for no reason the reader can see (GTK4Rs/AP-239).
///
/// `GtkLabel::current_uri` is the label's answer, and it is trustworthy **at the
/// moment this runs and not much longer**: it reports `select_info->active_link`
/// (`gtklabel.c:4608`), which the label updates from the pointer position on motion
/// *and* on press (`gtk_label_update_active_link`, called first thing in
/// `gtk_label_click_gesture_pressed`, `:4311`). The right-click that opens the context
/// menu is such a press, and this runs from that press's capture-phase handler, so the
/// active link is the one under `(x, y)` by construction. Do not cache the result or
/// read it later: on the next motion outside the link it becomes `None`.
fn link_in_widget_at(view: &gtk::TextView, x: f64, y: f64) -> Option<String> {
    let mut w = view.pick(x, y, gtk::PickFlags::DEFAULT);
    while let Some(node) = w {
        if let Some(btn) = node.downcast_ref::<gtk::LinkButton>() {
            return Some(btn.uri().to_string());
        }
        if let Some(label) = node.downcast_ref::<gtk::Label>() {
            if let Some(uri) = label.current_uri() {
                return Some(uri.to_string());
            }
        }
        w = node.parent();
    }
    None
}

/// Set `win.copy-link-location`'s enabled state — the single source of truth for
/// the Edit menu item, the toolbar button, and both panes' context-menu rows,
/// which all bind the one action by name (POLICY single-source-of-truth;
/// ScrAP-9).
///
/// Enabled iff a right-click armed a link (any mode — see below), OR the editor is
/// visible and the caret is inside a Markdown link. Called from every boundary that
/// can change either input:
///
/// * `apply_mode_action_state` — view-mode switch and tab switch;
/// * the editor buffer's `mark-set` — every caret move (typing, arrows, clicks);
/// * the editor buffer's `changed` — an edit that alters the link without moving
///   the caret (an undo, a live external reload), which `mark-set` alone would
///   miss. Recomputing from a delta signal only is GTK4Rs/AP-47's shape;
/// * [`set_context_link`] — a context menu opening on a link, and closing again.
pub(crate) fn update_copy_link_action_state(window: &ApplicationWindow) {
    // The pointer half is NOT mode-gated: a right-clicked link is a reading
    // gesture, and the preview it comes from is read-only in every mode. Only the
    // caret half needs an editor on screen to mean anything.
    let pointed_at_link = context_link(window).is_some();
    let caret_in_link = state(window)
        .and_then(|st| caret_link_target(&st.editor_buf))
        .is_some();
    set_action_enabled(
        window,
        "copy-link-location",
        pointed_at_link || edit_actions_enabled(current_mode(window), caret_in_link),
    );
}

/// The destination of the Markdown link under `buf`'s caret, or `None`.
fn caret_link_target(buf: &sourceview::Buffer) -> Option<String> {
    let caret = buf.iter_at_offset(buf.property::<i32>("cursor-position"));
    link_target_in_line(buf, &caret)
}

/// The destination of the Markdown link containing `pos`, or `None` — shared by the
/// caret gate and the editor's right-click, which ask the same question about
/// different positions.
///
/// Reads only `pos`'s **own line**, not the document: the caret path runs on every
/// caret move and every edit, and a whole-buffer extraction would make each
/// keystroke O(document). `TextIter::line_offset` is a CHAR offset and
/// [`crate::saferizer::BufferText`] keeps char offsets aligned with the buffer's
/// own (ScrAP-74), so the offset handed to the pure scanner addresses the
/// character the buffer thinks it does.
fn link_target_in_line(buf: &impl IsA<gtk::TextBuffer>, pos: &gtk::TextIter) -> Option<String> {
    let mut start = *pos;
    start.set_line_offset(0);
    let mut end = *pos;
    // Already at the paragraph delimiter → do not advance, or `forward_to_line_end`
    // walks on to the NEXT line's end and the "line" would span two.
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    let line = crate::saferizer::BufferText::of_range(buf, &start, &end);
    format::link_target_at(line.as_str(), pos.line_offset() as usize).map(str::to_owned)
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn enabled(window: &ApplicationWindow) -> bool {
        simple_action(window, "copy-link-location")
            .expect("win.copy-link-location registered")
            .is_enabled()
    }

    /// Put the caret at char offset `off` and let the mark-set wiring recompute.
    fn place_caret(st: &crate::winstate::TabState, off: i32) {
        let iter = st.editor_buf.iter_at_offset(off);
        st.editor_buf.place_cursor(&iter);
    }

    /// The live wiring, which the pure `link_target_at` tests cannot see: the
    /// action tracks the caret through the buffer's own signals, and follows the
    /// view mode. Mutation-checked (POLICY § Typed GTK seams): dropping the
    /// `update_copy_link_action_state` call from the `mark-set` handler leaves
    /// the action disabled after the caret moves into the link, failing the first
    /// assertion; dropping the `current_mode` half leaves it enabled in
    /// preview-only, failing the last.
    #[gtktest::test]
    fn copy_link_location_follows_the_caret_and_the_view_mode() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.copylink"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        //                     0123456789...
        let doc = "plain line\nsee [a](https://example.com/x) here\n";
        let window = crate::window::new_window(&app, "IT", doc, None);
        change_action_state(&window, "view-mode", &"edit".to_variant());
        let st = state(&window).expect("state registered after new_window");

        // Caret in ordinary prose on line 1 → nothing to copy.
        place_caret(&st, 3);
        assert!(!enabled(&window), "no link under the caret on line 1");

        // Caret inside the link on line 2 (offset of the `[` is 11 + 4).
        let link_open = doc.find("[a]").expect("fixture has a link") as i32;
        place_caret(&st, link_open + 2);
        assert!(
            enabled(&window),
            "caret inside the link enables the command"
        );

        // …and the action yields that URL, not the caption or the whole construct.
        assert_eq!(
            caret_link_target(&st.editor_buf).as_deref(),
            Some("https://example.com/x")
        );

        // Caret past the link's closing `)` → disabled again.
        place_caret(&st, link_open + 30);
        assert!(
            !enabled(&window),
            "caret past the link disables the command"
        );

        // Read-only preview: the editor caret is still in the link, but there is
        // no editor pane to act in, so every surface must be greyed.
        place_caret(&st, link_open + 2);
        assert!(enabled(&window), "precondition: enabled in edit mode");
        change_action_state(&window, "view-mode", &"preview".to_variant());
        assert!(!enabled(&window), "preview-only stands the command down");

        window.destroy();
    }

    /// The pointer half — the input the caret gate cannot supply, and the reason
    /// the **preview** pane's context-menu row is usable at all. The preview is
    /// read-only and has no caret, so before this the row was permanently greyed on
    /// the one surface a reader would right-click a link from.
    ///
    /// The hit-test that turns a click into a URL is `preview::link_url_at`, shared
    /// with the hover cursor and click activation and exercised by them; what is
    /// asserted here is everything downstream of it — that an armed pointer target
    /// enables the command **in preview-only mode**, that it takes precedence over
    /// the caret, and that disarming it (the popover closing) puts the command back
    /// where the caret alone says it should be.
    ///
    /// Mutation-checked for the gate: dropping the `pointed_at_link ||` term fails
    /// the first assertion.
    ///
    /// **What this test does NOT cover, stated because the obvious claim is false:**
    /// removing the disarm from the popover's `closed` handler does *not* fail the
    /// last assertion — this body calls [`set_context_link`] directly, so it pins
    /// the state machine and says nothing about the wiring that drives it. A
    /// headless test cannot reach that handler without a real popover; the disarm is
    /// verified in the live drive (`tests/MANUAL-TEST.md` 9.32) instead. Asserting
    /// the half you can reach and believing you covered the other is ScrAP-234's
    /// shape; writing down which half is which is the only mitigation available.
    #[gtktest::test]
    fn a_right_clicked_link_enables_the_command_even_with_no_editor_caret() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.copylink.pointer"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let doc = "prose with [a](https://example.com/x) in it\n";
        let window = crate::window::new_window(&app, "IT", doc, None);
        // Preview-only: no editor pane, so the caret half is stood down and the
        // pointer half is the ONLY thing that can enable the command.
        change_action_state(&window, "view-mode", &"preview".to_variant());
        let st = state(&window).expect("state registered after new_window");
        place_caret(&st, 0); // caret nowhere near the link
        assert!(!enabled(&window), "precondition: nothing to copy yet");

        set_context_link(&window, Some("https://example.com/x".to_string()));
        assert!(
            enabled(&window),
            "a right-clicked link enables the command in the read-only preview"
        );
        assert_eq!(
            copy_target(&window).as_deref(),
            Some("https://example.com/x"),
            "the pointed-at link is what gets copied"
        );

        // Precedence: with the editor caret ALSO in a link, the pointed-at one wins
        // — the reader singled it out.
        change_action_state(&window, "view-mode", &"edit".to_variant());
        let link_open = doc.find("[a]").expect("fixture has a link") as i32;
        place_caret(&st, link_open + 2);
        set_context_link(&window, Some("https://pointed.example/at".to_string()));
        assert_eq!(
            copy_target(&window).as_deref(),
            Some("https://pointed.example/at"),
            "the pointer beats the caret when both name a link"
        );

        // Disarmed with its popover: the caret's link is the answer again…
        set_context_link(&window, None);
        assert_eq!(
            copy_target(&window).as_deref(),
            Some("https://example.com/x"),
            "clearing the pointer target falls back to the caret"
        );
        // …and back in the read-only preview, with no caret link either, the
        // command must not still be enabled from a menu that has closed.
        place_caret(&st, 0);
        change_action_state(&window, "view-mode", &"preview".to_variant());
        assert!(
            !enabled(&window),
            "a closed context menu leaves no target behind"
        );

        window.destroy();
    }
}
