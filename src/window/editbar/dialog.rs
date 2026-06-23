//! The small modal input form (`input_form`) shared by the Tier-2 Insert dialogs
//! and Go To Line, plus the "Browse…" file chooser (`BrowseSpec`/`open_file_chooser`).

use super::super::*;

/// A "Browse…" affordance for one field of an [`input_form`] (the Insert Link / Image
/// URL): a button (mnemonic Alt+R) that opens a file chooser rooted at `start_dir` and
/// writes the chosen path into that field.
pub(super) struct BrowseSpec {
    /// Index of the field whose row gets the Browse button.
    pub(super) field: usize,
    /// Folder the chooser opens in (the loaded document's directory), or None for the
    /// chooser default.
    pub(super) start_dir: Option<std::path::PathBuf>,
    /// Restrict the chooser to image files (Insert Image); false offers any file
    /// (Insert Link).
    pub(super) image_filter: bool,
}

/// Run a small modal input form: one labelled `GtkEntry` per field, plus Cancel /
/// `confirm_label`. On confirm, calls `on_ok(window, values)` with the entry texts
/// in field order; Cancel or Escape closes without acting. A plain `GtkWindow` (no
/// `GtkDialog`, which is deprecated ≥ 4.10) keeps `clippy -D warnings` clean. Used
/// by the Tier-2 Insert Link / Image / Table commands (POLICY single-source-of-truth:
/// they are `win.format` targets like the inline wraps) and Go To Line —
/// `confirm_label` keeps the button's verb accurate for each ("Insert" vs "Go").
///
/// `focus_field` is the index of the field that takes the initial keyboard focus.
/// It is a caller's choice rather than "always field 0" because the right answer
/// depends on what the *caller* pre-filled: a dialog whose first field is already
/// seeded (Insert Link over a selection — the caption is filled in, the URL is not)
/// should land the caret on the field the user still has work to do in, not make
/// them Tab past a field that is already correct. An out-of-range index falls back
/// to the first field.
pub(super) fn input_form(
    window: &ApplicationWindow,
    title: &str,
    fields: &[(&str, String)],
    browse: Option<BrowseSpec>,
    confirm_label: &str,
    focus_field: usize,
    on_ok: impl Fn(&ApplicationWindow, Vec<String>) + 'static,
) {
    let dialog = gtk::Window::builder()
        .transient_for(window)
        .modal(true)
        .title(title)
        .default_width(380)
        .resizable(false)
        .build();

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);

    let entries: Vec<gtk::Entry> = fields
        .iter()
        .enumerate()
        .map(|(i, (label, initial))| {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let lbl = gtk::Label::new(Some(*label));
            lbl.set_xalign(0.0);
            lbl.set_width_chars(9);
            row.append(&lbl);
            let entry = gtk::Entry::new();
            entry.set_text(initial);
            entry.set_hexpand(true);
            // Enter triggers the default widget (Insert) from any field.
            entry.set_activates_default(true);
            row.append(&entry);
            // The browse field (the Link / Image URL) gets a "Browse…" button —
            // mnemonic Alt+R, active while the dialog is focused — that opens a file
            // chooser and fills the entry with the chosen path.
            if browse.as_ref().is_some_and(|b| b.field == i) {
                let start_dir = browse.as_ref().and_then(|b| b.start_dir.clone());
                let image_filter = browse.as_ref().is_some_and(|b| b.image_filter);
                let browse_btn = gtk::Button::with_mnemonic("B_rowse…");
                let entry_for_browse = entry.clone();
                browse_btn.connect_clicked(gtk::glib::clone!(
                    #[weak(rename_to = parent)]
                    dialog,
                    move |_| {
                        open_file_chooser(
                            &parent,
                            start_dir.clone(),
                            &entry_for_browse,
                            image_filter,
                        );
                    }
                ));
                row.append(&browse_btn);
            }
            vbox.append(&row);
            entry
        })
        .collect();

    let btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk::Align::End);
    btn_row.set_margin_top(8);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label(confirm_label);
    ok.add_css_class("suggested-action");
    btn_row.append(&cancel);
    btn_row.append(&ok);
    vbox.append(&btn_row);

    dialog.set_child(Some(&vbox));
    dialog.set_default_widget(Some(&ok));

    // Weak self-refs throughout: a strong `dialog.clone()` captured in a handler
    // the dialog itself owns (as a descendant widget's signal) forms a reference
    // cycle that never drops, leaking the dialog window on every Insert
    // Link/Image/Table invocation.
    cancel.connect_clicked(gtk::glib::clone!(
        #[weak(rename_to = d)]
        dialog,
        move |_| {
            d.close();
        }
    ));
    // Escape cancels.
    let key = gtk::EventControllerKey::new();
    {
        let d = dialog.downgrade();
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                if let Some(d) = d.upgrade() {
                    d.close();
                }
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    dialog.add_controller(key);

    {
        let d = dialog.downgrade();
        let win_weak = window.downgrade();
        let entries = entries.clone();
        ok.connect_clicked(move |_| {
            let values: Vec<String> = entries.iter().map(|e| e.text().to_string()).collect();
            if let Some(w) = win_weak.upgrade() {
                on_ok(&w, values);
            }
            if let Some(d) = d.upgrade() {
                d.close();
            }
        });
    }

    dialog.present();
    // `grab_focus` on a GtkEntry also selects its text, so a pre-filled focused
    // field is ready to be typed over rather than appended to.
    if let Some(entry) = entries.get(focus_field).or_else(|| entries.first()) {
        entry.grab_focus();
    }
}

