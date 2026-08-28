//! Shared oracles for the preview's paint tests — render a `CodePreviewView`
//! off-screen and read its framebuffer back.
//!
//! Test-only, and carrying its callers' cfg rather than a bare `#[cfg(test)]`:
//! every reader is a `gtk-integration-tests` body, so under a plain `cargo test`
//! these would be dead code reported by pipeline step 4 and invisible to step 2
//! (`sdd/POLICY.md` § GTK-object integration tests).
//!
//! It exists because the helpers were private to one test module and a second one
//! wanted them. Copying them would have been the cheaper edit and the wrong one:
//! `framebuffer_of` was already extracted once from four verbatim copies, and a
//! fifth copy is how a paint branch ends up untested (`F-SPRITEPAINT-001`).

use super::CodePreviewView;
use gtk::prelude::*;
use gtk::{gdk, gsk};

/// Render `view` off-screen and hand back its framebuffer as ARGB32 bytes.
///
/// The realize/unrealize pair is not optional: dropping a realized `CairoRenderer`
/// does NOT honour the object's teardown contract, and on a build with GLib
/// assertions compiled in that is a hard abort rather than a leak (GTK4Rs/AP-272).
pub(super) fn framebuffer_of(view: &CodePreviewView, w: f64, h: f64) -> Vec<u8> {
    let paintable = gtk::WidgetPaintable::new(Some(view));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(snapshot.upcast_ref::<gdk::Snapshot>(), w, h);
    let node = snapshot
        .to_node()
        .expect("the preview snapshots to something");
    let renderer = gsk::CairoRenderer::new();
    renderer
        .realize(None::<&gdk::Surface>)
        .expect("the Cairo renderer realizes without a surface");
    let texture = renderer.render_texture(&node, None);
    let (tw, th) = (texture.width() as usize, texture.height() as usize);
    let mut data = vec![0u8; tw * th * 4];
    texture.download(&mut data, tw * 4);
    renderer.unrealize();
    data
}

/// Whether `want` appears anywhere in an ARGB32 framebuffer.
///
/// Cairo ARGB32 on a little-endian host is B, G, R, A.
pub(super) fn contains_rgb(data: &[u8], want: (u8, u8, u8)) -> bool {
    data.chunks_exact(4).any(|px| (px[2], px[1], px[0]) == want)
}

/// Which scanlines of a `w`-pixel-wide ARGB32 framebuffer carry `want` anywhere.
///
/// The row-scoped half of the ordering oracle. A bare [`contains_rgb`] settles a pair
/// whose upper decoration's rectangle is wholly inside the lower one's — cover it and
/// the colour disappears from the frame entirely. Where the two rectangles merely
/// intersect (a blockquote's accent bar runs the whole quote while a heading band
/// covers only its heading's rows), the covered colour survives on the rows outside the
/// intersection and presence alone cannot see the swap; the assertion has to be made
/// per row.
pub(super) fn rows_with(data: &[u8], want: (u8, u8, u8), w: usize) -> Vec<bool> {
    data.chunks_exact(w * 4)
        .map(|row| row.chunks_exact(4).any(|px| (px[2], px[1], px[0]) == want))
        .collect()
}

/// An 8x8 PNG whose LEFT HALF is `rgba` and whose right half is fully clear.
///
/// **Half transparent, and that is the whole discriminating power of every test
/// that uses it.** An opaque tile hides an over-paint completely — the blockquote
/// bar's first fixture was opaque, and the mutation it was written to catch
/// passed. With half of every tile clear, a decoration that filled before it tiled
/// shows the flat colour through, and one that replaced shows the page.
pub(super) fn write_half_clear_tile(path: &std::path::Path, rgba: u32) {
    let pb = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 8, 8)
        .expect("allocate pixbuf");
    pb.fill(0x00_00_00_00);
    pb.new_subpixbuf(0, 0, 4, 8).fill(rgba);
    pb.savev(path, "png", &[]).expect("save png");
}

/// Map, pump and present a window holding `view`, and return it so the caller can
/// destroy it.
pub(super) fn present_for_paint(view: &CodePreviewView) -> gtk::Window {
    present_for_paint_sized(view, 400, 200)
}

/// [`present_for_paint`] at an explicit size, for a fixture that needs the room.
pub(super) fn present_for_paint_sized(view: &CodePreviewView, w: i32, h: i32) -> gtk::Window {
    let window = gtk::Window::new();
    window.set_default_size(w, h);
    window.set_child(Some(view));
    window.present();
    crate::testpump::until(crate::testpump::Clock::Frame, "the preview maps", || {
        view.width() > 0
    });
    window
}
