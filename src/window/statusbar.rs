//! The footer status bar (TDD 16.10–16.17, 25.23): its widgets, and one refresh choke
//! point per indicator. Every decision is `winstate::statusbar`'s; this module builds,
//! applies and schedules.
//!
//! **Word counting leaves the main thread.** Counting the 3 MB `large-doc.md` fixture
//! measured 22–34 ms in a release build — more than a frame. It runs on GLib's pool
//! with owned text in and counts out, at most one job application-wide (POLICY § all
//! GTK on the main thread; GTK4Rs/AP-243) with the rest queued one deep per tab, and a
//! result is applied only if the buffer generation it was computed from is still
//! current — discard, never merge (Deferred-operation CAM).

use super::*;
use crate::winstate::statusbar::{self as core, LineEndings, TextCount, TextStats};
use crate::winstate::{StatusCtx, TabId, WindowChrome};
use std::collections::VecDeque;
use std::rc::Weak;
use std::time::Duration;

/// A class on the status bar's root, so its CSS reaches nothing else (GTK4Rs/AP-77).
pub(crate) const STATUSBAR_CLASS: &str = "scrib-statusbar";
/// The class of the status bar's buttons (zoom, Cancel): flat, padded down to line up
/// with the label indicators.
pub(crate) const STATUSBAR_BUTTON_CLASS: &str = "scrib-statusbar-button";
/// The zoom slot's pages.
const ZOOM_SHOWN: &str = "shown";
const ZOOM_HIDDEN: &str = "hidden";

/// Recount delay after an edit — the live preview's own debounce, so a burst of typing
/// costs one count.
const TEXT_STATS_DEBOUNCE: Duration = Duration::from_millis(300);
/// A selection at most this long is counted on the main thread, where it costs well
/// under a millisecond; a longer one goes to the pool like a document.
const INLINE_SELECTION_BYTES: usize = 16 * 1024;
/// Horizontal and vertical margins around each indicator separator (ratified look).
const SEPARATOR_MARGIN_X: i32 = 6;
const SEPARATOR_MARGIN_Y: i32 = 3;
/// Width of the export progress bar.
const PROGRESS_WIDTH: i32 = 120;
/// The zoom indicator's hover hint; clicking it is `win.zoom-reset`.
const RESET_ZOOM_TOOLTIP: &str = "Reset zoom to 100%";

/// The status bar's widgets. Cheap to clone: every field is a reference-counted widget.
#[derive(Clone)]
pub(crate) struct StatusBar {
    /// The whole strip — what View ▸ Status Bar hides (TDD 9.17).
    pub(crate) root: gtk::Box,
    /// The message area's text, driven by `WindowChrome::status` (the one live status
    /// region, TDD 16.5).
    pub(crate) message: gtk::Label,
    /// Export progress, beside the message and hidden until an export is slow.
    pub(crate) progress: gtk::ProgressBar,
    /// The indicator group, pinned to the right edge.
    pub(crate) indicators: gtk::Box,
    pub(crate) words: gtk::Label,
    pub(crate) zoom: gtk::Button,
    /// A stack rather than a box that is hidden: its height is the button's whether or
    /// not the zoom level shows, so the status bar — and everything above it — never
    /// changes height on a mode switch or when export progress appears.
    pub(crate) zoom_slot: gtk::Stack,
    pub(crate) position: gtk::Label,
    pub(crate) position_slot: gtk::Box,
    pub(crate) line_endings: gtk::Label,
    pub(crate) endings_slot: gtk::Box,
}

/// Build the status bar: messages at the left, indicators grouped at the right in the
/// order words, zoom, line/column, line endings (TDD 16.10).
pub(super) fn build() -> StatusBar {
    let message = gtk::Label::new(None);
    message.set_xalign(0.0);
    message.set_hexpand(true);
    // A long message shortens rather than displacing an indicator: an ellipsizing
    // label's minimum width is a few characters, so the indicators keep theirs.
    message.set_ellipsize(gtk::pango::EllipsizeMode::End);
    message.set_accessible_role(gtk::AccessibleRole::Status);

    let progress = gtk::ProgressBar::new();
    progress.set_valign(gtk::Align::Center);
    progress.set_size_request(PROGRESS_WIDTH, -1);
    progress.set_margin_start(SEPARATOR_MARGIN_X);
    progress.set_visible(false);
    crate::a11y::name(&progress, "Export progress");

    // NO CANCEL BUTTON HERE, and no cancel command behind it — both were built, tested
    // and then withdrawn by operator ruling. Not because the cancel mechanism was wrong:
    // it was measured sound, and an export driven to cancel stopped cleanly and left the
    // destination file byte-identical. The button was withdrawn because it DID NOT READ AS
    // A BUTTON — flat, borderless, 17px tall in a status bar — and could not be clicked by
    // hand. A control the user does not recognise as clickable is not an affordance,
    // however correct its wiring.
    //
    // ⚠ The lesson generalises to anything else placed in this strip: a flat control here
    // is indistinguishable from the labels beside it. Restoring a cancel affordance is a
    // UI question first — what does the user click, and how do they know they can? — not
    // a matter of re-adding a widget beside the progress bar.
    let message_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    message_box.set_hexpand(true);
    message_box.append(&message);
    message_box.append(&progress);

    let words = indicator_label();
    let zoom = gtk::Button::with_label("100%");
    zoom.add_css_class("flat");
    zoom.add_css_class("dim-label");
    zoom.add_css_class(STATUSBAR_BUTTON_CLASS);
    zoom.set_focus_on_click(false);
    // The command itself, not a copy of it (POLICY § single GAction).
    zoom.set_action_name(Some("win.zoom-reset"));
    let position = indicator_label();
    let line_endings = indicator_label();

    let zoom_slot = gtk::Stack::new();
    zoom_slot.set_vhomogeneous(true);
    zoom_slot.set_hhomogeneous(false);
    zoom_slot.set_interpolate_size(false);
    zoom_slot.add_named(&slot(&zoom), Some(ZOOM_SHOWN));
    zoom_slot.add_named(
        &gtk::Box::new(gtk::Orientation::Horizontal, 0),
        Some(ZOOM_HIDDEN),
    );
    zoom_slot.set_visible_child_name(ZOOM_HIDDEN);
    let position_slot = slot(&position);
    let endings_slot = slot(&line_endings);
    position_slot.set_visible(false);
    endings_slot.set_visible(false);

    let indicators = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    indicators.set_halign(gtk::Align::End);
    indicators.append(&words);
    indicators.append(&zoom_slot);
    indicators.append(&position_slot);
    indicators.append(&endings_slot);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.add_css_class(STATUSBAR_CLASS);
    root.set_margin_start(6);
    root.set_margin_end(6);
    root.set_margin_top(2);
    root.set_margin_bottom(2);
    root.append(&message_box);
    root.append(&indicators);

    StatusBar {
        root,
        message,
        progress,
        indicators,
        words,
        zoom,
        zoom_slot,
        position,
        position_slot,
        line_endings,
        endings_slot,
    }
}

