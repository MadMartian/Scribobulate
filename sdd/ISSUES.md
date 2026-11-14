# Known Issues

**`Platform`** is one of **`Windows`** · **`Mac`** · **`Linux`** · **`Any`** — the platforms an
entry is known to affect, not where it was found. `Any` means reproduced on, or inherent to,
every platform; a named one means the others were checked and do not exhibit it. Before
narrowing an entry to a single platform, have that platform's peer seat fail to reproduce it
(POLICY § Manual integration testing) — behaviour found on one platform is not
platform-specific until someone else looks.

**`Scope`** is one of **`Test`** · **`Production`** · **`Project`** · **`Upstream`**.
`Test` affects only the suite or the pipeline; `Production` affects what a user runs;
`Project` is both. **`Upstream` means the defect is in a third-party library and we cannot
FIX it** — a workaround may exist, but the repair is not ours to make, so an `Upstream`
entry is not work waiting to be scheduled here. It is orthogonal to severity: an `Upstream`
entry can still be the worst thing in the register.

**An `Upstream` entry carries its evidence in its body.** Calling a defect upstream is the
claim that excuses us from fixing it, so it must be checkable by the next reader, not taken
on trust:
- **A link to the upstream ticket** (bug, issue, advisory or merge request), with its state
  and the date that state was checked. Name the ticket's own words that match our symptom,
  and say how the link was made — by a debugger trace into the library, by the library's own
  reproducer, or by symptom only.
- **Evidence that the defect is the library's**: a reproduction outside this application,
  a trace into the library's code, or the ticket's own reproducer.
- **When no ticket exists**, say so, with the date and where it was searched, and file one
  — or record why it was not filed. An entry with neither a link nor that statement is not
  `Upstream` yet.

**Read an entry sceptically before building on it.** Across the five batches that emptied
this register down from eighteen entries, **four** recorded root causes were measured and
found WRONG, and three entries turned out not to be defects at all — one whose stated worry
was structurally impossible while a different, real defect sat underneath it, reachable only
because the reproduction was built anyway. An entry is a report plus somebody's best
inference at the time, and the inference ages worse than the symptom. Reproduce first; fix
the thing you measured, not the thing that was written down.

**Two tables, and the split is the point.** The first lists **open** debt — things someone
is expected to fix — carrying a severity you triage on. The second lists **closed** entries:
problems investigated to a finding of *no reachable fix*, kept precisely so nobody spends a
session rediscovering a settled dead end. A closed entry is **not** a fixed one; a fixed
issue is deleted outright, because this file is a snapshot of what is currently broken and
not a changelog. Closed entries carry a `CLSD-dd` number that is never reused or renumbered,
and they hold no severity, because they are not queued work.

**One defect can be filed twice.** A missing reading position, seen from two ends, was
carried here as two unrelated entries and was nearly fixed twice before anyone noticed they
were one thing. Before opening work on an entry, scan the others for the same mechanism
described from a different vantage point.

**A grouped entry** holds several findings of one area under one letter, each a bullet
with its own severity, reproduction and platforms; the entry's severity is its worst
bullet's. One release pass found more deferrable defects than the alphabet has letters,
and these were filed that way rather than compacting or widening the IDs. A fixed finding
deletes its bullet; the letter goes when the last bullet does.

| ID | Platform | Scope | Issue | Severity |
|----|----------|-------|-------|----------|
| A | Windows, Mac | Production | No screen reader on Windows or macOS can read the app's accessible names: our GTK builds leave GTK's AccessKit accessibility backend off, so neither platform gets a tree to carry them. Linux/AT-SPI reads them | Medium |
| B | Mac | Upstream | macOS only: every native file-chooser invocation (Open, Save, Export) grows RSS by ~1.1 MB and does not give it back. Roughly four fifths is AppKit's own price for presenting an `NSSavePanel` — reproduced with no GTK in the process — with about a fifth GTK-attributable. Caching the panel upstream would recover ~95% | Medium |
| C | Any | Production | The preview's Annotate bubble sits over the line above a selection, so a click there lands on the bubble: the first click of a double- or triple-click opens the comment card, the next press closes it, and the reader gets a single click | Low |
| E | Windows | Upstream | After an edit the editor's scrollbar slider is sometimes not drawn until the next scroll (2 of 40 Enters); a GTK defect still open upstream | Low |
| F | Linux | Upstream | A crafted SVG can corrupt memory: Linux decodes SVG through the distribution's librsvg, which predates the fix for a published use-after-free (CVE-2026-96889, fixed in 2.63.2), and refuses any SVG that may use XInclude before librsvg sees it until the librsvg it loads carries the fix. Windows and the macOS bundle carry 2.63.2, and their builds fail below it | High |
| G | Windows, Mac | Test | Two split-mode live-reload tests in `window::outline_reload_tests` intermittently fail on Windows CI because the editor is not where the test parked it (top line 0, or 70 where >100 is required) | Low |
| I | Mac | Upstream | Closing a full-screen window (Exit or ⌘W) logs `Gdk-CRITICAL gdk_surface_request_layout: assertion 'frame_clock' failed` once: AppKit's exit-full-screen transition resizes the already-destroyed surface, and GDK's resize handler does not check for that; no visible effect | Low |
| K | Mac | Test | The GTK suite on macOS CI intermittently dies with `GLib-GObject-CRITICAL object_ref: assertion '!object_already_finalized' failed` then SIGSEGV — twice in six runs on near-identical trees, in two unrelated `window::` tests; green on re-run each time; 0 of 43 locally on the same GTK 4.24.1 | Medium |
| L | Any | Production | A highlight with no comment, `{==text==}` on its own, is valid CriticMarkup but the app treats it as not an annotation: it is left out of the annotations sidebar by design and the reader does not get a highlight with its markup hidden | Low |
| M | Windows | Production | Windows file-system edges: live reload stops after the document's folder is deleted and recreated, never starts on a UNC path, and once left a false "deleted" state after a rename away and back; `nul.md` opens as the NUL device instead of being refused | Medium |
| N | Any | Production | Five status notices are missing, late, covered or wrong: no "Opening…" for File ▸ Open, no second conflict prompt, a toast over the deleted-file prompt, a stale status over a truncation notice, the wrong reason logged for an unreadable image | Low |
| O | Any | Production | Export drops or garbles content: a task item with a nested list loses its checkbox (HTML, PDF), Hangul is tofu in a Windows PDF, an unaligned header is centred and the Annotations summary prints a literal `**` (HTML) | Medium |
| P | Any | Production | Preview rendering slips: an h-scrollbar at the narrowest window with the outline open, `<img width>` ignored, a `<details>` list item's bullet on its own line, a tiny disclosure marker on Windows, a raw `+++` in the front-matter snippet | Low |
| Q | Any | Production | A rejected key in a user theme falls back to the desktop's page colour rather than the built-in theme's, which can leave Sepia ink invisible; two toolbar icons take the Linux host theme's; three tooltips show no accelerator | Medium |
| R | Any | Production | The view lands somewhere else: a disclosure toggled at the document end jumps to the top, a click during Ctrl+End's travel on Windows, Side by Side on a short document on macOS, a reload during a resize drag, the first outline click into a collapsed block | Medium |
| S | Any | Production | Editor commands go dead or misbehave: Format greyed after Swap panes and a tab round trip, Insert Emoji never inserts on macOS, a middle-click paste off a glyph is dropped (Linux), a new tab inherits the Replace text, typing clears the annotation selection (Windows) | Medium |
| T | Any | Production | Annotation edge cases: annotating over a highlight prefills its old comment, the marker popover quotes raw source, a comment at a paragraph's start puts its chip beside the block before, a card vanishes instead of flipping on resize | Low |
| U | Any | Production | Session and launch edges: two same-instant launches skip the restore, a dragged tab order is not saved, a wrongly typed `sidebar_split` drops the whole session, `--` opens as a document, two history stops are wrong | Low |
| V | Any | Production | Crash-recovery snapshots: two live processes on one session write one swap file, so one edit is lost in a crash, and a snapshot body holding a NUL is not refused | Low |
| W | Any | Production | Footprint and responsiveness: ~0.6 MB retained per window open/close, a ~68 s first-render stall on a 3000-item nested list, and the process outliving its windows by 20–30 s after Quit on Windows | Medium |
| X | Windows | Production | Uninstalling on Windows leaves an empty `HKCU\Software\Scribobulate` key | Low |
| Y | Mac | Test | `window::outline_nav`'s tab-switch reveal test fails its "preview is long enough" precondition on a large Mac display: the test sets no window size | Low |

