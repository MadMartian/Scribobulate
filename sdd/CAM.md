# Change Accountability Matrices (CAM)

A CAM is a completeness checklist for a *category* of change. When a change falls
into a category below, it must **account for every applicable cell** in that
category's matrix — every surface it must appear in, every context it must work
in.

**A CAM catches *latent* gaps, not blocking requirements — that distinction
decides what belongs in a matrix.** A latent gap is one the happy path hides: the
feature works where it was built and *looks* finished, but a surface, context, or
mirror was silently missed — a command that works from the menu but was never added
to the formatting overlay or given an accelerator; markup that renders in the body
but not inside a table cell; a derived view that agrees with the document only until
the next tab switch. Nothing fails loudly; it ships looking fine and surfaces as a
bug report later, which is exactly why a checklist is needed. A **blocking**
requirement is the opposite — without it the feature is dead and you notice
instantly (no parser for the syntax ⇒ nothing renders; no `GAction` ⇒ nothing
dispatches) — so it needs no matrix cell to be remembered; it enforces itself. Put
latent completeness obligations in a CAM; leave blocking mechanics out, or a matrix
bloats with cells that never catch anything.

These matrices are prescriptive — they are part of the development rules. They
live here rather than inline in [`POLICY.md`](POLICY.md) because they are long and
consulted as a unit; POLICY states the binding rule (a change in a CAM's category
must satisfy every applicable cell), and this document holds the matrices
themselves.

| # | Matrix | Governs |
|---|--------|---------|
| 1 | Rules that govern all CAMs | Cross-cutting rules every matrix below obeys |
| 2 | Action CAMs — command surfaces | Where a command must appear (edit / format / other action) |
| 3 | Document Rendering CAM | Every context a markup/rendering feature must hold in |
| 4 | Derived-view CAM | Surfaces that mirror document state and can silently go stale |
| 5 | Reading-Position Preservation CAM | Every event that perturbs a text pane's viewport and must preserve the reading position |
| 6 | Document-Reference CAM | State that points INTO the document and must survive the document changing |
| 7 | Deferred-operation CAM | Work whose completion lands later (every document read and write), and everything that can change while it is out |
| 8 | Status-notice CAM | Transient status-bar notices, whose retraction must survive the holder being destroyed or moved |
| 9 | Document-Identity CAM | State keyed on a document's PATH, and the events that change it |
| 10 | Hot-path CAM | Handlers on a signal that fires continuously, whose cost is invisible on a small document |
| 11 | Multi-surface setting CAM | A setting the reader can change from more than one surface (menu bar, a context menu, a persistent control), and every member and rule it must carry to each |
| 12 | Granted CAM exceptions | Operator-approved deviations, recorded so they are not re-litigated |

---

## Rules that govern all CAMs

- **A change may belong to more than one CAM and must satisfy each.** Annotate,
  for example, is all three: an Edit action, a Document Rendering feature, and —
  because the annotations viewer projects it — a Derived-view change.
- **Every applicable CAM cell must be covered by a `tests/MANUAL-TEST.md`
  check.** This is where the matrix gets teeth: build-pipeline step 6 already
  requires a manual-test edit for any behaviour change — for a CAM change, derive
  those checks *from the cells* so no cell ships unverified. The manual-test plan
  keeps its own procedure and format; the CAM only dictates which checks must
  exist.
- **An operation that spans a suspension point names its subject once, up front.**
  Every matrix below assumes an operation acts on the thing it was invoked for, and
  the main loop running mid-operation is what breaks that assumption: an `await` on
  file I/O, a modal dialog's response, a debounce timer, a deferred idle. Across any
  of them the ambient answers — *which tab is active*, *which window is focused*,
  *which mode are we in*, *which status stack did this notice come from* — may all
  have changed, and re-asking gives a confident answer about something else. So
  resolve the subject when the user acts, carry it (an `Rc<TabState>`, a
  `StatusCtx`, an `AnchoredSpan`), and split the completion into work that belongs
  to the **captured subject** and work that belongs to **whatever is on screen** —
  both exist, and conflating them is the defect. Two matrices already record this
  rule in their own terms (Status-notice: *"capture the stack you pushed to; never
  re-resolve one at retraction time"*; Document-Reference, whose whole subject is a
  reference held across time); it is stated here because it governs all of them and
  because making an operation asynchronous introduces it wholesale into code that
  never had it (GEP-43).
- **A new row obliges a back-sweep of what already shipped, or an explicit record
  that it was not swept.** A CAM is consulted when a change lands, so a row added
  today is applied to future changes and to nothing else — every feature that shipped
  before it keeps whatever gap the row exists to catch, and no gate will ever ask.
  The matrix then reads as though the whole codebase satisfies it, which is the one
  claim a checklist must not make falsely. This is not hypothetical: Document
  Rendering row 8 (find/search) was written 2026-07-13, while the link-only table cell
  (then a `GtkLinkButton`) it would have caught shipped in the initial commit and its find path a week later —
  so find was blind to link-cell text for six weeks *after* the rule requiring
  otherwise existed, and it took a user report (ScrAP-250). When adding a row, sweep
  the features already in the category, fix or log what it finds, and note the sweep's
  extent in the row's anchor column. Where a sweep is too large for the change adding
  the row, say so in the row rather than leaving the reader to assume it happened.
- **Exceptions must be requested and explicitly green-lit by the operator.** A
  granted exception is recorded in the [Granted CAM exceptions](#granted-cam-exceptions)
  list at the foot of this document, not inline with the matrix it modifies —
  POLICY states the rule; that list records the approved deviations so they are
  not re-litigated each session.

## Action CAMs — command surfaces

Three command categories share one matrix. Each column lists the surfaces a
command in that category MUST appear in, plus two invariants that hold for all
three. **The two invariant rows are not new rules** — they are the existing "One
`GAction` is the single source of truth for every command" architecture rule in
POLICY.md (and ScrAP-9); this matrix is that rule's checklist face, so the
two never drift.

| Obligation | Edit action | Format action | Other action |
|---|:---:|:---:|:---:|
| Menu-bar item | ✓ (Edit menu) | ✓ (Format menu) | ✓ (relevant menu) |
| Toolbar section | ✓ | ✓ | ✓ |
| Context menu | ✓ | — | — |
| Formatting overlay | — | ✓ | — |
| Accelerator surfaced everywhere it is mirrored — menu hint, toolbar tooltip, **Keyboard Shortcuts help window** (commands that *have* an accelerator only) | ✓ | ✓ | ✓ |
| Single `GAction` source of truth | ✓ | ✓ | ✓ |
| Consistent enabled/sensitivity across all its surfaces | ✓ | ✓ | ✓ |

The **accelerator row is a reference row** — like Document Rendering CAM rows
9–10, it defers to a gate that already owns the concern rather than restating it.
Every command's accelerator flows from one source-of-truth table (`FILE_CMDS` /
`EDIT_CMDS` / `FORMAT_CMDS` / `VIEW_CMDS`, or `INLINE_ACCEL_CMDS` for a command
with no Cmd-table row), and `setup::register_accelerators` (the binding), the
menu hints, the toolbar tooltips, and `shortcuts::interface_xml` (the Keyboard
Shortcuts help window) all read that same table — so a table-backed command
cannot advertise a key it doesn't bind, or bind one it never shows (QA M-4). The
cell earns its place because the guarantee is only as wide as the tables: an
`INLINE_ACCEL_CMDS` row tagged with a group heading the help window doesn't
render, or any future off-table accelerator path, is *bound but silently absent
from the help window* — `interface_xml` renders a fixed set of group headings, so
a novel group falls through. This is the latent gap the row guards. It is
verified headlessly, not by a manual-test cell: `shortcuts.rs`'s
`every_inline_accel_cmd_is_displayed` asserts each inline row's label and
canonical accelerator reach the generated XML, and the `#[gtk::test]`
`registered_accels_match_the_inline_table` asserts what is bound equals what is
displayed.

### Toggleable actions — one further obligation, gated on behaviour not surface

A **toggleable** action *applies-or-removes* a markup: the inline wraps
(bold, italic, strikethrough, code-span, `==highlight==`) and the block formatters
that strip-if-already-present (heading, quote, ordered/unordered/task list, code
block). The gate is the behaviour, which is why this is prose below the matrix
rather than a fourth column — a toggleable can be a Format action or, in principle,
an Edit one. A toggleable action must, beyond appearing on every surface above:

- **Reverse on reapply.** Invoking it on markup it already produced removes it
  rather than doubling it (`**bold**` → `bold`; a second Heading-2 clears the
  `##`). The apply path is the happy path and works on its own; the **toggle-back is
  the latent half** that silently diverges (`****bold****`) — it ships looking fine,
  so it is the cell that needs the checklist.
- **Detect the applied state wherever it sits.** Inline: recognise the markers
  whether they fall *inside* the selection or *immediately outside* it. Block:
  recognise a line that is *already* prefixed. Toggle is idempotent from either
  starting point.
- **Behave consistently on an empty selection** — insert the marker pair (or line
  prefix) and park the caret to continue typing, the same as every sibling
  toggleable, rather than doing nothing or stranding a lone marker.

These facets live in the pure, GTK-free `format` transforms (`format::apply` →
`inline::apply_inline`, `heading`, `quote`, `list`, `codeblock`), so they are
primarily unit-tested there; the CAM's manual-test obligation is that the toggle
holds *from each surface* (menu / toolbar / overlay), not only in the unit layer.

A pure-insertion action (Insert Link/Image/Table, Horizontal Rule) has no reverse
and is exempt from all three. **Active-state indication** — showing whether a
toggleable is currently *on* at the caret (a pressed toolbar button, a checked menu
item) — is deliberately **out of scope**: `win.format` is stateless and the format
toolbar buttons are plain buttons, so there is no active state to keep in sync.
Adding caret-aware active state would be a feature decision, not a completeness
cell, and is not required by this matrix.

### Uncommon commands — outside this matrix by design

A command that is genuinely peripheral — infrequent, not part of the primary
editing/reading loop — does not enter the Action CAM at all: it owes no toolbar
button, no context-menu entry, no formatting-overlay presence. This is a
**category**, not a per-command deviation from the matrix above, so it is
recorded here as a definition rather than in [Granted CAM
exceptions](#granted-cam-exceptions) (which records departures *from* a
category a command IS in). It is built as an ad-hoc menu item outside the
`FILE_CMDS`/`EDIT_CMDS`/`FORMAT_CMDS`/`VIEW_CMDS` tables — a table row
auto-generates a toolbar button — but still driven by ONE `GAction`, so the
single-`GAction` and consistent-sensitivity rows still hold, and an
accelerator (if it has one) still flows through `INLINE_ACCEL_CMDS` so it
reaches the menu hint and the Keyboard Shortcuts window like any other.

Current members:

| Command | Why uncommon |
|---|---|
| `win.export` (PDF/HTML) | peripheral to this app's primary audience (developers reviewing agent-written prose); `File ▸ Export` submenu only |
| `win.save-all` | **operator, 2026-09-02** — its toolbar button sat confusingly adjacent to Save/Save As; demoted to a menu item + its existing accelerator |
| `win.pick-find-history` | a find-bar field's recent entries. Its only surface IS the drop-down beside the field it fills; a menu-bar item for it would have to enumerate a per-tab list of arbitrary strings in a shared model, and there is no command to name — the row IS the command |
| `win.find-in-selection` | the find bar's scope toggle; like the match options, the bar is its only surface. Same reasoning as the match options below, plus one of its own: it is meaningless without a selection, so it is the one find control that is routinely unavailable, and an unavailable permanent toolbar button teaches nothing |
| `win.find-match-case`, `win.find-whole-word`, `win.find-regex` | the find bar's match options. They qualify a query that only exists while the bar is open, so a permanent toolbar seat would advertise them where they mean nothing; their find-bar toggle is their only surface: no menu item (operator, 2026-10-24 — an `Edit ▸` item offered them while the bar was closed, where they confuse), no accelerator |

A command reclassified here keeps its accelerator and menu item; it loses only
the toolbar-section obligation the Action CAM would otherwise impose.

## Document Rendering CAM — markup / rendering features

A feature that introduces or changes how document markup is rendered in the
preview (a new markup type such as annotations, or a change to how existing
markup renders) must hold across every context below. Distinguish **creation**
(an action, gated by mode — e.g. disabled in preview-only) from **display**
(rendering, which must hold even where creation is disabled): the "preview-only"
part of row 1 is about *showing* the markup, not editing it.