fn indicator_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class("dim-label");
    label
}

/// An indicator preceded by its separator, so hiding the indicator hides both. The
/// word count leads the group and is always shown, so it needs no slot.
fn slot(indicator: &impl IsA<gtk::Widget>) -> gtk::Box {
    let separator = gtk::Separator::new(gtk::Orientation::Vertical);
    separator.set_margin_start(SEPARATOR_MARGIN_X);
    separator.set_margin_end(SEPARATOR_MARGIN_X);
    separator.set_margin_top(SEPARATOR_MARGIN_Y);
    separator.set_margin_bottom(SEPARATOR_MARGIN_Y);
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.append(&separator);
    slot.append(indicator);
    slot
}

/// Keep the indicator group on the visible monitor when the window is forced wider
/// than the screen (TDD 9.21's overflow clause) — applied to the GROUP, so every
/// indicator moves together.
fn fit_indicators(bar: &StatusBar) {
    super::chrome_fit::apply_visible_area_inset(&bar.indicators, 0);
}

/// Clicking the document's path copies it, whole and absolute — never the `~` or
/// shortened form the bar shows (TDD 16.19). Only while the label reads the base
/// message: over a hovered link or a passing notice, the click is not on the path.
pub(super) fn wire_path_copy(window: &ApplicationWindow) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
    // A whole click on the label, never the release that ends a drag begun elsewhere
    // (GTK4Rs/AP-169): the label is one target, so the hit is always it.
    crate::saferizer::ClickActivation::new()
        .max_travel(crate::saferizer::click_activation::drag_threshold())
        .wire(
            &click,
            |_, _| Some(()),
            glib::clone!(
                #[weak]
                window,
                move |_: &gtk::GestureClick, (), _, _| {
                    copy_path_if_shown(&window);
                }
            ),
        );
    chrome.statusbar.message.add_controller(click);
}

/// The click's work: copy the active document's path if the bar is showing it.
/// Returns whether it copied.
fn copy_path_if_shown(window: &ApplicationWindow) -> bool {
    let Some(st) = state(window) else {
        return false;
    };
    if !st.chrome().status.borrow().shows_path() {
        return false;
    }
    let Some(path) = st.path.borrow().clone() else {
        return false;
    };
    crate::clipboard::set_text(&window.clipboard(), &path.to_string_lossy());
    // A copy changes nothing on screen, so it is acknowledged (TDD 16.15).
    st.chrome()
        .push_timed_notice(core::PATH_COPIED, super::toast::INFO_STATUS_TIME);
    true
}

/// Every indicator, for the active tab: the one call a mode switch or a tab switch
/// makes (from `apply_mode_action_state`).
pub(crate) fn refresh_status_indicators(window: &ApplicationWindow) {
    refresh_position_indicator(window);
    refresh_zoom_indicator(window);
    refresh_text_indicators(window);
    note_selection_changed(window);
}

/// Line/column choke point (TDD 9.21): the active tab's own editor caret, hidden when
/// no editor pane is shown. Called on every caret move and from
/// [`refresh_status_indicators`].
pub(crate) fn refresh_position_indicator(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    let bar = &chrome.statusbar;
    let cursor = st
        .editor_buf
        .iter_at_offset(st.editor_buf.property::<i32>("cursor-position"));
    let line = cursor.line() + 1;
    let col = st.editor.visual_column(&cursor) + 1;
    match crate::winstate::line_col_indicator(st.view_mode.get(), line, col) {
        Some(text) => {
            bar.position.set_text(&text);
            crate::a11y::name_indicator(&bar.position, &core::position_accessible_name(line, col));
            bar.position_slot.set_visible(true);
        }
        None => bar.position_slot.set_visible(false),
    }
    fit_indicators(bar);
}

/// Zoom choke point (TDD 16.12): shown exactly when a preview is — the fact
/// `update_zoom_action_state` gates the zoom actions on. Called from
/// [`refresh_status_indicators`] and after every zoom change.
pub(crate) fn refresh_zoom_indicator(window: &ApplicationWindow) {
    let Some(chrome) = crate::winstate::chrome(window) else {
        return;
    };
    let bar = &chrome.statusbar;
    match core::zoom_text(current_mode(window), chrome.zoom_level.get()) {
        Some(text) => {
            bar.zoom.set_label(&text);
            crate::a11y::name_with_tooltip(
                &bar.zoom,
                &core::zoom_accessible_name(&text),
                RESET_ZOOM_TOOLTIP,
            );
            bar.zoom_slot.set_visible_child_name(ZOOM_SHOWN);
        }
        None => bar.zoom_slot.set_visible_child_name(ZOOM_HIDDEN),
    }
    fit_indicators(bar);
}

/// Word count and line-endings choke point (TDD 16.11, 16.13): render the active
/// tab's cached counts at once, and recount if they describe an older buffer.
pub(crate) fn refresh_text_indicators(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    render_text_indicators(&st);
    let current = st
        .text_stats
        .get()
        .is_some_and(|stats| stats.generation == st.text_generation.get());
    if !current {
        submit(CountJob {
            tab: st.id,
            generation: st.text_generation.get(),
            source: Some(st.editor_text()),
        });
    }
}

