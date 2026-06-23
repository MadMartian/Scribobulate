//! The external-strong-ref lifecycle for a `GtkNativeDialog`.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Holds the one external, app-owned strong reference a `GtkNativeDialog`
/// (`FileChooserNative` and friends) needs to survive its asynchronous
/// `response` round-trip. A `GtkNativeDialog` has the OPPOSITE liveness of a
/// `GtkWindow`: GTK keeps no reference of its own across the round-trip on every
/// backend, so without an external owner the dialog is finalized the instant the
/// calling function returns — before the user ever sees it (ScrAP-41).
///
/// [`NativeDialogHolder::show`] is the only way to obtain that guarantee: it owns
/// the strong ref, shows the dialog, runs `on_response(&dialog, response_type)`
/// when the user answers, and then drops the held ref exactly once so the dialog
/// can finalize normally.
///
/// **Precondition on the callback:** `on_response` receives the dialog by borrow
/// and must NOT capture its own clone of the dialog — the holder's ref is the
/// single one that gets dropped, so a captured clone would be stranded for the
/// process's life (the very leak this type exists to make unrepresentable).
pub(crate) struct NativeDialogHolder;

impl NativeDialogHolder {
    /// Show `dialog`, holding an external strong ref until its `response` fires,
    /// then invoke `on_response(&dialog, response_type)` and drop the ref once.
    pub(crate) fn show<D, F>(dialog: &D, on_response: F)
    where
        D: IsA<gtk::NativeDialog>,
        F: Fn(&D, gtk::ResponseType) + 'static,
    {
        // The sole strong, app-owned reference that outlives the calling
        // function; taken (dropped) exactly once when the response fires.
        let holder = Rc::new(RefCell::new(Some(dialog.clone())));
        // Both ends of the dialog's life are recorded. A native chooser is the
        // plan's second suspect for the `g_file_equal` dangling-`GFile` fault
        // (ScrAP-41: inverted liveness), and only a breadcrumb pair can show
        // whether one was live — or had just been dropped — when a crash landed.
        log::info!(
            "native dialog shown: {}",
            dialog.title().unwrap_or_default()
        );
        dialog.connect_response(move |d, resp| {
            log::info!("native dialog responded: {resp:?}");
            on_response(d, resp);
            holder.borrow_mut().take();
        });
        dialog.show();
    }
}
