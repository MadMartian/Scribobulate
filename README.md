# Scribobulate

A multi-platform native Markdown viewer and editor for **Linux**, **macOS**, and **Windows** —
for the documents AI agents generate. **Comment on what an agent wrote, right inside the
Markdown**, so the next agent reads your objection where you made it. Renders full-fidelity
Markdown on the CPU with effectively zero GPU memory (leaving room for those hungry Ollama
models), and live-reloads files the instant an agent changes them on disk.

<picture>
  <source srcset="assets/splash.webp" type="image/webp">
  <img src="assets/splash.gif" alt="Introducing Scribobulate — a looping tour of reading themes, the outline, the live-render pipeline, split editing, and in-document annotations">
</picture>

![Scribobulate](data/icons/scalable/apps/com.extollit.scribobulate.svg "Scribobulate")

> **Status: pre-release, and capable.** Rendering, the editing pane (with live
> split-preview), live reload, and conflict handling all work today —
> Scribobulate opens and displays full-fidelity Markdown (tables,
> syntax-highlighted code, task lists, images) at 0 MiB VRAM / ~80 MiB RAM as a
> single-instance multi-window, multi-tab app, reloads files as they change on disk, and
> when a change lands under unsaved edits it prompts you to reload or keep your
> work (a clean change reloads silently with a brief notice). Unsaved edits also
> survive the application dying: they are snapshotted as you type and offered back
> on the next launch.
>
> The first release is close. What I am still finding is minor — small glitches and
> polish, not gaps in the fundamentals above — and I am still finding them most days,
> which is the one reason I have not cut it yet. I will cut the release when that rate
> drops off rather than on a date, and I expect that to be weeks away rather than
> months. If you hit something in the meantime, [tell me](#feedback) — reports are what
> make that rate fall.

## What it does

Scribobulate is built for working alongside AI agents that rewrite Markdown
continuously: it watches the open file and re-renders automatically the moment an
agent changes it, so the plans, notes, and reports your agents produce are always
shown up to date. It displays your Markdown the way you expect — headings,
tables, syntax-highlighted code, task lists, and images — in a fast native GTK
window, with an optional editing pane and live preview. A file that opens with
YAML or TOML front matter, as agent definitions and static-site pages do, shows
it folded away at the top rather than as a wall of text where the title should
be. When a file changes
underneath unsaved edits, Scribobulate asks you what to do instead of silently
discarding your work — and if the application itself dies, your unsaved work is
waiting for you when you reopen it. And when the agent's prose needs an argument
rather than an edit, you comment on it in place.

### Annotate the document, in the document

Reviewing what an agent wrote used to mean copying a paragraph into a chat window and
explaining where it came from. Here you select the claim, leave a comment, and the comment
is saved *in* the Markdown as portable markup — so the next agent reads your objection
exactly where you made it, and answers it in place. No sidecar file, no database, no second
tool: the review travels with the document, through Git, into the HTML and PDF you export.

- Margin markers to open, edit, or remove a comment
- Annotations sidebar listing every comment in the document (F8)
- Walk the comments from the keyboard (Ctrl+Alt+N / Ctrl+Alt+P)

I have not found another Markdown editor that does this, and it is the feature I reach for
most when working with an agent.

> *One thing we're both very proud of is using Scribobulate to improve Scribobulate!*
> -- *MadMartian*

## Why I built it

I run models locally. [Ollama](https://ollama.com/), on my own hardware. I use Claude too, and this
project was built with it, which [AI-DILIGENCE.md](AI-DILIGENCE.md) accounts for in
full. What I want is for the local option to stay open, because freedom from cloud
computing and cloud providers should always be available to a developer. That is the
first reason Scribobulate renders on the CPU and holds 0 MiB of VRAM: every megabyte
of GPU memory it leaves alone is a megabyte your models get to use.

The second reason is that I wanted it to look beautiful. A tool for humans working
with AI should be a pleasant place for the human to sit, so the reading themes
(Sepia, Bedtime, Synthwave, Terminal, Candy, Pixel Quest) and the theming system behind them got
real attention, and the theme you choose travels into the HTML you export (a PDF
always prints in the plain light style, since paper has no dark mode).

The third reason is the one at the top of this page: I wanted to argue with an agent's
prose in the margin of the file it wrote, not in a chat window that has no idea which
paragraph I mean.

## Features

*(Shortcuts below are shown in Linux/Windows form. On macOS, Ctrl becomes ⌘ —
see Help ▸ Keyboard Shortcuts in the app for the exact mapping.)*

- **Annotate in the file** — comments saved *in* the Markdown, as described above:
  margin markers to open, edit, or remove one; an annotations sidebar (F8) listing
  every comment, sharing the sidebar's height with the outline on a divider you drag
  where you want it; Ctrl+Alt+N / Ctrl+Alt+P to walk them from the keyboard without
  hunting for markers.
- **Light on your machine** — native rendering on the CPU, not a browser engine.
  Leaves GPU memory free for the models and tools you actually care about.
- **Live reload** — when an agent (or anything else) rewrites an open file, the
  preview updates immediately and your place in the document is kept. However it
  was written — in place, or the temp-and-rename dance most editors do — it reads
  as one edit, not as the file being destroyed and rebuilt.
- **Your edits stay yours**
  - **Conflicts** — if a file changes while you have unsaved work, you're asked
    whether to reload or keep your version. Never a silent overwrite.
  - **Emptied or deleted files** — if an open file is deleted, or wiped blank by
    a write that went wrong, the open copy is kept rather than reloaded blank:
    the tab is flagged, closing it asks first, a crash can't lose it, and a
    prompt offers the one save that puts the file back. Dismiss it and the
    guards all stay — you're still holding the only copy.
  - **Crash recovery** — unsaved edits come back after a crash, still marked
    unsaved, with the choice to keep or discard. Your file is never written
    without an explicit save.
  - **Rename in place** — F2, or right-click the tab, renames the file a tab is
    reading without leaving the app. Same folder, name only; every surface
    follows at once and nothing is re-read.
- **Full-fidelity Markdown**
  - Tables, syntax-highlighted code, images, and task-list checkboxes
  - Collapsible `<details>` sections — click a summary line to fold a verbose
    aside away, and the document keeps your place rather than jumping. Find,
    the outline and copy all reach inside a collapsed block, so folding hides
    it from the page without hiding it from you
  - Hover a code block for a copy button — one click puts the code on the
    clipboard, fences and all container markers left behind
  - Clickable links that show where they lead when you hover them
  - Follow a link to another Markdown file and it opens as a tab — read a
    whole document set without hunting files by hand
- **Edit with a live preview**
  - Split view, editor-only, or preview-only
  - Format toolbar and shortcuts for bold, italic, headings, lists, quotes,
    code, task lists, and more
  - Lists and quotes continue as you type; fenced code blocks close themselves
- **Take the document with you** — File ▸ Export writes what you are reading as
  a standalone **HTML** file to share or a paginated **PDF** to keep. Images
  travel inside the file, so it still works after you send it; your annotations
  come along, and the HTML wears the reading theme you chose (the PDF always
  prints in the plain light style).
- **Find what you need**
  - Search the whole document, including table cells (Ctrl+F)
  - Replace in edit and split modes (Ctrl+H)
  - Outline sidebar jumps to any heading (F9)
- **Comfortable reading**
  - Reading themes: **Sepia**, **Bedtime**, **Synthwave**, **Terminal**,
    **Candy**, **Pixel Quest**, or match your desktop (**System**) — and you can adjust one, or
    write your own, in `themes.toml` (`man 5 scribobulate`)
  - Zoom the preview from 50% to 300% — on Ctrl+wheel, as anywhere else — and
    it is remembered across sessions. **Images zoom too**, and a diagram in SVG
    is re-drawn at the new size rather than blown up, so the small print inside
    it stays sharp instead of turning to mush
  - Arrange the split any way you like (left/right or top/bottom)
- **Tabs and windows that stay out of the way**
  - Several documents per window; drag tabs to reorder, between windows, or
    out to a new window
  - One process for the whole session — open more files without spawning more apps
  - Back and Forward through the documents you have been reading — and the
    sections within them, so following a table-of-contents link is a step you can
    take back — on the keys and mouse buttons your browser uses (Alt+←/→, or the
    thumb switches)
  - Session restore brings back windows, tabs, zoom, split, and sidebars —
    including where each window's sidebar divider sat
- **Safe by default**
  - Links and images stay inside the document's folder unless you opt in
  - Remote images are off until you enable them for documents you trust
- **Familiar desktop app** — native menus, toolbar, keyboard shortcuts, and
  in-app help (F1 for the shortcut list)

## Quickstart

### Linux

Install the package — no Rust toolchain, no development libraries:

```bash
sudo apt install ./scribobulate_0.1.0_amd64.deb     # Debian, Ubuntu
sudo dnf install ./scribobulate-0.1.0-1.x86_64.rpm  # Fedora, RHEL
```

Or build from source, which is what you want if you intend to change it:

```bash
sudo apt-get install -y libgtk-4-dev libgtksourceview-5-dev
cargo build --release
./target/release/scribobulate path/to/document.md
```

Or install from source into `~/.local` — binary, desktop entry, icon, themes, both
manual pages and the third-party notices — which also registers Scribobulate as a
Markdown handler so a double-click in your file manager opens it:

```bash
./install.sh          # needs cargo and the -dev libraries above
./uninstall.sh        # removes what it installed
```

`./install.sh` and `./uninstall.sh` are the entry points for the platforms that
install **from source** — Linux and macOS. They hold no install logic: each is a
`uname -s` router that dispatches to `packaging/<os>/`, so the same two commands
work on both and each platform's answer lives in one place. (Windows installs
from a prebuilt installer instead, and both scripts refuse there by design — see
[Windows](#windows) below.) Details:
[`packaging/linux/README.md`](packaging/linux/README.md).

To build the packages yourself, `packaging/linux/build-deb.sh` and
`build-rpm.sh` (the latter needs `rpm` installed on a Debian host). All three routes
take the version from `Cargo.toml` and install the same payload, defined once in
`packaging/linux/payload.sh`.

### macOS

Install the Homebrew GTK dependencies first — skipping this is the most common
build failure (`pkg-config` can't find `gtk4`/`cairo`/`pango`/etc.):

```bash
brew install gtk4 gtksourceview5 adwaita-icon-theme
```

Then build and package:

```bash
cargo build --release
packaging/macos/bundle.sh          # -> target/macos/Scribobulate.app
open target/macos/Scribobulate.app --args path/to/document.md
```

Or `./install.sh` to also get a `scribobulate` command on PATH:

```bash
./install.sh          # app -> ~/Applications,  CLI -> ~/.local/bin
sudo ./install.sh     # app -> /Applications,   CLI -> /usr/local/bin
scribobulate path/to/document.md
./uninstall.sh        # removes the app, the symlink and the manual pages
```

`/usr/local/bin` is on the stock macOS search path; `~/.local/bin` is not, and the
per-user install prints the line to add if it is missing from yours. Nothing is written
into Homebrew's prefix — Homebrew is a build dependency here, not an install location.

**Use `sudo` if you want the app in the `Applications` folder Finder shows you.** That
folder is `/Applications`; `~/Applications` is a separate one inside your home
directory. Without `sudo` the install succeeds, registers, and works from Spotlight
and the Dock — and still looks like it did nothing, because the folder you go and
look in is not the folder it installed to. Uninstall in the mode you installed with.

Either way it points the `scribobulate` command and the manual pages at that copy and
then removes the build copy — so `cargo clean` cannot quietly break your install, and
the machine is left holding exactly one copy.

**It refuses to run while another copy is already installed**, including one put
there by the other mode, or dragged to `/Applications` from a `.dmg`. All carry the
same bundle identifier, so macOS would let the Dock and the terminal launch different
copies with nothing to tell you they had diverged. The refusal names the command that
clears the other copy.

`./install.sh` dispatches to `packaging/macos/install.sh`; running that directly
is equivalent. Use `bundle.sh` above when you want the `.app` and nothing on your
PATH. `./uninstall.sh` removes what it installed, verifies each removal actually
happened, and **reports — never deletes —** a copy it did not put there.

More: [`packaging/macos/README.md`](packaging/macos/README.md).

### Windows

Build from source. The GTK runtime comes from
[gvsbuild](https://github.com/wingtk/gvsbuild), built once per machine; after
that `build.bat` sets the whole toolchain environment for you:

```powershell
.\packaging\windows\build.bat release
.\packaging\windows\build.bat run path\to\document.md
```

Or build the per-user installer, which puts Scribobulate on the Start menu
instead of in `target\release`:

```powershell
.\packaging\windows\package.ps1     # -> build\installer\Scribobulate-<version>-x64-setup.exe
```

It installs to `%LOCALAPPDATA%\Programs\Scribobulate`, carrying the GTK runtime
inside it.
Scribobulate and those GTK libraries import `VCRUNTIME140.dll`, which Windows
does not include — the UCRT that *is* part of Windows 10 and later is a
different runtime — so the installer also carries Microsoft's own
redistributable and runs it on the machines that need it. **Uninstall through
Settings ▸ Apps**, or the Start menu's *Uninstall Scribobulate* shortcut;
`./uninstall.sh` is for the platforms that install from source and refuses here
rather than half-removing an install it did not create.

The prerequisites, the one-time gvsbuild step and what to do when a build fails:
[`packaging/windows/README.md`](packaging/windows/README.md).

### Tips (all platforms)

A second launch reuses the running process. To force a **separate** instance
(e.g. a dev build beside your everyday one), pass `--new-instance` (`-n`):

```bash
./target/release/scribobulate -n path/to/document.md
```

Logging uses `RUST_LOG` (app and GTK/GLib share one sink):

```bash
RUST_LOG=warn ./target/release/scribobulate            # default
RUST_LOG=info,scribobulate=debug ./target/release/scribobulate
```

Unsaved work survives a crash: snapshots restore on the next launch with
**Keep** or **Discard recovery**. Your own files are never written without an
explicit save. Crash reports land in your state directory
(`~/.local/state/scribobulate/` or `%LOCALAPPDATA%\scribobulate\`) and do not
include document text — safe to attach to a bug report.

## Feedback

Bugs and suggestions go in [GitHub issues](https://github.com/MadMartian/Scribobulate/issues) —
there is a short form for each. Questions and anything that isn't quite either belong in
[Discussions](https://github.com/MadMartian/Scribobulate/discussions). Rough reports are
welcome; a problem nobody mentions is a problem nobody fixes.

**Check two places for it first**: the
[open issues](https://github.com/MadMartian/Scribobulate/issues), and
[`sdd/ISSUES.md`](sdd/ISSUES.md) in this repository.

The second one needs explaining, because it is not a public tracker and does not behave
like one. `sdd/ISSUES.md` is the maintainers' own working register: short-lived entries
that exist in order to be deleted, written for whoever picks the work up next, and
expected to disappear as the file empties. It is in the repository because the people
working on this project read it there, not because it is where you file things.

The exception is worth knowing about, and it has its own table near the top of the file:
**Closed issues**. An entry found **intractable** — a real limitation with no reachable
fix, often something the toolkit or the platform owns — moves there rather than being
deleted, so nobody re-investigates a settled dead end. Those are the ones that affect you and are not going away, and they get promoted to
GitHub issues in due course so they are visible where you would actually look. If you find
one there that matters to you and has not been promoted yet, say so — that is useful
signal about which limitations people actually hit.

## Documentation

Detailed documentation lives in the `sdd/` directory:

- `sdd/PRODUCT.md` — Product definition and rationale
- `sdd/TECH.md` — Technical architecture and system diagram
- `sdd/TDD.md` — Test specifications (Given/When/Then rubrics)
- `sdd/POLICY.md` — Development rules and constraints
- `sdd/ANTI-PATTERNS.md` — Register of this project's own costly dead ends
- `sdd/ISSUES.md` — Known issues

## How this project is built

Scribobulate is built with AI agents working under human direction, and
[AI-DILIGENCE.md](AI-DILIGENCE.md) is the full accounting: what tooling is used,
what is disclosed, and what has to pass before a change reaches you. Short
version: the application itself ships no AI features, no telemetry, and one
opt-in outbound connection.

## License

Apache License, Version 2.0 — see [LICENSE](LICENSE) for the full text.
