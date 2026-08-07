//! The preview-side **create** overlay GTK wiring (the preview create card) — the thin
//! GTK shell around the pure selection→source mapping in the parent module.
//!
//! Two coordinated pieces: a non-autohide **action popover** (Annotate, …) shown over a
//! live preview selection, and an in-surface **comment entry** card revealed when
//! Annotate is clicked. See [`wire_annotation_overlay`] for the full rationale (GTK4Rs/AP-83).
//!
//! This file is pure GTK signal/geometry wiring that cannot be exercised headlessly, so
//! it is EXCLUDED from the coverage ratchet (scripts/coverage.sh / POLICY.md), same as
//! `window/editbar/` and `codeview.rs`. The bug-prone arithmetic it depends on lives in
//! the parent module's pure, unit-tested `bar_placement` helper and the shared
//! `saferizer::ViewportRect` anchor gate.

use super::{bar_placement, create_from_selection};
use crate::codeview::CodePreviewView;
use crate::preview::cell_copymap;
use crate::preview::qdata::RenderData;
use crate::saferizer::{pin_above, Viewport, ViewportRect};
use crate::widgets::comment_entry::CommentEntry;
use gtk::glib;
use gtk::prelude::*;
use gtk::Label;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// Clear the selection on every table-cell `GtkLabel` that currently has one
/// (table-cell annotation). A cell label's selection is a selection island that a buffer click
/// does not clear on its own; used when the user clicks away into the body so the
/// create overlay can dismiss. A no-op when no cell is selected.
fn clear_cell_selections(view: &CodePreviewView) {
    let Some(labels) = crate::preview::scrib_labels(view) else {
        return;
    };
    for label in labels.borrow().iter() {
        if label.selection_bounds().is_some() {
            label.select_region(0, 0);
        }
    }
}

/// First table-cell label (if any) that currently has a non-empty selection.
fn selected_cell_label(view: &CodePreviewView) -> Option<(Label, i32, i32)> {
    let labels = crate::preview::scrib_labels(view)?;
    for label in labels.borrow().iter() {
        if let Some((a, b)) = label.selection_bounds() {
            if a != b {
                return Some((label.clone(), a, b));
            }
        }
    }
    None
}

/// Position the action popover / card at a cell-label selection using the label's
/// Pango layout clip region (table-cell annotations). Returns false when the
/// label has no layout or translate fails (GTK4Rs/AP-22/29 unallocated guard).
fn point_at_cell_selection(
    view: &CodePreviewView,
    label: &Label,
    a: i32,
    b: i32,
    pop: &gtk::Popover,
) -> bool {
    let layout = label.layout();
    // selection_bounds are character offsets into the label's plain text; Pango
    // index is byte offset into the layout text (UTF-8 plain).
    let plain = label.text();
    let to_byte = |ch: i32| -> i32 {
        plain
            .chars()
            .take(ch.max(0) as usize)
            .map(|c| c.len_utf8() as i32)
            .sum()
    };
    let byte_a = to_byte(a);
    let byte_b = to_byte(b);
    // Use extents of the selection range via `index_to_pos` for the midpoint
    // (table-cell annotation geometry).
    let pos_a = layout.index_to_pos(byte_a);
    let pos_b = layout.index_to_pos(byte_b.max(byte_a));
    let scale = gtk::pango::SCALE;
    let lx = (pos_a.x() + pos_b.x() + pos_b.width()) / 2 / scale;
    let ly = pos_a.y() / scale;
    let lh = (pos_a.height() / scale).max(1);
    // Label-local → view-widget coords.
    let Some((vx, vy)) = label.translate_coordinates(view, f64::from(lx), f64::from(ly)) else {
        return false; // unallocated (GTK4Rs/AP-22/29)
    };
    let Some(vr) = ViewportRect::at(Viewport::of(view), vx as i32, vy as i32, lh) else {
        return false; // off-viewport (GTK4Rs/AP-26) — do not pin to a hidden anchor
    };
    pin_above(pop, &vr);
    true
}

/// CHOKE-POINT crash guard (ScrAP-152, GTK4Rs/AP-128; sibling of
/// `codeview::markers::open_marker_popover`). `popup()` realizes the selection-action
/// popover's surface against `view`'s surface; on an UNREALIZED view that parent surface
/// is NULL → `gdk_surface_new_popup` `GDK_IS_SURFACE` assert → NULL-deref SIGSEGV
/// (gtkpopover.c / gdksurface.c). The popover is armed by a deferred ~40 ms timer that can
/// fire against an alive-but-unrooted ZOMBIE view after teardown (unrealize is synchronous,
/// finalize deferred; a co-pending strong ref keeps the weak `view.upgrade()` succeeding),
/// and the GTK4Rs/AP-26 viewport-geometry gate does NOT catch it — a zombie keeps its last
/// allocation, so the anchor still looks on-viewport. So every path that pops this popover
/// MUST route through here; a realized-but-unmapped fresh tab is still realized, so a
/// legitimate open is never wrongly skipped.
fn popup_selection_action(view: &CodePreviewView, pop: &gtk::Popover) {
    if !view.is_realized() {
        return;
    }
    pop.popup();
}

/// Position the entry card over a cell-label selection (overlay-local coords).
fn position_card_on_cell(
    view: &CodePreviewView,
    overlay: &gtk::Overlay,
    bar: &gtk::Widget,
    label: &Label,
    a: i32,
    b: i32,
) -> bool {
    let layout = label.layout();
    let plain = label.text();
    let to_byte = |ch: i32| -> i32 {
        plain
            .chars()
            .take(ch.max(0) as usize)
            .map(|c| c.len_utf8() as i32)
            .sum()
    };
    let pos_a = layout.index_to_pos(to_byte(a));
    let pos_b = layout.index_to_pos(to_byte(b.max(a)));
    let scale = gtk::pango::SCALE;
    let lx = (pos_a.x() + pos_b.x() + pos_b.width()) / 2 / scale;
    let ly = pos_a.y() / scale;
    let lh = (pos_a.height() / scale).max(1);
    let Some((vx, vy)) = label.translate_coordinates(view, f64::from(lx), f64::from(ly)) else {
        return false;
    };
    let height = view.height();
    // Same on-viewport gate the popovers use, through the same constructor — this card is
    // an in-surface GtkOverlay child rather than a popover, so it is not exposed to GTK4Rs/AP-26
    // itself, but "is there a visible anchor here?" is one question and deserves one
    // answer. The rect is discarded: `bar_placement` below owns this card's clamping.
    if ViewportRect::at(Viewport::of(view), vx as i32, vy as i32, lh).is_none() {
        return false;
    }
    let (ox, oy) = view
        .translate_coordinates(overlay, vx, vy)
        .map(|p| (p.0 as i32, p.1 as i32))
        .unwrap_or((vx as i32, vy as i32));
    bar.set_margin_start(0);
    bar.set_margin_top(0);
    let (_, nat) = bar.preferred_size();
    let (x, y) = bar_placement(
        ox,
        oy,
        lh,
        nat.width(),
        nat.height(),
        overlay.width().max(1),
        height,
    );
    bar.set_margin_start(x);
    bar.set_margin_top(y);
    true
}

