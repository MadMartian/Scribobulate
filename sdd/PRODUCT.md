# Product Definition

## What is Scribobulate?

Scribobulate is a lightweight, cross-platform Markdown viewer and editor —
Linux, macOS and Windows, from one source tree — built first
and foremost to read and manage the Markdown that AI agents produce — the plans,
design notes, reports, and memory files that agents generate and rewrite
continuously. It renders Markdown with full fidelity — headings, tables,
syntax-highlighted code, task lists, and images — using native widgets that draw
entirely on the CPU, so it consumes no meaningful GPU memory. It watches open
files and reloads them the instant they change on disk, keeping the rendered view
in lockstep with whatever an agent is writing. It also closes the loop the other
way: a reader can **annotate** the rendered document — highlight a claim and attach
a comment that is written back into the Markdown file itself — so a human can
review and push back on agent-authored prose, and the agent (or author) reads that
feedback inline on the next pass. Human ⇄ agent collaboration on the same Markdown
file, in both directions, is the point.

## What problem does it solve?

Existing Markdown viewers are surprisingly heavy for what they do. The immediate
motivation is another popular, GPU-canvas-based Markdown viewer that holds ~594
MiB of video RAM to display a single document, because it renders every pixel
onto a GPU surface instead of using native widgets. For a tool whose entire job
is to show formatted text, that trade-off is indefensible.

- **GPU memory waste**: Scribobulate renders through native widgets on the CPU,
  holding effectively zero video RAM — the same footprint profile as a native
  editor like Kate, not a self-rendering GPU application.
- **Stale previews during agent collaboration**: When an AI agent edits a
  Markdown file on disk, Scribobulate detects the change and reloads
  automatically, so the view is never out of date with the file.
- **Lost work from background edits**: When the file changes on disk while the
  user has unsaved edits, Scribobulate surfaces the conflict and lets the user
  choose, rather than silently discarding either side. When the file is instead
  *deleted* out from under an open document, its tab is flagged with a warning
  marker and — since the open buffer now holds the only copy — closing it prompts
  to save first, exactly as an unsaved document would.
- **Unsaved work lost to an unclean exit**: Every safeguard above protects a
  document from being overwritten; none of them protected it from the application
  simply dying. A crash, an out-of-memory kill or a power cut discarded everything
  typed since the last save, which is the asymmetry a reader notices most — the
  save path goes to considerable lengths so a crash *mid-write* cannot tear a file,
  while a crash *between* writes silently cost the whole session. Scribobulate now
  snapshots a document's unsaved edits while they are unsaved, and offers them back
  on the next launch, still marked unsaved and reversible.
- **No in-file review channel for agent-authored prose**: When an agent writes a
  plan, design note, or report, a human reviewer had no way to mark up a specific
  claim in place. Scribobulate lets the reviewer highlight any span in the rendered
  view and attach a comment stored as CriticMarkup in the same file, so the
  feedback travels with the document and the agent reads it directly — no
  side-channel, no separate review tool.

## Who is it for?

- Developers who collaborate with AI agents that frequently rewrite Markdown
  files and want a live, accurate preview of the current file state.
- Anyone on Linux, macOS or Windows who keeps many Markdown documents open and
  refuses to pay hundreds of megabytes of GPU memory for a text viewer.
- Writers and note-takers who want a fast viewer with an optional editing pane
  and live preview.
- Anyone who edits Markdown in their own editor and wants a faithful,
  auto-refreshing rendered view beside it.

## Why does it exist?

- **Another popular Markdown viewer** renders Markdown faithfully, but its
  GPU-canvas engine holds ~594 MiB of GPU memory per window and bypasses the
  system compositor entirely.
- **Browser-based and Electron viewers** render accurately but carry a full web
  engine and its memory cost, including separate renderer processes per document.
- **Terminal Markdown viewers** are light, but cannot show images, real tables,
  or a graphical editing experience.

Scribobulate fills the gap: a graphical, full-fidelity Markdown viewer and
editor with the resource footprint of a native desktop application.

## How is it used?

1. Open a Markdown file from the command line or the file chooser; its rendered
   form appears immediately.
2. Read the rendered document, or switch to the editor pane to make changes with
   live preview — applying Markdown formatting (bold, headings, code, quotes, and
   more) from the Format menu, the toolbar, or a pop-up at the selection. Or, while
   reading, select any claim in the preview and **annotate** it: a pop-up offers
   *Annotate*, you type a comment, and it is saved into the file — the claim is
   highlighted and a margin marker holds the comment for the author to read, edit,
   or resolve later.
