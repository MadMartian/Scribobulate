# Consolidated Code Review: `feature/details-disclosure` — Scribobulate

**Branch:** `feature/details-disclosure`
**Last updated:** 2026-09-01 (Round 1)
**Review scope:** `363790e..c0e1c0f` — 114 files, +17168 / −1270. The feature is
squashed into `ad3dd34`; `c0e1c0f` retires the plan.
**Requirements source:** `sdd/PLAN.details-disclosure.md`, deleted by `c0e1c0f` and
preserved for this review at `docs/PLAN.details-disclosure.retired.md`.

**Panel:** 6 reviewers at ×1 (operator-capped), across 13 conceptual groups with
alternating file order — spec-compliance, security, DRY/abstraction, anti-pattern,
testability, link-integrity. Plus the orchestrator's own gate runs.

**Verdict: NOT PASS.** No High findings remain open — all four are ✅ DONE. The Medium,
Low and Tidy tables are the outstanding work; the Tidy table is terminal (optional).

**Round-1 correction (post spot-check audit):** F-GATE-001 has been **WITHDRAWN** as a
false positive of the orchestrator's own making — see [Withdrawn](#-withdrawn).
F-AP-007 promoted Medium → High. F-AP-006 and F-013 demoted to Low. F-003's export
mechanism corrected. F-014 and F-GATE-003 added to the Medium table, having been cited
but never tabled.

> **Tree-state warning.** Every line number below is verified against **`HEAD` =
> `c0e1c0f`** via `git show HEAD:<path>`, not against the working tree. At the time of
> writing, `linux` holds uncommitted changes in `sdd/TDD.md`, `src/preview/css.rs`,
> `src/preview/mod.rs`, `src/widgets/disclosure.rs`, `src/widgets/table/mod.rs` and
> `tests/MANUAL-TEST.md`. Findings citing those six files may already be stale — they
> are marked ⚠ below and must be re-verified against the next commit.

---

## 🔒 Security Review

The branch's central security thesis — that `<details>`/`<summary>` can join the
raw-HTML allowlist without widening what is *trusted* — **holds at the design level
and at every sink**. Export escaping, the URL-scheme gate in front of the image cache,
the unclosed-`<details>` fix and collapsed-body suppression all traced clean, across
19 recorded attack surfaces.

What does **not** hold is the nesting guard that the plan itself calls *"the whole
security content of this change"*. It is bypassable two ways (**F-001**, **F-010**),
both reproduced against the shipped code by driving the extracted function
out-of-tree. Blast radius is bounded — a `GtkTextView` with no HTML or JS engine, and
the export sinks do not share the path — so the consequence is **content injection
and spoofing in the rendered page, not execution**. That is why nothing here is
Critical.

No Critical findings. `unsafe` usage, panic paths on attacker-influenced values, and
sprite/theme decode paths were traced and are clean.

---

## 🔴 Critical — Must Fix

None.

---

## 🟠 High

### ✅ DONE — F-001 — `literal_text_runs`' nesting guard is defeated by any stray close tag, emitting a `<script>`'s text as page content

*Found independently by **four** reviewers (F-SEC-001, F-SPEC-001, F-AP-001,
F-TEST-001). Two rated it High; merged severity is High.*

- **Location:** `src/renderer/rawhtml.rs:285-325`, decrement at `:313`
- **What:** The suppression depth is a plain counter. **Any** token shaped `</…>`
  decrements it, including one that closes nothing. Two `</x>` before the real
  `</script>` drop the depth to 0 while the cursor is still inside the `<script>`, and
  every run after that is emitted. `saturating_sub` is what makes it work: the counter
  cannot go negative, so a stray close tag at depth 0 is free and one inside a
  suppressed element is a full escape.
- **Why it matters:** `rawhtml.rs`'s own module doc (`:274-280`) states the invariant
  this breaks — *"never inside a `<script>`, `<style>`, `<iframe>` … whose text stays
  dropped exactly as before"*. The plan is blunter: *"Without that this stops being a
  narrow widening and becomes a general one."* The existing test that claims to pin
  this (`a_script_inside_an_allowlisted_block_contributes_no_text`, `:371-382`) passes
  only because its fixture is well-formed.
- **Reproduced** (function extracted verbatim, `rustc -O`, no repo edits):
  ```
  in : "<details>x<script>y</span>LEAK</script>z</details>"
  out: ["x", "LEAK", "z"]
  in : "<details>\n<summary>S</summary>\n<script>\n</b>\nalert(1)\n</script>\n</details>"
  out: ["\nalert(1)\n"]
  ```
- **Fix:** Stop counting; track *which* element opened suppression. Push the tag NAME
  onto a stack and pop only on a matching name — a stray `</b>` then becomes inert in
  both directions, which is also what a browser does. Additionally, once suppression is
  entered by a **raw-text element** (`script`, `style`, `textarea`, `title`, `xmp`),
  nothing inside is markup: skip literally to the matching `</name`. That one rule
  closes this and F-010's `<script foo="a>b">` variant together.

---

### ✅ DONE — F-002 — A region render that fails *after* the live delete reports the splice as a success, leaving the block deleted from the buffer

*Found by F-AP-002 (High) and F-TEST-004; the security reviewer independently reached
the same site but could only file it as "Needs Verification" (V-SEC-001) because it
could not construct a reaching input, and said so honestly.*

- **Location:** `src/preview/splice.rs:155` (the delete), `:316-325` (the recovery),
  `:196` (the unconditional `Some`)
- **What:** `splice()` deletes the volatile region from the live buffer at `:155`, then
  calls `render_region`. If the seed walk never reaches `key`'s region,
  `render_region` logs an error and substitutes **a fresh renderer that has written
  nothing** (`live.unwrap_or_else(…)`). Control returns to `splice()`, which builds and
  returns `Some(SpliceOutcome{…})` unconditionally. The caller sees `true` and **does
  not fall back to a full re-render**.
