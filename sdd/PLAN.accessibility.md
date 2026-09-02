# Plan: Accessibility beyond control naming

## Problem

Scribobulate exposes almost no accessible surface. The application renders documents
for a living, and **none of what it renders is reachable by assistive technology**:

- **Generated content carries no structure.** Table cells are `GtkLabel` children with
  no table/row/cell roles and no position, so a screen reader announces a flat run of
  strings with no way to know which column a value is in. Sidebar rows, tab-strip rows
  and toasts are in the same state.
- **The preview's self-drawn content does not exist to AT at all.** Task checkboxes,
  list markers, annotation chips, blockquote bars and code-block backgrounds are painted
  in `snapshot_layer`. They are not widgets, so no accessible object exists for any of
  them — including the task checkbox, which is *interactive*.

This is not an under-reviewed area, it is an unimplemented one. The priority is above
"nice to have" for a specific reason: this project has already taken a **hard abort**
from the AT-SPI stack — a `GtkWrapMode` the accessibility layer could not translate
terminated the process (GTK4Rs/AP-136; this project's own register never recorded it).
Assistive technology demonstrably
reaches this application's widgets. It is not a surface nobody touches.

**What is already done and out of scope here.** Control *naming* — the omission half —
landed separately: `src/a11y.rs` is the single choke point that sets a control's
accessible name and its tooltip together, `WidgetExt::set_tooltip_text` is banned in
`clippy.toml`, and a tree-walk integration test asserts that every icon-only control and
label-less text field in a live window carries a name. TDD §16.7 covers it. This plan is
strictly what remains.

### Root cause

Two different causes, which is why the remainder splits cleanly in two.

**Structure (Tier 2)** was simply never written. Every widget involved is a real
`GtkWidget` that GTK will happily carry roles, properties and relations for; nothing
blocks it but the work — with one open question, below.

**Self-drawn content (Tier 3)** is blocked by the rendering architecture and possibly by
the GTK floor. The content is drawn, not built: there is no object for GTK to attach an
`ATContext` to. The obvious repair — make each one a real widget at a
`GtkTextChildAnchor` — is **forbidden for the elements this tier is about**, and the
reasons are two, both **quantitative rather than categorical**:

- **(a) Minimum-width floor, proportional to the child.** `gtk_text_view_measure` takes
  `min = MAX(min, child_min)` over every anchored child and does nothing else, so a
  child sets a floor under the view's own minimum equal to its own minimum. A table
  contributes ~900px and re-arms the layout churn that blanks the view (ScrAP-23, and
  the §23 viewport-column bound around it).
- **(b) Offset shift at density.** Each anchor inserts a `U+FFFC` into the buffer, so
  converting per-item elements — list markers, task checkboxes, annotation chips, one
  per item — shifts every buffer-offset consumer. The copy map compounds it: it holds no
  node for an anchor it was not explicitly taught about, so an untaught one silently
  omits its construct from copied source.

Both bite hard for Tier 3 content at its densities, and neither is reopenable for it.

**Neither bites for a small, fixed-size, sparse control**, and that is measured rather
than argued (`probes/textview-anchored-toggle.c`, GTK 4.6.9): eight anchored ~30px
children leave the view's minimum width at **30** — `MAX`, not a sum — against **900**
for a single table-sized child, and produce `hadjustment.upper − page_size` of **0.0**,
so the ScrAP-23a overflow chain is not reachable through them. Such a child is also
reachable by Tab and activates on both Space and Enter in a **non-editable** view.

**Do not restate the old justification.** This rule previously read "an anchored child is
re-measured at minimum width", citing height-for-width, and that framing is why arguments
pitched at height-for-width kept missing the mechanism. A related trap: a bare wrapping
`GtkTextView`'s own minimum width is **0**, so a small child is not "below the view's
floor" — it *raises* the floor, from 0 to its own width. It is harmless because that
figure cannot arm the overflow chain, not because it disappears into an existing margin.

Two consequences for anything that takes this route: an anchored child sits **outside**
the view's own hover machinery (`hover_at_point`/`apply_hover` and the cursor set from
the view's motion handler, `preview/interactions.rs`), so it must carry its own cursor —
`pointer`, matching links, comment markers, checkboxes and copy buttons — or it hovers as
an I-beam and reads as unclickable; and it does not inherit the accent hover border the
drawn affordances get.

## Constraints that shape every option

- **GTK floor is 4.6** (`gtk4` 0.10 crate, no `v4_*` features; TECH.md: "4.6 works,
  ≥4.12 recommended", and the reference Linux host runs 4.6.9). Anything above that is a
  floor-raise decision with packaging consequences, not a code decision.
- ✅ **CORRECTED 2026-08-01 — row/column indexing IS expressible at 4.6. Table
  navigation is not blocked, and Tier 2 delivers the real thing.** This constraint
  previously read: *"`GtkAccessibleProperty` at this floor has no row/column index …
  so 'row 3, column 2' table navigation appears to be **inexpressible at 4.6**"*, and
  it was **wrong**. The error was looking in the wrong enum: row/column indexing is
  not a `GtkAccessible**Property**`, it is a `GtkAccessible**Relation**`, and 4.6.9
  carries the complete set. Verified twice over, on this host, both sides of the
  binding:

  - `/usr/include/gtk-4.0/gtk/gtkenums.h:1538-1554` (GTK **4.6.9**, `pkg-config
    --modversion gtk4`) declares `GTK_ACCESSIBLE_RELATION_{COL_COUNT, COL_INDEX,
    COL_INDEX_TEXT, COL_SPAN, POS_IN_SET, ROW_COUNT, ROW_INDEX, ROW_INDEX_TEXT,
    ROW_SPAN, SET_SIZE}` — including the two `*_INDEX_TEXT` variants for a
    human-readable alternative.
  - `gtk4` **0.10.3** exposes every one of them as typed variants —
    `Relation::RowIndex(i32)`, `Relation::ColSpan(i32)`, `Relation::SetSize(i32)`, …
    (`src/accessible.rs:157-173`) — reached through `update_relation(&[Relation])`
    (`src/accessible.rs:52`), with **no `v4_*` feature gate on any of them** (the
    only gate in that enum is a `v4_18` variant unrelated to tables). So neither the
    GTK4Rs/AP-94 gate hazard nor the GTK4Rs/AP-114 compile-but-fail-at-link hazard
    applies: this is usable today, on the floor, as shipped.

  Consequence for the options below: **approach 1's "Cons: table navigation may be
  flat" is void**, and approach 3's claim to "unlock the richer role/property set for
  Tier 2's tables" loses most of its force — a floor raise buys Tier 3, not Tier 2.
  `ScribTableWidget` can publish `RowCount`/`ColCount` on the table and
  `RowIndex`/`ColIndex`/`RowSpan`/`ColSpan` on each cell, which is exactly "row 3,
  column 2" navigation.

  *Lesson worth carrying past this plan (and the reason the error survived review):
  the mistaken text was specific, correctly enumerated, and verifiable — it listed the
  `GtkAccessibleProperty` members accurately. It was wrong only about **which enum
  the question belonged to**, and an exhaustive-looking list is exactly what stops a
  reader asking whether it is the right list. When concluding a capability is absent,
  the search must be shown to have covered every enum/namespace that could carry it —
  "not in X" is a claim about X, never about the capability.*
- **`GtkAccessibleText`** — the interface that lets a custom widget publish text and
  attributes to AT — is **GTK 4.14**, confirmed (researcher, 2026-08-01:
  `GDK_AVAILABLE_IN_4_14`). Tier 3 has no direct route at this floor. But note what it
  is *not*: it publishes text and text attributes, **not roles or states**, so it was
  never the single missing piece it looks like — it would not by itself have made a
  drawn task checkbox announce as "checkbox, unchecked".
- **Read-back is limited.** `gtk_accessible_get_at_context` is 4.10, so a test cannot
  read an accessible property's *value* at this floor. `gtk_test_accessible_has_property`
  and `gtk_test_accessible_has_role`/`has_relation`/`has_state` *are* present in 4.6.9
  (`nm -D`-confirmed — the ScrAP-83 discipline), which is enough to assert that
  something was set, and enough for role and relation assertions. Guards must be written
  against presence, not wording.

## Possible approaches

### 1. Tier 2 only: structure on the real widgets, accept the drawn content as a gap

Roles, properties and relations on everything that is already a widget:
`ScribTableWidget` → `Table`, its cells → `TableCell`; `TabBar` rows → `Tab` with a
selected state; sidebar rows; toasts → `Alert`; the insert dialogs' and comment entries'
`LabelledBy` relations. Document the self-drawn content as a known limitation.

**Pros**: no blocked dependency, entirely within the floor, testable headlessly with
`gtk_test_accessible_has_role`/`has_relation`. Delivers the single biggest real gain —
tables are the thing users most need announced.
**Cons**: leaves the interactive task checkbox unreachable, which is the most defensible
complaint anyone could make. ~~Table navigation may be flat (see the constraint above).~~
**Void as of 2026-08-01** — the relation set is complete at 4.6.9, so table navigation
here is the real thing, not a flat list of named cells.

### 2. Tier 2 + action-equivalence for the drawn affordances

Tier 2, plus a guarantee expressed as a rule rather than as an accessible object:
**every drawn affordance has a keyboard- and AT-reachable `GAction` equivalent.**
`win.next-annotation` already is one — and as of 2026-08-10 it is a working one in every
view mode, with a `win.prev-annotation` counterpart, which matters to this option's cost
estimate: the premise "we already have the pattern" was previously true of the
registration and not of the behaviour (the walk never advanced past the first annotation,
and did nothing at all in edit mode). The task checkbox would still need a "toggle the
task at the caret" action, and the annotation chip an "open the annotation at the caret"
one — the walk goes to the NEXT annotation, which is not the same question.
The drawn thing stays invisible to AT; the *capability* it offers does not.

**Pros**: reachable today at 4.6, no architectural change, and it is arguably the
correct design regardless — a painted rectangle is a poor accessible object even where
one is possible, whereas a named command is a first-class one. Composes with the
existing action/shortcut/menu machinery this project already insists on (ScrAP-9).
**Cons**: a screen-reader user learns the document has tasks only by trying the action;
nothing announces "checkbox, unchecked" while reading. Partial by design.

### 3. Raise the GTK floor to 4.14+ and implement `GtkAccessibleText` on the preview

Make `CodePreviewView` publish its own text, attributes and (with the newer role set)
its drawn constructs directly.

**Pros**: the only option that makes the drawn content genuinely accessible.
~~Also unlocks the richer role/property set for Tier 2's tables.~~ **Struck
2026-08-01** — Tier 2's tables need nothing from a floor raise; the relation set they
require is already complete at 4.6.9. A floor raise buys Tier 3 and nothing else here,
which materially weakens this option's case.
**Cons**: a floor raise is a distribution decision, not a code one — it strands the 4.6
Linux host TECH.md explicitly says the project actively runs on. Large, unscoped, and it
would want its own spike before anyone commits. Note it also *removes* the
`WrapMode::WordChar` AT-SPI abort (fixed in 4.8), which is a real secondary benefit.

### 4. Do nothing and close the register entry

**Pros**: honest, if the conclusion is that the floor forecloses the interesting half.
**Cons**: wrong — Tier 2 is entirely reachable today and is most of the practical value.
Only Tier 3 is genuinely constrained.

## Recommendation

**Approach 2, in two steps. ~~with a researcher round in front of it~~ — that round
is DONE (2026-08-01) and both answers are in; implementation is no longer gated on
anything.**

1. ~~**Ask the researcher two questions first**~~ — **answered, and one of them
   reversed a conclusion**:
   - *(a) at GTK 4.6, is a table cell's row/column position expressible at all?*
     **Yes — via `GtkAccessibleRelation`, whose set is complete at 4.6.9 and
     ungated in `gtk4` 0.10.3.** The plan's premise that it was inexpressible was an
     error (see the corrected constraint above, host-verified). This *raises* Tier 2's
     value: it delivers real table navigation.
   - *(b) is `GtkAccessibleText` really 4.14?* **Yes** (`GDK_AVAILABLE_IN_4_14`) — so
     Tier 3 has no direct route at this floor, as suspected. With the caveat that it
     publishes text and attributes only, never roles or states.
2. **Tier 2** — structure on the real widgets, guarded by `gtk_test_accessible_has_role`
   / `has_relation` walks in the same shape as the naming guard already in `src/a11y.rs`.
3. **Tier 3 as action-equivalence** — a `GAction` for every drawn affordance that offers
   a capability, plus an explicit, documented statement of what remains unreachable and
   why. If the researcher finds a 4.6 route to publishing drawn content, revisit.

Approach 3 stays recorded but unrecommended until someone decides the floor
independently of accessibility; it should not be the reason the floor moves.

## Sequencing and verification

- **Rubrics before code.** §16 currently carries one accessibility rubric (16.5, status
  announcements) plus 16.7 from the naming work. Tier 2 and Tier 3 each need their own
  before implementation — the SDD plan-kickoff stop applies.
- **Headless guards are real but partial.** Roles, relations and property *presence* are
  assertable under Xvfb and belong in the gated suite. They prove the markup exists; they
  do not prove a screen reader says anything useful.
- **One hands-on Orca pass per tier is mandatory, not optional.** Live AT is where this
  project's one accessibility defect actually surfaced, and it is operator time rather
  than agent time. TDD 16.5 is already on `tests/MANUAL-TEST.md`'s hands-on-only
  exception list; the new rubrics join it.
- **Budget for finding more AT-SPI bugs at 4.6.** That abort was found by an assistive
  technology reading a widget this application had not thought about. Lighting up more
  accessible surface exercises more of `gtkatspi*.c`. Expect at least one more, and treat
  a crash under Orca as an AT-SPI-translation bug to be worked around at our end (as
  that one was, by choosing a different `WrapMode`) rather than as a defect in the new
  accessibility code.

## Technical details preserved

- **The naming choke point is the template for the rest.** `src/a11y.rs` pairs a helper
  set with a `clippy.toml` ban and a live tree-walk guard. The walk is what gives the
  rule coverage that grows with the application instead of coverage equal to the widgets
  someone remembered to test — Tier 2's role/relation work should extend that same walk
  rather than add per-widget tests.
- **The method ban has a hole the walk covers.** `clippy.toml` can ban
  `WidgetExt::set_tooltip_text`, but the *builder* form (`MenuButton::builder()
  .tooltip_text(…)`) is a different path and slips through — three sites were found that
  way, and only by the tree walk (ScrAP-230). Any future ban on an accessibility-relevant setter has
  the same hole; pair every ban with a live assertion.
- **A visible label is already an accessible name.** GTK derives one from the other, so
  only icon-only controls and label-less fields need explicit naming. But a *changing*
  label is not an identity: `theme_btn` and `documents_btn` show the active theme and
  document, so both are named for what the control is, not what it currently displays.
- **Read-back symbols confirmed present at 4.6.9.** `gtk_test_accessible_has_property`,
  `has_role`, `has_relation`, `has_state` — all non-variadic and all in the 4.6.9
  `libgtk-4.so.1` (`nm -D`). `gtk_test_accessible_check_property` is present too but is
  variadic; prefer the `has_*` family from Rust.
