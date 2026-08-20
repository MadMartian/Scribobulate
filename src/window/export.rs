//! File ▸ Export — the GTK edge of the export feature (TDD §25).
//!
//! Everything decidable without a display lives in [`crate::export`]; this file owns
//! only what needs GTK: the `win.export` action, the destination chooser, dispatching
//! the write, and the outcome notice. The split is the same one `copymap` and
//! `affordance` make, and it is what puts the export's correctness inside the
//! coverage gate rather than resting on a human opening the file.
//!
//! # The default filename is a guarantee, not a convention
//!
//! The chooser is seeded with the document's **stem** plus the target extension —
//! never its filename. On Windows a native save chooser appends the filter's extension
//! only when the seeded name carries no dot-suffix at all, and does not enforce the
//! filter's type, so a name derived from `notes.md` comes back as `notes.md` and the
//! export writes PDF bytes over the reader's Markdown source. macOS replaces the
//! extension and is safe — which is the worst possible distribution, because the
//! dangerous derivation survives review by anyone testing on a Mac. Hence
//! [`default_export_name`] is a pure function with its own tests, asserted on every
//! platform (TDD 25.10).
//!
//! # Validation direction
//!
//! The name this application **proposes** is validated, through `docio::rename`'s
//! rules; the name the chooser **returns** is not. The platform agrees with those
//! rules on reserved device names and forbidden characters and disagrees on trailing
//! dots and spaces, so gating the return would reject a name the platform had already
//! accepted and rewritten. Once the chooser is open, naming the file is the reader's
//! responsibility (TDD 25.11).

use super::{dialog_dir_for, remember_dialog_dir};
use crate::export::{self, default_export_name, ExportTarget};
use crate::winstate::{self, state, TabState};
use gtk::gio;
use gtk::prelude::*;
use gtk::{ApplicationWindow, FileChooserAction, FileChooserNative, ResponseType};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Register `win.export`. One action, parameter `s`, targeting `html` and `pdf` —
/// the `win.change-case` shape, so every surface drives its behaviour *and* its
/// sensitivity from this one action (POLICY's single-source-of-truth rule; ScrAP-9).
pub(crate) fn register_export_action(window: &ApplicationWindow) {
    // File ▸ Export is a nested submenu, so activation goes through the same
    // constructor every other nested-submenu action uses — otherwise GTK 4.6–4.12
    // leaves the parent "File" menu popped open behind the chooser (ScrAP-116).
    let action = super::actions::nested_submenu_action(
        window,
        "export",
        Some(&gtk::glib::VariantType::new("s").unwrap()),
        |w, param| {
            let Some(target) = param
                .and_then(|v| v.str())
                .and_then(ExportTarget::from_target)
            else {
                return;
            };
            // The subject is resolved ONCE, when the reader acts, and carried across
            // the chooser and the write — the main loop runs during both, so a
            // re-ask after either would answer about whatever tab is active then
            // (ScrAP-244; Deferred-operation CAM).
            let Some(st) = state(w) else { return };
            choose_destination(w, st, target);
        },
    );
    window.add_action(&action);
}

/// Whether Export can act: whenever the tab holds a document, **including untitled
/// and unsaved ones**. An export reads the buffer and never the disk file, so having
/// never been saved is no impediment — that is TDD-visible behaviour (25.5) rather
/// than an implementation detail.
pub(crate) fn export_is_available(window: &ApplicationWindow) -> bool {
    state(window).is_some()
}

/// Drive the sensitivity of `win.export` from the one gate above.
pub(crate) fn update_export_action_state(window: &ApplicationWindow) {
    if let Some(action) = window.lookup_action("export") {
        if let Some(simple) = action.downcast_ref::<gio::SimpleAction>() {
            simple.set_enabled(export_is_available(window));
        }
    }
}

fn choose_destination(window: &ApplicationWindow, st: Rc<TabState>, target: ExportTarget) {
    let chooser = FileChooserNative::new(
        Some(&format!("Export as {}", target.label())),
        Some(window),
        FileChooserAction::Save,
        Some("Export"),
        Some("Cancel"),
    );
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&format!("{} files", target.label())));
    filter.add_pattern(&format!("*.{}", target.extension()));
    chooser.add_filter(&filter);
    chooser.set_current_name(&default_export_name(st.path.borrow().as_deref(), target));
    // Best-effort, and its failure is unobservable: on Windows the setter returns
    // `Ok(())` and `current_folder()` reads back `None` whether or not the folder was
    // honoured, so nothing downstream may assume the dialog opened where it was asked.
    if let Some(dir) = dialog_dir_for(Some(window)) {
        let _ = chooser.set_current_folder(Some(&gio::File::for_path(&dir)));
    }
    let win_weak = window.downgrade();
    // A `NativeDialog` needs ONE external strong reference for its lifetime — the one
    // dialog type whose liveness rule is the opposite of a `GtkWindow`'s (ScrAP-41).
    crate::saferizer::native_dialog::NativeDialogHolder::show(&chooser, move |ch, resp| {
        let chosen = (resp == ResponseType::Accept)
            .then(|| ch.file().and_then(|f| f.path()))
            .flatten();
        // Destroyed before the write, exactly as Save As does: the chooser's teardown
        // must not wait on a filesystem that may be slow to answer.
        ch.destroy();
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        // Cancelled: no file, no notice, no I/O at all (TDD 25.6).
        let Some(path) = chosen else {
            log::info!("tab {}: export to {} cancelled", st.id, target.target());
            return;
        };
        remember_dialog_dir(&path);
        run_export(&window, Rc::clone(&st), target, path);
    });
}

