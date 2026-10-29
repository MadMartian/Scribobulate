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

| ID | Platform | Scope | Issue | Severity |
|----|----------|-------|-------|----------|
| A | Windows, Mac | Production | No screen reader on Windows or macOS can read the app's accessible names: our GTK builds leave GTK's AccessKit accessibility backend off, so neither platform gets a tree to carry them. Linux/AT-SPI reads them | Medium |
| B | Mac | Upstream | macOS only: every native file-chooser invocation (Open, Save, Export) grows RSS by ~1.1 MB and does not give it back. Roughly four fifths is AppKit's own price for presenting an `NSSavePanel` — reproduced with no GTK in the process — with about a fifth GTK-attributable. Caching the panel upstream would recover ~95% | Medium |
| D | Any | Production | The preview's Annotate bubble sits over the line above a selection, so a click there lands on the bubble: the first click of a double- or triple-click opens the comment card, the next press closes it, and the reader gets a single click | Low |
| F | Linux, Mac | Test | `window::sidebarfilter::gtk_integration_tests::closing_the_filter_returns_the_focus_to_the_highlighted_row` intermittently finds the list at its top instead of scrolled to the highlighted row (3 failures on CI: Linux twice, the second on 2026-10-29, and macOS once on 2026-10-29, both 2026-10-29 failures reading "value 0, upper 0, page 0"; passed on retry, and about 160 of 160 loaded runs locally) | Low |
| G | Windows | Upstream | After an edit the editor's scrollbar slider is sometimes not drawn until the next scroll (2 of 40 Enters); a GTK defect still open upstream | Low |
| Q | Windows, Linux | Upstream | A crafted SVG can corrupt memory: Windows and Linux decode SVG through a librsvg with a published use-after-free (CVE-2026-96889, fixed in 2.63.2), and refuse any SVG that may use XInclude before librsvg sees it until they carry the fix. The macOS bundle carries 2.63.2, and its build fails below that | High |
| R | Windows, Mac | Test | Two split-mode live-reload tests in `window::outline_reload_tests` intermittently fail on Windows CI because the editor is not where the test parked it (top line 0, or 70 where >100 is required) | Low |
| S | Any | Production | On a dark desktop the comment field keeps a white background while its "Add a comment…" hint takes the dark theme's light ink, so the hint is invisible (1.09:1 contrast). Typed text reads fine | Low |
| T | Mac | Upstream | Closing a full-screen window (Exit or ⌘W) logs `Gdk-CRITICAL gdk_surface_request_layout: assertion 'frame_clock' failed` once: AppKit's exit-full-screen transition resizes the already-destroyed surface, and GDK's resize handler does not check for that; no visible effect | Low |
| W | Windows | Production | Restoring a window's size at a fractional Windows display scale (125%, 150%) has never been run, so whether a window comes back at its saved size there is unknown | Low |

## Closed issues

Intractable: no reachable fix, and the limitation is still real and present.
Do not reopen one without a new constraint. These numbers never change and are never
reused — unlike the letters above, which are positional and get reclaimed.

| ID | Platform | Scope | Issue |
|----|----------|-------|-------|
| CLSD-01 | Any | Upstream | Tables are selection islands; cells are individually selectable but not part of the continuous buffer |
| CLSD-02 | Any | Upstream | A paragraph that mixes fonts (any inline-code span) can lay out a few pixels wider than the wrap width it was given, summoning the preview's Automatic horizontal scrollbar and intermittently blanking the pane until a resize |
| CLSD-04 | Mac | Upstream | In fullscreen, a click issued while the transition animation is still running is never delivered — AppKit blocks input for its own ~250-500ms window, in any Cocoa application. Not ours to fix, and not GTK's |


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

## D. The Annotate bubble covers the line above a selection and takes a click meant for it

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

## F. The sidebar-filter focus test sometimes finds the list unscrolled

**Severity**: Low (a test-only intermittent; no product symptom has been seen).

**Observed** (2026-10-24, GitHub CI, Linux coverage leg, step 5c, run 40101250787 attempt 1
on the commit "Offer the View ▸ Toolbar toggles from a right-click on the toolbar…", 2026-10-24): `closing_the_filter_returns_the_focus_to_the_highlighted_row` failed at
`src/window/sidebarfilter.rs:1113`, "the highlighted row is scrolled into view, not the top
of the list". Attempt 2 of the same run passed; so did the same commit inside run
40102254669 and 6 of 6 local runs. Windows and macOS legs passed both times.

