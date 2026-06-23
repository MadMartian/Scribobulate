//! Every icon name the app uses must resolve — a miss renders the broken-image
//! placeholder (ScrAP-39). This is the whole-set gate for that.
//!
//! WHY THIS TARGET OWNS ITS PROCESS (`harness = false`, see `Cargo.toml`). The
//! original reason — that `#[gtk::test]` bodies are dispatched onto a
//! `glib::ThreadPool` worker (`gtk4-0.10.3/src/lib.rs:60-87`, `test_synced`) and so
//! abort where GTK requires main-thread initialisation — is now handled generally by
//! `src/gtk_suite.rs`, which runs every `#[gtktest::test]` body on the process main
//! thread. So this file is no longer the portable *shape*; it is a deliberate
//! exception, and the reason is isolation rather than threading:
//!
//! **This audit's subject is process-global state.** It registers a GResource and an
//! icon-theme search path, then asserts every name the app uses resolves. Sharing a
//! process with 147 other bodies makes that answer order-dependent, and wrong in the
//! dangerous direction — a name resolving because a *sibling* body added something to
//! the search path, while the shipped app misses it and renders the broken-image
//! placeholder. GTK cannot be un-initialised, so the only isolation available is a
//! separate process. Its `--render` evidence mode also needs its own argv, which a
//! shared runner owns.
//!
//! Do not "tidy" this into the suite. For an ordinary GTK assertion, write
//! `#[gtktest::test]` in `src/` instead; see POLICY's testing section.
//!
//! Run:
//!   `xvfb-run -a cargo test --features gtk-integration-tests` (whole suite)
//!   `cargo test --features gtk-integration-tests --test icon_resolution`
//!
//! Non-zero exit lists the offending names. Passing `--render <DIR>` also
//! rasterizes every name to `<DIR>/<name>.png` and reports the file each one
//! actually resolved to: `has_icon` proves only that a name RESOLVES, and it is
//! *stricter* than the render path, which strips the `-symbolic` suffix and can
//! land on a legacy raster that renders but no longer recolours
//! (ScrAP-169). The rendered PNG plus the resolved source is the evidence; the
//! boolean is only the cheap screen.

// The icon-name table is the single source of truth in `src/icons.rs`, pulled in as a
// module by path. `#[path]` (not `include!`) because `src/icons.rs` opens with `//!`
// inner doc comments, and a macro expansion may not introduce inner attributes.
//
// The crate IS a library now, but its module tree is `pub(crate)` and a `pub use`
// cannot widen that (E0364), so an external test target still cannot import `icons`.
// The main-thread suite in `src/gtk_suite.rs` could reach it — this target
// deliberately does not join that suite, for the reason in the header below.
#[path = "../src/icons.rs"]
mod icons;

/// The audited set comes from the ENUM, not from a list.
///
/// It used to walk `icons::ALL` and `icons::GLYPH_FALLBACK` — two hand-maintained
/// arrays with nothing tying them to the variants they partitioned. MEASURED (QA
/// round 5, M-5; ScrAP-216): adding a 50th `Icon` variant with an unresolvable name
/// left this target printing *"audited 49 names"* and exiting 0. An audit whose
/// population is a list somebody remembers to update checks existence where
/// correspondence matters, and it fails silent — the one direction an audit must
/// never fail.
///
/// `Icon::every()` is derived from an exhaustive `match`, so a new variant cannot
/// compile without joining it, and `Icon::resolution` is the exhaustive partition
/// that replaced the two arrays.
use icons::{Icon, Resolution};

/// The app's own GResource icon prefix — the one `app::setup` registers. Used to
/// tell the app's bundled art apart from GTK's built-in resource-served icons,
/// which live under a different prefix but look identical in a `resource://` URI.
const APP_RESOURCE_PREFIX: &str = "/com/extollit/scribobulate/icons";

