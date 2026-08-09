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
| 9 | Granted CAM exceptions | Operator-approved deviations, recorded so they are not re-litigated |

---

## Rules that govern all CAMs

- **A change may belong to more than one CAM and must satisfy each.** Annotate,
  for example, is all three: an Edit action, a Document Rendering feature, and —
  because the annotations viewer projects it — a Derived-view change.
- **Every applicable CAM cell must be covered by a `tests/MANUAL-TEST.md`
  check.** This is where the matrix gets teeth: build-pipeline step 7 already
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
  never had it (ScrAP-244).
- **A new row obliges a back-sweep of what already shipped, or an explicit record
  that it was not swept.** A CAM is consulted when a change lands, so a row added
  today is applied to future changes and to nothing else — every feature that shipped
  before it keeps whatever gap the row exists to catch, and no gate will ever ask.
  The matrix then reads as though the whole codebase satisfies it, which is the one
  claim a checklist must not make falsely. This is not hypothetical: Document
  Rendering row 8 (find/search) was written 2026-07-13, while the pure-link table cell
  it would have caught shipped in the initial commit and its find path a week later —
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
| 2 | Correct at top level **and** inside every container markup — table cells, block quotes, ordered/unordered list items, nested lists. **A container is not one context if it renders through more than one widget shape**: a table cell is a `GtkLinkButton` when its whole content is a link and a `GtkLabel` otherwise, and a feature built for one shape leaves the other silently inert while looking finished — enumerate the shapes and cover each, and give them one seam rather than one implementation apiece | ScrAP-259; ScrAP-250 |
| 3 | Composes with inline formatting in the same span without splitting a delimiter — **in BOTH tokenisers**: the pulldown-cmark constructs (bold/italic/inline-code/links/images) *and* the tight ones this crate scans itself (`==highlight==`, `~~strikethrough~~`, `^sup^`, `~sub~`), which reach any pulldown-driven decision as plain `Text` and are invisible to it. Applies on the **preview** path *and* the **editor** path — they balance through different code. The companion obligation is **extent**: the claim highlight must cover exactly the annotated characters even where the rendered text is shorter than its source (stripped construct markers) — body and table cell go through **one** shared mapper, so a fix cannot land on half of it | `copymap::wrap_span` (preview) + `copymap::balance_source_span` (editor); `annotate::map_cleaned_highlight_to_local` (extent, both paths); `renderer::scan_script_spans`; ScrAP-195/ScrAP-196 |
| 4 | Fully, atomically undoable/redoable — one edit is one undo step, consistent in every mode | TDD 9.16 |
| 5 | Copy/clipboard fidelity — selection→source mapping yields correct source text; preview copy excludes rendering artifacts | `copymap`; ScrAP-5 |
| 6 | Round-trips through save→reload **and** live external reload with stable on-disk syntax | TDD §3 / §5 |
| 7 | Does not break edit↔preview scroll-sync or selection mapping | `preview::sourcemap` |
| 8 | Find/search still matches the underlying document text through the markup — **in every container context of row 2, and whatever WIDGET the feature chose to render it with.** A rendering choice decides what find can reach: text the feature puts in the buffer is found by `forward_search`, text it puts in a widget is found only by a cell walk that recognises that widget's shape, and text one level deeper (a caption inside a button) is found by neither. So a feature that renders differently in a container owes a check that its text is still findable *there* — the body case passing says nothing about it (ScrAP-250). **And** the match highlights survive every preview-rebuild boundary — theme switch, view-mode switch (edit↔split↔preview), external reload, and tab switch — because each swaps in a fresh preview buffer that carries none of the overlay, so it must be re-applied (not left bare until the next match cycle), the same re-sync `refresh_outline`/`refresh_annotations` already get | `preview::cells::cell_search_targets`; `window::refresh_preview_find_highlight`; ScrAP-38/ScrAP-250 |
| 9 | Renders correctly under **every installed theme** — all colour, typography, and decoration geometry sourced from the active theme, never a literal | [`THEMING.md`](THEMING.md); POLICY "No hard-coded styling" |
| 10 | Stays within the footprint gate — a new rendering path is by definition a "significant change" | TDD §6 / footprint gate |
| 11 | **Interaction parity in container contexts** — every interactive surface the feature exposes (action enablement, an auto-popup/dismiss overlay, and click/hit targets like margin markers) behaves for a selection or target **inside a table cell** exactly as for the main buffer, driven off the cell's out-of-buffer selection signal (primary-clipboard `changed`, ScrAP-110), not only buffer signals | ScrAP-110 |
| 12 | **Rendering parity in container contexts** — one theme key feeds both the body-buffer path and the table-cell path (Pango markup + `bgalpha`, `u16` triple); no second literal, no drift | POLICY "One theme key, every application path"; ScrAP-36 |
| 13 | Survives **zoom** at every level — Pango scale for type, the `px()` path for pixel metrics; the theme never emits CSS `font-size` (zoom owns it exclusively) | ScrAP-127; ScrAP-64 |
| 14 | **Switches theme at runtime** without restart, in every open window | `re_render_all_windows` |
| 15 | **Legibility floor** asserted per theme (body contrast gate, headless) | `palette::contrast` |

