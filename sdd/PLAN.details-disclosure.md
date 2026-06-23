# Plan: HTML `<details>`/`<summary>` collapsible disclosure blocks

**Status**: **SHELVED, 2026-08-01** — feature approved in principle, mechanism
blocked on one unresolved measurement (below). Nothing implemented; no code, no
TDD rubrics landed.
**Researcher answers DELIVERED 2026-08-01** (Q1/Q2/Q4/Q5) — full write-up:
`~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-collapsible-disclosure-a11y-keyboard-idiom.md`.
Recorded here because the headline is a **retraction**, and a correction that lives only
in an agent's inbox is one that gets re-lost:

- **⛔ Q4 keyboard — the earlier answer was wrong, and the truth is a worse hazard.** It
  was reported that copying `GtkExpander` gets you Space but not Enter. Measured
  (4.6.9, real XTEST under Xvfb, 3 configs) that is false. A widget that only calls
  `gtk_widget_class_set_activate_signal()` gets **Space unconditionally**, but **Enter
  only while the window has no default widget** — because `activate-default` is not
  "activate the default widget": `gtk_window_real_activate_default`
  (`gtkwindow.c:2455-2464`) falls back to the *focus* widget unless that widget has
  `receives-default`. So Enter works in a bare test window and silently stops in the real
  dialog the moment a default button exists. `gtk_widget_set_receives_default(w, TRUE)`
  **alone** restores it; `GtkButton` does both that (`gtkbutton.c:435`) and five keyval
  shortcuts (`:316-325`), `GtkExpander` does neither (`gtkexpander.c:419`). **Do both, or
  compose `GtkToggleButton`.** Already carried by the `gtk4-rs` skill as GTK4Rs/AP-165.
- **Q1 prior art: effectively none.** Every GTK Markdown app renders its preview through
  a web engine, so the category solves `<details>` by delegating to a UA. Under this
  project's no-HTML-engine policy they are visual references only — take presentation
  from the platform disclosure widget and commands from editor code-folding.
- **Q2 HIG says almost nothing; the theme is precise.** Collapsed = `pan-end-symbolic`,
  expanded = `pan-down-symbolic`, 16×16 min (`_common.scss:3441-3451`). ⚠️ The RTL
  variant is a **separate icon name, not a mirror** — self-drawing the triangle means
  handling RTL yourself. No background/rule on the summary at rest, no indent or frame on
  the revealed body, and **no animation at all** (`gtkexpander.c` contains zero
  `transition`/`revealer`/`duration`) — shipping unanimated is fully idiomatic.
- **Q5 conventions**: default summary label is **"Details"**; the **whole summary line**
  toggles, not just the triangle; an empty body still renders and still toggles (do not
  special-case it — suppressing the affordance makes rendering lossy w.r.t. source); and
  **collapsed state must NOT survive a reload** — in HTML the state is the `open`
  *attribute*, a property of the document, not the session. That last one matters most
  here: honouring `<details open>` from source and treating user toggles as ephemeral
  *removes* the "key per-fold state to something stable across re-render" problem rather
  than solving it, which is directly relevant to this project's live reload.

**Requested by**: the `farming` knowledgebase agent, to collapse verbose content
(ASCII-art fallbacks beneath rich SVG versions, logs, "show your work" detail)
while keeping a document scannable.

## ⛔ The blocking question — read this before doing anything else

