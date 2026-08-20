# Plan: Externalising preview *decoration* to the theme model

**Status**: **SCOPED, NOT APPROVED** — 2026-08-18. Nothing implemented, no theme
key added, no TDD rubric landed. This file is the scope answer to "how far could
the theme model go?", and it stops at one decision only the operator can make
(§ "The blocking decision").

**Requested by**: the operator, to give custom themes more flare — glyphs,
graphics, background colours, shapes on headings and other Markdown-rendered
elements.

**⚠️ Rescoped 2026-08-20 by the delivery of export** ([`PLAN.export.md`](PLAN.export.md),
landed on Linux and verified on macOS). Nothing in the scope table below is wrong, but
its **cost model is**: a themed value no longer has three application paths, it has
**five** — the buffer tag, the table cell, the generated CSS, **the export HTML sink,
and the export PDF sink**. Both sinks already resolve real theme values today
(`export/html.rs` builds a stylesheet from the resolved `Palette` + `Theme`;
`export/pdf.rs` reads `heading_scale`, `heading_weight`, `bold_weight`, `rule_space`,
`blockquote_bar`, `blockquote_bar_width`, `rule`, `font_family`; `export/markup.rs`
reads `mark_bg`, `annotation_hl` and the link colour). Every tier below gets more
expensive, tier K+D gets a new failure mode that did not exist when this was scoped,
and the glyph-sanitisation seam gains a third escape context. The affected sections
each carry the correction inline; § "Export is two more application paths" is the
summary.