Rows 9–10 and 13 are **reference rows**: they defer to gates that already own
those concerns ([`THEMING.md`](THEMING.md) and the "No hard-coded styling" rule in
POLICY.md; the footprint gate in POLICY.md and TDD §6; ScrAP-127/ScrAP-64).
They appear in the matrix so the obligation is not forgotten, but the CAM does not
restate their rules.

Row 12 is the rendering twin of row 11: row 11 governs a feature's *interaction*
inside a table cell, row 12 governs its *appearance* there. It earns its place —
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
| 1 | Outline tree (headings) | ✓ | ✓ | ✓ | ✓ | `refresh_outline` |
| 2 | Outline scroll-spy highlight | ✓ | ✓ | ✓ | ✓ | `wire_scroll_spy`; ScrAP-46/ScrAP-57/ScrAP-89 |
| 3 | Annotations viewer (flat list) | ✓ | ✓ | ✓ | ✓ | `refresh_annotations`; `preview::refresh_annotations_in_place` |
| 4 | Window title, tab label + tooltip, View ▸ Documents (menu **and** toolbar combo) | ✓ (dirty) | ✓ | — | ✓ | `update_window_title`; `refresh_active_tab_label`/`badge_tab_label`; `refresh_documents_menu`; `refresh_documents_button` |
| 5 | Status bar — dirty/conflict message | ✓ | ✓ | — | ✓ | `refresh_dirty_status` |
| 6 | Status bar — Ln/Col indicator | ✓ | ✓ | ✓ | ✓ | `refresh_position_indicator` |
| 7 | Find bar — match count and match highlights | ✓ | ✓ | ✓ | ✓ | `update_match_count_label`; `refresh_preview_find_highlight` (Document Rendering CAM row 8) |
| 8 | Crash-recovery notice (per-tab prompt + per-window status count) | ✓ | ✓ | ✓ | ✓ | `toast::sync_recovery_toast` |

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
  rebuild locally, so a new event cannot half-implement the refresh (the ScrAP-38 /
  ScrAP-108 shape). A new derived view must name its choke point here.
- **Take the cheapest correct path.** When the rendered text is invariant, use the
  in-place refresh rather than rebuilding the buffer — a needless `set_buffer`
  brings the repaint/scroll-jump family with it.
- **Deferral is legitimate only for surfaces that are not visible.** A background
  tab may carry stale derived state provided it is re-derived on activation (the
  `needs_render` / `materialize_deferred_preview` / `pending_external` replay path).
  Nothing on screen may be stale.

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
(ScrAP-13/115 family). The happy path hides it completely: the pane renders
perfectly, only the scroll is wrong, and only on the one event that forgot — the
textbook latent gap.

The two perturbation kinds need different *restore mechanisms*, never a different
*concern*:

- **Geometry change** (width): the text re-wraps, no buffer swap. The raw pixel
  `value` survives but no longer maps to the same logical line.
