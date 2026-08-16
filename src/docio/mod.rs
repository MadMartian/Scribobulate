//! Document file I/O, kept off the GTK main thread.
//!
//! Every read and write of a *user document* — Open, session restore, link
//! navigation, crash recovery's twin check, Save, Save As, Reload, and the live
//! reload monitor's re-read — passes through this module, and every one of them
//! does its blocking work on GLib's I/O thread pool rather than on the thread
//! running the main loop.
//!
//! # Why this module exists at all
//!
//! A `std::fs::read_to_string` in a GTK signal handler freezes the window for as
//! long as the filesystem takes to answer. On a local disk that is imperceptible,
//! which is why it survived so long; on an unresponsive NFS or FUSE mount, a
//! synced cloud folder, or a spun-down drive, it is an indefinite hang of a
//! window the user is looking at. `sdd/POLICY.md` § Prohibited actions has always
//! forbidden it and TDD 1.4 has always required the window to stay responsive —
//! the code simply did not comply.
//!
//! # The shape: blocking core, async skin
//!
//! Each operation is written **once**, as an ordinary synchronous function, and
//! then handed to [`off_main`]. Nothing is reimplemented against an async
//! filesystem API, and that is deliberate rather than lazy:
//!
//! * The read carries the admission check POLICY § Input limits makes mandatory
//!   (`limits::is_regular_file_within_limit`, on real [`std::fs::Metadata`]).
//!   GIO's `query_info` would mean re-deriving "is this a regular file, is it
//!   under the cap" from a different vocabulary — and POLICY's own words for that
//!   are "a caller that reimplements one half has reimplemented the bug".
//! * The write is [`crate::atomic_io::write_atomic`], which owns its temp file,
//!   its mode/ownership preservation and its rename. GIO's `replace_contents`
//!   promotes its own temp from the stream's *close* path, which has already
//!   discarded the knowledge that the write failed — measured here as a
//!   known-good file becoming 0 bytes (GTK4Rs/AP-167). Swapping our write for GIO's
//!   would move this I/O off the main thread by giving up the guarantee the I/O
//!   exists to provide.
//!
//! So the blocking functions stay exactly as tested, and the only new thing is
//! *where* they run.
//!
//! # The pool this shares, and the bound on it
//!
//! `gio::spawn_blocking` and GIO's own async file operations are one process-wide
//! 10-thread pool — the same one the crash-recovery snapshot writer goes through.
//! Occupying too much of it makes snapshots late, which is unprotected unsaved
//! work. [`pool`] carries the measurements and the bound that prevents it.
//!
//! # The enforcement mechanism is encapsulation, not a lint
//!
//! POLICY § Typed GTK seams requires a promoted contract to land with its
//! mechanism. Here it is module privacy: the blocking halves are private to this
//! file, so the only way to read or write a document from anywhere else in the
//! crate is through the `async fn`s below. A `clippy.toml` ban on
//! `std::fs::read_to_string` was considered and rejected on POLICY's own
//! true-positive test — the crate has dozens of legitimate calls in tests, in
//! `atomic_io`, and in `session`/`config` — and a ban that fires mostly on
//! innocent calls only teaches people to reach for `#[allow]`.
//!
//! # What is deliberately NOT here
//!
//! The application's own small state files — `session.toml`, the config file, the
//! crash-recovery swap directory scan — are still read synchronously at startup.
//! They are ours, they live in the state directory, they are small and bounded,
//! and their contents *decide* whether any window is built at all, so there is
//! nothing on screen to keep responsive while they are read. The boundary this
//! module draws is **document I/O off the main thread; the application's own
//! state files not**.

mod pool;
mod rename;

// `NameChange`/`NameRefusal`/`FsRules` are deliberately NOT re-exported: they appear
// in `validate_new_name`'s signature but no caller outside this module names them
// (the dialog maps them straight to `()` and a string), and re-exporting a type
// nobody spells is an unused import under `-D warnings`.
pub(crate) use rename::{host_rules, rename_document, validate_new_name, RenameError};

// Gated to the same cfg as its only callers (the gtk-integration-tests modules in
// `window::save`), not the broader `cfg(test)`: under a bare `cargo test` the callers
// are not compiled and the re-export became an unused import.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use pool::slow_io;

