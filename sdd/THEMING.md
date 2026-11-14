# Theme awareness

How the preview's appearance is sourced, resolved, applied to the screen, and
re-applied when the desktop changes. [`TECH.md`](TECH.md) places this in the
architecture and names the modules that own it (`theme/`, `palette.rs`,
`tags.rs`, `preview/css.rs`, `colorscheme.rs`); the rules themselves live here
because they are long and consulted as a unit. The theme *file format* — every
key with its type, default and range — is [`SCHEMA.md`](SCHEMA.md); the binding
rules ("No hard-coded styling", "One theme key, every application path") are
[`POLICY.md`](POLICY.md).

**This document also carries the zoom rules**, which are otherwise an unrelated
concern. Theming and zoom are independent features that meet in exactly two
places — the CSS cascade and the pixel-metric arithmetic — and at both the
contract is a single invariant *about both* (the two providers write disjoint
properties; the theme owns SCALE and never SIZE). Split across two documents,
each would hold half an invariant; so it is stated once, here.

| Section | Covers |
|---|---|
| [Resolution order](#resolution-order) | Which link supplies a key's value |
| [Search path](#search-path) | Where `themes.toml` is read from, and the XDG trap |
| [Four application mechanisms](#four-application-mechanisms) | How a key reaches the screen |
| [Zoom, and why the theme owns SCALE but never SIZE](#zoom-and-why-the-theme-owns-scale-but-never-size) | The theme/zoom CSS boundary |
| [Pixel metrics and zoom](#pixel-metrics-and-zoom) | Design-time px, scaled explicitly |
| [Untrusted input](#untrusted-input) | A theme is data from disk |
| [Syntax palette: page luminance vs desktop luminance](#syntax-palette-page-luminance-vs-desktop-luminance) | Which lightness each surface follows |
| [Theme change detection and re-render](#theme-change-detection-and-re-render) | Where the change notification comes from per platform |
| [Decoded sprites live with the active theme](#decoded-sprites-live-with-the-active-theme) | When a theme's rasters leave memory |

---

The preview's appearance is **data**. Everything it renders — colour, typography,
and decoration geometry — is sourced from the active *reading theme*, and the
engine (`theme/`) holds no per-theme knowledge: no colour constants, no
`if theme == "sepia"` branches. Themes live in `data/themes.toml`, installed
alongside the app and compiled in (`include_str!`) as the last-resort fallback, so
a missing or malformed themes file can never prevent startup.

The rest of the app — toolbar, tab strip, outline sidebar, editor — stays on the
desktop GTK theme. Theming those is the window manager's job (POLICY scope
decision), so the reading theme is **preview-only**.

## Resolution order

Each key resolves by the first link that supplies it: **(1)** the selected theme,
**(2)** `[themes.system]` — the register of every styling value the app would
otherwise hardcode, so reading that one table answers "what does this app
hardcode?" — then **(3)** the desktop GTK theme probe + derivation
(`Palette::from_base`'s blend math and WCAG contrast walk).
[SCHEMA.md § Key resolution](SCHEMA.md#key-resolution) is the full order, including
how a per-level or per-depth key (`heading_color_h2`, `list_marker_color_3`) sits
inside links 1 and 2 — one walk in `spec::Sources`, so every key of every kind falls
back the same way.

Selecting **System** means links 2 and 3 only, which is byte-identical to the
pre-theming rendering (TDD 18.2). A theme therefore states only what makes it
distinctive; Sepia sets a page, a face, an accent and three highlights, and
*derives* its links, code fills, bar and table chrome from those for free.

**A key this build does not know, or a value of the wrong type, costs that key alone**
— dropped and logged at `warn`, naming the theme and the key, never the theme or the
file. The check is possible at all because the vocabulary is a registry
(`theme::keys`) rather than a struct with a field per key: the same table that says a
key exists says what type it takes, how many values it carries and whether it names a
sprite, so validation, the merge, the sprite walk and the per-level fallback all read
one list instead of four that can drift apart.

**Costing a key alone includes costing it against the shipped theme.** A user file's
value its key's parser refuses (`[themes.sepia] background = "nonsense"`) does not
displace the built-in's own value in the merge (`ThemeSpec::overlay`); were it kept, the
walk would refuse it and fall to `[themes.system]` — the desktop's page, under Sepia's
dark ink (TDD 18.11). A theme the built-ins do not state has nothing to keep, and the
walk's own fall-through applies.

## Search path

[SCHEMA.md § Search path](SCHEMA.md#search-path) holds the three-row table and the
per-key merge rule. Two traps are THEMING's to state:

⚠️ **In this repo's dev loop, row 1 is not `~/.config`.** `.cargo/config.toml`'s `[env]`
table points `XDG_CONFIG_HOME` at `target/test-config` for every process `cargo` starts,
so a `cargo run` (or `cargo test`) reads
`target/test-config/scribobulate/themes.toml` and a directly-launched binary reads the
ambient one. A theme override written to the other route's directory is not on the path
at all and is dropped in silence — the same shape as the merge being broken.
`tests/MANUAL-TEST.md` §1.2a is the full table.

⚠️ Row 1 is resolved by `config::user_config_dir()` (an `std::env` snapshot taken
*before* the XCompose workaround redirects `XDG_CONFIG_HOME`), never by
`glib::user_config_dir()` — the GLib helper's process-global cache makes that
redirect and an honest config read mutually exclusive (GTK4Rs/AP-173; a `clippy.toml`
`disallowed-methods` ban enforces it). Rows 2–3 use the GLib data-dir helpers
freely (the redirect touches `XDG_CONFIG_HOME` only) and iterate
`system_data_dirs()` rather than hard-coding `/usr/share` (KDE's first entry is
`/usr/share/plasma`).

## Four application mechanisms

A theme key reaches the screen by one of four paths, chosen by what the surface
*is* — not by the key. **One key feeds every path it appears on**; the
representations differ, the source does not (POLICY "One theme key, every
application path").

| Mechanism | What it is |
|---|---|
| [A — `GtkTextTag`](#mechanism-a--gtktexttag) | Pango attributes on buffer text |
| [B — self-drawn](#mechanism-b--self-drawn) | Vectors painted behind or beside the text |
| [C — generated CSS](#mechanism-c--generated-css) | A sheet `format!`ed from the theme |
| [D — inserted shape](#mechanism-d--inserted-shape) | A paintable placed inside the buffer |

The sub-sections after the four mechanisms cover the places where they meet: the
[inline-code chip](#the-inline-code-chip-mechanism-a-resolved-per-surface), the
[code-block card](#a-chip-modulates-its-surface-a-card-occludes-it), why
[mechanism C's reach is the narrow one](#mechanism-cs-reach-is-the-narrow-one), its
[two hand-offs to B](#mechanism-cs-two-hand-offs-to-b), and the two CSS nodes that need
[both properties](#the-page-rule-styles-the-widget-node-and-its--text-child).

### Mechanism A — `GtkTextTag`

**Covers:** headings, bold/italic/strike, links, inline code, sub/superscript, list
indents, the annotation highlight, find-all.

**How:** Pango attributes, not CSS nodes. **GTK cannot style a `GtkTextTag` with
CSS**, which is why themes are TOML and not a stylesheet.

#### Headings: the per-level fold happens once

Heading colour and face are per level
([SCHEMA.md § Heading keys are per level](SCHEMA.md#heading-keys-are-per-level)).
A level the theme does not narrow folds down to the bare `heading_color`/`heading_font`,
and that fold happens **once, in `Theme::resolve`**, so the tag, the table header and
the export sinks all index an already-correct value rather than each re-deriving the
fallback. The table header's INK (`table_head_fg` → `heading_color`) folds the same way
and in the same place, which is what lets a theme fill its header without having to
pick that fill for legibility against a colour chosen for headings.

#### Headings: rules and space

A heading may also carry a **rule** (`heading_overline`/`heading_underline` +
`heading_underline_color`) and open space above itself (`heading_space_above`).

> ⚠️ **Only the UNDERLINE side takes a colour** (GTK4Rs/AP-308: GTK 4.6 double-frees a
> run carrying a coloured overline *and* a coloured underline, per RUN, so two tags do
> not escape it — and a link inside a heading is exactly such a run). Nothing in this
> tree may set `overline-rgba`; a `clippy.toml` ban and a live tag-table walk hold it,
> and `src/theme/model.rs`'s `HeadingRule` carries the measurement. Two tags each
> colouring an UNDERLINE — a ruled heading with a link in it — is measured clean, which
> is what makes `heading_underline_color` and `link_underline_color` safe together.

#### Links and strikethrough

- A link's underline is a themed style + colour (`link_underline`/`link_underline_color`),
  floored at the single line the app has always drawn rather than at `none`.
- The strike line takes `strikethrough_color`; the body tag, the table-cell span and
  both export sinks read that one key.

#### The disclosure summary's ink

`disclosure_fg` is a tag for the same reason its BAND is not — *ink is a priority
question where a fill is an extent one.*

- It rides `disclosure-ink`, registered immediately after `blockquote-ink`, which
  settles the two orderings that are live: a summary inside a quote takes the narrower
  of the two inks, and `disclosure-preview` (registered after it) still dims a collapsed
  block's body preview.
- ⚠️ The label itself is ONE PLAIN RUN on every surface — `<summary>`'s text arrives
  inside raw HTML and never reaches the inline event path — so there is no link or
  `==mark==` in it to lose to yet; siting the ink at the bottom of the stack is what
  keeps that a renderer question.

#### The quote panel's ink, and the `bq-{depth}` tag family

The **quote panel's INK** (`blockquote_fg`) is a tag decoration. ⚠️ Its FILL
(`blockquote_bg`) is NOT, and the split is load-bearing — see
[mechanism B](#the-quote-panel).

- The ink rides `blockquote-ink`, its own tag registered before every other ink-setting
  tag, so a link, a heading or a `==mark==` inside a quote keeps its own colour.
- A `foreground` on the quote's own margin tag would have been one line and would have
  repainted all three, because that family (`bq-1`…`bq-6`, one per nesting depth, each
  carrying that depth's full indent) must be registered AFTER `link` for its margin to
  beat a code block's inside a quote, and the highest-priority tag that sets an
  attribute wins.
- That same priority ordering is what resolves nesting: the family is registered
  deepest-last, so a line inside a nested quote carries every enclosing level's tag and
  GTK simply picks the deepest one — no per-line depth arithmetic exists anywhere.
- It must stay **NON-accumulative** for the code-block reason above; making it
  accumulative would silently shift every quoted code block right by
  `code_block_padding`.

### Mechanism B — self-drawn

**Covers:** code-block panel, blockquote accent bar **and quote panel**, list-marker
gutter (bullet / numeral / task checkbox), the annotation chip, the heading band, the
disclosure summary band.

**How:** `CodePreviewView::snapshot_layer`, from `Palette` + `theme.metrics`.

> ⚠️ **Every drawn decoration's span vector must be in `snapshot_layer`'s early-return
> gate.** A drawn vector left out of that gate paints only on documents that happen to
> carry some OTHER decoration, silently. The heading band and the disclosure summary
> band below are both in it.

#### List markers

- The marker *glyph* takes the optional `list_marker_color` key (one key for all three
  kinds; unset ⇒ the widget foreground); it colours the glyph only — the item text is
  buffer content, unaffected.
- A theme may also stand its own **glyph string** or a **sprite** in for any of the
  drawn markers (`list_*_glyph`, `list_*_sprite`); a sprite outranks a glyph for the
  same marker, and the precedence is decided in ONE display-free module
  (`theme::decor`) that the drawn gutter and both export sinks read, so the screen and
  the artefacts cannot answer different keys.
- A marker glyph is the first theme-supplied TEXT to reach an exported artefact, so it
  carries its own escaping seam — see [Untrusted input](#untrusted-input).

**`theme::decor` owns every decoration's precedence, not just the marker's**: the
sprite-outranks-flat rule and the per-decoration differences (the band is three-way,
the bar two-way, the chip two-way with its ink still on top) live in one place rather
than once per renderer per decoration. Each shape is a struct of **ordered
candidates**, not an enum of one winner: a consumer tries the sprite and falls to the
next rung if it could not be produced, which is what stops a decode failure *erasing*
a decoration instead of degrading it.

#### The blockquote bar

The bar may be filled by a tiled sprite (`blockquote_bar_sprite`) instead of its flat
`blockquote_bar_color`; the sprite outranks the colour, as for every sprite-vs-flat
pair above. Every path states that with an **explicit branch** rather than leaving it
to composition: painting the fill and then the tile over it looks identical for an
opaque tile and lets the flat colour bleed through a transparent one, which is a
defect only the sprites nobody tested would reveal. The tile is clipped to the bar's
own width, so a theme using one sizes `blockquote_bar_width` to the tile.

#### Bullets: three nesting-depth tiers

The **bullet** — and only the bullet — states its colour, glyph and sprite in three
**nesting-depth tiers** (depth 1, depth 2, depth 3-and-deeper:
`list_marker_color_2`/`_3`, `list_bullet_glyph_2`/`_3`, `list_bullet_sprite_2`/`_3`).

- Each tier falls back to the next *shallower* one, so stating only the depth-2 key
  colours every depth from 2 down, and stating none leaves every tier on the bare key —
  which is exactly the behaviour that existed before the tiers did.
- As with the per-level heading fold, the fold happens **once, in `Theme::resolve`**,
  and `theme::depth_tier` is the single definition of which tier a depth reads; the
  gutter, the HTML sink and the PDF sink each index, none re-derives the fallback.
- An ordered numeral and a task box stay single-valued at every depth: they are the
  same marker wherever they sit, where a bullet dot's job is to say which level you are
  on.
- In the HTML sink the tiers become depth-scoped selectors (`ul > li`, `li ul > li`,
  `li li ul > li`) and CSS specificity does the depth arithmetic — bullet-scoped
  deliberately, since a bare `li li` would catch a nested numbered item too.

#### The task checkbox

- It may state its own colour (`list_task_marker_color`, one key for both states — the
  glyph carries the state, the colour carries the identity), folding to
  `list_marker_color` when omitted, while bullets and numerals keep the shared key.
- It may also state a **tick** (`list_task_tick_glyph`, TDD 18.63) — a glyph drawn
  *inside* the checked box in place of the checkmark, the box itself staying drawn.
  Unlike the marker glyphs it substitutes for nothing, so it yields to a state's own
  glyph or sprite, and it makes the HTML and PDF sinks draw both boxes themselves
  (`taskbox`), since neither an `<input>` nor a `☑` character can hold a glyph.

#### The heading band

Keys: `heading_band_color`, `heading_band_gradient_to_color`, `heading_band_radius`,
`heading_band_sprite` — each per level.

- **Extent: the CONTENT COLUMN** — the same extent the code-block card uses, not the
  text column a `paragraph_background_rgba` tag would pin it to. A tag band follows the
  TAG's margins, so a heading inside a quote or a list would band at a different width
  from its siblings, and the content column is the one extent all three renderings can
  agree on.
- A soft-wrapped heading gets one continuous band for free, because the extent comes
  from `line_yrange`, which spans every display row of the logical line. No
  display-line X is needed, which matters because at GTK 4.6 there is no way to obtain
  one on the paint path without a line-display cache insert (GTK4Rs/AP-89).
- Its text is **inset from the band** by `heading_band_padding` on each side while the
  band keeps the content column — through the heading tag's own margins, the same lever
  `code-block` and `blockquote` already use to sit their text inside the decoration
  drawn behind them, since a drawn rect cannot pad itself.
- That inset is applied **per level and only where that level has a band**: an
  unconditional heading margin would re-indent every heading in every theme, System's
  included, so the gate — not the metric's value — is what keeps TDD 18.2.
- ⚠️ Its span vector is in `snapshot_layer`'s early-return gate.

#### The quote panel

Keys: `blockquote_bg`, or `blockquote_bg_sprite` tiled in its place through the same
`widgets::tile_texture` every tiled decoration takes — and a tile ALONE makes a panel,
the fill being no precondition for it.

The panel is drawn at the CONTENT COLUMN from the OUTERMOST quote level's span, so the
bar sits on the panel's own left edge and the quoted text is inset from both edges by
that level's `bq-{depth}` tag margins.

> ⚠️ **The bars nest and the panel does not** (TDD 2.11b). The renderer records a
> `QuoteSpan` per LEVEL; each level draws its own accent bar one step further in, and an
> enclosing level's span contains the levels inside it so its bar runs PAST the nested
> region rather than stopping where it begins — while the fill is laid once, from the
> outermost level, and every level inside inherits it. Drawing it per level instead
> would step the fill right at each depth and, for a translucent `blockquote_bg`,
> composite it with itself so the inner region read darker for a reason no key asked
> for. The HTML sink needs an explicit `blockquote blockquote { background: transparent }`
> to hold the same rule, because its element selector would otherwise nest the fill for
> free.

> ⚠️ **It was a `paragraph_background_rgba` on that tag until TDD 18.29's fix**, and the
> reason it moved is the general lesson: **GTK fills a paragraph background PER
> PARAGRAPH**, so a quote holding an intro paragraph plus a nested list drew as several
> disconnected rectangles with the page showing between them, beside a bar that was
> already continuous — one quote wearing two different extents. Drawn from the shared
> span, the two cannot disagree; the vertical padding and the one-band-per-soft-wrapped-
> paragraph both survive the move because `line_yrange` already includes each line's
> `pixels_above/below_lines` and spans every display row.

Its INK stays a tag ([mechanism A](#the-quote-panels-ink-and-the-bq-depth-tag-family))
— an ink must lose to a nested link's own colour, which is a priority question, where
a fill's problem was an extent one.

#### The disclosure summary band

Keys: `disclosure_band_color`, `_gradient_to_color`, `_sprite`, `_radius`.

- It is the heading band's shape one construct over: the same content column, the same
  three-way `theme::Band` precedence, the same `decorplan::PAINT_ORDER` neighbourhood —
  on the quote panel a `<details>` can sit inside, under the accent bar and the gutter
  markers that cross its row.
- It is mechanism B and not C for the reason
  [Mechanism C's reach is the narrow one](#mechanism-cs-reach-is-the-narrow-one) gives:
  CSS reaches about ten widget nodes and a summary label is buffer text, so a line-wide
  fill has to be painted even though the SAME decoration is one
  `summary { background: … }` in the export.
- ⚠️ Its span vector is in `snapshot_layer`'s early-return gate.
- Unlike the heading band it insets no text, so it needs no padding key and no tag
  margin.

#### The annotation chip

The annotation chip is the FIRST decoration in the closed vocabulary:
`annotation_chip_bg`/`_fg`, or an `annotation_chip_sprite` file that replaces the flat
fill outright. Every key unset ⇒ the exact hardcoded amber/white the chip always used
(TDD 18.2).

### Mechanism C — generated CSS

**Covers:** the page (background/`color`/`font-family`), table cells (their ink,
borders, metrics — and their header's fill only while it is FLAT; see
[the hand-offs](#mechanism-cs-two-hand-offs-to-b)), the rule separator, the
image-selection tint, the conflict toast. (The preview's floating cards stay on the
desktop theme, so every card is one colour — TDD 17.65.)

**How:** GTK4 removed the GTK3 widget style overrides, so a widget's background/font is
**CSS-only**. CSS is a *generated artifact* here, never a source of truth —
`preview/css.rs::theme_css` `format!`s it from the theme, as `zoom_css_rule` already
did.

> ⚠️ **A `format!`ed sheet can be malformed, and GTK will not tell you.**
> `load_from_data` returns nothing and logs nothing; a declaration GTK cannot parse is
> dropped and the theme's intent simply never reaches the screen. Nor can a test that
> string-matches the generated sheet see it, because the malformed text still `contains`
> the fragments such a test looks for.
>
> So every builtin theme's sheet is loaded into a real `GtkCssProvider` with
> `connect_parsing_error` armed (`preview::css::parses`) — a helper that returns a whole
> declaration must be spliced *after* a semicolon, never into another declaration's
> value, and that guard is what proves it.

### Mechanism D — inserted shape

**Covers:** the end-of-heading marker (`heading_marker_sprite`, `heading_marker_size`).

**How:** the theme's own picture placed INSIDE the buffer, as a Pango shape, by
`renderer::emit::insert_heading_marker` — the only mechanism here that puts a decoration
into the text rather than beside or behind it.

**Why it exists:** the marker's position is *after the heading's last glyph*, and that
x is unobtainable. At GTK 4.6-4.12 the only source of an end-of-text x is a
`GtkTextLineDisplay`; its non-caching accessor (`gtk_text_layout_get_line_display`,
`size_only=TRUE`) is absent from `libgtk-4.so.1`'s dynamic symbol table so no binding or
shim reaches it, and every public path that yields an x inserts into the display cache
— GTK4Rs/AP-89's use-after-free. `gtk_text_buffer_insert_paintable` dissolves the
question instead of answering it: Pango places the shape past the final glyph run, so a
soft-wrapped heading carries its marker onto the LAST display row for free.

> ⚠️ **The cost is a `U+FFFC` in the buffer that owns no child anchor**, which the
> copymap guard must tell apart from an anchored child by asking the buffer, never by a
> list of render-time offsets (ScrAP-346).

> ⚠️ **An ANIMATED marker is an anchored `SpriteIcon` instead, at the same size and
> place** (TDD 27.9): GTK caches a text line's render node and re-snapshots a buffer
> paintable only after its `invalidate-contents`, which re-wraps and revalidates the
> line — per frame, restarting the text view's validation idles. An anchored child
> repaints alone. Its anchor carries a decoration mark, so the copymap guard gives it
> the paintable's verdict (ScrAP-346).

> ⚠️ Sized by a themed metric rather than font metrics, and that is only safe because
> **zoom re-renders the buffer** (`window::zoom::apply_zoom`); a size baked in at render
> time would otherwise go stale on zoom.

### The inline-code chip: mechanism A resolved per surface

This is the one place A and B have to agree about a colour. A chip is the surface it
sits on tinted 8% toward the ink *that surface* carries (ScrAP-356 holds the rule and
the one-tag-per-surface reason; an unknown surface such as a tile gets no chip — the
same verdict `Band::is_present` gives the band — and a surface already at the 4.5:1
floor is tinted *away* from the ink):

- a run inside a banded heading takes that level's band and ink;
- one inside a filled quote takes the panel's;
- a header cell's takes the header's fill;
- body prose (an unbanded heading and an unfilled quote included) keeps the page's,
  which is the only one a theme may state outright (`code_inline_bg`).

`palette::codechips` resolves all of them once and every sink reads it: the buffer's
tags, the table cell's Pango span, the HTML sheet's one descendant selector per surface,
and the PDF's runs. The renderer picks the surface, and `palette::surface_at` is the one
definition of the precedence (the nearer surface wins: a band beats a quote it sits
inside).

### A chip modulates its surface; a card occludes it

So they owe the surface different things.

- The fenced code block's card keeps **one colour on every surface**, because it is the
  backdrop a third-party syntax palette's token colours are tuned for, and a card that
  changed with what sits behind it would put those colours on a fill nothing has tested.
- What it owes the surface is **GEOMETRY**: a card is drawn by mechanism B, which is
  told nothing by the tags that indented the text it covers, so at the page's content
  column a quoted block's card is exactly the quote panel's own rectangle and erases it.
- The card is therefore inset by that depth's `quote_indent_px`, and the block's body
  tag is registered per depth — after the `bq-{depth}` family, so it wins the one
  `left-margin` property both set — which keeps the gap between card and code the same
  at every depth (TDD 18.62).

### Mechanism C's reach is the narrow one

The export HTML sink's is not.

- CSS here styles about ten widget nodes and reaches no heading, paragraph, list, quote
  or inline-code run — every one of those is buffer text (A) or self-drawn (B). That
  puts decorative headroom in inverse proportion to reach and sets the cost of every
  decoration in this vocabulary.
- `export/html.rs` emits its own elements rather than inheriting a `GtkTextView`, so it
  styles every construct declaratively: a decoration can be one CSS rule in the artefact
  and a new drawn vector plus its early-return gate entry on screen.

> **"The export already does this" is evidence about the artefact and none about the
> preview**, and the trap runs both ways — a decoration cheap to state in HTML may be
> unaffordable on the paint path.

### Mechanism C's two hand-offs to B

Both are worth knowing before reaching for CSS to fill any other surface. The general
rule both instance:

> **A decoration whose fill may be a compiled-in sprite cannot be carried by CSS**,
> whatever else recommends it. A GTK CSS `url()` needs a real resource or file path, and
> a built-in theme's sprite is compiled into the binary with no path anywhere
> (ScrAP-324). Giving one a path again is precisely the defect that entry records.

#### Hand-off 1: the horizontal rule

The horizontal rule is a stock `GtkSeparator` recoloured by a generated
`separator.scrib-rule` rule — but a theme may instead tile a sprite across it
(`rule_sprite`, TDD 18.31), and CSS cannot express that.

So a *tiled* rule is a different widget — `widgets::rule::SpriteRule`, which paints the
texture through `widgets::tile_texture`, the one seam every tiled sprite in this
vocabulary is painted with (the heading band and the blockquote bar take the same call;
a decoration sized by the LAYOUT rather than by its own tile takes its twin,
`widgets::draw_sprite_into`). It is built ONLY where the theme states one, so the flat
rule remains byte-for-byte the separator it always was.

#### Hand-off 2: the table header row

The **table header row** is the second, and the same rule drove it (TDD 18.57). A
table's cells are CSS-styled nodes and the header's fill was one `background-color` on
`.cell-head` — until the header gained a heading band's whole vocabulary
(`table_head_sprite`, `table_head_scene`, `table_head_gradient_to_color`, over
`table_head_bg` as the flat rung). A tile and a scene cannot be spelled in CSS here for
ScrAP-324's reason, so `ScribTableWidget` paints the band itself, in its own `snapshot`,
through the same `widgets::paint_band_into` a heading band takes.

> ⚠️ **The two mechanisms must not both paint**, and this one has a switch rather than
> a separate widget: `theme::decor::table_head_is_painted()`. The cells are CHILDREN of
> the table widget and are drawn *after* it, so a `.cell-head` background left in place
> while the widget paints covers the band completely — and the reverse, a rule that
> steps aside while nothing paints, is a header with no fill at all. Both are silent,
> and only one is reachable by a theme in today's build. So the predicate is
> stated once and READ by both sides — `preview/css.rs` to decide whether to emit the
> declaration, `widgets::table` to decide whether to paint — and neither re-derives it.
> The rule's other declarations (weight, ink, face) are unaffected either way; this
> drops a fill, not a header.

**The band's extent is the header CELL, not the header row** — one band per column
heading, so a scene appears once per heading and the gaps between columns stay
page-coloured. That is the operator's design call and also the only extent the three
renderings can share: the HTML sink's unit is the `<th>`, so a row-wide band is
inexpressible there and a preview that painted one would disagree with the artefact by
construction.

Its corner radius is `table_cell_radius` applied **unscaled**, matching the CSS-borne
metric of the very cell it sits under (see
[Pixel metrics and zoom](#pixel-metrics-and-zoom) — the one place passing `1.0` to
`band_corner_radius` is correct rather than a forgotten zoom).

### The page rule styles the widget node and its `> text` child

Mechanism C's page rule must style **both** the widget node (`color`,
`font-family` — also read by GTK's own caret/text paths) and its `> text` child
(`background-color`): styling the widget node's background alone works on GTK's
Default theme but is silently overpainted by an opaque system theme like
Breeze-Dark (GTK4Rs/AP-100). The `text` fill spans `MAX(screen, layout)`, covering the
non-node margins, so no white frame remains once `> text` is styled.

### The `selection` node needs both properties too

Same reason, and the easier one to miss, because the half that goes wrong is invisible
until something is selected: a rule stating only `background-color` leaves selected
text on the desktop theme's `theme_selected_fg_color`, which is the one ink on a themed
page the reading theme does not own.

> Measured under Bedtime with the fill themed and the foreground left alone: every
> selected glyph — body, heading and code alike — painted `#000000` at 2.1:1 on the
> fill.

`palette.selection_fg` supplies it:

- **Derived by default** — whichever of the page ink or the page itself reads better on
  the fill, walking toward white or black only if both fail AA.
- **Overridable** by a `selection_fg` key.
- The derivation is what keeps it right per theme rather than per author (the page ink
  would strand Sepia, brown on brown at 1.5:1; the page would strand Bedtime).
- Both the body buffer and the table cells' own `selection` node take it, by the
  ScrAP-36 parity rule ([`CAM.md`](CAM.md)'s rendering-parity row).

> ⚠️ **A cell does not get that for free where the body does.** A text view draws every
> selected glyph in its selection node's colour, a link tag's included; a `GtkLabel`
> keeps a link's own colour inside the selection, and a body selection spanning a table
> does not select the cells at all — its fill just shows through them.
> `widgets::table::linkink` closes both, and it READS the ink from the body's selection
> node (through stand-ins for GTK's private nodes) rather than taking `selection_fg`, so
> it is also right under System, where the desktop theme decides. It reads the BODY's
> node even for a selection made inside a cell, because a desktop theme can ink the two
> differently over one fill: GTK 4.6's Adwaita gives a label's selection white text on
> the same pale fill the body's dark selected text sits on (~1.5:1).

### Why a key on top of a working derivation

The same question applies to `selection_fg` and `mark_fg`, and the answer is the same
for both: **the derivation optimises for contrast, and contrast is not taste.**

- Bedtime's sand ink clears 5.3:1 on its violet selection band and still looks wrong
  there, warm ink on a cool fill; no ratio expresses that, so the answer has to be
  statable, not only computable.
- The reverse case is `mark_fg`: Bedtime's green band deliberately breaks the fill
  ceiling, so the body ink would read 3.58:1 on it and the band states its own ink
  instead.

A key that only ever restates the derived answer would be noise; these two exist
because the derived answer is sometimes right and unusable.

## Zoom, and why the theme owns SCALE but never SIZE

This is the load-bearing distinction of the design.

- **Pango `set_scale()`** is a tag attribute GTK **multiplies** onto the CSS base
  (`gtktextattributes.c:349-351`), so it never touches the CSS cascade and composes
  with zoom for free at any level. The theme owns `heading_scale`, `supsub_scale`.
- **CSS `font-size`** is a longhand the **zoom provider owns exclusively**
  (`zoom_css_rule`). There is deliberately **no `font_size` theme key**.

The theme provider (app-wide, unscoped, installed at `PRIORITY_APPLICATION + 1`)
and zoom's per-window provider write **disjoint** properties — theme owns `color`,
`font-family`, `background-color`; zoom owns `font-size` — so the two never
collide. This is by construction, not luck: a cross-provider conflict is arbitrated
by provider add-order, not selector specificity (GTK4Rs/AP-101), so keeping the
property sets disjoint is the only reliable defence. A test asserts the theme sheet
never emits `font-size` or the `font:` shorthand (which expands to include size).

## Pixel metrics and zoom

A theme states decoration geometry as **design-time px at zoom 1.0**. Pixel metrics
are widget/Pango properties and do **not** follow the CSS `font-size` rule, so they
are scaled explicitly on every render/zoom through `theme::px(n, zoom) =
(n * zoom).round()`. Theming swapped the *source* of the number; the zoom machinery
is unchanged. A theme never expresses "pixels at the current zoom".

**The documented exception: a metric carried by CSS is NOT zoom-scaled.** The four
table-cell metrics `preview/css.rs` writes — `table_cell_padding_v`,
`table_cell_padding_h`, `table_border_width`, `table_cell_radius` — reach the screen
as raw design-time px, so a zoomed table keeps hairline borders and design-time
padding around grown text. That is a consequence of the section above, not an
oversight: the theme provider is **app-wide** and zoom's is **per-window**, and the
two are safe only because they write disjoint property sets. Handing `theme_css` a
zoom factor would make one app-wide sheet claim a per-window value, which is the
cross-provider collision GTK4Rs/AP-101 records; and moving these four properties to the
zoom provider would give a per-window sheet a theme's value. So the rule is: a metric
applied through a **widget or Pango property** is scaled by `theme::px`; a metric
applied through **generated CSS** is not, and states its design-time value. Closing
this needs a per-window theme provider, which is a larger change than any metric here
justifies.

`list_step`/`list_item_gap` resolve once and feed **both** `tags.rs` (the
`li-{depth}` tag's `left_margin`) and `codeview/gutter.rs` (the drawn marker's x); a
themed step reaching one but not the other strands every marker ([POLICY § One theme key, every application path](POLICY.md)), so
themed-geometry tests assert the **resolved** pixel position on a realized view.

## Untrusted input

A theme is data from disk. Each kind of value it carries has its own defence.

### Geometry and colours

- Geometry is typed `i32` and **clamped**, so it cannot carry a `}` or `;` into a rule
  — injection is impossible by construction rather than by validation.
- Colours are re-emitted from a parsed `RGBA`, never echoed.

### The font stack

The one free-form string, `font_family`, is sanitised, and is **guaranteed to end in a
generic family**: fontconfig resolves an unknown family to the SANS default, not serif,
so a stack without a generic terminator silently lands on sans and defeats the theme
(`fc-match Charter` → Noto Sans).

### Glyph keys: one string, three grammars

**A glyph key is theme-supplied TEXT, and it reaches three different grammars.**

| Consumer | Grammar | Metacharacters |
|---|---|---|
| The drawn gutter | `PangoLayout` plain text | none (no parsing) |
| The PDF sink | Pango *markup* string | `<`, `&` |
| The HTML sink | HTML — and, inside it, a CSS `content:` string literal | `<`, `&` — and `"`, `\` |

**A single `markup_escape_text` is not sufficient**: an un-escaped `&` fails
`pango_parse_markup`, which renders the whole run EMPTY with no warning (ScrAP-163), and
an un-escaped `<` in HTML is an injection into a file this project hands to a browser it
does not control (TDD §25's untrusted-content rule is stricter here, never looser).

So a validated glyph is a `theme::MarkerGlyph`:

- private inner string, no `Display`, no `Deref`, one constructor;
- reachable only through projections named for the grammar they are going into
  (`as_plain`, `escaped_for_pango_markup`, `escaped_for_html`);
- the HTML projection delegates to the export sink's own escaper, so this project has
  one HTML escaper rather than one plus a copy that drifts.

Validation **refuses rather than truncates** an over-long glyph: cutting at a `char`
boundary can split a grapheme cluster and leave a lone combining mark, which renders
worse than the marker the theme was replacing.

### A sprite named by a `themes.toml` on disk

A sprite name is the untrusted case, and a materially different risk than a colour or
a font stack, because it names something on disk rather than a value re-emitted from a
parse. Which source a name resolves to — a compiled-in image for a built-in theme, a
file beside the theme file for one on disk — and every check `crate::sprite::resolve`
applies to the latter are in
[SCHEMA.md § How a `*_sprite` key resolves](SCHEMA.md#how-a-_sprite-key-resolves).
The rule THEMING adds: **a reference that fails any check is dropped to "this
decoration is absent" — the same inert-by-default behaviour an unset key gets — never a
partial render and never a path outside the theme's own directory.**

> The export HTML sink re-checks its OWN embed size cap independently rather than
> trusting `resolve`'s, since a base64 embed inflates by roughly a third on top of the
> decoded size and the two caps protect different budgets.

## Decoded sprites live with the active theme

A sprite is decoded lazily, on the first paint (or export) that needs it, into three
process-global caches in `sprite.rs` (natural texture, resampled texture, cairo
surface). **Those rasters are dropped when the active theme changes**
(`theme::set_active` → `sprite::clear_cache`), so a theme that named them no longer
keeps them resident once the reader has left it (TDD 18.58). Selecting that theme
again decodes them on the next paint; the unload is not a one-way loss. Pixel Quest
→ System is one instance of the same rule, not a special case.

The cache going empty is the first half. Widgets that cloned a texture out of it — a
heading-marker paintable in the buffer, a `SpriteRule`, a `SpriteIcon` (the disclosure
indicator, an animated heading marker) — keep that GObject alive until the theme-change
re-render destroys them (the active tab is rebuilt in place; a background tab drops its
preview and is marked deferred). Both halves have to happen: emptying the cache while
those holders still point at the rasters would free nothing a memory counter can see.

**What this does not unload.**

- Compiled-in PNG bytes stay in the binary — that is the built-in-theme-with-nothing-on-disk
  promise (ScrAP-324), a few kilobytes the OS can page out, not the decoded cost that
  scales with a user-supplied sprite.
- System fonts a theme names stay in Pango/fontconfig; those are the desktop's cache, not
  this engine's.
- GSK's last-frame nodes last until the next paint of the new theme, which the re-render
  already triggers.

## Syntax palette: page luminance vs desktop luminance

The desktop-probe derivation (resolution link 3) reads the desktop GTK theme's base
Adwaita named colours via `StyleContext::lookup_color`; SCHEMA.md lists them and the
theme keys they feed. The one architectural boundary worth stating here: the
preview's syntax palette follows the **page's** own luminance (a `syntect_theme`
key, else a by-luminance default — SCHEMA.md), whereas everything *outside* the
preview that needs lightness — the editor's GtkSourceView scheme above all —
follows the **desktop's** luminance via `palette::desktop_is_dark()`. Without that
split, a light reading theme on a dark desktop would flip the editor to a light
scheme.

## Theme change detection and re-render

`GtkSettings` `prefer-dark` and `gtk-theme-name` change notifications are
connected in `connect_startup` and call `re_render_all_windows()`, which
rebuilds every open window's buffer from retained source text — reusing the same
path as live file reload.

Document source text is retained per-window in the `winstate` registry
(`TabState.source`, keyed by tab id). File-monitor callbacks and the
editor buffer keep it in sync, so theme re-renders never re-read disk.

XDG Desktop Portal `SettingChanged` subscription is deferred — `GtkSettings`
covers the common GTK/GNOME case. Libadwaita is intentionally excluded:
`AdwStyleManager` would shorten detection but pulls in a large dependency against
the project's measured minimal footprint.

### Where the notification comes from, per platform

Something must *emit* those notifications. Where a platform supplies no source, the
fix has one shape — **supply the missing source, write the settings the desktop would
have written, change nothing downstream** — so this path is identical from
`re_render_all_windows()` onward, and the detection layer holds no theming knowledge
of its own. [TECH.md](TECH.md)'s platform table records each seam; in brief:

- **macOS:** the Quartz backend never reads the system appearance, so
  `platform/mac/appearance.rs` supplies the signal.
- **Windows:** GTK 4.22 never reads the Windows light/dark preference, so
  `prefer-dark` would never change. `platform::win32::track_system_dark_mode` polls
  `AppsUseLightTheme` (0 = dark) on the main loop, change-gated, and hands the answer
  to `colorscheme` — the same writer `platform/mac/appearance.rs` uses. The one surface
  GTK cannot repaint is the DWM-owned title bar, which
  `platform::win32::sync_caption_theme` sets from the same `desktop_is_dark()` probe on
  realize and on every re-render.
- **KDE/X11:** the live desktop dark↔light toggle is the same gap with a different
  missing source, still open; its fix belongs in the same shape: subscribe to the
  portal signal, write `prefer-dark`, change nothing downstream.
