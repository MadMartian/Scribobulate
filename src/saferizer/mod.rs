//! `saferizer` — project-local **typed seams** that promote GTK's implicit,
//! prose-only runtime contracts into compile-time ones.
//!
//! GTK's safety contracts are temporal and relational, and its C type system
//! encodes none of them; gtk-rs, a deliberately *thin* binding, passes them
//! through unenforced. Each submodule here wraps one such **ownership /
//! construction** contract in a type whose only safe path is the correct one —
//! a newtype with a single constructor, an RAII guard, or a phantom-typed key —
//! so the wrong call becomes *unrepresentable* rather than merely *documented*.
//! Where a type cannot hide an inherent GTK method, the raw call is banned
//! crate-wide via `clippy.toml`, and the sole `#[allow(clippy::disallowed_methods)]`
//! lives inside the wrapper.
//!
//! Reference shape: [`crate::winstate::selfdelete::SelfDeleteGuard`]. The rules for
//! adding a seam — and which contracts are type-shaped at all versus stay prose —
//! live in `sdd/POLICY.md`'s typed-GTK-seams policy.
//!
//! **Rustdoc rule for every seam here:** document the *contract* — what the type
//! guarantees, the precondition a caller must satisfy, and what a fallback / `None`
//! means — plus at most a one-line "why". Never the derivation.

// Seams are declared here as they land (one focused file + unit tests each).

pub(crate) mod buffer_text;
pub(crate) use buffer_text::BufferText;

pub(crate) mod qdata_key;

pub(crate) mod native_dialog;

pub(crate) mod buffer_mark;

pub(crate) mod click_activation;
pub(crate) use click_activation::ClickActivation;

pub(crate) mod viewport;

pub(crate) mod persistent_popover;
pub(crate) use persistent_popover::PersistentPopover;

pub(crate) mod popover_anchor;
pub(crate) use popover_anchor::{on_viewport, pin_above, pointing_to, Viewport, ViewportRect};