/// Point `pop`'s arrow at the horizontal midpoint of the preview selection — the same
/// validation-safe geometry as the editor `point_format_overlay` (iter → rect →
/// `buffer_to_window_coords(Widget, …)`, never a position→iter hit-test, so it
/// sidesteps GTK4Rs/AP-15). Returns whether a valid on-screen anchor was set (so the caller
/// can decide to `popup` vs `popdown`).
fn point_at_selection(view: &CodePreviewView, pop: &gtk::Popover) -> bool {
    let Some((start, end)) = view.buffer().selection_bounds() else {
        return false;
    };
    let r0 = view.iter_location(&start);
    let r1 = view.iter_location(&end);
    let (x0, y0) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, r0.x(), r0.y());
    let (x1, _y1) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, r1.x(), r1.y());
    let line_h = r0.height().max(1);
    let Some(vr) = ViewportRect::at(Viewport::of(view), (x0 + x1) / 2, y0, line_h) else {
        return false; // off-viewport (GTK4Rs/AP-26) — on EITHER axis (QA round 5, H-1)
    };
    pin_above(pop, &vr);
    true
}

/// Decide whether to show the buffer-selection action popover and place it. MUST be
/// called only from a context where `GtkTextView` geometry is VALIDATED — i.e. the
/// frame clock's `after-paint` phase (see `wire_annotation_overlay`), never a
/// wall-clock timeout or a bare tick callback, because every geometry read
/// (`iter_location`/`buffer_to_window_coords`) reads a lagging `yoffset` + cached
/// `find_line_top` and ONLY a paint validates them (GTK4Rs/AP-142, researcher
/// round 2). At that point `point_at_selection`'s on-viewport read is trustworthy: a
/// genuinely-visible selection reads on-viewport → show; a scrolled-away one reads
/// off-viewport → hide (the legitimate suppression). Marker-popover open → stay down.
fn decide_show_action_popover(view: &CodePreviewView, pop: &gtk::Popover) {
    if view.annotation_sink().is_some()
        && point_at_selection(view, pop)
        && !view.marker_card_is_showing()
    {
        popup_selection_action(view, pop);
    } else {
        pop.popdown();
    }
}

/// Position the floating entry `bar` (a GtkOverlay child) at the selection of any
/// `GtkTextView` (`view`), in **overlay-local** coordinates. Shared by the preview
/// create overlay AND the editor Annotate card (`window/editor_annotate.rs`), which
/// differ only in the concrete view type. Mirrors the editor format overlay's geometry
/// (`iter_location` → `buffer_to_window_coords(Widget, …)`, never a position→iter
/// hit-test, so it sidesteps GTK4Rs/AP-15) but targets an in-surface overlay child rather than
/// a popover (GTK4Rs/AP-83). Returns `false` when the selection is scrolled off-viewport.
///
/// This thin GTK shell (selection → widget coords → overlay-local) delegates the
/// bug-prone above/below + clamp arithmetic to the pure `bar_placement` helper and the
/// shared `saferizer::ViewportRect` gate. Runs ONLY in the event phase (Annotate click / action / coalesced driver),
/// NEVER inside the overlay's `get-child-position`/allocation: it calls
/// `buffer_to_window_coords` and `preferred_size`, which force GtkTextView layout
/// validation and a child measure — doing that from allocate re-enters layout (GTK4Rs/AP-22)
/// and bubbles `alloc_needed` (GTK4Rs/AP-104). The overlay's own layout reads only the plain
/// margins set here.
pub(crate) fn position_card(
    view: &impl IsA<gtk::TextView>,
    overlay: &gtk::Overlay,
    bar: &gtk::Widget,
) -> bool {
    let view = view.as_ref();
    let Some((start, end)) = view.buffer().selection_bounds() else {
        return false;
    };
    let r0 = view.iter_location(&start);
    let r1 = view.iter_location(&end);
    let (x0, y0) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, r0.x(), r0.y());
    let (x1, _y1) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, r1.x(), r1.y());
    let height = view.height();
    let line_h = r0.height().max(1);
    // See `position_card_on_cell`: one on-viewport question, one gate. The constructed
    // rect is discarded — `bar_placement` owns this card's clamping.
    if ViewportRect::at(Viewport::of(view), (x0 + x1) / 2, y0, line_h).is_none() {
        return false;
    }
    // Anchor on the selection MIDPOINT (x0..x1) at the first-line top y0 — the SAME anchor
    // `point_at_selection` gives the action popover — so `bar_placement` (which centers on
    // it) drops the entry card exactly where the popover's body sat, not at a corner.
    let mx = (x0 + x1) / 2;
    // View-widget coords → overlay-local. The overlay wraps the ScrolledWindow whose
    // viewport fills it, so the origins nominally coincide — but translate rather than
    // assume (0,0)==(0,0) (researcher: gtkoverlaylayout copies the child rect verbatim
    // in overlay-local space; don't hard-code the identity).
    let (ox, oy) = view
        .translate_coordinates(overlay, f64::from(mx), f64::from(y0))
        .map(|p| (p.0 as i32, p.1 as i32))
        .unwrap_or((mx, y0));
    // We position the bar via its start/top margins — but GTK FOLDS a widget's own
    // margins INTO its `measure()`/`preferred_size()` result, so measuring the bar
    // while it still carries the PREVIOUS placement's margins double-counts them
    // (natural height comes back as content + old margin_top), and the next placement
    // compounds the error — the card drifts, and near the bottom the inflated height
    // pins it to the top. Zero the margins BEFORE measuring so the size reflects only
    // the card's content, then set the freshly-computed margins (AP: margin-inflated
    // re-measure). `preferred_size` is a fresh measure — fine here in the event phase,
    // never from allocate.
    bar.set_margin_start(0);
    bar.set_margin_top(0);
    let (_, nat) = bar.preferred_size();
    let (x, y) = bar_placement(
        ox,
        oy,
        line_h,
        nat.width(),
        nat.height(),
        overlay.width().max(1),
        height,
    );
    bar.set_margin_start(x);
    bar.set_margin_top(y);
    true
}