**Not reproduced locally** (2026-10-24, Linux, GTK 4.6.9, instrumented build, private
Xvfb): about 160 runs under heavy parallel load, and 3 runs with animations off, all
passed. In each, the list was already laid out when the row was revealed and the scroll
landed near 900. Ruled out by those runs: a stale list size at the moment of the scroll,
and a close with animations off skipping the hand-back. Not established: the mechanism.

**Mitigation options**:
- The assertion's message now carries the list's scroll value, upper and page size. If
  it recurs, read them first: they say whether the reveal ran, ran against a stale size,
  or was undone afterwards. Change no product code before that.

**Recurred** (2026-10-29, GitHub CI run 40960989179, macOS leg, GTK 4.24.1, at
`src/window/sidebarfilter.rs:1156`): "value 0, upper 0, page 0", the same reading as the
second Linux failure. `upper 0` and `page 0` say the list had never been allocated when the
assertion read it, so the reveal had nothing to scroll; this is no longer Linux-only.

## G. On Windows the editor's scrollbar slider sometimes goes missing after an edit

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

## Q. Every SVG is decoded by librsvg, which has a published use-after-free

**Severity**: High (a crafted SVG can corrupt memory in the Windows and Linux builds wherever
the interim screen misses a route; a crash is the realistic worst case, code execution is not
ruled out).

**Report**: GHSA-wq5f-xc86-pv6w / CVE-2026-96889, fixed in librsvg **2.63.2**. Windows loads
2.62.3 (gvsbuild). Linux loads the distribution's: Ubuntu 22.04's 2.52.5 is "Not affected" per
Ubuntu's tracker (a vendor assessment we have not measured); other distributions are unassessed.
macOS from its bundle loads 2.63.2.

**Open because**: the vulnerable librsvg is still what Windows and Linux load. The interim
workaround — the XInclude screen ahead of every decode — is recorded, with every leak found in it
and what upstream must deliver to retire it, in [CVE.md](CVE.md) as **ScrSEC-1**. Read that entry
before changing the screen. This entry closes when ScrSEC-1 is marked Closed on both platforms.

**Not measured**: the screen's premise that librsvg follows no non-`data:` reference without a
base URL (the loader is fed a stream with none) is read from librsvg's design.

## R. Split-mode live-reload tests sometimes find the editor short of where they parked it on Windows CI

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

## S. On a dark desktop the comment field's hint text is invisible

**Severity**: Low (only the hint is unreadable; the field works and typed text is legible).

**Observed** (2026-10-26). macOS, GTK 4.22.4, dark desktop: in the comment card that
opens from **Annotate**, the field (`widgets/comment_entry.rs`, a `GtkTextView` in a
framed `GtkScrolledWindow`, with a `dim-label` placeholder laid over it) is white while
the card around it is dark. The placeholder "Add a comment…" measured 1.09:1 against it.
Same on `master`, so it predates the card-pointer change. Reproduced on Linux, private
Xvfb, GTK 4.6.9, `GTK_THEME=Adwaita:dark`: white field, near-invisible hint. Filed `Any`
because nothing in the field's construction is platform-specific; Windows has not been
driven.

**Not established**: which rule paints the field white under a dark theme. The
placeholder's ink follows the desktop theme, so either the field's fill or the hint's ink
is coming from somewhere other than the desktop theme. Read the field's CSS node tree with
the dark theme active before guessing.

## T. Closing a full-screen window logs a frame-clock critical on macOS

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

## W. A window's restored size at a fractional Windows display scale is unverified

**Severity**: Low (unverified, not observed broken; the worst case is a window restored larger or
smaller than it was saved).

On Windows the un-maximized size saved for a maximized window is converted between physical and
logical pixels by a scale the app measures from the window's client area
(`winplace/src/win32.rs`, `winplace/src/geom.rs`). Every Windows run so far was at 100%. MANUAL-TEST
15.10f at 125% and 150% has never been run: the `windows` seat does not change system settings, so
it needs the operator at a Windows desktop.

**Expected** (source reading only, not measured): GDK-Win32 4.22.4 reports an integer scale of
`dpi / 96` (`gdkdisplay-win32.c`, about lines 978-987), so 125% and 150% both read as scale 1 and
the measured scale should agree with it. A maximized window that is minimized at quit falls back
to GDK's integer scale, which is the one case where the two could differ if that reading is wrong.

**Mitigation options**:
- Run MANUAL-TEST 15.10f at 125% and at 150% on Windows, with one window maximized and minimized
  at quit. Delete this entry if both pass; otherwise the failing case becomes the fix's test.
- Accept the limitation until a user at a fractional scale reports a size change.

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
