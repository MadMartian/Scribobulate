# Technical Architecture

Scribobulate is a native GTK4 desktop application written in Rust. It renders
Markdown into real GTK widgets drawn on the CPU (no HTML engine, no GPU
pipeline), giving it the resource footprint of a native editor. See [PRODUCT.md](PRODUCT.md)
for what the application does and why.

The stack was chosen by measured viability spikes, not assumption: native
widgets + the GSK Cairo renderer hold **0 MiB VRAM and ~80 MiB RAM** for a
single document with full-fidelity rendering. The rejected alternatives
(GPU-canvas toolkits, WebKitGTK) and why they failed are ScrAP-1 and ScrAP-2.

## System overview

![System overview: one single-threaded GtkApplication process; each Document Window carries per-window chrome plus a TabView of tabs, each tab one document (TabState) whose SplitView holds a GtkSourceView source pane and a CodePreviewView preview pane; Markdown source flows through CriticMarkup extraction, pulldown-cmark, the Renderer and preview::build_render_products into the preview widget; the file on disk is read and written through docio on GLib's I/O thread pool (writes via atomic_io) and watched by a gio::FileMonitor whose pure decision core chooses ignore, conflict-toast or reload.](system-overview.svg)

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
| `gtk4` (`gtk`) | 0.10 | GTK4 bindings — windows, widgets, application/single-instance. |
| `sourceview5` (`sourceview`) | 0.10 | GtkSourceView 5 — the editable Source pane (Markdown syntax highlighting, undo). |
| `pulldown-cmark` | 0.13 | CommonMark/GFM parser; emits the event stream the renderer walks. Extensions are an explicit allowlist, not `Options::all()` (ScrAP-78); superscript, subscript, strikethrough and highlight are tokenised in-crate by `renderer::scan_script_spans` instead (ScrAP-66, ScrAP-195). |
| `serde` | 1 | Serialization framework — `#[derive(Deserialize)]` for the `Config` struct. |
| `syntect` | 5 | Pure-Rust syntax highlighter — colors code blocks inside the `GtkTextView` buffer via per-token foreground tags. `default-fancy` (fancy-regex, no C dep); its syntax set is supplied by `two-face`. |
| `two-face` | 0.5 | Supplies syntect's syntax set from bat's bundled grammars — adds TypeScript/TOML etc. that syntect's defaults omit (ScrAP-160). `syntect-fancy` feature (no C dep); grammar licences reproduced in `THIRD-PARTY-LICENSES.md`. |
| `toml` | 0.8 | TOML parser for `~/.config/scribobulate/config.toml`. |
| `log` | 0.4 | Logging facade — the single sink for app + (bridged) GTK/glib diagnostics. |
| `env_logger` | 0.11 | `log` backend; `RUST_LOG` runtime level/module control. See `logging.rs`. |
| `glib` | 0.21 | Re-exported by `gtk4`; depended on directly only to enable its `log` feature (the glib→`log` writer bridge). |
| `dunce` | 1 | `canonicalize` that resolves symlinks/`..` (and strips Windows `\\?\` verbatim) — the image-`src` containment gate (`links::resolve_contained_image`) and the Browse→relative path math. |
| `pathdiff` | 0.2 | `diff_paths` — the document-relative reference inserted by the Insert Link/Image Browse buttons (`links::relativize_for_insert`). |
| `glib-build-tools` (build) | 0.22 | `[build-dependencies]` only, never linked — `build.rs` uses it to compile the bundled-icon `GResource` (see the `icons.rs` row). |
| `winresource` (build, Windows host) | 0.1 | Windows-host `[build-dependencies]` only, never linked — `build.rs` uses it to embed the Win32 icon/`VERSIONINFO` resource section, the shell-facing icon channel (`packaging/windows/README.md`). |
| `tempfile` (dev) | 3 | Test-only — temp files/dirs for `session`/`workaround`/`atomic_io` filesystem tests, and the `links` image-containment tests. |
| `gtktest` (local, optional) | — | Test-only, enabled by `gtk-integration-tests`. Workspace member; the `#[gtktest::test]` registration attribute (a proc macro must be its own crate). |
| `inventory` (optional) | 0.3 | Test-only, enabled by `gtk-integration-tests`. Link-time registry that lets every `#[gtktest::test]` body register with the main-thread suite without a hand-maintained list. Deliberately uncounted — see the comment above it in `Cargo.toml`. |
| `libc` | 0.2 | Already present transitively via `gtk4`; direct dep for `atomic_io`'s `umask(2)` query and `forensics`'s POSIX signal surface (glibc already exports the backtrace machinery, so crash reporting needed no new crate). |
| *(none — GIO FileMonitor used instead of notify)* | — | Live reload uses `gio::FileMonitor` (inotify on Linux) via the GIO already linked. No extra crate. |

### System dependencies

| Component | Role |
|-----------|------|
| GTK 4 | Widget toolkit and main loop. 4.6 works; a defensive XCompose-size workaround fires only when `~/.XCompose` exceeds 8 KB — see ScrAP-3. GTK ≥ 4.12 recommended. |
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
| **Linux** (reference) | Distro GTK4 / GtkSourceView 5; `install.sh` | Nothing structural. `workaround.rs` defends the GTK 4.6 XCompose crash on hosts below 4.12 (ScrAP-3). |
| **macOS** | Homebrew GTK4 on the Quartz backend; `packaging/macos/` → `.app` bundle | No D-Bus session bus, so GIO cannot do single-instance activation → `platform/mac/single_instance.rs` elects a primary and forwards a second launch's arguments. Quartz never reads the light/dark appearance into `GtkSettings` → `platform/mac/appearance.rs` observes it and writes the setting. GDK's Quartz backend tags every toplevel a candidate *primary* fullscreen window with no dialog/transient exemption, so a dialog raised over a full-screen parent seizes a Space of its own → `platform/mac/fullscreen.rs` reclassifies a secondary window's `NSWindow` as *auxiliary* before GTK attaches it. GTK takes one application-wide menubar model while this app's menus carry per-window content, so nothing keeps the native menu pointed at the active window → `platform/mac/menubar.rs` re-exports it on activation, and `window/chrome.rs` builds no in-window bar there. A `.app` bundle is required before the app has any Dock or Cmd-Tab identity (`Info.plist`, never GTK's icon theme). **Not abstracted away:** macOS reserves the main thread for windowing, so the integration suite reaches this platform only through `src/gtk_suite.rs`, never `--lib`. |
| **Windows** 10/11 x64 | gvsbuild GTK4 under MSVC; `packaging/windows/` → per-user installer | No app-side single-instance seam — GIO has no Win32 backend for it, and none is needed: uniqueness rides a D-Bus session bus here as on Linux, GLib autolaunching one by spawning `gdbus.exe` from beside the GLib DLL. Uniqueness is therefore a **packaging** obligation owned by `packaging/windows/stage.ps1`, not a code one (ScrAP-249). The integration suite runs unmodified. GTK does not request a dark DWM caption or read the Windows light/dark setting → `platform/win32/` supplies both, plus the native-frame maximize repair described below. |

The platform-conditional code is **three modules**, gated for opposite reasons:
`workaround.rs` is `cfg(unix)` because its subject **does not exist** off unix
(and it actively runs on a GTK 4.6–4.11 Linux host — the gate is "not applicable
off unix", never "already dead"; ScrAP-3), whereas `platform/mac/` and
`platform/win32/` exist because GTK **does not do something** there that it does
elsewhere. The two OS seams are gated at their declarations in `platform/mod.rs`
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
- **`gio::FileMonitor`** is backed by `ReadDirectoryChangesW` instead of inotify;
  live reload is verified across that substitution.

### Integration boundaries

- **Filesystem ↔ Recovery snapshot**: a dirty document's buffer is periodically
  written to a *swap file* in the user state directory, so an unclean exit (a
  SIGSEGV, an OOM kill, a power loss) no longer discards unsaved work. This is the
  one write path besides save, and it is deliberately not the same one: it must be
  owner-only from the first byte and must never block the main thread, neither of
  which `atomic_io` provides. The write opens a co-located temp with GIO
  `replace_async` (`PRIVATE` → `0600` from the first byte) and **renames it into
  place only after a complete write** — owning the promote decision rather than
  calling `replace_contents_async`, which renames a truncated temp over the previous
  good snapshot on an ordinary disk-full (ScrAP-232). It shares GLib's I/O thread
  pool with document I/O, which is why `docio/` caps its own use of that pool rather
  than letting a slow filesystem make snapshots late (ScrAP-243). Recovery data is
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
  monitor's lifetime follows the tab that owns it. Whether a change is ignored,
  reloaded, or raised as a conflict is decided by a pure function of the buffer's
  dirty state; the behavioural contract is TDD §3 and §5.
- **Single-instance activation**: a second launch with file arguments is
  forwarded to the primary instance's `open` handler; no second process starts.
  The transport is per-platform and the difference stops at the transport — every
  path delivers the same `gio::File` list to the same handler. GIO's D-Bus
  forwarding serves **both** Linux and Windows; the platforms differ only in how
  the session bus comes to exist (a running user bus on Linux, GLib autolaunching
  one via `gdbus.exe` on Windows, which is why that helper is a shipping
  requirement — ScrAP-249). macOS is the only platform with no transport of its
  own, and `platform/mac/single_instance.rs` substitutes one. macOS also has an
  independent LaunchServices reuse path that is not ours; GTK's Quartz backend
  routes it into the same `open` handler.

Commands and their parameters, the annotation storage format, and the
reading-theme file contract are specified in [SCHEMA.md](SCHEMA.md).

## Module responsibilities

Every module below runs on the GTK main thread; see [Concurrency model](#concurrency-model).

| Module | Responsibility |
|--------|----------------|
| `main.rs` | `[[bin]]` entry point, and nothing else — delegates to `lib.rs`'s `run()`, plus the Windows subsystem attribute. Deliberately empty: a binary crate exposes no importable surface, so anything here is untestable by construction. |
| `lib.rs` | Crate root. Owns the module list and `run()`: `GtkApplication` setup, single-instance registration, the Cairo renderer override, and the `activate`/`open` wiring. The crate is a library so that test targets can link it; the module tree stays `pub(crate)`. |
| `gtk_suite.rs` | A **second crate root**, a `harness = false` test target whose `main()` runs the GTK test bodies on the process main thread — the one thing `#[gtk::test]` cannot do. Built `--cfg test`, so it reaches `pub(crate)` internals; re-declares `lib.rs`'s module list, a duplication gated by `scripts/lint-references.sh` check 4. |
| `suite_registry.rs` | The `inventory` registry `#[gtktest::test]` submits into and `gtk_suite.rs` iterates. Test-only (`cfg(all(test, feature))`). |
| `gtktest/` (workspace member) | The `#[gtktest::test]` attribute. Emits the body once and registers it with **both** harnesses — a `#[gtk::test]` wrapper carrying the original name for libtest, plus a registry submission for the main-thread suite — so a body cannot drift between the two runs. |
| `platform/mac/single_instance.rs` | macOS only (`#[cfg]`-gated at its declaration in `platform/mod.rs`). Owns the single-instance handoff that stands in for the D-Bus one GIO cannot perform without a session bus: it elects a primary and forwards a second launch's arguments into `main.rs`'s existing `open`/`activate` wiring, owning no application behaviour of its own. |
| `platform/mac/menubar.rs` | macOS only (`#[cfg]`-gated at its declaration in `platform/mod.rs`). Owns keeping the system menu bar showing the active window's own menu model — the placement GTK's Quartz backend cannot make for itself, since it takes one application-wide model and this app's menus carry per-window content. It builds no menus: the model is the one `app::menubar` assembles for every platform. |
| `platform/mac/appearance.rs` | macOS only (`#[cfg]`-gated at its declaration in `platform/mod.rs`). Observes the macOS light/dark appearance through a CoreFoundation distributed-notification observer and hands it to `colorscheme` — GTK never reads that setting itself on Quartz. It re-themes nothing directly: it supplies the missing *source* for the theme channel `app::setup::connect_theme_change` already drives, which is the same role `platform/win32/appearance.rs` plays on Windows. |
| `platform/mac/fullscreen.rs` | macOS only (`#[cfg]`-gated at its declaration in `platform/mod.rs`). Supplies the window *classification* GDK's Quartz backend never makes: every toplevel is tagged a candidate primary fullscreen window at construction, with no dialog/transient exemption, so attaching a transient child to a parent already in a fullscreen Space makes AppKit run a real fullscreen transition on the child instead of floating it (ScrAP-277). Subscribes once to GTK's toplevel list from `app::setup::on_startup`; for each secondary window (transient parent or modal) it withholds `transient-for` until realize, reclassifies the now-existing `NSWindow` as *auxiliary* via hand-rolled Cocoa FFI (the Objective-C runtime plus the one public GDK entry point `gdk_macos_surface_get_native_window`, 4.8+), then restores the parent — so GTK attaches the child exactly as it would have, against a window AppKit will not try to make fullscreen. Document windows are untouched. |
| `colorscheme.rs` | Owns **which** GTK settings carry the desktop's lightness and in what order — the write side of `palette::desktop_is_dark()`. `#[cfg]`-gated at its declaration to the two platforms whose desktop supplies no such source; on Linux the desktop writes the setting itself and nothing here has a caller. Shared so that the rule, which turns on the GTK version rather than the OS, has one definition rather than one per platform. |
| `accel.rs` | Owns the one transform from a command's **declared** accelerator to the spelling the host platform uses, and the register of keystrokes the platform reserves for itself. Display-free and platform-as-data, so both platforms' bindings are enumerable — and their freedom from collisions provable — from either one. |
| `a11y.rs` | Owns accessible naming: the one place a control's accessible **name** and hover **tooltip** are set together (`clippy.toml` bans the bare tooltip setter to make this the only route). Structural accessibility is not here — see [`PLAN.accessibility.md`](PLAN.accessibility.md). |
| `logging.rs` | The single logging sink. Owns the `log`/`env_logger` setup, the glib→`log` bridge that captures GTK diagnostics, and the registration of the logger `forensics` builds. Owning registration is the point: exactly one place in the tree decides what the process's logger is. |
| `forensics/` | Crash forensics — what the application leaves behind when it dies: the persistent log in the state directory, the identity stamp, the in-memory breadcrumb ring, the panic hook and (`cfg(unix)`) the fatal-signal handler that writes a crash report. Builds the logging sink; never registers it. See [§ Diagnostics and crash forensics](#diagnostics-and-crash-forensics). |
| `config.rs` | Owns `config.toml` loading and the process-wide `Config` singleton, including the XDG config-directory lookup. |
| `build.rs` | Build script. Emits the GTK, SourceView and pulldown-cmark crate versions read from `Cargo.lock`; compiles the bundled-icon `GResource`; on a Windows host, embeds the executable's Win32 icon + `VERSIONINFO` resource section. |
| `app/` | Application-scoped concerns: the command enums, `app.*` action registration, menu-bar and mnemonic construction, accelerators, and file opening. |
| `window/` | Per-document window UI. `mod.rs` is a thin orchestrator; the submodules below own one concern each. |
| `window/chrome.rs` | Owns the persistent window layout that survives content and mode swaps, including the content slot tabs mount into. |
| `window/toolbar.rs` | Owns toolbar construction as one box per visually delimited section. |
| `window/splitview.rs` | Owns `SplitView`, the per-tab orderable two-pane splitter widget. |
| `window/tabs/` | Owns tab lifecycle: creation, switching, closing, drag-and-drop between windows, and the per-tab editor build. |
| `window/restore.rs` | Owns session restore at startup. |
| `window/lifecycle.rs` | Owns the window close-request path and the session snapshot taken on close. |
| `window/actions.rs` | Owns window action-state cores — the readers and the sensitivity rules that gate commands by mode and selection. |
| `window/editoractions.rs` | Owns registration of the editor and document `win.*` actions. |
| `window/navhistory/` | Owns Back/Forward navigation's UI side, and deliberately only the parts that need live widgets: `mod` (the two `win.nav-*` actions, the single place their sensitivity is computed, the mouse thumb-button gestures, and the reconciliation sited in front of that read), `traverse` (activating a document and positioning its viewport), `record` (the two recording choke points). Its module doc states why recording a tab activation is centralised while its opt-outs are per-call-site — the inverse of this codebase's usual choke-point arrangement — and why recording a within-document jump is necessarily the other way round. |
| `window/copylink.rs` | Owns Edit ▸ Copy Link Location: the `win.copy-link-location` action and the one place its enabled state is computed. It resolves two inputs to one gate — the link a right-click landed on (held for that popover's lifetime in `WindowChrome::ctx_link`, and what makes the read-only preview's row usable) and the link under the editor caret — each through the seam that already owns its question, the display-free source scan `format::link_target_at` and the preview's single link hit-test `preview::link_url_at`. |
| `window/viewactions.rs` | Owns registration of the view, sidebar, chrome, split and zoom `win.*` actions. |
| `window/editbar/` | Owns the Markdown formatting commands: the `win.format` application path, the insert dialogs, the Format toolbar section, the editor focus gate, the caret overlay, and the Enter conveniences (list/quote continuation, code-fence auto-close). |
| `window/contextmenu.rs` | Owns the right-click popover for the text views. |
| `window/find.rs` | Owns the find engine for both the editor and preview panes and resolves which of them a find acts on, plus the two find values the per-tab state holds: the match cursor and the preview hit-list cache. |
| `window/findbar.rs` | Owns binding the shared find-bar widgets to the active tab's search state. |
| `window/save.rs` | Owns the save path, including the pre-write safety check and the atomic write. |
| `window/reload.rs` | Owns live-reload and conflict handling for external file changes. |
| `window/rename.rs` | Owns renaming a document's file in place: the dialog, the per-tab enablement rule's two readers, and the monitor choreography — the live-reload monitor is **cancelled before** the rename and re-attached after, on every path including failure, because a rename of a watched file delivers three events on the old monitor (Linux/Windows) and the save path's self-delete guard consumes only one of them. The filesystem call and every naming rule live in `docio/rename.rs`. |
| `window/toast.rs` | Owns the floating toast widgets — the conflict prompt and the transient info notice. |
| `window/scrollsync.rs` | Owns split-pane scroll synchronisation, coalesced to once per frame. |
| `window/livepreview.rs` | Owns the debounce that re-renders the preview from editor edits in split mode. |
| `window/zoom.rs` | Owns the preview zoom ladder and its application to the window. |
| `window/sidebar.rs` | Owns `SidebarPane`, the shared chrome of a sidebar section. |
| `window/outline_nav.rs` | Owns the outline sidebar's data side: rebuilding the heading tree and navigating from it. |
| `window/annotations_nav.rs` | Owns the annotations viewer's data side, and the one mode-aware annotation navigator the viewer's rows and the Next/Previous Annotation commands both route through — so a view mode cannot be served by one and not the other. |
| `winstate/` | Owns the typed per-window and per-tab state registry, and the pure decision cores that read it. |
| `widgets/tab/` | Owns the tab-strip widget: `TabBar`, its rows, and their drag, close and context-menu mechanics. |
| `widgets/table/` | Owns `ScribTableWidget`, the custom widget rendering Markdown tables inside the preview. |
| `widgets/table/linkcell.rs` | Owns a link in a table cell across **both widget shapes one renders in** — the pure-link cell's button and the read-back of its caption, the link markup a mixed cell's label carries, and the single activation both delegate to. Owning both is the point: the shapes are indistinguishable to a reader, so nothing may reach one without reaching the other. |
| `preview/` | Owns construction of the read-only preview widget and its interaction, scroll and CSS wiring. |
| `codeview/` | Owns `CodePreviewView`, the preview's `GtkTextView` subclass that self-draws code-block backgrounds, task checkboxes, list markers and the right-margin annotation chips. |
| `codeview/card.rs` | Owns `AnnotationCard`, the `GtkPopover` subclass an annotation chip opens: its anchor (the annotation's identity, never a rectangle), its scroll-tracking, its dismissal and its teardown — and, because it is its own `GtkNative` and therefore outside the window's accelerator reach, the local re-offer of every application accelerator (ScrAP-266). |
| `codeview/navkeys.rs` | Owns keeping the preview's navigation keys reachable when an anchored table cell holds the focus: a capture-phase key controller — the only phase that still sees a key a focused selectable `GtkLabel` has claimed — re-emitting the `move-cursor` GTK's own binding would have. Decision-free; it reads `keynav` (ScrAP-264). |
| `codeview/geometry.rs` | Owns the view's cache-free line/cell geometry reads, and the chip-rectangle arithmetic the painter and the annotation card share. |
| `renderer/` | Owns the low-level rendering mechanics that turn the parsed Markdown event stream into buffer text and tags. |
| `tags.rs` | Owns the preview's `GtkTextTag` set and its style setup. |
| `span.rs` | Owns `BufferSpan`, the named character-offset range the renderer uses to record where each block landed. |
| `keynav.rs` | Owns which document movement a navigation key means — a mirror of `gtktextview.c`'s own binding table — and whether the widget holding focus makes that movement the document's to perform. Display-free, so both are settled by unit test (ScrAP-264). |
| `farscroll.rs` | Owns every far scroll in either pane, and the one fact they all need: *when GTK has finished laying the document out*. Until it has, a scroll aimed past the validated frontier is uncomputable or silently dropped, so both the buffer-ends key wiring and the mark-based navigation seam re-issue against a settled layout (ScrAP-260). |
| `copymap.rs` | Owns display-free copy-as-Markdown for the preview. |
| `format/` | Owns the display-free Markdown formatting core behind the Format commands. |
| `outline.rs` | Owns the display-free document-outline model. |
| `outline_view.rs` | Owns the outline sidebar widget. |
| `annotations.rs` | Owns the display-free annotations-viewer model, and `step_index` — the one answer to "which annotation is next/previous from here", applied in whichever space the caller counts in (the preview's buffer offsets, the editor's source bytes). |
| `annotations_view.rs` | Owns the annotations viewer widget. |
| `annotate/` | Owns the CriticMarkup annotation core: scanning annotations out of the source, and the pure, total mutations that add, edit and remove them. Also owns `AnchoredSpan`, the range-plus-its-own-text handle every deferred mutation addresses its construct through (ScrAP-187). |
| `tasklist.rs` | Owns the display-free logic for toggling a Markdown task-list checkbox in the source. |
| `links.rs` | Owns link, anchor and image-resource handling, including the URL scheme allowlist and the containment gate for local resources. |
| `limits.rs` | Owns the input-cost limits a document is admitted under — the construct-nesting bound and the size/file-type admission test — and is the single source of truth for their values, which POLICY § Input limits deliberately does not restate. |
| `theme.rs` | Owns the reading-theme engine: parsing theme files and resolving keys. Carries no per-theme knowledge. |
| `palette.rs` | Owns the resolved preview colour set and the WCAG colour maths that derives it. |
| `icons.rs` | Owns the `Icon` enum, the single source of truth for the app's GTK icon names — including the application icon, which doubles as the application ID. Names no common theme carries are bundled as SVGs under `data/icons/` and compiled into a GResource by `build.rs`. |
| `session.rs` | Owns cross-session persistence: the session schema, reading and writing the state file, and — as the single lookup in the tree — the user **state-directory** resolution that the forensic log and crash reports share with it. |
| `atomic_io.rs` | Owns atomic file writes (write-temp-then-rename), so a crash mid-write cannot tear a document. |
| `docio/` | Owns every read and write of a user document, and the fact that none of them touch the main thread: the blocking halves are module-private, so its `async fn`s are the crate's only route to a document. `docio/pool.rs` owns the dispatch and the cap on how much of GLib's shared I/O pool this application may hold (ScrAP-243). `docio/rename.rs` owns the rename primitive and the filename rules, taking the platform's rules as **data** so both platforms' are unit-testable from either — and documents that GIO's destination refusal is a `lstat`-then-`rename`, i.e. a narrowed race rather than a guarantee. Being the only route is what lets the read door carry one cross-cutting obligation for free: a document absent from disk is checked for the debris a crash mid-rename strands, so Open, session restore, link navigation and crash recovery all recover it without any of them knowing about renames (ScrAP-272). |
| `app/openbatch.rs` | Owns the `GApplication` **open** handler: one invocation's file arguments read first, then turned into at most one window in a single uninterrupted pass, so two overlapping launches cannot interleave their window targeting. |
| `winstate/navhistory/` | Owns the per-window Back/Forward history as display-free data, split by which question each part answers: `place` (what an entry points at, and why its equality is load-bearing), `record` (writing the history), `mod` (walking it), `maintain` (repairing it when a tab leaves or a document's headings change), `decide` (the two decisions the GTK half would otherwise take inline, so they are unit-testable at all). The instance lives in the registry's per-window entry, so a tab's departure prunes it without any call site knowing. |
| `winstate/writegate.rs` | Owns the one-writer-at-a-time gate over a document's file, as a pass released on `Drop` rather than a flag with a rule attached. Display-free, so its release-on-every-exit contract is unit-tested directly. |
| `swapfile/` | Owns the display-free crash-recovery core: the swap file's frontmatter codec, its naming scheme, the baseline content digest, and the recovery decisions. Touches no GTK and no filesystem; its module doc states the two invariants the feature turns on. |
| `window/swap.rs` | Owns the crash-recovery write edge — the debounce, the focus-loss flush, and the write-temp-then-rename that promotes a snapshot only after a complete write — plus the single choke point that applies the dirty↔swap invariant. |
| `window/swaprecovery.rs` | Owns the startup recovery pass: sweeping incomplete snapshot temps, correlating each surviving header against the restored tabs, applying content, and the per-tab notice plus per-window status message. |
| `testsymlink.rs` | Owns the shared symlink fixture every test whose subject is a symlink builds through, and the runtime skip it reports where the platform refuses one. Test-only (`cfg(test)`); declared in both crate roots. |
| `saferizer/` | Owns the project-local typed seams that promote GTK's prose-only runtime contracts to compile-time ones — including `popover_anchor`, the single gate every popover anchor passes through (on-viewport proof plus its clamp, inseparably) and the sanctioned reader for GTK's `pointing_to` out-parameter; and `click_activation`, which owns *both* halves of every self-drawn affordance's click — its pairing rule lives in a display-free state machine, leaving the GTK closures decision-free (ScrAP-238); and `scrollpos`, the one route by which a scroll position is written, because a plain adjustment write is an unconditional supersede that cancels any scroll animation in flight and GTK exposes no way to ask whether one is (ScrAP-260). |
| `workaround.rs` | Owns the per-process `XDG_CONFIG_HOME` redirect that defends against the GTK 4.6 XCompose crash (ScrAP-3). |
| `platform/win32/` | Owns the project's **entire** Win32 surface (`#[cfg(windows)]`, gated once at its declaration in `platform/mod.rs`), split by cause and not by DLL: `frame` (the dark DWM title bar, and keeping GTK's remembered window size current while the OS holds the window maximized), `appearance` (reading the Windows light/dark setting GTK does not read itself), `privacy` (an owner-only state directory, which `std::fs` cannot express here), `process` (whether a pid is a running instance — Windows has no `/proc`). Every name is re-exported from its `mod.rs`, so callers address it as one module. It detects and hands off; it themes nothing directly. |

## Theme awareness

The preview's appearance is **data**. Everything it renders — colour, typography,
and decoration geometry — is sourced from the active *reading theme*, and the
engine (`theme.rs`) holds no per-theme knowledge: no colour constants, no
`if theme == "sepia"` branches. Themes live in `data/themes.toml`, installed
alongside the app and compiled in (`include_str!`) as the last-resort fallback, so
a missing or malformed themes file can never prevent startup.

The rest of the app — toolbar, tab strip, outline sidebar, editor — stays on the
desktop GTK theme. Theming those is the window manager's job (POLICY scope
decision), so the reading theme is **preview-only**.

The rules themselves — resolution order, the themes-file search path, the three
mechanisms a key reaches the screen by, untrusted-input handling, and where the
change notification comes from on each platform — are long and consulted as a
unit, so they live in [THEMING.md](THEMING.md). **That document also carries the
zoom rules**, which are otherwise an unrelated concern: theming and zoom are
independent features that meet only in the CSS cascade and the pixel-metric
arithmetic, where the contract is one invariant about both (the theme and zoom
providers write disjoint properties; the theme owns SCALE and never SIZE), so it
is stated once, there.

## Diagnostics and crash forensics

The application reports on itself, because on this platform nothing else will: a
crash leaves no core dump (apport refuses an unpackaged executable) and no symbols
(ScrAP-141), so what it records at the moment of death is the whole of the evidence.
`forensics/` produces four artefacts, all beside `session.toml` in the state
directory, which `session::state_directory` resolves once for both:

| Artefact | What it is |
|---|---|
| `scribobulate.log` | Every `info`-or-worse record, appended unbuffered, size-capped with one retained generation. |
| The identity stamp | Version, commit, profile, executable path/size/mtime, GTK **runtime** version, renderer, pid — the first records of every run and the header of every crash report. |
| The breadcrumb ring | The last 64 records, in memory, in fixed slots written with atomics only. |
| `crash-<stamp>-<pid>.log` | Written by the panic hook or the fatal-signal handler: identity and fault, then breadcrumbs, then backtrace and the process's executable mappings — the last making a frame resolve to module + offset with no core dump. |

Four properties constrain everything on that path, none visible from a call site:
the recording threshold is `max(RUST_LOG, Info)` — breadcrumbs must already exist
when an unanticipated crash arrives (POLICY § Logging states the call-site rules);
the crash path may not allocate or lock (`forensics/signal.rs` documents every
choice this forces); the handler re-raises, so a crash terminates exactly as it
would unhandled (ScrAP-203); and every artefact is created owner-only through one
shared `OpenOptions` (`forensics::private_options` states why one, not per-writer).

Windows keeps all of this except the fatal-signal report, POSIX signals being absent
there; the gate is "not applicable off unix", as `workaround.rs`'s is.

## Concurrency model

The application owns no threads. Every widget operation and every
`gio::FileMonitor` callback runs on the GTK main thread, driven by the GLib main
loop; there is no shared mutable UI state and no locking anywhere.

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
  reasons no downstream lever exists, are ScrAP-243.
- **The main loop runs during every one of these operations**, so anything resolved
  before an I/O call and used after it must be resolved *once* and carried, never
  asked twice. "Which tab is active" is the recurring instance: a save that asks
  before the write and again after can check one document's disk state and write
  another's. Document operations therefore take an explicit `Rc<TabState>`, and the
  two orderings that a re-ask cannot fix are guarded by types — a document's writes
  are serialised by `winstate::WriteGate` (a second Save is dropped, not raced,
  because GIO orders neither the renames nor the completions and explicitly
  re-sorts its queue), and a superseded read is discarded by a per-tab generation
  counter (ScrAP-244).

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