/// Wire the create overlay onto a freshly rendered preview `view` (called once per
/// render, like the copy/link wiring). Two coordinated pieces:
///
/// 1. A **selection action popover** (`pop`) — a small non-autohide GtkPopover, styled
///    and behaving like the editor format overlay, shown over a live preview selection.
///    Currently it holds one **Annotate** button, but it is built as a button box so
///    more preview-selection actions can join it later. It is deliberately
///    `autohide(false)`: an autohide popover takes an X11 seat grab whose cross-surface
///    coordinate mis-delivery prelights/steals focus onto the top chrome (GTK4Rs/AP-83,
///    researcher-confirmed against gtk-4-6) — but a *buttons-only* non-autohide popover
///    needs no grab (exactly like the format overlay), so it is immune. Positioned with
///    [`point_at_selection`]; shown/hidden by the coalesced selection driver below.
///
/// 2. A **comment entry bar** (`bar`) — an **in-surface child of the preview's
///    GtkOverlay**, NOT a popover. The typing entry is the one piece that can't safely
///    live in a popover surface: on X11 a popover-surface entry relies on
///    backend-fragile keyboard delivery, and an *autohide* one would re-introduce the
///    focus theft. An in-surface overlay child shares the toplevel surface — ordinary
///    `grab_focus()`, no second origin, no grab. Clicking **Annotate** dismisses the
///    popover and reveals this bar at the selection (positioned via [`position_card`]).
///
/// Because the entry gives up the popover grab, its dismissal is wired explicitly: a
/// focus-leave controller and an Escape key controller on the bar. While the entry is
/// open, the action popover stays down and selection updates are suspended so typing is
/// never interrupted.
pub(crate) fn wire_annotation_overlay(
    view: &CodePreviewView,
    overlay: &gtk::Overlay,
    scroller: &gtk::ScrolledWindow,
    render_data: &Rc<RefCell<RenderData>>,
) {
    // ── 1. selection action popover (non-autohide, like the format overlay) ──────
    let pop = gtk::Popover::new();
    pop.set_autohide(false);
    pop.set_has_arrow(true);
    pop.add_css_class("annotation-overlay");
    pop.set_parent(view);
    // A `set_parent`-attached popover is a native child GTK does NOT unparent for us —
    // the view's `dispose` must (GTK4Rs/AP-80), or teardown floods "GtkPopover is not a child
    // of ScribCodePreviewView".
    view.store_overlay_popover(&pop);

    // A horizontal button box so future preview-selection actions can join Annotate.
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let annotate_btn = gtk::Button::with_label("\u{1f4ac} Annotate");
    annotate_btn.set_has_frame(false);
    actions.append(&annotate_btn);
    pop.set_child(Some(&actions));

    // ── 2. comment entry bar (in-surface GtkOverlay child) ───────────────────────
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    // Use the const, not a literal — see the sibling editor card for why: this class is
    // shared with it purely for styling, and a bare literal would couple the two by
    // string coincidence and let them drift silently. It does NOT drive the
    // `win.select-all` standdown; that gates on the focused widget's TYPE
    // (`focus_in_text_entry`), covering every entry in the window rather than this card.
    bar.add_css_class(crate::window::ANNOTATION_CARD_CLASS);
    // The PREVIEW's card additionally follows the reading theme, so it reads as part
    // of the page it floats over rather than as desktop chrome. The editor's card
    // (window/editor_annotate) wears `.annotation-entry` alone and stays on the
    // desktop theme — the reading theme is preview-only (TDD 18.7).
    bar.add_css_class("scrib-preview-card");
    // START/START + margin_start/margin_top position it via `position_card`; without an
    // explicit align an overlay child FILLs the overlay.
    bar.set_halign(gtk::Align::Start);
    bar.set_valign(gtk::Align::Start);
    // Shown only once Annotate is clicked.
    bar.set_visible(false);
    overlay.add_overlay(&bar);
    // Clip the bar to the overlay bounds (so a bar near an edge clips rather than
    // enlarging the overlay) and keep it OUT of the overlay's size measurement (the bar
    // must never influence the preview's allocated size).
    overlay.set_clip_overlay(&bar, true);
    overlay.set_measure_overlay(&bar, false);

    // True while the comment entry is showing — suspends the selection-driven popover.
    // `pending` holds the buffer offsets of the selection captured when "Annotate" was
    // clicked, so the once-wired commit handler reads the right span. table-cell annotation: when the
    // selection is in a table cell, `pending_cell` carries the label + cell-local offsets
    // instead (buffer pending stays unused).
    let entry_open = Rc::new(Cell::new(false));
    let pending: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((0, 0)));
    let pending_cell: Rc<RefCell<Option<(Label, i32, i32)>>> = Rc::new(RefCell::new(None));
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    // RISK-1 (GTK4Rs/AP-128/152): cancel any pending ~40 ms selection timer when the view
    // unrealizes, so a torn-down view cannot have a deferred `pop.popup()` fire against a
    // NULL parent surface. The timer body ALSO routes its popup through the `is_realized()`
    // choke-point `popup_selection_action` (a `mark-set` during teardown could re-arm a
    // timer AFTER this fires) — the two are defense-in-depth, either alone prevents the
    // crash. Captures only the `timer` Rc, never the view, so it adds no reference cycle
    // (ScrAP-60). `take()` no-ops if the timer already fired (the slot is cleared on fire), so
    // this never double-removes a stale SourceId.
    view.connect_unrealize({
        let timer = timer.clone();
        move |_| {
            if let Some(id) = timer.borrow_mut().take() {
                id.remove();
            }
        }
    });

    // Hide the entry bar and clear the open flag.
    let hide_entry: Rc<dyn Fn()> = Rc::new({
        // WEAK, not strong. `hide_entry` is captured by closures owned by controllers
        // added to `bar` ITSELF — the `focus` `connect_leave` below and `wire_escape`'s
        // key controller — so a strong `bar` clone here closes an uncollectable cycle
        // (bar → controller → closure → hide_entry(Rc) → bar). That stranded the whole
        // comment card every render — the `GtkEntry` and its internal `GtkText`
        // gestures/controllers (`GtkGestureClick`/`GtkGestureDrag`/`GtkEventControllerKey`),
        // each holding a GHashTable — leaking RSS unbounded per live-reload
        // (ScrAP-155; the weak-capture idiom it enforces is ScrAP-60). Upgrading is
        // a no-op once the card is gone: nothing to hide.
        let bar = bar.downgrade();
        let entry_open = entry_open.clone();
        move || {
            entry_open.set(false);
            if let Some(bar) = bar.upgrade() {
                bar.set_visible(false);
            }
        }
    });

    // Commit: read the captured span + comment, create, then hide. `CommentEntry` wires
    // this to BOTH Enter and the Save button, so the two routes cannot diverge — the
    // defect recorded on the third copy of this surface.
    let card = CommentEntry::new("", {
        let view = view.downgrade();
        let render_data = render_data.clone();
        let pending = pending.clone();
        let pending_cell = pending_cell.clone();
        let hide_entry = hide_entry.clone();
        move |text: &str| {
            let Some(v) = view.upgrade() else { return };
            let create = {
                let rd = render_data.borrow();
                if let Some((label, sa, sb)) = pending_cell.borrow_mut().take() {
                    // Table-cell annotation: cell-local selection + cell copymap → same create_from_selection.
                    // apply_annotation_edit routes Create::Highlight through
                    // insert_or_extend_highlight (overlap-union for already-annotated claims).
                    if let Some(tree) = cell_copymap(&label) {
                        create_from_selection(&tree, &rd.shifts, &rd.md_owned, sa, sb, text)
                    } else {
                        None
                    }
                } else {
                    let (sa, sb) = pending.get();
                    create_from_selection(&rd.copymap, &rd.shifts, &rd.md_owned, sa, sb, text)
                }
            };
            // Dismiss the bar now (cheap; touches no document widgets), but DEFER the
            // sink — which mutates the buffer and REBUILDS the preview widget tree
            // (set_buffer + re-render) — to an idle turn. Running that rebuild
            // synchronously inside the Save button's "clicked" handler tears down and
            // re-creates widgets while the press gesture is still active, so GTK loses
            // track of the active state up the ScribCodePreviewView→…→window ancestor
            // chain ("Broken accounting of active state for widget …" — GTK4Rs/AP-30).
            // Deferring lets the gesture finish first. (Entry `activate` reaches here
            // too and is deferred identically.)
            hide_entry();
            if let Some(create) = create {
                // Collapse the preview selection so the freshly-created highlight's amber
                // wash is visible immediately. The sink edits the EDITOR buffer and
                // re-tags the preview IN PLACE (never touching the preview buffer's
                // selection), and GTK paints the opaque selection OVER tag backgrounds —
                // so a lingering selection masks the just-applied highlight, making the
                // annotation look like it didn't render.
                // A cell annotation has no buffer selection (`selection_bounds` → None),
                // so this is a no-op there.
                let buf = v.buffer();
                if let Some((_, end)) = buf.selection_bounds() {
                    buf.place_cursor(&end);
                }
                // Weak-capture the view across the idle hop (ScrAP-60 parity; the sink edits
                // the editor buffer + re-tags the preview, all surface-independent, so this
                // is not a crash class).
                glib::idle_add_local_once(glib::clone!(
                    #[weak]
                    v,
                    move || {
                        if let Some(sink) = v.annotation_sink() {
                            sink(crate::codeview::AnnotationEdit::Create(create));
                        }
                    }
                ));
            }
        }
    });
    card.style_as_card();
    let entry = card.entry.clone();
    bar.append(&card.entry);
    bar.append(&card.save);

    // Raise the comment-entry card over the CURRENT preview selection: capture the
    // selection offsets, dismiss the popover, reveal + position the entry, focus it.
    // Stored on the view as the annotate trigger AND called by the popover's Annotate
    // button, so the `win.annotate` action (keyboard/menu/toolbar) and the button share
    // one code path (SSOT). View + overlay are captured WEAK (the
    // view OWNS this closure via `set_annotate_trigger`, so a strong self/ancestor
    // capture would be an uncollectable cycle — ScrAP-60); pop/bar/entry are captured weak
    // too and upgraded inside.
    let show_entry: Rc<dyn Fn()> = Rc::new({
        let view = view.downgrade();
        let overlay = overlay.downgrade();
        let pop = pop.downgrade();
        let bar = bar.downgrade();
        let entry = entry.downgrade();
        let entry_open = entry_open.clone();
        let pending = pending.clone();
        let pending_cell = pending_cell.clone();
        let render_data = render_data.clone();
        move || {
            let (Some(v), Some(ov), Some(pop), Some(bar), Some(entry)) = (
                view.upgrade(),
                overlay.upgrade(),
                pop.upgrade(),
                bar.upgrade(),
                entry.upgrade(),
            ) else {
                return;
            };
            // Prefer buffer selection; fall back to a table-cell label selection (table-cell annotation).
            let cell = selected_cell_label(&v);
            let buffer_sel = v.buffer().selection_bounds().filter(|(a, b)| a != b);
            if buffer_sel.is_none() && cell.is_none() {
                return;
            }
            if let Some((label, a, b)) = cell {
                *pending_cell.borrow_mut() = Some((label.clone(), a, b));
                pending.set((0, 0));
                pop.popdown();
                entry.set_text("");
                entry_open.set(true);
                bar.set_visible(true);
                position_card_on_cell(&v, &ov, bar.upcast_ref(), &label, a, b);
                entry.grab_focus();
                return;
            }
            *pending_cell.borrow_mut() = None;
            let Some((a, b)) = buffer_sel else {
                return;
            };
            pending.set((a.offset(), b.offset()));
            pop.popdown();
            // Pre-populate with the comment(s) this annotation will MERGE.
            // `insert_or_extend_highlight` deliberately extends rather than nests — it
            // replaces every intersecting construct, `{>>comment<<}` included, with the
            // union carrying the new comment. The union of the SPANS is intended; the
            // silent loss of the existing COMMENTS was not. Showing them here makes the
            // merge visible and user-controlled at the moment it happens: the user sees the
            // prior text, edits or prunes it, and what they see is what is written back.
            //
            // Resolved through the SAME `selection_target` the commit uses, and the same
            // intersection predicate the mutation uses (`merged_comment_for` →
            // `intersecting_highlights`), so the card can never show one set of comments
            // while the commit destroys another.
            let existing = crate::window::host_window(&v)
                .and_then(|w| crate::window::document_source(&w))
                .and_then(|source| {
                    let rd = render_data.borrow();
                    match super::selection_target(
                        &rd.copymap,
                        &rd.shifts,
                        &rd.md_owned,
                        a.offset(),
                        b.offset(),
                    )? {
                        // A point comment inserts a NEW construct rather than replacing
                        // any, so it merges nothing and destroys nothing.
                        super::SelectionTarget::Point(_) => None,
                        super::SelectionTarget::Highlight(range) => {
                            crate::annotate::merged_comment_for(&source, range)
                        }
                    }
                })
                .unwrap_or_default();
            entry.set_text(&existing);
            entry_open.set(true);
            // Show the card BEFORE positioning it. `position_card` centers the card on the
            // selection midpoint via `bar.preferred_size()`, but `gtk_widget_measure`
            // early-returns 0 for a NOT-visible widget — so measuring while hidden yields
            // width 0, the centering `anchor - bw/2` collapses to `anchor`, and the card's
            // LEFT edge (not its centre) lands on the midpoint (it sits to the right,
            // "not aligned at all"). Measure needs visibility, not allocation, so
            // set_visible(true) first gives a real natural size to centre against (GTK4Rs/AP-85 /
            // ScrAP-68 family). The selection is intact and on-screen (the popover was
            // showing), so position_card succeeds; show regardless of its return.
            bar.set_visible(true);
            position_card(&v, &ov, bar.upcast_ref());
            entry.grab_focus();
            // Caret to the END, and AFTER `grab_focus` — order is load-bearing. A focused
            // `GtkText` selects all its text on focus-in (`gtk-entry-select-on-focus`,
            // default TRUE — gtktext.c:3310-3317), which would silently undo a caret set
            // before the grab. With the KKK pre-population that is not cosmetic: the merged
            // comment would open fully selected, so the user's first keystroke would replace
            // it and the existing comment would be destroyed after all — KKK's data loss
            // walking back in through the door the fix opened. Caret-at-end makes typing
            // APPEND: the user is amending, not starting over.
            entry.set_position(-1);
        }
    });
    view.set_annotate_trigger(show_entry.clone());
    annotate_btn.connect_clicked({
        let show_entry = show_entry.clone();
        move |_| show_entry()
    });

    // Escape on the entry bar dismisses it and returns focus to the preview (an
    // in-surface overlay child has no popover autohide, so Escape is wired explicitly).
    card.wire_escape(&bar, {
        let hide_entry = hide_entry.clone();
        let view = view.downgrade();
        move || {
            hide_entry();
            if let Some(v) = view.upgrade() {
                v.grab_focus();
            }
        }
    });

    // Focus leaving the WHOLE bar while the entry is open dismisses it (the entry gave
    // up the popover's outside-click autohide). Focus moving between the entry and Save
    // stays inside the bar subtree, so `leave` fires only for a genuine outside click /
    // focus move. On commit `hide_entry()` has already cleared `entry_open`, so the
    // ensuing leave from the hidden entry is a no-op (no double dismiss).
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave({
        let hide_entry = hide_entry.clone();
        let entry_open = entry_open.clone();
        move |_| {
            if entry_open.get() {
                hide_entry();
            }
        }
    });
    bar.add_controller(focus);

    // Coalesced show / reposition / hide of the ACTION POPOVER, mirroring the editor
    // format overlay: `mark-set` fires on every step of a drag-select, so updates are
    // debounced through one replaceable ~40 ms timer rather than popping up per event.
    // Generation token for the action popover's deferred `after-paint` anchor read
    // (GTK4Rs/AP-142). Bumped on every selection change, so a one-shot
    // `after-paint` handler still pending from an earlier selection cancels itself when
    // a newer selection supersedes its anchor before that paint lands.
    let anchor_gen = Rc::new(Cell::new(0u64));
    // The single in-flight `after-paint` handler (GTK4Rs/AP-142, researcher note 3):
    // a one-shot handler disconnects itself on its first fire, but if a NEWER selection
    // arms before the old one's paint lands, disconnect the old handler up front so a
    // never-firing handler can't dangle on the frame clock. One slot, always the latest.
    let pending_after_paint: Rc<RefCell<Option<(gtk::gdk::FrameClock, glib::SignalHandlerId)>>> =
        Rc::new(RefCell::new(None));
    let schedule = {
        let view = view.downgrade();
        let pop = pop.clone();
        let entry_open = entry_open.clone();
        let timer = timer.clone();
        let hide_entry = hide_entry.clone();
        let pending = pending.clone();
        let pending_cell = pending_cell.clone();
        let anchor_gen = anchor_gen.clone();
        let pending_after_paint = pending_after_paint.clone();
        move || {
            if let Some(id) = timer.borrow_mut().take() {
                id.remove();
            }
            // A newer selection event: invalidate any anchor poll still in flight.
            anchor_gen.set(anchor_gen.get().wrapping_add(1));
            let view = view.clone();
            // RISK-1 (GTK4Rs/AP-128/152): weak-capture `pop` too, so this deferred closure holds
            // NO strong widget across the ~40 ms wait (defense-in-depth beside the
            // `popup_selection_action` realize-gate and the unrealize SourceId cancel).
            let pop = pop.downgrade();
            let entry_open = entry_open.clone();
            let hide_entry = hide_entry.clone();
            let timer_inner = timer.clone();
            let pending = pending.clone();
            let pending_cell = pending_cell.clone();
            let anchor_gen = anchor_gen.clone();
            let pending_after_paint = pending_after_paint.clone();
            let id = glib::timeout_add_local_once(Duration::from_millis(40), move || {
                timer_inner.replace(None);
                let Some(v) = view.upgrade() else { return };
                let Some(pop) = pop.upgrade() else { return };
                let buf_sel = v.buffer().selection_bounds().filter(|(a, b)| a != b);
                let cell_sel = selected_cell_label(&v);

                // Dismiss the card only when the user has selected something
                // ELSE — and CHECK that, rather than infer it from "a selection signal
                // fired".
                //
                // This used to assume the two were the same thing ("typing in the comment
                // entry moves no buffer marks, so it never fires while the user types").
                // Typing indeed doesn't — but *selecting* does, and destructively.
                // `GtkText` writes every selection change to the display-level PRIMARY
                // clipboard, keyboard included (gtktext.c:3477, gated only on being
                // realized). When the card's entry claims PRIMARY it takes it FROM the
                // preview, and GTK responds by **clearing the preview buffer's selection
                // outright** — which fires `mark-set`. So Ctrl+A or Shift+Home inside the
                // card destroyed the card the user was typing into, discarding the comment.
                //
                // Note this is why muting our own GTK4Rs/AP-28 PRIMARY listener does NOT fix it:
                // the `mark-set` comes from GTK clearing the buffer, not from our listener.
                // Verified live (Xvfb + openbox): Shift+Home fires no accelerator at all
                // and still dismissed — which is what rules out the accelerator collision
                // as the mechanism, and what a fix aimed only at Ctrl+A would have missed.
                //
                // The rule that survives all of it: a selection that is merely GONE is not
                // the user choosing a different anchor. It is either our own PRIMARY theft
                // (above) or a click in the document — and a click moves focus out of the
                // card, which the focus-`leave` controller already dismisses on. Only a
                // different, NON-EMPTY selection supersedes the card's anchor. The commit
                // reads `pending`/`pending_cell`, never the live selection, so an anchor
                // cleared underneath it costs the card nothing.
                let anchor_cell = pending_cell
                    .borrow()
                    .as_ref()
                    .map(|(label, sa, sb)| (label.clone(), *sa, *sb));
                let superseded = match &anchor_cell {
                    // Cell-anchored: a body selection, or a selection in a different cell
                    // (or a different range in this one), supersedes it.
                    Some((label, sa, sb)) => {
                        buf_sel.is_some()
                            || cell_sel
                                .as_ref()
                                .is_some_and(|(l, a, b)| l != label || (*a, *b) != (*sa, *sb))
                    }
                    // Body-anchored: a cell selection, or a different body range.
                    None => {
                        cell_sel.is_some()
                            || buf_sel
                                .as_ref()
                                .map(|(a, b)| (a.offset(), b.offset()))
                                .is_some_and(|s| s != pending.get())
                    }
                };
                if entry_open.get() && !superseded {
                    // Noise. Return outright — falling through would also re-evaluate the
                    // action popover and pop it up ON TOP of the still-open card.
                    return;
                }
                if entry_open.get() {
                    hide_entry();
                }
                // Keep the `win.annotate` action's enabled state in step with the
                // preview selection (the action, its accelerator, and — later — its
                // menu/toolbar surfaces all gate on this). Recomputed from the window's
                // single source of truth so the popover and the action never diverge.
                if let Some(win) = crate::window::host_window(&v) {
                    crate::window::update_annotate_action_state(&win);
                }
                // Marker popover open (table-cell annotation): a table-cell selection stays
                // live when its margin marker is clicked, so the create popover must stay
                // down rather than stack over the marker popover.
                if v.marker_card_is_showing() {
                    pop.popdown();
                    return;
                }
                // Cell-label anchor: a GtkLabel's Pango layout is measured synchronously
                // (it is NOT lazily height-validated like a GtkTextView line), so its
                // geometry is trustworthy in this same turn — decide immediately.
                if let Some((label, a, b)) = cell_sel {
                    if v.annotation_sink().is_some()
                        && point_at_cell_selection(&v, &label, a, b, &pop)
                    {
                        popup_selection_action(&v, &pop);
                    } else {
                        pop.popdown();
                    }
                    return;
                }
                if buf_sel.is_none() {
                    pop.popdown();
                    return;
                }

                // Buffer selection. The DECISION is SHOW — a pointer-drag selection is
                // on-screen by construction, and a keyboard caret the view keeps visible; the
                // only legitimate HIDE is a scrolled-AWAY selection, which the scroll handler
                // below already covers. The hard part is POSITIONING without a stale read.
                //
                // Every GtkTextView geometry read (`iter_location`/`buffer_to_window_coords`,
                // and `get_visible_rect` — which returns `yoffset` verbatim) reads a LAGGING
                // `yoffset` + a cached, possibly-ESTIMATED `find_line_top`. ONLY a PAINT
                // validates them: gtk_text_view paint runs `flush_first_validate` and asserts
                // `onscreen_validated`, then draws at `find_line_top − yoffset` — the identical
                // transform the read uses (researcher round 2, gtk-4.6.9; GTK4Rs/AP-142).
                // So a read agrees with the painted position IFF it happens AFTER a paint of
                // the current scroll state. The earlier fixes read too early: a 40 ms
                // WALL-CLOCK timeout can fire before any paint, and even a bare
                // `add_tick_callback` samples at the frame clock's UPDATE phase — BEFORE paint
                // — so two ticks read the same pre-validation estimate and "converge"
                // stably-WRONG, placing a genuinely-visible selection off-viewport → the guard
                // hid it (live-confirmed on a large doc, real compositor; GTK4Rs/AP-56).
                //
                // Fix: read + decide in the ONE phase where geometry is validated — the frame
                // clock's `after-paint`, which fires AFTER `flush_first_validate`. Queue a draw
                // to guarantee a paint happens, connect a ONE-SHOT `after-paint`, decide there,
                // disconnect immediately. The generation token cancels if a newer selection
                // supersedes before the paint. Nothing strong-captures the view (GTK4Rs/AP-63).
                // Disconnect any still-pending after-paint handler from an EARLIER selection
                // before arming a new one, so a handler whose paint never lands (view
                // unmapped/occluded mid-flight) can't dangle (researcher note 3).
                if let Some((clk, id)) = pending_after_paint.borrow_mut().take() {
                    clk.disconnect(id);
                }
                let Some(clock) = v.frame_clock() else {
                    // Unmapped (no frame clock): best-effort direct decision. The realize gate
                    // in `popup_selection_action` no-ops it if the view is truly unrealized.
                    decide_show_action_popover(&v, &pop);
                    return;
                };
                v.queue_draw(); // guarantee a paint so `after-paint` fires with validated geometry
                let my_gen = anchor_gen.get();
                let anchor_gen = anchor_gen.clone();
                let v_weak = v.downgrade();
                let pop_weak = pop.downgrade();
                let pap = pending_after_paint.clone();
                let id = clock.connect_after_paint(move |clock| {
                    // One-shot: clear the shared slot + disconnect self on the first fire —
                    // the paint that just ran validated the on-screen band, so this single
                    // frame is enough (no stability race, no cap to tune). Gen-checked INSIDE
                    // the handler: a scroll/new-selection between `queue_draw` and this fire
                    // must not let a stale handler show (researcher note 1).
                    if let Some((_, id)) = pap.borrow_mut().take() {
                        clock.disconnect(id);
                    }
                    let (Some(v), Some(pop)) = (v_weak.upgrade(), pop_weak.upgrade()) else {
                        return;
                    };
                    if anchor_gen.get() != my_gen {
                        return; // a newer selection superseded this one before the paint
                    }
                    decide_show_action_popover(&v, &pop);
                });
                *pending_after_paint.borrow_mut() = Some((clock, id));
            });
            timer.replace(Some(id));
        }
    };

    // Selection changes (mark-set on drag/click, the has-selection property) drive
    // show/hide. CRUCIAL: connect on the CURRENT buffer AND re-connect on every buffer
    // swap. `re_render` replaces the preview buffer via `set_buffer` — which the
    // live-refresh after each annotation triggers — silently dropping the old buffer's
    // handlers; without re-wiring, the overlay would work for the FIRST annotation only
    // (GTK4Rs/AP-82). `notify::buffer` fires on the VIEW (which survives the swap) each time
    // the buffer is replaced, so we re-attach to the new buffer.
    let connect_selection: Rc<dyn Fn(&gtk::TextBuffer)> = Rc::new({
        let schedule = schedule.clone();
        let view = view.downgrade();
        move |buffer: &gtk::TextBuffer| {
            buffer.connect_mark_set({
                let schedule = schedule.clone();
                let view = view.clone();
                move |buf, _iter, mark| {
                    let name = mark.name();
                    if name.as_deref() == Some("insert")
                        || name.as_deref() == Some("selection_bound")
                    {
                        // A genuine buffer-cursor placement (a click in the body / empty
                        // area, collapsing to no buffer selection) means the user clicked
                        // AWAY from any table cell. A cell `GtkLabel`'s selection is sticky
                        // — a buffer click does NOT clear it (selection island) — so the
                        // create popover would otherwise linger over a cell "until you
                        // select text outside the table" (table-cell annotation). Clear lingering cell
                        // selections here so the subsequent `schedule` dismisses the
                        // popover, matching the body-selection dismissal.
                        if !buf.has_selection() {
                            if let Some(v) = view.upgrade() {
                                clear_cell_selections(&v);
                            }
                        }
                        schedule();
                    }
                }
            });
            buffer.connect_notify_local(Some("has-selection"), {
                let schedule = schedule.clone();
                move |_buf, _| schedule()
            });
        }
    });
    connect_selection(&view.buffer());
    view.connect_notify_local(Some("buffer"), {
        let connect_selection = connect_selection.clone();
        move |v: &CodePreviewView, _| connect_selection(&v.buffer())
    });

    // Table-cell selection driver (table-cell annotation). A `GtkLabel` cell is a selection island:
    // selecting inside it fires NO buffer signal (no `mark-set`/`has-selection` — the
    // text is not in the buffer), so the buffer-signal wiring above never runs for a
    // cell selection. The one cross-environment signal a selectable label exposes is the
    // DISPLAY-level primary clipboard's `changed` (GTK4Rs/AP-28 — the same signal Copy uses to
    // track cell selections). Route it through the SAME coalesced `schedule` as buffer
    // selections so a cell selection reaches full parity: it re-evaluates the action
    // popover (auto-show/-hide) AND refreshes `win.annotate`'s enabled state
    // (`schedule` calls `update_annotate_action_state`) AND dismisses the entry card
    // when the selection changes — the three things buffer selections already get. The
    // handler is stored on the view and disconnected in `dispose` (the primary clipboard
    // is long-lived; a per-render leak would otherwise accumulate — GTK4Rs/AP-28).
    //
    // NOTE: this listener over-fires badly — `::changed` means "the display's
    // PRIMARY *ownership* changed", NOT "our document's selection changed", and PRIMARY is
    // display-global (another application claiming it lands here too). It is left unguarded
    // on purpose: the dismissal decision is guarded ONCE, at the choke point inside
    // `schedule`'s timer, which is the only place that can distinguish noise from a genuine
    // re-anchor — and which also covers the `mark-set` route this guard could never reach.
    // One choke point, not two half-guards.
    {
        let clip = view.primary_clipboard();
        let schedule = schedule.clone();
        let id = clip.connect_changed(move |_| schedule());
        view.store_primary_sel_handler(&clip, id);
    }

    // Hide on scroll (unless the entry is open — the anchor scrolls away but the user
    // is mid-comment). The popover's pointing-to rect is in widget coords and goes stale
    // on scroll, so pop it down. The scroller's OWN vadjustment is the live one here —
    // the view's is not yet propagated at wire time (same rationale as
    // wire_scroll_position_tracking / ScrAP-52), so drive off the scroller's.
    {
        let vadj = scroller.vadjustment();
        vadj.connect_value_changed(glib::clone!(
            #[weak(rename_to = p)]
            pop,
            #[strong]
            entry_open,
            move |_| {
                if !entry_open.get() {
                    p.popdown();
                }
            }
        ));
    }
    // NB: the scroll handler above deliberately does NOT bump `anchor_gen`. An in-flight
    // anchor poll (GTK4Rs/AP-142) re-reads `selection_start_widget_y` every frame, so it
    // self-corrects to the current scroll position rather than needing cancellation — and
    // cancelling it on each `value-changed` while a pending post-`select`+scroll is still
    // SETTLING (vadj changes over several frames) would abort the poll before it converges,
    // leaving an on-viewport selection with no popover. Only a NEW selection (via
    // `schedule`) supersedes a poll.
}

