//! The registry `#[gtktest::test]` submits into, and the suite runner reads.
//!
//! Compiled into two crates, by design:
//!
//! * the ordinary **lib test target** (`cargo test`), where the submissions exist but
//!   nothing iterates them — libtest runs the bodies through `#[gtk::test]` as before;
//! * the **suite crate** (`src/gtk_suite.rs`, a `harness = false` target), where
//!   `main()` iterates this registry and calls each body on the process main thread.
//!
//! Hence the `cfg(all(test, …))` on its declaration in both crate roots: a
//! `harness = false` target is built `--cfg test`, so one gate covers both, and the
//! shipped library never contains it.

/// One registered GTK test body.
///
/// `run` is a bare `fn()` rather than a boxed closure so a submission is a `const`
/// the linker can place in the registry section with no runtime construction — which
/// is what lets bodies register from 37 files without a central list to maintain.
///
/// `allow(dead_code)` is narrow and load-bearing, not a papering-over: both fields
/// are *written* in every crate that compiles this file and *read* only in the suite
/// crate (`src/gtk_suite.rs`). In the ordinary lib test target the submissions exist
/// so that one attribute can serve both harnesses, but nothing iterates them there —
/// libtest runs the bodies directly — so `name`/`run` are genuinely never read in
/// that compilation. Scoped to this struct deliberately; the crate-wide allow that
/// would also silence it belongs only in the throwaway suite root.
#[allow(dead_code)]
pub(crate) struct Case {
    /// `module::path::function_name`, from `module_path!()` at the body's own site.
    pub(crate) name: &'static str,
    pub(crate) run: fn(),
    /// The body carried `#[ignore]`. libtest honours that attribute itself; this
    /// field is how the same instruction reaches THIS runner, which has no libtest
    /// underneath it and would otherwise run a quarantined body. One attribute must
    /// mean one thing under both harnesses, or `#[ignore]` becomes advice.
    pub(crate) ignored: bool,
    /// The body carried `#[should_panic]`. libtest inverts its verdict for such a
    /// body itself; this field is how the same instruction reaches THIS runner, which
    /// has no libtest underneath it and would otherwise report a caught panic as
    /// FAILED. One attribute must mean one thing under both harnesses, or
    /// `#[should_panic]` becomes advice.
    pub(crate) should_panic: bool,
}

inventory::collect!(Case);