use crate::limits;
use pool::off_main;
use std::path::{Path, PathBuf};

/// A document as it came off disk: what to title it, what text to show, and
/// whether it is safe to save back over.
///
/// `backing` is `Some` **only** when the returned `source` is the file's real
/// content. It is `None` for a brand-new blank document (no path at all) and for
/// a file that exists but could not be read or was refused — because in those
/// cases `source` is a placeholder notice the user never wrote, and a later save
/// must not be able to write that notice over their file (C3).
pub(crate) struct LoadedDoc {
    pub(crate) title: String,
    pub(crate) source: String,
    pub(crate) backing: Option<PathBuf>,
}

/// What the filesystem said about a document path — the whole result of the
/// blocking half, with no message formatting and no logging in it.
///
/// Separating this from the prose below is what lets the classification be
/// unit-tested against a real filesystem with no main loop, no display and no
/// async runtime, while the messages and the `log::` records stay on the main
/// thread where the forensic breadcrumb ring expects them.
enum DocRead {
    /// The path was refused before being read (not a regular file, or over the cap).
    Refused(limits::LoadRefusal),
    /// Read in full.
    Loaded(String),
    /// Nothing there — an intended NEW document (TDD 1.3), not an error.
    Missing,
    /// Exists, but could not be read (permissions, non-UTF-8, I/O error) — C3.
    Failed(std::io::Error),
}

/// The UTF-8 byte-order mark, as text (`U+FEFF`) and as the three bytes
/// `EF BB BF` it encodes to.
///
/// Two spellings because two of the readers below hold the file as a `String` and
/// one holds it as raw bytes for a digest; they must agree, or a document with a
/// BOM would load stripped and then compare unstripped against its own file.
const UTF8_BOM: &str = "\u{feff}";
const UTF8_BOM_BYTES: &[u8] = UTF8_BOM.as_bytes();

/// Drop a leading UTF-8 BOM from a document's text.
///
/// **The invariant: no read through this module ever yields a BOM.** A BOM is an
/// encoding artefact, not document content, and leaving it in the first token is
/// what made a file starting `# Heading` render its `#` literally and report "No
/// headings" — the construct is no longer at the start of the line as far as the
/// Markdown scan is concerned, and only the *first* construct is affected, which
/// is what makes it read as a bad fixture rather than a bug. Windows tooling emits
/// BOMs routinely (PowerShell 5.1's `Set-Content -Encoding utf8`, and Notepad for
/// years), so this is an ordinary authoring path rather than a curiosity.
///
/// It has to be applied at **every** reader here, not just the display one: the
/// save guard compares the on-disk bytes against the in-memory baseline, and crash
/// recovery digests the on-disk twin against the digest of that same baseline. A
/// BOM stripped on one side of either comparison and not the other turns every
/// save of a BOM-carrying document into a spurious "File changed on disk" prompt,
/// and every recovery of one into a spurious stale-baseline verdict. Doing it at
/// the module's own door — whose blocking halves are private, so there is no other
/// route to a document — is what makes "every reader" checkable rather than
/// remembered.
///
/// **A BOM is not restored on save.** The buffer is the document, the buffer has
/// no BOM, and `write_document` writes the buffer; so the first explicit save of a
/// BOM-carrying file normalises it away. That is a deliberate choice over carrying
/// a per-tab "this file had a BOM" flag through the write path, the Save As path
/// and the snapshot format — the app has no other encoding state, and nothing else
/// currently needs any. The file is untouched until the user actually saves it.
fn without_bom(text: String) -> String {
    match text.strip_prefix(UTF8_BOM) {
        // Only pays for the copy when there is actually a BOM to remove.
        Some(rest) => rest.to_string(),
        None => text,
    }
}

/// [`without_bom`] for the raw-bytes reader. Same invariant, same reason.
fn without_bom_bytes(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.starts_with(UTF8_BOM_BYTES) {
        bytes.drain(..UTF8_BOM_BYTES.len());
    }
    bytes
}

