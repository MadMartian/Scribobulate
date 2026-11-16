//! Per-render memory-growth gating (TDD 6.6–6.12).
//!
//! Two halves of one class, catching disjoint failures:
//!
//! * [`growth`] — after discarding warm-up, a sample series may step up at most
//!   once and must retain no more than a per-platform ceiling. Growth over many
//!   repetitions is the only honest shape: a single-shot "render, free, assert
//!   the number came back" cannot pass on a correct implementation, because
//!   freed pages stay with the allocator on every platform this project
//!   delivers to.
//! * [`footprint`] — one sampler, three `cfg` bodies. The field is named
//!   **footprint**, never RSS: `/proc` VmRSS, macOS `ri_phys_footprint` and
//!   Windows `PrivateUsage` (less the heaps' free committed bytes) are not the same
//!   quantity. [`instrument`] holds the reading to a known leak and a known free.
//!
//! The GTK drivers that *produce* the samples live in [`gtk`] (6.6–6.9, and
//! 6.7's still-image half) and [`playback`] (6.10, and 6.7 extended to the
//! animation state), compiled only under
//! the `memory-gates` feature, so pipeline step 5 never runs either. The
//! decision cores here run as ordinary unit tests (step 4) and stay inside
//! the coverage ratchet.

pub(crate) mod footprint;
pub(crate) mod growth;

#[cfg(all(test, feature = "memory-gates"))]
mod gtk;
// Windows only: the free-heap correction it proves is the Windows reading's, and step 5b
// refuses a run that prints a skip.
#[cfg(all(test, feature = "memory-gates", windows))]
mod instrument;
#[cfg(all(test, feature = "memory-gates"))]
mod playback;

#[cfg(all(test, feature = "memory-gates"))]
mod window;
