//! The theme engine's behavioural tests, split by the subject each one pins.
//!
//! They live in their own modules rather than one block at the foot of the engine
//! because they are the engine's largest single body of text, and a reader looking
//! for "what does a heading key promise?" should not have to walk the list-marker
//! cases to find out.

mod contrast;
mod diagnostics;
mod headings;
mod lists;
mod markers;
mod merge;
mod registry;
mod searchpath;
/// Gated on the integration feature because its preview probe needs a live
/// `GtkTextTagTable` — and because a helper that exists only for a feature-gated test
/// must carry that feature's cfg, or a plain `cargo test` reports it as dead code
/// (`sdd/POLICY.md` § GTK-object integration tests).
#[cfg(feature = "gtk-integration-tests")]
mod sinks;
mod sprites;
mod system;
