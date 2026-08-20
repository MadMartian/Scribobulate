//! Driving `GtkPrintOperation`'s **export** route to produce a PDF (TDD 25.16–25.24).
//!
//! # Why the destination is never handed to GTK
//!
//! `set_export_filename` pointed at the reader's chosen path destroys that file **as
//! its first act**: GTK opens — and therefore truncates — the destination before the
//! first page is drawn, measured as zero `draw-page` calls when the open fails. Two
//! properties make that worse than an ordinary half-write. The wreckage is a *valid*
//! PDF — it extracts cleanly, so nothing looks broken and the reader simply gets a
//! plausible shorter document where the old one was. And it is reached by pressing
//! **Cancel**, which is ordinary use, not a failure. So the operation writes to a temp
//! staged by [`crate::atomic_io::stage_publish`] and promotes it only on success
//! (TDD 25.21).
//!
//! # The promote gate
//!
//! Promote **iff** `Ok(PrintOperationResult::Apply)` **and** `pages_drawn ==
//! pages_expected`. **Never** consult `is_finished()` or `status()`: they are inverted,
//! wrong in both directions — on a run that *succeeded* they read `GENERATING_DATA` /
//! false, permanently, and on every run that failed or was cancelled they read
//! `FinishedAborted` / true. A gate on `status() == Finished` reports failure on
//! success; a gate on `is_finished()` waits forever on success (TDD 25.20).
//!
//! # Threading
//!
//! `run(Export)` blocks the **caller** on the main thread; it does not freeze the loop,
//! and it must **never** be dispatched onto `docio/pool.rs`'s workers — GTK is
//! single-threaded. And no always-ready source may be installed for the duration of an
//! export: the fatal property is *always-ready*, not high-frequency — a 1 ms timeout is
//! harmless, while an idle returning `Continue` unconditionally hung the export on
//! macOS and livelocked it on Windows.

use crate::export::pdf::{finish, Outcome};
use crate::export::{self, paginate};
use crate::winstate::{self, TabState};
use gtk::prelude::*;
use gtk::{ApplicationWindow, PrintOperation, PrintOperationAction};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

/// Page margin in points, leaving room for the margin notes annotations need
/// (TDD 25.13 constrains the page setup: a margin note needs margin to sit in).
const MARGIN_PT: f64 = 54.0;