| # | Obligation | Anchored in |
|---|---|---|
| 1 | Correct in edit-only, split, and preview-only modes — and rendered **immediately** (no mode switch needed) and **stably** (no flicker across scroll/click) | — |
| 2 | Correct at top level **and** inside every container markup — table cells, block quotes, ordered/unordered list items, nested lists. **A container is not one context if it renders through more than one widget shape**: a feature built for one shape leaves the other silently inert while looking finished — enumerate the shapes and cover each, and give them one seam rather than one implementation apiece. A table cell is one shape today (a `GtkLabel`, a link-only cell included); it was two while a link-only cell was a `GtkLinkButton`, which is how find went blind to link captions | GTK4Rs/AP-239; ScrAP-250 |
| 3 | Composes with inline formatting in the same span without splitting a delimiter — **in BOTH tokenisers**: the pulldown-cmark constructs (bold/italic/inline-code/links/images) *and* the tight ones this crate scans itself (`==highlight==`, `~~strikethrough~~`, `^sup^`, `~sub~`), which reach any pulldown-driven decision as plain `Text` and are invisible to it. Applies on the **preview** path *and* the **editor** path — they balance through different code. The companion obligation is **extent**: the claim highlight must cover exactly the annotated characters even where the rendered text is shorter than its source (stripped construct markers) — body and table cell go through **one** shared mapper, so a fix cannot land on half of it | `copymap::wrap_span` (preview) + `copymap::balance_source_span` (editor); `annotate::map_cleaned_highlight_to_local` (extent, both paths); `renderer::scan_script_spans`; ScrAP-195/ScrAP-196 |
| 4 | Fully, atomically undoable/redoable — one edit is one undo step, consistent in every mode | TDD 9.16 |
| 5 | Copy/clipboard fidelity — selection→source mapping yields correct source text; preview copy excludes rendering artifacts | `copymap`; GTK4Rs/AP-5 |
| 6 | Round-trips through save→reload **and** live external reload with stable on-disk syntax | TDD §3 / §5 |
| 7 | Does not break edit↔preview scroll-sync or selection mapping | `preview::sourcemap` |
| 8 | Find/search still matches the underlying document text through the markup — **in every container context of row 2, and whatever WIDGET the feature chose to render it with.** A rendering choice decides what find can reach: text the feature puts in the buffer is found by `forward_search`, text it puts in a widget is found only by a cell walk that recognises that widget's shape, and text one level deeper (a caption inside a button) is found by neither. So a feature that renders differently in a container owes a check that its text is still findable *there* — the body case passing says nothing about it (ScrAP-250). **And** the match highlights survive every preview-rebuild boundary — theme switch, view-mode switch (edit↔split↔preview), external reload, and tab switch — because each swaps in a fresh preview buffer that carries none of the overlay, so it must be re-applied (not left bare until the next match cycle), the same re-sync `refresh_outline`/`refresh_annotations` already get | `preview::cells::cell_search_targets`; `window::refresh_preview_find_highlight`; GTK4Rs/AP-47/ScrAP-250 |
| 9 | Renders correctly under **every installed theme** — all colour, typography, and decoration geometry sourced from the active theme, never a literal | [`THEMING.md`](THEMING.md); POLICY "No hard-coded styling" |
| 10 | Stays within the footprint gate — a new rendering path is by definition a "significant change" | TDD §6 / footprint gate |
| 11 | **Interaction parity in container contexts** — every interactive surface the feature exposes (action enablement, an auto-popup/dismiss overlay, and click/hit targets like margin markers) behaves for a selection or target **inside a table cell** exactly as for the main buffer, driven off the cell's out-of-buffer selection signal (primary-clipboard `changed`, GTK4Rs/AP-28), not only buffer signals. **That includes the pointer's own gestures**: a drag, double-press and triple-press select in a cell as they do in body text, and a link is hit only on its caption, so a swipe across it selects rather than follows. A widget brings its own gesture defaults into the document with it — a selectable `GtkLabel` drags a selection out as text, a `GtkLinkButton` is a link across its whole allocation — and each one found there was a violation of this row (TDD 2.9a) | GTK4Rs/AP-28; `widgets::table::cellselect` |
| 12 | **Rendering parity in container contexts** — one theme key feeds both the body-buffer path and the table-cell path (Pango markup + `bgalpha`, `u16` triple); no second literal, no drift | POLICY "One theme key, every application path"; ScrAP-36; `widgets::table::linkink` — SELECTED text in a cell, whose ink is read from the same selection node rather than restated |
| 13 | Survives **zoom** at every level — Pango scale for type, the `px()` path for pixel metrics; the theme never emits CSS `font-size` (zoom owns it exclusively) | GTK4Rs/AP-101; GTK4Rs/AP-77 |
| 14 | **Switches theme at runtime** without restart, in every open window | `re_render_all_windows` |
| 15 | **Legibility floor** asserted per theme (body contrast gate, headless) | `palette::contrast` |
| 16 | **Keyboard parity in container contexts** — a rendering choice that puts a **focusable** widget in the document must not disable the pane's own keyboard behaviour: with focus on that widget, every document-navigation key (←, →, ↑, ↓, Home, End, PageUp, PageDown and their Ctrl forms) still moves the *document*, exactly as with the pane focused, while a selection-extending (Shift) key still acts on the widget's own text. A focused child with key bindings of its own consumes them in its target phase and they never reach the view — silently, with no warning and no log line. **Swept** when written (2026-08-09): the renderer anchors four child kinds — the table widget, a rule `GtkSeparator`, an image `GtkPicture` and a missing-image `GtkImage` — and only the table's cells take focus; the repair is sited on the pane rather than per child, so it covers those and any future one | `keynav`; `codeview::navkeys`; GTK4Rs/AP-264 |
| 17 | **Exports as it renders** — the construct reaches an exported artefact as the preview shows it, in every container context of row 2. A rendering feature has **two** consumers: the preview widget and `export`'s display-free pipeline, which walks the same normalised event stream. They agree by construction only for constructs both were taught; a construct added to the renderer alone is silently *absent* from every export, and absence is exactly what nobody notices — the artefact still opens, still looks finished, and is simply missing something. Cheap to satisfy and invisible to omit, which is what earns it a cell. **Swept** when written (2026-08-19): the export pipeline was built against the full construct list and every construct in it is covered by `export::doc`'s tests. **A construct can be HALF-taught, and that is harder to see than a missing one** — a disclosure's body is ordinary Markdown events the walk never had to be taught, so it exported correctly from the first day, while the `<summary>` label lived inside raw HTML and reached no artefact at all. The export was neither absent nor right, and the part that worked is what made the part that did not look fine. So check the cell against the construct's *pieces*, not against whether it appears | `export::doc`; `export::html`; `export::pdf`; TDD 25.3 |
| 18 | **Holds while the window is UNFOCUSED** — every mark the feature renders as a WIDGET rather than as buffer text states its own ink, never inherits it from the page. A desktop GTK theme styles the widget nodes this project anchors in its documents — `label:backdrop { color: … }` is in Breeze — and an inherited value loses to any rule that MATCHES the node, from any provider, so the app's own sheet does not defend a mark it only reaches by inheritance. The failure is invisible where a feature is built (the window under test has focus) and invisible to every assertion on the generated stylesheet, since the rule that should exist is the one that is missing. MEASURED: on a themed page a table's body cells and a disclosure's glyph indicator both re-inked to the desktop's unfocused grey the moment another window was activated, beside prose that did not move. Cover it by naming the node the mark is drawn on — including the CHILD a widget puts its text or icon in, which `color` reaches only by inheritance | `preview::css`'s `LINK_CELL_SELECTORS` / `DISCLOSURE_MARKER_SELECTORS`; TDD 18.52 |

Row 17 is the export twin of row 1: row 1 governs a construct's appearance on
screen, row 17 governs its appearance in an artefact the reader hands to someone
else. It exists because the two renderings are the standing risk the export was
designed around — one `ExportDoc` feeds both sinks so they cannot drift from *each
other*, and this cell is what keeps either from drifting from the **preview**.
Vigilance is not a mitigation; a cell is.

Rows 9–10 and 13 are **reference rows**: they defer to gates that already own
those concerns ([`THEMING.md`](THEMING.md) and the "No hard-coded styling" rule in
POLICY.md; the footprint gate in POLICY.md and TDD §6; GTK4Rs/AP-101/GTK4Rs/AP-77).
They appear in the matrix so the obligation is not forgotten, but the CAM does not
restate their rules.

Row 18 is row 12 pointed at a STATE rather than a container: 12 asks whether one theme key feeds both paths, 18 asks whether the path that renders through a widget survives the window's own focus changing. Both are about a mark that is drawn by a widget rather than by the buffer, which is why a feature that reaches for a real widget owes them together.

Row 12 is the rendering twin of row 11: row 11 governs a feature's *interaction*
inside a table cell, row 12 governs its *appearance* there. Row 16 is the third of
that family and points the other way: 11 and 12 ask whether the *feature* works inside
the container, 16 asks whether the *container* has broken something the pane already
did. A rendering choice that reaches for a real widget — which this project makes
routinely, because a table cell must be a widget — imports that
widget's own event handling into the middle of a document, and what it takes away is
visible nowhere near the feature that added it. It earns its place —
the annotation and find highlights each already carry two independent hardcoded
copies (body tag + cell representation) with nothing keeping them in sync, a live
defect this row would have caught before any theme existed.

## Derived-view CAM — surfaces that mirror document state

A **derived view** is any surface that displays a *projection* of the document
rather than the document itself: the outline tree, the annotations viewer, the
window title and tab labels, the status bar. It holds no truth of its own — it is
recomputed from the document — so it can silently disagree with what the user is
looking at, and the user has no way to tell that what they see is stale.

The Document Rendering CAM governs the document's own rendering; this one governs
everything that *mirrors* it. A change that adds a derived view, or that mutates
document state a derived view projects, must account for every applicable cell.

The event classes (matrix columns):

- **A — in-session mutation**: typing/editing, a format action, an annotation
  add/edit/remove, undo/redo, a task-checkbox toggle.
- **B — persistence event**: save / Save As, external reload (prompted and live),
  open, new.
- **C — in-place view rebuild**: view-mode switch (edit↔split↔preview), runtime
  theme switch, zoom, live-preview re-render.
- **D — host change**: tab switch, tab close, cross-window tab move / pop-out,
  session restore, a deferred background tab materialising.