/// Open a native file chooser rooted at `start_dir`, writing the chosen file's path
/// into `entry` (relative to `start_dir` when the file lives under it, else absolute).
/// With `image_filter`, the chooser defaults to image files. Backs the Insert Link /
/// Image URL "Browse…" buttons.
fn open_file_chooser(
    parent: &gtk::Window,
    start_dir: Option<std::path::PathBuf>,
    entry: &gtk::Entry,
    image_filter: bool,
) {
    let chooser = FileChooserNative::new(
        Some(if image_filter {
            "Select Image"
        } else {
            "Select File"
        }),
        Some(parent),
        FileChooserAction::Open,
        Some("Select"),
        Some("Cancel"),
    );
    if image_filter {
        let images = gtk::FileFilter::new();
        images.set_name(Some("Images"));
        for pat in [
            "*.png", "*.jpg", "*.jpeg", "*.gif", "*.svg", "*.webp", "*.bmp",
        ] {
            images.add_pattern(pat);
        }
        chooser.add_filter(&images);
    }
    let all = gtk::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    chooser.add_filter(&all);
    // Start in doc dir if provided, otherwise the last-visited dialog dir.
    let effective_dir = start_dir
        .clone()
        .or_else(|| crate::app::LAST_DIALOG_DIR.with(|c| c.borrow().clone()));
    if let Some(dir) = &effective_dir {
        let _ = chooser.set_current_folder(Some(&gtk::gio::File::for_path(dir)));
    }
    let entry = entry.clone();
    crate::saferizer::native_dialog::NativeDialogHolder::show(&chooser, move |ch, resp| {
        if resp == ResponseType::Accept {
            if let Some(path) = ch.file().and_then(|f| f.path()) {
                remember_dialog_dir(&path);
                // Insert a document-relative reference when there's a base folder
                // (security + portability — same containment as image loading);
                // an untitled buffer has no base, so insert the absolute path.
                let text = match start_dir.as_deref() {
                    Some(base) => crate::links::relativize_for_insert(&path, base),
                    None => path.to_string_lossy().into_owned(),
                };
                entry.set_text(&text);
            }
        }
        ch.destroy();
    });
}
