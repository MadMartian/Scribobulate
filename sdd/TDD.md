# Test-Driven Development Rubrics

| # | Functional area | Rubrics |
|---|-----------------|---------|
| 1 | Opening & displaying documents | 1.1 – 1.11a |
| 2 | Rendering fidelity | 2.1 – 2.26l |
| 3 | Live reload (external edits) | 3.1 – 3.4 |
| 4 | Editing & saving | 4.1 – 4.9 |
| 5 | Reconciliation (conflict handling) | 5.1 – 5.4 |
| 6 | Resource footprint (viability gate) | 6.1 – 6.5 |
| 7 | Window & layout | 7.0b – 7.23 |
| 8 | Single-instance lifecycle | 8.1 – 8.7 |
| 9 | Menu bar, toolbar, and actions | 9.1 – 9.36 |
| 10 | Markdown formatting commands | 10.1 – 10.20 |
| 11 | Find & replace | 11.1 – 11.10 |
| 12 | Document outline | 12.1 – 12.22 |
| 13 | Preview zoom | 13.1 – 13.10 |
| 14 | Show Unsafe Images | 14.1 – 14.10 |
| 15 | Tabbed documents | 15.1 – 15.22 |
| 16 | Keyboard-shortcuts help & status surfaces | 16.1 – 16.9 |
| 17 | Annotation & review (CriticMarkup) | 17.1 – 17.53 |
| 18 | Preview reading themes | 18.1 – 18.53 |
| 19 | Local document-link navigation | 19.1 – 19.13 |
| 20 | Annotations viewer | 20.1 – 20.18 |
| 21 | Crash forensics | 21.1 – 21.12 |
| 22 | Crash recovery (swap files) | 22.1 – 22.16 |
| 23 | Back / Forward navigation history | 23.1 – 23.14 |
| 24 | Renaming an open document | 24.1 – 24.14 |
| 25 | Exporting a document | 25.1 – 25.24 |

---

## 1. Opening & displaying documents

### 1.1 Open from command line
- **Given** a path to an existing `.md` file passed as a launch argument
- **When** the application starts
- **Then** the file's rendered content is displayed in the preview pane

### 1.2 Open through the interface always lands in the current window
- **Given** the application is already running, with a window focused
- **When** the user opens a Markdown file via File ▸ Open (the file chooser)
- **Then** the file lands as a tab of THAT window — reusing a blank/untouched tab if one exists (1.5), otherwise added as a genuinely new tab — never spawning a separate window merely because no tab happened to be blank
- **And** this differs from a CLI/D-Bus batch launch's narrower rule (1.6/15.15): File ▸ Open is always invoked from a specific window the user is already working in, so it always targets that window, while a batch launch has no such window in mind and falls back to a brand-new one when nothing blank is available to reuse

### 1.3 Open a path that does not exist
- **Given** a launch argument pointing to a file that does not exist
- **When** the application starts
- **Then** the user is shown an empty editable document targeting that path (creatable on save), not an error crash

### 1.4 Open a very large document
- **Given** a Markdown file of several megabytes
- **When** it is opened
- **Then** the document renders and the window stays responsive (no indefinite freeze)

### 1.4c Document file access never freezes the window
- **Given** a document on a slow or unresponsive filesystem (a stalled network share, a spun-down drive, a synced folder)
- **When** it is opened, saved, reloaded, or re-read because it changed on disk
- **Then** the window keeps redrawing and responding to input for the whole time the filesystem takes to answer — the operation completes, fails or is refused when the answer arrives, and nothing is lost by waiting
- **And** this holds for a session restore of many documents too: the windows already on screen stay live while the remaining tabs' files are read

### 1.4d A document read the application is still waiting on cannot delay a crash-recovery snapshot
- **Given** several open documents whose files are being read at once — a checkout or a sync rewriting a whole tree — on a filesystem that is slow to answer
- **When** a crash-recovery snapshot of an unsaved document falls due
- **Then** it is written promptly rather than queued behind the document reads: the application's own file access never occupies enough of the shared I/O capacity to delay the mechanism that protects unsaved work

### 1.4e A document operation that takes a moment says so
- **Given** an Open, Save, Save As or Reload whose filesystem is slow to answer
- **When** the operation has been outstanding for about half a second
- **Then** the status bar reports it ("Saving…", "Reloading…", "Opening…") and stops reporting it the moment the operation ends, however it ends
- **And** an operation that completes faster than that shows **nothing at all** — the indicator must never blink on an ordinary local-disk save, or it becomes noise and stops being read
- **And** the live-reload watcher's own unprompted re-reads are silent: only an operation the user asked for reports progress

### 1.4a Refuse a path that is not a document
- **Given** a launch argument or an Open selection naming a path that is not a regular file (a FIFO, socket or device) or a regular file larger than the load limit
- **When** the application tries to open it
- **Then** the path is refused before it is read, the tab shows a refusal notice naming the reason, and the tab carries no backing path — so a later save cannot write the notice over the user's file
- **And** a path that merely does not exist is unaffected, still opening as an empty creatable document (1.3)

### 1.4b Survive a pathologically structured document
- **Given** a small Markdown file whose structure is nested far past anything a person would write (e.g. thousands of nested blockquotes)
- **When** it is opened
- **Then** the document opens and the application stays alive — no crash and no freeze — with copy-as-Markdown remaining correct over the whole document even where it is no longer character-precise inside the over-nested construct

### 1.5 Opening a file from a blank window reuses its blank tab
- **Given** a window containing a blank/untouched tab (from File ▸ New Document, View ▸ New Window, or a no-argument launch) that the user has not edited
- **When** the user opens a Markdown file from that window
- **Then** the file loads into that blank tab in place, considering every tab in the window — not only the one currently active — so no extra empty tab or window is left behind

### 1.6 Reuse never discards unsaved work (CLI/D-Bus batch launch only)
- **Given** no tab in the active window qualifies as blank/untouched (every tab has unsaved edits)
- **When** a CLI or D-Bus batch launch (a second `scribobulate <file>` invocation, or an `open` forward to the running instance) has no reusable blank tab to target
- **Then** the file(s) open in a brand-new window, and every existing window and tab with unsaved edits is left completely untouched
- **And** this rubric does NOT apply to interactive File ▸ Open — see 1.2, whose own "And" clause is the authoritative statement of File ▸ Open's contract: it always lands in the window the user invoked it from (reusing a blank tab if one exists, else adding a genuinely new tab), regardless of whether every existing tab is dirty. A batch launch has no "window the user is already working in" to target, which is why it falls back to a new window instead.

### 1.7 Opening or restoring many documents does not freeze the UI
- **Given** a large batch of Markdown files opened at once (a CLI/D-Bus batch launch, or a File ▸ Open multi-selection) **or** a saved session with many tabs being restored at startup
- **When** the application loads them
- **Then** the first (visible) document renders and the window becomes interactive promptly, while the remaining documents' tabs load in the background — the UI never freezes rendering every document up front
- **And** both bulk-load paths (open and restore) share one deferred-rendering mechanism: a background tab's preview is built either by the pre-render pump (one tab per timer tick, so input and paint interleave) or on the tab's first activation, whichever comes first — a tab restored in a non-default view mode / split arrangement replays that layout when it is first activated (7.2)
- *(File reads themselves stay synchronous; it is the per-document rendering, not the I/O, that is deferred.)*

### 1.8 A still-loading background tab shows a loading spinner
- **Given** one or more tabs added in the background by a bulk open/restore whose previews have not yet been rendered
- **When** the tab strip is shown
- **Then** each not-yet-rendered tab displays a small loading spinner beside its title, which clears the moment that tab's preview is built (by the pre-render pump or on first activation), so the user can see which tabs are still pending