- **Why it matters:** This is the one place in the entire feature where a refusal
  arrives *after* the buffer is touched, and it is converted into a success. Three
  compounding consequences: (a) the disclosure's whole rendered region is gone from the
  reader's page with no visible error; (b) PASS A's copymap, source map, heading sites
  and link spans are installed wholesale at `install.rs:189-205` against a buffer now
  short by the region's length, so **every offset below the splice is wrong** — exactly
  the silent map-desync the module doc names as the route's only real risk; (c) copy,
  find, outline navigation and annotation placement then all act on wrong positions.
  `install.rs:47-53` promises the opposite in writing: *"Every `false` here is a
  refusal made BEFORE the buffer is touched, so a fallback re-render is always
  operating on an untouched pane."*
- **Reachability, stated honestly (spot-check audit):** no reaching input is known, and
  `copymap::debug_verify` catches the divergence in debug builds. This is a **latent
  invariant violation**, not live corruption — worth the one-line fix at each site, but
  it is not a fire. The reason it stays High is that the code's own contract asserts the
  opposite in writing, and the failure it guards against is silent and total.
- **Fix:** Make `render_region` return `Option<…>` and propagate the `None`. Because
  the buffer is already mutated, `splice()` must not merely return `None` — return a
  distinct `SpliceError::RegionLost` that `install::splice_disclosure` maps to `false`
  *and* the caller treats as "buffer is now inconsistent, force a full re-render".
  `foldsplice`'s existing `false` path already does the right thing; this is one line
  at each site.

---

### ✅ DONE — F-003 — An inline `<details>` in ordinary prose desynchronises the pre-scan cursor, injects a summary line mid-paragraph, and silently disables the feature for the rest of the document — in **both** sinks

*F-SEC-003, extended by the orchestrator to the export path (see below).*

- **Location:** `src/renderer/events.rs:213` → `src/renderer/start.rs:315-316, 411`;
  cursor at `src/renderer/mod.rs:1013`; contradicted comment at
  `src/renderer/disclosure.rs:247-251`; export path at `src/export/walk.rs:227, 404,
  533-552`
- **What:** `disclosure::scan_document` deliberately ignores `Event::InlineHtml`, and
  justifies it in a comment: *"the renderer does not open a frame for one either, so
  the two stay agreed by both declining it."* **The renderer does not decline it.**
  `Event::InlineHtml(t) => self.feed_html(&t)` and `feed_html` now calls
  `feed_disclosure_html` (new on this branch — on `master` it ran only the `<picture>`
  scanner, which is why `events.rs:209-212` still says "Non-image HTML is dropped").
  So an inline `<details>` pushes a frame that is never popped, unconditionally
  advances `disclosures_seen` (`mod.rs:1013`), and falls through to the unconditional
  `emit_pending_summary()` at `start.rs:411`.
- **Consequences in the preview:** (a) the paragraph is split by two inserted newlines
  and a spurious `Details` label appears mid-prose — content the document never
  contained; (b) `disclosures_seen` is off by one for the rest of the document, so
  every real `<details>` fails the offset check, logs an error, and renders
  `foldable = false` — **the disclosure feature silently stops working below the inline
  tag**; (c) the stale frame stays on `disclosure_stack` for the whole render.
- **⚠ Consequence in the export — corrected by the spot-check audit.** The routing
  trace is verified: `export/walk.rs:227` routes `Event::Html` **and
  `Event::InlineHtml`** to `self.html()`, which pushes into `pending_details` (`:605`);
  `:404` flushes on `TagEnd::HtmlBlock`; `apply_details` (`:533-552`) consumes
  `disclosures[disclosures_seen]` with **no `span.start` cross-check and no
  diagnostic** (that gap is F-011).

  **But the one-disclosure case is NOT corrupted**, contrary to this finding's first
  draft. Because `pending_details` is not flushed until the next `TagEnd::HtmlBlock` —
  which is the real block's own — the phantom `DetailsOpen` opens its frame at exactly
  the point the real one would have, and the off-by-one cancels. *Do not write the
  one-block regression test: it passes, and it will mislead you into discarding this
  half of the finding.*

  **The real corruption needs two real disclosures below the prose mention**, where the
  batched flush opens two nested frames at once:

  ```
  flush at first real block: [DetailsOpen(inline), DetailsOpen(real#1), SummaryOpen, SummaryText("One"), SummaryClose]
    DetailsOpen -> spans[0] closed=true -> OPEN frame (depth 1)
    DetailsOpen -> spans[1] closed=true -> OPEN frame (depth 2)   <-- nests, does not skip
    "One" -> innermost frame
  </details>      -> closes only the INNER frame
  <details open>  -> spans[2] = None -> closed=false -> SKIPPED
    "Two" -> written to the still-open OUTER frame
  ```

  The artefact becomes **one** disclosure labelled `Two` whose body contains the whole
  `One` disclosure *and* `BODY2` — a structural re-nesting — plus a lost `open="true"`,
  because the outer frame took `open: false` from the phantom inline tag while the
  source said `<details open>`. Document Rendering CAM row 17's failure by a new route.

  Two facts that widen the trigger surface: **the flush fires on any HTML block, not
  any disclosure block** (`:404` is unconditional), so a stranded phantom is flushed by
  the next unrelated raw-HTML block — a standalone `<img>` or `<picture>` on its own
  line will do it; and an inline mention placed *after* the last real block corrupts
  the preview but not the export at all, since the phantom is never flushed.

- **Trigger is not exotic:** any prose that mentions `<details>` mid-sentence without
  backticks — this project's own release notes, or `sdd/TDD.md` prose rendered in the
  app.