/// Render and write, then report. The subject `st` was resolved before the chooser
/// opened and is carried here rather than re-asked.
fn run_export(window: &ApplicationWindow, st: Rc<TabState>, target: ExportTarget, path: PathBuf) {
    log::info!(
        "tab {}: exporting to {} at {}",
        st.id,
        target.target(),
        path.display()
    );
    match target {
        ExportTarget::Html => export_html(window, st, path),
        ExportTarget::Pdf => super::export_pdf::export_pdf(window, st, path),
    }
}

fn export_html(window: &ApplicationWindow, st: Rc<TabState>, path: PathBuf) {
    // Resolved on the main thread, where GTK is: the palette's link-3 fallback probes
    // the desktop through a style context. Everything downstream of this is pure.
    let theme = crate::theme::active();
    let palette = crate::palette::Palette::for_theme(&theme);
    let doc = export::doc::build(
        &st.editor_text(),
        &export::RenderOptions {
            doc_dir: st.doc_dir(),
            allow_unsafe_images: st.allow_unsafe_images.get(),
        },
    );
    let html = export::html::render(&doc, &palette, &theme);
    let bytes = html.len();

    let win_weak = window.downgrade();
    // Armed, not shown: nothing appears unless the write outlives the busy delay, so
    // a fast export never blinks (Status-notice CAM row 6).
    let busy = winstate::BusyNotice::arm(&st.chrome(), "Exporting…");
    // The notice's stack is captured HERE, at the push, never re-resolved when the
    // write lands — the tab can be closed or moved to another window while the write
    // is out, and a retraction that re-derives its own destination is the general
    // shape of the bug (Status-notice CAM columns B and C).
    let chrome = st.chrome();
    gtk::glib::MainContext::default().spawn_local(async move {
        // The same off-main-thread atomic write every document write uses: a
        // co-located temp renamed into place, so an interruption leaves no partial
        // file at the destination (TDD 25.7). Reused rather than duplicated — POLICY
        // § Architecture rules prefers extending an existing path to adding a
        // parallel one, and this IS that operation.
        let result = crate::docio::write_document(path.clone(), html).await;
        drop(busy);
        match &result {
            Ok(()) => log::info!("exported {} ({bytes} bytes)", path.display()),
            Err(e) => log::error!("export to {} failed: {e}", path.display()),
        }
        // Reported on success AND failure: a silent export is indistinguishable from
        // a broken one, and the file it wrote is somewhere the reader was not
        // watching (TDD 25.14). Against the captured chrome, so it lands in the
        // window the export was started from even if the tab has since moved or the
        // window that owns it now is a different one.
        report(&chrome, win_weak.upgrade().as_ref(), &path, result);
    });
}

/// Raise the outcome notice for a finished export.
pub(super) fn report(
    chrome: &Rc<crate::winstate::WindowChrome>,
    window: Option<&ApplicationWindow>,
    path: &Path,
    result: std::io::Result<()>,
) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    match result {
        Ok(()) => {
            super::toast::show_export_toast(chrome, &format!("Exported {name}"));
        }
        Err(e) => {
            // Named, not generic: on Windows the ordinary failure is a destination the
            // reader still has open in a viewer, whose remedy is theirs to apply, and
            // "write failed" describes neither the cause nor it (TDD 25.24).
            let message = describe_export_error(&e);
            super::toast::show_export_toast(chrome, &format!("Export failed — {message}"));
            if let Some(window) = window {
                log::warn!("export error surfaced to the reader: {message}");
                let _ = window;
            }
        }
    }
}

/// A reader-facing description of an export failure.
///
/// `PermissionDenied` on a *destination the reader chose* is overwhelmingly a file
/// another process holds open — on Windows `MoveFileExW` cannot replace a destination
/// whose existing handles did not grant `FILE_SHARE_DELETE`, and the ordinary case is
/// exporting over a PDF still open in a viewer. Naming that is the difference between
/// a message the reader can act on and one they cannot.
pub(crate) fn describe_export_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "the destination is open in another program, or is not writable. \
             Close the file and try again."
                .to_string()
        }
        std::io::ErrorKind::NotFound => "that folder no longer exists.".to_string(),
        std::io::ErrorKind::StorageFull => "there is no space left on the disk.".to_string(),
        _ => e.to_string(),
    }
}