### 1.9 A document that begins with a UTF-8 byte-order mark
- **Given** a Markdown file whose bytes begin `EF BB BF` — the encoding Windows tooling emits by default (PowerShell's `Set-Content -Encoding utf8`, and Notepad for years), so an ordinary authoring path rather than a curiosity
- **When** it is opened
- **Then** it renders exactly as the byte-identical file without the mark: a leading `# Heading` is a heading and appears in the outline, rather than the mark being carried into the first token and the whole first line rendering literally
- **And** only the document's *first* character is treated this way — a `U+FEFF` anywhere else is ordinary content and is preserved
- **And** the mark's removal is invisible to every comparison the application makes about that file: saving it does not raise a spurious "changed on disk" prompt, and its crash-recovery snapshot does not report a spurious stale baseline
- **And** the file on disk is untouched until the user saves it; an explicit save then writes the buffer, so the mark is not restored — the application holds no per-document encoding state and its own writes are BOM-less UTF-8

### 1.10 A document containing lone carriage returns
- **Given** text whose lines are separated by a bare `\r` with no `\n` — the classic Mac OS convention, which a keyboard/mouse sharing tool's clipboard bridge still converts to when the receiving machine is a Mac, and which macOS itself abandoned in 2001
- **When** it arrives by ANY route — a file being opened, reloaded, restored from a session or recovered from a crash snapshot, or text pasted, dropped or middle-click-pasted into the editor
- **Then** it renders exactly as the same document written with `\n`: headings are headings, blank lines separate blocks, and lists are lists — rather than the whole document collapsing into a single heading in the preview and a single entry in the outline while the editor pane beside it shows the lines laid out correctly
- **And** that split between the two panes is the defining symptom, because GTK treats a bare `\r` as a line separator and the Markdown parser does not
- **And** the repair is invisible to every comparison the application makes about the file: opening one does not mark it modified, saving it does not raise a spurious "changed on disk" prompt, and its crash-recovery snapshot does not report a spurious stale baseline
- **And** the file on disk is untouched until the user saves it; an explicit save then writes the buffer, so the document gains the line endings the rest of the platform uses
- **And** `\r\n` is **not** affected — a Windows-authored document parses correctly as it is, and this application does not rewrite it (the separate question of whether a save should preserve CRLF is deliberately left where it already sits, `tests/MANUAL-TEST.md` §4.2)
- **And** an **undo never puts a lone `\r` back**. On every document reachable in normal use this costs nothing to observe — undo restores exactly the bytes the delete removed, `\r\n` pairs included — because no buffer holds a lone `\r` in the first place. Where the two would differ, the rule wins: a buffer that somehow held one has it repaired on the replay rather than restored verbatim, since byte-exact undo of a sequence no buffer may legally contain is worth less than the rule every derived surface depends on

### 1.11 What a copy puts on the clipboard
- **Given** a selection in the editor, in a document with syntax highlighting active
- **When** the reader copies or cuts it, by any route — the keybinding, the Edit menu, the context menu or the selection bubble
- **Then** what lands on the clipboard is **plain text**, not a rich buffer: pasting it into another application yields the Markdown source, and pasting it back into this one inserts it as a single edit
- **And** a cut removes the selection and is a **single** undo step — one Ctrl+Z restores the document exactly as it was, never half of it
- **And** the same holds for the PRIMARY selection on the platforms that have one: selecting text publishes it, and clearing the selection **releases** PRIMARY rather than claiming it for an empty string, so other applications' middle-click paste is unaffected
- **And** none of this depends on rich formatting surviving an in-application paste — a Markdown document cannot represent a syntax-highlight tag, so carrying one was never meaningful

### 1.11a What a middle-click paste inserts
- **Given** a PRIMARY selection on a platform that has one, published by **any** application including this one
- **When** the reader middle-clicks in the **editor**
- **Then** the text is inserted as **plain text** at the click position, exactly once, and as a **single** undo step — one Ctrl+Z removes the whole paste
- **And** the bytes arrive unchanged: a selection containing `\r\n` inserts `\r\n`, because a rich buffer-to-buffer transfer is what used to split a paste into one edit per syntax-highlight tag and corrupt line endings across the split (ScrAP-312)
- **And** a middle-click in the **preview** does nothing — it is not an editable surface, and it must not paste, scroll or move the caret
- **And** every other text field in the application keeps the platform's own middle-click paste; this changes one view, not the process
- *(Not verified on every platform: PRIMARY middle-click is an X11 convention that Quartz and Win32 do not share, so this rubric is exercised on Linux. The unaffected memory behaviour of selecting is ScrAP-313's, not this rubric's.)*

---

## 2. Rendering fidelity

### 2.1 Basic formatting
- **Given** a document using headings, bold, italic, ordered and unordered lists, links, and blockquotes
- **When** it is rendered
- **Then** each element appears with its corresponding visual formatting

### 2.1a Heading tiers render at five levels; h6 folds onto h5
- **Given** a document containing headings at all six Markdown levels (`#` … `######`)
- **When** it is rendered in the preview and listed in the outline
- **Then** the preview shows **five** distinct heading tiers (h1 largest through h5), each visibly larger than the next, with h5 at body size but bold
- **And** `######` (h6) renders **identically** to `#####` (h5) — no distinct sixth tier exists on any surface (the buffer registers `h1`–`h5` only and the renderer folds h6 onto the `h5` tag), and the outline applies the same fold (an h6 entry takes the h5 row style)
- *(Format ▸ Heading still offers all six for valid Markdown authoring — see 10.4 — but h6 and h5 are indistinguishable once rendered: a deliberate fold, since h4 is already body-adjacent and leaves no room for a distinct sixth tier below it.)*

### 2.20 A single source line break renders as a line break
- **Given** a paragraph whose source wraps across several lines separated by a **single** newline each (e.g. `Line one\nLine two\nLine three`)
- **When** it is rendered
- **Then** each source line appears on its **own** rendered line — a single newline is an explicit line break (the hard-wrap model), not collapsed to a space as stock CommonMark would do
- **And** a blank line (two newlines) still starts a new, separated paragraph
- **And** the hard-wrap model applies *inside a list item's own text* too: an item's source line-wraps (and hard breaks) each render on their **own** rendered line, every line left-justified to the item's content margin (the marker is drawn in a left gutter, so there is no hanging `indent` to preserve — see 2.4a); a blank line starts a new (loose) paragraph, also at the content margin
- *(Trade-off: prose hard-wrapped at a fixed column breaks at each wrap point — accepted for an editor previewing the author's own notes.)*

### 2.2 Tables
- **Given** a GitHub-flavored Markdown table
- **When** it is rendered
- **Then** it appears as a laid-out table with aligned columns and even cell borders, not raw pipe text
- **And** it fits the preview pane width — wide cells wrap onto multiple lines (every cell border spans the full row height) rather than forcing a horizontal scrollbar; narrowing the window re-wraps the cells
- *(Cells are selectable `GtkLabel`s with working links — §2.9 — but are per-cell selection islands.)*

### 2.2·a11y Text views use a screen-reader-safe wrap mode (no AT-SPI abort)
- **Given** GTK 4.6 with a screen reader (Orca/AT-SPI) reading the preview
- **When** AT-SPI requests the text-run attributes of the preview view or a run under the inline-code tag
- **Then** the app does **not** abort: the preview view and the CodeInline tag use a `GtkWrapMode` in `PangoWrapMode`'s range (`Char`, never `WordChar`), because GTK 4.6's AT-SPI path casts the raw `GtkWrapMode` to `PangoWrapMode` untranslated and `WordChar`(3) is out of range → `pango_wrap_mode_to_string`'s `g_assert_not_reached()` (GTK4Rs/AP-136; 4.6-only, fixed 4.8+, restore `WordChar` at floor ≥4.8)
- **And** `Char` also never produces an over-wide line, preserving the no-horizontal-overflow invariant §2.2 / ScrAP-22 depends on (regression test in `preview::render`)

### 2.2a Table header row is visually distinguished
- **Given** a GFM table (the header row is the row above the `---` delimiter row)
- **When** it is rendered
- **Then** the header row's cells are **bold on a faint grayish, theme-aware fill** (the `cell-head` CSS class), distinguishing the header from the body rows, which are plain

### 2.2d A column's delimiter-row alignment reaches the page AND the preview
- **Given** a GFM table whose delimiter row states alignments — `|:---|---:|:---:|---|`
- **When** it is rendered in the preview and exported to PDF and HTML
- **Then** each column's cells are aligned as its delimiter stated — flush left for `:---` and for a bare `---`, flush right for `---:`, centred for `:---:` — **in all three**, and a cell that is nothing but a link is aligned the same way as a text cell
- **And** the preview and the export agree column for column: this is Document Rendering CAM row 17, and it was previously broken in the direction that rule does not look — the exports honoured the delimiter row and the preview hardcoded flush-left, so the same document read differently on screen and on the page
- **And** aligning a link-only cell **moves its text, never its box**: the cell's border still spans the full width of its column, so the table's column rules stay straight down the page (§2.2's "even cell borders"). An alignment that resizes the cell instead shrink-wraps its border to the caption and the rules step in and out row by row — the shape the first fix for this rubric shipped

### 2.2b Tab-separated tables render as tables
- **Given** a table whose cells and/or `---` delimiter row are separated by hard tabs (e.g. pasted from a spreadsheet), which GFM alone would reject as a table
- **When** it is rendered
- **Then** it appears as a real laid-out table, not a literal paragraph of pipes and em-dashes — inline hard tabs are normalised to spaces before parsing (length-preservingly, exempting leading indentation and verbatim code), so offsets/scroll-sync/copy are unaffected (ScrAP-75)
- **And** every surface derived from the document reads that same normalised text, not only the rendered page: the **outline** lists exactly the headings the page shows (a tab-padded table never becomes a phantom heading, and a hard tab inside a heading reads as a space, as on the page), the **export** produces a table, and an **annotation** made over a cell's text in the editor wraps that cell's content and never a span crossing a cell boundary

### 2.9 Links within table cells
- **Given** a table cell containing a Markdown hyperlink — **whether the link is the cell's entire content** (`[#1](https://github.com/…)`) **or sits beside other content** (`☑ [#1](…) filed`), which are the two shapes a reader cannot tell apart
- **When** it is rendered
- **Then** the link appears with the reading theme's link styling — the same colour and underline a link in body text has, and the same as the other cell shape — and hovering it shows a pointer cursor and its URL
- **And** activating it does exactly what the same link does in body text: an external URL opens in the system browser, a same-document `#fragment` scrolls to that heading, and a local document reference opens or is visibly refused — never stripped, never rendered as inert text, and never routed to a different policy because of which cell shape it landed in

### 2.13 Blockquote bar color adapts to the desktop theme
- **Given** a document containing a blockquote, rendered under the **System** reading theme (§18) on a light or dark desktop theme
- **When** it is displayed
- **Then** the left-bar color uses the desktop accent color — not a value hardcoded to a specific light- or dark-scheme (the quoted text itself uses the normal body-text color)

### 2.14 Code and link colors adapt to the desktop theme
- **Given** a document with fenced code blocks, inline code, and hyperlinks, rendered under the **System** reading theme (§18) on a dark desktop theme
- **When** it is displayed
- **Then** the code-block and inline-code backgrounds are subtly distinct from the view background (not a light-scheme grey slab), link text is readable against the background, and syntax-highlighted tokens are from a dark-scheme palette — none of these elements use light-theme hardcoded values

### 2.15 Theme change triggers live re-render
- **Given** a document window is open
- **When** the desktop color scheme toggles between light and dark (e.g. via system settings)
- **Then** the rendered document re-colors within the same window without requiring a manual reload, and without re-reading the file from disk

### 2.11 Blockquote left-bar indicator
- **Given** a document containing a `>` blockquote
- **When** it is rendered
- **Then** a vertical coloured bar appears on the left of the quoted text, aligned with the surrounding normal-text column, with the blockquote body indented to the right of the bar — matching the visual convention used in email clients for quoted sections

### 2.11a Multi-line blockquote body is uniformly indented
- **Given** a multi-line blockquote (several `>` source lines in one quoted paragraph) that wraps at the current viewport width
- **When** it is rendered
- **Then** **every** line of the quote — first, middle, last, and every wrapped continuation — sits at the same left inset past the accent bar, at any window width; no line collapses toward the bar (regression guard for the GtkTextView `one_style_cache` dropped-margin artifact — the tag is applied per line, content-only, ScrAP-76)

### 2.11b A nested blockquote gets its own bar and its own indent
- **Given** a blockquote containing a further `>` level (and a third below that), including one whose inner quote is followed by more outer-level content
- **When** it is rendered, and separately exported to HTML and PDF
- **Then** **each nesting level draws its own accent bar** at its own left offset, and every level's bar is visible **simultaneously** — the outer bar runs the full height of the outer quote, past the inner region rather than stopping where the inner one begins
- **And** each level's bar **starts and ends on that level's own text** — a nested bar begins level with the first line the nested level itself contributes, never reaching up over the parent's preceding line or the blank line separating them
- **And** the quoted text steps in by exactly one level's worth per depth, on **both** sides (a blockquote sets a left *and* a right margin), with every wrapped continuation line at its own level's inset — 2.11a holds per level, not only at depth 1
- **And** the per-level indent is carried by the **depth's own tag**, exactly as `li-{depth}` carries `depth · list_step`: one quote tag per logical line, holding that line's full depth, rather than one tag per level accumulating onto each other. That keeps the quote's margin out-prioritising a code block's inside it, which is why the quote tag is registered where it is and must stay non-accumulative (ScrAP-121, GTK4Rs/AP-96) — and a **list inside a quote** still nests correctly, because `li-{depth}` is accumulative and adds onto whichever quote depth is the line's base
- **And** the **background does not nest**: `blockquote_bg` paints ONE continuous panel over the outermost quote and every level inside it inherits that fill (operator, 2026-08-28). Depth is carried by the bars alone, so 18.29's single-panel contract is unchanged and an inner level never paints a second fill over its parent's
- **And** depth is **clamped at `MAX_QUOTE_DEPTH`** (6, mirroring `MAX_LIST_DEPTH`): past the cap a level renders at the cap's indent and bar rather than stepping further, so a pathologically nested document still opens and stays responsive (1.4b) and can never narrow the content column to nothing nor push the preview over-wide (2.2·a11y)
- **And** a **sprite-tiled** bar (18.28) tiles per level, each keeping the document-anchored phase, so the levels cannot drift against one another while scrolling
- **And** a **single-level** quote is byte-identical to before this rubric existed

### 2.10 Block separation after tables
- **Given** a Markdown table followed immediately by a heading or paragraph
- **When** it is rendered
- **Then** the heading or paragraph begins on its own line, separated from the table — not on the same visual line as the table grid

### 2.3 Syntax-highlighted code
- **Given** a fenced code block annotated with a language (e.g. ```` ```rust ````)
- **When** it is rendered
- **Then** the code appears in a monospace block with language-appropriate syntax highlighting

### 2.3a Code-block card stays within its own lines
- **Given** a fenced code block immediately followed by text with no blank line between them — e.g. a hard-broken loose continuation paragraph wedged under a code block inside a nested list item
- **When** it is rendered
- **Then** the code block's coloured card wraps only its own lines (its uniform inner padding above and below the code) and does **not** paint over the following line's text — the abutting paragraph reads on its own clear line below the card (regression guard for the self-drawn decoration re-adding the tag-supplied line padding, ScrAP-150)

### 2.3b A code block offers a one-gesture copy button
- **Given** a rendered fenced (or indented) code block
- **When** the pointer rests anywhere over the block
- **Then** a small copy button is revealed in the block's top-right corner, inside the card and clear of the code's right edge; it takes an accent border and the pointer cursor when the pointer is on the button itself, and it disappears again when the pointer leaves the block
- **And when** the button is clicked
- **Then** the clipboard holds **exactly that block's code** — every line of it, including any scrolled off screen, with **no** ```` ``` ```` fences, no container `> `/indent markers, and no trailing blank line (deliberately *not* 2.8h's selection→source mapping: a selection is mapped back to Markdown, whereas this answers "give me the code")
- **And** the button shows a checkmark in place of the copy glyph for about a second, then returns to the copy glyph, leaving no selection and no caret movement behind
- **And** this holds at **top level and inside every container** — a list item, a blockquote, a nested list — and for a **one-line** block, whose card is too short for the button's full corner inset (the button centres in what there is rather than vanishing)
- **And** in a block **taller than the pane** the button rides the top of the visible portion, so a long block is copyable without scrolling back to its first line
- **And** the reveal follows the **document**, not only the pointer: with the pointer resting still and the content scrolled underneath it, the block that is now under the pointer shows the button and the block that has moved away loses it — the reader who scrolls a long block and reaches for the button finds it there, without having to move the mouse first
- **And** it survives **zoom** at every level (it is sized from one text row plus the block's own inner padding, both already zoom-scaled) and every installed **reading theme** (drawn in the theme's own page ink on the theme's code-block fill — no literal colour)

### 2.4 Task lists
- **Given** a list using `- [ ]` and `- [x]` items
- **When** it is rendered
- **Then** each item shows a checkbox reflecting its checked state, drawn in the left gutter, and the checkbox is the item's **sole** marker — no bullet or number is drawn before it (the renderer skips the list marker for a task item), at every nesting depth
- **And** clicking anywhere on the drawn checkbox — including its left edge — toggles the source `[ ]`↔`[x]`: the tab goes dirty, one Ctrl+Z reverts the toggle, and the preview re-renders (the checkbox is interactive, not an inert widget), in preview-only and split modes alike
- **And** copy is unaffected: selecting a task item reconstructs its `- [ ]` / `- [x]` source

### 2.4a List content margin and inter-item spacing (drawn gutter)
- **Given** a bulleted, ordered, **or** task list whose items span more than one visual line — a long item that soft-wraps, or an item written across several source lines — at any nesting depth
- **When** it is rendered
- **Then** the marker sits alone in a left **gutter** (drawn, not buffer text) and **every** line of the item's text — the first line, every soft wrap, and every hard-broken source line — left-justifies to the item's uniform content margin, never re-outdenting to the marker column or left of it (every content line carries the same `left_margin` with `indent=0`, so no line can outdent — ScrAP-118)
- **And** a **loose** item — paragraphs separated by a blank line — renders its later paragraphs at that same content margin too, not outdented (fixed by the uniform margin; the former per-line-style-cache outdent is gone)
- **And** a gap separates adjacent items (spacing appears **between** items, not within an item — the item's first line opens the gap above), applied identically to bulleted, ordered, and task lists
- **And** an item inside a **container** — a blockquote, or an enclosing list item — indents relative to *that container's* content margin, **marker included**: a quoted list sits wholly inside the quote (never crossing to or left of the quote's accent bar, and never lopsided — indented on one side only), and each nesting level steps in by exactly one level's worth from the level above it, no more (POLICY Document Rendering CAM row 2; ScrAP-121)
- **And** the **marker itself** (bullet dot, ordered number, or task checkbox) stays **top-aligned on the item's first visual line** — level with the first line of the item's text — no matter how many lines the item wraps to, and never drifts toward the vertical middle of a multi-line item; a single-line item is unaffected (the gutter clamps the item's whole-logical-line height, which spans every soft-wrapped display row, down to its first display row before centering the marker — ScrAP-159)

### 2.4b Empty list items draw no marker
- **Given** a list item with **no content** after its marker — an empty bullet (`- `), an empty number (`1. `), or an empty task (`- [ ]` on its own line)
- **When** it is rendered
- **Then** the gutter draws **nothing** for that item — no bullet dot, no number, and **no checkbox** — because a marker renders only when the item has text/content; a content-less checkbox is not a special case (the renderer records no gutter marker for an item that produced no buffer content)
- **And** empty items interleaved with content items suppress only themselves: the content items keep their markers, kinds, and order (an ordered list's counter still advances across a skipped empty item), and copy of the empty item's source (`- [ ]`) is unaffected

### 2.5 Inline images
- **Given** a document referencing an image by a path relative to the document's folder (e.g. `![](logo.png)` or `![](img/logo.png)`)
- **When** it is rendered with "Show Unsafe Images" off (the default)
- **Then** the image is displayed inline at its location — the path is resolved against the **document's directory**, not the process working directory
- **And** an image whose path resolves *outside* the document's directory (an absolute path elsewhere, a `..` traversal, or a symlink under the folder that points outside) is **blocked**: a broken-image placeholder icon (`image-missing`) is shown in its place — see 2.7 and §14
- **And** when the image renders, its alt text is **not** also shown (the picture stands in for it); when the image **cannot** be shown — blocked by policy, unresolvable path, or a file/URL that fails to decode — a broken-image placeholder icon is shown in its place (never a silent fallback to bare alt text), with the reason in its tooltip (see §14.9)
- **And** a **destination is a URL, not a raw path**: `![](A%20file.svg)` displays the file named `A file.svg`, as it does on GitHub — and the app's own **Insert Image / Insert Link ▸ Browse** writes that encoded form, so a file chosen from the picker is one the app can read back. A raw space (`![](A file.svg)`) is **not** an image in Markdown and is deliberately not made into one: it renders as literal text, which is what the reader's own Markdown says
- **And** a contained local image whose **path contains a colon** — a colon in the filename (`assets/notes:v2.png`), or a Windows drive letter (`C:\pics\x.png`) — still renders: the colon is **not** mistaken for a URL scheme and the reference wrongly refused (a genuine `file://`/`smb://` scheme is still blocked; ScrAP-151)
*(The "raw space stays literal" half is a deliberate decision, not an unimplemented case: making it render means rewriting the buffer before parsing, and that pre-pass is contractually length-preserving — `%20` adds two bytes per space and would drift scroll-sync, copy offsets and annotation spans (ScrAP-75).)*

### 2.21 Wide images fit the preview pane
- **Given** a document with an image wider than the current preview viewport (a narrow window or a split pane)
- **When** it is rendered
- **Then** the image is scaled down to fit the pane width (aspect ratio preserved) and renders fully — it does **not** blank out or force a horizontal scrollbar
- **And** the image re-fits when the pane width changes (window resize, split toggle)
- **And** an image narrower than the pane keeps its natural size, left-aligned — it is **not** stretched to fill the column

### 2.23 Rich image embeds — raw-HTML `<picture>` and `<img>`
- **Given** a document with a raw-HTML `<picture>` element (ordered `<source srcset=…>` candidates followed by an `<img src=…>` fallback), or a bare `<img src=…>` element
- **When** it is rendered
- **Then** a `<picture>` shows as a **single** inline image — the first candidate the app can actually decode (its `<source>`s in order, then the `<img>` fallback) — using the same display machinery as a Markdown image (fits the pane per 2.21, selectable/tinted per 2.18, broken-image placeholder per 2.5); the raw HTML fragments are **not** shown as literal text. This holds whether the `<picture>` is written across multiple lines or on a single line
- **And** the fallback grouping is established **only** by an enclosing `<picture>`: a `<source>` and an `<img>` (or several `<img>`s) that are **not** wrapped in a `<picture>` render as **independent** images — an ungrouped `<source>` never suppresses a sibling `<img>`
- **And** each candidate `src` is subject to the **same** "Show Unsafe Images" / document-containment gate as a Markdown image (2.5, 2.7, §14): a remote, escaping, or other-scheme `src` is blocked, and an `onerror`/script attribute is never executed
- **And** a candidate whose image format the system has no decoder for (e.g. WebP with no WebP loader installed) is skipped in favour of the next candidate in its `<picture>`; when a format's decoder **is** installed, that candidate renders; if nothing in the group can be decoded, the broken-image placeholder is shown (with the `<img>` fallback's `src` in its tooltip)
- **And** raw HTML outside the rendered allowlist — `<picture>`/`<source>`/`<img>` plus `<details>`/`<summary>` (2.26) — (e.g. `<script>`, `<iframe>`, `<div>`) continues to be dropped entirely — neither rendered nor shown as literal text (sanitize-by-omission is unchanged)
- **And** an animated GIF/WebP shows its **static first frame** (frame animation is out of scope)

### 2.25 A Markdown construct the renderer cannot render is visible, never silently dropped
- **Given** a document containing constructs from parser extensions this build does not handle — math (`$E=mc^2$`, `$$…$$`), footnotes (`[^1]` and its definition), a definition list, a wikilink, and YAML or TOML front matter
- **When** it is rendered, and when it is exported
- **Then** each appears as its own **literal source text** — the reader sees what they wrote, unstyled — and nothing vanishes
- **And** the parser is asked for **only** the extensions the renderer has handlers for, so those constructs never become parser events at all
- **And** the three event dispatchers match the parser's `Event`, `Tag` and `TagEnd` vocabularies **exhaustively**: a parser upgrade that adds a construct fails to compile rather than rendering it as nothing
- **Rationale** the failure mode this pins is *silence*: an enabled-but-unhandled extension is dropped, not degraded, so `$E=mc^2$` rendered empty and `[^1]` vanished with every gate green (ScrAP-78)

### 2.26 Disclosure blocks render as a collapsed summary
- **Given** a document containing a raw-HTML `<details>` element with a `<summary>`
- **When** it is rendered
- **Then** the summary shows as a single line carrying a disclosure indicator, its body is **not** shown, and none of the raw HTML appears as literal text
- **And** the collapsed line shows a short **preview of the body's opening text ending in an ellipsis**, in a dimmed secondary colour taken from the active reading theme — so a collapsed block still hints at what it contains rather than hiding it entirely
- **And** the preview never alters the document's copyable source: copying the collapsed block yields its Markdown, with no preview text or ellipsis introduced

### 2.26a Activating a summary toggles its body
- **Given** a rendered collapsed disclosure block
- **When** the user activates its summary
- **Then** the body appears beneath the summary, the disclosure indicator reflects the open state, and activating it again hides the body
- **And** the whole **summary line** is the click target — the label, and the empty space to the right of it — not merely the indicator, which stays small because it reads as an indicator set in prose rather than as a button dropped into a paragraph
- **And** a click on the indicator itself toggles exactly **once**, and drag-selecting the summary's own text does not toggle at all
- **And** the pointer shows the **hand cursor** across that whole line, not the I-beam it would show over ordinary prose — an affordance that works while looking inert is one a reader never tries

### 2.26b A disclosure marked `open` renders expanded
- **Given** a `<details open>` element
- **When** it is rendered
- **Then** its body is visible without any user action, and it can still be collapsed like any other disclosure

### 2.26c Markdown inside a disclosure body renders as Markdown
- **Given** a disclosure whose body contains fenced code, lists, emphasis, inline code, links, blockquotes, tables or images
- **When** the body is shown
- **Then** each construct renders exactly as the same construct renders at top level — the body is ordinary document content, not literal text and not a reduced subset
- **And** a disclosure nested **inside** a container — a blockquote or a list item — renders and toggles there exactly as it does at top level

### 2.26d A malformed disclosure degrades predictably
- **Given** a `<details>` with no `<summary>`, or whose body is not separated by the blank lines CommonMark requires, or which is never closed
- **When** it is rendered
- **Then** a missing `<summary>` shows the label "Details"; a body without blank lines renders as **literal text** rather than parsed Markdown (matching CommonMark and GitHub — this is correct, not a defect); and an unclosed `<details>` does not swallow the remainder of the document
- **And** that literal text is the block's **body**, so it folds with the block: a closed unspaced disclosure hides it and an `open` one shows it, exactly as a spaced body behaves. Emitted outside the block instead, a collapsed disclosure would print the body it had just hidden and its own control would be a visible no-op
- **And** toggling it is **lossless in both directions**: the body returns on expand. Its rendered body is not a range of Markdown events, so the region splice cannot reproduce it and the toggle takes the full-re-render path instead
- **And** an unclosed `<details>` shows its summary text as an ordinary line with **no disclosure control** — there is nothing it could fold, and everything after it renders exactly as it would if the tag were absent
- **And given** an unspaced body that also contains a `<script>` element
- **Then** the body's own text appears as literal text and the script's text appears nowhere — the allowlist still governs which elements may contribute text at all, so showing an allowlisted element's own text is not a licence for its children's
- **And** that holds however the element is spelled and wherever its close tag sits: a self-closing `<script/>` suppresses exactly as `<script>` does (HTML5 does not acknowledge the flag on a non-void element), the same applies to every element whose content HTML parses as text rather than markup (`<style>`, `<textarea>`, `<title>`, `<xmp>`, `<iframe>`, `<noembed>`, `<noframes>`, `<plaintext>`), and no close tag written inside one of them can release it
- **And given** an unspaced body containing an HTML comment, a doctype, a CDATA section or a processing instruction
- **Then** the rest of the body still appears, and the construct itself appears nowhere — none of them is an element, so none can open a suppression that never closes and silently delete the remainder of the block
- **Rationale** browsers close an unclosed `<details>` implicitly at the end of its parent, making the rest of the document its body; that is deliberately not copied, because a document is untrusted content (TDD 2.7) and an authoring slip — or a half-typed tag in a live-preview session — must not be able to hide a document

### 2.26e Sibling and nested disclosures toggle independently
- **Given** a document with two sibling disclosures, and a disclosure nested inside another
- **When** the user toggles one of them
- **Then** its siblings are unaffected, an inner disclosure toggles independently of its outer, and re-expanding an outer disclosure restores the inner one's **own** prior state rather than resetting it
- **And** that holds for siblings that share one raw-HTML block — two adjacent `<details>` with no blank line between them, or a compact one-line `<details><summary>…</summary>…</details>`, which CommonMark makes a single type-6 block. A disclosure's identity is its own `<details>` tag's source offset, not its block's, so siblings in one block are two blocks to the reader in every respect: two folds, two bodies, two hidden-match records

### 2.26f A collapsed body claims no space in the pane
- **Given** a collapsed disclosure whose body contains a table or image wider than the preview pane
- **When** the document is displayed
- **Then** no horizontal scrollbar appears and the preview does not blank — content inside a collapsed block imposes no width on the pane (the ScrAP-23a over-wide chain must not be reachable through collapsed content)

### 2.26g A disclosure exports as it renders
- **Given** a document containing disclosure blocks, both collapsed and `open`
- **When** it is exported to HTML or to PDF
- **Then** each disclosure's summary **and its full body** appear in the artefact, with the body's Markdown rendered as Markdown — an export is the document, not the viewport, so a collapsed block is never omitted
- **And** the artefact reproduces the preview's *omissions* as exactly as its content, for raw HTML written across **more than one line** as well as on one: every walk over a raw-HTML block scans the whole accumulated block, never a line at a time, so a `<script>` whose open and close tags sit on different lines — which is how one is normally written — cannot be dropped from the pane and present in the file
- **And** an HTML export carries a real `<details>` element, so the reader of the artefact gets the affordance too; its `open` attribute follows the **document**, never the reader's fold state. A PDF has no such affordance, so it lays every body out unconditionally
- **Rationale** the preview and the export are two consumers of one event stream and agree only for constructs both were taught; a construct added to the renderer alone is silently absent from every export, and the artefact still opens looking finished (Document Rendering CAM row 17)

### 2.26h Toggling a disclosure holds the reader's place
- **Given** a document being read at some position, with a collapsed disclosure above that position
- **When** the reader expands or collapses that disclosure
- **Then** the reader stays on the same *content* — the view is never thrown back to the top of the document, nor clamped to its end, nor moved by the length of the block that opened
- **And** content below the toggled block moves with it, down as the block opens and up as it closes, which is what the reader asked for
- **And given** the reading position is **inside** a block the reader is collapsing
- **Then** the view settles on that block's summary line — the nearest position that still exists

### 2.26i A toggle does not make the document travel
- **Given** a long document being read some distance down, with a disclosure block above the reading position
- **When** the reader expands or collapses that block
- **Then** the view does not visibly travel — the document never drops to its top and glides back — and the line under the reader's eye is still under it when the transition finishes
- **And** this does not degrade with document length: a longer document must not make the transition more visible
- **Rationale** 2.26h promises the destination; this promises the journey. They are different failures — a re-render that lands correctly after visibly throwing the reader to the top satisfies 2.26h and is still the thing the reader complains about

### 2.26j Everything that points into the document survives a toggle
- **Given** a document containing a disclosure, with headings, links, an image and a table positioned **below** it
- **When** the reader expands or collapses that disclosure
- **Then** copying any selection below the block yields that selection's own Markdown, activating a link below it opens that link's own target, the outline still scrolls to the heading it names, and find still lands on the match it reports
- **And** nothing below the toggled block addresses the wrong text — a reference that silently *acts* on the wrong place is the failure here, not a redraw glitch

### 2.26k An unreachable image is fetched at most once per URL per TTL, not once per toggle
- **Given** a document containing a remote image, with "Show Unsafe Images" enabled, whose host is slow or unreachable
- **When** the reader toggles a disclosure several times
- **Then** the dead URL is requested **at most once per TTL**, not once per toggle — a toggle that lands while a negative entry is live does no network work at all
- **And** each attempt that does run is **bounded** by `imagefetch`'s connect timeout rather than open-ended, so the cost of a failure is a known quantity
- **And** the negative entry expires on a deliberately short TTL, so a transient outage does not read as permanent for the rest of the session; a re-attempt roughly once a minute per URL is the contract, not a defect
- **Limitation, stated rather than promised** the fetch is synchronous on the main thread, so the toggle on which an attempt actually runs **does block the window** for the connect timeout (MEASURED at 5.001s against the 5s timeout). This rubric therefore gates the FREQUENCY of that stall, which is what the cache can deliver; it does not promise responsiveness, which only an asynchronous fetch could. Do not read a stall on the attempting toggle as a failure of this rubric; an attempt on *every* toggle is one.
- **Rationale** a toggle re-walks the document, so every image tag is re-visited, and an uncached fetch would re-run per toggle. The earlier wording promised "the window stays responsive throughout", which the shipped synchronous fetch does not deliver — so the manual check spent its text documenting the rubric being violated. A rubric that states a contract the code cannot meet trains its reader to ignore the check
- **Coverage** the decidable half is tested against the **shipped** thread-local cache, not a stand-in — `imagecache::the_shipped_cache_re_attempts_a_dead_url_only_after_its_ttl` counts fetch attempts across a run of toggles inside and past the TTL, which the injected clock (`get_or_fetch_at`) makes reachable without sleeping through a real minute; `imagecache::policy`'s own tests cover the cache type beneath it. The attempt count across REAL toggles is `tests/MANUAL-TEST.md` §2.26k, read from the log rather than the clock

### 2.26l An edit forgets which blocks the reader had collapsed
- **Given** a document open in split mode with one or more disclosures collapsed by the reader
- **When** the user types anywhere in the editor — above, inside or below a collapsed block
- **Then** every block returns to the state the document states for it (`<details open>` expanded, plain `<details>` collapsed), and no block is left collapsed that the document does not itself mark collapsed
- **And** the reset takes effect on the **keystroke**, not on the debounced re-render that follows it, so a disclosure toggled during that window is decided against the edited source rather than against the pre-edit one
- **And given** a control the reader activates in the window between the keystroke and the re-render — a control the PREVIOUS render built, whose key names text that has moved
- **Then** the activation is **discarded**, and discarded visibly enough to be diagnosed (a `debug` record naming the generation it was minted against). It is not honoured, and it is not guessed at: re-keying it would have to define a disclosure's identity across an edit, which is the guess this rubric exists to refuse
- **And** no block other than the one whose summary the reader activates ever changes its collapsed state as a result of an edit — which is what the discard is *for*, since a stale key can land on a different block's new start offset
- **Rationale** a fold is keyed on the source byte offset of its opening raw-HTML block, and an edit moves every offset after it. Re-keying survivors would have to define a disclosure's identity across an edit that can split, merge or delete one — a guess the reader cannot predict — so the state is dropped instead, matching HTML, where a disclosure's state is the `open` attribute and therefore a property of the document rather than of the reader. Left unreset, a stale key silently reverts a collapsed block mid-typing or, when it collides with a different block's new start offset, **collapses the wrong block** — hiding content the reader never asked to hide

### 2.12 Links within blockquotes
- **Given** a blockquote containing a Markdown hyperlink
- **When** the user activates that link
- **Then** it opens in the system default browser — the link is not rendered as unclickable styled text

### 2.6 External links
- **Given** a rendered document containing a hyperlink to an external URL
- **When** the user activates that link
- **Then** it opens in the system default browser rather than navigating the preview pane away from the document, and the browser's window is **raised to the front** — the launch carries an activation token, so a window manager's focus-stealing prevention permits the raise even when the browser was already running (a tokenless launch opens the tab silently behind the app, which is indistinguishable from the link doing nothing; ScrAP-129)

### 2.24 A click affordance activates only on a complete click
- **Given** a rendered document containing hyperlinks, and the reader using the mouse to select text
- **When** the pointer is pressed somewhere that is *not* a link — or on a *different* link — and released over a link
- **Then** that link does **not** activate: no browser launch, no tab opened, no scroll to a fragment; the selection the drag made is the only outcome
- **And** a swipe that begins *and* ends inside one link's caption (selecting the caption to copy it) likewise does not activate it — travelling further than the desktop's drag threshold makes it a drag, not a click
- **And** an ordinary click — press and release on the same link without dragging — activates it exactly as before (2.6, 2.17, §19)
- **And** the same rule holds for every pointer affordance the panes draw themselves: a gutter task checkbox (2.4), a right-margin comment marker (§17) and a code block's copy button (2.3b) each require their press and release to land on the same one, so a selection drag that happens to end over any of them leaves it alone

### 2.24a A press inside an existing selection belongs to the drag, not to the affordance under it
- **Given** a selection in the preview that covers a pointer affordance — a link, a right-margin comment marker, a gutter task checkbox, or a code block's copy button
- **When** the reader presses inside that selection, on the affordance
- **Then** the affordance does **not** activate: the click clears the selection instead, and the next click on it behaves normally (2.24). One wasted click, self-correcting, nothing at risk
- **And** this is **GTK's behaviour, deliberately left in place, not a defect of this application**: `gtk_text_view_click_gesture_pressed` claims the sequence for its own drag gesture on any single non-touch press whose iter lies inside the selection, in order to start a drag-and-drop, and it does so unconditionally rather than gated on the view being editable. A claim sets `DENIED` on every other gesture handling that sequence, `DENIED` is terminal, and it is **not** a cancellation — so the application's gesture receives `pressed` and then neither `released` nor `cancel`, which is why the click cannot be observed at all rather than merely arriving late (the same arbitration wall as ScrAP-142; measured on an instrumented build against GTK 4.6.9)
- **And** it is **priced and deliberately not worked around**: claiming the sequence ourselves is the only way to out-rank GTK's claim, and it would buy this one self-correcting click at the cost of the case it steals — a press over an affordance would no longer be available to begin a selection drag, and because intent is unknowable at the moment the claim must be made, a press that *did* become a drag would end in nothing happening at all. A silent no-op that fixes itself is the better of the two silences
- **And** the rule does not reach a press **outside** the selection: that press is the application's as usual, and 2.24's complete-click contract governs it

### 2.22 Hovering a link reveals its target
- **Given** a rendered document containing a hyperlink whose caption differs from its URL
- **When** the pointer rests over the link text
- **Then** a tooltip shows the link's URL, so the reader can see where a link leads before committing to a click (a Markdown document is untrusted content — TDD 2.7); hovering ordinary, non-link text shows no tooltip, and existing tooltips on rendered content (e.g. an image placeholder's) are unaffected

### 2.17 Same-document anchor links scroll to their heading
- **Given** a rendered document with a link whose target is a bare fragment (e.g. `[Skill loading](#2-skill-loading)`) and a heading whose GitHub-style slug matches
- **When** the user activates that link
- **Then** the preview scrolls to that heading rather than handing the fragment to an external opener; duplicate headings disambiguate as `slug`, `slug-1`, `slug-2` — including a heading far below the current viewport (a `scroll_to_iter` unvalidated-region hazard; see ScrAP-22)

### 2.7 Untrusted content is contained
- **Given** a document containing embedded HTML or script
- **When** it is rendered
- **Then** the content cannot read local files outside the document's folder or execute privileged actions (sandboxed rendering)
- **And given** an image whose `src` is an absolute path, a `..` traversal, or a symlink (under the doc folder) pointing outside the document's directory
- **When** it is rendered
- **Then** the file is **not** loaded — the canonicalized target must stay at or beneath the document's directory (an untitled document, having no directory, resolves no local images)
- **And given** an image, local or remote, whose header declares dimensions that decode past the project's pixel cap — a decompression bomb, which is small enough on disk to clear every byte limit and expands to hundreds of megabytes or more
- **When** it is rendered
- **Then** its dimensions are read from the header and it is refused **before** the decode, showing the ordinary broken-image placeholder — cost is part of the threat model, and a byte cap bounds the transfer while saying nothing about what it expands to
- **And given** a disclosure the reader has COLLAPSED, whose body contains a raw-HTML `<img>`
- **When** the document is rendered
- **Then** nothing is resolved, nothing is fetched and nothing is anchored for it — a reader's fold state is a privacy signal, so a remote image behind a fold they did not open must not report back, and a local one must not draw inside a region rendered as nothing

### 2.8 Rendered text is selectable and copyable as Markdown
- **Given** a rendered document with prose, headings, code blocks, bold/italic, and links
- **When** the user selects a region and copies (Ctrl+C)
- **Then** the clipboard contains the Markdown source that produced the visible selection — headings include their `#` prefix, bold text retains its `**` delimiters, code blocks include the fenced backticks — not the stripped rendered plain text
- **And** blockquote text participates in the continuous selection like ordinary prose (blockquotes are buffer text, not embedded widgets)
- **And** the copy is **character-precise**, not block-granular: a partial selection copies only the highlighted characters' source, reconstructing delimiters so the result is always valid Markdown (see 2.8a–2.8h)
- *(Only **tables** remain embedded selection islands; an in-cell selection copies that cell's own Markdown source, character-precise like body text — see 2.8f.)*

### 2.8a Within a construct, the enclosing delimiter is excluded; enclosed delimiters are included
- **Given** a rendered document containing a formatting construct — paired inline (bold, italic, code span, link), a **fenced code block**, **or** a leading-marker block (heading, single-line blockquote)
- **When** the user selects a region **entirely inside** one construct's content and copies
- **Then** the enclosing delimiter/marker is **excluded** (four letters of a heading copy no `#`; text inside bold copies no `**`; a link caption fragment copies neither the brackets nor the URL; two lines of a code block copy no ```` ``` ````)
- **And** the delimiters of any construct **fully enclosed** by the selection are **included**

### 2.8b Crossing a construct boundary balances the delimiters
- **Given** a selection that starts inside a construct and extends past its end (or vice-versa)
- **When** the user copies
- **Then** the construct's delimiters are reconstructed whole and artificially completed at the interior endpoint, so there are no dangling delimiters — e.g. selecting from the `k` of `strike` through `outs` of `~~strike out~~ outside` copies `~~ke out~~ outs`
- **And** spanning out of a link caption copies `[fragment](url)` with the **whole** URL

### 2.8c A whole-document selection copies the entire source (= Copy Document)
- **Given** any rendered document
- **When** the user Selects All in the preview and copies
- **Then** the clipboard holds the entire Markdown source, identical to the Copy Document command

### 2.8d Prose is character-precise
- **Given** a multi-paragraph document
- **When** the user selects a region of prose (within or across paragraphs) and copies
- **Then** the clipboard holds exactly that prose's source — no surrounding block is over-copied — with blank-line separators preserved between spanned blocks

### 2.8e Every selection yields well-formed Markdown
- **Given** any selection in the preview (including partial constructs, escapes/entities, images, tables, lists, blockquotes)
- **When** the user copies
- **Then** the clipboard always holds well-formed Markdown: an atomic token (an escape, entity, image, table) is copied whole rather than half; an image or table overlap copies the whole construct source, and a code block that overlaps copies per 2.8h — never an unclosed fence

### 2.8g Multi-line blockquotes and list items are character-precise
- **Given** a multi-line blockquote (`> a` / `> b`) or a list (bulleted, ordered, or task)
- **When** the user selects text *within* the block and copies
- **Then** the copy is the char-precise text with the leading markers **excluded** — no `>` on any quote line (including continuation lines), no `-`/`1.`/`[ ]` on an item — consistent with 2.8a
- **And** Selecting All (or spanning out of the block) reconstructs every marker: each quote line regains its `> ` (blank `>` lines and nested `>>` included), and each item its exact source marker with ordered numbers and task-box state preserved
- **And** a list item that itself contains a nested list or block structure (a loose item) is **also** char-precise: a within-item selection excludes every marker; a selection crossing from an outer item into a nested one reconstructs the nested marker with its indent, and a loose item's blank-line-separated paragraphs keep their separation

### 2.8f Table-cell selection copies the cell's Markdown, character-precise
- **Given** a rendered table whose cell contains formatted content (bold, code span, or a link) — a cell is a selection island, not part of the continuous buffer
- **When** the user selects text inside that cell and copies
- **Then** the clipboard holds the **cell's Markdown source** for the selection — formatting is preserved (`**bold**`, `` `code` ``, `[caption](url)`), not the stripped rendered plain text — and the delimiter rule of 2.8a/2.8b applies within the cell (a partial selection inside the bold run copies no `**`; a whole-cell or crossing selection reconstructs the delimiters)

### 2.8h Code-block selection is character-precise; the fences balance
- **Given** a rendered fenced code block of several lines
- **When** the user selects part of it — a fragment of one line, or whole lines — and copies
- **Then** the clipboard holds **exactly the selected code**, with **no** ```` ``` ```` fences and no other line of the block (consistent with 2.8a)
- **And given** a selection that starts outside the block and ends inside it (or the reverse)
- **When** the user copies
- **Then** **both** fences are reconstructed around the selected code, the closing fence on a line of its own, so the paste is a complete code block (2.8b/2.8e) — never an unclosed ```` ``` ````
- **And** an **indented** (4-space) code block behaves the same, its continuation indent preserved so the copy re-parses as the same block, and a code block inside a blockquote or list item excludes that container's `> `/indent markers within (2.8g)
- **And** annotating (`{==…==}`) a selection inside a code block still wraps the **whole** block — a copy may be divided at a character; an annotation may not


### 2.8i Copying across a collapsed disclosure includes its body
- **Given** a selection that spans a collapsed disclosure block
- **When** the user copies
- **Then** the clipboard contains the Markdown source **including** the collapsed body and its `<details>`/`<summary>` markup — a copy reflects the document, not what happens to be on screen
- **And** this holds for every anchored affordance the construct introduces: a widget anchored in the buffer contributes a `U+FFFC` that the copy map must account for, and an unaccounted one silently omits the construct's source rather than failing loudly
### 2.18 Selecting over an image marks it as selected
- **Given** a rendered document containing an image
- **When** the user drag-selects (or Select-All) a region that spans the image
- **Then** a semi-transparent tint (the text-selection colour) appears over the image to show it is part of the selection, and clearing the selection removes it
- **And** the tint never blocks input — the selection drag passes through the image

### 2.19 Editor pane follows the desktop dark/light theme
- **Given** the editor (GtkSourceView) is shown (edit or split mode) under a dark — or light — desktop theme
- **When** it is displayed
- **Then** its background and syntax colours use a matching dark (or light) style scheme — the editor is not light while the rest of the app is dark (its scheme is set, not left on GtkSourceView's default)
- **And** toggling the desktop colour scheme updates the editor scheme live, alongside the preview re-render, without reopening the document

---

## 3. Live reload (external edits)

> The defining workflow: an AI agent edits the Markdown file on disk while the
> human watches in Scribobulate.

### 3.1 External edit with no local changes
- **Given** a document is open with no unsaved edits in the editor pane
- **When** an external process modifies the file on disk
- **Then** the preview and editor update to reflect the new content without the user taking any action

### 3.2 Reading position is preserved
- **Given** the user has scrolled partway through a long document
- **When** the document reloads from an external change
- **Then** the reading position stays approximately where it was

### 3.3 Rapid successive external edits
- **Given** an external agent writes the file many times in quick succession
- **When** the changes arrive
- **Then** updates are coalesced and the window remains responsive (no flicker storm or backlog)

### 3.4 File removed externally
- **Given** an open document
- **When** the file is deleted on disk
- **Then** the user is informed and the current content is retained in the editor (recoverable by saving)

---

## 4. Editing & saving

### 4.1 Live preview while typing
- **Given** the editor pane has focus
- **When** the user types Markdown
- **Then** the preview pane reflects the changes promptly
- **And** this holds however large the document is and whenever the keystroke lands —
  including while a previous render of the same document is still laying itself out
- **And** a re-render never replaces the preview's `GtkTextBuffer`; it rebuilds the
  content of the buffer the view already holds. Handing a live `GtkTextView` a
  different buffer leaves the layout's cached line displays pointing at the freed one,
  and the next thing to touch that cache — GTK's own paint, or its input-method
  position update inside a scroll — kills the process (ScrAP-258; unfixed in every GTK
  4 through 4.23, so it is avoided rather than waited out)

### 4.2 Source syntax highlighting
- **Given** the editor pane displaying Markdown source
- **When** the document contains headings, emphasis, code, and links
- **Then** the source text is syntax-highlighted as Markdown

### 4.3 Saving to disk
- **Given** unsaved edits in the editor
- **When** the user saves
- **Then** the on-disk file matches the editor content

### 4.9 A successful save is acknowledged with a transient notice
- **Given** a document with unsaved edits
- **When** the user saves it (Save, or Save As to a chosen path)
- **Then** a brief, button-less "File saved." toast appears for ~2.5 s and then auto-dismisses, and "File saved" is announced in the status bar — the same acknowledgement shape a reload gets (5.4), because otherwise a successful save's only feedback is the unsaved indicator *ceasing* to be shown, and an absence is easy to miss. The save and reload notices share one widget, so they can never overlap: whichever happened most recently is the one shown.

### 4.10 A second Save while one is still being written is dropped, not raced
- **Given** a save of a document that has not finished being written (a slow filesystem)
- **When** Save is invoked again for the same document
- **Then** exactly one write happens: the second request is discarded rather than started alongside the first
- **And** nothing is lost by discarding it — the document is still shown as having unsaved changes and Save is still available, so pressing it again once the first write lands writes the newest text

### 4.11 A save always writes the document it was invoked for
- **Given** a save that has been invoked for a document
- **When** the user switches to a different tab before the write completes
- **Then** the written file, the cleared unsaved-changes state, and the retired recovery data all belong to the document Save was invoked for — not to whichever document is on screen when the write finishes
- **And** the same holds for a Save As, and for an "Overwrite" confirmation answered after switching tabs

### 4.4 Unsaved-changes indicator
- **Given** the user has made edits not yet written to disk
- **When** they look at the window
- **Then** an unsaved-changes indicator is visible, and it clears after saving

### 4.5 Saving a new document
- **Given** a document opened from a non-existent path (per 1.3) with content entered
- **When** the user saves
- **Then** the file is created on disk with that content

### 4.6 Saving does not disrupt the editor
- **Given** the editor pane has unsaved edits and the document has a file path
- **When** the user saves
- **Then** the on-disk file updates and the editor keeps its exact content, cursor position, and focus — the view does not flicker, reload, or reset from the save round-tripping through the file watcher
- **And** no false "File deleted on disk" notice appears — the atomic save's own write-temp-then-rename must not be misread as an external deletion by the file monitor (ScrAP-54)

### 4.7 Save As names an untitled document and can relocate a titled one
- **Given** a new/untitled document (no backing file) with content entered
- **When** the user invokes Save (Ctrl+S) or Save As (File ▸ Save As… / Ctrl+Shift+S)
- **Then** a Save As file chooser appears; choosing a location writes the content, the window adopts that file (Copy Full Path / Reload become available), and the document then reads as saved (clean)
- **And** the window is retitled by the *same* formula every other naming path uses (15.7) — so a one-tab window reads "`saved-as.md` — Scribobulate", suffix included, and a window holding several tabs reads "`saved-as.md` (+*N* documents) — Scribobulate", keeping the count of its other documents rather than being retitled to the chosen filename alone
- **And given** a chosen filename that already ends in `.md` (case-insensitive)
- **Then** the file is saved with that single `.md` extension, never a doubled `notes.md.md`
- **And given** the document has just been Save-As'd (or is a first save of a previously-untitled document)
- **When** another process later deletes that file for real
- **Then** the "File deleted on disk" notice still appears — the self-save round-trip guard (TDD 4.6, ScrAP-54) must not stay stuck armed from the Save As's own rename and swallow this later, genuine deletion (QA round-1 M1)
- **And given** a titled document
- **When** the user invokes Save As and picks a different file
- **Then** the content is written there and the window switches to backing the new file — the live-reload watcher now monitors the new file and no longer reacts to changes on the previous one

### 4.8 Save stays available in preview-only mode when there are unsaved changes
- **Given** the window is in preview-only mode and the document has unsaved changes
- **When** the user looks at the Save command (menu bar / toolbar) and invokes it
- **Then** Save is enabled and writes the buffer to disk — Save writes the file, it does not mutate the buffer, so it is exempt from the preview-mode read-only lockout that greys the buffer-mutating actions
- **And** Save is disabled in every mode when the document is clean (nothing to write), and enabled in every mode (edit / split / preview) when it is dirty — its sensitivity tracks the unsaved-changes state, not the view mode
- **And given** the document is clean but its backing file has been **deleted** on disk (a genuine external deletion, not the app's own crash-safe self-rename), **then** the file monitor makes Save **enabled** while the "File deleted on disk — save to restore it" notice is shown, and activating Save re-creates the file with the buffer's content; once the file exists again (a successful save, a reload, or an external re-create) Save sensitivity returns to tracking the dirty flag. Pure predicate: `save_enabled(dirty, backing_missing) == dirty || backing_missing` (unit-tested in `winstate::decisions`).
- **And** Save As remains available in every mode regardless of dirty state (it can write a copy of even a clean document to a new path)

### 4.13 Option+Left/Right moves the caret by a word in every text surface, on macOS
<!-- Why this rubric is macOS-scoped BY MECHANISM rather than by convention: Option+Left/Right in the editor silently ran GtkSourceView's own `move-words` binding — a word-TRANSPOSITION edit, not navigation — instead of moving the caret, because macOS has no other claim on that key and, ON QUARTZ, GtkSourceView's own class binding wins over the app's Back/Forward accelerator (§23.6) declared on the same keystroke — an ordering measured to be REVERSED on Win32 and X11 (ScrAP-311), so this rubric is macOS-scoped by mechanism and not merely by convention. See accel.rs MAC_RESERVED and macwordnav.rs for the full mechanism, sourced to `gtksourceview.c:953`. -->
- **Given** any of this application's text surfaces has focus, on macOS — the document editor, the find field, the replace field, the annotation comment entry, or the shared prompt field behind Go To Line, Rename and Insert Link/Image/Table
- **When** the reader presses Option+Left or Option+Right
- **Then** the caret moves one word back or forward, exactly as Ctrl+Left/Ctrl+Right already do on every platform (Linux/Windows convention) — Option is the macOS spelling of the same movement, not a second, different one
- **And** the buffer's content and word order are completely unchanged — this is caret movement only, never the word-transposition edit `GtkSourceView` itself would otherwise perform on this key (that edit is what wins the key on Quartz with no interceptor; on Win32 and X11 the accelerator wins instead and this rubric has nothing to guard — ScrAP-311)
- **And** Option+Shift+Left/Right extends the selection by a word instead of moving the caret, matching Ctrl+Shift+Left/Right
- **And** this does **not** invoke Back/Forward (23.6) — on macOS that command no longer shares this key (Cmd+[ / Cmd+] instead), so the two can never race
- **And** Ctrl+Left/Ctrl+Right keep working unchanged (GTK's own binding) — this adds a second spelling, it does not replace the first
- **And** the surfaces that are not the document editor were never *destructive* on this key, they were **inert**: nothing outside the editor is a `GtkSourceView`, so no `move-words` binding exists there to run, and `GtkText` binds word movement to Ctrl+Left/Right only. Doing nothing at all is the milder face of the same gap between GTK's Ctrl-based bindings and the Option-based convention every native macOS text field honours, and the rule is one rule across all of them
- **And** everything else those fields already answer for is untouched — Escape still closes the find bar, Enter still triggers a prompt form's default button, and a field's own Ctrl+Left/Right still moves by word

### 4.12 Save All writes every tab that needs saving
- **Given** a window with several tabs, of which more than one is dirty (or clean over a deleted backing file)
- **When** the user invokes Save All (File ▸ Save All or its accelerator — no toolbar control; an "Uncommon command", CAM.md)
- **Then** every such tab is written: titled tabs use the same content-gated Save as a single Save (including the overwrite prompt when the file changed on disk), and each untitled dirty tab gets a Save As chooser in turn
- **And** Save All is enabled whenever **any** tab in the window needs writing — not only when the *active* tab is dirty — and disabled when every tab is clean with its backing file present
- **And** a write started for one tab always lands on that tab even if the user switches tabs mid-batch (4.11), and a cancelled Save As / overwrite for one tab does not prevent the rest of the batch from continuing
- **And** when the batch finishes, the tab the user had focused when they invoked Save All is active again

---

## 5. Reconciliation (conflict handling)

### 5.1 External edit with unsaved local changes
- **Given** the editor has unsaved local edits
- **When** the file is changed on disk by another process
- **Then** the user is notified of the conflict and offered to reload (discarding local edits) or keep local edits — the local edits are never silently overwritten

### 5.2 Saving over an externally changed file
- **Given** the file on disk has changed since it was loaded, and the user has unsaved edits
- **When** the user saves
- **Then** the user is warned before their save overwrites the newer on-disk version

### 5.3 Choosing to reload discards cleanly
- **Given** a conflict notification (per 5.1)
- **When** the user chooses to reload
- **Then** the editor and preview both reflect the on-disk content and the unsaved indicator clears

### 5.4 Clean external edit auto-reloads with a transient notice
- **Given** the editor has **no** unsaved local edits and auto-reload is enabled
- **When** the file is changed on disk by another process
- **Then** the content is reloaded silently (no conflict prompt) and a brief, button-less "File reloaded from disk." toast appears for ~2.5 s then auto-dismisses; a further reload while it is still visible resets the timer rather than stacking a second toast

---

### 5.5 A document that stops being admissible is refused on re-read, not read anyway
- **Given** an open document whose file has since grown past the load limit, or been replaced by something that is not a regular file
- **When** the application re-reads it — an explicit Reload, a save's check for external changes, or the live-reload watcher noticing it changed
- **Then** the read is refused rather than attempted: Reload reports why, a save asks before overwriting rather than assuming it is safe, and the watcher does nothing
- **And** the application never waits on it — the same admission test that guards the first read guards every later one

### 5.6 Overlapping re-reads of one document never apply an older answer
- **Given** a document being rewritten repeatedly on disk, so several reads of it are outstanding at once
- **When** those reads complete in an order other than the one they were started in
- **Then** only the newest read's content is applied; an older one is discarded rather than replacing the buffer with stale text and recording it as the saved baseline

## 6. Resource footprint (viability gate)

> Scribobulate exists because another popular Markdown viewer holds ~594 MiB of
> VRAM (a GPU-canvas self-rendered pipeline). These rubrics encode the reason the
> project exists.
> **They are a go/no-go gate: footprint must be proven on a minimal proof-of-concept
> before significant feature work begins.** If the chosen stack cannot meet 6.1 and
> 6.3, the project pivots to a different stack or is scrapped.

### 6.1 GPU memory under hard ceiling
- **Given** a typical Markdown document open in the application
- **When** GPU memory use is measured (e.g. `nvidia-smi` on Linux/NVIDIA)
- **Then** the process holds **fewer than 50 MiB** of VRAM — and far below that viewer's ~594 MiB

### 6.2 No GPU compositing pipeline held
- **Given** the application is running with a document open
- **When** its rendering configuration is inspected
- **Then** preview rendering uses software compositing (no full GL/GLES pipeline held for the lifetime of the window)

### 6.3 System RAM stays bounded
- **Given** a typical Markdown document open, including the WebKit render process
- **When** total resident memory (RSS across all the app's processes) is measured
- **Then** it stays within an acceptable ceiling confirmed by the viability spike (target: well under a few hundred MiB; exact number set once measured), and does not climb without bound across repeated live-reload cycles

### 6.4 On macOS, GPU memory is fixed overhead — not a rendering cost
> macOS composites every visible window through the GPU, so a literal zero
> reading (6.1's Linux formulation) is unattainable there and is **not** the
> gate. What must hold is that whatever the process does hold is *fixed
> overhead* rather than a rendering pipeline — the distinguishing evidence is
> that it does not grow with what the app is asked to draw.

- **Given** a typical Markdown document open on macOS
- **When** the process's GPU memory and GPU engine utilization are measured (e.g. Activity Monitor / `ioreg`), first while the window is resized between small and maximized, and then with documents of very different sizes at a fixed window size
- **Then** the reading stays far below 6.1's 50 MiB ceiling, **does not grow with window area**, **does not grow with document size or complexity**, and GPU engine utilization for the process stays at or near zero throughout — while system RAM (6.3) *does* rise with document size, confirming the document is rendered on the CPU
- **And** a reading that scales with either dimension, or sustained GPU engine activity, means software compositing is not active and **fails** this gate — that, not a non-zero byte count, is the macOS signal that 6.2 has been violated

### 6.5 On Windows, GPU memory is fixed overhead — not a rendering cost
> Windows composites every visible window through the GPU, so a literal zero reading
> is unattainable there and is **not** the gate. What must hold is that whatever the
> process does hold is *fixed overhead* rather than a rendering pipeline: the
> distinguishing evidence is that it does not grow with what the app is asked to draw.
> The Linux formulation of 6.1 — that the app appears as **no GPU client at all** —
> is a property of that platform's accounting and must not be read as a byte
> measurement on Windows, or a healthy build fails a gate it actually passes.

- **Given** a typical Markdown document open on Windows
- **When** the process's dedicated GPU memory and GPU engine utilisation are measured, first while the window is resized between small and maximised, and then with documents of very different sizes at a fixed window size
- **Then** the reading stays far below 6.1's 50 MiB ceiling, **does not grow with window area**, **does not grow with document size or complexity**, and GPU engine utilisation for the process stays at or near zero throughout — while system RAM (6.3) *does* rise with document size, confirming the document is rendered on the CPU
- **And** a reading that scales with either dimension, or sustained GPU engine activity, means software compositing is not active and **fails** this gate — that, not a non-zero byte count, is the Windows signal that 6.2 has been violated

---

## 7. Window & layout

### 7.0b The window carries the application's own icon on every platform
- **Given** Scribobulate is running, installed or not
- **When** the user looks at the title bar, the taskbar button, or Alt+Tab
- **Then** each shows Scribobulate's own application icon, never the toolkit's
  generic default
- **And** this holds without any platform install step having populated a
  filesystem icon theme — the icon resolves from the binary's own bundle

### 7.0c The app follows the operating system's light/dark setting
- **Given** the reading theme is left on **System**
- **When** the user has the OS set to dark, or switches it while the app is running
- **Then** the app's chrome and editor render dark without a restart
- **And** the window's title bar renders dark with them — never a light caption
  above a dark app
- **And** switching back to light reverses both

### 7.0d A maximized window stays maximized while a menu or dropdown is open
- **Given** the window has been maximized using the operating system's own
  maximize button (not a command inside the app)
- **When** the user opens any menu or dropdown — a menu-bar menu, the toolbar's
  reading-theme picker, a right-click context menu
- **Then** the window still fills the screen, both while that menu is open and
  after it closes, with no band of stale desktop left along any edge
- **And** restoring the window afterwards returns it to exactly the size and
  position it had before it was maximized

### 7.0e The operating system's own menus name and picture the app correctly
- **Given** Scribobulate is installed
- **When** the user meets the app somewhere the *system* presents it rather than
  the app itself — a file's "Open with" menu, the default-apps settings, the
  task manager, the executable's own properties
- **Then** it is called **Scribobulate**, never a file name and never a file
  name with an extension on it
- **And** it is shown with Scribobulate's own icon, never a generic placeholder
- **And** the same holds for a build that has never been installed

### 7.1 Layout modes
- **Given** the application is running
- **When** the user switches layout
- **Then** they can view editor-only, preview-only, or a side-by-side split

### 7.2 Layout persists across sessions
- **Given** the user has chosen a layout and window size
- **When** they reopen the application
- **Then** the previous layout and window size are restored

### 7.3 Split-pane arrangement
- **Given** the window is in side-by-side split mode
- **When** the user activates View ▸ Swap Panes (or the toolbar toggle)
- **Then** the editor and preview panes exchange positions (editor moves to the right / bottom pane, preview to the left / top) while scroll sync and zoom continue to work correctly
- **And when** the user activates View ▸ Vertical Split (or the toolbar toggle)
- **Then** the split reorients from left/right (horizontal) to top/bottom (vertical), with both panes retaining their content and scroll sync
- **And** both controls are disabled (greyed) when the window is not in split mode
- **And** the pane arrangement (swap flag + vertical flag) survives a session restart — opening the same document restores the same layout

### 7.4 Clean shutdown with unsaved changes
- **Given** unsaved edits exist — in a previously-loaded file **or** a new/untitled document the user has typed into
- **When** the user closes the window, or quits the application (Ctrl+Q / the Exit menu)
- **Then** they are prompted to save, discard, or cancel rather than losing work silently
- **And given** the unsaved document is untitled (never saved to a file)
- **When** the user chooses Save in that prompt
- **Then** a Save As file chooser appears; choosing a location writes the content and the window closes — the work is not lost
- **And** the prompt window carries the application's name as its caption, like every other modal confirmation the application raises (the overwrite warning, the save-error report) — a dialog left untitled is rendered as a lone "." by the Windows native frame, in its title bar and in the taskbar
- **And given** a pristine new/welcome window the user has **not** edited
- **When** the user closes it
- **Then** it closes immediately with no prompt (no false friction)
- **And given** a window has several tabs, more than one of them dirty
- **When** the user closes that window (or quits the application)
- **Then** they are prompted once per dirty tab, sequentially — switching to that tab so the prompt is visibly about it — and choosing Cancel at any point aborts the whole close, leaving every tab exactly as it was

### 7.5 Reading position survives a mode switch; split panes are line-accurately synced
- **Given** the user has scrolled partway through a document in one view mode
- **When** they switch to another mode (preview, edit, or side-by-side)
- **Then** the new view stays at approximately the same relative position rather than jumping back toward the top
- **And given** the user repeats the mode round trip several times
- **Then** the reading position does not accumulate — after several round trips it is still within tolerance of where it started, not several steps away from it, and in particular has not been walked to the end of the document
- **And given** side-by-side split with a document of uneven block heights (headings/code/tables mixed with prose)
- **When** the user scrolls either pane
- **Then** the other follows so the **same document position** stays aligned across both panes (the heading/line at the top of one is at the top of the other) — **line-accurate**, not merely the same 0–1 fraction, which drifts when a source line renders taller than the next; typing (which re-renders the preview) neither breaks the alignment nor blanks the preview
- **And** in edit/split the outline highlights the heading at the **caret** (the working position), tracked live as the caret moves

### 7.6 A tab dragged into another window moves there
- **Given** two Scribobulate windows are open
- **When** the user drags a tab from one window's tab strip and drops it onto the other window's tab strip
- **Then** the tab (its content, undo history, and current view mode) moves into the destination window, and disappears from the source
- **And given** the same setup
- **When** the user drags a tab and drops it back onto its OWN window's strip
- **Then** nothing crosses window boundaries — that drag reorders in place (15.6) instead

> Formerly required holding Shift to cross a window boundary (a plain drag
> reordered in-window only); widgets/tab's custom tab strip retired
> that gate — it existed only to keep a hand-rolled `GtkDragSource` from
> racing `GtkNotebook`'s own private reorder gesture (ScrAP-50), and
> a fully-owned strip has no second gesture to race. A plain drag now
> reorders, escalates to cross-window, or drops to the desktop based purely
> on where it ends up — matching ordinary browser tab-strip behavior.

### 7.7 A window emptied by a tab move closes itself
- **Given** a window has exactly one tab open
- **When** that tab is dragged into another window or dragged onto the desktop
- **Then** the now-empty source window closes automatically rather than being left open with nothing in it
- **And given** a window has exactly one tab open
- **Then** View ▸ Move Tab to New Window (menu item, toolbar button, and accelerator) is disabled — moving a window's only tab there would just leave an identical, empty-of-purpose window behind, so this specific path never arises through the command (TDD 15.4)

### 7.8 A single-tab window can still receive a dragged-in tab
- **Given** a window has only one document open
- **When** the user drags a tab from another window toward it (or, with only one Scribobulate window open, looks at its tab strip)
- **Then** the tab strip is always visible and accepts the drop, landing the dragged tab there — the strip no longer hides at exactly one tab, even when this is the only open Scribobulate window (see TDD 15.7: a hidden strip has zero allocated height and cannot be a drag source OR a drop target, which used to make a lone window's own sole tab permanently undraggable)

### 7.9 Dragging a tab shows where it can be dropped
- **Given** the user is dragging a tab
- **When** the drag passes over another window's tab strip (or continues hovering its own, mid-reorder)
- **Then** that strip visibly highlights (glows) for as long as the drag hovers there, and stops as soon as it leaves

### 7.9a A dragged tab is represented by an image of itself under the pointer
- **Given** the user has begun dragging a tab
- **When** the pointer moves, whether over a valid drop target or not
- **Then** an image of the dragged tab — its label and close button, at full strength — follows the pointer for the duration of the drag
- **And** the presence of a pointer-following *surface* does NOT satisfy this: a drag surface is created, correctly sized, and tracks the pointer even when nothing has been painted into it, in which case it shows whatever was already on screen behind it. Verify the **content** is the tab (correctly framed, its own label legible), not merely that something moves with the cursor — the two are indistinguishable at a glance and that is precisely how this went unnoticed

### 7.10 Dragging a tab onto the desktop opens a new window (X11)
- **Given** a Linux desktop running X11
- **When** the user drags a tab off its window's strip and releases it over empty desktop space
- **Then** a new window opens containing that tab

### 7.11 A tab's own `×` button closes it without switching to it first
- **Given** a window has two or more tabs open, and the tab in question is NOT the active one
- **When** the user clicks that tab's own `×` close button (revealed on hover, or always shown on the active tab)
- **Then** that specific tab closes — prompting to save first if it's dirty — without ever making it the active tab, and whichever tab was already active stays active throughout (unless the tab being closed WAS the active one, in which case a neighboring tab becomes active exactly as Ctrl+W already does)
- **And given** the window has exactly one tab
- **Then** clicking its `×` closes the whole window (same single-tab rule as Ctrl+W)

### 7.12 A tab's right-click context menu offers per-tab commands
- **Given** any tab in the strip
- **When** the user right-clicks it
- **Then** a context menu appears with Save, Save As, Close Tab, Close Other Tabs, Move to New Window, Copy Full Path, Reload, and Rename, each acting on THAT tab specifically (not necessarily the active one) — Close Other Tabs is disabled when it is the window's only tab, and Move to New Window is disabled under the same single-tab condition View ▸ Move Tab to New Window already uses (TDD 15.4)

### 7.13 The tab context menu is keyboard-navigable
- **Given** a tab's right-click context menu is open, each row showing one letter underlined
- **When** the user presses that bare letter (no modifier)
- **Then** the corresponding command runs on that tab — `s` = Save, `a` = Save As, `c` = Close Tab, `o` = Close Other Tabs, `m` = Move to New Window, `f` = Copy Full Path, `r` = Reload, `n` = Rename — with each letter matching the access letter of the same command's File-menu (or View-menu, for Move to New Window) surface where one exists
- **And** a disabled row (Close Other Tabs on the window's only tab; Save on a clean tab with its backing file present) ignores its access key

### 7.14 Close Other Tabs prompts for dirty tabs sequentially
- **Given** a window with several tabs, more than one of the OTHER tabs (not the right-clicked one) dirty
- **When** the user invokes Close Other Tabs on a tab
- **Then** the clean other tabs close immediately, and the dirty ones are prompted to save/discard **one at a time** — switching to each so its prompt is visibly about it — never several stacked dialogs at once (parity with §7.4's window-close sweep)
- **And** choosing Cancel (or backing out of a Save As) aborts the batch: the remaining un-prompted dirty tabs stay open, already-closed tabs stay closed, and focus returns to the kept tab

### 7.15 Reading position survives a horizontal window resize
- **Given** the user has scrolled partway through a long document in preview or split mode
- **When** the window is resized horizontally — narrower or wider, including a **repeated incremental drag** of the frame edge (not just a single step)
- **Then** the preview keeps the same reading line at the top of the viewport: the re-wrapped text does not drift upward toward the start of the document, not even cumulatively across the many resize steps of a drag (parity with §3.2 reload and §13.7 zoom — the reading position is preserved across every re-layout, rebuild *or* re-wrap)
- **And** this holds when an external reload arrives *during* the resize — the reload re-anchors to the same tracked reading line rather than to a transiently clamped near-top position

### 7.16 Dragging a tab onto the desktop opens a new window (macOS)
- **Given** a macOS desktop
- **When** the user drags a tab off its window's strip and releases it over empty desktop space
- **Then** a new window opens containing that tab (same outcome as 7.10 on X11)

### 7.17 An installed Scribobulate carries its own icon everywhere
- **Given** Scribobulate has been installed for the platform — `packaging/linux/install.sh` on Linux, the `.app` bundle on macOS, or an equivalent OS packaging step — and is running
- **When** the user looks at the title bar, the taskbar/dock, or the window switcher
- **Then** each shows Scribobulate's own application icon, never the toolkit's generic default
- **And** an *uninstalled* run is not held to this: the surfaces the OS owns (taskbar/dock, switcher) may show a generic icon, because on some platforms an unpackaged binary has no application identity to attach one to
- **And** Help ▸ About shows the app icon as its logo even on an *uninstalled* run — that surface is served by the bundled GResource rather than by OS packaging, so it must hold either way

### 7.18 The app never becomes unresponsive after a popover-owning tab is closed, moved, or reloaded
- **Given** a contextual popover (e.g. the code-block marker popover) is open
- **When** the tab it belongs to is closed, moved to another window, or its document is reloaded from an external change
- **Then** the app remains fully responsive to clicks and keyboard input afterward — it never becomes silently unresponsive as though it still held an input grab

### 7.19 A dialog raised over a full-screen window opens inside it
- **Given** a document window has been put into the operating system's own full-screen mode using the OS's control (the green button on macOS), so it occupies the whole display
- **When** the user opens any dialog the app raises over that window — Help ▸ About, or the Save/Discard/Cancel prompt for unsaved changes
- **Then** the dialog appears at its own modest size, floating over the document window, in the same full-screen workspace — the display never switches away from the document to show the dialog somewhere else
- **And** the dialog is dismissable exactly as it is in a normal window: Escape closes About and cancels the confirmation, and all three confirmation buttons act
- **And** when it closes, the document window is showing its content again — fully painted, never a black or blank rectangle where the document was
- **And** the document window is still full-screen afterwards, and the green button still takes an ordinary window into full-screen as before

### 7.20 A tab opened into a full tab strip lands after the others and is visible
- **Given** a window whose tab strip already overflows — more open documents than fit, so the strip shows its scroll chevrons — and whose tabs have been through the states a real session puts them in (a background tab shows, then loses, its loading spinner; tabs are re-titled as documents open and save)
- **When** the user opens another document into that window, or creates a new one
- **Then** its tab is drawn **after** the last existing tab, evenly spaced with the rest — never on top of its left-hand neighbour, and never leaving two labels superimposed
- **And** the strip scrolls far enough to show the new tab **in full**: it is the active tab, so it must be visible, not clipped past the right-hand edge
- **And** the tabs that were already there stay evenly spaced, whatever changed their widths earlier in the session

### 7.21 Every install route delivers the same payload, attribution included
- **Given** any of the three Linux install routes — the `.deb`, the `.rpm`, or the from-source install into `~/.local`
- **When** the install completes
- **Then** every file the payload defines is present: the binary, the desktop entry, the application icon, the reading-themes file, the sprites those themes name, **both** manual pages (`scribobulate.1` and `scribobulate.5`), and `THIRD-PARTY-LICENSES.md` — stated as a list rather than a count, because a count is the one fact about a payload guaranteed to be wrong by the next addition, and this one already was
- **And** `THIRD-PARTY-LICENSES.md` is not optional documentation — the syntax-highlighting grammars are statically linked into the binary under licences that require their notices to accompany a binary distribution, so an install lacking it is a licence violation rather than a cosmetic gap
- **And** the rpm marks that file as a licence, so `--excludedocs` cannot drop it
- **And** the routes cannot diverge on *what* an install consists of: the payload is defined once and every route reads that definition
- **And** the from-source route pins the desktop entry's `Exec`/`TryExec` to the absolute binary path, because its bin directory is frequently absent from the launcher's `PATH`
- **And** installing does not widen permissions on directories it did not create — a user-private directory under the install prefix keeps the mode the user gave it
- **And given** the macOS route — `packaging/macos/install.sh`, backed by the `.app` that `bundle.sh` produces
- **Then** both manual pages are present inside the bundle at `Contents/Resources/man/man{1,5}/scribobulate.{1,5}.gz`, and are reachable **by name** from a man directory the platform actually searches
- **And** that directory is established by **measurement** (`manpath`) rather than by carrying the Linux XDG location across: macOS searches no per-user man directory at all, so `~/.local/share/man` would be a directory nothing reads and an install into it is indistinguishable from a successful one until someone runs `man`
- **And** the pages are staged by the same shared helper every other route uses, so the substitutions, the date policy and the compression cannot drift per platform
- **And** the bundle holds the only copy — what the install places is a link into it, as it already is for the executable — so the `.dmg` route carries the pages too, present and readable by path even though a bundle is not on MANPATH
- **And** uninstalling removes them **including when the bundle was deleted first**: the links dangle at that point, and a guard that tests for existence rather than for a link reports success while leaving them behind

---

### 7.22 A freshly opened document puts the working position at its beginning
- **Given** a document is opened, restored from the previous session, or reloaded from disk
- **When** it is first shown in the editor — by the view-mode action, the toolbar button, or session restore
- **Then** the caret sits at the beginning of the document, and the footer's line/column indicator reads the first line
- **And** the outline sidebar highlights the document's first section, not its last

### 7.23 An install leaves exactly one Scribobulate, and says so when it cannot
- **Given** a machine that may already carry a Scribobulate — a copy installed from the `.dmg`, a distro package, or residue from another platform's install route
- **When** the developer install runs
- **Then** it never silently produces a second copy that competes with the first
- **And** on macOS it refuses before building, naming what it found and how to remove it, because two bundles sharing one identifier let the Dock and the terminal launch different copies with nothing to signal the divergence
- **And** on Linux it warns and proceeds, because a distro package alongside a user-local build is an ordinary supported arrangement
- **And** it reports what it found and never deletes a copy another route installed
- **And** what it puts on PATH is the artefact it just built, never one it merely found
- **And** nothing it installs resolves into the build directory, so emptying that directory cannot silently break the install
- **And** a dangling link is reported as a hazard rather than treated as absent

## 8. Single-instance lifecycle

> One process, many windows. Launching the app repeatedly must not spawn
> duplicate processes — each process pays the full WebKit footprint.

### 8.1 Second launch reuses the running process
- **Given** the application is already running with a document open
- **When** the application is launched again with a path to a different Markdown file
- **Then** that file opens in a new window belonging to the existing process, and no second application process is created

### 8.2 Re-opening an already-open file focuses it
- **Given** a document is already open in some tab of some window (active or in the background)
- **When** the application is launched again with that same file's path from a source that carries a desktop activation token (a file manager, "Open With", `gtk-launch`, `gio open`, or equivalent)
- **Then** the window containing that tab is focused and that tab becomes the active one, rather than a duplicate window opening
- **And given** the second launch instead comes from a bare shell command with no activation token (e.g. `scribobulate path` typed directly into a terminal)
- **Then** the window-manager's focus-stealing prevention may legitimately substitute a taskbar/demands-attention flash for an actual raise-and-focus — this is desktop-level behavior common to virtually every application, not an app bug, and is not something `gtk_window_present()` can or should override (researcher-verified, ScrAP-47); verify this rubric via a tokened launch method, not a bare terminal command

### 8.3 Independent window lifecycle
- **Given** multiple document windows are open in the one process
- **When** the user closes one window
- **Then** the remaining windows stay open and the process keeps running; closing the last window exits the process

### 8.4 Footprint does not scale per process
- **Given** several documents opened as several windows in the single process
- **When** GPU and system memory are measured
- **Then** the engine is shared (footprint grows modestly per window, not by a full new ~WebKit-process baseline per document)

### 8.5 `--new-instance` forces a separate process
- **Given** the application is already running
- **When** it is launched again with `--new-instance` (or `-n`)
- **Then** a brand-new, independent process starts (it does not activate or hand its arguments to the running instance), so a development build can run alongside the everyday one; any file path given still opens in the new process

### 8.7 A force-killed instance never wedges the next launch
- **Given** the application was terminated without running its shutdown path — force-quit, `kill -9`, or a crash — so any rendezvous state it left behind (a lock, a socket, a registration) was never cleaned up
- **When** the application is launched again
- **Then** it starts normally and becomes the new primary, taking over and discarding the abandoned state rather than refusing to launch, hanging, or silently starting a second process that later rubrics in this section would then fail on
- **And given** the running instance holds the primary role but is not yet, or no longer, able to answer a handoff (mid-startup, or shutting down)
- **Then** a launch that cannot complete the handoff still opens the user's document — an independent process is the correct fallback, because failing to open a file is a worse outcome than a duplicate one
- *(This rubric exists because a hand-rolled rendezvous can strand state in a way GIO's D-Bus registration cannot: a bus name vanishes with its process, whereas a lock file and socket outlive theirs. It is numbered 8.7 rather than 8.6 because `tests/MANUAL-TEST.md` already uses 8.6 for the per-window memory-reclamation gate.)*

---

## 9. Menu bar, toolbar, and actions

### 9.1 File ▸ New Document opens a tab in the current window
- **Given** the application is running with a document open
- **When** the user selects File ▸ New Document or presses Ctrl+T
- **Then** a new tab opens in the current window — pre-populated with the **welcome/starter template content** (a starting point, not literally empty; the same content a no-argument launch or View ▸ New Window shows), and counted as an untouched "blank" tab (reusable in place per 1.5, and a "blank welcome" tab for 9.10's purposes) until the user edits it — without closing the existing window or any of its other tabs (opening a brand-new *window* instead is View ▸ New Window — §15)

### 9.2 File > Open lets the user pick a Markdown file
- **Given** the application is running
- **When** the user selects File > Open or presses Ctrl+O and confirms a file in the dialog
- **Then** that file's rendered content appears in the current window

### 9.3 Edit > Copy tracks selection state across all surfaces
- **Given** a rendered document with no text selected
- **When** the user opens the Edit menu or right-clicks for the context menu
- **Then** the Copy item is grayed out in both menus simultaneously
- **And given** a text selection is active
- **When** the user opens either menu
- **Then** Copy is enabled in both — the main menu and the context menu cannot show different enabled states from each other

### 2.16 GtkSettings fallback when no XDG portal is present
- **Given** no `org.freedesktop.portal.Settings` backend is running (e.g. bare X11 session without a portal)
- **When** the application starts
- **Then** it falls back to `GtkSettings` for theme detection, renders with readable contrast, and does not crash or emit unhandled errors

### 9.5 Toolbar icon buttons mirror action enabled state
- **Given** a window with no text selected
- **When** the user looks at the toolbar
- **Then** the Copy button is visually disabled (greyed out)
- **And given** a text selection becomes active
- **When** the toolbar is visible
- **Then** the Copy button becomes enabled — matching the Edit menu Copy item exactly

### 9.4 Code block inner padding is uniform
- **Given** a fenced code block containing text and at least one blank line
- **When** it is rendered
- **Then** it has visually equal padding on all four sides (top, bottom, left, right) and the background color is unbroken across the blank line — not split into separate segments
- **And** the block's background is visibly distinct from the document/page background (it reads as a panel, not flush with the page) under both light and dark desktop themes

### 9.6 Save availability follows the view mode
- **Given** the window is in preview mode
- **When** the user looks at the Save control (toolbar button and File menu item)
- **Then** Save is disabled (greyed out), because there is nothing to save in a read-only view
- **And given** the window is in edit or split mode
- **When** the user looks at Save
- **Then** Save is enabled

### 9.7 View-mode selection is consistent across surfaces
- **Given** the window is in any view mode
- **When** the user looks at the three view toggle buttons in the toolbar and the View menu
- **Then** exactly one mode (preview, edit, or split) is shown active in both places, and they always agree
- **And when** the user changes the mode from either surface
- **Then** the other surface updates to match — the toolbar and menu cannot show different active modes

### 9.8 Copy Full Path copies the document's absolute path
- **Given** a document opened from a file
- **When** the user invokes File > Copy Full Path (or the toolbar button)
- **Then** the clipboard contains the document's absolute path including the filename

### 9.9 Copy Full Path is available when a file is open
- **Given** a document opened from a file
- **When** the user looks at the Copy Full Path control
- **Then** it is enabled in both the File menu and the toolbar

### 9.10 Copy Full Path is unavailable on a blank window
- **Given** a blank welcome window (File > New or a no-argument launch, with no file open)
- **When** the user looks at the Copy Full Path control
- **Then** it is disabled in both the File menu and the toolbar, because there is no path to copy

### 9.11 Cut and Delete follow the view mode and the editor selection
- **Given** the window is in preview mode (read-only)
- **When** the user looks at the Cut and Delete controls (toolbar, Edit menu, and context menu)
- **Then** both are disabled in every surface, because preview cannot be edited
- **And given** the window is in edit or split mode with text selected in the editor
- **When** the user looks at the Cut and Delete controls
- **Then** both are enabled in every surface, and invoking them removes the selection (Cut also places it on the clipboard)
- **And given** edit or split mode with no editor selection
- **When** the user looks at the controls
- **Then** both are disabled again

### 9.12 File ▸ Reload reverts to the on-disk version
- **Given** a document open from a file
- **When** the user invokes File ▸ Reload (or the toolbar button)
- **Then** the buffer and preview are replaced with the current on-disk content; if there were unsaved edits, the user is asked to confirm discarding them first
- **And given** a blank window with no file open
- **When** the user looks at Reload
- **Then** it is disabled
- **And given** the file cannot be read when File ▸ Reload is invoked (permissions changed, transient I/O, the path is now a directory)
- **Then** an error dialog is shown naming the failure — Reload never fails silently (QA round-1 M2); this is specific to the explicit Reload gesture, not the monitor-driven auto-reload paths, which stay silent on a read failure since no user is awaiting that specific gesture there

### 9.13 Auto-Reload toggle gates live reload
- **Given** Auto-Reload is on (default), shown checked in the File menu and pressed in the toolbar
- **When** the file changes on disk
- **Then** the live-reload behaviour (silent reload or conflict prompt) occurs
- **And given** the user turns Auto-Reload off
- **When** the file changes on disk
- **Then** nothing happens until the user turns Auto-Reload back on, at which point the latest on-disk state is reconciled (caught up)

### 9.14 Edit menu mirrors the editor's Insert Emoji and Change Case
- **Given** the window is in edit or split mode
- **When** the user opens the Edit menu
- **Then** it offers Insert Emoji (opening the emoji chooser at the cursor) and a Change Case submenu (UPPER / lower / Title / tOGGLE) acting on the selection — matching the editor's own context menu
- **And given** preview mode (no editor)
- **When** the user opens the Edit menu
- **Then** those items are disabled (and Change Case also requires a selection)

### 9.15 Copy is enabled by a selection inside a table cell
- **Given** the preview (or split preview) showing a table, with text selected inside one table cell
- **When** the user looks at the Copy controls (Edit menu, toolbar, and context menu)
- **Then** Copy is enabled in every surface and copies the selected cell text — a table-cell selection enables Copy exactly as a buffer-text selection does
- **And given** the selection is then cleared
- **When** the user looks at Copy
- **Then** it is disabled again in every surface

### 9.16 Undo and Redo follow the editor's undo stack in every view mode
- **Given** any view mode after the user has made at least one edit
- **When** the user invokes Undo (Edit menu, toolbar, context menu, or Ctrl+Z)
- **Then** the last edit is reverted, and Redo becomes enabled in every surface; invoking Redo (or Ctrl+Shift+Z) re-applies it
- **And given** there is nothing left to undo (or to redo)
- **When** the user looks at Undo (or Redo)
- **Then** it is disabled in every surface — Undo and Redo enable only when the editor has a corresponding stack entry
- **And given** preview mode after creating a CriticMarkup annotation (which mutates the document from preview — §17.5)
- **When** the user invokes Undo
- **Then** it is enabled and reverts the annotation, and the preview updates to drop its highlight and margin marker (Undo/Redo are no longer gated on the editor being visible, precisely so a preview-made annotation can be undone where it was made)
- **And given** a freshly opened or reloaded document with no edits since
- **When** the user looks at Undo
- **Then** it is disabled: loading/reloading a file is not an undoable edit (Undo can never revert the buffer to the previous document or to empty)
- **And given** the user has undone an edit and then Redone it
- **When** they make any further discrete edit — a **format** command (Bold, Italic, a heading, a list), a smart-newline list/quote continuation, or an annotation — and then press Undo exactly once
- **Then** only that further edit is reverted; the redone edit survives. The two are independent undo steps and are never merged into one group, whichever routine made the second edit (`GtkTextBuffer`'s built-in undo leaves no barrier after a `redo()`, so a discrete edit must flush one itself before it records — see the undo-group seam below)
- **And** this holds by construction rather than by each routine remembering to do it: every routine that edits the buffer as a discrete undo step goes through one guard (`window::undo::UndoGroup`) that flushes the barrier and opens the action, and the raw `begin_user_action`/`end_user_action` calls are banned (`clippy.toml`), so a newly-added edit routine cannot re-introduce the merge

### 9.18 Help > About opens the About dialog
- **Given** a document window
- **When** the user selects Help ▸ About (F1 opens the keyboard-shortcuts window instead — §16.1)
- **Then** a modal About dialog opens showing the application name, version, description, copyright, and a **Scribobulate on GitHub** link to the project's source repository, which opens in the system browser
- **And** the description names no platform — the same copy ships on every platform, so a platform list there would go stale as platforms are added (it once read "Linux-first … also compatible with macOS" while a Windows build shipped)
- **And** the dialog shows a License button that displays the Apache License, Version 2.0 text
- **And** the System tab shows the GTK runtime version and the versions of the gtk4, sourceview5, and pulldown-cmark crates
- **And** the Credits tab carries two attribution sections: the bundled open-source components (the syntect/two-face grammars, whose upstream licences require their notice to travel with a binary distribution) and the AI assistance the project was developed with. The first is a licence obligation and the second is a voluntary acknowledgement — no regulation compels it, and the difference is worth knowing before either is edited away
- **And** every credit entry renders as plain text, never as a clickable `mailto:` link — GTK routes an `<…>` fragment in a credit or author line through its mail-address parser whatever scheme it carries (GTK4Rs/AP-50)

### 9.17 The View menu toggles the toolbar and status bar, and remembers the choice
- **Given** a document window
- **When** the user toggles **View ▸ Toolbar** (or **View ▸ Status Bar**) off
- **Then** the toolbar (or footer status bar) is hidden, and the View-menu checkbox reflects the new state; toggling it on restores it
- **And given** the toolbar is hidden
- **When** the user looks for a way to bring it back
- **Then** the always-visible menu bar's View menu still offers the Toolbar toggle — hiding the toolbar never strands the user
- **And given** the user has hidden the toolbar (or status bar), then closes and relaunches the application
- **When** the window reopens
- **Then** the toolbar (or status bar) is still hidden — the visibility is persisted in the session (TDD 7.2) and restored

### 9.19 Copy Document copies the whole document, not the selection
- **Given** a document is open, in any view mode
- **When** the user invokes Edit ▸ Copy Document (menu, toolbar, or its accelerator)
- **Then** the document's full Markdown source is placed on the clipboard, regardless of any current selection or which view mode is active — distinct from Copy, which only copies a selection

### 9.20 Go To Line jumps the editor caret to a chosen line
- **Given** edit or split mode, with the editor pane focused
- **When** the user invokes View ▸ Go To Line… (menu, toolbar, or Ctrl+G) and enters a line number
- **Then** a small dialog prompts for a line number, pre-filled with the caret's current line; confirming moves the caret to the start of that line and scrolls it into view, clamping an out-of-range number to the first/last line
- **And given** pure-preview mode (no editor pane), or edit/split mode with focus outside the editor (e.g. the preview pane, find bar, or a menu/popover)
- **Then** the command is disabled (menu item, toolbar button, and accelerator all inert) — there is no editor caret to place in preview mode, and no unambiguous "the editor" target without editor focus in split mode

### 9.33 Ctrl+Home and Ctrl+End reach the ends of a document however large it is
- **Given** edit or split mode with the editor pane focused, and a document large enough that GTK has not finished laying out every line — a freshly opened or reloaded one of tens of thousands of lines
- **When** the user presses Ctrl+End (or Ctrl+Home)
- **Then** the caret goes to the very end (or start) of the document **and the viewport follows it all the way there**, on the FIRST press — not part of the way, and without needing the key pressed again; on a document GTK has already laid out this is immediate, and on one it has not, it happens as soon as the layout is ready rather than never
- **And given** the user moves the caret elsewhere in the meantime (a click, Go To Line, Find, the opposite key)
- **Then** the pending jump is abandoned rather than dragging the reader to a place they have stopped asking for
- **And given** the read-only preview pane rather than the editor
- **Then** the same keys behave the same way — read-only does not exempt a pane from carrying GTK's buffer-ends bindings
- **And given** the focus sits on a **table cell** in the preview (the reader clicked into one, or tabbed to it) rather than on the pane itself
- **Then** these keys — and the rest of the document-navigation set: Home, End, ←, →, ↑, ↓, PageUp, PageDown, and their Ctrl forms — still move the *document*, exactly as they do with the pane focused; a cell is part of the document the reader is navigating, not a place navigation stops working (ScrAP-264)
- **And** a **selection-extending** key (any Shift form) still acts on the cell's own text, which is the only selection a table cell can hold, so keyboard selection inside a cell is not taken away to buy the above

### 9.34 Every far navigation arrives, however large the document
- **Given** a freshly opened or reloaded document of tens of thousands of lines, which GTK has not finished laying out
- **When** the user navigates to somewhere well outside the current viewport — View ▸ Go To Line, a Find hit, or an outline-sidebar entry
- **Then** the viewport arrives at that target; if GTK cannot compute the destination yet the navigation completes as soon as it can, rather than stopping wherever the layout happened to have reached
- **And given** the caret has moved on before the layout is ready
- **Then** the deferred arrival is abandoned, so a navigation the reader has already replaced never takes effect late

### 9.21 The footer shows the editor caret's line and column
- **Given** edit or split mode
- **When** the caret moves in the editor (typing, arrow keys, a click, Go To Line, Find, …), or the user switches to a different tab
- **Then** the footer status bar's right-hand side shows "Ln L, Col C" (both 1-based, C accounting for tab expansion) for the ACTIVE tab's editor caret — regardless of which pane currently has literal focus, so in split mode scrolling or clicking in the preview pane never changes it
- **And given** pure-preview mode (no editor pane)
- **Then** the indicator is hidden entirely, not shown stale or blank-but-present
- **And given** the user switches tabs
- **Then** the indicator immediately reflects the newly-active tab's own mode and caret position, never the tab just left
- **And given** the toolbar's content-derived min-width (I5) has forced the window wider than its monitor, **then** the "Ln L, Col C" indicator — and likewise the bottom-right conflict/reload/info toast's action buttons — stay within the visible monitor rather than off its right edge; on a display wide enough to hold the toolbar they show at their normal right-hand placement, unchanged (`window::chrome_fit::overflow_inset`, applied at each show/update point).

### 9.22 View ▸ Toolbar hides individual toolbar sections, remembers them, and never orphans a separator
- **Given** a document window with the toolbar shown
- **When** the user opens **View ▸ Toolbar** (now a submenu)
- **Then** it lists **Show** (the whole-toolbar toggle) first, a separator, then six section checkboxes — **File, Edit, Format, View, Split, Zoom** — each ticked to reflect that section's current visibility
- **And when** the user unticks one section (e.g. Zoom)
- **Then** exactly that section's buttons **and its leading separator** disappear together, the remaining sections keep their canonical left-to-right order, and no doubled or orphaned separator ever appears — a re-shown section returns to its original slot, never the end
- **And when** the user unticks the **last still-visible** section (so hiding it would leave the bar empty)
- **Then** the whole bar hides instead (as if **Show** were unticked) — that section's tick is **left checked**, not cleared — so the app never shows a confusing empty ~2px strip, and re-ticking **Show** brings the bar back showing exactly that one section (a lossless round-trip); no empty-bar state is reachable interactively
- **And when** the user unticks **Show** (hides the whole bar)
- **Then** all six section checkboxes become **disabled/greyed but keep their ticks**, and re-ticking Show restores the exact per-section configuration that was in effect before — nothing dropped, nothing extra shown
- **And given** any section (or the whole bar) is hidden
- **Then** that section's *commands* stay live — its accelerators still work (a hidden Zoom section still zooms on Ctrl++), and every toolbar button's own enabled/greyed state is exactly what view mode / editor focus / the zoom ladder dictate, unchanged by any show/hide (visibility is orthogonal to command sensitivity)
- **And given** sections have been hidden so the toolbar is narrower
- **Then** the window can be dragged narrower than before (its content-derived minimum width drops to hug the reduced toolbar)
- **And given** the user hides some sections (and/or the whole bar), then closes and relaunches the application
- **When** the window reopens
- **Then** the same sections are hidden and the same checkboxes ticked — per-section visibility is app-wide and persisted in the session (TDD 7.2), exactly like the whole-bar toggle
- **And given** a fresh profile with no saved session (or a session file predating the per-section feature)
- **When** a window opens
- **Then** the toolbar shows **File, Edit, and View** and hides **Format, Split, and Zoom** (those three `View ▸ Toolbar` checkboxes start unticked) — a deliberately short default toolbar the user extends by ticking the sections their workflow needs; formatting stays fully available meanwhile via the Format menu and its accelerators even with the Format section hidden

### 9.23 The menus are keyboard-navigable via mnemonics and access keys
> **Platform scope: the Alt+letter clause below is Linux and Windows only.** It
> describes a menu bar drawn *inside* the window, which is what those desktops
> expect (9.35). macOS has no mnemonic mechanism at all — its menus are native,
> Option+letter types an alternate character rather than addressing a menu, and
> the toolkit strips the `_` markers when it builds the native items, so there is
> nothing underlined to press. The keyboard route to the menus there is the
> system's own (Ctrl+F2, "Move focus to the menu bar"), which the application
> neither provides nor can break. This is a carve-out for the *menu bar* only:
> every clause below about the **context menus'** access keys holds on all three
> platforms, because those are drawn by the application (9.24).
- **Given** the application window is focused, on a platform whose menus are in the window
- **When** the user presses Alt+F, Alt+E, Alt+R, Alt+V, or Alt+H
- **Then** the File, Edit, Format, View, or Help menu opens respectively, with the access letter shown underlined while Alt is held
- **And when** a menu (or one of its submenus) is open and the user presses a row's underlined letter with no modifier
- **Then** that item activates — e.g. Edit then `t` is Cut, View then `o` is Outline and `e` is Edit mode — and because each access letter is unique within its open popover, no keystroke is ambiguous (a collision would merely cycle focus, never activate)
- **And** the underscore mnemonic marker never leaks into a surface that shares the command label but does not interpret it: a toolbar button's tooltip reads "Save", not "_Save"

### 9.24 The document context menu shares the menu bar's access keys, with Change Case as a submenu
- **Given** edit or split mode with a text selection, and the editor's right-click context menu open
- **When** the user presses a row's underlined letter
- **Then** the command activates using the SAME access letter as the Edit menu (Cut = `t`, Copy = `c`, Undo = `u`, …), so the key learned in one surface works in the other
- **And** Change Case is a submenu, not a flat list: pressing `g` (or clicking the row) slides to a sub-page whose four variants carry the menu bar's own Change Case letters (UPPER `u` / lower `l` / Title `t` / tOGGLE `c`); pressing one transforms the selection and dismisses the menu
- **And** each fresh right-click reopens on the main page, so `t` is always Cut there, never Title on the submenu
- **And** a disabled row ignores its access key

### 9.25 In split mode, Copy and Select All follow the focused pane
- **Given** side-by-side (split) mode, with the editor and preview panes both visible
- **When** the user focuses the PREVIEW pane and selects text there
- **Then** Copy (Edit menu, context menu, toolbar, Ctrl+C) is enabled and copies the PREVIEW selection, and Select All (Ctrl+A, Edit menu, context menu) selects all of the PREVIEW
- **And when** the user instead focuses the EDITOR pane and selects text there
- **Then** the same commands act on the EDITOR
- **And when** the focused pane has no selection — even if the OTHER pane still visibly shows one
- **Then** Copy is disabled: enablement and target track the pane that holds focus, not a fixed pane (the editor). Focus stolen by a transient surface (the context-menu/menu-bar popover, the find bar) does not change which pane is considered focused
- **And** in preview-only or edit-only mode there is a single visible view, so these commands act on it exactly as before — the focused-pane distinction applies only to split mode

### 9.26 Activating a nested-submenu item leaves no other menu open
- **Given** the menu bar with a nested submenu open — Format ▸ Heading ▸ (Heading 1–6), Edit ▸ Change Case ▸ (UPPER / lower / Title / tOGGLE), or View ▸ Documents ▸ (a tab)
- **When** the user activates one of its items
- **Then** that item's action fires (the heading tier is applied, the case is changed, or the tab is switched) **and** the entire menu chain closes — no other top-level menu (in particular the adjacent Edit menu) is left popped open afterward
- **And given** a plain, non-nested top-level item (e.g. View ▸ Zoom In, File ▸ Save)
- **When** the user activates it
- **Then** its action fires and the menu closes cleanly, exactly as for the nested case — the two paths are indistinguishable to the user, no stray menu in either

### 9.27 Copy tracks a selection from a preview-only window's first render
- **Given** a window freshly opened in preview-only mode — shown from its first render, with no prior view-mode or tab change — and text selected in the body OR inside a table cell (a selection island)
- **When** the user opens the Edit menu, right-clicks for the context menu, or looks at the toolbar
- **Then** Copy is enabled in every surface and copies that selection — selection tracking is live from the first render, not only after a later mode or tab switch
- **And given** the same fresh preview-only window with nothing selected
- **When** the user checks any Copy surface
- **Then** Copy is greyed out

---

### 9.28 Ctrl+A in a text entry selects the entry's text, not the document
- **Given** the find bar is open and its entry holds keyboard focus, with an existing selection in the document
- **When** the reader presses Ctrl+A
- **Then** the **find entry's own text** is selected, and the document's selection is unchanged
- **And** the same holds for the Find & Replace **replace** entry, and for the annotation comment card's entry — every text entry in the window, not a hand-picked list of surfaces
- A window `GAction` accelerator dispatches at capture phase, root→target, *before* a focused widget's own bubble-phase keybinding — so an un-stood-down `win.select-all` beats every entry in the window and selects the document instead. The standdown is keyed on the focused widget's **type** (`gtk::Text`, the delegate every `GtkEntry`/`GtkSearchEntry` focuses into), so a new entry added anywhere is covered without anyone remembering to tag it

### 9.29 Standing the action down is what gives the entry its own select-all
- **Given** focus is in any text entry
- **Then** `win.select-all` is **disabled** — and Ctrl+A still works *in the entry*, because a failed shortcut activation lets propagation continue to the focused widget's own binding
- The entry does not merely stop losing the key; it **gains** a select-all it never had. A fix that withheld the accelerator without disabling the action would leave the entry inert

### 9.30 The widened standdown does not touch the panes
- **Given** the editor or preview pane holds keyboard focus, with no entry involved
- **When** the reader presses Ctrl+A
- **Then** the whole document selects, exactly as before
- `GtkTextView` is a different GObject type from `gtk::Text`, so the type-keyed predicate leaves document-wide select-all correct in the panes for free — this is the case the widening must not regress

### 9.31 Every command icon drawn without a glyph fallback resolves on every platform
- **Given** Scribobulate is running on any supported desktop, under whatever icon
  theme that host happens to ship
- **When** the user looks at the menu bar, the toolbar, or a context menu
- **Then** every command that is drawn as an icon alone shows a real glyph — never
  the broken-image placeholder
- **And** this holds with no install step having populated a filesystem icon theme:
  a name the host theme lacks resolves from the binary's own bundle instead
- Only the Format commands may fall back, and they fall back to a *letter* (B, I,
  •, 1., …), not to a placeholder — every other icon is load-bearing
- The host theme still wins where it has the name, so a KDE or GNOME desktop keeps
  its own idiomatic art; the bundle only fills gaps

### 9.32 Copy Link Location copies the link the reader pointed at, or the one under the editor caret
- **Given** the user **right-clicks a link** — a rendered link in the preview pane (including a link that is a whole table cell), or a Markdown link in the editor source — in any view mode
- **When** they choose Copy Link Location from that context menu
- **Then** the clipboard holds **that** link's destination: the one under the pointer, not merely some link in the document
- **And given** edit or split mode with the editor caret anywhere inside a Markdown link — `[caption](url)` or an image `![alt](url)`, from the opening bracket through the closing parenthesis
- **When** the user invokes the command from a surface that has no pointer — the Edit menu, the Edit toolbar section, or a context menu opened away from any link
- **Then** the clipboard holds the caret's link destination **only** — not the caption, not the surrounding markup, and not any `"title"` that follows the URL; a destination containing a balanced parenthesis pair (`…/Ruby_(gem)`) is copied whole
- **And** when both are true, the right-clicked link wins: the reader singled it out
- **And given** no link is pointed at and the caret is in ordinary prose, in a bare `[bracketed]` span, or in a reference link (`[text][ref]`), which carries no inline destination
- **When** the user looks at the command
- **Then** it is disabled in every surface simultaneously — menu bar, toolbar, and both context menus — and it re-enables the moment the caret moves back into a link, or an edit puts a link under a caret that never moved
- **And given** preview-only mode, which has no editor pane and so no caret
- **Then** the command is disabled in the menu bar and toolbar (like every other editor-only Edit command, 9.11) yet **enabled in the preview's context menu whenever that right-click landed on a link** — reading a link's target is not an editing operation
- **And** a right-clicked target lasts exactly as long as its menu: once the context menu closes, the command reverts to what the caret alone allows, so no surface stays enabled for a link nobody is pointing at

### 9.35 The menus appear where the desktop keeps menus, and only once
- **Given** the application is running on macOS
- **When** the user looks for the menus
- **Then** they are in the **system menu bar** at the top of the screen — File, Edit, Format, View and Help, following the standard application menu that carries About, Hide and Quit
- **And** there is **no second menu row inside the window**: the window's own first row is the toolbar, and no File/Edit/Format/View/Help strip is drawn below the title bar
- **And** the system menu bar shows **the application's own menus**, never a toolkit placeholder — an Edit menu holding only the generic Undo/Cut/Copy/Paste entries, or a Window menu the application never defined, means the real model was never handed over
- **And given** the window has been put into the operating system's own full-screen mode
- **When** the user moves the pointer to the top of the screen to reach the menus, and clicks a menu once
- **Then** that menu opens on the **first** click — the reveal that the pointer triggers *is* the menu bar, so nothing is drawn over the menus and no click is spent on something else
- **And** no other control is activated by that click, and in particular the window is not closed by it
- **And given** more than one document window is open
- **When** the user brings a different window to the front and opens View ▸ Documents
- **Then** the list names **that** window's tabs, and Format ▸'s Link/Image items read Insert or Edit according to **that** window's selection — the menus follow the active window rather than showing whichever window was built last
- **And** each item's enabled state follows the active window too: a command unavailable in that window is greyed there and available in the other
- **And given** any other platform, whose desktops keep menus inside the window
- **Then** the menu bar is the window's first row exactly as before, and the system menu bar is not used

### 9.36 Keyboard shortcuts use the platform's own command modifier
- **Given** the application is running on macOS
- **When** the user presses the shortcut for any command — Save, Open, Copy, Find, Undo, New Document
- **Then** it is the **Command** key that invokes it: Cmd+S saves, Cmd+O opens, Cmd+F finds
- **And** the same combination with Control instead does nothing — the shortcut moved, it was not merely duplicated
- **And** every surface that *shows* a shortcut agrees with the key that works: the menu item's hint, the toolbar tooltip, the context-menu hint and the Keyboard Shortcuts window all read ⌘, never ⌃
- **And given** a shortcut the operating system itself reserves — Cmd+H hides an application, Cmd+Option+H hides the others
- **Then** the system keeps it: pressing Cmd+H hides Scribobulate rather than opening Find & Replace
- **And** the command that would have claimed it is still reachable from the keyboard under a different combination, and every surface shows that combination — a command may be moved by this rule, never silently left with a shortcut that does nothing
- **And given** any other platform
- **Then** the shortcuts are unchanged — Ctrl+S saves, and every surface reads "Ctrl"
- **And** on no platform do two different commands share one keystroke

## 10. Markdown formatting commands

> The Format menu, its toolbar section, and the caret overlay all drive one
> parameterised `win.format` action that wraps the editor selection (or caret) in
> Markdown markup. The command set is Bold, Italic, Heading 1–6, Strikethrough,
> Highlight (`==mark==`), Code Span, Superscript, Subscript, Code Block, Quote,
> Bulleted List, Numbered List, Task List, and Horizontal Bar. Lists and blockquotes additionally
> auto-continue on Enter, and a lone code fence auto-closes (10.13–10.14).

### 10.1 An inline command wraps the selection
- **Given** edit or split mode with text selected in the editor
- **When** the user invokes Bold, Italic, Strikethrough, Highlight, Code Span, Superscript, or Subscript (menu, toolbar, overlay, or accelerator)
- **Then** the selection is wrapped in the corresponding Markdown markup (`**`, `*`, `~~`, `==…==`, `` ` ``, `^…^`, `~…~`) and stays selected

### 10.2 Re-applying an inline command toggles it off
- **Given** a selection that is already wrapped in a command's markup (the markers inside or immediately outside the selection)
- **When** the user invokes that same command
- **Then** the markup is removed rather than doubled

### 10.3 An inline command with no selection brackets the caret
- **Given** the caret is in the editor with no selection
- **When** the user invokes an inline command
- **Then** an empty pair of markers is inserted with the caret placed between them

### 10.4 Heading applies a tier to the spanned line(s)
- **Given** the caret on a line, or a selection spanning one or more lines
- **When** the user picks Heading 1–6 (the Format ▸ Heading submenu, the toolbar `(Hn)` menu, the overlay, or Shift+F1–F6)
- **Then** each spanned line is prefixed with that many `#` followed by a space, replacing any existing heading prefix; picking the same tier again removes the heading

### 10.5 Block commands prefix, fence, or insert
- **Given** a selection (or caret) in the editor
- **When** the user invokes Quote, Code Block, Bulleted List, Numbered List, Task List, or Horizontal Bar
- **Then** Quote prefixes each spanned line with `> `, Code Block fences the spanned lines with ```` ``` ```` lines, Bulleted List prefixes each spanned line with `- `, Numbered List prefixes each spanned line with a `1. `, `2. `, `3. `… number sequence (renumbered from 1), Task List prefixes each spanned line with a GFM task marker `- [ ] `, and Horizontal Bar inserts a `---` rule on its own line
- **And** Quote, Code Block, Bulleted List, Numbered List, and Task List toggle **off** when re-applied to already-formatted text (accepting any bullet marker `- `/`* `/`+ ` or ordered delimiter `.`/`)` across a mixed run; Task List additionally requires a checkbox `[ ]`/`[x]`/`[X]` after the bullet, so a bare bullet is **not** treated as a task item)
- **And** the preview renders the Bulleted/Numbered result as a real list, and the Task List result as real (read-only) checkboxes

### 10.6 Format commands require editor focus
- **Given** preview mode, or any state where the editor pane does not hold keyboard focus
- **When** the user looks at the Format menu, the toolbar Format buttons, and the heading control
- **Then** the Format commands are disabled; they enable only while the editor has focus
- **And when** the user clicks a Format toolbar button or opens the heading menu
- **Then** doing so does not itself disable the commands (focus is not stolen from the editor)
- **And when** in edit/split the user opens the find bar and navigates to a match (which selects it in the editor while keyboard focus is in the find field)
- **Then** the Format commands — and the caret overlay that appears over the match — stay **enabled**: the find bar is a sticky transient surface, like the Format toolbar, so it does not disable the focus-gated commands
- **And when** the user switches to preview via the **View menu** (keyboard focus stays in the menubar the whole time)
- **Then** the Format commands still disable — disabling in preview is gated on the view mode, not only on focus, so the menu-driven switch greys every Format surface together (menu, toolbar, overlay, heading control), consistent with the toolbar

### 10.7 Accelerators apply the markup
- **Given** edit or split mode with editor focus
- **When** the user presses a Format accelerator (e.g. Ctrl+B, Ctrl+Shift+X, Shift+F2)
- **Then** the corresponding markup is applied, matching the hints shown in the Format menu

### 10.8 The caret overlay appears on selection
- **Given** edit or split mode
- **When** the user selects a non-empty range of text
- **Then** a small formatting overlay with the same commands appears centered above the selection with a downward arrow
- **And when** the user applies a command from the overlay
- **Then** the selection is preserved so commands can be chained, and the overlay dismisses on Escape, on scrolling, when the selection clears, or when the editor loses focus

### 10.9 The heading control shows a placeholder caption
- **Given** the toolbar (or overlay) heading control at rest
- **When** the user looks at it
- **Then** it reads `(Hn)` with no tier selected, and its menu lists H1–H6; picking a level applies it and the caption returns to `(Hn)`

### 10.10 Superscript and subscript render raised and lowered
- **Given** a document containing `E=mc^2^` and `H~2~O`
- **When** it is rendered in the preview
- **Then** the `2` after `^…^` appears **raised and smaller** (superscript) and the `2` in `~…~` appears **lowered and smaller** (subscript), with the `^`/`~` markers removed
- **And** recognition is **tight** in the Pandoc sense — a marker opens a run only when its partner arrives before any whitespace — so `2^10` (never closed), `1~2` and `a^b c^d` (a space inside) stay **literal**

### 10.10a Strikethrough renders struck, and one tilde never becomes two
- **Given** a document containing `~~struck~~`
- **When** it is rendered in the preview
- **Then** it appears **struck through** with the `~~` markers removed, and its content may contain spaces (`~~several words gone~~`) — unlike the tight `^`/`~` runs of 10.10
- **And** a single `~` is subscript while a double `~~` is strikethrough, and the two do not interfere: a multi-tilde line `H~2~O and CO~2~` renders **both** subscripts and no strike

### 10.10b A tight fence spans the inline markup it wraps
- **Given** a `~~` or `==` fence that *wraps other inline markup* — `~~a **bold** b~~`, `==a *em* b==`, a fence around a link, or one spanning a soft line break
- **When** it is rendered in the preview
- **Then** the whole run is struck (or highlighted) **including the nested markup**, which keeps its own formatting, and the `~~`/`==` markers are removed — the fence is recognised across the inline markup it wraps, not only within one unbroken run
- **And** the same run copies back as its original source with both delimiters intact, exports struck, and reads without markers in the outline; annotating any part of it wraps the **whole** fence rather than landing `{==…==}` between its halves
- **And** a fence that *interleaves* with the markup rather than nesting inside or around it (`~~a **b~~ c**`, whose closing `~~` sits inside a `**` that opened inside the fence) stays **literal** — it describes no tree, so it is refused rather than rendered as a guess; likewise a `~~`/`==` that would have to span a **block** boundary (two table cells, two paragraphs) or that is really the content of a code span

### 10.11 Insert Link / Image / Table prompt for fields and splice once
- **Given** the editor pane is focused, with an optional selection
- **When** the user invokes Insert Link, Insert Image, or Insert Table (Format menu — below a separator, after the inline commands — / toolbar / caret overlay / Ctrl+L for Link, Ctrl+Alt+I for Image, Ctrl+Shift+T for Table)
- **Then** a small modal dialog appears prompting for the fields (Link: text + URL; Image: alt + URL + title; Table: columns + rows + first cell), with the selection pre-filled into the caption / alt / first-cell field
- **And** the field taking initial keyboard focus is the one the user still has work to do in, consistently for **both** Insert Link and Insert Image: with a selection (or when editing existing markup) the caption / alt text is already settled, so **the URL field is focused** and typing goes straight into it without a Tab; with no selection every field is empty and the first field (Text / Alt text) is focused. Insert Table is unaffected — its pre-filled Columns/Rows are defaults the user may well want to change, so focus stays on the first field.
- **And** confirming inserts the corresponding Markdown — `[text](url)`, `![alt](url "title")`, or a GFM table skeleton — as a **single undoable edit** (one Ctrl+Z reverts the whole insertion), and the preview renders it as a real link / image / table
- **And** Cancel or Escape closes the dialog leaving the document unchanged
- **And given** the Insert Link or Insert Image dialog, its URL field has a **Browse…** button (mnemonic Alt+R) that opens a local file chooser (Image filters to image files; Link offers any file); the chooser starts in the loaded document's folder (defaults when no document is loaded), and the chosen file fills the URL field — as a path relative to the document's folder when the file lives under it, else the absolute path
- **And given** the selection is **exactly** one existing link (for Insert Link) or one existing image (for Insert Image)
- **When** that command is invoked
- **Then** the dialog opens as "Edit Link" / "Edit Image" with every field pre-filled from the selected markup, and confirming **replaces** the selection with the updated markup (not a re-wrap) — a mismatched selection (image markup under Insert Link, or a link under Insert Image) is treated as a normal insert with the raw selection as the caption / alt
- **And** while that selection is held (in edit/split mode), the command's surfaces relabel **Insert → Edit**: the Format menu item, the toolbar button tooltip, and the caret-overlay button tooltip all read "Edit Link" / "Edit Image"; they revert to "Insert …" when the selection clears or formatting is unavailable (preview mode / editor unfocused). The app-level Format menu follows whichever window is focused.

### 10.12 The caret overlay works in a tab moved to another window
- **Given** a tab in an editor-visible mode (Edit or Split) has been moved to a new window (View ▸ Move Tab to New Window or a cross-window drag)
- **When** the user selects text in that tab's editor in the new window
- **Then** the caret formatting overlay appears over the selection in that window — exactly as it would have before the move

### 10.13 Lists and blockquotes auto-continue on Enter
- **Given** the caret on a list-item or blockquote line in the editor (a bulleted `- `/`* `/`+ ` item, an ordered `1. `/`1) ` item, a task `- [ ] `/`- [x] ` item, a `>` blockquote — including a list nested in a quote like `> 1. x` or `> - [ ] x` — at or past the marker)
- **When** the user presses Enter (Return, keypad or ISO Enter, with no modifiers — Shift/Ctrl+Enter is a plain line break)
- **Then** if the line has content, a new line begins carrying the same continuation: the blockquote prefix repeated verbatim, the same bullet, the number incremented by one (same `.`/`)` delimiter), or a **fresh unchecked** task box `- [ ] ` (continuing a `- [x] ` item still starts unchecked) — with leading indentation preserved and the caret placed after the new marker
- **And** if the line is empty (only whitespace after the marker(s)), the marker is removed from the current line and **no** new line is added (the list/quote ends)
- **And** each such Enter is a **single** undoable edit — one Ctrl+Z removes the whole inserted `\n<marker>` (or restores the cleared marker), and redo mirrors it
- **And** a newline the user did **not** type never triggers any of this: a paste (Ctrl+V or middle-click PRIMARY), a drag-and-drop of text, an undo/redo replay and every programmatic edit are left to GTK untouched, landing **verbatim and complete** — every line of a pasted block arrives, including the last, whether or not the copied region carried syntax highlighting, and whether or not the destination line is itself a list item (ScrAP-199: a same-app paste arrives as several `insert-text` emissions, one of which is a bare `\n`, so "the inserted text is a newline" is not a test for "the user pressed Enter")

### 10.14 A lone *opening* code fence auto-closes on Enter
- **Given** the caret at or past a line that is a lone opening code fence — leading indentation, a run of three or more backticks, and nothing else (no language/info string) — where the document above is **not** already inside an open fenced block
- **When** the user presses Enter
- **Then** a matching closing fence (same indentation and backtick run) is placed on the line below, and the caret lands on the empty middle line between the two fences
- **And** it is a **single** undoable edit (one Ctrl+Z removes the whole auto-closed fence)
- **And** a fence carrying a language (e.g. ```` ```rust ````) is left alone — a normal newline
- **And** a lone fence that is the **closing** delimiter of an already-open block (the document above is inside a fenced block) does **not** auto-close — a normal newline, so it never stacks a spurious second fence

### 10.15 A different inline emphasis nests over existing markup without destroying it
- **Given** a selection whose text is already wrapped in one inline emphasis marker (e.g. `**bold**` or `~~struck~~`)
- **When** the user applies a *different* inline command whose marker shares the same delimiter character but a different length (Italic `*` over `**bold**`, or Subscript `~` over `~~struck~~`)
- **Then** the new emphasis **nests** around the existing markup (`**bold**` → `***bold***`, `~~struck~~` → `~~~struck~~~`) — the existing markers are preserved, never collapsed to a shorter run or stripped (a single `*` must not turn `**bold**` into `*bold*`, which would silently drop the bold)
- **And** re-applying the *same* command to a genuinely wrapped run still toggles it off (10.2 unchanged): a single-character marker toggles off only on an **odd** same-character run (`*x*`, `***x***`), never on the **even** run that is the other marker (`**`, `~~`)

### 10.16 Highlight renders as a translucent wash- **Given** a document containing `==marked==` text
- **When** it is shown in the preview (split or preview-only render it; edit-only shows the source)
- **Then** the marked span is drawn with a **translucent background wash** behind its characters, the `==` markers removed, and the body text stays legible on top — rendered immediately, in every mode that shows the preview
- **And** the wrap / toggle-off / empty-caret behaviour and the copy-to-source round-trip (`==text==`) are the shared inline-command contracts (10.1–10.3) and the copy contract (§2.8), not restated here

### 10.17 The highlight colour is sourced per reading theme- **Given** a document with `==marked==` text shown in the preview
- **When** the active reading theme changes
- **Then** the wash colour comes entirely from that theme's `mark_bg` key — never a literal — resolving to pale yellow (System), warm tan-rose (Sepia), muted green (Bedtime), radioactive toxic-green (Synthwave), amber phosphor (Terminal), and vivid lime (Candy)
- **And** where a theme also states `mark_fg`, the marked text takes that ink on both the body and the in-cell path; where it does not, the marked text keeps the body foreground and only its background changes
- **And** switching the reading theme at runtime recolours every marked span, in every open window, without a restart

### 10.18 Highlight holds inside containers, including table cells- **Given** `==marked==` text inside a list item, a blockquote, and a table cell
- **When** the document is rendered
- **Then** each marked span shows the wash exactly as it does at top level
- **And** the table-cell case (drawn as Pango markup, since a cell is a `GtkLabel` outside the buffer) takes its colour from the **same** `mark_bg` key as the body-text path — one key, no second literal, no drift

### 10.19 Highlight recognition is tight-flanked- **Given** text containing `==` runs
- **When** it is parsed for highlighting
- **Then** a fence is recognised only when non-whitespace immediately follows the opening `==` and precedes the closing `==` (interior spaces allowed, e.g. `==a marked phrase==`)
- **And** ordinary prose where `==`/`=` are operators or spaced (`a == b`, `x -= 1`, `y == 2`, `== spaced ==`) renders literally, with no wash

### 10.20 A block command applied inside a blockquote works inside the quote
- **Given** the caret (or a selection) on a blockquoted line in the editor — `> Heading`, `> item`, including a nested `> > note` or a tight `>>note`
- **When** the user invokes Heading 1–6, Bulleted List, Numbered List, Task List, or Code Block
- **Then** the marker is placed **inside** the quote and the quote survives — `> ### Heading`, `> - item`, `> 1. item`, `> - [ ] item`, and a code block fenced as `> ``` ` / content / `> ``` ` — with the line's exact `>` nesting and spacing preserved; the preview renders a real heading / list / code block *within* the blockquote
- **And** the same command re-applied toggles it **off** again, leaving the quote intact (`> ## Title` + Heading 2 → `> Title`; `> - item` + Bulleted List → `> item`), and picking a different heading tier re-tiers in place rather than stacking a second prefix
- **And** Quote itself is unaffected: it still prefixes each spanned line with `> `, adding a nesting level where a line is already quoted, and removing exactly one level when toggled off
- **And** a selection spanning quoted and unquoted lines formats each line in place — the unquoted lines gain no quote marker and the quoted ones keep theirs
- **And** a list marker is *not* treated as a container: Heading on `- item` still yields `## - item`

---

## 11. Find & replace

> A bottom find bar searches whatever the user is looking at — the editor buffer in
> edit/split, the rendered preview in pure-preview mode; replace is available where
> the buffer is editable.

### 11.1 Opening find and find-replace
- **Given** a document window
- **When** the user presses Ctrl+F (or Edit ▸ Find) or Ctrl+H (or Edit ▸ Find & Replace)
- **Then** a find bar slides in at the bottom of the window with the search field focused; Ctrl+H additionally reveals the replace row

### 11.2 Incremental search highlights matches with a count
- **Given** the find bar is open
- **When** the user types a search term
- **Then** all matches in the active view (the editor in edit/split, the **preview** in pure-preview mode) are highlighted and a count is shown ("N of M", or "No matches"); clearing the field hides the count

### 11.3 Navigating matches wraps around
- **Given** a search term with multiple matches in the active view (editor or preview)
- **When** the user presses Enter or clicks Next (or Shift+Enter / Prev)
- **Then** the selection advances to the next (or previous) match and scrolls it into view, wrapping past the last match to the first (and before the first to the last) — find-**next** must genuinely advance, not re-select the current match
- **And given** a document large enough that the editor is still counting when the first Next is pressed (the counter shows "…" rather than a number)
- **When** the user steps through several matches without waiting for that count to settle
- **Then** the position reported for each step is the match actually landed on — or, where it genuinely cannot yet be known, none at all — never a "1" standing in for "not known yet"; so when the count does settle, the counter names the match the selection is sitting on rather than the first one

### 11.6 Find works on the preview pane
- **Given** pure-preview mode (the editor is not visible)
- **When** the user opens find and searches
- **Then** matches in the rendered preview are highlighted and Next/Prev navigate and scroll the preview to each match (the search targets the preview's text, not the hidden editor buffer)

### 11.4 Replace acts only where the buffer is editable
- **Given** the find bar with the replace row in edit or split mode
- **When** the user invokes Replace or Replace All
- **Then** Replace changes the current match (and advances) and Replace All changes every match in the editor
- **And given** preview mode
- **When** the user looks at the replace controls
- **Then** they are disabled (with an explanatory tooltip), because the preview is read-only

### 11.5 The find bar survives mode switches and closes cleanly
- **Given** the find bar is open
- **When** the user switches view mode
- **Then** the bar stays open (it is not part of the swappable content area)
- **And** the preview find-match highlights **survive every boundary that rebuilds the preview buffer** — view-mode switch, runtime theme switch, and external reload — re-applied for the active tab rather than left bare until the next match is cycled (Document Rendering CAM row 8; ScrAP-38)
- **And** a match position is never carried from one pane's occurrence list into the other's: the editor's list and the preview's unified body+cell list are numbered independently, so after a mode switch the counter and the next Next/Prev either resume in the list the visible pane actually owns or start from the top — never at a number that was a position in the *other* list
- **And when** the user presses Escape or the close button
- **Then** the bar hides, match highlighting clears (both body text and inside table cells), and focus returns to the editor
- **And given** pure-preview mode scrolled to an arbitrary reading position (including within a tall table)
- **When** the user closes the find bar
- **Then** the reading position does not move at all — closing find never reloads or re-scrolls the preview

### 11.7 Find scrolls to a match inside a table cell
- **Given** pure-preview mode showing a **tall** table (taller than the viewport) whose cells contain the search term, with the find bar open
- **When** the user navigates (Next/Prev) onto a cell match, then onto a further cell match in the **same** table
- **Then** the preview scrolls so each matched **cell's own row** is brought into view — not merely the table's top — so consecutive in-cell matches each move the viewport rather than appearing "stuck" at the first table's top (the cell→table two-step `scroll_to_cell_offset`/`cell_row_y_h`, ScrAP-109)

### 11.9 Find matches every piece of text the reader can see, links included
- **Given** pure-preview mode showing a document whose link text appears in each context it can — a body paragraph, a heading, a list item, a blockquote, a table cell **alongside other text**, and a table cell that is **nothing but the link**
- **When** the user searches for a word that occurs in every one of those link captions
- **Then** the count includes them all and Next/Prev navigates to each in turn, highlighting it — a link's caption is text on the page, so which widget happens to render it (buffer text, a cell label, or the caption inside a table cell's link button) can never decide whether find sees it (ScrAP-250)

### 11.8 Find never acts on a pane the user cannot see
- **Given** pure-preview mode, and a preview view the application cannot resolve (a widget-tree change has broken the lookup)
- **When** the user searches, or presses Next/Prev
- **Then** nothing is highlighted, scrolled or counted, and the failure is logged — the hidden editor is never searched as a fallback, because acting on an invisible buffer is worse than not acting
- **And** the match counter shows no matches rather than the hidden editor's count, which would be a confidently wrong number in place of a missing one

### 11.10 Find reaches a match inside a collapsed disclosure
- **Given** a document with a collapsed disclosure whose body contains the search term
- **When** the user searches for that term
- **Then** the disclosure containing the match expands, and the match is scrolled to and highlighted like any other — a match is never reported at a location the user cannot see
- **And** the match **counter counts it before it is reached**: what a document contains is not what the viewport shows, and reporting "No matches" for text plainly in the document is the failure 11.8 already names as worse than not acting
- **And** a match inside a disclosure nested in another collapsed disclosure is reached by the same single step — expanding the outer block reveals only the inner block's summary, so one gesture opens as many levels as stand in the way

## 12. Document outline

> A collapsible sidebar lists the document's headings and navigates to them.

### 12.1 The outline lists every heading with its level and text, in order
- **Given** a document with nested headings (e.g. H1 ▸ H2 ▸ H3)
- **When** the outline is shown
- **Then** it lists every heading, in document order, indented/nested by level, with the heading's plain text

### 12.2 An empty or heading-less document shows a placeholder, not an error
- **Given** an empty document, or one with no headings
- **When** the outline is shown
- **Then** the panel shows a muted "No headings" placeholder and the application does not error

### 12.3 Inline formatting is stripped from the outline text
- **Given** a heading whose text contains inline markup (`**bold**`, `` `code` ``, a link)
- **When** the outline is shown
- **Then** the entry shows the plain heading text, without the Markdown markers

### 12.4 Activating an entry navigates to that heading
- **Given** the outline is shown in preview or split mode
- **When** the user activates an outline entry
- **Then** the preview scrolls so that heading is at the top of the view
- **And given** pure-edit mode
- **When** the user activates an entry
- **Then** the editor caret moves to that heading in the source

### 12.5 Single click and keyboard both navigate
- **Given** the outline has focus or is clicked
- **When** the user single-clicks an entry, or moves the selection with the arrow keys
- **Then** navigation happens immediately (no double-click required)

### 12.6 Subtrees collapse and expand via a visible chevron
- **Given** an outline entry that has nested sub-headings
- **When** the outline is shown
- **Then** the entry shows a **visible** disclosure chevron, and clicking it folds/unfolds the subtree without navigating

### 12.7 The outline toggles on and off
- **Given** a document window
- **When** the user toggles the outline (View ▸ Outline, the toolbar button, or F9)
- **Then** the sidebar hides and the content reclaims the width; toggling again restores it

### 12.8 Navigating a long document never blanks the preview
- **Given** a long document containing blockquotes, horizontal rules, code blocks, and/or tables, shown in **preview OR split** mode
- **When** the user rapidly clicks outline entries to jump around (incl. far top↔bottom jumps and fast window/splitter resizes)
- **Then** the preview always stays rendered — it never goes blank, never spams "snapshot … without a current allocation", and shows no spurious horizontal scrollbar (regression guard for the child-anchor reflow blank, ScrAP-23)
- **And** this holds even when a table or rule is nested inside a **list item or blockquote**: an indented anchored child is bounded to `content − 1 − inset`, where `inset` is the horizontal margin its enclosing block steals (list = left-only `depth·list_step`; blockquote = both sides `2·(bar+gap)`), so it never extends past the viewport and never summons the Automatic h-scrollbar (ScrAP-23a)
- *(Met for blockquotes/rules/code blocks — buffer text + `snapshot_layer` chrome — and for tables, which use the custom measure-stable `ScribTableWidget` bounded to `content − 1` (the `SPACE_FOR_CURSOR` reservation), minus the enclosing block's indent inset for a nested table/rule. No content type embeds a churning anchored widget.)*

### 12.9 Outline navigation in split mode drives the preview
- **Given** split (side-by-side) mode
- **When** the user activates an outline entry
- **Then** the preview scrolls to that heading and the editor follows it — the preview is the scroll driver for the jump, rather than the editor's position overriding it

### 12.10 Scroll and outline position survive a view-mode switch
- **Given** the user has moved into a document — scrolled down in preview, or the caret placed in a lower section in edit/split — with the outline reflecting the current section
- **When** the user switches view mode (preview, edit, or split)
- **Then** the document stays at approximately the same position and the outline keeps its selection — neither resets to the top (complements §7.4)

### 12.12 Outline scroll-spy highlights the current section while scrolling (preview)
- **Given** the window is in **preview** mode with a document containing at least two headings
- **When** the user scrolls the preview so that section N occupies the top of the viewport
- **Then** the outline highlights the row for section N
- **And** in **split** mode the outline instead follows the editor **caret** — scrolling either pane without moving the caret does not change the highlight (12.14, 7.5)
- **And** activating a different outline entry while the document is stationary navigates correctly and persists the selection as the user-activated heading

### 12.13 Outline scroll-spy does not interfere with user-activated navigation (preview)
- **Given** the window is in **preview** mode and the user clicks an outline entry (section A) to navigate
- **When** the user then manually scrolls the preview to section B without clicking the outline
- **Then** the outline highlights section B (the preview scroll-spy overrides the visual selection)
- **And** switching modes and back re-selects section A (the last user-activated entry, not the spy highlight)

### 12.14 Outline tracks the caret's section in edit-only mode
- **Given** the window is in edit-only mode with a document containing at least two headings
- **When** the user moves the caret into a different section
- **Then** the outline highlights the row for the heading the caret sits in — the working position — tracked live as the caret moves
- **And** a pure scroll of the editor that does not move the caret does not change the highlight: edit and split mode are **caret-driven**, not viewport-driven (only **preview** mode scroll-spies the top-of-viewport section, since it has no caret) — see 7.5

### 12.15 Outline highlight-tracking survives a cross-window tab move
- **Given** a tab has been dragged (or moved via View ▸ Move Tab to New Window) from one window into another
- **When** the user continues working in that tab after the move — scrolling its preview (in preview mode), or moving the caret (in edit/split mode)
- **Then** the outline sidebar's highlighted section keeps tracking the current position — the top-of-viewport section in preview, the caret's heading in edit/split — exactly as it would have if the tab had never moved; the tracking is never left dead against the old window's orphaned scroller

### 12.16 The outline highlights the right section immediately, without a nudge
- **Given** a document positioned at a lower section N (not the first) — scrolled there in preview mode, or with the caret in section N in edit/split mode
- **When** that view is shown with its position preserved but no further scroll or caret move occurs — on entering the mode, switching to the tab, or after moving the tab to another window
- **Then** the outline immediately highlights the row for section N — the top-of-viewport section in preview, the caret's section in edit/split — and the user does not have to nudge the scroll or move the caret first to correct a highlight left stale at the top (guards against the outline spy going stale after a re-home)

### 12.17 Expand all and Collapse all fold the whole outline at once
- **Given** a document with nested headings (three or more levels deep) shown in the outline
- **When** the user clicks the **Collapse all** button in the outline header
- **Then** only the top-level (root) headings remain visible, each collapsed, and they stay collapsed (no automatic re-expansion)
- **And even when** the outline was **scrolled to the bottom** before collapsing (so the list view was showing deep leaf rows), the result is still the root rows at the top — never a stale far-end leaf row stranded alone with no expander (ScrAP-157)
- **And** every nested node is collapsed too — not merely hidden: expanding a single root afterward reveals its direct children *in a collapsed state* (the user descends one level at a time), rather than the whole subtree springing open at once
- **And when** the user clicks **Expand all**
- **Then** every heading is revealed, down to the deepest level
- **And** on a heading-less document (the "No headings" placeholder) both buttons do nothing and never error

### 12.18 The outline scroll-spy follows expand and collapse
- **Given** a document positioned at a lower section (a deep heading at the top of the preview viewport, or the caret in it in edit/split) so the outline highlights that deep heading
- **When** the user collapses so the highlighted heading is no longer visible (Collapse all, or collapsing one of its ancestor nodes)
- **Then** the highlight **rises to the nearest still-visible ancestor** heading — it is never left stale on a now-hidden row — and the document does **not** scroll or move the caret
- **And when** the user re-expands toward that heading (Expand all, or expanding an ancestor node one level at a time)
- **Then** the highlight **descends back toward the exact heading**, reaching it once it is fully visible again — following the visible frontier, still without scrolling the document
- **And** these expand/collapse-driven highlight moves are purely visual: expanding or collapsing a node must **never** scroll the preview or move the caret. Only a genuine outline click / keyboard-select on a *different* heading navigates (ScrAP-89)

### 12.19 Closing a tab mid-navigation aborts the pending scroll gracefully
- **Given** a preview tab with a navigation scroll still in flight (from an outline click, find-in-preview, or a cross-document fragment link)
- **When** the tab or window is closed or dismissed before that scroll has settled
- **Then** the pending scroll aborts gracefully and the tab closes successfully
- **And** a scroll requested on a tab that stays open still lands on its target, including a freshly opened fragment link

### 12.20 The outline rebuilds as the document's headings change
- **Given** the outline is shown for a document
- **When** the document's headings change — one typed, retitled, re-levelled, or deleted in the editor; an undo or redo of such an edit; an external reload that changes them
- **Then** the outline reflects the new headings without the user switching view mode, switching tab, or reloading first — and a selected heading that still exists keeps its selection (POLICY Derived-view CAM row 1, columns A/B)

### 12.21 Tab switch scrolls the outline list to its selected row
- **Given** two tabs with enough headings that the outline list scrolls, tab A positioned so the outline highlights a far section (top-of-viewport in preview, or the caret's heading in edit/split), and the shared outline scroller left showing a different part of the list (e.g. after visiting a shorter-outline tab, or parked at the top)
- **When** the user switches to tab A
- **Then** the outline still highlights the correct section for tab A (12.16) **and** that selected row is scrolled into view in the outline list — the highlight is never left correct-but-off-screen under a stale scroller position from the previous tab
- **And** this reveal runs only after the scroll-spy has settled the selection for the newly active tab (not on every document `value-changed`), so a user who scrolled the outline by hand while reading is not fought mid-scroll
- **And** the reveal does not re-fire outline navigation (spy guards stay quiet — ScrAP-89)

## 13. Preview zoom

> Zoom In / Zoom Out / Reset Zoom scale the preview text on a discrete ladder
> (50 → 75 → 100 → 125 → 150 → 175 → 200 → 250 → 300%). Zoom is **window-scoped**
> (an accessibility accommodation; it does not vary as the user switches tabs in
> one window). Implemented via **two halves that must stay in lock-step**: a CSS
> `font-size: {zoom}em` rule scoped to a per-window class
> (`.scrib-win-<id> textview.scrib-preview`, so a process-global provider cannot
> collide across windows — ScrAP-64) plus explicit pixel-margin scaling
> in `setup_tags` and `preview.rs` (both must scale; neither alone is sufficient).

### 13.1 Zoom In enlarges preview text proportionally
- **Given** the window is in preview or split mode
- **When** the user invokes Zoom In (Ctrl++, Ctrl+=, the View menu item, or the toolbar button)
- **Then** the preview text grows to the next step on the zoom ladder; body text, heading sizes, and paragraph spacing all scale together

### 13.2 Zoom Out shrinks preview text proportionally
- **Given** the window is in preview or split mode
- **When** the user invokes Zoom Out (Ctrl+-, View menu, or toolbar button)
- **Then** the preview text shrinks to the next smaller step on the zoom ladder

### 13.3 Reset Zoom returns to 100%
- **Given** the window is in preview or split mode at a non-default zoom level
- **When** the user invokes Reset Zoom (Ctrl+0, View menu, or toolbar button)
- **Then** the zoom level returns to 1.0 (100%) — body text at the base font size

### 13.4 Zoom controls are bounded and disabled at limits
- **Given** the zoom level is at its minimum (50%) or maximum (300%)
- **When** the user looks at the Zoom Out or Zoom In control respectively
- **Then** that control is disabled — stepping past the ladder boundary is not possible
- **And** Reset Zoom is disabled when the zoom level is already 1.0 (no-op reset)

### 13.5 Zoom controls are disabled in pure-edit mode
- **Given** the window is in edit mode (no preview visible)
- **When** the user looks at the Zoom In, Zoom Out, and Reset Zoom controls (View menu or toolbar)
- **Then** all three are disabled — zoom applies to the preview, which is not visible

### 13.6 Zoom level persists across sessions
- **Given** the user sets the zoom level to a non-default value and closes the application
- **When** the application is relaunched
- **Then** the preview opens at the zoom level from the previous session

### 13.7 Reading position is preserved across zoom changes
- **Given** the user has scrolled partway through a document in preview or split mode
- **When** the user changes the zoom level
- **Then** the viewport stays approximately at the same relative position in the document rather than jumping to the top
- **And** this holds for **repeated, rapid** zoom steps (and a zoom taken immediately after a scroll), not only a single well-spaced step — successive zooms do not accumulate an upward drift toward the top

### 13.8 Zoom is isolated per window (multi-window)
- **Given** two or more windows are open in the same process, each showing a preview
- **When** the user zooms one window (or opens a new window)
- **Then** only that window's preview changes size; every other window keeps its own zoom level — text *and* geometry (headings, spacing, table cells) — with no shift, collapse, or garble in any other window, and opening a later window never resets an earlier window's zoom (ScrAP-64)

### 13.9 A tab moved between windows adopts the destination window's zoom
- **Given** a preview- or split-mode tab in a window at one zoom level (e.g. 100%) and a destination window at a different zoom level (e.g. 300%)
- **When** the tab is moved into the destination window (cross-window drag or View ▸ Move Tab to New Window)
- **Then** the moved preview re-renders fully at the destination window's zoom — **both** the font and the pixel geometry (heading scale, spacing, and especially table-cell layout) scale together, with no residual garble from the source zoom — and the source window is otherwise unaffected (ScrAP-64)

### 13.10 The editor stays responsive after switching out of a zoomed preview
- **Given** the user changed the zoom level while in preview mode
- **When** they switch to edit (or split) mode
- **Then** the editor pane is immediately navigable — the mouse wheel, PageUp/PageDown, and the caret all scroll it normally (the reading position carries over and the scroll range is never left frozen/collapsed)

### 12.11 The outline panel has a labelled header
- **Given** the outline sidebar is shown
- **Then** a fixed caption reading "Outline" sits at the top of the panel and does not scroll with the heading list
- **And** the header carries a close (×) button that hides the sidebar (sharing the same `win.outline` toggle as the toolbar button / View menu / F9)


### 12.23 Fast wheel-scrolling the outline never jumps the list
- **Given** a document with enough headings that the outline list scrolls over many screens (this file is the reference case), positioned somewhere below the top
- **When** the reader scrolls the outline list upward with the mouse wheel **quickly** — fast enough to deliver more than one scroll event per displayed frame
- **Then** the list travels smoothly upward by the same distance per click as a slow scroll, and never jumps — in particular it never snaps back to the end of the list, at any point in the gesture
- **And** the same holds for the annotations list, which is the same kind of pane
- **And** slow scrolling, scrolling downward, dragging the scrollbar, and clicking a row to navigate are all unchanged

### 12.22 The outline includes headings inside collapsed disclosures
- **Given** a document with headings inside a collapsed disclosure block
- **When** the outline is shown
- **Then** those headings are listed in order like any others, and activating one expands its disclosure and navigates to it — the outline models the document, not the viewport
---

## 14. Show Unsafe Images

### 14.1 Remote images are blocked by default
- **Given** a document containing a remote image (e.g. `![badge](https://example.com/badge.png)`)
- **When** the document is rendered and "Show Unsafe Images" is **off** (the default)
- **Then** a broken-image placeholder icon (`image-missing`) is shown where the image would be, and the alt text is **not** shown

### 14.2 Remote images load when the toggle is on
- **Given** a document containing a remote http/https image URL
- **When** "Show Unsafe Images" is toggled **on** (View menu checkbox or toolbar button)
- **Then** the image is fetched from the network and displayed inline in the preview
- **And** this holds on **every supported platform**, and on a host with no desktop VFS layer installed at all — the fetch is the application's own HTTP client, never a URI handed to GIO, whose `http`/`https` support is a separate Linux-desktop daemon (ScrAP-292)
- **And** a fetch that does not produce an image — no network, a non-success status, or a response past the remote-image size limit — leaves the "Could not load image" placeholder **and** records the reason at `warn` naming the URL, so the failure is diagnosable from the log rather than only from a tooltip

### 14.3 Out-of-folder local images are blocked by default
- **Given** a document containing an image whose local path resolves outside the document's folder (absolute path or `..` traversal)
- **When** rendered with "Show Unsafe Images" **off**
- **Then** a broken-image placeholder icon is shown (the containment gate is enforced — TDD §2.7)

### 14.4 Out-of-folder local images load when the toggle is on
- **Given** a document referencing a local image outside the document folder
- **When** "Show Unsafe Images" is **on**
- **Then** the image is loaded from its absolute path and displayed inline

### 14.5 Toggling off immediately shows broken-image placeholders
- **Given** "Show Unsafe Images" is on and the preview shows remote or out-of-folder images
- **When** the user toggles the setting off
- **Then** the preview re-renders immediately and all unsafe images are replaced with the broken-image placeholder icon

### 14.6 The toggle state persists across restarts
- **Given** the user sets "Show Unsafe Images" to on (or off)
- **When** the application is closed and reopened
- **Then** the toggle restores to the last saved state

### 14.7 The toggle appears in the View menu and toolbar
- **Given** the application is open
- **Then** "Show Unsafe Images" appears as a checkable item in the View menu
- **And** the toolbar carries a toggle button (`image-x-generic-symbolic`) for the same action
- **And** both surfaces reflect the same state — checking one checks the other

### 14.8 Non-http/https schemes are always refused
- **Given** a document containing an image with a `file://`, `smb://`, or other non-web scheme
- **When** rendered with "Show Unsafe Images" **on**
- **Then** the image is still refused (broken-image placeholder shown) — only http and https are admitted by the unsafe flag

### 14.9 An image that cannot be shown always leaves a placeholder with a reason
- **Given** an image that cannot be displayed — blocked by policy, its path/URL unresolvable (**not found** at render time), or a resolved file/URL that fails to decode as an image
- **When** the document is rendered (including after toggling "Show Unsafe Images")
- **Then** a broken-image placeholder icon (`image-missing`) is shown in its place — the render **never** silently degrades to the bare alt string
- **And** the placeholder's tooltip states the reason and the offending `src`: "Blocked image (enable Show Unsafe Images to load): …", "Image not found: …", or "Could not load image: …"
- **And** in particular, toggling "Show Unsafe Images" on an image whose file is absent at that instant replaces the "blocked" placeholder with a "not found" placeholder (icon retained) — it does **not** remove the icon and leave only alt text, which read as the toggle having done nothing
- **And** "blocked" is reserved for a reference the toggle could actually admit — a remote URL, or a local path that **exists** and escapes the document folder. A path that is *contained* (or would be) but has **no file behind it at render time** reads as **not found**, with the gate on or off: it is unresolvable, not refused, and the "enable Show Unsafe Images" wording would otherwise invite the reader to lift a safety gate that was never what stopped them. This is what a document and its images arriving together — a checkout, a sync, a generator — looks like when the image loses the race by a frame (the render is a snapshot; the reason it gives must still be true)

### 14.10 Toggle re-renders live editor content in split mode
- **Given** the application is in split (side-by-side) mode and the user has typed a new unsafe image into the editor
- **When** the "Show Unsafe Images" toggle is changed
- **Then** the preview re-renders from the **current editor buffer contents**, not from any stored snapshot — the newly-typed image either loads or shows a placeholder depending on the new toggle state

---

## 15. Tabbed documents

> A window holds one or more tabs, each an independent document.
> §7 covers cross-window drag; this section covers tab lifecycle, per-tab vs.
> per-window scoping, and multi-tab session persistence.

### 15.1 New Document opens a tab in the current window
- Same rubric as 9.1 (a new blank tab opens in the current window and becomes active, without closing the existing window or any of its other tabs) — see 9.1.

### 15.2 Close Tab closes only the active tab
- **Given** a window with two or more tabs
- **When** the user selects File ▸ Close Tab or presses Ctrl+W
- **Then** only the active tab closes, the window stays open showing the next tab, and the other tabs are unaffected
- **And given** a window with exactly one tab
- **When** the user closes that tab
- **Then** the window itself closes (parity with §7.4's window-close prompt)

### 15.3 View ▸ New Window opens a separate window
- **Given** the application is running with one or more windows open
- **When** the user selects View ▸ New Window or presses Ctrl+N
- **Then** a brand-new window opens with one new blank tab, and every existing window is left untouched

### 15.4 View ▸ Move Tab to New Window detaches the active tab
- **Given** a window with two or more tabs
- **When** the user selects View ▸ Move Tab to New Window (or presses Ctrl+Shift+N)
- **Then** the active tab (its content, undo history, and view mode) moves into a brand-new window
- **And given** a window has only ONE tab
- **Then** the command is disabled entirely (menu item, toolbar button, and accelerator all inert) — moving a window's only tab would just leave an identical, empty-of-purpose window behind (see 7.7 for the still-reachable drag/desktop-drop paths, which are unaffected)

### 15.5 Previous Tab / Next Tab cycle within the window
- **Given** a window with two or more tabs — including when the active tab is the window's initial tab, not yet manually switched to
- **When** the user selects View ▸ Next Tab / Previous Tab, or presses Ctrl+PageDown/Up or Ctrl+Tab/Ctrl+Shift+Tab
- **Then** the active tab advances or retreats by one, wrapping around at either end, without affecting any other window

### 15.6 Tabs can be reordered within a window
- **Given** a window with two or more tabs
- **When** the user drags a tab sideways within its own window's tab strip
- **Then** the tab's position in the strip changes accordingly, with no effect on its content or active state

### 15.7 Window title names the active document and counts the others; the tab strip is always visible
- **Given** a window with exactly one tab
- **Then** the window title shows that tab's filename (or "Scribobulate" if untitled) with no parenthetical count, and the tab strip is nonetheless visible (rewritten — a hidden strip is the only grab handle a tab drag has, native reorder and the custom drag source alike, so hiding it at one tab made that tab permanently undraggable; no longer conditioned on tab count or how many other windows are open)
- **And given** a window with two or more tabs
- **Then** the window title shows the **active** tab's filename followed by a count of the window's *other* tabs — "notes.md (+2 documents) — Scribobulate", singular at exactly one other ("notes.md (+1 document) — Scribobulate") — the tab strip is visible, and each tab's own label shows its filename plus a "•" marker while it has unsaved changes
- **And given** the active tab is untitled (no backing file) while the window holds others
- **Then** the app name takes the filename's place and still carries the count ("Scribobulate (+2 documents)"), exactly as a lone untitled tab reads a bare "Scribobulate"
- **And when** the user switches tabs, with no other change to the window
- **Then** the title re-aims at the newly active document and keeps the same count — the title tracks *which* document is on screen, not merely how many there are
- **And** hovering a tab shows a tooltip with the backing file's full absolute path, or "Unsaved" for a tab that has never been saved to a path (updated the moment a Save As adopts a path)

### 15.8 View mode and split arrangement are restored per tab
- **Given** two tabs in the same window, one in Preview and the other in Split (with its own swap/orientation settings)
- **When** the user switches from one tab to the other
- **Then** the view mode and split arrangement shown change to match the newly active tab's own stored settings, not the previously active tab's

### 15.9 Zoom is scoped per window and applies to every tab
- **Given** a window with two or more tabs
- **When** the user changes the zoom level
- **Then** every tab's preview in that window rescales (both the live one and any in the background), and every other window's zoom level is unaffected

### 15.10 Session restore rebuilds every window and every tab
- **Given** two or more windows were open at quit, each with two or more tabs in a mix of view modes, split arrangements, and zoom levels
- **When** the application is relaunched
- **Then** every window reopens with the same size and zoom level, every tab reopens with its own path (or blank, if untitled), view mode, and split arrangement, and each window's previously active tab is the one shown

### 15.11 Closing a window with several dirty tabs prompts sequentially
- **Given** a window with three tabs, two of them dirty
- **When** the user closes the window (or quits the application)
- **Then** a Save/Discard/Cancel prompt appears for the first dirty tab, then — once resolved — for the second, and choosing Cancel at any point aborts the whole close, leaving all three tabs exactly as they were

### 15.12 Find query and match position are preserved per tab
- **Given** the user has searched for a term in one tab
- **When** they switch to a different tab and back
- **Then** the find bar shows the same query and match state it had when they left that tab, not the other tab's query
- **And** switching tabs while a find query is active in the tab being left — to a tab with a different (including empty) query of its own — never crashes or aborts the process, regardless of how many times it's repeated (a `RefCell` double-borrow that produced a non-catchable process abort — ScrAP-53)

### 15.13 A background tab's own file changes are tracked correctly
- **Given** a window with two tabs, each backed by a different file on disk, with the second tab inactive (in the background)
- **When** the second tab's *own* backing file changes on disk
- **Then** the conflict/reload handling (§3, §5) is evaluated against the second tab's own path, source, and dirty state — not the currently active tab's — and any resulting toast or reload applies to the second tab, not the one on screen

### 15.14 Per-tab toggles and path-dependent commands follow the active tab
- **Given** two tabs in the same window with different "Show Unsafe Images" settings, and only one of them backed by a file
- **When** the user switches between them
- **Then** the Show Unsafe Images toggle immediately reflects the newly active tab's own setting, and Copy Full Path / Reload immediately enable or disable to match whether the newly active tab has a backing file

### 15.15 One invocation opens at most one new window, all its files as tabs
- **Given** the application is launched (or handed a request via D-Bus) with two or more Markdown file paths at once — whether from a shell glob or explicit separate arguments
- **When** it starts processing them
- **Then** every file that isn't already open elsewhere lands as a tab of a single new window (or the current blank window/tab, per 1.5/1.6) — that one invocation never opens more than one new window
- **And given** one or more of the specified files is already open in a tab of some other, already-open window
- **When** the same invocation processes them
- **Then** that file is skipped from the new window — its existing tab is focused instead (15.16) — while every other, not-already-open file still lands together in the one new window

### 15.16 An already-open file is focused, wherever it is
- **Given** a file is already open in some tab of some window, whether or not that tab is the active one
- **When** the same file is opened again (via the interface, the command line, or a D-Bus-forwarded launch)
- **Then** the window containing that tab is focused and that tab becomes active, rather than opening a duplicate

### 15.17 Moving or closing a tab leaves the source window showing a surviving tab
- **Given** a window with two or more tabs — **including the specific case where the active tab is the very first tab the window was opened with and the user has not switched away from it and back**
- **When** the user moves that active tab to a new window (View ▸ Move Tab to New Window or dragging it out) or closes it (Ctrl+W / the tab's ×) while other tabs remain
- **Then** the source window switches to a remaining tab and shows that tab's document content **and** its outline — never a blank content pane with the moved/closed tab's outline still displayed (a "phantom tab")

### 15.18 The Documents list (View menu and toolbar combo) is a live, per-window list of that window's tabs
- **Given** a window with several open tabs — and possibly other windows open, each with their own tabs
- **When** the user opens View ▸ Documents
- **Then** it lists exactly THAT window's open tabs, in tab-strip order (each item switches to its tab when chosen; an unsaved tab reads "Untitled"; a filename's underscores are shown, not swallowed), with the currently-active tab marked
- **And** the list stays correct as the tab set changes — opening a file, New Document, closing a tab, moving/popping a tab between windows, reordering, and renaming via Save As all update it — while every other window's Documents shows only ITS OWN tabs, never this window's
- **And** the toolbar's **Documents combo box** is a *second surface of this same list*, not a parallel one: it presents the same items in the same order, marks the same active tab, and is driven by the same single action — so opening it, choosing from it, or watching it update is indistinguishable from the menu, and the two can never disagree
- **And** the combo's button **label shows the active document's filename** (ellipsized if long; "Untitled" for an unsaved tab), tracking every tab switch, close, move, and Save-As rename — a switch made from the strip or the menu retargets the combo label with no separate action, because the label reflects the one active tab rather than mirroring a surface

---

### 15.19 The tab context menu's per-tab commands act on the tab you right-clicked
- **Given** two open tabs, both backed by real files, tab A active and tab B in the background
- **When** the reader right-clicks tab B and chooses **Reload**
- **Then** tab B becomes the active tab **and** tab B's content reloads from disk, while tab A's content is unchanged
- **And When** the reader right-clicks tab B and chooses **Copy Full Path**
- **Then** tab B becomes active **and** the clipboard holds **tab B's** path, not tab A's
- **And When**, with tab B dirty, the reader right-clicks it and chooses **Save**
- **Then** tab B becomes active **and** tab B's content is written to disk, while tab A is untouched; choosing **Save As** on tab B likewise focuses it first, so the chooser's suggested name/location and the write both concern tab B
- These commands are window-scoped actions that always target the *active* tab, so the menu must focus the clicked tab **before** driving them. Without this, a right-click on a background tab silently acts on a different document — and Reload/Save prompt on a dirty buffer, so focusing first is also what lets the reader see the document the prompt is about

### 15.20 The tab context menu's sensitivity reflects the clicked tab, not the active one
- **Given** a right-clicked tab with no backing file (untitled), while the *currently active* tab does have one
- **Then** both Copy Full Path and Reload appear **insensitive** in that menu
- **And given** a right-clicked tab that is clean with its backing file present, while the *currently active* tab is dirty
- **Then** Save appears **insensitive** in that menu (Save As stays sensitive regardless — it carries no dirty/backing gate on any surface)
- The gate is derived from the clicked tab's own `has_path()` / dirty state, never from the action's `is_enabled()` — that would report the **active** tab's state on a menu opened over a different tab. (Contrast the Move-to-New-Window item, which *correctly* reads `is_enabled()`, because its precondition — more than one tab — is genuinely window-scoped and identical for every tab. The distinction is per-tab vs window-scoped, not a house style.)

### 15.21 Tabs present as tabs, and the active one stands out
- **Given** a window with two or more tabs, under any desktop GTK theme, light or dark
- **Then** each inactive tab presents visually as a distinct tab handle — a bounded surface, set apart from the strip it sits on and from its neighbours — rather than a bare label floating on the strip
- **And** the active tab is distinguishable from the inactive tabs at a glance, by appearance alone and without reading any label
- The *means* — fills, borders, edge treatments, which surface is lighter than which — are deliberately left unspecified. Theming is fluid and derived from the desktop theme, so naming a mechanism here would make this rubric fail on every legitimate restyle. What must hold is the distinction, in both theme variants.

### 15.22 A tab whose backing file was deleted is badged and guarded like a dirty tab
- **Given** an open, saved document whose buffer is clean (no unsaved edits)
- **When** its backing file is deleted on disk (a genuine external deletion, not the app's own crash-safe self-rename on save)
- **Then** its tab carries a leading **yellow ⚠** warning marker — the complement to the ⟳ reload badge (15.13) — shown whether the tab is active or in the background, and combinable with the dirty "•"; and the persistent "File deleted on disk — save to restore it" notice appears (this last is the existing §3.4 behaviour)
- **And** the ⚠ clears the moment the file exists again — a Save or Save As re-creates it, a Reload reads it back, or an external program re-creates it — the tab returning to its plain label
- **And** closing that tab (its ×, Ctrl+W, Close Other Tabs) or its window prompts **Save / Discard / Cancel** first, exactly as an unsaved tab does — because the buffer holds the document's only remaining copy and closing without a Save would lose it — even though the buffer is byte-for-byte "clean" against a baseline whose file is gone; choosing Save re-creates the file and lets the close proceed
- The badge and the close guard read the same per-tab "backing missing" state that already makes Save enabled over a deleted file (§ above, `save_enabled(dirty, backing_missing)`); the label formula and the close-prompt predicate are unit-tested in `winstate::decisions` and `winstate::tab`

## 16. Keyboard-shortcuts help & status surfaces

> These rubrics cover the discoverability surfaces: the keyboard-shortcuts
> window, the accelerator-bearing tooltips, and
> the status-bar reload announcement. The persistent unsaved-changes indicator is
> §4.4; the transient *visual* reload toast is §5.4 — 16.3 is the complementary
> *status-bar* half of that same event.

### 16.1 A keyboard-shortcuts window is discoverable
- **Given** any open window
- **When** the user presses F1 (or Ctrl+?), or chooses Help ▸ Keyboard Shortcuts
- **Then** a keyboard-shortcuts window opens, listing the application's shortcuts

### 16.2 The shortcuts window is accurate and complete
- **Given** the keyboard-shortcuts window is open
- **When** the user reads it
- **Then** every command that has a keyboard shortcut is listed, grouped by area (File, Edit, Format, View, Windows & Tabs), each showing its command name and its actual, platform-correct key combination — and a shortcut shown there really triggers that command

### 16.3 A clean auto-reload is announced in the status bar
- **Given** the editor has no unsaved edits and auto-reload is enabled
- **When** the file is changed on disk by another process and reloads
- **Then** in addition to the transient visual toast (§5.4), a brief "File reloaded" message appears in the status bar and clears itself after a few seconds, leaving the persistent status (§4.4) intact underneath

### 16.4 Toolbar controls have tooltips that include the shortcut
- **Given** a toolbar button for a command that has a keyboard shortcut
- **When** the user hovers the pointer over it
- **Then** a tooltip appears showing the command's name and, in parentheses, its shortcut (e.g. "Open (Ctrl+O)"); a command with no shortcut shows just its name

### 16.5 Status messages are announced to assistive technology
- **Given** a screen reader is active
- **When** the status bar text changes (unsaved-changes, "File reloaded")
- **Then** the change is announced politely (the status region carries an accessible "status" role) without stealing keyboard focus

### 16.7 Every control assistive technology can reach has a name
- **Given** a screen reader is active, and a window's toolbar, find bar, sidebars and tab strip
- **When** the user moves focus through the controls
- **Then** every icon-only button, dropdown and label-less text field announces a name describing the command it runs — not silence, and not the file path or availability note that a tooltip may separately carry
- **And** the same holds for a control in a transient **dialog** — the Go To Line / Insert Link / Insert Image / Insert Table prompt fields — which a window-scoped audit does not reach; a field sitting beside a visible `GtkLabel` still announces its own name rather than relying on the reader to associate the two
- **And** a control whose visible label shows a *value* rather than its purpose (the reading-theme and open-documents dropdowns, which display the active theme and document) announces its purpose, so the name does not change as the value does
- **And** a shortcut is announced as a shortcut rather than as part of the control's name

### 16.8 A timed status notice clears from the window that showed it
- **Given** a transient status-bar notice is up in a window (a "File reloaded"/"File saved" announcement, a link-navigation error, or "File deleted on disk"), and it clears itself after a few seconds
- **When** the tab that raised it is dragged to another window, or closed, before the notice's time is up
- **Then** the notice still clears from the window it appeared in, leaving that window's persistent status (§4.4) intact underneath — it is never left on screen permanently, and it never appears in the window the tab moved to

### 16.9 A menu item's shortcut hint is the key that command is bound to
*(Drafted for a review finding that measurement refuted, and aimed at what survived. The finding was that two View-menu items set no `accel` attribute and therefore showed no key. They showed the right key: GTK derives the hint from the registered accelerator when the model declares none, MEASURED on two release binaries driven identically (GTK 4.6.9/X11) and read off the live macOS system menu bar by the `mac` seat through the Accessibility API. The attribute was then removed everywhere, since it could only restate the binding or silently contradict it — a third binary proved an attribute WINS over the binding. The reachable defect is therefore a command whose accelerator is never registered, which is the one state that yields a hintless item.)*
- **Given** a command that has a keyboard shortcut and a menu-bar item
- **When** the user opens the menu containing it
- **Then** that item shows the command's shortcut beside its label, in the platform's own spelling (Cmd on macOS, Ctrl elsewhere), and the key shown is the key that command is actually bound to — agreeing with the shortcuts window (§16.2) and the toolbar tooltip (§16.4), the other two discoverability surfaces
- **And** a command that has no shortcut shows no hint, so a blank is information rather than an omission

### 16.6 Online Markdown reference is reachable from Help
- **Given** any open window
- **When** the user chooses Help ▸ Markdown Reference
- **Then** the system default browser opens a Markdown syntax reference (the CommonMark reference the app's renderer follows), and the application keeps running normally

## 17. Annotation & review (CriticMarkup)

### 17.1 A highlighted claim renders with a highlight, not its markup
- **Given** a document containing `{==the earth is flat==}{>>citation needed<<}`
- **When** it is shown in the preview
- **Then** the words "the earth is flat" appear with a coloured highlight background, and neither the `{==…==}`/`{>>…<<}` delimiters nor the comment text appear in the rendered text

### 17.2 Suggested-edit markup renders inert, without braces
- **Given** a document containing `{++inserted++}`, `{--deleted--}`, and `{~~old~>new~~}`
- **When** it is shown in the preview
- **Then** the visible text reads "inserted", "deleted", and "new" respectively, with no CriticMarkup delimiters shown (v1 applies no distinct styling to these kinds)

### 17.3 A comment is reachable from a margin marker
- **Given** a rendered document with a highlighted claim that carries a comment
- **When** the user clicks the marker in the preview's right margin beside that line
- **Then** a popover opens showing the claim and its comment text

### 17.4 Several annotations on one line share one marker with a count
- **Given** a single visual line carrying two or more commented annotations
- **When** the preview is shown
- **Then** that line has exactly one margin marker displaying the count (e.g. "2"), and clicking it lists every annotation on the line, each with its own comment

### 17.5 A reader can annotate a selected claim, saved into the file
- **Given** the document is shown in the preview and the reader selects a claim
- **When** they choose Annotate, type a comment, and Save
- **Then** the selected text becomes highlighted with a margin marker, and the underlying Markdown file gains `{==<claim>==}{>>comment<<}` around exactly the selected span (the buffer is marked unsaved until saved to disk)

### 17.6 A selection spanning blocks becomes a point comment
- **Given** the reader's preview selection crosses a blank-line block boundary
- **When** they annotate it
- **Then** instead of a highlight, a point comment `{>>comment<<}` is inserted at the end of the selection, and its marker appears on that line

### 17.7 An existing comment can be edited in place
- **Given** an annotation's popover is open with Edit available
- **When** the user edits the comment text and saves
- **Then** the comment body in the file is replaced with the new text and nothing else in the document changes

### 17.8 An annotation can be removed
- **Given** an annotation's popover is open with Remove available
- **When** the user removes it
- **Then** a highlight+comment leaves the claim text in place but strips its CriticMarkup; a point comment is deleted entirely — and the marker disappears on re-render

### 17.9 Annotations persist across reload and reopen
- **Given** a file that was annotated and saved
- **When** the file is reloaded or reopened
- **Then** every annotation is shown again exactly as stored, because it lives in the file as CriticMarkup — no separate sidecar

### 17.10 Copying annotated text yields clean prose
- **Given** a rendered document containing annotations
- **When** the user selects across annotated text and copies
- **Then** the clipboard holds the clean Markdown prose (the highlighted words, not the `{==…==}{>>…<<}` markup)

### 17.11 Split-mode scroll sync stays aligned with annotations present
- **Given** a split view of an annotated document
- **When** the user scrolls either pane
- **Then** the other pane tracks to the corresponding position without drift introduced by the removed CriticMarkup (an un-annotated document syncs exactly as before)

### 17.12 Malformed or block-crossing CriticMarkup is shown literally, never lost
- **Given** a document with an unclosed `{==` or a `{==…==}` whose close crosses a blank line
- **When** it is shown in the preview
- **Then** the text is rendered literally (delimiters visible) rather than being swallowed or treated as an annotation

### 17.13 Annotating over an existing annotation extends it, never nests
- **Given** a document with an existing highlight annotation
- **When** the reader selects text that overlaps that annotation (a part of it, or extending past it) and annotates it
- **Then** the existing highlight is replaced by a SINGLE highlight spanning the union of the old span and the selection, carrying the new comment — never a nested or overlapping pair of annotations, and the on-disk CriticMarkup stays well-formed

### 17.14 Several annotations can be created in one sitting
- **Given** the reader has just created one annotation in the preview (which live-refreshes the view)
- **When** they select different text and annotate again
- **Then** the second annotation is created normally — the Annotate pop-up appears on the new selection and the annotation is added, repeatably, without reopening or reloading the document
- **And** the second annotation's highlight lands over the SAME words in the immediate live (in-place) refresh as it does after a full reload — it must not drift by the byte length of the earlier annotation's stripped CriticMarkup delimiters. The live highlight range is measured in cleaned space (the typed `CleanedByteOffset`), so an earlier annotation's delimiters cannot shift it; measuring in original space would.

### 17.15 A comment marker signals that it is clickable
- **Given** the preview shows a comment marker in the right margin
- **When** the pointer hovers over that marker
- **Then** the cursor changes to a pointer (hand) — the same affordance a link shows — making it discoverable that the marker can be clicked

### 17.16 Changing or clearing the selection dismisses the annotate overlay
- **Given** the annotate overlay is showing for a selection — either its "Annotate" button or its open comment entry
- **When** the reader changes the selection to different text, or clears it (clicks elsewhere)
- **Then** the overlay (including any in-progress, unsaved comment entry) is dismissed; if a new selection was made, a fresh overlay appears for it, and if the selection cleared, nothing remains

### 17.17 The comment entry is wide enough for its hint and a real comment
- **Given** the reader opens the comment entry from the annotate overlay
- **When** the entry appears
- **Then** it is wide enough that the "Add a comment…" placeholder is fully visible (not truncated) and there is room to type a real comment beside the Save control

### 17.18 Annotating across inline formatting keeps the constructs whole
- **Given** a preview selection that spans inline formatting — inline code, bold/italic, a link, or one of the app's own tight constructs (`==highlight==`, `~~strikethrough~~`, `^sup^`, `~sub~`) — possibly across several wrapped lines
- **When** the reader annotates it
- **Then** the highlight covers the whole selection contiguously and every inline construct still renders intact (the `{==…==}` wraps each touched construct WHOLE — it never splits a `` ` ``, `**`, `*`, `[]()`, `==`, `~~`, `^` or `~` delimiter), producing well-formed CriticMarkup
- **And** the amber wash covers **exactly the annotated claim** — no more — even where the rendered text is shorter than its source because the app stripped a construct's markers (`==mark==` → `mark`): annotating a plain word on such a line washes only that word, and annotating the construct washes only its content. This holds identically in body text and in a table cell (one shared mapper, not two)

### 17.19 A single plain word in a rich paragraph annotates only that word
- **Given** a preview paragraph that also holds soft-wrapped lines, inline code, and bold, and is followed by more content (a second paragraph or a code block)
- **When** the reader selects a single plain word inside it and annotates
- **Then** only that word is highlighted — the annotation never runs to the end of the paragraph or spills into the following block

### 17.20 Typing a comment keeps focus in the entry
- **Given** the reader has opened the comment entry from the annotate overlay and it holds keyboard focus
- **When** the reader types the comment, including the very first character
- **Then** every character lands in the entry and focus stays there — no menu (or other widget) steals focus mid-typing

### 17.21 Committing an annotation leaves the UI consistent
- **Given** the reader commits an annotation action from a popover button — Save a new comment, Save an edited comment, or Remove
- **When** the preview re-renders to reflect the change
- **Then** the marker/highlight updates cleanly, the widget tree stays consistent (no active-state corruption), and the app remains responsive for the next action

### 17.22 An annotation over inline code (or any span with its own background) stays visible
- **Given** the reader annotates a claim that is, or overlaps, inline `code` (which draws its own background) — or any styled span that fills its background
- **When** the highlight is applied and rendered
- **Then** the amber highlight remains clearly visible over that span (it is never painted over / hidden by the span's own background) and its margin marker appears

### 17.23 Annotate is a first-class command reachable from several surfaces, gated on a selection
- **Given** a document open in the editor or preview
- **When** the user consults the ways to annotate — the keyboard accelerator (Ctrl+Alt+M), the Edit menu's Annotate item, the Annotate toolbar button, and the right-click context menu's Annotate item
- **Then** all four drive the one `win.annotate` command and are enabled exactly when the active pane has a non-empty selection (disabled otherwise), never diverging from one another

### 17.24 The editor pane can annotate its own selection into the source
- **Given** the editor pane (edit or split mode) with a span of source text selected
- **When** the user invokes Annotate, types a comment, and Saves
- **Then** an in-surface comment card appears at the selection, and on Save the selected span is wrapped as `{==…==}{>>comment<<}` (or a point comment for a blank-line-crossing selection) directly in the source buffer, marking it unsaved — the same normal Save path as any edit

### 17.25 The comment card is placed at the selection at any scroll offset
- **Given** a selection anywhere in a scrolled view (near the top, middle, or bottom of the viewport), in either pane
- **When** the comment card is raised
- **Then** it appears centered on the selection and adjacent to it (just above, or below when there is no room above) — never pinned to a corner or drifting away from the selection on repeated use

### 17.26 Adding or removing an annotation does not disturb the reading position
- **Given** the preview is scrolled to a position partway through a document
- **When** an annotation is added or removed
- **Then** the highlight and margin marker update in place while the preview stays exactly where it was — the pane does not jump, scroll, or visibly repaint the whole document

### 17.27 Reloading after an unsaved annotation change reverts cleanly to the on-disk state
- **Given** a document (in any view mode, including split) whose annotations have been changed in memory — one added, edited, or removed — but not saved to disk
- **When** the user reloads the document from disk (the toolbar Reload button or File ▸ Reload)
- **Then** the document reloads cleanly and its highlights and margin markers revert to exactly the on-disk state — showing whatever annotations the saved file holds (which may be some, or none) and none of the discarded in-memory ones (a clean reload; not crashing is implied)

### 17.28 Annotating a table-cell selection creates CriticMarkup in the source
- **Given** a document open in preview (or split) containing a table with selectable cell text
- **When** the user selects text inside a table cell
- **Then** the 💬 Annotate create pop-up appears over the cell selection on its own AND the `win.annotate` command is enabled on every surface — full parity with a body-text selection (§17.5, §17.23) — even though a cell selection is a selection island that fires no buffer signal (it is tracked via the primary clipboard's `changed`, ScrAP-110)
- **And when** the user invokes Annotate, types a comment, and Saves
- **Then** the selected cell text is wrapped as `{==…==}{>>comment<<}` in the source (bold/code constructs included whole), the title shows unsaved, and the cell re-renders with an amber highlight over the claim **immediately, without any view-mode switch**, in every view mode (preview / edit / split) — the same undoable source splice as a body annotation. (The in-place tag refresh cannot repaint a cell highlight, which is Pango markup on the cell `GtkLabel` rather than a buffer tag, so a cell annotation forces a full re-render; a body annotation still refreshes in place.)

### 17.29 A cell annotation is undoable from preview-only and survives save→reload
- **Given** a cell annotation just created from preview-only mode
- **When** the user undoes (Ctrl+Z), then redoes, then saves and reloads
- **Then** Undo removes the CriticMarkup and the cell highlight; Redo restores both; after Save→Reload the annotation reappears from the file exactly as stored

### 17.30 A cell annotation's margin marker sits beside the correct row
- **Given** a multi-row table with an annotated claim in a row that is not the top row
- **When** the preview shows the table (preview-only or split)
- **Then** the right-margin comment-marker chip is vertically aligned with the annotated cell's row — not pinned to the table's top edge (cell-marker pairing)

### 17.31 A cell marker opens its comment popover, and the create overlay dismisses with the cell selection
- **Given** a table containing an annotated cell (a margin marker chip) and, optionally, a live cell-text selection showing the 💬 create pop-up
- **When** the user clicks the cell's margin marker chip
- **Then** that marker's comment popover opens (claim + comment + Edit/Remove) — never the create card — and any showing create pop-up is dismissed rather than left stacked over the same table
- **And when** the user clears the cell selection (clicks an empty area of the preview, which does not itself clear a `GtkLabel`'s sticky selection)
- **Then** the create pop-up dismisses and the cell selection clears — the overlay accounts for cell-selection state (driven off the primary-clipboard `changed`), not only buffer-selection changes which never fire for a cell selection

### 17.32 A cell annotation's margin marker renders immediately and stably
- **Given** a table taller than the viewport with an annotation in a lower cell row, in preview-only or split
- **When** the document loads, and when the view is scrolled (wheel / keyboard / scrollbar) or clicked — with no mouse motion afterward
- **Then** the marker chip is drawn beside its cell row from the first paint (no mouse-move required), stays drawn and tracks the row across the scroll, and does not flicker on click
- **Because** the cell's scroll-invariant buffer-Y is measured once (when the anchored cell is allocated) and cached on the marker, not re-read from live geometry every snapshot — a per-frame read returns `None` mid-scroll and dropped the chip until a motion event re-snapshotted (Document Rendering CAM row 1: immediate + stable; ScrAP-22 / ScrAP-109)

### 17.33 An editor-pane annotation never splits an inline construct
- **Given** the editor pane with a selection that starts or ends *inside* an inline construct — e.g. `bol` within `**bold**`, part of a `` `code span` ``, the text of a `[link](url)`, **or any of the tight constructs this app tokenises itself: `==highlight==`, `~~strikethrough~~`, `^superscript^`, `~subscript~`**
- **When** the user invokes Annotate, types a comment, and Saves
- **Then** the CriticMarkup wraps the WHOLE construct (`{==**bold**==}{>>…<<}`, `{===mark==…==}` never landing inside the `==` pair), never between a delimiter and its partner — for **every** construct kind, whichever parser owns it; a selection touching no construct stays character-precise (prose where `==`/`^`/`~` are literal, like `a == b` or `2^10`, is never widened), one already covering a construct exactly is not widened, and a selection running from inside one construct into another swallows both
- **Because** the editor buffer is raw source with no copymap to balance against, so the raw byte span is balanced by `copymap::balance_source_span` — the editor-side sibling of the preview's `wrap_span` — before wrapping; the preview path already balanced via `wrap_span`. The balanced span is also what the blank-line/point-comment test runs on, since widening can cross a block boundary

---

### 17.34 Marker navigation lands on its target, and never fails silently
- **Given** a document whose next annotation is below the fold
- **When** the reader activates `win.next-annotation` (or clicks a marker chip)
- **Then** the document scrolls to that annotation and its comment popover opens — **and** the document is left scrolled to it even in the degenerate case where the popover cannot open. The give-up is visible, never a silent no-op: a reader who pressed a key must see that something happened

### 17.35 An expired navigation does not throw a late popover
- **Given** a marker navigation whose wall-clock budget has already elapsed
- **When** the target's chip is painted anyway
- **Then** no popover opens — an expired navigation must not surprise the reader with a popover pointed at a rect they have since scrolled away from

### 17.36 A deferred validation scroll cannot undo a completed navigation
- **Given** a marker popover that has just opened after a scroll
- **When** a deferred validation scroll re-targets the vadjustment
- **Then** the view holds the **post-scroll** position (the annotation), never the pre-scroll one. There is one animation slot per `GtkAdjustment` and `set_value` beats an in-flight animation, so a guard saving the wrong value silently reverts the navigation while every other assertion still passes

### 17.37 Every bounded wait in the navigation path is wall-clock, not frame-counted
- **Given** any bounded wait in the annotation navigation path
- **Then** its bound is a wall-clock duration, never a frame count — the behaviour must not depend on the display's refresh rate, which the app does not control

### 17.38 A selection keystroke in the comment card never eats the typed comment
- **Given** the annotation comment card is open with text typed into it (either surface)
- **When** the reader presses any selection keystroke (Ctrl+A, Shift+Home, Shift+End, Shift+Arrow)
- **Then** the card stays open, the typed comment survives, and the selection applies to the **card's entry** — not to the document

### 17.39 The card dismisses on a genuinely different selection, and on Escape
- **Given** the comment card is open
- **When** the reader selects different, non-empty text in the document
- **Then** the card dismisses — the anchor it was raised over is superseded
- **And When** the reader presses Escape instead
- **Then** the card dismisses and focus returns to the pane
- **But** a selection that merely becomes *empty* never dismisses it: that is the app's own clipboard bookkeeping, not the reader choosing something else

### 17.40 Both annotation cards show what they are about to destroy
- **Given** a document containing `the earth is {==flat==}{>>citation needed<<}`, with `flat` selected in **either** pane
- **When** the Annotate card is raised
- **Then** the comment entry is pre-filled with `citation needed`, the caret rests at the **end** with nothing selected, and typing **appends** rather than replaces
- **And** this holds identically on the editor and preview surfaces — the symmetry is the point; each pane resolves the selection differently but the reader must not be able to tell

### 17.41 Committing a merge unedited destroys nothing
- **Given** a selection spanning `{==alpha==}{>>note A<<} middle {==omega==}{>>note B<<}`
- **When** the Annotate card is raised
- **Then** the entry shows `note A | note B` — every intersecting comment, ` | `-joined, in source order
- **And When** it is committed without editing
- **Then** the single merged annotation carries `note A | note B`: both comments survive, and the joined text re-extracts as ONE comment with the ` | ` intact

### 17.42 A card that merges nothing offers nothing
- **Given** a selection over unannotated text, or one crossing a blank line (which becomes a point comment)
- **When** the Annotate card is raised
- **Then** the comment entry is empty — the pre-population never invents a comment where none is at risk

### 17.43 Enter commits on every annotation comment entry
- **Given** any annotation comment entry — the editor Create card, the preview Create card, or the marker-chip Edit popover
- **When** the reader presses Enter
- **Then** the comment is committed, identically to clicking Save. All three surfaces route through one shared entry so the two commit paths cannot diverge — a test that only clicks Save passes while Enter is inert

### 17.44 A multi-block annotation over an existing annotation lands cleanly, never silently
- **Given** a document `First para.\n\nThe earth is {==flat==}{>>cite<<} today.` with a preview (or editor) selection that spans **more than one block** AND whose end falls **part-way through** the existing `{==flat==}` highlight
- **When** the reader commits a comment
- **Then** a cross-block selection resolves to a point comment (D5), and its anchor is snapped **outside** the overlapped construct: the new `{>>…<<}` attaches immediately after the whole `{==flat==}{>>cite<<}`, as its own standalone comment
- **And** the source now extracts as **two** distinct, well-formed annotations — the existing highlight+comment intact, plus the new point comment — so the commit is never a silent no-op and never corrupts the construct (splicing at the raw D5 anchor would nest `{>>…<<}` inside `{==…==}`, and the scanner would swallow the comment into the claim text)
- **And** the cross-block path deliberately does **not** extend or pre-populate from the overlapped annotation (that is the intra-block highlight path's contract only — §17.13, §17.42)

### 17.45 An annotation card acts on its own annotation after the document moves
- **Given** an open annotation card for `{==beta==}{>>note<<}` in `alpha {==beta==}{>>note<<} gamma`
- **And given** the document then changes underneath it — the reader types earlier in the document, an undo lands, or the split-mode live re-render re-scans the source — so every byte offset the card captured when it was built now addresses different text
- **When** the reader clicks **Remove** (or commits an **Edit**) on that still-open card
- **Then** the mutation applies to **its own** annotation at its current position, leaving every other character of the document untouched — because the card re-locates its construct by the text it captured, never by the offsets alone
- **And** where the document contains several identical annotations, the one nearest the card's original position is chosen, so a shifted document resolves to the same annotation rather than a different copy of it

### 17.46 An annotation card whose annotation is gone does nothing at all
- **Given** an open annotation card whose construct has since been deleted from the document — by hand, by an undo, or by a reload
- **When** the reader clicks **Remove** or commits an **Edit**
- **Then** the document is left **exactly** as it was: no text is removed, no markup is spliced, and the application does not terminate
- **And** this holds when the document is now *shorter* than the offsets the card captured, which is the case that previously aborted the process rather than failing (a panic in a GTK signal handler takes unsaved work with it)

### 17.47 An open annotation card follows the document it annotates
- **Given** an open annotation card pointing at its margin chip
- **When** the reader scrolls the preview while it is open
- **Then** the card moves with its chip, staying beside the line it is about — it never stays where it was and ends up pointing at unrelated text

### 17.48 An annotation card released by the viewport comes back with its chip
- **Given** an open annotation card
- **When** the reader scrolls until its chip leaves the viewport
- **Then** the card disappears rather than sitting pinned over unrelated text
- **And when** the reader scrolls back so the chip is visible again, the same card reappears beside it, still showing the same annotation — scrolling past an annotation and back is not a dismissal

### 17.49 Clicking outside an annotation card dismisses it
- **Given** an open annotation card
- **When** the reader clicks anywhere outside it — on the document, the toolbar, the editor pane, or the sidebar
- **Then** the card dismisses, and the click still reaches whatever it was aimed at
- **And** clicking the card's own Edit, Remove, Save or Cancel controls activates that control instead, never dismissing the card out from under the click
- **And** clicking a margin chip while a card is open leaves the reader with the card for that chip, rather than dismissing and losing an in-progress edit

### 17.50 Activating another annotation moves the card to it
- **Given** an open annotation card, and a second annotation elsewhere in the document
- **When** the reader activates that second annotation — from the annotations viewer, or with the next-annotation command — so the preview scrolls to it
- **Then** the card shows the second annotation **beside its own chip**, at the position the document ended up at, never at the place the first chip occupied or where the second chip sat before the scroll

### 17.51 An annotation card whose annotation vanishes stops pointing at anything
- **Given** an open annotation card
- **When** the annotation it is showing disappears from the document — removed, undone, or dropped by a reload
- **Then** the card goes away rather than re-anchoring itself to whatever annotation now occupies that place in the document

### 17.52 An annotation card can be closed from its own corner
- **Given** an open annotation card
- **When** the reader clicks the **×** in its top-right corner
- **Then** the card closes, exactly as Escape or a click outside it would
- **And** opening a card still puts the keyboard on its first action (Edit), never on the × — so a reader who opens a card and presses Space acts on the annotation rather than dismissing it

### 17.53 Cancel abandons the edit, not the annotation
- **Given** an annotation card whose Edit has been activated, with the comment part-rewritten
- **When** the reader clicks **Cancel**
- **Then** the card returns to its read state — claim, stored comment, Edit and Remove — and stays open on the same annotation; the reader does not lose their place over an edit they backed out of
- **And** the document is unchanged, and pressing Edit again offers the **stored** comment, not the abandoned draft

### 17.54 The annotation walk advances, and it goes back
- **Given** a document holding several comment-bearing annotations
- **When** the reader invokes **Next Annotation** repeatedly (Ctrl+Alt+N, or Edit ▸ Next Annotation)
- **Then** each invocation moves on to the next annotation in document order and opens its comment — never re-opening the one already showing — wrapping from the last back to the first
- **And when** they invoke **Previous Annotation** (Ctrl+Alt+P, or Edit ▸ Previous Annotation)
- **Then** the walk goes the other way, wrapping from the first back to the last, so overshooting an annotation costs one keypress rather than a lap of the whole document
- **And** the walk is measured from where the reader *is*: clicking or navigating into the document between presses resumes the walk from there, rather than from wherever it last was
- **And** both are ordinary commands — one action each, an Edit-menu item each, and both listed in the Keyboard Shortcuts help window

### 17.55 The walk reaches annotations in every view mode
- **Given** a document with annotations, in preview, split, or pure-edit mode
- **When** the reader walks to an annotation
- **Then** preview and split scroll to it and open its comment card (split additionally placing the editor caret on it), and pure-edit moves the editor caret to the annotation's source position — each mode presenting it exactly as it presents an activated annotations-viewer row, because there is one navigator and not one per surface
- **And** in pure edit the caret lands on the right character past multi-byte text (as §20.14 requires of the viewer)
- **And** a document with no annotations at all does nothing, quietly: the command stays enabled, because a greyed-out Next Annotation is indistinguishable from a broken one

## 18. Preview reading themes

The preview pane's purpose is reading. A *reading theme* restyles everything the
preview draws — colour, typography, and decoration spacing — from data, leaving the
rest of the app on the desktop GTK theme. `System` reproduces the desktop-derived
appearance that predates the feature; `Sepia` is the book-like reading theme.

### 18.1 A reading theme can be chosen
- **Given** the app as installed with its shipped themes
- **When** the user opens the theme chooser (menu or toolbar)
- **Then** at least "System" and "Sepia" are offered, with the active one indicated — and both surfaces always show the same choice
- **And** a theme's **name and picker symbol are its own**: a theme that states no symbol is offered by name alone on *both* surfaces, never wearing the base theme's glyph on one of them

### 18.2 System renders exactly as it did before themes existed
- **Given** a fresh profile where no theme has ever been chosen
- **When** a document is displayed
- **Then** page, body text, headings, links, code, blockquote bar and highlights are identical to the pre-theming rendering — System is the regression bar, not a new look

### 18.3 Selecting a theme re-styles the preview live
- **Given** an open document rendered under System
- **When** the user selects Sepia
- **Then** the preview repaints book-like — an off-white yellowish page, a serif body face, and body text in black or a soft brown — in the same window, without re-reading the file from disk and without losing the reading position
- **And given** the window has other tabs open, including one the user has never activated
- **When** the user switches to any of them afterwards
- **Then** that tab shows the newly selected theme in full — page, ink and typeface together — never the new page under the previous theme's ink, and without needing a second theme change to correct it

### 18.4 The theme reaches everything the preview draws
- **Given** a document containing a blockquote, inline code, a fenced code block, a link, a table, an image, a horizontal rule, and an annotation
- **When** it is rendered under Sepia
- **Then** no element is left on desktop-theme colours — no white slab, blue bar, or grey island on the sepia page

### 18.5 Highlights stay visible under every theme
- **Given** a document with an annotation highlight and find matches
- **When** it is rendered under each installed theme
- **Then** both remain clearly visible against that theme's page — no near-invisible pale wash on a cream page

### 18.6 A highlight looks the same in a table cell as in prose
- **Given** an annotated or found term appearing both in body prose and inside a table cell
- **When** it is rendered under any theme
- **Then** the cell's highlight is the same colour as the body's — the two rendering paths differ, the colour does not

### 18.7 The theme applies to the preview only
- **Given** Sepia is selected
- **When** the window is displayed
- **Then** the toolbar, tab strip, outline sidebar and editor keep the desktop theme — theming the rest of the app is the window manager's job

### 18.8 Body text is legible in every theme
- **Given** every installed theme
- **When** body text is rendered on its page
- **Then** the contrast clears a legibility floor, so a later "warm it up a bit" tweak cannot quietly degrade readability
- **And** the floor reaches **every ink a theme states**, not a hand-picked few — the link, the mark, the table header, the list markers, the rule, the quote bar and the annotation chip as well as the body and the headings — each measured on the surface it is actually read on, with the theme's own translucent washes composited first
- **And** each ink answers to the floor its ROLE takes: 4.5:1 where a reader parses words, WCAG's 3:1 non-text floor where the ink is a drawn mark
- **And** a heading's ink is measured against **every appearance its band can take** — both endpoints of a gradient, not just the first — and skipped with a named reason where a sprite outranks the fill, because no ratio describes reading text on arbitrary pixels
- **And** a pairing that is deliberately below its floor is **named**, with its reason, and a named exception that is no longer below its floor is a failure: a licence standing over nothing is inherited silently by the next theme to state that key

### 18.9 Theme and zoom compose
- **Given** a document under Sepia
- **When** the user zooms in and out
- **Then** body and headings scale together, the heading hierarchy keeps its relative proportions, and the theme's colours and typeface are unaffected at any zoom level

### 18.10 A theme's spacing lands where it says
- **Given** a theme that sets the list indent and blockquote bar metrics, and a document with a nested list inside a blockquote
- **When** it is rendered
- **Then** markers and text appear at the positions that theme specifies, and each marker stays aligned with its own text at every nesting depth — verified by resolved on-screen position, never by the theme value having been read

### 18.11 A malformed or hostile theme file never breaks the app
- **Given** a theme file with invalid syntax, out-of-range spacing, or values containing stylesheet punctuation
- **When** the app starts
- **Then** it starts normally and renders legibly on a fallback theme — no crash, no broken layout, and no styling escaping the surface it was meant for

### 18.12 The chosen theme persists
- **Given** Sepia is selected
- **When** the app is quit and reopened
- **Then** Sepia is still the active theme

### 18.13 A user-supplied theme is honoured
- **Given** the user places their own theme file in their configuration directory
- **When** the app starts
- **Then** their theme appears in the chooser and renders — the app's internal configuration-directory redirect must not silently swallow it

### 18.14 A new theme needs no code change
- **Given** a new theme added to the theme data file, and nothing else
- **When** the app starts
- **Then** it appears in the chooser and renders correctly — the engine holds no per-theme knowledge

### 18.15 A theme can colour the list marker without recolouring its text
- **Given** a theme that sets a distinct list-marker colour, and a document with bulleted, numbered, and task-list items
- **When** it is rendered under that theme
- **Then** the bullet dot, the ordered numeral, and the task checkbox all take the theme's marker colour, while each item's text keeps the body foreground — the marker glyph is themed, the content is not; a theme with no marker key leaves markers on the body foreground exactly as before

### 18.16 In-cell selection follows the reading theme, matching the body
- **Given** a table cell with selected text and a body selection, under a reading theme (not System)
- **When** they are rendered
- **Then** the cell's selection highlight is the theme's selection colour — the same colour as the body's selection — not the desktop default; and under System, where the body selection stays the desktop default, the cell selection matches it (both fall back together, never one themed and the other not)

### 18.17 Selected text stays the theme's, and stays legible
- **Given** any reading theme that states its own page, and a selection covering body text, a heading, and text inside a table cell
- **When** it is rendered
- **Then** the selected text is drawn in a colour the THEME owns — never the desktop's selected-text ink — and its contrast against that theme's selection fill clears the same legibility floor as body text (18.8), on both the body and the in-cell path
- **And** a theme may state that ink outright (`selection_fg`); omitted, it is derived from the page and its own ink, so a theme that states only a fill still cannot strand its selected text
- **And** under System, where no page is stated, both paths keep the desktop's own selection colours together, exactly as before themes existed (18.2)

### 18.18 A table cell and the export sinks render themed emphasis identically to the body
- **Given** a document with bold, superscript, or subscript both in body prose and inside a table cell, under a theme setting `bold_weight` and `supsub_scale`
- **When** it is rendered, and separately exported to PDF and HTML
- **Then** the table cell matches the body exactly, and the PDF's Pango markup carries the same weight/size/rise the body tag applied

- **And** the TABLE HEADER takes the same `bold_weight`, and the same `heading_font` where the theme states one, on all three surfaces — it is bold text like any other, and hardcoding a weight per surface (`font-weight: bold`, a browser default, a Pango `<b>`) means three different numbers for one key
### 18.19 A theme can restyle the annotation chip by colour or by sprite
- **Given** a theme setting `annotation_chip_bg`/`annotation_chip_fg`, or a `annotation_chip_sprite` file (theme-relative, validated the same way every sprite key is — no absolute path, no traversal, symlink-contained, allowlisted extension, size-capped)
- **When** a document with a CriticMarkup comment is rendered
- **Then** the gutter chip shows the theme's colours, or the sprite in place of the flat fill — with the overflow-count numeral still legible on top
- **And** under System, where no chip key is set, the chip stays the exact hardcoded amber/white it always was (18.2)
- **And** the HTML export's claim-back-link takes the same colours (or the sprite, embedded so the artefact stays self-contained); the PDF export's comment note takes the same colours — a sprite has no expression in the PDF's inline Pango markup, which is a stated scope limit, not a silent gap

### 18.20 A broken image is never left on desktop colours
- **Given** a document referencing a missing or refused image, under a non-System reading theme
- **When** it is rendered
- **Then** the placeholder's fill and border resolve from the theme, closing the one construct 18.4 currently misses

### 18.21 A theme can give headings per-level colour and face
- **Given** a theme setting distinct colours and/or faces for h1–h5
- **When** a document with headings at every level is rendered
- **Then** each level shows its own colour/face; a link inside a heading still wins over it (existing priority); and a level the theme leaves unset falls back to the theme's single `heading_color`/`heading_font`, unchanged from today

### 18.22 A theme can decorate a heading with a rule and control the space above it
- **Given** a theme setting a heading overline and/or underline rule, and `heading_space_above`
- **When** headings are rendered
- **Then** the rule(s) appear on the stated side and the stated space above is honoured — closing the asymmetry where only space-*below* existed before

### 18.23 A theme can style strikethrough and link underline independently of colour
- **Given** a theme setting `strikethrough_color`, and a link-underline style (`none`/`double`/`wavy`) with its own colour
- **When** struck-through text and a link are rendered
- **Then** both apply without perturbing bold, italic, or mark, which stay themed exactly as today

### 18.24 A theme can replace list markers with a glyph string, or a sprite
- **Given** a theme setting bullet/ordered/task glyphs (sanitised, length-clamped), or bullet/ordered/task sprite files (theme-relative, validated the same way every sprite key is — no absolute path, no traversal, symlink-contained, allowlisted extension, size-capped)
- **When** a document with all three list kinds is rendered, and separately exported to HTML and PDF
- **Then** the gutter draws the glyph, or the sprite, in place of the dot/numeral/checkbox
- **And** the same glyph reaches the HTML export HTML-escaped, and the PDF export Pango-escaped; a sprite reaches the HTML export embedded (the artefact stays self-contained) and the PDF export drawn as an image — one key, three renderings (TDD §25's completeness rule)
- **And** where a sprite marker applies, the item's own text run carries **no** marker prefix — a picture *instead of* the bullet, never both
- **And** a marker whose sprite **cannot be produced** — admitted but undecodable — falls back to the theme's glyph, and then to the drawn dot/numeral/checkbox. It never leaves the marker absent, which for a task item would leave an invisible checkbox behind a hit-box that is still clickable

- **And** an item whose FIRST block is not a paragraph — a fenced code block, a nested list — still carries its marker in every medium, glyph or picture alike; the drawn gutter marks every item whatever it contains, and the page must agree
### 18.41 Every decoration degrades when its sprite cannot be produced
- **Given** a theme naming a sprite for any decoration — heading band, blockquote bar, horizontal rule, annotation chip, or any list marker — whose file is admitted but cannot be decoded
- **When** the document is rendered, and separately exported to HTML and PDF
- **Then** each falls back to what it would have been without the sprite: the band to its gradient then its fill, the bar and the chip to their flat colours, the rule to a plain separator, a marker to its glyph and then to the drawn primitive
- **And** the fallback is never a *partial* render and never a gap — `sdd/THEMING.md` § Untrusted input's inert-by-default rule
- **And** the failure is logged, because an absent decoration is otherwise indistinguishable from a theme that named none (ScrAP-324)

### 18.25 A theme can band a heading, with a fill or a sprite
- **Given** a theme setting a heading band (fill, and optionally radius/gradient within the closed decoration vocabulary), or a sprite image as the band's fill (theme-relative, validated the same way every sprite key is)
- **When** a heading — including one that soft-wraps — is rendered, and separately exported to HTML and PDF
- **Then** the band spans the stated extent and survives soft-wrap as one continuous band, whichever fill it carries
- **And** the band appears in both export sinks, the PDF resolving at System-light per 25.9, a sprite embedded in the HTML sink and drawn as an image in the PDF sink
- **And** a **sprite alone bands the level**: `heading_band_sprite_hN` with no `heading_band_color_hN` beside it paints the band, reserves the heading's inset, and does so identically on all three surfaces — a sprite outranks the fill and does not depend on one
- **And** a `heading_band_gradient_to_color` with no fill beneath it is **ignored and logged**: a gradient is a second stop and needs a first one, so the key renders nothing, and a key that renders nothing says so

### 18.26 A theme can vary a bullet's colour, glyph and sprite by nesting depth
- **Given** a theme setting a bullet colour/glyph/sprite for depth 1, and distinct overrides for depth 2 and depth 3-and-deeper (each optional, unset falling back to the shallower depth)
- **When** a document with a bullet list nested three-or-more levels deep is rendered, and separately exported to HTML and PDF
- **Then** each depth paints with its own resolved colour/glyph/sprite in the gutter
- **And** it reaches the HTML export via a depth-scoped `::marker` selector, and the PDF export via a themed colour on the marker text — closing a pre-existing gap where the PDF sink coloured no marker at all, of any kind, at any depth

### 18.27 A theme can colour task checkboxes independently of bullets and numerals
- **Given** a theme setting a task-marker colour distinct from `list_marker_color`
- **When** a document with a checked and an unchecked task item is rendered, and separately exported to HTML and PDF
- **Then** both checkbox states take the stated colour while bullets and ordered numerals in the same document keep `list_marker_color`'s colour
- **And** omitted, task markers fall back to `list_marker_color` exactly as today (TDD 18.2)

### 18.28 A theme can tile a sprite behind the blockquote bar
- **Given** a theme setting a sprite image for the blockquote accent bar (theme-relative, validated the same way every sprite key is)
- **When** a blockquoted document is rendered, and separately exported to HTML and PDF
- **Then** the bar fills with the sprite tiled at its natural size in place of the flat `blockquote_bar_color` colour
- **And** the tile grid is anchored to the **document**, not to the viewport: scrolling a quote taller than the pane moves the pattern with the quoted text by the same number of pixels, and the grid does not re-phase at the moment the quote's top leaves the pane. (A viewport-anchored grid leaves the tiles nailed to the screen while the text slides underneath — the defect the flat bar could never show, since a flat fill carries no phase)
- **And** the tile is not sliced along the bar's left edge: the grid's horizontal anchor is the bar's own left edge, so a decoration that does not begin on a tile boundary still shows whole tiles across the bar's width
- **And** omitted, the bar stays the flat themed colour exactly as today

### 18.29 A theme can give a blockquote its own background and ink
- **Given** a theme setting `blockquote_bg` and/or `blockquote_fg`, independent of the accent bar's own colour
- **When** a blockquoted document is rendered, and separately exported to HTML and PDF
- **Then** quoted text paints on the stated background in the stated ink, alongside the existing bar
- **And** the background is ONE continuous panel over the whole quote — every block it contains (paragraphs, a nested list, a fenced code block), in every medium — never a separate fill per block with page colour showing between them
- **And** the ink re-inks the quote's PROSE ONLY: a link, **a heading** and **a `==mark==`** inside the quote each keep their own colour — and where the theme states no colour for one of them, "its own colour" is the resolved BODY ink, never the quote's
- **And** omitted, quoted text stays plain body text on the page background exactly as today (TDD 18.2) — and the heading and mark tags set no foreground at all, so a theme stating no quote ink leaves them byte-identical

### 18.30 A theme can colour table header text independently of heading_color
- **Given** a theme setting `table_head_fg`
- **When** a table is rendered, and separately exported to HTML and PDF
- **Then** the header row's text takes the stated colour instead of `heading_color`
- **And** omitted, the header text falls back to `heading_color` exactly as today — **and this parity now genuinely holds in the PDF too**: before this rubric the PDF sink read no header colour at all (every cell painted in body ink), so "falls back to heading_color" was true of the preview and HTML but not the artefact until this landed

### 18.31 A theme can tile a sprite across the horizontal rule
- **Given** a theme setting a sprite image for the horizontal rule (theme-relative, validated the same way every sprite key is)
- **When** a document containing a `---` rule is rendered, and separately exported to HTML and PDF
- **Then** the rule fills with the sprite tiled at its natural size in place of the flat `rule_color`
- **And** omitted, the rule stays the flat themed colour exactly as today

### 18.32 A heading key can be stated once for every level, or narrowed to one
- **Given** a theme stating a bare heading key and the same key narrowed to a level (`heading_color` plus `heading_color_h2`)
- **When** a document with headings at every level is rendered, and separately exported to HTML and PDF
- **Then** the narrowed level takes its own value and every other level takes the bare one, on all three surfaces
- **And** a level the theme leaves unset takes the bare key; a key stated in neither form takes its own default; and h6 still renders as h5 throughout, as it always has

### 18.33 An unrecognised theme key is reported, never silently swallowed
- **Given** a `themes.toml` carrying a key this build does not recognise — a misspelling, or a key from a later version
- **When** themes are loaded
- **Then** every recognised key in that theme still applies, and one `warn` record naming the theme id and the offending key is logged
- **And** the rendering is unchanged: an unknown key is inert, exactly like an unset one

### 18.34 A theme's own bare key outranks the system theme's narrowed one
- **Given** `[themes.system]` stating a narrowed key (`heading_color_h1`) while the selected theme states only the bare `heading_color`
- **When** an h1 is rendered
- **Then** it takes the selected theme's bare value — source order decides between two themes, narrowing decides only within one
- **And** with the selected theme silent on both forms, that h1 takes `[themes.system]`'s narrowed value

### 18.35 The key vocabulary has one spelling, and a retired one is not it
- **Given** a theme written against a pre-rename spelling (`sprite_rule`, `heading_colors`, `link`, `strikethrough_rgba`, an array-valued `heading_scale`)
- **When** themes are loaded
- **Then** each such key is reported by 18.33's unknown-key path and applies nothing — a retired spelling is never quietly honoured, so a theme file that looks like it works cannot be one that does not

### 18.46 A theme key that can never apply says so
- **Given** a themes file stating a key that is shadowed at **every** level it could reach — the bare `heading_space_above` in a user's `[themes.system]`, over a built-in that states `heading_space_above_h1` through `_h5`
- **When** themes are loaded
- **Then** one `warn` record names the theme id, the key, and the narrower spellings that beat it — and the key still applies nothing, exactly as the resolution order says it should
- **And** a bare key that still wins at *any* level, a key beaten only by a *different* theme, and a key some surface reads bare regardless of levelling (`heading_color`, `heading_font`, `list_marker_color`) are each reported by nothing
- **And** the shipped `data/themes.toml` states no such key
- **Rationale** the third refusal class, and the one that was silent: the key is spelled right, is the right type, and parses — so 18.33's unknown-key path and 18.35's wrong-type path both pass it through. Without a record, a key that never applies is indistinguishable from one that applied and did nothing

### 18.44 A declared key reaches a surface, or says why it does not
- **Given** the registry of keys a themes file may speak
- **When** any one of them is stated at a value nothing ships
- **Then** the output of **every surface that key claims** changes — the preview's CSS, tag set or drawn decoration; the HTML artefact; the laid-out page
- **And** a key that reaches a surface it does not claim is equally a failure: the exception is stale and must be wired or restated
- **And** every excluded surface carries a **reason**, because an unexplained exclusion is indistinguishable from a key somebody forgot to wire
- **Rationale** a key declared and never read is worse than an unknown one: an unknown key warns (18.33), while a declared-but-inert key is accepted, documented, and silent

### 18.42 A themed inline style is one span, whichever surface builds it
- **Given** a theme stating `mark_fg`, a strike colour, a bold weight, a superscript rise, a link colour or an annotation wash
- **When** the same construct is rendered in a table cell (which the preview styles with Pango markup, not a tag) and exported to PDF (which lays every run out through Pango)
- **Then** both carry the **same span** — the two are one builder reading one theme, not two copies reading two sources
- **And** every span's opening and closing tag come from **one** call, because a strike's plain form closes `</s>` and its themed form closes `</span>`, and a mismatched pair fails `pango_parse_markup` and renders the whole run empty with no warning (ScrAP-163)

### 18.43 A translucent theme colour stays translucent in every artefact
- **Given** a theme stating a colour with alpha — `blockquote_bg = "#0a183080"`, or either of the two shipped translucent defaults
- **When** the document is rendered, exported to HTML and exported to PDF
- **Then** all three show a **wash**: the preview composites it, the HTML sheet carries eight-digit hex, and the page composites it onto the paper
- **And** a colour with no alpha is unchanged on every surface, in its six-digit spelling — so a theme that states none is byte-identical to before this held (TDD 18.2)

### 18.40 The themes file is found by a stated search path, first match wins
- **Given** a host that may carry a themes file in the user's configuration directory, in the per-user data directory, or in any system data directory
- **When** themes are loaded
- **Then** the candidates are tried in that order — user override, per-user install, then each `$XDG_DATA_DIRS` entry in the order the platform lists them — and the **first one that exists wins whole**
- **And** a later candidate is **not merged over** an earlier one, so a system install cannot add keys to a user's own themes file
- **And** every system data directory is a candidate: the list is iterated, never hard-coded to `/usr/share` (on KDE its first entry is `/usr/share/plasma`)
- **And** the directory a theme's sprite references resolve against is **the found file's own parent**, not the first candidate tried and never the working directory
- **And** on a host where no candidate exists, the compiled-in themes stand alone

### 18.36 A sprite reference is admitted by what it IS, not only by how big it is
- **Given** a themes file on disk whose sprite reference names a FIFO, or a symlink to a file whose real extension is not on the allowlist, or a path that leaves the theme's own directory in any spelling
- **When** themes are loaded
- **Then** each is refused and logged, and the decoration is absent — a size test alone would admit the FIFO (whose reported length is zero) and then block the main thread on the read forever
- **And** an ordinary contained reference beside the same file still resolves, so the refusals are about the hazard and not about the directory

### 18.37 A sprite is bounded by its decoded size, not only by its file size
- **Given** a sprite file inside the byte cap whose header declares more pixels than the decoded-raster cap (a decompression bomb: a 20000×20000 PNG is under 512 KiB)
- **When** any surface asks for that sprite — the preview's texture, the pre-resampled marker, or the PDF sink's image surface
- **Then** it is refused before being decoded, and one `warn` naming the sprite and its declared dimensions is logged
- **And** the decoration degrades to absent, the same answer every other refusal in this vocabulary gives

### 18.38 A sprite that cannot be produced says so
- **Given** a sprite file that passes every admission check but cannot be decoded — truncated, corrupt, or a `.png` that is not one
- **When** it is asked for through either decode path
- **Then** a `warn` naming the reference is logged on **both**, and neither path silently answers "absent"
- **Rationale** an inert-by-default decoration makes "the theme stated no sprite" and "the reference resolved but would not decode" produce identical pixels; the log record is the only observable that distinguishes them (ScrAP-324)

### 18.39 A sprite's admission is re-established when it is read, not only when it is resolved
- **Given** a sprite that passed resolution and has since grown past the byte cap, or ceased to be a regular file
- **When** it is read — at first paint, at a theme swap, or at an export
- **Then** it is refused on the open handle and logged, and the read is bounded by the cap rather than predicting it
- **Rationale** resolution runs once at load and the read happens many times afterwards; between them the guarantee is a path and nothing else

### 18.45 A themed rule reaches the widget, not only the stylesheet
- **Given** a reading theme that states a `link_color` (and, where it states them, `link_underline` / `link_underline_color`)
- **When** a table cell whose ENTIRE content is a link is rendered — the `GtkLinkButton` shape, not the mixed cell's label
- **Then** the button node RESOLVES to the theme's link ink, and its caption carries the theme's underline, exactly as the body link and the mixed cell beside it do
- **And** the check reads the resolved style off a real widget, never the generated rule text: a selector naming a class no widget carries generates, formats and asserts identically to one that matches, so rule-text assertions are blind to the whole failure (a blanket rename of the theme vocabulary spelled GTK's own `link` class as this project's `link_color` key, and 1279 tests stayed green)
- **Rationale** a stylesheet is an instruction, not an effect; a test of the instruction is a test of the same defect one layer up

### 18.47 Two decorations that overlap composite in one stated order
- **Given** a document that nests one drawn decoration inside another — a heading band, a code-block card or a list marker inside a blockquote; a marker on the row a band or a card covers; a code block's copy button inside its own card
- **When** the preview paints
- **Then** the CONTAINED decoration lands ON the containing one: the quote panel is the ground for everything a quote holds, the accent bar and the gutter markers run over the band and the card they cross, and the copy button is drawn over its card
- **And** that order is stated ONCE, as data the paint iterates, so changing it is an edit to a value a reviewer reads rather than a rearrangement of statements in a draw callback
- **And** two decorations whose drawn columns cannot intersect are ordered by nothing and are recorded as such rather than left unmentioned — the right-margin annotation chip against anything in the content column, and a quoted list's markers against the accent bar they are deliberately placed clear of
- **Rationale** the pairs are derived from the vectors the paint draws from, not enumerated by hand: a pair overlaps only if the two constructs nest in Markdown AND their rectangles intersect in x. Both halves are measurable — the nestings against the renderer's own products, the columns against the arithmetic the painters share — so the list is auditable rather than remembered, which is what a decoration added later needs

### 18.48 A theme can band a disclosure's summary line
- **Given** a theme setting `disclosure_band_color` (optionally with `disclosure_band_gradient_to_color` or `disclosure_band_radius`), or a `disclosure_band_sprite` as the band's fill
- **When** a document containing a `<details>` is rendered — collapsed and expanded, at top level and nested inside a blockquote or a list item
- **Then** the band sits behind the whole summary line, spanning the same width a banded heading spans rather than only the width of the label's text, and survives soft-wrap as one continuous band
- **And** a **sprite alone bands the line**: `disclosure_band_sprite` with no `disclosure_band_color` beside it paints the band, exactly as a heading's sprite does without a fill
- **And** a `disclosure_band_gradient_to_color` with no fill beneath it is **ignored and logged** — a gradient is a second stop and needs a first one, and a key that renders nothing says so

### 18.49 A theme can re-colour a disclosure's summary text
- **Given** a theme setting `disclosure_fg`
- **When** a document containing a `<details>` is rendered
- **Then** the summary line's own text takes that colour, and no other line does
- **And** a summary inside a blockquote takes this colour rather than the quote's, while a collapsed block's body preview keeps `disclosure_preview_fg` where a theme states one — the narrower statement wins

### 18.50 A disclosure's band and its summary colour are set independently
- **Given** a theme setting only one of `disclosure_band_color` and `disclosure_fg`
- **When** a document containing a `<details>` is rendered
- **Then** the stated one applies and the unstated one leaves its surface exactly as it was — a theme may band without re-colouring, and re-colour without banding

### 18.51 A disclosure's band and summary colour reach an exported document
- **Given** a theme setting a disclosure band and summary colour
- **When** a document containing a `<details>` is exported to HTML and to PDF
- **Then** both artefacts show the band behind each summary line and the summary text in its themed colour
- **And** the band's corner rounding reaches neither artefact, as no drawn corner rounding does

### 18.52 A themed page keeps its colours when the window loses focus
- **Given** a reading theme that states its own page colours (any theme but System)
- **When** the reader activates another window, so this one is drawn as unfocused
- **Then** the rendered document is unchanged — every part of it, including the parts drawn as widgets rather than as buffer text: a table's cells and header cells, a link in either cell shape, and a disclosure's indicator
- **And** only the window's own chrome follows the desktop's unfocused styling, because that is the desktop's to decide
- **And** under System, which states no colours of its own, the whole page follows the desktop together — the cells never dim while the prose does, or the reverse
- **Rationale** the widget-borne parts take their ink by INHERITANCE from the page, and a desktop theme states an unfocused ink on the node each of them actually draws with (`label:backdrop`), which an inherited value loses to regardless of which provider it came from — so a page that looks right focused can re-ink half of itself the moment focus leaves, and the half it re-inks is the half no stylesheet assertion can see

### 18.53 A themed page inks its disclosure indicator, stating it or inheriting the page's
- **Given** a reading theme that states a page of its own — whether or not it says anything about the disclosure fold
- **When** a document containing a `<details>` is rendered
- **Then** the indicator is drawn in a colour the theme owns: the one it states for the marker, or the page's own text colour where it states none
- **And** under System, which states no page, the indicator keeps the desktop's chevron in the desktop's colour, exactly as it did before any of these keys existed
- **Rationale** the fallback is the rule the drawn list markers already follow — a marker's ink is the body's until the theme says otherwise — so the two kinds of marker cannot disagree; and an indicator left on the desktop's colour is not merely off-palette, it is the one part of a themed page that changes when the window loses focus (18.52)

### 19.1 A relative link to a Markdown sibling opens as a new tab
- **Given** a rendered document containing a relative link to another `.md`/`.markdown` file that resolves within the current document's folder
- **When** the reader clicks it
- **Then** the target opens as a new tab in the CURRENT window — never an external handler — reusing an already-open tab for that file instead of duplicating it (the same dedup as File ▸ Open, comparing canonicalized paths so two spellings of one file are one tab)

### 19.2 Containment is enforced by default
- **Given** "Load Unsafe Linked Documents" is OFF (the default) for the tab containing the link
- **When** a clicked link resolves to a target OUTSIDE that document's folder (an absolute path elsewhere, a `..` traversal, or a symlink under the folder pointing outside)
- **Then** navigation is refused and a visible status-bar notice explains why — never a silent no-op, which is the failure that makes a click look broken rather than blocked

### 19.3 The toggle lifts containment for the current tab's own links only
- **Given** "Load Unsafe Linked Documents" is turned ON for a tab
- **When** a link in THAT tab's document resolves outside its folder
- **Then** navigation is permitted (the target still canonicalizes to a real file)
- **And** the tab landed on has its OWN toggle at its default (OFF) — turning the toggle on in one tab never grants navigation permission to whatever the linked document links to next; permission re-roots at every hop and cannot ratchet across the filesystem

### 19.4 The toggle is per-tab and does not persist
- **Given** the reader turns "Load Unsafe Linked Documents" on for one tab
- **When** a different tab, a different window, or a new application launch is observed
- **Then** the toggle is off — the permission was granted for that one document, in that one session, and is never silently reinstated. It is deliberately NOT in the session schema, unlike every other chrome toggle: a security consent that outlives the session that granted it is a materially weaker consent than the per-document model justifies

### 19.5 The toggle survives a tab move but never leaks into a new window's default
- **Given** a tab with the toggle ON is moved to a new window (Move Tab to New Window, or a cross-window drag)
- **When** the destination window is observed
- **Then** its File-menu checkbox and toolbar button report ON — the moved tab's true value, never the new window's construction-time OFF default. A per-window action mirroring per-tab state must seed from the arriving tab; a menu that misreports a security toggle is worse than one that is merely inconvenient

### 19.6 Non-Markdown local targets are refused, never launched externally
- **Given** a clicked local link resolves to an existing file that is not `.md`/`.markdown`
- **When** the click is processed
- **Then** navigation is refused with a visible "Not a Markdown document" notice — it is NEVER handed to the external URI launcher, regardless of the containment toggle's state. Falling through to the launcher here would be the same `file://` leak the scheme gate exists to prevent, reached through a different door

### 19.7 An explicit `file://` link is always refused
- **Given** a document contains an explicit `file://` (or any other non-`http`/`https`/`mailto`) scheme link
- **When** it is clicked
- **Then** it is refused with a visible notice — the external-launch scheme gate is unchanged and unaffected by the containment toggle. The two gates answer different questions: one governs launching a URI externally, the other governs navigating inside the app

### 19.7a A local path containing a colon is treated as a path, not mistaken for a scheme
- **Given** a link whose path contains a colon — a colon in the filename (`report:draft.md`) or a Windows drive letter — with no `://` and not a `mailto:`
- **When** the scheme gate classifies it
- **Then** it is treated as the schemeless local reference it is — resolved through the containment gate (§19.2) like any sibling — and is **not** refused as a "disallowed scheme"; the colon-before-the-first-slash is not a URL scheme (a genuine `file://`/`smb://` is still refused by §19.7; ScrAP-151)
- **Verification**: unit-tested cross-platform by `links::scheme_of` / `is_allowed_url` (string literals, no file). There is deliberately **no on-disk fixture** — a colon-named file is invalid on Windows (NTFS reads `:` as an alternate-data-stream separator) and would break `git clone` for everyone, and unlike the equivalent `resolve_image` temp-file test a committed file cannot be platform-gated with `#[cfg(unix)]`; see ScrAP-164

### 19.8 A missing target is a visible error, distinguished from a refusal
- **Given** a clicked local link resolves to a path that does not exist
- **When** the click is processed
- **Then** a visible "Link target not found" notice appears — distinguished from a containment refusal, which names the folder-boundary reason instead. The reader must be able to tell "this file is missing" from "this file is out of bounds"

### 19.9 A leading `~` expands before the containment gate, never around it
- **Given** a link or image path beginning `~/` (a bare `~`, or `~/` followed by a path)
- **When** it is resolved
- **Then** the `~` expands to the user's home directory FIRST, on the raw href, before the relative/absolute decision — so the expanded path is an absolute path outside the document's folder and is **Refused** unless the tab's toggle is on (never **Missing**, which is the silent wrong answer of reporting that an existing file does not exist; and never admitted past the gate, which would make the tilde a containment bypass)
- **And** a `~` anywhere but position 0 stays a literal path component (`./~/x.md` names a real subdirectory), and `~user/…` is deliberately unsupported
- **And** the image path reaches the same verdict as the link path — both resolve through one shared helper, so the two content types cannot drift apart about what a path means

### 19.10 A cross-document link fragment scrolls the target to the matching heading

- **Given** a relative link with a trailing `#fragment` (e.g. `TECH.md#module-map`) whose fragment slug matches one of the target document's own headings
- **When** the reader clicks it and the target opens as a new tab
- **Then** the new tab is not just opened — it is scrolled so that heading sits at the top, exactly as if the reader had clicked that heading in the target document's own outline sidebar
- **And** the lookup consults the **target tab's own** `heading_map`, the same map a same-document `#anchor` click reads — never a separately-parsed copy of the target's headings, which could silently disagree with the renderer about what a heading slugs to

### 19.11 The same fragment scrolls an already-open target tab, not just focuses it

- **Given** the target document is ALREADY open in some tab (this window or another)
- **When** a fragment link to it is clicked
- **Then** that tab is focused (19.1's existing dedup) AND scrolled to the matching heading — focusing alone, leaving the reader wherever that tab happened to be scrolled, is not sufficient

### 19.12 A fragment that matches no heading in the target still opens it, silently

- **Given** a link's fragment slug matches none of the target document's headings
- **When** it is clicked
- **Then** the target still opens (or is focused) normally, scrolled nowhere in particular, and no error notice appears — the same silent outcome a same-document `#anchor` click already has when its slug matches nothing. Deliberate: one failure shape gets one treatment, and a visible notice here would make the cross-document case louder than the identical same-document case for no reason the reader could infer

### 19.13 A link with no fragment is unaffected

- **Given** a relative link with no trailing `#fragment`
- **When** it is clicked
- **Then** navigation behaves exactly as before this feature existed — the target opens/focuses and nothing scrolls it

## 20. Annotations viewer

> A collapsible sidebar section, beneath the outline, lists the document's
> annotations (CriticMarkup comments) and navigates to them.

### 20.1 The viewer lists every comment-bearing annotation, in document order
- **Given** a document containing point comments and comment-bearing highlights scattered through it
- **When** the annotations viewer is shown
- **Then** it lists one row per annotation in document order, each showing the comment text with the annotated claim as dimmed secondary text (point comments show comment only)

### 20.2 Comment-less highlights and inert kinds are not listed
- **Given** a document containing a bare `{==highlight==}` (no comment) and inert kinds (`{++ins++}`, `{--del--}`, `{~~a~>b~~}`) alongside real annotations
- **When** the viewer is shown
- **Then** only comment-bearing annotations appear — a bare highlight is a rendering feature, not an annotation — and the list membership matches exactly the set of chips the preview draws (one shared predicate, no drift)

### 20.3 An empty or annotation-less document shows a placeholder, not an error
- **Given** a document with no comment-bearing annotations
- **When** the viewer is shown
- **Then** it shows a muted "No annotations" placeholder and the application does not error

### 20.4 Activating an entry navigates to that annotation and reveals it
- **Given** the viewer is shown in preview mode
- **When** the user activates an annotation entry (a single click, or an arrow/PgUp/PgDown key)
- **Then** the document scrolls to that annotation and its comment card opens exactly as if its margin chip had been clicked
- **And given** pure-edit mode, activating an entry moves the editor caret to the annotation's source position and opens **no** card
- **And given** split mode, activating an entry scrolls the preview to the annotation — the preview is the scroll driver (mirroring outline 12.9) — opens its card, **and** moves the editor caret onto that annotation, so the reviewer can begin editing it there, with the editor scroll-following the preview into the same region

### 20.5 Single click and keyboard both navigate — and the keyboard keeps the list
- **Given** the viewer has focus or is clicked
- **When** the user single-clicks an entry, or moves the selection with up/down/PgUp/PgDown
- **Then** navigation happens immediately (no double-click required)
- **And** the keyboard focus **stays in the list** across the navigation, so the next arrow press moves to the next row: browsing the list is a sequence of presses, not one press per trip through the Tab order. (The comment card the navigation opens therefore does *not* take focus — unlike a card opened by clicking its margin chip or by the Next/Previous Annotation commands, where the reader has asked to act on that one annotation and the card's own Escape/Edit/Remove must be reachable.)

### 20.6 Navigation resolves the correct annotation even when membership isn't 1:1
- **Given** a document where a bare highlight or inert kind sits *before* a comment-bearing annotation (so the annotation's list position differs from its chip's position)
- **When** the user activates that annotation's row
- **Then** the document navigates to *that* annotation — never a neighbor — because navigation keys on the annotation's source span identity, not a positional row index

### 20.7 An annotation inside a table cell is navigable
- **Given** a comment-bearing annotation anchored to text inside a table cell, off-screen
- **When** the user activates its viewer row (preview/split)
- **Then** the document scrolls the table into view and the annotation's card opens over the correct cell, holding the post-scroll position (the navigation is not silently reverted)

### 20.8 The viewer toggles on and off from every surface, as one command
- **Given** a document window
- **When** the user toggles the annotations viewer (View ▸ Annotations, the toolbar view button, or the in-pane ×)
- **Then** the pane hides/shows, and all surfaces reflect one consistent checked/sensitivity state — they are a single action, not independent toggles

### 20.9 Outline and annotations toggle independently; an empty sidebar disappears
- **Given** the outline and annotations panes
- **When** each is toggled on or off
- **Then** they stack (bold headers, and — since 20.21 — a draggable divider between them) when both are shown, either fills the sidebar alone when it's the only one shown, and the **whole sidebar is hidden** — content reclaims the full width — only when both are hidden

### 20.10 The viewer does not scroll-spy, and claims nothing it was not told
- **Given** the annotations viewer is shown with a selected row
- **When** the user scrolls the document or moves the caret without activating a viewer entry
- **Then** the viewer's selection does **not** change — it reflects only what the user last activated (annotations are notes, not sections that own a region)
- **And given** a list shown for the first time in a window, with nothing yet activated
- **Then** **no** row is selected: the highlight means "the annotation you last went to", so before the reader goes anywhere there is nothing to show

### 20.11 The list rebuilds as the document changes
- **Given** the viewer is shown
- **When** the document changes — live editing (after debounce), a view-mode switch, an external reload, or a theme re-render
- **Then** the list reflects the current annotations, reading the live editor buffer in edit/split and the stored source in preview, preserving the selection where the annotation still exists

### 20.12 Navigating via the viewer never blanks the preview
- **Given** a long document with tables, code blocks, blockquotes, and rules, shown in preview or split
- **When** the user rapidly navigates to viewer entries (including far top↔bottom jumps and fast resizes)
- **Then** the preview stays rendered — never blank, no "snapshot … without a current allocation" spam, no spurious horizontal scrollbar

### 20.13 Viewer visibility survives a restart
- **Given** the annotations viewer toggled to a given state (shown or hidden) in a window
- **When** the app is closed and relaunched restoring that session
- **Then** the pane returns in the same state — persisted per-window alongside outline visibility, defaulting to its initial state for sessions predating the field

### 20.14 The edit-mode caret lands on the right character past multi-byte text
- **Given** an edit-mode document with multi-byte UTF-8 (e.g. emoji, accented text) *before* a comment-bearing annotation
- **When** the user activates that annotation's viewer row
- **Then** the caret lands exactly at the annotation's source position — the byte offset is converted to a character offset, so it is not displaced by the width of the preceding multi-byte runs

### 20.15 Editing annotations in the editor updates the list live
- **Given** the viewer is shown in edit or split mode
- **When** the user types a new `{>>comment<<}`, edits a comment's text, or deletes an annotation in the editor
- **Then** after the debounce the list gains, updates, or drops that row accordingly, and a still-present selected annotation keeps its selection despite surrounding edits shifting spans

### 20.16 Rapid navigations leave the document coherent, not desynced
- **Given** a long document whose annotations are scattered far apart, shown in preview or split
- **When** the user navigates to several entries in quick succession (selecting them by click or key) — before each navigation's scroll has settled — including far up-and-down jumps
- **Then** the document ends at the **last** selected annotation, with the selection and the scroll position agreeing — never parked at an earlier target while a different row is selected. Each new navigation supersedes any still-running scroll from a previous one, rather than the two fighting over the viewport (a superseded scroll must not keep pulling the view back to its stale target)

### 20.17 Creating, editing, or removing an annotation updates the list immediately, in every mode
- **Given** the annotations viewer is shown, in **any** view mode — including **preview-only**
- **When** the user adds an annotation, edits its comment, or removes it through the preview's own annotate affordances (the selection pop-up and the margin card's Edit/Remove)
- **Then** the list gains, updates, or drops that row **at once**, in the mode the change was made in — the user never has to switch mode, switch tab, or reload to see the list agree with the document, and the surrounding rows keep their order and selection (POLICY Derived-view CAM row 3, column A)

### 20.18 Tab switch scrolls the annotations list to its selected row
- **Given** two tabs whose annotations lists are long enough to scroll, tab A with a selected annotation row that is not the first, and the shared annotations scroller left showing a different part of the list (e.g. after visiting a shorter list, or parked at the top)
- **When** the user switches to tab A
- **Then** the still-present selection is restored (20.11) **and** that selected row is scrolled into view — the highlight is never left correct-but-off-screen under a stale scroller position from the previous tab
- **And** restoring the selection still does not re-navigate (20.11); the list scroll is visual-only

### 20.19 Showing a sidebar pane hands the keyboard to its list
- **Given** the outline or annotations pane is hidden
- **When** the user shows it (View ▸ Annotations / F8, View ▸ Outline / F9, or the toolbar button)
- **Then** the keyboard focus moves into that pane's list, so the arrow keys drive it at once — a reader who asked for a pane in order to use it is given it, rather than having to Tab past the tab strip and the pane's own × to reach it
- **And** merely *reconciling* visibility — a tab switch, a session restore, the pane already being shown — never moves the focus; only the toggle does
- **And** hiding a pane does not move the focus either

### 20.20 Escape leaves the list without leaving the reader stranded
- **Given** the annotations list holds the keyboard focus, having opened an annotation's comment card
- **When** the user presses Escape
- **Then** the card is dismissed and the focus returns to the document pane (the editor in pure-edit mode, the preview otherwise), so Escape means the same thing here as it does in the card itself (§17.39)

### 20.21 The reader decides how the sidebar's height is shared
- **Given** both sidebar sections are shown, stacked outline-over-annotations
- **When** the user drags the divider between them
- **Then** the two sections take the shares the drag gives them, each still scrolling its own list; the divider **cannot take either section away** — it stops at a section's minimum height, so a section leaves the screen only by its own `win.outline` / `win.annotations` toggle and the toggle's state never disagrees with what is displayed (20.9)
- **And** hiding one section and showing it again returns to the divider position the reader chose, rather than resetting the split
- **And** with only one section shown there is no divider at all — that section fills the sidebar (20.9), and a window-height change is shared between the two rather than taken entirely from one
- **And** the state persists across app restarts — each window remembers its own divider position, restored with that window's session; it is stored as a *fraction* of the sidebar's height, so a window restored at a different height keeps the reader's ratio rather than a stale pixel count. A session predating the field, or one whose value is corrupt, restores to the even split rather than to a jammed divider

## 21. Crash forensics

> These rubrics cover what the application leaves behind when it dies. They exist
> because five recorded SIGSEGVs on the operator's machine left *nothing* that named
> the fault or the activity leading to it: no core dump (apport ignores an unpackaged
> binary), no symbols, and a log whose last line was hours stale. The deliverable is
> a report that answers "what was it doing" — the breadcrumb ring — not a backtrace.
> The reading here is diagnostic, not user-facing: nothing in §21 changes what the
> application does (21.11).

### 21.1 Every run leaves a persistent, self-identifying log
- **Given** the application launched by any means — desktop icon, terminal, file association
- **When** the process starts
- **Then** a log file in the user's state directory receives, as its first records, the application version, the build identity, the GTK runtime version, the active renderer, the process id and the start time — regardless of `RUST_LOG`
- **And** the records that reach stderr today keep reaching it, unchanged

### 21.2 The persistent log is bounded and keeps the recent past
- **Given** many runs, or one run that logs heavily
- **When** the state directory is inspected
- **Then** the log occupies a bounded amount of disk — capped size, a small fixed number of files retained — and it is always the *oldest* records that are discarded, never the current run's

### 21.3 Lifecycle events are recorded without turning on any switch
- **Given** a default launch, with no `RUST_LOG` set
- **When** a document is opened, saved, reloaded or closed; a tab or window closes; an external file change arrives; a file chooser opens
- **Then** each of those is recorded — with the path or tab it concerns — in both the persistent log and the in-memory breadcrumb ring, while stderr stays as quiet as it is today

### 21.4 A crash writes a report naming the fault
- **Given** the application dies from a fatal signal — segmentation fault, bus error, illegal instruction, abort, **or the breakpoint trap a GLib *fatal* log message dies of** (`g_error`, and any level promoted to fatal by a `G_DEBUG=fatal-*` flag or by `g_log_set_always_fatal`: GLib breakpoints rather than aborting on unix, so this death is a `SIGTRAP` and is no less a crash for it — `g_assert_not_reached()` is the lone abort)
- **When** the state directory is inspected afterwards
- **Then** a crash report file exists whose first lines name the signal, the faulting address, the faulting instruction pointer with the module and offset it falls in, and 21.1's build identity
- **And** the process still terminates from that same signal — the handler reports and re-raises, it never swallows the fault or changes the exit status

### 21.5 A crash report says what the application was doing
- **Given** a crash report was written
- **When** it is read
- **Then** it contains the most recent recorded events in order, with timestamps — including any GTK/glib diagnostics the benign-noise filter demotes, since a demoted transient is still forensic context

### 21.6 A truncated report is still useful
- **Given** the crash handler itself faults while writing the report
- **When** the partial file is read
- **Then** the fixed header is already on disk: identity and fault first, breadcrumbs next, and the backtrace and module map last — so what survives is what matters most

### 21.7 A panic writes the same report and still unwinds
- **Given** a Rust panic
- **Then** a report of the same shape is written, naming the panic message and its source location, **and** the panic still unwinds normally — `Drop` runs, and the message reaches stderr exactly as it does today

### 21.8 A crash report is self-resolving without symbols
- **Given** the shipped binary is stripped, and the distribution's GTK carries no debug symbols
- **When** an investigator reads a report
- **Then** it records the load address of every module a recorded frame falls in, so a frame resolves to *module + offset* by arithmetic alone — no core dump, no symbol server

### 21.9 The next launch notices an unread report
- **Given** a crash report was written
- **When** the application is next launched
- **Then** it says so exactly once, naming the file — and a subsequent launch does not repeat it
- **And given** the machine's clock has since moved *backwards* (an NTP correction, a local-time RTC, a restored snapshot), so the new report's name sorts below one already announced
- **When** the application is launched
- **Then** it is still announced — "already announced" is a set the application remembers, never an inference from ordering

### 21.10 A report is safe to hand over
- **Given** any crash report
- **When** its contents are reviewed before sending
- **Then** it carries event names, file paths and application state — never document text, buffer excerpts or clipboard contents

### 21.11 The forensic machinery changes nothing else
- **Given** all of the above in place
- **When** the application runs normally
- **Then** no second glib log sink is registered, panics still unwind rather than abort, hot-path trace instrumentation stays off unless explicitly requested, and neither startup time nor any user-visible behaviour changes

### 21.12 A crash report is readable only by the user who ran the application
- **Given** a crash report on disk, however it came to exist — written by the panic hook, by the fatal-signal handler, or rewritten by one over the other's file
- **When** its permissions are inspected on a multi-user machine
- **Then** it is owner-only, because it carries breadcrumbs, file paths and session diagnostics
- **And** this holds for the *second* writer as well as the first: creating the file is what sets its mode, so a report that already exists must not keep whatever an earlier writer's umask gave it

---

## 22. Crash recovery (swap files)

An unclean exit — a SIGSEGV, an OOM kill, a power loss — must no longer discard
every unsaved edit. The mechanism is a periodic full-content **swap file** per
dirty document, governed by one rule that the rubrics below are mostly restatements
of: *a swap file exists for a document if and only if that document is dirty.*

### 22.1 Unsaved edits survive an unclean exit
- **Given** a document with unsaved edits open in a tab
- **When** the application dies uncleanly and restarts
- **Then** the tab comes back with the pre-crash buffer content and still marked as having unsaved changes — the user's file on disk is untouched

### 22.2 Discarding unsaved work discards its recovery data with it
- **Given** a dirty tab whose content has been snapshotted
- **When** the user closes it and chooses to discard the unsaved changes
- **Then** the recovery data for that document is gone immediately, so a later launch never resurrects work the user explicitly threw away
- **And** because every dirty tab leaves by this route or by being saved, a cleanly quit session leaves nothing to recover at all

### 22.3 Saving removes the document's recovery data
- **Given** a dirty document that has been snapshotted
- **When** the user saves it, including under a new name
- **Then** no recovery data remains for that document

### 22.4 Editing back to the saved content removes the recovery data
- **Given** a dirty document that has been snapshotted
- **When** the user undoes or edits until the buffer matches what is on disk
- **Then** no recovery data remains — the same outcome as saving, reached without any path being taught about undo

### 22.5 Never-saved documents are recovered too
- **Given** an untitled buffer with typed content and no file path
- **When** the application dies uncleanly and restarts
- **Then** its content returns in a tab identified as a recovered untitled document, carrying no path — so a later save cannot silently write it somewhere

### 22.6 A document the session did not restore is still recovered
- **Given** an unclean exit that landed between the snapshot and the session record, so the restored layout does not contain the document
- **When** the application restarts
- **Then** the document is still recovered into a tab, because what is recovered is decided from the recovery data itself and never from the session record

### 22.7 A file changed on disk since the crash does not apply silently
- **Given** a recoverable document whose file on disk has changed since the crash
- **When** the application restarts
- **Then** the content is still recovered, and the user is shown the same external-change conflict prompt an ordinary outside edit raises (§5) rather than a second prompt built for this case

### 22.8 Recovery is visible, and an empty recovery is silent
- **Given** an unclean exit that left recoverable documents
- **When** the application restarts
- **Then** each recovered tab shows a dismissible notice naming when the recovered changes were captured, and the window's status bar reports how many of its documents were recovered
- **And** a window that recovered nothing shows neither — never "Recovered 0 documents"

### 22.9 Recovery is applied first and reversible second
- **Given** a recovered tab showing its notice — the content is already restored, the user was not asked first
- **When** the user chooses to discard the recovery
- **Then** the tab reloads from disk, becoming clean and showing the on-disk content, and no recovery data remains for it

### 22.10 A foreign file in the recovery location is left strictly alone
- **Given** an unrelated file sitting where recovery data is kept
- **When** the application starts and scans
- **Then** the file is ignored and logged, never parsed as recovery data and never deleted
- **And** the same holds for recovery data of ours that is too damaged to read: it is kept, because it may be the only surviving copy of the user's work

### 22.11 Recovery data identifies its own document without help
- **Given** one document's recovery data alone, with no session record available
- **When** it is read
- **Then** it states which file it belongs to, whether that document was untitled, and when it was captured
- **And** this holds even for a path containing a newline or a line resembling the format's own delimiter, which must round-trip byte-identically rather than truncating the recovered document

### 22.12 Typing is never interrupted, and leaving the pane commits at once
- **Given** a document being edited
- **When** the user types continuously
- **Then** the interface never stalls waiting for recovery data to be written, and a snapshot still happens at a bounded maximum interval rather than being starved indefinitely by the continuous typing
- **And** when the user's attention leaves the editor instead — switching view mode, opening a menu, moving to another window or another application — the snapshot is taken immediately rather than waiting out the pause timer

### 22.13 Recovery data is private to the user
- **Given** any document, including one the user has made readable only by themselves
- **When** its content is snapshotted
- **Then** the snapshot is readable only by its owner, from the moment it is created
- **And** on a platform whose privacy mechanism is not POSIX mode bits, the containing directory carries it instead (as §21.12 already requires of crash reports)

### 22.14 A second running instance's recovery data is not taken
- **Given** two instances running, each with dirty documents
- **When** one of them starts up and scans for recoverable documents
- **Then** it leaves alone anything a confirmed live instance owns
- **And** where liveness cannot be confirmed, it recovers rather than skipping — a duplicated tab is recoverable, silently abandoned work is not

### 22.15 A failed snapshot is reported on both surfaces
- **Given** recovery data cannot be written (out of space, quota exhausted, no file descriptors)
- **When** a snapshot is attempted
- **Then** the affected tab shows a notice and the window's status bar reports it, saying the safety net is off — not that the document failed to save, which is untrue and alarming
- **And** it reports the transition, not every retry: a persistent failure does not emit a notice every few seconds, and the notice clears on the first success

### 22.16 Two windows on the same file recover independently
- **Given** the same file open in two windows with different unsaved edits
- **When** the application dies uncleanly and restarts
- **Then** both sets of edits are recoverable — neither overwrote the other's recovery data
- **And** they are recovered into *separate* tabs: two unsaved buffers for one path are two documents, so the second must never be applied over the first (22.17's correlation by path stops at the first tab it claims)

### 22.17 Reopening the crashed document by name recovers into that tab, not a second one
- **Given** an unclean exit left unsaved work in a document
- **When** the user restarts by **opening that file again** — an Explorer double-click, a command-line argument, a desktop-file association, `xdg-open` — rather than by a bare relaunch
- **Then** the work is recovered into the tab that file opened, and the window holds **one** tab for it, not a clean one beside a recovered one the user has to tell apart
- **And** this holds however the two paths are spelled — a relative argument, a symlink, `..`, or a different letter case where the filesystem is case-insensitive — because the same "is this the same file?" rule governs here as governs re-opening an already-open document (8.2/15.16)
- **And** an untitled recovery is never correlated this way: it names no file, so it always comes back in a tab of its own

## 23. Back / Forward navigation history

The browser gesture, applied to documents: a per-window history of *which
document was being read*, walked with the same keys and mouse buttons a web
browser uses. One rule underlies every rubric below: **a history entry is
created by an act of navigation, and traversing history is not one of them.**

### 23.1 Back returns to the document that was being read before
- **Given** a window in which the reader has moved from one document to another — by clicking a tab, by Ctrl+PageUp/PageDown or Ctrl+Tab, by picking one from View ▸ Documents, by following a link, or by opening a file
- **When** the reader invokes Back
- **Then** the document that was active *before* that move becomes active again, and the window is otherwise unchanged — no document is reloaded, re-rendered from disk, or has its own scroll position disturbed by the traversal

### 23.2 Forward returns to where Back was invoked from
- **Given** the reader has gone Back at least once and has not navigated since
- **When** the reader invokes Forward
- **Then** the document Back left is active again — Back and Forward are exact inverses over an unchanged history

### 23.3 Traversal does not itself become history
- **Given** any history
- **When** the reader goes Back and Forward repeatedly
- **Then** no new entries accumulate: the reachable set of documents is the same after ten traversals as after none. A history mechanism that recorded its own traversals would make Back stop being an escape route — the failure this rubric exists to prevent

### 23.4 A new navigation discards the forward trail
- **Given** the reader has gone Back one or more times, so a forward trail exists
- **When** they then navigate somewhere new instead of going Forward
- **Then** the forward trail is discarded and Forward is unavailable — browser semantics; the trail described a future the reader has now declined

### 23.5 Both commands are available on every surface, and greyed out when they lead nowhere
- **Given** a window
- **When** the View menu and the toolbar's view section are inspected
- **Then** Back and Forward appear in both, each driven by the one action that also carries the keyboard and mouse bindings, and each insensitive exactly when there is nothing in that direction — at the oldest entry Back is greyed out rather than wrapping around to the newest (deliberately unlike Previous Tab, which cycles: a history has ends, a ring does not)
- **And** their sensitivity is correct immediately after every event that can change it — a navigation, a traversal, a tab closing, a tab moving to another window — never only after some later unrelated interaction

### 23.6 The browser's own inputs work
- **Given** a window with history in both directions
- **When** the reader presses Alt+Left / Alt+Right (Cmd+[ / Cmd+] on macOS — see below), or the dedicated Back / Forward keys a keyboard may carry (`XF86Back` / `XF86Forward`), or the two thumb buttons a mouse may carry (buttons 8 and 9)
- **Then** each drives the same navigation as the menu item — one action, several inputs, no per-input behaviour
- **And** the keyboard bindings appear in the Keyboard Shortcuts window; the mouse buttons deliberately do not, that window describing keys
- **And** on macOS, Back/Forward's keyboard binding is Cmd+[ / Cmd+] — Safari and Finder's own spelling — rather than Alt+Left/Right: measured **on Quartz**, that keystroke was never actually reachable for Back/Forward from a focused editor to begin with there (`GtkSourceView`'s own `move-words` word-transposition binding wins the same key first — TDD §4.13; the opposite is measured on Win32 and X11, where this accelerator wins and `move-words` never fires, so the ordering is a property of the backend and not of the toolkit — ScrAP-311), but every native macOS text field still binds Option+Left/Option+Right to word navigation, and Back/Forward has no more business contesting that key than `move-words` does. `XF86Back`/`XF86Forward` and the two thumb buttons are unaffected — the same input on every platform

### 23.7 History is per window and never crosses between them
- **Given** two windows, each with its own history
- **When** Back is invoked in one
- **Then** only that window's active document changes; the other window's history and active document are untouched, and no traversal ever activates a document living in a different window

### 23.8 A document that leaves the window leaves its history
- **Given** history entries for a document that is then closed, or moved to another window
- **When** the reader walks the history afterwards
- **Then** it behaves as though that document had never been visited: no traversal activates it, no traversal is a visible no-op standing in for it, and Back becomes insensitive rather than appearing to be available while having nowhere to go
- **And** landing on a neighbouring document *because* one was closed is not itself a navigation — closing a tab must not push the tab it exposes onto the history

### 23.9 Internal page switches the reader did not ask for are not navigations
- **Given** an operation that switches tabs on the reader's behalf in order to show them something about each — Save All prompting for each untitled document, Close Other Tabs prompting per unsaved document, session restore selecting the tab that was active last launch, startup crash recovery revealing a recovered document
- **When** the operation finishes
- **Then** the history is as it was before it started (save for anything the reader genuinely navigated to), and Back does not replay the tour

### 23.10 History is a session-local, bounded convenience
- **Given** any amount of navigation
- **When** the window is closed and a later launch restores the session
- **Then** the restored window starts with no history — Back and Forward are insensitive, and the restored active document is the only thing in it
- **And** within a session the history is bounded (oldest entries are dropped past a fixed depth), so a long reading session cannot grow it without limit

### 23.11 Following a link to a section of the same document is a navigation
- **Given** a document that links to its own headings — a table of contents, a cross-reference, or the outline sidebar listing them
- **When** the reader follows one of those links, or activates an outline row, while that document is already the active one
- **Then** the preview scrolls to that heading **and** a history entry is recorded: Back becomes available even though the active document never changed, and the tab strip's selection never moves
- **And** the same holds for the *arrival* of a cross-document fragment link (§19.10): the target document is recorded with the heading it was opened at, so Forward re-lands on the heading rather than on wherever that tab happened to be sitting
- **And** the viewport movements that are **not** navigations record nothing — find-next, the split-pane scroll sync, the reading-position restores of the [Reading-Position Preservation CAM](CAM.md#reading-position-preservation-cam--events-that-perturb-a-text-pane-viewport), and an outline activation in pure-edit mode (which moves the editor caret, not the preview)

### 23.12 Back returns to where the link was followed from
- **Given** the reader followed such a link
- **When** they invoke Back
- **Then** the viewport returns to the position they were reading at **when they clicked** — not to the top of the document, and not to wherever the entry was originally created for — and Forward returns to the heading. The active document is not changed, reloaded or re-rendered by either press
- **And** that holds when the position they clicked from *was* the top of the document, which is where a reader following a table of contents at the head of the file always is: Back scrolls back up to it, rather than treating "already at the top" as nothing to restore and leaving the reader on the section they asked to leave (ScrAP-262)
- **And** for a document the *link itself opened*, the position Back returns it to is its **top** — the reader arrived there and never chose a position in it, so it must not be thrown to the document's end or left sitting on the section it opened at (ScrAP-263)
- **And** re-following the link for the section the reader is already in moves the viewport but adds no entry, so Back never needs two presses to leave a place reached once

### 23.13 Sections and documents are one history, in the order they happened
- **Given** a reader who has moved both between documents and between sections within them
- **When** they walk Back
- **Then** the two kinds of navigation interleave in the order they occurred: a step that crosses a document boundary switches tabs, a step within one document does not, and neither kind is skipped or reordered because of the other
- **And** the bound of 23.10 counts both, so a long walk through one document's own sections cannot grow the history without limit

### 23.14 A section the document no longer has is not a stop
- **Given** history entries naming headings, and the document is then reloaded or edited so some of those headings no longer exist
- **When** the reader traverses
- **Then** an entry whose heading is gone falls back to naming *just that document*: the traversal still activates it and leaves its viewport exactly where the reader left it — the same silent outcome §19.12 gives a link whose fragment matches nothing, never a jump to the top, which would destroy a reading position the reader never offered up
- **And** an entry that falls back to the document the reader is *already* on is not a stop at all — the traversal continues to the next place, and Back reports itself insensitive rather than remaining available with nothing perceptible behind it
- **And** the commands' sensitivity always agrees with what a traversal will actually do: Back is never reported available on the strength of an entry that has already gone stale, whichever way the document was rebuilt — an in-place re-render or a wholesale one (23.5)

## 24. Renaming an open document

Changing the *name* of the file a tab is reading, in place. The category is not
"rename" but **a document's identity changing while the document is open** — the
same category as the first save of an untitled buffer and as Save As — and the
rule underlying every rubric below is that **a path change is not a content
change**: it must not touch the buffer, the baseline, the dirty flag, the undo
stack, the reading position or the rendered preview, and must not re-read the
file.

Scope is deliberately narrow: the filename only, within the document's own
directory. Moving a document to another directory is a different command with a
different answer (it would invalidate the relative-resource base that resolves
images and local links) and is not this feature.

### 24.1 A clean, titled document can be renamed in place
- **Given** a saved document with no unsaved changes
- **When** the reader chooses Rename and supplies a new filename
- **Then** the file on disk carries the new name **in the same directory**, the old name is gone, and the bytes are unchanged

### 24.2 The rename is not an edit
- **Given** a document that has just been renamed
- **Then** the document stays clean, and the buffer, the reading position, the undo history and the rendered preview are all exactly as they were — no re-read of the file occurs, and no re-render is triggered
- **And** no crash-recovery snapshot is created or orphaned by the rename. A clean document has no snapshot, so this is a no-op *by position in the lifecycle* rather than by exemption — recorded because it is the kind of premise a later change silently invalidates, and it ends the moment rename is permitted on a dirty document

### 24.3 Every surface that names the document follows
- **Given** a renamed document
- **Then** the window title, the tab label, the tab tooltip, the View ▸ Documents menu item and the toolbar Documents combo all show the new name — immediately, and without the reader taking any other action

### 24.4 Live reload follows the file
- **Given** a renamed document
- **When** something else writes to the **new** path
- **Then** the change is picked up exactly as it was before the rename
- **And** re-creating a file at the **old** path changes nothing about this document — it is not reloaded from it, and its tab is not badged on account of it

### 24.5 A rename does not look like a deletion
- **Given** a clean document being renamed
- **Then** no "File deleted on disk" notice appears, the tab gets no ⚠ deleted-backing badge, and Save does not become enabled — the rename removes the old name from the directory, and none of that may surface as though the reader's file had been lost

### 24.6 Rename is unavailable when it cannot be correct
- **Given** a document that is untitled, or has unsaved changes, or whose backing file is known to be gone, or has a write in flight
- **Then** the Rename command is insensitive — in every view mode and on every surface at once, so no surface offers a rename the others refuse

### 24.7 An existing file is never overwritten
- **Given** a directory already containing a file of the chosen name
- **When** the reader confirms the rename
- **Then** the rename is refused, the reason is reported, and **both** files are untouched — the document keeps its old name and the other file keeps its contents

### 24.8 A vanished source is reported, not papered over
- **Given** the document's file has been deleted since the command was enabled
- **When** the rename is attempted
- **Then** it is refused and reported, the document is marked as having lost its backing file, and Save becomes available so the reader can re-create it — the same state the file monitor's own deletion handling produces, reached by the same code

### 24.9 A name that cannot be a filename is refused before anything happens
- **Given** the rename dialog
- **When** the reader types a name that is empty, `.`, `..`, contains a path separator, or is otherwise not a legal filename on this platform
- **Then** the confirm control is insensitive and the reason is visible in the dialog — the refusal happens *before* the filesystem is touched, never as an error afterwards

### 24.10 Changing only the letter case is a rename, not a collision
- **Given** a document named `notes.md`
- **When** the reader renames it to `Notes.md`
- **Then** the rename succeeds and the file carries the new capitalisation — on a case-insensitive filesystem exactly as on a case-sensitive one, the destination being the source rather than a different file

### 24.11 Renaming from the tab strip acts on the tab that was right-clicked
- **Given** a window with several tabs
- **When** the reader right-clicks a tab that is **not** active and chooses Rename
- **Then** that tab's document is the one renamed, and the reader is shown it — never whichever document happened to be active

### 24.12 The rename cannot be aimed at the wrong document
- **Given** the rename dialog is open for one document
- **When** the reader switches tabs, or another operation on that document completes, while the dialog is open
- **Then** the rename still applies to the document it was invoked for — the subject is resolved once, when the reader acts, and carried across the dialog and the filesystem call rather than re-asked afterwards

### 24.13 The new name is a name the directory actually holds
- **Given** a completed rename on a filesystem that stores a spelling other than the one it was given
- **Then** the tab, the title and the re-attached file monitor all use the **stored** spelling — not the requested one, which names no directory entry
- **And** where the directory does hold the requested spelling it is used unchanged; another name for the same file, such as a hard link beside it, is not a spelling correction
- **And** a rename that succeeded is never reported as failed because this follow-up read failed

### 24.14 A document stranded by a crash mid-rename comes back
- **Given** a case-only rename, which is two steps, interrupted between them — the document is under neither its old name nor its new one
- **When** the old path is next opened
- **Then** the document is found with its content intact, rather than reported missing with an offer to create a blank one over it
- **And** the rename is not replayed, and nothing is moved when the document is present, when more than one candidate matches, or for a file that is not this app's own debris
- **And** the recovery is silent — the reader is left in exactly the pre-rename state, so there is nothing to announce, accept or discard (ratified; the reasoning is ANTI-PATTERNS #272)

## 25. Exporting a document

Producing a presentation artefact — **HTML** to share, **PDF** to record — from the
document the reader has open. The category is a **second consumer of the same
document**, not a second renderer: an export is a function of the document *source*
and the same normalised event stream the preview is built from, never of the preview
widget. That is what makes it hold on a tab that was never rendered, in any view
mode, on an unsaved buffer, with no display.

Two constraints underlie every rubric below. **What leaves the application is opened
by software this project neither controls nor sandboxes**, so every containment
decision the preview makes is inherited and never relaxed — an export widens what is
*rendered elsewhere*, never what is *trusted*. And **one pipeline feeds both sinks**,
so a rubric that names one construct names it for HTML and PDF alike; where the two
genuinely differ, the rubric says so.

Rubrics 25.16 – 25.24 are the PDF sink (phase 2). They are authored here rather than
at phase 2's start so they describe what the sink must do rather than what it ended
up doing.

### 25.1 An exported HTML file exists and opens
- **Given** a document open in any view mode
- **When** the reader chooses File ▸ Export ▸ HTML and confirms a destination
- **Then** one self-contained HTML file is written at that destination, and it opens in a browser showing the document as the preview shows it

### 25.2 A never-rendered tab exports identically
- **Given** a deferred tab that has never built a preview — restored but not activated, or a window in edit-only mode
- **When** the reader exports it
- **Then** the output is identical to the same document exported after it has been rendered — the export never depends on what the reader did beforehand, and never builds a preview in order to succeed

### 25.3 Every construct exports as the preview shows it
- **Given** a document exercising every construct in the Document Rendering CAM's construct list, in each of its contexts — top level, table cell, block quote, ordered / unordered / task-list item, and nested lists
- **Then** each appears in the export with the content and structure the preview gives it — none silently omitted, none doubled
- **And** a construct that renders through more than one widget shape in the preview (a table cell is a link button when its whole content is a link and a label otherwise) exports the same either way — the shapes are indistinguishable to a reader, so they must be indistinguishable in the artefact
- **And** a **list item's content stays one paragraph**: an item holding several inline runs — text, inline code, a link, a soft break — is one block, not one block per run. A *tight* item's content reaches the exporter as bare inline events with no paragraph around it, unlike a loose item's, so the two arrive differently and must leave identically. **The fixture must give an item two or more inline runs**: with a single run the broken and correct paths produce the same output, which is why this went unnoticed until a real document was exported
- **And** a list **mixing** task items with plain ones exports **every plain item's marker** — a bullet or number, exactly as the preview draws it in the gutter. Deciding "does this LIST contain a task" and "does this ITEM need its own marker suppressed" are different questions; answering the second with the first strips the marker from every item once any one of them is a task, and that failure has **no on-screen symptom** — the preview is unaffected because it draws markers per item — so only comparing an exported file against the preview catches it

### 25.4 Raw HTML is dropped exactly as the preview drops it
- **Given** a document containing `<script>`, `<iframe>` and `<div>`, and a `<picture>` / `<source>` / `<img>` group
- **When** it is exported
- **Then** the export contains no `<script>`, `<iframe>` or `<div>` — neither executable nor escaped into visible text, since escaping would put text on the page the preview never showed
- **And** the `<picture>` group yields exactly one image, its `src` having passed the same containment gate the preview applies
- **And** the permitted element set is read from the single place the preview's scanner reads it from; a second copy of that set on the export path is a defect, not an implementation detail

### 25.5 The export is of the buffer, not the file
- **Given** a document with unsaved changes, or an untitled buffer never written to disk
- **Then** the Export command is enabled, and the artefact carries the buffer's current text — not the bytes on disk, and not nothing

### 25.6 Cancelling the destination chooser is a clean no-op
- **Given** the destination chooser open
- **When** the reader cancels it
- **Then** no file is created or modified anywhere, and no notice is raised

### 25.7 A failed write reports and leaves nothing behind
- **Given** a destination in a read-only directory, or a filesystem with no space
- **When** the export is attempted
- **Then** the failure is reported to the reader, and no partial, empty or temporary file is left at or beside the destination

### 25.8 The export survives its host going away
- **Given** an export write in flight
- **When** the tab is closed, the document is reloaded, or the window is closed
- **Then** the artefact is written completely and correctly to the destination the reader chose, nothing is lost or corrupted, and no tab is resurrected by the completion
- **And** the outcome notice is reported against the status stack the export was started from, never one re-resolved when the write lands

### 25.9 The export is themed, never literal
- **Given** any installed reading theme
- **When** a document is exported to HTML
- **Then** every colour, typeface and decoration metric in the artefact resolves through the theme engine from the **active reading theme** — a literal styling value anywhere in either sink is a defect
- **And** the PDF resolves through the same engine against the **System theme's light resolution** by default, paper having no dark mode; "default to System-light" is a resolution request, not a licence for a literal
- **And** the PDF sink expresses **every** key the other two surfaces express, not four of five: a heading's ink and face as well as its scale, weight, band and rule; the mark ink; both code fills; and the three metrics — list step, list-item gap, and the gap between a quote's bar and its text — which were `INDENT_PT`, `BLOCK_GAP_PT` and "whatever the bar's own width happened to be"
- **And** a design-time **pixel** metric reaches the page converted to **points**, never read as a point count

- **And** a design-time PIXEL metric is converted to points before it reaches the page, EVERY key without exception: the unit error is coherent per key, so a sink that converts some and not others looks correct from whichever key a reader checks
- **And** an embedded sprite's payload appears in the artefact ONCE per distinct image, however many constructs use it — a payload emitted per USE is linear in the document's length and turns one 512 KiB image into hundreds of megabytes of HTML
### 25.10 The default filename cannot destroy the source
- **Given** a document named `notes.md`
- **When** the destination chooser opens for either target
- **Then** the name it proposes is `notes.html` or `notes.pdf` — the document's **stem** plus the target extension, never the document's filename
- **And** this is asserted on every platform rather than assumed, because a chooser that appends the filter's extension only to a name carrying no suffix returns `notes.md` unchanged and the export writes over the reader's source

### 25.11 The name proposed is validated; the name returned is not
- **Given** any open document
- **Then** the name the chooser is seeded with satisfies the application's own filename rules, checked before the dialog opens
- **And** the name the chooser returns is taken as given — once the chooser is open, naming the file is the reader's responsibility and the platform's, and gating the return would reject a name the platform has already accepted and rewritten

### 25.12 Images travel with the export
- **Given** a document with a local image admitted by the containment gate
- **When** it is exported to HTML
- **Then** the image's bytes are embedded, so the artefact still renders correctly after being moved, renamed or sent to someone else
- **And** the **PDF carries the image as a real image object**, decoded and drawn onto the page — not a text placeholder describing it. Checkable from outside the application: `pdfimages -list` on the artefact lists it. Contained to the column and the page and never upscaled, the preview's `max-width: 100%` rule in a page's units
- **And** an image whose bytes cannot be decoded falls back to a **visible note** rather than a silent gap, in both sinks — a reader must be able to tell an image was expected
- **And** a remote image is referenced by URL and **not fetched at export time**, unless *that document's* Show Unsafe Images gate is on, in which case it is embedded — enabling that option is the reader ratifying those images
- **And** the gate is read per document; it is never taken from a global preference and never inferred from another tab
- **And** a PDF exported from a document with remote images and the gate off has gaps where those images are, and this is stated to the reader rather than left to be discovered

### 25.13 Annotations appear in the export
- **Given** a document carrying CriticMarkup annotations
- **Then** the export shows each claim highlighted and its comment beside it — an aside in HTML, a margin note in PDF, matching what the preview shows
- **And** the highlight covers exactly the annotated characters, even where the rendered text is shorter than its source because construct markers were stripped
- **And** the comment appears **once per annotation**, however many pieces the highlight is drawn in. A claim that spans inline markup — `{==a **bold** word==}` — is split at every construct boundary, and in HTML the anchor the aside links back to is written on exactly one of those pieces: N copies of a comment is a different document, and N elements sharing one `id` is invalid and makes the back-link ambiguous

### 25.14 An export announces itself, either way
- **Given** a completed export
- **Then** a transient status notice reports the outcome — on success *and* on failure — because a silent export is indistinguishable from a broken one and the file it wrote is somewhere the reader was not watching

### 25.15 An export does not move the footprint
- **Given** a release build with a representative document open
- **When** the document is exported
- **Then** the VRAM and RSS figures §6 gates are unchanged — an export is a new rendering path and therefore a significant change by §6's own definition

### 25.16 A page break never splits a line of text
- **Given** a document long enough to paginate
- **When** it is exported to PDF
- **Then** no line of text is divided across a page boundary — a line falls wholly on one page or wholly on the next

### 25.17 A table too wide for the page wraps, then scales — never clips
- **Given** a table wider than the printable width of the page
- **When** the document is exported to PDF
- **Then** the table is contained within the printable width — **never clipped at the margin, never reflowed into a different table**
- **And** *reflowed into a different table* means a change to the table's **structure**: a column dropped, merged, split, reordered, or degraded into prose or a list. **The column count is the invariant.** Wrapping a cell's text onto more lines *inside its own column* is not a reflow — the table a reader sees has the same columns in the same order, and every cell is still in the cell it belongs to
- **And** the two remedies apply **in that order**: a table that cannot fit at its natural widths has its columns narrowed and its cells wrapped; **only** a table that still overflows once every column is at its own minimum content width (its longest unbreakable word) is uniformly scaled. Wrapping is preferred because it costs a reader nothing, where scaling costs legibility — so a table that could have wrapped must never be found scaled
- **And** each row is one indivisible fragment, so 25.16's page-break rule holds for tables: a break falls **between** rows and never through one, however many lines a row's tallest cell wraps to
- *(Ordering ratified by the operator 2026-08-20, replacing the unauthored "bound on how far scaling may go": the bound is no longer needed as a number, because scaling is now the last resort rather than the first response, and the regime that reaches it is one no wrapping could have saved. Automated in `export::pdftable`'s unit tests and `export::pdf`'s layout tests; driven end-to-end per MANUAL-TEST 25.17.)*

### 25.18 Text round-trips out of the PDF byte-exact
- **Given** an exported PDF containing ASCII, precomposed and combining accents, em and en dashes, curly quotes, ellipses, CJK, arrows, typographic symbols and box drawing
- **Then** every one of those categories extracts **byte-exact, per line, over the whole line**, on every platform — no platform carve-out and no caveat in the predicate
- **And** decomposed sequences survive uncomposed; `U+0065 U+0301` does not silently become `U+00E9`
- *(Method constraints are not predicates: extract with `pdftotext -enc UTF-8 -raw` — **`-raw`, not `-layout`**, because 25.18b forbids asserting against a layout-reconstructing extractor and `-layout` IS one, so the two clauses contradict each other if this says `-layout`. Decode the output as CESU-8 before concluding anything about characters, never gate on font metadata, and record which extractor build produced the measurement. A seat may have only a layout-reconstructing build available — Xpdf 4.00 ships inside Git for Windows — in which case `-raw` is not a preference but the only admissible mode on that box.)*

### 25.18a An extraction result is never evidence about appearance
- **Given** any assertion about text extracted from an exported PDF
- **Then** it is paired with a check of the **rendered page**, and a round-trip failure is never reported as a rendering defect
- **And** the two genuinely disagree for colour glyphs, so an extraction-only suite would condemn a surface that is correct

### 25.18b The emoji limit is asserted per platform, as measured behaviour
- **Given** an exported PDF containing an emoji **above the BMP**
- **Then** each platform's measured behaviour is asserted as that platform's contract — its purpose is to catch a **change**, not to demand a fix for behaviour this project cannot reach
- **And** the emoji is astral rather than BMP, because a BMP emoji round-trips cleanly on Windows and a rubric written against one passes while the limit it documents goes unmeasured
- **And** the assertion is not made against a layout-reconstructing extractor's output, which is a reader-side weakness this project accepts explicitly

### 25.19 Emoji in an exported PDF survive a reader's clipboard
- **Given** an exported PDF opened in a mainstream reader
- **When** the reader selects a line containing emoji and copies it
- **Then** the pasted text is correct — this is the acceptance criterion the emoji question was closed on, and a different path from command-line extraction
- **And** monochrome emoji are never substituted for colour ones to obtain it: they are drawn differently rather than desaturated, so the substitution changes what the reader sees

### 25.20 A PDF export's success is asserted from what it drew
- **Given** a completed PDF export
- **Then** success is concluded from the operation's return value **and** the count of pages drawn against pages expected
- **And** never from `is_finished()` or `status()`, which are inverted in both directions — success never reports finished, and finished means aborted

### 25.25 A named font reaches the PDF as that named face
- **Given** `[themes.system]` states a multi-word `font_family` and a multi-word `heading_font`, both installed on the host
- **When** the document is exported to PDF
- **Then** the artefact's embedded fonts are those faces by NAME, body and heading alike
- **And** the check asserts the face Pango RESOLVED, never the requested string, the laid-out width, or "not the default font": the theme holds a CSS font stack in which a multi-word family is quoted, Pango's own list parser rejects quotes and falls through to the stack's generic terminator, so a totally broken sink lands on plain `serif` — a real face, a different width, and exactly what a reader would expect a serif theme to look like
- **And** the fixture goes through `sanitize_font_family` and asserts the value came back QUOTED, since a bare generic (`monospace`) is left unquoted and so passes on the broken sink
- **Rationale** the de-quoting seam existed for the preview's tag sink and was `pub(super)` inside `tags/`; the PDF sink could not reach it and handed Pango the CSS spelling verbatim. The debt was closed on the strength of the seam existing, which is a claim about the seam and not about its callers

### 25.21 A cancelled or failed export leaves the destination untouched
- **Given** a destination that already holds a PDF
- **When** an export over it is cancelled part-way, or fails part-way
- **Then** the destination is **byte-identical** to what it was before the export began
- **And** the assertion is against the previous file's bytes, because the partial an export leaves behind is itself a structurally valid, cleanly-extracting PDF that no integrity check can distinguish from a complete one
- **And** the check seeds a real previous PDF and drives a real cancel part-way through a real render, and records structurally why it is green, so a mutation sweep cannot delete it as vacuous

### 25.22 Export cost asserts its shape, not a duration
- **Given** documents of increasing length at a fixed content weight
- **Then** per-page cost does not grow with document length
- **And** the assertion is never a wall-clock number, which a slower machine or a denser page would fail while the contract held — the same page count is some forty times apart in cost between dense and sparse content

### 25.23 A long export reports progress and can be cancelled
- **Given** a document long enough that the export crosses the responsiveness threshold
- **Then** an indicator appears in the **status bar** — not a dialog — driven by pages completed and triggered by **elapsed time**, never by a page count
- **And** the reader can cancel it, which stops after the current page and leaves the destination as 25.21 requires

### 25.24 A destination another process holds open fails by name
- **Given** an export destination that another process holds open — the ordinary case being a PDF still open in a viewer
- **When** the export is published on Windows
- **Then** it reports a **named** failure telling the reader to close the file, never a generic write error, since the remedy is the reader's and a generic message describes neither the cause nor it
- **And** the same export on Linux and macOS succeeds — the check targets **local disk**, the stricter case, a network destination succeeding where local NTFS does not

## 26. Self-contained macOS bundle

> The `.dmg` exists to satisfy build-pipeline step 10's stated intent: *an artefact a
> non-developer can install with no toolchain*. A bundle that resolves its libraries out
> of `/opt/homebrew` fails that intent outright rather than narrowly — the recipient has
> no Homebrew, so it does not launch at all.
>
> **These rubrics gate the OBSERVATION, not the file list.** Each one asserts what the
> running bundle can do from inside itself, never that some directory was staged. The
> distinction is load-bearing and was measured: GtkSourceView's language specs and style
> schemes are not on disk in the Homebrew prefix at all — they are a GResource compiled
> into `libgtksourceview-5.0.dylib` — so a staging checklist would have passed a bundle
> whose data came from somewhere else entirely, and would stay silent if that GResource
> were ever stripped.
>
> **A pass is only evidence if the check can fail.** The development machine has Homebrew
> GTK installed, so a bundle that still links it launches there perfectly. 26.3 exists to
> keep 26.2 honest.

### 26.1 The bundle resolves no library outside itself
- **Given** a `Scribobulate.app` produced by `bundle.sh`
- **When** every Mach-O in the bundle is walked transitively with `otool -L`
- **Then** every load path resolves under `@rpath`, `@executable_path`, `/usr/lib` or `/System/Library`, and none under `/opt/homebrew` or `/usr/local`
- **And** each staged library's own install **ID** is rewritten too, not only its dependents' load commands — an absolute ID works by accident and breaks the moment anything re-resolves it

### 26.2 The bundle launches with the Homebrew prefix unreachable
- **Given** the built bundle, on a machine that does have Homebrew GTK installed
- **When** it is launched under a sandbox denying `file-read*` on `/opt/homebrew` and `/usr/local`
- **Then** it starts and presents a window, rather than dying in `dyld`

### 26.3 The negative control is proven able to fail
- **Given** a bundle that still links Homebrew paths
- **When** it is launched under that same sandbox
- **Then** it fails in `dyld` naming the blocked library
- **And** the check is rejected as vacuous if it cannot be made to fail this way — a control that passes both a good and a bad subject measures nothing, which is what `DYLD_*` path scoping would have done here, since absolute load commands never consult it

### 26.4 The resources that fail late and silently are found
- **Given** the bundle running with the Homebrew prefix unreachable
- **When** the icon set, a file dialog, and an image-bearing document are exercised
- **Then** no icon falls back to the broken-image placeholder, no missing-GSettings-schema abort occurs, and images render
- **And** each of these is asserted as an outcome, because all three fail as a degraded window rather than as a link error, and so are invisible to 26.1 and 26.2

### 26.5 The editor's own syntax data resolves from inside the bundle
- **Given** the bundle running with the Homebrew prefix unreachable
- **When** a Markdown document is opened in the editor pane
- **Then** `LanguageManager` resolves `markdown` and `StyleSchemeManager` resolves one of the schemes the application names
- **And** neither is allowed to fail by returning `None`, which is legal, silent, and leaves the **edit pane on the light scheme while the rest of the application follows dark** — a defect a user reports as a theming bug rather than as a missing file

### 26.6 The bundled runtime is attributed
- **Given** a bundle carrying a GTK runtime it redistributes
- **When** the staged licence texts are checked against the libraries actually bundled
- **Then** every bundled library has a licence text present and non-empty, and the covered set is **derived** from what is bundled rather than hand-listed
- **And** each row declares a string that must occur in the staged text, since an SPDX identifier is a declaration about a licence and not evidence about the bytes shipped

### 26.7 The packaging gate rejects a bad artefact
- **Given** the packaging step
- **When** it produces a `.dmg` that is zero-byte, carries no GTK runtime, or disagrees with `Cargo.toml` on the version
- **Then** the step fails
- **And** that is established by mutation — each gate is shown failing on a subject known to be bad, never inferred from a green run