## Closed issues

Intractable: no reachable fix, and the limitation is still real and present.
Do not reopen one without a new constraint. These numbers never change and are never
reused — unlike the letters above, which are positional and get reclaimed.

| ID | Platform | Scope | Issue |
|----|----------|-------|-------|
| CLSD-01 | Any | Upstream | Tables are selection islands; cells are individually selectable but not part of the continuous buffer |
| CLSD-02 | Any | Upstream | A paragraph that mixes fonts (any inline-code span) can lay out a few pixels wider than the wrap width it was given, summoning the preview's Automatic horizontal scrollbar and intermittently blanking the pane until a resize |
| CLSD-04 | Mac | Upstream | In fullscreen, a click issued while the transition animation is still running is never delivered — AppKit blocks input for its own ~250-500ms window, in any Cocoa application. Not ours to fix, and not GTK's |
| CLSD-05 | Any | Upstream | Syntax highlighting depends on `bincode` 1.3, which RustSec marks unmaintained (RUSTSEC-2025-0141, informational): `syntect` reads its compiled-in syntax and theme dumps with it and no syntect release is off it. Waived in `deny.toml`; the data is never a user's file |


## CLSD-01. Tables are selection islands

**Status**: Closed (intractable — every exit is walled *within* GTK's selection
machinery, source-verified to a measured verdict below; the one theoretical escape
leaves those bounds only by becoming a different project. Real and unresolved, not
fixed — retained as a documented permanent limitation. Not actionable.)

The preview is a single `GtkTextView` buffer with `GtkTextTag`s for formatting.
All prose, headings, code blocks, **blockquotes**, and inline content participate in
continuous cross-document selection. Tables are embedded as `GtkTextChildAnchor`
islands (a custom `ScribTableWidget` holding `GtkLabel` cells, each
`set_selectable(true)`); a cell's text is selectable on its own, but a drag-select
cannot span from body text into a table cell, nor across cells, in one gesture.

**Continuous cross-cell selection is unavoidable** (researcher-verified, gtk-4-6): an
anchored child occupies a single `U+FFFC` object-replacement char in the buffer, and
the `GtkTextView` selection model treats it as one opaque unit with no path into the
child's text. There is no cross-widget continuous selection in GTK 4.6. (Blockquotes
moved into buffer text precisely to get continuous selection where it *was* possible;
tables can't, because they need 2-D widget layout.) A drag cannot span from body
text into a cell, but selecting text *within* a cell copies that cell's own Markdown
source character-precisely (each cell carries its own `copymap`, formatting preserved
— TDD 2.8f); a buffer selection overlapping the table anchor copies the whole table
source.

**Upstream ticket**: none. GNOME/gtk issues searched 2026-10-23 for selection across
labels, widgets and child anchors; nothing matches. This is GTK's per-widget selection
design rather than a defect, so a ticket would be a feature request. Not filed: a filing publishes under the operator's identity, and no seat holds GNOME GitLab credentials, so it waits on the operator.
The evidence that the wall is GTK's is the source trace and probe below.

### ⛔ Unmitigable within GTK's selection machinery — investigated, probed, closed

**Do not re-open this on a re-read.** Both routes past the anchor were taken to a measured
verdict (researcher + probe, gtk-4.6.9). The obvious designs all look viable on paper and
fail only at runtime, which is why the negative results are recorded here rather than
rediscovered.

**1. Mid-drag promotion — "let the label start the drag, take over when the pointer
escapes the cell" — is impossible, not merely hard.** `GtkLabel`'s lazily-created
selection machinery (`gtk_label_ensure_select_info`, `gtklabel.c:4826`) includes a
`GtkGestureClick` that **claims on press** (`:4313`). A claimed sequence sets `DENIED` on
*every gesture on parent widgets in the propagation chain* (`gtkgesture.c:84-92`), and
**`DENIED` is terminal** (`:1020-1035`). So by the time the pointer escapes, an ancestor
gesture can never claim. Observation was never the problem — capture-phase ancestors *do*
see the motion; **claiming** is.

**2. GTK's own sanctioned escape hatch — "claim early, decide late"** (`gtkgesture.c:94-99`:
a capture-phase ancestor claims on press, then denies to hand the press back, GTK
*emulating* it) — **was probed and fails twice** (Xvfb, double-click on a word, reading
`selection_bounds()`):

| Setup | Selection | |
|---|---|---|
| control — no ancestor gesture | `(0,5)` = `"alpha"` | ✅ word-select works unaided |
| deny on drag-update only (the documented shape) | **`None`** | ⛔ label receives nothing |
| + deny on release (that gap patched) | **`(0,11)` = `"alpha bravo"`** | ⛔ silently wrong |

- The documented shape **has no branch that fires for a click** — a click produces no
  motion (`drag_update = 0`), so a deny placed on drag-update never runs, the sequence
  stays claimed, and the press never reaches the label.
- Patching that doesn't save it: double-click then selects **two words**. The ancestor's
  claim emits `::cancel` on the gestures underneath (`gtkgesture.c:88-89`) →
  `gtk_gesture_click_cancel` (`gtkgestureclick.c:282-288`) → `_gtk_gesture_click_stop`,
  which **zeroes the counter**: `priv->current_button = 0; priv->n_presses = 0;`
  (`:112-113`). **The claim wipes the multi-click state on the way IN, before any
  emulation** — and the emulation replays *one event*, not the counter, so it is
  structurally incapable of rebuilding it. **`gtkgesture.c:94-99`'s "one similar event will
  be emulated" preserves event *coherence*, not gesture *state*** — the docs tell the
  literal truth and still mislead anyone designing this. Corollary: pre-empting a
  **stateless** gesture is recoverable; pre-empting a **stateful** one is not.
  *(The counter wipe is source-verified; the exact accounting for why the result is
  precisely two words rather than two independent single-clicks is unexplained, and
  deliberately not guessed at.)*
- The failure is **plausible-but-wrong**, not empty — it would feed Copy silently. Adopting
  it would break double-click word-select, which works correctly today.

**3. A keyboard-only trigger survives but isn't worth building.** `GtkLabel::move-cursor` is
a public keybinding signal (`:2205`) that fires *before* the default handler clamps, so a
boundary escape is observable — and it pre-empts no gesture. But a table where Shift+Down
crosses cells and **dragging does not** is less coherent than today's honest dead-stop.

