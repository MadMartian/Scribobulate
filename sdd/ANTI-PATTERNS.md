# Anti-Patterns

Scribobulate's register of costly dead ends. It is a **project index, not an essay collection**: each entry is a few lines — the trap, where *this* tree implements the fix, and a pointer to the reusable home of the full lesson. Lessons about gtk4-rs itself, and general engineering discipline, are not here at all: they live in the `gtk4-rs` and `general-engineering-principles` skills and are cited `GTK4Rs/AP-N` / `GEP-N` at the site (the stubs and tombstones that once pointed there were retired 2026-09-24; their numbers are frozen in `scrap-numbers.manifest` with the skill entry each became). The full essays live in this file's git history — `git log --diff-filter=M --format='%h %ad %s' -- sdd/ANTI-PATTERNS.md` finds the revisions, and the last long-form one predates the compression to this index (2026-08-28). Read the table of contents, then only the entries whose titles match the task (SDD principle 7).

**Citation convention.** An entry here is `ScrAP-N` (bare `#N` only inside this file); a `gtk4-rs` skill entry is `GTK4Rs/AP-N`, one of its techniques `GTK4Rs/T-N`; a `general-engineering-principles` entry is `GEP-N`. A bare `AP-N` or `T-N` is illegal anywhere in the tree (`cargo xtask lint-references` check 8). Skills are named, never pathed — they may not be installed on every machine. When both registers hold a lesson, cite `ScrAP-N`; it is always resolvable.

**Routing rule — applied when an entry is MINTED, never in a later migration.**
1. About gtk4-rs itself (gtk4, glib, gio, gdk)? → weave it into the `gtk4-rs` skill and cite `GTK4Rs/AP-N` at the site. **Nothing here** — no stub, no number. The skill owns the routing; if an agent fails to reach it, improve the skill's routing, not this file.
2. General engineering discipline that survives deleting every Scribobulate noun? → route it to `general-engineering-principles` and cite `GEP-N` at the site. **Nothing here** either — the `**Routed**` tombstones were retired 2026-09-24 the same way as the gtk4-rs stubs.
3. Neither — Scribobulate internals, or a non-gtk4-rs dependency (Pango, GtkSourceView, pulldown-cmark, librsvg, syntect, serde/toml, the toolchain)? → it stays here, **in ≤ 6 lines**: Symptom · Root cause · Resolution · Lesson · Scribobulate · See. Extend an existing entry rather than minting a sibling for the same root cause. Route a Pango lesson on whose API *contract* it is about, and raise it before routing.

**Numbers are frozen** (check 9): never renumbered, never reused; a retired entry keeps its `## N.` heading as a landing spot. Reserved gaps — do not fill: **176–179** (Windows port; holder gone, held pending operator resolution), **186** (`feat/spelling`, inbound), **276–289** (unmerged branches). **Next free number: 366**+ (358 is in use on an unmerged branch on the `improving` remote) — check this table and announce the range you claim; never derive it from the highest heading below. (It read 354 while 354 and 355 both had bodies, so a writer who obeyed it minted a duplicate — and the same sentence forbids the one check a reader would otherwise make. Check 9 can only see a duplicate after it exists. **Check 21 now asserts the one relation the header must satisfy whatever the reserved gaps are — strictly above the highest heading present** — so this line is no longer guarded by prose alone; move it in the same change that mints.)

**Growth** is gated in bytes (check 11). The ratchet only tightens; consolidate in the change that trips it.

**Disposition** (`Disp`): `C` resident · `D` dead landing spot. (`A` gtk4-rs and `B` general-engineering-principles no longer exist as rows: those entries were retired to their skills, numbers frozen in the manifest.)

