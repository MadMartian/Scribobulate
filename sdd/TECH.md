# Technical Architecture

Scribobulate is a native GTK4 desktop application written in Rust. It renders
Markdown into real GTK widgets drawn on the CPU (no HTML engine, no GPU
pipeline), giving it the resource footprint of a native editor. See [PRODUCT.md](PRODUCT.md)
for what the application does and why.

The stack was chosen by measured viability spikes, not assumption: native
widgets + the GSK Cairo renderer hold **0 MiB VRAM and ~80 MiB RAM** for a
single document with full-fidelity rendering. The rejected alternatives
(GPU-canvas toolkits, WebKitGTK) and why they failed are GTK4Rs/AP-1 and GTK4Rs/AP-2.

## System overview

![System overview: one single-threaded GtkApplication process; each Document Window carries per-window chrome plus a TabView of tabs, each tab one document (TabState) whose SplitView holds a GtkSourceView source pane and a CodePreviewView preview pane; Markdown source flows through CriticMarkup extraction, pulldown-cmark, the Renderer and preview::build_render_products into the preview widget; the file on disk is read and written through docio on GLib's I/O thread pool (writes via atomic_io) and watched by a gio::FileMonitor whose every event is answered by a re-read and whose pure decision core chooses ignore, conflict-toast, reload, or — only once a settle re-read confirms it — keeping the buffer because the file was deleted or emptied; and a second, display-free consumer of the same event stream builds one ExportDoc that two sinks turn into an HTML file or a paginated PDF.](system-overview.svg)

GSK is configured to use the **Cairo software renderer** (`GSK_RENDERER=cairo`,
set in-process before GTK initialises), so the application never holds a GL/GLES
context on any platform. On Linux it does not appear as a GPU client at all;
macOS (Quartz) and Windows (DWM) composite every window through the GPU, so a
process there carries a small **fixed** GPU figure — which is why TDD 6.4 and 6.5
gate on independence from window area and document complexity rather than a zero
reading.

## Third-party components

### Rust crates

| Package | Version | Role |
|---------|---------|------|
| `gtk4` (`gtk`) | 0.10 | GTK4 bindings — windows, widgets, application/single-instance. Built with the `v4_6` feature, which *matches* the GTK floor in POLICY § Build rather than raising it: the binding gates each API behind the version that introduced it, so an API from 4.6 is otherwise absent from the crate and its absence reads as a type error (GTK4Rs/AP-94). `GdkTexture::from_bytes` — how a fetched remote image becomes a texture — is one of those. Never raise this past the documented floor: an above-floor wrapper compiles and fails at link/runtime instead (GTK4Rs/AP-114). |
| `sourceview5` (`sourceview`) | 0.10 | GtkSourceView 5 — the editable Source pane (Markdown syntax highlighting, undo). |
| `pulldown-cmark` | 0.13 | CommonMark/GFM parser; emits the event stream the renderer walks. Extensions are an explicit allowlist, not `Options::all()` (ScrAP-78); superscript, subscript, strikethrough and highlight are tokenised in-crate by `renderer::scan_script_spans` instead (ScrAP-66, ScrAP-195). |
| `serde` | 1 | Serialization framework — `#[derive(Deserialize)]` for the `Config` struct. |
| `syntect` | 5 | Pure-Rust syntax highlighter — colors code blocks inside the `GtkTextView` buffer via per-token foreground tags. `default-fancy` (fancy-regex, no C dep); its syntax set is supplied by `two-face`. |
| `two-face` | 0.5 | Supplies syntect's syntax set from bat's bundled grammars — adds TypeScript/TOML etc. that syntect's defaults omit (ScrAP-160). `syntect-fancy` feature (no C dep); grammar licences reproduced in `THIRD-PARTY-LICENSES.md`. |
| `toml` | 0.8 | TOML parser for `~/.config/scribobulate/config.toml`. |
| `ureq` | 3.4 | The HTTP(S) GET behind a remote image on the opt-in "Show Unsafe Images" path (`imagefetch.rs`). Added because the obvious dependency-free route — `gio::File::for_uri` — resolves an `https://` URI only where a **GVfs** backend claims the scheme (`gvfsd-http`), which is a Linux-desktop component with no macOS build, so the feature was silently dead there (GTK4Rs/AP-292). `default-features = false` + `rustls`/`platform-verifier`/`win-system-proxy`: the `ring` provider (no OpenSSL, no build tool beyond the C compiler), no gzip/json/cookies, and verification against the **machine's own trust store** with the Windows per-user proxy honoured — because the routes this replaces already did both (glib-networking on Linux, GLib's in-process `GWinHttpVfs` on Windows), so bundled roots would have regressed corporate desktops on two platforms to fix a third. |
| `log` | 0.4 | Logging facade — the single sink for app + (bridged) GTK/glib diagnostics. |
| `env_logger` | 0.11 | `log` backend; `RUST_LOG` runtime level/module control. See `logging.rs`. |
| `glib` | 0.21 | Re-exported by `gtk4`; depended on directly only to enable its `log` feature (the glib→`log` writer bridge). |
| `dunce` | 1 | `canonicalize` that resolves symlinks/`..` (and strips Windows `\\?\` verbatim) — the image-`src` containment gate (`links::resolve_contained_image`) and the Browse→relative path math. |
| `pathdiff` | 0.2 | `diff_paths` — the document-relative reference inserted by the Insert Link/Image Browse buttons (`links::relativize_for_insert`). |
| `pangocairo` | 0.21 | The one sanctioned way to draw a Pango layout onto a cairo surface, which is what the PDF export sink does. `pango_cairo_show_layout_line` — never a per-run `show_glyph_string` loop, which hands cairo positioned glyphs with no UTF-8 and no clusters and silently destroys the text layer, leaving a page that looks correct and cannot be searched, selected or copied. libpangocairo is **already linked into the process by GTK**, so this adds bindings and no system dependency, no runtime and nothing to the footprint. Pinned to 0.21 to match the `pango` and `cairo-rs` `gtk4` 0.10 already brings in; a mismatched release train would hand cairo a `Context` from a different binding generation. |
| `richimg` (workspace member) | — | **This project's own** animated-raster decoder: bytes in, composited straight-alpha RGBA8 frames out, for WebP, GIF and APNG. Holds no GTK, so a decode runs on a worker thread and only owned pixel data returns to the main one. It exists because the route it replaces LEAKED — `webp-pixbuf-loader`'s animated branch retains every decode, invisibly to refcount assertions (ScrAP-351) — and because WebP decoded on one platform of three. `src/imagedecode/` is the single application-side caller; `clippy.toml` bans GTK's own encoded-image entry points everywhere else. A `default-member`, so steps 1, 2 and 4 format, lint and test it. |
| `image-webp` | =0.2.4 | WebP decode inside `richimg`. Pure Rust, `#![forbid(unsafe_code)]`, MIT OR Apache-2.0; the backend the `image` crate, GNOME glycin, resvg and Servo use. **Pinned exactly**: 0.2.4 has known panics on crafted input (image-webp#182, #118), which is why every decoder call in `richimg` runs under `catch_unwind`, and its dispose-to-background is a no-op unless a clear colour is set. Re-read PLAN/`webp.rs` before bumping. |
| `gif` | =0.14.2 | GIF decode inside `richimg` (image-rs, MIT OR Apache-2.0, fuzzed through `image`). Emits raw frame rectangles only — the canvas, the four disposal modes and the transparent index are `richimg`'s own, so the composited output matches what browsers show. `default-features = false`: the dropped defaults are encoder-only helpers. |
| `png` | =0.18.1 | APNG decode inside `richimg` (image-rs, MIT OR Apache-2.0, fuzzed on OSS-Fuzz). Supplies the frames and their `fcTL`/`acTL` control data; the three dispose ops and two blend ops are `richimg`'s. A still PNG never reaches it — that stays GTK's decode. |
| `glib-build-tools` (build) | 0.22 | `[build-dependencies]` only, never linked — `build.rs` uses it to compile the bundled-icon `GResource` (see the `icons.rs` row). |
| `winresource` (build, Windows host) | 0.1 | Windows-host `[build-dependencies]` only, never linked — `build.rs` uses it to embed the Win32 icon/`VERSIONINFO` resource section, the shell-facing icon channel (`packaging/windows/README.md`). |
| `tempfile` (dev) | 3 | Test-only — temp files/dirs for `session`/`workaround`/`atomic_io` filesystem tests, and the `links` image-containment tests. |
| `gtktest` (local, optional) | — | Test-only, enabled by `gtk-integration-tests`. Workspace member; the `#[gtktest::test]` registration attribute (a proc macro must be its own crate). |
| `inventory` (optional) | 0.3 | Test-only, enabled by `gtk-integration-tests`. Link-time registry that lets every `#[gtktest::test]` body register with the main-thread suite without a hand-maintained list. Deliberately uncounted — see the comment above it in `Cargo.toml`. |
| `libc` | 0.2 | Already present transitively via `gtk4`; direct dep for `atomic_io`'s `umask(2)` query and `forensics`'s POSIX signal surface (glibc already exports the backtrace machinery, so crash reporting needed no new crate). |
| `regex`, `walkdir` (xtask) | 1, 2 | **Not in the application.** The `xtask/` build-gate crate's only dependencies, and neither reaches the shipped binary — nothing in `src/` depends on that crate. Both were already in `Cargo.lock` through the application's own tree, so the gate added nothing to fetch or to audit. |
| *(none — GIO FileMonitor used instead of notify)* | — | Live reload uses `gio::FileMonitor` (inotify on Linux) via the GIO already linked. No extra crate. |

### System dependencies

| Component | Role |
|-----------|------|
| GTK 4 | Widget toolkit and main loop. 4.6 works; a defensive XCompose-size workaround fires only when `~/.XCompose` exceeds 8 KB — see GTK4Rs/AP-3. GTK ≥ 4.12 recommended. |
| GtkSourceView 5 | Source editing and code highlighting (`libgtksourceview-5`). |
| GLib / GIO | Application lifecycle, single-instance activation, async file ops. GIO abstracts the IPC backend per platform — D-Bus where a session bus exists, a named mutex plus hidden-window forwarding on Windows — so the app wires only `startup`/`activate`/`open` and never touches either directly. |

**Linux, macOS and Windows all build from this one source tree.** Linux is the
*reference* platform — the canonical host for the POLICY gates, never lowered to make
another platform pass — but it is not the only supported one. What differs per platform
is deliberately small, and every difference has the same shape: the platform is missing
a **transport** or a **source** that GTK/GIO supplies elsewhere, so a thin module
supplies it and hands straight off to the shared machinery. Behaviour never forks per
platform.

| Platform | Toolchain & packaging | What the platform lacks, and what supplies it |
|---|---|---|
| **Linux** (reference) | Distro GTK4 / GtkSourceView 5; `packaging/linux/install.sh` | Nothing structural. `workaround.rs` defends the GTK 4.6 XCompose crash on hosts below 4.12 (GTK4Rs/AP-3). |
| **macOS** | Homebrew GTK4 on the Quartz backend; `packaging/macos/` → `.app` bundle | No D-Bus session bus, so GIO cannot do single-instance activation → `platform/mac/single_instance.rs` elects a primary and forwards a second launch's arguments. Quartz never reads the light/dark appearance into `GtkSettings` → `platform/mac/appearance.rs` observes it and writes the setting. GDK's Quartz backend tags every toplevel a candidate *primary* fullscreen window with no dialog/transient exemption, so a dialog raised over a full-screen parent seizes a Space of its own → `platform/mac/fullscreen.rs` reclassifies a secondary window's `NSWindow` as *auxiliary* before GTK attaches it. GTK takes one application-wide menubar model while this app's menus carry per-window content, so nothing keeps the native menu pointed at the active window → `platform/mac/menubar.rs` re-exports it on activation, and `window/chrome.rs` builds no in-window bar there. A `.app` bundle is required before the app has any Dock or Cmd-Tab identity (`Info.plist`, never GTK's icon theme). **Not abstracted away:** macOS reserves the main thread for windowing, so the integration suite reaches this platform only through `src/gtk_suite.rs`, never `--lib`. **No GVfs at all** (Homebrew has no such formula), so GIO resolves no `http`/`https` URI — which is why remote images are fetched by `imagefetch.rs` on *every* platform rather than through `gio::File::for_uri` (GTK4Rs/AP-292). Unlike the seams above this needed no `platform/mac/` module: the missing transport was replaced for everyone, which is the preferred shape whenever the substitute is portable. |
| **Windows** 10/11 x64 | gvsbuild GTK4 under MSVC; `packaging/windows/` → per-user installer | No app-side single-instance seam — GIO has no Win32 backend for it, and none is needed: uniqueness rides a D-Bus session bus here as on Linux, GLib autolaunching one by spawning `gdbus.exe` from beside the GLib DLL. Uniqueness is therefore a **packaging** obligation owned by `packaging/windows/stage.ps1`, not a code one (GEP-39). The integration suite runs unmodified. GTK does not request a dark DWM caption or read the Windows light/dark setting → `platform/win32/` supplies both, plus the native-frame maximize repair described below. |

The platform-conditional code is **four modules**, gated for three different reasons:
`workaround.rs` is `cfg(unix)` because its subject **does not exist** off unix
(and it actively runs on a GTK 4.6–4.11 Linux host — the gate is "not applicable
off unix", never "already dead"; GTK4Rs/AP-3), whereas `platform/mac/` and
`platform/win32/` exist because GTK **does not do something** there that it does
elsewhere, and `platform/x11.rs` exists because GTK **does something it does not expose**: its X11
backend places popovers by screen coordinates it never hands the application. The
platform seams are gated at their declarations in `platform/mod.rs`
rather than at the crate roots, so each gate is a single fact shared by both crate
roots (`lib.rs` and `gtk_suite.rs`) instead of two declarations that can drift.
The platform modules are the only sanctioned places to call the OS directly
(POLICY § Architecture rules); they detect and hand off to shared machinery, and
theme nothing themselves — the *how* of each repair is documented in the modules'
own comments.

Platform-specific notes that shape the architecture:

- **User directories resolve per platform.** `XDG_CONFIG_HOME`/`XDG_STATE_HOME` are
  honoured first everywhere; only the *fallback* is platform-specific (`HOME`-relative
  on unix, `%APPDATA%`/`%LOCALAPPDATA%` on Windows). The split is deliberate:
  configuration should roam between machines; session state (window geometry) should
  not. Both are hand-rolled from `std::env` — `glib::user_config_dir()` is banned
  project-wide (`clippy.toml`).
- **Window decorations differ by design.** `lib.rs` sets `GTK_CSD=0` under
  `#[cfg(windows)]` to take the native Win32 frame; this holds only because the app
  has no custom titlebar — a `set_titlebar()` anywhere would silently restore CSD.
  The native frame is what supplies the window's resize borders on all edges, the
  Alt+Space system menu, and Snap Layouts (the maximize button hit-tests as
  `HTMAXBUTTON`, with no manifest opt-in); under CSD the window has none of them. The
  variable is set in-process rather than by the installer or a launcher so the frame
  does not depend on how the binary was started, and it is not in
  `.cargo/config.toml`'s `[env]` because it is a GTK-wide variable: on Wayland it can
  leave a window undecorated. The design ceiling is "native frame, GTK interior" — GTK4
  has no maintained Windows theme, so imitating Windows chrome in CSD is not pursued.
- **`gio::FileMonitor`** is backed by `ReadDirectoryChangesW` instead of inotify;
  live reload is verified across that substitution.

### Integration boundaries

- **Filesystem ↔ Recovery snapshot**: a document's buffer is periodically written
  to a *swap file* in the user state directory while it is at risk — dirty, or its
  file deleted or emptied — so an unclean exit (a SIGSEGV, an OOM kill, a power
  loss) no longer discards work that exists nowhere else. This is the
  one write path besides save, and it is deliberately not the same one: it must be
  owner-only from the first byte and must never block the main thread, neither of
  which `atomic_io` provides. The write opens a co-located temp with GIO
  `replace_async` (`PRIVATE` → `0600` from the first byte) and **renames it into
  place only after a complete write** — owning the promote decision rather than
  calling `replace_contents_async`, which renames a truncated temp over the previous
  good snapshot on an ordinary disk-full (GTK4Rs/AP-167). It shares GLib's I/O thread
  pool with document I/O, which is why `docio/` caps its own use of that pool rather
  than letting a slow filesystem make snapshots late (GTK4Rs/AP-243). Recovery data is
  never written next to the user's document: a central directory keeps each open
  file's `gio::FileMonitor` from seeing a stream of events it would then have to
  learn to ignore, and works for read-only directories and untitled buffers alike.
- **Filesystem ↔ Document**: the application reads a file into the Source buffer
  and writes it back on save, both through `docio/` and therefore both off the main
  thread — a window never waits on a filesystem. Saves are content-gated rather
  than mtime-gated — the on-disk bytes are compared against the baseline last read,
  so a coarse filesystem clock cannot mask a real conflict — and are written
  atomically, so a crash mid-write cannot tear the file. Every read of a document
  path, including a re-read of one already open, passes `limits`' admission test
  first. A two-sided text merge is out of scope.
- **File watcher ↔ main loop**: a `gio::FileMonitor` is attached per open file.
  Events arrive on the GLib main loop directly, with no background thread, and the
  monitor's lifetime follows the tab that owns it. **No event decides anything on its
  own** — every one of them, deletions included, is answered by a re-read, and whether
  a change is ignored, reloaded, raised as a conflict, or recorded as the file being
  deleted or emptied out from under its buffer is decided by a pure function of what
  that read found and the document's state. The re-read is the whole of how a deletion
  is detected (a `NotFound` is an observation, not a failure), because an event that
  *says* `DELETED` is also what the platform reports for the rename every careful
  external writer performs; a loss is therefore concluded only from a second read a
  settle interval later, which is the one mechanism serving both losses. The
  behavioural contract is TDD §3 and §5.
- **Single-instance activation**: a second launch with file arguments is
  forwarded to the primary instance's `open` handler; no second process starts.
  The transport is per-platform and the difference stops at the transport — every
  path delivers the same `gio::File` list to the same handler. GIO's D-Bus
  forwarding serves **both** Linux and Windows; the platforms differ only in how
  the session bus comes to exist (a running user bus on Linux, GLib autolaunching
  one via `gdbus.exe` on Windows, which is why that helper is a shipping
  requirement — GEP-39). macOS is the only platform with no transport of its
  own, and `platform/mac/single_instance.rs` substitutes one. macOS also has an
  independent LaunchServices reuse path that is not ours; GTK's Quartz backend
  routes it into the same `open` handler.

Commands and their parameters, the annotation storage format, and the
reading-theme file contract are specified in [SCHEMA.md](SCHEMA.md).

## Module responsibilities

Every module below runs on the GTK main thread; the three kinds of work that leave it
(document I/O, image decoding, the word count) go through GLib's I/O pool and are
described under [Concurrency model](#concurrency-model). A row names what a subsystem
*owns*; how it does so is in that subsystem's own module headers, which is where a
reader who needs the mechanism is already looking.

| Subsystem | Responsibility |
|-----------|----------------|
| `main.rs`, `lib.rs` | Entry points. `main.rs` only delegates; `lib.rs` is the crate root that owns the module list and `run()` — `GtkApplication` setup, single-instance registration, the Cairo renderer override and the `activate`/`open` wiring. The crate is a library so test targets can link it. |
| `app/` | Application-scoped concerns: the command enums, `app.*` action registration, menu-bar and mnemonic construction, accelerators, and file opening — including the synchronous cold-start claim and the single-pass open handler that keep two overlapping launches from targeting windows twice. |
| `window/` | Per-document window UI; `mod.rs` is a thin orchestrator over one-concern submodules. Owns the persistent chrome, status bar and per-button-wrapping toolbar (no chrome sets the window's minimum width), the app-wide toolbar and split arrangements, tab lifecycle including cross-window drag, session restore and close, action registration and sensitivity, Back/Forward, the context menu, toasts, split-pane scroll sync, the live-preview debounce, zoom and its wheel input, the sidebar panes, and the outline and annotation navigators. |
| `window/find.rs`, `window/findbar.rs`, `window/find/` | Find in both panes: which pane a find acts on and the per-tab match state; the shared bar's binding to the active tab; and, GTK-free under `find/`, the matcher, options, history, search-in-selection scope, highlight plan and body-text mapping. The editor's half is `GtkSourceSearchContext`. |
| `window/{save,export,export_pdf,reload,backingloss,rename}.rs` | The GTK edge of the document operations — Save, Export to HTML and PDF, live reload and its conflict handling, backing loss, and rename: the dialogs, notices and monitor choreography. Filesystem work is delegated to `docio/` and `atomic_io.rs`. |
| `window/editbar/`, `format/`, `clipboard.rs`, `lineendings.rs`, `macwordnav.rs` | Editing: the Format commands (display-free core in `format/`; the application path, insert dialogs, the format bar in its two shapes, the caret overlay and the Enter conveniences in `editbar/`); what the editor puts on a clipboard and the application's one writer of clipboard text; the no-lone-carriage-return rule at both doors a document arrives by; macOS word navigation. |
| `swapfile/`, `window/swap.rs`, `window/swaprecovery.rs` | Crash recovery: the display-free swap-file codec, naming, baseline digest and recovery decisions; the debounced write edge that promotes a snapshot only after a complete write; the startup recovery pass. |
| `winstate/` | The typed per-window and per-tab state registry, and the display-free decision cores that read it — Back/Forward history data, the one-writer-at-a-time write gate, the status bar's decisions. |
| `widgets/` | The custom widgets: the tab strip, the disclosure toggle, the sprite-filled rule and sprite icon, the wrapping toolbar row, and the Markdown table with its link cells. |
| `renderer/`, `tags.rs`, `span.rs`, `readingpos.rs` | The render pipeline from the parsed event stream to buffer text: the raw-HTML allowlist and tag lexing, disclosure identity and pairing, the front-matter walk seam every whole-document parse reads through, and image extent decisions; the preview's `GtkTextTag` set with its display-free spec; the named block spans a painter reads; and `DocPosition`, the one document coordinate both panes resolve. |
| `preview/`, `codeview/`, `decorplan.rs`, `affordance.rs`, `keynav.rs`, `farscroll.rs`, `taskbox.rs` | The read-only preview: its construction, interaction, scroll and CSS wiring; `CodePreviewView`, the `GtkTextView` subclass that self-draws every decoration (one painter per decoration, plus the annotation card popover); the display-free paint plan, affordance geometry and key-navigation decisions those painters read; the task checkbox's shape, which the PDF sink draws too; and every far scroll, re-issued only against a settled layout. |
| `fold.rs`, `preview/splice/`, `window/foldsplice.rs`, `window/foldreveal.rs` | Disclosure folding: the display-free fold model; changing a rendered preview in place for one toggle (a typed verdict, a typestate on the write order); and the window layer's two intents — stay put, or reveal-and-navigate. |
| `outline/`, `outline_view.rs`, `annotations.rs`, `annotations_view.rs`, `annotate/`, `tasklist.rs`, `copymap.rs` | Views derived from the source: the outline model and sidebar, the annotations model and viewer with the CriticMarkup scan-and-mutate core, task-list toggling, and copy-as-Markdown — the models display-free, the widgets thin. |
| `export/` | Export to HTML and PDF as one pipeline with two sinks — a function of the source and the same event stream the preview is built from, never of the preview widget. Split along the toolkit boundary so every decision is unit-tested and only measurement and ink touch Pango and cairo. |
| `links.rs`, `limits.rs`, `docref.rs` | Untrusted-input policy: the URL scheme allowlist and local-resource containment gate; the input-cost limits a document is admitted under (single source of truth); and `AnchoredSpan`, the one way a place in a document is held across time ([CAM.md](CAM.md) § Document-Reference). |
| `imagecache/`, `imagedecode/`, `imagefetch.rs`, `animation/` | Images: the process-wide texture cache in front of decode and fetch; the only route from encoded bytes to pixels (content-sniffed, admission-tested, pixel-capped); the one network transport; and animated playback — policy, frame scheduling, the pool worker, the paintable, visibility, and theme sprites. |
| `theme/`, `sprite.rs`, `palette/`, `cssfrag.rs`, `icons.rs`, `colorscheme.rs` | Appearance as data: the reading-theme engine (key registry, file model, resolution, decoration precedence, search path and merge) with no per-theme knowledge; sprites from a themes file or compiled in; the resolved palette, its WCAG maths and per-surface inline-code chips; the CSS fragments the preview and HTML sinks share; the icon names; and which GTK settings carry the desktop's lightness. The theme is preview-only; its rules, and zoom's, are in [THEMING.md](THEMING.md). |
| `session/`, `config.rs`, `atomic_io.rs`, `docio/` | Persistence and document I/O: session read/write, schema, migrations and the single state-directory lookup; the `config.toml` singleton; atomic write-temp-then-rename; and every read and write of a user document, off the main thread through GLib's pool with one admission gate and one budget, plus the rename primitive and filename rules. |
| `platform/`, `workaround.rs`, `accel.rs` | The seams whose answer differs per OS, gated once at their declarations: X11's popover screen coordinates; on macOS single instance, the system menu bar, appearance, fullscreen classification, bundle lookups, pointer crossing and the eager pasteboard; on Windows the frame, appearance, an owner-only state directory, process liveness and reduced motion; the GTK 4.6 XCompose redirect; and accelerator spelling per platform. They detect and hand off; each repair's *how* is in its own header. |
| `logging.rs`, `logrepeat.rs`, `forensics/` | Diagnostics: the single logging sink with the glib bridge, collapsing runs of identical records before they reach anything; and crash forensics — the persistent log, identity stamp, breadcrumb ring and crash report the application leaves in the state directory. The artefacts and the constraints on the crash path are in `forensics/`'s header; call-site rules in POLICY § Logging. |
| `saferizer/` | Typed seams that promote GTK's prose-only runtime contracts to compile-time ones: popover anchors, self-drawn click activation, scroll-position writes, and the document monitor. |
| `a11y.rs` | Accessible naming — the one place a control's accessible name and tooltip are set together. Structural accessibility is [PLAN.accessibility.md](PLAN.accessibility.md). |
| `gtk_suite.rs`, `suite_registry.rs`, `gtktest/`, `gtk_log_harness.rs`, `testsymlink.rs`, `xtask/`, `build.rs` | Build and test infrastructure: the second crate root that runs GTK test bodies on the main thread, one process per module; the attribute that registers a body with both harnesses; the harnesses' collapsing log writer; the symlink fixture; the lint gates (`cargo xtask lint-references`); and the build script (crate versions, the icon `GResource`, Win32 resources). |

## Concurrency model

The application owns no threads. Every widget operation and every
`gio::FileMonitor` callback runs on the GTK main thread, driven by the GLib main
loop; there is no shared mutable UI state and no locking anywhere.

**Image decoding is the second thing to leave the main thread, and the first that is
not I/O.** An animation frame costs ~9 ms for a 900×670 WebP and grows with pixel
count, so decoding on the main thread would let one large, fast animation miss frames
for the whole window. `richimg` holds no GTK type, so `animation::worker` hands a whole
`richimg::Animation` to GLib's existing I/O pool and gets an owned `richimg::Frame`
back on the main context — the same shape document I/O already uses, plain owned data
crossing and no GTK object. The texture is built and swapped on the main thread. Two
bounds make that safe on a pool of ten shared with document I/O and the crash-recovery
snapshot writer (GTK4Rs/AP-243): one decode per animation is in flight **by construction**
(the decoder is moved into the call and returns with the frame), and at most two
decodes application-wide reach the pool at once.

**The status bar's word count is the third, for the same reason**: counting a
multi-megabyte document costs more than a frame. `window::statusbar` hands the text to
the pool as an owned `String` and gets counts back — one job application-wide, one
waiting — and discards a result whose buffer has changed since it was read.

**Single-threaded does not mean synchronous, and the filesystem is where that
shows.** Document I/O — Open, session restore, link navigation, Save, Save As,
Reload, the live-reload monitor's re-read, and crash recovery's twin check — is
dispatched to **GLib's own I/O thread pool** and its result is delivered back on
the thread-default main context. The application still starts no thread of its
own: `gio::spawn_blocking` and GIO's `replace_async` both hand work to a pool GLib
already runs, and only plain owned data (`PathBuf`, `String`, `Vec<u8>`) crosses
the boundary, so no GTK object is ever touched off the main thread. `docio/` is the
single door; its blocking halves are private, so there is no second way through.

Two consequences are load-bearing rather than incidental:

- **That pool is shared with the crash-recovery snapshot writer, and it is small**
  (ten threads). Occupying too much of it does not make a snapshot fail; it makes it
  **late**, which for a mechanism that protects unsaved work is the same problem. So
  `docio/pool.rs` admits at most four document operations at a time and the rest wait
  in-process, where waiting is free. The measurements that fix that number, and the
  reasons no downstream lever exists, are GTK4Rs/AP-243.
- **The main loop runs during every one of these operations**, so anything resolved
  before an I/O call and used after it must be resolved *once* and carried, never
  asked twice. "Which tab is active" is the recurring instance: a save that asks
  before the write and again after can check one document's disk state and write
  another's. Document operations therefore take an explicit `Rc<TabState>`, and the
  two orderings that a re-ask cannot fix are guarded by types — a document's writes
  are serialised by `winstate::WriteGate` (a second Save is dropped, not raced,
  because GIO orders neither the renames nor the completions and explicitly
  re-sorts its queue), and a superseded read is discarded by a per-tab generation
  counter (GEP-43). A **second** counter, `winstate::WriteEpoch`, records only the
  saves this application itself completed: the save guard's comparison reads the
  baseline at decision time, so a write of ours landing inside its read would
  otherwise have it judge pre-write bytes against a post-write baseline and accuse
  the user of a conflict they caused themselves (TDD 5.7). It is separate from the
  generation counter above because that one is claimed by the live-reload watcher,
  and a guard that re-issues on it never lands at all on a filesystem GIO polls.

**What deliberately stays synchronous**: the application's own small state files —
`session.toml`, the config file, and the crash-recovery swap-directory scan. They
are ours, bounded, in the state directory, and their contents *decide* whether any
window is built at all, so there is nothing on screen to keep responsive while they
are read. The boundary is **document I/O off the main thread; the application's own
state files not**.

Rendering is deferred rather than dispatched. Preview rendering is CPU-heavy and
main-thread-bound, so on a multi-file open **or** a session restore only the visible
document renders eagerly; every other tab is built **deferred**, warmed
one-per-tick by a shared pre-render pump or built on first activation, whichever
comes first (`window/restore.rs`, `window/tabs/`). A deferred tab's persisted view
state is replayed through the view GActions only at its activation boundary
(GTK4Rs/AP-147).
