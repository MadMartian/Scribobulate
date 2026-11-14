//! The floating toast furniture: the persistent conflict prompt and the shared,
//! auto-dismissing info notice ("File reloaded from disk." / "File saved.").
//!
//! Split out of `reload.rs` once the info notice grew a second caller: reload owned
//! the toasts only because it was the first thing that needed one, and leaving the
//! save path to reach across into `reload::` for its notice would have been an odd
//! dependency to read. The two kinds are deliberately different shapes — the
//! conflict toast is a *prompt* that persists until answered, the info toast is a
//! *notice* that fades (see [`winstate::InfoToast`]).

use super::*;
use crate::icons::Icon;
use std::time::Duration;

/// The toast shell's designed margin from the bottom-right corner of the content
/// overlay. Named (not a bare literal) because it is also the *base* the visible-area
/// clamp (which insets the toast further when the window overflows a small screen)
/// adds its overflow inset onto — both the shell and the clamp must agree on it, or a
/// normal-width display would shift the toast. Shared with `reload.rs`, which applies
/// the same clamp when it shows the conflict toast.
pub(super) const TOAST_MARGIN_END: i32 = 20;

/// How long an info notice stays up before auto-dismissing (TDD 5.4: "~2.5 s").
const INFO_TOAST_TIME: Duration = Duration::from_millis(2500);
/// How long the matching status-bar announcement stays up. Deliberately longer than
/// the visual toast: the toast is glanceable and its job is done once seen, while
/// the status line is what a screen reader announces, and that wants a wider window.
pub(crate) const INFO_STATUS_TIME: Duration = Duration::from_secs(4);

/// Build the floating toast shell shared by the conflict and info toasts: a hidden,
/// bottom-right-anchored `GtkBox` with an icon and a label already appended. The icon
/// and label are handed back as well as parented, so the info toast can retarget them
/// per notice; callers append any additional buttons themselves.
fn build_toast_shell(icon_name: &str, text: &str) -> (gtk::Box, gtk::Image, Label) {
    let toast = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    toast.add_css_class("conflict-toast");
    toast.set_halign(gtk::Align::End);
    toast.set_valign(gtk::Align::End);
    toast.set_margin_end(TOAST_MARGIN_END);
    toast.set_margin_bottom(20);
    toast.set_visible(false);
    let icon = gtk::Image::from_icon_name(icon_name);
    let label = Label::new(Some(text));
    toast.append(&icon);
    toast.append(&label);
    (toast, icon, label)
}

/// Build the floating conflict toast (hidden until a conflict arises).  "Reload"
/// discards local edits for the on-disk version; "Dismiss" keeps editing and
/// suppresses further conflict prompts until the next save/reload.
pub(super) fn make_conflict_toast(window: &ApplicationWindow) -> gtk::Box {
    let (toast, _icon, _label) =
        build_toast_shell(Icon::DialogWarning.name(), "File changed on disk.");

    let reload = gtk::Button::with_label("Reload");
    reload.add_css_class("suggested-action");
    let dismiss = gtk::Button::with_label("Dismiss");
    toast.append(&reload);
    toast.append(&dismiss);

    reload.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            super::reload::reload_from_disk(&w);
        }
    ));
    dismiss.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            if let Some(st) = state(&w) {
                st.suppress_conflict.set(true);
                st.chrome().conflict_toast.set_visible(false);
            }
            // The backing-loss prompt stood down while this one was up (they share a
            // corner). Answering this one is what lets it back, and nothing else would
            // re-derive it here — a document can be BOTH lost and in conflict, which
            // is exactly the state a flagged file returning with other content is in.
            sync_backing_loss_toast(&w);
            // The recovery prompt yields the corner the same way (TDD 22.7): recovered
            // work over a file that changed since the crash raises both.
            sync_recovery_toast(&w);
        }
    ));
    toast
}