/// `tab`'s buffer changed: its counts no longer describe it. The tab on screen recounts
/// after the debounce; a background tab recounts when it is next activated.
pub(crate) fn note_buffer_changed(window: &ApplicationWindow, tab: &Rc<TabState>) {
    tab.text_generation
        .set(tab.text_generation.get().wrapping_add(1));
    if !state(window).is_some_and(|active| active.id == tab.id) {
        return;
    }
    let chrome = tab.chrome();
    restart_text_stats_timer(&chrome, TEXT_STATS_DEBOUNCE, {
        let window = window.downgrade();
        move || {
            if let Some(window) = window.upgrade() {
                refresh_text_indicators(&window);
            }
        }
    });
}

/// A selection changed in either pane (TDD 16.11).
///
/// **This attaches no GLib source, and that is the whole point.** It runs from GTK's own
/// `mark-set` emission, which on the Quartz backend can be executing inside a nested
/// `CFRunLoop` — GDK-macOS replaces GLib's poll with `nextEventMatchingMask` and drives
/// `g_main_context_prepare`/`check` from a CFRunLoop observer that falls through at any
/// nesting level above the first. A recursive `check()` warns and returns *before*
/// draining the wakeup, so the wakeup stays readable and `n_ready > 0` sticks; the loop
/// then refuses to sleep for the rest of its life. Anything that attaches a source from
/// here pokes that wakeup, so a `g_timeout_add` per caret move — which is what this
/// function used to do — turns a sleeping nested loop into a spin.
///
/// A small selection is therefore counted **synchronously**: plain CPU on this stack
/// costs no wakeup, changes no timeout, and pumps no run loop. Anything larger keeps the
/// figure it had; the `changed`-armed debounce and every tab or mode switch recount it,
/// and neither of those is on the input-method reset path this handler sits on.
pub(crate) fn note_selection_changed(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    match selected_text(window, &st) {
        None => chrome.selection_count.set(None),
        Some(selection) if selection.len() <= INLINE_SELECTION_BYTES => {
            chrome.selection_count.set(Some((st.id, selection.count())));
        }
        // Too large to count on the caret path, and deferring it would mean attaching a
        // source from exactly the place that must not. So the figure is CLEARED rather
        // than left standing: the previous selection's count is not this selection's,
        // and an indicator showing a smaller number as though it were current is worse
        // than one showing nothing. The `changed`-armed debounce and every tab or mode
        // switch recount it, so the blank is brief and self-correcting.
        Some(_) => chrome.selection_count.set(None),
    }
    render_text_indicators(&st);
}

/// (Re)start the text-statistics debounce timer. The timer clears its own cell when it
/// fires, so a later restart never removes a source that no longer exists.
///
/// **The cell used to be a parameter and the body did not honour it.** It cancelled and
/// re-armed whichever cell the caller passed, while the timer's own body cleared
/// `chrome.text_stats_timer` by name — so any second caller would have left its source
/// id behind in its own cell and cleared a timer belonging to something else. It was
/// correct only because there is exactly one caller, and a parameter that is ignored is
/// worse than no parameter: it advertises a generality the body does not have, and the
/// next caller is the one who finds out. Naming the field once, here, makes the
/// function honest about being the text-statistics timer.
fn restart_text_stats_timer(
    chrome: &Rc<WindowChrome>,
    delay: Duration,
    fire: impl FnOnce() + 'static,
) {
    if let Some(id) = chrome.text_stats_timer.borrow_mut().take() {
        id.remove();
    }
    let weak: Weak<WindowChrome> = Rc::downgrade(chrome);
    let id = glib::timeout_add_local_once(delay, move || {
        if let Some(chrome) = weak.upgrade() {
            chrome.text_stats_timer.borrow_mut().take();
        }
        fire();
    });
    *chrome.text_stats_timer.borrow_mut() = Some(id);
}

/// Text a selection holds, and whether it is Markdown source (the editor) or text as
/// rendered (the preview, a table cell).
enum SelectionText {
    Markdown(String),
    Rendered(String),
}

impl SelectionText {
    fn len(&self) -> usize {
        match self {
            Self::Markdown(text) | Self::Rendered(text) => text.len(),
        }
    }

    fn count(&self) -> TextCount {
        match self {
            Self::Markdown(text) => TextCount::of_markdown(text),
            Self::Rendered(text) => TextCount::of_text(text),
        }
    }
}

/// What the focused pane has selected, if anything.
///
/// Editor: the source slice, counted as Markdown (approximate where the slice cuts a
/// construct). Preview: the buffer's rendered text — which omits anchored children
/// (ScrAP-74) — or, failing that, a table cell's own label selection, which no buffer
/// signal reports (GTK4Rs/AP-28) and which the preview's selection driver re-schedules
/// this count for.
fn selected_text(window: &ApplicationWindow, st: &TabState) -> Option<SelectionText> {
    let view = focused_text_view(window)?;
    let is_editor = view == *st.editor.upcast_ref::<TextView>();
    let buffer = view.buffer();
    if let Some((start, end)) = buffer.selection_bounds() {
        let text = crate::saferizer::BufferText::of_range(&buffer, &start, &end).into_string();
        return Some(if is_editor {
            SelectionText::Markdown(text)
        } else {
            SelectionText::Rendered(text)
        });
    }
    if is_editor {
        return None;
    }
    let labels = crate::preview::scrib_labels(&view)?;
    let labels = labels.borrow();
    labels.iter().find_map(|label| {
        let (a, b) = label.selection_bounds()?;
        let text: String = label
            .text()
            .chars()
            .skip(a.min(b).max(0) as usize)
            .take(a.abs_diff(b) as usize)
            .collect();
        Some(SelectionText::Rendered(text))
    })
}

/// Put `tab`'s cached counts on screen. Callers pass the ACTIVE tab.
fn render_text_indicators(tab: &TabState) {
    let chrome = tab.chrome();
    let bar = &chrome.statusbar;
    let Some(stats) = tab.text_stats.get() else {
        // Not counted yet — a first count is a job away. Blank, never a guess.
        bar.words.set_text("");
        bar.endings_slot.set_visible(false);
        return;
    };
    let selection = chrome
        .selection_count
        .get()
        .and_then(|(owner, count)| (owner == tab.id).then_some(count));
    let text = core::word_count_text(stats.count, selection);
    bar.words.set_text(&text.label);
    crate::a11y::name_with_tooltip(&bar.words, &text.accessible_name, &text.tooltip);
    bar.line_endings.set_text(stats.endings.label());
    crate::a11y::name_indicator(&bar.line_endings, &stats.endings.accessible_name());
    bar.endings_slot.set_visible(true);
    fit_indicators(bar);
}

