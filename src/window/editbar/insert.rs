//! The Tier-2 Insert commands (Link / Image / Table) and Go To Line — each runs an
//! [`input_form`](super::input_form) and splices the result via
//! [`splice_markup`](super::splice_markup).

use super::super::*;
use super::*;

/// Insert/Edit Link — `[caption](url)`. If the selection is exactly one existing link
/// the dialog edits it (all fields pre-filled); otherwise the selection seeds the
/// caption. The URL field can Browse to a local file (rooted at the document's folder).
pub(crate) fn insert_link(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let (start, end, sel) = selection_range_and_text(&st.editor_buf);
    let start_dir = st.doc_dir();
    // If the selection is exactly one existing link, EDIT it (pre-fill both fields)
    // rather than wrapping it again; otherwise the selection becomes the caption.
    let (dlg_title, text0, url0) = match format::parse_link(&sel) {
        Some((caption, url)) => ("Edit Link", caption, url),
        None => ("Insert Link", sel, String::new()),
    };
    // Focus the URL (field 1) whenever the caption is already seeded — by a
    // selection (Insert over selected text) or by the link being edited (Edit Link).
    // In both cases the caption is settled and the URL is the field the user
    // actually came here to type; making them Tab past a correct field is friction.
    // With nothing selected both fields are empty, so Text (field 0) is the natural
    // starting point and the caret stays there.
    let focus_field = if text0.is_empty() { 0 } else { 1 };
    input_form(
        window,
        dlg_title,
        &[("Text", text0), ("URL", url0)],
        Some(BrowseSpec {
            field: 1,
            start_dir,
            image_filter: false,
        }),
        "Insert",
        focus_field,
        move |w, vals| {
            let caption = vals.first().map(String::as_str).unwrap_or("");
            let url = vals.get(1).map(String::as_str).unwrap_or("");
            splice_markup(w, start, end, &format::link_markup(caption, url));
        },
    );
}

/// Insert/Edit Image — `![alt](url "title")`. If the selection is exactly one existing
/// image the dialog edits it (all fields pre-filled); otherwise the selection seeds the
/// alt text.
pub(crate) fn insert_image(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let (start, end, sel) = selection_range_and_text(&st.editor_buf);
    // Root the Browse chooser at the loaded document's directory, if any.
    let start_dir = st.doc_dir();
    // If the selection is exactly one existing image, EDIT it (pre-fill all fields).
    let (dlg_title, alt0, url0, ttl0) = match format::parse_image(&sel) {
        Some((alt, url, title)) => ("Edit Image", alt, url, title),
        None => ("Insert Image", sel, String::new(), String::new()),
    };
    // Same rule as `insert_link` — focus the field the user still has work to do in.
    // The two dialogs are siblings in the same menu with the same shape (a caption-ish
    // field seeded from the selection, then the URL), so they must agree: a focus rule
    // that fires for Link but not Image is just an inconsistency the user has to learn.
    let focus_field = if alt0.is_empty() { 0 } else { 1 };
    input_form(
        window,
        dlg_title,
        &[("Alt text", alt0), ("URL", url0), ("Title", ttl0)],
        Some(BrowseSpec {
            field: 1,
            start_dir,
            image_filter: true,
        }),
        "Insert",
        focus_field,
        move |w, vals| {
            let alt = vals.first().map(String::as_str).unwrap_or("");
            let url = vals.get(1).map(String::as_str).unwrap_or("");
            let title = vals.get(2).map(String::as_str).unwrap_or("");
            splice_markup(w, start, end, &format::image_markup(alt, url, title));
        },
    );
}

/// Insert Table — a GFM skeleton; the selection pre-fills the first header cell.
pub(crate) fn insert_table(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let buf = &st.editor_buf;
    let (start, end, sel) = selection_range_and_text(buf);
    // A table must begin its own line, so prepend a newline when the insertion point
    // is mid-line — otherwise the GFM table would not parse as a block.
    let at_line_start = buf.iter_at_offset(start as i32).starts_line();
    input_form(
        window,
        "Insert Table",
        &[
            ("Columns", "3".to_string()),
            ("Rows", "2".to_string()),
            ("First cell", sel),
        ],
        None,
        "Insert",
        0,
        move |w, vals| {
            let parse = |s: &String| s.trim().parse::<usize>().ok().filter(|n| *n > 0);
            let cols = vals.first().and_then(parse).unwrap_or(2);
            let rows = vals.get(1).and_then(parse).unwrap_or(2);
            let first = vals.get(2).map(String::as_str).unwrap_or("");
            let mut markup = format::table_markup(cols, rows, first);
            if !at_line_start {
                markup.insert(0, '\n');
            }
            splice_markup(w, start, end, &markup);
        },
    );
}

/// Go To Line #: prompt for a 1-based line number, place the caret there, and
/// scroll it into view. Editor-only — `win.go-to-line` is disabled
/// in preview mode (`apply_mode_action_state`) and off editor focus
/// (`setup_editor_focus_gate`), the same two-part gate `win.format` uses,
/// since placing a caret needs an actual editor to place it in.
pub(crate) fn go_to_line(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let buf = &st.editor_buf;
    let cursor = buf.iter_at_offset(buf.property::<i32>("cursor-position"));
    let current_line = cursor.line() + 1;
    let line_count = buf.line_count().max(1);
    input_form(
        window,
        "Go To Line",
        &[("Line #", current_line.to_string())],
        None,
        "Go",
        0,
        move |w, vals| {
            let Some(st) = state(w) else { return };
            let requested = vals
                .first()
                .and_then(|s| s.trim().parse::<i32>().ok())
                .unwrap_or(current_line);
            let target = (requested - 1).clamp(0, line_count - 1);
            let buf = &st.editor_buf;
            let iter = buf.iter_at_line(target).unwrap_or_else(|| buf.end_iter());
            buf.place_cursor(&iter);
            // A left-gravity mark, never `scroll_to_iter` (GTK4Rs/AP-22). GTK does NOT
            // "defer the scroll until line validation and retry it" — the comment
            // that used to stand here said so and it is false: on a document GTK
            // is still laying out, the request is DISCARDED (ScrAP-260), which on a
            // cold 40 000-line file left Go To Line 30 000 showing line 177. The
            // seam re-issues it once the layout can answer.
            let mark = buf.create_mark(None, &iter, true);
            crate::farscroll::scroll_to_mark_when_ready(
                st.editor.upcast_ref(),
                &mark,
                0.0,
                true,
                0.0,
                0.5,
            );
            st.editor.grab_focus();
        },
    );
}