/// The blocking half of [`read_document`]: stat, admit, read. Runs on the pool.
fn read_document_blocking(path: &Path) -> DocRead {
    // Before anything else: a crash between the two steps of a case-only rename
    // leaves the document under `<name>.rename-<pid>-<seq>` and nothing at `path`,
    // so an absence here is not always the intended-new-document it looks like
    // (`rename::recover_rename_orphan`). Recovering at the module's own door rather
    // than at one call site is what makes it cover Open, session restore, link
    // navigation and crash recovery alike — the blocking halves are private, so this
    // is the only route to a document.
    //
    // Ordered ahead of the admission check on purpose: a recovered file must be
    // stat'd and size-capped like any other, not admitted because it was absent when
    // the check ran. Costs one `read_dir` of the parent, and only on a path that is
    // missing — never on an ordinary open.
    if !path.exists() {
        rename::recover_rename_orphan(path);
    }
    // Admission check BEFORE the read (QA round 3, D-4). `read_to_string` on an
    // unbounded input is an unbounded allocation, and the content is then held in
    // three copies (this `String`, the `GtkTextBuffer`, the render products) — so
    // the cost is a multiple of the file size. Worse, a FIFO never ends and a
    // character device never runs out, so neither is a size question at all:
    // `limits::is_regular_file_within_limit` tests type AND size because either
    // alone admits the other's failure case.
    //
    // A `metadata` error is deliberately NOT treated as a refusal — it falls
    // through to the read below, so "not found" keeps producing a new empty
    // document (TDD 1.3) and every other error keeps its existing message and its
    // existing C3 handling. This check can only ADD refusals, never reclassify an
    // outcome the read was already getting right.
    //
    // Residual race, stated rather than papered over: the path is stat'd and then
    // opened by name, so a file swapped for a FIFO in between slips past. Opening
    // first and stat'ing the handle would close that, but `open()` on a FIFO
    // blocks — which is the very hang this guard exists to prevent. Since the read
    // now runs on the pool, losing that race costs a pool thread rather than the
    // window, which is a strictly smaller failure than the one this guard was
    // written for. The guard is a resource bound, not a security boundary; the
    // security boundary for paths is `links::resolve_image`/the doc-link gate,
    // which is canonicalize-based and unaffected by this.
    if let Ok(meta) = std::fs::metadata(path) {
        if let Err(refusal) = limits::is_regular_file_within_limit(&meta) {
            return DocRead::Refused(refusal);
        }
    }
    // Distinguish "not found" (an intended new document — TDD 1.3, empty and
    // creatable on save) from "exists but unreadable" (C3, see above).
    match std::fs::read_to_string(path) {
        Ok(content) => DocRead::Loaded(without_bom(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocRead::Missing,
        Err(e) => DocRead::Failed(e),
    }
}

/// Placeholder document shown when a file exists but cannot be read (permissions,
/// non-UTF-8, I/O error). Opening such a file as a blank, path-backed document
/// would let a save silently overwrite content the user never saw (C3); instead
/// the tab shows this notice and is given NO backing path, so Save stays disabled.
fn read_error_doc(path: &Path, err: &std::io::Error) -> String {
    format!("# ⚠ Could not open file\n\n`{}`\n\n{err}\n", path.display())
}

/// The placeholder shown when a path is REFUSED rather than failed (QA round 3,
/// D-4). Deliberately worded differently from [`read_error_doc`]: "could not"
/// invites the user to retry or check permissions, and neither will help here —
/// the application declined, and the reason is a property of the file that will
/// not change on its own.
fn refusal_doc(path: &Path, refusal: &limits::LoadRefusal) -> String {
    format!(
        "# ⚠ Refused to open file\n\n`{}`\n\n{refusal}\n",
        path.display()
    )
}

/// Turn a completed [`DocRead`] into the document to show. Main-thread half: the
/// user-facing prose and the lifecycle records (POLICY § Logging — `info` is the
/// forensic threshold, and these must reach the breadcrumb ring the crash report
/// dumps, which is main-thread state).
fn describe(path: &Path, title: String, read: DocRead) -> LoadedDoc {
    match read {
        DocRead::Refused(refusal) => {
            log::warn!("refusing to load {}: {refusal}", path.display());
            eprintln!(
                "scribobulate: refusing to load {}: {refusal}",
                path.display()
            );
            LoadedDoc {
                title,
                source: refusal_doc(path, &refusal),
                // `None` backing path, exactly as for an unreadable file: the user
                // never saw this content, so a later save must not be able to
                // overwrite it with the refusal notice (C3).
                backing: None,
            }
        }
        DocRead::Loaded(content) => {
            // The size, never the content (TDD 21.10): this record reaches the
            // persistent log and every crash report.
            log::info!("loaded {} ({} bytes)", path.display(), content.len());
            LoadedDoc {
                title,
                source: content,
                backing: Some(path.to_path_buf()),
            }
        }
        DocRead::Missing => {
            log::info!("opened {} as a new document (not on disk)", path.display());
            LoadedDoc {
                title,
                source: String::new(),
                backing: Some(path.to_path_buf()),
            }
        }
        DocRead::Failed(e) => {
            log::warn!("cannot read {}: {e}", path.display());
            eprintln!("scribobulate: cannot read {}: {e}", path.display());
            LoadedDoc {
                title,
                source: read_error_doc(path, &e),
                backing: None,
            }
        }
    }
}

/// The window/tab title for a document path — filename plus the app name, so the
/// window is identifiable in taskbars and window switchers ("README.md —
/// Scribobulate").
///
/// The formula itself is [`crate::winstate::window_title_for_tabs`]'s, called for
/// the one-tab case: this function's job is to turn a *path* into the name that
/// formula takes, not to re-derive what a title looks like. Keeping it a caller
/// rather than a copy is what stops the two from drifting apart the way Save As's
/// third derivation did.
fn title_for(path: &Path) -> String {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled".to_string());
    crate::winstate::window_title_for_tabs(1, Some(&filename))
}

/// Read a document's initial source text and compute its window/tab title.
///
/// Shared by every path that opens a document — the interactive/CLI/D-Bus `open`
/// route, session restore, link navigation and crash recovery — so their read
/// logic cannot drift apart.
pub(crate) async fn read_document(path: Option<&Path>) -> LoadedDoc {
    let Some(p) = path else {
        return LoadedDoc {
            title: "Scribobulate".to_string(),
            source: crate::app::WELCOME.to_string(),
            backing: None,
        };
    };
    let title = title_for(p);
    let owned = p.to_path_buf();
    let read = off_main(move || read_document_blocking(&owned)).await;
    describe(p, title, read)
}

/// Read a document that is **already open**, for a comparison rather than a
/// display: the save guard's "did this change under me?" check, an explicit
/// Reload, and the file monitor's re-read.
///
/// Unlike [`read_document`] this reports failure as an error rather than as a
/// placeholder document, because every caller has its own answer for a failed
/// read and none of them wants a notice pasted into the user's buffer.
///
/// It carries the same admission check as [`read_document`], which is new: these
/// three re-read a path that was admitted once, at open time, and then trusted
/// forever. POLICY § Input limits says *every* read of a document path goes
/// through the check, and a file can grow past the cap — or be replaced by
/// something that is not a regular file — long after it was opened. A refusal
/// surfaces as an ordinary read error, so each caller's existing failure handling
/// covers it: Save asks before overwriting, an explicit Reload reports it, and the
/// monitor stays silent.
pub(crate) async fn read_document_text(path: PathBuf) -> std::io::Result<String> {
    off_main(move || read_document_text_blocking(&path)).await
}

/// The blocking half of [`read_document_text`]. Runs on the pool.
fn read_document_text_blocking(path: &Path) -> std::io::Result<String> {
    // `metadata` first so a genuinely missing file still reports `NotFound` — the
    // save guard distinguishes that case (nothing to conflict with, safe to write)
    // from every other failure (cannot verify, ask first), and collapsing the two
    // is the QA round-2 N6 defect.
    let meta = std::fs::metadata(path)?;
    if let Err(refusal) = limits::is_regular_file_within_limit(&meta) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            refusal.to_string(),
        ));
    }
    std::fs::read_to_string(path).map(without_bom)
}

