# Plan: HTML `<details>`/`<summary>` collapsible disclosure blocks

**Status**: **IN PROGRESS** on `feature/details-disclosure`. The mechanism is decided,
the core renders and toggles in the running app, and the maps built alongside the
buffer now honour the suppression. What is left is enumerated under
[Remaining work](#remaining-work).

**Landing**: this branch carries many working commits and must reach `master` as
**exactly one**, via `git merge --squash` — never a fast-forward, never a partial
cherry-pick (POLICY § One commit per batch). **Ratified as ONE batch by the operator**,
image cache and `lint-references` gate mechanism included: both were reached through
this work and share its verification rig, so they land inside its single commit rather
than beside it.

**Requested by**: the `farming` knowledgebase agent, to collapse verbose content
(ASCII-art fallbacks beneath rich SVG versions, logs, "show your work" detail) while
keeping a document scannable.

## Problem

Scribobulate drops all raw HTML except `<picture>`/`<img>` — everything else is
sanitized by omission (ScrAP-147, TDD 2.23). A document using `<details>`/`<summary>`
rendered with the disclosure markup gone and its body always expanded, losing the
authoring intent entirely.

Three constraints shaped every option, and two are absolute:

1. **No HTML engine.** POLICY forbids WebKitGTK/Servo/litehtml. The preview is a
   `GtkTextView` subclass with self-drawn chrome, so "collapsible" means buffer text
   plus drawing, never an embedded browser.
2. **Height-for-width block content cannot be an anchored widget** (ScrAP-23/ScrAP-23a).
   The body cannot become a `GtkExpander`.
3. **Raw HTML is sanitized by omission, deliberately.** Widening the allowlist is a
   security-posture decision. `<details>`/`<summary>` clear that bar (structural, no
   scripting surface, no URL-bearing attributes, `open` is boolean) and the operator
   approved them on that basis. It must remain an allowlist.

## The mechanism, and why

✅ **A collapsed body is never rendered.** Collapsed state is a render input; toggling
re-renders with the body's events suppressed. No invisible text ever enters the buffer.

❌ **`GtkTextTag:invisible` over the body — rejected on measurement.** This was the
originally ratified approach. The researcher measured GTK 4.6.9's AT-SPI text interface
at the bus (Xvfb + private a11y bus + pyatspi; no Orca and no real desktop needed) with
a fixture of `AAAA` + invisible `XXXX` + `BBBB`, caret at 12:

| Reading | Value |
|---|---|
| `CharacterCount` | 12 (raw) |
| `GetText(0, CharacterCount)` | `AAAABBBB` — **8 characters, not 12** |
| `GetCharacterAtOffset(4..8)` | `U+0000` at every hidden offset |

The offset space is inconsistent and it is client-visible. 4.18+ inverts it (predicted,
unmeasured: `CaretOffset` can exceed `CharacterCount`). Beyond that, `invisible` makes
`line_yrange`'s zero height permanently ambiguous with "not yet validated", which is a
trap for any future geometry-polling code, and GTK's own `gtktexttag.c` caveat about
programmatic navigation in buffers with invisible segments is unchanged from 4.6.9
through 4.22.4. Not rendering the body deletes all of it, plus the anchored-child
remove/re-add plumbing, by construction.

❌ **Whole block as an anchored `GtkExpander`** — this is ScrAP-23 exactly. The body
stops being buffer text, losing find, selection, copy-as-Markdown and all chrome.

❌ **Buffer mutation (delete/re-insert the body)** — shifts every downstream character
offset on every toggle, tangles a view-only affordance with undo, and re-enters the
`insert_range` hazard behind ScrAP-199.

### The affordance is a real anchored widget

Settled by measurement (`probes/textview-anchored-toggle.c`, GTK 4.6.9), which also
narrowed [`PLAN.accessibility.md`](PLAN.accessibility.md)'s anchored-child rule to the
mechanism that actually bites:

| Config | View minimum width |
|---|---|
| text only | 0 |
| text + 8 toggles (~30px each) | **30** — `MAX`, never a sum |
| text + one table-sized child | 900 |

`hadjustment.upper − page_size` measured **0.0**, so the ScrAP-23a overflow chain is not
reachable through small anchored children. Note the rule's old justification was wrong
in an instructive way: a bare wrapping `GtkTextView`'s own minimum is **0**, so a small
child does not sit "below the view's floor", it raises the floor from 0 to its own
width. It is harmless because that figure cannot arm the overflow chain.

**Four properties are not optional**, each learned by driving the app rather than from
a test, and each now encoded in `widgets/disclosure.rs`:

1. **Compose `GtkToggleButton`.** A widget that only calls
   `gtk_widget_class_set_activate_signal()` gets Space unconditionally but Enter only
   while the window has no default widget, because `activate-default` falls back to the
   focus widget (`gtkwindow.c:2455-2464`). Enter then works in development and silently
   stops in any real dialog. `GtkToggleButton` sets `receives-default` and installs the
   five keyvals itself. Verified with a default button present: Space 9, Enter 6, mouse 7.
2. **Swap the indicator** (`pan-end-symbolic` ↔ `pan-down-symbolic`). The icon is the
   entire feedback channel: a build that emitted `toggled` 29 times without changing its
   arrow was reported from a live session as doing nothing at all. The RTL variant is a
   separate icon name rather than a mirror.
3. **The whole summary LINE is the click target**, not the indicator. The arrow is
   ~16px and should stay that way — it reads as an indicator in prose — which makes it
   a poor thing to aim at; a browser makes the whole `<summary>` clickable for the same
   reason. Hit-tested by buffer LINE (`line_at_y`, not `iter_at_location`, which is a
   glyph test and answers nothing past the end of a line), through the
   `saferizer::ClickActivation` complete-click seam so drag-selecting the label does
   not fold the block out from under the selection. A press that landed on the control
   is left to the control (ScrAP-79): the button emits `toggled` itself, and a second
   activation path would flip the fold twice and leave the block as it was — which
   reads as the arrow doing nothing. The line must also HOVER as a control: it is
   ordinary buffer text, so without a term in `interactions::is_clickable_at` the very
   area just made clickable keeps the I-beam and reads as inert.
4. **Carry its own cursor and baseline.** An anchored child sits inside the text area and
   does not participate in the view's hover machinery, so without `pointer` it hovers as
   an I-beam and reads as unclickable; without `valign: Baseline` the indicator floats
   above its own summary text.

Accessibility work beyond this is **deferred to [`PLAN.accessibility.md`](PLAN.accessibility.md)**
by operator decision, including verifying `STATE_EXPANDED` across the bus.

## What has landed

- `src/fold.rs` — the display-free model. Keyed on the source byte offset of a
  disclosure's opening raw-HTML block: stable across zoom, theme, view-mode and
  live-preview re-renders, and cleared by `TabState::set_source` because a changed
  document moves every key. That also matches HTML, where a disclosure's state is the
  `open` attribute and therefore a property of the document.
- `src/renderer/rawhtml.rs` — the allowlist, extracted from `picture.rs` and widened.
  One owner, because it now feeds two scanners and two sinks.
- `src/renderer/disclosure.rs` — the scanner, including summary-text extraction. The
  label lives inside the raw HTML and never reaches the ordinary event path.
- `src/widgets/disclosure.rs` — the toggle.
- Suppression at `Renderer::process`, with raw-HTML events exempt so `</details>` can
  still end it.
- **`Renderer::collapsed_site`, and every map built alongside the buffer honouring it.**
  `preview::build` builds the copy map, the per-cell copy maps, the source map and the
  heading index from the SAME event stream the buffer is built from, and a collapsed
  body reaches the buffer as nothing — so an entry recorded for it claims positions
  belonging to whatever came next. MEASURED before the gate: a selection after a
  collapsed block copied the block's source, and `copymap::debug_verify` reported 1:1
  leaf drift on every such document. A collapsed block now earns exactly one copy node,
  widened at `</details>` to cover its whole source, which is what makes TDD 2.8i true
  rather than merely stated.
- **`outline::HeadingSite`** — one entry per heading the SOURCE declares, so the
  outline's `doc_index` cannot slip. MEASURED before it: three source headings against
  two rendered ones meant activating the hidden heading scrolled to the one after it
  and activating that one did nothing. A hidden heading carries the summary line of the
  block hiding it plus the fold chain to expand (TDD 12.22).
- **`disclosure::scan_document`** — the document's `<details>`↔`</details>` pairing,
  scanned before the walk. The walk is forwards and one-pass, so at a `<details>` the
  renderer cannot know whether the block is ever closed; assuming it is meant an
  unclosed one never popped its frame and **suppressed every remaining event**.
  MEASURED: `before` + `<details><summary>S</summary>` + a body + `## After` rendered
  as `before` and the summary line, and nothing else — rubric 2.26d's exact
  prohibition, and for an untrusted document one stray tag blanking the page. An
  unpaired block now renders its label with **no toggle**: a control that cannot fold
  anything is a control that lies, and the only thing it could fold is the rest of the
  document. Browsers close such a block implicitly at the end of its parent;
  deliberately not copied, for the reason above.
- **Find reaches a collapsed body** (TDD 11.10). `PreviewHit::Hidden` is a third hit
  source beside the buffer sweep and the table-cell walk — the only one that searches
  something other than a widget, because a collapsed body is in no buffer and no label.
  It carries no coordinates: stepping onto it EXPANDS and re-enters, and the rebuilt
  list holds the real match in its place. Expansion at navigation rather than at search
  time is an operator decision — typing a common letter must not expand every block in
  the document. The resume is re-entrant, so a match two collapsed levels deep is
  reached in one gesture.
  The query is matched against `disclosure::body_plain_text`, the text the body WOULD
  render as, never its Markdown source: searching the source finds `*` in every
  emphasised word and misses `foobar` in `foo*bar*`, and both of those are visible to a
  reader as a wrong answer. Its residue is stated at the function — content that
  renders into a WIDGET (a table cell, an image alt) is reported as its source text.
- **Export carries the summary** (TDD 2.26g). `Block::Disclosure` groups the label with
  its body, so an HTML artefact holds a real `<details>` whose `open` attribute follows
  the DOCUMENT rather than the reader's fold state, and a PDF — having no such
  affordance — lays every body out. The export consults the same
  `disclosure::scan_document` the renderer does, so the two sinks cannot disagree about
  which blocks are real. What was actually missing was narrower than "the construct":
  the body had always exported correctly, being ordinary Markdown events, and only the
  label was absent — which is why the artefact looked finished (CAM row 17 now records
  that a construct can be half-taught).
- `src/window/foldreveal.rs` — expand the folds hiding something, re-render, then act on
  the render that results. A **chain**, not one key: a collapsed block nested inside a
  collapsed block renders nothing at all, so opening the outer one alone reveals only
  the inner one's summary.
- **The indicator is themed** — `disclosure_marker_color` (one key, both states, the
  same shape and reason as `list_task_marker_color`), `disclosure_glyph` /
  `disclosure_expanded_glyph` and their sprite twins resolving independently, and
  `disclosure_marker_size` replacing the `INDICATOR_PX` constant whose own comment
  already claimed it was themed. Reuses `theme::decor`'s `MarkerChoice`, so the
  precedence is not written here and `candidates()` hands back ORDERED RUNGS — a
  sprite that will not decode falls to the glyph rather than erasing the arrow, which
  matters most for this control because the arrow is its entire feedback channel.
  Declared `preview_only`: a PDF has no disclosure to mark and an exported
  `<details>`'s marker is drawn by the reader's browser.
- **`window::zoom::RenderShape`** — a re-render declares whether it changes the buffer's
  content or only its scale, which is what decides the valid reading-position anchor.
  A fold toggle changes the line count, so the long-standing line anchor named a
  different place afterwards: MEASURED, a reader parked mid-document opening a block
  above them was thrown to source byte 0. The source-derived `DocPosition` the mode
  switch already carries between panes now carries the reader between shapes of one
  pane (TDD 2.26h, Reading-Position CAM row 11).
- **The maps are recorded against the RENDERER's cursor**, not the buffer's length.
  `Renderer::end_offset()` is `tip()`, so a region render files its waypoints,
  highlight runs and copy nodes at the splice rather than at the end of the document.
- **`renderer::DisclosureExtent`** — where each disclosure this render DREW sits in
  the buffer: its summary line (terminator excluded, so a line-wide decoration has an
  extent to paint) and its body. A collapsed block earns an entry with an EMPTY body
  rather than no entry, because that empty span is exactly where an expansion writes.
  A block nested inside a collapsed one earns none — it drew nothing, and the frame is
  marked `emitted` on that path anyway, so the test is whether an ANCESTOR is
  collapsed. An unclosed one earns none either: its extent would name a region running
  to the end of the document, which is 2.26d's prohibition wearing a different hat.
- **`src/imagecache/`** — a URL-keyed texture cache in front of the one remote-image
  fetch, with the eviction/TTL policy extracted as a pure core. Required BY the splice
  rather than merely adjacent to it: the scratch walk re-visits every image tag, and
  the fetch it re-ran was synchronous and uncached on the main loop, so an uncached
  toggle would have swapped a visible scroll for a frozen window. Negative results are
  cached too, on a short TTL — without that a dead URL re-freezes on every toggle,
  and with a long one a transient failure reads as permanent for the session.
- **The collapsed line previews its body** (TDD 2.26) — the opening text, ellipsised,
  inked by `disclosure_preview_fg`, drawn from the same reduction `find` searches so a
  preview and a search agree about what a body says. It is real buffer text written
  inside the event whose copy node `</details>` widens, which is what keeps a copy over
  the summary line yielding the block's Markdown rather than the preview.
- **The summary line's BAND is themeable** (TDD 18.48) — `disclosure_band_color`,
  `disclosure_band_gradient_to_color`, `disclosure_band_sprite`,
  `disclosure_band_radius`, plus `disclosure_fg` for the line's ink. Mechanism B, not
  C: CSS reaches about ten widget nodes and the label is buffer text, so the fill is a
  DRAWN vector — `preview::build` projects each `DisclosureExtent::summary` into
  `ViewInstall`, `codeview::disclosurebands` paints it beside `codeview::bands`, and it
  is a `decorplan::PaintStep` and a `DRAWN_VECTORS` row (which is the early-return
  gate). The ink is a TAG registered immediately after `blockquote-ink`, so a summary
  inside a quote takes the narrower ink while a link in the label keeps its own. Both
  export sinks carry it: a `summary` rule in the HTML artefact, a `BlockFill` on the
  label's line in the PDF. The radius is `not_on_paper`, for `table_cell_radius`'s
  reason. Unlike the heading band it insets no text, so there is no padding key.
- `src/copymap.rs` — `classify` is exhaustive and understands raw HTML; the anchor
  coverage guard replaces a root check that compared the tree root against the value the
  root is constructed with and could never fail.
- `cargo xtask lint-references` check 15 — any `match` naming an `Event`/`Tag`/`TagEnd`
  variant must be exhaustive, discovered by the arms rather than from a list.
- **THE SPLICE IS WIRED** — `preview::render`'s toggle handler calls
  `window::splice_disclosure_in_place`, which resolves the pane and hands off to
  `preview::splice::install::splice_disclosure`; a full re-render remains the fallback
  and every refusal is made before the buffer is touched. The three things the wiring
  was blocked on are closed: read-back getters on `CodePreviewView`
  (`width_bounded`/`image_bounded`/`tables`), a merged install of the widget-bearing
  products beside PASS A's wholesale maps, and the scroll question — which needed a
  RESTORE rather than nothing and rather than a compensation.
- **`preview::splice::install::ReaderAnchor` + `farscroll::settle`** — the reading
  position across a toggle. GTK 4.6-4.18 lands every compensating pass `top_margin`
  pixels short (fixed upstream in 4.19.3, `b300698629`), and the emission count is
  BIMODAL at a fixed dose, so no compensation can be right; the anchor records the
  reader's offset from the viewport top before the splice and re-establishes it once
  the adjustment goes quiet. `farscroll::after_scroll_settles` is that wait: the
  layout-valid oracle AND N ticks with no adjustment write, bounded on stalled progress
  exactly as `after_line_heights_validated`'s own deadline is. MEASURED through the
  production path on a 20-separator body: `+368px` drift with the restore removed,
  `+0px` with it, and the restore's own write provokes no further ones.
- **`widgets::disclosure::set_expanded`** — the indicator refresh a splice makes
  necessary. The control sits ABOVE the region a toggle changes, so it is the one
  widget that is not rebuilt; the file's own note that no such setter should exist was
  a true statement about the re-render implementation that had frozen into a
  requirement.
- TDD 2.26, 2.26a-j, 2.8i, 11.10, 12.22, 18.48, 18.49.

**Verified in the running app** (Xvfb): collapsed by default, `<details open>` expanded,
click toggles and un-toggles, Markdown in the body renders as Markdown, siblings
independent.

## Remaining work

**Every numbered item on this plan has landed**, the splice included — a toggle now
splices its own region and the reader keeps their place. What remains below is residue:
things measured and recorded rather than things owed.

Two lessons from the theme dressing are worth carrying rather than re-deriving:

- **The contrast sweep's surface for a summary ink is the BAND, not the page.**
  `disclosure_fg`, `disclosure_preview_fg` and `disclosure_marker_color` are all read on
  it, and `disclosure_preview_fg` was measured against the page under a comment saying
  there was no panel behind a summary line — true until the band key existed.
- **A sprite indicator that will not decode degrades to its glyph, which looks fine.**
  A plate silently became its ▶ on a driven Xvfb run with every theme-side test green;
  the only observables were a warning line and a screenshot. No data-level guard can see
  it, which is why `widgets::disclosure` now asserts the shipped plate reaches the
  control.


### Test coverage, as it stands

Every rubric has at least one test and the ones that cost defects are mutation-tested
(2.8i, 2.26h, 11.10, 12.22, 18.48's gate entry, 2.26d's unclosed case, the click
target, the cursor). Two
stated gaps, neither silent: **2.26d's unspaced-body clause is deliberately unasserted**
(item 4 — pinning behaviour that contradicts a live rubric would bury the divergence),
and the body preview's ellipsis is deliberately not asserted against a SHORT body in
the founding-claim test — a body inside the preview limit renders to the same length
either way, which proves nothing about a shift.

### The detail behind those items, and the residue

- **An unspaced `<details>` body is DROPPED, where rubric 2.26d promises literal text.**
  MEASURED: `<details>\n<summary>S</summary>\nnot separated\n</details>` renders as the
  summary line alone — `not separated` appears nowhere. With no blank lines the whole
  construct is ONE raw-HTML block, and raw-HTML content is sanitised by omission
  (TDD 2.23), which is the same rule that correctly drops a `<script>`'s text. The
  rubric describes GitHub rather than this tree.

  **RATIFIED: show the text.** The rubric stands and the renderer changes — emit the
  non-tag runs of an ALLOWLISTED element as literal text. Silent loss is what TDD 2.25
  exists to forbid, and the widening is narrow because the allowlist stays an
  allowlist: `<script>`, `<iframe>`, `<div>` and everything else keep sanitise-by-
  omission, text included.

  **The nesting caveat is the whole security content of this change.** The unspaced
  case is ONE raw-HTML block, so a naive "emit the non-tag runs" also emits the body of
  a `<script>` nested inside the `<details>` — the exact text the omission rule exists
  to drop. The emission must therefore track nesting WITHIN the block and drop any run
  sitting inside a non-allowlisted element. Without that this stops being a narrow
  widening and becomes a general one.
- **The scroll excursion** — a toggle re-renders the whole document and the reader
  watches it happen. The mechanism is now settled by measurement; what is left is the
  work, not the design.
  MEASURED on a 107 KB document (28 000 px tall): the rebuild collapses the
  vadjustment's `upper` to ~650, which clamps `value` to 0, then GTK re-validates line
  heights top-down over ~17 idle passes before the restore can land — so the view
  drops to the top and glides back. **The landing position is exact** (11 705 → 11 705
  across a toggle); it is the journey that is visible. This is a property of every
  in-place re-render, not of folding: a zoom step on the same document produces an
  identical trajectory.

  **A targeted edit does NOT pay that cost** (MEASURED): inserting 20 lines
  mid-document on a fully validated view moved `upper` 27939 → 28299 and `value`
  11705 → 12049, with no collapse and no excursion. So splicing the affected range is
  the mechanism.

  **The cheap spelling of it is closed, and the reason is measured** (GTK4Rs/AP-320). Rendering to a scratch buffer and `insert_range`-ing the changed region
  fails on anchored children: `insert_range` SKIPS every `GtkTextChildAnchor` and does
  not leave its `U+FFFC` behind, so the destination is one character short per
  table/image and every offset map misaligns from the first one onward. A disclosure
  body is exactly where tables and images live. Two adjacent worries were measured and
  are NOT obstacles: `apply_tag` does not collapse `upper` (the btree returns the last
  stored height while a line is invalid), and `delete` invalidates only its own range
  and unparents anchored widgets itself, so no pre-detach is needed. There is also no
  paint-hold to hide the excursion with — `gdk_surface_freeze_updates` is private, and
  the excursion is a real value clamp rather than a paint glitch.

  **So the shape is: the renderer writes at a MARK.** Delete the changed range from the
  live buffer, then render the new content there — the renderer creating its own
  anchors and applying its own tags — rather than copying a pre-rendered region in.

  **This is now the feature, not only a mechanism.** `preview::splice` performs the
  region write, `preview::splice::install` adopts it into the live pane, and
  `preview::render`'s toggle handler is its caller.

  **The scroll question is settled, and the answer was a RESTORE.** The residual drift
  the mechanism left is the upstream `top_margin` compensation defect (GTK4Rs/AP-321;
  fixed in 4.19.3 by `b300698629`), whose per-pass loss is exact but whose PASS COUNT
  is measurably bimodal — so adding the quantum back is wrong some of the time whatever
  value it takes, and would double-correct once the floor moves past 4.19.3. Recording
  the reader's viewport offset before the splice and re-establishing it after
  quiescence counts nothing, so the bimodality cannot reach it, and it degrades to a
  swallowed same-value write on a fixed GTK. The wait is the hard part and is
  `farscroll::after_scroll_settles`: a restore issued DURING the settle is silently
  eaten, because every compensating `set_value` destroys `first_validate_idle`.

  **A trap worth keeping**: `emit_pending_summary` writes its newline into whichever
  buffer it ran against, so a seed can be logically correct — `trailing_newlines`
  accounting for that newline — while the buffer it is replayed into is physically
  missing the very byte that state describes. Seeding a renderer copies its state, not
  its side effects.

  **Step 1 was the renderer's write cursor.** `Renderer::tip()` is now the one
  definition of where a render writes, and `write_at` points it at a region instead of
  the buffer end. The mark is right-gravity so the cursor advances as content is laid
  down; a left-gravity one writes every run at the same place and reverses the
  document, which is mutation-tested rather than reasoned about. `write_at` carries the
  gated tests' cfg until its production caller exists.

  **Step 2 is the region and the maps**, and the maps are the whole difficulty. A fold
  shifts every buffer offset after it, so the copy map, source map, heading sites and
  link spans are all stale below the splice.

  **Two prerequisites have landed.** The renderer's write cursor is now also the
  cursor the MAPS are recorded against: `preview::build` read `buf.char_count()` — the
  buffer's length, which is the write position only while a render appends — and now
  reads `Renderer::end_offset()`. And `renderer::DisclosureExtent` records where each
  drawn disclosure's summary line and body sit in the buffer, which is the region a
  toggle changes. The source range of a body was already known without rendering; the
  buffer it OCCUPIES is a fact only the render knows, and nothing recorded it, so a
  collapse had nothing to delete.

  ### ✅ Re-walk into a scratch buffer for the maps only

  Widgets come from the live splice; every map comes from a full scratch walk. Correct
  by construction — the scratch text equals the spliced live text, so its offsets are
  the live ones — and it reuses `build_products` verbatim rather than growing a second
  implementation of each map's coordinate rules.

  **Decisive:** the maps are not one artefact but ~15 buffer-keyed products spread
  across three storage sites (`RenderData` qdata, `CodePreviewView` imp fields,
  per-label qdata), and `CopyTree` is recursive with an absolute buffer pair on every
  node, sibling and ancestor, no parent pointer and no per-fold handle.

  **Its cost is CPU, not correctness**: a throwaway widget build per table and image,
  and a re-highlight per code block. It does NOT cost the excursion, which is the
  user-visible defect. The remote-image half of that cost is now removed by
  `imagecache` — without it every toggle re-ran a synchronous uncached HTTP fetch per
  remote image, trading a visible scroll for a frozen UI.

  **Its risk is that "scratch text == spliced live text" is an assumption nothing
  enforces.** `block_sep`/`at_start`/`trailing_newlines` and the `disclosures_seen`
  pre-scan cursor all start at document-start, so a partial walk that does not seed
  them differs from a full render by up to two newlines at a region boundary — and a
  one-character divergence silently offsets every map below the splice.
  `copymap::debug_verify` is the oracle, and it runs only under `debug_assertions`
  inside `build_products`; the splice must call it against the LIVE buffer and assert
  scratch and live slices are char-identical in a gated test.

  **The region is NOT the body, and this was measured rather than foreseen.** A
  collapsed block previews its body's opening text on the summary LINE, so the two
  fold states begin to differ immediately after the summary label, not at the body.
  `DisclosureExtent::volatile` names the corrected span — label end through body end,
  contiguous because the preview, the line terminator and the body are adjacent. A
  splice aimed at the body alone would strand a stale preview and put every offset
  below it out by that fragment's length.

  **And the region must be deleted through the newline run that FOLLOWS it.**
  `block_sep` is written lazily by the next block, so the separator after a disclosure
  is not part of the block's own render and its length depends on what preceded it:
  MEASURED, one newline after a collapsed body where an expanded one leaves two. The
  region render re-establishes it. Both corrections came from the founding-claim test
  failing, which is the argument for having written it before the implementation.

  **The seeding problem has an exact answer, and it is the reason this route is
  cheap.** A region render must arrive with the inter-block state a full render would
  have had there — `at_start`, `trailing_newlines`, `lists`, `blockquote_depth`,
  `inline_tags`, `disclosure_stack`, `disclosures_seen`, `slug_seen` — and several of
  those the renderer carries as BUFFER OFFSETS (`item_starts`, `blockquote_starts`,
  `heading_start`, `link_start`), which look like they would need translating between
  the scratch and live coordinate spaces. **They do not: the two spaces coincide up to
  the region.** Everything before the toggled block renders identically under either
  fold state, so the scratch buffer's prefix IS the live buffer's prefix, character for
  character, and the region starts at the same offset in both. The seed is therefore a
  straight snapshot of the scratch renderer's state at the moment its walk reaches the
  region — no arithmetic, and the scratch walk is its own oracle for what that state
  is. Rubric 2.26c makes this load-bearing rather than academic: a disclosure nested in
  a blockquote or a list item reaches its body with that state non-empty.

  **So the region render cannot be avoided, and copying cannot replace it.** With the
  prefix identical and the suffix identical-but-shifted, the only work is making the
  live text equal the scratch text over one range — which would be a copy if a copy
  were possible, and `insert_range` is the closed dead end above (GTK4Rs/AP-320: it
  skips every child anchor). A render is what creates anchors, so a render it is.

  ### ❌ Rebase the existing maps

  Entries before the region unchanged, entries after shifted by the delta, entries
  inside replaced by a partial walk. Rejected on the shape of the work: `raw_evs`
  rebases mechanically, but every one of the ~15 products needs its own correct
  rebase, and the copy tree needs ancestor-span widening with no handle identifying
  which node owns a given fold. That is a maintenance surface proportional to the
  number of maps, permanently, against a one-off CPU cost.
- **`<picture>` copy is fixed but unverified end to end.** The unit layer proves the
  classification; proving the clipboard needs a GTK test driving a real buffer.
- **Check 15's 13 wildcard dispatchers.** **RATIFIED discriminator: an explicit,
  greppable opt-out comment on the wildcard arm**, which the check reads. Telling a
  selector from a dispatcher by pattern SHAPE was the rejected option — `_ if cond =>`
  is textually a wildcard, so the analysis is unreliable in both directions. An opt-out
  marker makes every one of the 13 a triaged human decision once, and makes a NEW
  wildcard fail by default. `outline.rs:217`'s `_ => {}` is not annotated but fixed: a
  new text-bearing variant silently dropping out of heading labels is the failure the
  check exists for.
- **Cross-session persistence of collapsed state** remains out of scope.

## Technical details worth preserving

**The parser already does the hard part.** With the blank lines CommonMark requires,
pulldown-cmark emits the disclosure as three separate things (MEASURED):

```text
Start(HtmlBlock) 80..115 / Html("<details>\n") / Html("<summary>Title</summary>\n") / End(HtmlBlock) 80..115
Start(Paragraph) 116..126 / Text("body text") / End(Paragraph)
Start(HtmlBlock) 127..138 / Html("</details>\n") / End(HtmlBlock) 127..138
```

So the body needs no special rendering path at all; it is ordinary document content,
which is what lets TDD 2.26c promise that everything inside renders as it does at top
level. The work is carrying the pairing *across* those blocks, and nesting makes it a
stack. `End(HtmlBlock)` is the event whose source range spans the whole block, which is
why it is the one that earns a copy node.

**Behaviour decisions** (agreed with the requester, ratified by the operator):

| Question | Decision |
|---|---|
| Find matches inside a collapsed block | Auto-expand to the hit |
| Copy-as-Markdown over a collapsed block | Include the body |
| Outline headings inside a collapsed block | Listed |
| Collapsed state persistence | Within a session yes; cross-session out of scope |
| Reading position across a toggle | Held, as a position in the DOCUMENT: content below the toggled block moves with it, and the reader never returns to the top (TDD 2.26h) |
| No `<summary>` element | Default label "Details" |
| No blank lines around the body | Render as literal text (CommonMark/GitHub behaviour) |

**Design details adopted from `TeplFoldRegion`** (libgedit-tepl, GTK3, the only known
implementation of this idea in GNOME): line-snapped bounds, and marks rather than
character offsets for anything that must survive edits. Its anonymous-tag-per-fold detail
no longer applies, since this design uses no tags at all. Tepl never hit the anchored
child problem because it folds source code, which has no child anchors.

**No GTK4 prior art exists.** Nothing in gtk4-demo, nothing in gtk4-rs, and GtkSourceView
has no code folding at all. Every GTK Markdown app renders its preview through a web
engine, so the category solves `<details>` by delegating to a UA.
