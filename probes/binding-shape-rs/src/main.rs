//! **Which gtk4-rs shape may a buffer-repair handler be written in?**
//!
//! A standalone probe outside the workspace, deliberately, and not a test target.
//! What it asserts is a property of **gtk4-rs**, not of Scribobulate, so as a gate it
//! would go red the day the binding is fixed upstream — reporting good news as a
//! failure. It is also process-global by nature: it owns the process's log handler in
//! order to count GLib diagnostics, and it registers a `GObject` subclass, which
//! cannot be un-registered. Neither survives sharing a process with the main suite.
//!
//! # What it is for
//!
//! `connect_insert_text`'s trampoline hands the closure a **copy** of the caller's
//! `GtkTextIter` and never writes it back (`gtk4-0.10.3/src/text_buffer.rs:76`,
//! identical at `0.9.7:85`):
//!
//! ```text
//! let mut location_copy = from_glib_none(location);   // COPY
//! f(..., &mut location_copy, ...)                     // closure gets the COPY
//! ```
//!
//! So a nested `buffer.insert(iter, …)` revalidates the copy while the iterator
//! `insert_range_untagged` is holding stays stale. Across a multi-run
//! `insert_range` the destination iterator never advances, later chunks land at a
//! stale position, and the pasted text comes back **reordered** — which is a strictly
//! worse outcome than the line-ending corruption this work started on.
//!
//! The subclass path does the opposite: `text_buffer_insert_text` ends
//! `*iter_ptr = *iter.to_glib_none().0` (`gtk4-0.10.3/src/subclass/text_buffer.rs:360-373`),
//! writing the iterator back. `sourceview5::subclass::BufferImpl: TextBufferImpl`, so a
//! `GtkSourceBuffer` subclass gets it.
//!
//! This target measures both **in the real binding**, because that is the claim —
//! `probes/binding-shape.c` establishes the mechanism in C, and a C probe cannot
//! prove anything about a Rust trampoline. It exists to be able to **falsify** the
//! vfunc recommendation before the project builds on it: if arm R2 is also noisy, the
//! write-back read is wrong and the recommendation must be withdrawn.
//!
//! Run: `cargo run --manifest-path probes/binding-shape-rs/Cargo.toml`

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use sourceview::subclass::prelude::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Source text whose tag toggles land inside each `\r\n`, so `insert_range` chunks
/// the pairs apart. The same fixture `probes/binding-shape.c` uses, so the two are
/// directly comparable.
const PAYLOAD: &str = "ab\r\ncd\r\nef\r\ngh";
/// Tagged spans, each ending on an `\n` that is preceded by a `\r`.
const SPANS: [(i32, i32); 3] = [(0, 3), (4, 7), (8, 11)];

static DIAGNOSTICS: AtomicUsize = AtomicUsize::new(0);

/// Rewrite every lone `\r` to `\n`; the repair under test.
fn repair(text: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    for i in 0..out.len() {
        if out[i] == b'\r' && out.get(i + 1) != Some(&b'\n') {
            out[i] = b'\n';
        }
    }
    String::from_utf8(out).expect("ASCII substitution preserves UTF-8")
}

fn has_lone_cr(text: &str) -> bool {
    let b = text.as_bytes();
    b.iter()
        .enumerate()
        .any(|(i, &c)| c == b'\r' && b.get(i + 1) != Some(&b'\n'))
}

// ── Arm R2's subclass ────────────────────────────────────────────────────────────
mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RepairBuffer;

    #[glib::object_subclass]
    impl ObjectSubclass for RepairBuffer {
        const NAME: &'static str = "ScribRepairBuffer";
        type Type = super::RepairBuffer;
        type ParentType = sourceview::Buffer;
    }

    impl ObjectImpl for RepairBuffer {}
    impl TextBufferImpl for RepairBuffer {
        /// The whole point: no `stop_emission`, no reentrancy flag. Hand the parent
        /// the repaired text and let it place the iterator, which the subclass
        /// trampoline then writes back to the caller.
        fn insert_text(&self, iter: &mut gtk::TextIter, new_text: &str) {
            if has_lone_cr(new_text) {
                self.parent_insert_text(iter, &repair(new_text));
            } else {
                self.parent_insert_text(iter, new_text);
            }
        }
    }
    impl BufferImpl for RepairBuffer {}
}