| # | Derived surface | A | B | C | D | Choke point |
|---|---|:-:|:-:|:-:|:-:|---|
| 1 | Outline tree (headings), its sidebar filter and the filter bar's "M of N" count | ✓ | ✓ | ✓ | ✓ | `refresh_outline` → `rebuild_outline_list`, which also re-shows the active document's own filter in the window-shared box (`sidebarfilter::sync_to_tab`, column D) |
| 2 | Outline scroll-spy highlight | ✓ | ✓ | ✓ | ✓ | `wire_scroll_spy`; GTK4Rs/AP-52/GTK4Rs/AP-52/GTK4Rs/AP-112 |
| 3 | Annotations viewer (flat list, its sidebar filter, the heading's count, and the filter bar's "M of N") | ✓ | ✓ | ✓ | ✓ | `refresh_annotations` → `rebuild_annotations_list` (filter re-shown as in row 1); `preview::refresh_annotations_in_place` |
| 4 | Window title, tab label + tooltip, View ▸ Documents (menu **and** toolbar combo) | ✓ (dirty) | ✓ | — | ✓ | `update_window_title`/`retitle_window`; `refresh_active_tab_label`/`badge_tab_label`; `refresh_documents_menu`; `refresh_documents_button` |
| 5 | Status bar — persistent line (lost file · live reload off · unsaved changes) | ✓ | ✓ | — | ✓ | `refresh_dirty_status` |
| 6 | Status bar — Ln/Col indicator | ✓ | ✓ | ✓ | ✓ | `refresh_position_indicator` |
| 7 | Find bar — match count and match highlights | ✓ | ✓ | ✓ | ✓ | `update_match_count_label`; `refresh_preview_find_highlight` (Document Rendering CAM row 8). In split mode the pane searched follows focus (TDD 11.19), so a move to the other pane is a column-C rebuild too: `findbar::retarget_find_to_focused_pane` |
| 8 | Crash-recovery notice (per-tab prompt + per-window status count) | ✓ | ✓ | ✓ | ✓ | `toast::sync_recovery_toast` |
| 9 | Insert Link's Section picker (heading tree, its filter, the `#slug` a pick writes) | n/a | ◑ | n/a | n/a | `editbar::section::section_button`, built from the editor's text when the dialog opens. The dialog is modal, so no edit (A), view rebuild (C) or tab change (D) can happen while it is up. ◑ B: a live reload of a clean buffer while it is open leaves the list as it was at opening — the same exposure as the dialog's own insertion range, which is captured at the same moment |
| 9 | Status bar — word count and line endings (document, and the selection's words) | ✓ | ✓ | — | ✓ | `refresh_text_indicators` (an edit reaches it through `note_buffer_changed`, a selection through `schedule_selection_count`) |
| 10 | Status bar — zoom level | — | — | ✓ | ✓ | `refresh_zoom_indicator` |
| 11 | Backing-loss prompt (per-tab floating Save/Dismiss) | ✓ | ✓ | ✓ | ✓ | `toast::sync_backing_loss_toast`. Row 5's twin: **one fact, two surfaces, different jobs** — the line states the condition, this carries the control. Both derive from `backing_loss` and nothing else, so neither can report a loss the other does not; a second condition source here would be the drift this matrix exists to prevent. Column C is load-bearing (window-shared widget, per-tab state) and the prompt yields its corner to the conflict prompt, which shares it |

Rules that give the matrix its teeth:

- **Both directions, always.** A derived view desyncs two ways: *mutation* (the
  document changed, the view must re-derive — columns A/B) and *rebuild* (the view
  was replaced, the derived state must be re-applied — columns C/D). Satisfying one
  direction is not evidence for the other; Document Rendering CAM row 8 is the
  rebuild-side instance for find, and this matrix is where the mutation side lives.
- **Propagation is mode-agnostic.** *Creation* of markup may be gated by view mode
  (an editing action can be insensitive in preview-only); *propagation* never is. A
  mutation that is reachable in a mode must refresh every derived view in that same
  mode, by the same code path.
- **Immediate, not self-healing.** "It corrects itself on the next tab switch /
  mode switch / reload" is a **fail**, not a mitigation. The user must never take an
  unrelated action to make a visible surface tell the truth.
- **One choke point per surface, called by every event.** Each row's refresh
  function is the single entry point; events call it rather than re-implementing the
  rebuild locally, so a new event cannot half-implement the refresh (the GTK4Rs/AP-47 shape). A new derived view must name its choke point here.
- **Take the cheapest correct path.** When the rendered text is invariant, use the
  in-place refresh rather than rebuilding the buffer — a needless `set_buffer`
  brings the repaint/scroll-jump family with it.
- **A deferral on screen is capped under 250 ms.** A background tab may carry stale
  derived state for as long as it stays in the background, provided it is re-derived on
  activation (the `needs_render` / `materialize_deferred_preview` / `pending_external`
  replay path). A visible surface may defer its refresh — a menu rebuild moved out of a
  signal handler so it cannot crash (GTK4Rs/AP-76) is the standing case — but the stale
  state must be gone in under 250 ms, and a deferral that can queue behind other work
  (layout, rendering, I/O) has no bound at all, so it does not meet the cap.

**Row 8 is the matrix's own worked example of why column B exists.** The recovery
notice reports *unsaved recovered content*, and its action is "Discard recovery" —
which reverts the tab to what is on disk. Every column but B was satisfied by the
obvious implementation; B was not, and the gap was not cosmetic. Left standing after a
**save**, the notice would have gone on describing a recovery that no longer bore on
what the user was looking at, while offering them a button that threw away the work
they had just committed. Nothing failed, no test went red, and the happy path — recover,
read the notice, dismiss it — looked complete. It retires at the same choke point that
recomputes dirtiness, so save, revert, reload and undo all reach it without being taught
individually.

A second-order rule this row establishes: **a derived surface whose control performs a
destructive action must retire at least as eagerly as it appears.** A stale *label* is a
correctness bug the user can see through; a stale *button* acts on their behalf, using
a premise that has expired.

Action **enabled/sensitivity** state is deliberately out of scope here — it is the
Action CAMs' own invariant row. This matrix covers surfaces that display document
content, not surfaces that gate commands.

## Reading-Position Preservation CAM — events that perturb a text-pane viewport

A text pane (the editor `GtkSourceView`, the preview `CodePreviewView`) holds the
user's **reading position** as *viewport state*. Unlike a derived view it is not a
projection of document content — so it sits outside the Derived-view CAM — but it is
lost just as silently. Any change that **re-lays-out the text** perturbs it: either a
**geometry change** (the pane's width changes → the text re-wraps) or a **content
rebuild** (the buffer is swapped). Unless the reading position is captured *before*
and restored *after* GTK's lazy line-height validation, the viewport **jumps — toward
the top**, because a transiently shrinking-then-growing `upper` clamps `value`
(GTK4Rs/AP-13/115 family). The happy path hides it completely: the pane renders
perfectly, only the scroll is wrong, and only on the one event that forgot — the
textbook latent gap.

The perturbation kinds need different *restore mechanisms*, never a different
*concern*:

- **Geometry change** (width): the text re-wraps, no buffer swap. The raw pixel
  `value` survives but no longer maps to the same logical line.
- **Content rebuild, same text** (buffer swap, identical rendered content): the buffer
  is new; `value` may or may not survive.
- **Content rebuild, DIFFERENT text** (buffer swap where lines appear or vanish — a
  disclosure opening or closing): neither `value` nor the line survives, because the
  place a line number names has moved.

The first two preserve the **same way**: capture the top buffer **line**, restore it
after validation. The only variable is view *warmth* — a warm, already-validated view
takes a deferred `scroll_to_mark`; a freshly-built or cold view with a far target
needs the progressive `set_value`-off-`notify::upper` restore, because a one-shot
lands at the top (ScrAP-115). **That warm/fresh choice is made once inside the choke
point, never by the call site.**

**The third kind breaks the line anchor, and it fails in exactly the shape this
matrix warns about.** A line number is only a reading position while the buffer holds
the same lines; once a fold adds or removes some, the same number names different
content, and past the shortened document's end it clamps — MEASURED on a reader parked
mid-document opening a block above them: the line anchor put them back at source byte
0, the top. The anchor for this kind is the one coordinate the change does not move,
the source: `readingpos::DocPosition`, the same value row 5 carries between panes.
**So a re-render declares which kind it is** (`window::zoom::RenderShape`) rather than
every call site remembering — the two anchors are each correct for one kind and
silently wrong for the other, which is not a difference a reader of the call site can
see.

| # | Perturbing event | Kind | Editor | Preview | Status today |
|---|---|---|:-:|:-:|---|
| 1 | Zoom in / out / reset | rebuild | ✓ | ✓ | ✓ `rerender_and_restore_scroll` |
| 2 | Live-preview re-render (editor edit) | rebuild (in-place) | — | ✓ | partial — relies on `value` survival, no explicit restore |
| 3 | External reload — live **and** prompted | rebuild | ✓ | ✓ | ✓ `reload.rs` |
| 4 | Runtime theme switch | rebuild | — | ✓ | ✓ (reuses the reload path) |
| 5 | View-mode switch (edit↔split↔preview) | rebuild | ✓ | ✓ | ✓ `content_reading_position` / `apply_content_reading_position` — a `readingpos::DocPosition`, which is stronger than the line anchor this row asked for: the two panes hold different text, so a line is meaningful in one and wrong in the other, whereas a document position each pane resolves for itself crosses the boundary |
| 6 | Tab switch / deferred materialize / session restore | host | ✓ | ✓ | ✓ (materialize path) |
| 7 | **Horizontal resize / window maximize-restore** | geometry | ◑ | ✓ | ✓ (preview) `size_allocate` raw-width re-anchor (GTK4Rs/AP-153); editor pane not yet covered |
| 8 | **Split-pane drag** (divider move → the preview pane's width changes) | geometry | ◑ | ✓ | ✓ (preview) — same re-anchor; the width key is **cause-agnostic**, so #7's fix covers this too. Live-verify pending |
| 9 | **Sidebar toggle** (show/hide outline / annotations → the preview pane's width changes) | geometry | ◑ | ✓ | ✓ (preview) — same cause-agnostic re-anchor as #7. Live-verify pending |
| 10 | **Crash recovery applies a snapshot** (buffer replaced with recovered content) | rebuild | n/a | n/a | n/a **by position in the lifecycle, not by exemption** — see below |
| 11 | **Disclosure expand / collapse** (`<details>` toggled, by the reader or by find/outline reaching into a collapsed block) | rebuild, **different text** | — | ✓ | ✓ the reader's toggle splices in place and restores through its own reader anchor (`preview::splice::install`); in split mode the scroll sync is HELD until that restore lands and the editor then follows the preview once (`window::foldsplice`) — a sync left running re-projects the editor's coarser line over the restore (TDD 2.26h). Find/outline reveals re-render with `RenderShape::ChangedContent` — a `readingpos::DocPosition`, because the line anchor is invalid there by construction |

**Why the three geometry rows collapse to one fix (preview).** The re-anchor keys on
the preview's **raw allocation width inside its own `size_allocate`**, so it is
**cause-agnostic**: it fires on *any* width change and does not care what produced it
— a window resize (#7), a split-pane divider drag (#8), or a sidebar toggle (#9) all
funnel through the same `size_allocate` with a changed width and get the same restore.
A geometry event that changes the preview's *height* but not its *width* (an
`Automatic` horizontal-scrollbar appearing/disappearing; the vertical bar is pinned
`Always`) does **not** re-wrap the text, so it never triggers the clamp and needs no
re-anchor — which is why "scrollbar-policy flip" was dropped from #9 as a non-cause.

**The `◑` in the Editor column is a real, tracked gap.** The re-anchor lives in the
preview's `CodePreviewView`; the editor pane is a `GtkSourceView` with no equivalent
hook. Whether the editor actually drifts depends on whether it word-wraps (a wrapping
prose editor would share the bug; a non-wrapping code editor that h-scrolls would not)
— **audit and, if it wraps, extend the same line-anchor re-anchor to it.** Until then
the geometry rows are honestly `◑` (preview done, editor open), not `✓`.

**Why row 10 is `n/a` and what would end that.** Crash recovery genuinely swaps a text
pane's buffer, so the recognition trigger below fires and it belongs in this matrix. It
needs no restore today for one reason only: it runs **once, during startup**, on a tab
whose viewport is still at the top because nothing has scrolled it yet — there is no
reading position in existence to lose. That is a fact about *when* it runs, not about
what it does, and it is recorded here rather than left implicit precisely because it is
the kind of exemption a later change silently invalidates. **Any of these ends it and
requires routing through the bracket:** recovering into a live session rather than at
startup; restoring a scroll or caret position from the swap header (not currently
carried — SCHEMA.md pins the header fields, and adding either would create a held
reference across a process restart; see the Document-Reference CAM note below); or
re-applying a recovery after the user has scrolled. The
*discard* half already needs no special handling — it reverts through the ordinary reload
path, which is row 3.

**The startup premise is no longer structural, but the row survives on its own merits
(2026-08-02).** Moving document I/O off the main thread made the recovery pass `async`
— it awaits a read per snapshot, so the main loop now runs between the windows
appearing and the snapshots being applied, which it previously did not. "Nothing has
scrolled it yet" is therefore a likelihood rather than a guarantee, and on the slow
filesystem that change exists to serve the gather can take seconds with the windows
already live.

That turns out **not** to matter for the preview, and the reason is worth recording
because it is this matrix working as designed: `apply_recovered_content` rebuilds
through `rerender_tab_preview_in_place` → `rerender_and_restore_scroll`, which is row
1's choke point and already captures the top line before the swap and restores it
after. The "one bracket, every event routes through it" rule meant recovery inherited
the restore without anyone deciding it should. **The `n/a` was always over-modest** —
the preview half is covered by construction, not by timing.

What the async change does leave is the **editor** pane: recovery replaces the editor
buffer with `set_text`, which resets its viewport, and there is no editor-side
re-anchor. That is the same `◑` gap rows 7-9 already carry, reached by a new event
rather than a new defect — so it is tracked there, not duplicated here. Row 10 stays
`n/a` for the preview and joins the editor gap for the editor.

Rules that give the matrix its teeth:

- **One bracket, every event routes through it.** Capture the reading **line** and
  restore after validation through a single capture/restore choke point
  (`with_preserved_reading_line`). The warm-vs-fresh mechanism is chosen *inside* the
  bracket by view warmth and target distance, never by the call site. A call site
  that hand-rolls its own capture/restore is the GTK4Rs/AP-110 latent-regression
  shape — the next perturbing event added will forget it, and a happy-path test won't
  catch the silent jump.
- **Line, not fraction, for same-buffer preservation.** A pixel fraction mixes tall
  and short lines and drifts (GTK4Rs/AP-14), and snaps to the top while
  `upper − page_size ≤ 0` during validation. Row 5's surviving fraction use is a
  defect this matrix flags, not a sanctioned variant. Reserve fraction (and the
  source-map) **exclusively for cross-buffer *mirroring*** — the continuous
  editor↔preview scroll-sync — a *different* concern this CAM does **not** govern:
  mirroring maps between two documents' coordinate systems, where a single-buffer line
  anchor is meaningless.
- **Geometry change is a first-class perturbing event.** This is precisely the event
  class the Derived-view CAM's columns (mutation / persistence / rebuild / host) do
  **not** model, which is why the resize jump shipped unnoticed. A change that alters a
  pane's *width* — a window resize, a pane-drag, a sidebar toggle, or a height-for-width
  child re-measuring — is scroll-perturbing even though it touches no document content.
- **Restore after validation, never one-shot on a cold view.** A far target on a
  freshly-rebuilt view lands at the top from a one-shot `scroll_to_mark` / `set_value`
  (ScrAP-115); the fresh mechanism re-applies progressively off `notify::upper` until
  `line_at_y(value)` converges.
- **Immediate, not self-healing.** "It corrects itself on the next scroll / re-render /
  mode switch" is a **fail**, not a mitigation — the same rule the Derived-view CAM
  carries.
- **A `tests/MANUAL-TEST.md` check per ✓ cell**, derived from the cells (the cross-CAM
  rule). A geometry-change cell in particular is a **live-display** check — a headless
  `#[gtk::test]` that maps and pumps to full allocation settles the very validation
  race the bug rides on and yields a false PASS (ScrAP-78).

**Recognition trigger** (the obligation the choke point *cannot* enforce — a caller who
doesn't know they perturb scroll won't route through the bracket): *does your change
alter a text pane's width, swap its buffer, or add/resize a height-for-width child?* If
yes, it is scroll-perturbing → route it through the bracket and add a manual-test cell.

**Boundary — out of scope:** continuous cross-pane scroll-*sync* (editor↔preview
mirroring) is a separate concern with its own mechanism (source-map / fraction), and
deliberate **navigation** (outline click, find-next, `#anchor` jump) *intends* to move
the viewport. This CAM governs only *preserving* an existing reading position across an
incidental perturbation, never establishing a new one.

## Document-Reference CAM — state that points INTO the document

The Derived-view CAM governs state *computed from* the document. This one governs
state that *points at* it: an offset, a byte range, a line number, or an index into
a collection derived from it, **held somewhere across time** — in a closure, a
widget's state, a queued idle, a pending request, a row model.

The two are inverses and neither implies the other. A derived view that goes stale
*displays* something wrong, and the user can see it. A held reference that goes
stale *acts* on the wrong place, and the user cannot see it coming: the code is
still confidently doing what it was asked, to text that is no longer the text it
was asked about. This is the more dangerous of the two, and it had no matrix —
which is how the annotation Remove/Edit corruption shipped (ScrAP-187).

**The editor buffer drifts constantly.** Every cell below is reachable without any
unusual sequence: the reader types, applies a format command, toggles a checkbox,
undoes, or simply waits for the split-mode live re-render to re-scan. A reference
captured before any of those and used after it is addressing a different document.

The invalidation classes (matrix columns):

- **A — content mutation**: typing/editing, a format action, an annotation
  add/edit/remove, undo/redo, a task-checkbox toggle. Shifts every offset after the
  edit; may delete the referent outright.
- **B — wholesale replacement**: external reload, open, tab switch, session
  restore, a background tab materialising. The referent may not exist at all.
- **C — re-derivation**: a re-scan or re-render that rebuilds the *collection* a
  positional reference indexes into (the marker list, the entry list), even when
  the document text is unchanged.
- **A′ — in-place region mutation of a rendered pane**: a disclosure fold splices one
  block's region into the live preview buffer, deleting and rewriting it without
  rebuilding the pane (`preview/splice/`). Distinct from A because the SOURCE does not
  change at all — so anything keyed on source bytes is untouched — while every **buffer**
  offset below the splice shifts by the region's length delta. It is the mirror image of
  C, which replaces the collection while the text stands still. Stated as its own class
  because a reference that survives A and C by being a source-byte reference can still be
  wrong under A′ if it is in fact a buffer offset, and the two are the same Rust type.

| # | Held reference | Points into | A | A′ | B | C | How it survives |
|---|---|---|:-:|:-:|:-:|:-:|---|
| 1 | Annotation card's Remove / Edit target (built in `src/preview/build.rs`) | source bytes | ✓ | n/a | ✓ | ✓ | `AnchoredSpan` — carries the construct's own text and re-resolves at apply time; mutations total (ScrAP-187) |
| 2 | Annotations viewer's selected row | source bytes | ✓ | n/a | ✓ | ✓ | Stored as the annotation's start byte and **re-resolved against a fresh scan on every rebuild**; a vanished annotation simply loses the selection |
| 3 | Task-checkbox toggle span | source bytes | ✓ | n/a | ✓ | — | The toggle re-locates a well-formed marker at the span and returns `None` otherwise, which the caller makes a clean no-op |
| 4 | Pending marker-open request | marker-list **index** + buffer offset | ✓ | ⚠ | ✓ | ⚠ | Bounded by a wall-clock deadline and re-aimed each frame, but the target is a **positional index** into a list a re-render replaces — see the rule on positional references below |
| 5 | Scroll re-anchor target line | buffer line | ✓ | ✓ | ✓ | — | Re-read each frame; a drifted line mis-positions the viewport only, and the next settle corrects it |
| 6 | Back/Forward history entry's **place** in a document (TDD §23) | heading **slug**, or a buffer line | ◑ | ◑ | ✓ | ✓ | Two strengths, chosen by what the recording site can know. A slug is re-resolved against the tab's live heading map, and a render that no longer contains it **degrades the entry to "just this document"** rather than letting it point somewhere wrong (23.14). A line is the weak form — an arbitrary scroll position offers no stronger handle — and takes row 5's bargain: it clamps and mis-positions the viewport only |
| 7 | Reading position carried across a view-mode switch | source bytes (`readingpos::DocPosition`) | ✓ | n/a | n/a | ✓ | Held only for the span of the swap — captured from the pane being left, resolved into the pane being entered — so class B cannot reach it (a wholesale replacement is not in flight during a mode switch), and the editor flushes to source before the capture so class A is settled. Class C is what makes it a `DocPosition` rather than a preview buffer offset: the preview is REBUILT by the very switch being crossed, so any reference into its buffer would be resolved against a collection that no longer exists |
| 8 | Every buffer-keyed map a render installs — copymap, source map, heading sites, link spans, annotation placements, collapsed-block body ranges | preview **buffer** char offsets | n/a | ✓ | ✓ | ✓ | The splice reinstalls **all of them wholesale** from its own full re-parse (PASS A), never patching the live ones — so they are replaced rather than shifted, and a partial update is not a state the code can reach. The one way this fails is the splice reporting success having not written the region, which is why that refusal is a typed `SpliceVerdict::RegionLost` the caller must re-render on rather than a `bool` — a review finding, since a `bool` was reporting success with the region already deleted. Class A is `n/a` because a source mutation clears the fold map and forces a full re-render before any splice can run |
| 9 | Preview create-annotation card's target (`src/preview/annotate.rs`, `PendingTarget`) | source bytes | ✓ | n/a | ✓ | ✓ | `AnchoredSpan` (nearest-occurrence), **captured when the card is RAISED, not when Save is pressed**. The card is open for as long as the reader is typing a comment, which makes it the longest-held reference in the application; the selection is crossed out of preview-buffer space into source space at capture, because a buffer offset has nothing to re-resolve against once row 8 has been reinstalled underneath it. Resolved once, at `window::annotate::apply_annotation_edit`, the one point the mutation is applied |
| 10 | Editor create-annotation card's target (`src/window/editor_annotate.rs`) | source bytes | ✓ | n/a | ✓ | n/a | The same `PendingTarget` as row 9 and resolved at the same choke point. Narrower only in what can move it — the card holds focus, so the movers are undo/redo, Replace All and an external reload rather than the reader's own typing |
| 11 | Disclosure control's block (`src/preview/render.rs`, `src/widgets/disclosure.rs`) | **cleaned** source bytes | ✓ | n/a | ✓ | ✓ | `AnchoredSpan` over the block's **opening delimiter** (`renderer::disclosure::opening_delimiter`), held on the widget and re-resolved at the click against `TabState::previewed_cleaned`. The ambiguity policy is **`Unique`**, not nearest: a document repeats `<details><summary>Example</summary>`, and choosing between two of them toggles a block the reader was not pointing at. Unresolvable ⇒ **re-render the pane**, never a silent refusal (TDD 2.26n). The identity is the delimiter and never the whole block — anchor the block and typing inside it destroys its own identity — and the front-matter disclosure is synthetic, so it anchors on the opening fence line and relies on front matter being at byte 0. The key resolved at the click is applied — fold toggle and splice together — in an idle bound to the clicked tab at its text generation and view mode (`window::defer_with_tab`); a tab switch, a text change or a mode switch before the idle drops the click, leaving that tab's fold model agreeing with its render, and puts back the control's `active` (which GTK flipped before `toggled`) with `toggled` blocked (`widgets::disclosure::ActiveSync`) |
| 12 | Find cursor's current preview hit (`src/window/find.rs`, `FindCursor::Preview`) | preview **buffer** offset + ordinal | ✓ | ✓ | ✓ | ✓ | The POSITION is the reference and the ordinal is only what the reader is shown. Re-found in the rebuilt list by position (`resume_ordinal`): the same hit if it is still there, otherwise the count before where it was, so Find Next means "the first match after where I am". An ordinal alone is a class-C reference into a list the live re-render and the fold splice both replace, and it fails with "N of M" reading correct. **Limit, accepted:** every cell hit of one table shares the table's anchor position (and every hidden hit its block's summary), so there the position names the group and the ordinal picks within it; a rebuild that adds a match earlier in the same table resumes one hit early |
| 13 | Outline row's heading (`src/outline_view.rs`, `src/window/outline_nav.rs`) | heading title **path** | ✓ | n/a | ✓ | ✓ | The row carries `outline::expansion::HeadingPath`, and the activation re-derives BOTH the document-order index and the source offset from a fresh parse of the live document (`resolve_heading`). The row's `doc_index` stays and is carried with the path, because the two answer different questions: the PREVIEW's `heading_sites` are indexed by the build the row came from (the outline rebuild and the re-render are one tick), while the EDITOR's caret wants the live offset. An index is never applied to a list of a different generation, which is the whole of the rule. The Back/Forward slug is taken by TITLE rather than by either index — that record is durable, so a wrong one cannot be noticed later. A heading whose path is gone navigates nowhere rather than to a neighbour. A reveal through a collapsed disclosure (`window::foldreveal::reveal_folds`) carries the fold chain and the index across an idle bound to the tab at its text generation and view mode, and is dropped whole — nothing expanded, no scroll — if any of them moved. In split mode the chain and the index come from the preview render, which trails the editor by the live-preview debounce; the stamp does not see that lag |
| 15 | Find's captured **editor** scope — the "search in selection" passage (`src/window/find.rs`, `FindScope::Editor`) | a pair of `GtkTextMark`s in the editor buffer | ✓ | n/a | ✓ | n/a | **Marks, because the range must track edits made INSIDE it** — Replace All is exactly such an edit, and it changes the passage's length while the reader is still confined to it. Left gravity on the start and right on the end, so an insertion at either boundary lands inside rather than escaping. An `AnchoredSpan` is the wrong instrument here for two reasons: it re-finds by captured text, and the text is what changes; and a selection is unbounded in size, so anchoring by its text is unbounded in cost. Class B (a reload) collapses both marks onto offset 0, so the reload path RELEASES the scope explicitly rather than leaving a bound that silently stops bounding while the toggle says it does. Every resolution guards `mark.buffer() == buf` first (GTK4Rs/AP-89) |
| 16 | Find's captured **preview** scope (`src/window/find.rs`, `FindScope::Preview`) | preview **buffer** char range + the render it was taken from | ✓ | ✓ | ✓ | ✓ | Keyed on the same `view_serial` + `generation` pair the hit cache uses, and checked on every access. Nothing tracks the range because nothing can: a re-render replaces the text wholesale. **Unresolvable ⇒ re-derive, never refuse** — the toggle turns itself off and the search covers the whole pane, because reinterpreting the range against the new render confines the search to whatever now happens to sit at those offsets, which is a confident answer about a passage the reader never chose (TDD 11.16). This is the arm the retired `fold_epoch` got wrong and `PreviewFindCache` got right, and the two differ in nothing else |
| 14 | Outline selection and the scroll-spy's guard (`TabState::outline_selected`, `outline_spy_doc`) | heading title **path** | ✓ | n/a | ✓ | ✓ | Both are re-resolved into the current build's indexes at `refresh_outline`, exactly as the sibling `outline_collapsed` already was. Held as indexes, the first re-selected whichever heading had moved into that position and the second suppressed a genuine activation whenever two builds' rows happened to share one |
| 17 | Open annotation comment card's anchor and the markers it lists (`src/codeview/card.rs`, re-keyed by `src/codeview/cardfollow.rs`) | source bytes (the first annotation's start) + marker-list **indices** | ✓ | ✓ | ✓ | ✓ | Re-keyed at every render of the view, at the one place a render installs its markers: each listed annotation is found in the new markers by its construct's text and neighbours, nearest where it was, and the card adopts the new indices and start, then re-points after the next paint. Any member not found dismisses the card (TDD 17.51). Before this the identity was re-read only on a scroll, so a render in place left both naming the previous render, and the card pointed at text that had moved in under it (TDD 17.47) |

**Row 6's `◑` under content mutation is the one deliberate weakness in this matrix,
and it is bounded rather than unnoticed.** A slug survives an edit (class A) by
construction — that is the whole reason it is stored instead of the offset the
`heading_map` holds — but the *line* half does not, and it cannot be upgraded: the
reference describes a position the reader chose by scrolling, and no identity exists
for "42% of the way down, between two paragraphs". What keeps it acceptable is the
failure *shape*, not its likelihood: a drifted line scrolls the reader to slightly
the wrong place in the right document, which is visible, harmless, and immediately
correctable by scrolling — the same bargain row 5 already makes. That is a different
class of outcome from rows 1–4, where a stale reference *acts* on the wrong text.

The sweep this row's addition obliges (the cross-CAM new-row rule): the only other
state pointing into a document across time is rows 1–5, all of which predate it and
were re-read when this row was written; no gap was found, so nothing was fixed under
it. Rows 1–5 are unchanged.

**Rows 15 and 16 were added by the find-in-selection work. The sweep they oblige**
(the cross-CAM new-row rule): the category they enter is *find's own held state*, whose
only prior member is row 12, and it was re-read when these were written — it already
re-finds by position and needs nothing. No other find state points into a document
across time: the query is text, the options are booleans, and the hit list is rebuilt
rather than held. Nothing was fixed under these rows.

**Row 17 was added by the work on cards following their anchor.** The sweep it obliges:
the other two cards over a pane, the Annotate pop-up and the editor's formatting bar, hold a
rectangle and no document reference, and both close when their selection collapses, which
a reload does (driven in both split and preview, release build, Xvfb, 2026-10-29). Row 4
holds an index into the same marker list and was not re-read under this row.

**Class A′ and row 8 were added by the disclosure fold-splice work, and the sweep it
obliges is recorded here.** Rows 1, 2, 3 and 7 are keyed on SOURCE bytes and a splice
changes no source, so A′ cannot reach them — `n/a`, not `✓`. Row 5 already re-reads its
line every frame and takes the mis-position bargain, which is what an A′ shift costs it.
Row 6's line half takes the same bargain, hence `◑` for the same reason as its A cell.
Row 4 is the one that earns a `⚠`: it holds a marker-list index **and** a buffer offset,
and a splice re-derives the list and shifts the offset at once — it is bounded by its
wall-clock deadline and re-aimed each frame, which is the same mitigation it already
relies on under C, so no new mechanism was owed, but it is the row to re-read first if a
pending marker ever opens on the wrong annotation after a fold.

**Rows 9-14 were added by the held-references work, and the sweep they oblige is
recorded here.** They are not new constructs — all six predate the rows, which is the
finding rather than an aside: a sweep of `src/` found 28 entities holding a reference
into the document across time, seven of them at risk, and **not one of the seven had a
row**. That is how a construct reaches production without anyone having asked how it
survives an edit; nobody read "anything holding a position in the document" and decided
to skip it, because at the keystroke the construct is a `usize` captured into a closure
and nothing about writing that line announces itself as entering a category. The sweep
covered every offset, byte range, line number and collection index held across a turn in
`src/`; the fourteen it cleared were already re-derived at use or bounded to a single
turn. What makes the rows self-enforcing from here is not their prose but
`cargo xtask lint-references`' held-reference check: every construction site of the one
held-reference type must be named by a row in this matrix, so "this construct never got a
row" is a build failure rather than something someone notices years later.

**The shared idle hop.** The fold paths' references cross `window::defer_with_tab`,
which stamps the tab, its `text_generation` and its view mode when armed and runs the
body, in the stamped mode, only if the same tab is active at the same generation and mode.
A key or index is therefore never applied after the editor text or the mode it was armed
under has moved on. That is a claim about arming, not resolution: a reference resolved
from a split-mode preview render that trails the editor by the debounce was already
behind when it was stamped. The state change itself (fold toggle, fold expansion) goes
inside the body, so a dropped hop leaves the fold model as it was; what the gesture
changed before arming — a toggle's `active` — is put back by the caller's drop handler.
The drop is not the refusing arm described next: it discards one gesture and leaves a
control that acts on the next one. A reload, edit or mode switch rebuilds the controls;
a tab switch rebuilds nothing, and the control the reader returns to is live because its
`active` was put back. Any other deferral carrying a document reference resolves
INSIDE the deferred body, never before scheduling it.

**One mechanism, two arms, and the arm is the whole difference.** A generation stamp
compared at use is not wrong in itself: `window::find::PreviewFindCache` keys its hit
list on the render generation and is correct, because a mismatch makes it **rebuild**.
The disclosure control's retired `fold_epoch` keyed a control on the same kind of stamp
and made a mismatch **refuse**, which is why one is a wasted rebuild and the other was a
pane full of live controls that silently did nothing. If a second generation-stamp caller
is ever added, build the type that makes rebuild the only reachable response rather than
reviewing the second one by hand.

Rules that give the matrix its teeth:

- **Never carry a bare offset across a turn.** An integer is the one form of
  reference that cannot be checked — it is always "valid", it just stops meaning
  what it meant. Carry something that can be re-established: the text at the range,
  a stable identity, or a `GtkTextMark` (which GTK moves with the edits for you).
- **Re-resolve at use, not at capture.** The capture site knows only what was true
  then. Resolution belongs at the one choke point where the mutation is applied, so
  every path through it is covered at once.
- **The consuming primitive must be total.** A function handed a range from another
  point in time must decline an impossible one (`get`, not `[]`) rather than panic
  — in a GTK signal handler a panic aborts the process and takes unsaved work with
  it. This is the floor, not the fix: on its own it converts corruption into a
  command that silently does nothing.
- **Refusing is not enough — resolve where you can, and a refusal must RE-DERIVE.**
  A held reference that gives up whenever the document moved makes the feature
  useless in exactly the session where it is most used. Prefer re-resolution by
  identity and reserve refusal for a genuinely absent referent — and when you do
  refuse, rebuild the view the reference was minted by, so the reader's next gesture
  acts. A refusal that leaves the control on screen is the shape of the defect rows
  9-14 were written after: invisible where it is caused (a save), invisible where it
  is felt (a click that does nothing), and self-healing on the next keystroke, which
  is why it was reported repeatedly and never reproduced on demand.
- **Whole content is the strongest reference there is, and is sometimes affordable.**
  Crash recovery holds no cell in this matrix, and that is a design outcome rather than
  an oversight worth checking for: a swap file carries the document's *entire text* plus
  a digest of the on-disk baseline, so there is no offset, range or index to go stale —
  the "carry something that can be re-established" rule taken to its limit. The digest is
  re-resolved against the file at recovery time, never trusted from capture. Noted here
  because the cheap-looking additions to that format (a caret offset, a scroll line, a
  selection range) would each introduce a genuine row 1-style held reference across the
  longest gap in the application — a process restart — and must be designed as one.
- **A positional index into a re-derivable collection is the weakest reference
  there is** (ScrAP-74) and rates a ⚠ wherever it appears: it goes stale in class C
  with the document text completely unchanged, so nothing about the document's
  content warns you. If such an index must be held, hold the identity beside it and
  re-resolve, or bound its lifetime to a single turn.

## Deferred-operation CAM — work whose completion lands later

Every document read and write leaves the main thread (`docio`), so the GTK main loop
runs while one is out. That makes a whole class of change **multi-dimensional**: at any
moment a document can have a load, a save's guard read, a save's write, a reload's
read, and a crash-recovery snapshot write all in flight together, plus the startup
recovery pass working through its list. Their completion order is not the order they
were started in — GLib's I/O pool explicitly re-sorts its queue (`gtask.c:2199`) — so
each pairing is its own question with its own answer.

This is the matrix for that. It is not the Document-Reference CAM (which governs a
reference held across time, pointing *into* the document) and it is not the Derived-view
CAM (which governs a projection going stale). It governs an **operation's own result
arriving into a world that changed while it was away**.

The failure shape is uniform and quiet: the operation completes successfully, applies
its answer, and the answer was about a document state that no longer exists. Nothing
errors. The worst cell measured here ends with a tab reading **clean** while its buffer
differs from its own file — so the one surface a user would check to notice actively
says everything is fine.

The interference classes (matrix columns):

- **A — the same operation again**: a second Save, a second Reload, a burst of watcher
  events, two `open` invocations.
- **B — a different operation on the same document**: save vs reload vs snapshot vs
  recovery. The costly column.
- **C — the host changes**: the tab is switched away from, moved to another window, or
  closed while the operation is out.
- **D — the window or the application goes away**: window closed, coordinated quit.
- **E — the document's identity changes**: Save As re-points the path; the file is
  deleted or recreated externally.

| # | Operation in flight | A | B | C | D | E | Mechanism |
|---|---|:-:|:-:|:-:|:-:|:-:|---|
| 1 | Read for **open / link-nav / session restore** (builds a tab) | ✗ | n/a | n/a | ✓ | n/a | `app.hold()` + weak window re-resolve; gather-then-build keeps each batch atomic |
| 2 | Read for **reload** (explicit, and the watcher's) | ✓ | ✓ | ✓ | ✓ | ✓ | `DocEpoch` ticket; `tab_by_id` re-resolve; active-vs-background split |
| 3 | **Save's guard read** | ✓ | ◑ | ✓ | ✓ | ✓ | `WriteEpoch` mark **plus a path re-check**, re-reading rather than acting. Deliberately NOT a `DocEpoch` ticket — that counter is claimed by the watcher too, and a guard re-issuing on it never lands on a polled filesystem (the measured livelock in `window/save.rs`) |
| 4 | **Save's write** | ✓ | ◑ | ✓ | ✓ | ✓ | `WriteGate` (drop, not queue); explicit `Rc<TabState>`; tab-scoped completion |
| 5 | **Crash-recovery snapshot write** | ✓ | ◑ | ✓ | ✓ | ✓ | `swap.in_flight` + latest-wins coalescing; `tab_by_id` |
| 6 | **Startup recovery pass** | ✓ | ✓ | ✓ | ✓ | ✓ | runs once; bumps `DocEpoch` on apply; re-resolves windows/tabs after each await |
| 7 | **Backing settle re-read** (TDD 3.4, 3.5) | ✓ | ✓ | ✓ | ✓ | ✓ | one pending timer per tab, re-armed not stacked (so a backend reporting one replacement as several events costs one re-read), cancelled when a loss is recorded; weak `tab_by_id`; the re-read is an ordinary row-2 read, so its ticket and the active-vs-background split apply unchanged, and a loss is recorded once however many reads conclude it |
| 8 | **PDF export** (`run(Export)` iterates the main loop while it draws) | ✓ | ✓ | ✓ | ✓ | ✓ | `win.export` disabled while `WindowChrome::export_op` is set; the document is captured before the run, so edits and saves during it do not reach it; a tab or window close cancels the export and is deferred until it returns (`defer_until_export_stops`) |
| 9 | **Status-bar word count** (on GLib's pool) | ✓ | ✓ | ✓ | ✓ | n/a | one job application-wide, the rest queued **one per tab** (a tab's repeat request merges into its own entry, and no tab's request is ever displaced by another's — see the coalescing rule below); a result is applied only if the tab's buffer generation is unchanged, re-resolving the tab by id and rendering only if it is still the active one |

Rules that give the matrix its teeth:

- **Mutations announce; deferred readers check.** Anything that changes a document's
  content or its baseline calls `DocEpoch::bump`; anything that will *apply* a deferred
  result checks its ticket first and **discards** on a mismatch. Discard, never merge —
  a superseded answer carries no marker distinguishing it from a current one, so there
  is nothing to merge on. One counter gives both properties, because a reader takes its
  ticket *by* bumping ("I am the newest reader").
- **A write never checks; only readers can be superseded.** A completed write produced
  the bytes on disk, so its own baseline update is the truth by construction.
- **Coalesce per subject, and only where something re-issues what you drop.** A single
  waiting slot shared by every subject has to choose between two subjects' requests, and
  the one it discards is gone unless some other event asks again. The word count shipped
  that way: one pending slot, the newer request winning outright, justified by "the tab it
  displaces recounts on activation" — true of a background tab and false of another
  *window's* active tab, which recounts only on a mode or tab switch. Three windows opened
  back to back therefore left the middle one's word-count and line-endings indicators
  permanently blank. Queue one entry per subject instead, merging a subject's repeat
  request into its own entry; that keeps the in-flight bound (still one job on the pool)
  without making the queue pick a loser. Dropping is legitimate only when the drop is
  *recoverable by construction* — row 4's second save is dropped because the buffer stays
  dirty and the command stays available, so the user's next press writes the newest text.
- **Serialise writes to one path; do not queue them.** Two writes can land in either
  order and report completion in either order, so the newest bytes on disk and the
  newest baseline recorded can be different texts (C1). The second request is dropped:
  the buffer is still dirty, so the command stays available and pressing it again writes
  the newest text, whereas queuing would commit an intermediate state nobody asked for.
  The snapshot writer coalesces instead — because its writes are unprompted, so no user
  is waiting on any particular one. **Same premise, different correct answer**, which is
  why they are two mechanisms and not one.
- **Split every completion into subject-scoped and surface-scoped work.** Both exist:
  the swap sync and the tab badge belong to the document that was written; the status
  bar and the toast belong to whatever is on screen. Conflating them is a defect in
  either direction (GEP-43).
- **Force the divergence in the guard, or you have not written one.** A test that
  issues the operation and asserts leaves the world unchanged, so both readings agree
  and the bug is invisible — the first guard written here survived its mutation run for
  exactly that reason. `spawn_local` does not poll until the loop iterates, so a
  synchronous change on the next line is deterministic. And prefer pinning the
  *wiring* (does the real path bump?) at integration level with the *semantics* proved
  in display-free unit tests — a test that has to win a race to pass is asserting the
  wrong thing.

**The open cells, stated rather than rounded up:**

- **1/A — two overlapping `open` invocations can duplicate a tab.** Each checks
  "already open?" before its reads and neither has built anything yet, so both miss.
  New with the async open; costs a duplicate tab, no data risk. Closing it means moving
  the check inside the build pass or reserving the path up front.
- **3/B — a baseline moved by an applied RELOAD while the guard read is out.** The
  guard then compares pre-reload bytes against a post-reload baseline and asks. Left
  open on purpose: unlike the same cell's self-write half — a save of ours landing
  inside the read, closed by the `WriteEpoch` mark (TDD 5.7) — the file there really
  did change on disk, so a question is a defensible answer, and
  re-reading on it would re-open the livelock against an external writer that rewrites
  the file continuously. Staleness degrades to a question; starvation degrades to
  silence.
- **4/B and 5/B — a save's snapshot deletion versus an in-flight snapshot write.** The
  save retires the document's snapshot through the dirty↔swap choke point, which
  cancels the pending debounce — but a snapshot write already dispatched to the pool
  can still rename its temp into place afterwards, resurrecting the file for a document
  that is now clean. **Pre-existing** (the window was always non-zero; the async save
  widens it by the write's duration). Consequence is bounded: the next launch offers
  already-saved work back as "unsaved", which is a false positive, not a loss. Closing
  it means the delete participating in `swap.in_flight` rather than only in the timer.

## Status-notice CAM — transient messages that must be retracted

A **status notice** is an entry pushed onto a window's footer message stack, which returns
a `StatusCtx` handle that something must later `pop`. It is not a derived view — it
reports an *event or condition*, not a projection of the document — and it is not a
reading position. It is a **held handle with an obligation attached**, and the obligation
is the part that gets lost.

The failure mode is uniform and unpleasant: an un-popped notice stays on screen
**permanently**, and popping a handle against the *wrong* stack matches nothing and
silently does the same. Neither produces an error, a warning, or a log line. The base
entry (`set_base`, updated in place) is a different mechanism and is **not** governed
here; only `push`/`pop` pairs are.

The event classes (matrix columns) — everything that can happen between the push and its
intended pop:

- **A — the condition resolves**: the reported thing stops being true (the write
  succeeds, the reload finishes, the timer expires). The intended retraction.
- **B — the holder is destroyed**: the tab or window the notice is about goes away (tab
  close, Discard, window close, a coordinated quit). After this, *nothing can retract it*
  — there is no object left to call the retraction on.
- **C — the holder moves**: a cross-window tab move. **A `StatusCtx` is scoped to the
  stack that issued it**, so a retraction resolved through the tab's *current* chrome
  pops the origin's id out of the destination's stack, matches nothing, and strands the
  notice in the origin window forever.
- **D — re-entry**: the condition recurs while a notice is already outstanding (must not
  stack a second entry), and recurs again after a retraction (must report afresh, not
  stay suppressed).

| # | Notice | Retraction trigger | A | B | C | D | Owner |
|---|---|---|:-:|:-:|:-:|:-:|---|
| 1 | Snapshot-failure ("not being backed up") | condition — first successful write | ✓ | ✓ | ✓ | ✓ | `window/swap.rs`; re-home handled in `TabState::set_chrome` |
| 2 | Crash-recovery count ("Recovered … in N documents") | event — first interaction with the window | ✓ | ✓ (per-window: the stack dies with the window) | ✓ (per-window, never travels with a tab) | — (once per launch) | `window/swaprecovery.rs` |
| 3 | Transient info notice (saved / reloaded / recovered) | **timed** (~4 s) | ✓ | ✓ | ✓ | ✓ (each notice is its own ctx) | `window/toast.rs` |
| 4 | Link-navigation notice | **timed** (~6 s) | ✓ | ✓ | ✓ | ✓ | `window/linknav.rs` |
| 5 | Quiet-command confirmation ("Document copied" / "Link location copied" / "Renamed to …" / "N replacements made") | **timed** (~4 s) | ✓ | ✓ | ✓ | ✓ (each confirmation is its own ctx) | `window/editoractions.rs`, `window/copylink.rs`, `window/rename.rs`, `window/find.rs` |
| 6 | Operation-in-progress ("Saving…" / "Reloading…" / "Opening…") | **the operation ends** (`Drop`) | ✓ | ✓ | ✓ | ✓ | `winstate::BusyNotice` — armed, not shown: nothing appears unless the operation outlives `BUSY_NOTICE_DELAY`, so a fast save never blinks. `Rc`-backed so ONE notice spans a logical operation made of several futures (the save guard's read, the decision, the write) |
| 7 | Hovered link target (the URL under the pointer) | **condition** — the pointer leaves the link or the view, or the view unrealizes | ✓ | ✓ (unrealize) | ✓ (retracted against the captured stack) | ✓ (one slot application-wide: a second link replaces the first) | `window::statusbar::set_hover_target` |
| 8 | Export progress ("Exporting page P of N…" + progress bar + Cancel) | **the export returns** (`ExportProgress::finish`, also run on drop) — armed, not shown, like row 6 | ✓ | ✓ (a close waits for the export to stop) | ✓ (captured stack) | — (Export is disabled while one runs) | `window::statusbar::ExportProgress` |
| 9 | Blocked-image notice ("Blocked image x.svg: SVGs that may use XInclude are refused…" / "Blocked N images: …", prefixed "doc.md: " when that document is not in front of its window; "Blocked N images in M documents: …" when several of a window's documents refuse at once; "Blocked theme image x.png: …" for a theme sprite, in the window in front; "Blocked image x.svg and theme image y.png: …" when that window refuses both at once, a background document then named beside its own images — "Blocked image x.svg in doc.md and theme image y.png: …" — never prefixed) | **timed** (~6 s) | ✓ | ✓ (a tab closed before the notice is shown has nobody to tell: dropped) | ✓ (the render carries the tab's **id**, and the window is resolved from it when the notice is shown, not when the image was refused; once pushed, `push_timed_notice` holds it) | ✓ (coalesced: everything refused before the next idle is ONE notice per window, its documents and the theme named in one sentence, since two pushed together expire together and the later hides the earlier for its whole life; the screen records which tab (or the theme) was told about which content, so a re-render or theme reload reports nothing new while another document showing the same image is told) | `window/imagerefusal.rs`, fed by the renderer and `sprite.rs` through `imagedecode::screen` |

**Every timed row (3, 4, 5, 9) holds B and C through one mechanism:
`WindowChrome::push_timed_notice`.** It captures the chrome that issued the handle
(weakly) and retracts against *that* stack, so no timed notice re-resolves a stack at
fire time. The three rows previously resolved the tab's chrome *when the timer fired*,
which reads the tab's **current** window: a tab moved inside the notice's lifetime popped
the origin's handle out of the destination's stack (column C), and a tab *closed* inside
it upgraded to nothing so the pop never ran at all (column B) — both leaving the origin's
footer line up permanently, with no error. Guarded by TDD 16.8 and its two
`winstate/chrome.rs` tests, one of which carries a positive control proving the
re-resolving shape does strand the notice.

A matrix omits what nobody thought to look for: the lost-file notice was once missing
from this one while the rows either side of it were being examined — the same notice, in
the same shape, written by a different hand in a different module. So when a row is
added, grep for the *mechanism* (every `status…push` paired with a timer or a handle)
rather than enumerating the notices you can remember. That notice has since left the
matrix altogether, which is the rule below applied: a condition that can end before its
timer is not a timed notice, so a lost file is now part of the persistent line
(Derived-view CAM row 5) for exactly as long as it holds.

Rules that give the matrix its teeth:

- **Every push needs a pop that is guaranteed on *every* path, not just the happy one.**
  A retraction wired only to the condition resolving is a permanent notice the moment the
  holder is destroyed first. Row 1 shipped with exactly that gap: closing a tab
  mid-failure left its window reporting a document that no longer existed.
- **A `StatusCtx` belongs to the stack that issued it — retract *before* re-homing, never
  after.** Resolving the stack through a live back-reference means the handle and the
  stack can disagree, and the disagreement is silent. Retracting on the way out is
  simpler than migrating the entry, and correct: if the condition still holds, the next
  occurrence re-reports it against the window the user is now looking at.
- **Capture the stack you pushed to; never re-resolve one at retraction time.** The rule
  above is for the *condition*-driven notice, whose retraction is genuinely triggered
  later by other code; a **timed** notice has no such excuse — it knows its stack at push
  time and needs nothing else, so `WindowChrome::push_timed_notice` captures the chrome
  and every timed notice goes through it. Re-resolving (`tab.chrome()`, `state(window)`)
  looks equivalent and is not: it answers "which window does this tab live in *now*",
  which is a different question from "which stack owns this handle", and the two diverge
  precisely in the cases this matrix exists for. A retraction that re-derives its own
  destination is the general shape of the bug; holding the destination is the general
  fix. `StatusStack::pop` now logs a foreign handle rather than ignoring it, so a
  re-introduction of the shape says so.
- **Hold the handle *as* the "already reporting" flag.** One `Option<StatusCtx>`, not a
  `bool` beside a handle — those can only ever disagree by being wrong, and the
  disagreement is what produces either a duplicate notice or an unretractable one.
- **Decide whether a notice is timed or conditional, and do not be both for one
  condition.** A condition-driven notice with a timed twin for the same event pushes two
  entries saying nearly the same thing, one of which expires — harmless until the two
  disagree about which is authoritative.
- **A `tests/MANUAL-TEST.md` check per ✓ in columns B and C.** Column A is normally
  covered by the feature's own happy-path check; B and C are the latent ones and are
  invisible to it. Row 1's are `22.15b`; rows 3–5 share `16.8`, since one mechanism
  now holds those cells for all three.

**Recognition trigger** (the obligation no choke point can enforce — an author pushing a
notice does not know they have taken on a lifetime): *am I calling `push` and holding the
returned handle anywhere other than a local variable popped in the same function?* If
yes, this matrix applies — write down, at the push site, what pops it on each of A, B and
C before writing the pop.

## Document-Identity CAM — state keyed on a document's path

A document's **path is load-bearing state, not a label**. Several things in this
tree are keyed on it, and the failure when one is missed is uniformly silent: the
identity change succeeds, the title updates, everything *looks* finished, and
something that reads the path is now reading the wrong one.

**The category is not "rename".** It is *a document's identity changing while the
document is open*, and it had three members before Rename existed: the first save of
an untitled buffer, Save As adopting a path, and Save As re-pointing one.

**Why the six matrices above do not cover it.** [Derived-view](#derived-view-cam--surfaces-that-mirror-document-state)
row 4 covers the *display* of a document's name, and its column B already includes
Save As — so the name surfaces are governed. [Document-Reference](#document-reference-cam--state-that-points-into-the-document)
governs references pointing *into* a document (offsets, ranges, indices); a path is
not one. [Deferred-operation](#deferred-operation-cam--work-whose-completion-lands-later)
column E is literally "the document's identity changes", but governs it from the
*in-flight operation's* side. **Nothing governed the non-visual, not-in-flight
machinery keyed on the path**, which is where every latent gap in this category lives.

The identity events (matrix columns):

- **A — adopt**: a document that had no path acquires one (first save of untitled;
  Save As from untitled).
- **B — re-point**: a document with a path acquires a different one (Save As to
  another path; **Rename**).
- **C — lose**: the backing file goes away (external delete; a tab discarded).

| # | Path-keyed state | A | B | C | Choke point / anchor |
|---|---|:-:|:-:|:-:|---|
| 1 | `TabState.path` — the truth every other row derives from | ✓ | ✓ | ✓ | `app::attach_file_backing` |
| 2 | The live-reload `gio::FileMonitor`, the `expect_self_delete` guard and `backing_missing` — **one mechanism, not three** | ✓ | ✓ | ✓ | `attach_file_backing`; `window::rename` for the cancel-first path; GTK4Rs/AP-62 |
| 3 | The crash-recovery swap file's **stem** and its header `path` | ✓ | ✓ | ✓ | `swapfile::swap_path`; `window::swap::delete_snapshot`; `swaprecovery::retire_source_snapshot` |
| 4 | Name surfaces — window title, tab label + tooltip, View ▸ Documents (menu **and** combo) | ✓ | ✓ | ✓ | *reference row* → [Derived-view CAM row 4](#derived-view-cam--surfaces-that-mirror-document-state) |
| 5 | The relative-resource base `TabState::doc_dir()` — images, local link navigation, Insert Link/Image relativisation | ✓ | ✓ | — | `links::resolve_contained_image`; `links::relativize_for_insert`; a folder change re-renders the preview, `window::save::rerender_for_new_folder` |
| 6 | Same-file identity: the open-tab dedup lookup and the per-path write gate | ✓ | ✓ | ✓ | `app::find_open_tab_for_path`; `winstate::WriteGate` |
| 7 | Operations already in flight over the old path | ✓ | ✓ | ✓ | *reference row* → [Deferred-operation CAM column E](#deferred-operation-cam--work-whose-completion-lands-later) |
| 8 | The persisted session record and the last-visited dialog directory | ✓ | ✓ | ✓ | `session/`; `app::remember_dialog_dir` |

Rules that give the matrix its teeth:

- **A path change is not a content change.** It must not touch the saved baseline,
  the dirty flag, the buffer, the undo stack, the reading position or the rendered
  preview, and must not re-read the file — save one same-content re-render of the
  preview when the FOLDER changes (row 5), which keeps the reading position and
  re-resolves only what the image paths point at. Stating this is what keeps an identity
  change *out* of the Reading-Position and Document-Reference matrices instead of
  quietly acquiring cells in both (TDD 24.2).
- **One choke point re-points the backing, and the monitor and its self-delete guard
  travel with it.** They are one mechanism; a path change that re-points the monitor
  without settling the guard has half-changed the identity (GTK4Rs/AP-108 shape).
- **Refusal is expressed against the filesystem's own notion of identity, never a
  string compare.** Case-insensitive filesystems and symlinks both make `old != new`
  a wrong answer.
- **A move is not a rename**, because only one of them invalidates row 5. Any
  operation that can change the *directory* owes row 5 an answer; one that cannot may
  state that it cannot and stop. Rename holds the directory fixed by construction —
  the primitive it uses cannot express a move — which is what makes row 5 a provable
  no-op for it rather than an obligation nobody would think to write.
- **Row 3 is answered by remembering the file written, never by recomputing its name.**
  The swap file's name carries the stem, so after any path change the recomputed name is
  one nothing was written to. `SwapState::on_disk` records the path actually promoted;
  the delete removes that, and a promote under a new name removes the one it supersedes
  (`window::swap::record_promoted`). That holds for Rename even once it is permitted on a
  dirty document, which today it is not.
- **A `tests/MANUAL-TEST.md` check per applicable ✓** — the cross-CAM rule.

### The back-sweep this matrix obliges

The cross-CAM new-row rule requires sweeping what already shipped in the category.
The category's three prior members all live in `save.rs` / `open.rs`, so the sweep is
affordable and was **not** skipped. Drafting these rows produced **two candidate
defects in Save As**, recorded here so the sweep's extent is honest rather than implied:

1. **Save As from an untitled dirty buffer orphaned its swap file — MEASURED, FIXED.**
   The chooser's focus flush filed `untitled-<docid>.swap`; `adopt_and_save` set the
   path before the write, so the delete computed `<newstem>-<docid>.swap` and removed
   nothing. The next launch offered the pre-save text back as a recovered untitled tab —
   observed in real use, the snapshot's capture time matching the second the chooser
   opened. Fixed by the row 3 rule above; pinned by
   `window::swap::tests::save_as_removes_the_snapshot_filed_under_the_previous_name`.
2. **Save As into another folder left the preview's images resolved against the old
   folder — MEASURED, FIXED.** The preview resolves relative images at render time and
   a save does not re-render, so the old folder's picture stayed (one the out-of-folder
   block would refuse) until something redrew the pane. Fixed by a same-content
   re-render when the folder changes; pinned by
   `window::save::gtk_integration_tests::save_as_into_another_folder_re_resolves_the_previews_images`.

## Hot-path CAM — handlers on a continuously-firing signal

A **hot path** is a handler wired to a signal that fires many times per user gesture, or
many times unattended per document: a viewport adjustment, a caret move, a keystroke, a
size allocation, a frame-clock tick, a paint. A change that wires one, or that adds work
inside one, must account for every applicable cell.

This is a latent gap of the purest kind. The handler is *correct* — it computes the right
answer every time — so no test goes red, no assertion can be written against "it is
right", and the happy path is not merely hidden but genuinely passed. What is wrong is the
**cost per emission**, and cost is invisible on the document a feature is built against: a
20-line fixture makes a whole-document re-parse free. It appears only at scale, and it
appears as a symptom no one attributes to the handler — *the app hangs when I resize*, *the
UX is ridiculously laggy*.

**Measured, and the reason this matrix exists.** The outline's scroll-spy re-derived the
whole document on every `value-changed` of the preview's vertical adjustment:
`on_scroll` → `deepest_visible_row_pos` → `current_heading_levels` →
`outline::extract_headings`, a full Markdown parse. That made the cost quadratic in
document length — and worse than "per scroll", because GTK validates a large buffer
incrementally and nudges the adjustment once per validated line, so merely *opening* a file
drove it. On this project's own `sdd/TDD.md` (3,987 lines) it called `extract_headings`
**4,001 times** and held the main loop at 100% for **20 seconds** in a release build, during
which the window did not paint. The fix was one line of reading — the same parse's result
was already cached for the caret path, and only this reader had been missed.

**The columns are OBLIGATIONS, not event classes** — unlike every matrix above. A hot path
has one event by definition (its own signal), so what varies is what the handler owes:

- **B — bounded per emission.** The work is O(1) or O(log n) in document size. No
  whole-document parse, no whole-document scan, no walk of a model whose length is the
  document's.
- **C — reads a cache with ONE refresh choke point.** The derived value is computed where
  the document *changes* — which is a Derived-view CAM row's choke point — and read here.
  Two caches of one fact, or a cache written from two places, is the drift this forbids.
- **S — staleness named.** What invalidates the cache and which event refreshes it, stated
  where the cache is declared. A hot path may legitimately read a value one render old;
  what it may not do is leave the window unstated.
- **D — coalesced.** Where the work genuinely cannot be bounded, it runs on a debounce or
  an idle with a stated cap and a generation guard, never once per emission.
- **M — measured at scale.** Proven on a document large enough for the cost to show, with
  the number written down. A fixture is not a measurement, and "it feels fine" is not
  either.
- **F — fresh in every mode and after every mutation.** The cache column C introduces is
  read in **edit-only, split and preview-only**, and after each of the Derived-view CAM's
  four event classes — an in-session edit (A), a save / open / external reload (B), a
  view-mode switch, theme or zoom (C), and a tab switch, cross-window move or session
  restore (D). Proven in each, not argued from the refresh's call sites.

**F is the cell this matrix exists to pair with C, and the order matters.** A hot path
that re-derives is *expensively* correct: it cannot be stale, because it reads the
document every time, so every mode and every mutation is satisfied by construction and
nobody has to think about it. Replacing that derivation with a cache read removes the cost
**and introduces a staleness surface that did not previously exist** — the handler now
depends on someone else having refreshed, in a mode it may not have been tested in, after
an event nobody enumerated. So the fix for a B/C violation is not complete when the
measurement improves; it is complete when F is proven. A hot path is thereby pulled into
the Derived-view CAM's jurisdiction the moment it starts reading a cache, and the two
matrices have to be satisfied together.

| # | Hot path | B | C | S | D | M | F | Anchor |
|---|---|:-:|:-:|:-:|:-:|:-:|:-:|---|
| 1 | Viewport adjustment `value-changed` — outline scroll-spy, split scroll-sync, wheel coalescing, far-scroll settle | ✓ | ✓ | ✓ | — | ✓ | ✓ | `outline_nav::on_scroll`; `scrollsync`; `wheelcoalesce`; `farscroll`. F: Derived-view row 2's own choke points refresh it — `livepreview` (A), `reload`/`swaprecovery` (B), `viewactions` (C), `tabs::switch`/`window::new_window`/`app::setup` (D) |
| 2 | Buffer `changed` (once per keystroke) — dirty status, word/line counts, the Ln/Col indicator (an edit carries the caret without `mark-set`; one line's length per emission), live-preview re-render, crash-recovery snapshot | — | ✓ | ✓ | ✓ | ✓ | ✓ | `statusbar::note_buffer_changed` (debounced, generation-guarded, counted off-thread); `window::swap` |
| 3 | Editor buffer `mark-set` (the caret and selection-bound moves) — Ln/Col indicator, outline caret spy, formatting overlay | ✓ | ✓ | ✓ | — | ✓ | ✓ | `tabs::lifecycle::wire_tab_buffer_signals`; `refresh_position_indicator`; `outline_nav::editor_cursor_doc_index` (binary search over `TabState::heading_index`, whose entries carry each heading's character offset, so a caret move copies nothing). What bounds the cost is the emission count. `mark-set` fires for EVERY mark, and GtkSourceView's highlighter sets thousands on a large document, so the handler returns early for anything but `get_insert()` / `selection_bound()`. M: a 3 MB document switched to the editor mid-load, release, Xvfb, 2026-10-11 — the main loop was busy ~88 s without the filter and ~2 s with it. Windows froze for ~130 s without it |
| 4 | `size_allocate` / content-column change — anchored-child bounds, table width binding | ✓ | — | ✓ | — | ✓ | — | `codeview::CodePreviewView::size_allocate`; `set_bound_width` (re-binds only on a real width change — GTK4Rs/AP-23). Reads no document-derived cache, so F does not arise |
| 5 | `snapshot_layer`, once per frame per decoration | ✓ | — | — | — | ✓ | ✓ | `decorplan::PAINT_ORDER`; every painter is gated to the VISIBLE range before it measures (which is also the GTK4Rs/AP-22 correctness rule, so the two agree). F: each decoration vector is REPLACED by its `set_*` on every render, and `DRAWN_VECTORS` is what keeps a new one from being forgotten |
| 6 | Frame-clock tick (`add_tick_callback`) — animation frames, sprite advance, scroll settle | ✓ | — | ✓ | — | — | — | `animation::tick`; `animation::sprites`; `farscroll::settle` |
| 7 | Pointer motion / hover — copy-button, checkbox and marker hit-boxes, and the hovered link's target (a table-cell link costs one `pick`, asked only when the render holds a table) | ✓ | ✓ | ✓ | — | — | ✓ | `codeview`'s `*_hitboxes`, repopulated per paint for the visible rows only and cleared by the setter that invalidates them |
| 8 | Sidebar filter `search-changed` — once per settled keystroke in the outline's or the annotations viewer's filter box | — | ✓ | ✓ | ✓ | ✓ | ✓ | `window::sidebarfilter::on_search_changed` → `rebuild_outline_list` / `rebuild_annotations_list`. B: a list rebuild is O(headings or annotations), the size of what is filtered. C: reads `TabState::outline_headings` / `annotation_entries`, written only by `refresh_outline` / `refresh_annotations` (Derived-view rows 1 and 3) — never a re-parse. D: GtkSearchEntry's fixed ~150 ms delay (4.6 has no `search-delay`); the rebuild is synchronous and reads the current filter, so no stale result can land. M: `sdd/TDD.md`, 626 headings, release, 2026-09-29 — 0.8 ms for a narrow query, 36 ms for a one-letter query keeping 568 rows. F: the caches are the rows' own choke points, so every mode and event class that refreshes a row refreshes what this reads. **Not swept** when written (2026-09-29): the find bar's `search-changed` is the one earlier handler of the same kind, and it was not walked against this row. Insert Link's Section picker filter (`editbar::section`) is the same shape: B O(headings); C reads the headings captured when the dialog opened, never a re-parse; D the same entry delay |
| 9 | Preview selection change — the buffer's `mark-set` and the display's PRIMARY `changed` (which is how a table cell's selection is observed, GTK4Rs/AP-28): an anchored image's selection tint, and a table cell's selected-text ink | ✓ | — | ✓ | — | ✓ | — | `preview::interactions::connect_selection_marks`; `widgets::table::linkink`. B: the `mark-set` handler compares one buffer offset per anchored image and table — the image half predates this row — and a table touches its cells only when its answer changes, reading the body's selection ink once per emission rather than once per cell; the PRIMARY handler re-inks at most two cells (the focused one and the last one inked), never a walk of the document's cells. S: a cell's link ranges are fixed for its widget's life, since a re-render builds new cells. M: 500 tables (1,000 body cells), release, Xvfb, 2026-09-30 — 0.39 ms median per selection change, 10.5 ms for the one emission that flips every table (Select All, and its clearing). PRIMARY is display-global, so B is also owed across LISTENERS, not only per listener: every preview and every window hears every claim. An unmapped preview (a background tab, or a pane the view mode hides) skips the work and catches up on `map` without raising the Annotate card (a minimized or other-desktop window stays mapped — X11, GTK 4.6.9, measured — and does the work); a window that is not active skips its Copy/Annotate/Find-scope re-derivation and catches up on activation (`window::actions::connect_selection_states_on_activation`). M: a session of 11 windows and 57 tabs, release, Xvfb, 2026-10-29 — app CPU over a 100-step drag 29 ticks before, 18–20 after. Reads no document-derived cache, so C and F do not arise |
| 10 | Editor vertical adjustment `changed` and `value-changed` — the scrollbar relink (TDD 4.14), and the formatting overlay's scroll-hide (TDD 10.8); the preview vertical adjustment's `value-changed` — the Annotate pop-up's scroll-hide (TDD 17.5) | ✓ | — | — | ✓ | ✓ | — | `window::splitview::relink_vscrollbar_after_range_changes`: O(1) per emission (a hide/show of one widget), coalesced to one repair per main-loop turn plus one repeat. MEASURED on a 4,000-line document: CPU over 300 wheel notches varied more between runs than between on and off; the scrollbar stays painted through a continuous scroll. `editbar::overlay::wire_editor_format_overlay`'s scroll-hide is one truncated-value compare and at most one `popdown` per emission, uncoalesced (it must hide in the frame the text moves); `preview::annotate::overlay::wire_annotation_overlay`'s scroll-hide is the same compare through the same `saferizer::viewport::moves_painted_text`. Reads no cache, so C/S/F do not apply |

Rules that give the matrix its teeth:

- **"It is only called on scroll" is the claim to distrust.** A signal you reason about as
  user-driven is also driven by the toolkit: GTK's incremental validation moves a scroll
  adjustment, a theme switch re-allocates, a reflow re-snapshots. Count the emissions on a
  real document rather than reasoning about who causes them — and count them by
  instrumenting the handler, not by estimating.
- **The cache belongs to the writer, not to the reader.** A hot path that needs a derived
  value does not compute it and memoise it locally; it reads the cache the Derived-view
  CAM row that owns that value already refreshes. This is what makes the two matrices one
  system: Derived-view says *when* the value is recomputed, this one says *that* nothing
  else may recompute it.
- **A second reader of an existing cache is a cell, not a free change.** The defect that
  produced this matrix was not an uncached value — the cache existed and was already
  correct. It was a *second reader* of the same fact that re-derived instead. When adding
  one, look for an existing cache of that fact before writing the derivation.
- **Bounded beats coalesced.** A debounce hides a cost rather than removing it, and it
  buys nothing for the emissions the toolkit drives in a burst — they all land after the
  timer too. Reach for column D only when the work is irreducibly whole-document, and then
  give it a generation guard so a stale result cannot overwrite a fresh one.
- **A mode is not a context you can reason your way past.** The three view modes read the
  document from two different places — `TabState::shown_source` answers the editor buffer
  in edit and split and the stored source in preview — so a cache built in one mode and
  read in another is the specific way F fails, and it fails invisibly: the outline still
  highlights *a* heading. Drive each mode.
- **Swept** when written (2026-09-20): all seven rows above were walked against the shipped
  code. Six were already satisfied; row 1's scroll-spy was the single defect, and it is
  fixed in the change that adds this matrix. Rows 5 and 6 carry no **M** because their cost
  is bounded by the viewport rather than by the document, which is the property the
  footprint gate and `decorplan`'s visibility gates already measure.

## Multi-surface setting CAM — a setting the reader can change from more than one place

A **multi-surface setting** is a value the reader chooses (on/off, or one of N) that more
than one surface both *changes* and *shows*. The toolbar layout is one: its whole-bar
**Show** and six per-section toggles are offered by View ▸ Toolbar and by the toolbar's
own right-click menu (TDD 9.22, 9.40).

**Why the Action CAM does not cover it.** The Action CAM decides which surfaces a
*command* must appear on, and requires one `GAction` and one enabled state behind them.
A setting owes four things a command does not, and each is a latent gap — the surface
that was built works, the other looks finished:

- **the same members on every surface.** A setting is usually a *family* (six sections,
  N themes). A member added to one surface's list and not the other's is offered in one
  place only, and nothing fails.
- **the value, not only the enabled state.** Every surface shows the current tick, radio
  or pressed state, and keeps showing it while it is open, whoever changed it.
- **the setting's rules, from every surface.** A rule that reinterprets a change (the
  toolbar's last-section rule turns "hide the last section" into "hide the bar") holds
  wherever the change is made, not only where the rule was first written.
- **one value everywhere it applies.** Every window that shows the setting, and the next
  launch, see the same value.

The surfaces (matrix columns):

- **M — menu bar.** The in-window `GtkPopoverMenuBar`, and on macOS the native menu bar
  built from the same model (TDD 9.35). One column, because they are one model.
- **X — context menu on the thing the setting changes.** Opened by a right-click on the
  affected chrome (the toolbar).
- **P — a persistent control.** A toolbar toggle, a picker or a find-bar check showing the
  value at all times. The toolbar layout has none: a control on the bar that hides the
  bar would vanish with it.

| # | Obligation | M | X | P | Choke point / anchor |
|---|---|:-:|:-:|:-:|---|
| 1 | **Built from one enumeration.** Every surface walks the same list of members, in the same order and grouping, so a member added there reaches every surface. No surface lists members by hand | ✓ | ✓ | — | `app::commands::toolbar_toggles` (built from `TBTN_SECTION_IDS`); `app::menubar::build_toolbar_layout_menu`; `window::toolbarmenu::open_toolbar_menu` |
| 2 | **Binds the setting's own action by name.** No per-surface handler, no per-surface copy of the value, no `set_sensitive` | ✓ | ✓ | — | *reference row* → POLICY "One action per command"; the `app.show-toolbar` / `app.show-tbtn-<id>` actions in `window::toolbarchrome` |
| 3 | **Shows the current value, live.** The tick follows the action's state, including a change made elsewhere (another window, the other surface) while this one is open | ✓ | ✓ | — | GTK's actionable binding (menu model check item; `GtkCheckButton` with an action name) — nothing here mirrors state |
| 4 | **Greyed exactly when the action is disabled** — never from the surface's own logic | ✓ | ✓ | — | `toolbarchrome` I3 (section items disabled while the bar is hidden). Unreachable on X today: the menu is on the bar, which is shown whenever it can be right-clicked |
| 5 | **The setting's rules hold from every surface**, because they live in the action's `change_state`, not in a surface's handler | ✓ | ✓ | — | `toolbarchrome::add_section_action` (the last-section rule, I8) |
| 6 | **Same labels and access keys** on every surface that has access keys, unique within each surface | ✓ | ✓ | — | `app::mnemonics::MENU_MNEMONICS` via `mnem`; `commands` test `toolbar_toggles_each_have_a_distinct_access_key`; the menu bar's `menu_access_keys_unique_per_popover` |
| 7 | **One value in every window, and across a relaunch** | ✓ | ✓ | — | *reference row* → `toolbarchrome` I1–I5 (app-wide) and the session (TDD 9.22, 7.2) |
| 8 | **A test compares the surfaces with each other** — the actions one surface binds against the actions another binds, read from each surface as built, never from the shared list (which would compare the list with itself) | ✓ | ✓ | — | `window::toolbarmenu` test `the_toolbar_menu_offers_the_view_toolbar_toggles` (reads the shipped View ▸ Toolbar model) |

Rules that give the matrix its teeth:

- **Two kinds of change enter this matrix, and both owe every applicable cell.** A new
  *member* of an existing setting (a seventh toolbar section) owes rows 1, 6 and 8 a check
  on every surface — row 1 is what makes that one edit, row 6 needs its access key added to
  `MENU_MNEMONICS` (the unit test fails until it is), row 8 confirms both surfaces grew. A
  new *surface* for an existing setting owes every row in its column.
- **A surface that cannot reach a member says so, it does not drop it silently.** If a
  surface must offer a subset, the subset is computed from the shared enumeration and the
  reason is stated beside it.
- **A rule added to the setting goes in the action, never in a surface.** Written in a
  surface's handler it holds there only — row 5's failure, and the hardest to see, since
  the surface that has it is the one its author tested.
- **A `tests/MANUAL-TEST.md` check per applicable ✓** — the cross-CAM rule. For the toolbar
  layout they are 9.22a–l (M) and 9.40a–d (X).

**Back-sweep: not done when written (2026-10-24), and stated here so the matrix does not
read as satisfied by the whole tree.** The toolbar layout, the member the matrix was
written for, satisfies every cell above. Other settings already offered on more than one
surface were not walked against it: the view mode (View menu, toolbar), the Outline and
Annotations panes (View menu, toolbar, the pane's own close button), Show Unsafe Images
(View menu, toolbar), the reading theme (View menu radios, the toolbar picker, which binds
a stateless shim rather than the setting's own action — a row 2 question to settle when it
is swept), the split arrangement (View menu, toolbar) and the find options (Edit menu, find
bar). A change to any of them sweeps it against this matrix in that change.

## Granted CAM exceptions

A deviation from any matrix above must be requested and explicitly green-lit by
the operator. Each approved deviation is recorded here so it is not re-litigated.
This list records only the approved deviations; the matrices themselves are the
rule.

- **No toolbar button for the "no state to show" command family** — **GRANTED
  (operator, 2026-08-15).** The Action CAMs' "Other action" column requires a toolbar
  button. These four commands do not have one, and will not:

  | Command | Group | Surfaces it *does* have |
  |---|---|---|
  | `win.rename` | File | File menu, tab-strip context menu, `F2`, Keyboard Shortcuts window |
  | `win.close-tab` | File | File menu, tab-strip context menu, `Ctrl+W`, Keyboard Shortcuts window |
  | `win.go-to-line` | View | View menu, `Ctrl+G`, Keyboard Shortcuts window |
  | `win.next-annotation` / `win.prev-annotation` | Edit | Edit menu, accelerators, Keyboard Shortcuts window |

  **The argument, once, for all of them:** each is a command with **no state a button
  could usefully show**, in a toolbar already at its width budget. A toolbar button
  earns its place by being either a frequent target or a *status display*; these are
  neither — nothing about "Rename" or "Go To Line" is worth a persistent pixel, and
  their enabled state is already legible from the menu, which greys visibly (unlike a
  toolbar chevron, which is pixel-identical enabled and disabled — GEP-41).

  **Every other cell of the Action CAM is satisfied** for all four: one `GAction`, one
  enabled-state source of truth, and the accelerator surfaced everywhere the SSOT
  table reaches.

  **Three of these were deviating *unrecorded* before this entry.** `win.close-tab`,
  `win.go-to-line` and `win.next-annotation` each shipped without a toolbar button and
  without an exception; the sweep that added Rename is what surfaced them. That is the
  cross-CAM back-sweep rule working — and worth noting as the reason exceptions are
  recorded as a *family* rather than one at a time: a per-command entry would have let
  the other three stay invisible. If this is ever revisited, all four change together;
  a family that appears half in a surface is worse than one that appears in none.

- **No toolbar button and no accelerator for `app.play-animations`** — **GRANTED
  (operator, 2026-09-11).** View ▸ Play Animations is a menu-only command: no toolbar
  button, no keyboard shortcut, no entry in the Keyboard Shortcuts window, and no key
  on the animation itself.

  **The argument:** animations in Markdown documents are rare, so a persistent toolbar
  pixel is not earned, and the toolbar is already at its width budget. The keyboard was
  ruled out deliberately rather than left undone — the natural key, media Play/Pause,
  **does not reach a focused application on any shipping desktop** (X11 KDE and GNOME
  both grab it for an MPRIS player; Wayland compositors own it; GDK translates neither
  the macOS `NX_KEYTYPE_PLAY` nor the Windows `WM_APPCOMMAND`), and claiming it
  system-wide would make Scribobulate the session's "now playing" target, which is a
  product decision this is not.

  It follows the `win.show-unsafe-images` precedent exactly — same View-menu section,
  same menu-only shape — and satisfies every other Action CAM cell: one stateful
  `GAction`, process-wide, with every window's menu mirroring the one state, and no
  per-surface enablement anywhere.

- **Front matter does not export as it renders** (Document Rendering CAM row 17) —
  **GRANTED (operator, 2026-09-18).** The preview shows a document's front matter as a
  collapsed disclosure; an HTML or PDF export omits it entirely, as does the outline
  sidebar and the status bar's word count (TDD 2.27). Row 17, and rubric 2.26g behind
  it, would have the block reach an exported artefact exactly as the preview draws it.

  **The argument:** row 17 exists because a construct taught to the renderer alone is
  silently *absent* from an artefact, and absence is what nobody notices. That is a
  statement about content. Front matter is not content — it is what the document says
  about itself, addressed to a static-site generator rather than to a reader, and the
  generator that consumes it strips it from the page it publishes for exactly this
  reason. So the asymmetry is the feature: the preview shows it because the author
  editing the file wants to see it, and the export drops it because the person handed
  the artefact was never its audience.

  **Every other cell is satisfied by construction**, which is the reason this is the
  only deviation. The block reaches the renderer as synthetic events indistinguishable
  from an authored `<details>` wrapping a fenced code block (`renderer::frontmatter`),
  so it inherits the disclosure's and the code block's coverage of every container,
  theme, zoom, focus and mode cell rather than restating it. The two cells that are NOT
  inherited — row 5 (copy fidelity) and row 8 (find reach), both properties of the
  synthetic events' source ranges rather than of how the block is drawn — are covered
  by `preview::build`'s
  `front_matter_copies_as_its_own_source_and_stays_findable_while_collapsed`.

- **Annotate** (`win.annotate`, group `Edit`) — the approved deviation from the
  Action CAM is the command's presence in the **caret formatting overlay**, a
  Format surface an Edit action would not otherwise occupy. Justified because
  annotating a selection is ergonomically part of the same inline-editing gesture
  as formatting. The overlay and the Format toolbar section share one button, and
  every surface binds the single `win.annotate` action, whose enabled state
  remains the sole source of truth.

- **Sidebar filter commands** (`win.filter-outline`, `win.filter-annotations`, group
  `View`) — **GRANTED (operator, 2026-10-02).** Two Action CAM cells deviate, both by
  design:

  | Cell | Deviation | Why |
  |---|---|---|
  | Toolbar section | No toolbar button | Each pane's own header search toggle is the command's natural home; a toolbar button would duplicate it away from the pane it filters. |
  | Single `GAction` source of truth | The header toggle is bound to the search bar's `search-mode-enabled`, not to the action | The command pressed while the bar is open but unfocused FOCUSES the box (TDD 20.25), while a click on a pressed toggle closes it. One action cannot be both. |

  **Every other cell is satisfied**: one `GAction` per pane behind the View menu, the
  accelerator and the Keyboard Shortcuts window; the toggle carries the command's
  shortcut in its tooltip; the command is never disabled, so there is no sensitivity for
  the two surfaces to disagree about (`window/sidebarfilter.rs`'s module header).