The recommended mechanism (`GtkTextTag:invisible`) **corrupts GTK's own AT-SPI
text interface**, on both our GTK vintages, in different ways (§ "Cost of
`invisible`…" below for the source citations). Two facts make this severe rather
than cosmetic:

- **It is the default state, not an edge case.** `<details>` renders *collapsed*
  unless marked `open`, so any document using the feature carries invisible text
  from first render. This is the normal condition for exactly the documents the
  feature exists to serve.
- **It degrades the one part of the preview's accessibility that currently
  works.** The pane's self-drawn chrome is already absent from the accessibility
  tree ([`PLAN.accessibility.md`](PLAN.accessibility.md)), but the *prose* reads
  correctly, and that is the whole value of the pane to a screen-reader user.
  There is no mitigation on our floor: `GtkTextView`'s AT-SPI implementation is
  internal at 4.6, and `GtkAccessibleText` — which we could implement ourselves —
  is 4.18+.

**But the consequence is INFERRED, not observed.** The contract violation is
verified at source; no Orca misbehaviour has been reproduced. This project's
standing rule is that a source-read is a prediction, and that rule has already
been vindicated once on this very feature (an early source-derived claim about
anchored-child painting was measured false).

**So the decisive next action is a measurement, not a decision**: render a
document containing a collapsed `<details>` and navigate it with Orca on a real
session (not Xvfb), checking whether reading position, caret and text actually
desync. It is a bounded test and it settles the mechanism outright.

- **If the desync is real** → adopt approach 2 (below). Its costs are known and
  bounded, and it deletes three hazard families at once.
- **If it degrades gracefully** → proceed with approach 1 as recommended, and
  document the contract violation as a known limitation.

A defensible alternative to running the test at all is to take approach 2 on the
**precautionary** reading — "do not corrupt the accessibility interface on an
inference we chose not to check". That is a legitimate call and needs no further
research; it simply costs the toggle latency.

## Problem

Scribobulate drops all raw HTML except `<picture>`/`<img>` — everything else is
**sanitized by omission** (§ Root cause below, ScrAP-147, TDD 2.23). A document
using `<details>`/`<summary>` therefore renders with the disclosure markup gone
and its body always expanded, losing the authoring intent entirely.

The ask is behavioural, not mechanical: a clickable summary label, a body hidden
until expanded, `<details open>` honoured, and Markdown *inside* the block parsed
as Markdown rather than shown as literal text.

Why it matters beyond one requester: it is the first raw-HTML construct we would
support that is **interactive** and **stateful**. `<picture>` resolves to a static
image at render time; a disclosure has per-block runtime state that must survive
re-render, live-reload and (optionally) the session. That makes it a template for
any future interactive embed, so the design is worth getting right once.

### Root cause

Nothing is broken — this is a missing feature — but three existing constraints
shape every option, and two of them are absolute:

1. **No HTML engine.** POLICY forbids adding WebKitGTK/Servo/litehtml. The
   preview is a `GtkTextView` subclass (`CodePreviewView`) with self-drawn chrome.
   Whatever "collapsible" means here, it is buffer text plus `snapshot_layer`
   drawing, not an embedded browser.
2. **Height-for-width block content cannot be an anchored widget.** ScrAP-23 and
   ScrAP-23a: a `GtkTextChildAnchor` child re-measures at minimum width, and one
   sitting at an indented margin overflows by exactly the indent, arming the
   marginal `Automatic` h-scrollbar and re-entering the ScrAP-22/ScrAP-23
   validation-churn blank. So the body cannot simply become a `GtkExpander`.
3. **Raw HTML is sanitized by omission, deliberately.** Extending the allowlist is
   a security posture decision, not a rendering one. `<details>`/`<summary>` clear
   that bar (structural, no scripting surface, no URL-bearing attributes, `open`
   is boolean) and the operator approved it on that basis — but the *shape* of the
   extension matters: it must remain an allowlist, never a "pass through unknown
   tags" relaxation.

## Previously attempted

Nothing shipped. One probe (`/tmp/detprobe`, GTK 4.6.9, Xvfb, `GSK_RENDERER=cairo`)
established the mechanics below; a researcher pass established the version deltas,
the absence of upstream prior art, and the call-site consequences. Both are
recorded under § Technical details preserved because re-deriving them is expensive
and two of the findings are counter-intuitive enough to be re-guessed wrongly.

## Possible approaches

### 1. `GtkTextTag:invisible` over the body, plus remove/re-add of anchored children

Summary renders as an ordinary buffer line carrying a self-drawn disclosure
triangle and a click hit-box — the exact mechanism the task-list checkboxes
already use (`codeview/mod.rs`, `checkbox_hitboxes`). The body stays buffer text;
collapsing applies an anonymous `invisible` tag over its range and **removes** any
anchored children (tables, images) in that range from the view, re-adding them on
expand.

**Pros**: body remains real buffer text, so find, selection, copy-as-Markdown and
`snapshot_layer` chrome all keep working on it; collapse/expand is cheap (no
re-render, no reflow of the rest of the document); measured to round-trip exactly;
reuses two mechanisms already load-bearing here (self-drawn affordance + hit-box,
anchored-child bookkeeping).
**Cons**: introduces invisible text into a buffer whose geometry reads are
everywhere, permanently making `line_yrange` height `0` ambiguous (§ C2 below);
requires remove/re-add plumbing with strict ordering; four GTK call-site
behaviours change silently the day it ships.

### 2. Collapsed state as a render input (re-render on every toggle)

Keep collapsed state in the per-tab typed state; on toggle, re-render the preview
with the collapsed bodies omitted from the event stream entirely.

**Pros**: correct by construction — **no invisible text ever enters the buffer**,
which deletes *three* hazard families outright: the AT-SPI offset corruption above
(the reason this plan is shelved), the §C2 zero-height ambiguity, and all the
anchored-child remove/re-add plumbing. TDD 2.24f becomes trivially true rather
than something to engineer for. Copy stays correct for free, since it already
resolves against the source Markdown.
**Cons**: our full render is ~150 ms synchronous (ScrAP-148 territory — a
main-thread block that freezes a spinner), so every toggle costs a perceptible
stall on a deliberate, infrequent action; and **preview find must become
source-aware** — it is currently a `forward_search` sweep of the *buffer*
(`window/find.rs`), so a collapsed body that is not in the buffer cannot be found
there at all. Satisfying "find auto-expands to a hit" would mean searching the
source Markdown and mapping back, which is genuinely new machinery.

**Costing corrected 2026-08-01.** This approach was initially over-penalised on
two counts. The reading-position re-anchor is **not** new work — reload, zoom,
theme and view-mode changes already capture-and-restore it, so this reuses proven
machinery. And collapsed bodies do **not** vanish from copy, because copy resolves
against source (§C4). The genuine new cost is source-aware find, which the first
costing missed entirely. Net: cheaper than first assessed, and it is the approach
that survives the accessibility finding.

### 3. Whole block as an anchored `GtkExpander`

Render each `<details>` as a real `GtkExpander` widget at a child anchor.

**Pros**: GTK does the disclosure affordance, keyboard nav and accessibility for
free; measures correctly as a widget.
**Cons**: **rejected** — this is ScrAP-23 exactly. The body stops being buffer
text, so it leaves find, selection, copy-as-Markdown and all `snapshot_layer`
chrome behind, and height-for-width content inside it re-measures at minimum
width. Loses more than it buys.

### 4. Buffer mutation (delete the body on collapse, re-insert on expand)

**Pros**: no invisible text; the buffer always reflects what is shown.
**Cons**: **rejected** — shifts every downstream character offset on every toggle
(the renderer records block positions as `BufferSpan` character ranges), tangles a
view-only affordance with undo, and re-enters the `insert_range` hazard behind
ScrAP-199.

## Recommendation

**Contingent — see the blocking question at the top of this file.**

**Approach 1**, with the remove/re-add variant rather than `set_visible(false)`,
was ratified by the operator on 2026-08-01 over approach 2, the deciding factor
being that a toggle stays instant — the property a user feels on every click.

**That ratification is now suspended.** It was made before the AT-SPI finding
existed. On the evidence as it stands the recommendation inverts to **approach
2**, unless the Orca measurement shows the desync is benign in practice. Do not
read the paragraph above as a live decision.

### Collapsed-state preview (operator, 2026-08-01)

A collapsed disclosure does not show a bare summary. It shows the summary line
followed by a **short preview of the body's opening text, terminated by an
ellipsis**, in a dimmed/secondary colour drawn from the active reading theme —
the same idiom editors use for a folded range (VS Code's `⋯`, folded-content
previews elsewhere).

Implementation note: the preview is **drawn chrome**, painted in `snapshot_layer`
alongside the disclosure triangle — *not* buffer text. This matters. Inserting
preview text into the buffer would shift every downstream character offset (the
renderer records block positions as character ranges, and copy resolves against
them), so the preview must be painted, never inserted. Its source string comes
from the fold model, which already holds the body's extent.

This was raised as a possible cure for the §C2 zero-height ambiguity. It is not —
that ambiguity comes from hidden lines *existing* and reporting `h == 0`,
regardless of what is painted beside them — and it is adopted purely on its own
merits as a UX idiom. §C2 stays handled by the rule in § Technical details
("never infer collapsed-ness from geometry; ask the fold model").

The deciding evidence is measured, not argued. All three of GTK's anchored-child
loops — `measure`, `allocate_children` and `snapshot` — iterate
`priv->anchored_children` and consult visibility **nowhere**. Emptying that list
for the collapsed range is therefore the only lever that exists, and it resolves
all three at once. Probed: min-width `900 → 0` on removal, and `900 / 152` with
`line_yrange` restored to `h:18` on re-add — an exact round-trip.
`set_visible(false)` releases the width too (`900 → 0`) but leaves the child in
the list, so `allocate_children`'s unconditional force-`validate_yrange` still runs
every cycle for content the user has collapsed.

Approach 2 remains the honourable fallback if the invisible-text ambiguity proves
worse in practice than § C2 predicts, and it is cheap to reach from approach 1
(the collapsed state lives in the same place either way).

### Design details adopted from TeplFoldRegion

`TeplFoldRegion` (libgedit-tepl, GTK3) is the only known implementation of this
idea anywhere in GNOME, and it uses precisely this mechanism
(`tepl-fold-region.c:62-64`). Three details are worth copying:

- **One anonymous tag per fold**, created on collapse and removed from the tag
  table on expand — so a document with nothing collapsed contains **zero**
  invisible tags, and every §C behaviour below is dormant rather than merely
  unexercised. This is the single most valuable detail: it makes the feature's
  blast radius proportional to its use.
- **Line-snapped bounds** on both ends.
- **`GtkTextMark` bounds, not character offsets** — marks survive edits and
  re-render; offsets do not.

Tepl never hit our anchored-child problem because it folds *source code*, which
has no child anchors. That is why this is unsolved upstream rather than merely
undiscovered.

### Behaviour decisions (agreed with the requester, pending operator ratification)

| Question | Decision | Rationale |
|---|---|---|
| Find matches inside a collapsed block | **Auto-expand to the hit** | Browser parity; a match the user cannot see is worse than an expand they did not ask for |
| Copy-as-Markdown over a collapsed block | **Include the body** | You copied the document, not the view |
| Outline sidebar, headings inside a collapsed block | **Listed** | The outline models the document, not the viewport |
| Collapsed state persistence | **Within a session: yes. Cross-session: optional polish** | Per-tab typed state (guardrail #4); session schema change is separable |
| No `<summary>` element | **Default label "Details"** | Browser convention |
| No blank lines around the body | **Render as literal text** | CommonMark/GitHub behaviour; inherited free, not a bug to fix |

**Deliberately left open until the feature exists** (operator, 2026-08-01):
whether collapsing a block *above* the viewport should hold the reading position
(a re-anchor, as a width change does per ScrAP-162) or accept the upward shift.
It is a feel judgement, not a correctness one, and it is cheaper to answer against
a working toggle than to specify in advance. TDD 2.24a therefore asserts the
toggle's *effect* and says nothing about scroll position — that clause is
**pending**, not forgotten, and is the first thing to revisit once the feature is
usable. Cross-session persistence of collapsed state remains out of scope.

## Accessibility and keyboard reach

A disclosure triangle drawn in `snapshot_layer` with a click hit-box has **no
accessible object at all** — no role, no expanded/collapsed state, nothing for a
screen reader to announce, and no keyboard route. This is not a new gap this
feature introduces: [`PLAN.accessibility.md`](PLAN.accessibility.md) already
records it for the whole preview ("Task checkboxes, list markers, annotation
chips … are not widgets, so no accessible object exists for any of them —
including the task checkbox, which is *interactive*"), and the deeper fix
(implementing `GtkAccessibleText`) is deliberately deferred there because it
requires raising the GTK floor from 4.6 to 4.14+.

So the rule for this feature is **join the existing pattern, do not deepen the
debt**. The pattern exists: the annotation markers hit precisely this problem and
supplied the missing *reach* with a **GAction** (`codeview/markers.rs`, documented
there as "This is the accessibility path"), and the accessibility plan proposes
the same remedy for the task checkbox. That also satisfies the standing rule that
one command is one `GAction` referenced by every surface.

**Therefore the disclosure toggle must be a `win.*` GAction from the outset**, with
the toggle as one surface onto it rather than the only way in. A pointer-only
toggle would be a regression against a documented project position, and
retrofitting the action later means the click handler and the action drift.

### The affordance must be a real widget, not drawn chrome (researcher-verified)

**A range of buffer text cannot carry an accessible role or an expanded state.**
The AT-SPI tree is built *solely* by walking the widget tree
(`gtk/a11y/gtkatspicontext.c`, `GetChildren` :590-600, `GetChildAtIndex`
:541-560); there is no registration mechanism for a non-widget accessible, the
lone exception being a hardcoded `GTK_IS_STACK_PAGE` case. `GtkAccessible` is
nominally a `GObject` interface, so one *can* implement it on a non-widget — and
nothing will ever enumerate it. The only per-range channel is AT-SPI text
attributes, a **hardcoded whitelist** (`gtkatspitextbuffer.c:73-137`, :179-539);
arbitrary `GtkTextTag` properties do not pass through, so `role=disclosure` is not
inventable as a text attribute.

**But this does not invalidate the design.** The dead end at
[ScrAP-23](ANTI-PATTERNS.md) is about **height-for-width** content — content whose
height depends on the width it is given (tables). A disclosure toggle is a
**fixed-size ~16×16 control** with no height-for-width behaviour at all, so it is
simply not in that category. Only the *affordance* becomes a widget; the body
stays buffer text, and find, selection and copy are untouched.

**Placement is a child anchor, not `add_overlay`** — overlay children do not track
scroll on our floor (fixed in 4.19.1, first stable 4.20.0), so every visible
disclosure would need hand-repositioning on every scroll.

#### ⚠ Unresolved conflict with `PLAN.accessibility.md`

That plan states the repair "make each one a real widget at a
`GtkTextChildAnchor`" is **"architecturally forbidden here … not reopenable for
accessibility"**, citing ScrAP-23. Taken literally it forbids the toggle above.
It should not be taken literally, and the wording there looks **overbroad**:

- ScrAP-23 is scoped, by its own title, to **height-for-width block content** —
  content whose height depends on the width it is given. A fixed-size ~16×16
  control is not in that category.
- The preview **already anchors widgets routinely** — every table and every image
  is a child at an anchor, handled by the ScrAP-23a width-bounding. So anchored
  children per se are plainly not forbidden.
- **The operative mechanism is min-width churn, and it scales with the child**
  (researcher, 2026-08-01, refining the framing above — the objection is *not*
  height-for-width, which is why an argument pitched at height-for-width kept
  missing it). `GtkTextView`'s `measure` does `min = MAX(min, child_min)` and
  nothing else, so an anchored child's contribution to the view's minimum width is
  simply its own minimum width. That is the 900→0/900 churn measured for a table.
  **A 16px toggle contributes 16px** — below the view's own minimum by an order of
  magnitude, so it moves nothing. The rule is therefore *correct for tables and
  over-general as stated*, and the reason is quantitative rather than categorical:
  the hazard is proportional to the child, and this child is tiny.

The real constraints behind that sentence appear to be two, only one of which it
states: (a) height-for-width content specifically, and (b) that converting the
*self-drawn* elements it is about — list markers, task checkboxes, annotation
chips, one per item — would insert a `U+FFFC` per element and shift every
buffer-offset consumer, which is precisely why `codeview/markers.rs` draws
annotation markers rather than anchoring them.

Both reasons bite hard for markers and checkboxes at density. Neither obviously
bites for **one fixed-size toggle per disclosure block**, of which a document has
a handful.

**This needs the operator's ruling, not an agent's re-interpretation** — the
sentence is load-bearing and was written deliberately. Either it is narrowed to
say what it means (min-width churn proportional to the child, plus
offset-shift-at-density), or the disclosure toggle is genuinely forbidden and
approach 2 becomes the only route, since a drawn affordance cannot be made
accessible at all. Flagged rather than resolved.

*Status note (2026-08-01): the researcher who recommended the anchored toggle has
since acknowledged the collision and explicitly declined to overrule the project's
rule — "your operator's call, not mine" — while pointing out that the rule's stated
justification (height-for-width) is not the mechanism that actually bites. So what
is on the table is a narrowing of the **justification**, which the measurement
above supports, not a weakening of the **rule**, which nobody is asking for.*

**Recipe, from `GtkExpander` (`gtkexpander.c`, 4.6.9 — the canonical GTK4
disclosure):** `ACCESSIBLE_ROLE_BUTTON` (`:421`; ARIA's disclosure pattern is
likewise "a button with `aria-expanded`", so this maps cleanly),
`ACCESSIBLE_STATE_EXPANDED` initialised at `:469` and updated on every toggle at
`:923`, plus an accessible name. All available in gtk4-rs 0.10.3.

#### Keyboard activation is NOT free — install the keyvals explicitly

A custom toggle widget does not inherit `GtkExpander`'s keyboard behaviour, and
the failure mode is the quiet kind. **Install the five activation keyvals
yourself**, exactly as `gtkbutton.c:194-195` does: `space`, `KP_Space`, `Return`,
`ISO_Enter`, `KP_Enter`.

The reason is worth stating, because the obvious mental model is wrong and this
project was briefly working from a wrong version of it (researcher retraction,
**measured**, 2026-08-01 — an earlier claim here held that a focusable widget
simply "gets Space but never Enter", and that is not what happens):

> `activate-default` **falls back to the focus widget** when the window has no
> default widget (`gtkwindow.c:2455-2464`). So on a bare window, **Enter appears to
> work** — it reaches the focused toggle and activates it. The moment anything sets
> a default widget on that window, Enter fires *that* instead, and the toggle
> **silently stops responding to Enter** with no warning and no error.

So the hazard is not "Enter doesn't work". It is that **Enter works during
development and stops working in the presence of a dialog or any other default
widget** — an intermittent, environment-dependent regression that a test written on
a bare window would pass. Space is unaffected either way.

Two consequences for this plan: a rubric asserting Enter-activation must set a
default widget on the window first, or it is asserting the fallback rather than the
binding; and the keyval installation is not optional polish that can follow the
first working version — it is the only thing that makes Enter *reliable* rather than
incidental.

Two gaps to accept and document rather than solve:

1. **`RELATION_CONTROLS` cannot be expressed.** `GtkExpander` points it at the
   disclosed *widget*; our disclosed content is buffer text with no accessible
   object, so the relation has no valid target.
2. **The toggle is not in text reading order.** GTK 4.6 has no AT-SPI Hypertext
   implementation at all, which is what would bind an embedded object into the
   text stream at its `U+FFFC`, so a screen reader reaches the toggle by **Tab**
   rather than by encountering it while reading. Not fixed by upgrading — at
   4.22.4 `GtkTextView` implements `ACCESSIBLE_TEXT` but not
   `ACCESSIBLE_HYPERTEXT`. The same limitation already applies to our anchored
   tables and images.

### Cost of `invisible` we did not know when the mechanism was chosen

Adopting `invisible` **breaks GTK's own AT-SPI text interface**, independently of
the disclosure affordance. The desirable half is free: collapsed text is **not**
read aloud, because every AT-SPI text getter passes `include_hidden_chars = FALSE`
(`gtkatspitextbuffer.c:744`, `:837`, `:935`, `:983`; `gtkatspitext.c:842`).

The offset space, however, is inconsistent — and differently on each vintage:

- **4.6.9** — `GetText(a,b)` indexes by **raw** buffer offset but returns
  **filtered** text (`gtkatspitext.c:839-842`), while `CharacterCount` is raw
  (`:1225-1230`). So `GetText` returns *fewer* than `b − a` characters.
- **4.22.4** — the inverse. `get_contents` takes raw offsets and returns filtered
  text (`gtktextview.c:10703-10716`), the caret position is a raw offset
  (`:10753-10762`), but `GtkTextView` does not override `get_character_count`
  (`:11109-11119`), so the default derives a **visible** count
  (`gtkaccessibletext.c:214-229`). **`CaretOffset` can therefore exceed
  `CharacterCount`.**

Verified at source level. The practical consequence for any given AT client is
**inferred, not observed** — no Orca misbehaviour has been reproduced. It arises
only while something is collapsed, in a pane whose self-drawn content is already
documented as absent from the accessibility tree.

## Technical details preserved

Measured on GTK **4.6.9** (Xvfb, `GSK_RENDERER=cairo`) unless noted. Version
deltas researcher-sourced from a 4.22.4 tree.

**The parser already does the hard part.** With the blank lines CommonMark
requires, pulldown-cmark emits `HtmlBlock(<details><summary>…</summary>)`, then
**ordinary Markdown events** for the body, then `HtmlBlock(</details>)`. The body
needs no special rendering path at all — the work is carrying pairing state
*across* those blocks, for which `<picture>`'s cross-event grouping
(`renderer/start.rs::feed_html`, ScrAP-147) is direct prior art. Nesting needs a
stack rather than a flag.

**`invisible` is genuinely implemented, despite the folklore.**
`gtktextlayout.c:2377`'s `if (!style->invisible)` drops the segment when the line
display is built; `totally_invisible_line` (`:1155`, called `:2167`/`:2319`)
short-circuits a wholly-invisible line; `:3266` confirms `display->height == 0`
means invisible. Measured: view min-height `152 → 76` px, and a screenshot of the
collapsed state shows only the summary line.

**The same guard covers the child-anchor arm** — so an anchored child inside a
collapsed range is dropped from the layout and never re-positioned.

**But three loops ignore visibility entirely**, and this is the load-bearing fact:

| Loop | 4.6.9 | 4.22.4 | Consults visibility? |
|---|---|---|---|
| `gtk_text_view_measure` | `:4306` | `:4567` | **No** — `min = MAX(min, child_min)` over every anchored child |
| `gtk_text_view_allocate_children` | `:4406` | `:4682` | **No** — force-`validate_yrange`s around every anchor |
| `gtk_text_view_snapshot` | `:5920` | `:6228` | **No** — `gtk_widget_snapshot_child` for every anchored child |

Consequences: a collapsed 900 px child keeps the view's min-width at 900
(measured `900 → 900`); collapsing buys no validation saving; and **a collapsed
child is hidden only by being parked off-canvas**, never by being skipped in paint.

**Parking is version-dependent — a real three-vintage split.** `allocate_children`
parks a collapsed child at negative coordinates. On ≤ 4.20.0 that is
`x = -child_width`, flush against the origin; commit `a4b4fec72e` (Matthias
Clasen, 2025-09-02, "textview: Move anchored children further away", issue #7717,
first tagged **4.20.1**) changes it to `x = -1000 - child_width` because, in
upstream's own words, a child at just-outside-origin "runs the risk of it getting
inadvertently exposed via e.g. css padding". Since nothing tests visibility in the
paint path, on our 4.6 floor a collapsed child's only protection is that flush
park. Removing the child from the list sidesteps this entirely — which is a second
reason to prefer remove/re-add over `set_visible(false)`.

**Measured park behaviour** (corrects an early wrong reading of mine): after
collapse and *real frame ticks*, the child's allocation is `x:-900 y:-40` —
parked, as the source predicts. An earlier read showing `x:0 y:72` was taken after
a tight `MainContext` pump rather than a genuine `size_allocate`, and was simply
premature. **Do not read allocation state from a pump loop.**

**§C — what adopting `invisible` actually changes.** The plain iterator API is not
"changed" by invisible text; it was never visibility-aware. GTK keeps visibility
in a *parallel* function set (`get_visible_text`, `forward_visible_line`,
`forward_visible_cursor_position`, …), so the ordinary functions never branch on
it. **Character-offset arithmetic and `select_range` need no audit.** Exactly four
call-site classes change:

1. `forward_search` — `gtktextiter.c:5023`/`:5347`,
   `visible_only = (flags & GTK_TEXT_SEARCH_VISIBLE_ONLY)`. Default is **off**, so
   preview find *will* match inside collapsed blocks and scroll to text the user
   cannot see, unless we auto-expand.
2. `gtk_text_buffer_get_text(…, include_hidden_chars)` — `gtkbuffer.c:2211-2214`
   dispatches to `get_text` vs `get_visible_text`. Copy-as-Markdown wants `TRUE`.
3. The `GtkTextBuffer:text` **property** hardcodes `FALSE` (`:1040`) — it silently
   drops collapsed content.
4. **GTK's built-in Ctrl+C already drops hidden text.** The registered
   `TEXT_BUFFER → text/plain` serializer calls `gtk_text_iter_get_visible_text`
   (`gtkbuffer.c:395-405`). The day this ships, any copy path serviced by GTK's
   own serializer silently omits every collapsed block, with no code change from
   us. **Verify which path services Ctrl+C in the preview before shipping.**

Keyboard navigation is free: `GtkTextView`'s own motion uses the `_visible_`
variants throughout (`:6410`, `:6424`, `:6471`, `:7288`) and vertical motion
refuses zero-height lines (`gtktextlayout.c:3190-3210`).

**§C2 — `line_yrange` height `0` becomes ambiguous, permanently.** Measured: a
collapsed line returns `y:18 h:0`, indistinguishable from "unvalidated", which is
the other producer of `h == 0` (`gtktextlayout.c:3190`). We are **not** exposed
today — our painters gate on offset ranges (`first_line < vis_start || > vis_end`)
rather than retrying on zero height, and the one convergence poll
(`converge_and_scroll_to_offset`) breaks on `upper` stability plus a monotonic
deadline and a generation token, never on a geometry height. But the next author
who writes "height is 0, retry next frame" geometry polling will spin forever on a
collapsed line. This needs an anti-pattern entry and a guard, not a prerequisite
refactor.

What gives it teeth: the `y` is **plausible** — correctly ordered and monotonic
with the line above — so a collapsed line does not return an obviously-bogus rect
that a sanity check would catch. It returns a **well-formed rect of zero height**.
A "poll until validated" loop will pass every assertion its author thinks to write
and still never converge.

And the reflexive escape hatch is closed: `gtk_text_iter_get_attributes()` (which
fills `GtkTextAttributes.invisible`) is **private in GTK4** — declared only in
`gtktextiterprivate.h:47`, absent from the public `gtktextiter.h`, and
correspondingly absent from gtk4-rs (`src/auto/text_iter.rs` has no
`attributes()`). "Just ask GTK whether this character is invisible" is not
available at any price. The public discriminator is the tag itself
(`TextIter::has_tag`), and since we mint the fold tag ourselves it is exact.

**Rule to enforce: never infer collapsed-ness from geometry — ask the fold
model.** Our own fold registry answers it without touching GTK at all.

**§C4 — Ctrl+C: already immune, no work required.** The general hazard is real:
every copy route funnels through `klass->copy_clipboard` (`gtktextview.c:842`) —
the Ctrl+C/Ctrl+Insert keybindings (`:1816`, `:1829`) and the context-menu action
`clipboard.copy` (`:1568`, whose handler at `:8844-8851` only emits
`copy-clipboard`) — and GTK's default delegate serializes via
`gtk_text_iter_get_visible_text`, whose output would silently omit collapsed
bodies. Worse, with GTK's provider left in place the format is chosen by the
**receiving** application, so the same Ctrl+C would include or omit collapsed
blocks depending on where the user pastes.

**None of that reaches us.** `preview/interactions.rs::wire_copy_clipboard`
already owns the signal and calls `stop_signal_emission_by_name("copy-clipboard")`
in *every* branch — including the nothing-selected branch — so GTK's default
handler never runs and its serializer is never consulted. And the content is
resolved by `copymap::resolve` from the **source Markdown** against character
offsets, not extracted from buffer text. Since character offsets are not
visibility-aware (§C), a collapsed body is included automatically.

So TDD 2.8h is satisfied by construction and needs no new code — only a
regression test to prove it stays that way. This is a case where an existing
architectural decision (copy resolves against source, never against the buffer)
pre-paid for a hazard introduced years later.

**GTK's own caveat, verbatim and unchanged at 4.6.9 (`gtktexttag.c:740-748`) and
4.22.4 (`:672-680`)**: "there may still be problems with the support for invisible
text, in particular when navigating **programmatically** inside a buffer
containing invisible segments." Note what it actually warns about — programmatic
navigation, i.e. §C2's territory — not offsets and not search.

**No GTK4 prior art exists.** Nothing in gtk4-demo, nothing in gtk4-rs, and
GtkSourceView has no code folding at all (not in 5.4.1, not on master 5.21.1 — the
only `fold` hit in its headers is `casefold_needle`). We are inventing, not
porting.

### Plumbing notes for implementation

1. Hold a **strong Rust ref** to each removed widget while collapsed —
   `gtk_text_view_remove` drops the view's last ref otherwise.
2. **Order matters**: remove children *then* apply the tag; remove the tag *then*
   re-add children.
3. Check `child_anchor.is_deleted()` before re-adding — live-reload can delete it
   underneath us.
4. Anonymous tag per fold, removed from the tag table on expand (Tepl).
5. Mark-based, line-snapped bounds (Tepl).
6. Acceptance probe: re-run the min-width measurement; a collapsed block
   containing a wide table must report the *text* width, not the table's.

## Drafted TDD rubrics (NOT landed — reinstate on implementation)

These were written, reviewed and split with the operator, then **removed from
TDD.md when the feature was shelved** — rubrics describing behaviour the system
does not have are stale by definition and would mislead any agent that read them
as a contract. They are preserved verbatim here, ready to paste back.

On reinstatement: §2.23's raw-HTML clause must also be amended to name the widened
allowlist (`<picture>`/`<source>`/`<img>` **plus** `<details>`/`<summary>`) rather
than duplicating the invariant, and the TOC ranges for §2, §11 and §12 updated.

Note §2.24a deliberately says nothing about whether the reading position holds
when a block above the viewport collapses — that was left open by the operator to
judge against a working toggle. Approach 2 would force the question, since every
toggle re-renders.

---

### 2.24 Disclosure blocks render as a collapsed summary
- **Given** a document containing a raw-HTML `<details>` element with a `<summary>`
- **When** it is rendered
- **Then** the summary shows as a single line carrying a disclosure indicator, its body is **not** shown, and none of the raw HTML appears as literal text
- **And** the collapsed line shows a short **preview of the body's opening text ending in an ellipsis**, in a dimmed secondary colour taken from the active reading theme — so a collapsed block still hints at what it contains rather than hiding it entirely
- **And** the preview never alters the document's copyable source: copying the collapsed block yields its Markdown, with no preview text or ellipsis introduced

### 2.24a Activating a summary toggles its body
- **Given** a rendered collapsed disclosure block
- **When** the user activates its summary
- **Then** the body appears beneath the summary, the disclosure indicator reflects the open state, and activating it again hides the body

### 2.24b A disclosure marked `open` renders expanded
- **Given** a `<details open>` element
- **When** it is rendered
- **Then** its body is visible without any user action, and it can still be collapsed like any other disclosure

### 2.24c Markdown inside a disclosure body renders as Markdown
- **Given** a disclosure whose body contains fenced code, lists, emphasis, inline code, links, blockquotes, tables or images
- **When** the body is shown
- **Then** each construct renders exactly as the same construct renders at top level — the body is ordinary document content, not literal text and not a reduced subset

### 2.24d A malformed disclosure degrades predictably
- **Given** a `<details>` with no `<summary>`, or whose body is not separated by the blank lines CommonMark requires, or which is never closed
- **When** it is rendered
- **Then** a missing `<summary>` shows the label "Details"; a body without blank lines renders as **literal text** rather than parsed Markdown (matching CommonMark and GitHub — this is correct, not a defect); and an unclosed `<details>` does not swallow the remainder of the document

### 2.24e Sibling and nested disclosures toggle independently
- **Given** a document with two sibling disclosures, and a disclosure nested inside another
- **When** the user toggles one of them
- **Then** its siblings are unaffected, an inner disclosure toggles independently of its outer, and re-expanding an outer disclosure restores the inner one's **own** prior state rather than resetting it

### 2.24f A collapsed body claims no space in the pane
- **Given** a collapsed disclosure whose body contains a table or image wider than the preview pane
- **When** the document is displayed
- **Then** no horizontal scrollbar appears and the preview does not blank — content inside a collapsed block imposes no width on the pane (the ScrAP-23a over-wide chain must not be reachable through collapsed content)

### 2.8h Copying across a collapsed disclosure includes its body
- **Given** a selection that spans a collapsed disclosure block
- **When** the user copies
- **Then** the clipboard contains the Markdown source **including** the collapsed body and its `<details>`/`<summary>` markup — a copy reflects the document, not what happens to be on screen

### 11.9 Find reaches a match inside a collapsed disclosure
- **Given** a document with a collapsed disclosure whose body contains the search term
- **When** the user searches for that term
- **Then** the disclosure containing the match expands, and the match is scrolled to and highlighted like any other — a match is never reported at a location the user cannot see

### 12.21 The outline includes headings inside collapsed disclosures
- **Given** a document with headings inside a collapsed disclosure block
- **When** the outline is shown
- **Then** those headings are listed in order like any others, and activating one expands its disclosure and navigates to it — the outline models the document, not the viewport