glib::wrapper! {
    pub struct RepairBuffer(ObjectSubclass<imp::RepairBuffer>)
        @extends sourceview::Buffer, gtk::TextBuffer;
}

/// Build a source buffer holding [`PAYLOAD`] with [`SPANS`] tagged, sharing `table`
/// so `insert_range` is willing to run at all (it asserts on a tag-table mismatch —
/// getting that wrong yields zero emissions and reads exactly like a clean null).
fn tagged_source(table: &gtk::TextTagTable) -> gtk::TextBuffer {
    let src = gtk::TextBuffer::new(Some(table));
    src.set_text(PAYLOAD);
    let tag = gtk::TextTag::new(None);
    table.add(&tag);
    for (from, to) in SPANS {
        let a = src.iter_at_offset(from);
        let z = src.iter_at_offset(to);
        src.apply_tag(&tag, &a, &z);
    }
    src
}

fn bytes_of(buf: &impl IsA<gtk::TextBuffer>) -> String {
    let (s, e) = buf.as_ref().bounds();
    buf.as_ref().slice(&s, &e, true).to_string()
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x} ")).collect()
}

fn report(arm: &str, emissions: usize, got: &str) -> bool {
    let diags = DIAGNOSTICS.swap(0, Ordering::SeqCst);
    println!("\n  == {arm} ==");
    println!(
        "    emissions: {emissions} {}",
        if emissions > 1 {
            "(multi-run precondition MET)"
        } else {
            "*** SINGLE RUN — rig broken, verdict below is meaningless ***"
        }
    );
    println!("    diagnostics: {diags}");
    println!("    expected  {}", hex(PAYLOAD));
    println!("    actual    {}", hex(got));
    let order_kept = got.replace('\r', "\n") == PAYLOAD.replace('\r', "\n");
    println!(
        "    verdict: {}",
        if got == PAYLOAD {
            "byte-identical".to_string()
        } else if order_kept {
            "CRLF corrupted, document ORDER INTACT".to_string()
        } else {
            "*** SCRAMBLED — document order lost ***".to_string()
        }
    );
    assert!(emissions > 1, "{arm}: multi-run precondition not met");
    order_kept
}