fn render_if_active(tab: &Rc<TabState>) {
    let Some(window) = window_of_content_box(&tab.content_box) else {
        return;
    };
    if state(&window).is_some_and(|active| active.id == tab.id) {
        render_text_indicators(tab);
    }
}

/// One counting request. Owned data only, so its payload can cross to the pool.
struct CountJob {
    tab: TabId,
    /// The buffer generation `source` was read at.
    generation: u64,
    /// The whole document, when its counts are stale.
    source: Option<String>,
}

impl CountJob {
    /// Fold a newer request for the SAME tab into this pending one: each half is kept
    /// from whichever request has it, so a selection recount cannot discard a pending
    /// document recount.
    fn absorb(&mut self, newer: CountJob) {
        debug_assert_eq!(self.tab, newer.tab, "only a same-tab request is folded in");
        if newer.source.is_some() {
            self.source = newer.source;
            self.generation = newer.generation;
        }
    }
}

#[derive(Default)]
struct Counter {
    running: bool,
    /// Requests waiting for the pool, at most **one per tab** — a newer request for a
    /// tab already queued is folded into that entry rather than appended.
    ///
    /// **Never one slot.** A single slot has to choose between two tabs' requests, and
    /// whichever it discards is never re-submitted: nothing re-asks for a count except
    /// a mode or tab switch in the window that owns it. That is invisible while the
    /// loser is a background tab — it recounts when it is next activated — and a
    /// permanently blank word-count and line-endings indicator when it is the *active*
    /// tab of another window, which is what three windows opened back to back produce
    /// (TDD 16.11, 16.13).
    pending: VecDeque<CountJob>,
}

/// This gate's share of GLib's I/O pool, from the one place the shares are added up.
///
/// It is a `bool` below rather than a counter because the share is one; the assertion
/// is what keeps those two facts the same fact. Raising the budget without reshaping
/// the gate would silently leave this at one thread while the pool accounting believed
/// otherwise — the sum would still be honoured, so nothing else would complain.
const MAX_CONCURRENT_COUNTS: usize =
    crate::docio::budget::cap(crate::docio::budget::Consumer::WordCount);
const _: () = assert!(
    MAX_CONCURRENT_COUNTS == 1,
    "the single-slot latch below IS this cap; give the budget more and this gate has \
     to become a counter, like docio's and animation's, rather than a bool"
);

thread_local! {
    /// The application-wide bound: one count on the pool at a time (GTK4Rs/AP-243), the
    /// rest queued one deep per tab. The number is `docio::budget`'s.
    static COUNTER: RefCell<Counter> = RefCell::default();
}

fn submit(job: CountJob) {
    let start = COUNTER.with(|counter| {
        let mut counter = counter.borrow_mut();
        if counter.running {
            match counter
                .pending
                .iter_mut()
                .find(|queued| queued.tab == job.tab)
            {
                Some(queued) => queued.absorb(job),
                None => counter.pending.push_back(job),
            }
            None
        } else {
            counter.running = true;
            Some(job)
        }
    });
    if let Some(job) = start {
        run(job);
    }
}

/// Releases the application-wide "a count is running" latch when the future that
/// claimed it goes away — completed, cancelled, or dropped with its main context.
///
/// Without this the latch is set by [`submit`] and cleared only on the completion
/// path, so a future dropped before it finishes strands `running` at `true` and every
/// later count parks in `pending` forever: the indicator silently stops updating for
/// the rest of the process. That is unreachable in a running application, which never
/// drops that future, and reachable in a test binary, which builds and tears down
/// hundreds of windows in one process — the shape POLICY § Unit tests forbids, where
/// process-global state outlives the test that installed it.
struct RunningLatch;

impl Drop for RunningLatch {
    fn drop(&mut self) {
        let next = COUNTER.with(|counter| {
            let mut counter = counter.borrow_mut();
            let next = counter.pending.pop_front();
            counter.running = next.is_some();
            next
        });
        if let Some(next) = next {
            run(next);
        }
    }
}

fn run(job: CountJob) {
    let CountJob {
        tab,
        generation,
        source,
    } = job;
    glib::MainContext::default().spawn_local(async move {
        // Held for the future's whole life, so the latch is released on EVERY exit —
        // completion, panic, or the future being dropped with its main context — and
        // the next pending job is started from one place rather than from the happy
        // path only.
        let _latch = RunningLatch;
        #[expect(clippy::disallowed_methods, reason = "sanctioned dispatcher for docio::budget::Consumer::WordCount; the admission gate is the single-slot Counter above")]
        let outcome = gtk::gio::spawn_blocking(move || {
            source.map(|text| (TextCount::of_markdown(&text), LineEndings::classify(&text)))
        })
        .await;
        match outcome {
            Ok(total) => apply(tab, generation, total),
            Err(_) => log::error!(
                "status bar: counting tab {tab}'s words panicked; the indicator keeps its \
                 last value"
            ),
        }
    });
}

fn apply(tab_id: TabId, generation: u64, total: Option<(TextCount, LineEndings)>) {
    let Some(tab) = crate::winstate::tab_by_id(tab_id) else {
        return;
    };
    if let Some((count, endings)) = total {
        // An edit landed while this was counting, so it describes an older buffer:
        // discard it. That edit's own debounce is already on its way.
        if tab.text_generation.get() == generation {
            tab.text_stats.set(Some(TextStats {
                generation,
                count,
                endings,
            }));
        }
    }
    render_if_active(&tab);
}

/// The link target the message area is showing, if any.
struct HoverTarget {
    view: glib::WeakRef<gtk::Widget>,
    /// The stack the notice was pushed on — retracted there, never re-resolved
    /// (Status-notice CAM column C).
    chrome: Weak<WindowChrome>,
    ctx: StatusCtx,
    url: String,
}

thread_local! {
    /// One pointer, so at most one hovered link application-wide.
    static HOVER: RefCell<Option<HoverTarget>> = const { RefCell::new(None) };
}