| # | Anti-pattern | Disp |
|---|--------------|------|
| 4 | Using Pango `<a href>` markup in GtkLabel for standalone link widgets | C |
| 6 | Using a horizontal rule to indicate a blockquote | D |
| 7 | Placing the blockquote `DrawingArea` in an outer overlay outside the `ScrolledWindow` | D |
| 9 | Duplicating action logic across context menu, main menu, and keyboard shortcut | C |
| 10 | Walking the widget tree to re-discover anchor-embedded GtkLabel widgets | C |
| 14 | Restoring `GtkTextView` scroll via adjustment manipulation after `set_buffer` | D |
| 27 | Searching find-next from the caret after `select_range` (re-finds the current match) | C |
| 36 | Letting the editor `GtkSourceSearchContext` `notify::occurrences-count` overwrite the preview buffer's `forward_search` count in preview mode | C |
| 35 | Reading `st.source` for a programmatic preview re-render in split mode | C |
| 45 | A `GtkNotebook` with `show-tabs` false cannot be a cross-window tab-drag drop target | D |
| 51 | A `GtkSourceSearchContext` `occurrences-count` handler that strong-captures its own context is a permanent self-reference leak | C |
| 58 | Reparenting a reused `GtkSourceView` across view-mode containers re-fires its gutter's never-unbound `vadjustment` binding → a use-after-free | C |
| 62 | A custom tab/stack widget leaves its active-index model unset for the default-visible first page | C |
| 66 | Relying on pulldown-cmark's native superscript/subscript for tight `E=mc^2^` / `H~2~O` | C |
| 73 | Reconstructing character-precise copied Markdown from sparse parser waypoints, and mis-reading pulldown-cmark offset semantics | C |
| 74 | Aligning char offsets with `GtkTextBuffer::get_text()` — it omits anchored children | C |
| 75 | A hard tab in a GFM table breaks table recognition; normalise tabs — but length-preservingly | C |
| 77 | UI-testing a formatter over the selectable read-only Preview pane | C |
| 78 | `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text | C |
| 86 | Probing a broader Markdown marker before a narrower one that embeds it mis-parses the input — test narrowest-first | C |
| 92 | A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale | C |
| 93 | Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block | C |
| 97 | Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs | C |
| 111 | The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally | C |
| 114 | An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render | C |
| 115 | Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence | C |
| 122 | Translating a stripped-then-parsed document's ranges back to original coordinates instead of per-position translation silently swallows the stripped bytes (the range-merge gotcha) | C |
| 123 | A coverage ratchet's floor recorded as stale prose drifts from the real (climbing) figure, silently loosening the gate | C |
| 130 | A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright | C |
| 147 | Raw-HTML `<picture>`/`<img>` silently dropped — block HTML is emitted per-line, wrapped in `Tag::HtmlBlock` | C |
| 148 | Splicing at an offset mapped OUT of a delimiter-stripped coordinate space | C |
| 158 | A content-less list item still emits a full item (and task marker) — an unconditional per-item gutter decoration draws a stray marker | C |
| 160 | syntect's bundled default syntax set has no TypeScript/TSX/TOML — a fence in one of those languages silently falls back to plain text and renders as one flat colour | C |
| 163 | Switching a `GtkLabel` to `set_markup` silently makes every interpolated string a Pango-markup injection/breakage surface — an un-escaped filename metacharacter renders the label EMPTY, with no crash | C |
| 187 | A byte range captured at build time and applied at click time is a bet, not a coordinate | C |
| 194 | A shared per-line helper that hands out a RAW line makes every block transform blind to the container prefix — one rule, four copies to get wrong | C |
| 195 | A decision driven off one parser's event stream cannot see the constructs a second tokeniser owns | C |
| 196 | A fallback keyed on a symptom, not a cause, silently swallows the next cause that shares the symptom | C |
| 200 | `GtkSourceIndenter` is unusable from gtk-rs — the subclass trampoline frees the caller's `GtkTextIter` | C |
| 228 | A property implemented on one branch, documented as a property of the whole function | C |
| 233 | Delegating a delimited format's unforgeable-terminator invariant to a third-party serialiser's escaping | C |
| 250 | A widget swapped in for one feature's sake moves its text out of every text-walker's reach | C |
| 255 | A construct whose glyphs are buffered at its `End` event is not opaque — it is char-precise in a coordinate space nobody wrote down | C |
| 261 | A derived-state hook installed at the producer misses the rebuild shape the producer also has | C |
| 282 | An operation counter is a complexity oracle only for operations you control | C |
| 294 | Letting a coverage ratchet be satisfied by widening the exclusion instead of testing the code | C |
| 298 | A TIGHT list item's content arrives as bare inline events with no `Tag::Paragraph` wrapper | C |
| 306 | A chooser's `set_current_folder` is best-effort and its failure is unobservable | C |
| 309 | A cancelled export destroys the destination, and the wreckage is a valid file | C |
| 311 | Which of two same-key bindings wins is a property of the BACKEND, not of the toolkit | C |
| 312 | Repairing the editor buffer when the defect is in the source string — the preview never reads the buffer | C |
| 314 | Instantiating ANY `sourceview::Buffer` subclass corrupts the heap — and the backtrace is innocent | C |
| 315 | Laying a table out with tab characters — a tab ladder cannot express a column | C |
| 316 | A repair handler on `insert-text` is also a handler on the UNDO machinery, and the divergence it causes is silent | C |
| 324 | A compiled-in asset resolved against a runtime directory is absent everywhere that directory isn't | C |
| 329 | A gate read through a pipe reports the pipe's last stage, not the gate | t |
| 334 | A repaint failure that also happens in an unrelated application is upstream, and the platform seam it invites is the wrong response | C |
| 338 | A settle wait pointed at a value the code records SYNCHRONOUSLY ahead of the work it waits for — it observes a constant, reports converged on the first turns, and leaves fixed drains doing the real waiting | C |
| 340 | Splicing a region whose rendered content was written by an event ABOVE that region | C |
| 343 | Enlarging a decoded `GdkTexture` to display a VECTOR image at a larger size | C |
| 345 | Judging a theme change against a machine that has ever run `install.sh` — the installed `themes.toml` overrides the built-in PER KEY | C |
| 346 | Assuming every `U+FFFC` in the preview buffer is an anchored child | C |
| 347 | A legibility gate measuring an ink against a surface its level can never show | C |
| 349 | Judging whether a cursor took from a screenshot — and letting an uncontrolled variable name the defect | C |
| 350 | A comment that tells you NOT to do something — load-bearing in the direction nothing checks | C |
| 351 | A gdk-pixbuf loader module that retains every decode — invisible to refcount assertions, visible only as slope | C |
| 352 | `GdkTexture::from_file`/`from_bytes` reach a pixbuf module's INCREMENTAL path, not its one-shot `load` | C |
| 354 | A reachability probe whose PRECONDITION names an asset that does not resolve — the key reads as reaching nothing | C |
| 355 | Solving a themed fill against the surface it is MIXED from rather than the page it is READ on | C |
| 356 | Deriving a text run's own fill from the PAGE when the preview draws a surface behind it | C |
| 359 | A pixel test that counts an exact colour over antialiased text — a glyph fringe can forge it | C |
| 360 | Assuming two libraries' renames end in the same Windows call — `std::fs::rename` is not `MoveFileExW` | C |
| 361 | A command that scrolls the split pane the reader is NOT in — the scroll sync puts it straight back | C |
| 362 | An emoji with a variation selector, or newer than Emoji 12, in a theme symbol — Windows draws a missing-glyph box | C |
| 363 | A strong `TabState` capture in a widget the tab bar owns — the tab reaches the bar, so it is a cycle | C |
| 364 | An open card that re-points only on scroll — a render in place moves its annotation under it | C |
| 365 | A pasted fence that never closes re-pairs the document's own fences — each mask check then finds one comment more | C |

---

## 4. Using Pango `<a href>` markup in GtkLabel for standalone link widgets
**Symptom**: a link rendered via Pango `<a href>` in a `GtkLabel` was believed to style and activate but with no pointer cursor on hover and activation on button-*press* rather than *release*.
**Root cause**: *(as recorded — since disproved)*: `GtkLabel` was thought to handle `<a href>` without `GtkLinkButton`'s full interaction model. **At 4.6.9 it does both**: `gtk_label_update_cursor` sets `"pointer"` over an active link (`gtklabel.c:737`) and `gtk_label_click_gesture_released` emits `activate-link` on **release** (`:4400`). See GTK4Rs/AP-239.
**Resolution**: a `GtkLabel` with `<a href>` markup, for every table cell including one that is nothing but a link. For a while a link-only cell was a `GtkLinkButton` (`has_frame = false`), chosen on the merits it does have (focusable, carries the URL as a property). It was retired because a button is a click target across its whole allocation and cannot have its caption selected: the whole cell followed the link and the reader could not swipe its text (TDD 2.9a). A label's link is exactly its caption.

## 6. Using a horizontal rule to indicate a blockquote
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 7. Placing the blockquote `DrawingArea` in an outer overlay outside the `ScrolledWindow`
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 9. Duplicating action logic across context menu, main menu, and keyboard shortcut
**Symptom**: the main-menu Copy stayed enabled regardless of selection — the surfaces drifted out of sync.
**Scribobulate**: one `win.copy` `SimpleAction`; every surface (menu / toolbar / context menu / accelerator) binds to it by name — POLICY single-source-of-truth.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-9).

## 10. Walking the widget tree to re-discover anchor-embedded GtkLabel widgets
**Symptom**: a recursive `find_selectable_labels()` tree-walk to re-find anchored cell labels is fragile and breaks across re-renders.
**Scribobulate**: a qdata handoff from the render pass to the copy-action wiring step.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-10).

## 14. Restoring `GtkTextView` scroll via adjustment manipulation after `set_buffer`
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 27. Searching find-next from the caret after `select_range` (re-finds the current match)
**Symptom**: "Find next" sticks on the current match while "find previous" works — the asymmetry is the tell.
**Root cause**: `select_range(ins, bound)` parks the caret (`cursor-position`) at the match **start**; `sc.forward(caret)` returns the first match at-or-after the caret — the same match, forever. Backward happens to advance, so only *next* looks broken.
**Resolution**: step from the **far edge of the current selection in the direction of travel** — forward from `selection_bounds().end`, backward from `.start`; fall back to the caret only with no selection. Same rule for a plain `TextIter::forward_search` loop.

## 36. Letting the editor `GtkSourceSearchContext` `notify::occurrences-count` overwrite the preview buffer's `forward_search` count in preview mode
**Symptom**: in preview mode the find count shows more matches than navigation can reach — the editor source context counts `| cell |` markdown the preview buffer can't navigate to (table cell text lives in `GtkLabel` child widgets, never in the buffer's btree).
**Root cause**: `set_search_text()` triggers `notify::occurrences-count` on the *editor* context, whose handler ran unconditionally and overwrote the correct preview (body-only) count with the inflated editor count.
**Scribobulate**: gate the handler with an early return when the active view mode is preview; reach cell text via a dedicated preview-hits builder.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-46, cell-highlight + two-step scroll); findings: researcher-findings-textview-search-anchored-cell-text.md.

## 35. Reading `st.source` for a programmatic preview re-render in split mode
**Symptom**: in split mode a programmatic preview re-render (zoom/toggle/theme) uses stale content — just-typed editor text vanishes until a mode round-trip.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the split-mode preview re-render site.

## 45. A `GtkNotebook` with `show-tabs` false cannot be a cross-window tab-drag drop target
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 51. A `GtkSourceSearchContext` `occurrences-count` handler that strong-captures its own context is a permanent self-reference leak
**Symptom**: no crash/warning — steady unbounded growth, one `GtkSourceSearchContext` (plus its `SearchSettings` and the buffer's tag table) leaked per closed tab.
**Root cause**: a signal connected ON the context, whose closure captures a strong `.clone()` of that same context — a GObject self-reference cycle (refcount-only, no cycle collector). The buffer↔context relationship itself is weak both ways in GtkSourceView source.
**Resolution**: read the emitter from the signal's own first parameter (`move |sc, _| …`), capturing nothing.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63, the general self-capture kernel); findings: researcher-findings-searchcontext-self-capture-signal-cycle.md.

## 58. Reparenting a reused `GtkSourceView` across view-mode containers re-fires its gutter's never-unbound `vadjustment` binding → a use-after-free
**Symptom**: switching view mode (Preview ↔ Edit ↔ Split) emits six `g_object_unref: assertion 'G_IS_OBJECT (object)' failed` per switch, only on the 2nd+ switch — a genuine read-after-free.
**Root cause**: the reused `GtkSourceView`'s gutter binds `view."vadjustment"` with `G_BINDING_SYNC_CREATE`, but `connect_view` never stores the returned `GBinding` (an upstream defect, unchanged in `main`), so it's never explicitly unbound. Rebuilding the mode container REPARENTS the reused view; every reparent re-runs `notify::vadjustment`, re-firing the binding against a tree mid-teardown.
**Resolution**: a custom `GtkWidget` container subclass mounts the editor's `GtkScrolledWindow` **once** and NEVER reassigns its child slot again; mode/orientation/order are pure layout parameters (`set_child_visible`, allocation order), never a `set_child` call.
**See**: gtk4-rs skill → state-and-subclassing (custom container that holds reused children as layout parameters). Findings: researcher-findings-gtksourceview-reparent-gutter-vadjustment-binding-unref.md.

## 62. A custom tab/stack widget leaves its active-index model unset for the default-visible first page
**Symptom**: moving/closing the FIRST tab of a window (never explicitly switched to) left the source window with a blank content pane and the moved tab's stale outline — a "phantom tab".
**Root cause**: the custom tab-bar/tab-view widget tracks the active tab in its own `Cell<Option<usize>>`, set only by its switch-to-index method — but a window's INITIAL page is shown by the `GtkStack`'s default (first child visible) and never travels through that path.
**Scribobulate**: a dedicated first-page-active marker, called once right after appending the first page, sets the active slot to `Some(0)` WITHOUT firing a switch callback.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-75); related GTK4Rs/AP-74, GTK4Rs/AP-104.

## 66. Relying on pulldown-cmark's native superscript/subscript for tight `E=mc^2^` / `H~2~O`
**Symptom**: tight, Pandoc-style superscript/subscript (`E=mc^2^`, `H~2~O`) never rendered — the literal `^`/`~` showed instead; a multi-tilde line also lost its SECOND subscript once native superscript was disabled.
**Root cause**: pulldown-cmark recognises `^`/`~`/`~~` with CommonMark FLANKING-delimiter rules (like emphasis) — the inverse of Pandoc's TIGHT rule; and any enabled tilde feature FRAGMENTS a paragraph across multiple `Text` events at a stray unpaired marker, defeating a per-event scanner.
**Resolution**: the Markdown-options setup disables `ENABLE_SUPERSCRIPT`/`SUBSCRIPT`/`STRIKETHROUGH` at every parse site; a dedicated scan step tokenises `^x^`/`~x~`/`~~x~~` ourselves with tight Pandoc semantics on clean, unfragmented text.
**See**: pulldown-cmark 0.13 `Options`; CommonMark §6.2 flanking rules; Pandoc superscript/subscript extension. Empirically bisected.

## 73. Reconstructing character-precise copied Markdown from sparse parser waypoints, and mis-reading pulldown-cmark offset semantics
**Symptom**: copying a *partial* preview selection returned the WHOLE enclosing block's Markdown source (four letters of a heading → the entire `# Heading` line).
**Root cause**: the sparse waypoint map records source offsets only at pulldown event boundaries and snaps outward — block-granular by construction. A block's Start/End range includes the TRAILING newline, an escaped char's `Text` token DROPS the backslash, and an entity tokenises apart from its rendered char.
**Resolution**: the copy-map builder constructs a buffer-annotated construct TREE in the same render pass that fills the buffer, reconstructing delimiters from source only when a selection crosses a construct's content boundary; leaf runs interpolate char-precisely.
**Lesson**: to reconstruct *balanced* source from a rendered selection you need a construct tree annotated with real render offsets, not a flat source-offset map.
**Case (scroll sync, 2026-10-11)**: mapping the editor's top line into the preview by the nearest waypoint at-or-before the line start landed on the block ABOVE a heading (88 px of the previous section showing). A block's waypoints begin after its markup (`### `), and whether one sits exactly on the line's first byte depends on the PREVIOUS block: after a list it does, pointing at the last bullet's end. The fix (`readingpos::line_to_preview_char`) takes the first waypoint strictly past the line start. A "first waypoint inside the line" rule passed a paragraph-before-heading fixture and was still wrong after a list — test every preceding block kind.

## 74. Aligning char offsets with `GtkTextBuffer::get_text()` — it omits anchored children
**Symptom**: a debug assertion (and any offset-indexed logic) drifted between the buffer's character offsets and a `buf.text()` string — by one char per anchored child.
**Root cause**: `gtk_text_buffer_get_text()`/`get_iter_text()` silently OMIT anchored children, but `char_count()`, `iter.offset()`, `slice()`/`get_slice()`, and `selection_bounds()` all count each as one `U+FFFC`. A `get_text`-derived char array and any iter/`char_count`-derived offset diverge silently, only on documents that HAVE anchors.
**Scribobulate**: the copymap drift guard and the copy path both use `buf.slice()`, never `text()`, when correlating with iter/char_count offsets.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-5).

## 75. A hard tab in a GFM table breaks table recognition; normalise tabs — but length-preservingly
**Symptom**: a table pasted from a spreadsheet, cells separated by hard TABS, rendered as a literal paragraph (`---` even turned into an em-dash via smart punctuation) — the byte-identical table with spaces parsed fine.
**Root cause**: a GFM delimiter row's grammar admits only `-`, `:`, `|`, spaces — a tab is a CommonMark/GFM-conformant rejection, not a pulldown bug.
**Resolution**: a dedicated tab-normalization step replaces a hard tab with ONE space — LENGTH- and POSITION-preserving (so copymap/scroll-sync byte offsets never drift) — exempting leading indentation and verbatim code regions (found via a structural pre-parse).
**See**: pulldown-cmark 0.13 `ENABLE_TABLES`; GFM spec §Tables; CommonMark §2.2/§6.2.