fn main() {
    gtk::init().expect("GTK init on the process main thread");

    // Register the bundled GResource + icon-theme resource path exactly as
    // `app::setup::init_resources_and_css` does, so the bundled fallbacks
    // (copy-document / expand-all / collapse-all) resolve here as at runtime.
    gtk::gio::resources_register_include!("scribobulate.gresource")
        .expect("bundled GResource registers");
    let display = gtk::gdk::Display::default().expect("a default display");
    let theme = gtk::IconTheme::for_display(&display);
    theme.add_resource_path(APP_RESOURCE_PREFIX);

    println!("icon theme: {:?}", theme.theme_name());
    println!("search path: {:?}\n", theme.search_path());

    let audited = Icon::every().count();
    let mut hard_misses = Vec::new();
    let mut soft_misses = Vec::new();
    for icon in Icon::every() {
        let name = icon.name();
        if theme.has_icon(name) {
            continue;
        }
        match icon.resolution() {
            Resolution::MustResolve => hard_misses.push(name),
            Resolution::GlyphFallback => soft_misses.push(name),
        }
    }

    println!(
        "audited {} names: {} resolve, {} miss with a glyph fallback, {} MISS with NO fallback",
        audited,
        audited - hard_misses.len() - soft_misses.len(),
        soft_misses.len(),
        hard_misses.len()
    );
    if !soft_misses.is_empty() {
        println!("\nmissing, glyph-fallback covers them:");
        for n in &soft_misses {
            println!("  {n}");
        }
    }
    if !hard_misses.is_empty() {
        println!("\nMISSING, WOULD RENDER THE BROKEN-IMAGE PLACEHOLDER:");
        for n in &hard_misses {
            println!("  {n}");
        }
        std::process::exit(1);
    }
    println!("\nall no-fallback icon names resolve.");

    let mut args = std::env::args().skip(1);
    if let Some(dir) = args
        .find(|a| a == "--render")
        .and_then(|_| std::env::args().nth(2))
    {
        render_all(&theme, &dir);
    }
}

/// Rasterize every audited name and report which FILE each one resolved to.
/// Uses the GSK Cairo renderer — the same software renderer the app ships on.
fn render_all(theme: &gtk::IconTheme, dir: &str) {
    use gtk::{gdk, gsk, prelude::*};

    std::fs::create_dir_all(dir).expect("render dir");
    let renderer = gsk::CairoRenderer::new();
    renderer
        .realize(None::<&gdk::Surface>)
        .expect("cairo renderer realizes");

    println!("\nrendering to {dir}/:");
    for icon in Icon::every() {
        let name = icon.name();
        let paintable = theme.lookup_icon(
            name,
            &[],
            128,
            1,
            gtk::TextDirection::Ltr,
            gtk::IconLookupFlags::empty(),
        );
        // Where the lookup actually landed. Read the URI, NOT `path()`:
        // `path()` is None for anything served from a GResource, so a working
        // bundled icon would print blank and read as a failure.
        //
        // And a `resource://` URI alone does not mean it is OURS — GTK serves
        // its own built-in icons from /org/gtk/libgtk/icons/, which would
        // otherwise masquerade as the app's bundle and make this audit pass for
        // the wrong reason. So the app's own resource prefix is matched
        // explicitly and labelled distinctly from any other resource source.
        let source = match paintable.file().map(|f| f.uri()) {
            Some(uri) if uri.contains(APP_RESOURCE_PREFIX) => format!("<app gresource> {uri}"),
            Some(uri) if uri.starts_with("resource://") => format!("<GTK built-in> {uri}"),
            Some(uri) => uri.to_string(),
            None => "<no source — lookup returned no file>".to_string(),
        };

        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, 128.0, 128.0);
        match snapshot.to_node() {
            Some(node) => {
                let texture = renderer.render_texture(&node, None);
                let path = format!("{dir}/{name}.png");
                texture.save_to_png(&path).expect("write png");
                println!("  {name:38} <- {source}");
            }
            None => println!("  {name:38} <- {source}  (EMPTY RENDER — no pixels!)"),
        }
    }
}