/// Build the floating **recovery** prompt (hidden until a crash recovery happens).
///
/// A persistent prompt rather than a fading notice, and the distinction is the same one
/// this module's doc draws between the conflict toast and the info toast: a notice that
/// fades is right for something already done and needing no answer, while this offers a
/// choice the user must be able to take at their own pace. Automatic application is safe
/// for the *file* — nothing is written without an explicit save — but a user who never
/// saw the notice would have no way to know their buffer differs from disk and no route
/// back to it.
///
/// "Discard recovery" is deliberately **not** a second recovery pipeline run backwards:
/// the tab is by then an ordinary dirty tab, so reverting it is the existing reload path,
/// and the recovery data then goes away on its own because the tab is clean and the
/// governing invariant says a clean document has none. One rule, not a special case.
pub(super) fn make_recovery_toast(window: &ApplicationWindow) -> (gtk::Box, Label) {
    let (toast, _icon, label) =
        build_toast_shell(Icon::ViewRefresh.name(), "Recovered unsaved changes.");

    let keep = gtk::Button::with_label("Keep");
    keep.add_css_class("suggested-action");
    let discard = gtk::Button::with_label("Discard recovery");
    toast.append(&keep);
    toast.append(&discard);

    keep.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| dismiss_recovery_toast(&w)
    ));
    discard.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            // Revert to what is on disk. That alone clears the dirty flag, and the
            // dirtiness choke point then removes the recovery data — so this must NOT
            // also delete it by hand, which would be the second deletion path
            // GTK4Rs/AP-108/GEP-25 warn about.
            super::reload::reload_from_disk(&w);
            dismiss_recovery_toast(&w);
        }
    ));
    (toast, label)
}

/// Build the floating **backing-loss** prompt: the file behind this document was
/// truncated or deleted, so the buffer is now its only copy, and Save puts it back.
///
/// **Why a prompt and not just the status line.** The loss already has a persistent
/// status-bar line, and that line NAMES the remedy ("save to restore it") without
/// offering it. This is the actionable surface: two surfaces, one fact, different jobs
/// — the line states the condition for as long as it holds, the prompt carries the
/// control. That division is the Derived-view CAM's shape rather than a duplication,
/// and it is why this toast must never grow a second copy of the *condition* logic:
/// both derive from `backing_loss` and nothing else.
///
/// **Save is the ACTION, not a handler.** The button binds `win.save` by name, so its
/// sensitivity is GTK's to drive from the one `SimpleAction` every other Save surface
/// uses (POLICY § single source of truth). Wiring a click handler here would be a
/// second Save path that could diverge from the menu's — and it would have to
/// re-derive an enablement rule `save_enabled` already owns, which is precisely the
/// rule that makes Save live for a clean buffer over a lost file.
///
/// Contrast the recovery prompt above, whose Keep/Discard are genuinely not actions
/// and so are hand-wired correctly.
pub(super) fn make_backing_loss_toast(window: &ApplicationWindow) -> (gtk::Box, Label) {
    let (toast, _icon, label) = build_toast_shell(Icon::DialogWarning.name(), "");

    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    save.set_action_name(Some("win.save"));
    let dismiss = gtk::Button::with_label("Dismiss");
    toast.append(&save);
    toast.append(&dismiss);

    dismiss.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            // Retires the PROMPT, never the protection. The buffer is still the only
            // copy, so the ⚠ badge, the close prompt and the crash-recovery snapshot
            // all stay — they derive from `backing_loss`, which this does not touch.
            // Same rule as the conflict prompt's Dismiss: answering a question must
            // never quietly reduce what is guarding the document.
            if let Some(st) = state(&w) {
                st.suppress_backing_toast.set(true);
                st.chrome().backing_loss_toast.set_visible(false);
            }
        }
    ));
    (toast, label)
}

