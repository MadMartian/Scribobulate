//! Builds the read-only Markdown *preview* widget (the "preview"/"split" view
//! mode). Drives the low-level `renderer::Renderer` mechanics and assembles the
//! resulting `GtkTextView` inside a `GtkScrolledWindow`. The editor counterpart
//! lives in `window/`.
//!
//! ## File layout
//!
//! Decomposed from the former monolithic `preview.rs`; the crate-facing API
//! (`preview::render`, `preview::re_render`, `preview::css`, the scroll helpers, …)
//! is re-exported here so call sites are unchanged.
//!
//! * [`sourcemap`] — the pure, unit-tested selection→source waypoint helpers
//!   (`waypoint_src_offset`, `invert_source_map`, `finalize_source_map`).
//! * [`qdata`] — the [`RenderData`] per-render state struct and the typed
//!   `scrib_*` GLib-qdata accessors that store/read it on the view.
//! * [`cells`] — table-cell label/copymap helpers (`collect_cell_labels`,
//!   `attach_cell_copymaps`, `cell_search_targets`, …).
//! * [`build`] — the shared parse-and-render core: [`RenderProducts`] +
//!   `build_render_products` + `apply_preview_margins`.
//! * [`interactions`] — the buffer/gesture wiring split out of `render`
//!   (`connect_image_tints`, the table-click / link / copy-clipboard gestures).
//! * [`render`] — the public `render` (fresh view) and `re_render` (in-place swap).
//! * [`scroll`] — the scroll-position / restore helpers (validation-safe, GTK4Rs/AP-22/65).
//! * [`css`] — the preview/table/outline/tab-strip CSS.

pub(crate) mod annotate;
mod build;
mod cells;
mod css;
mod interactions;
mod qdata;
mod render;
mod scroll;
mod sourcemap;

pub(crate) use cells::{cell_copymap, cell_search_targets};
pub(crate) use css::{css, theme_css};
pub(crate) use interactions::{activate_link_url, link_url_at};
pub(crate) use qdata::{scrib_labels, scrib_render_data};
pub(crate) use render::{re_render, refresh_annotations_in_place, render};
pub(crate) use scroll::{
    preview_scroll_fraction, preview_top_line, restore_preview_scroll,
    restore_preview_scroll_to_line, restore_preview_scroll_to_line_fresh,
    scroll_preview_to_fragment, scroll_preview_to_heading,
};