/// Show `url` in the message area while the pointer rests on a link in `view`, or take
/// it down with `None` (TDD 16.14).
///
/// A condition-popped notice: it holds exactly as long as the pointer does, and moving
/// from one link to another replaces it rather than stacking a second.
pub(crate) fn set_hover_target(view: &impl IsA<gtk::Widget>, url: Option<&str>) {
    let view = view.as_ref();
    let unchanged = HOVER.with(|hover| {
        hover.borrow().as_ref().is_some_and(|target| {
            target.view.upgrade().as_ref() == Some(view) && Some(target.url.as_str()) == url
        })
    });
    if unchanged {
        return;
    }
    retract_hover_target();
    let Some(url) = url else { return };
    let Some(chrome) = crate::window::host_window(view).and_then(|w| crate::winstate::chrome(&w))
    else {
        return;
    };
    let ctx = chrome.status.borrow_mut().push(url);
    HOVER.with(|hover| {
        *hover.borrow_mut() = Some(HoverTarget {
            view: view.downgrade(),
            chrome: Rc::downgrade(&chrome),
            ctx,
            url: url.to_string(),
        });
    });
}

/// Take down `view`'s hover target if it is the one showing — when the pointer leaves
/// the view, and when the view is unrealized (a tab moved or closed under the pointer).
pub(crate) fn clear_hover_target(view: &impl IsA<gtk::Widget>) {
    let view = view.as_ref();
    let mine = HOVER.with(|hover| {
        hover
            .borrow()
            .as_ref()
            .is_some_and(|target| match target.view.upgrade() {
                Some(shown) => shown == *view,
                None => true,
            })
    });
    if mine {
        retract_hover_target();
    }
}

fn retract_hover_target() {
    let Some(target) = HOVER.with(|hover| hover.borrow_mut().take()) else {
        return;
    };
    if let Some(chrome) = target.chrome.upgrade() {
        chrome.status.borrow_mut().pop(target.ctx);
    }
}

/// The progress display for one PDF export (TDD 25.23), and the registration that makes
/// the export cancellable and close-safe while it runs.
pub(crate) struct ExportProgress {
    inner: Rc<ProgressInner>,
}

struct ProgressInner {
    chrome: Weak<WindowChrome>,
    shown: Cell<Option<StatusCtx>>,
    timer: Cell<Option<glib::SourceId>>,
    done: Cell<usize>,
    total: Cell<usize>,
    finished: Cell<bool>,
}

impl ExportProgress {
    /// Register `op` as `chrome`'s running export and arm its display, which appears
    /// only if the export is still running after the busy delay — triggered by elapsed
    /// time, never by a page count. `run(Export)` iterates the main loop while it draws,
    /// so the timer fires and the display repaints between pages.
    pub(crate) fn arm(chrome: &Rc<WindowChrome>, op: &gtk::PrintOperation) -> Self {
        *chrome.export_op.borrow_mut() = Some(op.clone());
        let inner = Rc::new(ProgressInner {
            chrome: Rc::downgrade(chrome),
            shown: Cell::new(None),
            timer: Cell::new(None),
            done: Cell::new(0),
            total: Cell::new(0),
            finished: Cell::new(false),
        });
        let weak = Rc::downgrade(&inner);
        let id = glib::timeout_add_local_once(crate::winstate::BUSY_NOTICE_DELAY, move || {
            let Some(inner) = weak.upgrade() else { return };
            inner.timer.set(None);
            if inner.finished.get() {
                return;
            }
            let Some(chrome) = inner.chrome.upgrade() else {
                return;
            };
            let (done, total) = (inner.done.get(), inner.total.get());
            let ctx = chrome
                .status
                .borrow_mut()
                .push(&core::export_progress_text(done, total));
            inner.shown.set(Some(ctx));
            let bar = &chrome.statusbar;
            bar.progress
                .set_fraction(core::export_progress_fraction(done, total));
            bar.progress.set_visible(true);
        });
        inner.timer.set(Some(id));
        Self { inner }
    }

    /// `done` of `total` pages have been drawn.
    pub(crate) fn page_drawn(&self, done: usize, total: usize) {
        let inner = &self.inner;
        inner.done.set(done);
        inner.total.set(total);
        let (Some(ctx), Some(chrome)) = (inner.shown.get(), inner.chrome.upgrade()) else {
            return;
        };
        chrome
            .status
            .borrow_mut()
            .update(ctx, &core::export_progress_text(done, total));
        chrome
            .statusbar
            .progress
            .set_fraction(core::export_progress_fraction(done, total));
    }

    /// The export has returned: take the display down, release the operation, and run
    /// whatever was deferred until it stopped. Idempotent; also run on drop.
    pub(crate) fn finish(&self) {
        self.inner.finish();
    }
}

impl ProgressInner {
    fn finish(&self) {
        if self.finished.replace(true) {
            return;
        }
        if let Some(id) = self.timer.take() {
            id.remove();
        }
        let Some(chrome) = self.chrome.upgrade() else {
            return;
        };
        if let Some(ctx) = self.shown.take() {
            chrome.status.borrow_mut().pop(ctx);
        }
        chrome.statusbar.progress.set_visible(false);
        chrome.export_op.borrow_mut().take();
        let deferred: Vec<Box<dyn FnOnce()>> = chrome.after_export.borrow_mut().drain(..).collect();
        if !deferred.is_empty() {
            // Outside the export's own stack: a close must not run inside `run()`.
            glib::idle_add_local_once(move || {
                for work in deferred {
                    work();
                }
            });
        }
    }
}

impl Drop for ProgressInner {
    fn drop(&mut self) {
        self.finish();
    }
}

/// If a PDF export is running in `window`, cancel it and run `then` once it has
/// stopped, returning `true`; otherwise `false`, and the caller proceeds.
///
/// A window or tab closed while `run(Export)` is iterating the main loop would be
/// destroyed beneath the operation drawing into it (Deferred-operation CAM row 8).
pub(crate) fn defer_until_export_stops(
    window: &ApplicationWindow,
    then: impl FnOnce() + 'static,
) -> bool {
    let Some(chrome) = crate::winstate::chrome(window) else {
        return false;
    };
    let Some(op) = chrome.export_op.borrow().clone() else {
        return false;
    };
    log::info!("export: a close arrived while exporting — cancelling the export first");
    op.cancel();
    chrome.after_export.borrow_mut().push(Box::new(then));
    true
}