## 77. UI-testing a formatter over the selectable read-only Preview pane
**Symptom**: a formatter click over a visibly-selected Preview pane silently no-opped for every command — a selection in a selectable READ-ONLY view looks identical to an editor selection, but the format action is correctly disabled there.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: `src/window/editbar/focusgate.rs` — the window focus-widget gate (`connect_focus_widget_notify` + `is_ancestor`) that keys `win.format` on editor focus, not on selection presence; a read-only-preview…

## 78. `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text
**Symptom**: math (`$E=mc^2$`) rendered as nothing, footnote refs (`[^1]`) vanished, and YAML/`+++` frontmatter leaked into the body as a stray paragraph — silent content loss, no warning.
**Root cause**: `Options::all()` turns on EVERY pulldown-cmark extension, including ones the renderer has no handler for; the dispatcher's catch-all silently drops standalone events and leaks a container's inner `Text`.
**Resolution**: the Markdown-options setup is an explicit ALLOWLIST of only the extensions actually handled (`TABLES | TASKLISTS | SMART_PUNCTUATION | HEADING_ATTRIBUTES | GFM`); anything else degrades to literal `Text` rather than vanishing.

## 86. Probing a broader Markdown marker before a narrower one that embeds it mis-parses the input — test narrowest-first
**Symptom**: a GFM task item (`- [ ] foo`) auto-continued on Enter as if a plain bullet, leaving the checkbox dangling — everything looked right for plain bullets/numbered lists, so the bug only showed on the newest marker type.
**Root cause**: a task marker (`- [ ] `) is a bullet marker plus more; in an `if let … else if let …` chain, the FIRST-tested broader parser (bullet) matched the shared prefix and short-circuited before the narrower task parser ever ran.
**Resolution**: order the parser chain NARROWEST-first — the task-marker parser before the bullet-marker parser before the ordered-marker parser; give the narrow parser its own detector for the discriminating part (the checkbox).

## 92. A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale
**Symptom**: creating/editing/removing an annotation in preview-only mode wrote correct source but left the preview's highlights/markers/popover text stale until a manual reload.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the mode-agnostic annotation re-render site.

## 93. Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block
**Symptom**: a CriticMarkup comment marker/highlight placed by mapping a cleaned-source offset to a buffer position landed on the BLANK-LINE separator above its paragraph instead of the paragraph itself.
**Root cause**: pulldown's offset iterator reports the source range of the ENTIRE BLOCK for a block Start/End event, even though this renderer emits only the block separator there — an all-events offset lookup resolves a paragraph interior onto that misleading range.
**Resolution**: the preview-build offset-anchoring map is restricted to CONTENT events only (`Text`/`Code`/`Break`), excluding `Start`/`End` block-structure events.

## 97. Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs
**Symptom**: annotating a single plain word in a paragraph highlighted the ENTIRE paragraph.
**Root cause**: `wrap_span` inferred "inline" from whether a node's source open/close delimiter byte-ranges were non-empty — a paragraph ALSO has non-empty trailing "close" bytes, so it was mis-flagged inline and taken whole.
**Resolution**: the copy-map's branch-node representation carries an explicit kind set from the CONSTRUCT KIND at build time, never inferred from byte shape (originally an `inline: bool`; now the `BranchKind` enum — see ScrAP-255 for why the boolean pair became one enum).

## 111. The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally
**Symptom**: creating a cell annotation didn't show its amber highlight; removing the last cell annotation didn't clear it — both fixed only by an unrelated full re-render.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the in-place annotation-refresh site (the anchored-child cell-label reconciliation).

## 114. An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render
**Symptom**: an annotation created in preview-only mode vanished on a mode switch, then reappeared on the next toggle.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the mode-switch source-flush site.

## 115. Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence
**Symptom**: annotating a word inside a formatted table cell highlighted a DIFFERENT (the FIRST) occurrence of the same word elsewhere in the cell.
**Root cause**: the highlight was injected via `result.find(escaped_slice)` — a TEXT search returning the first occurrence, not the annotated char position; a range crossing an inline-format boundary also isn't one contiguous substring.
**Resolution**: a dedicated char-range markup-wrapping routine walks the markup tracking the PLAIN-char index (tags=0, entities=1) and opens/closes the span POSITIONALLY, closing before and reopening after every existing tag to preserve well-nesting.

## 122. Translating a stripped-then-parsed document's ranges back to original coordinates instead of per-position translation silently swallows the stripped bytes (the range-merge gotcha)
**Symptom**: an inline annotation adjacent to other CriticMarkup in the same paragraph (`the earth is {==flat==}{>>cite?<<} ok`) produced downstream text that silently OMITTED the stripped delimiter bytes whenever a translated RANGE was used.
**Root cause**: with delimiters stripped from the parsed ("cleaned") text before parsing, pulldown-cmark emits the surrounding prose as ONE `Text` event whose CLEANED range maps to a NON-CONTIGUOUS original region (the deleted bytes sit in the middle); a range has two endpoints and cannot express that hole, so translating it once (range-in, range-out) silently drops the gap.
**Resolution**: keep render maps in CLEANED coordinates and translate PER-POSITION at the point of use (a dedicated cleaned-to-original translation step), never as a range; identity-translate when the shift table is empty so unannotated documents keep byte-identical behaviour.
**See**: the CriticMarkup cleaned↔original shift-table mapping (verified against pulldown-cmark 0.13.4).

## 123. A coverage ratchet's floor recorded as stale prose drifts from the real (climbing) figure, silently loosening the gate
**Symptom**: POLICY's coverage prose cited "~66.48% lines" while the real figure had climbed to 67.83% over several unrelated cycles — nothing alerted, since a ratchet only fires on a DROP.
**Root cause**: the figure lived in three places (the coverage-gate script's floor constant, POLICY's inline snippet, prose) with no single source of truth; a contributing misread nearly set the floor from `cargo-llvm-cov`'s eye-catching FIRST (Regions) column instead of the gated LINES column.
**Resolution**: ratchet the floor and the stated figure together whenever coverage rises; verify a ratchet change by the gate's EXIT CODE, never the printed percentage.

## 130. A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright
**Symptom**: `sdd/system-overview.svg` rendered perfectly in Inkscape but the app itself showed a broken-image placeholder for its own architecture diagram.
**Root cause**: three `<text>` elements carried a DUPLICATE `class` attribute — a fatal XML well-formedness error; Inkscape's libxml2 recovery mode silently keeps the first occurrence and continues, while librsvg parses strictly and fails the WHOLE document with no partial render.
**Resolution**: `xmllint --noout file.svg` is the gate for any hand-authored/generated SVG, run BEFORE ever trusting a render; confirm in the actual (strict) consumer, never the lenient authoring tool.

## 147. Raw-HTML `<picture>`/`<img>` silently dropped — block HTML is emitted per-line, wrapped in `Tag::HtmlBlock`
**Symptom**: three related failures. (a) A GitHub-style `<picture>…</picture>` hero (WebP `<source>` + GIF `<img>` fallback) renders as NOTHING in-app, even though it displays fine on GitHub — the app's own README hero was invisible when the app opened its own README.
**Root cause**: - (a) The renderer's pulldown-cmark event loop drops `Event::Html`/`Event::InlineHtml` via a catch-all `_ => {}` (sanitize-by-omission — correct for untrusted HTML). pulldown-cmark 0.13 emits a **block** HTML construct **line-by-line** — one `Event::Html` per source line — **wrapped** in `Event::Start(Tag::HtmlBlock)` … `Event::End(TagEnd::HtmlBlock)`.
**Lesson**: when a renderer mirrors an HTML element's semantics, honour the element's **grouping/scoping**, not just the presence of the child tags — and remember the *same* logical construct reaches you as **either a block or inline events** depending on formatting the author didn't think about, so grouping st…
**Scribobulate**: a pure, unit-tested scanner turns a fragment into an ordered tag stream (`PictureOpen` / `PictureClose` / `Candidate(src)`); the renderer replays that stream against a `<picture>` grouping state carried **on the `Renderer`, across events** (`feed_html`/`picture_open`).
**See**: TDD 2.23 (the rich-images work — `<picture>`/`<img>` + WebP fallback — was retired into this entry + GTK4Rs/AP-66).