/// Show or hide the backing-loss prompt for `window`'s active tab.
///
/// Derived from `backing_loss` ALONE — never from a monitor event — so a loss that was
/// never concluded (a file gone only for the instant of somebody's rename-over) cannot
/// raise a prompt any more than it can raise the status line (TDD 3.4, 3.5). One bound,
/// both surfaces.
///
/// Called on every tab switch as well as whenever the loss changes, because the widget
/// is window-shared while the state it reports is per tab — the same obligation
/// [`sync_recovery_toast`] carries, for the same reason.
pub(crate) fn sync_backing_loss_toast(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    let Some(loss) = st.backing_loss.get() else {
        // The file is back. Retire the suppression with the condition, so a LATER loss
        // raises a fresh prompt instead of being swallowed by an answer the user gave
        // about a different event.
        st.suppress_backing_toast.set(false);
        chrome.backing_loss_toast.set_visible(false);
        return;
    };
    // Every toast is bottom-end aligned in one overlay, so two visible at once would
    // sit on top of each other. The conflict prompt wins while it is up: it asks about
    // content that would be DISCARDED by the wrong answer, which is the more
    // consequential question, and it retires by itself. This one returns on the next
    // sync once that is answered.
    if st.suppress_backing_toast.get() || chrome.conflict_toast.is_visible() {
        chrome.backing_loss_toast.set_visible(false);
        return;
    }
    chrome.backing_loss_toast_label.set_text(loss.prompt());
    super::chrome_fit::apply_visible_area_inset(&chrome.backing_loss_toast, TOAST_MARGIN_END);
    chrome.backing_loss_toast.set_visible(true);
}

/// Clear the recovery notice for the active tab and hide the shared widget.
fn dismiss_recovery_toast(window: &ApplicationWindow) {
    if let Some(st) = state(window) {
        st.recovered_at.set(None);
        st.chrome().recovery_toast.set_visible(false);
    }
}