#[cfg(test)]
mod latch_tests {
    use super::{CountJob, Counter, RunningLatch, TabId, VecDeque, COUNTER};

    /// The strand this guard exists to make unrepresentable: a future that claimed the
    /// application-wide latch and then went away without completing must still release
    /// it, or every later count parks in `pending` forever and the indicator silently
    /// stops updating for the rest of the process.
    ///
    /// Display-free on purpose — the bug is in the latch, not in GTK, so it is provable
    /// without a window. The paired "a pending job is started on release" path needs a
    /// main context to dispatch into and is covered by the GTK bodies below.
    #[test]
    fn dropping_the_guard_releases_the_latch_even_when_the_job_never_finished() {
        COUNTER.with(|counter| {
            *counter.borrow_mut() = Counter {
                running: true,
                pending: VecDeque::new(),
            }
        });

        drop(RunningLatch);

        let running = COUNTER.with(|counter| counter.borrow().running);
        assert!(
            !running,
            "a dropped future must release the latch; leaving it set strands every \
             later count in `pending` with nothing to start it"
        );
    }

    /// TDD 16.11 / 16.13 — two tabs waiting on the pool at once must both still be
    /// waiting. A one-slot queue discarded the older of them, and nothing re-asks for a
    /// count except a mode or tab switch in the window that owns the tab, so a discarded
    /// request left that window's word-count and line-endings indicators blank for as
    /// long as it stayed on the mode it opened in.
    ///
    /// Display-free: the queue is the defect, so it is provable without a window. The
    /// paired end-to-end proof is `every_window_counts_its_own_document` below.
    #[test]
    fn a_second_tabs_request_waits_beside_the_first_rather_than_displacing_it() {
        let job = |raw: u64| CountJob {
            tab: TabId::from_raw(raw),
            generation: 0,
            source: Some(String::from("alpha beta")),
        };
        COUNTER.with(|counter| {
            *counter.borrow_mut() = Counter {
                // A count is already on the pool, so both requests below queue rather
                // than starting (and this body needs no main context to dispatch into).
                running: true,
                pending: VecDeque::new(),
            }
        });

        super::submit(job(1));
        super::submit(job(2));
        super::submit(job(1));

        let queued: Vec<u64> = COUNTER.with(|counter| {
            counter
                .borrow()
                .pending
                .iter()
                .map(|job| job.tab.raw())
                .collect()
        });
        assert_eq!(
            queued,
            vec![1, 2],
            "both tabs wait, in order, and a repeat request folds into the tab's own \
             entry rather than adding a second"
        );

        COUNTER.with(|counter| *counter.borrow_mut() = Counter::default());
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::window::new_window;
    use crate::winstate::BackingLoss;
    use gtk::gio::ApplicationFlags;

    fn make_app(name: &str) -> gtk::Application {
        let app = gtk::Application::new(Some(name), ApplicationFlags::NON_UNIQUE);
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register before building a window");
        app
    }

    /// Pump until `done` or 3 s — a count lands from GLib's pool through the main loop.
    fn pump_until(done: impl FnMut() -> bool) -> bool {
        crate::testpump::until_or_for(
            crate::testpump::Clock::Frame,
            Duration::from_millis(3_000),
            done,
        )
    }

    fn children(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        let mut child = widget.as_ref().first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            out.push(current);
        }
        out
    }

