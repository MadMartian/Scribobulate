# Scribobulate

<picture>
  <source srcset="assets/splash.webp" type="image/webp">
  <img src="assets/splash.gif" alt="Introducing Scribobulate — a looping tour of reading themes, the outline, the live-render pipeline, and split editing">
</picture>

A native Markdown viewer and editor for Linux, macOS, and Windows —
for the documents AI agents generate: rendering full-fidelity Markdown on the CPU
with effectively zero GPU memory, and live-reloading files the instant an agent
changes them on disk.

![Scribobulate](data/icons/scalable/apps/com.extollit.scribobulate.svg "Scribobulate")

> **Status: early but capable.** Rendering, the editing pane (with live
> split-preview), live reload, and conflict handling all work today —
> Scribobulate opens and displays full-fidelity Markdown (tables,
> syntax-highlighted code, task lists, images) at 0 MiB VRAM / ~80 MiB RAM as a
> single-instance multi-window, multi-tab app, reloads files as they change on disk, and
> when a change lands under unsaved edits it prompts you to reload or keep your
> work (a clean change reloads silently with a brief notice). Unsaved edits also
> survive the application dying: they are snapshotted as you type and offered back
> on the next launch.

## What it does

Scribobulate is built for working alongside AI agents that rewrite Markdown
continuously: it watches the open file and re-renders automatically the moment an
agent changes it, so the plans, notes, and reports your agents produce are always
shown up to date. It displays your Markdown the way you expect — headings,
tables, syntax-highlighted code, task lists, and images — in a fast native GTK
window, with an optional editing pane and live preview. When a file changes
underneath unsaved edits, Scribobulate asks you what to do instead of silently
discarding your work — and if the application itself dies, your unsaved work is
waiting for you when you reopen it.

## Features

- **Negligible GPU memory**: renders through native widgets on the CPU, holding
  no meaningful VRAM — unlike GPU-canvas- or browser-based viewers that consume
  hundreds of megabytes.
- **Live reload**: edits made by an agent or another program appear immediately,
  with your reading position preserved.
- **Safe conflicts**: an external change to a file you're editing prompts you to
  reload or keep your version — never a silent overwrite. If the file is deleted
  out from under an open document, its tab is flagged and closing it prompts to
  save — never a silently lost document.
- **Crash recovery**: unsaved edits are snapshotted as you type, so an unclean exit
  — a crash, an OOM kill, a power cut — costs seconds of work rather than the
  session. They come back on the next launch, still marked unsaved, with the option
  to discard the recovery. Your own file is never written without an explicit save.
- **Full fidelity**: GitHub-flavored tables, real syntax highlighting, inline
  images, task-list checkboxes, blockquote bars, and clickable hyperlinks —
  hover one to see where it actually leads before you commit to the click.
- **Follow links between documents**: a relative link to a sibling Markdown file
  (`[architecture](TECH.md)`) opens it as a new tab in the same window — agent-authored
  document *sets* cross-link heavily, and this project's own `sdd/` tree is the
  example, so you read them as a set instead of hunting each file down by hand. A
  file that's already open is focused rather than duplicated. Links are contained to
  the document's own folder by default; a target outside it is refused with a visible
  reason, never a silent dead click. **Load Unsafe Linked Documents** (File menu or
  toolbar) lifts that per tab when you trust the document — off by default, never
  remembered between sessions, and never inherited by the documents it leads to, so
  permission can't ratchet across your filesystem one hop at a time.