- **Why High and not Critical:** the renderer's offset check fails safe. A phantom
  frame can never be `foldable` (an inline event's `event_src.start` can never equal an
  `HtmlBlock`'s `range.start`), so rubric 2.26d's "suppress every remaining event"
  catastrophe is not reachable this way.
- **Fix:** Make the renderer decline inline disclosure tags, matching the claim
  `disclosure.rs:250` already makes. Split `feed_html`: `Event::InlineHtml` keeps the
  `<picture>` scanner (which genuinely needs cross-event grouping) and skips both
  `feed_disclosure_html` and `feed_html_literal_text`. Do the same in
  `export::walk::html()`. Correct the stale comment at `events.rs:209-212`. Add a
  regression test asserting `para <details> tail` renders as one paragraph with no
  `Details` label, leaves a following real `<details>` foldable, **and** — using the
  **two-block** fixture above, not a one-block one — exports with the correct
  summary/body pairing and preserves `open`.

---

### ✅ DONE — F-AP-007 — Fold keys are never cleared on the split-mode live-preview path, so typing silently changes which blocks are collapsed

*Promoted Medium → High by the spot-check audit; the orchestrator accepts the argument.*

- **Location:** `src/fold.rs:65` (`FoldKey` is a source byte offset), `TabState::set_source`
  (the only clear, and not on the live-preview path)
- **What:** A `FoldKey` is the source byte offset of a disclosure's opening raw-HTML
  block. The only thing that clears the fold map is `set_source`, which the split-mode
  live-preview path does not go through. So **every character typed above a
  `<details>` moves its key**, and the map is never reconciled.
- **Why High:** two content-visible outcomes. A collapsed block silently reverts to the
  document default mid-typing, with the reading-position restore then firing against
  the wrong geometry. Worse, a surviving key can coincide with a *different* block's
  new start offset and collapse **the wrong block** — hiding content the reader never
  asked to hide. That is the same class of harm `scan_document` exists to prevent,
  arriving through the write path nobody re-checked.
- **Why it outranks the other Highs on likelihood:** F-003 needs a document that
  mentions `<details>` in prose. This needs **one keystroke in split mode**, which is
  the application's primary editing surface.
- **Fix:** Reconcile or invalidate the fold map on the live-preview re-render path.
  Either re-key surviving folds against the new source offsets, or clear on any source
  mutation and accept that a live edit resets fold state — the second is cheaper and
  defensible, but it must be a decision, not an omission.

---

## 🟡 Medium

33 findings (F-AP-007 promoted out to High; F-014 and F-GATE-003 added; F-AP-006 and
F-013 demoted to Low by the spot-check audit but left in place here with a ⬇ marker so
the merge history stays visible). Full argument, evidence and reproduction for each is in the per-reviewer
files listed under **Evidence trail** below; the fix column here is the actionable
summary.

| ID | Finding | Location | Proposed fix |
|---|---|---|---|
| **F-014** | Coverage ratchet lowered rather than the gate fixed: `FLOOR` 82 → 80 | `scripts/coverage.sh:577`, rationale `:60-90` | **Operator decision, not a code change.** Documented and measured (82.09 → 80.65 unit-only; 94.84 with `gtk-integration-tests`), and the alternative was raised and declined. But the comment itself notes this is the *second* drop from one cause and that "the gate is the thing to fix, not the floor". Choose: (a) accept; (b) accept + split the gate into a unit floor and a GTK-integration floor so the next instance raises a number instead of lowering one; (c) require the extraction work before landing. <br>*also found as F-AP-023, F-GATE-002, F-TEST-011*  |
| **F-GATE-003** ✅ DONE | `coverage.scope` gained 13 new modules but not three others created by the same branch | `scripts/coverage.scope` | `window/foldsplice.rs`, `window/foldreveal.rs`, `codeview/disclosurebands.rs` are absent — and `foldsplice.rs` is named in `coverage.sh`'s own floor-drop rationale, so it is in the numerator's story but not the scope list. State the rule that decided which new modules entered scope, then either add the three or record each exclusion by name in `IGNORE`, where it is visible to whoever reads the number. |
| **F-AP-003** ✅ DONE | Every splice refusal is silent — including one whose own rustdoc says it is logged | `src/preview/splice.rs:140` and `:146`; | `log::debug!` at each refusal with its distinguishing reason ("no render data", "no preview pane", "view is not a CodePreviewView", "key absent from the previous render's extents", "key absent from PASS A"), and `log::error!` at `splice.rs:146` as the doc already promises. Add one `#[gtktest::test]`… |
| **F-AP-004** ✅ DONE | `ReaderAnchor::capture` mutates the buffer before two refusal points, leaking a `GtkTextMark` per refused splice | `src/preview/splice/install.rs:75` (capture) → `:77-91` | Move `ReaderAnchor::capture` to *after* the `super::splice` call's refusal points are past — it cannot, because the anchor must be read from the pre-splice buffer. So instead: give `ReaderAnchor` a `Drop` that deletes its mark unless it was consumed, or (simpler and type-enforced) have `super::splic… |
| **F-AP-005** ✅ DONE | Two overlapping fold toggles arm two independent settle-restores over one adjustment, with no generation token | `src/preview/splice/install.rs:97-99` and `:408-437`; | Give `CodePreviewView`'s `imp` a `Cell<u64>` settle generation. `after_scroll_settles` (or a thin wrapper in `install`) bumps it, captures the new value, and the timer closure returns `Break` without running `f` if the stored generation has moved on. Equivalently, store the pending `SourceId` in the… |
| **F-AP-006** ⬇Low ✅ DONE | The disclosure toggle's deferred work strong-captures the `ApplicationWindow`; its sibling `foldreveal` weak-captures it | `src/preview/render.rs:1040-1072` (idle armed at `:1060`), | Mirror `foldreveal`: `let win = window.downgrade();` then `let Some(window) = win.upgrade() else { return };` inside the idle, and add the `view.is_realized()` gate the AP-128 corrective pairs with it. Better still, route both deferrals through one helper so the third caller cannot get it wrong (the… |
| **F-AP-008** ✅ DONE | The reading-position restore discards its own outcome with `let _ =` | `src/preview/splice/install.rs:412-430` (the discard at `:430`) | `if restored.is_none() { log::debug!("preview::splice: reading position not restored (…)"); }` with the reason distinguished, or return the reason as a small enum and log it. The mark deletion below must stay unconditional either way. |
| **F-AP-009** ✅ DONE | `reveal_folds` calls `toggle` where it means "expand", relying on a caller-side invariant nothing enforces | `src/window/foldreveal.rs:44-55` (the loop at `:52-54`); | Add `FoldState::expand(&mut self, key, open_in_source)` (or a `set_collapsed(key, open_in_source, collapsed: bool)` that writes the `toggled` set to the value that yields the requested render state) and have `reveal_folds` call it. Idempotent by construction, and the duplicate-key and already-expand… |
| **F-012** ✅ DONE | Lint check 15's brace-depth model never enters character literals, so the new gate is blind in any file containing `'{'` or `'}'` | `xtask/src/lint/patterns.rs:471` (the claim), `:476` (the | Add the missing `else if c == '\''` arm — but note Rust lifetimes (`'a`) share the delimiter, so the correct predicate is "a `'` followed within 4 chars by a closing `'` (allowing an escape)", or simply blank the two-or-three-character forms `'X'`, `'\X'`, `'\u{…}'`. Add a corpus case whose fixture … <br>*also found as F-AP-011, F-TEST-010* |
| **F-013** ⬇Low ✅ DONE | Negative image-cache entries are excluded from the byte budget *and* from the LRU, and nothing ever sweeps them | `src/imagecache/policy.rs:61-65` (the two stores), | Give `Cache` a `max_negative_entries` and evict oldest-first when it is exceeded (a small `VecDeque<String>` of negative keys is enough), or opportunistically sweep entries older than `negative_ttl` on each `record_failure`. Either is a handful of lines, unit-testable in `policy.rs`'s existing displ… <br>*also found as F-SEC-005* |
| **F-AP-013** ✅ DONE | `live_anchored` walks the whole buffer character by character, with a linear widget scan per anchor, on the main loop for every toggle | `src/preview/splice/install.rs:110-133` (the walk at `:119-131`, | Cheapest correct change: make `known` a `HashSet<gtk::Widget>` (glib objects hash by pointer) to remove the inner scan, and skip forward using `iter.forward_find_char(|c| c == '\u{FFFC}', None)` instead of stepping one character at a time. Better: store the `(anchor, widget)` pairs alongside the wid… |
| **F-AP-014** ✅ DONE | `select_preview_hit_at_or_after`'s `unwrap_or(0)` turns "the expansion produced no reachable match" into "jump to the document's first match" | `src/window/find.rs:824-850` (the fallback at `:834-837`) | `let Some(idx) = … else { log::debug!(…); return None; };` — leave the current hit and count untouched rather than redirecting, since the block *has* been expanded and the reader can see it. Add a test whose hidden match lies inside a table cell in the collapsed body. |
| **F-AP-015** | Scratch-buffer and live-buffer offsets share one type, and which fields of `RenderProducts` are valid against which buffer is stated only in prose | `src/preview/splice.rs:79-95` (the `SpliceOutcome` doc), | Split `RenderProducts` into `MapProducts` (buffer-space offsets, valid against any buffer whose text matches) and `WidgetProducts` (bound to the buffer they were built for), and have `build_render_products_with_theme` return both. `splice` then structurally cannot install the widget half, and the ne… |
| **F-AP-016** | The region render's method sequence is temporal coupling enforced by one function | `src/preview/splice.rs:262-315` (`seed` `:282`, `write_at` `:283`, | Wrap the sequence in a small builder/typestate: `RegionWriter::begin(renderer, view, seed)` → `.write_at(offset)` returning a `RegionWriter<Writing>` that alone exposes `summary_tail`/`process`, and whose `finish()` yields the `RegionWidgets`. The project already climbs this enforcement ladder elsew… |
| **F-AP-017** ✅ DONE | The splice's only correctness oracle is compiled out of release builds | `src/preview/splice.rs:183-194` (the `#[cfg(debug_assertions)]` | Keep the full `debug_verify` where it is, but add a cheap release-safe invariant at the same point — e.g. assert the live buffer's char count equals PASS A's `copymap` root span, and on mismatch `log::error!` and return `None` from `splice()` so the caller falls back to a full re-render. That is O(1… |
| **F-DRY-001** | `codeview::disclosurebands::draw` is `codeview::bands::draw` with the noun changed | `src/codeview/disclosurebands.rs:24-107` (and its twin `src/codeview/bands.rs:14-123`) | Extract the shared painter into `src/codeview/bands.rs` (it already owns `band_corner_radius`'s caller side) or a new `src/codeview/bandpaint.rs`: ```rust /// Paint one line-wide band at the content column. The one arithmetic behind /// TDD 18.25 (headings) and TDD 18.48 (disclosure summaries). pub(… |
| **F-DRY-002** | `RenderData` is filled from `RenderProducts` field-by-field at four separate sites | `src/preview/render.rs:58-73`, `src/preview/render.rs:291-306`, | Mirror the `ViewInstall` precedent. Group the buffer-keyed maps into one value carried by both types: ```rust // src/preview/build.rs /// Every buffer-keyed map a render produces — the half of `RenderProducts` that is /// installed wholesale, as one value, so no route can install a subset. pub(super… |
| **F-DRY-003** | `RegionSeed`'s twelve fields are declared, captured and re-applied by hand — a new renderer field is silently unseeded | `src/renderer/mod.rs:757-770` (struct), `src/renderer/mod.rs:844-866` | Make the seed a *sub-struct of the renderer* rather than a parallel copy of its fields: ```rust // src/renderer/mod.rs /// The renderer's INTER-BLOCK state: everything a region render must arrive with. /// Membership is the contract — a field that lives here is seeded, a field that /// does not is a… |
| **F-011** ✅ DONE | "Is the Nth `<details>` closed?" is decided twice, and the export copy dropped the divergence guard | `src/export/walk.rs:533-552` (and its twin `src/renderer/mod.rs:1012-1026`) | Put the cursor in `renderer::disclosure`, where the spans already live, and let both walks hold one: ```rust // src/renderer/disclosure.rs /// A document-order cursor over [`scan_document`]'s spans, with the fail-safe the /// renderer already applies: a walk that disagrees with the pre-scan is answe… <br>*also found as F-AP-019* |
| **F-DRY-006** | `splice::render_region` re-implements `build_products`' renderer-construction preamble, and re-flattens `SpliceInputs` into eleven positional arguments | `src/preview/splice.rs:215-260` (and its original `src/preview/build.rs:305-357`) | One prepared-inputs value, owned by `preview::build` and consumed by both routes: ```rust // src/preview/build.rs /// A document prepared for rendering: normalised, CriticMarkup-extracted, with the /// palette its theme derives. The ONE definition of what a render's inputs are — /// PASS A and a reg… |
| **F-DRY-010** | Find's "land on this hit, or expand and re-enter" sequence is written twice | `src/window/find.rs:824-858` (and its twin `src/window/find.rs:751-809`) | Two helpers in `src/window/find.rs`: ```rust /// Mark `hits[idx]` as the current match — highlights, scroll, cursor and label, /// which must move together. `Some(..)` instead when the hit is `Hidden`: nothing is /// marked, and the caller must reveal that fold and re-enter. fn land_on_hit( view: &C… |
| **F-010** ✅ DONE | A `>` inside a quoted attribute value splits the tag, leaking text from inside a non-allowlisted element | `src/renderer/rawhtml.rs:302-305` = tail.find('>') else {` is the sole match at 302; `let tag = &tail[..=gt];` is the sole match at 305). Same defect in the disclosure scanner at `src/renderer/disclosure.rs:88-92` = lower[start..].find('>') else {` at 88, `let tag_lower = &lower[start..=end];` at 92). | Give both scanners one shared tag-end routine that walks from `<` tracking quote state (`"` and `'`) and returns the first `>` outside quotes. It belongs in `rawhtml.rs` beside `recognise_html_element` for the same "one owner" reason the allowlist itself lives there — `disclosure.rs` and any future … <br>*also found as F-DRY-004* |
| **F-SPEC-002** ✅ DONE | An allowlisted block's literal text reaches the preview but no exported artefact — the construct is half-taught in exactly the way CAM row 17 now warns about | `src/export/walk.rs:600-636` (`fn html`, grep-confirmed at `:600`; `scan_disclosure_tags` call at `:606`) | Call `rawhtml::literal_text_runs` from `export::walk::html()` and push each non-empty run as a `Block::Paragraph`/`Inline::Text`, so the one owner of the rule feeds both sinks — which is the stated reason the allowlist was extracted to `rawhtml.rs` in the first place. Add an `export::doc` test asser… |
| **F-SPEC-003** ✅ DONE | A collapsed unspaced `<details>` still shows its body, and its toggle is a visible no-op | `src/renderer/start.rs:313-344` (`feed_disclosure_html` call at `:315`, `feed_html_literal_text` call at `:316`, the function body `:332-344`) | Decide the behaviour and state it in TDD 2.26d, then make the code match. The cheapest consistent answer: emit the literal runs from *inside* the disclosure frame rather than after it — i.e. drive `literal_text_runs` from the same replay as `feed_disclosure_html`, so a run sitting between `</summary… |
| **F-SPEC-004** ✅ DONE | `sdd/TECH.md`'s module map does not describe the branch's central subsystem | `sdd/TECH.md:207` (`preview/` row), `:214` (the `codeview` painter row), `:226` (`farscroll.rs` row) | Add four rows, at the map's altitude (what each owns, not how it works): `preview/splice/` (region write + live adoption + `ReaderAnchor`), `window/foldsplice.rs` (resolves the pane and hands off; the full re-render is the fallback), `farscroll/settle.rs` (the *quiescence* wait, distinct from the la… |
| **F-SPEC-005** ✅ DONE | `tests/MANUAL-TEST.md`'s 2.26k check documents the rubric being violated instead of the rubric being changed, and no automated test covers 2.26k at all | `tests/MANUAL-TEST.md:697-698` | Rewrite TDD 2.26k to the contract that actually shipped — the fetch is bounded by `imagefetch`'s connect timeout and is made **at most once per URL per TTL**, so a toggle costs the timeout at most once rather than every time — and move the responsiveness promise to where it can be met (an async fetc… |
| **F-SPEC-006** ✅ DONE | Four new rubrics ship with no `tests/MANUAL-TEST.md` check | `sdd/TDD.md:340` (2.26b), `:345` (2.26c), `:360` (2.26e), `:365` (2.26f) against `tests/MANUAL-TEST.md:687-698` | Add four checks in §2 against `tests/fixtures/disclosure.md`, which already carries every shape they need (an `open` block, a body of mixed Markdown, a nested pair, a sibling pair, and a wide table). Each is two or three sentences; 2.26e's should drive the inner-state-survives-an-outer-cycle sequenc… |
| **F-SPEC-007** ✅ DONE | The Document-Reference CAM gains no cell, though the branch's own manual check cites it and the splice introduces a new invalidation event for the preview buffer | `sdd/CAM.md:453-491` (matrix rows 1–7; row 7 grep-confirmed at `:491`) | Either add the invalidation class explicitly ("**A′ — in-place region mutation of a rendered pane**: a fold splice shifts every buffer offset below the region while the source is unchanged") and mark rows 4–6 against it with how each survives, or add a row for the preview-side maps `preview::splice`… |
| **F-SPEC-008** ✅ DONE | The disclosure toggle publishes an interactive accessible role with no accessible name | `src/widgets/disclosure.rs:62-79` (`fn build`, grep-confirmed at `:62`) | In `build()`, call `crate::a11y::name(&toggle, …)` with the block's summary label — the caller in `renderer::start` already holds it at `:531-541` and can pass it in, which also makes the announcement say *which* disclosure. Extend the a11y walk's fixture to open a document with a disclosure so the … |
| **F-SPEC-009** ✅ DONE | `sdd/system-overview.svg` is unchanged, and its render-pipeline caption now states something this branch measured to be false | `sdd/system-overview.svg:127-129` | Amend the render-pipeline box to name the three paths and what each preserves (append: full render; `re_render` buffer swap, which loses the reading position and re-anchors; `splice`, region-only, which keeps the widget tree *and* the position). Re-run `xmllint --noout sdd/system-overview.svg` and c… |
| **F-TEST-005** | The spliced `RenderData` install is a 14-field hand copy with three fields ever asserted, and no differential oracle against a full re-render | `src/preview/splice/install.rs:142-239` (`install_outcome`), field writes at `:190-204` | One differential test, which covers all fourteen at once and cannot rot as fields are added. `splice/tests.rs` already builds both sides (`assert_splice_matches_full_render` renders `after` fresh for the text comparison), so the marginal cost is small: after the char-identity assertion, compare the … |
| **F-TEST-006** | ~2,900 lines of the excursion rig assert properties of GTK's own validation budget inside a REQUIRED CI gate, with self-admitted order dependence | `src/preview/splice/excursion/kink.rs:255-430` (the grid), `:110` (`DOSES`), `:125` (`REPEATS`), `:118` (the order-dependence admission); same shape in `budget.rs:265`, `margin.rs:359`, `trace.rs:242`, `wholelist.rs:189`, `dose.rs`, `drift.rs:246/286/314` | Split the rig by *what the assertion is about*, which is a one-line change per body and no loss of any measurement: - **Keep in the gate:** `excursion.rs`'s two `compare_routes` bodies and all of `wired.rs`. These assert *this project's* contract (TDD 2.26h/i/j) and are exactly right where they are.… |
| **F-TEST-007** | `after_scroll_settles` has no test of any kind and no injection point for its clock, its tick source, or its layout oracle | `src/farscroll/settle.rs:103-173`, tick source at `:146`, oracle arm at `:116` | Two steps, in order of value. 1. **Make the wrapper drivable.** Split the body into `after_scroll_settles` (the public seam, unchanged) and a `fn arm_settle(view, tick: Duration, valid: Rc<Cell<bool>>, f)` it delegates to. That gives a GTK-integration test a way to run the whole state machine at, sa… |

---

## 🟢 Low

10 findings, post-filter. Batch these into one cleanup pass — do not round-trip them
individually. Pushback is expected and welcome: "not worth fixing because X" closes a
Low finding.

| ID | Finding | Location | Proposed fix |
|---|---|---|---|
| **F-AP-018** ✅ DONE | The deletion range is taken from a previous render's extents and used without validation; `iter_at_offset` clamps silently | `src/preview/splice.rs:140-155` | Before the delete, refuse (returning `None`, so the caller falls back) when `old_volatile.end > buf.char_count()` or `old_volatile.start > old_volatile.end`, and log it. |
| **F-AP-020** ✅ DONE | A stale or shifted body range makes a hidden-match count silently zero | `src/window/find.rs:355-365` (the discard at `:360`) | `let Some(hidden) = rd.md_owned.get(block.body.clone()) else { log::error!("preview find: collapsed body range {:?} is outside md_owned", block.body); continue; };` |
| **F-DRY-007** | `excursion::wired::Wired` re-implements `excursion::rig::Rig`'s settle discipline, and `READING_FRACTION` is defined twice | `src/preview/splice/excursion/wired.rs:141-191` (and its twin | Extend `rig.rs`'s existing free-function seam by three more entries, and delete `wired.rs:63-66`: ```rust // src/preview/splice/excursion/rig.rs — beside top_line_text/anchor_reader/reader_offset /// Pump until line heights are validated AND the range has been quiet for `quiet`. /// The `quiet` wind… |
| **F-DRY-014** ✅ DONE | `copymap::construct_of_tag` and `construct_of_tagend` are mirrors enforced only by a comment | `src/copymap.rs:175-203` (and its twin `src/copymap.rs:131-173`) | Generate both from one table so the mirror is structural: ```rust macro_rules! constructs { ($( $c:ident => ($tag:pat, $end:pat) ),+ $(,)?) => { fn construct_of_tag(tag: &Tag) -> Option<Construct> { Some(match tag { $( $tag => Construct::$c, )+ _ => return None }) } fn construct_of_tagend(end: TagEn… |
| **F-DRY-015** ✅ DONE | Two "what the page would say" reductions, and the new one omits the tight-construct marker suppression | `src/renderer/disclosure.rs:295-342` (and `src/outline/mod.rs:142-158`) | One reduction, in `renderer`, taking the block-scope table both callers already build: ```rust // src/renderer/mod.rs (or a small src/renderer/plaintext.rs) /// The text a run of Markdown WOULD render as: inline markup stripped, tight-construct /// fence markers dropped through `BlockScripts`, block… |
| **F-SEC-004** ✅ DONE | `has_attr`'s boundary rule reads `open` out of another attribute's quoted value | `src/renderer/rawhtml.rs:164-183` fn has_attr` is the sole match at 164; `let ends = tag[after..]` is the sole match at 173) | `has_attr` should not scan the raw tag text. Once F-SEC-002's quote-aware tokenizer exists, give it an attribute iterator (yielding `(name, value)` pairs with quote state honoured) and implement both `attr` and `has_attr` on top of it. That removes the whole class rather than adding a third boundary… |
| **F-SPEC-010** ✅ DONE | A new test's doc comment states a divergence the same commit closed, and points at the deleted plan | `src/preview/build.rs:3545-3551` (the "deliberately NOT asserted" sentence grep-confirmed at `:3547`, "See the plan." at `:3549`, the test it documents at `:3551`) | Delete the paragraph and replace it with a one-line pointer to `an_unspaced_disclosure_body_shows_as_literal_text` at `:3876`, which is where that clause is now pinned. |
| **F-TEST-002** ✅ DONE | The two dispatchers this branch grew past 280 LOC hold every new decision behind a live `GtkTextBuffer` | `src/preview/build.rs:279-604` (`build_products`, ~326 LOC); `src/renderer/start.rs:12-298` (`Renderer::start_tag`, ~287 LOC) | Not a rewrite. One targeted extraction each, chosen so it pays for itself: from `start_tag`, a pure `fn disclosure_action(event_kind, frame_stack_top, folds, open_in_source) -> DisclosureAction` returning an enum (`Suppress`, `EmitSummary`, `OpenFrame`, `PassThrough`) that the vfunc then executes — … |
| **F-TEST-003** ✅ DONE | `splice()` has no display-free seam; the one piece of pure arithmetic in it — the delete boundary — is inline against `TextIter` | `src/preview/splice.rs:150-155` (the delete boundary), `:113-199` (`splice`) |  |
| **F-TEST-009** ✅ DONE | `imagecache`'s thread-local cache has no reset seam and no injectable clock at the production boundary | `src/imagecache/mod.rs:75-78` (the `thread_local!`), `:98-113` (`get_or_fetch`, which supplies `Instant::now()` itself) | Add a `#[cfg(test)] pub(crate) fn reset_for_test()` that replaces the cell's contents, and take `now: Instant` as a defaulted parameter on the wrapper (or expose a `get_or_fetch_at(uri, now, fetch)` that `get_or_fetch` calls with `Instant::now()`). Both are two-line changes and together make the 2.2… |

---

## 🧹 Tidy (Optional)

6 findings, post-filter. No functional impact. Fix only where the cost is trivial *and*
it improves clarity for the next reader. **Terminal** — no verification round follows
and you need not explain a skip.

| ID | Finding | Location | Proposed fix |
|---|---|---|---|
| **F-AP-021** ✅ DONE | `FoldKey`'s field is `pub`, so the offset-space guarantee its own doc claims is not enforced | `src/fold.rs:57-65` | Make the field private with `FoldKey::from_source_offset(usize)` and `fn source_offset(self) -> usize`. Mechanical, and it makes the claim in the doc comment true. |
| **F-AP-022** ✅ DONE | `recognise_html_element`'s tag-name boundary set omits `\n`, `\r` and `\f` | `src/renderer/rawhtml.rs:100-108` (the predicate at `:106`) | `b.is_ascii_whitespace() || b == b'>' || b == b'/'`, and add `<details\nopen>` to that test. |
| **F-DRY-019** ✅ DONE | `imagecache::policy::record_success` carries a leftover doc paragraph describing a method that no longer exists | `src/imagecache/policy.rs:102-113` | Delete `policy.rs:102-104` (the stray paragraph), and change `policy.rs:12` and its neighbours to link the free function `[`get_or_fetch`]`. |
| **F-SPEC-014** ✅ DONE | `Renderer::collapsed_site`'s doc comment is stranded above a different function | `src/renderer/mod.rs:999-1000` (the orphaned comment) and `:1027` (`pub(crate) fn collapsed_site`) | Move lines 999-1000 down to sit immediately above `:1027`. |
| **F-SPEC-015** ✅ DONE | `farscroll::settle`'s module doc contradicts itself about what bounds the wait | `src/farscroll/settle.rs:41` and `:57` | Reword `:57` to "the gate is the conjunction, with an absolute tick cap **on top of** the stalled-progress term — the same 'promptly when it settles, late and against partial geometry when it never does' degradation `after_line_heights_validated` already chose." |
| **F-TEST-008** ✅ DONE | `hidden_match_count` is a pure function left inside the coverage-excluded `window/find.rs`, with no unit test | `src/window/find.rs:386-396` | Move it beside `body_plain_text` in `src/renderer/disclosure.rs` (in scope, already carrying that module's tests) or into `find/plan.rs`, and add three cases: empty needle, empty body, and a case-mismatched match — the last pinning that hidden and visible matches fold identically, which is the sente… |

---

## ⛔ Withdrawn

### F-GATE-001 — "the build pipeline fails at step 1" — **WITHDRAWN, orchestrator error**

Reported as High: `cargo fmt --check` failing on `src/widgets/table/mod.rs:346,570`,
attributed to pre-existing rustfmt drift, with the claim that fail-fast meant steps 2–9
had never run on this branch.

**It does not reproduce at `HEAD`:**

```
$ git archive c0e1c0f | tar -x -C /tmp/qa-pristine
$ cd /tmp/qa-pristine && cargo fmt --check ; echo $?
0
```

The diff lived entirely in `linux`'s uncommitted working-tree edit, in flight when the
pipeline run read the tree. The faulty step was substituting one claim for another:
`git diff --name-only 363790e..HEAD -- <file>` returning nothing proves the file is
*unchanged by the branch*, not that it is *clean at HEAD*.

Recorded rather than deleted because the failure is instructive: this is the same
dirty-tree artefact the orchestrator had already caught and rejected for the
integration suite an hour earlier, and had already written into the audit trail as a
principle — and then the consolidated review overrode its own audit trail with a
stronger, wrong claim. A gate result against a tree not pinned to a SHA is not
evidence, whichever direction it points.

---

## ✅ Resolved

None yet — this is Round 1.

---

## ✅ Positives

Recorded because they are load-bearing, not for morale.

- **The design record is exceptional.** Nearly every non-obvious decision in this
  branch carries a measurement, a rejected alternative and a reason. The reviewers
  were repeatedly able to test a claim *because the claim was stated precisely enough
  to be falsifiable* — the plan's own "the nesting caveat is the whole security content
  of this change" is what pointed four reviewers at F-001. Vague design docs do not
  produce findings like these; they produce shrugs.
- **`imagecache/policy.rs` is a model extraction** — the eviction/TTL policy as a pure,
  unit-tested core with the GTK-touching half kept thin. The DRY reviewer named
  `install.rs`'s `merge`/`merge_pairs`/`keep_survivors` as exemplary for the same
  reason.
- **The unclosed-`<details>` fix is right and well-argued.** "A control that cannot
  fold anything is a control that lies" is the correct call, and deliberately not
  copying the browser's implicit close is defensible and documented.
- **Export escaping traced clean.** A summary label of `</summary><script>` does not
  become script in the exported artefact. That is the sink where this feature could
  have gone genuinely wrong, and it did not.
- **Link integrity is spotless.** The plan retirement in `c0e1c0f` left zero dangling
  references tree-wide — including removing the `AGENTS.md` entry in the same commit
  that deleted the file. Retirements usually leak; this one did not.
- **Line-number discipline held.** 254 file:line citations across the panel, **zero**
  out of bounds — and on the ~30 the spot-check auditor resolved semantically, every one
  pointed at the code described (one off-by-one, at `mod.rs:1013`). Note this is a
  bounds check plus a sample, not proof of every citation: the withdrawn F-GATE-001
  cited two in-bounds lines that pointed at the wrong content.
- **The coverage floor drop is documented to a standard most projects never reach** —
  three measurements, the rejected alternative, and an explicit note that it is the
  second instance of one cause. It is still a finding (F-014), but as a routing
  decision, not a concealment.

---

## Checklist Assessment

| Area | Status | Notes |
|------|--------|-------|
| Functionality | ❌ | F-AP-007 changes which blocks are collapsed on one keystroke; F-003 corrupts ordinary documents in both sinks. F-002 is a **latent** invariant violation — no reaching input is known and `debug_verify` catches it in debug builds. |
| Code Quality | ⚠️ | Strong extraction in places (`imagecache/policy`, `install::merge`); offset spaces share one integer type, and the tag tokenizer is written twice. |
| Testing | ⚠️ | 1457 unit tests pass. But the splice's only correctness oracle is `debug_assertions`-only, `after_scroll_settles` has no test at all, and 4 new rubrics have no check. |
| Security | ⚠️ | Thesis holds; the guard implementing it does not (F-001, F-010). Bounded blast radius — no execution surface. |
| Performance | ⚠️ | Synchronous uncached fetch on the main loop is bounded but real (2.26k measured a 5.001s freeze); `live_anchored` walks the buffer per toggle. |
| Documentation | ⚠️ | Plan retirement is clean, but TECH.md has no row for the branch's headline subsystem and `system-overview.svg` is unchanged. |

---

## Gate results

All taken against `HEAD` = `c0e1c0f`, except where noted. ⚠ The integration suite result
is inconclusive and step 1's original FAIL was withdrawn — both were artefacts of a
dirty tree. See audit trail §6 and §11.1.

| Gate | Result |
|---|---|
| 1. `cargo fmt --check` | ✅ **PASS at `HEAD`** — the earlier FAIL was a dirty-tree artefact; F-GATE-001 withdrawn |
| 2. `cargo clippy --all-targets --features gtk-integration-tests -- -D warnings` | ✅ clean, zero warnings |
| 3. `cargo build --release` | ✅ clean |
| 4. `cargo test` | ✅ 1457 + 41 passed, 0 failed, 2 ignored |
| 5. `scripts/run-integration.sh` | ⚠️ inconclusive — first run raced a dirty tree; re-run pending a committed SHA |
| 6. coverage ratchet | ⚠️ not executed; inspected statically — F-014, F-GATE-003 |
| 7–9 | not run by QA. `linux` reports `scripts/pipeline.sh` green end-to-end on their working tree; unconfirmed against a pinned SHA |

---

## Evidence trail

Per-reviewer findings, retained rather than deleted this round because `docs/` is
gitignored and the round is not closed. Each carries the full argument, the
reproduction and an honest per-group coverage table.

| File | Reviewer |
|---|---|
| `docs/code-review-round-1-security.md` | Security (+ 4 Needs-Verification, 19 traced-clean surfaces) |
| `docs/code-review-round-1-spec.md` | Spec compliance vs plan, TDD, POLICY, TECH, CAM |
| `docs/code-review-round-1-antipattern.md` | Anti-pattern / design health |
| `docs/code-review-round-1-dry.md` | DRY / abstraction |
| `docs/code-review-round-1-testability.md` | Testability / gate design |
| `docs/code-review-round-1-links.md` | Link integrity (clean) |
| `docs/code-review-round-1-orchestrator.md` | Orchestrator gate runs |
| `docs/audit-trail.md` | Decisions, drops, corrections, coverage holes |