/// Read a document's raw bytes for a content comparison — crash recovery's check
/// that the on-disk twin still matches the baseline its snapshot was taken
/// against. Bytes rather than text because the answer is a digest, not something
/// anyone reads.
///
/// "Raw" means undecoded, not unfiltered: a leading BOM comes off here too, because
/// the digest this feeds is compared against one taken over the in-memory baseline,
/// which never has one ([`without_bom`]).
pub(crate) async fn read_document_bytes(path: PathBuf) -> std::io::Result<Vec<u8>> {
    off_main(move || {
        let meta = std::fs::metadata(&path)?;
        if let Err(refusal) = limits::is_regular_file_within_limit(&meta) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                refusal.to_string(),
            ));
        }
        std::fs::read(&path).map(without_bom_bytes)
    })
    .await
}

/// Write a document to `path` atomically, off the main thread.
///
/// The atomicity is [`crate::atomic_io::write_atomic`]'s and is unchanged by
/// running here — it writes a co-located temp and renames it into place, so an
/// interruption at any point (including the process dying while the pool thread
/// is mid-write) leaves the previous file intact rather than truncated.
pub(crate) async fn write_document(path: PathBuf, content: String) -> std::io::Result<()> {
    off_main(move || crate::atomic_io::write_atomic(&path, &content)).await
}