- **Icon toolbar**: Quick-access icon buttons — New, Open | Copy, Cut, Copy Document, Delete, Select All — with hover tooltips that name the command and its keyboard shortcut ("Open (Ctrl+O)"). Those names are published to assistive technology as well, so a screen reader announces the command rather than an anonymous icon — a tooltip on its own never reaches one. Sensitivity mirrors the action state automatically (Copy greys out when nothing is selected). Copy Document copies the whole document's raw Markdown source to the clipboard regardless of view mode, distinct from Copy (selection only).
- **Native menu bar**: File, Edit, Format, View, and Help menus with keyboard accelerators and selection-aware enabled state. Fully keyboard-navigable in the conventional way — `Alt+F`/`Alt+E`/`Alt+R`/`Alt+V`/`Alt+H` open each menu, then the underlined access letter invokes an item (e.g. Edit then `t` = Cut). The right-click context menus (document and tab) carry the same access keys.
- **In-app help**: Help ▸ Keyboard Shortcuts (F1 or Ctrl+?) opens a searchable window listing every shortcut, grouped by area; Help ▸ Markdown Reference opens the CommonMark syntax guide (the syntax the app renders) in your browser; Help ▸ About shows the version, system info, and Apache 2 license.
- **Markdown formatting**: a Format menu, a toolbar section, and a pop-up overlay at the selection for Bold, Italic, Heading 1–6, Strikethrough, Highlight (`==mark==`), Code Span, Code Block, Quote, Bulleted List, Numbered List, Task List (GFM `- [ ] ` checkboxes), and Horizontal Bar — each wraps the selection (or toggles the markup off) and has a keyboard shortcut (Ctrl+B / Ctrl+I, Shift+F1–F6 for headings, Ctrl+Alt+C for a task list, and more). Lists and blockquotes also auto-continue as you type: pressing Enter on a list item or `>` quote line starts the next one (repeating the bullet / quote prefix, or incrementing the number, indentation preserved), and Enter on an empty one ends it. Typing a bare ```` ``` ```` fence and pressing Enter drops the matching closing fence below and parks the caret between them.
- **Annotate & review** (Ctrl+Alt+M): select a claim — in the rendered preview or the editor — and attach a comment. It's saved into the Markdown file itself as portable CriticMarkup (`{==highlight==}{>>comment<<}`), so the author (human or agent) reads your feedback inline on the next open — an in-file review channel for pushing back on agent-authored prose, with no side-channel or separate tool. Annotated claims show an amber highlight with a margin marker; click a marker to read, edit, or remove the comment. Reachable from the keyboard, the toolbar, the Edit menu, or the pop-up over a preview selection.
- **Find** (Ctrl+F): search across the entire document — body text and table cell contents — with highlighted matches and Next/Previous navigation. In preview mode the search reaches into table cells (not just body text), including the caption of a cell that is nothing but a link. Replace (Ctrl+H) is available in edit and split modes.
- **Document outline**: a collapsible, labelled heading sidebar (View ▸ Outline, F9, or its in-panel × to close) turns a long document into a map — single-click or arrow-key a heading to jump straight to that section, with header buttons to expand or collapse the whole tree at once.
- **Annotations viewer**: a companion sidebar section (View ▸ Annotations, F8, the toolbar's flag button, or its in-panel × to close) lists every comment in the document — so a document under review stops hiding its feedback in the margins. Single-click or arrow-key an entry to jump the document to that annotation and open its comment card, exactly as clicking its margin marker would. In edit mode it moves the caret there instead. It stacks beneath the outline and toggles independently; when both are off the sidebar disappears and the content takes the full width.
- **Reading themes**: the preview is for *reading*, so it doesn't have to look like UI chrome. **Sepia** gives you a book-like page — a serif face and soft brown text on warm off-white; **Synthwave** is a midnight 80s-outrun page with neon accents; **Terminal** is a canonical ANSI/VGA page — light-gray text on true black in a monospace face; **Candy** is a sweet-shop page, lemon and turquoise and hot pink on deep indigo in a rounded face — while the toolbar, tabs, and editor stay on your desktop theme. **System** (the default) keeps the desktop-derived look. Pick one from View ▸ Reading Theme or the toolbar; it applies instantly and is remembered across sessions. Themes are plain data, not code: drop a `themes.toml` in `~/.config/scribobulate/` to tweak a shipped theme, override the colours and spacing the app would otherwise bake in, or add a theme of your own — colour, typography, and spacing all included. It never touches your text.
- **Preview zoom**: Ctrl++ / Ctrl+- / Ctrl+0 (or the View menu / toolbar buttons) scales the preview text in accessibility-friendly steps from 50% to 300%; your preferred level is remembered across sessions.
- **Split-pane arrangement**: in side-by-side mode, swap the editor and preview positions (left/right or top/bottom) and toggle between horizontal and vertical splits — both controls are in the View menu and the toolbar, and the chosen arrangement is restored at next launch.
- **Show Unsafe Images**: a View-menu toggle (and toolbar button) to load remote (http/https) image URLs and images outside the document folder — off by default so untrusted documents can't reach the network. When toggled off while images are visible, they revert immediately to a broken-image placeholder. The setting persists across sessions. **Security note:** while enabled, remote image URLs are fetched with no filtering of internal/private network addresses, so a malicious document could probe your local network (it can only *display* the fetched image — no response data is returned to the document). By design, this is left to your judgement: enable it only for documents you trust.
- **Tabs**: each window can hold several open documents (Ctrl+T for a new
  tab); the tab strip is always visible, even with just one tab in the only
  open window, so it's always there as a drag handle. Each tab has its own
  hover-revealed `×` close button (closes that specific tab without switching
  to it first), a hover tooltip showing the document's full file path (or
  "Unsaved" before it has one), and a right-click menu (Close Tab, Close Other
  Tabs, Move to New Window, Copy Full Path, Reload — the last two act on the tab
  you right-clicked, not whichever happens to be active), alongside File ▸ Close Tab (Ctrl+W), View ▸ Move Tab to New
  Window, and View ▸ Previous/Next Tab, all reachable from the View and File
  menus. View ▸ Documents lists every tab open in that window for one-click
  switching. Tabs can be dragged to reorder within a strip, dragged directly
  between two open windows — an eligible tab strip glows while a drag hovers
  over it — or dropped onto the desktop to spawn a new window (X11).
  Opening several files at once — `scribobulate *.md`, a shell glob, or a
  multi-file hand-off to an already-running instance — opens at most one new
  window with every file as a tab in it; a file that's already open
  somewhere else is simply focused there instead of duplicated.
- **One process, many windows**: opening more documents reuses the running
  process instead of spawning new ones — View ▸ New Window (Ctrl+N) opens an
  additional one deliberately.
- **Session restore**: quitting and relaunching Scribobulate reopens every
  window you had open, each with its own tabs, view mode, split arrangement,
  zoom level, and sidebar panes (outline and annotations) exactly as you left
  them. Zoom is carried into
  new windows too — pop a tab out at 150% and it stays at 150%, because a zoom
  level you chose to read comfortably shouldn't reset every time a window opens.

## Requirements

| Requirement | Details |
|-------------|---------|
| OS | Linux-first (X11 today). **macOS and Windows 10/11 x64 are both supported** — the same sources build and run on Homebrew's GTK4, where `packaging/macos/` produces a `.app` bundle, and on a gvsbuild GTK4 under MSVC, where `packaging/windows/` produces a per-user installer. Wayland remains a design goal, not a guarantee. |
| Runtime | GTK 4.6+, GtkSourceView 5, GLib. GTK ≥ 4.12 is recommended. On Windows the runtime is bundled with the app — nothing to install separately. |
| Build | Rust 2021 toolchain, `libgtk-4-dev`, `libgtksourceview-5-dev` (`glib-compile-resources`, used to bundle a couple of app-private icons, ships as part of the same GLib dev toolchain `libgtk-4-dev` already pulls in). On Windows: MSVC + [gvsbuild](packaging/windows/README.md). |
| Icons | A real icon theme — `adwaita-icon-theme` or `breeze-icon-theme`. Most desktops already have one; a minimal or non-desktop install may have only `hicolor-icon-theme`, which is an empty directory structure with no icon files, and roughly half the app's toolbar icons will render as broken-image placeholders. On macOS via Homebrew this is **not** installed with `gtk4` — `brew install adwaita-icon-theme` explicitly. On Windows the icon theme ships inside the installer, so this does not apply. Verify with `cargo test --features gtk-integration-tests --test icon_resolution`. |
| Optional | `webp-pixbuf-loader` (`apt install webp-pixbuf-loader`) enables in-app **WebP** image rendering; without it PNG/JPEG/GIF still render and a `<picture>` falls back to its `<img>` |

## Quickstart

**Linux**

```bash
sudo apt-get install -y libgtk-4-dev libgtksourceview-5-dev
cargo build --release
./target/release/scribobulate path/to/document.md
```

On **macOS** the dependencies come from Homebrew, and the icon theme is not
optional (see Requirements above):

```bash
brew install gtk4 gtksourceview5 adwaita-icon-theme
cargo build --release
packaging/macos/bundle.sh          # -> target/macos/Scribobulate.app
open target/macos/Scribobulate.app --args path/to/document.md
```

The bundle gives the app a proper Dock/Cmd-Tab identity, which a bare binary
cannot have. It is not self-contained — it needs the Homebrew dependencies
present — so it suits running the app locally rather than handing it to someone
else. `packaging/macos/README.md` has the details.

On **Windows** — run the installer (`Scribobulate-<version>-x64-setup.exe`). It installs per-user, so
there is no admin prompt, and it does **not** take over `.md` unless you tick the box. Unlike the
macOS bundle it *is* self-contained: the GTK runtime and icon theme ship inside it. Building it
from source is documented in [`packaging/windows/README.md`](packaging/windows/README.md).

By default a second launch reuses the running instance (one process, many
windows). To force a **separate** process — e.g. running a dev build alongside
your everyday instance — pass `--new-instance` (`-n`):

```bash
./target/release/scribobulate -n path/to/document.md
```

Logging is controlled by the `RUST_LOG` environment variable; app messages and
GTK/GLib diagnostics share one sink:

```bash
RUST_LOG=warn ./target/release/scribobulate            # default
RUST_LOG=info,scribobulate=debug ./target/release/scribobulate
RUST_LOG=warn,scribobulate::scroll=trace ./target/release/scribobulate
```

**If it ever crashes, your unsaved work is not lost.** While a document has unsaved
changes they are snapshotted to your state directory a few seconds after you stop
typing (and immediately when you switch away), so an unclean exit — a crash, an
out-of-memory kill, a power cut — costs you seconds of work rather than the session.
On the next launch the affected documents come back exactly as they were, still marked
unsaved, with a notice offering **Keep** or **Discard recovery**. Your own files are
never written to without an explicit save, and the snapshots are readable only by you.
They are deleted the moment they are no longer needed: saving, undoing back to the
saved content, or discarding a tab all remove them, so a clean quit leaves nothing
behind.

It also says so on the next launch and leaves the evidence in
your state directory (`${XDG_STATE_HOME:-~/.local/state}/scribobulate/`, or
`%LOCALAPPDATA%\scribobulate\` on Windows):

- `scribobulate.log` — a rolling log of what the app has been doing, kept
  regardless of `RUST_LOG`.
- `crash-<timestamp>-<pid>.log` — written at the moment of death: the build, the
  fault, and the last 64 things the application did before it happened.

Neither file records any of your document's text — paths, sizes and event names
only — so a crash report is safe to attach to a bug report as-is. (The recovery
snapshots described above *do* contain your text, by necessity — they are what your
work is recovered from — which is why they live in a directory only you can read and
are deleted as soon as the document is saved.)

## Documentation

Detailed documentation lives in the `sdd/` directory:

- `sdd/PRODUCT.md` — Product definition and rationale
- `sdd/TECH.md` — Technical architecture and system diagram
- `sdd/TDD.md` — Test specifications (Given/When/Then rubrics)
- `sdd/POLICY.md` — Development rules and constraints
- `sdd/ANTI-PATTERNS.md` — Why the native-widget stack was chosen
- `sdd/ISSUES.md` — Known issues

## License

Apache License, Version 2.0 — see [LICENSE](LICENSE) for the full text.