    /// TDD 16.10 — messages left, indicators right in their fixed order, and a long
    /// message shortens instead of displacing an indicator.
    /// TDD 16.18–16.19: the bar names the document by its path, and a click on the
    /// path copies it whole — only while the path, not a notice, is what shows.
    #[gtktest::test]
    fn the_path_shows_and_a_click_copies_it_whole() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.path");
        let dir = std::env::temp_dir().join(format!("scrib-status-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("doc.md");
        std::fs::write(&file, "# Doc\n").expect("write fixture");
        let window = new_window(&app, "w", "# Doc\n", Some(&file));
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        let shown = core::status_path(
            &file,
            Some(&glib::home_dir()),
            crate::config::config().window.status_path_max,
        );
        assert!(
            pump_until(|| chrome.statusbar.message.text() == shown),
            "the bar reads {:?}, not the path {shown:?}",
            chrome.statusbar.message.text()
        );

        let cursor =
            |label: &gtk::Label| label.cursor().and_then(|c| c.name()).map(|n| n.to_string());
        assert_eq!(
            cursor(&chrome.statusbar.message).as_deref(),
            Some("pointer"),
            "the hand says the path can be clicked"
        );
        assert!(copy_path_if_shown(&window), "a click on the path copies it");
        let read: Rc<RefCell<Option<String>>> = Default::default();
        window
            .clipboard()
            .read_text_async(gtk::gio::Cancellable::NONE, {
                let read = read.clone();
                move |text| *read.borrow_mut() = text.ok().flatten().map(|s| s.to_string())
            });
        assert!(
            pump_until(|| read.borrow().is_some()),
            "the clipboard read returns"
        );
        assert_eq!(
            read.borrow().as_deref(),
            Some(&*file.to_string_lossy()),
            "the copy is the whole absolute path, not the bar's rendering"
        );
        assert_eq!(chrome.statusbar.message.text(), core::PATH_COPIED);

        // Over a notice the click is not on the path.
        assert!(
            !copy_path_if_shown(&window),
            "over a notice nothing is copied"
        );
        assert_eq!(
            cursor(&chrome.statusbar.message),
            None,
            "no hand over a notice"
        );

        // A document with no file shows no path.
        let untitled = new_window(&app, "u", "# New\n", None);
        let untitled_chrome = crate::winstate::chrome(&untitled).expect("chrome registered");
        assert!(pump_until(|| untitled_chrome
            .statusbar
            .message
            .text()
            .is_empty()));
        assert!(!copy_path_if_shown(&untitled));
        assert_eq!(
            cursor(&untitled_chrome.statusbar.message),
            None,
            "no path, no hand"
        );

        window.destroy();
        untitled.destroy();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[gtktest::test]
    fn messages_sit_left_and_indicators_right_in_order() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.layout");
        let window = new_window(&app, "w", "# One two\n", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        let bar = &chrome.statusbar;

        let top = children(&bar.root);
        assert_eq!(top.len(), 2, "a message area and an indicator group");
        assert!(top[0].hexpands(), "the message area takes the free width");
        assert_eq!(&top[1], bar.indicators.upcast_ref::<gtk::Widget>());
        assert_eq!(bar.indicators.halign(), gtk::Align::End);

        let order = children(&bar.indicators);
        let expected: Vec<gtk::Widget> = vec![
            bar.words.clone().upcast(),
            bar.zoom_slot.clone().upcast(),
            bar.position_slot.clone().upcast(),
            bar.endings_slot.clone().upcast(),
        ];
        assert_eq!(order, expected, "words, zoom, line/column, line endings");

        assert_eq!(bar.message.ellipsize(), gtk::pango::EllipsizeMode::End);
        assert_eq!(bar.message.accessible_role(), gtk::AccessibleRole::Status);
        assert_ne!(
            bar.words.accessible_role(),
            gtk::AccessibleRole::Status,
            "only the message area is a live region (TDD 16.17)"
        );
        window.destroy();
    }

    /// TDD 16.11 / 16.13 — the counts follow the buffer, off the main thread.
    #[gtktest::test]
    fn word_count_and_line_endings_follow_the_buffer() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.words");
        let window = new_window(
            &app,
            "w",
            "Hello **bold** [world](https://x.example/)\r\n",
            None,
        );
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        let tab = state(&window).expect("a tab");
        refresh_status_indicators(&window);
        assert!(
            pump_until(|| chrome.statusbar.words.text() == "3 words"),
            "got {:?}",
            chrome.statusbar.words.text()
        );
        assert_eq!(chrome.statusbar.line_endings.text(), "CRLF");

        let mut end = tab.editor_buf.end_iter();
        tab.editor_buf.insert(&mut end, "two more\n");
        assert!(
            pump_until(|| chrome.statusbar.words.text() == "5 words"),
            "got {:?}",
            chrome.statusbar.words.text()
        );
        assert_eq!(chrome.statusbar.line_endings.text(), "Mixed");
        window.destroy();
    }

    /// TDD 16.11 / 16.13 — every window counts its own document, however many are
    /// opened at once. Three, because two never exposed the defect: the first count
    /// runs immediately and the second waits, so a one-slot queue only starts
    /// discarding at the third — and what it discarded was the SECOND window's, whose
    /// indicators then stayed blank until someone switched its view mode.
    #[gtktest::test]
    fn every_window_counts_its_own_document() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.everywindow");
        let windows: Vec<_> = (0..3)
            .map(|i| new_window(&app, &format!("w{i}"), "alpha beta gamma\n", None))
            .collect();
        let words = |w: &ApplicationWindow| {
            crate::winstate::chrome(w)
                .expect("chrome registered")
                .statusbar
                .words
                .text()
                .to_string()
        };
        let endings = |w: &ApplicationWindow| {
            crate::winstate::chrome(w)
                .expect("chrome registered")
                .statusbar
                .endings_slot
                .is_visible()
        };

        let counted = pump_until(|| windows.iter().all(|w| words(w) == "3 words"));
        assert!(
            counted,
            "every window counts its own document; got {:?}",
            windows.iter().map(words).collect::<Vec<_>>()
        );
        assert!(
            windows.iter().all(endings),
            "the line-endings indicator rides the same count, so it shows everywhere too"
        );

        for window in &windows {
            window.destroy();
        }
    }

    /// TDD 16.16 — a lost file is part of the persistent line until it returns.
    #[gtktest::test]
    fn a_lost_file_stays_reported_until_it_returns() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.lost");
        let window = new_window(&app, "w", "text\n", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        let tab = state(&window).expect("a tab");
        let notice = BackingLoss::Deleted.notice();

        crate::window::mark_backing_lost(&tab, BackingLoss::Deleted);
        assert!(chrome.status.borrow().label_text().contains(notice));
        // A timed notice pushed on top retracts back to the loss, not to nothing.
        let other = chrome.status.borrow_mut().push("Document copied");
        chrome.status.borrow_mut().pop(other);
        assert!(chrome.status.borrow().label_text().contains(notice));

        crate::window::clear_backing_loss(&tab);
        assert!(!chrome.status.borrow().label_text().contains(notice));
        window.destroy();
    }