/// Test-only: iterate the default main context until `done` reports true, and
/// report whether it did.
///
/// Document I/O completes on the main context, so a test driving an operation that
/// used to be synchronous — Save, Reload, Open, following a link — has to let the
/// loop run before it can assert. This is that step, in one place, so each test
/// site is a single line naming the condition it is waiting for.
///
/// Two deliberate choices:
///
/// * It **blocks** (`iteration(true)`) rather than spinning, because the wakeup is a
///   thread-pool completion posted to this context and there may be nothing else
///   pending until it lands — a non-blocking spin can run its whole budget out
///   before the pool has answered, which is a flaky test rather than a fast one.
///   A tick source keeps `iteration` returning regularly so a condition that never
///   becomes true fails the test instead of hanging it.
/// * The wall-clock deadline is a **failure bound, not the completion signal**
///   (GTK4Rs/AP-122): success is always `done()` observing the real state. Nothing here
///   waits out a duration and then assumes.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn settle(done: impl Fn() -> bool) -> bool {
    let ctx = gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    // Guarantees a source is always due, so the blocking iteration below cannot
    // park forever when the awaited work never arrives.
    let ticker = gtk::glib::timeout_add_local(std::time::Duration::from_millis(2), || {
        gtk::glib::ControlFlow::Continue
    });
    while !done() && std::time::Instant::now() < deadline {
        ctx.iteration(true);
    }
    ticker.remove();
    done()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every assertion below is about the classification, which is the blocking
    /// half — so it is exercised directly rather than through the pool. That is the
    /// point of the split: the decisions are testable with no main loop and no
    /// display, and only the dispatch needs a live GLib context.
    fn read(path: &Path) -> DocRead {
        read_document_blocking(path)
    }

    /// Drive a document-I/O future to completion on a **private** main context.
    ///
    /// `MainContext::new()` rather than `default()`: libtest gives each test its own
    /// thread, and the default context is owned by whichever thread acquired it —
    /// asking for it here aborts (glib's `thread_guard`). A fresh context is owned by
    /// this thread, drives the pool completion exactly as the application's does, and
    /// needs no display, so the async path is exercised inside the headless gate
    /// rather than only in the GTK suite.
    fn drive<F: std::future::Future>(fut: F) -> F::Output {
        gtk::glib::MainContext::new().block_on(fut)
    }

    /// A document stranded by a crash mid-rename comes back through the module's own
    /// door, rather than presenting as the blank new document TDD 1.3 describes.
    ///
    /// Asserted here, at the door, and not only against `recover_rename_orphan`: the
    /// recovery is worth nothing if the reader concludes `Missing` before reaching it,
    /// and the ordering between the two is exactly what a later edit could invert
    /// without any unit test of the recovery itself noticing.
    #[test]
    fn a_document_stranded_mid_rename_is_recovered_by_the_reader() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        std::fs::write(dir.path().join("notes.md.rename-4242-0"), "# survived\n").unwrap();

        match read(&doc) {
            DocRead::Loaded(content) => assert_eq!(content, "# survived\n"),
            other => panic!("expected the stranded document, got {:?}", DebugRead(&other)),
        }
    }

    /// An absence with no orphan beside it is still an intended new document — the
    /// recovery must not have turned every missing file into an error or a scan that
    /// changes the answer.
    #[test]
    fn an_absence_with_no_orphan_is_still_a_new_document() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            read(&dir.path().join("never-existed.md")),
            DocRead::Missing
        ));
    }

    /// `DocRead` carries an `io::Error` and so cannot derive `Debug`; this names the
    /// variant for a panic message without widening the type's own surface.
    struct DebugRead<'a>(&'a DocRead);
    impl std::fmt::Debug for DebugRead<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(match self.0 {
                DocRead::Refused(_) => "Refused",
                DocRead::Loaded(_) => "Loaded",
                DocRead::Missing => "Missing",
                DocRead::Failed(_) => "Failed",
            })
        }
    }

    /// The read really does go to the pool and really does come back — proved with no
    /// display, no window and no GTK at all.
    ///
    /// This is the seam's own round trip: `off_main` → `gio::spawn_blocking` →
    /// completion on the main context. Everything above it in the crate is ordinary
    /// synchronous code awaiting this one hop, so if this holds, the rest is
    /// composition.
    #[test]
    fn a_document_round_trips_through_the_pool_without_a_display() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("round-trip.md");

        drive(write_document(
            path.clone(),
            "written off the main thread\n".into(),
        ))
        .expect("the write completes");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "written off the main thread\n"
        );

        let doc = drive(read_document(Some(&path)));
        assert_eq!(doc.source, "written off the main thread\n");
        assert_eq!(doc.backing.as_deref(), Some(path.as_path()));
        assert!(doc.title.starts_with("round-trip.md"));

        assert_eq!(
            drive(read_document_text(path.clone())).unwrap(),
            "written off the main thread\n"
        );
        assert_eq!(
            drive(read_document_bytes(path)).unwrap(),
            b"written off the main thread\n"
        );
    }

    /// **A leading UTF-8 BOM never survives a read — through EVERY reader.**
    ///
    /// The symptom that produced this test is a single wrong line: a file whose
    /// bytes begin `EF BB BF # Heading` renders the `#` literally and reports "No
    /// headings", because the BOM is carried into the first token and the construct
    /// is therefore not at the start of the line. Only the *first* construct is
    /// affected, which is why it reads as a bad fixture rather than a defect.
    ///
    /// **The three-reader sweep is the load-bearing half, not thoroughness.** The
    /// display reader is the one the symptom points at, and fixing only it is worse
    /// than fixing none: `read_document_text` backs the save guard's on-disk
    /// comparison and `read_document_bytes` backs crash recovery's twin digest, both
    /// against an in-memory baseline that came from the *display* reader. Strip on
    /// one side of either comparison and every save of a BOM-carrying document
    /// raises a spurious "File changed on disk" prompt, and every recovery of one
    /// reports a spurious stale baseline. Mutation: reverting any single one of the
    /// three `without_bom` calls fails exactly one assertion below.
    #[test]
    fn a_leading_utf8_bom_is_stripped_by_every_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bom.md");
        let body = "# BomHeading\n\nbody\n";
        std::fs::write(&path, format!("{UTF8_BOM}{body}")).unwrap();
        assert_eq!(
            &std::fs::read(&path).unwrap()[..3],
            UTF8_BOM_BYTES,
            "precondition: the fixture really does start EF BB BF"
        );

        assert_eq!(
            drive(read_document(Some(&path))).source,
            body,
            "the displayed document must start at the heading, or the renderer sees \
             a first line that does not begin with '#'"
        );
        assert_eq!(
            drive(read_document_text(path.clone())).unwrap(),
            body,
            "the save guard compares this against a baseline that has no BOM — a \
             mismatch here is a spurious overwrite prompt on every save"
        );
        assert_eq!(
            drive(read_document_bytes(path)).unwrap(),
            body.as_bytes(),
            "crash recovery digests this against a baseline that has no BOM — a \
             mismatch here reports the on-disk twin as changed when it is not"
        );
    }

    /// The strip is a prefix test, not a filter: `U+FEFF` anywhere but the first
    /// character is ordinary document content and stays.
    #[test]
    fn a_bom_shaped_character_inside_the_document_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("interior.md");
        let body = format!("# Heading\n\nzero{UTF8_BOM}width\n");
        std::fs::write(&path, &body).unwrap();

        assert_eq!(drive(read_document(Some(&path))).source, body);
    }

    /// A pathless read is the blank-document case and must not touch the filesystem
    /// or the pool at all — it is the WELCOME text, immediately.
    #[test]
    fn a_pathless_read_yields_the_welcome_document() {
        let doc = drive(read_document(None));
        assert_eq!(doc.source, crate::app::WELCOME);
        assert!(
            doc.backing.is_none(),
            "a blank document has nothing to save over"
        );
    }

    /// A panic on the pool thread is re-raised on the caller's side rather than
    /// swallowed. POLICY § Logging forbids `panic = "abort"` so the crash-report hook
    /// can run on the unwind; a seam that turned a panic into an `Err` would silently
    /// downgrade a programmer error into a failed save and lose the report.
    #[test]
    fn a_panic_on_the_pool_thread_is_re_raised_not_swallowed() {
        let caught = std::panic::catch_unwind(|| {
            drive(off_main(|| -> i32 { panic!("the blocking half panicked") }))
        });
        assert!(
            caught.is_err(),
            "the panic must reach the caller, where the crash-report hook lives"
        );
    }

    /// QA round 3, D-4: a FIFO must be REFUSED, not read.
    ///
    /// This is the case a size cap alone cannot catch — a FIFO's `len()` is 0, so
    /// every "is it too big?" test admits it, and `read_to_string` then blocks
    /// forever waiting for a writer that never comes. The test would HANG rather
    /// than fail without the type check, which is exactly what the guard prevents.
    #[cfg(unix)]
    #[test]
    fn a_fifo_is_refused_rather_than_read_forever() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pipe.md");
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: `c_path` is a valid NUL-terminated string outliving the call.
        let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(rc, 0, "mkfifo failed: {}", std::io::Error::last_os_error());

        assert!(
            matches!(read(&path), DocRead::Refused(_)),
            "a FIFO must be refused before it is read"
        );
        let doc = describe(&path, "t".into(), read(&path));
        assert!(doc.source.contains("Refused to open file"));
        assert!(
            doc.backing.is_none(),
            "a refused path must not be savable-back-over (C3) — a later save \
             would replace the user's pipe with the refusal notice"
        );
    }

    /// The guard must not reclassify anything the read already handled: a missing
    /// file is still a NEW document (TDD 1.3), not a refusal.
    #[test]
    fn a_missing_file_is_still_a_new_document_not_a_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.md");
        assert!(matches!(read(&path), DocRead::Missing));
        let doc = describe(&path, "t".into(), read(&path));
        assert_eq!(
            doc.source, "",
            "a missing file opens as an empty new document"
        );
        assert_eq!(
            doc.backing.as_deref(),
            Some(path.as_path()),
            "a new document keeps its backing path so it can be saved"
        );
    }

    /// And an ordinary document is untouched by the admission check.
    #[test]
    fn an_ordinary_document_loads_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# hello\n").unwrap();
        let doc = describe(&path, title_for(&path), read(&path));
        assert_eq!(doc.source, "# hello\n");
        assert_eq!(doc.backing.as_deref(), Some(path.as_path()));
        assert!(doc.title.starts_with("doc.md"));
    }

    /// The re-read path (Save's guard, Reload, the monitor) carries the admission
    /// check too — it was previously a bare `read_to_string` that trusted a check
    /// made once, at open time. Mutation: dropping the `is_regular_file_within_limit`
    /// call from `read_document_text_blocking` makes this hang instead of failing,
    /// which is the failure mode being prevented.
    #[cfg(unix)]
    #[test]
    fn re_reading_an_open_document_re_applies_the_admission_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("swapped.md");
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: `c_path` is a valid NUL-terminated string outliving the call.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        let err = read_document_text_blocking(&path).expect_err("a FIFO must not be re-read");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    /// …and a genuinely absent file still reports `NotFound` through it, because the
    /// save guard branches on exactly that to decide "nothing on disk to conflict
    /// with, safe to write" (QA round-2 N6). An admission check that swallowed the
    /// kind would turn a safe save into an overwrite prompt.
    #[test]
    fn re_reading_a_missing_document_still_reports_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_document_text_blocking(&dir.path().join("gone.md"))
            .expect_err("a missing file is an error on the re-read path");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