## 148. Splicing at an offset mapped OUT of a delimiter-stripped coordinate space
**Symptom**: annotating a cross-block preview selection that ends inside an existing `{==highlight==}{>>comment<<}` showed no new comment chip, and the typed comment vanished.
**Root cause**: **an offset translated out of a delimiter-stripped ("cleaned") coordinate space is safe to READ from but not safe to INSERT AT.** A cross-block selection's end is mapped cleaned→original through the shift table (`cleaned_to_original`), and any cleaned offset that falls within a kept highlight's *content* maps to a byte **strictly inside** the original `{==…==}` — the `{==` and `==}` were deleted i…
**Resolution**: **before splicing at an anchor that came out of a stripped space, snap it to a boundary the stripped space could not see.** A point comment must land cleanly *outside* any construct — it never extends one (extending is the intra-block highlight path's job; a cross-block selection was deliberately ne…
**Lesson**: **a coordinate is only as trustworthy as the space it was measured in.** An offset from a projection that *deleted* structure (a cleaned/stripped/normalised view) can be dereferenced for reading but must be re-validated against the *full* text before it is used as an insertion or deletion point — th…
**Scribobulate**: `annotate::point_comment_anchor` (pure, unit-tested in `annotate/mutate.rs`), applied at the single commit choke point `window::annotate::apply_annotation_edit`'s `Point` arm — which **both** the preview sink and the editor Create card route through, so neither call site can forget the guard.
**See**: TDD 17.44 (the cross-block point-comment contract + the deliberate no-extend decision); GEP-23 (an ANTI-PATTERNS entry must be self-contained — this one inlines the mechanism rather than citing the now-…

## 158. A content-less list item still emits a full item (and task marker) — an unconditional per-item gutter decoration draws a stray marker
**Symptom**: an empty task-list item `- [ ]` on its own line drew a checkbox in the preview gutter despite having no content. The same shape affected the other kinds: a content-less bullet (`- `) or number (`1.
**Root cause**: two compounding facts, both non-obvious. - The renderer pushed a `ListMarker` at **every** `Tag::Item`, unconditionally — nothing gated on the item actually producing content. pulldown-cmark emits `Start(Item)` … `End(Item)` for a content-less item too, so an empty item recorded a marker, and the gutter draw (which iterates every recorded marker whose first line is on-screen) drew it.
**Lesson**: a per-item — or per-block — decoration must gate on the item having produced content, not on the parser having emitted an item. CommonMark parsers emit a complete item (and even a task marker) for a content-less list item, so "the parser didn't give me an item" is the wrong emptiness test; "the rend…
**Scribobulate**: at `TagEnd::Item`, treat the item as empty when the walk inserted **no buffer content** for it (`end_offset() == item_start`) and drop the marker it pushed.
**See**: TDD 2.4b (empty items draw no marker); renderer `TagEnd::Item`; sibling pulldown quirks #66/#75/#147.

## 160. syntect's bundled default syntax set has no TypeScript/TSX/TOML — a fence in one of those languages silently falls back to plain text and renders as one flat colour
**Symptom**: a ` ```typescript ` (also `tsx`, `toml`, `kotlin`, `swift`, `dart`) fenced code block in the preview shows as a flat, single-colour block — every token the same ink, i.e. "all gray" — while ` ```js `, ` ```rust `, ` ```python ` highlight normally.
**Root cause**: syntect's `SyntaxSet::load_defaults_newlines()` (its bundled set, derived from Sublime's default packages) does **not** include a TypeScript grammar — nor TSX/TOML/Kotlin/Swift/Dart. The emitter (`renderer/emit.rs::insert_code_block`) resolves the fence via `ss.find_syntax_by_token(lang).unwrap_or_else(|| ss.find_syntax_plain_text())`.
**Lesson**: a highlight engine that resolves an unknown language by silently falling back to plain text turns "unsupported grammar" into "looks rendered but isn't" — the failure is invisible and per-language.
**Scribobulate**: build the engine's `SyntaxSet` from **`two_face::syntax::extra_newlines()`** instead of `SyntaxSet::load_defaults_newlines()` (`renderer::syntect()`). `two-face` embeds bat's vetted syntax dump, a **superset** of syntect's defaults: it keeps every bundled grammar (js/rust/python still resolve) and a…

## 163. Switching a `GtkLabel` to `set_markup` silently makes every interpolated string a Pango-markup injection/breakage surface — an un-escaped filename metacharacter renders the label EMPTY, with no crash
**Symptom**: adding a coloured "⚠" deleted-backing badge to a tab required a per-glyph colour, which a plain-text `GtkLabel` (`set_label`) can't express — so the tab label was converted to Pango markup (`set_markup`) with the "⚠" wrapped in a `<span foreground="#e5a50a">`. The badge itself renders fine.
**Root cause**: `gtk_label_set_markup` runs the string through `pango_parse_markup`, which treats `&`/`<`/`>` as entity/tag syntax. `set_label` does not — the two entry points look interchangeable (both "set the label's text") but have opposite escaping contracts.
**Lesson**: `set_label` and `set_markup` are **not** drop-in swaps — converting a label to markup silently makes every interpolated runtime string an escaping obligation, and the penalty for forgetting is a **blank/garbled label + a soft warning**, never a crash, so a happy-path test with an ASCII filename pass…
**Scribobulate**: the tab strip's label is now markup, so the single funnel that composes it (`window/tabs/documents.rs::tab_display_markup`) escapes the filename with `glib::markup_escape_text` **before** interpolation, and the pure label formula (`winstate::decisions::tab_label_markup`) takes the **already-escaped*…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-154); `winstate/decisions.rs::tab_label_markup` (pure, escaped-name + colour param) and its unit tests; `window/tabs/documents.rs::tab_display_markup`…

## 187. A byte range captured at build time and applied at click time is a bet, not a coordinate
**Symptom**: **Remove** on an annotation card, with unsaved edits in the document, deleted the wrong text. Twice more by the same mechanism: a comment committed over a range a re-render had moved, and every disclosure control in a split pane dying on a Ctrl+S.
**Root cause**: the mutation said *"delete bytes 6..32"* rather than *"delete this annotation"*. A range holds only against the string it came from, and this one crossed time in a closure with nothing re-establishing that the two were the same.
**Resolution**: carry the range with **the text that occupied it** — `docref::AnchoredSpan`, this tree's one held reference, whose rule has three clauses: an **ambiguity policy** per construct (nearest occurrence for a distinctive identity; refuse for one a document repeats); `None` obliging the caller to **re-derive the view**, never to do nothing; a CAM row per construction site, which check 24 gates.
**Lesson**: a bare integer is the one form of reference that cannot be checked, and an index into mutable content is a reference. Once it outlives the instant it was computed — a closure, a widget's state, a queued message, a row model — it needs an identity that can be re-established. A generation stamp is not one: it sees that something moved without naming what, so its mismatch arm can only refuse.

## 194. A shared per-line helper that hands out a RAW line makes every block transform blind to the container prefix — one rule, four copies to get wrong
**Symptom**: every block formatting command corrupted a blockquoted line. Heading 3 on `> Heading` produced `### > Heading`; Bulleted/Numbered/Task List on `> item` produced `- > item` / `1. > item` / `- [ ] > item`.
**Root cause**: the block formatters all opened by resolving a whole-line span through one shared helper (`text::block_span`) and then reading `span.text` and splitting it on `'\n'` — i.e. the shared layer handed each transform the **raw line**, prefix and content fused.
**Lesson**: when a family of transforms shares a helper, **what the helper hands back defines the blind spot they all inherit** — a raw line is a fused pair (container, content) and every consumer that treats it as content is wrong in the same way.
**Scribobulate**: the split happens once, in the shared block-span layer, and the raw form is sealed off. - `BlockSpan::lines()` yields `BlockLine { prefix, content }`, splitting on the one existing `quote_prefix_len` parser (`>` runs each with an optional single space; ASCII, so the byte split is char-safe).
**See**: TDD 10.20 (block commands inside a blockquote); MANUAL-TEST 10.20; `format::text` module docs ("The container-prefix seam"); the enforcement ladder is the gtk4-rs skill's GTK4Rs/AP-108/GTK4Rs/AP-130.

## 195. A decision driven off one parser's event stream cannot see the constructs a second tokeniser owns
**Symptom**: annotating a *partial* selection inside `==highlight==`, `~~strike~~`, `^sup^` or `~sub~` spliced the CriticMarkup **between** the delimiters — `a ==m{==ar==}{>>note<<}k== b` — markup that parses as neither construct.
**Root cause**: `copymap::balance_source_span` decided what to swallow by walking pulldown-cmark's event stream and matching `Code`, `Emphasis`, `Strong`, `Strikethrough`, `Link`, `Image`. The four failing constructs are **not pulldown constructs here**: pulldown has no highlight/mark option at all, and its caret/tilde flanking rules never match the tight Pandoc forms authors type, so this crate tokenises all fou…
**Lesson**: when a project parses *some* of its syntax with a library and *some* itself, every decision derived from the library's output inherits a blind spot exactly the shape of the syntax you own — and it fails silently, because your constructs are indistinguishable from prose in that stream.
**Scribobulate**: make the second tokeniser span-shaped and consult it. - `renderer::scan_script_spans(text) -> Vec<ScriptSpan { outer, inner, script }>` is now the primitive — the **single definition** of what these four constructs are — returning the whole-construct (`outer`, delimiters included) and content (`inne…
**See**: TDD 17.33 / 17.18; CAM Document Rendering row 3 (widened to name both tokenisers and both paths); `renderer::scan_script_spans`; `copymap::balance_source_span`; sibling pulldown quirks #66/#75/#147/#1…

## 196. A fallback keyed on a symptom, not a cause, silently swallows the next cause that shares the symptom
**Symptom**: found on the live display while verifying #194/#195, not by any test. An annotation's amber claim highlight covered the **whole** text run instead of the claim, on any line holding one of the four in-crate constructs.
**Root cause**: the cleaned-source→buffer mapper decided per content event, and gave up — tagging `(before, after)`, the whole event — whenever `buf_len != cleaned[s..e].chars().count()`. That branch is correct and necessary for a **synthesised** run: smart punctuation (`--`→`–`, `...`→`…`) and entities substitute characters, so there is no per-character correspondence and tagging half a synthesised glyph would b…
**Lesson**: when you write a defensive fallback, key it on the **cause** you are defending against, not on the observable that led you to it. A symptom-keyed guard is a permanent trap door: every later cause that happens to present the same way falls through it silently, and because a conservative fallback *deg…
**Scribobulate**: test the cause instead. `annotate::kept_chars` counts the chars of a run that actually reach the buffer (construct delimiters dropped, everything else 1:1) from the scanner's spans; the precise path runs whenever that count equals the event's buffer length — which **subsumes** the old 1:1 case (no c…
**See**: TDD 17.18 (claim extent, including the marker-stripped case); MANUAL-TEST 17.39; `annotate::kept_chars` / `map_cleaned_highlight_to_local`; siblings #194 (one rule, N copies) and #195 (two tokenisers,…

## 200. `GtkSourceIndenter` is unusable from gtk-rs — the subclass trampoline frees the caller's `GtkTextIter`
**Symptom**: implementing `GtkSourceIndenter` — the sanctioned, keystroke-only home for auto-indent behaviour (`is_trigger(view, location, state, keyval)` + `indent(view, iter)`, `GTK_SOURCE_AVAILABLE_IN_ALL`) — SIGSEGVs the app on the **first Enter**, with no warning, no panic, and an empty stderr.
**Root cause**: the binding's subclass trampoline takes the caller's **transfer-none** `GtkTextIter*` with `from_glib_full`, so the Rust wrapper owns it and frees GtkSourceView's own iterator when it drops (`sourceview5-0.10.0/src/subclass/indenter.rs:107-110`):
**Resolution**: don't use the interface from Rust at this version. Do by hand what it would have done, from the same place: a `PropagationPhase::Capture` `GtkEventControllerKey` on the view (GtkSourceView installs its own capture-phase key controller for exactly this purpose, `gtksourceview.c:1442-1443`), mirroring…
**Lesson**: the discriminator that saves the hour is **re-run the crash with an empty vfunc body**. A segfault inside a freshly written subclass reads as "my code is wrong" and invites a long bisect of one's own logic; if it still crashes doing *nothing*, the binding is the defect and the correct move is to rou…

## 228. A property implemented on one branch, documented as a property of the whole function
**Symptom**: fixing an unrelated format ambiguity made `the_marker_forgets_reports_that_no_longer_exist` fail — a test that had passed since it was written, over code the fix did not touch.
**Root cause**: `announce_unread_report`'s comment claimed the seen-marker was "pruned to reports that still EXIST, which is what keeps a set-valued marker bounded". The pruning lived in `seen_set`, and only in its **legacy-watermark** branch, where it is structural — a watermark is *evaluated against* the present set, so filtering by it is how that branch works at all.
**Scribobulate**: `src/forensics/report.rs` — `seen_set` applies one `extant` predicate on every branch and owns the bound in its own doc comment; the writer's comment now points at it rather than restating it.

## 233. Delegating a delimited format's unforgeable-terminator invariant to a third-party serialiser's escaping
**Symptom**: A frontmatter-style file format — a magic line, a TOML metadata block, a bare `+++` terminator, then a verbatim payload — silently truncated its payload when one metadata value (a filesystem path) contained a newline.
**Root cause**: The `toml` crate (0.8) chooses between basic, literal and **multi-line** string forms by an internal heuristic, and for any value containing a newline it selects a multi-line basic string — whose defining purpose is to reproduce those newlines verbatim:
**Resolution**: Enforce it twice — by construction, then by verification.
**Lesson**: **When you write down that a hazard is handled *by someone else's code*, that sentence is a hypothesis with a test attached, not a conclusion.** The tell is a design note that identifies a risk precisely and then discharges it by appeal to an upstream guarantee — the precision of the analysis lends…
**Scribobulate**: `src/swapfile/codec.rs` — `to_wire`/`from_wire` (construction) and the `encode` fence check (verification); the invariant is stated in the module doc. Sibling of GTK4Rs/AP-167, which came from the same feature: both are cases of a convenience API's advertised behaviour being narrower than its name.

## 250. A widget swapped in for one feature's sake moves its text out of every text-walker's reach
**Symptom**: The find bar reports "No matches" for a word the reader can see on the page. In a table, `| [Handbook](…) |` — a cell that is *nothing but* a link — is never found; the same word written as `see [Handbook](…) again`, in the cell beside it, is found normally.
**Scribobulate**: fixed at the root — every table cell, a link-only one included, is now a direct-child `GtkLabel` (`widgets::table::linkcell`, `cell_markup_label`), so there is no second shape for a walk to miss. `gtk4::LinkButton::with_label`/`::new` stay banned in `clippy.toml`, now pointing at the label route; `preview::cell_search_targets` walks direct children only.

## 255. A construct whose glyphs are buffered at its `End` event is not opaque — it is char-precise in a coordinate space nobody wrote down
**Symptom**: Selecting a couple of words inside a rendered code block and choosing Copy — from the context menu, the Edit menu, or Ctrl+C, all one `win.copy` action — put the **entire fenced block, fences included** on the clipboard.
**Root cause**: The copymap captures each render event's live buffer range as `(before, after)` around that event's processing. That is exact for every construct whose interior events insert their own glyphs — and a code block's do not: `Renderer` *accumulates* the body while the `Text` events go by (inserting nothing, so each captured range is **zero-width**) and flushes the whole block in one syntect-highlighte…
**Resolution**: `copymap::code_block_node` lays the interior events' source runs out across the `End` event's buffer range, in order, producing one leaf per run — and **proves the layout before trusting it**: the flushed char count must equal the body's, mirroring `insert_code_block`'s own rule (trailing blank line…

## 261. A derived-state hook installed at the producer misses the rebuild shape the producer also has
**Symptom**: a hook that keeps state consistent with the rendered document fires for every re-render *except* the one the feature exists for. The headless test — which drives the in-place re-render — passes; on the live display the same scenario leaves the state stale, silently, with no warning and no log line.
**Root cause**: "the preview was rebuilt" is **two code paths, not one**. Preview mode's external reload rebuilds by a wholesale render into a brand-new scroller/view widget; split mode re-renders the existing view in place. A hook installed on the in-place path is absent from the wholesale one, and nothing types, lints or tests the difference — the producer's shape is invisible from the hook's own site.
**Resolution**: move the hook off the producer and put it **immediately in front of its consumer** — here, reconcile the history against the live heading set inside the function that computes the two actions' sensitivity, so the reconciliation and the value it protects are computed in the same call and cannot disag…
**Lesson**: when a producer has more than one code shape, a hook on the producer is a latent regression — the next shape added will not have it, and a test written against the shape you are looking at will not notice. Prefer siting derived state in front of the
**Scribobulate**: `window/navhistory.rs`'s `reconcile_nav_history_headings` is private and called from `refresh_nav_history_actions` (before it reads `nav_can`) and from `traverse` (before it steps); `preview/render.rs` deliberately calls nothing.
**See**: kin GTK4Rs/AP-55 (the same two rebuild shapes, reached through a stale *signal* rather than a missing hook — different root cause, same architectural fact); TDD 23.14.

## 282. An operation counter is a complexity oracle only for operations you control

**Symptom**: a wall-clock growth-ratio guard flakes on shared CI. The obvious repair — count operations instead of time, machine-independent by construction — was built, and the resulting guard could not fail.

**Root cause**: two, and the second is the general one. The test corpus made the counted structure trivially small, so a linear and a logarithmic lookup performed identically. More fundamentally, the regression being guarded did its work **inside the standard library** (`str::find` scanning the source), which no counter in the project can reach: a reintroduction ticks the counter exactly as often as correct code, leaving the ratio linear while the run takes minutes.

**Resolution**: keep the **absolute** ceiling, which has wide headroom, has never flaked, and is what actually catches the regression; keep the ratio but time **equal work** on both sides (one `k·n` run vs `k` runs of `n`), so run-time-proportional noise cancels. More samples cannot fix it: a sample longer than a scheduler slice is preempted on every draw (MEASURED 10/10 at 8.0x; `src/testtiming.rs`).

**Lesson**: "count operations, not time" is right only where the operations that scale are **yours**. Before replacing a timing oracle with a counting one, ask where the work in the regression actually happens — if it is behind an API you do not instrument, the counter measures call frequency, which was never in question. A guard that cannot fail is worse than the flaky one it replaced (GEP-77).

**Cost**: an implementation, a mutation test, and a revert — cheap, and only because the mutation was run. Recorded because the reasoning is persuasive enough to be re-attempted. — Severity: Low

---

## 294. Letting a coverage ratchet be satisfied by widening the exclusion instead of testing the code
**Symptom**: a change whose only new logic was display-free and fully unit-tested still failed build-pipeline step 5. The feature's GTK wiring had landed in `preview/interactions.rs` — in scope, and 0% covered like every preview wiring file — while its decidable half sat in `codeview/`, which the scope regex exc…
**Scribobulate**: the pure geometry and the shared point-in-rectangle hit test moved out of the excluded `codeview/` tree into `affordance.rs`, where the gate counts them and their tests; `FLOOR` rose 77.72 → 77.75 in the same change, with the reason recorded beside it.
**See**: project-specific (process/tooling; the routing rule keeps these here). POLICY § Build pipeline (coverage ratchet) for the rule, `scripts/coverage.sh` for the floor, the scope and the per-module rationale.

## 298. A TIGHT list item's content arrives as bare inline events with no `Tag::Paragraph` wrapper
**Symptom**: an exported document breaks its lines after almost every token inside a numbered or bulleted list — `POLICY.md`, then a line break, then the next four words, then a break, then a comma on its own line. Only *inside* list items; the same prose at top level is fine.
**Root cause**: pulldown-cmark wraps a **loose** list item's content in `Tag::Paragraph` and a **tight** item's in nothing at all — the inline events arrive directly inside `Tag::Item`. A consumer that reaches for "no inline container is open, so start a paragraph" therefore starts a *new* paragraph for every inline event the item contains: one for the text run, one for the inline code, one for the link, one for…
**Scribobulate**: `src/export/walk.rs` carries an implicit-paragraph frame — `Open::ImplicitParagraph`, opened lazily by `Builder::push_inline` the first time an inline arrives with an empty inline stack, and closed by `Builder::flush_implicit` into the enclosing block frame.
**See**: project-specific; the fix and its rationale live in code comments at `src/export/walk.rs` (`Open::ImplicitParagraph`, `Builder::flush_implicit`, `is_block_start`).

## 306. A chooser's `set_current_folder` is best-effort and its failure is unobservable
**Symptom**: on Windows `set_current_folder` returns `Ok(())` and `current_folder()` reads back `None` **whether or not** the folder was honoured.
**Root cause**: the setter cannot report what the native dialog did with it, and the getter is not the check it looks like — it is **non-discriminating**, returning the same answer in both cases. Worse than an API with no check at all, because one appears to exist.
**Resolution**: nothing downstream may assume the dialog opened where it was asked. Treat the initial folder as a courtesy, never as state.
**Scribobulate**: `window::export::choose_destination` sets it through a discarded `let _ =` with the reason in a comment beside the call.
**See**: gtk4-rs skill → printing-and-export (GTK4Rs/AP-299).

## 309. A cancelled export destroys the destination, and the wreckage is a valid file
**Symptom**: cancelling a `GtkPrintOperation` export leaves a valid, readable, **partial** PDF at the destination, having already replaced whatever was there; it extracts cleanly and the extractor exits 0, so nothing signals that the previous file is gone. MEASURED (Windows, GTK 4.22.4): a 43,973-byte destination replaced by a 171,327-byte 51-page partial.
**Root cause**: `set_export_filename` hands cairo the destination directly — no temp-and-rename — and GTK opens, and therefore truncates, it **before the first page is drawn** (zero `draw-page` calls when the open fails). Cancel is a normal-completion path, not an error path, so the destruction is reached by pressing Cancel in ordinary use.
**Resolution**: never point an export sink at a user-visible destination. Render to a private temp file and publish atomically **on success only**, gated on the application's own page count rather than the toolkit's return value (GTK4Rs/AP-294).
**Lesson**: a write that begins by truncating its destination has no cancel path — "cancel" and "half-write" are the same outcome, and a valid-looking partial is worse than a corrupt one because nothing downstream complains.
**Scribobulate**: `atomic_io::AtomicPublish` holds the create-private-temp → write → publish sequence; `export_pdf` stages into it and promotes only on `Ok(Apply)` with `drawn == expected` (TDD 25.21).
**See**: GTK4Rs/AP-167 — the same family (a failure mid-write) and the same promote-only-after-a-complete-write rule, on the swap file; this entry's trigger is user intent, not a fault.

## 311. Which of two same-key bindings wins is a property of the BACKEND, not of the toolkit
**Symptom**: a fix, a TDD rubric and a module doc all state as settled fact that `GtkSourceView`'s `move-words` class keybinding beats a window `GAction` accelerator declared on the same keystroke while the view holds focus. On Quartz that is measured and true.
**Root cause**: **not established.** It wants the per-backend shortcut-controller phase; nobody has source-traced it. The Windows seat's reading is that this is GTK4Rs/AP-121's shape (a window accelerator at capture/global beating a focused widget's bubble-phase class binding) — **INFERRED**, and it does not explain why Quartz differs.
**Resolution**: **state the winner of a keybinding contest together with the backend it was measured on.** A sentence like *"the class binding wins that contest"* reads as a toolkit property and will be inherited as one; the next seat then either ports a fix nobody needs or skips one somebody does.
**Scribobulate**: TDD §4.13 and §23.6 and `macwordnav`'s module doc now scope the claim to Quartz and record the two contrary legs. `macwordnav` itself is unaffected and stays macOS-only — it pre-empts a binding that only wins there.
**See**: gtk4-rs skill → GTK4Rs/AP-121 (the shape this is INFERRED to belong to); routed to that skill's maintainer with the discrimination method and the control traps.

## 312. Repairing the editor buffer when the defect is in the source string — the preview never reads the buffer
**Symptom**: a document whose lines are separated by a bare `\r` renders in the preview as **one enormous heading** containing the whole file, and in the outline as one entry — while the **editor pane beside it shows the lines correctly** and the footer reports the right `Ln`/`Col`.
**Root cause**: GTK and pulldown-cmark disagree totally about whether a bare `\r` is a line ending. MEASURED on both platforms — Quartz/4.22.4 and X11/4.6.9: the buffer reports `line_count = 6` and highlights the heading, while the byte-identical string parses to one `Start(Heading(H1))` swallowing the document.
**Resolution**: repair at the **ingress doors**, never at a parse site. (a) `docio`'s readers, beside `without_bom`, at **all three** — the save guard and crash recovery each compare on-disk content against an in-memory baseline, so repairing one side of a comparison turns every save into a spurious "changed on dis…
**Lesson**: , five, and the first four are the same shape — *check the thing that PRODUCES the thing in front of you*. (1) **When two views of a document disagree, fix the input they SHARE**, not the one whose output is wrong; a green suite over a fix sited one layer too late is the expected outcome.
**Scribobulate**: `src/lineendings.rs` owns the rule and the display-free repair; `docio`'s three readers and `window::actions::load_into_editor` apply it, and that half is landed and sound.

## 314. Instantiating ANY `sourceview::Buffer` subclass corrupts the heap — and the backtrace is innocent
**Symptom**: `cargo test --features gtk-integration-tests --lib` SIGSEGVs in `g_slice_alloc`, reached from `g_main_context_dispatch` on `gtk4::test_synced`'s thread. The `--test gtk_suite` main-thread harness passes. The same tests pass when run **alone**, at any thread count. Only a FULL `--lib` run crashes.
**Lesson**: **when a crash lands in an allocator, the bug is somewhere you have already been.** Stop reading the stack and start deleting variables — and prefer an arm that makes the suspect *exist but do nothing* (E) over one that removes it entirely (A), because only the first distinguishes "this code is wron…
**Scribobulate**: it forecloses the mechanism that would have made ScrAP-312's clipboard repair impossible-to-break, so that repair ships as a marked fence instead — the two conditions it depends on are named at `lineendings::wire_paste_normalization`.

## 315. Laying a table out with tab characters — a tab ladder cannot express a column
**Symptom**: an exported PDF's tables look ragged and border-less, the same column starting at a different x on almost every row. All the text is present and correctly ordered, so it reads as a styling omission rather than a missing feature.
**Root cause**: `export/pdf.rs`'s `table()` joined each row's cells with `\t` and emitted one ordinary Pango paragraph per row. A tab advances to the next stop in a **fixed ladder** (24pt here), so a cell one character too wide shoves its neighbour a whole stop right.
**Resolution**: a measured column grid — `export/pdftable.rs`, display-free and unit-tested, decides widths from per-column max-content and min-content measurements; `pdf.rs` measures and inks.
**Lesson**: ScrAP-75 records that a hard TAB inside a GFM table breaks table recognition, and the fix normalises tabs away on the way IN. The export sink then chose tabs as its column mechanism on the way OUT.
**Scribobulate**: `src/export/pdftable.rs` (99.5% covered) + `export/pdf.rs`'s table path. Guards: `export::pdftable::tests`, `export::pdf::pdf_layout_tests`' table set — chiefly `every_row_of_a_table_shares_one_column_grid` — and `the_shared_rule_agrees_with_the_previews_own_fit_columns`, which runs identical inputs…

## 316. A repair handler on `insert-text` is also a handler on the UNDO machinery, and the divergence it causes is silent
**Symptom**: an undo puts back **different bytes than were deleted**, and nothing anywhere says so — no warning, no critical, no failing assertion. The history's own model still believes the original bytes were restored, so a later redo compounds it rather than exposing it.
**Root cause**: `gtk_text_buffer_history_insert` reaches the buffer through the **public** `gtk_text_buffer_insert`, so a handler that rewrites inserted text is on the replay path as much as on the paste path — it was never opted out of.
**Resolution**: Do not bracket: the no-lone-CR invariant outranks byte-exact undo of a sequence no buffer may legally hold, and the two only ever differ on a buffer that already violates the invariant.
**Lesson**: This was filed as not-currently-reachable on the strength of two lines appearing in the right order inside one function, with a comment saying the order was load-bearing.
**Scribobulate**: `src/lineendings.rs` — `new_editor_buffer` (the choke point, the only route to an armed buffer) and the private `wire_paste_normalization`; `window::tabs::lifecycle`'s `build_tab_editor` is its single production caller.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-303), read back from the installed copy of the skill on this host rather than taken on report; attribution there is split th…

## 324. A compiled-in asset resolved against a runtime directory is absent everywhere that directory isn't
**Symptom**: a shipped theme's sprite renders as its flat fallback on a fresh install, a developer build run with no user config, and a macOS bundle — no warning, no crash, green suite throughout. The reference was never invalid; it was simply never resolved.
**Root cause**: two ways a resource can be *named* — compiled into the binary, or read from disk — but only one way it was *resolved*: against a themes-file's own directory, a step that only the disk-file case ever ran.
**See**: kin to GEP-68/319/320/321 (this register's own family of checks/mechanisms that cannot go red for the right reason) — here the mechanism that went silently right was a *resolution step*, not a test…

## 329. A gate read through a pipe reports the pipe's last stage, not the gate
**Symptom**: a coverage ratchet was set, re-run to confirm, and reported green twice — while actually failing.
**Root cause**: The gate was invoked as `scripts/coverage.sh | tail -2; echo $?`. `$?` after a pipeline is its LAST command's status, so the `0` was `tail`'s; the output showed the gate's own summary lines, and only the VERDICT was substituted.
**Resolution**: invoke a gate directly and read its own exit status; where a pipeline is genuinely wanted, `set -o pipefail` first. Read a coverage floor from the column the gate reads, never the column the summary leads with.
**Boundary**: an ACTING verb can return 0 having done nothing (`codesign --sign` on an unrecognised bundle); follow it with the same tool's VERIFYING verb — GEP-52.
**Scribobulate**: `scripts/coverage.sh`'s header carries the column warning and its instances, and the floor is set from the Lines column and verified by running the script directly. The value lives only in the script (POLICY § Build pipeline).
**See**: cargo-llvm-cov and shell invocation. Kin — GEP-11's family (a green that means nothing) and GEP-4, the other way a coverage number misleads: 326 is a real…

## 334. A repaint failure that also happens in an unrelated application is upstream, and the platform seam it invites is the wrong response
**Symptom**: on a KDE/X11 desktop, toggling the system dark↔light theme leaves parts of the application drawn in the previous scheme until something forces a repaint.
**Scribobulate**: NOT fixed, and deliberately not investigated further (operator, 2026-08-28). The remedy it invites is a new `src/platform/linux/` portal seam to observe the desktop's appearance signal directly, which is a real module with a real maintenance cost, built to work around somebody else's bug.
**See**: kin to the `Upstream` scope rule in `sdd/ISSUES.md`'s header, which exists for the same reason — an upstream defect is not work waiting to be scheduled here.

## 338. A settle wait pointed at a value the code records SYNCHRONOUSLY ahead of the work it waits for — it observes a constant, reports converged on the first turns, and leaves fixed drains doing the real waiting
**Scribobulate**: `window::scrollsync`'s reading-position guards. `settled_top_line` polled `preview::preview_top_line` for four equal readings — but that call prefers `CodePreviewView`'s `restore_target_line`, a `Cell` the restore path writes synchronously (only the scroll itself is deferred). So it polled a value that was already final: MEASURED `polls=5 stable=4 converged=true` — the minimum possible turn count — on every call, on a healthy run and equally on a starved one that was reading line 0 off an unvalidated view. It also discarded `until_or_for`'s convergence flag, so a timeout was indistinguishable from success. The real waiting was being done by two fixed drains (400ms, 120ms) against a pane that moves through TWELVE distinct offsets over ~365ms on an idle box. Consequence: the one-time crossing cost wandered with host speed — 11 on Linux, 9/11/13/17/19 over ten macOS runs, 11-or-17 over twenty Windows runs, against a bound of 20 — and the hosted CI runner reached 25 and went red on a required gate immediately before a merge. Now `scrollsync::settle` samples the LIVE viewport (`view_top_offset`, mirroring the pane `content_reading_position` reads), waits for a quiet DURATION via `testpump::until_stable`, and asserts convergence with a message that says precondition rather than drift; the cost is 1 with zero variance on all three platforms. The fixture's own lines-per-section now derives the bound that was a literal 20.
**See**: kin — GEP-1 (a guard whose INPUT SET is not the thing it polices; this is the same defect one layer down, in what a WAIT samples), GTK4Rs/AP-122 (the sibling bound-unit error), GEP-1, GEP-5.

## 340. Splicing a region whose rendered content was written by an event ABOVE that region
**Symptom**: an UNSPACED `<details>` (rubric 2.26d) collapsed correctly and then could never be opened again — the fold splice deleted the body and wrote nothing back, and the heading below lost its block separator.
**Root cause**: a region render is seeded at the region's start and replays only the events BELOW it. An unspaced block's body is literal text inside the block's own opening raw-HTML event, which sits above the region — so the region walk never emits it, while a FULL render of either fold state is correct. Only the transition is wrong, which is why no unit test could see it and driving the running app could.
**Resolution**: the renderer records `DisclosureExtent::spliceable`, false for any frame that wrote literal text of its own, and `preview::splice` refuses before it touches the buffer so the caller's full re-render runs instead.
**Lesson**: a splice's precondition is not "I know the region" but "every character in the region is produced by an event inside it". Before adding a region-scoped fast path, ask which event WROTE the content — a construct whose rendering is emitted by an ancestor event is not spliceable at any region granularity.
**Scribobulate**: `renderer::DisclosureExtent::spliceable` (set in `renderer::start::record_disclosure_extent`), refused in `preview::splice::splice`; pinned by `preview::build`'s `an_unspaced_disclosure_is_not_spliceable_and_a_spaced_one_is`, which carries its own positive control.
**See**: GTK4Rs/AP-321 (the reader-position half of the same splice).

## 343. Enlarging a decoded `GdkTexture` to display a VECTOR image at a larger size
**Symptom**: an SVG diagram drawn above its natural size is soft, and the TEXT inside it — the part the reader enlarged it to read — goes first.
**Root cause**: two facts that only bite together. `GdkTexture::from_file` decodes a scalable source at its NATURAL size with no way to ask for another (GTK 4.6 falls through to `gdk_pixbuf_new_from_stream`, whose **no-op size callback** discards any size before librsvg sees it). And enlarging the result cannot be sharp: GSK 4.6 sets no cairo filter at all, so it lands on cairo's default `FILTER_GOOD`, and `gtk_snapshot_append_scaled_texture` is 4.10+.
**Resolution**: make the target size an INPUT to the decode — `Pixbuf::from_stream_at_scale(admitted_bytes, w, -1, true)` — banned outside `imagedecode` — which reaches librsvg's vector renderer (MEASURED: a 4× thinner anti-aliased fringe than a bilinear upscale). Take BYTES, never the path: the path-taking twin re-opens the file after admission, a check-then-use seam. Gate on `PixbufFormat::is_scalable()`, free in the header probe. Riders: pass ONE axis and `-1` (`preserve_aspect_ratio = false` letterboxes); cap the TARGET pixels (a `viewBox="0 0 24 24"` probes as 576 and bounds nothing); fall back to the natural-size decode on loader failure (the SVG loader is a separate package everywhere).
**Lesson**: a scaling defect that looks like a filtering problem is often a decoding problem one layer up: ask whether the SOURCE was resolution-independent and the decode discarded that — no work at the drawing end recovers it.
**Scribobulate**: `imagecache::loader::rasterize_vector` and `LoadedImage` (which carries the size at zoom 1.0 apart from the texture's own — for a re-rendered vector they differ); target bound `renderer::image::cap_raster`. Plain-gdk-pixbuf repro and measurements: `probes/svg-rasterise-rs`.
**See**: TDD 13.11; kin GTK4Rs/AP-58, GTK4Rs/AP-66; `sprite.rs` pre-resamples for the same GSK reason.

## 345. Judging a theme change on a machine that has ever run `install.sh`
**Root cause**: `$XDG_DATA_HOME/scribobulate/themes.toml` (row 2, from `install.sh`) merges over the compiled-in file **per key**, so an edit half-lands and reads as a rendering bug. Verify under a scratch `XDG_DATA_HOME`, or refresh that copy and its `sprites/`.
**See**: `sdd/THEMING.md`; kin GTK4Rs/AP-173; GEP-57.

## 346. Assuming every `U+FFFC` in the preview buffer is an anchored child
**Root cause**: decoration enters as a Pango shape (`insert_paintable`), or animated as an anchored widget — same character, opposite verdict. Ask the buffer (no anchor, or one `copymap::mark_decoration_anchor` marked), never render-time offsets; splices re-base them.
**See**: `copymap::debug_verify`; kin ScrAP-74; GEP-4; GEP-19.

## 347. A legibility gate measuring an ink against a surface its level can never show
**Root cause**: `band_surfaces` short-circuited to the page for ANY sprite-bearing band, but a band degrades to its FILL, rejecting a legible ink at 1.03:1. Deleted — `Band::without_sprite` is that chain.
**See**: kin F-AP-B-302; GEP-6.

## 349. Judging whether a cursor took from a screenshot — and letting an uncontrolled variable name the defect
**Root cause**: pixels cannot NAME a cursor, so "sometimes the hover cursor does not take, on some elements" survived months and three hypotheses. It was neither intermittent nor per-element: ONE variable — whether the pointer was inside the window's frame when it MAPPED — decided the whole surface, and which runs fell either side of it produced the per-element table. Read identity by name (`probes/quartz-cursor-identity.m`; `XFixesGetCursorImage` on X11), and validate the reader against two genuinely different LIVE windows first — both seats built readers that answered "always arrow" for every input, which is indistinguishable from the defect under test.
**Scribobulate**: `src/platform/mac/pointercrossing.rs` buys the crossing events GDK's Quartz backend never delivers. Load-bearing on GNOME/gtk commit `f207402228`'s coupling of input regions to tracking areas, NOT a documented contract; nothing in-repo can pin it and `tests/MANUAL-TEST.md` 7.25m is the only cover. Scope is TOPLEVELS — popovers and text handles set their own regions, which is also the control.
**Also**: a 20-point GRID scan is not 20 readings about the button — every point after the first re-enters an already-hovered region, so a grid reported the same affordance healthy on one platform and broken on another, both truthfully. A procedure that destroys the condition under test while looking thorough (TDD 2.3b, 7.25).
**See**: GEP-13; `probes/macos-cursor-map-latch.c`; GTK issue #6134.

## 350. A comment that tells you NOT to do something — load-bearing in the direction nothing checks
**Root cause**: a wrong POSITIVE claim is caught the first time somebody relies on it. A wrong "you can skip this" is falsified only by someone independently deciding to do the thing anyway — and the comment exists to talk them out of exactly that. No code path fails, no test goes red, and nothing ever re-reads a comment. Both instances were prose discouraging the one investigation that would have found the bug.
**Scribobulate**: `refresh_hover_for_scroll` (now `refresh_hover_after_paint`) carried "the cursor is wrong only briefly, which the next motion event corrects" — inside the very function that exists because a STATIONARY pointer produces no next motion event. Measured false and kept marked false rather than deleted, because someone reasoned there once and will again. Same week, same shape: `pointercrossing.rs`'s header claimed the input-region coupling was "pinned by the check named below" and named none, which tells a reader a guard exists and so stops them writing one.
**See**: kin ScrAP-349; GEP-12.

## 351. A gdk-pixbuf loader module that retains every decode — invisible to refcount assertions, visible only as slope
**Symptom**: ~12 MB per render of one animated WebP, never returned (104 MB → 2708 MB over 98 headless renders). Every gate green, every object we held proven finalized.
**Root cause**: not ours. `webp-pixbuf-loader` (0.0.5-5~22.04.1) over-references `GdkPixbufWebpAnim` through a `GdkPixbufWebpAnimIter` it never releases, so the anim keeps the decoded frame and the whole file buffer alive; its refcount is 2 where GIF's is 1. Per-loader-module, not per-format: a STATIC WebP of the same size is flat, GIF is clean. A 20-line C program making only our render's two calls reproduces the app's per-iteration leak to within 1%.
**Resolution**: own the decode (`richimg`) so the module is never entered, and gate the class rather than the instance — GROWTH over N renders after discarded warm-up, measured as what one allocation cannot explain (TDD 6.6, 6.11), plus a finalization half (TDD 6.7, 6.9). Caching only reduces how often an unfixable leak is invoked.
**Lesson**: **a leak can be entirely real and invisible to refcount assertions** — our `GdkTexture` was weak-ref-verified finalized 40/40 while the retaining owner sat a layer below it, in a C module we never named as a dependency. A gate watching the objects you own cannot see a leak owned by something you merely called; only growth over many repetitions can. And freed memory is not returned memory on any target platform, so a single-shot "render, free, assert it came back" cannot pass even on correct code.
**Scribobulate**: `src/imagedecode/` is the only decode route, with `clippy.toml` banning GTK's encoded-image entry points elsewhere; the gate is `src/memgate/` (the growth predicate in `memgate::growth`, sampler field `footprint`, never `rss`), its own pipeline step 5b.
**See**: TDD 6.6–6.9; ScrAP-352 (why the plainest GTK call reaches that module's leaking branch at all); kin GTK4Rs/AP-66.

## 352. `GdkTexture::from_file`/`from_bytes` reach a pixbuf module's INCREMENTAL path, not its one-shot `load`
**Symptom**: `gdk_pixbuf_new_from_file` on an animated WebP errors ("Cannot create WebP decoder") while `GdkTexture::from_file` on the same bytes succeeds — and leaks. Two entry points to one module behaving as two decoders made the defect look format-specific and unreproducible by the obvious API.
**Root cause**: two code paths in the module. GTK 4.6 decodes PNG/JPEG/TIFF itself and falls through to `gdk_pixbuf_new_from_stream` for everything else, driving the module's `begin_load`/`load_increment`/`stop_load` trio; `new_from_file` calls its one-shot `load`. A bug in only one of the two is reachable from the plainest GTK call and absent from the call you would reproduce it with.
**Resolution**: measure the ENTRY POINT the application actually uses, each route its own subject: here `new_from_stream`/`GdkPixbufLoader` leaked 11.7–12.9 MB/call, `Pixbuf::file_info` 2.31 MB, `gdk_pixbuf_animation_new_from_file` (width/height only) was flat, and `PixbufAnimation::static_image` leaked the same AND SIGSEGV'd on truncated input. The two leaking calls were super-additive (2.31 + ~12.2 alone, 22.0 together).
**Lesson**: "the library decodes this format" is not one behaviour. Before blaming a format or a library, establish WHICH path your call takes into it — a one-shot and an incremental API can disagree about errors, leaks and crashes on the same bytes.
**Scribobulate**: the SVG dimension probe that used the animation API for this reason is gone; sizing reads `imagedecode::probe_vector_dimensions`, and every raster decode goes through `src/imagedecode/`.
**See**: kin `GTK4Rs/AP-66`, GTK4Rs/AP-66, GTK4Rs/AP-311; ScrAP-351 is the leak this asymmetry hid.

## 354. A reachability probe whose PRECONDITION names an asset that does not resolve — the key reads as reaching nothing
**Symptom**: the sink sweep reports a freshly added key as reaching NO surface, the verdict it gives a key nobody wired. It was wired on all three, and the sweep was right about what it measured.
**Root cause**: a gated key is probed with the TOML its `Reach::needs` states, and that TOML has to WORK. Two anchor keys declared a `needs` naming `sprites/x.png`, which resolves to nothing — so the scene it was to supply never existed, and moving that scene changed no output.
**Resolution**: a `needs` names something that resolves (`sprites/copper-plate.png`, the compiled-in reference the sweep's sprite probe uses for this reason). Read a neighbouring key's `needs` before inventing one.
**Lesson**: a precondition is part of the instrument. A probe that fails because its own setup was inert accuses the subject, specifically enough to be believed — which is what sends you rewriting correct code.
**Scribobulate**: `theme::tests::sinks::every_declared_key_reaches_every_surface_it_claims`.
**See**: kin GTK4Rs/AP-252 (a setup step that silently does not take effect, one layer out in a driven UI).

## 355. Solving a themed fill against the surface it is MIXED from rather than the page it is READ on
**Symptom**: a quote panel's stripes, solved to an equal perceptual distance (ΔE ~11) from the panel's ground, read as bands of bare page — twice, on two swatches, each rejected by eye after the numbers agreed.
**Root cause**: the ground `#131c52` and the page `#101a4d` sit 2.1 ΔE apart, so they look interchangeable and are not — against the PAGE the failing stripes measured 9.8, the tile's weakest. And an equal ΔE says nothing about WHERE the distance went: one swatch spent it on lightness (a lighter patch of page), the next inside the page's own blue (a navy patch of page).
**Resolution**: measure every fill against the page the reader sees it on, then LOOK. The lever for a panel is its ΔE target re-solved for every swatch at once, never one ratio nudged by hand.
**Lesson**: perceptual distance is necessary, not sufficient, and the reference is half the measurement.
**Scribobulate**: Candy's `blockquote_bg_sprite`, whose derivation comment in `data/themes.toml` carries the ratios and why three swatches are absent.
**See**: TDD 18.59.

## 356. Deriving a text run's own fill from the PAGE when the preview draws a surface behind it
**Symptom**: inline `code` in a Pixel Quest h2 was a pale blue chip carrying the heading's cream ink — one unreadable word (~1.3:1) inside a legible heading; the same clash on every banded level and the quote panel, and no chip at all in a table cell.
**Root cause**: the fill was `mix(page, body_ink, 0.08)`, right only where the page IS what is behind the run — a band, a panel and a header fill are all drawn over it. And a `GtkTextTag` setting a background and no foreground is not self-contained: GTK resolves each attribute to the highest-priority tag that SETS it, so the ink stayed the heading's while the fill answered to the page (kin GTK4Rs/AP-84).
**Resolution**: the chip is a relationship — *this surface, 8% toward the ink that surface carries* — resolved once per surface for every sink, one tag per surface because a tag carries one background and cannot ask what is painted behind it. An unknown surface (a tile) gets NO chip rather than the page's.
**Lesson**: a decoration derived from the page inherits the page's assumption that nothing else is drawn there; and toward-the-ink is a move DOWN the ink's own contrast, so a surface already at the floor must be tinted the other way.
**Scribobulate**: `palette::codechips` (resolution + precedence), `tags::CODE_INLINE_SURFACES`, `pangospan::code`, `export::html::code_surface_css`.
**See**: TDD 18.61; kin ScrAP-355, GTK4Rs/AP-84.

## 359. A pixel test that counts an exact colour over antialiased text — a glyph fringe can forge it
**Symptom**: a PDF test that counts rows of an exact marker colour on a rasterised page failed intermittently for weeks, only inside a full parallel suite run (locally and on CI), with a few short marker-coloured runs inside the quoted text; never on demand. Dropped from ISSUES once for not reproducing, and it recurred.
**Root cause**: the host's subpixel (LCD) text antialiasing darkens each glyph edge one channel at a time, and over a magenta panel (255,0,255) an edge darkened only in blue IS pure red (255,0,0) — the marker. Which edges land exactly there varies run to run under parallel load. Asking pangocairo/cairo for greyscale antialiasing (on the layout context or the cairo context) was measured to change nothing; why was not established.
**Resolution**: a marker no darkened edge can produce — yellow (255,255,0) over magenta, whose green is 0 and which ink only darkens. Found by saving the asserted page on every run and diffing a failing page against a passing one: only a few letters' edges differed.
**Lesson**: an exact-colour oracle on a raster that also carries text must use a colour outside everything the text's antialiasing can produce over that background; and keep the rendered output of an intermittent pixel test, because a picture shows in seconds what numbers about it hid for weeks.
**Scribobulate**: `export::pdf::measure::tests` — the blockquote tiling test's `MARK` and its `KeptPage` (pages under `target/test-artifacts/`).
**See**: GTK4Rs/AP-56 (headless pixels prove less than they look).

## 360. Assuming two libraries' renames end in the same Windows call — `std::fs::rename` is not `MoveFileExW`
**Symptom**: a code comment on the snapshot promote said Rust's rename and GLib's both end in `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, so a rename over a file another process holds open must be refused. The design and two suspected causes of a flaky test rested on that.
**Root cause**: modern `std::fs::rename` first renames with POSIX semantics (`SetFileInformationByHandle`, `FileRenameInfoEx`), which replaces a file that has open readers. MEASURED rustc 1.97.1 / Win10 19045 / NTFS, held reader on the destination: `std::fs::rename` refused 0 of 1000 (0 of 20000 under a concurrent reader), raw `MoveFileExW` refused 1000 of 1000 with `ERROR_ACCESS_DENIED`. A fallback to `MoveFileExW` where the filesystem lacks POSIX rename was not probed.
**Resolution**: the comment now states what each call does, with the measurement, and keeps the refusal handling for the unprobed fallback.
**Lesson**: "both end in the same system call" is a claim about a library's implementation, which changes between releases; measure it. The measured share-mask half of the same comment held; only the inferred half was wrong.
**Scribobulate**: `window::swap::promote_snapshot` (its doc comment).
**See**: GEP-83 (judge a promote by what it decided, not by the destination's bytes).

## 361. A command that scrolls the split pane the reader is NOT in — the scroll sync puts it straight back
**Symptom**: in split mode with the preview focused, Find's Next/Previous selected each match in the editor, but neither pane scrolled; the view stayed where it was. No warning. With the editor focused, the same steps scrolled both panes.
**Root cause**: two layers. Find chose its pane from the view mode alone ("editor unless pure preview"), so in split it always searched the editor, whichever pane had focus. And the split scroll sync takes its lead from the focused pane: when the editor's adjustment changed, the sync re-projected the leading preview's position back onto the editor. MEASURED (Linux, headless GTK test): preview focused, the editor's selection moved to lines 204 and 604 while its scroll value stayed 0 throughout; editor focused, it went 0 → 3305 → 10505 and the preview followed.
**Resolution**: one rule picks the pane, `winstate::find_pane(mode, focused)`. In split it uses the sticky focused pane (TDD 9.25), the one Copy already follows. Search, count, highlights and stepping all act on that pane, so a step scrolls the pane that leads the sync. Moving focus to the other pane with the bar open re-runs the search there.
**Lesson**: in split mode, a programmatic scroll only sticks on the pane that leads the sync. A command that acts on one pane must pick it from the focused pane, not the view mode, and scroll that pane. To move the other pane, make it lead first.
**Scribobulate**: `window::find::find_target`, `winstate::viewmode::find_pane`, `window::editbar::focusgate::sync_gate_to_pane_focus`; the sync in `window::scrollsync`. Tests: `window::find::splittests`. Recurred in Undo/Redo, which scrolled the editor whichever pane led (test `undo_and_redo_reveal_the_change_in_split_mode_when_the_preview_drove_last`). After that second occurrence the rule became a seam: `window::panescroll` is the one route for a navigation scroll, it makes the moved pane lead first, and `clippy.toml` bans scrolling the editor with `farscroll::scroll_to_mark_when_ready` directly.
**See**: GTK4Rs/AP-70 (a multi-pane action must target the focused pane); GTK4Rs/AP-16 (split sync picks its driver by input source).

## 362. An emoji with a variation selector, or newer than Emoji 12, in a theme symbol — Windows draws a missing-glyph box
**Symptom**: on Windows, the theme pickers showed a missing-glyph box for Pixel Quest (`🕹️`) and, on Windows 10, for System (`🪟`). Linux and macOS drew both.
**Root cause**: two causes. (1) Pango on Windows draws ANY character followed by U+FE0E/U+FE0F as the missing-glyph box, even one it draws in colour bare (📖 too); `pango-view` logs "couldn't load font 'emoji …'". Without the selector a text-default emoji (`🕹` U+1F579) does draw, but in a monochrome symbol font; naming `Segoe UI Emoji` as its family draws it in colour, with or without the selector (pango-view only, not tried in-app). MEASURED Pango 1.57.1 / GTK 4.22.4 / Win10 19045. (2) Windows 10's Segoe UI Emoji stops before Emoji 13, so `🪟` U+1FA9F has no font at all; Windows 11's has it.
**Resolution**: on Windows only, the picker symbol drops every variation selector and prefers the theme's `symbol_windows` key when stated (System states `💻`, Pixel Quest `🎮`: both Emoji_Presentation=Yes, Emoji ≤ 12). Linux and macOS draw `symbol` as written — the operator ruled they must not wear Windows' substitute.
**Lesson**: a glyph the app shows must be checked against Windows' fonts and Pango backend separately; "it draws on Linux" says nothing about either. A user theme may still state an Emoji 13+ symbol with no `symbol_windows`; accepted, since most users stay on the built-in themes.
**Scribobulate**: `theme::spec::ThemeSpec::own_symbol_for`; key `symbol_windows` in `theme::keys`. Test: `theme::tests::system::picker_symbols_suit_each_platform`.

## 363. A strong `TabState` capture in a widget the tab bar owns — the tab reaches the bar, so it is a cycle
**Symptom**: a window closed with a tab's right-click menu open stayed alive: the tab bar, the menu and the window chrome were never freed. Nothing was visible; only a weak-reference test of the window-close path showed it.
**Root cause**: every tab-menu row's closure captured the tab `#[strong]`. A `TabState` reaches its window's chrome, the chrome reaches the `TabView` and its bar, and the bar hosts the menu, so each row closed the loop `TabState → chrome → TabView → bar → menu → row → TabState`. Any widget parented under the tab bar or the chrome is on that loop.
**Resolution**: the rows capture the tab `#[weak]` and do nothing when it is gone; one shared window-close guard test now runs over the document, tab and toolbar menus.
**Lesson**: in this tree, a closure on any widget inside a window's chrome must hold a `TabState` (or the window) weakly — ownership runs from the tab outward to the chrome, never back. The cycle shows only on the path nobody exercises by hand (closing a window with the menu open), so the guard test is what catches it.
**Scribobulate**: `window::tabs::contextmenu::show_tab_context_menu`; test `closing_the_window_with_the_tab_menu_open_releases_the_menu`. Shape: GTK4Rs/AP-63, GTK4Rs/AP-140.

## 364. An open card that re-points only on scroll — a render in place moves its annotation under it
**Symptom**: in Side by Side with a comment card open, a live reload that inserted text above the annotation moved the chip about 150 px down while the card stayed put, pointing at other text. No input was involved, so nothing drew attention to it.
**Root cause**: the card's rectangle was re-derived only on scroll, and its identity (start offset plus marker indices) was fixed at open. A render in place rebuilds the markers without scrolling, so after an edit above it both are stale, and the indices can name a different annotation. The register had guessed the anchor widget reallocated; it never moves, so re-placing on allocation would have fixed nothing.
**Resolution**: every `set_markers` re-keys the open card onto its annotation in the new render by construct text and neighbours, then re-points it after the next paint; a card whose annotation is gone is dismissed.
**Lesson**: anything that floats over the rendered document and holds a position or index into it must be re-derived at every render, not only at every scroll: reloads, side-by-side edits, folds and theme or zoom re-renders all rebuild the text in place.
**Scribobulate**: `codeview::cardfollow`, called from `CodePreviewView::set_markers`; tests `the_card_follows_its_annotation_through_a_render_in_place`, `the_card_goes_away_when_a_render_drops_its_annotation`. CAM Document-Reference row 17. Deferral: GTK4Rs/AP-142. Diagnosis: GEP-15.

## 365. A pasted fence that never closes re-pairs the document's own fences — each mask check then finds one comment more
**Symptom**: a comment holding a stray backtick rendered as literal text, with no marker and missing from the annotations list, but only when four or more fence-holding comments followed a document fence that followed one.
**Root cause**: in pulldown-cmark's raw parse, an unclosed fence pasted into a comment pairs with the document's next opening fence, which turns the document's closing fence into an opener that runs to the end of the file. The lexer trusted that block, so each mask check uncovered just one more fence comment; four rounds ran out, and the fallback then lost the stray-backtick comment.
**Resolution**: once a check sees a block opening inside an unmasked comment and running past it, the next mask takes every later fence-holding comment at once, and the following check confirms it; the parse bound is unchanged.
**Lesson**: a settle loop whose budget grows by one item per round needs a one-step jump wherever one item can hide the next; otherwise the round count scales with the document, not with the ambiguity.
**Scribobulate**: `annotate::written::settle` (`escaping_block`, `fence_bodies_after`); tests `pasted_fences_after_a_document_fence_settle_in_one_step`, `the_manual_pasted_fence_document_keeps_every_comment`; TDD 17.61.
