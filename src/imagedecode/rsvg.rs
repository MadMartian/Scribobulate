//! SVG decoding on Linux: librsvg called directly, never through gdk-pixbuf.
//!
//! **Why not gdk-pixbuf.** From gdk-pixbuf 2.44 (Ubuntu 26.04) gdk-pixbuf hands every
//! decode to glycin, which runs its own loaders, carries its own librsvg, ignores the
//! type a loader was created for (a loader made for `svg` decodes a PNG), and refuses
//! UTF-16 SVG. MEASURED on gdk-pixbuf 2.44.5 / glycin 2.1.5 (GTK4Rs/AP-374). So on Linux the decoder
//! behind an SVG depended on the distribution, and `svgencoding`'s rule that content
//! routed to the SVG decoder is only ever SVG did not hold. Calling librsvg here gives
//! one decoder on every Linux, the one the XInclude screen and `sdd/CVE.md` reason
//! about, and it reads UTF-16 itself (through libxml2's encoding detection).
//!
//! **Loaded at run time** (`dlopen` of `librsvg-2.so.2`), not linked, so the build needs
//! no librsvg development package. The library is what the distribution packages
//! already depend on. When it cannot be loaded, every SVG is refused, as an image with
//! no installed decoder always was.
//!
//! **Sizing matches the gdk-pixbuf loader librsvg ships**: the natural size is
//! `rsvg_handle_get_dimensions` (the size that loader reported in `size-prepared`), and
//! a document is rendered with `rsvg_handle_render_document` into a viewport of the
//! requested size. Only the API librsvg 2.52 (Ubuntu 22.04) has is used.

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib;
use gtk::glib::translate::FromGlibPtrFull;

#[repr(C)]
struct DimensionData {
    width: c_int,
    height: c_int,
    em: f64,
    ex: f64,
}

#[repr(C)]
struct Rectangle {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

type NewFromData =
    unsafe extern "C" fn(*const u8, usize, *mut *mut glib::ffi::GError) -> *mut c_void;
type GetDimensions = unsafe extern "C" fn(*mut c_void, *mut DimensionData);
type RenderDocument = unsafe extern "C" fn(
    *mut c_void,
    *mut gtk::cairo::ffi::cairo_t,
    *const Rectangle,
    *mut *mut glib::ffi::GError,
) -> glib::ffi::gboolean;

struct Api {
    new_from_data: NewFromData,
    get_dimensions: GetDimensions,
    render_document: RenderDocument,
}

const LIBRARY: &std::ffi::CStr = c"librsvg-2.so.2";

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: dlopen of a library by soname, kept open for the life of the process
        // (the handle is never closed), so every symbol read from it stays valid. Each
        // symbol is transmuted to the signature librsvg's public header declares for it.
        unsafe {
            let handle = libc::dlopen(LIBRARY.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if handle.is_null() {
                log::warn!("librsvg could not be loaded, so no SVG will be decoded");
                return None;
            }
            let symbol = |name: &std::ffi::CStr| {
                let address = libc::dlsym(handle, name.as_ptr().cast::<c_char>());
                (!address.is_null()).then_some(address)
            };
            Some(Api {
                new_from_data: std::mem::transmute::<*mut c_void, NewFromData>(symbol(
                    c"rsvg_handle_new_from_data",
                )?),
                get_dimensions: std::mem::transmute::<*mut c_void, GetDimensions>(symbol(
                    c"rsvg_handle_get_dimensions",
                )?),
                render_document: std::mem::transmute::<*mut c_void, RenderDocument>(symbol(
                    c"rsvg_handle_render_document",
                )?),
            })
        }
    })
    .as_ref()
}

/// A parsed SVG document.
pub(super) struct Document {
    api: &'static Api,
    handle: std::ptr::NonNull<c_void>,
}

impl Drop for Document {
    fn drop(&mut self) {
        // SAFETY: the handle is a GObject this value owns the only reference to.
        unsafe { glib::gobject_ffi::g_object_unref(self.handle.as_ptr().cast()) };
    }
}