    /// TDD 20.22 — the viewer heading counts the listed annotations.
    #[gtktest::test]
    fn the_annotations_heading_counts_the_listed_annotations() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.annotations");
        let annotated = new_window(&app, "a", "{==a==}{>>one<<} and {==b==}{>>two<<}\n", None);
        let plain = new_window(&app, "p", "No annotations here.\n", None);
        crate::window::refresh_annotations(&annotated);
        crate::window::refresh_annotations(&plain);
        let title = |w: &ApplicationWindow| {
            crate::winstate::chrome(w)
                .expect("chrome")
                .annotations_title
                .text()
                .to_string()
        };
        assert_eq!(title(&annotated), "Annotations (2)");
        assert_eq!(title(&plain), "Annotations");
        annotated.destroy();
        plain.destroy();
    }

    /// TDD 16.14 — one hover target at a time, retracted back to what was underneath.
    #[gtktest::test]
    fn a_hover_target_replaces_rather_than_stacks_and_clears() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.hover");
        let window = new_window(&app, "w", "text\n", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        let view = chrome.statusbar.words.clone();
        chrome.status.borrow_mut().set_base("base");

        set_hover_target(&view, Some("https://one.example/"));
        assert_eq!(chrome.status.borrow().label_text(), "https://one.example/");
        set_hover_target(&view, Some("https://two.example/"));
        assert_eq!(chrome.status.borrow().label_text(), "https://two.example/");
        set_hover_target(&view, None);
        assert_eq!(
            chrome.status.borrow().label_text(),
            "base",
            "moving between links replaced the notice; leaving retracted it"
        );

        set_hover_target(&view, Some("https://three.example/"));
        clear_hover_target(&view);
        assert_eq!(chrome.status.borrow().label_text(), "base");
        window.destroy();
    }

    /// TDD 2.9/16.14 — a link in a **table cell** shows its target too, whether it is
    /// the cell's whole content or one word among others (Document Rendering CAM row 2). The hover used to ask only the buffer's
    /// link spans, which a cell does not have, so the footer stayed blank over every
    /// table link while the cell's own tooltip still showed the URL.
    ///
    /// Driven through the view's real hover re-derivation (`refresh_hover_now`, the
    /// body of `refresh_hover_after_paint` → the cursor hook → `set_hover_target`), with no motion fed to the cell: the
    /// mixed cell must be hit-tested from its layout, because `GtkLabel::current_uri`
    /// answers `None` for a link that was only hovered. Mutation-checked: answering only
    /// from `link_at` fails both assertions; reading `current_uri` fails them too.
    ///
    /// TDD 2.9a — only the caption is the link. The link-only cell's column is widened
    /// by its header, and the empty right-hand part of that cell must answer nothing: a
    /// link-only cell was once a `GtkLinkButton`, a link across its whole slot.
    #[gtktest::test]
    fn a_table_cell_link_shows_its_target_on_hover() {
        const MD: &str = "| A header wide enough to leave room | B |\n|---|---|\n| [Pure](https://pure.example/) | see [Mixed](https://mixed.example/) here |\n";
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.cellhover");
        let window = new_window(&app, "w", MD, None);
        window.present();
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        chrome.status.borrow_mut().set_base("base");

        fn descendants(root: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
            let mut child = root.first_child();
            while let Some(c) = child {
                descendants(&c, out);
                child = c.next_sibling();
                out.push(c);
            }
        }
        let find_cells = || {
            let view = super::super::zoom::get_preview_view(&window)?;
            let mut all = Vec::new();
            descendants(view.upcast_ref(), &mut all);
            let cell = |needle: &str| {
                all.iter()
                    .filter_map(|w| w.downcast_ref::<gtk::Label>())
                    .find(|l| l.label().contains(needle))
                    .filter(|l| l.width() > 0)
                    .cloned()
            };
            Some((view, cell("pure.example")?, cell("mixed.example")?))
        };
        assert!(
            pump_until(|| find_cells().is_some()),
            "the table's cells were allocated"
        );
        let (view, pure, mixed) = find_cells().expect("cells");

        let hover = |target: &gtk::Widget, lx: f64, ly: f64, want: &str| {
            let p = target
                .compute_point(&view, &gtk::graphene::Point::new(lx as f32, ly as f32))
                .expect("the cell sits inside the view");
            view.set_pointer_position(Some((p.x(), p.y())));
            // Synchronous, and read back at once: a real pointer over the window (a CI
            // desktop has one) delivers its own motion, which would re-derive the target
            // at ITS position if the loop ran in between.
            view.refresh_hover_now();
            let shown = chrome.status.borrow().label_text();
            let (x, y) = (f64::from(p.x()), f64::from(p.y()));
            assert_eq!(
                shown,
                want,
                "[diagnostics] realized={} mapped={} picked={:?} link_url_at={:?}",
                view.is_realized(),
                view.is_mapped(),
                view.pick(x, y, gtk::PickFlags::DEFAULT)
                    .map(|w| w.type_().name()),
                crate::preview::link_url_at(&view, x, y),
            );
            shown
        };

        // Aim at the layout position of a caption's first glyph.
        let glyph = |label: &gtk::Label, word: &str| {
            let layout = label.layout();
            let idx = layout.text().find(word).expect("caption laid out") as i32;
            let rect = layout.index_to_pos(idx + 1);
            let (ox, oy) = label.layout_offsets();
            (
                f64::from(ox) + f64::from(rect.x()) / f64::from(gtk::pango::SCALE),
                f64::from(oy)
                    + f64::from(rect.y() + rect.height() / 2) / f64::from(gtk::pango::SCALE),
            )
        };
        let (px, py) = glyph(&pure, "Pure");
        let want = "https://pure.example/";
        assert_eq!(hover(pure.upcast_ref(), px, py, want), want);

        // The same link-only cell, well to the right of its caption: plain cell, no link.
        let (_, nat_w, _, _) = pure.measure(gtk::Orientation::Horizontal, -1);
        assert!(
            pure.width() > nat_w + 40,
            "fixture no longer discriminates: the cell ({}px) is not much wider than \
             its caption ({nat_w}px), so there is no empty part of it to hover",
            pure.width()
        );
        assert_eq!(
            hover(pure.upcast_ref(), f64::from(pure.width() - 10), py, "base"),
            "base"
        );

        let (lx, ly) = glyph(&mixed, "Mixed");
        let want = "https://mixed.example/";
        assert_eq!(hover(mixed.upcast_ref(), lx, ly, want), want);

        view.set_pointer_position(None);
        crate::window::clear_hover_target(&view);
        assert_eq!(chrome.status.borrow().label_text(), "base");
        window.destroy();
    }

    /// Deferred-operation CAM row 8 — while an export runs, Export is disabled and a
    /// close waits for it to stop.
    #[gtktest::test]
    fn an_export_in_progress_disables_export_and_defers_a_close() {
        let app = make_app("com.extollit.scribobulate.integrationtest.statusbar.export");
        let window = new_window(&app, "w", "# Doc\n", None);
        let chrome = crate::winstate::chrome(&window).expect("chrome registered");
        let enabled = |w: &ApplicationWindow| {
            w.lookup_action("export")
                .expect("win.export registered")
                .is_enabled()
        };

        let op = gtk::PrintOperation::new();
        let progress = ExportProgress::arm(&chrome, &op);
        super::super::export::update_export_action_state(&window);
        assert!(
            !enabled(&window),
            "a second export cannot start during the first"
        );

        let closed = Rc::new(Cell::new(false));
        let flag = Rc::clone(&closed);
        assert!(defer_until_export_stops(&window, move || flag.set(true)));
        assert!(!closed.get(), "the close waits for the export");

        progress.finish();
        assert!(
            pump_until(|| closed.get()),
            "the deferred close runs once it stops"
        );
        super::super::export::update_export_action_state(&window);
        assert!(enabled(&window));
        assert!(
            !defer_until_export_stops(&window, || {}),
            "with no export running, nothing is deferred"
        );
        window.destroy();
    }
}
