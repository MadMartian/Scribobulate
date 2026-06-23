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
const INFO_STATUS_TIME: Duration = Duration::from_secs(4);

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
            // ScrAP-116/ScrAP-219 warn about.
            super::reload::reload_from_disk(&w);
            dismiss_recovery_toast(&w);
        }
    ));
    (toast, label)
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