*(Related GTK facts established during this investigation, in case they're wanted
elsewhere: `GtkLabel` exposes no cursor position — the public getter normalises
`anchor`/`end` away, `:2118-2120` — and the PRIMARY-clipboard "hole" GTK4Rs/AP-28 once alleged
**does not exist**; see GTK4Rs/AP-28 / GTK4Rs/AP-120.)*

**The limitation is accepted, and the impact is small.** In-cell selection is already
char-precise (TDD 2.8f); a buffer selection over the table anchor already copies the whole
table source. Nothing here is broken — the feature simply cannot be added through GTK's
selection machinery.

**The one theoretical escape, priced honestly and not recommended**: drop
`set_selectable(true)` and have `ScribTableWidget` own selection outright — hit-test via
Pango `xy_to_index`, draw the highlight in a `snapshot()` override. This sidesteps gesture
arbitration entirely (exactly what defeated the routes above) and is *technically*
possible, so this entry says "unmitigable **within GTK's selection machinery**" rather than
"impossible" outright. But it means reimplementing char selection, double-click-word,
triple-click-line, keyboard selection and PRIMARY ownership — all of which GTK provides
free today, as the probe's control demonstrates — to un-break a minor limitation
nobody has asked for. It would be a deliberate project chosen on product grounds, not an
increment, and it should not be started from this entry.

## A. No screen reader on Windows or macOS can read the app's accessible names

**Severity**: Medium (no screen reader on either platform reaches any control; the names
themselves are set correctly, and Linux/AT-SPI reads them).

Reopened 2026-10-24 by the operator from CLSD-03, a number now retired. That entry was
closed as inherent to GTK's Windows and Quartz backends, and the premise was wrong.