/// Show the recovery prompt for `window`'s active tab, if that tab has one outstanding.
///
/// Called both when a recovery is first applied and on every tab switch, since the widget
/// is window-shared while the state it reports is per tab.
pub(crate) fn sync_recovery_toast(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    // A recovery notice is a statement about *unsaved* recovered content, so the moment
    // the document stops being dirty — saved, reverted, reloaded — it retires itself.
    //
    // Not a tidiness rule: a stale notice here is actively dangerous, because its
    // "Discard recovery" button reverts the tab to what is on disk. Left standing after
    // a save, that button would throw away work the user had just committed, while the
    // label went on describing a recovery that no longer has anything to do with what
    // they are looking at. Found by the Derived-view CAM's column B (persistence
    // events), which is exactly the class the happy path hides.
    if !st.is_dirty() {
        st.recovered_at.set(None);
    }
    let Some(when) = st.recovered_at.get() else {
        chrome.recovery_toast.set_visible(false);
        return;
    };
    // Every toast is bottom-end aligned in one overlay, and the conflict prompt takes
    // the corner while it is up — the rule the backing-loss prompt already follows,
    // for the same reason. Recovered work over a file that changed since the crash
    // raises both (TDD 22.7), and drawn second this one covered the conflict prompt
    // entirely: the reader was not told the file had changed until they had answered
    // Keep, and a Save in the meantime went ahead. The notice is not lost, only
    // deferred — `recovered_at` still holds it, and answering the conflict re-syncs it.
    if chrome.conflict_toast.is_visible() {
        chrome.recovery_toast.set_visible(false);
        return;
    }
    let stamp = glib::DateTime::from_unix_local(when)
        .and_then(|d| d.format("%H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "an earlier session".to_string());
    chrome
        .recovery_toast_label
        .set_text(&format!("Recovered unsaved changes from {stamp}."));
    super::chrome_fit::apply_visible_area_inset(&chrome.recovery_toast, TOAST_MARGIN_END);
    chrome.recovery_toast.set_visible(true);
}

/// Build the shared, button-less info notice (TDD §5.4). Its icon and text are set
/// per notice by [`show_info_toast`]; it starts hidden and unlabelled.
pub(super) fn make_info_toast() -> winstate::InfoToast {
    let (widget, icon, label) = build_toast_shell(Icon::ViewRefresh.name(), "");
    winstate::InfoToast::new(widget, icon, label)
}

/// Show the shared info notice AND announce the same thing in the status bar.
///
/// Both, not either: the visual toast fades, but the status label carries
/// `GTK_ACCESSIBLE_ROLE_STATUS`, so the status push is what a screen reader announces
/// (aria-live=polite) — TDD 16.3. The push is ephemeral (a timed
/// self-pop by `ctx`), leaving the persistent base message ("Unsaved changes" / "")
/// untouched underneath.
fn show_info_toast(
    window: &ApplicationWindow,
    icon_name: &str,
    toast_text: &str,
    status_text: &str,
) {
    let Some(st) = state(window) else { return };
    // Keep the bottom-right notice on screen when the toolbar min-width
    // has forced the window wider than the monitor. `TOAST_MARGIN_END` is the shell's
    // designed inset (build_toast_shell); the helper adds only the overflow, so this
    // is a no-op on any normal-width display.
    super::chrome_fit::apply_visible_area_inset(st.chrome().info_toast.widget(), TOAST_MARGIN_END);
    st.chrome()
        .info_toast
        .show(icon_name, toast_text, INFO_TOAST_TIME);
    // Through the chrome, never re-resolved through the window/tab at fire time:
    // the handle must be retracted from the stack that issued it. See
    // `WindowChrome::push_timed_notice`.
    st.chrome().push_timed_notice(status_text, INFO_STATUS_TIME);
}

/// The crash-recovery safety net has stopped working for this document.
///
/// Deliberately worded around the *net*, not the document: the user's file is untouched
/// and still saveable, and telling someone mid-edit that a save failed when it did not is
/// worse than silence. Shown once per transition into the failed state — the persistent
/// half of the report is the status-bar entry `window::swap` pushes alongside it, which
/// stays up for as long as the condition lasts.
pub(super) fn show_swap_failure_toast(window: &ApplicationWindow) {
    show_info_toast(
        window,
        Icon::DialogWarning.name(),
        "Unsaved changes are not being backed up.",
        "Unsaved changes are not being backed up",
    );
}

/// A clean reload just replaced the content under the user — flag it (TDD 5.4).
pub(super) fn show_reload_toast(window: &ApplicationWindow) {
    show_info_toast(
        window,
        Icon::ViewRefresh.name(),
        "File reloaded from disk.",
        "File reloaded",
    );
}

/// Confirm a successful write. Save is otherwise *silent* on success — the only
/// feedback is the unsaved indicator clearing, which is an absence, and absences are
/// easy to miss. Save As additionally renames the tab and title, but plain Save over
/// an unchanged-looking document had no positive acknowledgement at all.
pub(super) fn show_saved_toast(window: &ApplicationWindow) {
    show_info_toast(
        window,
        Icon::DocumentSave.name(),
        "File saved.",
        "File saved",
    );
}

/// Report an export's outcome — success **and** failure (TDD 25.14).
///
/// Takes the chrome rather than the window, because an export's completion lands
/// later and the notice must be retracted from the stack that issued it. Re-resolving
/// a stack through the tab at fire time answers "which window does this tab live in
/// *now*", which is a different question from "which stack owns this handle", and the
/// two diverge in exactly the cases the Status-notice CAM exists for (columns B/C).
pub(super) fn show_export_toast(chrome: &std::rc::Rc<winstate::WindowChrome>, text: &str) {
    super::chrome_fit::apply_visible_area_inset(chrome.info_toast.widget(), TOAST_MARGIN_END);
    chrome
        .info_toast
        .show(Icon::DocumentSaveAs.name(), text, INFO_TOAST_TIME);
    chrome.push_timed_notice(text, INFO_STATUS_TIME);
}