- **Content rebuild** (buffer swap): the buffer is new; `value` may or may not
  survive.

Both preserve the **same way**: capture the top buffer **line**, restore it after
validation. The only variable is view *warmth* — a warm, already-validated view takes
a deferred `scroll_to_mark`; a freshly-built or cold view with a far target needs the
progressive `set_value`-off-`notify::upper` restore, because a one-shot lands at the
top (ScrAP-115). **That warm/fresh choice is made once inside the choke point, never
by the call site.**

| # | Perturbing event | Kind | Editor | Preview | Status today |
|---|---|---|:-:|:-:|---|
| 1 | Zoom in / out / reset | rebuild | ✓ | ✓ | ✓ `rerender_and_restore_scroll` |
| 2 | Live-preview re-render (editor edit) | rebuild (in-place) | — | ✓ | partial — relies on `value` survival, no explicit restore |
| 3 | External reload — live **and** prompted | rebuild | ✓ | ✓ | ✓ `reload.rs` |
| 4 | Runtime theme switch | rebuild | — | ✓ | ✓ (reuses the reload path) |
| 5 | View-mode switch (edit↔split↔preview) | rebuild | ✓ | ✓ | ✗ uses `content_scroll_fraction` (drifts — should be the line anchor) |
| 6 | Tab switch / deferred materialize / session restore | host | ✓ | ✓ | ✓ (materialize path) |
| 7 | **Horizontal resize / window maximize-restore** | geometry | ◑ | ✓ | ✓ (preview) `size_allocate` raw-width re-anchor (ScrAP-162); editor pane not yet covered |
| 8 | **Split-pane drag** (divider move → the preview pane's width changes) | geometry | ◑ | ✓ | ✓ (preview) — same re-anchor; the width key is **cause-agnostic**, so #7's fix covers this too. Live-verify pending |
| 9 | **Sidebar toggle** (show/hide outline / annotations → the preview pane's width changes) | geometry | ◑ | ✓ | ✓ (preview) — same cause-agnostic re-anchor as #7. Live-verify pending |
| 10 | **Crash recovery applies a snapshot** (buffer replaced with recovered content) | rebuild | n/a | n/a | n/a **by position in the lifecycle, not by exemption** — see below |

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
  that hand-rolls its own capture/restore is the ScrAP-108/ScrAP-130 latent-regression
  shape — the next perturbing event added will forget it, and a happy-path test won't
  catch the silent jump.
- **Line, not fraction, for same-buffer preservation.** A pixel fraction mixes tall
  and short lines and drifts (ScrAP-65), and snaps to the top while
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

| # | Held reference | Points into | A | B | C | How it survives |
|---|---|---|:-:|:-:|:-:|---|
| 1 | Annotation card's Remove / Edit target | source bytes | ✓ | ✓ | ✓ | `AnchoredSpan` — carries the construct's own text and re-resolves at apply time; mutations total (ScrAP-187) |
| 2 | Annotations viewer's selected row | source bytes | ✓ | ✓ | ✓ | Stored as the annotation's start byte and **re-resolved against a fresh scan on every rebuild**; a vanished annotation simply loses the selection |
| 3 | Task-checkbox toggle span | source bytes | ✓ | ✓ | — | The toggle re-locates a well-formed marker at the span and returns `None` otherwise, which the caller makes a clean no-op |
| 4 | Pending marker-open request | marker-list **index** + buffer offset | ✓ | ✓ | ⚠ | Bounded by a wall-clock deadline and re-aimed each frame, but the target is a **positional index** into a list a re-render replaces — see the rule on positional references below |
| 5 | Scroll re-anchor target line | buffer line | ✓ | ✓ | — | Re-read each frame; a drifted line mis-positions the viewport only, and the next settle corrects it |
| 6 | Back/Forward history entry's **place** in a document (TDD §23) | heading **slug**, or a buffer line | ◑ | ✓ | ✓ | Two strengths, chosen by what the recording site can know. A slug is re-resolved against the tab's live heading map, and a render that no longer contains it **degrades the entry to "just this document"** rather than letting it point somewhere wrong (23.14). A line is the weak form — an arbitrary scroll position offers no stronger handle — and takes row 5's bargain: it clamps and mis-positions the viewport only |

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
- **Refusing is not enough — resolve where you can.** A held reference that gives
  up whenever the document moved makes the feature useless in exactly the session
  where it is most used. Prefer re-resolution (nearest match by identity) and
  reserve refusal for genuinely absent referents.
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
| 3 | **Save's guard read** | ✓ | ✓ | ✓ | ✓ | ✓ | `DocEpoch` ticket **plus a path re-check**, re-issuing rather than acting |
| 4 | **Save's write** | ✓ | ◑ | ✓ | ✓ | ✓ | `WriteGate` (drop, not queue); explicit `Rc<TabState>`; tab-scoped completion |
| 5 | **Crash-recovery snapshot write** | ✓ | ◑ | ✓ | ✓ | ✓ | `swap.in_flight` + latest-wins coalescing; `tab_by_id` |
| 6 | **Startup recovery pass** | ✓ | ✓ | ✓ | ✓ | ✓ | runs once; bumps `DocEpoch` on apply; re-resolves windows/tabs after each await |

Rules that give the matrix its teeth:

- **Mutations announce; deferred readers check.** Anything that changes a document's
  content or its baseline calls `DocEpoch::bump`; anything that will *apply* a deferred
  result checks its ticket first and **discards** on a mismatch. Discard, never merge —
  a superseded answer carries no marker distinguishing it from a current one, so there
  is nothing to merge on. One counter gives both properties, because a reader takes its
  ticket *by* bumping ("I am the newest reader").
- **A write never checks; only readers can be superseded.** A completed write produced
  the bytes on disk, so its own baseline update is the truth by construction.
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
  either direction (ScrAP-244).
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
| 5 | "File deleted on disk — save to restore it" | **timed** (~6 s) | ✓ | ✓ | ✓ | ✓ | `app/open.rs` |
| 6 | Operation-in-progress ("Saving…" / "Reloading…" / "Opening…") | **the operation ends** (`Drop`) | ✓ | ✓ | ✓ | ✓ | `winstate::BusyNotice` — armed, not shown: nothing appears unless the operation outlives `BUSY_NOTICE_DELAY`, so a fast save never blinks. `Rc`-backed so ONE notice spans a logical operation made of several futures (the save guard's read, the decision, the write) |

**Every timed row (3, 4, 5) holds B and C through one mechanism:
`WindowChrome::push_timed_notice`.** It captures the chrome that issued the handle
(weakly) and retracts against *that* stack, so no timed notice re-resolves a stack at
fire time. The three rows previously resolved the tab's chrome *when the timer fired*,
which reads the tab's **current** window: a tab moved inside the notice's lifetime popped
the origin's handle out of the destination's stack (column C), and a tab *closed* inside
it upgraded to nothing so the pop never ran at all (column B) — both leaving the origin's
footer line up permanently, with no error. Guarded by TDD 16.8 and its two
`winstate/chrome.rs` tests, one of which carries a positive control proving the
re-resolving shape does strand the notice.

Row 5 is here because it was **missing** from this matrix while the two rows either side
of it were being examined — it is the same notice, in the same shape, written by a
different hand in a different module. A matrix omits what nobody thought to look for, so
when a row is added, grep for the *mechanism* (here: every `status…push` paired with a
timer) rather than enumerating the notices you can remember.

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

## Granted CAM exceptions

A deviation from any matrix above must be requested and explicitly green-lit by
the operator. Each approved deviation is recorded here so it is not re-litigated.
This list records only the approved deviations; the matrices themselves are the
rule.

- **Annotate** (`win.annotate`, group `Edit`) — the approved deviation from the
  Action CAM is the command's presence in the **caret formatting overlay**, a
  Format surface an Edit action would not otherwise occupy. Justified because
  annotating a selection is ergonomically part of the same inline-editing gesture
  as formatting. The overlay and the Format toolbar section share one button, and
  every surface binds the single `win.annotate` action, whose enabled state
  remains the sole source of truth.