**Cause**: our GTK builds leave GTK's AccessKit backend off. AccessKit publishes UI
Automation on Windows and NSAccessibility on macOS:
[gtk#6394](https://gitlab.gnome.org/GNOME/gtk/-/issues/6394) (closed 2024-12-20), added by
[!8036](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/8036), first released in GTK
4.18. At 4.22.4 it is the `accesskit` build option, **default `disabled`** (`meson.options`).
Neither of our GTK sources enables it: Homebrew's `gtk4` formula and gvsbuild's `gtk.py` pass
no `-Daccesskit`. Checked 2026-10-23 by reading those three files; not yet measured with the
option on.

⚠ The `Platform` cell reads `Windows, Mac` rather than one of the four single values the
header defines: the consequence is identical on both and the causes are backend-specific,
so filing it twice would be the header's own "one defect filed twice" trap, and `Any` would
be false — Linux/AT-SPI reads these names correctly.

MEASURED on both seats while ratifying the status bar (2026-09-15), with AccessKit off.
**Windows** (GTK 4.22.4 gvsbuild, Win10 19045): UI Automation returns the toplevel (class
`gdkSurfaceToplevel`) with **zero descendants**, against a positive control of Notepad
returning two. **macOS** (GTK 4.22.4/Quartz): the window exposes 4 chrome elements and its
"entire contents" is **empty**, with the reader validated in the same run (the native menu
bar enumerates, and Save/Save As report their real enabled states).

The consequence for verification: the accessibility rubrics (TDD 16.5, 16.7, 16.17) are
**unrunnable** on Windows and macOS rather than failing. Their names ARE set — `a11y.rs` is
the single choke point and `clippy.toml` bans the bare tooltip setter — and AT-SPI reads them
on Linux, so a run that reports these checks as "not observed" is reporting this gap, not a
defect in the code under test.

**Fix**: build GTK with `-Daccesskit=enabled` on both platforms — a gvsbuild override on
Windows and a GTK built by us instead of Homebrew's bottle on macOS — then rerun the
2026-09-15 reads. Costs to weigh first:
- The backend needs the Rust `accesskit-c` library at GTK build time on both platforms.
- Open crash reports with the backend on:
  [gtk#7653](https://gitlab.gnome.org/GNOME/gtk/-/issues/7653) (Windows) and
  [gtk#8281](https://gitlab.gnome.org/GNOME/gtk/-/issues/8281) (macOS, reopening a window).
  Each needs a check that the app does not hit it before the build delivers.

## B. Every native file chooser invocation grows RSS on macOS

**Severity**: Medium. Monotonic within everything measured at the per-invocation scale, but the
cost is overwhelmingly AppKit's own price for presenting an `NSSavePanel`, and it is not
reachable by any change this project can make.

**Re-measured 2026-08-27 with a control, which sharpened the claim in both directions.** Ten
`File ▸ Open` invocations, CANCELLED every time so no document ever loaded, grew RSS
222,432 → 232,288 KiB — monotonic, never reclaimed, **≈985 KiB per invocation**. The control
is what makes that a cause rather than a coincidence: ten cycles at the same cadence, same
frontmost-and-Escape driving, chooser never opened, moved RSS by **+32 KiB total**. So the
growth is the chooser, not elapsed time and not the driving method.

**And the counter-evidence, recorded because it is the half that would otherwise be
mis-read.** A separately-observed instance sitting at 257 MiB after ~1.5 h of ordinary use
was NOT this issue accumulating: watched across a further window it went 257 → 251.5 →
230.5 MiB — *downward*. "Idle instance at a high RSS" reads as corroboration and is the
opposite. The per-invocation leak is real; something reclaims at a larger scale, and the
shape of that reclamation is unmeasured. Do not describe this entry as unbounded growth.

**Symptom**: opening a `GtkFileChooserNative` and cancelling it grows resident memory on
macOS, per invocation, and the memory never returns. Neither Linux nor Windows reproduces it.

**Status: ATTRIBUTED TO THE PLATFORM, and OWNER-BLOCKED.** Not a GTK defect and not this
project's reference discipline — both call sites were audited and cleared. Roughly 93% is
recoverable upstream by REUSING the panel rather than releasing it; a patch sketch exists,
must be authored twice because the 4.6 and current variants differ, and two naive forms of it
deliver a use-after-free. The upstream filing is gtk#8498 (below).

**Upstream ticket**: [gtk#8498](https://gitlab.gnome.org/GNOME/gtk/-/work_items/8498), "macOS: GtkFileChooserNative is never finalized if hidden while its panel is open", filed by the operator 2026-10-24, open. It reports the GTK-owned share by symptom only, with a self-contained C reproducer run on GTK 4.24.1 / macOS 27.0.1: a native chooser hidden while its panel is up finalizes 0 of 12 times, against 12 of 12 for one never shown. That is the evidence the defect is GTK's. Earlier searches (2026-10-23 and 2026-10-24) found no prior ticket. The AppKit share has no Apple Feedback report.
Related, none confirmed to be this defect: wxWidgets #22173, Qt QTBUG-39205.

**What this project does about it**: nothing, deliberately. There is no application-side fix,
the exposure is one panel per invocation on one platform, and TDD §6's ceiling is not
threatened by it.

**The full investigation is `probes/native-chooser-rss-investigation.md`**, beside the probe
that produced it — measurements with their conditions, the retain-cycle analysis, the patch
sketch and its hazards, and the instrument failures that shaped it. It lives there rather than
here because this entry exists in order to be deleted when the defect is fixed, and the
evidence must outlive it. Do not restate its figures here; several carry caveats that do not
survive summarising, and the transferable lessons already have permanent homes in
`sdd/ANTI-PATTERNS.md`.

## C. The Annotate bubble covers the line above a selection and takes a click meant for it

**Severity**: Low (a click lands on the bubble instead of the text beneath it; nothing is lost).

**Observed** (2026-09-30, Linux, bare Xvfb, GTK 4.6.9): select text in a table cell, so the
preview's **Annotate** bubble appears above the selection. The bubble covers the
text of the cell above. Double-click and drag in that cell, and only one press reaches
it: the double-click becomes a
single click and a drag that should extend by words selects characters. The same drive with
the bubble dismissed first behaves correctly. Probably what spoiled a triple-click the
operator reported. In body text the bubble covers the line above in the same way; not
separately driven there.

**Mechanism** (2026-10-23, Linux, private Xvfb, GTK 4.6.9, debug build with a backtrace
on the bubble's hide): the first click of the double-click is a complete click on the
bubble's **Annotate** button. It opens the comment card (`show_entry` in
`preview/annotate/overlay.rs`) and hides the bubble; the second press then lands on the
cell, takes focus out of the card, and the card's focus-leave closes it. The cell sees one
press, so it acts as a single click. The earlier report that the first press reached the
cell was wrong. Filed `Any` because the bubble's placement is shared code; macOS and
Windows have not been driven.

**Decision** (operator, 2026-10-24): accepted, no remedy scheduled. The reader can dismiss
the bubble by clicking elsewhere first. Not taken: placing the bubble clear of text (in the
margin or below the selection), which changes a placement other features and tests rely on.

## E. On Windows the editor's scrollbar slider sometimes goes missing after an edit

**Severity**: Low (the slider alone, occasionally, and the next scroll brings it back).

**Observed** (2026-10-01, Windows, GTK 4.22.4 gvsbuild, release build, edit-only mode,
`sdd/POLICY.md`, scripted Enter-then-scroll drive): after an Enter, the editor's overlay
scrollbar draws its trough but not its slider. 2 of 40 Enters with the repair below in
place, 7 of 40 without it; 0 of 40 with no edit. Scrolling brings it back, as does a resize.
`Trying to snapshot GtkGizmo … without a current allocation` is logged with or without the
repair, and macOS logs it too with the scrollbar still drawn, so the warning does not
discriminate.

**Cause**: GTK, still open upstream — GtkTextView changes its scroll range from inside
paint, which leaves the scrollbar a frame behind.
- Upstream bug: [GNOME/gtk#6057](https://gitlab.gnome.org/GNOME/gtk/-/issues/6057),
  "GtkTextView causes queue_allocate during snapshot", open since 2023-08-24 (state checked
  2026-10-23). Its reproducer is a bare `GtkTextView` in a `GtkScrolledWindow`: typing
  lines makes the scrollbars glitch and logs `Trying to snapshot GtkGizmo … without a
  current allocation`, the warning this editor logs, traced to
  `gtk_range_adjustment_changed` calling `gtk_widget_set_alloc_needed` during snapshot.
  The link is by symptom and code path: our Windows case has not been traced to that call
  in a debugger, and the warning alone does not discriminate (see Observed).
- Proposed fixes, all unmerged on 2026-10-23:
  [!5222](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/5222) (range: handle
  `GtkAdjustment::changed` at the next frame),
  [!5547](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/5547) (textview: don't
  validate during snapshot),
  [!6334](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/6334) (draft).

Linux on GTK 4.6 had a worse, separate defect (the scrollbar stayed undrawn until a resize,
24 of 25 Enters), fixed upstream in GTK 4.10 by
[!5564](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/5564) (merged 2023-02-28). The editor now hides and
re-shows its scrollbar after every scroll-range or position change
(`window::splitview::relink_vscrollbar_after_range_changes`), which ended the Linux defect
(0 of 30) and reduced this one. Not seen on macOS (0 of 20).

**Known cost of that repair, accepted**: the hide/show resets GTK's record that the pointer
is over the scrollbar, so with the pointer resting perfectly still on the scrollbar the
scrollbar now fades 1–2 s after each edit (Windows, 13 of 20) where it used to stay shown.
Any pointer motion brings it back. Judged negligible by the operator.

**Mitigation options**:
- Accept it until GTK fixes gtk#6057.
- Not an option: non-overlay scrollbars for the editor, as the preview has. Tried 2026-10-23 and
  rejected: the hide/show repair above leaves a classic scrollbar drawn but unclickable
  (no thumb drag, no trough click; Linux and Windows), and without the repair GTK 4.6
  stops drawing its slider after about 11 Enters. A fresh Windows harness also counted
  0 of 40 on the unchanged overlay scrollbar, so the 2-of-40 figure above has not been
  reproduced since.

## F. Every SVG is decoded by librsvg, which has a published use-after-free

**Severity**: High (a crafted SVG can corrupt memory on Linux wherever the interim screen
misses a route; a crash is the realistic worst case, code execution is not
ruled out).

**Report**: GHSA-wq5f-xc86-pv6w / CVE-2026-96889, fixed in librsvg **2.63.2**. Windows loads
2.63.2 (gvsbuild 2026.10.0) and the screen stands down there. Linux loads the distribution's: Ubuntu 22.04's 2.52.5 is "Not affected" per
Ubuntu's tracker (a vendor assessment we have not measured); other distributions are unassessed.
macOS from its bundle loads 2.63.2.

**Open because**: the librsvg Linux distributions ship is older than 2.63.2 (Ubuntu 26.04:
2.61.3). The interim
workaround — the XInclude screen ahead of every decode — is recorded, with every leak found in it
and what upstream must deliver to retire it, in [CVE.md](CVE.md) as **ScrSEC-1**. Read that entry
before changing the screen. The screen stands down by itself on a Linux whose librsvg reaches the floor; this entry
closes when ScrSEC-1 is marked Closed.

**Not measured**: the screen's premise that librsvg follows no non-`data:` reference without a
base URL (the loader is fed a stream with none) is read from librsvg's design.

## G. Split-mode live-reload tests sometimes find the editor short of where they parked it on Windows CI

**Severity**: Low (test-only; the run passes on a re-run of the failed job).

**Symptom**: on Windows CI only, `window::outline_reload_tests` fails in split mode:
- `a_live_reload_keeps_a_heading_at_the_top_of_the_split_preview` (List case):
  `assert_preview_top_is_the_heading` finds editor top line 0, preview top line 0, heading 445.
- `a_live_reload_keeps_the_caret_and_the_filtered_outline_in_split`: the precondition
  `before_top > 100` fails at top line 70. Earlier, the same test failed with editor top 0.

Seen on CI runs 38311458383 (both tests in one run, in the same test thread) and once before
that. The failed job passed on re-run each time.

**What is known**: both failures are in setup or settling, not in the reload under test. The
editor never reached the parked line, or did not stay there. Linux: 0 of 20 runs of the module,
on both the old and the new code, unloaded. Mac: green on every CI run; one local failure in a
full macOS pipeline (2026-10-27, Paragraph case, editor top 441 where 442 was expected: an
off-by-one, not the Windows "top 0" shape), and 5 of 5 module reruns passed; a second sighting
of that same Paragraph 441/442 shape in a full pipeline on 2026-10-29, again with 5 of 5 module
reruns passing and the pipeline green on re-run. Not reproduced locally on Windows.

**Suspects, unmeasured**: the editor's scroll to the park line is an animated `scroll_to_mark`
that lands in stages (GTK4Rs/AP-14, GTK4Rs/AP-115), and the helpers' settle may time out on a
slow runner before it has finished; the two failures in one thread may also mean one test left
state behind for the next.

**Next step**: have the Windows seat run the module in a loop under CPU load and log the
editor's vadjustment value and upper across the park, as was done for the reading-position
flake fixed by "Keep the reading position when a view switch comes before the editor has scrolled into place" (2026-10-11).

## I. Closing a full-screen window logs a frame-clock critical on macOS

**Severity**: Low (one critical per window closed; nothing visible, the session saves
correctly).

**Observed** (2026-10-27, `mac` seat). macOS, Quartz, GTK 4.22.4 and again on 4.24.1.
Closing a full-screen window logs `Gdk-CRITICAL gdk_surface_request_layout: assertion
'frame_clock' failed`: at File ▸ Exit (2 of 2 on 4.24.1) and on ⌘W with no quit (1 of 1).
With no full-screen window, 0 of 2. Linux (GTK 4.6.9) 0 of 3; Windows (GTK 4.22.4) none.

**Cause** (backtrace under `G_DEBUG=fatal-criticals`, 4.24.1). `gtk_window_close` destroys
the surface (`gtk_window_unrealize` → `gdk_surface_destroy`, which drops the frame clock).
AppKit then runs its own exit-full-screen transition on the closing `NSWindow`
(`_NSExitFullScreenTransitionController setupWindowForAfterFullScreenExit`), which sets
the frame; `-[GdkMacosWindow windowDidResize:]` calls `gdk_surface_request_layout` on the
dead surface without checking that it is destroyed. No app frames are on the path besides
the ordinary `close` (`window::lifecycle::quit_all_windows`,
`window::tabs::lifecycle::close_active_tab`); `platform/mac/fullscreen.rs` and
`winplace::macos::watch` are not involved.

**Worse when the surface is already freed.** On macOS CI (GTK 4.24.1) the GTK suite
closed a full-screen window and started the next case at once; the late callback then hit
a finalized surface (`GDK_IS_MACOS_SURFACE (self)` criticals) and once crashed the suite
with SIGSEGV. The suite now leaves full screen before destroying such a window. In the
app the surface outlived the callback in every sample (10 ⌘W cycles under
`MallocScribble=1`, one machine: 0 crashes), so no production crash has been seen, but it
is not ruled out.

**Fix**: upstream, a destroyed-surface guard in `windowDidResize:`. An app-side workaround
(leaving full screen before closing) would add AppKit's exit animation to every close of a
full-screen window, a visible cost for a log line; it has not been tried.

## K. The GTK suite on macOS CI intermittently dies on a finalized-object ref

**Severity**: Medium (test-only, but two of six macOS CI runs on near-identical trees; each
costs a re-run of the gate, and a release gate that goes red one time in three is a
gate nobody trusts).

**Symptom**: on macOS CI, packaging dry run 42970152472 on the merge of
`feature/release-procedure` (2026-11-11), in
`gtk_suite::window::scrollsync::gtk_integration_tests::repeated_view_mode_round_trips_do_not_walk_the_reading_position`:
`(process:42279): GLib-GObject-CRITICAL **: object_ref: assertion '!object_already_finalized' failed`
then `FAILED (killed by signal 11)`. The test immediately before it in the same thread,
`repeated_edit_round_trips_do_not_walk_the_reading_position_either`, logged
`gdk_frame_timings_presented() called on skipped frame` and passed. 798 other tests passed.

**Second sighting** (2026-11-12, push run 43137305088 on the release branch after the
notices fixes): `gtk_suite::window::livepreview::gtk_integration_tests::a_control_whose_identity_the_document_repeats_toggles_neither_block`,
same critical, same signal, 798 others passed; green on re-run. Two unrelated tests, so
the holder is not in either test — it is in what both exercise: a window built, driven
and torn down inside one process that has already done the same ~400 times. The runner's
GTK is Homebrew 4.24.1, the same version the Mac seat's 0-of-43 was measured on.

**What is known**: the same tree passed macOS in push run 42966560560 minutes earlier and in
42963000129 (the branch head it merged, identical tree) before that, so it is intermittent. A finalized-object
`ref` followed by a segfault is a dangling GObject pointer, not a timing miss: something in the
window-mode round trip (`round_trip(&window, "split")` — the split pane mounted and unmounted
four times) still holds a widget the previous trip finalised. Not reproduced anywhere else; no
report from Linux or Windows CI.

**Next step**: the Mac seat looped the `scrollsync` module ×40 and the `window::` group ×3
under load with `G_DEBUG=fatal-criticals` and `MallocScribble=1`: 0 of 43. Next is the
whole `gtk_suite` binary in CI's order, under load, with `MallocScribble=1`, until the
critical fires and names the holder; if it points at a closure or idle captured across a
window teardown, that is an application defect (CAM § handlers on continuously firing
signals) and this entry becomes Production scope.

## L. A highlight with no comment is valid CriticMarkup and is not treated as an annotation

**Severity**: Low (a reader can still see the text; the markup is in the way).

CriticMarkup's highlight is `{==text==}`; the specification pairs it with a comment
(`{==text==}{>>comment<<}`) as the usual shape, and every implementation that renders
CriticMarkup (MultiMarkdown, Marked) renders the bare form as a highlight (`<mark>`) on
its own. Scribobulate's contract covers only the pair: TDD 17.1 renders a highlighted
claim *with its comment*, and `annotations::extract_entries` deliberately skips a bare
highlight (`a_bare_highlight_is_not_listed`), so it never reaches the annotations
sidebar. Found 2026-11-12 when an agent wrote questions as bare highlights for the
operator to answer in the app and the operator could not find them.

**Mitigation options**:
- Render a bare highlight as a highlight with its delimiters hidden (17.1's treatment
  without the comment card), and list it in the sidebar as a highlight with an empty
  comment, so Reply attaches `{>>…<<}` to it. Needs a TDD §17 rubric for the bare form.
- Render it as a highlight only, leave it out of the sidebar, and say so in §17.
- Accept the limitation: write the pair; the agent instruction now says so.

## M. Windows file-system edges: live reload stops, never starts, or misreads a device

**Severity**: Medium (the folder case silently stops live reload; Save still warns before
overwriting, so nothing is lost). Found in the 1.0.0 dry run (Windows seat, on the build
of "Merge bug/nul-byte-document: …", 2026-11-13, and its predecessors); each bullet is
fixed and removed on its own.

- **The document's folder deleted and recreated** (Medium): open `dir\doc.md`, delete
  `dir`, recreate it with `doc.md` → "File deleted on disk" stays, the recreated content
  is never offered; one File ▸ Reload works, later appends are never seen. GIO's Win32
  directory monitor loses its handle with the directory. Linux keeps firing. Known
  issues: *"On Windows, live reload stops after the document's folder is deleted and
  recreated — reopen the file."*
- **A UNC path** (Low): a document opened as `\\localhost\C$\…` gets no monitor events, so
  live reload never fires; a .NET `FileSystemWatcher` on the same share sees them. Known
  issues: *"On Windows, a document opened through a network (UNC) path does not
  live-reload — use File ▸ Reload."*
- **Rename away and back in one folder** (Low, intermittent): the notice, badge and
  prompt stayed up over a file back with identical bytes; Linux clears it in the first
  frame. Not reproduced on a later build (2/2, frames to 9 s) — reproduce before
  working it.
- **`nul.md`** (Low): opens as the NUL device ("loaded … (0 bytes)"), no refusal, against
  TDD 1.4a. Win32 treats `NUL`, `CON`, `PRN`, `AUX`, `COM1`–`9`, `LPT1`–`9` with any
  extension as devices; the 1.4a screen should name-check them.

## N. Five status notices are missing, late, covered or wrong

**Severity**: Low (each is a missing or mistimed message; no edit is lost). Found in the
1.0.0 dry run, Linux and Windows, cross-platform by code path.

- **No "Opening…" for File ▸ Open** (TDD 1.4e): from a slow mount, 15–20 frames at 1.5 s
  and 4 s read delay never show it; it is armed on link-follow only. "Reloading…"
  shows at ~0.5 s.
- **One conflict prompt per conflict** (TDD 5.1): after Dismiss, further external
  changes raise no new prompt. The buffer stays dirty and the Save overwrite guard
  (5.2) still fires; only the per-change notice is missing.
- **A toast over a prompt**: Save As, then delete the file → the "File saved." toast
  covers the "File deleted on disk" prompt's buttons for a moment.
- **A stale status over a notice**: "File saved" or "File reloaded" outlives a
  truncation or deletion notice's status text by up to ~6 s; the badge and prompt are
  immediate.
- **The wrong reason logged** (TDD 6.8): an unreadable image is logged with a reason that
  is not why it failed. Log text only.

## O. Export drops or garbles content

**Severity**: Medium (the nested-task case silently drops task state the preview shows).
Found in the 1.0.0 dry run.

- **A task item with a nested list** (Medium, any platform, 3/3): exports without its
  checkbox in HTML and PDF; the preview draws it.
- **Hangul in a Windows PDF** (Medium, Windows; TDD 25.18): Hangul draws as hex-box tofu
  and extracts with its spaces dropped (Japanese-then-Korean lines split); n + U+0303
  draws a detached tilde. The preview falls back to Malgun Gothic; the PDF path's font
  map does not. Linux is byte-exact. Start at the cairo-win32/DirectWrite font map in
  `src/export/pdf/`. Known issues: *"PDF export on Windows does not embed a fallback font
  for Hangul — Korean text shows as boxes."*
- **An unaligned column's header is centred** (Low; TDD 2.2d): the HTML export gives it
  no class, and a browser centres `<th>` by default.
- **A literal `**` in the Annotations summary** (Low; TDD 25.13): the HTML export prints
  it for a claim that crosses bold.

## P. Preview rendering slips

**Severity**: Low (cosmetic). Found in the 1.0.0 dry run, any platform unless marked.

- **An h-scrollbar at the floor** (TDD 2.2e): at the 360 px window floor (TDD 9.38) with
  the outline open, the ~100 px preview grows a horizontal scrollbar that moves content
  3–6 px. None at ≥ 420 px or with the outline closed.
- **`<img width="100">` is ignored**: the image draws at the pane's width. No rubric
  covers HTML `width`/`height`.
- **A `<details>` as a list item's first child**: the bullet sits on an empty row and the
  summary on the next (`disclosure.md`, "Disclosure in a list item"); TDD 2.26c says a
  nested disclosure renders as at top level. The body indent is right.
- **`disclosure_marker_size = 24` draws a 3×4 px speck** (Windows; Linux draws 10×11 px).
  macOS not yet read.
- **The collapsed front-matter snippet** shows the TOML `+++` fence and smart-quotes its
  strings.

## Q. Theme fallbacks and toolbar chrome

**Severity**: Medium (the Sepia case is unreadable text on a dark desktop). Found in the
1.0.0 dry run.

- **A rejected key falls through to System** (Medium, any platform; TDD 18.11): a user
  `[themes.sepia]` with an invalid `background` takes the desktop's page colour, not
  Sepia's own. On a dark desktop that is Sepia's dark ink on dark grey, invisible (2/2,
  macOS); a light desktop gives a white page, which hides it.
- **Two toolbar icons from the host theme** (Low, Linux; TDD 9.31): on a dark desktop
  Show Unsafe Images draws two overlapping rectangles and Annotations a badged envelope.
  The icon names likely resolve in the host theme before the bundled resource. Windows
  and macOS draw the bundled icons.
- **Three tooltips without an accelerator** (Low; TDD 16.4): Insert Link…, Increase
  Indent, Horizontal Bar.

## R. The view lands somewhere other than where the reader was

**Severity**: Medium (visible jumps; nothing is lost). Found in the 1.0.0 dry run.

- **A disclosure toggled at the document end** (Medium, any platform; TDD 2.26h): a
  one-line disclosure the fold splice refuses (its body literal text in the opening
  raw-HTML block) takes the full re-render, and only at maximum scroll does the restore
  fail, throwing the view to the top. Intermittent (4/5 after an outline jump to the end;
  Windows +79 px on 2/9); likeliest a race between the outline jump's far-scroll
  re-issue and the re-render's restore — adjustment-lifecycle work, research first.
  Known issues: *"Toggling a disclosure at the very end of a document can jump the
  preview to the top."*
- **A click during Ctrl+End's travel** (Medium, Windows; TDD 9.33a; 3/3 and 2/2): the caret
  lands where clicked, the viewport snaps to the top or runs on to the end, the caret off
  screen. Linux 0/4 (Ctrl+End completes in < 0.1 s there); macOS not read. Known issues:
  *"On Windows, clicking while Ctrl+End is still scrolling can leave the caret off
  screen — press Ctrl+End again or click once more."*
- **Side by Side from the View menu on a short document** (Medium, macOS; 14/14 by
  menu, 0/12 by ⌥2; Linux 0/15): the preview shows only its last line. The menu route
  alone logs `project_scroll(map) … target=16.0 max=32.0`
  (`src/window/scrollsync.rs`): the editor-driven projection runs on map before the
  preview's range exists. The system menu bar activates the action on a different path,
  which is why Linux cannot see it. File ▸ Reload un-sticks it. Known issues: *"macOS: on
  a short document, View ▸ Side by Side may show only the end of the preview — press ⌥2
  or File ▸ Reload."*
- **A reload during a narrowing drag** (Low; TDD 7.15): an external reload landing mid-drag
  re-anchors near the top; resize alone and reload alone both hold.
- **The first outline click into a collapsed block** (Low; TDD 12.22; 4/4): expands the
  block but does not scroll the heading to the top; a second click does.
- **Setting `view-mode` over D-Bus** (Low, harness only): `org.gtk.Actions.SetState`
  leaves the preview scrolled where the accelerator does not; MANUAL-TEST §1.5 drives
  view mode by accelerator.

## S. Editor commands go dead or misbehave

**Severity**: Medium (Format goes dead until the reader clicks into the editor). Found in
the 1.0.0 dry run.

- **Format greyed after Swap panes** (Medium, any platform; TDD 10.6a; Windows 3/4, Linux
  4/4): two tabs in Split, a selection in A's editor, View ▸ Swap panes, switch to B and
  back → the Format menu stays greyed and Ctrl+B inert; only Heading stays live. Normal
  pane order passes. Known issues: *"After swapping panes, click into the editor to
  re-enable the Format menu."*
- **Insert Emoji on macOS** (Medium, macOS; TDD 9.14): the chooser opens but a pick (click,
  Enter, search) never inserts and Esc does not dismiss it; Linux and Windows insert.
  Quartz-only path in `GtkEmojiChooser`. Known issues: *"Insert Emoji does not insert
  on macOS — type the emoji or paste it."*
- **A middle-click paste off a glyph** (Low, Linux/X11; 6/6): on a blank line, past a
  line's end or on the empty last line the PRIMARY paste is dropped (the handler bails
  when no iter is under the pointer); falling back to the line's end iter fixes it.
- **A new tab's Replace field** carries the previous tab's replace text (Low).
- **Typing above the selected annotation** clears the annotation list's selection (Low,
  Windows 3/3; TDD 20.15). Linux passes; the row re-selects on the next click.

## T. Annotation edge cases

**Severity**: Low (wrong prefill, quoting or placement; the markup written is right).
Found in the 1.0.0 dry run, Windows and Linux, pure markup logic unless marked.

- **Annotating over an existing highlight** prefills the card with the old comment.
- **The marker popover** quotes a formatted claim as raw source.
- **A comment at the very start of a paragraph** puts its margin chip beside the
  preceding block (TDD 17.61; 2/2).
- **On resize, a card whose anchor word moves up** vanishes instead of flipping (TDD 17.64;
  Windows 2/2).

## U. Session, launch and history edges

**Severity**: Low (each needs an unusual sequence or a hand-edited file). Found in the
1.0.0 dry run, cross-platform by code path.

- **Two launches in the same instant** (TDD 8.2a), one bare and one with a file: the file
  launch becomes primary and the session is not restored; a later quit overwrites the
  saved session. A second launch 0.3 s later is clean.
- **A dragged tab order is not saved** (TDD 15.10; 2/2): relaunch restores the pre-drag
  order.
- **A wrongly typed `sidebar_split`** (TDD 20.21): `sidebar_split = "abc"` in
  `session.toml` fails the whole file's parse (the field is a plain `f64` in
  `src/session/schema.rs`), which reads as "no saved session" — the document tabs are
  lost. Out-of-range numbers clamp as TDD says. A tolerant deserializer for the field
  fixes it.
- **`--` opens as a document** (TDD 8.8): an extra empty tab named `--` beside the file;
  `-- --help` forwarded to a running primary opens both. Needs a GLib argument-handling
  answer first. Known issues: *"A `--` on the command line opens an empty tab named
  `--` — leave it out."*
- **History stops** (TDD 23.9; Linux and Windows): Save All with both Save As choosers
  cancelled drops a stop (2/2); with no saved session, startup recovery leaves Back
  enabled onto the blank Untitled tab (3/3).

## V. Crash-recovery snapshot gaps

**Severity**: Low (both need an unusual setup). Found in the 1.0.0 dry run.

- **Two live processes on one session**: two processes launched bare with
  `--new-instance` against one state home with a saved session restore the same tab
  with the same doc id and both snapshot to one `shared-<doc_id>.swap`; after a crash
  the last writer's edit survives and the other is lost (seen once on Linux).
  Single-instance activation normally prevents it. Either the swap name carries the
  process id, or recovery refuses to share a doc id across live processes.
- **A NUL in a snapshot body is not refused**: the document readers refuse a NUL byte,
  but the snapshot decoder (`swapfile::codec::decode`) does not. Reachable only through a
  tampered snapshot; refuse it there too.

## W. Footprint and responsiveness

**Severity**: Medium (the window-cycle retention is unbounded). Found in the 1.0.0 dry run.

- **~0.6 MB retained per window open/close cycle** (Medium, any platform; TDD 8.6): linear
  over 80 cycles, no plateau — a per-window residual, not the whole tab UI (~10 MB). The
  pipeline's memory gates never cycle a window: add one with the fix. No Known-issues
  line (hundreds of cycles before a reader notices).
- **A 3000-item nested list stalls first render** (Low): a 1.2 MB file holds the main loop
  ~68 s before the window appears (Linux; 2000 items ~35–39 s at 100 % CPU on macOS),
  then renders. A candidate for incremental first render (a PLAN), not a point fix.
- **The process outlives its windows after Quit on Windows** (Low): windows gone in 2 s,
  the process lingers 20–30 s; Linux exits in ≤ 0.53 s. Unmeasured: the next step is
  one run with `RUST_LOG=scribobulate=debug` at exit to see what it waits on.

## X. Uninstalling on Windows leaves an empty registry key

**Severity**: Low (invisible to the user). `HKCU\Software\Scribobulate` survives an
uninstall, empty: the installer marks `Capabilities` `uninsdeletekey` and gives the
parent no flag. Reproduce: install, uninstall, look for the key. Fix: one `.iss` row
(`uninsdeletekeyifempty` on the parent). Found at gate 9 of the 1.0.0 dry run, every
Windows build. Known issues: *"Uninstalling on Windows leaves an empty registry key,
`HKCU\Software\Scribobulate`; it is harmless and can be deleted."*

## Y. A tab-switch reveal test assumes a window size it never sets

**Severity**: Low (test-only; CI's macOS runner passes it). On a 1728×1117 pt (@2×) Mac
display, `window::outline_nav`'s `a_tab_switch_before_the_reveal_runs_drops_it` fails
its "preview is long enough" precondition (`upper > 2 × page_size`) — 1 of 813 in the
macOS integration step, identically on two builds of the 1.0.0 dry run, so not a
regression. The test sets no default size where its siblings use 900×600; set one.

## CLSD-02. A paragraph that mixes fonts lays out wider than the wrap width it was given

**Status**: Closed (no public API at the GTK 4.6 floor makes the layout report a width the
wrap budget respects; the two reachable correctives both cost more than the defect)

**Platform**: Any — the mechanism is Pango's line-extent accounting, not a backend's. Only
Linux/GTK 4.6.9 was measured; font metrics differ per platform, so *which* window widths
exhibit it will differ, not *whether* it can.

A `GtkTextTag` that changes the font FAMILY over a character range — in this project, every
inline-code span — splits the paragraph into separate Pango items at the tag boundary. A space
that lands on a wrap point is granted for free by the break logic (`find_break_extra_width`),
but the routine that collapses that hanging space afterwards (`zero_line_final_space`) is keyed
on the last run's last glyph, which is a different object once the items are split. The space
therefore stays, sitting a few pixels past the wrap width, and `GtkTextLayout` reports the
line's LOGICAL extent — hanging space included — as the layout width. That becomes
`hadjustment.upper`, which exceeds `page_size`, which summons the Automatic horizontal
scrollbar, whose appearance and disappearance re-arms the width↔height-for-width churn that
leaves the preview stuck blank until a manual resize (GTK4Rs/AP-22, GTK4Rs/AP-23).

MEASURED (GTK 4.6.9, gtk4-rs 0.10, X11/Xvfb, `#[gtktest::test]`, this repository's own
`sdd/ANTI-PATTERNS.md` as the corpus): a sweep of 41 window widths (600–1000 step 10) at zoom
1.0 found 2 widths over-wide, by 5px and 7px. Isolation is decisive in both directions —
removing ONLY the tag's `set_family` takes the overflow to zero at every width and zoom tried;
removing ONLY the tag's `set_wrap_mode` changes nothing (wrap mode is a paragraph attribute
taken from the view, so a character-range tag never alters it), and the tag's background does
not participate in width.

Impact is narrow and real: on roughly 5% of window widths for a code-dense document the reader
gets a horizontal scrollbar it cannot use and a pane that intermittently blanks while
scrolling. It is invisible at every other width.

**Upstream ticket**: none for this case. GNOME/pango issues searched 2026-10-23 for
`zero_line_final_space`, trailing and hanging space width. Two closed tickets concern
trailing spaces but not a space left hanging where a font change splits the items:
[pango#646](https://gitlab.gnome.org/GNOME/pango/-/issues/646) and
[pango#243](https://gitlab.gnome.org/GNOME/pango/-/issues/243). Not filed: a filing publishes under the operator's identity, and no seat holds GNOME GitLab credentials, so it waits on the operator.
The evidence that the defect is Pango's is the isolation measurement above and the two
Pango functions named in it.

**What walls each exit** — recorded so the dead ends are not re-explored:

- **Reserve slack in the wrap budget** (extra `right_margin`, CSS padding, or the private
  `gtk_text_layout_set_screen_width`) — REFUTED BY MEASUREMENT, not by argument. Any change to
  the wrap budget moves the breakpoint, so the failure RELOCATES rather than clearing: the same
  41-width sweep failed at exactly 2 widths with 0px, 8px and 16px of extra right margin, only
  at different widths each time. A single-width control cannot see this, and reads as a fix.
- **Derive the slack from the fonts' space advances** — the quantity does not exist. The hang is
  however much of the granted glyph sits past the wrap point plus accumulated shaping error at
  the item seams plus the layout's `ceil`, not a glyph metric: measured hangs of 4px and 6px
  against a body space of 3px and a code space of 8px. Any constant is a guess, and it would
  relocate anyway per the point above.
- **Clamp `hadjustment.upper` down to `page_size` when the excess is below a threshold**, from a
  `size_allocate` override after chaining up. This one WOULD close the invariant without
  relocating, and does not re-arm the churn. Declined: it is a symptom gate, not a wrap fix; the
  threshold can only ever be an observed bound from a width sweep rather than a derived
  quantity; and because the hang is not always pure whitespace it may clip 1–3px off a real
  glyph — trading a rare scrollbar for rare silent truncation of the reader's text.
- **Drop the monospace family on inline code** — removes the trigger completely and is the
  positive control that proves the mechanism. Declined: inline code reading as code is the
  product, so this trades a rare layout defect for a permanent, universal regression in what the
  reader sees.
- **`hscrollbar_policy = Never`** — banned outright and independently of this entry: it makes
  `GtkScrolledWindow` adopt the child's minimum width and ratchet, so the window can no longer
  shrink to fit (GTK4Rs/AP-139).

**Mitigation options**:
- Accept the limitation (chosen). A reader who hits it can resize the window a little; the
  defect is a property of the width, so any nearby width clears it.
- Revisit if the toolkit floor rises — this was checked against GTK 4.12 and the width
  computation is unchanged, so a fix would have to come from Pango's collapse logic rather than
  from GTK.
- Revisit if the clamp above stops being a symptom gate — if a way appears to distinguish a
  hanging space from a clipped glyph, the clamp becomes safe and this reopens.

## CLSD-04. In fullscreen on macOS, a click during the transition animation is never delivered

**Status**: Closed (no repair of ours is reachable). Kept so the
investigation is not run a second time.

**The report**: on the macOS build in fullscreen, clicking a toolbar button did nothing and
a second click was needed; the menu bar behaved the same way, and a toolbar button appeared
to activate on the wasted click.

**All three parts are accounted for, and none is a defect in this project or in GTK.**

- **The wasted toolbar click is AppKit's animation blocking its own input.** Measured as a
  timing sweep from the zoom button's mouse-up to the test click, real input throughout, no
  synthetic pointer placement: 0 ms, 100 ms and 250 ms all fail **silently** — the click
  never reaches GTK at all, invisible even to window-level instrumentation — while 500 ms,
  1 s and 2 s all work. The boundary matches an ordinary `NSWindow` fullscreen transition's
  duration. Every Cocoa application has this; a reader who hits the green button and reaches
  straight for a toolbar command is clicking inside that window.
- **The menu bar needing two clicks is macOS auto-hiding it in fullscreen**: a cold click at
  the top edge is spent on the reveal.
- **The phantom activation was a keyboard focus ring**, which in a screenshot is
  indistinguishable from a button that was just pressed. Confirmed by Escape moving the ring
  with no click involved. Assert on whether an action fires, never on what a screenshot
  looks like.

**A genuine toolkit defect was found on the way and is NOT this.** After a fullscreen
transition has fully settled, placing the pointer with `CGWarpMouseCursorPosition` — which
moves the cursor without posting a motion event — and then clicking makes the first click
skip the picked widget's own controllers entirely, while an ancestor's capture-phase gesture
still sees it and `pick()` resolves correctly. Reproducible with zero application code on
GTK 4.22.4 / macOS 27.0; `probes/macos-fullscreen-first-click.c` holds the measurement. No
mouse or trackpad gesture teleports a cursor, so no user meets it — it is recorded as an
upstream curiosity, not as this entry's subject.

**Dead ends, each closed by measurement, and not to be revisited**: a coordinate-space or
title-bar-origin offset; the macOS behaviour where the click that activates an inactive
window is not delivered; lost or mis-picked input; a stale implicit grab; this project's own
macOS pointer-crossing seam; a disabled action or insensitive widget; and any
ours-versus-upstream asymmetry — both binaries agree once entry and pointer arrival are held
constant. Returning to a fullscreen Space from another application is also clean.

**Linux and Windows have not been checked**, so the `Mac` narrowing is provisional — the
register's rule is that behaviour seen on one platform is not platform-specific until a
peer seat looks.

## CLSD-05. Syntax highlighting depends on `bincode` 1.3, which RustSec marks unmaintained

**Status**: Closed (the fix is syntect's to make, not ours; there is nothing to patch —
the advisory is informational, the crate is finished, and the only bytes it decodes are
compiled into the executable).

`syntect` 5.3.0 — the latest release, 2025-09-27 — deserialises its bundled syntax and
theme dumps with `bincode` 1.3.3 behind its `dump-load` feature, which this application
needs: every syntax set it highlights with (`syntect`'s own and `two-face`'s) is a
prebuilt dump embedded in the binary. RustSec advisory
[RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141) (2025-12-16) is
*informational: unmaintained* — the bincode team has ceased development and states that
1.3.3 is complete and needs no updates. There is no vulnerability and no patched version.

**Upstream**: [trishume/syntect#623](https://github.com/trishume/syntect/issues/623)
"`bincode` is unmaintained" (open, last activity 2026-04-04, checked 2026-11-10) and
[#606](https://github.com/trishume/syntect/issues/606) "Replacement for bincode as a
dep?" (open). Linked by the advisory's own package name and `cargo tree -i bincode`,
which shows `syntect` as its only dependant. The fix is syntect's: a release whose dump
format uses a maintained codec.

**Exposure**: none from a user's document. The only bytes `bincode` ever decodes here
are the dumps compiled into the executable; a document, image or theme file never
reaches it. The release procedure records this as a waiver (`deny.toml`'s ignore entry
cites this number) so the advisory check stays green on purpose rather than by
suppression.

**Why it's closed**: every exit runs through syntect. Replacing bincode ourselves means
forking syntect or regenerating its dump format with another codec — a maintained copy
of an upstream crate, which is a different project. Dropping `dump-load` means compiling
the syntax definitions at start-up (`yaml-load`), which brings back the `yaml-rust`
advisory the shipped graph was just cleared of and costs seconds per launch. Leaving
syntax highlighting out is not the product. With no vulnerability to patch, waiting on
upstream costs nothing. When a syntect release is off bincode 1, take it and delete this
entry and the `deny.toml` ignore in the same change.
