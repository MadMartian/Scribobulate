//! The memory gates' instrument against the two things it must tell apart (TDD 6.13): a
//! leak, which it must count, and memory the allocator keeps after a free, which it must
//! not.
//!
//! Compiled only on Windows, whose reading the correction belongs to (VmRSS and the macOS
//! physical footprint are read as the kernel reports them), and only under
//! `--features memory-gates`, beside the gates it serves: the reading
//! is process-wide, so it is taken in the suite child like theirs (see
//! [`super::footprint::measuring`]).

use crate::memgate::footprint::{current, measuring};
use std::hint::black_box;

/// Small blocks per allocator in the leak: enough that the leak dwarfs the noise of the
/// reading itself, small enough to take well under a second.
const LEAK_BLOCKS: usize = 20_000;
const LEAK_BLOCK_BYTES: usize = 100;
/// Large blocks in the leak: past the heap's own block limit (about 512 KB on x64), so the
/// heap hands each one to `VirtualAlloc` rather than to a segment.
const LEAK_LARGE_BLOCKS: usize = 4;
const LEAK_LARGE_BYTES: usize = 1024 * 1024;

/// Allocated and freed before the leak: the churn whose freed memory the heap keeps.
const CHURN_BLOCKS: usize = 400_000;

/// Leak `LEAK_BLOCKS` small blocks through Rust's allocator and as many through `g_malloc`,
/// plus the large ones, touching every byte. Returns the bytes leaked.
///
/// `black_box` is what keeps the leak: without it a release build sees that the pointer
/// never escapes and elides the allocation outright (MEASURED: a control without it read
/// exactly 0 bytes for its Rust half and exactly the leak for its `g_malloc` half).
fn leak() -> u64 {
    for _ in 0..LEAK_BLOCKS {
        std::mem::forget(black_box(vec![1u8; LEAK_BLOCK_BYTES]));
        // SAFETY: `g_malloc` aborts rather than returning null, and the block is written
        // within its size and never freed, which is the point.
        unsafe {
            let block = gtk::glib::ffi::g_malloc(LEAK_BLOCK_BYTES);
            std::ptr::write_bytes(black_box(block).cast::<u8>(), 1, LEAK_BLOCK_BYTES);
        }
    }
    for _ in 0..LEAK_LARGE_BLOCKS {
        std::mem::forget(black_box(vec![1u8; LEAK_LARGE_BYTES]));
    }
    (2 * LEAK_BLOCKS * LEAK_BLOCK_BYTES + LEAK_LARGE_BLOCKS * LEAK_LARGE_BYTES) as u64
}

/// Allocate many small blocks of mixed sizes, free every other one so the heap fragments,
/// then free the rest. Nothing is kept.
fn churn() {
    let mut blocks: Vec<Vec<u8>> = (0..CHURN_BLOCKS)
        .map(|i| black_box(vec![1u8; 48 + (i % 7) * 24]))
        .collect();
    let mut odd = 0usize;
    blocks.retain(|_| {
        odd += 1;
        odd.is_multiple_of(2)
    });
    drop(black_box(blocks));
}

/// TDD 6.13's instrument: the footprint a memory gate judges rises by a leak, through
/// either allocator and in large blocks as in small, and does not rise by memory that was
/// freed. On Windows the second half is the reason the reading subtracts the heaps' free
/// committed bytes (`platform::win32::process::current_allocated_private_bytes`): the heap
/// keeps freed memory committed, and `PrivateUsage` alone read a window cycle's churn as
/// growth.
///
/// MEASURED, release build, Windows seat: the churn reads +0.08 to +0.18 MiB (5 runs) and
/// the leak +7.84 to +7.86 MiB for 7.81 MiB. Mutation: `PrivateUsage` alone reads the churn
/// as +1.17 to +3.65 MiB and fails 5 runs in 5.
#[gtktest::test]
fn the_footprint_counts_a_leak_and_not_a_freed_block_ttd_6_13() {
    let Some(_measuring) = measuring("6.13") else {
        return;
    };
    let mib = |b: i64| b as f64 / (1024.0 * 1024.0);

    let before_churn = current().expect("footprint");
    churn();
    let after_churn = current().expect("footprint");
    let kept = after_churn as i64 - before_churn as i64;
    println!("[memgate instrument] churn: {:+.2} MiB", mib(kept));
    assert!(
        // Between the two readings' measured ranges, with a margin of three or more to the
        // correct one and nearly two to the mutant.
        kept < 640 * 1024,
        "TDD 6.13: allocating and freeing {CHURN_BLOCKS} blocks moved the footprint by \
         {:+.2} MiB; a freed block the heap keeps committed is being counted as growth",
        mib(kept)
    );

    let before_leak = current().expect("footprint");
    let leaked = leak();
    let after_leak = current().expect("footprint");
    let grew = after_leak as i64 - before_leak as i64;
    println!(
        "[memgate instrument] leak of {:.2} MiB: {:+.2} MiB",
        mib(leaked as i64),
        mib(grew)
    );
    assert!(
        // MEASURED: +7.84 to +7.86 MiB for 7.81 MiB. Ten percent each way
        // leaves room for the reading's own noise and none for a leak counted twice.
        grew >= (leaked as i64) * 9 / 10 && grew <= (leaked as i64) * 11 / 10,
        "TDD 6.13: a leak of {:.2} MiB moved the footprint by {:+.2} MiB; the gate's reading \
         must see a leak, and see it once",
        mib(leaked as i64),
        mib(grew)
    );
}
