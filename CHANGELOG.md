# Change log

Newest first. Each release lists what a user can see — new, changed, fixed, and
anything platform-specific — and leaves out what a user cannot (refactors, test and
register work). The release notes on GitHub copy the section for that version and add
Known issues, Requirements and the signing disclosure.

## v1.0.0 — 2026-11-14

The first release. There is no previous version to diff against, so this entry is the
product as it stands.

### New

- **Annotate in the file.** Select text in the preview or the editor and press
  Ctrl+Alt+M (or use Edit ▸ Annotate, the toolbar, or the context menu) to attach a
  comment. Comments are saved *in* the Markdown as CriticMarkup
  (`{==claim==}{>>comment<<}`), so they travel with the file and any other tool —
  including an AI agent — can read them. The annotations sidebar lists them; the
  Shift+F8 filter finds one by text.
- **Full-fidelity Markdown preview** — tables, syntax-highlighted code, task lists,
  images (including animated and SVG), front matter, collapsible sections, and the
  CriticMarkup annotation marks above.
- **Edit with a live preview.** Side-by-side or single pane, the preview follows the
  caret, and the outline sidebar jumps to a heading in either. Block commands (headings,
  quotes, lists, task lists, code blocks) keep the document's own line endings.
- **Live reload.** When anything rewrites an open file — an agent, a sync client,
  another editor — the preview updates in place. A change that lands under unsaved
  edits asks whether to reload or keep your work; a clean change reloads with a brief
  notice.
- **Your edits stay yours.** Unsaved edits are snapshotted as you type and offered back
  on the next launch if the application dies — one tab per snapshot, even when the same
  file was open twice.
- **Export** what you are reading as HTML or PDF, with annotations rendered as
  highlighted claims and margin comments; a PDF table row taller than a page continues
  onto the next page.
- **Find and replace** across the document, with match highlighting in the preview;
  Back/Forward history across tabs and heading jumps.
- **Themes** — seven reading themes built in (System, Sepia, Bedtime, Pixel Quest and
  more), plus your own in a small TOML file; the theme travels into the HTML you export.
  Zoom the preview from 50% to 300% with Ctrl+wheel; it is remembered across sessions.
- **Tabs and windows that stay out of the way** — a single instance, many windows,
  session restore of every window's tabs, split and divider positions.
- **Light on your machine** — rendering on the CPU, 0 MiB of GPU memory and about
  80 MiB of RAM for a typical document.
- **Safe by default** — links and images stay inside the document's folder unless you
  opt in per tab; remote images are off until you enable them; files that are not text
  (invalid UTF-8, or containing a NUL byte) are refused rather than opened as an empty,
  saveable document.
- **Familiar desktop app** — native menus, toolbar, keyboard shortcuts, in-app help
  (F1), and `.deb`, `.rpm`, `.dmg` and Windows installer packages with file-type
  registration.

### Platform-specific

- **Linux** — on X11 the window remembers its desktop, activity, monitor and position
  across sessions; `.deb` and `.rpm` packages register the Markdown file type.
- **macOS** — one native menubar follows the active window; dialogs over a full-screen
  window stay on its Space; a second launch forwards its arguments to the running app.
- **Windows** — an installer with file-type registration (`.md` ▸ Scribobulate) and a
  desktop shortcut.