impl Document {
    /// Parse `bytes` as SVG. The error is librsvg's, or says librsvg is missing.
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let api = api().ok_or("librsvg is not installed")?;
        let mut error = std::ptr::null_mut();
        // SAFETY: `bytes` is valid for its length for the duration of the call, which
        // copies what it keeps; `error` is a valid out-pointer.
        let handle = unsafe { (api.new_from_data)(bytes.as_ptr(), bytes.len(), &mut error) };
        match std::ptr::NonNull::new(handle) {
            Some(handle) => Ok(Self { api, handle }),
            None => Err(take_error(error, "librsvg could not read it as SVG")),
        }
    }

    /// The natural size in pixels: what librsvg's own gdk-pixbuf loader reports.
    pub(super) fn size(&self) -> (i32, i32) {
        let mut dims = DimensionData {
            width: 0,
            height: 0,
            em: 0.0,
            ex: 0.0,
        };
        // SAFETY: a live handle and a valid out-struct of the declared layout.
        unsafe { (self.api.get_dimensions)(self.handle.as_ptr(), &mut dims) };
        (dims.width, dims.height)
    }

    /// Render the whole document into a `w`×`h` image. The caller has checked the size
    /// against the pixel cap.
    pub(super) fn render(&self, w: i32, h: i32) -> Result<Pixbuf, String> {
        use gtk::cairo;

        let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)
            .map_err(|err| format!("no {w}×{h} surface could be made ({err})"))?;
        {
            let cr = cairo::Context::new(&surface).map_err(|err| err.to_string())?;
            let viewport = Rectangle {
                x: 0.0,
                y: 0.0,
                width: f64::from(w),
                height: f64::from(h),
            };
            let mut error = std::ptr::null_mut();
            // SAFETY: a live handle, a live cairo context, a valid viewport and a valid
            // error out-pointer.
            let drawn = unsafe {
                (self.api.render_document)(
                    self.handle.as_ptr(),
                    cr.to_raw_none(),
                    &viewport,
                    &mut error,
                )
            };
            if drawn == glib::ffi::GFALSE {
                return Err(take_error(error, "librsvg could not render it"));
            }
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().map_err(|err| err.to_string())?;
        Ok(pixbuf_from_argb32(&data, w, h, stride))
    }
}

fn take_error(error: *mut glib::ffi::GError, fallback: &str) -> String {
    if error.is_null() {
        return fallback.to_owned();
    }
    // SAFETY: librsvg set `error` and passed ownership of it to the caller.
    unsafe { glib::Error::from_glib_full(error) }.to_string()
}

/// Cairo's premultiplied, native-endian ARGB32 as gdk-pixbuf's straight RGBA8.
fn pixbuf_from_argb32(data: &[u8], w: i32, h: i32, stride: usize) -> Pixbuf {
    let (width, height) = (w as usize, h as usize);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in data.chunks(stride).take(height) {
        for px in row.chunks_exact(4).take(width) {
            let argb = u32::from_ne_bytes([px[0], px[1], px[2], px[3]]);
            let a = (argb >> 24) as u8;
            let unpremultiply = |c: u32| -> u8 {
                let c = (c & 0xff) as u16;
                match a {
                    0 => 0,
                    255 => c as u8,
                    a => ((c * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8,
                }
            };
            rgba.extend_from_slice(&[
                unpremultiply(argb >> 16),
                unpremultiply(argb >> 8),
                unpremultiply(argb),
                a,
            ]);
        }
    }
    Pixbuf::from_mut_slice(rgba, gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, w, h, w * 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_and_a_utf16_svg_read_the_same_size_and_render_it() {
        let plain = r##"<svg xmlns="http://www.w3.org/2000/svg" width="7" height="3"><rect width="7" height="3" fill="#00ff00"/></svg>"##;
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(plain.encode_utf16().flat_map(u16::to_le_bytes));
        for bytes in [plain.as_bytes(), &utf16] {
            let doc = Document::parse(bytes).expect("parses");
            assert_eq!(doc.size(), (7, 3));
            let pixbuf = doc.render(14, 6).expect("renders");
            assert_eq!((pixbuf.width(), pixbuf.height()), (14, 6));
            let px = pixbuf.read_pixel_bytes();
            assert_eq!(&px[..4], &[0, 255, 0, 255], "opaque green, unpremultiplied");
        }
    }

    #[test]
    fn content_that_is_not_svg_is_refused() {
        let png = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logo.png"),
        )
        .expect("logo.png");
        assert!(Document::parse(&png).is_err());
        assert!(Document::parse(b"").is_err());
    }

    #[test]
    fn half_transparent_ink_is_unpremultiplied() {
        // One pixel of 50% red, premultiplied: A=128, R=128.
        let argb: u32 = (128 << 24) | (128 << 16);
        let px = pixbuf_from_argb32(&argb.to_ne_bytes(), 1, 1, 4);
        assert_eq!(&px.read_pixel_bytes()[..4], &[255, 0, 0, 128]);
    }
}