/// Live-display tests for the comment card's dismissal contract. Need a real
/// display and a real paint; run with `cargo test --features gtk-integration-tests`, under
/// Xvfb if headless.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod jjj_tests {
    use super::*;

    const MD: &str = "Some prose to select and annotate in the preview.\n";

    fn view_of(pane: gtk::Widget) -> CodePreviewView {
        pane.downcast::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("overlay wraps the scroller")
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("scroller holds the CodePreviewView")
    }

    fn pump_until(budget: u32, done: impl Fn() -> bool) -> bool {
        let ctx = glib::MainContext::default();
        for _ in 0..budget {
            if done() {
                return true;
            }
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        done()
    }

    fn find_descendant<T: IsA<gtk::Widget>>(root: &impl IsA<gtk::Widget>) -> Option<T> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(t) = c.clone().downcast::<T>() {
                return Some(t);
            }
            if let Some(found) = find_descendant::<T>(&c) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// Raise the card over a preview selection and return `(pane, view, entry)`.
    fn open_card(win: &gtk::Window) -> (gtk::Widget, CodePreviewView, gtk::Entry) {
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        win.set_default_size(700, 400);
        win.set_child(Some(&pane));
        win.present();
        assert!(
            pump_until(200, || view.buffer().end_iter().offset() > 0),
            "precondition: the preview rendered"
        );

        // A selection is what `win.annotate` acts on.
        let buf = view.buffer();
        let (a, b) = (buf.iter_at_offset(5), buf.iter_at_offset(10));
        buf.select_range(&a, &b);
        // Let `schedule`'s ~40 ms debounce settle BEFORE raising the card. The selection
        // arms that timer, and it dismisses any open card when it fires — so raising the
        // card in the same instant would have it torn down ~40 ms later by the setup's own
        // timer, and the test would "reproduce" a bug that is purely its own haste. A real
        // user drag-selects and then clicks Annotate, always far more than 40 ms apart.
        let ctx = glib::MainContext::default();
        for _ in 0..25 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        assert!(view.trigger_annotate(), "the annotate trigger is wired");

        let entry = pump_find_entry(&pane).expect("the card's entry appears");
        (pane, view, entry)
    }

    fn pump_find_entry(pane: &gtk::Widget) -> Option<gtk::Entry> {
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            if let Some(e) = find_descendant::<gtk::Entry>(pane) {
                if WidgetExt::is_visible(&e) {
                    return Some(e);
                }
            }
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        None
    }

    /// Changing the SELECTION inside the card's own entry must not dismiss
    /// the card or discard the typed comment.
    ///
    /// This is the mechanism both reported keystrokes share, and the reason the register's
    /// accelerator hypothesis could not be the whole story. `GtkText` writes its selection
    /// to the display-level PRIMARY clipboard on *any* selection change — pointer or
    /// keyboard, gated only on the widget being realized (gtktext.c:3477, reached from
    /// `gtk_text_set_selection_bounds`; Shift+Home arrives via `move_cursor` at :3935-3938
    /// with `extend_selection`, Ctrl+A via `gtk_text_select_all` at :1401-1404). Our own
    /// ScrAP-110 cell-selection listener watches that clipboard's `::changed` and routes it to
    /// `schedule()`, which dismisses the card. The card's entry was tripping our own
    /// listener.
    ///
    /// `select_region` is the faithful trigger: it is the *same* `set_selection_bounds`
    /// path both keystrokes reach, so this reproduces the bug without depending on key
    /// delivery. (The real keystrokes were also driven under Xvfb to confirm the dispatch.)
    #[gtktest::test]
    fn selecting_inside_the_card_entry_does_not_dismiss_it() {
        let win = gtk::Window::new();
        let (_pane, _view, entry) = open_card(&win);

        entry.set_text("a comment I do not want to lose");
        // Exactly what Ctrl+A and Shift+Home both do to the entry.
        entry.select_region(0, -1);
        // The dismissal runs through `schedule`'s ~40 ms debounce, so give it every chance.
        let dismissed = pump_until(60, || !WidgetExt::is_visible(&entry));

        assert!(
            !dismissed,
            "selecting inside the card's own entry must not dismiss the card — the \
             display-level PRIMARY clipboard ::changed listener is ours, and it was \
             eating our own entry's selections"
        );
        assert_eq!(
            entry.text(),
            "a comment I do not want to lose",
            "and the typed comment must survive"
        );
        win.destroy();
    }

    /// The dismissal the card DOES owe: a selection change in the preview itself (the user
    /// went back to the document) still dismisses. The fix must not buy card stability
    /// by breaking the legitimate dismissal — nor by breaking the ScrAP-110 table-cell
    /// tracking the PRIMARY listener exists for.
    #[gtktest::test]
    fn a_preview_selection_change_still_dismisses_the_card() {
        let win = gtk::Window::new();
        let (_pane, view, entry) = open_card(&win);
        entry.set_text("draft");

        // The user re-selects in the DOCUMENT, not the card.
        let buf = view.buffer();
        let (a, b) = (buf.iter_at_offset(15), buf.iter_at_offset(25));
        buf.select_range(&a, &b);

        assert!(
            pump_until(60, || !WidgetExt::is_visible(&entry)),
            "a selection change in the preview must still dismiss the card"
        );
        win.destroy();
    }

    /// The action popover (`.annotation-overlay`) parented to `view`, if any.
    fn action_popover(view: &CodePreviewView) -> Option<gtk::Popover> {
        let mut child = view.first_child();
        while let Some(c) = child {
            if let Ok(pop) = c.clone().downcast::<gtk::Popover>() {
                if pop.has_css_class("annotation-overlay") {
                    return Some(pop);
                }
            }
            child = c.next_sibling();
        }
        None
    }

    /// GTK4Rs/AP-142: scroll the preview, THEN select on-viewport text
    /// — the Annotate action popover must appear over the selection.
    ///
    /// This is a WIRING / FUNCTIONAL guard, and honest about its limit. The reported bug
    /// is a timing race: the selection→anchor read ran inside a 40 ms WALL-CLOCK timeout
    /// that can fire before the post-scroll validation/paint frame, so
    /// `buffer_to_window_coords` returns a stale (off-viewport) widget-y and the
    /// on-viewport guard SUPPRESSES the popover (researcher-confirmed vs gtk-4.6.9:
    /// only a PAINT validates geometry — `iter_location`/`buffer_to_window_coords`/
    /// `get_visible_rect` all read a lagging yoffset + estimated heights; see the module
    /// fix comment). The fix defaults a fresh selection to SHOW and reads the anchor in the
    /// frame clock's `after-paint` phase (`queue_draw` + one-shot `connect_after_paint`),
    /// the one point where the read is validated — never a wall-clock timeout or a bare
    /// `add_tick_callback` (which samples at UPDATE, before paint).
    ///
    /// The STALE-READ regression itself cannot be caught deterministically headless: to
    /// pick a genuinely on-viewport selection this test must settle the scroll, and once
    /// the region is validated an early read also sees the correct y — the exact
    /// GTK4Rs/AP-78 masking (a prior bare-`add_tick_callback` fix passed this test yet shipped
    /// broken until the LIVE run). So this asserts only that the fix DELIVERS the popover
    /// end-to-end (it guards the after-paint wiring from a future break that stops it ever
    /// showing). The stale-timing gate is the operator's LIVE display (GTK4Rs/AP-56).
    #[gtktest::test]
    fn scroll_then_select_shows_the_annotate_popover() {
        // Tall enough to scroll well past a page.
        let mut md = String::new();
        for i in 0..400 {
            md.push_str(&format!(
                "Paragraph {i} carries some selectable prose here.\n\n"
            ));
        }
        let win = gtk::Window::new();
        let pane = crate::preview::render(&md, None, 1.0, false);
        let view = view_of(pane.clone());
        // The action popover only shows when the view has an annotation sink (normally
        // wired by the split view). A no-op sink is enough for this geometry test.
        view.set_annotation_sink(std::rc::Rc::new(|_| {}));
        win.set_default_size(600, 400);
        win.set_child(Some(&pane));
        win.present();

        let ctx = glib::MainContext::default();
        assert!(
            pump_until(200, || view.buffer().end_iter().offset() > 0),
            "precondition: the preview rendered"
        );
        for _ in 0..40 {
            ctx.iteration(false);
            std::thread::sleep(Duration::from_millis(4));
        }

        // Scroll like a mouse-wheel reader: a DISCRETE jump partway down, settle, then
        // select a passage that is genuinely ON-viewport at that position (GTK4Rs/AP-15:
        // visible_rect().y() + line_at_y, never a position hit-test). A discrete jump
        // never validates the skipped region above, so the selection's `find_line_top`
        // stays estimated until a paint validates the band — the staleness the fix's
        // after-paint read sidesteps.
        let buf = view.buffer();
        let scroller = pane
            .downcast_ref::<gtk::Overlay>()
            .unwrap()
            .child()
            .and_downcast::<gtk::ScrolledWindow>()
            .unwrap();
        let vadj = scroller.vadjustment();
        crate::saferizer::scrollpos::jump(&vadj, vadj.upper() * 0.5);
        for _ in 0..30 {
            ctx.iteration(false);
            std::thread::sleep(Duration::from_millis(4));
        }

        let vy = view.visible_rect().y() + WidgetExt::height(&view) / 3;
        let (iter, _) = view.line_at_y(vy);
        let off = iter.offset();
        let sel_a = buf.iter_at_offset(off);
        let sel_b = buf.iter_at_offset((off + 6).min(buf.end_iter().offset()));
        buf.select_range(&sel_a, &sel_b);

        let pop = action_popover(&view).expect("the action popover is wired + parented");
        assert!(
            pump_until(200, || pop.is_mapped()),
            "after scroll-then-select the Annotate action popover must appear over the \
             on-viewport selection (GTK4Rs/AP-142: the anchor read must ride the frame clock, \
             not the wall-clock debounce)"
        );

        win.destroy();
    }

    /// The `popup_selection_action` choke-point crash guard (ScrAP-152/GTK4Rs/AP-128).
    /// The selection-action popover is armed by a deferred ~40 ms timer; if it fires after
    /// the view is torn down, `popup()` realizes the popover against a NULL parent surface
    /// → `GDK_IS_SURFACE` assert → SIGSEGV — the GTK4Rs/AP-128/152 shape Section A fixed in
    /// codeview, found live in preview by the Wave-9 hardening audit. A rendered-but-never-
    /// presented view is UNREALIZED, so a direct popup through the choke-point must be a
    /// clean no-op — reaching the assertion without aborting IS the pass (mirrors codeview's
    /// `open_marker_popover_is_a_no_op_on_an_unrealized_view`). Mutation note: delete the
    /// `is_realized()` guard in `popup_selection_action` and this aborts with signal 11 —
    /// `popup()` fires against the `set_parent`'d-but-unrealized popover's NULL parent
    /// surface. The timer body's weak `pop`-capture and the unrealize SourceId cancel are
    /// the other two defense-in-depth layers; this pins the choke-point half.
    #[gtktest::test]
    fn popup_selection_action_is_a_no_op_on_an_unrealized_view() {
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane);
        assert!(
            !view.is_realized(),
            "precondition: a rendered-but-never-presented view is unrealized"
        );

        // Mirror the real wiring: a non-autohide popover parented to the view.
        let pop = gtk::Popover::new();
        pop.set_autohide(false);
        pop.set_parent(&view);

        // Route through the choke-point. The guard must return before any `popup()`.
        popup_selection_action(&view, &pop);

        assert!(
            !pop.is_mapped(),
            "the is_realized() guard must skip the popup on an unrealized view (no realize \
             against a NULL parent surface)"
        );

        // A set_parent'd popover is not auto-unparented (GTK4Rs/AP-80).
        pop.unparent();
    }
}