fn main() {
    // Own the process's log handler so diagnostics are countable. This is what makes
    // the target `harness = false` rather than a body in the shared suite.
    glib::log_set_default_handler(|_domain, _level, _msg| {
        DIAGNOSTICS.fetch_add(1, Ordering::SeqCst);
    });
    gtk::init().expect("GTK init");
    sourceview::init();

    println!(
        "GTK {}.{}.{} / gtk4-rs 0.10 / {}",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version(),
        gtk::gdk::Display::default()
            .map(|d| d.type_().name().to_string())
            .unwrap_or_else(|| "no display".into())
    );

    // ── R1: connect_insert_text + stop_emission + nested insert ──────────────────
    let table = gtk::TextTagTable::new();
    let src = tagged_source(&table);
    let dst = gtk::TextBuffer::new(Some(&table));
    let r1_emissions = std::rc::Rc::new(Cell::new(0usize));
    let busy = std::rc::Rc::new(Cell::new(false));
    dst.connect_insert_text({
        let n = r1_emissions.clone();
        let busy = busy.clone();
        move |buf, iter, text| {
            n.set(n.get() + 1);
            if busy.get() || !has_lone_cr(text) {
                return;
            }
            let fixed = repair(text);
            glib::signal::signal_stop_emission_by_name(buf, "insert-text");
            busy.set(true);
            buf.insert(iter, &fixed);
            busy.set(false);
        }
    });
    let mut at = dst.start_iter();
    let (a, z) = src.bounds();
    dst.insert_range(&mut at, &a, &z);
    let r1_order_kept = report(
        "R1  connect_insert_text (signal)",
        r1_emissions.get(),
        &bytes_of(&dst),
    );

    // ── R2: TextBufferImpl::insert_text vfunc override ───────────────────────────
    let table2 = gtk::TextTagTable::new();
    let src2 = tagged_source(&table2);
    let dst2: RepairBuffer = glib::Object::builder()
        .property("tag-table", &table2)
        .build();
    let r2_emissions = std::rc::Rc::new(Cell::new(0usize));
    dst2.connect_insert_text({
        let n = r2_emissions.clone();
        move |_, _, _| n.set(n.get() + 1)
    });
    let mut at2 = dst2.start_iter();
    let (a2, z2) = src2.bounds();
    dst2.insert_range(&mut at2, &a2, &z2);
    let r2_order_kept = report(
        "R2  TextBufferImpl::insert_text (vfunc)",
        r2_emissions.get(),
        &bytes_of(&dst2),
    );

    // ── R3: does a BOUNDED-LENGTH insert reach the vfunc truncated? ──────────────
    // `insert_markup` funnels through `gtk_text_buffer_insert_with_attributes`
    // (gtktextbuffer.c:4928), which calls
    //     gtk_text_buffer_insert (buffer, iter, text + start, end - start)
    // — a bounded length into the MIDDLE of a longer string, so the byte at
    // `text[len]` is a live character rather than a NUL. The subclass trampoline
    // ignores its `_length` and reads `text_ptr` as a NUL-terminated `GString`
    // (gtk4-0.10.3/src/subclass/text_buffer.rs), so if that is unsound in the real
    // binding this arm inserts MORE text than GTK asked for.
    //
    // This codebase reaches no such caller today — `grep -rniE
    // "snippet|completion|insert_markup|insert_with_attributes|insert_with_tags"`
    // over `src/` returns only unrelated prose — so this is a guard on a door we
    // have not opened, not a live defect. It is here because the vfunc route is
    // being adopted and the caveat should be measured rather than remembered.
    let table3 = gtk::TextTagTable::new();
    let dst3: RepairBuffer = glib::Object::builder()
        .property("tag-table", &table3)
        .build();
    let mut at3 = dst3.start_iter();
    dst3.insert_markup(&mut at3, "AB<b>CD</b>EF");
    let got3 = bytes_of(&dst3);
    println!("\n  == R3  bounded-length insert via insert_markup (vfunc) ==");
    println!("    expected  {}  (\"ABCDEF\")", hex("ABCDEF"));
    println!("    actual    {}", hex(&got3));
    let bounded_ok = got3 == "ABCDEF";
    println!(
        "    verdict: {}",
        if bounded_ok {
            "the trampoline honoured the bounded length"
        } else {
            "*** OVER-READ — the trampoline ignored `len` and inserted trailing text ***"
        }
    );

    println!("\n  ── the claim under test ──");
    println!("    R1 keeps document order: {r1_order_kept}");
    println!("    R2 keeps document order: {r2_order_kept}");
    assert!(
        r2_order_kept,
        "the vfunc route was supposed to preserve document order; if this fails the \
         write-back read of subclass/text_buffer.rs is wrong and the recommendation \
         to move off connect_insert_text must be withdrawn"
    );
    println!("    R3 bounded-length insert correct: {bounded_ok}");
    println!("\n  vfunc route holds: it does not reorder the document.");
    if !bounded_ok {
        println!(
            "  CAVEAT CONFIRMED: adopt the vfunc, but never let a bounded-length\n  \
             caller (insert_markup / insert_with_attributes / a GtkSourceView\n  \
             snippet or completion) reach it until the binding is fixed upstream."
        );
    }
}