**Evidence labels used below**: *READ* = taken from the source at the cited
`file:line`. *MEASURED* = read out of the installed crate sources
(`gtk4 0.10.3`, `gsk4 0.10.3`, feature `v4_6` — this project's floor). *INFERRED*
= follows from those two but has not been rendered and looked at. No claim here
has been pixel-confirmed; that is deliberate, and § "Open questions" names the
four that must not be designed on without proof.

## Problem

[`THEMING.md`](THEMING.md) makes the preview's *appearance* data: colour,
typography and decoration **metrics** all resolve from `themes.toml`. What a
theme cannot do today is decorate — it cannot put a glyph beside a heading, a
band behind one, a shape around a code block, or a different marker in a list.
Six shipped themes therefore differ only in palette, face and spacing, which is
why Sepia and Synthwave read as the same document in two colour schemes.

The question this plan answers is not "what would look good" but **what each
rendered component can be made to support, by which mechanism, at what cost, and
what is closed off entirely**.

### Root cause of the cost distribution

The preview reaches the screen by three mechanisms
([`THEMING.md` § Three application mechanisms](THEMING.md#three-application-mechanisms)),
and **decorative headroom is inversely proportional to reach**:

| Mechanism | Reaches | Decorative ceiling |
|---|---|---|
| **A — `GtkTextTag`** | headings, bold/italic/strike, mark, sup/sub, inline code, code-block text, body links, list indents, annotation + find highlights | Text attributes only. **No shape, no padding around a fill, no border, no gradient, no image.** A tag background is a hard rectangle clipped to the glyph run. |
| **B — self-drawn (`snapshot_layer`)** | code-block card, blockquote bar, list-marker gutter, annotation chip | Effectively unlimited — gradients, rounded clips, shadows, blur, textures, arbitrary Cairo paths, Pango glyph runs (*MEASURED*, § "Technical details preserved"). |
| **C — generated CSS** | **only ten real widgets**: the page node, `> text`, `selection`, `scribtable`/`.cell`/`.cell-head`/`.cell link`, `separator.scrib-rule`, `.scrib-image-sel`, the preview cards/toast | Full GTK CSS — borders, radii, `box-shadow`, gradients. |

So the elements a reader thinks of as *the document* — headings, paragraphs,
lists, quotes, inline code — are buffer text or self-drawn, and **CSS cannot
reach any of them**. Every heading decoration is therefore either a text
attribute (A, cheap, limited) or a new drawn decoration (B, unlimited, five
coordinated edits). That single fact sets the entire cost table below; it is not
an implementation detail that a refactor could soften, because widgetising the
body to reach it is itself closed (§ "Approach 4").

## ⛔ The blocking decision — read before designing anything

[`POLICY.md`](POLICY.md) § Architecture rules bounds the theme model:

> Bounds: a theme sets the *metric of a decoration*; it does not change *what is
> drawn* or *how layout is computed*.

**Glyphs, bands and shapes cross that line by definition.** The feature as
requested is not implementable under the rule as written, and no amount of key
design routes around it. Two honest options:

1. **Keep the rule.** The feature reduces to tiers K and K+P below — richer text
   attributes and richer metrics, no new decorations. Real, cheap, and materially
   less than what was asked for.
2. **Amend it to a closed decoration vocabulary.** Proposed wording, for
   ratification, not adopted:

   > A theme selects from a **closed vocabulary of decorations the engine already
   > knows how to draw**, and states their appearance. It does not describe new
   > drawing, and it does not change layout: every decoration is either inert
   > (absent by default) or occupies space the engine reserves for it
   > unconditionally.

   This keeps the two invariants the current rule exists to protect — the engine
   holds no per-theme knowledge (TDD 18.14) and System stays byte-identical
   (TDD 18.2) — because the code path for every decoration exists whether or not
   a theme uses it, and an unset key means "not present", not "guess".

The rest of this plan is written assuming option 2 is at least a live
possibility; every tier is marked so the option-1 subset can be lifted out
cleanly.

## Scope: what each rendered component can support

*READ* — mechanism and current state from the source; headroom is the union of
what the mechanism permits (*MEASURED*) and what the component's existing seams
would accept.

Cost tiers, defined precisely in § "What each tier costs":
**K** new theme key only · **K+P** key + a tag property we do not currently set ·
**K+D** key + a new self-drawn decoration · **K+R** also needs new data out of the
renderer.

| Component | Mech | Themed today | Headroom |
|---|---|---|---|
| **h1–h5** (h6 folds to h5) | A | scale, weight, space-below, colour, family — *one* colour and *one* face for all five levels (`tags.rs:209-242`) | **K+P**: per-level colour/face; `overline`+`overline_rgba` (a rule *above* the heading — the cheapest real flare in the list); `underline`/`underline_rgba` (rule below); `letter_spacing`; `variant`/`text_transform` small-caps; `line_height`. **`heading_space_above` does not exist at all** — only space-below (`theme.rs:566-581`). **K+P**: a background **band** via `paragraph_background_rgba` — spans the text column, takes vertical padding from `pixels_above/below_lines`, survives soft-wrap as one band (*MEASURED*, § "Heading bands"). **K+D**: edge-to-edge band, rounded panel, gradient, border; a gutter glyph (§, ❧, level pips) drawn exactly as list markers are. |
| **Body paragraph** | — | **nothing — body text carries no tag at all** (`renderer/emit.rs:78-107`) | **K**: paragraph spacing, `letter_spacing`, `line_height` — but with no body tag these go on the view or need one created. |
| **Bold / italic / strike** | A | `bold_weight` only (`tags.rs:254-261`) | **K+P**: italic style, `strikethrough_rgba`, bold colour. |
| **Mark `==…==`** | A | `mark_bg`, `mark_fg` | **K+P**: highlighter underline, letter-spacing. **K+D**: a rounded or rough highlighter band — a tag background cannot have a radius or padding (ScrAP-21). |
| **Sup / sub** | A | scale, rise | Nothing material missing. |
| **Inline code** | A | background only | **Face and padding come from `config`, not the theme** (`tags.rs:301-302` → `config.rs:174-175`) — a theme cannot set the code face. **K+P**: foreground key, letter-spacing, `font_features`. A padded rounded chip is impossible as a tag → **K+D**. |
| **Fenced code block** | B + A | card fill; margins/padding from **config**; syntax theme | The card is one flat `append_color` (`codeview/mod.rs:556`). **K**: border, corner radius, gradient fill, inset shadow — all available at 4.6, all absent. **K+R**: a language-label strip or per-language accent (the fence info string does not reach the paint products). |
| **Body links** | A | colour | **K+P**: underline style (none / double / **wavy**) and `underline_rgba` (`tags.rs:342-345` hardcodes `Underline::Single`). |
| **Table** | C | cell padding, border colour/width/radius, header fill, header text colour/font | Richest surface by far — gradients, `box-shadow`, per-side borders are already expressible in the generated sheet. **K+R**: zebra striping (needs an alternate-row class from the renderer). |
| **Blockquote** | B + A | bar width, bar colour, text gap | **K**: bar radius/gradient. **K+D**: a tinted panel behind the quote; a large decorative ❝ in the gutter. |
| **Bulleted list** | B | marker colour, indent step, item gap | Bullet is a Cairo arc with a hardcoded radius (`gutter.rs:154-172`). **K**: radius. **K+D-lite**: per-depth **glyph strings** — the gutter already draws text via `append_layout` for ordered numerals, so the `Bullet` match arm swaps primitive for a near one-liner. |
| **Ordered list** | B | same | **K**: the hardcoded `6.0px` period gap (`gutter.rs:179-180`), numeral weight/scale. Numbering *format* (`1.` vs `1)` vs `i.`) is content, not style — out of scope under either policy option. |
| **Task checkbox** | B | marker colour | Everything else hardcoded: box 13px, radius 3px, stroke widths, checkmark path fractions (`gutter.rs:188-231`). **K**: size, radius, stroke, checked fill. **K+D-lite**: a glyph check (✓/✗) instead of the drawn polyline. |
| **Horizontal rule** | C | colour, space above/below | **K**: height, gradient, dashed/double via border properties. A centred-glyph rule is **K+R**. |
| **Images** | C | selection tint only (`css.rs:326-329`) | **K**: frame, radius, shadow on the overlay box. Rounded clipping of the picture *itself* needs proof — `push_mask` is 4.10 (§ "Open questions"). |
| **Broken-image placeholder** | widget | **nothing — carries no theme class**, so mechanism C cannot reach it (`renderer/start.rs:383-395`) | **K**: add a class, then style it. |
| **Annotation highlight** | A + B | the wash (`annotation_hl`) | The **gutter chip is hardcoded amber `RGBA(0.90, 0.62, 0.10, 0.95)` with white ink and unzoomed literal geometry** (`codeview/mod.rs:756-757`, `codeview/geometry.rs:187-193`). **K**: chip colour/ink/size — and doing it closes a live POLICY deviation. |
| **Find highlights** | A + Pango | both colours themed | Nothing missing. |

## What each tier costs

*READ* — these are the actual edit paths, not estimates.

**K — a new key, existing use site.** Eleven steps, none optional:
`data/themes.toml` (document + `[themes.system]` value) → `theme.rs:54-69` floor
const → `theme.rs:77-85` clamp range → `ThemeSpec` field (`theme.rs:264-349`) →
**the `take!` list in `ThemeSpec::overlay` (`theme.rs:468-514`)** → `Theme` /
`Typography` / `Metrics` field → `Theme::resolve` (`theme.rs:633-761`) → tests →
`Palette` field + `Palette::from_base` if derived (`palette.rs:59-91`,
`:150-271`) → the use site → [`THEMING.md`](THEMING.md) mechanism table.
⚠️ The `take!` step is the silent one: omitting it compiles, and built-in themes
still work while **every user-file override of that key is dropped** — the shipped
`list_marker` bug, pinned at `theme.rs:1190-1199`.
⚠️ **Since export landed, eleven steps are thirteen**: the key must also reach
`export/html.rs`'s stylesheet and, where it is a value the page sink draws or measures,
`export/pdf.rs` / `export/markup.rs`. This has the same silent-failure shape as `take!`
— omitting it compiles, the preview is correct, and the key simply does not exist in
the exported artefact. It is *worse* than `take!` in one respect: the drop is invisible
until somebody opens an exported file, which is not something the suite does for you.

**K+P — key + an unused tag property.** K, plus a setter in `tags.rs`. The
constraint is not the setter, it is § "Body/cell parity" below.

**K+D — key + a new drawn decoration.** K, plus five coordinated edits in the
mechanism-B path: a `RefCell<Vec<BufferSpan>>` field beside `blocks`/`blockquotes`
(`codeview/mod.rs:70-131`), a setter beside `set_code_blocks` (`:1136`), a line in
`install_products_into_view` (`preview/build.rs:550-570` — the deliberate single
install choke point), a scan producing the spans, **and the new vector added to
the early-return gate at `codeview/mod.rs:473-479`**. Miss the gate and the
decoration silently never paints, with no warning. The draw loop itself can reuse
`span_card_y_extent` verbatim.
**Plus the two export sinks** (§ "Export is two more application paths"): a decoration
that exists only in the B path is absent from both artefacts, so a K+D item now carries
an HTML expression and a cairo draw in `export/pdf.rs` as part of *being done*, not as a
follow-up. Call it **seven coordinated edits**, and treat the honest tier name as K+D+2.

**K+R** — K+D plus a new field on the renderer's products, and a decision about
what the parser hands forward. The export walk (`export/walk.rs`) consumes the same
event stream, so a K+R item's new data must be carried there too or the artefact sees
a construct the preview decorates and it cannot.

## Cross-cutting constraints

1. **Body / cell / export parity is a tax on every inline key.** [`POLICY.md`](POLICY.md)
   requires one key to feed every path. Table cells are Pango markup and today use
   shorthand `<b>`, `<i>`, `<s>`, `<sup>`, `<sub>`, `<tt>` (`renderer/start.rs:152-188`),
   which **cannot express** `bold_weight: 600` or `supsub_scale`. So those keys
   already silently do not apply inside tables, and every new inline key inherits
   the drift. The fix is contained — move the cell path to `<span weight=… size=…
   letter_spacing=…>` fed from the same theme values — but it is a **prerequisite**,
   not a follow-up.
   **And it is now a three-way parity, with a reuse opportunity that did not exist
   before.** `export/markup.rs` is already a second producer of Pango markup from
   themed values, and it already emits long-form `<span>` attributes rather than the
   shorthand. So the cell-path fix should **converge on that emitter rather than
   hand-rolling a third copy** — three independent Pango-markup producers reading the
   same theme is the drift this constraint exists to prevent, arriving by a different
   door. Check before writing: whichever way it lands, one inline key must reach the
   body tag, the cell markup, the HTML sink and the PDF sink, or it is themed in some
   renderings of the document and not others.
2. **Glyph strings are a new class of untrusted theme input.** Today the only
   free-form string is `font_family`, behind a type-enforced sanitiser
   (`CssSafeFontStack`, `theme.rs:177-257`). A glyph reaches a Pango *layout* in
   the gutter (plain text, safe) but the *same* value would reach Pango *markup* in
   a table cell — ScrAP-163 exactly, where an unescaped `&` renders the label
   **empty**. Any glyph key needs `glib::markup_escape_text` at the one funnel plus
   a grapheme-count clamp (a "glyph" of 500 characters is a layout blow-up).
   **Export adds a third escape context, and it is the strict one.** The same glyph
   would reach `export/html.rs`, where the correct escape is **HTML**, not Pango
   markup — a value that is safe in a Pango layout is not thereby safe in a file
   opened by a browser. `PLAN.export.md`'s untrusted-content constraint governs:
   what leaves the application is opened by software this project neither controls
   nor sandboxes, so the obligation there is *stricter, never looser*. A single
   `markup_escape_text` funnel is therefore **not sufficient** once a glyph key
   exists; each sink escapes for its own grammar, and the theme file is
   attacker-influenced input read from `$XDG_CONFIG_HOME`. Note this makes a glyph
   key the first path by which theme-supplied text reaches an exported artefact at
   all.
   **Never** let a theme name an icon (GTK4Rs/AP-48, GTK4Rs/AP-102, GTK4Rs/AP-174:
   host themes override bundled names, and the project's compile-checked `icons.rs`
   enum exists precisely to keep icon names out of data) and **never** a file path
   (ScrAP-130, plus a path-traversal surface on a file read from `$XDG_CONFIG_HOME`).
   Text glyphs are cheap and safe; icons are new plumbing — the preview paint path
   contains **no** texture or icon rendering at all today, only Pango layouts
   (`gutter.rs:177-185`, `codeview/mod.rs:769-773`).
3. **Zoom.** Every new metric is a design-time px at zoom 1.0 through
   `theme::px` ([`THEMING.md` § Pixel metrics and zoom](THEMING.md#pixel-metrics-and-zoom)).
   Note the chip path never reads zoom at all today (`gutter_zoom` is consulted only
   in the `BelowText` branch), so theming it means introducing a zoom input, not
   just a colour.
4. **System parity and legibility.** TDD 18.2 requires System to stay
   byte-identical, so every new key must default to today's rendering; any new
   ink/fill pair joins the contrast floors (TDD 18.8, 18.17) asserted at
   `theme.rs:1025-1038` and `palette.rs:400-423`.
5. **Tag hazards that decide designs rather than estimates.** *Character* backgrounds do
   not composite — highest priority wins (ScrAP-99). A **paragraph** background is a
   separate, lower layer and does compose with them (*MEASURED* — § "Heading bands"),
   so inline code inside a banded heading keeps its own fill and the band shows around
   it. `paragraph_background` is pinned **horizontally** to the text column (ScrAP-21);
   its *vertical* extent is padded by `pixels_above/below_lines`. Paragraph attributes must be applied per logical line
   (ScrAP-76) and margins compose only when accumulative (ScrAP-121). A `line_height`
   key would silently displace every list marker, because the marker clamp derives
   from `pixels_above_lines` plus a single-row Pango height (ScrAP-159). And nothing
   added to `snapshot_layer` may force layout validation (ScrAP-22).

## Export is two more application paths

*READ*, 2026-08-20, from the landed `src/export/`.

The mechanism table above answers "how does a themed value reach **the screen**". Since
export landed there is a second question with a different answer: **how does it reach the
artefact**. Two sinks, both display-free, neither of which runs any of mechanisms A, B or C:

| Sink | Reaches it by | Decorative ceiling |
|---|---|---|
| **D — `export/html.rs`** | a generated inline stylesheet over emitted HTML, from the resolved `Palette` + `Theme` | Full CSS, and **unlike mechanism C it reaches every construct** — headings, paragraphs, lists, quotes and inline code included, because the sink emits the elements itself rather than inheriting a `GtkTextView`. Structurally the *richest* surface in the project. |
| **E — `export/pdf.rs` + `markup.rs`** | Pango measurement and cairo ink onto a page; inlines via Pango markup | Whatever cairo can draw, same as mechanism B in kind. Already draws the blockquote bar and the horizontal rule from themed values. |

**Three consequences, in descending order of how much they change a design:**

1. **A mechanism-B decoration has no export representation at all, by construction.**
   The export pipeline is a function of the document *source* and never runs
   `snapshot_layer` — that is the whole reason it works on a never-rendered tab. So a
   heading band, a code-card border, a blockquote panel or a checkbox glyph added only
   to the B path **appears in the preview and is silently absent from both exports**.
   That is exactly the failure [`PLAN.export.md`](PLAN.export.md) § Risks names first
   — "two renderings of one document drift" — and its stated mitigation is structural,
   not vigilance: both sinks consume one `ExportDoc`, and the Document Rendering CAM
   carries an **export cell** so a new construct cannot land without one. **A K+D
   decoration is therefore a K+D+2 decoration**, and the CAM will say so at review
   time whether or not this plan does.
2. **CSS finally reaches the body — but only in the artefact.** Mechanism C's ten-widget
   limit, which shapes this entire plan's cost table, does **not** apply to sink D. A
   decoration that is expensive on screen (rounded heading band, gradient, border) may be
   nearly free in exported HTML. That asymmetry is a trap in both directions: it invites
   designing a decoration that is cheap in the artefact and unaffordable on screen, and it
   means "we already do this in the export" is never evidence that the preview can.
3. **The PDF resolves against the System theme's light resolution by default**
   ([`PLAN.export.md`](PLAN.export.md) O1), not the active reading theme. So a decoration's
   dark-theme appearance has no PDF expression today, and any key whose *point* is a dark
   treatment needs to say what paper does with it. That is a resolution request, not a
   licence for a literal — TDD 25.9 makes a literal styling value in either sink a defect,
   which is the same rule as this plan's, now enforced on a third and fourth path.

**The gate that catches all of it**: TDD 25.9 requires every colour, typeface and
decoration metric in an artefact to resolve through the theme engine. A new theme key
that never reaches the sinks does not fail that rubric — nothing asserts a key is
*used* — so this is a completeness obligation on the author, and the CAM's export cell
is where it is checked.

## Defects surfaced by the scoping

Real today, independent of whether this feature is built. Register entries are
**not** allocated here — [`POLICY.md`](POLICY.md) § SDD register writes gives the
registers one writer.

- **The annotation chip is hardcoded styling in rendering code** — amber
  `RGBA(0.90, 0.62, 0.10, 0.95)`, white ink, and unzoomed literal geometry
  (`codeview/mod.rs:756-757`, `codeview/geometry.rs:187-193`). A direct deviation
  from "No hard-coded styling", and it is *not* covered by `annotation_hl`, which
  themes the text wash only. **Export widened this one**: TDD 25.13 puts annotations
  in both artefacts — the claim highlight plus an aside in HTML and a margin note in
  PDF — so the chip's appearance is now a question in three renderings, and the wash
  it does *not* cover already resolves from `annotation_hl` in `export/markup.rs`.
  Whoever themes the chip should settle what the artefacts draw at the same time.
- **The broken-image placeholder carries no theme class** — unreachable by the
  generated sheet, so it stays on desktop colours on every theme (TDD 18.4's
  "no grey island" claim has a hole here).
- **The code face and its padding are `config`, not theme** (`config.rs:174-175`),
  as are the view's left/right margins baked into the `code-block` and `blockquote`
  tag margins (`tags.rs:327-332`). A theme cannot state the code face.
- **`px()` exists in three copies** — `tags.rs:192`, `theme.rs:106`,
  `preview/build.rs:588`.
- **Headings have `heading_space_below` but no `heading_space_above`**, and one
  colour/face for all five levels.

## Possible approaches

### 1. Keys only, current policy (tiers K and K+P)

Richer text attributes and metrics; no new decorations.

**Pros**: no policy change; every step is inside the existing model; the wavy/
double link underline, per-level heading colours, heading `overline`, small-caps
and letter-spacing are visible wins for one key each.
**Cons**: delivers none of "glyphs, graphics, shapes"; headings still cannot carry
a band or a mark.

### 2. Closed decoration vocabulary (adds K+D / K+R)

Amend the policy as drafted, then add decorations one at a time, each inert by
default: heading band, heading gutter glyph, list-marker glyphs, checkbox glyph,
code-card border/radius/gradient, blockquote panel.

**Pros**: delivers the request; the engine stays per-theme-agnostic; each
decoration is independently shippable and independently testable; reuses the
gutter/card seams, which are already pure functions over `(Metrics, zoom, line)`
with headless tests.
**Cons**: needs the policy ratified first; each decoration is five coordinated
edits with one silent trap (the empty gate); grows the mechanism-B paint path,
whose every addition sits on the ScrAP-22 hazard.

### 3. A drawing DSL in the theme file

Let a theme describe shapes (paths, gradients, offsets) that the engine
interprets.

**Pros**: maximum expressiveness with no code change per decoration.
**Cons**: rejected. The engine becomes an interpreter for untrusted data from
`$XDG_CONFIG_HOME`; the test surface is unbounded; a malformed theme could no
longer be *clamped* into safety, which is the property [`THEMING.md` § Untrusted
input](THEMING.md#untrusted-input) is built on; and it discards the "theme states
nothing the engine cannot already draw" invariant that keeps TDD 18.14 true.

### 4. Widgetise more of the preview so CSS can reach it

Anchor headings, quotes and code as real widgets to inherit the full CSS surface.

**Pros**: the richest styling vocabulary, declaratively.
**Cons**: rejected, and the reasons are load-bearing rather than
preferential — height-for-width block content as an anchored child re-measures at
minimum width (ScrAP-23), an anchored child inside an indented block overflows the
content column and re-arms the horizontal-scrollbar churn (ScrAP-23a), and an
overlay child's minimum feeds the view's own minimum with no opt-out
(GTK4Rs/AP-189). This route has already cost this project a rendering rewrite once.

### 5. Do nothing

**Pros**: zero risk. **Cons**: six themes that differ only in palette.

## Recommendation

**Ratify option 2, then build it in two phases — but phase 1 is worth doing even
if option 1 is chosen instead.**

**Phase 1 — no policy change required.**
1. Fix the body/cell parity prerequisite (`<span>` attributes in cell markup), so
   every later inline key lands on both paths.
2. Theme the annotation chip (closes the hardcoded-styling deviation) and give the
   broken-image placeholder a class.
3. The cheap text-attribute keys, in descending visibility: link underline style +
   colour, per-level heading colour, heading `overline`/`underline` rule,
   `heading_space_above`, heading small-caps and letter-spacing, `strikethrough_rgba`.

**Phase 2 — needs the amended policy.** Two decorations first, chosen because
they exercise both K+D shapes and prove the vocabulary before it grows:
1. **List-marker glyph strings** — the smallest possible K+D (the gutter already
   draws text; it is a match-arm swap plus a sanitised key), and it lands the glyph
   sanitisation seam that every later glyph key reuses. **Now also the case that lands
   the per-sink escaping seam** — this is the first theme-supplied *text* to reach an
   exported artefact, so it must escape for HTML in `export/html.rs` as well as for
   Pango markup, and a single funnel will not do it (constraint 2).
2. **Heading decoration band** — the marquee case, and the one that proves the
   drawn-decoration path end to end: new span collection, install choke point, the
   empty gate, gradient/rounded-clip drawing, zoom-scaled metrics. **Add both sinks to
   that end-to-end**: a CSS rule in the HTML sink and a cairo draw in `export/pdf.rs`,
   or the band is a preview-only decoration and the two renderings have drifted on the
   feature's flagship case. Note the asymmetry — the band is *cheap* in the HTML sink
   (real CSS, reaching a real `<h1>`) and dear on screen, which is the reverse of the
   intuition the mechanism table builds.

Everything else (code-card chrome, blockquote panel, checkbox glyph, table
striping) follows the same two shapes and can be prioritised by taste once those
two exist.

**One sequencing consequence worth stating plainly**: export has raised the floor cost
of this feature across the board, and it did so *after* the scoping that produced the
tier table. If the operator ratifies option 2, the honest estimate is higher than this
plan carried on 2026-08-18 — not because anything here was wrong, but because the
number of places a themed value must land went from three to five.

## Open questions — do not design past these without proof

Route to the researcher; each changes a design decision, not an estimate.

1. ~~Paragraph-background extent and compositing.~~ **ANSWERED 2026-08-18,
   *MEASURED*** — see § "Heading bands: what a paragraph background actually does".
   Both tiers are viable; which one applies depends only on whether the band must
   reach past the page inset.
2. **`line_height` semantics on a `GtkTextTag` at 4.6** — factor or absolute, and
   how it interacts with `pixels_above_lines`. The list-marker vertical clamp
   (ScrAP-159) derives from `pixels_above_lines` plus a single-row Pango height, so a
   `line_height` key could displace every marker in the document with nothing
   failing.
3. **Rounded clipping of an anchored `GtkPicture` at 4.6** — whether CSS
   `border-radius` on the overlay clips the child, or whether it takes a Cairo path
   (`push_mask` is 4.10, above the floor).
4. **Colour-font / emoji glyphs through `append_layout` under the GSK Cairo
   renderer** — a decorative glyph key invites emoji, and the software renderer's
   colour-glyph path has not been exercised anywhere in this tree.
   **Still open for the preview, but no longer unexercised in the project, and the
   adjacent evidence cuts both ways** (*MEASURED*, [`PLAN.export.md`](PLAN.export.md)
   O9 and P-17). Export drives pangocairo directly and colour emoji **render correctly
   on all three platforms** — full colour, correct glyphs, positions and order, verified
   against raster controls. That materially de-risks "will it paint". What it does *not*
   answer is the GSK Cairo renderer's own path, which is a different consumer. And it
   surfaces a consequence a glyph key must be designed against rather than discover: an
   astral colour emoji is **rasterised into the PDF and is absent from its text layer
   entirely on macOS** (embedded as an Image + SMask XObject, so there is no text-showing
   operator at all), and on Windows it becomes a Type3 `d0` font that one 2017 extractor
   mis-orders. So a decorative emoji glyph is a **picture** in the artefact, not text —
   fine for a bullet or a heading ornament, wrong for anything a reader would search for.
   Prefer a monochrome text glyph where the decoration carries meaning; note the
   operator has separately ruled that monochrome emoji may **not** be substituted for
   colour ones, since they are drawn differently rather than desaturated.

## Heading bands: what a paragraph background actually does

*MEASURED* 2026-08-18 — GTK 4.6.9, PyGObject probe, Xvfb, `GSK_RENDERER=cairo`,
`import`-captured and pixel-probed. Four runs; probe scripts are throwaway
(`/tmp`), the numbers are the record.

| Question | Answer |
|---|---|
| Horizontal extent | The paragraph's laid-out **text column**, not the widget. View 500px, margins 40/40 → band `x 40..458`. Margins 0/0 → `x 0..498`, i.e. **full view width**. |
| Does a tag margin move it? | Yes, and it **replaces** the view's rather than adding (ScrAP-121's non-accumulative rule): tag `left_margin=80`/`right_margin=30` → `x 80..468`, identical under view margins 0 *and* 40. |
| Vertical extent | The whole paragraph row **plus its line spacing**: base row `h=18`; with `pixels_above_lines=20` → `h=38`; with `+pixels_below_lines=10` → `h=48`. **So a band gets padding for free, without drawing.** |
| Soft-wrapped heading | One continuous band over **all** display rows — a 2-row heading at 1.6 scale measured `y 18..122`, `h=105`, full column width on every row. |
| Composites with a character background? | **Yes — they are different layers.** A `background`-tagged run inside a paragraph-banded line painted its own fill *over* the band (`x 43..57` yellow, band `x 58..458` around it). ScrAP-99 governs character backgrounds among themselves; it does not bite here. |
| Applied content-only (no trailing `\n`) | Still paints the full column — the ScrAP-76 per-line discipline and a paragraph band are compatible. |

**Consequence for the request.** A tag band cannot reach edge-to-edge while the
page inset lives on the view's own margins (`config.rs:163-164`, 20px each side);
it would stop 20px short and leave a page-coloured gutter. Moving that inset onto
per-block tags to get full width is **not recommended**: tag margins override
rather than add, so every block construct — body paragraphs, `li-{depth}`,
blockquote, code block — would have to restate its own margin, re-deriving the
exact arithmetic ScrAP-121 was written about.

So the two options are genuinely distinct, and neither dominates:

- **Text-column band (tier K+P)** — one key, one setter, no new drawing, vertical
  padding included, correct under wrap and under inline code. Cannot have a radius,
  gradient, or border, and stops at the page inset.
- **Edge-to-edge band (tier K+D)** — spans `0..view.width()` by construction, plus
  radius/gradient/shadow/border, at the cost of the five coordinated edits. The
  code-block card is the same technique with a different x-extent
  (`codeview/mod.rs:526-528` insets by the view margins; nothing forces that).

## TDD rubrics

None drafted. Per [`POLICY.md`](POLICY.md) and the SDD plan-kickoff rule, rubrics
for the chosen phase are proposed **before** implementation begins, not at
debrief. The existing §18 rubrics already constrain the shape of them: 18.2
(System byte-identical), 18.4 (nothing left on desktop colours), 18.8/18.17
(contrast floors), 18.10 (verify themed geometry by resolved on-screen position,
never by the key having been read), 18.14 (a new theme needs no code change).

**§25 now constrains them too**, and adds a predicate §18 has no equivalent of:
25.9 (every value in either artefact resolves through the theme engine; a literal in
either sink is a defect, and the PDF resolves against System-light by default) and 25.3
(every construct appears in the export as the preview shows it). So a decoration rubric
that asserts only an on-screen result is **half a rubric** — 18.10's "verify by resolved
on-screen position" needs a sibling that verifies the artefact, or a decoration can pass
its whole rubric set while being absent from both exported files.

## Register and skill follow-up (open)

The measurement above is **transferable GTK knowledge, not a Scribobulate fact**,
so under the register's routing rule its home is the `gtk4-rs` skill, not this
tree. It **sharpens GTK4Rs/AP-21** rather than adding a lesson:

- "A `paragraph-background` fill is pinned and cannot pad" is a **horizontal**
  statement. *Vertically* it pads exactly, via `pixels_above_lines` /
  `pixels_below_lines` — measured `h=18 → 38 → 48`. A reader who takes "cannot pad"
  as unqualified rejects a band that would have worked.
- The horizontal pin is to the **paragraph's text column**, which follows the
  *tag's* margins where it sets them (override, not sum — ScrAP-121) and the view's
  otherwise. With zero effective margins it does reach the full widget width.
- **A paragraph background and a character background are different layers and do
  compose** — the ScrAP-99 "highest priority wins" rule is about character
  backgrounds among themselves and does not apply across the two.

**Status: not yet routed.** It goes to `gtk4skiller` in the `skills` room
(ToasterTalk), never by editing the skill directly and never via a spawned
sub-agent. **ScrAP-21's own stub in this tree stays as written** — it describes the
code-block case, where the horizontal pin is exactly the point, and it needs no
edit for this. Nothing here blocks the plan; it is recorded so the finding is not
re-derived by the next person to measure it.

## Technical details preserved

*MEASURED* from the installed crates at this project's floor (`gtk4 0.10.3`,
`gsk4 0.10.3`, feature `v4_6`). Re-deriving these costs an hour.

**Mechanism A — `GtkTextTag` properties available at 4.6.** All of these are
callable today: `background_rgba`, `background_full_height`,
`paragraph_background_rgba`, `foreground_rgba`, `underline` + `underline_rgba`,
`overline` + `overline_rgba`, `strikethrough` + `strikethrough_rgba`, `weight`,
`style`, `stretch`, `variant`, `text_transform`, `letter_spacing`, `line_height`,
`font_features`, `family`/`font_desc`, `scale`, `size`, `rise`, `left_margin`,
`right_margin`, `indent`, `accumulative_margin`, `pixels_above_lines`,
`pixels_below_lines`, `pixels_inside_wrap`, `justification`, `tabs`, `wrap_mode`,
`allow_breaks`, `insert_hyphens`, `word`, `sentence`, `show_spaces`.
**The preview sets twelve of them** (`tags.rs`): `left_margin`, `right_margin`,
`scale`, `family`, `weight`, `rise`, `pixels_above_lines`, `pixels_below_lines`,
`pixels_inside_wrap`, `indent`, `foreground_rgba`/`foreground`,
`background_rgba`/`background`, plus `underline`, `style`, `strikethrough`,
`wrap_mode`, `accumulative_margin`, `priority`.
**Entirely unused, therefore free headroom**: `letter_spacing`, `line_height`,
`text_transform`, `variant`, `font_features`, `stretch`, `overline`,
`overline_rgba`, `underline_rgba`, `strikethrough_rgba`,
`paragraph_background_rgba`, `background_full_height`, `tabs`.

**Mechanism B — `Snapshot` primitives at 4.6.** Available: `append_color`,
`append_cairo` (arbitrary vector paths — already used for the bullet and the
checkbox), `append_layout` (Pango glyph runs), `append_texture`,
`append_linear_gradient`, `append_radial_gradient`, `append_conic_gradient`, both
`repeating_*` variants, `append_inset_shadow`, `append_outset_shadow`,
`push_shadow`, `push_blur`, `push_opacity`, `push_blend`, `push_color_matrix`,
`push_cross_fade`, `push_repeat`, `push_clip`, `push_rounded_clip`, `append_node`.
**Above the floor, do not reach for**: `append_fill`, `append_stroke`,
`push_fill`, `push_stroke` (4.14), `push_mask` (4.10),
`append_scaled_texture` (4.10), `push_component_transfer` (4.20). A wrapper above
the floor compiles and fails at link/runtime (GTK4Rs/AP-114).
**Currently used by the preview**: `append_color`, `append_cairo`,
`append_layout` — nothing else. There is no `push_clip`, no rounded rect, no
gradient and no texture anywhere in the preview paint path.

**Where the paint happens**: `codeview/mod.rs:453` (`snapshot_layer`), four
contiguous regions — cards `:530-557`, bars `:559-596`, gutter `:598-718`, chips
`:721-790`. Geometry is pure and headlessly tested in `codeview/geometry.rs` and
`codeview/gutter.rs`; paint inputs are bundled in `MarkerPaint`
(`gutter.rs:117-121`) specifically so new inputs do not change call-site arity;
marker kinds dispatch on a `match` over `ListMarkerKind` (`gutter.rs:153`,
`renderer/mod.rs:220-231`), which is where a glyph swap goes.

**Where the CSS happens**: `preview/css.rs::theme_css` (`:164-364`), twelve rules,
pure `fn(&Theme, &Palette) -> String`, installed app-wide at
`STYLE_PROVIDER_PRIORITY_APPLICATION + 1` (`app/setup.rs:189-206`). Invariants
asserted by tests: never `font-size` or the `font:` shorthand (`css.rs:591-606`),
never `@theme_*` in a generated rule (`:632-637`), brace balance under a hostile
theme (`:749-775`).
