//! Markdown formatting commands, the Insert Link/Image/Table dialogs, and the
//! Format toolbar section + caret formatting overlay.
//!
//! Decomposed from the former monolithic `window/editbar.rs` into focused
//! submodules:
//!   - [`edit`]      — the buffer-edit primitives (`apply_edit`/`apply_format`/
//!     `splice_markup`/`selection_range_and_text`)
//!   - [`newline`]   — the Enter conveniences (`wire_newline_edits`, list/quote
//!     continuation + code-fence auto-close, mapping `format::*` to buffer edits;
//!     driven by a capture-phase key controller, never an `insert-text` hook —
//!     ScrAP-199)
//!   - [`dialog`]    — the modal `input_form` + file-chooser Browse (`BrowseSpec`)
//!   - [`insert`]    — the Tier-2 commands (`insert_link`/`insert_image`/
//!     `insert_table`) and `go_to_line`
//!   - [`formatbar`] — the Format toolbar/overlay row (`build_format_bar`, `format_button`)
//!   - [`relabel`]   — the Insert↔Edit surface relabel (`update`/`resync_format_edit_surfaces`)
//!   - [`focusgate`] — the sticky editor-focus gate for `win.format`/`win.go-to-line`
//!   - [`overlay`]   — the single per-window caret formatting overlay (GTK4Rs/AP-106)
//!
//! Submodules reach each other's editbar-internal `pub(super)` helpers through the
//! private globs below; the `pub(crate) use` re-exports expose the window-facing
//! surface to the rest of the `window` module (`window/mod.rs`'s `use editbar::*`).

mod dialog;
mod edit;
mod focusgate;
mod formatbar;
mod insert;
mod newline;
mod overlay;
mod relabel;

// Intra-subsystem sibling resolution: a submodule's `use super::*` (super = this
// module) reaches its siblings' editbar-internal `pub(super)` helpers — the ones
// NOT re-exported below — through these private globs:
//   edit   → selection_range_and_text, splice_markup
//   dialog → input_form, BrowseSpec
use dialog::*;
// Re-exported to the whole `window` module: `window::rename` runs the same modal
// form, with a live validator gating its confirm control (TDD 24.9). Extending the
// shared form was preferred over a second dialog helper (POLICY: prefer extending an
// existing code path) — the two would have drifted on focus, Escape and leak-safety.
pub(in crate::window) use dialog::{input_form, FieldExtras, LiveValidate};
use edit::*;

// The window-facing surface (brought into `window` scope by `window/mod.rs`'s
// `use editbar::*`, and reachable qualified as `editbar::…`).
pub(crate) use edit::apply_format;
pub(crate) use focusgate::setup_editor_focus_gate;
pub(crate) use formatbar::build_format_bar;
pub(crate) use insert::{go_to_line, insert_image, insert_link, insert_table};
pub(crate) use newline::wire_newline_edits;
pub(crate) use overlay::{build_format_overlay, wire_editor_format_overlay};
pub(crate) use relabel::{resync_format_edit_surfaces, update_format_edit_surfaces};
