# Plan: Exporting a document to HTML and PDF

**Status**: **approved and implemented on Linux** (2026-08-19), **verified on macOS and on
Windows** (2026-08-20). TDD §25 is landed; `tests/MANUAL-TEST.md` §25 is written.

**Windows verification, as ratified by that seat**: every platform question closed. The
named held-open-destination failure ([25.24](TDD.md#2524-a-destination-another-process-holds-open-fails-by-name))
proven end to end on **local NTFS** — the strict case — with a lock positive control, the
app's own log chain (`os error 5` → named message) and the reader-facing pixel on both the
toast and the status bar; the stem-plus-extension guarantee driven through the real Win32
chooser with the source file byte-identical afterwards; the emoji limit measured with the
CESU-8 trap reproduced exactly; O9's three discriminators re-checked against the shipped
sink and **unchanged**; the promote gate confirmed to consult neither `is_finished()` nor
`status()`; and [25.15](TDD.md#2515-an-export-does-not-move-the-footprint) **passing on both
halves**, which macOS could not reach. Measured on GTK **4.22.4** — not the 4.6 floor, so
this seat can falsify a floor claim but never confirm one.

**macOS verification, as ratified by that seat**: nine of the ten platform questions
verified — the librsvg/`GdkTexture` decode, `pdfimages -list` showing real image objects,
the tight-list-item paragraph, the live `NSSavePanel` naming behaviour, the emoji limit
(re-measured CESU-8-aware, see [P-13](#p-13-two-extractor-output-encoding-traps--the-second-manufactured-a-long-lived-false-finding)),
the first clipboard round trip anyone has run on this platform, the open-destination
promote, the absent always-ready source, and the promote gate. **One is unverified rather
than passing on that platform: [25.15](TDD.md#2515-an-export-does-not-move-the-footprint)'s
RSS half — it passes on Windows, where the confounding chooser growth does not occur.** Its
GPU half is verified — the process never appears as a GPU client, before or after 30 export
cycles, at either document size. The RSS half cannot be measured while every cycle opens a
file chooser, because **the chooser itself grows RSS by ~1 MB per invocation on macOS,
whether or not an export follows, and the plain Open dialog does the same**.

**One macOS reading is INFERRED and its own report labelled it MEASURED — corrected here.**
[TDD 6.4](TDD.md)'s document-size conjunct: the macOS seat's verdict line reads "GPU:
MEASURED zero, both document sizes", but its narrative says the third check, on the
large-document instance, was **not taken** — the operator's own session had come frontmost
and the seat correctly stopped rather than click into their windows, then reasoned from
mechanism that GTK does not switch renderers on document content. That reasoning is sound
and the conclusion is very probably right, but it is an **inference, not a measurement**, and
the summary line outran it. The equivalent leg **is** measured on Windows at a 200× document
([P-19](#p-19-no-gpu-client-has-two-forms-on-windows-and-a-missing-counter-row-is-indistinguishable-from-a-broken-counter)).
Re-taking it on macOS is cheap and wants only a free display. That is an
application-wide defect export merely exposed, it is in the issue register, and **it is not
export's to fix**. A Linux probe over 40 chooser cycles did not reproduce it — but with the
caveat that GTK backs the dialog with its own widget there and with a real `NSSavePanel` on
macOS, so that negative is about a different implementation.

**This plan stays until all three seats have implemented it**, then it is retired in
one pass: the [parked findings](#parked-findings) are *moved* to the homes their
Destination lines name, and the file is deleted. Retiring it now would delete the
document the other two seats implement from.

**Sequencing, operator-set and binding: one seat at a time, no cross-seat chatter
while an implementation is in flight.** Linux is done; macOS is done (2026-08-20);
Windows starts only when told and finishes completely. Overlapping seats on one
feature is what forced a full revert once before.

**What landed on Linux**: R1 (the `AtomicPublish` scaffold), R2 (the raw-HTML
allowlist as named data), phase 1 (HTML) and phase 2 (PDF). **One deliberate
addition to this plan**: `pangocairo` 0.21 is a new dependency — no binding for
`pango_cairo_show_layout_line` exists in `gtk4`/`pango`/`cairo-rs`, and the library is
already linked into the process by GTK. Recorded in [TECH.md](TECH.md#rust-crates).

**Two defects the operator found by exporting real documents, both fixed — and both worth
knowing before you port this, because each was invisible to a passing suite:**

1. **A tight list item's content arrives with no `Tag::Paragraph` around it**, so a consumer
   that paragraphs per event splits the item into one block per token. Recorded as
   **ScrAP-298**; the fix is `walk.rs`'s implicit-paragraph frame, flushed at **block**
   boundaries only. A fixture with one inline run per item cannot detect it.
2. **The PDF sink drew an italic `[image: …]` note and discarded the decoded bytes**, so an
   exported PDF held no image objects at all while the HTML half embedded correctly the
   whole time. `pdfimages -list` on the artefact is the check; a screenshot is not, because
   a placeholder renders as *something*.

**A host-configuration finding the other two seats must check rather than assume**: SVG
images decode through `GdkTexture`, which means through gdk-pixbuf's **librsvg loader**.
That loader is present on this Linux host, so the README hero and TECH.md's system diagram
both embedded. Where it is absent the decode fails and the export falls back to a visible
note. Verify it per platform; it is not a code difference.

**One rubric is not implemented**: 25.23's interactive **Cancel**. The "Exporting…"
busy notice exists; a cancel affordance does not, because `run(Export)` blocks its
caller and the plan's own measurement says an always-ready source installed for the
duration of an export hangs macOS and livelocks Windows. It wants its own design pass.

All three platform seats measured the feature's foundations before it was built and
**nothing blocked it**. **All ten open decisions are settled.** Findings that would normally
go to `sdd/ANTI-PATTERNS.md` are still held in [§ Parked findings](#parked-findings): they
distribute at **retirement**, once all three seats have implemented, not at first
implementation — several of them are about macOS and Windows behaviour that only those seats
can confirm still holds.

## Problem

Scribobulate renders a Markdown document faithfully and is where a human reviews prose an
agent wrote — but that rendering never leaves the window. A reader who wants to hand the
reviewed document to someone without Scribobulate installed has only the Markdown source,
which is the form they were using this application to avoid.

- **HTML** — one file that opens in anything, keeps links and heading anchors, re-flows to
  the reader's window. The sharing format.
- **PDF** — fixed pagination, prints, archives unchanged. The record format.

Neither exists. The nearest things are Copy Document and `copymap`'s copy-as-Markdown, both
deliberately *round-trip* formats rather than presentation ones.

**Why one plan covers both**: they share everything except the final sink — source of truth,
theme resolution, the command, the chooser, the write path, the image/link/raw-HTML
policies, the annotation representation, and every CAM obligation. Building them separately
would duplicate seven decisions and let them drift. **One pipeline, two sinks**, phased.

## Constraints inherited

Each rules out an otherwise obvious move.

- **No web engine or HTML-rendering dependency** (POLICY § Dependencies; ScrAP-1).
  *Emitting* HTML is not embedding an engine. Operator: a dependency is unnecessary — the
  HTML emitted here is simple enough to hand-code.
- **No hard-coded styling** (POLICY; THEMING.md). An export sink is a **third** application
  path for every theme key, beside the body buffer and the table cell.
- **One `GAction` per command** (POLICY; ScrAP-9), accelerator declared once.
- **Document reads and writes go through `docio`, off the main thread** (POLICY). An
  exported file is a document write, and its completion lands later (CAM row 7).
- **A Markdown document is untrusted content** (TDD 2.7). These documents arrive by agent,
  clone and hand-off; they can carry embedded HTML or script, and image `src` values that
  escape the document folder. On screen those are contained. **An export removes that
  containment**, because the artefact is opened by software this project neither controls
  nor sandboxes. The obligation is stricter here, never looser.
- **Input limits** (`limits::is_regular_file_within_limit`) admit what export reads —
  images, and only images.
- **A new rendering path is a "significant change"** (CAM Document Rendering row 10 →
  TDD §6), so the footprint measurement is owed.
- **The main thread is the only thread** (gtk4-rs guardrail #1; GTK4Rs/AP-148).
- **`ANTI-PATTERNS.md` and `ISSUES.md` have one writer**, the Linux seat (POLICY). Findings
  from other seats arrive as entry content, not edits.

## Export is a function of the document source, not the preview widget

Three independent reasons, any one sufficient:

1. **A tab may have no preview.** Deferred tabs build on first activation; edit-only mode
   never builds one. A widget-reading exporter silently depends on what the user did
   beforehand.
2. **The widget tree is only correct where it is visible.** Off-screen anchored children
   are parked at negative coordinates and painted that way (GTK4Rs/AP-166); line heights
   validate lazily with no completion signal (ScrAP-260); a geometry read before allocation
   answers the buffer's *last* line, silently (GTK4Rs/AP-263).
3. **Only a display-free pipeline is testable and inside the coverage gate** (POLICY
   § Build pipeline step 6). The gate can only run code the suite can execute, and the suite
   runs without a display — so a widget-reading exporter sits permanently outside the
   coverage number and its correctness rests on a human opening the file.

The precedent is in the tree: **`copymap`** is a pure, unit-tested second consumer of the
same event stream, for exactly this reason (GTK4Rs/AP-5). Export is that shape with a
different output — and it then works in every view mode, on a never-rendered tab, on an
unsaved buffer, with no display.

## Chosen approach

**HTML sink — H2: a display-free emitter over the same normalised event stream.** Enter
through the conditions the preview enters through — CriticMarkup extraction →
`renderer::md_options` → `normalize_inline_tabs` → the event stream, plus
`scan_script_spans` — and build a display-free `ExportDoc` the sink serialises. Every
decision about *what a construct is* is made once, upstream of both consumers. It agrees
with the preview by construction rather than by vigilance, and reuses `links`, `limits`,
`annotate` and the theme engine rather than re-deciding any of them. The cost — an emitter
to keep in step as constructs are added — is paid once, because **both sinks consume the
same `ExportDoc`**, and the Document Rendering CAM gains an **export cell** so a new
construct cannot land without one.

**PDF sink — P1 (`GtkPrintOperation` in Export action) writing to a temp file, promoted on
success.** Decided by the operator; see [O8](#o8--how-the-pdf-sink-reaches-disk).

### Rejected — recorded so they are not retried

| # | Approach | Why not |
|---|---|---|
| H1 | `pulldown_cmark::html::push_html` over a fresh parse | A **different renderer**. Blind to constructs a second tokeniser owns (`^sup^`, `~sub~`, `~~strike~~`, `==highlight==` — ScrAP-66/195) and to CriticMarkup; never consults the scheme allowlist or the image containment gate; **emits raw HTML verbatim**, so a `<script>` in an agent-written document would land executable in a file the user is about to send. |
| H3 | Serialise the preview's `GtkTextBuffer` and tag set | Tags are display artefacts with no document meaning; tables, images and rules live in anchored widgets *outside* the buffer; `buffer.text()` omits child anchors so every later offset drifts (GTK4Rs/AP-5). |
| P3 | Snapshot the live preview widget onto a PDF surface | Fails all three reasons above; impossible for a deferred tab; unverifiable headlessly. Recorded because "we already draw this, just draw it somewhere else" is the first idea everyone has — and it is true only of the band on screen. |
| P4 | Offscreen `GtkTextView` at full document height, drawn in page slices | Depends on GTK laying out and validating an unrooted widget with no frame clock — the class of behaviour this project has repeatedly measured to differ from the documentation (GTK4Rs/AP-166, GTK4Rs/AP-263, `farscroll`). Not rejected outright: if P1 *and* P2 both fail, spike it behind a measurement. |
| P5 | Emit HTML and hand it to an external converter | No converter exists on all three platforms; output would depend on software the project neither ships nor tests; spawning a process is a new architecture and a new attack surface for untrusted content. |
| P6 | `GtkPrintOperation` in **Preview** action with our own cairo context | **Not rejected — measured, viable, the strongest alternative.** Keeps GTK's pagination and `PrintContext`-resolution metrics while owning the sink, so the data-loss window is structurally absent rather than closed. Confirmed on all three platforms across GTK 4.6.9 → 4.22.4, byte-identical output to `run(Export)` at 3/5/100/400 pages, equivalent runtime. **Not chosen** because it pulls in the `cairo-rs` `pdf` feature that P1 avoids, and needs three nested signal handlers. Revisit if P1's promote proves awkward; its sharp edges are parked at P-10. |

## Module layout and phasing

**As built on Linux** (the planned five became seven; both splits were the 500-line rule
firing, and each landed on a real seam rather than an arbitrary cut):

```
src/export/
  mod.rs       the ExportDoc model, ExportTarget, the default-name guarantee
  doc.rs       display-free: entry point, claim placement, image reading
  walk.rs      display-free: the pulldown event walk → ExportDoc
  html.rs      display-free: ExportDoc + resolved style → String
  paginate.rs  display-free: measured fragments + page metrics → page boundaries
  markup.rs    display-free: an ExportDoc's inlines → Pango markup
  pdf.rs       the only GTK-touching file: Pango measurement, cairo drawing, the promote gate
src/window/
  export.rs      the win.export action, the chooser, the HTML dispatch, the notice
  export_pdf.rs  the GtkPrintOperation run and the staged-temp promote
```

**Every one of these is inside the coverage gate**, `pdf.rs` included — the plan expected it
to be outside, and it is not: `src/export/` is not in `coverage.sh`'s `IGNORE`, and the Pango
layout and the cairo draw are both reachable **without a display** (a font-map context and an
`ImageSurface`). Measured 94.77% on `pdf.rs`. "It touches the toolkit" is not on its own a
reason to exclude a file; the question is whether a `DISPLAY` is needed, and here it is not.
`pdf.rs` still stays a **thin adapter with no logic** — if it grows a decision, logic has
leaked into it.

**The two decision cores that would otherwise have hidden behind the `src/window/`
exclusion** — the default-name guarantee and the PDF promote gate — were deliberately moved
into `src/export/` rather than left in `window/`, per POLICY's scope rule. Do the same.

**Phase 1 — HTML.** Delivers the whole shared spine: command and surfaces, chooser, write
path, theme → CSS, image policy, annotation representation, status notice, rubrics. The
sink is a string, so the spine is proven before any pagination is attempted.

**Phase 2 — PDF.** Adds `paginate.rs` and `pdf.rs`. The gate between phases is deliberate:
if phase 2 stalls, the user has the sharing format and the tree has no half-built second
renderer.

> **Both phases are through on Linux.** A porting seat is not re-deciding the phasing: the
> code exists and is platform-neutral. What each remaining seat owes is **verification on its
> own platform** — the §25 rubrics that carry a platform caveat (25.18b's emoji limit, 25.19's
> clipboard round trip, 25.24's held-open destination on Windows), the chooser's per-platform
> naming behaviour (O10), and the librsvg question in the header. Where a platform needs a
> code change, it belongs behind the existing seams, not in a fork of the pipeline.

## Surfaces and mechanics

- **Command**: one `win.export`, parameter `s`, targets `html` and `pdf` — the
  `win.change-case` shape. SCHEMA.md's `win.` table gains the row. Enabled whenever the tab
  holds a document, **including untitled and unsaved**: export reads the buffer, never the
  disk file. That is TDD-visible behaviour, not an implementation detail.
- **Surfaces**: main menu only — File ▸ Export ▸ { PDF, HTML }. No toolbar button, no
  accelerator ([O4](#o4--which-surfaces)). One action drives every surface's
  sensitivity, so adding a surface later is a menu-model change rather than new plumbing.
- **Destination**: the existing native file chooser. A `FileChooserNative` needs an external
  owning reference — the one dialog type with the opposite liveness rule from a `GtkWindow`
  (GTK4Rs/AP-41).
  - The default name is the document **stem** plus the target extension — **never the
    document's filename**. A correctness guarantee, not a convention
    ([O10](#o10--the-default-name-and-filename-validation)).
  - Validate the name this application **proposes**, through `docio/rename.rs`'s rules;
    never the name the chooser **returns**.
  - The chooser's **initial folder is best-effort and its failure unobservable** on Windows
    — nothing downstream may assume the dialog opened where it was asked (parked P-7).
- **Write (HTML)**: a new `docio` operation on GLib's pool under `docio/pool.rs`'s admission
  cap (ScrAP-243), serialised per destination path. A **deferred operation**, so CAM row 7
  applies in full: the tab can close, the document can reload, the window can close while
  the write is out, and the subject must be resolved **once** and carried (ScrAP-244).
- **Write (PDF) is a different write.** `set_export_filename` + `run(Export)` has GTK and
  cairo open and write the file themselves; this project never holds the bytes. **GTK opens
  — and therefore truncates — the destination before the first page is drawn** (measured:
  zero `draw-page` calls when the open fails), which is why the export writes to a **temp
  file** promoted only on success ([O8](#o8--how-the-pdf-sink-reaches-disk)). `run(Export)`
  blocks the **caller** on the main thread; it does not freeze the loop. **Never dispatch it
  onto `docio/pool.rs`'s workers.**
- **The promote gate**: promote **iff** `Ok(PrintOperationResult::Apply)` **and**
  `pages_drawn == pages_expected`. **Never** consult `is_finished()` or `status()` — both
  are inverted (parked P-4). `Ok(Cancel)` deletes the temp and never promotes: a cancelled
  run leaves a **structurally valid, `%%EOF`-terminated, correctly-xref'd** PDF that no
  integrity check can distinguish from a complete one.
- **Drawing call**: `pango_cairo_show_layout` (or `show_layout_line`). **Never** a per-run
  `show_glyph_string` / `show_glyph_item` loop — `show_glyph_string` hands cairo positioned
  glyphs with no UTF-8 or clusters and **silently destroys the text layer**, and
  `show_glyph_item` puts pen management on the caller.
- **Never install an always-ready source for the duration of an export.** The fatal property
  is **always-ready, not high-frequency**: a 1 ms timeout is harmless on both platforms,
  while an idle returning `Continue` unconditionally **hung** the export on macOS and
  **livelocked** it on Windows (330 s of CPU on a half-second export).
  **As built, the export path DOES install one source — do not conclude otherwise from a
  grep of `src/export/` and `src/window/export*.rs`, which finds none.** `BusyNotice::arm`
  (`winstate/busynotice.rs`) calls `timeout_add_local_once`, and both export entry points
  arm it. It is safe on all three axes that matter: **one-shot** (`_once`, so it cannot
  re-arm), **delayed 500 ms** rather than always-ready — strictly weaker than the 1 ms
  timeout already cleared — and **removed on `Drop`**, so a fast export cancels it before it
  fires. Recorded this way deliberately (Windows seat, 2026-08-20): a reader grepping only
  the export files gets the right verdict from the wrong facts, and would not notice if
  `BusyNotice` were later changed to a repeating source.
- **Progress**: no progress bar exists today; when one is added it belongs in the **status
  bar**, not a dialog. Thresholds: under 250 ms needs no spinner, under 100 ms needs no
  backgrounding. Against the measured curves a document must exceed **~40 dense pages**
  before backgrounding is warranted and **~100** before a spinner is. Run synchronously on
  the main thread and show the indicator only once the export crosses the threshold, driven
  by pages completed. **The trigger must be elapsed time, not a page count** — the same page
  count is ~40× apart in cost between dense and sparse content.
- **Cancellation**: `PrintOperation::cancel()` from inside `draw-page` returns `Ok(Cancel)`
  and stops after the current page. A Cancel button is in scope for phase 2.
- **Notice**: success *and* failure raise the transient status notice (CAM row 8) — a silent
  export is indistinguishable from a broken one, and the file it wrote is somewhere the user
  did not watch.
- **Logging**: export start and completion are lifecycle boundaries → `info`, once, at the
  choke point; path and byte count, never content (POLICY § Logging).

## Decisions

### ✅ O1 — Which theme does an export use?

Both sinks resolve every value through the theme engine; no literals in either. **HTML uses
the active reading theme.** **PDF resolves through the same engine against the System
theme's light resolution by default** — paper has no dark mode. "Default to System-light" is
a resolution request, not a literal; an implementation reaching for a hex value here has
broken the rule the decision was made under. Deferred, not dropped: an explicit "export in
the active theme" choice, which costs a parameter on a command that already takes one.

### ✅ O2 — How do images travel?

**Local images admitted by the containment gate are embedded** — data URIs in HTML, drawn
into the PDF. A relative-path HTML breaks the moment the file is moved, which is the only
reason anyone exported it. Base64 is ~20 display-free, unit-tested lines rather than a new
crate.

**Remote images are referenced by URL and not fetched at export time — unless the document's
"Show Unsafe Images" gate is on**, in which case they are embedded, because enabling that
option *is* the user ratifying them. This is a security decision, not a performance one, and
the gate is **per-document** state (TDD §14): never a global preference, never inferred from
another tab.

**Consequence to communicate rather than let a reader discover**: a PDF exported from a
document with remote images and the gate off has **gaps** where those images are, since a
PDF cannot reference a URL the way HTML can.

### ✅ O3 — Do annotations appear in the export?

**Yes, by default.** The in-file review loop is the product thesis, and an export that drops
the review is the wrong document. HTML: the claim highlight plus the comment as an aside.
PDF: the claim highlight plus a margin note, matching the preview. No toggle in v1.

**Constrains [O6](#o6--page-size-and-margins-phase-2)**: a margin note needs margin
to sit in, so the page setup must leave room for one.

### ✅ O4 — Which surfaces?

**Main menu only — File ▸ Export ▸ { PDF, HTML }. No toolbar button.** Two reasons, and the
second outlives the first:

1. The file toolbar's button section is already cluttered.
2. **Export is peripheral to this application's primary audience** — software developers
   working with AI agents, who review prose here and act on it in their own tools.

A toolbar reorganisation would retire the first reason; the second stays true until the
audience changes. Menu order is PDF then HTML — presentation order only. HTML still ships
first, in phase 1.

### ✅ O5 — Raw HTML in the source

The preview **drops** every raw-HTML construct except `<picture>`/`<source>`/`<img>`, whose
`src` still passes the containment gate (ScrAP-147 / TDD 2.23).

**DECIDED: the export reproduces that omission** — not an escape, and certainly not a
pass-through. Escaping would put text on the page the preview never showed; passing through
would put executable markup from an untrusted document into a file the user is about to
send. **Sanitize-by-omission is already this project's answer; export inherits it rather
than inventing a second one**, which is also the only form that satisfies the
untrusted-content constraint, since an export is opened by software this project neither
controls nor sandboxes.

**There is no allowlist as data.** The permitted set is four hardcoded branches in
`scan_image_tags` (`src/renderer/picture.rs`) — `<picture`, `</picture`, `<source`, `<img`,
each matched on a tag-name boundary so `<source>` ≠ `<sourcex>` and `<img>` ≠ SVG's
`<image>`. Export must reproduce that set exactly and **cannot refer to a set that does not
exist**, so it either calls the same scanner or grows a second copy that drifts. Addressed
by [R2](#r2--consolidate-the-raw-html-element-allowlist).

### ✅ O6 — Page size and margins (phase 2)

Take the platform's default `PageSetup`, with **page size and margins read from the existing
configuration file** and the platform default as fallback. **No preferences or settings
dialog** — possibly in future; the config file is the cheap intermediate. Both seats measured
the platform default as US Letter (612 × 792 pt), so a European user is the first person who
will want the config key, which argues for having it from the start.

### ✅ O7 — A table too wide for the page (phase 2)

**Scale it to fit.** Not clipped, not reflowed. The v1 bound — how far scaling may go before
the result is unreadable — goes into the rubric at phase 2 kickoff. Scale-to-fit is a single
transform with an obvious correctness test; splitting a table across pages is a layout
engine, which is the scope creep [§ Risks](#risks) warns about.

### ✅ O8 — How the PDF sink reaches disk

**Write to a temp file and promote it on success.** Operator's decision: a pattern used
throughout desktop applications, reliable, and **already used by Scribobulate for saving
Markdown** — so this plan should **reuse or abstract that code** rather than reinvent it
([R1](#r1--extract-the-atomic-publish-scaffold)).

**Why the alternative cannot ship.** `set_export_filename` pointed at the user's chosen path
destroys that file **as its first act** — GTK opens and truncates the destination before the
first page is drawn. Measured end to end: exporting 400 pages over an existing 43,973-byte
PDF and cancelling at page 50 left a **171,327-byte, valid, cleanly-extracting 51-page PDF**
where the original was. Two properties make this worse than an ordinary half-write:

1. **The wreckage is a valid PDF.** Nothing looks broken — the user gets a plausible shorter
   document where the old one was.
2. **It is reached by pressing Cancel**, which is normal use. GTK4Rs/AP-167 was about a
   *failure* destroying the previous file; this destroys it on success-shaped intent.

**Obligations this decision carries:**

- The promote gate is **page-count-based**, never return-value-based — see
  [§ Surfaces and mechanics](#surfaces-and-mechanics).
- The reuse is a **refactor, not a call**: `atomic_io::write_atomic` owns create-temp →
  write → rename, and here the *write* belongs to GTK
  ([R1](#r1--extract-the-atomic-publish-scaffold)).
- The temp file must be created **private before GTK writes into it**, or the
  write-private-from-the-first-byte property the existing code guarantees is lost.

**Platform caveat**: on Windows an **open handle on the destination blocks the promote** —
`MoveFileExW` cannot replace a destination whose handles did not grant `FILE_SHARE_DELETE`.
The ordinary case is exporting over a PDF the user still has open in a viewer. Both POSIX
seats succeed unconditionally. It is also **destination-dependent** (measured failing on
local NTFS, succeeding on a network share), so a rubric must target **local disk**, the
stricter case. This needs a **named error** on Windows — "close the file and try again" —
not a generic write failure.

**Fallbacks if P1 ever fails**: P2 (direct cairo PDF surface, memory sink, publish through
`atomic_io`) is verified working on both 4.22.4 seats; P6 is verified on all three. Because
the paginator is shared, either swap is a sink change rather than a redesign.

### ✅ O9 — Emoji in the PDF export

**DECIDED 2026-08-19, on the operator's own hands-on test: accepted.** They opened a spike PDF
— built the proposed way, on the Windows seat — in **Edge** and in **Acrobat**, and measured:
it **renders correctly**, emoji are **selectable**, and **copy-paste from Acrobat into an
external editor works flawlessly** across a variety of emoji. Ruling: *"If it works well
through Acrobat on Windows then I don't care about any other readers on Windows."*

**The accepted limit, per platform** — this is what a rubric must hold:

- **Windows**: emoji render, select and copy correctly in mainstream readers. The reordering
  described below is confined to the layout modes of Xpdf 4.00 and is **out of scope by this
  ruling**.
- **macOS**: emoji text is absent from the file and unrecoverable in **any** reader. Accepted
  earlier as a rare edge case, and the **stricter** of the two limits.
- **Linux**: byte-correct.

**That test reached two places no seat could.** Acrobat was NOT-RUN for the whole survey — not
installed on that box — and was the reader everyone most wanted; the operator installed it. And
**copy-paste fidelity had never been tested by anyone**: every seat measured *extraction* via
command-line tools, which is a different path from a selection-and-clipboard round trip through
a GUI reader.

**Also decided: monochrome emoji are forbidden** — they are drawn differently, not desaturated,
so substituting them changes what the reader sees. This closes the one cheap escape from the
mechanism below: `PANGOCAIRO_BACKEND=fc` gives correct extraction order **at the cost of colour
emoji everywhere, screen included**. Trading colour away to satisfy one old extractor is a bad
deal and must not survive as advice.

#### Why this was ever in doubt — the mechanism, kept because a regression would look identical

**The exported page always renders correctly** on every platform — full-colour emoji, correct
glyphs, positions and order, verified against raster controls at 14 pt and 36 pt. **The issue
was only ever searchability**, and only in some tools:

| | renders | emoji text in file | neighbouring prose findable |
|---|---|---|---|
| **Windows** | colour | **yes** — correct `/ToUnicode` | **yes** (PDFium, Acrobat) |
| **macOS** | colour | **no** — absent entirely, unrecoverable on two independent stacks | yes |
| **Linux** (cairo 1.18.4) | colour | **yes** — byte-correct | yes |

**Two axes, and they are separate — do not collapse them.** *Colour vs monochrome* is the
**ordering** axis: a colour glyph takes the Type3 path and is mis-ordered by a layout
extractor, a monochrome one does not. *BMP vs astral* is the **encoding** axis: an astral
character emerges as CESU-8 surrogate halves ([P-13](#p-13-two-extractor-output-encoding-traps--the-second-manufactured-a-long-lived-false-finding)),
a BMP one does not. U+2705 sits on the safe side of the first and the unsafe side of the
second — it is **BMP *and* colour**, so it encodes cleanly and is *still* hoisted. Measured
on Windows 2026-08-20, both characters on one line of the real sink's output. A fixture
chosen to exercise one axis therefore says nothing about the other, and the sentence
"U+2705 round-trips cleanly on Windows" is true only of encoding.

**Cause on Windows.** pangocairo's win32 font map reaches cairo's **DWrite** colour-glyph path,
which rasterises the colour glyph and wraps it in a **Type3 font whose CharProcs are `d0`** —
the coloured-glyph operator, which unlike `d1` carries **no glyph bounding box** — painting an
image XObject. A layout-reconstructing extractor is left with no per-glyph extent under three
stacked y-flips and misassigns the row. The in-file control is sharp: U+2705 (colour) goes to
the Type3 font and is misplaced; U+2192 and U+2713 (monochrome) go to an ordinary Type0 on the
same line and are placed correctly. **Colour versus monochrome, not emoji versus not.**

**But the structure is not sufficient to break extraction.** poppler 22.02 orders the identical
Type3/`d0` structure correctly in every mode — **five extractors correct, one wrong**, the one
being the layout modes of Xpdf 4.00 (2017).

**And the Windows and Linux outputs are structurally identical where it matters**, measured
rather than inferred. Three discriminators were named in advance as the ones a layout analyser
keys on, and all three match:

| | Windows (DWrite / Segoe COLR) | Linux (FreeType / Noto CBDT) |
|---|---|---|
| `FontMatrix` sign | `[1 0 0 -1 0 0]` | `[1 0 0 -1 0 0]` — **including the `-1`** |
| `d0` vs `d1` | `d0` | `d0` |
| Image vs Form XObject | `/Image`, DeviceRGB 8bpc, `/SMask` | `/Image`, DeviceRGB 8bpc, `/SMask` |

**There is no third y-flip on Windows that Linux lacks** — the hypothesis worth moving bytes
for, and it lost. Both stack three flips at the same layers with the same signs. `/Encoding
/Differences`, CharProc body shape, `%PDF-1.7`, one `BT`/`ET` and zero `/ActualText` all match; (**the `/ActualText` zero is a property of those probe
fixtures, not of the sink** — an artefact containing a decomposed accent has one, and it
is load-bearing: see [P-18](#p-18-cairos-actualtext-span-is-what-makes-nfd-survive-the-round-trip-not-the-font-encoding).)
the only differences are font-metric magnitudes, which is what two different fonts at two sizes
should differ by.

⇒ **The reordering is a property of the extractor, not of what cairo produces.** The accurate
statement is *"Xpdf 4.00's layout analysis mishandles bbox-less Type3 `d0` glyphs"*, **not**
"the exported PDF is degraded". Nothing this project would emit is malformed.

**One of the two open items now has a candidate answer.** The macOS seat, measuring the real
`export::pdf` sink rather than a spike (2026-08-20): macOS does not omit `/ToUnicode` from a
text run — **there is no text run**. `pdfimages -list` shows the astral colour emoji rasterised
as its own **Image + SMask XObject pair** (58×58 rgb + gray smask), where Windows/DWrite wraps
the same glyph in a Type3 `d0` font ([P-14](#p-14-cairos-windows-colour-glyph-path-wraps-colour-glyphs-in-a-type3-d0-font-breaking-layout-reconstructing-extraction)).
An Image XObject carries no text-showing operator, so nothing is extractable by construction —
a different failure class from Windows', which is a reader-side reading-order bug over glyphs
that *are* in the text model. Full entry at
[P-17](#p-17-macos-embeds-a-colour-emoji-as-an-image-xobject-so-its-text-is-absent-by-construction).
MEASURED: the differing XObject shape. INFERRED: that it is the *cause* of the missing
`/ToUnicode`.

**Open, and neither blocks**: read cairo issue **#870, "Cairo 1.18.2 regression when writing
PDFs with fonts"** — on-point and unread, because gitlab.freedesktop.org blocks automated
fetches. **Do not**
hand-roll `/ActualText` spans: cairo already emits them and that emission is itself a
run-splitter — **and a second, stronger reason has since been measured: one of the spans
cairo emits is doing necessary work** ([P-18](#p-18-cairos-actualtext-span-is-what-makes-nfd-survive-the-round-trip-not-the-font-encoding)),
so a hand-rolled span would be competing with it rather than filling a gap.

**Caveat on the structural comparison, so a future spike is not over-read**: both files compared
were **probe artefacts, not the feature**. Any PDF built to represent what this project would
ship should be re-checked against those same three discriminators — a two-minute grep now that
they are named. **Discharged on both platforms** (2026-08-20). macOS: the re-check found the
embedding to be an Image XObject rather than a Type3 font at all — see
[P-17](#p-17-macos-embeds-a-colour-emoji-as-an-image-xobject-so-its-text-is-absent-by-construction).
Windows: all three discriminators re-checked against the shipped sink and **unchanged** —
`FontMatrix [1 0 0 -1 0 0]` including the `-1`, `d0` ×2 with `d1` ×0, and `/Image` DeviceRGB
8bpc with an `/SMask`. The Type3 route is still the Windows route; it has not drifted toward
the macOS shape. The y-flip audit was re-run too and still loses: three stacked flips plus a
fourth inside the CharProc, no Windows-only extra, `BT`/`ET` 1/1. The **one** difference from
the probe artefacts is `/ActualText`, and it is load-bearing — [P-18](#p-18-cairos-actualtext-span-is-what-makes-nfd-survive-the-round-trip-not-the-font-encoding).
### ✅ O10 — The default name and filename validation

The two platforms behave differently and **only one is dangerous**:

| seeded name | Windows | macOS |
|---|---|---|
| `report` | → `report.pdf` | → `report.pdf` |
| `report.pdf` | → `report.pdf` | → `report.pdf` |
| **`notes.md`** | **→ `notes.md`, UNCHANGED** | → `notes.pdf`, extension replaced |

Windows appends the filter's extension only when the name carries **no dot-suffix at all**,
and does not enforce the filter's type. So a default derived from the document's filename
comes back untouched and **the export writes PDF bytes over the user's Markdown source**.
macOS replaces the extension and is safe.

**Operator ruling**: guarantee the proposed name carries the target extension; a user who
deliberately gives a Markdown file a `.pdf` name takes that risk themselves. So
**stem-plus-extension is a guarantee, asserted on every platform** — not a convention.
Deriving from the filename would be silently fine on macOS and destructive on Windows, which
is the worst possible distribution: it survives review by anyone testing on a Mac.

**Validation direction**: validate the name proposed, never the name returned. The platform
agrees with `docio/rename.rs` on reserved device names and forbidden characters, and disagrees
on trailing dots and spaces. Gating the chooser's return would reject a name the platform
already accepted and rewrote. Beyond that, **once the chooser opens, naming the file is the
user's responsibility** — this application makes a best effort to prepopulate a name that does
not mislead, and neither duplicates nor second-guesses the platform's rules.

## Platform facts

Measured on all three seats: Linux (GTK **4.6.9**, cairo **1.16.0** — the supported floor),
macOS (4.22.4, Homebrew, Quartz, arm64), Windows (4.22.4, gvsbuild, MSVC, Win10 19045).
Symbols were read from the binaries the application links, never from version strings
(GTK4Rs/AP-272).

| Question | Answer |
|---|---|
| Export symbols present? | **Yes, all three.** 37 exported `gtk_print_operation_*` symbols on Windows and macOS independently; `cairo_pdf_surface_create` and `_create_for_stream` in both shipped cairos. `set_export_filename` is the **only** export sink. |
| Does export touch the print subsystem? | **No.** Measured by import interception on Windows: zero calls into `WINSPOOL.DRV`, with a positive control proving the hook was live. No dialog; synchronous return in 9–130 ms. A user with no printer, driver or spooler exports normally — a **standing requirement**, not just a finding. *(NOT-RUN: the no-printer condition itself on Windows, which lacks admin; macOS answered it under genuinely no printer.)* |
| Is PDF text searchable? | **Yes.** Byte-exact round-trip over ASCII, precomposed *and* combining accents, em/en dashes, curly quotes, ellipsis, CJK, arrows, typographic symbols, and box-drawing in light/heavy/double sets. **NFD survives uncomposed** — `U+0065 U+0301` does not silently become `U+00E9`. Fonts subset-embed rather than outline. Astral colour emoji are the sole failure, and only on ordering ([O9](#o9--emoji-in-the-pdf-export)). |
| Reachable from a test? | **Yes, no platform carve-out.** On Windows both a `#[gtk::test]` body and a plain `#[test]` with manual `gtk::init()` drive `run(Export)`; no display, window or main loop needed. GTK4Rs/AP-159's macOS abort does not bite there. The project's `harness=false` suite owns `main()` on the process main thread, which is strictly more favourable. *(One constraint: the two test shapes cannot share a binary — parked P-9.)* |
| Cost | **Linear, no knee, across page count, platform and a 40× range of content weight.** Windows ~2.5 ms/page dense and ~0.06 sparse; macOS ~0.86 ms/page, ~3× faster as expected on different hardware. One second at **~370 dense pages** or **~15,000 sparse**. **The shape is the design's; the constant is the content's** — any ceiling must be a shape assertion, never a page number. |
| Path shapes | Nothing differs from the existing save path. Ordinary drive-letter and `file:///` forms on Windows; no MSIX/package identity in the repo, so no per-package filesystem virtualisation. *(NOT-RUN: iCloud/OneDrive-backed folders; a real SMB server.)* |

## Rollout work items

Work this plan takes on because of decisions already made. Each is a deliberate scoped edit,
sized here so it is not discovered halfway through `pdf.rs`.

### R1 — Extract the atomic-publish scaffold

> **LANDED (Linux, 2026-08-19), and platform-neutral — the other seats inherit it and do
> not redo it.** `atomic_io::AtomicPublish` holds the sequence; `write_atomic` is its first
> caller and `stage_publish` its second. `write_atomic`'s twelve existing tests, including
> the one proving the guard is wired in rather than merely correct, pass unchanged.

**Why**: [O8](#o8--how-the-pdf-sink-reaches-disk) says reuse the Markdown save path, and
`atomic_io::write_atomic` **cannot be called** for an export — it owns create-temp → write →
rename, and under P1 the *content* step belongs to GTK.

**What it already does**, all of which export needs and none of which it should reimplement:
canonicalize through a symlink to the real target; probe the target's mode and owner; create
a **unique, private (0600)** temp beside the target, retrying on collision; arm a
`TempFileGuard` so every early return removes the temp; preserve owner, then relax mode;
`sync_all`; **close before renaming** (Windows refuses to rename a file still open without
`FILE_SHARE_DELETE`); rename; disarm; `fsync` the parent directory.

**The shape**: keep that sequence in one place and make the content step vary — a scaffold
that hands out the temp path, then publishes it. `write_atomic` becomes its first caller, the
exporter its second.

**Two details that must not be lost**: cairo opens the temp path itself, so the scaffold must
**create the file privately first** and let cairo write into the existing file (a truncating
open preserves the mode) — handing over only a *name* silently loses the
private-from-first-byte guarantee. And GTK closes its own surface at the end of `run(Export)`,
so the publish happens only after the run returns.

**Sizing**: `src/atomic_io.rs` carries six numbered QA fixes and a lot of mode/ownership care.
The refactor must not change `write_atomic`'s observable behaviour; its existing tests —
including the one proving the guard is *wired in* rather than merely correct — are the gate.

### R2 — Consolidate the raw-HTML element allowlist

> **LANDED (Linux, 2026-08-19), and platform-neutral — the other seats inherit it and do
> not redo it.** `renderer::RawHtmlElement` + `RENDERED_HTML_ELEMENTS` + `recognise_html_element`
> in `renderer/picture.rs`, with the tag-name-boundary rule attached to the set because that
> rule is part of the set's meaning. The scanner's ten existing tests pass unchanged; three
> new ones pin the set itself and the boundary rule.

**Why**: decided by the operator as part of this plan's rollout. There is no allowlist — the
permitted set is control flow, four hardcoded branches in `scan_image_tags`
(`src/renderer/picture.rs`), with no constant a second consumer can refer to. Export forces
the issue: it must reproduce the set exactly and cannot refer to one that does not exist, so
left alone it grows a second copy that drifts — the failure H2 was chosen to prevent,
arriving through a side door.

**The shape**: one named collection with one owner, consumed by the scanner and the export
path alike. The tag-name-boundary matching rule travels with it, since that rule is part of
the set's meaning. TDD 2.23 — explicit that `<script>`, `<iframe>` and `<div>` are dropped
entirely rather than shown as literal text — is the gate for the refactor being
behaviour-preserving.

**And the CAM**: check at kickoff that raw HTML is *in* the Document Rendering CAM's construct
list, so the export cell actually covers `<picture>` et al.

## TDD rubrics (draft, not landed)

A new §25 "Exporting a document", for the operator to author at kickoff.

- **25.1** Export HTML from a rendered document → the file exists and opens
- **25.2** Export from a **never-rendered (deferred) tab** → identical output
- **25.3** Every construct in the Document Rendering CAM's contexts (body, table cell,
  blockquote, list) appears in the export as the preview shows it
- **25.4** Raw HTML is dropped exactly as in the preview (`<script>` absent; `<picture>`
  fallback honoured) — depends on [O5](#o5--raw-html-in-the-source)'s allowlist note
- **25.5** An unsaved buffer exports the **buffer**, not the file on disk
- **25.6** Cancelling the destination chooser is a clean no-op (no file, no notice)
- **25.7** A failed write (read-only directory, full disk) reports and leaves no partial file
- **25.8** Closing the tab or window while the write is out does not lose or corrupt the
  export, and does not resurrect the tab
- **25.9** Export uses the active reading theme's values, with no literal in either sink
- **25.9a** *(phase 2)* Emoji in an exported PDF are **selectable and copy-paste correctly**
  in a mainstream reader — the acceptance criterion [O9](#o9--emoji-in-the-pdf-export) was
  actually closed on, and a different path from command-line extraction. Hand-verified rather
  than automated unless a headless equivalent proves cheap
- **25.10** *(phase 2)* A page break never splits a line of text
- **25.11** *(phase 2)* **The positive claim, universal**: every non-emoji category
  round-trips **byte-exact, per line, over the whole line**, on both platforms. No platform
  carve-out and no caveat in the predicate. Method constraints are not predicates and live in
  parked P-1 and P-13: extract with `pdftotext -enc UTF-8 -raw` (never `-layout`, which is
  the layout-reconstructing mode 25.18b forbids asserting against), never gate on font
  metadata, and record which build produced the measurement
- **25.11a** *(phase 2)* The extraction assertion is **paired with a check of the rendered
  page**; a round-trip failure is never reported as a rendering defect. The two genuinely
  disagree, so an extraction-only suite would condemn the surface that works
- **25.11b** *(phase 2)* The emoji limit, asserted **per platform as measured behaviour**
  rather than as an aspiration, since neither behaviour is one this project can fix — its
  purpose is to catch a **change**, not to demand a fix. The emoji used must be **above the
  BMP**: U+2705 round-trips cleanly on Windows, so a rubric written against a BMP emoji
  passes while the limit it documents is unmeasured. **Do not assert against a
  layout-reconstructing extractor's output** — [O9](#o9--emoji-in-the-pdf-export) accepts
  that failure explicitly, so a rubric encoding it would fail on behaviour this project has
  ruled in scope-free
- **25.12** Footprint unchanged by an export (TDD §6 gate)
- **25.13** *(phase 2)* Success is asserted from `run()`'s return value **and** pages drawn —
  **never** from `is_finished()` or `status()`, which are inverted (parked P-4)
- **25.14** *(phase 2)* Export cost asserts the **shape** — per-page cost does not grow with
  document length — never a wall-clock number a slower machine or a denser page would fail
- **25.15** *(phase 2)* Export reports progress and can be cancelled
- **25.16** *(phase 2)* **An export cancelled *or failed* part-way leaves the destination
  byte-identical to what it was before**, including when the destination is an existing PDF.
  It asserts the **previous file's bytes**, because the partial left behind is itself a valid,
  cleanly-extracting PDF. It is an **outcome** assertion, so it is correct against every
  candidate route. Two requirements, both biting *because* the chosen route makes it easy to
  pass: it must record **why it is green** — held structurally, not by a guard — so a mutation
  sweep does not delete it as vacuous; and it must **seed a real previous PDF and drive a real
  cancel part-way through a real render**, or it is a tautology wearing a rubric's clothes
- **25.17** *(phase 2, Windows)* Exporting over a destination another process holds open
  reports a **named** failure telling the user to close the file, not a generic write error.
  On POSIX the same export succeeds

## Risks

- **Two renderings of one document drift.** Structural mitigation: both sinks consume one
  `ExportDoc`, and the Document Rendering CAM gains an export cell so a new construct cannot
  land without one. Vigilance is not a mitigation.
- **Scope creep into a layout engine.** PDF table pagination is where a well-scoped feature
  becomes open-ended; [O7](#o7--a-table-too-wide-for-the-page-phase-2) bounds it to
  scale-to-fit, decided before phase 2 starts.
- **Platform inequality of *capability*** — absent-with-reason, never silently broken
  (ScrAP-292). **Not materialised**: all three seats measured the feature alive.
- **Platform inequality of *behaviour*** — a distinct shape the rule above does not cover. The
  feature exists everywhere and *behaves* differently: the publish fails on Windows when the
  destination is held open; the chooser may silently not open where it was asked; emoji cost
  differs by platform. None justifies removing the command. **Eliminating a divergence is the
  priority; naming it is the floor, not the goal.** Where one cannot be removed it gets a named
  platform-specific message or a documented limit, and any rubric encoding one platform's
  behaviour is marked as such.
- **A synchronous main-thread render** — GTK4Rs/AP-148's shape, and the plan's largest
  unknown. **Measured and licensed**: linear across page count, platform and content weight,
  with no knee through 2,000 pages. Assert the shape, not a number.
- **A promote blocked by an open destination** — Windows-only, destination-dependent, and in
  scope only as far as a named error. Platform variance in file-writing behaviour is accepted
  and beyond this application's scope to normalise.

## Parked findings

**Held here deliberately.** While this plan is unapproved, findings that would normally be
filed in `sdd/ANTI-PATTERNS.md` — or surfaced to a reusable skill — stay in it. Filing them
now would assert a permanence the plan has not earned: if export is never built, an
anti-pattern entry about its write path is a lesson about code that does not exist.

**No ID has been allocated for any of these, deliberately** — an ID is frozen once written
and citable from code. The Linux seat allocates them at distribution time (POLICY's
single-writer rule), which is also why entries from other seats arrive as content rather
than edits. **On implementation each entry is *moved*, not copied, to the home named in its
Destination line, and this section is deleted with the rest of the plan.**

### P-1. A font's Unicode flag does not predict whether its text extracts

**Destination**: `sdd/ANTI-PATTERNS.md`, false-PASS family (GTK4Rs/AP-168's shape).

**Symptom**: a PDF-text-extractability check gated on font metadata passes while the feature
is broken. **Root cause**: `pdffonts` reports `uni=yes` for *every* font in the file —
including Type 3 fallbacks whose text extracts as nothing — so the column reads identically
for text that round-trips and text that does not. **Corrective**: assert a byte-exact
round-trip of the string through `pdftotext`; never inspect font metadata.
**Measured**: macOS, GTK 4.22.4/Quartz, seven text categories.

### P-2. A save chooser returns a foreign extension unchanged, and the export overwrites the source

**Destination**: `sdd/ANTI-PATTERNS.md` or the GTK4/Windows skill — decide at distribution.

**Symptom**: a GTK4 native save chooser on Windows returns the seeded name **unchanged**
whenever it already carries any dot-suffix, so a default derived from the source document's
filename round-trips intact and the writer overwrites the source. **Root cause**: the chooser
appends the filter's extension only when the name has none, and does not enforce the selected
filter's type. **Corrective**: derive an export default as `file_stem` + the target
extension; never pass the document's filename through.

**What makes this worth an entry is that macOS is safe** — the same seed comes back with the
extension *replaced*. So the dangerous derivation is silently correct there and destructive on
Windows: the worst possible distribution, because it survives review by anyone testing on a
Mac. Any entry written from this must say so, or "derive from the stem" reads as style advice.

**Measured**: Windows, GTK 4.22.4 gvsbuild / Win10 19045, `FileChooserNative` Save with a
single `*.pdf` filter; macOS, same seeds on the real `NSSavePanel`.

### P-3. A silent instrument returns a confident wrong answer that looks like a finding

**Destination**: the reusable testing/measurement home — about *instruments*, not this
project. Written as a **family**: six independent instances surfaced in a single
investigation, and the shared shape is worth more than any one of them.

**The shape**: a measuring instrument fails in a way that produces well-formed output
indistinguishable from a real result. The subject is then reported as defective, or clean, on
the strength of a reading never taken.

**Six measured instances:**

1. A driven save-chooser probe reported "no chooser window" for all eight cases — which reads
   exactly like GTK failing to show a native chooser. Cause: the harness's `GetWindowTextW`
   P/Invoke marshalled ANSI, truncating every title to one character. *Tell*: window
   **enumeration** found the right **number** of windows.
2. A dependents listing filtered on `\.dll` hid `winspool.drv`, and would have supported the
   conclusion that the shipped GTK has no Windows print backend. It does.
3. An import-table walk read data directory entry 0 (exports) instead of entry 1 (imports).
   It faulted loudly; had it merely found no descriptor it would have produced the *same*
   wrong conclusion as instance 2 by an independent route.
4. A launcher silently split its own arguments, nearly producing a headline finding —
   *"`set_current_folder` fails whenever the path contains a space"* — reproduced four times,
   alternating space/no-space so that last-used-folder could not explain it. False:
   PowerShell's `Start-Process -ArgumentList` quotes nothing, so every path with a space
   arrived pre-split and the probe read only the fragment before it. **Every layer was
   honest; the input was wrong.** Note what the rigour did: controlling for the confound
   already thought of made the wrong answer *more* convincing.
5. A UI-automation call reported success and did nothing. System Events' `set frontmost of
   process` **silently no-ops on an unbundled binary**, so keystrokes went to the driving
   terminal. Second limb: `Cmd+A` does not reliably select-all in that `NSTextField`, so a
   "clear the field" step left `.pdf` behind and raised a dot-file alert briefly mistaken for
   the result being measured. *Tell for both*: the observed state never changed in the way
   the command claimed.
6. **A buggy probe produced the exact symptom under investigation** — emoji extracting before
   the ASCII, one message from being reported as a reproduction. The loop never advanced the
   pen, so cairo emitted large *positive* `TJ` offsets, which move the pen backwards; the
   emoji were physically drawn to the left and the extraction was **correct about a layout
   that was genuinely broken**. *Tell*: an ink-position check on the rendered page. **The most
   dangerous instance**, because the instrument did not fail silently and did not produce an
   implausible result — it produced *the expected answer*.

**Correctives**, in order of power:

- **A negative result needs a positive control.** Before trusting "zero calls observed", make
  a call you know should be observed and confirm the instrument sees it. This includes a
  symbol search: a grep that finds nothing proves nothing until it has found something.
- **Every gate must read the status of the thing it claims to be gating, and a green that
  could also mean "never ran" is not a green.** Three instances, three authors: a harness
  grepping output for `FAILED` over a run that died before emitting one; a shell reading `$?`
  after a pipeline, reporting `tail`'s status rather than `cargo`'s; and a draft rubric
  asserting a font's `uni` flag — metadata *about* the text rather than the text.
- **Echo the parameter back from inside the process under test**, never from the driver.
- **When every case in a matrix fails identically, suspect the observation layer** before the
  subject, and look for a signal that crosses the suspected-broken layer intact.
- **Controlling for the confound you thought of says nothing about the one you did not.**
- **Check a second layer before reporting a reproduction.** When a probe reproduces the
  symptom you were hunting, that is the moment to add an independent check, not to send the
  message.

### P-4. Both of `GtkPrintOperation`'s completion signals can misreport

**Destination**: the GTK4/Rust home — a `GtkPrintOperation` contract property. Drives rubrics
25.13 and 25.16.

**Symptom, Export route**: `status()` and `is_finished()` are **inverted, wrong in both
directions**. On a run that **succeeded** they read `GENERATING_DATA` / **false** —
permanently, surviving a main context pumped to exhaustion plus a 500 ms wait. On every run
that **failed or was cancelled** they read `FinishedAborted` / **true**. So `is_finished() ==
true` means *aborted*, and success never reports finished. A gate on `status() == Finished`
reports failure on success; a gate on `is_finished()` waits forever on success.

**Symptom, Preview route**: an application that stops its own render loop **without** calling
`op.cancel()` gets `Ok(Apply)` *and* plain `Finished` for a render that did 2 of 5 pages.

**The distinction that makes it actionable**: if the application calls `op.cancel()`, GTK's
reporting *is* honest (`Ok(Cancel)`, `FinishedAborted`) even though the app-owned loop keeps
running — cancel is advisory there.

**Corrective**, both halves: track pages rendered against pages expected as the application's
own state, **and** route every early stop through `op.cancel()` so the reported outcome is not
a lie. The second half is free, and skipping it produces a silently-wrong success.

**Measured** on all three seats.

### P-5. A cancelled export destroys the destination, and the wreckage looks fine

**Destination**: `sdd/ANTI-PATTERNS.md`, GTK4Rs/AP-167's family — but a **distinct** entry:
GTK4Rs/AP-167's trigger is a *failure*, this one's is ordinary user intent.

**Symptom**: cancelling a `GtkPrintOperation` export leaves a valid, readable, **partial** PDF
at the destination, having already replaced whatever was there. It extracts cleanly and
`pdftotext` exits 0, so nothing signals that the previous file is gone. **Root cause**:
`set_export_filename` gives cairo the destination directly; there is no temp-and-rename, and
cancel is a normal-completion path rather than an error path. **Worse than it first appears**:
GTK opens — and truncates — the destination *before the first page is drawn*, so the
destruction happens as the operation's first act.

**Corrective**: never point an export sink at a user-visible destination; render to a temp
file or memory and publish atomically on success only.

**Measured**: Windows, GTK 4.22.4 gvsbuild / Win10 19045 — a 43,973-byte destination replaced
by a 171,327-byte 51-page partial that `pdftotext` reads without error.

### P-6. An open handle on the destination blocks an atomic promote — on Windows only

**Destination**: the reusable cross-platform home — a property of `MoveFileExW` versus
`rename(2)`, not of this project.

**Symptom**: a temp-then-rename publish fails with `PermissionDenied` (os error 5) on Windows
whenever any process holds the destination open without `FILE_SHARE_DELETE`; the identical
operation succeeds on POSIX. **Root cause**: `MoveFileExW` cannot replace a destination whose
existing handles did not grant delete sharing; `rename(2)` has no equivalent constraint and
leaves the holder reading the old inode. **Corrective**: expect and *name* this failure at any
user-chosen destination — a generic "write failed" misdescribes both the cause and the remedy,
which is that the user closes the file.

**And it is destination-dependent, not a blanket Windows rule**: on a VMware HGFS share all
five share modes succeeded, where on local NTFS `0x0`/`0x1`/`0x3` all failed. **A rubric
written against one destination is wrong for the other; test against local disk**, the
stricter case.

**Measured**: Windows (Win10 19045, `std::fs::rename`), Linux (kernel 6.8, ext4 and tmpfs) and
macOS (arm64, APFS).

### P-7. The chooser's initial folder is best-effort, and its failure is unobservable

**Destination**: `sdd/ANTI-PATTERNS.md` or the GTK4/Windows skill.

**Symptom**: `GtkFileChooserNative` on Windows silently ignores `set_current_folder` for a
folder the shell cannot resolve to an `IShellItem`, opening at the shell's last-used folder
instead. **Root cause, and the part that matters**: the setter returns `Ok(())` regardless,
`current_folder()` reads back `None` regardless — *including* where the folder demonstrably
was honoured — and the only failure signal is a `Gtk-WARNING` on stderr. There is no
combination of the two that lets an application detect the outcome. GTK4Rs/AP-275's shape at a
different API: **an API that cannot report failure is not an API that does not fail.**

**Corrective**: treat the initial folder as best-effort; never assume the dialog opened where
you asked; never build the default filename or an overwrite check on that assumption.

**Measured**: Windows, GTK 4.22.4 gvsbuild — honoured for local paths with *and without*
spaces, not honoured for a VMware HGFS UNC path, honoured for the same share as a mapped
drive.

### P-8. An extraction failure is not evidence about appearance

**Destination**: `sdd/ANTI-PATTERNS.md`, filed against GTK4Rs/AP-168's family as its
**inversion** — the entry should say so. GTK4Rs/AP-168's lesson is that a suite is evidence only
about the surfaces its assertions name; here the assertion is aimed at a surface that genuinely
*is* broken, and would have condemned an adjacent one that is not.

**Symptom**: colour emoji in a cairo-generated PDF render correctly — glyphs, colours,
positions, left-to-right order — but extract badly. A text-extraction assertion therefore
reports "export is broken" about an artefact that is, to every user who looks at it, correct.
**Root cause**: the glyph run's extraction metadata, not the drawing operators. **Corrective**:
assert PDF text round-trips **per line**, and never treat an extraction failure as evidence
about appearance without looking at the rendered page.

**Measured**: Windows, cairo 1.18.4, Segoe UI Emoji at 14 pt *and* 36 pt (so not a size or
crowding artefact); visual confirmed against a cairo `ImageSurface` raster control with the
capture gated on the target being the foreground window.

### P-9. A `#[gtk::test]` body and a plain `#[test]` calling `gtk::init()` cannot share a binary

**Destination**: the GTK4/Rust testing home — a `gtk4-rs` harness property. GTK4Rs/AP-71's
family.

**Symptom**: a test binary containing *both* shapes panics with "Attempted to initialize GTK
from two different threads" (`gtk4-0.10.3` `rt.rs:136`). **Root cause**: the attribute macro's
body runs on a glib `ThreadPool` worker and the plain test on its libtest thread; GTK permits
only one. **Corrective**: pick one shape per test binary.

**Why this earns an entry**: the failure appears in the *plain* test, which invites the reading
"a plain `#[test]` cannot init GTK on Windows". That is false — isolated, it passes cleanly.
The failure was **contamination between the two shapes**, and the misreading would have written
a non-existent platform restriction into this plan.

**Measured**: Windows, gtk4 0.10.3 / GTK 4.22.4 / Win10 19045.

### P-10. On the preview route, `render_page` ends the page — do not also call `show_page`

**Destination**: the GTK4/Rust home — a `GtkPrintOperationPreview` contract detail.

**Symptom**: driving `render_page` *and* calling cairo's `show_page` emits a blank page after
every rendered one — 3 pages became 6. **Root cause**: `render_page` ends the page itself; the
two are not complementary.

**Corrective**, four parts, each a one-liner once known:

1. Never call `show_page` on this route.
2. **Size the surface from `::got-page-size` per page** rather than assuming a paper size.
   Hardcoding A4 against a Letter setup produced *correct layout inside a wrong media box* — a
   silent failure that looks right until someone prints it.
3. **Treat `op.cancel()` as advisory**: the application owns the render loop, so cancel does
   not break it.
4. **Do not infer completion from the return value.** `run(Preview)` reports **whether
   `op.cancel()` was called**, not whether the loop completed: stop early without calling it →
   `Ok(Apply)`; call it → `Ok(Cancel)`; neither says how many pages were rendered.

**Measured**: Windows (GTK 4.22.4 gvsbuild) and Linux (GTK **4.6.9**), so these are contract
properties across a wide version range rather than one build's quirks.

### P-11. A measurement that agrees with a satisfying hypothesis stops being designed against

**Destination**: the reusable testing/measurement home, as P-3's companion. P-3 is about
instruments that fail *silently*; this is an instrument that works, produces a real number, and
still yields a false conclusion.

**Symptom**: a probe measured *"`run(Export)` iterates the main context exactly once per page"*
across page counts of 100/400/1000/2000, ticks equalling page count every time. The claim was
false and reached this plan as measured. **Root cause**: the counting source was a **1 ms
timeout**, which can fire at most once per millisecond; the heavy test content took
2.6–2.75 ms per page, so every page exceeded the source's period and the counts coincided. Make
the pages cheap and the coincidence evaporates — 200 light pages produced 28 ticks, not 200.

**The discriminator that failed, and why it looked airtight**: comparing ticks against elapsed
milliseconds across different *page counts* rules out `ticks == elapsed_ms`. It does **not**
rule out `ticks == min(iterations, elapsed_ms)` — what a rate-limited source actually measures,
and what the numbers were consistent with all along. **Varying page count cannot separate those
hypotheses; varying content weight at fixed page count can.**

**The framing that generalises**, from the seat that made the error: the instrument was
**adequate for the question it was built to answer and inadequate for the claim later made from
the same data**. It was built to answer "is the main loop pumped at all?" — which it answers
correctly, and that answer has survived every re-measurement. Then a *cadence* was read off the
same numbers, a strictly stronger claim, and the design was never re-examined. Nothing about the
probe changed between the sound conclusion and the unsound one; only the strength of what it was
asked to carry.

So the rule is **not** "check your instrument" — it had been checked, for the original question.
It is: **when a measurement tempts you into a stronger claim than the one you designed it for,
that is a new experiment and it needs a new design.** The claim crept; the instrument did not
move. *The tell*: the measurement agreed with a hypothesis its author found satisfying, so the
designing-against stopped there.

**And when the right instrument breaks the subject, say so and stop.** The unrate-limited idle
source that would have measured the true cadence livelocks the export; the honest result is
NOT-RUN with the reason, not a weaker instrument's number reported as if it answered.

### P-12. A save panel's displayed name is not the name you will get

**Destination**: the reusable UI-testing home — **not** a macOS one. See the
cross-platform note below.

**Symptom**: clearing a macOS save panel's field and typing a bare name shows exactly that name
with **no extension appended on screen** — yet the accepted `GFile` comes back with the target
extension applied (`bare` displayed, `bare.pdf` returned). The extension is applied at
**accept** time from the format popup, not at edit time — differing from the same panel's
*pre-fill* behaviour, where a seeded name shows its extension immediately.

**Why it matters beyond trivia**: it invites a specific false measurement. Screenshotting the
field and reading it is a reasonable way to test a chooser, and that method reports "no
extension appended" for a case where one is.

**Corrective**: assert on the returned `GFile`'s path and nothing else.

**REPRODUCED ON WINDOWS 2026-08-20 — this is not a macOS lesson.** The Windows seat
drove the real Win32 chooser on the shipped `export::pdf` path with `notes.md` open:
the File name field showed **`notes`**, with the extension carried by the "Save as
type" filter and applied at **accept**, and the file that landed was `notes.pdf`.
Same property, same corrective, a second toolkit backend. So the entry is about
`GtkFileChooserNative`'s accept-time extension application generally, and any entry
written from it must not be filed as platform-specific — a reader who takes it for a
Quartz quirk will screenshot a Win32 field and draw the same wrong conclusion.

**Measured**: macOS, `GtkFileChooserNative` over the real `NSSavePanel`; and Windows
10 19045 / GTK 4.22.4 gvsbuild over the real Win32 save dialog.

### P-13. Two extractor-output encoding traps — the second manufactured a long-lived false finding

**Destination**: the reusable testing/measurement home. Neither is a defect in the produced
PDF: `/ToUnicode` is correct, `/ActualText` is 0, and every codepoint is recoverable.

**Trap 1 — the Latin-1 default: emoji vanish entirely.** An extraction run without
`-enc UTF-8` reports no emoji at all — zero emoji bytes, zero U+FFFD, zero `?`. Silent total
loss. **Build-scoped**: measured true of **Xpdf 4.00 (Glyph & Cog)**, measured **false** of
**poppler 26.08**, whose default output encoding is UTF-8. A property of a *build*, not of the
command name. **And it is not diagnostic of anything** — reproduced on a file with no emoji and
no Type3, where the characters silently dropped were U+2192 and U+2713, monochrome glyphs from
an ordinary CID font.

**Trap 2 — `-enc UTF-8` emits CESU-8, and this is what produced the U+FFFD.** With the flag
set, astral characters come out as **two encoded surrogate halves** rather than one 4-byte
sequence:

```
eda0bd edb982   eda0bd edba80   e29c85   28 636f6c6f7572 ...
   U+D83D U+DE42     U+D83D U+DE80    U+2705    "(colour..."
```

`ED A0 BD` is the high surrogate U+D83D as a 3-byte sequence — **CESU-8, not UTF-8**. A strict
UTF-8 decode fails; a *replacing* reader manufactures U+FFFD; repaired through a UTF-16
round-trip the line is exactly `🙂🚀✅(colour emoji)` — **nothing lost**.

> **The U+FFFD were never in the PDF and never in the extractor's output.** They are
> manufactured by whatever decodes it, and the count varies with the decoder's replacement
> policy — which is why one investigation recorded "four per emoji", another 12 per line, and
> another 2, for a thing that did not exist.

**The internal control**: ✅ is U+2705, **BMP**, so it encodes as valid 3-byte UTF-8 and
survives every reader — which is exactly why it pasted correctly throughout while 🙂 and 🚀
turned to mojibake. Same file, same line, same extractor: **BMP survives, astral does not. The
split is the encoder, not the PDF.**

**Correctives**: pass `-enc UTF-8` explicitly and **record which build produced each
measurement**; then decode the result as CESU-8, or round-trip it through UTF-16, before
concluding anything about characters. **Never read a U+FFFD in a terminal as evidence about a
file — it is a statement about the decoder between you and the bytes.**

### P-14. cairo's Windows colour-glyph path wraps colour glyphs in a Type3 `d0` font, breaking layout-reconstructing extraction

**Destination**: the GTK4/Rust or cross-platform rendering home — an upstream cairo limitation,
not a defect in this project's code.

**Symptom**: text exported to PDF renders perfectly and searches correctly in mainstream
viewers, but `pdftotext`-family **layout modes** emit the line out of order — colour emoji
hoisted ahead of the label preceding them, adjacent lines merged. **No text is lost; only order
changes.**

**Root cause**: on Windows, pangocairo's win32 font map reaches cairo's **DWrite** colour-glyph
path, which rasterises each colour glyph and embeds it as a **Type3 font whose CharProcs are
`d0`** operators painting `/Subtype /Image` XObjects. **`d0` declares a coloured glyph and,
unlike `d1`, carries no glyph bounding box.** The extractor's reading-order analysis is left
with no per-glyph extent — only an odd `/FontBBox` against `/FontMatrix [1 0 0 -1 0 0]`, nested
under a further y-flipped `Tm` and `cm` — and assigns the glyphs to the wrong row. *(Row
misassignment: INFERRED. Everything else: MEASURED.)*

**Controlled comparison inside one file**: U+2705 (colour) goes to the Type3 font and is
misplaced; U+2192 and U+2713 (monochrome) go to an ordinary Type0 **on the same line** and are
placed correctly. **Colour versus monochrome, not emoji versus not.**

**But the structure is necessary, not sufficient**: poppler 22.02 orders the identical
Type3/`d0` structure correctly in **every** mode. So the accurate wording is *"Xpdf 4.00's
layout analysis mishandles bbox-less Type3 `d0` glyphs"* — **a reader-side weakness, not a
malformed PDF.**

**What was tried and refuted**, each a plausible headline: *a corrupt text layer* — refuted
three ways (`-raw`, an independent stdlib content-stream walker, and PDFium find-in-page all
read it correctly; `BT`/`ET` is 1/1, `/ToUnicode` correct, `/ActualText` 0). *The cairo version*
— refuted: the failing box runs 1.18.4, the same version measured order-preserving elsewhere.
*A PDF-1.4 restriction disabling ActualText* — refuted: the header is `%PDF-1.7` and no such
call exists in this project's tree.

**Corrective — and the obvious workaround is deliberately NOT recommended.**
`PANGOCAIRO_BACKEND=fc` produces correct extraction order **at the cost of colour emoji
everywhere, screen included** (paired raster controls: win32 colour, fc monochrome outlines),
and monochrome is forbidden by operator ruling. **Trading colour emoji away to satisfy one old
extractor is a bad deal** and must not survive in a register as advice. If a future rubric
demands both, it needs cairo emitting `d1` with a bbox, or `/ActualText` spans around Type3
runs.

**Citations**: cairo 1.18.4 (gvsbuild), GTK 4.22.4, Win10 19045, Segoe UI Emoji 2019 build
(COLR + CPAL only). **Not the 4.6 floor** — that seat can falsify a 4.6 claim, never confirm
one.

### P-15. One extractor's output is not evidence about a file — run its no-heuristic mode as the control

**Destination**: the reusable testing/measurement home, beside P-3 and P-13.

**Symptom**: an extraction that looks scrambled is recorded as *"the exported document is
degraded"*, and a plan is built on it. **This happened**: the finding stood for a day and drove
a rejection before it was checked.

**Root cause**: layout-reconstructing modes (`-layout`, `-simple`, `-table`, and the default)
apply a reading-order heuristic that can fail on structurally valid files, and the failure is
**indistinguishable from a corrupt text layer** unless the heuristic is taken out of the loop.
Measured: cairo emits the whole line as **one** `BT`/`ET`, correctly ordered, and the default
mode still breaks it apart — splitting on a **Type3 font transition** rather than any structural
boundary.

**Corrective**: before attributing a reordering to the producer, run **`pdftotext -raw`**
(content-stream order, no layout analysis) as a free control, and confirm with **one extractor
of a different lineage**. If `-raw` is correct, the file is sound and the defect belongs to the
reader. **Three-seat agreement on a *default* mode is not independent evidence — it is the same
heuristic three times.**

### P-16. Most of what one investigation first called defects turned out to be measurement artefacts

**Destination**: the reusable testing/measurement home, as the capstone of the P-3 / P-11 /
P-13 / P-15 family.

**The claim**: over one investigation into a single symptom — colour emoji in an exported PDF —
each of the following was recorded as a finding and later measured to be wrong.

| premise | what it actually was |
|---|---|
| "Emoji extract as U+FFFD runs" | **console rendering of surrogate pairs.** No U+FFFD in the file, in any mode |
| "…and the Latin-1 default is what produced them" | also wrong — it is **CESU-8** output that strict decoders replace |
| "Every extraction result here is poppler" | one seat was **Xpdf 4.00**; nobody had checked |
| "`pdftotext` defaults to Latin-1" | **build-scoped**, and the characters it dropped were monochrome arrows, not emoji |
| "The newer cairo broke it" | cairo 1.18.4 is **strictly better** than 1.16, and the failing seat runs 1.18.4 |
| "COLR stays layered vector in the PDF" | cairo **rasterises** it; a bitmap lands in the PDF on every platform |
| "The seat is on the DWrite path" *(source read)* | conceded as GTK4Rs/AP-272 — then the **refutation** was itself an uncontrolled negative grep for a symbol name that does not exist |
| "macOS is the good platform" | macOS loses the emoji text **entirely**; Windows keeps correct `/ToUnicode` |
| "The exported PDF is degraded" | **five extractors correct, one wrong** — one 2017 extractor's layout mode |
| A probe reproduced the exact target symptom | the probe never advanced the pen; extraction was *correct* about a broken layout |
| "This test is latently broken on every platform" | generalised from one platform; three clean runs elsewhere refuted it |

**One is a different class from the rest.** Two seats disagreed about whether `origin` was a
bare repository or a working tree — and **both were correct about their own repository**. The
remote relationship was one-way by construction, so `origin/master` named a different branch on
each machine and one seat's ahead/behind measurement was not reproducible by *any* command on
the other. **No amount of re-measuring would have caught it**, because neither measurement was
wrong. Only saying *which repository each party meant* resolved it. A **shared-vocabulary
failure**, not a measurement artefact — the one item better instruments could not have
prevented.

**The lesson is not "be more careful."** Each was produced by a competent seat applying a
reasonable method, and most were caught by *another* seat rather than the one that made them.
What caught them, every time, was one of: **a positive control**, **a second independent
instrument**, **a check at a different layer** (rendered ink rather than extracted text; the
shipped binary rather than the source; PDFKit rather than the extractor; the dependency version
rather than the library), or **someone else's falsifiable prediction**.

**Transferable rules:**

- **A negative result needs a positive control** — including a symbol search.
- **Every gate must read the status of the thing it claims to be gating**; a green that could
  also mean "never ran" is not a green.
- **Name the artefact you are measuring, not the platform you are on.** Half the corrections
  above are labels that outran their evidence.
- **Cross-seat agreement is not independence** if the seats ran the same tool in the same mode.
- **When a result confirms what you hoped, add a discriminator** rather than reporting it.
- **Report a non-reproduction as a non-reproduction**, and retract a tidy characterisation
  before it hardens in a register.

### P-17. macOS embeds a colour emoji as an Image XObject, so its text is absent by construction

**Destination**: `sdd/ANTI-PATTERNS.md`, alongside [P-14](#p-14-cairos-windows-colour-glyph-path-wraps-colour-glyphs-in-a-type3-d0-font-breaking-layout-reconstructing-extraction)
— the two are one lesson in two mechanisms and should land as a pair, or the reader takes
either for the general case.

**Authored by the macOS seat**; allocated and formatted by the Linux seat under POLICY's
single-writer rule. Its reasoning and its MEASURED/INFERRED labelling are unchanged.

**Symptom**: an astral colour emoji that **renders correctly** on the exported page extracts as
if it were never there — no CESU-8 surrogate pair, no U+FFFD, no `/ToUnicode` entry, on
macOS/Quartz. Neighbouring prose on the same line extracts correctly, so the file is not
broken and the extractor is not failing.

**Root cause (MEASURED, against the real `export::pdf` sink rather than a spike)**:
`pdfimages -list` on the artefact shows the glyph rasterised as its own **Image + SMask XObject
pair** (58×58 rgb + gray smask). Windows/DWrite wraps the same glyph in a **Type3 `d0` font**
(P-14). An Image XObject carries no text-showing operator at all, so there is nothing for a text
extractor to find — **by construction**, not by a reader-side heuristic failing. That is a
different failure class from P-14's, which is a reading-order bug over glyphs that genuinely
are in the text model.

**INFERRED**: that this is *why* macOS emits no `/ToUnicode` — cairo/Quartz took the
image-XObject path instead of the Type3 one, so there is no text run to attach a `/ToUnicode`
to. MEASURED is only that the XObject shape differs between the two platforms.

**Corrective**: do not describe the macOS emoji limit as "the `/ToUnicode` is missing" — that
phrasing implies a text run with a defective map, and invites a fix (hand-rolled `/ActualText`,
a font substitution) aimed at a structure that is not present. State it as *the glyph is not
text on this platform*. And do not generalise either platform's mechanism to "how cairo embeds
colour glyphs": two backends, two mechanisms, one shared outcome only at the level of
searchability.

**Why it belongs in a register rather than a footnote**: it discharges the caveat
[O9](#o9--emoji-in-the-pdf-export) attached to its own structural comparison — that both files
compared were probe artefacts rather than the feature — and it is the first measurement of the
macOS mechanism taken through the shipped sink.

**Measured**: macOS, GTK 4.22.4/Quartz, arm64, poppler 26.08.0, U+1F680 through
`export::pdf`'s `GtkPrintOperation` Export path.

### P-18. cairo's `/ActualText` span is what makes NFD survive the round trip, not the font encoding

**Destination**: the reusable testing/measurement home, beside
[P-13](#p-13-two-extractor-output-encoding-traps--the-second-manufactured-a-long-lived-false-finding)
— it is about what an extractor is really reading, not about this project's code.

**Authored by the Windows seat**; allocated and formatted by the Linux seat under POLICY's
single-writer rule, reasoning and labels unchanged.

**The finding**: an exported PDF containing a **decomposed** accent (`e` + U+0301) draws the
**precomposed** glyph — Pango composes the pair for rendering — and cairo emits an
`/ActualText` span so the *logical* text stays decomposed:

```
/Span << /ActualText <feff00650301> >> BDC
  [()-38(\351)]TJ
EMC
```

`<feff 0065 0301>` is BOM + `e` + combining acute, wrapped around a single `\351` (é) glyph.

**Why it matters, and it is not trivia**: [TDD 25.18](TDD.md#2518-text-round-trips-out-of-the-pdf-byte-exact)'s
"decomposed sequences survive uncomposed; `U+0065 U+0301` does not silently become `U+00E9`"
is true on this platform **because of that span, not because of anything in this project and
not because of the font encoding**. The drawn glyph *is* precomposed. If cairo ever stopped
emitting the span, the rubric would start failing with **no code change here** — so a reader
treating 25.18's NFD clause as a claim about our pipeline is reading it wrong. It is a claim
about the artefact, and the mechanism sits in a dependency.

**The transferable lesson, and it is the exact inverse of
[P-8](#p-8-an-extraction-failure-is-not-evidence-about-appearance).** P-8 says an *extraction
failure* is not evidence about appearance. This says an *extraction success* is not evidence
about the glyph either. **"The text round-trips" and "the character drawn on the page is the
one you asked for" are independent claims**, and a PDF can satisfy the first while drawing
something else — legitimately, by design, through marked content. The drawn glyph and the
declared text are different objects and the extractor reports the declared one. Anyone
asserting a normalisation property from extraction alone is measuring the **producer's
declaration**, which is the right thing for a searchability rubric and the wrong thing for a
rendering one. Pair an NFD assertion with a rendered-page check for the same reason 25.11a
pairs the emoji one.

**Two halves, and the seam is marked deliberately.** The measurement above is P-13 family and
belongs in the testing/measurement home. The second half — *a rubric's guarantee can live in a
dependency's emission rather than in your pipeline* — is engineering discipline, which has no
reusable home yet and so stays project-side. Kept in one entry so that the day such a home
exists it splits cleanly instead of being rediscovered.

**Corrective**: state 25.18's NFD clause as a property **of the artefact**, not of the
pipeline. When it fails, look at `/ActualText` in the artefact before
looking at `walk.rs` or `markup.rs`. And never hand-roll an `/ActualText` span —
[O9](#o9--emoji-in-the-pdf-export) already says cairo's emission is a run-splitter; this is
the stronger reason, that one of those spans is doing work the contract depends on.

**Corrects a reading of [P-14](#p-14-cairos-windows-colour-glyph-path-wraps-colour-glyphs-in-a-type3-d0-font-breaking-layout-reconstructing-extraction)**:
its recorded `/ActualText` count of **0** is a property of the probe fixtures it was measured
on, which contained no decomposed input — not a property of the sink. An artefact from the
shipped path with NFD in it has one.

**And it sharpens the P-14 / P-17 pair.** Measured through the shipped sink on both platforms,
the two are closer than "two backends, two mechanisms" suggests: **both rasterise the colour
glyph to an image; the difference is whether that image is wrapped in a font.** Windows wraps
it in a Type3 font that carries a `/ToUnicode`, so a text run exists and the codepoints are
recoverable; macOS emits the image with no text run at all, so there is nothing to attach a
`/ToUnicode` to. Same rasterisation, opposite extractability, and the wrapper is the entire
difference. Say it that way when the two entries land together.

**Measured**: Windows 10 19045 / GTK 4.22.4 gvsbuild / cairo 1.18.4, the real `export::pdf`
sink at `02045f6` — 44,397 bytes, `%PDF-1.7`, 57 indirect objects, `/ActualText` ×1,
`BT`/`ET` 1/1, `d0` ×2 and `d1` ×0.

### P-19. "No GPU client" has two forms on Windows, and a missing counter row is indistinguishable from a broken counter

**Destination**: the reusable testing/measurement home, beside
[P-3](#p-3-a-silent-instrument-returns-a-confident-wrong-answer-that-looks-like-a-finding)
— it is about reading an instrument, not about this project.

**Authored by the Windows seat**; allocated and formatted by the Linux seat.

**The distinction**: [POLICY](POLICY.md)'s Windows footprint criterion is *the absence of a
process row*, not a byte count. Measured through the shipped export sink at two document
sizes, **both forms occur**:

* a `GPU Process Memory` instance **existing for our PID and reading 0 B** (the 3× window-area
  run), and
* **no instance for our PID at all** (the 200× document-size run).

Both mean zero. **The absent row is the stronger reading** and the one POLICY's wording
actually describes. Do not flatten them into "0" in a report — a future reader comparing two
runs will otherwise see a difference where the record says there is none, or miss one where
there is.

**The trap, and the reason this is a finding rather than trivia**: *a missing row and a dead
counter produce identical output.* "No instance for our PID" is exactly what a mistyped
counter path, a permissions failure, or an unavailable counter set also returns. **A negative
reading here needs a positive control**, and the one used was the right shape: the counter set
returned **11 instances** at that same moment, one of them nonzero (pid 4, 0.01 MB). The
instrument reads real values; our process simply is not in it.

**Corrective**: when asserting an absence from a performance counter, assert in the same
breath that the counter is live and populated. Record the control's numbers, not just its
existence — "the control passed" is itself an unfalsifiable claim.

**Measured**: Windows 10 19045 / GTK 4.22.4 gvsbuild / VMware SVGA 3D, `export::pdf` at
`02045f6`. Fixtures of identical *shape* and 200× differing *size* (1,158 B / 57 lines vs
231,760 B / 11,201 lines → a 201-page PDF), window held constant, so document size varies and
content weight does not. **VRAM did not move with document size, because there is no VRAM.**
RSS did (79.00 → 136.61 MB at rest) and that is not the gate — 6.5's ceiling is VRAM, and a
200× document holding more text in system memory is the Cairo software renderer working.
