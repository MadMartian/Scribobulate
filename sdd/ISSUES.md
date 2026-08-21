# Known Issues

**`Platform`** is one of **`Windows`** · **`Mac`** · **`Linux`** · **`Any`** — the platforms an
entry is known to affect, not where it was found. `Any` means reproduced on, or inherent to,
every platform; a named one means the others were checked and do not exhibit it. Before
narrowing an entry to a single platform, have that platform's peer seat fail to reproduce it
(POLICY § Verifying a change on macOS) — behaviour found on one platform is not
platform-specific until someone else looks.

**`Scope`** is one of **`Test`** · **`Production`** · **`Project`** · **`Upstream`**.
`Test` affects only the suite or the pipeline; `Production` affects what a user runs;
`Project` is both. **`Upstream` means the defect is in a third-party library and we cannot
FIX it** — a workaround may exist, but the repair is not ours to make, so an `Upstream`
entry is not work waiting to be scheduled here. It is orthogonal to severity: an `Upstream`
entry can still be the worst thing in the register.

| ID | Platform | Scope | Issue | Severity |
|----|----------|-------|-------|----------|
| A | Any | Upstream | Tables are selection islands; cells are individually selectable but not part of the continuous buffer | Closed |
| D | Any | Production | A `~~strikethrough~~` fence that wraps other inline markup (`~~a **bold** b~~`) renders the `~~` literally | Low |
| E | Linux | Production | A running instance doesn't repaint when the desktop switches dark↔light on KDE/X11; the new scheme only applies on restart | Low |
| G | Any | Production | A large document leaves the process spinning a CPU core at ~100% while idle — a GTK/Pango relayout pass that re-shapes text every main-loop iteration and never converges | High |
| N | Any | Production | A click that lands *inside* an existing preview selection never reaches the pane's own click affordances — the first click on a link/marker/checkbox under a selection does nothing | Low |
| O | Mac | Upstream | A GTK4/Quartz autorelease-pool crash SIGABRTs the macOS integration suite in about two runs in three, most often on the one focus-churning test | Medium |
| P | Any | Test | Coverage is still not fully host-independent: `theme.rs`'s search path walks the ambient `XDG_DATA_DIRS`, and the Windows config dir is unpinnable | Low |
| Q | Any | Test | Two wall-clock growth-ratio guards (tab normalisation, annotation extraction) go red on a loaded machine — the ratio is scheduler noise on a small baseline, not an exponent | Low |
| R | Mac | Production | macOS only, INTERMITTENT: the preview's hover cursor sometimes does not take over body text or a link, showing the default arrow; the drawn affordances that repaint on hover are always correct | Low |
| S | Mac | Upstream | macOS only: every native file-chooser invocation (Open, Save, Export) grows RSS by ~1.1 MB and does not give it back. Roughly four fifths is AppKit's own price for presenting an `NSSavePanel` — reproduced with no GTK in the process — with about a fifth GTK-attributable. Caching the panel upstream would recover ~95% | Medium |
| T | Windows | Test | The Windows pipeline port announces carve-outs it does not apply — the two bash ports now append `--skip`, so the first carve-out added makes Windows report a test as skipped that it ran | Low |

## A. Tables are selection islands

**Severity**: Closed (intractable — every exit is walled *within* GTK's selection
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
**does not exist**; see GTK4Rs/AP-28 / ScrAP-135.)*

**Severity stays Low and the limitation is accepted.** In-cell selection is already
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
free today, as the probe's control demonstrates — to un-break a Low-severity limitation
nobody has asked for. It would be a deliberate project chosen on product grounds, not an
increment, and it should not be started from this entry.

## D. A strikethrough fence wrapping other inline markup renders the `~~` literally

**Severity**: Low (rare authoring pattern; plain `~~struck~~` is unaffected)

Superscript (`^x^`), subscript (`~x~`) and strikethrough (`~~x~~`) are parsed by this
crate's own tight-syntax scanner (`renderer::scan_scripts`), because pulldown-cmark's
native versions use flanking rules that reject the tight Pandoc syntax authors type and
fragment multi-tilde lines (see ScrAP-66). The scanner runs **per pulldown
`Text` event**. A `~~ … ~~` fence whose content contains *other inline markup* —
`~~a **bold** b~~` — is split by pulldown into `"~~a "`, `Strong("bold")`, `" b~~"`, so
the two `~~` halves never meet in one scanned run and the fence is not recognised: the
`**bold**` still renders bold, but the surrounding `~~` show as literal characters and
no strike is applied. Plain `~~struck text~~` (no nested markup), the overwhelmingly
common case, renders correctly.

A related, rarer edge: `x\^2\^` (escaped literal carets) cannot be distinguished from
`x^2^` after pulldown has consumed the backslash escapes, so it renders as a superscript.

**Mitigation options**:
- **Coalesce adjacent inline events before scanning**: buffer consecutive `Text` (and
  re-emit nested `Strong`/`Emphasis`/etc.) within a paragraph and scan the reassembled
  run, so a fence can span nested markup. Non-trivial control-flow change to the renderer.
- **Accept the limitation (current state)**: nested markup inside strikethrough is rare;
  plain strikethrough and all tight super/subscript work. **TDD 10.10** makes the
  boundary explicit — it blesses the plain `~~struck~~` case as a contract and scopes
  the wrapping case (`~~a **bold** b~~`) OUT as this accepted limitation.

### Upstream bug — GTK GitLab filing recommendation

**Verdict (researcher-verified, 2026-06-30):** This is a **genuine GTK bug** — a wrong-predicate
oversight in `gtk_text_view_allocate_children`, not intentional design. Filing an upstream issue is
**justified and recommended** (moderate-to-good acceptance odds; fresh GNOME/GTK search found no
matching open issue). The bug is byte-identical from GTK 4.6.9 through current main (4.23.2).
Findings doc: `~/Documents/Projects/AI/Research/Gtk4Rust/gtk-issue-draft-anchored-container-grandchild-stale-paint.md`

**Keep the opaque-base workaround regardless** — even if the upstream patch lands it will not
backport to 4.6/4.8/etc.

<details>
<summary>Ready-to-file GTK GitLab issue text (paste as-is)</summary>

````markdown
**Title:**
GtkTextView: anchored *container* child's region is never invalidated when a *grandchild* changes (allocate_children gate uses self-only alloc_needed)

**Body:**

## Summary
When a widget is anchored in a GtkTextView via a child anchor, and that anchored widget is a *container*, a visual change to one of its *descendants* (e.g. `gtk_label_set_attributes()` on a grandchild label) is not repainted. The stale pixels remain until an unrelated event forces the view to re-validate the line (a scroll, or any buffer mutation). Anchoring the styled widget directly (so it is the anchored widget itself, not a grandchild) does NOT exhibit the bug — which pinpoints the cause.

## Steps to reproduce
1. Build and run the minimal C program below.
2. Click "Set BG" → a yellow background appears behind CELL TEXT.
3. Click "Clear BG" → `gtk_label_set_attributes(label, NULL)` clears the model, but the yellow background **stays painted**.
4. Scroll the GtkTextView, or type into the buffer → the yellow finally disappears.

**Expected**: clearing the attributes repaints the anchored region immediately (step 3).
**Actual**: the region is not invalidated; the old paint persists until step 4.

*Diagnostic asymmetry*: if you anchor the `GtkLabel` directly (replace the `GtkBox cell` with the `label` itself), the bug disappears — the region repaints correctly. The bug only appears when the restyled widget is a *descendant* of the anchored widget.

## Minimal reproducer (C)

```c
/* Build: cc stale-anchor-bg.c -o stale-anchor-bg $(pkg-config --cflags --libs gtk4)
 * Run:   GSK_RENDERER=cairo ./stale-anchor-bg
 */
#include <gtk/gtk.h>

static GtkWidget *label;

static void set_bg (GtkButton *b, gpointer u) {
  PangoAttrList *al = pango_attr_list_new ();
  pango_attr_list_insert (al, pango_attr_background_new (65535, 65535, 0)); /* yellow */
  gtk_label_set_attributes (GTK_LABEL (label), al);
  pango_attr_list_unref (al);
}

static void clear_bg (GtkButton *b, gpointer u) {
  gtk_label_set_attributes (GTK_LABEL (label), NULL); /* <-- background stays painted */
}

static void activate (GtkApplication *app, gpointer u) {
  GtkWidget *win = gtk_application_window_new (app);
  gtk_window_set_default_size (GTK_WINDOW (win), 400, 300);

  GtkWidget *box    = gtk_box_new (GTK_ORIENTATION_VERTICAL, 6);
  GtkWidget *btnset = gtk_button_new_with_label ("Set BG");
  GtkWidget *btnclr = gtk_button_new_with_label ("Clear BG");
  GtkWidget *tv     = gtk_text_view_new ();

  GtkTextBuffer *buf = gtk_text_view_get_buffer (GTK_TEXT_VIEW (tv));
  GtkTextIter it;
  gtk_text_buffer_get_start_iter (buf, &it);
  gtk_text_buffer_insert (buf, &it, "Anchored container child below:\n", -1);

  /* Anchored CONTAINER child; the grandchild label is what we restyle. */
  GtkTextChildAnchor *anchor = gtk_text_buffer_create_child_anchor (buf, &it);
  GtkWidget *cell = gtk_box_new (GTK_ORIENTATION_HORIZONTAL, 0);
  label = gtk_label_new ("CELL TEXT");
  gtk_box_append (GTK_BOX (cell), label);
  gtk_text_view_add_child_at_anchor (GTK_TEXT_VIEW (tv), cell, anchor);

  g_signal_connect (btnset, "clicked", G_CALLBACK (set_bg),   NULL);
  g_signal_connect (btnclr, "clicked", G_CALLBACK (clear_bg), NULL);

  gtk_box_append (GTK_BOX (box), btnset);
  gtk_box_append (GTK_BOX (box), btnclr);
  gtk_box_append (GTK_BOX (box), tv);
  gtk_window_set_child (GTK_WINDOW (win), box);
  gtk_window_present (GTK_WINDOW (win));
}

int main (int argc, char **argv) {
  GtkApplication *app = gtk_application_new ("org.example.staleanchorbg",
                                             G_APPLICATION_DEFAULT_FLAGS);
  g_signal_connect (app, "activate", G_CALLBACK (activate), NULL);
  int r = g_application_run (G_APPLICATION (app), argc, argv);
  g_object_unref (app);
  return r;
}
```

## Root cause

`gtk_text_view_allocate_children()` (`gtk/gtktextview.c`) invalidates the layout line around an anchor only when the anchored child reports it needs allocation:

```c
/* gtk/gtktextview.c — main (4.23.2): line 4574  |  4.6.9: line 4431 */
if (_gtk_widget_get_alloc_needed (child->widget))
  {
    GtkTextIter end = child_loc;
    gtk_text_iter_forward_char (&end);
    gtk_text_layout_invalidate (priv->layout, &child_loc, &end);
  }
```

`_gtk_widget_get_alloc_needed()` returns only the widget's **own** flag:

```c
/* gtk/gtkwidget.c — main: 11037  |  4.6.9: 10525 */
_gtk_widget_get_alloc_needed (GtkWidget *widget) { return widget->priv->alloc_needed; }
```

But a change to a descendant marks `alloc_needed` only on **that descendant**; ancestors receive only `alloc_needed_on_child`:

```c
/* gtk/gtkwidget.c — main: 11045  |  4.6.9: 10533 */
gtk_widget_set_alloc_needed (GtkWidget *widget) {
  widget->priv->alloc_needed = TRUE;
  do {
    if (priv->alloc_needed_on_child) break;
    priv->alloc_needed_on_child = TRUE;   /* ancestors get THIS, not alloc_needed */
    ...
    widget = priv->parent;
    ...
  } while (TRUE);
}
```

So when `gtk_label_set_attributes()` (which `queue_resize`s the label) runs on a grandchild, the anchored container `child->widget` ends up with `alloc_needed == FALSE` (only `alloc_needed_on_child == TRUE`). The gate is skipped, the layout line is never invalidated, the anchor's cached display line is reused, and the region is never re-snapshotted — hence the stale paint. When the anchored widget IS the restyled label, its own `alloc_needed` is TRUE, the gate fires, and the region repaints correctly (the diagnostic asymmetry above).