pub(super) fn export_pdf(window: &ApplicationWindow, st: Rc<TabState>, path: PathBuf) {
    // Paper has no dark mode (TDD 25.9). The PDF resolves through the same engine as
    // every other surface, but against the **System** theme rather than the active
    // reading theme, and against a paper base rather than the desktop probe — a dark
    // reading theme prints as a washed-out ghost of itself on a white sheet. Both
    // halves are resolution requests: `themes()` and `for_paper` own the values, and
    // there is no colour named here.
    let theme = std::rc::Rc::new(crate::theme::themes().resolve(crate::theme::SYSTEM_ID));
    let palette = crate::palette::Palette::for_paper(&theme);
    let doc = export::doc::build(
        &st.editor_text(),
        &export::RenderOptions {
            doc_dir: st.doc_dir(),
            allow_unsafe_images: st.allow_unsafe_images.get(),
        },
    );

    // Staged BEFORE the operation exists, so there is no window in which GTK holds
    // the reader's destination. The temp is created private (0600) first and cairo's
    // truncating open preserves that mode, keeping the write private from its first
    // byte (see `atomic_io::AtomicPublish`).
    let publish = match crate::atomic_io::stage_publish(&path) {
        Ok(p) => p,
        Err(e) => {
            log::error!(
                "export: could not stage a temp beside {}: {e}",
                path.display()
            );
            super::export::report(&st.chrome(), Some(window), &path, Err(e));
            return;
        }
    };
    let temp = publish.temp_path().to_path_buf();

    let op = PrintOperation::new();
    op.set_export_filename(&temp);
    op.set_unit(gtk::Unit::Points);
    op.set_show_progress(false);

    let laid: Rc<std::cell::RefCell<Option<export::pdf::Laid>>> =
        Rc::new(std::cell::RefCell::new(None));
    let pages: Rc<std::cell::RefCell<Vec<std::ops::Range<usize>>>> =
        Rc::new(std::cell::RefCell::new(Vec::new()));
    // The application's OWN count of what it drew. GTK's own reporting cannot be
    // trusted for this (see the module doc), so success is concluded from here.
    let drawn = Rc::new(Cell::new(0usize));

    op.connect_begin_print({
        let laid = Rc::clone(&laid);
        let pages = Rc::clone(&pages);
        let theme = Rc::clone(&theme);
        let doc = doc.clone();
        move |op, ctx| {
            let width = ctx.width() - MARGIN_PT * 2.0;
            let height = ctx.height() - MARGIN_PT * 2.0;
            let pango_ctx = ctx.create_pango_context();
            let l = export::pdf::lay_out(&doc, &pango_ctx, width, height, &theme);
            let p = paginate::paginate(&l.fragments, &export::pdf::metrics_for(height));
            // At least one page: an empty document still produces a file, rather than
            // a zero-page PDF that no reader will open.
            let n = p.len().max(1);
            op.set_n_pages(n as i32);
            *laid.borrow_mut() = Some(l);
            *pages.borrow_mut() = p;
        }
    });

    op.connect_draw_page({
        let laid = Rc::clone(&laid);
        let pages = Rc::clone(&pages);
        let theme = Rc::clone(&theme);
        let drawn = Rc::clone(&drawn);
        move |_op, ctx, page| {
            drawn.set(drawn.get() + 1);
            let cr = ctx.cairo_context();
            let laid = laid.borrow();
            let Some(l) = laid.as_ref() else { return };
            let pages = pages.borrow();
            let Some(range) = pages.get(page as usize) else {
                return;
            };
            export::pdf::draw_page(&cr, l, range.clone(), &palette, &theme, MARGIN_PT);
        }
    });

    // Synchronous on the main thread. Against the measured curves — linear across page
    // count, platform and a 40× range of content weight, no knee through 2,000 pages —
    // a document must exceed roughly forty dense pages before backgrounding would be
    // warranted, so this stays synchronous and the busy notice covers the rest.
    let busy = winstate::BusyNotice::arm(&st.chrome(), "Exporting…");
    let result = op.run(PrintOperationAction::Export, Some(window));
    drop(busy);

    let expected = op.n_pages().max(0) as usize;
    let outcome = finish(result, drawn.get(), expected);
    let chrome = st.chrome();
    match outcome {
        Outcome::Promote => match publish.publish_external() {
            Ok(()) => {
                log::info!("exported {} ({expected} pages)", path.display());
                super::export::report(&chrome, Some(window), &path, Ok(()));
            }
            Err(e) => {
                log::error!("export: promoting {} failed: {e}", path.display());
                super::export::report(&chrome, Some(window), &path, Err(e));
            }
        },
        Outcome::Discard { report_error } => {
            // The temp goes with the guard; the destination is byte-identical to what
            // it was, because GTK never had it (TDD 25.21).
            publish.abandon();
            if report_error {
                log::warn!("export to {} did not complete", path.display());
                super::export::report(
                    &chrome,
                    Some(window),
                    &path,
                    Err(std::io::Error::other("the export did not complete")),
                );
            } else {
                log::info!("export to {} cancelled", path.display());
            }
        }
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod export_destination_tests {
    use crate::export::pdf::{finish, Outcome};
    use gtk::prelude::*;
    use gtk::{PrintOperation, PrintOperationAction};
    use std::cell::Cell;
    use std::rc::Rc;

    /// **TDD 25.21** — a cancelled export leaves the destination byte-identical.
    ///
    /// This drives a **real** `GtkPrintOperation` through a **real** render and calls
    /// `cancel()` part-way, over a **real** previous PDF. None of that is incidental:
    /// the partial such a run leaves behind is itself a structurally valid, cleanly
    /// extracting PDF, so an assertion against "is the file a valid PDF" cannot tell
    /// the two apart — only the previous file's **bytes** can.
    ///
    /// It records structurally why it is green, so a mutation sweep cannot delete it as
    /// vacuous: the previous file survives *because* the operation was pointed at a
    /// staged temp instead of at the destination. Point it at the destination and this
    /// test fails, which is the positive control below.
    #[gtktest::test]
    fn a_cancelled_export_leaves_the_previous_pdf_byte_identical() {
        let dir = tempfile::tempdir().expect("temp dir");
        let dest = dir.path().join("keepme.pdf");

        // A REAL previous PDF, produced the same way an export produces one — not a
        // stub, because the failure mode is a plausible shorter document replacing a
        // longer one, and a stub could not exhibit it.
        write_pdf(&dest, 8, None);
        let original = std::fs::read(&dest).expect("read the seeded PDF");
        assert!(original.len() > 1000, "the seed must be a real PDF");

        // The export: staged beside the destination, cancelled at page 3 of 8.
        let publish = crate::atomic_io::stage_publish(&dest).expect("stage");
        let temp = publish.temp_path().to_path_buf();
        let drawn = write_pdf(&temp, 8, Some(3));
        publish.abandon();

        let after = std::fs::read(&dest).expect("read the destination");
        assert_eq!(
            after, original,
            "the previous PDF was altered by a cancelled export"
        );
        assert!(drawn < 8, "the cancel must have stopped the render short");
        assert!(!temp.exists(), "the staged temp must be cleaned up");
        // No debris beside the destination either.
        let siblings: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(siblings, vec!["keepme.pdf".to_string()], "{siblings:?}");
    }

    /// The **positive control** for the test above, and the reason it is not vacuous.
    ///
    /// Pointing the same cancelled operation straight at the destination — the route
    /// this feature deliberately does not take — destroys it. If this ever stops
    /// destroying the file, the test above has stopped proving anything and both need
    /// re-deriving.
    #[gtktest::test]
    fn pointing_a_cancelled_export_at_the_destination_does_destroy_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let dest = dir.path().join("victim.pdf");
        write_pdf(&dest, 8, None);
        let original = std::fs::read(&dest).expect("read");

        // The rejected route: `set_export_filename` on the user's own path.
        write_pdf(&dest, 8, Some(3));

        let after = std::fs::read(&dest).expect("read");
        assert_ne!(
            after, original,
            "the unstaged route must destroy the destination — if it no longer does, \
             the staging test above is no longer evidence of anything"
        );
        // And the wreckage is a VALID PDF, which is what makes it dangerous: nothing
        // downstream can tell it from a complete one.
        assert!(
            after.starts_with(b"%PDF-"),
            "the partial is a structurally valid PDF, which is the whole problem"
        );
    }

    /// Render `pages` pages of trivial content to `path`, cancelling after
    /// `cancel_after` pages when given. Returns how many pages were drawn.
    fn write_pdf(path: &std::path::Path, pages: i32, cancel_after: Option<usize>) -> usize {
        let op = PrintOperation::new();
        op.set_export_filename(path);
        op.set_unit(gtk::Unit::Points);
        op.set_n_pages(pages);
        let drawn = Rc::new(Cell::new(0usize));
        op.connect_draw_page({
            let drawn = Rc::clone(&drawn);
            move |op, ctx, _page| {
                drawn.set(drawn.get() + 1);
                let cr = ctx.cairo_context();
                // Real ink, so the pages differ in size and a truncation is visible.
                let layout = ctx.create_pango_layout();
                layout.set_text(&"content ".repeat(200));
                layout.set_width(400 * gtk::pango::SCALE);
                cr.move_to(50.0, 50.0);
                pangocairo::functions::show_layout(&cr, &layout);
                // Cancel from INSIDE draw-page, which is the sanctioned way and returns
                // `Ok(Cancel)` after the current page completes.
                if let Some(n) = cancel_after {
                    if drawn.get() >= n {
                        op.cancel();
                    }
                }
            }
        });
        let result = op.run(PrintOperationAction::Export, None::<&gtk::Window>);
        let expected = op.n_pages().max(0) as usize;
        let outcome = finish(result, drawn.get(), expected);
        if cancel_after.is_some() {
            assert_eq!(
                outcome,
                Outcome::Discard {
                    report_error: false
                },
                "a cancel must reach the gate as a clean discard"
            );
        } else {
            assert_eq!(outcome, Outcome::Promote, "a complete run must promote");
        }
        drawn.get()
    }
}