3. Use the outline sidebar to see the document's heading structure at a glance and
   jump to any section with a click (toggle it from the View menu or with F9), and
   the **annotations viewer** beneath it to see every comment in the document at
   once and jump to any one — so a document under review no longer hides its feedback
   in the margins. Either sidebar hands the keyboard over when you show it, and the
   comments can be walked forwards and backwards without the pointer at all, which
   matters because the margin markers are drawn rather than built from widgets and a
   screen reader cannot reach them directly. Choose a **reading theme** for the preview — the default follows the desktop, or
   pick a book-like page (View ▸ Reading Theme, or the toolbar). Scribobulate ships
   **System** (desktop-matching), **Sepia**, **Bedtime**, **Synthwave**, **Terminal**, and **Candy**; the
   choice is remembered, and themes are plain data a reader can add to or adjust.
4. Leave Scribobulate open while an AI agent (or any other program) edits the
   file — the view reloads automatically as the file changes.
5. If the file changes on disk while you have unsaved edits, choose whether to
   reload the new version or keep your own changes.
5a. If Scribobulate itself dies with unsaved edits open, reopen it: the affected
   documents come back as they were, still marked unsaved, each offering **Keep**
   or **Discard recovery**. Nothing was written to your files in the meantime.
6. Open additional documents: File ▸ New Document (Ctrl+T) adds a blank tab to
   the current window; opening a file from disk loads into a blank tab if one
   is available in the current window, or otherwise opens in a new window
   (View ▸ New Window opens one explicitly). Naming several files at once —
   a shell glob, multiple command-line arguments, or a hand-off to an
   already-running instance — opens at most one new window, with every file
   as a tab in it, except any file already open elsewhere, which is simply
   focused instead of duplicated. All of this stays within the single
   running Scribobulate process. Tabs within a window can be closed, dragged
   to reorder, cycled with the keyboard, moved out into their own window, or
   dragged directly into a different open window. **Back** and **Forward** retrace
   the documents you have been reading in that window, and the sections within
   them — following a table-of-contents link, or an outline entry, is a step you
   can take back, returning to the position you followed it from. The same keys
   and mouse buttons a web browser uses, plus the View menu and the toolbar.
   A tab's file can also be **renamed** where it sits (F2, File ▸ Rename, or the
   tab's context menu) — within its own folder and by name only, so the links and
   images it resolves relative to itself keep working.
7. Hand the document to someone who does not have Scribobulate: File ▸ Export
   writes it as a self-contained **HTML** file — the sharing format, which
   re-flows to the reader's window and keeps its links and heading anchors — or
   as a paginated **PDF**, the record format that prints and archives unchanged.
   Local images are embedded rather than referenced, so the artefact survives
   being moved or sent, and the review comments travel with it.
8. Quit and come back later: every window reopens with its own tabs, view
   mode, split arrangement, and zoom level exactly as they were left.

## Vocabulary

| Term | Definition |
|------|-----------|
| **Document** | A single Markdown file open in Scribobulate. |
| **Preview** | The rendered, formatted view of a document. |
| **Source** | The editable Markdown text of a document. |
| **Reading theme** | A named appearance for the *preview* — its page colour, typeface, and spacing — chosen for reading rather than matching the desktop UI. Authored as plain data; the default reproduces the desktop's own colours, and a reader can add or adjust themes without touching the app. Applies to the preview only; the rest of the app follows the desktop theme. |
| **Live reload** | Automatically re-reading and re-rendering a document when its file changes on disk. |
| **Conflict** | A file changing on disk while the user has unsaved edits to it. |
| **Swap file** | A periodic full-content snapshot of a document with unsaved edits, kept in the user's state directory so an unclean exit does not discard them. Borrowed from vim, with one difference worth keeping in mind: it is a snapshot rewritten on a debounce, not an incremental journal, and it is not a lock. |
| **Tab** | One document's view within a window; a window holds one or more tabs, always shown in a tab strip (even a lone tab, so it always has a drag handle). |
| **Window** | A titled top-level GTK window holding one or more tabs; many windows share one Scribobulate process. |
| **Single instance** | The guarantee that launching Scribobulate again reuses the running process instead of starting a new one. |
| **Annotation** | A reviewer's comment attached to a span of a document, stored inline in the Markdown file as CriticMarkup so it travels with the file. |
| **Highlight** | The span of text an annotation refers to, shown with a coloured background in the preview. |
| **Comment marker** | The margin indicator in the preview that reveals an annotation's comment — and its Edit / Remove actions — when clicked, or when the reader walks onto that annotation from the keyboard. |
| **Annotation card** | The small panel a comment marker opens, showing one annotation's claim and comment with its Edit / Remove actions. It stays attached to its marker: it moves as the document scrolls, steps aside when its marker scrolls out of view and returns with it, and closes on Escape, on a click anywhere else, or from the × in its corner. |
| **Export** | Writing the document out in a presentation format — HTML or PDF — rather than as Markdown. Distinct from Copy Document and copy-as-Markdown, which are deliberately *round-trip* formats: an export is for a reader, not for another editor. |
| **CriticMarkup** | The plain-text convention Scribobulate uses to store annotations in the file (`{==highlight==}{>>comment<<}`), readable in any editor. |
