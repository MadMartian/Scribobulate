//! Per-platform seams, one module per target OS — a directory where the platform
//! needs more than one file, a single file where it does not.
//!
//! Each child is `#[cfg]`-gated **at its declaration** here and never internally,
//! so a build for another platform compiles none of it and the `-D warnings`
//! clippy gate stays satisfiable with no `#[allow]` (the same rule `workaround`
//! follows).
//!
//! Declaring them here rather than at the crate root is what makes that gating a
//! single fact: `lib.rs` and `gtk_suite.rs` are two crate roots that must otherwise
//! re-declare every module, and a platform module missing from the second silently
//! drops its whole test surface from the main-thread suite.
//!
//! The bar for putting something here is narrow: it must be work the toolkit does
//! for us elsewhere and does not do on this platform. Anything that changes what
//! the app *does* belongs in the shared code, behind a capability, so the
//! platforms cannot drift in behaviour — only in the plumbing underneath.

#[cfg(target_os = "macos")]
pub(crate) mod mac;

#[cfg(windows)]
pub(crate) mod win32;
