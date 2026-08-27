//! Custom GTK4 widgets built for Scribobulate.
//!
//! This is the home for every hand-written `GtkWidget` subclass (and the plain
//! Rust façades that pair with them). Each widget lives in its own submodule
//! directory so its GObject glue, its pure decision/layout arithmetic (a
//! GTK-free, unit-tested `layout` module — see POLICY §coverage gate), and any
//! façade split across focused files rather than one monolith:
//!
//! - [`table`] — `ScribTableWidget`, the churn-free anchored Markdown-table
//!   widget (a `ConstantSize` widget with cached cell rects; see GTK4Rs/AP-23).
//! - [`tab`] — the `GtkNotebook`-free tab strip (`TabBar` + the `TabView`
//!   façade); kills the GTK4Rs/AP-60 crash class and adds per-tab close/context-menu.
//! - [`comment_entry`] — `CommentEntry`, the single annotation comment
//!   entry + Save pair shared by all three annotation surfaces, wiring every
//!   commit route once.
//! - [`rule`] — `SpriteRule`, the horizontal rule when a theme tiles a sprite across
//!   it (TDD 18.31). Built only where the theme states one; the flat rule stays the
//!   stock `GtkSeparator` it has always been.
//! - [`textfield`] — the constructors every `GtkEntry`/`GtkSearchEntry` in the
//!   application comes from, so the two silent follow-ups a hand-built field owes
//!   (accessible name; macOS word navigation) cannot be forgotten one surface at a
//!   time.

use gtk::prelude::*;

pub(crate) mod comment_entry;
pub(crate) mod rule;
pub(crate) mod tab;
pub(crate) mod table;
pub(crate) mod textfield;

/// Unparent every child of a custom `GtkWidget` subclass.
///
/// **Contract:** a custom widget that parents its children with `set_parent`
/// (rather than delegating to a layout manager / container that owns them) must
/// unparent every child in its `dispose`. GTK does **not** do this automatically
/// for custom widget subclasses; skipping it leaks the children and emits
/// finalize-time warnings. Call this once from `dispose`.
pub(crate) fn unparent_all_children(widget: &impl IsA<gtk::Widget>) {
    while let Some(child) = widget.first_child() {
        child.unparent();
    }
}