## Suggested fix (one line)

Use the descendant-aware predicate `gtk_widget_needs_allocate()` — which already ORs in `alloc_needed_on_child` — in place of the self-only `_gtk_widget_get_alloc_needed()` at the gate:

```c
/* gtk/gtktextview.c, gtk_text_view_allocate_children
 * main (4.23.2): line 4574  |  4.6.9: line 4431 */
- if (_gtk_widget_get_alloc_needed (child->widget))
+ if (gtk_widget_needs_allocate (child->widget))
```

`gtk_widget_needs_allocate()` is defined a few lines from the getter the gate currently uses (`gtk/gtkwidget.c` — main: 11077, 4.6.9: 10561) and already returns `resize_queued || alloc_needed || alloc_needed_on_child`.

*Trade-off*: this will invalidate+revalidate the anchor line whenever any descendant of an anchored **container** needs allocation (marginally more layout work for container anchors), but that is exactly the condition under which the region must be re-validated.

## Affected versions

Verified by source reading that the gate and the `alloc_needed` / `alloc_needed_on_child` machinery are identical from **4.6.9 through current main (4.23.2)**. Expected on all of 4.6 / 4.8 / 4.10 / 4.12 / 4.14 / 4.16 / 4.18 / 4.20 / 4.22 and dev.

## Caveat

The suggested fix is a code-reading proposal — it has not been compiled or runtime-tested. The root-cause analysis and the reproducer are verified; the one-line change is offered for a maintainer to validate.
````

</details>



## E. Live desktop dark↔light toggle doesn't repaint a running instance on KDE/X11

**Severity**: Low (cosmetic; the new scheme applies on the next launch, and users
rarely retheme mid-session — but a visible inconsistency with libadwaita/Qt apps,
which *do* update live)

With the app's reading theme left on **System**, switching the KDE global colour
scheme from dark to light (or back) while an instance is running leaves that
instance's chrome unchanged; the new scheme is only picked up when a fresh instance
launches. Confirmed on the operator's live KDE/X11 session (2026-07-19): a running
window stayed dark across a desktop dark→light switch, while a freshly-launched
instance rendered light. Native GNOME/libadwaita and Qt/KDE apps repaint live under
the same toggle, so the gap is app-visible.

**Root cause is a propagation-channel gap, not a missing feature.** The app already
wires a live-update path: it subscribes to `GtkSettings` `gtk-application-prefer-dark-theme`
and `gtk-theme-name` notifications and, on either, re-probes the desktop colours and
re-renders every window. The failure is upstream of that handler — KDE-on-X11 does
**not** push a live colour-scheme change through the **XSettings** channel that backs
those `GtkSettings` properties, so the notify never fires on a running client (a fresh
launch reads the current value once, which is why restart works). The apps that update
live listen on a *different* channel: the XDG desktop portal signal
`org.freedesktop.portal.Settings` → `SettingChanged("org.freedesktop.appearance",
"color-scheme")` (libadwaita's `AdwStyleManager` watches exactly this). Scribobulate is
pure gtk4-rs (no libadwaita), so it isn't listening there.

**The detection channel is now researcher-confirmed (2026-07-19, empirically on the
operator's own KDE/X11 box).** `xdg-desktop-portal-kde` 5.27.11 is installed, running,
and routed (`kde-portals.conf`: `org.freedesktop.impl.portal.Settings=kde`); a direct
`gdbus call … Settings.Read org.freedesktop.appearance color-scheme` returns the live
value. Its source derives `color-scheme` from Qt's `paletteChanged` (no X11/Wayland
branch) and `Q_EMIT`s `SettingChanged`, so the scheme rides the **portal**, exactly
what libadwaita/Qt watch and what our `GtkSettings`/XSettings handler cannot see. The
signal is `SettingChanged(namespace s, key s, value v)` on the session bus at
`org.freedesktop.portal.Desktop` `/org/freedesktop/portal/desktop`, iface
`org.freedesktop.portal.Settings`; value maps `0=no-preference, 1=dark, 2=light`. A
version gotcha for the *initial* read: `Read` double-wraps the variant and `ReadOne`
(single-wrap) exists only at interface **version ≥ 2** (xdg-desktop-portal ≥ 1.18);
the operator's box is version 1, so the initial read needs a double-unwrap (the
`SettingChanged` signal is single-wrapped on every version).

**Two gates remain before the fix can be committed (both need the operator's live
session — a `gdbus monitor --session --dest org.freedesktop.portal.Desktop` while
flipping the scheme):**
1. Confirm the live *push* actually fires (the static `Read` only proves the value is
   readable; the go/no-go is watching `SettingChanged` arrive on the toggle).
2. **Whether the app's existing re-render even reflects the new scheme once the signal
   fires.** The desktop-dark truth comes from `palette::desktop_is_dark()`, which
   probes `GtkStyleContext` colours — and on KDE/X11 *those are also stale* on a live
   flip (GTK never reloads the KDE theme without the XSettings/portal push). So the
   portal handler must likely use the signal's `color-scheme` value **directly** as the
   desktop-dark truth (overriding the stale probe) rather than re-probing; and the
   GTK-themed chrome (editor/toolbar, rendered by the KDE Breeze GTK CSS) may still not
   follow live without forcing a GTK theme reload — the reading-area palette we control
   can, the base-theme chrome is the open question. Confirm on the live session which
   surfaces actually switch before deciding whether the fix is "preview follows" or
   "everything follows".

**Mitigation options**:
- **Subscribe to the XDG desktop-portal `color-scheme` signal** (gio `DBusProxy` on the
  session bus, no new deps) alongside the two existing `GtkSettings` subscriptions, and
  on change drive the desktop-dark truth from the signal value + re-render. Gate on the
  two live checks above.
- **Accept the limitation**: the scheme applies on the next launch, users seldom
  retheme mid-session, and nothing is broken — only slower to follow than native apps.

## G. A large document pegs a CPU core at ~100% while idle (GTK/Pango relayout loop that never converges)

**Severity**: High (the symptom is a full CPU core held at ~100% **indefinitely while idle**,
which directly contradicts the product's negligible-footprint thesis — but it is gated to
LARGE documents, tens of thousands of lines; typical small files are unaffected. Agent-generated
reports and plans, the product's own primary use case, can be that large, so it is reachable in
normal use rather than a corner case.)

Opening a large document leaves the process at ~100% CPU **forever, even after it is fully
rendered and sitting idle with no input**. Characterised headless (Xvfb, release build of
`442ae1f`) on `tests/fixtures/large-doc.md` (3 MB / 41,785 lines): `pidstat` averaged **99.83%**
across a 60 s idle window (20 samples, all ~100%, process alive throughout). A normal document
(`tests/fixtures/lists.md`) opened the same way idles at **~0%**, isolating the spin to document
size.

**Second consequence, established 2026-08-07.** While the layout is invalid, GTK keeps its
incremental line-height validation idle permanently ready, and that starves anything the app
schedules below it. Far navigation (Ctrl+Home/End, Go To Line, find, outline) is deferred until
validation completes for correctness reasons (ScrAP-260), so on a document caught in this spin
that navigation would never arrive at all. It is bounded rather than exposed — the deferral
carries a timer-based deadline above the validate idle's priority, which degrades to a partial
landing instead of hanging — but that mitigation exists *because of this issue* and would be
unnecessary without it. Measured counter-point: a 200 000-line plain-prose file settles to 0 %
CPU in ~30 s and does **not** reproduce the spin, so whatever drives it is not size alone.

Surfaced during the macOS-port bring-up, where a stack sample suggested a GtkSourceView
incremental-highlighter feedback loop (its progress `mark-set` re-dirtying the highlighter's own
region). Confirmed here to reproduce on Linux — so it is **not platform-specific** — but an
independent trace on this side **does not support the highlighter theory**.

**Trace** (gdb `thread apply all bt` on the spinning process, main thread, at idle). Every
worker thread is parked (futex / `cond_wait`); the hot main thread is entirely in text SHAPING
under a recursive GTK measure/layout pass:

```
#0–7   libharfbuzz   hb_shape_plan_execute / hb_shape_full          (text shaping)
#8–14  libpango      pango_shape_item + layout
#15–24 libgtk-4      measure / allocate / snapshot   (frames #18–22 are ONE return address ×5 → recursive widget-tree measure)
#25–27 glib          g_main_context_dispatch → g_main_context_iteration
#28    gio           g_application_run → main
```

There are **no GtkSourceView / highlight / mark / region frames anywhere**, and **no app-own
frames in the hot path**. So the CPU burns in a GTK **relayout / re-shape loop that never
converges** — a `size_allocate` / `queue_resize` pass re-shaping the large widget tree's text
via Pango/HarfBuzz on every main-loop iteration — not the incremental highlighter.

**Symptom vs driver — not yet fully pinned.** One stop-sample shows *where* the CPU is spent
(Pango shaping under GTK measure), not *what keeps scheduling* the pass. The macOS-side
`mark-set`-handler theory therefore survives only as a candidate **driver**: a handler that
re-`queue_resize`s in response to a signal the relayout itself emits would produce exactly this.
The buffer's `mark-set` is listened for in four places — `window/tabs/lifecycle.rs` (`:95`),
`window/editbar/overlay.rs` (`:155`), `preview/interactions.rs` (`:23`),
`preview/annotate/overlay.rs` (`:843`) — which are the suspects to bisect. (Several already
coalesce because `mark-set` is chatty, so the culprit is more likely a coalescing timer that
keeps re-arming than a naive re-dirty.)

**Distinct from F** (same *preview-overlay relayout* family, opposite outcome): F is a **rare,
recoverable blank** from a `GtkOverlay` snapshotted without an allocation; this one is a
**permanent ~100% CPU spin** with no blank. (Both entries previously called each other N and O
— letters that no longer name them.)

⚠ **The `GTK_DEBUG=geometry` probe both entries recommended CANNOT RUN on the reference
host, and its silence is not evidence.** Measured 2026-08-04: a distribution GTK is built
without debug support, so every informational `GTK_DEBUG`/`GDK_DEBUG`/`GSK_DEBUG` key reports
`[unavailable]` and emits nothing — an empty log therefore means *the instrument is dark*, not
*no widget re-queued a resize* (ScrAP-251). Restoring that key requires a locally built,
debug-enabled GTK loaded ahead of the distribution one; `sdd/PLAN.profiling.md` records the
cost and the alternatives.

**Mitigation options**:
- **Root-cause the driver** (recommended; not yet done): take several samples to confirm the
  loop consistently sits in shaping/layout — `perf record` against the unstripped debug binary
  gives named application frames today, with no change to the tree, and is the substitute for
  the unavailable geometry key; then bisect the four `mark-set` handlers by
  disabling each and re-measuring idle CPU. If one stops the spin, that handler is the driver;
  if none does, the driver is not `mark-set`, and the search moves to whatever re-invalidates
  the (likely preview) widget tree's layout every iteration.
- **Likely fix shapes** (pending the driver): make the offending handler idempotent so it does
  not re-invalidate the region it reacts to; coalesce/gate the relayout so it converges; or
  ensure an incremental idle returns `G_SOURCE_REMOVE` once stable. Left open deliberately —
  fixing the wrong layer (e.g. throttling shaping) would mask the loop rather than end it.
- **Accept the limitation**: not viable long-term — an idle full-core spin on the product's own
  primary use case (large agent-generated documents) defeats the negligible-footprint thesis the
  project exists to honour.

## N. A click inside an existing selection never reaches the preview's click affordances

**Severity**: Low (one wasted click, self-correcting — the click clears the selection and
the next one works; no data at risk)

With text selected in the preview, a primary click whose press lands **inside** that
selection does not activate the affordance under it: a link does not open, a margin
comment marker does not open its card, a gutter checkbox does not toggle, a code block's
copy button does not copy. The click clears the selection instead, and a second click
behaves normally.

**Measured, and pre-existing.** Reproduced identically on the binary from before the
complete-click fix (ScrAP-238) and on the one after, under Xvfb: press+release on a
fragment link with a selection covering it produced no scroll on either build, while the
same click with nothing selected navigated on both. So the seam that now pairs press with
release neither introduced nor addresses it.

**Cause — measured, then traced to the two lines that implement it.** Instrumented build,
Xvfb: on the swallowed click the app's gesture receives `pressed` and then **nothing** — no
`released`, and no `cancel` either, which is why it fails silently.

`gtk_text_view_click_gesture_pressed` (`gtktextview.c`, GTK 4.6.9) handles a single
non-touch press whose iter lies inside the selection by claiming the sequence for its own
drag gesture — *"Claim the sequence on the drag gesture, but attach no selection data,
this is a special case to start DnD"* — unconditionally, not gated on the view being
editable. A claim denies every other gesture handling that sequence, and
`gtk_gesture_click_end` (`gtkgestureclick.c`) emits `released` only
`if (current == sequence && state != GTK_EVENT_SEQUENCE_DENIED && interpreted)`. DENIED is
terminal (the same arbitration wall issue A documents), and it is not a cancellation, so
no `cancel` fires to tell the app anything happened.

So the release is not late or misrouted — GTK deliberately withholds it, and the app's
affordance cannot observe the click at all.

**Mitigation options**:
- Claim the sequence ourselves on a press that lands on an affordance — **priced and not
  recommended**. It is the only way to out-rank GTK's claim, and it buys one click at the
  cost of the case it steals from: a press on a link is then no longer available to start
  a drag, so dragging the selection (GTK's DnD) from a point that happens to sit over a
  link stops working, and — since intent is unknowable at press time — a press that does
  turn into a drag would end in nothing happening at all, which is worse than today. The
  press cannot be disambiguated at the moment the decision must be made.
- Leave it, which is the standing choice: the failure is one wasted click that fixes
  itself, and every route past it trades a silent no-op for a silent loss of the drag.


## O. A GTK4/Quartz autorelease-pool crash intermittently SIGABRTs the macOS integration suite

**Severity**: Medium (the macOS GTK suite cannot be trusted to complete; no data at risk,
and no Scribobulate code is implicated — but a red run there means nothing until re-run)

`cargo test --features gtk-integration-tests --test gtk_suite` intermittently aborts the
whole test process on macOS. Not one specific test — whichever happens to trigger a
text-view mark-set at the wrong moment relative to macOS's input-method state.

**Measured** via four independent **Apple crash reports** — the system crash reporter, not
a Rust panic (`termination: {namespace: OBJC, flags: 646, code: 1}`) — across four separate
runs. That distinction is the one that matters diagnostically: the Rust harness reports
only that the process died, which is equally consistent with a defective test, and the
discriminating evidence exists only at OS level. All four stacks are identical in
signature:

```
gtk_text_view_mark_set_handler                                   (libgtk-4.1.dylib)
  -> discard_preedit                                             (libgtk-4.1.dylib)
  -> +[NSTextInputContext currentInputContext_withFirstResponderSync:]   (AppKit)
  -> TSM input-method session (de)activation                     (HIToolbox)
  -> nested CFRunLoop pump for NSPasteboard promised-data resolution
  -> objc_autoreleasePoolPop -> AutoreleasePoolPage::busted_die() (libobjc.A.dylib)
```

**No Scribobulate frame appears anywhere in the faulting stack.** The cause is GTK4's
Quartz backend firing `discard_preedit` on any `GtkTextView` mark-set — i.e. on any caret
or selection change — which activates/deactivates the macOS input-method bridge, which
pumps a nested run loop for pasteboard-promise resolution and corrupts the autorelease
pool stack. Not reachable from application code.

**Rate and distribution** (n=6 full-suite runs, isolated): **4 crashed, 2 clean — about
two in three.** Of the 4 crashes, **3 were on the same test**,
`select_all_stands_down_for_every_text_entry_and_recovers_for_the_editor`; the single
outlier was the first observation, which was also a contaminated run (a concurrent build
on the same machine). All 4 crash reports carry a byte-identical stack signature.

**Why this is still not filed against that test**, even though it is the dominant trigger
— the argument is the stack, not the distribution. No application frame appears in it, so
nothing in that test's *code* is faulting; what the test does is arrive at the toolkit
path more often. It exists to verify select-all standing down across *every* text entry,
so its body is mostly rapid focus-switching between entries — which is precisely what
drives `discard_preedit` / `NSTextInputContext` activation churn. A test that exercises
the mechanism hardest crashing most is consistent with the mechanism, not evidence of a
defect in the test.

> **An earlier version of this entry claimed the opposite and was wrong.** On n=3 it read
> "2 crashed on *different* tests, so a defective test would fail on the same one every
> time" — and offered that distribution as the load-bearing proof. A larger sample
> inverted it: the spread was an artefact of a small n whose one cross-test data point
> came from the contaminated run. The mechanism argument survived unchanged because it
> never rested on the distribution; the distribution argument did not. Recorded because
> the retracted reasoning is more instructive than the correction — a frequency pattern
> read off three samples is a hypothesis, and it was stated here as evidence.

**Unverified**: whether it reproduces on other macOS or GTK versions. Linux and Windows
have no equivalent Quartz/AppKit/TSM path, so it is plausibly macOS-only *by
construction* — but that is an argument, not a test result.

**Mitigation options**
- **Re-run and treat a single abort as inconclusive** — what the macOS seat does today.
  Cheap, but it means the suite's silence is weaker evidence there than on Linux.
- **Raise it upstream** with the two crash reports. The stack is specific enough to be
  actionable and nothing about it is project-specific.
- **Re-test on a newer GTK** when one is available; this is the kind of interaction an
  upstream fix moves without anyone here doing anything.

Measured on macOS 26.6.1 (25G76), GTK 4.22.4 (Homebrew), by the macOS seat. Primary
evidence is machine-local and not transferable:
`~/Library/Logs/DiagnosticReports/gtk_suite-9047c36e3af692e9-2026-08-07-232935.ips` and
`…-2026-08-08-015722.ips`.

---

## P. Two residual paths still take their coverage from the host's environment

**Severity**: Low (no user-visible effect; it degrades a *gate* rather than the app)

The main instance is fixed: `.cargo/config.toml`'s `[env]` now pins `XDG_CONFIG_HOME` to a
scratch directory, `Config::load()` carries a mutation-tested assertion that it is set, and
the two-host differential that exposed this (`scripts/coverage.sh` against the same run with
a scrubbed config dir) is now byte-identical where it used to differ by four lines.

**Two paths remain, and they are the same defect wearing different variables:**

1. **`theme.rs`'s search path**, measured at 3 lines. `find_themes_file()` walks
   `system_data_dirs()` — `XDG_DATA_HOME`, then each entry of `XDG_DATA_DIRS` — so how much
   of that loop executes depends on how many data directories the host advertises. A
   developer box reports more than a hosted runner, so those lines are covered here and not
   there. **Do not fix this by pinning `XDG_DATA_DIRS`**: it is how GTK finds icon themes and
   GSettings schemas, and pinning it would break icon resolution across the integration
   suite. The right fix is a deterministic unit test over the path assembly, which means
   making the directory list a parameter rather than an ambient read.
2. **The Windows config directory.** `config_home_fallback()` resolves through `APPDATA`
   there, which is not pinned and should not be — it is a directory the whole toolchain
   uses. The behavioural hazard (a test reading the developer's real `config.toml`) therefore
   persists on Windows, where the assertion in `Config::load()` is deliberately `unix`-gated
   so it cannot fire on a correct run. Coverage is unaffected there, since step 6 is
   contract-declared non-applicable on Windows.

**The general form, which is the part worth keeping**: coverage produced by the *ambient
environment* rather than by a test is false comfort. It looks like tested code and is
nobody's assertion, so it silently moves with the host — and pinning a variable only
relocates the accident. `config_home_fallback()` illustrates the trade honestly: pinning
`XDG_CONFIG_HOME` made its three lines *uncovered* on every host, which is worse-looking and
strictly more truthful, because nothing ever tested them. A deterministic test is the real
answer in all three places.

**Consequence for the ratchet — now bounded rather than tracked**: `FLOOR` is a whole
number and advances one whole point at a time (POLICY step 6). That is deliberately wider
than this entry's residual: ~0.02pt cannot move a threshold quoted in points, so the two
paths above no longer put the gate at risk of a false red. They still cost something real —
the floor cannot rise to the next integer until coverage clears it *with margin on every
host*, and a residual that moves the figure is exactly what eats that margin — so closing
them is still worth doing. It is no longer urgent. `scripts/coverage.sh` is the source of
truth for the value and carries the arithmetic; ScrAP-123 carries the lesson.

---

## Q. A complexity guard measures an exponent with a wall clock, and flakes on a loaded machine

**Severity**: Low (a false red, never a false green — it cannot hide a regression, only
invent one; but a required pipeline step goes red on a correct tree, and the failure text
accuses a specific, already-fixed regression by name, so the next reader starts by
re-auditing code that is fine)

`renderer::normalize`'s `tab_normalisation_over_a_single_enormous_line_grows_linearly`
times normalisation of 128 KiB and 512 KiB inputs and asserts the ratio is `< 8.0`, on the
reasoning that the *exponent* is the property that regressed and a ratio is
machine-independent where a wall-clock bound is "either flaky or blind". The reasoning is
right about the property and wrong about the immunity.

**Measured, on two hosts of the same platform:**

- *Hosted Linux runner*: `8.7x (6.32ms -> 54.918ms)`, failing the pipeline at step 4. The
  same commit passes locally 5 runs out of 5, and a second CI run of the identical tree
  **passed** — so it is intermittent rather than a property of that machine. The tell is in
  the absolute numbers: the runner's small sample is ~6.3 ms where this box is well under a
  millisecond, so CI is not merely noisier, it is roughly an order of magnitude slower per
  byte. A ratio taken from a 6 ms denominator on a shared vCPU has scheduling jitter of the
  same order as the signal it is measuring.
- *Reference host*: one failure in 40 consecutive `scripts/coverage.sh` runs, 2026-08-09 —
  `normalisation grew 8.1x for 4x the input (4.814539ms -> 38.939987ms)`. Same shape, much
  rarer. The instrumented (llvm-cov) build widens the window further.

**A ratio is machine-independent only while both samples sit in the same regime.** Two
plausible contributors, and they are not exclusive: scheduler preemption on a shared core,
and the 512 KiB input (plus an equal-sized output) crossing a cache boundary the 128 KiB one
does not — on a smaller-cache machine the larger sample pays a per-byte penalty the smaller
never sees, and the ratio inflates with the exponent unchanged.

Not the same defect as the SIGSEGV that used to share this symptom (a coverage run going
red about one time in three); that one was the fatal-signal handler outliving its own
test and is fixed — see ScrAP-265. This is the residue that reproduced *instead* of it.

**Options, roughly in order of honesty:**

1. **Count operations, not time.** The property is algorithmic, so measure the algorithm —
   instrument the line walk with a step counter and assert *that* grows linearly.
   Machine-independent by construction rather than by hope. Costs a counter reachable from
   tests.
2. **Move both inputs outside cache** (e.g. 4 MiB and 16 MiB) so both are bandwidth-bound and
   the ratio reflects the exponent again. Simple; costs run time.
3. **Best-of-N per size.** Timing noise is one-sided, so a minimum is the robust estimator.
   Helps with preemption, does nothing about a cache-regime difference.
4. **Widen the threshold.** Cheapest and the worst: 8.7 already sits between linear (~4x) and
   quadratic (~16x), so widening spends the discrimination the guard exists for.

Whichever is taken, take it for both: the same shape is used by at least one sibling guard
(`annotate::scan`'s), so the choice is not local to this test.

Do **not** simply delete it — it guards a real regression that has happened once (a per-tab
backwards line walk, pre-fix cost measured in tens of seconds). The companion absolute
ceiling (3 s for 512 KiB) still holds and is not implicated.

**Workaround**: re-run. A ratio between 8 and roughly 10 on an otherwise-green suite is
this issue; a genuine return of the quadratic walk reads ~16x and does not come and go.

**The sibling guard has now been measured failing too, and it is already best-of-N**
(2026-08-21, reference host, `cargo test` step 4 of a routine pipeline run):
`annotate::scan::tests::extraction_over_adversarial_input_grows_linearly_not_quadratically`
went red once and passed on an immediate isolated re-run of the same binary. That matters
for the choice above, because it is evidence *against* option 3 — the annotation guard
already takes the best of N repetitions per sample, and a preemption still moved the ratio
across the threshold, so best-of-N narrows the window rather than closing it. Options 1 and
2 are unaffected by this measurement, and counting operations is still the only one that
stops being a timing test.

---

## R. macOS: the preview's hover cursor sometimes does not take over body text or a link

**Severity**: Low (nothing is unreachable — links still activate on click and the copy
button still copies; what is lost is the *affordance*, i.e. every link on the page looks
un-clickable to a macOS reader until they try it)

**READ THIS FIRST: it is INTERMITTENT, and that was established late.** The table below is
what was measured, and every cell of it is real — but a later cold launch, same fixture,
same point, same procedure, produced the *correct* text-beam where two earlier independent
cold launches had produced the arrow. So the macOS column is not reliably reproducible, and
any theory built only on the table (including the two this entry has already discarded)
will over-explain it. Whatever gates this is not captured by "cold versus warm", and is not
named here because it is not known.

MEASURED on macOS (GTK 4.22.4/Quartz/Homebrew) against the identical build on Linux
(GTK 4.6.9/X11), same fixtures, cursor identity read by name rather than judged from a
screenshot — `XFixesGetCursorImage` on X11, screenshots on Quartz:

| pointer over | asks for | Linux/X11 | Windows/GDK-Win32 | macOS/Quartz |
|---|---|---|---|---|
| plain body text | `text` | `text` | IBEAM | **arrow** |
| a body link | `pointer` | `pointer` | HAND | **arrow** |
| a task checkbox | `pointer` | `pointer` | HAND | hand |
| a code-block copy button | `pointer` | `pointer` | HAND | hand |

**Three platforms, and macOS is the outlier** — Windows read via `GetCursorInfo().hCursor`
against `LoadCursorW(NULL, IDC_*)` handles after a settle, with the copy-button cell
corroborated by accent-pixel count so the pointer was provably on the button when the
handle was sampled.

**All four go through one `set_cursor_from_name` call in one motion handler**
(`preview::interactions::wire_link_gestures`); only the string differs. So this is not
about links, and not about any hit-test: two of the four cells take effect on Quartz and
two do not.

**Ruled out, measured, not assumed.** The first hypothesis was that GTK's tooltip resets
the cursor — a link is the only one of the four that makes `query-tooltip` return true and
put a window on screen. Sampled at ~200 ms (pre-tooltip) and ~2000 ms (tooltip visibly up),
same pointer position, no re-entry: **arrow both times**. The tooltip is innocent; the
cursor never takes at all on those two cells.

**Standing hypothesis, NOT confirmed, and now bounded to one backend.** The only
structural difference between the working and failing cells is that the checkbox and
copy-button branches also `queue_draw()` when the hovered identity changes, while the link
and plain-text branches draw nothing — suggesting a cursor set via `gtk_widget_set_cursor`
does not take effect until something invalidates the surface. **The Windows result refutes
the general form**: there the two "draw nothing" cells get their cursors, and get two
*different* ones (IBEAM and HAND), so this is not a property of GDK backends at large. If
it holds anywhere it holds on Quartz alone.

**TWO HYPOTHESES HAVE BEEN TRIED AND NEITHER SURVIVED. Do not re-derive them.**

1. *The tooltip resets the cursor* — a link is the only cell that makes `query-tooltip`
   return true. Refuted by sampling at ~200 ms (pre-tooltip) and ~2000 ms (tooltip visibly
   up): arrow both times.
2. *The cursor does not take until the surface is repainted, so only the cells whose hover
   fires a `queue_draw` work* — which fitted every observation at the time, including the
   original three-way result (run with a fresh instance per target, so the "working" cells
   are exactly the ones that repaint themselves). Refuted twice over: on Windows the two
   "draw nothing" cells get their cursors and get two *different* ones, so it is not a GDK
   property; and the motion-only test that would have confirmed it on Quartz — approach one
   spot of plain text from other plain text versus straight off the copy button — returned
   the correct text-beam **both** ways, on an instance where plain text had already started
   behaving before the test began.

**What is left is the intermittency itself, and it is not yet characterised.** A cold
launch has produced the arrow twice (independently, same instance) and the correct
text-beam once (a different instance), with no known difference in procedure. Until that
reproduces on demand there is nothing to hand a researcher: a mechanism proposed against an
observation this unstable will fit and still be wrong, which is how both hypotheses above
were reached. **The next useful step is a reproduction rate, not a theory** — the same cold
launch and single hover, repeated enough times to say how often it bites, which turns an
anecdote into something a mechanism can be tested against.

**Pre-existing, and older than the affordance that exposed it.** Nothing here was
introduced by the code-block copy button; that button is one of the two cells that *works*,
and the pattern only became visible once an affordance existed that happens to repaint on
hover. Do not file it against that change.

**Dead end, so it is not re-attempted.** Forcing a repaint through the macOS menu bar
failed three ways: keyboard-only menu navigation quit the application partway through;
`System Events` `click menu item` on the theme's leaf entry failed `-1728` reproducibly
with and without its emoji prefix, on a fresh instance, *while querying that same item's
name succeeded*; and a raw-coordinate fallback both moved the pointer (which the test
forbids) and missed. A theme switch would also have been ambiguous even if it had landed —
it re-renders the whole preview, so an arrow afterwards could mean either "the repaint did
not help" or "the re-render reset the cursor".

**Harness note for whoever picks this up.** "Did the cursor change" is not a question a
screenshot answers reliably on either platform. On X11 read the cursor's *name* directly
(`XFixesGetCursorImage` via ctypes — it returns `b'pointer'` / `b'text'`). On macOS,
`CGSCurrentCursorSeed` was tried as a corroborating signal and is **unusable the way it was
built**: the seed increments on `screencapture -C` itself, measured by running the identical
sampling loop over blank space with no hover target and seeing the same climb. An instrument
inside its own measurement, and the artefact it produced happened to match the hypothesis
under test.

## S. Every native file chooser invocation grows RSS on macOS

**Severity**: Medium (unbounded within everything measured — no plateau over 20
cycles — and it affects every file dialog in the application, not one feature. Not a
crash, not data loss, and slow enough that an ordinary session will not notice it.)

**macOS only, MEASURED.** Opening a `GtkFileChooserNative` and **cancelling it** —
never completing an operation — grows the process's RSS by roughly **0.9–1.4 MB per
invocation**, steadily, with no plateau across 20 cycles. A large one-time jump
(~24–28 MB) accompanies the *first* construction in a process and is separate: that
is the native-panel machinery warming up and it does not recur.

Measured on macOS 4.22.4/Quartz, release build, window fixed, RSS via `ps -o rss=`:

| probe | cycles | growth | per cycle |
|---|---|---|---|
| Export chooser, opened and **cancelled** | 19 | 170528 → 192592 KB | ~1160 KB |
| File ▸ Open chooser, opened and **cancelled**, fresh instance | 19 | 169776 → 186304 KB | ~870 KB |
| Full export cycle (chooser + real export, PDF) | 20 | 175648 → 198784 KB | ~1150 KB |
| Full export cycle (chooser + real export, HTML) | 10 | 197440 → 209504 KB | ~1206 KB |

**It is not the export path**, and the numbers above are what establish that: a
cancelled chooser that runs no export at all grows at the same rate as a completed
export, and the plain **Open** dialog — which has nothing to do with exporting —
grows the same way. Export is simply a new place that opens a chooser repeatedly,
which is how this surfaced. Do not file it against the export feature.

**It is not a classic leak.** `leaks <pid>` against a live instance after 30 combined
cycles reported **472 leaks / ~103 KB**, against an RSS climb of ~34 MB over the same
run — two orders of magnitude apart. A second instance measured 353 leaks / ~96 KB
against a comparable climb. Whatever is accumulating is still **reachable**, so it
reads as fragmentation or a growing cache (font map, panel construction/teardown, some
platform-side pool) rather than unreachable memory.

**The application's own reference discipline is not the suspect.** Both call sites —
`window/export.rs`'s `choose_destination` and `app/appactions.rs`'s open action — go
through `saferizer::native_dialog::NativeDialogHolder`, which takes exactly one
external strong reference and drops it once when `response` fires, and both call
`destroy()` on the dialog in the response handler. That is the shape ScrAP-41
prescribes, and it is the same code on every platform.

**Linux does not reproduce it — with an important caveat about what was measured.**
An in-process probe constructing, showing, cancelling and dropping 40 `FileChooserNative`
dialogs under Xvfb (GTK 4.6, release, RSS via `/proc/self/statm`) ended **+2612 KB over
40 cycles, ~65 KB/cycle, and non-monotonic** — the reading fell twice (+2936 KB at cycle
10, +664 at 20, +700 at 30, +2612 at 40), which is noise around a flat baseline rather
than growth. **But the caveat is the whole value of the result:** with no portal present,
GTK on Linux backs `FileChooserNative` with its own in-process `GtkFileChooserDialog`
widget (the run emits "GtkDialog mapped without a transient parent"), whereas macOS backs
it with a real `NSOpenPanel`/`NSSavePanel`. So this is a negative about **a different
implementation**, not about the same code path behaving differently. It narrows the
suspect to the platform panel; it does not exonerate anything portable.

**Windows does not reproduce it either — and that negative is the informative one.** The
Linux non-reproduction came with a caveat: GTK backs `FileChooserNative` with its own
in-process widget there, so it measured a different implementation. **Windows closes that
gap.** It uses a genuine native panel — the Win32 common dialog, a separate shell-owned
window, gated per cycle by reading its title — and over **40 open-and-cancel invocations**
grew **+5.9 MB total, decelerating** (+2.9 MB in the first five cycles, +1.6 MB across
cycles 20→40): lazy one-time initialisation of the shell dialog machinery, not a
per-invocation cost. macOS's sustained ~1 MB/invocation would have been ≈ +40 MB over the
same run.

So **"a native panel implies the climb" is refuted**: two native-panel platforms, opposite
behaviour. Whatever this is, it is specific to macOS's `NSOpenPanel`/`NSSavePanel` path
rather than a property of `GtkFileChooserNative`'s native-dialog architecture. That is a
real narrowing of the search and it should be the starting point for whoever attributes it.

### ◐ ATTRIBUTED TO THE PLATFORM, NOT TO GTK — and not fixable by anyone here

**⚠️ PROVISIONAL: every measurement below was taken with the Mac's screen LOCKED, and this
finding is specifically about panel PRESENTATION — the powerbox / remote-view path a locked
screen is most likely to alter. An unlocked re-run is owed before any of it is quoted
outside this file.**

**The cost is MOSTLY AppKit's own price for presenting an `NSSavePanel` — about four fifths of
it — with a real GTK component on top.** Measured with **no GTK and no GLib in the process at
all**, a panel retained exactly as GTK retains it, 20 cycles per run, n=3 (GTK cell n=5).
**Both columns are kept deliberately**: the locked→unlocked delta is evidence in its own right
about what a locked session does to this measurement.

| configuration | locked | UNLOCKED | delta |
|---|---|---|---|
| AppKit, `orderOut:` only | 558.0 | **518.5** | −39.5 (−7.1%) |
| AppKit + accessory view | 973.7 | **918.7** | −55.0 (−5.6%) |
| AppKit, reuse panel + accessory | 73.3 | **49.4** | −23.9 (−32.6%) |
| AppKit, reuse + reconfigure | 78.3 | **53.9** | −24.4 (−31.2%) |
| AppKit, cached popup repopulated | 76.6 | **52.2** | −24.4 (−31.9%) |
| **GTK, the application's shape** | 927.7 | **1136.8** | **+209.1 (+22.5%)** |

⛔ **WITHDRAWN: "GTK is marginally cheaper than bare AppKit; there is no GTK-attributable
component in the bytes."** That was a **locked-session artefact and the direction flips.**
Like-for-like, accessory view in both: locked, GTK 927.7 vs AppKit 973.7 — GTK cheaper by 46.0
(−4.7%); unlocked, GTK 1136.8 vs AppKit 918.7 — **GTK DEARER by 218.1 (+23.7%)**. So there *is*
a GTK-attributable component, roughly **a fifth of the total**, and it was **invisible while
the screen was locked**. AppKit's 918.7 remains the dominant term at about 81%.

**Do not quote the 218 as a precise figure.** Two caveats, both from the seat that measured it:
the cells are not perfectly like-for-like — the GTK cell carries a transient parent window and
the internal `GtkFileChooserDialog` widget tree, the AppKit control carries neither — and GTK's
spread widened from 2.5% locked to ~10% unlocked (1088.0–1196.6). The defensible statement is
**"GTK adds roughly a fifth on top; direction confirmed, magnitude approximate."** INFERRED
mechanism, untested: with a real display the `GdkMacosWindow` parent and GTK's internal chooser
actually render and allocate, where locked they cost almost nothing.

✅ **Every non-byte finding is untouched, and the refcount evidence is IDENTICAL rather than
merely similar**: live panels 20 fresh / 1 reused; deallocs 0; `_retainedSelf` == the panel
with the same flag values; all six properties persisting on a cached panel; the extension
doubling reproducing exactly; state hygiene 20/20.

✅ **And the reuse mitigation got BETTER**: AppKit fresh 918.7 → reused 49.4 is a **94.6%**
saving unlocked, against 92.5% locked.

**The locked-screen caveat earned its keep, and that is worth stating plainly**: carrying it
cost a delay and it **changed one sign**. Every conclusion that rested on counts survived
untouched; the one that rested on a between-mode byte comparison inverted. That is the
sharpest available illustration of this entry's own rule 1 — the counts were evidence, the
byte differences were not.

That also explains the platform split this entry opens with, which nothing else did: Windows
and Linux never reproduced it because it is the macOS panel's own cost, not a property of
`GtkFileChooserNative`.

**And the retainer is now NAMED: the panel retains ITSELF.** macOS 26.6.1's `NSSavePanel`
carries an ivar `_retainedSelf`, read directly after dismissal in both modes:

```
_retainedSelf = 0x8af560600, panel = 0x8af560600, same = YES,
_panelCompleted = 1, _panelIsNowUseless = 0, _observingBridge = 1
```

The panel holds a strong reference to itself, still held after the completion handler has run
and after `orderOut:`/`close`. That is `DEALLOCATED = 0` explained in one ivar; it is
in-process AppKit state; it reproduces with no GTK anywhere in the process; and **a
self-retain we do not own cannot be balanced by GTK or by this application.** MEASURED: the
ivar values. INFERRED: that AppKit never balances it — from zero deallocations across runs up
to 40 cycles.

`_openWindows` is separately exonerated: relaunching with
`-NSApplicationStronglyReferencesOpenWindows NO` changes nothing (563.4 / 544.0 / 553.3,
n=3).

**"Never returned" is now tested rather than assumed.** Twenty cycles, then **45 seconds idle
at base run-loop level**, n=2: RSS 83168 → 83104 (−64 KB) and 83232 → 83184 (−48 KB), against
~10.7 MB accumulated. Live panels still 20, deallocs still 0. **0.5% comes back and the rest
stays** — there is no deferred drain and no delayed reclaim. That is also the last nail in the
retracted GDK autorelease-pool lead: undrained autoreleases would have come back on an idle
base-level loop, and they do not.

### ✅ ~93% IS RECOVERABLE UPSTREAM — by REUSING the panel, not by releasing it

**This is the first thing in the investigation that moves the number**, and it arrived by
testing the one configuration nobody had: not *releasing* a fresh panel better, but not
creating a fresh one at all. Pure AppKit, no GTK, 20 presentations per run, n=3, KB per
presentation — all cells locked-session and therefore internally comparable even before the
unlocked re-run:

| configuration | KB/presentation |
|---|---|
| fresh panel + fresh accessory view — **GTK's exact shape** | 980.2 |
| fresh panel, no accessory view | 574.3 |
| **reused** panel + fresh accessory view | 395.0 |
| **reused** panel + **reused** accessory view | **73.3** |
| (panel never shown at all, for scale) | ~21 |

**Caching both objects removes ~94.6% of the cost** (unlocked; 92.5% locked), and the decomposition is clean and
additive: a freshly created panel costs ~510 KB that is never reclaimed, a freshly created
accessory view costs ~330 KB of its own, and the two are independent — reusing the panel
while still building a new popup each time only reaches 395. A 40-cycle reuse run holds at
60.3 KB/cycle, so the residual is small and does not compound the way fresh allocation does.

GTK allocates **both** fresh on every invocation — the panel in `filechooser_quartz_launch`,
the `FilterComboBox` at `:314`. Caching them (at minimum one `NSSavePanel` and one
`NSOpenPanel`, which are different classes) is an upstream change with a measured number
attached, which is exactly what the four earlier remedy proposals lacked.

⛔ **The mitigation is NOT "hold the panel in a static" — it has a hard prerequisite.**
Every property GTK sets on the panel is set with a **one-sided guard and no else-branch
reset**, which is safe only because the panel is brand new each time:

```
if (data->select_multiple) [panel setAllowsMultipleSelection:YES];   /* :253-256, no else */
if (data->create_folders)  [panel setCanCreateDirectories:YES];      /* :260-263, no else */
if (data->accept_label)    [data->panel setPrompt:...];              /* :277-278, no else */
if (data->title)           [data->panel setTitle:...];               /* :280-281, no else */
if (data->message)         [data->panel setMessage:...];             /* :283-284, no else */
if (data->filters) { ... [data->panel setAccessoryView:...]; }       /* :313-350, no else */
```

On a cached panel each becomes stale state: a multi-select Open followed by a single-select
one still allows multiple selection; a titled chooser followed by an untitled one keeps the
old title; and a filtered chooser followed by an unfiltered one **keeps the previous accessory
view** — which is also the ~330 KB object measured independently above. So the mitigation is
"**convert configuration from partial to TOTAL**": every property assigned unconditionally on
every launch, including explicit resets when the GTK-side value is NULL or false, and
`accessoryView = nil` when there are no filters. **This is a stated prerequisite, not a
footnote** — a maintainer who caches without it ships six stale-state bugs, and they land on
users rather than on whoever files this. Two conveniences in the same finding: GTK sets no
delegate on the panel at all (no `setDelegate:` in the file), so there is no delegate-rewiring
hazard; and the cache must be keyed by **class** — `NSOpenPanel` and `NSSavePanel` are chosen
by action at `:229`/`:237`/`:251`, so it is two cached instances minimum.

✅ **Re-presentation of a completed panel WORKS — measured, so the thesis lives.** One panel
instance re-presented and reconfigured every cycle (distinct title, prompt,
`nameFieldStringValue`, `directoryURL`), reading state back off the live panel after each
presentation: **12/12 presentations returned that cycle's configuration, 0 mismatches**, twice,
n=2. No stale name, folder, title or prompt; `-URL` derives from the current cycle's directory
and name; `_panelIsNowUseless` stayed 0 through the last cycle.

⛔ **BUT CACHING THE FILTER CONTROL AS WRITTEN IS A USE-AFTER-FREE.** Verified on **GTK 4.6.9**
(`/opt/dev/oss/gtk`, branch `gtk-4-6` @ `492b44f20c`) — and the same shape holds on `main`
under different names, see the version table below:

```objc
@interface FilterComboBox : NSObject<NSComboBoxDelegate>   /* :75 */
- (id) initWithData:(FileChooserQuartzData *) quartz_data
{ [super init]; data = quartz_data; return self; }          /* :85-90 — raw pointer, never updated */
```

`comboBoxSelectionDidChange:` (`:91-106`) dereferences it hard — `data->filter_combo_box`,
`data->filters`, `data->panel`, `data->self` — ending in
`g_object_notify (G_OBJECT (data->self), "filter")`, and `filechooser_quartz_data_free` ends
with `g_free (data)`. So a **cached** control holds a dangling pointer from the first
invocation's teardown onward, and the first filter change on the second invocation is four
use-after-frees, one of them on freed GObject memory. **A crash, not a leak.** The prerequisite
is to re-point `data` on every launch.

Note the delegate object at `:306` — `[data->filter_combo_box setDelegate:[[FilterComboBox
alloc] initWithData:data]]` — is `alloc`'d inline and never released, and AppKit holds a
delegate weakly, so that unbalanced `+1` is the only thing keeping it alive. Both facts matter
to any caching patch.

⚠️ **A VERSION WARNING that cost this entry real accuracy — TWICE, in opposite directions.**

The Quartz filter control **differs between GTK 4.6 and main**, and every seat here got burned
by not saying which tree it meant:

| | GTK 4.6.9 (`gtk-4-6`) | `origin/main` |
|---|---|---|
| ivar | `filter_combo_box` (`:69`) | `filter_popup_button` (`:73`) |
| class | `FilterComboBox : NSObject<NSComboBoxDelegate>` (`:75`) | `FilterComboBox : NSPopUpButton` (`:79`) |
| control | `NSComboBox`, delegate installed via `setDelegate:` (`:306`) | `NSPopUpButton` subclass, allocated directly (`:314`) |
| handler | `comboBoxSelectionDidChange:` (`:91-106`) | `popUpButtonSelectionChanged:` (`:99`) |
| populate | `addItemsWithObjectValues:` (`:304`) | `addItemsWithTitles:` (`:315`) |
| launch priming | none | `[… popUpButtonSelectionChanged:NULL]` (`:348`) |

The reimplementation landed in `2a96dde115` ("macos: use NSPopUpButton for filter selection in
native filechooser"). **Direction matters and was initially got backwards here:
`NSComboBox` is the OLDER shape.** 4.6 is the pre-2023 code; `main` and the 4.22.4 the
measurements run on are both post-migration. Settled by the strongest artefact available — the
**shipped binary** on the measuring machine
(`/opt/homebrew/Cellar/gtk4/4.22.4/lib/libgtk-4.1.dylib`): `nm -u` imports
`_OBJC_CLASS_$_NSPopUpButton` and contains **zero** `NSComboBox` references; `strings` has
`popUpButtonSelectionChanged:` and `addItemsWithTitles:` and lacks
`comboBoxSelectionDidChange:` and `addItemsWithObjectValues:`.

**⚠️ Consequence for anyone patching this: the fix must be authored twice, the 4.6 variant is
the WORSE bug, and its naive fix is a USE-AFTER-FREE.**

On 4.6 there are **two** unbalanced allocations where `main` has one, and
`filechooser_quartz_data_free` (`:182-205`) releases only `filters` and `filter_names` —
`filter_combo_box` appears nowhere in it:

- the `NSComboBox` at `:303` — an unbalanced `+1` **with** a handle, the direct analogue of
  `main`'s `filter_popup_button`;
- the delegate at `:306`, `[data->filter_combo_box setDelegate:[[FilterComboBox alloc]
  initWithData:data]]` — an unbalanced `+1` with **no handle**, never stored anywhere.

⛔ **And that second leak is LOAD-BEARING.** AppKit holds delegates **weakly**, so the combo
box does not own it — the unbalanced retain is *precisely what stops the combo box holding a
dangling delegate pointer*. A 4.6 backport that simply releases the delegate **converts a leak
into a use-after-free**. Any 4.6 fix must `[filter_combo_box setDelegate:nil]` *before*
releasing, and must capture the pointer first because none is kept today.

**That is the SECOND naive patch on this file that ships a use-after-free** — the other being
a cached filter control whose raw `data` pointer is never re-pointed. Two independent
"obvious" fixes, both crashes.

⚠️ **And a THIRD way the obvious patch fails, which is quieter: it balances the books without
freeing anything.** `setAccessoryView:` **retains its argument**, so releasing
`filter_popup_button` while the panel still references it balances GTK's `alloc` and
**cannot deallocate the object**. The correct order is Qt's, and Qt does exactly this:
`orderOut:` → `setAccessoryView:nil` → release the popup button → release the panel. A patch
that omits the `nil` looks right, passes review, and reclaims nothing.

✅ **AND THE ORDER THAT INCLUDES IT DOES RECOVER — MEASURED, which upgrades part of the
ownership fix from correctness to footprint.** A dealloc spy on the popup, emulating GTK's
ownership exactly: releasing with `accessoryView` still set gives **0/20 deallocations at
932.2 KB/cycle**; `setAccessoryView:nil` *then* release gives **19/20 at 542.3** — about
**390 KB per invocation reclaimed**. Corroborated from inside GTK, interleaved and paired,
n=5: with-filter 1156.7 vs without-filter 735.3, a per-rep delta of **421.4 KB**
(range 344–492) bracketing the 390 measured in a process containing no GTK at all.

**Read the two figures with their scopes, which differ**: 390 KB is the AppKit-only
accessory component; GTK's 421 KB delta is filter-vs-no-filter and therefore also carries
`GtkFileFilter` and the internal chooser's filter handling, so it **bounds the accessory
component from above** rather than measuring it.

**So "no ownership fix recovers a byte" was too pessimistic and is corrected.** It holds for
the *panel* retain, which `_retainedSelf` pins regardless. It does **not** hold for the
accessory view: that half is a real ~390 KB per invocation, and an upstream report may say
so.

**On this file, assume the obvious fix is wrong until the ownership graph is drawn.** Three
independent "obvious" patches now: two crash, one silently no-ops.

**Not this project's exposure either way**: the macOS seat runs 4.22.4, so the 4.6 variant is
noted for the upstream fix and is not ours to chase.

**This seat's own error, recorded because it is the sharper of the two.** Told that the
researcher's identifiers "did not exist", this seat read `/opt/dev/oss/gtk`, asserted it was
current `main`, and ran `git log -S "NSPopUpButton"` — which returned nothing and was reported
as "never existed in GTK, across the entire repository history". **The checkout was on branch
`gtk-4-6` at 4.6.9**, so `-S` searched only that branch's ancestry and could not see a change
made on `main` after the 4.6 branch point. The researcher's identifiers were correct for `main`
and for the 4.22.4 the measurements run on; this seat's were correct for 4.6.9; and the
confident falsification was the least accurate claim of the three. `git log --all -S` finds it
in two seconds.

**The standing rule that comes out of it**: state the tree with every source claim — branch and
commit, not "the source" — and remember that `git log -S` is scoped to the ancestry of whatever
`HEAD` happens to be. A repository-wide negative requires `--all`.

✅ **The popup half survives repopulation — so the headline is firm at ~92%.** GTK
repopulates the filter control per invocation because the filter list changes
per call, and a cached popup refilled with a different list each cycle costs **76.6 KB per
presentation** (75.8 / 76.6 / 77.5) against 73.3 for a static one. The item list is not where
the money is. So caching both objects, with both reconfigured per invocation, removes **~94.6%**
of the per-invocation cost, and both halves are now measured rather than one measured and one
expected.

⛔ **The stale-state prerequisite is now MEASURED, not inferred from source, and it is worse
than the source read suggested.** An `NSOpenPanel` configured on cycle 1 (multi-select,
`canChooseDirectories`, title, message, prompt, accessory view) and then presented on cycle 2
with **nothing set at all** — exactly what GTK does for an untitled, unfiltered, single-select
chooser — reads back:

```
multiSelect=1  title='First invocation title'  message='First invocation message'
prompt='Choose'  accessoryView=NSPopUpButton  canChooseDirs=1
_panelCompleted=1  _panelIsNowUseless=0
```

**All six persist. AppKit resets nothing between presentations.** Every one of GTK's one-sided
`if` configurations therefore becomes a stale-state bug on a cached panel — including
filtered → unfiltered keeping the previous accessory view, which is simultaneously the ~330 KB
object being reclaimed. The same run also settles two mechanics: `_panelIsNowUseless` stays 0
across the second presentation, and `_panelCompleted` stays 1 after the first completion
*without* preventing re-presentation — **"completed" is not "spent"**.

### The caching MR's THREE non-optional preconditions

It must not be filed as "hold it in a static". All three are measured or source-verified:

1. **Re-point the popup's `data` pointer on every launch** (`setData:`) — otherwise it is a
   use-after-free, above. The difference between reclaiming 330 KB and shipping a crash.
2. **Repopulate the popup's items per invocation** — free, per the 76.6 measurement.
3. **Make panel configuration TOTAL**, with explicit resets when the GTK-side value is
   NULL/false and `accessoryView = nil` when there are no filters — otherwise six stale-state
   bugs, measured above.
4. **Reset the name field explicitly on every launch, and VERIFY THE RETURNED URL rather than
   the properties.** Not an ordering fix — see below. This is the nastiest of the four,
   because every property-level check passes while the answer is wrong.

**The extension artifacts, MEASURED — and there are TWO of them, only one caused by caching.**
`allowedContentTypes` itself replaces cleanly across reconfiguration (8/8, alternating real
UTType lists, no residue). But the panel rewrites the name field's extension to match the
current type list, and a fresh-panel control separates what that produces:

| cycle | name set | types set | FRESH panel (GTK today) | CACHED panel |
|---|---|---|---|---|
| 1 | `doc-1.pdf` | PDF | `doc-1.pdf` ✅ | `doc-1.pdf` ✅ |
| 2 | `doc-2.pdf` | HTML | `doc-2.html` | `doc-2.html` |
| 3 | `doc-3.pdf` | PDF | `doc-3.pdf` ✅ | **`doc-3.pdf.pdf`** ⛔ |

- **Cycle 2 is PRE-EXISTING and out of scope.** The extension rewrite happens on a fresh panel
  too, so it is in GTK today, unrelated to caching, and probably correct behaviour. A patch
  that suppressed it would change what current users get. Do not let an MR "fix" it.
- **Cycle 3 is caching-induced** and appears only on the cached panel. That is the real
  precondition.

**Reversing the order relocates the artifact, it does not remove it.** GTK's order (name then
types) gives `doc-1.pdf | doc-2.html | doc-3.pdf.pdf`; reversed (types then name) gives
`doc-1.pdf | doc-2.pdf.html | doc-3.pdf` — cycle 3 comes out clean and cycle 2 breaks instead.
So there is no one-line ordering fix, and **both orders leave a cycle where every property
reads back correctly and the URL is wrong.** Hence: explicit name reset per launch, and verify
the URL.

### ⛔ A CRASH CLAIM THAT WAS FALSIFIED — kept because the falsification is the lesson

An "empty filter array crashes the chooser" defect was worked up here as source-verified,
gate-free and filable ahead of everything else. **It does not crash.** Measured on macOS
26.6.1: an end-to-end repro (a named `GtkFileFilter` with no patterns and no MIME types, added
to a chooser and shown) exits 0 having reached the apply path; and the premise tested directly
against a bare `NSSavePanel` with no GTK — `setAllowedFileTypes:` with `@[]`, with `nil`, and
with `@[@""]` — raises **no exception in any case**.

**Steps 1–3 of the chain are still correct** and independently verified twice: a rule-less
filter yields a non-nil zero-element array (`_gtk_file_filter_get_as_pattern_nsstrings`
returns `NULL` only on a MIME→UTI failure), `file_filter_to_quartz` guards `== NULL` only, and
the empty array reaches `setAllowedFileTypes:`. **Step 4 was wrong**: Apple's SDK header says
*"If the array is not nil and the array contains no items, an exception will be raised"*, and
that **documented contract was treated as observed behaviour**. The shipping implementation on
26 does not enforce it — plausibly because the deprecated property is now bridged onto
`allowedContentTypes`, whose "no restriction" value is precisely `@[]`. Unverifiable here: it
may still hold on macOS 11–13, and nobody has one.

**The transferable rule, which is the whole reason this is written up rather than deleted:**

> **An SDK header or documented contract is primary evidence for API SURFACE only** — "this
> class is not a singleton", `@property (copy)`, `API_DEPRECATED`. **Any claim about RUNTIME
> BEHAVIOUR** — an exception raised, an object freed, a handler invoked — **needs an
> observation.** A deprecated symbol's documented contract should be assumed stale until
> observed, since a deprecated API is exactly the one whose implementation has been rewritten
> underneath its documentation.

The disproof was in the same message as the claim: the deprecation note and the `nil`-vs-`@[]`
sentinel difference were both written down, and a deprecated property bridged onto a successor
whose "no restriction" value is `@[]` is precisely the circumstance in which the old contract
would be relaxed. Both halves of the counter-argument were on the page and no conclusion was
drawn from them.

**What survives**: not a crash and not a user-visible bug — an empty `allowedFileTypes` is
accepted and behaves as "no restriction", which is what the `nil` branch would have done. The
guard is still defensible as **robustness**, and in a form that cannot be falsified by anyone's
OS version: **a zero-rule filter should not produce a popup entry at all**, because it cannot
usefully appear in the list. And the deprecation note stands on its own, being an API-surface
claim: `setAllowedFileTypes:` is deprecated since macOS 12 and still called on 26, and
`allowedFileTypes` uses `nil` for "no restriction" where `allowedContentTypes` uses `@[]`, so
carrying the sentinel across inverts the branch.

**Do not file this as a crash report.** A maintainer who runs it on current macOS gets the
negative, and a first filing that does not reproduce costs more than the gated items put
together. Fold the guard and the deprecation note into the ownership issue as labelled
robustness items.

**Limits the caching recommendation must carry****Limits the caching recommendation must carry**, none of them reachable from a locked
headless probe: an actual **accept** has never been exercised (every measurement cancels, and
`-ok:` has been non-callable programmatically since 10.15), so "returns the correct URL when
the user picks a file" is inferred from the panel's derived `-URL`; **Save ↔ Open on one instance is impossible** (different classes, so a cache
is two instances minimum); and **sandboxed powerbox behaviour** is untested.

📏 **A numerical caution for anyone quoting these**: per-presentation figures are **not
comparable across different cycle counts**. The 12-cycle runs read 106.2 and 91.6 KB/cycle
against 74.9–80.8 for the same configuration at 20 cycles, because a fixed warm-up is
amortised over fewer presentations. Compare like with like or the mitigation looks worse than
it is.

**Severity note.** Before the reuse finding, the honest status was closer to
**intractable-but-real** than to open-pending-fix. It is no longer: there is a reachable
upstream change with a measured recovery. It stays **Medium and open**. Two things gate any
severity change in either direction — the unlocked re-run (every figure here is from a locked
session, against the presentation path a lock is most likely to alter) and the reconfiguration
probe above.

**Consequences, and they are unusually tidy:**

- **Every failed remedy now has ONE explanation instead of four.** Reaping non-visible
  panels, releasing the panel, releasing the accessory view — all recovered nothing, because
  none of the cost was ever GTK's to reclaim.
- **GTK's unbalanced retains are still real defects** (below), and still worth reporting
  upstream — but they are **not** the memory story, and a fix for them will not move this
  number. Anyone filing them must not claim a footprint impact it cannot deliver.
- ⚠️ **"No fix at any layer changes the footprint" was TRUE of every remedy tried, and is
  now FALSE.** See the reuse finding below — the one configuration nobody had tested.
  Recovering it is an upstream GTK change, not an app-side one, so the prohibition on
  app-side mitigation is untouched.

⚠️ **Do not restate this cost against the 50 MiB ceiling.** That ceiling (TDD §6) is a
**VRAM** contract and this is resident memory; they are different budgets. The arithmetic is
tempting and has already been attempted twice from two directions. If a severity statement is
needed, state it in RSS terms and say so.

---

**The GTK ownership defects — real, source-verified, and now known NOT to be this issue's
cause.** Kept because they are worth an upstream report on correctness grounds, and because
the reasoning behind each retraction is the useful part.

**What the source says, and what that does NOT establish.**
`gtk/gtkfilechoosernativequartz.c` runs two inconsistent ownership models on its two exit
paths, and this part is source-verified against `origin/main`:

- `filechooser_quartz_launch` takes an explicit `+1` on the panel and sets
  `setReleasedWhenClosed:YES`. **The ordinary path** (the user picks a file, or cancels)
  sends only `[data->panel orderOut:nil]`; `orderOut:` does not trigger
  `releasedWhenClosed`, and there is no `[data->panel release]` anywhere in the file. The
  missing release is real.
- **The programmatic-hide path** (`gtk_file_chooser_native_quartz_hide`) sends `[panel
  close]`, which *does* balance the panel retain — but it never calls
  `filechooser_quartz_data_free` and never clears `self->mode_data`, and `show()` overwrites
  `mode_data` unconditionally, so it orphans the **entire `FileChooserQuartzData`** instead,
  including the `g_object_ref(self)` taken at launch. That is why
  `gtk_file_chooser_native_finalize` — and `gtk_window_destroy(self->dialog)` — never runs.
  MEASURED and reproduced by GObject instance count: after 12 cycles, 12 live
  `GtkFileChooserNative`, 12 `GtkFileChooserDialog`, 12 `GtkFileChooserWidget`, 1632
  `GtkColumnViewCell`. **This half stands.**

⛔ **What does NOT follow, and was wrongly asserted here: that the missing release explains
this issue's RSS growth.** A dealloc spy reports panels **deallocated = 0 in every mode** —
including a mode that adds an explicit `[panel release]` balancing GTK's retain.

**The documented mechanism was checked and RULED OUT — which is a better position than
naming it.** Apple's AppKit Release Notes for macOS 10.13 ("NSWindow Lifecycle Changes") say
an ordered-in `NSWindow` is strongly referenced by AppKit *until it is explicitly ordered-out
**or** closed*. That looked like the answer for about an hour. It is not: a class-dump shows
`NSApplication` keeps **two** window lists (`_windowList` and `_openWindows`, with distinct
`_addWindow:`/`_addOpenWindow:` pairs), only `_openWindows` retains, and the reference it
holds is released on order-out — and GTK **does** send `[data->panel orderOut:nil]` on the
transient-parent branch, which is the branch every measured run took. So the documented
AppKit reference is already gone in the exact configuration that measured
`DEALLOCATED = 0`. Two related corrections come with it: `[NSApp windows]` is documented as
"all currently existing windows", so **membership proves the object is alive, not that the
list retains it** — the +1-per-invocation count is consistent with either — and
`-[NSSavePanel cancel:]` fired the completion handler **20/20** where `close` fired it
**0/12**, so ending the panel *session* and closing the *window* are not interchangeable.

⛔ **SUPERSEDED hypothesis, kept because its shape was right and its mechanism was wrong.**
The panel *is* an out-of-process object — `NSSavePanel` conforms to the remote-view exported
protocol — and the leading guess was that `NSXPCConnection`'s strong `exportedObject` slot
pinned it. Wrong mechanism: the connection is not what holds it, the panel holds *itself*
(above). Right neighbourhood, wrong object, and it would have sent the next investigator to
ViewBridge.

**Not a macOS 26 regression, which keeps severity where it is.** Class-dump comparison across
AppKit versions: on 1671.20.108 (≈10.14) `NSSavePanel : NSPanel <FIFinderViewDelegate,
NSTouchBarDelegate>` has none of `retained`, `panelCompleted`, `panelIsNowUseless` or
`observingBridge` — the mechanism is **absent**. On 1865.10.102 (≈11) it conforms to
`NSOpenAndSavePanelRemoteViewExportedToServiceProtocol` and carries all four as synthesised
`BOOL` properties — **present**. On 26.6.1 the flag `BOOL retained` has become the pointer
`_retainedSelf` and the protocol is renamed. Same mechanism, refactored, and it arrives *with*
the panel going out of process in the 10.15/11 window. VERIFIED: absent on 10.14, present on
11. INFERRED: that it was never balanced back then — a flag existing does not prove it was
never released, and the never-balanced half rests on the 26-only zero-dealloc evidence. So
this is roughly **five-year-old platform behaviour, not a fresh regression**.

**Open sub-question, stated rather than left silent, because the two answers read very
differently to Apple**: the *mechanism* is verified absent on 10.14 and present on 11, but
whether it was ever *balanced* on 11/12/13 is **unverified** — it needs a dealloc census on
one of those machines, and no seat here has one. "Apple broke this" and "this has been true
for five years" are different reports, and only the second is currently supported.

**Precedent — and its SCOPE, because it is easy to overclaim. Qt ships the balancing
release; Qt does NOT cache.**
`qtbase/src/plugins/platforms/cocoa/qcocoafiledialoghelper.mm` takes the *same*
`[[NSSavePanel savePanel] retain]` GTK takes, and in `dealloc` does
`[m_panel orderOut:m_panel]` … `[m_panel release]` — order-out **first**, release **second**.
That is the ordering the whole investigation converged on independently, and "Qt, in the same
position, does this" costs nothing to say — **about ownership**. It is not a caching precedent:
`QCocoaFileDialogHelper::show()` builds a fresh delegate on every show and the panel is
allocated in that delegate's `init`, so Qt allocates a fresh panel per invocation exactly as
GTK does. Citing Qt for the caching recommendation would be cited straight back.

**And that null answer is worth more than a precedent would have been.** On these numbers Qt
should exhibit the same ~510 KB per presentation — and it does: QTBUG-39205 ("Memory leak bug
of QFileDialog") plus forum reports of `QFileDialog` reaching 200–300 MB over repeated opens,
which this entry previously carried as "unconfirmed, may not be the same defect". They now
have a mechanism that predicts them, which upgrades them from padding to **corroboration** —
a second major toolkit, same lifecycle, same symptom, never diagnosed, entirely independent of
GTK.

**Adjacent platform behaviour on this exact macOS version**: Mozilla bug 2053177 (macOS 26,
resolved fixed) records that the `NSSavePanel`/`NSOpenPanel` sheet completion handler can run
*before* the panel's modal session has fully unwound; Firefox's remedy is to defer its
completion work by one main-thread turn. The measurements here are on macOS 26.6.1, so
AppKit is plausibly still tearing down while GTK's handler runs its `orderOut:`, its
`data_free` and its response emission.

**Cross-toolkit corroboration, none of it confirmed to be this defect**: an unanswered Apple
Developer Forums report of per-invocation growth under `NSOpenPanel` measured with both
`leaks` and `ps`; wxWidgets #22173 (a lifetime bug on repeated panel presentation, with
`NSRemoteView`/ViewBridge warnings); Qt QTBUG-39205. No Apple radar and no acknowledged
OS-side leak. "This shape has been seen in three other toolkits and never diagnosed" is
context, not evidence.

⚠️ **The residual puzzle is therefore still open, and is now the whole question**: with the
documented AppKit reference ruled out, something else retains the panel. The mechanism behind
the per-invocation cost remains **unattributed**.

**What is MEASURED and survives both retractions** (macOS seat,
`probes/native-chooser-rss.m`, real user-cancel path). Object counts first, because counts
are refcount evidence and single-run byte deltas are not:

- Objects accumulate per invocation, counted directly: **+1 `NSSavePanel`/`NSOpenPanel`**,
  and **+1 `NSAccessoryViewWindow`** when a `GtkFileFilter` is present.
- **Every GTK-side instance count returns to zero** on the real cancel path
  (`GtkFileChooserNative`, `GtkFileChooserDialog`, `GtkFileChooserWidget`,
  `GtkFileSystemModel`, `GtkColumnViewCell`, `GFileInfo`). The Rust reference handling is
  exonerated by measurement, not by assertion — this result has never wavered.
- **The cost tracks PRESENTING the panel**, not anything reachable from our layer (n=3 per
  cell, interleaved): no filter **596–626 KB/cycle**; with a filter **948.5** (spread 6);
  never shown **21**; the internal `GtkFileChooserDialog` built and destroyed with no panel
  at all, **5**.
- It is **not** the folder listing (empty folder 551 vs 123-entry 606), **not** the transient
  parent (579), **not** the GTK widget tree.
- **The baseline magnitude does meet the standing rule**: **927.7 KB per invocation, n=5,
  spread 2.5%** (five 20-cycle runs, the application's exact shape — Save + `GtkFileFilter` +
  transient parent, real `-cancel:` dismissal), with the window census naming what it covers:
  `NSSavePanel` 20, `NSAccessoryViewWindow` 20, `GdkMacosWindow` 1 — exactly 1:1 per
  invocation for both panel classes. Note what that implies about the retractions below:
  **the figures that failed to reproduce were always DIFFERENCES between modes, never the
  baseline.**
- ⛔ **RETRACTED — "a pure-AppKit control is ~9.6 KB/cycle".** It was n=1 and nine subsequent
  runs disagree. It was also **confounded**: it dismissed with `close` while GTK dismisses
  with `orderOut:`, so it varied two things at once. That single number was the entire basis
  for "AppKit doesn't do this, GTK's use of it does" — the claim this entry was built on for
  most of its life. Correcting the confound did not refine the control, it **inverted** it.
  Nobody has explained the plateau that one run showed, and nobody has invented a reason for
  it.

⛔ **RETRACTED — the GDK autorelease-pool lead.** `gdk/macos/gdkmacoseventsource.c` drains one
static process-wide pool only at loop level zero, where a plain `NSApplication` drains every
iteration at every level, and that was proposed as the explanation for a GTK-vs-AppKit
asymmetry. **There is no asymmetry.** A plain `NSApplication` shows the same ~560 KB per
presentation, so there is nothing for the pool to explain. It may still stand as a separate
GDK concern on its own merits; it is not this one, and it should not be carried here as an
open lead.

### ⛔ Two retractions, and the second one is the lesson

**Retraction 1 — the byte figures.** A 2×2 of RSS-per-cycle across filter/no-filter and
reap/no-reap was measured **once per cell** and briefly landed here, concluding that the
retains were ~85% of the growth. Re-run with **n=3 interleaved**, reaping shows no benefit
at all (948.5 vs 920–937 KB/cycle). The 418 KB/cycle figure that carried the conclusion was
**noise reported as signal**. A single run per cell is exactly the shape that reads as a
clean result and is not one.

**Retraction 2 — the verdict itself, on a false negative the measurement CONSTRUCTED.** The
supporting observation was "0 live `NSSavePanel`s after reaping" — a count of `[NSApp
windows]`. But `-close` **removes a panel from that list without freeing it**, so the
instrument reported success for an operation that had freed nothing. A dealloc spy shows
panels deallocated = 0 in *every* mode, including one with an explicit `[panel release]`
balancing GTK's retain. **The panel is held by more than GTK's one reference**, and the
patch that was about to be proposed would not have freed it.

That is the transferable lesson, and it is worth more than the verdict would have been: **a
count of a registry is not a count of live objects.** Membership of `[NSApp windows]` is a
property the operation under test directly manipulates, so measuring it measured the
operation's own side effect and called it the outcome. Ask what would make the number move
*other than* the thing you are testing, and prefer an instrument the operation cannot
touch — here, a dealloc spy.

**The screen being locked was the wrong caveat**, twice over. It was attached in good faith
and it was real, but `CGSSessionScreenLockedTime` was unchanged between the reproducing and
non-reproducing runs. Repeatability, and then instrument validity, were the variables.

**Standing rules for this issue**, each bought with a retraction, in the order that earned
its place:

1. **No measurement-derived claim leaves this file without a CONTROL that varies only the one
   thing under test.** Two conclusions were within a message of publication here, and both
   were overturned by a control rather than by more source reading — the first because the
   control itself varied two things at once (`close` vs `orderOut:`), the second when that
   control was corrected and inverted the result.
   **The source reading was correct throughout, and it was still wrong four times about what
   the defect MEANT.** Reading the code told us truly what the code does; it never once told
   us whether it mattered. **And the unlocked re-run is this rule's cleanest demonstration**:
   every conclusion resting on a COUNT came back identical, while the one resting on a
   between-mode BYTE comparison inverted its sign. Every one of the four reversals came from an
   experiment — a dealloc
   spy replacing a blind counter, a two-variable control being caught, that control's re-run
   inverting the premise, and a caching probe breaking a frame four failed remedies had agreed
   on. **And the hazard is not that source reading is unreliable; it is that it felt
   authoritative each time it was insufficient.** This rule is first because it is the one
   that did the work.
2. No byte figure is recorded without **n and spread**.
3. No window-list count is treated as a liveness count.
4. **A selector, symbol or line number taken from a dumped or vendored artifact is re-checked
   against the version actually under test before it is handed to anyone as a probe** — and
   **every source claim names its tree**, branch and commit, not "the source". This one cost
   three separate errors: a selector five AppKit majors stale, identifiers carried between a
   version that was measured and a version that was read, and a repository-wide "this never
   existed" that had searched only one branch's ancestry (`git log -S` is scoped to `HEAD`'s
   ancestry; a repository-wide negative needs `--all`).
6. **An SDK header or documented contract is primary evidence for API SURFACE only.** Any claim
   about RUNTIME BEHAVIOUR — an exception raised, an object freed, a handler invoked — needs an
   observation, and a deprecated symbol's documented contract should be assumed stale until
   observed. A whole crash filing was built on a header sentence the shipping implementation no
   longer enforces.
5. **When several remedies fail identically, check whether they SHARE AN ASSUMPTION before
   concluding the problem is intractable.** Four remedies here recovered nothing — reaping
   panels, releasing the panel, releasing the accessory view, balancing the retain — and all
   four are the same move: trying to give an object back. Not one tested *not taking it in the
   first place*, which is where 93% of the cost turned out to be. Four experiments that agree
   are not independent evidence when they share a premise; the agreement read as strength and
   was a blind spot with four windows.

**And the GTK ownership defect is best described as LATENT, not as a leak.** GTK takes a `+1`
it never returns on the completion path — verified from source, contrary to Qt's practice, and
a plain `alloc` with no matching `release` in the filter-control case. But it is
currently **masked**: AppKit retains the panel regardless, so the missing release frees nothing
today. It is incorrect code that would begin to matter the moment AppKit's behaviour changed.
That is a real but modest bug, and any report of it must say so in those terms.

**The growth is genuinely UNBOUNDED, not a bounded one-time cost.** Apple's shipping SDK
headers settle it verbatim — `/* Creates a new instance of the NSSavePanel. This class is
not a singleton. */`, character-identical across the 10.13 through 14.5 SDKs and reflowed
but unchanged in 15.5/26.5. A fresh `NSPanel` + view hierarchy + (10.15+) its
`NSRemoteView`/XPC plumbing leaks per invocation.

⚠️ **Two pieces of folklore to refuse, both of which read as authoritative.** (1) "`openPanel`
reuses an existing panel / is a singleton" — that comes from an Apple document Apple itself
banners **Retired**, and the shipping headers contradict it; it still circulates through
search engines and model summaries. (2) "`isReleasedWhenClosed` is ignored for save/open
panels" — there is **no** Apple documentation saying that. The only "ignored" clause Apple
writes concerns windows owned by window controllers. Either line would get a bug report
publicly corrected.

### ⛔ There is NO app-side mitigation. Do not let anyone "fix" this here.

This is the decisive constraint and it is a prohibition, not a preference.
`_gtk_native_dialog_emit_response` (`gtk/gtknativedialog.c`) clears `priv->visible`
**before** it emits `response`, and `gtk_native_dialog_hide()` opens with `if
(!priv->visible) return;`. So calling `.hide()` from inside a `response` handler returns
early and never reaches `klass->hide` — the file's one `[data->panel close]` is
**unreachable from application code on the dismissal path**.

A future agent will read an attributed leak and reach for a local fix. There isn't one. Two
rejected app-side routes are recorded so they are not re-proposed:

- *A macOS seam that reaps non-visible `NSSavePanel`s after a response.* Rejected on
  POLICY's platform-seam bar — a seam supplies a source or a transport the toolkit does not,
  and this is our code cleaning up after a toolkit bug. Its supposed benefit came from the
  retracted figures above, so it is not known to recover anything.
- *Dropping the `GtkFileFilter` from both call sites.* Rejected — it trades a user-facing
  capability, and its supposed benefit came from the same retracted set.

**The standing constraint on this codebase**, which outlives the leak: never call `hide()`
or `destroy()` on a **visible** native dialog. Both call sites
(`window/export.rs`'s `choose_destination`, `app/appactions.rs`'s open action) go through
`saferizer::native_dialog::NativeDialogHolder` and destroy from the `response` handler,
which is the safe shape. The unsafe one is reachable from ordinary code elsewhere in the
ecosystem: `gtkfiledialog.c`'s `cancelled_cb` → `response_cb` → `gtk_native_dialog_destroy`
→ dispose, which calls `hide()` while still visible — so tripping a `GCancellable` while a
panel is up walks into the second leak above.

**"It's deprecated" is not an answer.** The deprecation is a **header-location fact only** —
the public header moved to `gtk/deprecated/gtkfilechoosernative.h`, while the implementation
and every backend, `gtkfilechoosernativequartz.c` included, remain live internals under
`gtk/`. `GtkFileDialog`, the recommended 4.10+ replacement, constructs a
`GtkFileChooserNative` in ten places and brackets its single construction site with
`G_GNUC_BEGIN_IGNORE_DEPRECATIONS`. So **every `GtkFileDialog.open()` / `save()` /
`select_folder()` on macOS runs the leaking path.**

**Vintage: present since day one, nine years.** `ff2c5e38` (Tom Schoonjans, 2017-06-30,
"GtkFilechooserNative: add macOS support") introduced the retains *and*
`setReleasedWhenClosed:YES`, with `[data->panel close]` already confined to the hide path.
The shape has never changed; the accessory-view leak joined in 2023-08-11.

**Never reported upstream — a verified negative with working controls.** Searches of GNOME
GitLab (issues and MRs, `GNOME/gtk`) for `filechoosernativequartz`, `NSOpenPanel`,
`NSSavePanel` and `savepanel` return nothing while control queries return plenty, and all
34 commits touching the file are unrelated to object ownership. Two honest limits on that
negative: GitLab search covers title and description only, not comment bodies, and Stack
Overflow could not be reached at all — the tool returned zero even for its own bare control
query, which is a silently blocked search rather than a true negative.

**And the retain itself is legitimate — nobody should "fix" this by deleting it.** Apple's
own guidance is that an autoreleased panel must be retained before display; GTK is manual
retain/release Objective-C, not ARC. Keep the retain, add the release. Apple also documents
that on the completion-handler path the panel *may still be onscreen* when the handler runs
and that `orderOut:` is the sanctioned way to dismiss it — and `orderOut:` does not consult
`releasedWhenClosed`. GTK is following Apple's advice and then never releasing.

### 📋 Upstream filing: REVIEWED AND READY — blocked on the owner's decision, not on quality

The GNOME GitLab issue body is **written, reviewed and corrected**. The gate this entry set —
no measurement-derived claim leaves here without an unlocked re-run — **has been passed**; what
remains is an authority question. **Nobody here files it**: publishing on a third-party
tracker under the operator's identity is an external, public and effectively irreversible act
in his name, and no seat holds GNOME GitLab credentials in any case (checked, not assumed —
no `glab`, no config, no `gitlab.gnome.org` entry in `.netrc` or `.git-credentials`).

Body lives in the researcher's findings doc, §11 of
`researcher-findings-quartz-filechooser-native-retain-leaks.md`. Two defects were found in
review and fixed: a **stale commit pin** — the header claimed `main` re-verified unchanged
while `main` had moved ten commits — now re-pinned as a **pair** (read at one SHA, confirmed
current at another, with the ten commits' touched files listed so the "none of these files"
claim is checkable without re-running the compare); and a function line-range that pointed at
a comment rather than the declaration.

**When it is filed, put the URL here** and this paragraph becomes a link.

**⚠️ IF IT IS FILED UPSTREAM, LEAD WITH THE `hide()` LEAK — NOT THE PANEL RETAIN.**
The panel retain is now the **weakest** of the three findings: it is arithmetically real but
recovers **zero bytes**, because `_retainedSelf` pins the object regardless. A report that
leads with it invites a maintainer to apply the fix, measure nothing, and discredit everything
behind it. The `hide()`-path leak is the opposite in every respect — it is **GTK-owned heap**
(the `g_object_ref(self)`, the `GFile`s, the filter arrays, the C strings, the struct, plus
`GtkFileChooserNative`/`GtkFileChooserDialog`/`GtkColumnViewCell` trees that never finalize),
none of it AppKit's, none self-retained, **all of it genuinely recoverable**, and measured at
~4.6 MB/cycle. Lead with what a fix actually reclaims.

**There is no upstream patch shape to record, and that is now a stronger statement than it
was.** Four successive proposals were withdrawn on further reading or measurement — the last
because a dealloc spy showed that balancing GTK's retain frees nothing while AppKit still
holds its own reference. Do not copy a patch out of this entry; there isn't one, and the
missing `release` — real though it is in source — is not known to be this issue's cause. One
sub-part is unambiguous on its own terms and worth naming: the filter control and its delegate are each an
`alloc` with no matching `release` anywhere in the file, which is a leak regardless of who
else retains the panel. It will still not produce an observable dealloc while the panel
outlives the button.

⚠️ **Do not restate this issue's cost against the 50 MiB ceiling.** That ceiling (TDD §6) is
a **VRAM** contract, and this is resident memory. The two are different budgets and
conflating them would put a false claim into an upstream bug report.

**One honest gap**, stated because it is load-bearing for anyone re-deriving this: nobody
has documented whether `-[NSWindow close]` fires a `-beginWithCompletionHandler:` block.
The docs are silent both ways; Qt and Chromium both route around `close`; and this
project's own measurement is 0/12 completions via `close` against 20/20 via `-cancel:`. The
recommendation above is built so that it does not depend on that being settled.

**Family note, so the two are not conflated.** The *portal* backend has the mirror-image
hazard — it frees its data *before* emitting the response, risking a use-after-free, where
Quartz emits before freeing. Different bug, same family: who owns the backend data across
the response emission.

So the entry stays open at Medium: the accumulation is real, reproduced by object count,
present in every shipped macOS build, and below this project's layer — but its mechanism is
**not** established, and the next person to pick it up should start from the negative
results above rather than from a cause.

**Consequence for the export rubrics**: TDD 25.15 ("an export does not move the
footprint") **passes on Windows** — 15 verified exports cost +4.3 MB against 40 chooser-only
cycles costing +5.9 MB, so the export contributes nothing distinguishable from opening the
chooser, and the per-process GPU counters read a flat 0 B throughout. It remains
**unverified on macOS**, not failing — export's own contribution to RSS
cannot be separated from the chooser's while every measurement cycle opens one. The GPU
half of that rubric *is* verified: the process never appears as a GPU client, before or
after 30 export cycles, at either document size.

---

## T. The Windows pipeline port announces carve-outs it does not apply

**Severity**: Low (inert today — the contract declares no carve-outs; it becomes a false
green on the first one added)

`scripts/pipeline.sh` and `packaging/macos/pipeline.sh` both append libtest `--skip`
arguments for a step's declared carve-outs, and both refuse a contract whose
carve-out-bearing command is not a `cargo test` invocation. `packaging/windows/pipeline.ps1`
does neither: `Show-Carveouts` prints the list and `Invoke-ContractStep` then runs the
contract command unmodified.

So the three ports agree on *what* is carved out and disagree on whether it happens. The
gap is invisible while `scripts/pipeline.steps` declares no `carveout.*` rows — which is
exactly what makes it worth recording, because the run that first exposes it is the one that
adds a carve-out, and what it prints is `skipped: <test>` for a test it ran. A Windows
release build would then carry a green verdict for a suite that never omitted anything, and
the report is the only place anyone would look.

The port's own comment used to justify announce-only as parity with the Linux port, which
was true when written; the bash ports have since gained application, so that justification
now argues the opposite of what it says. The comment has been corrected in place — this
entry tracks the behaviour, not the prose.

**Mitigation options**:
- **Port the bash implementation.** `carveout_skip_args` / `apply_carveouts` and the
  `validate_contract` rule are ~30 lines between them and the shape is already settled on
  two platforms. Needs a PowerShell host to exercise: per POLICY's rule that a gate is
  trusted only once it has been shown to fail, the `--` separator handling and the
  validation rejection both want a real run before the change is believed. That host was not
  available when this was found, which is why the entry exists rather than the fix.
- **Make it a contract-level refusal.** Have all three ports reject a `carveout.windows`
  row outright until the Windows port applies it. Turns a silent false report into a loud
  refusal at the moment someone tries to use the feature, and costs nothing to write on the
  two ports that already work. Leaves Windows unable to carry a carve-out at all.
- **Drop the announcement.** Printing nothing is at least not a false claim. Loses the
  "carve-outs: none" statement that makes the silence mean something on the other two ports,
  and buries the divergence rather than surfacing it.
- **Accept it**, on the grounds that the list is empty. The weakest option: the cost lands
  on whoever adds the first carve-out, who has no reason to suspect the report.
