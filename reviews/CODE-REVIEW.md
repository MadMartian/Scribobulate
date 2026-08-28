# Consolidated Code Review: `feature/decor` — Scribobulate

**Branch:** `feature/decor`
**Last updated:** 2026-08-27

**Review scope:** diff `96951f0658896fd0b3786404fdd160d8fd757fbf..7f6b09d` — 10 commits,
59 files, +10703/−1806. Working tree clean and byte-identical to HEAD. The branch adds a
decoration vocabulary (glyphs, sprites, heading bands, blockquote panels, tiled rules,
annotation chips) across three renderers — the GTK preview, the HTML sink and the PDF
sink — and splits the monolithic `src/theme.rs` into `src/theme/{mod,keys,spec,value,model,sources,resolve}.rs`
plus `src/theme/tests/`.

**Pipeline:** 14 independent reviewers (spec-compliance ×3, DRY/abstraction ×3,
anti-pattern ×3, testability ×3, security ×1, link-integrity ×1) across ten file groups →
6 nitpick-filter passes scoring every Low/Tidy finding 0/1/2 (100 scored, **48 kept, 52
dropped** — the dropped ones appear nowhere in this document) → 4 line-number verification
passes (**1,505 references checked**, 97–99.7% accurate per family; the three corrections
are applied inline and flagged where they are made) → this consolidation. Per-file reviewer
coverage is in [`coverage-map.md`](./coverage-map.md).

**Finding counts:** ~235 raw findings across the 14 reports → **94 consolidated entries**,
after merging duplicate clusters (many findings were made independently by two to eight
reviewers and appear here once, at the highest severity any reviewer proposed), applying the
nitpick filter, and moving the developer's pre-declared scope limits into the deferred
section.

| Severity | Count |
|---|---|
| 🔒 Security | 4 findings (1 High, 2 Medium, 1 Low) + 2 needs-verification + 1 tidy |
| 🔴 Critical | 0 |
| 🟠 High | 23 |
| 🟡 Medium | 39 |
| 🟢 Low | 23 |
| 🧹 Tidy | 5 (optional; no verification, no reply needed) |
| ⏸️ Deferred | 5 scope limits + 6 register IDs + 2 out-of-scope |
| ⚖️ Adjudicated | 2 (one finding reinstated at Low, one reviewer finding superseded) |

**Merged duplicate clusters worth naming** (each is one entry below, not several):
`F-PDF-001` (5 reviewers), `F-SINK-001` (8), `F-SPRITE-BRANCH-001` (7), `F-TEST-001` (7
instances found by 4 reviewer families), `F-BAND-001` (4), `F-ALPHA-001` (3),
`F-STALEREF-001` (5), `F-H6-001` (3, with the site count reconciled explicitly),
`F-FLOOR-001` (4), `F-COV-001` (4), `F-UTF8-001` (2), `F-TILE-001` (5), `F-CHOOSER-001` (4).

**Source of truth:** `sdd/SCHEMA.md`. Where the code and SCHEMA.md disagree, the code is
wrong — or the schema must be amended, and that is the developer's call to make
explicitly, not by leaving the divergence in place.

---

## 🔒 Security Review

One reviewer, ranging across all ten groups. **All security findings bypass the nitpick
filter and appear here regardless of severity.** Three of the four were reproduced
empirically on this machine, and the reviewer's own summary is worth repeating: the design
is genuinely strong, and every finding lands in one place — the byte-level admission of an
untrusted sprite file. `crate::sprite::resolve` implements every clause SCHEMA.md
prescribes, but each clause is a check performed once at load time on *metadata*, while
the actual open happens much later against nothing but a stored `PathBuf`.

### `F-SEC-001` — Sprite decode has a byte cap but no pixel cap (decompression bomb) ✅ Done

- **Severity:** High
- **Reference:** `src/sprite.rs:50` (the cap), `src/sprite.rs:286-296` (enforcement),
  `src/sprite.rs:320-346` (`texture`), `src/sprite.rs:350-383` (`scaled`); amplified at
  `src/export/pdf/measure.rs:508-510` and `:134-141`; also reached from `src/export/html.rs:988`
- **Found by:** security
- **Spec:** `sdd/SCHEMA.md` § "How a `*_sprite` key resolves"; `sdd/POLICY.md` § "Input limits"

`MAX_SPRITE_BYTES` (512 KiB) bounds the *compressed* file; nothing bounds the *decoded*
raster. Measured on this machine: a 20000×20000 single-colour PNG compresses to 388 332
bytes — inside the cap — and both `Gdk.Texture.new_from_filename` and
`GdkPixbuf.Pixbuf.new_from_file` allocate ~1.2 GB from it. Neither loader refuses on
dimensions and neither has a configurable limit. The PDF sink amplifies it: `measure.rs:508-510`
decodes **per list item** and retains each `cairo::ImageSurface` on the line
(`measure.rs:556`), and `heading_band_ink` (`:134-141`, called per heading at `:227`) has
the same shape — while `ink.rs` deliberately hoists the quote-bar and rule decodes out of
their loops for exactly this reason. On the paint path this is inside GTK's `snapshot`, so
the process dies with unsaved buffers.

**Fix:** add `const MAX_SPRITE_PIXELS: i64 = 4096 * 4096;` to `crate::sprite`, enforce it
in `texture()` immediately after decode (return `None` and log), and route `scaled()`
through `texture()`'s gate rather than decoding a second ungated `Pixbuf`. A sprite
refused this way degrades to "decoration absent", which is already this vocabulary's
answer to every other refusal. Separately, hoist the list-marker and heading-band decodes
out of their per-item loops the way `ink.rs` already does.

### `F-SEC-002` — `resolve` admits a FIFO; the read then blocks the main thread forever ✅ Done

- **Severity:** Medium
- **Reference:** `src/sprite.rs:286-296` (the `metadata` check), consumed at `src/sprite.rs:200`,
  `:326`, `:366`
- **Found by:** security
- **Spec:** `sdd/POLICY.md` § "Input limits" names this exact bug verbatim — *"a size test
  alone admits a FIFO (whose reported length is zero) and then blocks the main thread
  forever … A caller that reimplements one half has reimplemented the bug."*

`resolve`'s final gate is a size test alone; it never asks `m.is_file()`. A FIFO reports
`len() == 0`, passes, and the later `std::fs::read` / `Texture::from_filename` blocks on
`open(2)` until a writer that never comes. Reproduced: `mkfifo`, then all three admission
conditions pass and `timeout 3 head -c 1` exits 124. `src/limits.rs:170-178` already ships
`is_regular_file_within_limit`, which checks the type *then* the length; `sprite::resolve`
reimplemented the length half and dropped the type half.

**Fix:** add the type arm to the same match, ahead of the size arm — three lines, and
POLICY already ships the helper. This is the recommended first fix of the whole review.

### `F-SEC-003` — Sprite validation is check-then-use: neither containment nor the byte cap is re-established at open time ✅ Done

- **Severity:** Medium
- **Reference:** validation at `src/sprite.rs:275-296`; unguarded opens at `src/sprite.rs:200`,
  `:326`, `:366`; the mirrored-number-not-enforcement comment at `src/export/html.rs:965-968`
  vs the check at `:978`
- **Found by:** security
- **Spec:** `sdd/SCHEMA.md` § "How a `*_sprite` key resolves" reads as though containment
  holds at the point of the read; it does not.

`resolve` runs once at `theme::load()` (`src/theme/mod.rs:245`); `bytes()`/`texture()`/`scaled()`
run at first paint, at every theme swap and at every export. Between them the guarantee is
represented by a `PathBuf` and nothing else. Two consequences: **(a)** `bytes()` is a bare
`std::fs::read(p)` with no bound, so a file that grew after `resolve` measured it is read
whole — `export/html.rs:978` re-checks 512 KiB but only *after* the read has allocated, so
it bounds the base64 embed and not the read; **(b)** containment is proved once, so a
directory component later replaced by a symlink is followed at read time, and those bytes
are base64-embedded into a self-contained HTML file the user is about to email.

**Fix:** one private `open_checked(path) -> Option<Vec<u8>>` in `crate::sprite` that opens
first, calls `File::metadata()` on the **open handle** (closing the TOCTOU), re-checks
`is_file()` and the length on that handle, and reads through `.take(MAX_SPRITE_BYTES + 1)`
so the cap bounds the read rather than predicting it. Route `bytes()` through it and have
`texture()`/`scaled()` decode from those bytes (`Texture::from_bytes`, `Pixbuf::from_stream`
— both already implemented for the `Compiled` arm), which removes the last two unguarded
opens and collapses both source arms onto one read path. Re-asserting containment at open
time is genuinely hard (`openat2(RESOLVE_BENEATH)` is Linux-only); accepting (b) as
residual is reasonable **if it is stated in the module docs and in SCHEMA.md**.

### `F-SEC-004` — The extension allowlist is checked on the authored name, not the file actually opened ✅ Done

- **Severity:** Low
- **Reference:** `src/sprite.rs:265-273` (check on `candidate`) vs `:275-285` (canonicalisation, afterwards)
- **Found by:** security
- **Spec:** SCHEMA.md states the allowlist exists to "keep a theme file from steering the
  image loader at arbitrary bytes on disk by extension alone".

A symlink `chip.png → notes.txt` inside the theme directory passes the allowlist while the
loader is pointed at `notes.txt`. Two things keep this Low: containment still holds (the
target must be inside the theme's own directory), and because `resolve` returns the
*canonical* path, `SpriteRef::extension()` reports the real target's extension, so
`export/html.rs`'s MIME match (`:981-986`) refuses it with `_ => return None`.

**Fix:** move the extension check below the canonicalisation and test `real.extension()`
instead of `candidate.extension()` — a two-line reorder that makes the allowlist mean what
SCHEMA.md says it means.

### Needs verification (not findings — open questions the reviewer could not close here)

| ID | Question | Why it is open |
|---|---|---|
| `VERIFY-001` | Windows path forms in `resolve` (`src/sprite.rs:254-273`) | Reasoned through, not executed. `\\?\`/UNC, `C:evil.png`, `\evil.png`, 8.3 short names, ADS and trailing dot/space were all traced to a refusal — but by the `Component::Normal` filter at `:258-264`, **not** by `Path::is_absolute` at `:254`, which is false for several of them on Windows. **Action:** run the existing resolve tests on Windows CI with added cases for `\evil.png`, `C:evil.png`, `..\escaped.png`, and comment that the component filter (not `is_absolute`) is what refuses them. |
| `VERIFY-002` | `NATURAL`/`RESAMPLED` sprite caches are unbounded in entry count (`src/sprite.rs:299-310`, cleared only at `src/theme/mod.rs:326`) | `RESAMPLED` is keyed on `(SpriteRef, w, h)` with `w`/`h` from zoom-scaled geometry. The reviewer did not report it because zoom appears to be a small discrete set and the metrics are clamped (≤400×zoom). **Action:** confirm zoom really is discrete. If continuous zoom can produce arbitrary `(w, h)`, this is unbounded per-session growth and needs an LRU bound. Overlaps ISSUES **M** (deferred), but M is about eviction policy in general; this is about whether the entry count is bounded at all. |

### Security tidy (optional — no verification, no reply needed)

`T-SEC-001` — `src/export/html.rs:761` and `:807` both call
`css_string_escape(g.escaped_for_html().as_str())`, HTML-escaping a marker glyph on its
way into a CSS `content:` string. `<style>` is an HTML raw-text element, so entities are
not decoded: `list_bullet_glyph = "&"` renders as the literal five characters `&amp;` on
every bullet. This contradicts the doc comment eight lines above (`:711-714`). Both
orderings are safe — `escaped_for_html` removes `<` so the glyph cannot spell `</style>`,
and `css_string_escape` handles `"` and `\`. The correct call is
`css_string_escape(g.as_plain())`; better still, add a fourth projection
`escaped_for_css_string` so `MarkerGlyph`'s "three grammars" comment stops being an
undercount.

### Cleared surfaces

Recorded because a cleared surface is worth more to the next reader than silence. The
security reviewer traced and cleared: path traversal (component-wise `Path::starts_with`
on two canonicalised operands — the `themes-evil`/`themes` string-prefix trap does not
apply); the embedded-table boundary in both directions; CSS injection in `preview/css.rs`
(all thirteen `Metrics` fields checked against their clamp range individually); HTML
injection and the `javascript:` scheme allowlist; `data:` URI construction; Pango markup
injection; `MarkerGlyph`'s validate-at-the-boundary shape; total clamping including NaN
and `usize::MAX`; the tiled rule widget (`gtk_snapshot_push_repeat`, one GSK node, no
unbounded loop); `tags.rs` setting properties rather than markup; the three unsanitised
`String`s (`name`, `symbol`, `syntect_theme`) reaching no dangerous sink; ordered-list
numeral overflow (pulldown-cmark caps the start at nine digits); `cleaned_offset_to_buf`'s
raw slicing (offsets are char boundaries by construction — but worth a comment, since the
sibling slice twenty lines up was hardened to `.get()` in a previous round and this one
was left raw); and all four shell/PowerShell scripts. Also worth recording positively:
neither `src/sprite.rs` nor `src/widgets/rule.rs` is in `scripts/coverage.sh`'s `IGNORE`
regex, so the new security boundary and the new paint widget are both inside the gate.

**Recommended security order of work:** `F-SEC-002` (three lines, closes a hard hang) →
`F-SEC-001` (highest impact) → `F-SEC-003` (the `open_checked` refactor is a
simplification as well as a fix) → `F-SEC-004` and `T-SEC-001` together.

---

## 🔴 Critical — Must Fix

**None.** No reviewer proposed a Critical finding. The security reviewer found no injection
of any kind (CSS, HTML or Pango markup) and no path traversal; the anti-pattern reviewers
found no `unwrap`/`expect`/`panic!` on any production paint or export path (see
*Adjudicated disagreements* for the one production `expect`, which is provably
unreachable). The highest-severity items are in the section below.

---

## 🟠 High

### `F-SINK-001` — Nothing enforces that a declared theme key reaches any surface at all ✅ Done

- **Severity:** High
- **Reference:** `src/theme/keys.rs:139-229` (the registry, 69 keys), `src/theme/resolve.rs:93-259`
  (the sole construction site), and the three consumption surfaces `src/tags.rs` /
  `src/preview/css.rs`, `src/export/html.rs:379-518`, `src/export/pdf/{measure,ink,decide}.rs`
  + `src/export/markup.rs`
- **Found by:** spec-C (F-9, High), DRY-C (C-1, High), DRY-A (H-3, High), DRY-B (F-10, Medium),
  anti-pattern-C (#1 structural half, High), anti-pattern-A (M12, Medium), testability-C (T-1, High),
  testability-A (M5, Medium) — **eight reviewers independently**
- **Spec:** `sdd/SCHEMA.md:133` — *"Both targets go through one pipeline and differ only in
  their final sink."* `sdd/POLICY.md` § "One theme key, every application path".
  `sdd/PLAN.preview-decoration.md` § "Export is two more application paths" predicts this
  failure in its own words: *"nothing asserts a key is **used** — so this is a completeness
  obligation on the author."*

The registry closed the drift hole for *parsing* (an unknown key now warns, TDD 18.33) and
left it wide open for *consumption*. Adding one key today takes up to five coordinated edits
(`data/themes.toml`, `theme/keys.rs`, `theme/resolve.rs` + `theme/model.rs`, the preview
path, `export/html.rs`, `export/pdf/*`) and **nothing fails when one is missed**. Only the
sprite family has a completeness guard (`src/theme/tests/sprites.rs:129-134`), covering 10 of
69 keys. `F-PDF-001` is this finding's output: the plan's own prediction coming true on the
plan's own branch. A key declared in `KEYS` but never read by `Theme::resolve` is worse than
an unknown key — `ThemeSpec::validate` admits it *without* a warning because `keys::lookup`
claims it, so it is accepted, SCHEMA-documented and completely inert with no log line at all.
(DRY-A verified the current tree is clean on that direction: all 69 declared constants *are*
read in `resolve.rs`. This is a guard for the next key.)

**Fix:** extend `theme::keys::Key` with a mandatory `surfaces:` field (`PREVIEW | HTML | PDF`,
plus a `NotApplicable(&'static str)` variant carrying a required reason). Then write one
registry-driven sweep — not one test per key — that for every `Key` claiming a surface
resolves a theme with that key set to a sentinel and asserts the sentinel is observable in
that surface's output (the HTML stylesheet string, the PDF's laid-out `Line`/markup, the
preview's tag/CSS). `src/theme/tests/sprites.rs:100` already does exactly this shape for the
model; it needs pointing at the sinks. Where a `NotApplicable` reason is given, require it to
match a line in SCHEMA.md — which would also have caught `F-RADIUS-001`.

### `F-PDF-001` — Eleven theme keys reach the preview and the HTML sink and are silently absent from the PDF sink ✅ Done

- **Severity:** High
- **Reference:** the PDF sink's whole theme surface — `src/export/pdf/measure.rs:104-125`
  (`layout_of`, the only font descriptor this sink builds; `:117-118` reads `font_family` and
  nothing else), `:224-268` (the `Block::Heading` arm — no `foreground` span, no space
  metric), `:295-312` (`Block::CodeBlock` — no card, no fill), `:483`, `:539`, `:565`;
  `src/export/pdf/ink.rs:46`, `:124-125`, `:285-308`; `src/export/pdf/measure/table.rs:101-108`;
  `src/export/markup.rs:76-88`. Contrast the HTML sink honouring the heading four in one loop
  at `src/export/html.rs:499-516`.
- **Found by:** anti-pattern-C (#1, High — **the eleven-key count and table are its measurement**,
  produced by enumerating every non-test `theme.*` read in `src/export/pdf/**` and
  `src/export/markup.rs` and diffing against `src/export/html.rs` and `src/preview/`),
  spec-C (F-1 High, F-3/F-5/F-7 Medium), DRY-C (C-2/C-3/C-4, all High), testability-C (T-2, High)
- **Spec:** `sdd/SCHEMA.md:133` (`win.export` — "differ only in their final sink"); TDD 18.21,
  18.22, 18.32 (*"on all three surfaces"*), 25.3, 25.9.

| Key | Preview | HTML | PDF |
|---|---|---|---|
| `heading_color` / `heading_colors[]` | `tags.rs:275` | `html.rs:398`, `:510` | **absent** |
| `heading_font` / `heading_fonts[]` | `tags.rs:276` | `html.rs:392`, `:509` | **absent** |
| `heading_space_above` | yes | `html.rs:513` | **absent** |
| `heading_space_below` | yes | `html.rs:503` | **absent** |
| `mark_fg` | `renderer/mod.rs:93` | `html.rs:406` | **absent** |
| `code_inline_bg` | `tags.rs:395` | `html.rs:456` | **absent** |
| `code_block_bg` | `codeview` card | `html.rs:457` | **absent** |
| `list_step` | `tags.rs:528` | `html.rs:432` | **absent** — literal `INDENT_PT` (`geometry.rs:32`) |
| `list_item_gap` | `tags.rs:526` | `html.rs:433` | **absent** — literal `BLOCK_GAP_PT` (`pdf/mod.rs:66`) |
| `blockquote_text_gap` | `tags.rs:474` | `html.rs:463` | **absent** — the gap *is* the bar's own width |
| `table_cell_radius` | `preview/css.rs:258` | `html.rs:478` | **absent** |

Four of five heading decorations reach the page — scale (`measure.rs:226`), weight (`:254`),
band (`:227`), rule (`:232`) — and the *ink* does not. A Synthwave export prints banded,
ruled, correctly-scaled headings in body black. `mark_fg`'s mechanism is `F-SPAN-001`: a
second Pango-span builder that was never updated. `list_step`/`list_item_gap`/`blockquote_text_gap`
are TDD 25.9 violations ("a literal styling value anywhere in either sink is a defect").

**⚠️ Scope note on the deferral.** ISSUES **J** ("PDF export ignores heading colour / font /
spacing", Low, pre-existing) covers **only the bare `heading_color` / `heading_font`** — those
two existed at the diff base (verified by the orchestrator via `git show 96951f0:src/theme.rs`).
It does **not** cover `heading_space_above`, and it does **not** cover **any** per-level
`_h1…_h5` heading key. Those are new on this branch and are **born dead in the PDF sink** —
they were introduced for TDD 18.21/18.22, reached two surfaces of three, and TDD 18.32 says
"on all three surfaces" in its own predicate. That part of this finding is a branch defect,
not deferred debt.

**Compounding.** SCHEMA.md's `blockquote_fg` row says a heading inside a quote keeps its own
colour. In HTML that holds structurally (`h1 { color }` beats inherited `blockquote { color }`).
In the PDF it holds for a **link** (its `foreground` span in `markup.rs:147` beats the cairo
pen) and **fails for a heading**, because a heading carries no foreground span at all and
therefore takes the quote's pen. Fixing this finding fixes that symptom.

**Fix:** extend `markup::heading_rule_span` (`src/export/markup.rs:183`) into a
`heading_span(theme, level_index)` that also emits `foreground=` from `heading_colors[i]` and
`font_family=` from `heading_fonts[i]`; give `LayoutSpec` (`pdf/mod.rs:202-208`) a `family`
field so the heading arm can pass `heading_fonts[i].or(font_family)`; carry
`heading_space_above/_below` into `Fragment.space_before` / a new `space_after` instead of the
flat `BLOCK_GAP_PT`; add ` foreground=` to `markup.rs:82`'s `Highlight` span for `mark_fg`;
read `theme.metrics.list_step` / `list_item_gap` / `blockquote_text_gap` in place of the three
constants. Then close the class with `F-SINK-001`.

### `F-BAND-001` — `heading_band_sprite` is silently inert unless `heading_band_color` is also stated ✅ Done

- **Severity:** High
- **Reference:** `src/theme/model.rs:211-221` (`HeadingBand::is_absent`, the rustdoc rule at
  `:215-217`), and the three renderers re-applying the same gate:
  `src/codeview/mod.rs:689`, `:702-704`; `src/export/html.rs:532-534`;
  `src/export/pdf/measure.rs:135`. Heading inset is gated the same way at `src/tags.rs:277`, `:283`.
- **Found by:** spec-A (F1, High), spec-B (#2, Medium), anti-pattern-B (#9, Low — accepts the
  behaviour, flags the missing diagnostic), testability-B (T-3, High — proposes
  `a_band_with_no_fill_stated_paints_nothing_even_with_a_sprite` to force the question)
- **Spec:** `sdd/SCHEMA.md` § Key naming — *"**A sprite outranks its flat sibling.**"* And the
  Headings table's `heading_band_sprite` row: *"A sprite tiled at its natural size across the
  band, in place of its fill. **Outranks the fill and the gradient.**"* — with **no** fill
  precondition. The adjacent `heading_band_gradient_to_color` row *does* carry one ("Ignored
  where the level states no fill"), so the asymmetry is deliberate in the document and an
  author reading it concludes the sprite stands alone.

A theme stating `heading_band_sprite_h1` and no `heading_band_color_h1` gets **no band, no
sprite, no heading inset and no log line**. All three renderers agree with each other and all
three disagree with SCHEMA.md, so this is a coherent design decision — argued at
`model.rs:215-217` ("a sprite … cannot conjure one") — that the spec does not describe. Per
the review brief, where code and SCHEMA.md disagree the code is wrong *or the schema must be
amended, and the developer chooses*. This is precisely ScrAP-324's failure class with a third
case added: "the reference resolved fine and was then discarded for want of an unrelated key"
looks identical to "the theme stated no sprite".

**Coverage:** `src/theme/tests/headings.rs:243-262` exercises radius + gradient only. The
sprite-without-fill case — the one a schema reader will write — is untested on every surface.

**Fix:** pick a direction explicitly. Either (a) drop the fill precondition in all three
renderers plus `tags.rs:277`, or (b) amend SCHEMA.md's `heading_band_sprite` row to carry the
gradient row's caveat, note it as the stated exception to "a sprite outranks its flat
sibling", **and** log a `warn` once at theme-resolve time when a resolved sprite is discarded
for want of a fill (the same for `gradient_to`, whose row already states the precondition and
is also silent). Either way add the missing test to `src/theme/tests/headings.rs`.

### `F-SPRITE-BRANCH-001` — The sprite-outranks-flat rule is open-coded once per renderer per decoration; a seam already exists in the repo ✅ Done

- **Severity:** High
- **Reference:** the existing seam — `MarkerSubstitute` at `src/codeview/gutter.rs:140` (a
  clean `Sprite | Glyph | Drawn` enum resolved by the pure `marker_substitute`,
  `gutter.rs:158-186`). The open-coded sites: `src/codeview/mod.rs:730-757` (band), `:842-853`
  (bar), `:1044-1067` (chip); `src/renderer/events.rs:170-174` (rule);
  `src/export/html.rs:540-556`, `:582-604`, `:620-630`, `:646-651`, `:738-764`, `:785-810`,
  `:949-960`; `src/export/pdf/ink.rs:139`, `:170`, `:236`; `src/export/pdf/measure.rs:137-146`,
  `:508-511`
- **Found by:** DRY-B (F-2, High — counted 17 branches), testability-B (T-2, High), DRY-C
  (C-8, Medium), DRY-A (M-1, Medium — counted 13 sites across 6 files), spec-A (F6, Medium),
  anti-pattern-B (#5), spec-B (#4)
- **Spec:** `sdd/SCHEMA.md` § Key naming mandates *the branch* per renderer and says why:
  *"filling first and tiling over looks identical for an opaque sprite and lets the flat
  colour bleed through a transparent one, which is a defect that appears only for the sprites
  nobody tested."* `sdd/THEMING.md` mechanism B names the intended shape: *"the precedence is
  decided in **ONE** pure function (`codeview::gutter::marker_substitute`) that the drawn
  gutter and both export sinks read."* `sdd/PLAN.preview-decoration.md`: *"That precedence is
  now a property of the closed decoration vocabulary itself, not a one-off decision each new
  decoration re-derives."*

**This returns as a defect notwithstanding deferral D4**, because the developer explicitly
invited it — *"Generalising it IS a fair finding if you can show a seam — say so"* — and the
seam exists already. **Orchestrator-verified measurement, to be used in place of any
reviewer's count:** **8** sprite decoration kinds are declared on `Sprites` in
`src/theme/model.rs`; **36 non-test call sites** consult `sprites.` across 8 files
(`export/html.rs` 16, `export/pdf/decide.rs` 9, `codeview/gutter.rs` 5, `codeview/mod.rs` 2,
and one each in `widgets/rule.rs`, `renderer/events.rs`, `preview/build.rs`,
`export/pdf/measure.rs`); `MarkerSubstitute` appears in `gutter.rs` **only**.

The mandate is honoured everywhere — every site really does branch, none paints over — but it
is honoured by convention, not by construction. Worse, *the precedence semantics differ
subtly per decoration and nothing records that*: the band is three-way (sprite → gradient →
fill), the bar two-way (sprite → colour), the chip two-way but the sprite replaces only the
*fill* while the ink still paints on top (`codeview/mod.rs:1025-1028`). Those distinctions are
discoverable only by reading every site. `F-BAND-001` is the first drift already realised —
three sites agree with each other and disagree with the schema, and nothing would have caught
it if one of the three had diverged instead.

**Fix:** promote `MarkerSubstitute`'s shape to the vocabulary. A resolved-decoration enum on
`theme::Sprites` (or beside `marker_substitute` in a display-free module), produced once by
`Theme::resolve` alongside the folds it already does, with the precedence stated in exactly
one `impl` per shape (two-way, three-way, chip). Each renderer then reads the answer instead of
re-deriving it, the per-decoration semantic differences become declared rather than implicit,
and the branch is testable without a display — which is `F-SPRITEPAINT-001`'s fix too.

### `F-TEST-001` — Seven tests pass without asserting the rule they are named for ✅ Done

- **Severity:** High (one finding, seven instances)
- **Found by:** anti-pattern-C (#5, High), testability-B (T-1, High), spec-A (F12),
  anti-pattern-A (M14), testability-A (L4, H4), testability-C (T-14), anti-pattern-C (#17, #18)
  — four reviewer families independently
- **Spec:** ScrAP-217 / ScrAP-220 / ScrAP-221 — a guard whose coverage does not include its own
  subject, and a comment stating a false premise about what a failure looks like.

| # | Test | Reference | Why it passes anyway |
|---|---|---|---|
| 1 | `resolve_refuses_a_symlink_that_escapes_the_theme_directory` | `src/sprite.rs:465` (test at `:463-477`, guarding `:275-285`) | `#[cfg(unix)]` is **inside the body**, not on the test. On Windows it compiles to an empty function, runs, and reports green while asserting nothing. Containment is the *only* admission rule not decidable from the string alone, and Windows is where the user-writable search-path row (`%APPDATA%`) lives. |
| 2 | `hostile_markup_in_the_document_does_not_reach_pango_as_markup` | `src/export/pdf/measure/tests.rs:674` (body `:673-688`) | Asserts `!fragments.is_empty()` and `all(height > 0.0)`. Delete `escape_pango` at `markup.rs:31` and `set_markup` blanks the layout — but an empty layout still reports `line_count() == 1` at the font's own row height, so **both assertions still hold**. The comment at `:685-686` states the opposite premise, and that is where the false confidence lives. |
| 3 | `a_theme_that_states_its_own_light_page_keeps_it` | `src/export/pdf/mod.rs:458-471` (guard at `:463`) | Every assertion sits inside `if let Some(stated) = sepia.background`. If Sepia ever stops stating a background the test passes vacuously and reports nothing. |
| 4 | `an_undecodable_image_falls_back_to_a_visible_note_rather_than_a_gap` | `src/export/pdf/measure/tests.rs:180-201`, assertion at `:200` | Asserts only `!text.trim().is_empty()`, which any text satisfies. A note replaced by unrelated content passes. |
| 5 | `an_absurd_ordered_start_does_not_panic` | `src/export/pdf/decide.rs:462-466`, guarding `:135` | Passes `n = 0`, so `s + 0` never exercises the addition the name is about. `s = u64::MAX` with a second item overflows — a debug-build panic inside an export, which `start.rs:333-337` calls the worst outcome on a render walk. |
| 6 | `the_depth_keys_do_not_reach_the_ordered_or_task_markers` | `src/theme/tests/lists.rs:150-169`, assertion at `:165-168` | Its only assertion is on the **bare** `list_marker_color`, which the depth keys could not affect under any implementation. `list_task_color` and anything ordered are never read. It would pass with the rule broken — and SCHEMA.md's Lists ⚠️ callout is the one list contract a reader is most likely to get backwards. |
| 7 | `builtin_system_spec_matches_the_floor` | `src/theme/tests/system.rs:26-55` | A hand-written list of 19 assertions guarding 22 declared floors — see `F-FLOOR-001`. Its docstring (and `resolve.rs:7-9`, `:41-43`) promises totality. |

**Shared remedy — apply to all seven:** assert on the observable the name claims, then
**mutation-check** by deleting the rule and confirming the test goes red. Concretely: #1 move
`#[cfg(unix)]` onto the test (a gated-out test at least reports as *absent*) and add a
`#[cfg(windows)]` junction case plus three platform-independent string-shape negatives
(`"C:x.png"`, `"sub/../../x.png"`, `"x.png:stream"`); #2 assert on `layout_text_for_test`
(`pdf/mod.rs:230`) — the metacharacters survived as *text* and the injected span is inert
content; #3 replace the `if let` with `.expect("sepia states its own page")`; #4 assert the
note's actual shape; #5 use two items and make the arithmetic `saturating_add`; #6 assert
`list_task_color` folds to the bare value and only `list_bullet_colors[]` moves; #7 see
`F-FLOOR-001`. The repository already has the right pattern to copy —
`value.rs:402` (`a_hostile_glyph_is_inert_in_every_grammar_it_reaches`) actually *parses* the
generated markup rather than string-matching it, `markup.rs:328`/`:346` and `decide.rs:294`
call `pango::parse_markup`, and `theme/tests/sprites.rs:80-83` carries an explicit
anti-vacuity assertion. The one test that names *injection* is the one that does not.

*(Instances 4 and 5 scored 1 in the nitpick filter as standalone Lows and do not appear
elsewhere in this document; they are retained here because the systemic finding is High.)*

### `F-SPRITEPAINT-001` — Half the sprite paint branches have no test that would fail if they broke ✅ Done

- **Severity:** High
- **Reference:** untested preview arms — `src/codeview/mod.rs:708-756` (heading band),
  `:1044-1067` (annotation chip). Untested PDF draw paths —
  `src/export/pdf/ink.rs:161-207` (band ink), `:211-227` + `src/export/pdf/measure.rs:504-561`
  (list-marker sprite, incl. the `if sprite.is_some() { String::new() }` suppression at
  `:511-513`), `ink.rs:56-62`/`:139-148` (bar sprite on the page)
- **Found by:** testability-B (T-3, High), testability-C (T-3, High), spec-B (#5 Medium, #7 Low),
  spec-C (F-11, Medium)
- **Spec:** TDD 18.19, 18.24, 18.25, 18.28; SCHEMA.md § Key naming's stated motivation for the
  explicit-branch rule — *"a defect that appears only for the sprites nobody tested."*

| Branch | Test that fails if the sprite stops outranking the flat value |
|---|---|
| Blockquote bar (preview) | ✅ `a_blockquote_bar_sprite_tiles_and_replaces_the_flat_colour`, `codeview/mod.rs:1871` |
| Horizontal rule (preview) | ✅ `a_rule_sprite_swaps_the_separator_for_a_tiling_widget`, `preview/build.rs:1843` |
| List marker (precedence only) | ✅ `a_sprite_outranks_a_glyph_for_the_same_marker`, `gutter.rs:720` — never decodes anything |
| **Heading band (preview)** | ❌ none — `a_heading_band_is_painted_…` (`:1805`) drives the **flat** arm only |
| **Annotation chip (preview)** | ❌ none — proven only to reach an exported HTML file (`html.rs:1046`) |
| **Heading band ink (PDF)** | ❌ `tests.rs:1058` asserts only `line.band.is_some()`; deleting the whole `if let Some(band)` block in `ink.rs` leaves the suite green |
| **List-marker sprite (PDF)** | ❌ nothing asserts a `Line::marker` is produced or drawn; nothing asserts the text marker is suppressed — deleting `measure.rs:511-513` puts a bullet **and** a picture on the page |
| **Blockquote bar sprite (PDF)** | ❌ `pdf/mod.rs:447` asserts only that the **bytes decode** |

The templates already exist and are unusually good: the bar's preview test uses a
**half-transparent** 8×8 tile specifically so a fill-then-tile-over cannot pass, and its
fixture comment records that the first, opaque version *passed the mutation it was written to
catch*. The rule-sprite PDF test (`tests.rs:513`) asserts absence for an unstated key,
presence and tiling for a stated one, and the measured height fold. Every row above wants one
of those two shapes; `pdf/measure/tests.rs` already has the `drawn_page` + `colour_extent` /
`colour_rows` / `extent_where` pixel harness.

### `F-LOG-001` — `sprite::scaled` swallows every failure silently while its twin logs ✅ Done

- **Severity:** High
- **Reference:** `src/sprite.rs:350-383`; the three silent exits are `:366`, `:371`, `:378`.
  Contrast `texture()` at `:320-346`, which logs. Consumed by
  `src/codeview/gutter.rs:261` and `src/codeview/mod.rs:1051`.
- **Found by:** anti-pattern-B (#1, High), testability-B (T-12)
- **Spec:** `sdd/ANTI-PATTERNS.md` **ScrAP-324** — *"an inert-by-default fallback makes a whole
  defect class unobservable … 'the theme stated no sprite' and 'the reference resolved
  nowhere' produce identical pixels and identical logs."* SCHEMA.md § How a `*_sprite` key
  resolves states a refused sprite is "refused **and logged**".

`scaled` is *the* path for every sprite this branch added that is drawn rather than tiled —
the list-marker gutter and the annotation chip. A user theme whose PNG clears `resolve`'s
metadata/extension gate but is unreadable by gdk-pixbuf (truncated, a `.png` that is actually
WebP, a permissions change between resolve and paint) produces no sprite, no flat fill, no
glyph, no warning and no crash. The `NATURAL`/`RESAMPLED` caches memoise the `None`, so it
does not even retry: one silent failure at first paint is permanent for the process. The
asymmetry also defeats the obvious diagnostic — an operator running with `RUST_LOG=warn` sees
`texture()`'s line for the *bar* sprite, nothing for the *bullet*, and reasonably concludes
the bullet resolved fine.

**Fix:** route both `texture` and `scaled` through one `decode(r) -> Option<Pixbuf>` helper
that owns the logging, so a future third decode path cannot forget it — the same "one seam,
every caller" discipline `SpriteOrigin::resolve` already applies to the resolution half. Guard
it with a test that drives an undecodable-but-admissible file through `scaled` and asserts a
warn record was emitted; a pixel assertion cannot tell "logged and absent" from "silent and
absent". *(This composes with `F-MARKER-001` into one wholly invisible failure — fix them
together.)*

### `F-MARKER-001` — The list-marker sprite arm is the one renderer where a decode failure erases the decoration ✅ Done

- **Severity:** High
- **Reference:** `src/codeview/gutter.rs:249-265` — the `if let` at `:261-263` and the
  **unconditional** `return;` at `:264`
- **Found by:** anti-pattern-B (#2, High), spec-B (#3, Medium), testability-B (T-10, Medium)
- **Spec:** `sdd/THEMING.md` § Untrusted input — *"A reference that fails any check is dropped
  to 'this decoration is absent' — the same inert-by-default behaviour an unset key gets,
  never a partial render."* SCHEMA.md § Lists states the identical rule for the glyph key:
  refused, *"falling back to the drawn marker"*.

Every sibling renderer degrades correctly — the band falls to gradient then fill, the bar to
`blockquote_bar_color`, the rule to the stock `GtkSeparator`, the chip to the flat
`annotation_chip_bg`. The list marker falls to **nothing**: not the theme's glyph, not the
drawn bullet/numeral/checkbox. Because `marker_substitute` returns `Sprite` before any decode
is attempted, a theme stating both a sprite and a glyph loses the glyph too. Precedence
between two *stated* values is a different question from what to do when the winner cannot be
produced. The consequence is sharper than a missing decoration: a task checkbox that fails to
resample is *gone*, while its hit-box (`codeview/mod.rs:953-964`) is still recorded from
`checkbox_rect` — an invisible, clickable checkbox, with (per `F-LOG-001`) no log line
anywhere in the process.

**Fix:** have `marker_substitute` return the *ordered* candidates (`Sprite → Glyph → Drawn`)
rather than the single winner, and let the paint site walk them until one produces ink. That
keeps the precedence decision in the one pure, tested function it already lives in and moves
"what if the winner cannot be produced" to where it belongs. Add a display-free test that a
`SpriteRef::File` pointing at a non-image, plus a stated glyph, yields the glyph. Ask the same
question of `src/export/html.rs` and `src/export/pdf/ink.rs`, which consume the same
precedence and are the natural place for the identical variation to exist a fifth time.

### `F-ALPHA-001` — Colour alpha is discarded by both export sinks while the preview honours it — inconsistently within one `match` ✅ Done

- **Severity:** High
- **Reference:** `src/palette.rs:46-54` (`to_hex`, drops alpha unconditionally);
  `src/export/pdf/ink.rs:29-35` (`set_ink` → `cr.set_source_rgb`, three channels). The sharpest
  demonstration is one `match` at `src/export/pdf/ink.rs:170-204`: the gradient arm `:185-198`
  passes `f64::from(c.alpha())` to `add_color_stop_rgba` — **alpha kept** — and the flat arm
  `:200-203` calls `set_ink` — **alpha dropped**, four lines apart. 33 `to_hex` call sites in
  `src/export/html.rs`, 7 in `src/export/markup.rs`, 1 at `src/export/pdf/decide.rs:152`.
  Preview contrast: `src/codeview/mod.rs:656-662` (`append_color` composites full RGBA),
  `src/tags.rs:331`, `:369`, `:371`.
- **Found by:** anti-pattern-C (#3, High), DRY-C (C-5, High), spec-C (F-10, Medium)
- **Spec:** SCHEMA.md § Key naming — every colour key parses `#RRGGBBAA`. Two shipped defaults
  are translucent (`mark_bg = #fff59d_88`, `annotation_hl_color = #FFD133_61`), so this is not
  hypothetical, and a translucent wash is the natural authoring choice for the key this branch
  added: `blockquote_bg`, *"a panel behind quoted text"*.

A theme stating `blockquote_bg = "#0a183080"` renders as a translucent wash in the preview and
as a **solid navy block** in both the exported HTML and the exported PDF. Nothing warns. The
reader sees three different documents. The project already solved this twice — `ThemeColor`
carries the alpha-preserving `hex()` + `alpha_pct()` pair used correctly for `mark_bg` in both
`renderer/mod.rs:98` and `markup.rs:82`, and `preview/css.rs:136-146`'s `rgba_css` states the
policy explicitly. The export sinks do neither: they do not emit alpha and they do not
pre-composite.

**Fix:** rename `palette::to_hex` → `to_hex_opaque` and add `to_hex_rgba` beside it so all 41
call sites must state which they mean (CSS accepts 8-digit hex in every browser this artefact
targets; the `Palette`-derived reads are already composited by `Palette::from_base` and keep
the opaque spelling). Give `set_ink` an alpha-aware twin — `cr.set_source_rgba(…)` is a
one-word change at `ink.rs:30` and removes the gradient/flat inconsistency without a second
decision. On the markup path emit Pango's `alpha=` / `bgalpha=` where the value is not opaque,
the same treatment `mark_bg` already gets two arms up. Guard it by extending
`pdf/measure/tests.rs`'s pixel oracle with a companion to `extent_where` (`:326`) that scans
for a *composited* colour, which would fail today and pass after the fix.

### `F-EVENT-001` — Recurrence of ScrAP-78: catch-all `_ => {}` over the pulldown-cmark event vocabulary in three dispatchers ✅ Done

- **Severity:** High
- **Reference:** `src/renderer/events.rs:205` (over `Event`), `src/renderer/start.rs:272`
  (over `Tag`), `src/renderer/end.rs:321` (over `TagEnd`); the live instance is the guarded arm
  at `events.rs:153` and its twin at `end.rs:315`
- **Found by:** anti-pattern-C (#4, High)
- **Spec:** `sdd/ANTI-PATTERNS.md:924` — **ScrAP-78**, *"`Options::all()` (or any
  enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of
  degrading to literal text."* This project has already been burned by exactly this, in
  exactly this crate. A catalogued recurrence is High by the round's rubric.

There is a live instance today, not only a future one: `events.rs:153` reads
`Event::Html(t) if self.in_html_block => …`, and a guarded arm that fails its guard **falls
through to `_`**. A block-HTML event arriving while `in_html_block` is false is dropped in
total silence. `end.rs:315` has the same shape, and if *that* guard fails, `in_html_block` is
left `true` and `html_acc` keeps stale bytes for the next block to inherit. The upgrade hazard
is the catalogued one: pulldown-cmark has added `Event::InlineMath` / `Event::DisplayMath` and
further `Tag` variants across recent releases, and under these three arms a version bump
renders a maths block as *nothing at all* with every existing test green.

**Fix:** `pulldown_cmark::Event` is not `#[non_exhaustive]`, so deleting the bare `_` makes
the compiler enumerate the gap at the next upgrade — which is the whole point. Spell the
genuinely inert variants explicitly with their reason; where one must stay unhandled, route it
through one named sink (`other => self.unhandled_event(other)`) that `log::debug!`s the
variant name once per document, because ScrAP-78's lesson is that the failure is *silence*.
Make the two guarded HTML arms total so nothing can reach `_` by failing a guard.

### `F-SPAN-001` — Five duplicated Pango-span builders, one already divergent ✅ Done

- **Severity:** High
- **Reference:** `src/renderer/mod.rs:86-102` (`mark_open`) vs `src/export/markup.rs:76-88`
  (`Inline::Highlight`) — **already divergent**; plus `renderer/mod.rs:69-77` vs
  `markup.rs:89-97`, `:125-136` vs `:46-59`, `:108-111` vs `:43`, `:140-156` vs `:60-75`
- **Found by:** anti-pattern-C (#2, High), DRY-C (C-7, Medium), DRY-B, testability-C (T-7, Medium)
- **Spec:** POLICY § "One theme key, every application path".

The two mark builders are byte-identical except for `{fg}` — the `mark_fg` projection at
`renderer/mod.rs:93-96`. That single difference is the mechanism behind `F-PDF-001`'s
`mark_fg` row: not an oversight in a shared function, but a second copy that was never
updated. `renderer/mod.rs:89-92` even documents the reasoning for adding `mark_fg` *to that
copy*, with no reference to the twin one module over. The other four pairs have not diverged
yet, and that is luck rather than structure. There is a second axis: the renderer copies
resolve against the global `crate::theme::active()` while the export copies take an explicit
`&Theme` (documented at `markup.rs:39-42` as deliberate, for the PDF's System-light
resolution) — so the two copies read from two different sources, which means a test on one
says nothing about the other. Testability-C records the consequence: the renderer builders'
tests are self-referential.

**Fix:** lift one display-free `pangospan` module owning every span in this vocabulary, taking
an explicit `&Theme`, and returning `(open, close)` as a **pair for all five** — not only
`strike_tags`. `renderer/mod.rs:118-124` already explains why the pair must come from one call
(a mismatched `</s>` / `</span>` fails `pango_parse_markup` and renders the cell EMPTY,
ScrAP-163), and that argument applies verbatim to any future themed variant of the other four.
`renderer/mod.rs` keeps thin wrappers so the global lookup stays at the boundary. Enforcement
is a per-span parity test (`assert_eq!(pangospan::mark(&t).0, renderer::mark_open())` under a
test-set active theme) — there is no method to ban, so the ladder stops at the test rung.

### `F-PKG-001` — The RPM `%files` manifest was not updated when `payload.sh` gained `data/sprites/` ✅ Done

- **Severity:** High
- **Reference:** `packaging/linux/payload.sh:82` (added on this branch) vs
  `packaging/linux/build-rpm.sh:91-97` (untouched); `packaging/linux/build-deb.sh:114` for
  contrast
- **Found by:** spec-B (#1, High)
- **Spec:** SCHEMA.md § How a `*_sprite` key resolves, consequence 2 — *"a packaging omission
  of `data/sprites/` costs a log line and nothing else"* is a statement about the **runtime**
  cost, not a licence for the packaging step to fail. POLICY § Build pipeline requires the
  packaged artefact to be the one the gates ran against.

`payload.sh:82` stages `/usr/share/$PKG/sprites/*` into the buildroot; `%files` lists it
nowhere. `build-rpm.sh` sets no `%_unpackaged_files_terminate_build 0`, so rpmbuild's default
turns this into a **hard build failure** ("Installed (but unpackaged) file(s) found") — and
where that macro is relaxed the RPM simply ships without the sprites. The `.deb` is unaffected
(`dpkg-deb --build` packages the staged tree wholesale), so the two Linux packages diverge —
precisely the failure `payload.sh`'s own header says the file exists to prevent: *"build-deb.sh
and build-rpm.sh install the same five things … Written twice, the two layouts drift."* The
payload is now shared and the manifest is not.

**Fix:** add `%{_datadir}/$PKG/sprites` to `%files` (a bare directory path packages it
recursively). Better: derive `%files` from the staged tree, or add a gate that diffs
`stage_payload`'s output against the manifest.

### `F-THEME-001` — `symbol` inherits `[themes.system]`'s glyph, contradicting the module's own contract and diverging the two chooser surfaces ✅ Done

- **Severity:** High
- **Reference:** `src/theme/resolve.rs:126` (`symbol: src.text(&keys::SYMBOL)` — the two-source
  bare walk, `sources.rs:50-52`) vs `src/theme/resolve.rs:117-125` (`name`, which correctly
  uses `own_text`); the contract at `src/theme/spec.rs:184-191`; `data/themes.toml:43`.
  Divergent surfaces: `src/app/menubar.rs:229-234` (via `chooser_list`, `theme/mod.rs:168-191`,
  which uses `own_text` → **no symbol**) vs `src/window/actions.rs:314-318` (resolved
  `Theme::symbol` → **"🪟 <name>"**)
- **Found by:** anti-pattern-A (H1, High), spec-A (F2, Medium)
- **Spec:** TDD 18.1 — the chooser surfaces *"always show the same choice"*. TDD 18.14 — a
  theme added as data appears in the chooser and renders correctly.
  `ThemeSpec::own_text`'s own rustdoc states the contract: *"a theme's display **name and
  picker symbol** belong to that theme."*

All seven shipped themes state a symbol, so this is latent today and bites exactly the case
TDD 18.14 is about: a user-added theme. There is no test for a symbol-less theme.

**Fix:** `symbol: selected.own_text(&keys::SYMBOL).map(str::to_string)`, matching `name`,
matching `chooser_list`, matching the documented contract. Add a merge test that adds a
symbol-less theme and asserts both label paths agree.

### `F-SYS-001` — `[themes.system]` is handled by a bespoke carve-out split across two files that must be edited together ✅ Done

- **Severity:** High
- **Reference:** `src/theme/mod.rs:209-218` — `.filter(|_| id != SYSTEM_ID)` at **`:214`** —
  and its compensating branch at `src/theme/resolve.rs:117-125`, specifically `(id == SYSTEM_ID)`
  at **`:120`**
- **Found by:** anti-pattern-A (H2, High), spec-A (Summary, "one-off code exceptions")
- **Spec:** `src/theme/mod.rs:4-6` — *"This module is the **engine**. It carries NO per-theme
  knowledge: no colour constants, no `if theme == "sepia"` branches"* — repeated at
  `data/themes.toml:4`, and `mod.rs:70-73` documents `SYSTEM_ID` as *"Not privileged in any
  other way."* Both statements are now false.

Blanking `selected` for the system id buys nothing: `Sources::walk` iterates `[selected,
system]` and returns the first hit, so when the two are the same spec the walk is idempotent.
The filter's only *effect* is to destroy the system theme's own display name, which then
requires the second carve-out in a **different file** to put it back. Delete either one alone
and the system theme's name silently becomes `"system"` in every picker. Nothing links them
and no test covers the coupling — `tests/system.rs:150-157` asserts through `chooser_list`,
which uses `own_text` and touches neither branch. It is the same shape as ScrAP-324: one
origin got the step, the other did not.

**Fix:** delete both carve-outs. `Themes::resolve` stops filtering; `Theme::resolve`'s `name`
becomes `selected.own_text(&keys::NAME).map(str::to_string).unwrap_or_else(|| id.to_string())`.
For `id == SYSTEM_ID`, `selected` is now the system spec and `own_text(&NAME)` finds `"System"`
directly — no branch. Add a regression test pinning both properties the deleted branches were
protecting (system resolves to `"System"`; a symbol-less theme yields `symbol == None`,
which also closes `F-THEME-001`).

### `F-FLOOR-001` — `builtin_system_spec_matches_the_floor` promises totality and delivers 19 of 22, and `resolve.rs` claims otherwise twice ✅ Done

- **Severity:** High
- **Reference:** the claim at `src/theme/resolve.rs:7-9` and again at `:41-43`; the guard at
  `src/theme/tests/system.rs:26-55`; the un-const'd overlay floors at `src/theme/resolve.rs:174-179`
  and their data twins at `data/themes.toml:46-53`; the tautological assertion at
  `src/theme/tests/headings.rs:250`
- **Found by:** anti-pattern-A (H3, High), testability-A (H4, High), spec-A (F3 + F10, Medium),
  DRY-A (M-3, Medium)
- **Spec:** `resolve.rs:7-9` states, twice and in identical words, that the guard *"asserts
  **each one** equals the shipped `data/themes.toml` `[themes.system]` value, so the data file
  stays the place a human reads and edits, and drift is a test failure."* POLICY § *No
  hard-coded styling* and THEMING § Resolution order make `[themes.system]` the register that
  answers "what does this app hardcode?".

| Floor | In `[themes.system]`? | Asserted? |
|---|---|---|
| `F_LINK_UNDERLINE` (`resolve.rs:73`) | yes — `themes.toml:76` | **no** |
| `F_HEADING_BAND_PADDING` (`:66`) | yes — `themes.toml:90` | **no** |
| `F_HEADING_BAND_RADIUS` (`:59`) | **no** | only tautologically at `headings.rs:250` — the system theme states no radius, so the assertion compares the floor against itself and passes with the shipped value changed to anything |
| the four overlay colours (`#FFD133_61`, `#f6d32d`, `#ff7800`, `#fff59d_88`) | yes — `themes.toml:46-53` | **no** — and they are not consts at all, just inline string literals |

`F_LINK_UNDERLINE` is the highest-consequence miss: `resolve.rs:70-73` explains it is `Single`
and not `None` *"because changing it would move System (TDD 18.2)"* — the byte-identical
rendering guarantee. Change `themes.toml:76` to `"none"` today and every heading-rule and
System-regression test still passes. `heading_band_padding` is the one floor whose own doc
flags it as exceptional ("NON-ZERO, unlike every other decoration default here"), i.e. the
value most likely to be "corrected" in one place and not the other. `heading_band_radius` is
absent from `[themes.system]` entirely, so for that key the floor genuinely **is** a second
source of truth, which the module doc denies twice. Related: SCHEMA.md documents
`find_hl_current_color`'s default as **derived**, but `resolve.rs:176` makes it a
non-`Option` fixed `#ff7800` that can never fall through to the desktop probe.

**Fix:** add `heading_band_radius = 0` to `[themes.system]`; promote the four overlay hexes to
named consts; and make the guard un-forgettable rather than longer — resolve `[themes.system]`
twice, once against the shipped spec and once against `ThemeSpec::default()`, and assert the
two `Theme`s are `==` (it already derives `PartialEq` at `model.rs:256`; normalise `name`/`id`).
That is one assertion with no list, and it fails the moment any shipped value stops matching
its floor, including for a key added next year. Delete or repair the tautology at
`headings.rs:250`, and correct SCHEMA.md's Default column for `find_hl_current_color`.

### `F-REG-001` — The registry carries type and levelling but not default or clamp; the vocabulary is spelled in five places, three unenforced ✅ Done

- **Severity:** High
- **Reference:** `src/theme/keys.rs:66-72` (`struct Key { name, kind, levelling }`) and
  `:139-229`; the missing halves open-coded at `src/theme/resolve.rs:27-35` (five clamp ranges),
  `:45-82` (22 `F_*` floors) and `:113-258` (a 146-line `Theme { … }` literal re-pairing key ↔
  floor ↔ range **by hand, per key** — `METRIC_RANGE` alone is passed at 13 call sites)
- **Found by:** DRY-A (H-1, High); the consequences are `F-FLOOR-001` and `F-SINK-001`
- **Spec:** `sdd/SCHEMA.md` states a key's four properties uniformly — type suffix, default,
  clamp range, optional narrowing. The registry expresses two. `keys.rs:1-18` says the registry
  exists to end exactly this drift class; it ended it for validation, merge and sprite
  resolution and left it standing for defaults and clamps.

Adding one metric key touches seven or eight places, of which only two are linked by the
compiler; omitting the `[themes.system]` entry, the floor guard assertion or the SCHEMA row
compiles, passes and drifts.

**Fix:** put the floor and the clamp in the registry, where the schema already puts them —
extend `keys!`'s optional-tail grammar the way it already handles the optional levelling word
(`HEADING_SPACE_BELOW = "heading_space_below" : Int Heading floor [4,4,2,2,2] clamp 0..=400;`)
and have `Sources`' accessors read them off the `Key` they are already handed. This deletes 22
floor consts and 5 range consts, strips ~40 argument-passing lines from the `Theme{}` literal,
and — the important part — turns the floor guard into a loop instead of a list.

### `F-CONTRAST-001` — Every ink-on-fill contrast pair in the shipped themes is hand-computed in a TOML comment and asserted nowhere ✅ Done

- **Severity:** High
- **Reference:** `src/theme/tests/contrast.rs:31-66` (the ink list built by hand at `:44-53`),
  `:88-100` (the band check, `:92`); the ungated hand-computed ratios at `data/themes.toml:459-461`,
  `:483-490`, `:522-530`, `:630-632`, `:667`, `:670-672`
- **Found by:** DRY-A (H-4, High), anti-pattern-A (M7 + M8, Medium), testability-A (M6, Medium)
- **Spec:** `contrast.rs:7-9` — the gate's whole purpose is *"what stops a later 'warm it up a
  bit' tweak from quietly degrading readability."*

The three `every_theme_*` gates cover `background`/`foreground`, `heading_colors[]`,
`heading_rule.underline_color[]`, `link_underline_color` and `strikethrough_color`. They cover
**none** of `rule_color`, `mark_fg`, `table_head_fg`, `list_marker_color`,
`annotation_chip_fg`, `link_color` or `blockquote_bar_color` — six of which carry a
hand-computed ratio in a `themes.toml` comment and nothing else. `link_color` is the sharpest:
it is body-adjacent *text*, Pixel Quest's is deliberately below the 4.5:1 floor, and because
nothing checks it, a future theme's accidental low-contrast link is indistinguishable from
Pixel Quest's deliberate one.

Separately, the band check reads `heading_band.fills[level]` only. A band has two other
possible appearances, both in the model and both in SCHEMA.md: `gradient_to[level]` (a theme
with a dark fill and a pale second stop passes on the fill and is unreadable across the bottom
half of its own band) and `sprites.heading_band[level]` (where a sprite exists the fill is not
the surface at all, so the gate measures a colour that is never painted). Two shipped themes
reasoned their way around this hazard in prose comments (`themes.toml:283-288`, `:596-600`)
that the automated floor cannot see.

**Fix:** drive the ink list off the model as a `(ink, surface, floor)` table rather than by
hand, with a small **named** allow-list carrying the two deliberate sub-AA exceptions so each
is *stated* rather than merely unmeasured. Check the band ink against both gradient endpoints,
and `continue` with a named reason where a sprite outranks the fill.

### `F-COV-001` — The coverage ratchet did not move for this branch, and the measured number fell ~0.9pt with the gate green ✅ Done

- **Severity:** High
- **Reference:** `scripts/coverage.sh:270` (`FLOOR=80.33`), the ratchet log at `:242-269`
  (last entry claims **81.74%** at `51caea6`), the slack statement at `:268`, the `IGNORE`
  regex at `:404`, and the scope note at `:349-352`
- **Found by:** testability-A (H1 + H2, High), spec-A (F7, Medium), DRY-A (M-8, Medium),
  anti-pattern-A (M9 — **superseded**, see *Adjudicated disagreements*)
- **Spec:** POLICY build-pipeline step 6 — *"Scoped line coverage is a no-regression **ratchet**
  … When new tests raise coverage, raise `FLOOR` in the script in the same change."* The
  script's own log at `:263-269`: *"Skipping a move because it is small is how a ratchet
  quietly stops tracking."*

Three things, one gate:

1. **The floor was not raised** for `f61a7fa` (TDD 18.29-18.31), `595d517` (the registry
   rewrite, TDD 18.32-18.35, plus `keys.rs`'s four new registry tests and the whole `sources.rs`
   test module) or `7f6b09d` (the module split). Every one of those files is inside the gate.
2. **The measured number moved down and nobody logged it.** DRY-A actually ran
   `scripts/coverage.sh` at HEAD: it **passes at 80.85% against `FLOOR=80.33`**, while the last
   log entry claims 81.74% at `51caea6`. That is a ~0.9pt drop with no log entry, and the gate
   stays green because of the slack.
3. **The slack exceeds the change.** 1.4pt of a ~30k-line gated scope is roughly 400 lines that
   can evaporate with the gate green; `src/theme/` is ~2,300 new lines. A gate whose margin
   exceeds the size of the change it is gating is not gating that change. The margin grew from
   ~1pt to ~1.4pt across three entries, which is the mechanism by which a ratchet stops tracking.

**Related scope problem:** `scripts/coverage.sh:404`'s `IGNORE` excludes `codeview/` wholesale
via `codeview[/\\][a-z_]+\.rs`, which swallows `marker_substitute` (`gutter.rs:158`) — the one
pure decision function of the sprite family, added on this branch. POLICY build-pipeline step 6
says *"pure decision logic is always in"* and *"when adding logic to an excluded file, extract
the decision core into its own logic module … that extraction is the mechanism by which the
floor rises."* The scope note at `:349-352` still claims codeview's "one pure piece" is
`group_by_line`.

**Fix:** in a dedicated commit (not on the way past a feature), re-measure, raise `FLOOR` with
a log entry in the existing style covering 18.29-18.35 and the split, and **explain the 0.9pt
drop**. Narrow the `codeview/` exclusion or move `marker_substitute` into a gated module. Then
decide the margin question deliberately: either close the gap to ~0.3pt, or keep the aggregate
slack and add a *second*, tight per-directory gate for code under active development
(`cargo llvm-cov --fail-under-lines 90 -- src/theme`), which is what makes "extraction raises
the floor" measurable per module rather than in aggregate.

### `F-H6-001` — The h6→h5 fold has five independent hand-rolled implementations, two of which already disagree ✅ Done

- **Severity:** High
- **Reference:** `src/renderer/emit.rs:95-104` (`_ => TagName::H5`);
  `src/renderer/end.rs:31-37` (`_ => 4`); `src/outline_view.rs:104-112` (`_ => "outline-h5"`);
  `src/export/pdf/decide.rs:205-207` (`heading_scale_index`, `.min(4)`);
  `src/export/html.rs:500` (`.min(crate::theme::HEADING_LEVELS - 1)`). A sixth, defensive
  re-clamp sits at `src/codeview/mod.rs:701`. The guard test's own local constant is at
  `decide.rs:471`.
- **Found by:** DRY-A (H-2, High), spec-C (F-8, Medium — **counts five**), anti-pattern-C
  (#9, Medium — **counts four**)
- **Spec:** `sdd/SCHEMA.md:352-355` — *"There are five levels, not six … The renderer maps h6
  onto the h5 tag **before a tag is ever chosen — on every surface, preview and outline
  alike**."* TDD 2.1a. POLICY § "Prefer extending an existing code path over adding a parallel one."

**Reconciling the counts, explicitly.** Spec-C lists five sites; anti-pattern-C lists four and
adds *"two of which already disagree"*. The two lists are consistent: anti-pattern-C's scope
(Groups 6/7/9) did not include `src/outline_view.rs`, which is not a changed file on this
branch — so its four are spec-C's five minus that one. **Use five as the site count** (plus the
sixth defensive re-clamp at `codeview/mod.rs:701`, which spec-C notes separately), and
anti-pattern-C's *disagreement* observation as the reason it is High rather than cosmetic:
`decide.rs:206` hardcodes `.min(4)` while `html.rs:500` derives its bound from
`crate::theme::HEADING_LEVELS - 1`. If `HEADING_LEVELS` ever changes, `html.rs` adapts and
`decide.rs` does not — and `decide.rs`'s own guard test hardcodes `const SCALE_LEN: usize = 5`
locally, so it would not catch the disagreement either. Three of the five hardcode `4` or `H5`.

The irony worth recording: `renderer/mod.rs:300-302` documents `HeadingSpan::level_index` as
*"computed once here so the paint path indexes rather than re-deriving a fold that would then
have two definitions to disagree"* — while being itself the second of five definitions. And
the project already built the named seam: `decide::heading_scale_index`, whose doc says *"The
clamp is the whole function."* Exactly one of the five calls it.

**Fix:** promote one `crate::theme::heading_slot(level: u8) -> usize`, defined as
`(level as usize).saturating_sub(1).min(HEADING_LEVELS - 1)`. Rewrite all five to call it
(`emit.rs` then needs a `const HEADING_TAGS: [TagName; HEADING_LEVELS]`, which removes the fold
there too). Re-point `decide::heading_scale_index` at it and fix `decide.rs:471` to use
`crate::theme::HEADING_LEVELS`. Sweep afterwards: `grep -rn 'min(4)\|_ => 4,\|=> TagName::H5'`
should come back empty outside the seam.

### `F-MARKERKEY-001` — Marker-kind → theme-key dispatch is open-coded eight times across three sinks ✅ Done

- **Severity:** High
- **Reference:** `src/codeview/gutter.rs:171-178` and `:204-212`;
  `src/export/pdf/decide.rs:119-128`, `:129-138`, `:166-171`, `:188-195`;
  `src/export/html.rs:582-587` and `:731-764`
- **Found by:** DRY-B (F-1, High), DRY-C (C-8, Medium)
- **Spec:** SCHEMA.md § Lists — bullet is depth-tiered; ordered, task-unchecked and
  task-checked each read their own key; a task item inside an ordered list is still a checkbox.
  `keys.rs`'s module doc: *"a list that is written once and read four times cannot drift from
  itself; four lists could, and did."*

Eight tables encode one fact. Four of them live in a single ~100-line file
(`pdf/decide.rs`), each re-stating the arm order and each carrying its own copy of the "task
arms come first" comment. A new marker kind, or a change to which key a kind reads, is eight
coordinated edits with **zero** compiler or test enforcement, and six of the eight still
compile after a partial change. The codebase argues itself out of the fix at
`gutter.rs:196-199` — *"the FOLD is shared, the kind dispatch cannot be"* — but that premise is
false as stated: nothing prevents normalising the PDF sink's `(task, start)` pair into a
`ListMarkerKind` at its boundary. `ListMarkerKind` lives in `crate::renderer` and is
display-free; `MarkerSubstitute` is display-free too and only *happens* to live beside the
paint code. The stated obstacle ("the module that owns the gutter's version draws with GTK")
is an argument for moving the dispatch **out** of `gutter.rs`, not for duplicating it.

**Fix:** promote the marker decision into a display-free `src/theme/marker.rs` and delete all
eight tables in favour of one. This is the same move `F-SPRITE-BRANCH-001` asks for and should
be done in the same change.

### `F-HTMLDUP-001` — `html.rs`'s ordered-marker arm is a verbatim reimplementation of `emit_marker_rule` a few lines above it ✅ Done

- **Severity:** High
- **Reference:** `src/export/html.rs:738-764` (the 1-element loop) vs `:785-810`
  (`emit_marker_rule`, its own definition)
- **Found by:** DRY-B (F-3, High), DRY-C (C-9, Medium), anti-pattern-C (#12, Medium)

A `for` loop over a one-element array literal reimplements a function defined a few dozen lines
away in the same file, over the same data, for the ordered marker only. Any correction to the
sprite/glyph emission — including `F-ALPHA-001`'s and `F-SPRITE-BRANCH-001`'s — must be applied
twice or the ordered marker silently diverges from every other marker in the sheet.

**Fix:** delete the loop and call `emit_marker_rule` with the ordered selector.

### `F-RADIUS-001` — `table_cell_radius` rounds table cells in the preview and code blocks in HTML ✅ Done

- **Severity:** High
- **Reference:** `src/preview/css.rs:252-258` (table cells) vs `src/export/html.rs:420` +
  `:478` (the `pre` rule) and `:427` (`th, td` — **no radius**); PDF strokes plain rectangles at
  `src/export/pdf/ink.rs:417-430` with no scope-limit comment (contrast `HeadingBandInk`'s
  explicit *"No radius"* at `pdf/mod.rs:131-141`)
- **Found by:** DRY-C (C-6, High), spec-C (F-4, Medium)
- **Spec:** SCHEMA.md § Table — `table_cell_radius`, "Clamped 0–400", with no sink limit stated.

A theme setting `table_cell_radius = 8` rounds its **table cells** in the preview, rounds its
**code blocks** in the HTML export and leaves the table cells square, and rounds nothing on the
page. The two renderings are exactly inverted for one key. The same reuse also puts
`table_cell_padding_v/h` on `<pre>` in HTML alone. This is not a naming quibble: it is one key
with two meanings depending on which sink you ask, recorded nowhere, produced by reaching for
whatever format argument was already in scope. The related `heading_band_radius` case is the
same shape read from the other end — the PDF's limit is stated in a **rustdoc**
(`pdf/mod.rs:132-149`) and not in SCHEMA.md, which is where a theme author reads.

**Fix (preferred):** add `code_block_padding` and `code_block_radius` to `keys.rs`, wire them
through `preview/css.rs`'s card path and `html.rs`'s `pre` rule, and restore
`border-radius: {radius}px` to `th, td`. If new keys are unwanted now, at minimum move the
radius to `th, td` and name the reuse explicitly in the stylesheet arguments. Separately,
decide `heading_band_radius`/`table_cell_radius` in the PDF: implement (cairo has `arc`) or
state the limit in SCHEMA.md in the same voice `annotation_chip_sprite` uses, with a TDD
citation. What is not acceptable is a limit that exists only in a rustdoc.

### `F-XDG-001` — Theme resolution reads the XDG search path with no seam, and SCHEMA's Search-path section has zero tests ✅ Done

- **Severity:** High
- **Reference:** `src/theme/mod.rs:254-282` (`find_themes_file`, which takes **no parameters**),
  `:239-250` (`load`), and the `XDG_CONFIG_HOME` snapshot note at `:226-235`
- **Found by:** testability-A (H3, High); testability-A M2 and M8 are its consequences
- **Spec:** SCHEMA.md lines 232-249 — the three-row table, first-match-wins, and the Windows
  `%APPDATA%` rule at line 245. SCHEMA line 248 records that this is not hypothetical:
  *"it once made user theme overrides unreachable on Windows outright."*

`find_themes_file` reads `crate::config::user_config_dir()`, `glib::user_data_dir()` and
`glib::system_data_dirs()` inline and `read_to_string`s each candidate until one hits. There is
no trait, no parameter, no injection point — so **every** rule SCHEMA states about the search
path is untested: row ordering, `$XDG_DATA_DIRS` being *iterated* rather than hard-coded to
`/usr/share`, first-match-wins (a later file does not merge over an earlier one), the returned
directory being the found file's own parent (which is the sprite origin), and the Windows
fallback. A test today could only work by mutating `std::env` — process-global, breaks
`cargo test`'s parallelism — which is a hazard, not a test. The code that shipped the Windows
defect still has no test, and the `XDG_CONFIG_HOME` snapshot workaround means the correct
behaviour here is *subtle*, which is exactly what needs a test rather than a paragraph.

**Fix:** a data seam, not a trait (a trait object for one call site is over-abstraction). Split
the 28 lines into *"where would I look?"* (pure, over a snapshotted `SearchBases { config,
data_home, system_dirs }`) and *"read the first one that exists"* (thin I/O). The pure half then
gets the six tests above, and testability-A's M2 (production merges from a `Directory` origin
while ~15 merge tests merge from a `Compiled` origin through a `#[cfg(test)]`-only constructor)
falls out of the same seam.

---

## 🟡 Medium

### `F-BQ-001` — `blockquote_fg` re-inks a heading and a `==mark==` inside a quote ✅ Done

- **Severity:** Medium
- **Reference:** `src/tags.rs:214` (`blockquote-ink` registered first, therefore lowest
  priority — correct), `:306` (`if let Some(c) = heading_color`), `:370`
  (`if let Some(fg) = mark_fg`), `:449` (`t.set_foreground(Some(&link_fg))` — **unconditional**)
- **Found by:** spec-B (#6, Medium)
- **Spec:** `sdd/SCHEMA.md` § Blockquote, `blockquote_fg` — *"Re-inks the quote's prose only: a
  link, **a heading** or **a `==mark==`** inside the quote **keeps its own colour**."* TDD 18.29.

**⚠️ Scope note on the deferral.** Deferral **D3** (blockquote ink riding a second,
separately-prioritised `GtkTextTag` from the one carrying its margin) is by design and the
priority ladder **is** correct — that half is accepted and not re-litigated. But SCHEMA.md's
`blockquote_fg` row guarantees **three** constructs keep their own colour, and the code
guarantees **one**. Orchestrator-verified: `src/tags.rs:449` sets the link foreground
unconditionally (safe), while `:306` (heading) and `:370` (mark) set it only `if let Some(…)`.
A theme that states `blockquote_fg` and leaves `heading_color`/`mark_fg` unstated therefore has
no tag above `blockquote-ink` setting a foreground on those runs, so **two of the three named
constructs take the quote's ink**. That is wider than the declared limit and is a real defect.
The PDF sink fails the heading case for an unrelated second reason — see `F-PDF-001`.

**Test gap:** `a_quote_panel_inks_the_quote_but_never_the_link_inside_it`
(`src/preview/build.rs:1895`) covers the link only. Neither the heading nor the mark case has a
preview test.

**Fix:** either register the heading and mark tags with an unconditional foreground (the
resolved body ink where the theme states none, exactly as the link tag does), or amend the
SCHEMA row to *"keeps its own colour where it states one"*. Add the two missing test cases so
the boundary is pinned either way.

### `F-METRIC-001` — The PDF sink reads three design-time **pixel** metrics as **points** while converting a fourth group ✅ Done

- **Severity:** Medium · **Found by:** spec-C (F-6)
- **Reference:** converted — `src/export/pdf/measure/table.rs:104-106` (`× PT_PER_PX`),
  `measure.rs:392` (image natural size). **Not** converted — `src/export/pdf/ink.rs:124`
  (`blockquote_bar_width`), `measure.rs:353` (`rule_space`), `measure.rs:136` and `:248`
  (`heading_band_padding`)
- **Spec:** SCHEMA.md § Key naming — *"Metrics are design-time pixels at zoom 1.0"*; THEMING §
  Pixel metrics and zoom. The PDF has no zoom, so "scaled on apply" collapses to px→pt.

A `blockquote_bar_width = 3` draws a 3 pt (≈4 px-equivalent) bar on the page beside a 3 px bar
on screen — 33% over — while the table borders beside it convert correctly. Worse, it is
coherent *per key*, so a reader checking one metric concludes the sink is right.
**Fix:** route every `Metrics` read in the sink through one `fn metric_pt(px: i32) -> f64`.
Note the clamp is already single-sourced correctly (`theme/resolve.rs:26-34`, applied once in
`Theme::resolve` and re-implemented in neither sink — verified, no finding); it is the *unit*
that is not.

### `F-TILE-001` — The tile-a-texture sequence is copy-pasted three times (five in the tree) with no seam ✅ Done

- **Severity:** Medium · **Found by:** spec-B (#4), anti-pattern-B (#5), DRY-A (M-2), DRY-B (F-5), DRY-C (C-10)
- **Reference:** `src/codeview/mod.rs:739-748` (band), `:844-853` (bar),
  `src/widgets/rule.rs:112-116` (rule); the resample-then-`append_texture` half again at
  `src/codeview/gutter.rs:261-262` and `mod.rs:1051-1062`; the export sinks name the preview's
  `push_repeat` in comments rather than sharing it (`src/export/html.rs:1726`,
  `src/export/pdf/ink.rs:171`)
- **Spec:** SCHEMA.md requires *the branch* to be per-renderer; it says nothing about the
  *paint*, which is one operation with one correct spelling at this GTK floor.
  `sdd/THEMING.md:110` already describes the rule widget as using "the same
  `push_repeat`/`append_texture` pair every other sprite in this vocabulary is painted with" —
  a claim about a shared seam that does not exist.

Three real variations already exist and none is stated: the band and bar anchor the tile at
`rect.x()/rect.y()` (buffer coordinates, so the phase shifts as the document scrolls) while
`SpriteRule` anchors at `0,0`; and `SpriteRule::snapshot` guards zero/negative dimensions
(`rule.rs:104-106`) while the two `codeview` sites do not.
**Fix:** one `tile_texture(snapshot, rect, origin, tex)` in `src/widgets/mod.rs` (beside
`unparent_all_children`) with the zero-size guard folded in and the `origin` parameter making
the phase decision explicit, plus a `draw_into` twin for the two resample sites. A future
filter choice once the GTK floor passes 4.10 (`gsk_snapshot_append_scaled_texture`,
GTK4Rs/AP-114, which `sprite.rs:28-34` already anticipates) then lands once.

### `F-GOD-001` — `snapshot_layer` is a 735-line god function in a GTK draw callback, gated by a hand-maintained mirror ⛔ Declined

- **Severity:** Medium · **Found by:** anti-pattern-B (#4), testability-B (T-4)
- **Reference:** `src/codeview/mod.rs:518-1253`; the gate at `:546-553`

One function owns the layer gate, the viewport read, quote extents, the quote panel, the
heading band (radius clip, sprite tiling, gradient), the code cards and their hit-rect table,
the blockquote bars, the list-marker gutter with its checkbox hit-boxes, the annotation chips
with grouping and count numerals, the copy buttons, and the pending-open state machine. Three
concrete costs: (1) the gate at `:546-553` is a hand-maintained mirror of the drawn-vector set
and **the code says so** — *"⚠️ EVERY drawn vector belongs in this gate. A new decoration whose
vector is missing here paints on a document that happens to have a code block or a list in it
and silently never paints on one that does not"* — a warning that exists because the structure
makes the omission possible; `heading_spans` needed a bespoke pixel test written specifically
to catch its omission. (2) A panic here unwinds through C and aborts the process; the body is
currently panic-free (verified) but the surface to keep that true grows per decoration. (3)
Nothing here is testable except by rendering the whole view — which is why
`F-SPRITEPAINT-001`'s two gaps exist.
**Fix:** one function per decoration taking a small `PaintCtx`, and replace the hand-maintained
gate with a derived one (pair each paint function with `fn has_work(&self) -> bool` and make
the early return `if decorations.iter().all(|d| !d.has_work(imp))`). Also factor the four
near-identical ~30-line render-and-download blocks in the test module
(`:1831-1850`, `:1915-1931`, `:2015-2032`, `:2206-2219`) into one `framebuffer_of` helper —
they already carry the same `GTK4Rs/AP-272` comment and must be edited in lockstep.

**⛔ DECLINED — two of the three parts landed; the 735-line decomposition did not.** Deferred by the operator 2026-08-27 and carried as **D6** in the Deferred section, with the ordering-net prerequisite stated there.

LANDED: the gate is derived from a single `DRAWN_VECTORS` table (`codeview/mod.rs`),
each row carrying the decoration's name, its nothing-else-draws FIXTURE and the
predicate — plus `every_drawn_vector_opens_the_gate_by_itself`, which is the bespoke
test `heading_spans` needed, written once for the whole table and extended by the same
one row that extends the gate. It also asserts each fixture populates NO OTHER vector,
which is the property that makes the sweep about the decoration rather than about some
construct the fixture happens to carry. The last three verbatim render-and-download
blocks now call `framebuffer_of`, leaving one `render_texture` in the file.

NOT LANDED, and the reason: extracting seven paint bodies out of a 680-line GTK draw
callback is verified only by the pixel suite, and the property most at risk is the one
the suite asserts least — the ORDER between decorations ("a heading band or a code card
INSIDE a quote must land ON the panel, not under it"). The AboveText half is not a paint
function either: it carries the pending-open state machine that dispatches an idle. This
is the same species as `F-RENDERER-001`, which the review itself says is "worth
scheduling rather than bundling", and it wants its own change with its own before/after
pixel comparison per decoration. RESIDUAL, stated in the table's own rustdoc: Rust
cannot reflect over `imp`'s fields, so a vector added to the struct and NOT to the table
is still invisible to the sweep — what catches that is the pixel test each decoration
owns (verified: deleting the `heading bands` row turns
`a_heading_band_is_painted_on_a_document_that_has_nothing_else_drawn` red).

### `F-GOD-002` — `stylesheet()` is 140 lines and one `write!` with 44 named arguments ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-C (#13), testability-C (T-11)
- **Reference:** `src/export/html.rs:379-518`; the single `write!` spans `:412-495`

`html.rs` as a whole is *well* decomposed — 15 named helpers, most with a stated rationale and
a mutation-checked test. `stylesheet` is the exception. It is diff-hostile (adding one rule
touches two places 60 lines apart, and every argument is a `String`, so a mis-substitution
still compiles), and ~20 tests reach into it by line-prefix string matching
(`css.lines().find(|l| l.starts_with("h1 "))` at `:1245` and eight siblings, `starts_with("th {")`
at `:1518`), which is why one test has to assert on the exact text `"margin: 1.2em 0 4px;"` —
an assertion on formatting, not behaviour.
**Fix:** split into six grouped emitters (`page_rules`, `block_rules`, `list_rules`,
`inline_rules`, `chrome_rules`, `heading_rules`), each returning a `String` and each
independently testable — the shape the file already uses everywhere else. **Do this before
`F-ALPHA-001`'s change**, which touches roughly 20 of the 44 arguments and is far safer against
six small functions than one.

### `F-INSTALL-001` — `install_products_into_view` takes 12 positional arguments behind an `#[allow(clippy::too_many_arguments)]`, with two transposable pairs ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-B (#6), DRY-B (F-4)
- **Reference:** `src/preview/build.rs:548-588` (the `allow` at `:548`, the parameters at
  `:550-561`); call sites `src/preview/render.rs:114-127` and `:331-344`; the destructures that
  feed them at `render.rs:32-57` and `:267-292`. *(Line reference corrected: DRY-B's `build.rs:563-568`
  is `src/preview/build.rs:563-568` — the root `build.rs` has 183 lines and is not the file meant.)*

`RenderProducts` (`build.rs:27-70`) exists explicitly to end lockstep editing and its rustdoc
says so. Both callers then immediately destructure it back into 22 loose bindings and hand 12
of them **positionally** to a function carrying the `#[allow]`. The hazard was moved, not
removed: adding one render product — which is exactly what this branch did with `heading_spans`
— takes nine coordinated edits, and `code_blocks`/`blockquote_ranges`/`heading_spans` are all
`Vec<…Span>` while `code_block_bg`/`blockquote_bar` are both `gdk::RGBA`, so two adjacent
same-typed arguments transposed at one call site and not the other compiles clean and produces
a wrong preview on exactly one render route.
**Fix:** delete the `#[allow]` by removing its cause — give the function a `ViewInstall` struct,
have `RenderProducts` *contain* it rather than inlining its nine fields, and add
`into_parts()`. Field names then make transposition unrepresentable and a new render product is
a one-line edit the compiler propagates.

### `F-CHOKE-001` — A third render route bypasses the choke point whose rustdoc says it cannot be bypassed ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-B (#7)
- **Reference:** `src/preview/render.rs:374-492` (`refresh_annotations_in_place`); the claims it
  falsifies are at `src/preview/build.rs:562-575` and `src/codeview/mod.rs:174-186`

`install_products_into_view` documents itself as the identical install sequence both `render`
and `re_render` must apply in lockstep, and its `bump_render_generation` call carries the
stronger claim *"because the bump lives in the same choke point that rebuilds the content, no
render path can forget to invalidate."* `refresh_annotations_in_place` rebuilds render products
at `:387` and installs a hand-picked **two** of the nine things the choke point installs
(`set_markers` at `:461`, `set_list_markers` at `:471`), calling neither
`install_products_into_view` nor `bump_render_generation`. Today it is safe and the code argues
why (the structural-identity guard at `:399-401`), but the invariant *as written* is false —
and the comment at `:463-471` is that failure having already happened once, for `list_markers`,
whose checked state was not derivable from the buffer slice.
**Fix (preferred):** split into `install_content` and `install_annotations` so the *set* of
things a route installs is a visible choice rather than an omission, and have this route call
`install_annotations` plus `bump_render_generation`. **Otherwise:** amend `build.rs:562-567`'s
claim to name the third route and why it is exempt. Either way, add it to the list of routes
the rustdoc names.

### `F-PKG-002` — Three packaging scripts, three sprite-copy semantics, no parity guard ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-B (#8), testability-B (T-9), DRY-B (F-14)
- **Reference:** `install.sh:51`, `packaging/linux/payload.sh:82`, `packaging/windows/stage.ps1:153`

All three carry the **same six-line comment, verbatim** (`install.sh:44-50`,
`payload.sh:75-81`, `stage.ps1:146-152`) and then do three different things:

| Condition | `install.sh` | `payload.sh` | `stage.ps1` |
|---|---|---|---|
| `data/sprites/` gains a subdirectory | `install` errors on a directory operand → abort | same | ships it recursively |
| `data/sprites/` is empty | glob stays literal → `install` fails → abort | same | succeeds, ships an empty dir |
| Filename with a space | word-splits | word-splits | fine |
| Run from a different CWD | fine (`$REPO_DIR`-anchored) | **breaks** (`data/sprites/*` is CWD-relative) | fine (`$RepoRoot`-anchored) |

Only the space row is currently un-triggerable. The failure is asymmetric: Windows ships
something Linux refuses to package. The *reference* direction is well guarded
(`src/theme/tests/sprites.rs:56`); the *packaging* direction is unguarded entirely.
**Fix:** `find data/sprites -type f -exec install -Dm644 -t "<dest>" {} +` on both shell scripts
(survives empty, survives subdirectories, does not word-split), anchor `payload.sh` to a
computed repo root like its siblings, and add a ten-line test cross-checking `BUILTIN_SPRITES`'s
keys against the on-disk file set. Deduplicate the six-line comment to one canonical statement.

### `F-RESOLVE-LOG-001` — Two unlogged refusal paths inside `sprite::resolve`, among six that log ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-B (#3)
- **Reference:** `src/sprite.rs:279-281` (`base.canonicalize()` failure) and `:286-296` (the
  `Err(_) => None` arm at `:295`); the contract is at `:234-236` — *"Any failure logs and returns `None`."*

Both are reachable in the field: `canonicalize` fails when the themes file's own directory was
removed or became unreadable between load and resolve (a real state on a removable or network
mount), and `metadata` fails on a permissions change. In both cases **every** sprite in that
themes file goes silently inert — "my whole theme lost its decorations", with nothing in the
log. The tell for how it arrived is the style: those two are written as `let Ok(…) else` and a
match arm, unlike the six `if … { log::warn!; return None }` blocks around them, so the
discipline was applied by shape rather than by rule.
**Fix:** log both in the same voice. Better: have `resolve` return `Result<PathBuf, Refusal>`
with one variant per gate and let its single caller (`SpriteOrigin::resolve`, `:108`) log — then
a new gate cannot be added without a reason, and the wording lives in one place instead of eight.
*(This composes with security `F-SEC-002`: the same match is where `is_file()` belongs.)*

### `F-WARN-001` — The unknown-key `warn` is asserted by nothing, and four value-level refusals name neither the theme nor the key ✅ Done

- **Severity:** Medium · **Found by:** spec-A (F4 Medium + F9 Low), testability-A (M3, Medium),
  anti-pattern-A (M5, Medium)
- **Reference:** the warn exists and names both at `src/theme/spec.rs:99` and `:108-112`; the
  tests that observe only *survival* are at `spec.rs:257-264` and `:266-270`; the anonymous
  refusals are `src/theme/spec.rs:162` (names the key, not the theme),
  `src/theme/value.rs:219`, `:150`, `:314`/`:319-322` (name neither); `parse_color`
  (`value.rs:47-59`) refuses **silently**, with no log at all
- **Spec:** SCHEMA.md § Key resolution — *"An unrecognised key is ignored and logged at `warn`,
  naming the theme id and the key … silence would make a key that never applies
  indistinguishable from one that applied and did nothing."* TDD 18.33; TDD 18.35 lists an
  array-valued `heading_scale` among the retired spellings that must be reported.

Three compounding gaps. (1) **Nothing observes the log** — there is no capture harness anywhere
in the crate (one unrelated `set_boxed_logger` at `forensics/mod.rs:214`), so a refactor that
downgraded the call to `debug!`, dropped the theme id, or deleted it entirely would pass every
gate while producing exactly the silence the rationale forbids. Nine warn sites are in this
position. (2) **The array case is untested** — `keys.rs:302-328` covers the string spellings TDD
18.35 names but not `heading_scale = [2.2, 1.8]`, which takes the *wrong-type* path
(`spec.rs:214-220`), not the unknown-key path. (3) **`parse_color` is the one parser that
refuses silently**, and colours are ~35 of the ~70 keys — the most typo-prone value class has
no diagnostic at all, and `sources.rs:259-274` proves the skip while asserting nothing about
observability.
**Fix:** add a minimal `#[cfg(test)]` `log::Log` capture with a mutex-guarded RAII installer
(POLICY § Unit tests already prescribes that shape for process-global test state) and assert
level, theme id and key for both the unknown-key and wrong-type paths. Thread a
`Ctx { id, spelling }` into the value-level refusal path, or move refusal into
`ThemeSpec::validate`, which already holds both — that also fixes `F-DIAG-001`. Add the
array-valued `heading_scale` case beside `a_malformed_value_costs_its_own_key_and_never_the_theme`.

### `F-MERGE-001` — SCHEMA's load-bearing "a bare user key does not displace a built-in's narrowed key" consequence is uncovered ✅ Done

- **Severity:** Medium · **Found by:** spec-A (F5)
- **Reference:** the code is **correct** — `overlay` merges per spelling at
  `src/theme/spec.rs:176-178`. The gap is `src/theme/tests/headings.rs:41-120`, which merges a
  *narrowed* user key over a theme (`sepia`) shipping **neither** form — the easy direction only —
  and `src/theme/sources.rs:276-290`, which tests two bare keys.
- **Spec:** SCHEMA.md § Key resolution, the paragraph explicitly flagged as load-bearing.

No test merges a bare `heading_color` over a theme that ships `heading_color_h1`, despite two
shipped themes being in exactly that state (`data/themes.toml:168` synthwave, `:549-551`
pixelquest). Reversing the walk order inside `Sources::walk` would still pass the whole suite
for this case.
**Fix:** merge `[themes.synthwave] heading_color = "#000000"` and assert `heading_colors[0]` is
still synthwave's own `#ff3caf` while `heading_colors[1..]` take the user's bare value.

### `F-SHAPE-001` — The registry↔model array-shape check is a `debug_assert`, while the doc says the two "cannot disagree" ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-A (M6, Medium), spec-A (F13, Low)
- **Reference:** `src/theme/sources.rs:72-81` — the doc claim at `:73`, the `debug_assert_eq!` at `:79`

`N` is inferred from the *destination field's* array length, not from `key.levelling`, and
nothing in the type system connects them. `src.colors::<BULLET_TIERS>(&keys::HEADING_COLOR)`
compiles and, **in release**, resolves three of five heading levels and silently drops h4/h5.
The reverse compiles too and yields two permanently-`None` slots. `debug_assert` is compiled
out of the shipped binary, so the doc invites reliance on a guarantee the artefact users run
does not have — at exactly the moment (wiring a new levelled key) the mismatch would be
introduced. Only the sprite family has a real structural guard
(`src/theme/tests/sprites.rs:129-134`); colours, fonts, glyphs, ints, floats and lines have none.
**Fix:** make it real (an unconditional `assert!` behind levelling-named wrappers —
`heading_colors(&Key)` / `depth_colors(&Key)` — costs nothing at once per key per resolve), and
generalise the sprites test into a registry-driven shape guard covering every levelled field. At
minimum, make the doc honest and add a `#[should_panic]` mismatch test.

### `F-DEPTH-001` — No registry-driven guard for the `Depth` levelling, only for `Heading` ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-A (M13)
- **Reference:** `src/theme/sources.rs:315-354`
  (`every_heading_key_narrows_to_a_level_and_falls_back_to_its_bare_form`, which walks
  `keys::KEYS`) vs `src/theme/sources.rs:240-255`, which tests one hand-picked depth key
  *(the reviewer wrote this second path as `src/theme/tests/../sources.rs:315-354`; normalised
  to `src/theme/sources.rs`)*

The heading test's own docstring makes the case: *"asserted by walking the registry rather than
by naming keys, so a heading key added later is covered the moment it is declared."* The `Depth`
levelling has three declared keys and its fallback rule is the **more** unusual of the two (each
tier falls back to the next shallower one, not to the bare key) — and it gets one hand-picked
key. `LIST_BULLET_SPRITE`'s shallower-tier chain is exercised nowhere.
**Fix:** clone the heading test for `Levelling::Depth`, asserting the shallower chain
(`sample(kind)` at `sources.rs:178-188` already supplies distinguishable values for every kind
including `Sprite`). Better: fold both into one test parameterised on levelling, so a third
levelling added later is covered by construction. Related: testability-A M7 records that a
*refused* depth-tier glyph falls back to the shallower tier where SCHEMA says it falls back to
the drawn marker — and neither behaviour is tested; settle that in the same pass.

### `F-BARWIDTH-001` — `blockquote_bar_width` must be edited in lockstep with the sprite's own pixel width ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-A (M11)
- **Reference:** `data/themes.toml:637-638` (`blockquote_bar_sprite = "sprites/copper-plate.png"`,
  `blockquote_bar_width = 24`); the tile `data/sprites/copper-plate.png` measures 24×24 px; the
  coupling is stated in prose at `themes.toml:633-636` and in SCHEMA.md's Blockquote table

Redraw the plate at 32 px, or drop `blockquote_bar_width` while keeping the sprite, and the bar
renders a **clipped slice** of a tile — a decoration that is *present but wrong*, which is worse
than this vocabulary's usual inert-by-default failure and produces no log line. ScrAP-324's own
lesson applies: *"Where a feature degrades silently on failure, at least one guard must inspect
what the INPUT said, never only what the output did."*
**Fix:** extend `src/theme/tests/sprites.rs:56-84`, which already walks the shipped file with
each theme's sprite references in hand: for every built-in theme naming a
`blockquote_bar_sprite`, decode the compiled-in bytes, read the texture width, and assert
`blockquote_bar_width >= width`. One assertion, data-driven, covering every future theme.

### `F-DEADDOC-001` — Sixteen live doc comments describe mechanisms this branch deleted ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-A (M10, Medium), anti-pattern-C (#8, Medium),
  spec-A (F14, Tidy — nitpick score 2)
- **Reference:** `overlay`'s `take!` list (now one `extend` at `src/theme/spec.rs:176-178`) is
  still named as the live guard by twelve test docstrings — `src/theme/tests/merge.rs:28`,
  `:198`, `:199`, `:212`, `:225`; `tests/headings.rs:38`, `:98`, `:158`, `:265`;
  `tests/lists.rs:33`, `:171`, `:241`. `rewrite_sprite_paths` (replaced by
  `ThemeSpec::resolve_sprites`) is still cited as live at `src/theme/model.rs:352`,
  `src/codeview/gutter.rs:727`, `src/codeview/mod.rs:1896`. Four more assert live coupling is
  dead code at `src/renderer/mod.rs:282-283` and `:396-398`.
- **Spec:** ScrAP-221 — a comment that was true of the implementation and has frozen into a
  statement of requirement.

These are not stray comments; they are the stated **rubric** of the tests. A maintainer reading
`lists.rs:171-173` learns that a per-depth user override must be tested "because omitting a key
from the `take!` list silently drops every user override" — and will keep writing one such test
per new key, forever, for a failure mode the registry made unrepresentable. Worse, it *masks*
the real remaining hazard, which is different and narrower (`F-SINK-001`): a key can still reach
`ThemeSpec` and never reach `Theme`.
**Fix:** rewrite the twelve to name the surviving obligation, retarget the three
`rewrite_sprite_paths` citations, and keep the dated `list_marker` regression note at
`lists.rs:240-243` but mark it as history rather than as a live mechanism.

### `F-FIND-001` — `occurrences_count()`'s `-1` sentinel leaks into `do_find_next`'s loop bound ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-C (#6)
- **Reference:** `src/window/find.rs:765-783` — `:766` (`let total = sc.occurrences_count();`)
  and `:777` (`if n > total { break; }`); the doc it falsifies is at `:786-799`

`find.rs:794-799` argues at length that the sentinel *"is decoded where it ENTERS, in
`update_match_count_label`, and never travels."* That claim is false as written. While the
`GtkSourceSearchContext` is still scanning — precisely the state the sentinel exists to signal,
and the common state on the first Find-Next of a large document — `total == -1`, so `n > total`
is `1 > -1` on the first pass and the loop breaks after one iteration. `find_cursor` is then set
to `FindCursor::Editor(1)` regardless of which match was landed on. The label is masked (`:823-826`
shows `"…"`), so the symptom is that a `1` minted from a sentinel **persists** as the tab's cursor
state after the scan completes — exactly the "under-claim vs wrong-claim" distinction the
`FindCursor` type's own doc (`:104-107`) is defending. Second, `:765` performs an O(document)
rescan on **every** Next/Prev press, while the preview path had this identical defect fixed on
this branch's neighbouring work (`PreviewFindCache`, `find.rs:320-433`, whose doc says so) —
one engine was optimised and the other left, with nothing recording the decision.
**Fix:** decode the sentinel where it enters (`match sc.occurrences_count() { n if n < 0 => None,
n => Some(n) }`) and set `FindCursor::None` rather than `Editor(n)` when it is `None` — the type
already has that state. Hoist the decode into a `fn occurrence_total(sc) -> Option<i32>` seam so
the doc becomes true by construction. For the rescan, use `occurrence_position(&start, &end)` if
available at the project's floor, or cache like `PreviewFindCache` and state why the two engines
differ — an asymmetry stated is not a defect; an asymmetry unstated is.

### `F-TASKSPRITE-001` — The task-marker sprite is emitted per list item as a full base64 data URI ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-C (#7)
- **Reference:** `src/export/html.rs:581-607` (`task_marker_html`), called from `:155` inside
  the `for item in items` loop at `:142`; the sibling general path is `:785-810`
  (`emit_marker_rule`) and `:714-766` (`list_marker_css`), which emit each sprite **once** into
  the stylesheet; `sprite_data_uri` at `:974-997` re-reads and re-encodes on every call

Every other marker sprite reaches the artefact as one CSS rule holding one copy. With
`SPRITE_EMBED_CAP = 512 KiB` (`:969`) and base64's ~4/3 inflation, a 500-item task list produces
roughly **340 MB of HTML** from a single 512 KiB PNG, with 500 re-reads and re-encodes. The doc
at `:576-580` soundly explains why the *mechanism* differs; it does not address why the
**payload** must. The same shape appears for the band: `heading_band_css` is called six times
from the loop at `:499` (`i` = 0,1,2,3,4,4), so a band sprite is encoded six times and appears
six times — twice for the same slot.
**Fix:** memoise `sprite_data_uri` on a `HashMap<SpriteRef, …>` threaded on `Page` (`:81-84`,
which already exists for exactly this kind of state) — that alone removes the repeated decode
with no output change. Then emit the payload once regardless of mechanism (a
`li.task-list-item::before` rule, or one `:root { --task-marker: url(<uri>) }` custom property
the per-item `<img>` references). Guard with
`assert_eq!(out.matches("data:image/png;base64,").count(), 1)` on a 200-item task list — the same
shape as the mutation-checked assertions already at `:1080-1113`.

### `F-TABLEROW-001` — `#[allow(clippy::too_many_arguments)]` plus boolean blindness on `Layouter::table_row` ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-C (#10)
- **Reference:** `src/export/pdf/measure/table.rs:163` (the `allow`), `:164-174` (nine
  parameters ending in two adjacent unlabelled booleans, `is_head` and `keep_with_next`), call
  sites `:83-93` and `:96`

The `allow` is in the one module that introduces three dedicated structs specifically to avoid
this. Two adjacent same-typed booleans at the end of a nine-parameter signature is the
transposition hazard those structs exist to remove.
**Fix:** fold the two flags into a small `RowKind`/`RowFlags` value and delete the `allow`.

### `F-DECODE-001` — The PDF list walk decodes the marker sprite per item, and an item whose first block is not a paragraph gets no marker at all ✅ Done

- **Severity:** Medium · **Found by:** anti-pattern-C (#11), spec-C (F-13, Low), security (amplification of `F-SEC-001`)
- **Reference:** `src/export/pdf/measure.rs:508-510` inside the `for (n, item)` loop at `:503`;
  the attachment guard at `:526-527`; the surfaces are retained on the line at `:556`

Two problems in one block. **(a)** `decode` (`pdf/mod.rs:293-307`) builds a `GdkTexture`,
allocates a cairo `ImageSurface` and copies pixels — per list item. A 500-item bulleted list
with a themed bullet performs 500 identical decodes of the same PNG during a synchronous
export, and holds 500 independent surfaces. The sibling module states the opposite rule twice
in as many words (`ink.rs:54-55`, `:69-71`: *"Decoded ONCE for the page rather than per quoted
line"*) and `measure.rs:131-133` states it a third time for the band. The list-marker site is
the exception and nothing marks it as one — and it is the site with the highest multiplicity,
which is why security `F-SEC-001` names it as the amplification path. **(b)** The sprite and
marker are computed before the item's blocks are walked, but attached only inside the
`if i == 0 { if let Block::Paragraph | Block::Heading = block { … } }` guard — so an item whose
first block is a fenced code block or a nested list decodes the sprite, discards it, and renders
with **no marker at all**, glyph or picture. That is reachable from ordinary Markdown and has
no test.
**Fix:** hoist the decode into a per-`Layouter` `HashMap<SpriteRef, Option<(ImageSurface, f64, f64)>>`
(cairo refcounts the surface, so cloning per item is free — the same argument `measure.rs:132`
already makes for the band), and hoist the marker attachment out of the block-kind guard so it
lands on the first line the item produced, whatever produced it. Add a fixture: a list item
beginning with a fenced block, asserting a marker is present.

### `F-PX-001` — Four call sites re-declare `theme::px`, two with different rounding semantics ✅ Done

- **Severity:** Medium · **Found by:** DRY-B (F-6)
- **Reference:** `src/theme/value.rs:33-35` (the one conversion); four re-declarations across the
  consumers

`theme::px` is the single design-time→pixel conversion and it also has **no unit test**
(testability-A L2, nitpick score 2) despite being the arithmetic applied to every metric.
*(See the Deferred section: ISSUES **O** covers "`px()` exists as three copies" and is not
re-litigated here. What is recorded and **not** covered by O is DRY-B's measurement that two of
the four sites use **different rounding semantics** — that is a behavioural divergence rather
than a duplication, and it is the developer's call whether O absorbs it or it wants its own
entry.)*
**Fix:** route all four through `theme::px`, and add the missing unit test over its rounding
edges.

### `F-PROBE-001` — The desktop named-colour probe chain and its literal floors are written four times ✅ Done

- **Severity:** Medium · **Found by:** DRY-B (F-7)
- **Reference:** four copies across `src/palette.rs` and its consumers

Resolution link 3 (the desktop probe) plus its literal fallback is spelled four times. A change
to the probe order, or to a floor, must land in all four.
**Fix:** one `probe_named(name, floor) -> RGBA` seam in `palette.rs`, which already owns the
"none of them names a colour" contract.

### `F-WCAG-001` — The WCAG contrast walk is written twice in `palette.rs` ✅ Done

- **Severity:** Medium · **Found by:** DRY-B (F-8)
- **Reference:** two implementations of the same ratio walk within `src/palette.rs`

Two implementations of one WCAG computation in one module, either of which can be corrected
without the other. This is also the machinery `F-CONTRAST-001` wants to drive its gate from.
**Fix:** collapse to one, and have `src/theme/tests/contrast.rs` call it rather than carrying
its own thresholds.

### `F-CSS-001` — Two CSS generators overlap on eight concepts, and one drift has already shipped ✅ Done

- **Severity:** Medium · **Found by:** DRY-B (F-9)
- **Reference:** `src/preview/css.rs::theme_css` vs `src/export/html.rs::stylesheet`; the live
  drift is `heading_font`, which reaches the **preview's** table header (`preview/css.rs:274`)
  and not the export's

DRY-B compared the two key by key and corrects the brief's premise: the genuine overlap is ~7-8
concepts, not the whole sheet, because the preview expresses most of the vocabulary through
`GtkTextTag`s and self-drawn snapshots. Within that overlap, `link_underline` is decided twice
and the `Option<RGBA>` → CSS-declaration-fragment idiom is spelled three times across the two
files — and the `heading_font` divergence is already live.
**Fix:** share the small pieces that genuinely overlap (the colour-fragment helper, the
`link_underline` decision) rather than attempting to merge two sheets with different jobs; fix
the `heading_font` table-header divergence in the same change. Note `F-BOLD-001` is the same
shape for `bold_weight`.

### `F-BOLD-001` — `bold_weight` is honoured for `**bold**` on all three surfaces and for the table header on none ✅ Done

- **Severity:** Medium · **Found by:** DRY-C (C-11)
- **Reference:** three independent hardcodings — `src/export/pdf/measure/table.rs:117`,
  `src/preview/css.rs:280`, `src/export/html.rs:427`

`Typography::bold_attr` is shared correctly by all three surfaces for inline bold — it is one of
the branch's better seams — and the table header, which is also bold, hardcodes a weight in
three separate places instead.
**Fix:** read `bold_attr` for the table header on all three surfaces.

### `F-ESCAPE-001` — Two Pango escapers for one grammar, with the seam that gets it right next door ✅ Done

- **Severity:** Medium · **Found by:** DRY-C (C-12)
- **Reference:** `src/export/markup.rs:228` (`escape_pango`) vs
  `MarkerGlyph::escaped_for_pango_markup`; the stale `as_plain` doc at `src/theme/value.rs:330-335`

`MarkerGlyph` is the model — one validated value, three named projections, no `Deref`, and
`escaped_for_html` delegating to the single HTML escaper *"so this project has one HTML escaper
rather than one plus a copy that drifts."* The Pango side is not held to the same standard: a
second escaper exists, and `as_plain`'s doc no longer describes the seam. Security `T-SEC-001`
is the concrete consequence at the CSS boundary (a fourth grammar the "three grammars" comment
undercounts).
**Fix:** delegate `escaped_for_pango_markup` to the one `escape_pango`, add the
`escaped_for_css_string` projection security asks for, and correct the `as_plain` doc.

### `F-RANGE-001` — The range-merge is written twice in `renderer/mod.rs`, both times with positional tuple access ✅ Done

- **Severity:** Medium · **Found by:** DRY-C (C-13)
- **Reference:** `src/renderer/mod.rs:165-181` and `:545-558`

Two implementations of one merge, both violating the project's destructure-by-name convention.
**Fix:** one `fn merge_ranges(&mut Vec<Span>)` with named fields.

### `F-CHOOSER-001` — `chooser_list` returns an anonymous 3-tuple, forcing positional access at seven call and test sites ✅ Done

- **Severity:** Medium · **Found by:** DRY-A (M-6, Medium), anti-pattern-A (L1, Low — 11
  positional sites), testability-A (L3, Low), nitpick batches 2/3/5 (all score 2)
- **Reference:** `src/theme/mod.rs:184`; call and test sites at `src/theme/model.rs:421`,
  `src/theme/tests/system.rs:87`, `:88`, `:91`, `:93`, `:94`, `:152`, `:153`,
  `tests/merge.rs:176`, `tests/headings.rs:313`
- **Spec:** the project convention (recorded in the user's own standing preferences) is to
  destructure tuples **by name**, never positionally.

The triple returns two indistinguishable `String`s, so every call site re-derives which is
which. This is the widest instance of the positional-tuple class on the branch, and it crosses
five test-file boundaries.
**Fix:** a named `ChooserEntry { id, label, symbol }` struct. `ThemeColor` (`model.rs`) read as
`.0` five times, `BUILTIN_SPRITES[0].0` (`src/sprite.rs`), `layout.extents().1`
(`src/export/pdf/measure/table.rs:128`, `:135`, `:186`), the eight bare tuple vectors on
`Renderer` (`src/renderer/mod.rs:266`, `:329`, `:358`, `:374`, `:380`, `:385`, `:397` — one a
4-tuple mixing two index spaces, documented as a hazard in its own comment), and
`src/palette.rs:213-214` / `src/codeview/mod.rs:2390` are the same class; see the Low section.

### `F-CLAMP-001` — Clamp ranges are bare `(T, T)` tuples, read positionally in production and tests ✅ Done

- **Severity:** Medium · **Found by:** DRY-A (M-7)
- **Reference:** `src/theme/resolve.rs:27-35`, and the `pub(super)` reads from
  `src/theme/tests/system.rs:87-94`

`(min, max)` as a bare pair is the same convention violation as `F-CHOOSER-001` and, unlike a
label, a transposition here silently changes what a theme may state.
**Fix:** `struct Clamp<T> { min: T, max: T }` — which is also what `F-REG-001`'s registry
extension wants.

### `F-SPRITETEST-001` — Three near-identical sprite-resolution tests the registry could drive ✅ Done

- **Severity:** Medium · **Found by:** DRY-A (M-9)
- **Reference:** three hand-written variants in `src/theme/tests/sprites.rs`, beside `:100`,
  which already drives off `keys::KEYS`

The file contains both the exemplary registry-driven sweep and three hand-maintained mirrors of
the same question.
**Fix:** fold the three into the registry-driven sweep.

### `F-CELLSEL-001` — The three cell-link CSS selectors are spelled in three places, and the tests mirror rather than reference the list ✅ Done

- **Severity:** Medium · **Found by:** DRY-A (M-10, Medium), anti-pattern-B (#18, Tidy — nitpick score 2)
- **Reference:** `src/preview/css.rs:335-343` (production) vs `:537-541` and `:664-668` (two
  test-local literal arrays)

A fourth selector added to production fails neither test — they assert *these three* exist, not
that the production list is covered. The comment at `:326-334` records that the third selector
was found by looking at a driven render rather than by a test, which is standing evidence that
this list grows.
**Fix:** export `const LINK_CELL_SELECTORS: [&str; 3]` and have both tests iterate it.

### `F-GLOBAL-001` — Global theme and sprite-cache state is restored only on the happy path ✅ Done

- **Severity:** Medium · **Found by:** testability-B (T-8)
- **Reference:** the `#[cfg(test)]` mutators around `src/theme/mod.rs:321-330` and the caches at
  `src/sprite.rs:299-310`

A test that panics mid-body leaves the process-global active theme and the sprite caches set for
whatever runs next, so a failure in one test can turn into a spurious failure or a spurious pass
in another. Related: testability-A M8 records that `themes()` memoises a real filesystem read
with no reset seam, and testability-B T-5 that the parent-traversal test writes a **fixed
filename** into the shared temp directory (two concurrent runs collide).
**Fix:** an RAII guard whose `Drop` restores, per POLICY § Unit tests' own prescription for
process-global test state; and give the traversal test a unique temp directory.

### `F-TAGS-001` — `setup_tags_with_theme` is a 365-line monolith with no display-free seam ✅ Done

- **Severity:** Medium · **Found by:** testability-B (T-6)
- **Reference:** `src/tags.rs`

Every theme→tag decision on the preview's principal mechanism is inside one function that
requires a `GtkTextBuffer`, so none of them — including `F-BQ-001`'s priority ladder and the
`heading_band_padding` inset — can be asserted without standing up a view.
**Fix:** extract the decisions into pure `fn(&Theme) -> TagSpec` values and keep the GTK half a
thin applier, which is exactly what `codeview/gutter.rs` already does and what makes it the
best-tested file in the branch.

### `F-BUILDPRODUCTS-001` — `build_render_products_into` resolves both globals internally though pure alternatives exist ✅ Done

- **Severity:** Medium · **Found by:** testability-B (T-7)
- **Reference:** `src/preview/build.rs`

The function reaches for `crate::theme::active()` and the sprite cache rather than taking them,
so the whole render-products construction can only be exercised against whatever the process
happens to have active.
**Fix:** take `&Theme` as a parameter; the call sites already hold one.

### `F-RENDERER-001` — `Renderer` is a god struct and the whole event walk has zero unit tests ⛔ Declined

- **Severity:** Medium · **Found by:** testability-C (T-6)
- **Reference:** `src/renderer/mod.rs` (the struct), `src/renderer/events.rs:12` (`process`),
  `src/renderer/start.rs:12` (`start_tag`), `src/renderer/end.rs:15` (`end_tag`)

`process` / `start_tag` / `end_tag` cannot be reached without GTK, so the three dispatchers
carrying `F-EVENT-001`'s catch-alls have no unit test at all. Combined with `F-SPAN-001`'s
ambient-theme problem, the renderer's markup builders' tests are self-referential
(testability-C T-7).
**Fix:** the `pangospan` extraction from `F-SPAN-001` plus a buffer-free event sink is the
cheapest route in; this is a larger piece of work and is worth scheduling rather than bundling.

**⛔ DECLINED — scheduled, on the finding's own advice, but PART of it landed anyway.** Deferred by the operator 2026-08-27 and carried as **D7** in the Deferred section, with its acceptance criterion stated there.
`Renderer` now holds the `Rc<Theme>` it renders under and the six themed markup wrappers
(`mark_open`, `ann_hl_open`, `bold_open`, `strike_tags`, `superscript_open`,
`subscript_open`) take `&Theme` instead of reading the process-global — done as part of
`F-BUILDPRODUCTS-001`, and it removes the "self-referential markup tests" half
(testability-C T-7): `pangospan`'s parity check no longer installs an active theme to
prove itself. What remains is the buffer-free event sink that would let `process` /
`start_tag` / `end_tag` be unit-tested at all, which is the larger piece.

### `F-INKSEAM-001` — The `decide` seam succeeded; `ink` and `measure` did not become mechanics ✅ Done

- **Severity:** Medium · **Found by:** testability-C (T-4)
- **Reference:** `src/export/pdf/ink.rs:6-7` claims *"It decides nothing … what a construct is
  came from `decide`. What is left is cairo."* It decides at least five things: theme-key-else-palette
  for the quote bar (`:53`) and the rule (`:68`), and sprite-vs-flat precedence for the bar
  (`:139-154`), the band (`:170-204`) and the rule (`:236-248`). `measure.rs` likewise resolves
  theme keys and reads sprite bytes off disk with no injection seam.

`decide.rs` is genuinely pure and directly testable and needs no change — that half worked. The
other half's doc comment states the rule it is breaking, which is `F-SPRITE-BRANCH-001`'s
mechanism inside this sink.
**Fix:** move the five decisions into `decide` (or the shared precedence seam from
`F-SPRITE-BRANCH-001`) and give the sprite read an injection point so `measure` can be tested
without a filesystem.

### `F-PAGINATE-001` — `lay_out → paginate → draw_page` is temporally coupled with nothing enforcing it, and the harness re-implements the production wiring ✅ Done

- **Severity:** Medium · **Found by:** testability-C (T-5)
- **Reference:** `src/export/pdf/mod.rs` (the three-stage API), and the test harness that
  re-creates the caller's sequence

The order, the theme and the margins must all be supplied consistently and nothing in the types
says so, while the tests build their own wiring — so the tests can pass against a sequence the
production caller does not use. Note the module has already done this well elsewhere:
`PageDrawn`/`PageTally` (`pdf/mod.rs:266-287`) are exactly the "make the invariant structural"
move, applied to a different invariant.
**Fix:** extend the `PageDrawn` pattern to the stage order, and have the tests call the same
entry point the window does.

### `F-HIGHLIGHT-001` — `apply_preview_highlights` interleaves a pure hit→attribute mapping with GTK mutation, in a coverage-excluded file ✅ Done

- **Severity:** Medium · **Found by:** testability-C (T-8)
- **Reference:** `src/preview/build.rs`

The mapping half is pure and would be directly testable; it is inlined with the buffer mutation
and sits in a file the coverage gate cannot see, so it is invisible to both the test suite and
`F-COV-001`'s ratchet.
**Fix:** the same extraction POLICY step 6 prescribes — pull the decision core into a gated
module, which is the mechanism by which the floor rises.

### `F-TOMLDUP-001` — `data/themes.toml` restates the same hex two to four times within one theme, and records that they have already drifted ✅ Done

- **Severity:** Medium · **Found by:** DRY-A (M-4 and M-5)
- **Reference:** `data/themes.toml`; `heading_band_gradient_to_color` restates the theme's own
  `background` in both shipped uses, with nothing linking them

The file itself records an occasion when the repeated values were corrected by hand and drifted.
`F-CONTRAST-001` is the safety consequence.
**Fix:** where a value *is* the theme's `background`, make that derivable rather than restated;
otherwise add the drift guard beside the floor guard (`F-FLOOR-001`).

### `F-TABLEHEAD-001` — `table_head_fg`'s "falls back to the bare `heading_color` and never to a per-level key" is untested, and nothing asserts the shipped themes file states zero unknown keys ✅ Done

- **Severity:** Medium · **Found by:** testability-A (M1 and M4)
- **Reference:** `src/theme/resolve.rs:160` (the fold — verified correct by spec-C),
  `src/export/html.rs:1514`, `src/preview/css.rs:777`

The fold is right and nothing pins it; a change to per-level would pass. Separately, no test
asserts that `data/themes.toml` itself produces **zero** unknown-key warnings — which, with
`F-WARN-001`'s missing log harness, means a retired spelling could ship in the primary
documentation file undetected. (The Low section records that four such spellings are quoted in
its comments today.)
**Fix:** two small tests, ~20 lines together.

---

## 🟢 Low

Every entry below survived the nitpick filter with a score of 2. Low/Tidy findings that scored
0 or 1 are absent from this document by design and are not listed anywhere.

### `F-DIAG-001` — A refusing value parser fires once per heading level and once per bullet tier ✅ Done
- **Severity:** Low · **Found by:** spec-A (F8) · **Reference:** `src/theme/sources.rs:74-81`
  (`Sources::each` calls `pick` per slot, and `pick` re-runs the value function each time);
  the multiplied warns are `src/theme/value.rs:219`, `:137-184`, `:308-326`
- **Spec:** TDD 18.33 — *"**one** `warn` record naming the theme id and the offending key is logged."*

A single authored `heading_underline = "zigzag"` produces **five** identical records — one per
heading level — because every level's chain reaches the same bare spelling and re-parses it. A
hostile `heading_font` re-runs the whole split/validate/re-emit five times; a bad
`list_bullet_glyph` warns three times. Not a correctness defect, but it turns "one bad key costs
that key" into a log the author must de-duplicate by eye, and the number of diagnostics a theme
emits becomes a function of a key's levelling. **Fix:** parse a spelling at most once per key —
memoise per spelling inside `each`, or move refusal to `ThemeSpec::validate`, which sees each
authored spelling exactly once (that also fixes `F-WARN-001`'s anonymity half).

*Done by the Medium batch's `F-WARN-001`, which took the memo route: `Sources::refused`
(`src/theme/sources.rs`) records `(source, spelling)` on first refusal and `walk` skips a
re-parse, so a refusal is reported once whatever the key's levelling. Pinned by
`src/theme/tests/diagnostics.rs`'s "A refusal is reported once, not once per level".*

### `F-COLORDOC-001` — The `#RRGGBB_AA` colour spelling is accepted, used by the schema's own defaults, and undocumented ✅ Done
- **Severity:** Low · **Found by:** spec-A (F11) · **Reference:** `src/theme/value.rs:47-59`
  implements it; the rationale is at `:39-46`; `data/themes.toml` uses it throughout
- **Spec:** SCHEMA.md § Key naming lists only `#RRGGBB`, `#RRGGBBAA` and CSS names — yet SCHEMA's
  own Default column *uses* the underscore form twice (`mark_bg = #fff59d_88`,
  `annotation_hl_color = #FFD133_61`). The document contradicts itself: an author reading the
  type line and writing `#fff59d88` gets a different (still valid, GDK-parsed) result from the
  shipped default the same table quotes.
**Fix:** add `#RRGGBB_AA` to the colour line with the one-clause explanation `value.rs:39-46`
already carries (the tag path takes an RGBA; the Pango cell path needs the alpha decomposed).

### `F-BUILTIN-001` — `BUILTIN_SPRITES` must be edited in lockstep with four other places; only one direction is guarded ✅ Done
- **Severity:** Low · **Found by:** spec-B (#8) · **Reference:** `src/sprite.rs:68-81`; the one
  guard is `src/theme/tests/sprites.rs:56`; the unguarded partners are `data/sprites/`,
  `data/themes.toml`, `install.sh:51`, `packaging/linux/payload.sh:82`,
  `packaging/windows/stage.ps1:153` and the rpm manifest (`F-PKG-001`)
- **Spec:** ScrAP-324's second transferable half — *"Enumerating every key inside a function
  protects against a forgotten key; it says nothing about a forgotten caller of that function."*

`every_built_in_theme_sprite_reference_is_embedded` catches a *reference with no table row*.
Nothing catches a file added to `data/sprites/` and never embedded, an embedded row no theme
names (`sprite.rs:57-58` calls that "dead weight" and accepts it), or an embedded sprite absent
from a packaging script. **Fix:** a test asserting `BUILTIN_SPRITES`'s key set equals the file
set under `data/sprites/`, which also makes `F-PKG-001`'s class detectable in CI.

*Already done by the Medium batch: `every_sprite_on_disk_is_compiled_in_with_the_same_bytes`
(`src/theme/tests/sprites.rs`) landed with `F-PKG-002` and asserts that set equality plus byte
parity. `BUILTIN_SPRITES`'s rustdoc now names both guards and states that an embedded row no
theme names stays deliberately unguarded — it costs binary size and nothing else.*

### `F-STALEREF-001` — Live spec documents still cite the deleted `src/theme.rs` ✅ Done
- **Severity:** Low · **Found by:** links (its only Critical-labelled category), spec-A (F14),
  spec-B (#9), spec-C (F-15), anti-pattern-A (L4) — five reviewers
- **Reference (all verified present):** `sdd/SCHEMA.md:386` (`heading_overline`'s warning cites
  "(`src/theme.rs`, `HeadingRule`)"); `sdd/THEMING.md:5`, `:6`, `:34`, `:106`;
  `sdd/TECH.md:252`; plus eleven line-number citations into the deleted file —
  `sdd/PLAN.preview-decoration.md:206`, `:230`, `:231` (×2), `:232`, `:233`, `:238`, `:284`,
  `:323`, `:664`; `sdd/ISSUES.md:646`, `:728`. Three further citations sit in code comments:
  `src/theme/model.rs:352`, `src/codeview/gutter.rs:727`, `src/codeview/mod.rs:1896`.
  `data/themes.toml:3`, `:52`, `:177` and `scripts/coverage.sh:244`, `:255` carry the same
  dangling path (anti-pattern-A L4).
- **Spec:** SDD principle 5 — *"Keep documents current or delete them."*

`sdd/TECH.md:230-231` *was* updated to `theme/`, so the omission is partial rather than
systematic, which is how it survived review. These matter more than usual because
`HeadingRule`'s rustdoc is the **only** place the GTK 4.6 double-free measurement lives (deferral
D2's justification), and three specs point at a path that no longer exists. **Fix:** retarget to
`src/theme/model.rs` / `src/theme/spec.rs` / `theme/`. The line-number citations need the
function located in the split files first.

*Every one retargeted, plus three the entry did not list (`AGENTS.md`, `src/config.rs`,
`src/session.rs`) — `grep -rn 'theme\.rs'` over the tree now returns nothing outside
`reviews/`. `HeadingRule`'s two spec citations (SCHEMA, THEMING) point at
`src/theme/model.rs`, where the GTK 4.6 double-free measurement lives. Line numbers into
the deleted file were DROPPED rather than re-derived: they addressed a monolith and the
symbol names resolve on their own. `sdd/ISSUES.md` carries no such citation — the entry's
`:646`/`:728` are stale line numbers, verified.*

*One thing the retarget surfaced and did not fix: `PLAN.preview-decoration.md`'s "What each
tier costs" § describes the pre-registry edit path — a per-key `ThemeSpec` field, the `take!`
list, a per-key `Theme::resolve` branch — none of which exist after `F-REG-001`. Retargeting
its paths alone would have left a wrong description behind correct-looking pointers, so the
section carries a supersession note pointing at `theme/keys.rs`. The real answer is plan
retirement; that is a change of its own.*

*Separately, the link reviewer flagged `sdd/PLAN.accessibility.md:68-69` citing
`src/accessible.rs`, which does not exist (`src/a11y.rs` exists but lacks the referenced
functions). That file is **not** on this branch's diff and is recorded here only so it is not
lost.*

### `F-TECHDOC-001` — `TECH.md`'s module map has no entry for `src/widgets/rule.rs` ✅ Done
- **Severity:** Low · **Found by:** spec-B (#10) · **Reference:** `sdd/TECH.md` lines 201-203
  carry `widgets/tab/`, `widgets/table/` and `widgets/table/linkcell.rs`; `src/sprite.rs`, the
  branch's other new top-level module, *was* added at line 231
- **Spec:** SDD skill, "Maintaining SDD documents" — *"TECH.md: Update when architecture changes
  — new modules."*

Not a detail: `sdd/THEMING.md:110` calls this widget "mechanism C's one documented hand-off to
B", and it is the only widget in the app whose *identity* is decided by a theme key.

### `F-GLOB-001` — The sprites glob makes an empty `data/sprites/` a hard install failure, halfway through ✅ Done
- **Severity:** Low · **Found by:** spec-B (#11), anti-pattern-B (#16) · **Reference:**
  `install.sh:51` (with `set -euo pipefail` at `:15`), `packaging/linux/payload.sh:82`
- **Spec:** SCHEMA.md § How a `*_sprite` key resolves, consequence 2 — *"a packaging omission of
  `data/sprites/` costs a log line and nothing else."* The scripts make the absence of that
  directory the **most** expensive failure in the pipeline rather than the cheapest.

With no `nullglob`, an empty or absent directory leaves the literal `data/sprites/*`, `install`
fails, and `set -e` aborts — **after** the binary (`:32`), icon (`:37`) and `themes.toml` (`:43`)
are installed and **before** the desktop entry, the cache refresh and the MIME registration. The
user is left with a working binary no file manager knows about. `stage.ps1:153` does not have
this problem. **Fix:** covered by `F-PKG-002`'s `find … -exec install` form; if the sprite set is
mandatory, validate it up front beside `cargo build`, so the script fails before writing anything.

*Done by the Medium batch's `F-PKG-002`, exactly as anticipated: `install.sh` and
`packaging/linux/payload.sh` now run a character-identical
`find … -type f -exec install -Dm644 -t … {} +`, so an empty or absent `data/sprites/`
installs nothing and aborts nothing. The glob is gone from both.*

### `F-ZOOM-001` — Table-cell decoration metrics reach generated CSS unscaled by zoom ✅ Done
- **Severity:** Low · **Found by:** spec-B (#13) · **Reference:** `src/preview/css.rs:251-259`
  emits `table_cell_padding_v`, `table_cell_padding_h`, `table_border_width` and
  `table_cell_radius` as raw design-time px; `table_cell_radius` is the value added on this branch
- **Spec:** THEMING § Pixel metrics and zoom — *"A theme states decoration geometry as design-time
  px at zoom 1.0 … scaled explicitly on every render/zoom through `theme::px(n, zoom)`."*

A zoomed table keeps hairline borders and design-time padding around grown text. This is a
genuine architectural tension rather than a slip: the theme provider is app-wide while zoom's is
per-window, and the two must write disjoint properties (THEMING § *Zoom, and why the theme owns
SCALE but never SIZE*, GTK4Rs/AP-101), so `theme_css` cannot carry a zoom factor without breaking
that invariant. **The right correction is probably to SCHEMA/THEMING** — stating that CSS-carried
metrics are the documented exception — but as written the two documents disagree with the code.

*Taken as recommended — a documentation correction, not a behaviour change.
`sdd/THEMING.md` § Pixel metrics and zoom now states the exception, names the four keys
and gives the reason (app-wide theme provider vs per-window zoom provider; ScrAP-127),
`sdd/SCHEMA.md`'s "scaled on apply" line points at it, and `preview/css.rs` says the same
at the emission site. Closing it for real needs a per-window theme provider, which no
metric here justifies.*

### `F-RULETHICK-001` — `RULE_THICKNESS_PT` is a literal styling value in the PDF sink ✅ Done (register half REPORTED)
- **Severity:** Low (pre-existing) · **Found by:** spec-C (F-12) · **Reference:**
  `src/export/pdf/mod.rs:83`, drawn at `src/export/pdf/ink.rs:254-260`
- **Spec:** TDD 25.9 — *"a literal styling value anywhere in either sink is a defect."*

The doc comment argues honestly that there is nothing to theme it *from*, because the preview's
separator takes its thickness from GTK's own CSS — and that argument is correct. But the rubric
does not carve it out, and this branch has just added `rule_sprite`, whose height **is** the
tile's own (`measure.rs:344-353`), so the flat rule is now the only rule on the page whose
thickness no theme can state. **Fix:** an `sdd/ISSUES.md` entry naming the two-part fix (a key
plus the preview separator), rather than leaving the argument only in a rustdoc.

*The `sdd/ISSUES.md` half is REPORTED, not written — the registers have one writer and this
seat is not it (POLICY § SDD register writes). The proposed entry is in the batch report.
Done here: `RULE_THICKNESS_PT`'s rustdoc now records that `rule_sprite`'s landing narrowed
the argument — a tiled rule takes the tile's own height, so the flat rule is the only rule
left whose thickness no theme can state.*

### `F-UTF8-001` — `SpriteRef::name()` is silently lossy on a non-UTF-8 path ✅ Done
- **Severity:** Low · **Found by:** DRY-A (L-6), DRY-B (F-15) — two reviewers independently, both
  kept at score 2 · **Reference:** `src/sprite.rs:142` and the extension read at `:150-155`

A non-UTF-8 path yields `""`, losing the extension, the MIME type and every diagnostic — so the
decoration vanishes with **no log**, which is exactly the failure mode `src/sprite.rs:5-9` says
the whole subsystem exists to end. **Fix:** return `Option<&str>` (or fall back to
`to_string_lossy` *and* log), so the loss is visible at the boundary rather than silent.

### `F-HARDCOLOR-001` — Hardcoded colours outside `palette.rs`, contradicting that module's stated contract ✅ Done
- **Severity:** Low · **Found by:** DRY-B (F-13) · **Reference:** `src/codeview/mod.rs`,
  `src/codeview/gutter.rs`; the contract is `src/palette.rs`'s own module doc — *"none of them
  names a colour"*
- **Spec:** POLICY § Architecture rules, *No hard-coded styling*.

Kept despite Low severity because it contradicts an explicit module contract with specific prose
intent. **Fix:** move the literals into `palette.rs` (or `[themes.system]`, which is the register
for values the app would otherwise hardcode).

*Taken via `palette/`, not `[themes.system]`: making these keys STATED would change what
`annotation_chip_decor` resolves rather than only where the number lives, and TDD 18.2's
byte-identical-System guarantee is not worth risking for a Low. Three literals moved —
`ANNOTATION_CHIP_FLOOR`, `ANNOTATION_CHIP_INK_FLOOR`, and the hovered-checkbox accent, which
turned out to be `F-PROBE-001`'s FIFTH probe site: it open-coded the `ACCENT_NAMES` chain and
re-spelled `ACCENT_FLOOR` as a literal, outside `palette/` where that fix could not reach it.
It now calls the new `palette::desktop_accent()`. `gutter.rs` names no colour at all — its
`set_source` takes one.*

### `F-RETIRED-001` — `data/themes.toml`'s comments quote four spellings that `keys.rs` proves are dead ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-A (L5), testability-A (T1) · **Reference:**
  `data/themes.toml:189`, `:201`, `:527`, `:643`

`data/themes.toml` is the primary documentation a theme author reads, and it teaches four
spellings that produce a `warn` and do nothing. `keys.rs:302-328`
(`a_retired_spelling_is_not_a_key`) proves they are dead. **Fix:** four `sed` edits — the only
Low the testability reviewer said they would push on, because it is in the file theme authors read.

### `F-INDEX-001` — Every per-level array index is an unchecked panic site in library code ⛔ Declined
- **Severity:** Low · **Found by:** anti-pattern-A (L9) · **Reference:**
  `src/theme/model.rs:181-183` and the per-level array indexing throughout; the same file
  explicitly warns against this pattern elsewhere

A bare `usize` parameter indexing a fixed-size array is a caller contract enforced by nothing, in
library code, on a path reachable from a GTK `snapshot` vfunc where a panic is a process abort.
**Fix:** take a level newtype, or clamp at the boundary the way `theme::depth_tier`
(`value.rs:360-362`) already does with `saturating_sub` + `min` — which is tested at `usize::MAX`.

⛔ **Declined — the clamp already exists and is the sole producer.** `theme::heading_slot`
(`keys.rs`) does exactly the `saturating_sub` + `min` this asks for, is tested across
`0..=8`, and is the only thing in the tree that produces a heading slot: `renderer/end.rs`,
`renderer/emit.rs`, `outline_view.rs`, `export/html.rs` and `export/pdf/decide.rs` all call
it and nothing computes an index another way. So there is no reachable panic — only an
unenforced *type* contract, and a `HeadingSlot` newtype to enforce it would touch ~30 index
sites across six modules, which is out of proportion to a Low. The contract is now stated
once, at `HeadingRule::is_absent_at`, covering every `[…; HEADING_LEVELS]` field in that
module.

### `F-FONTDOC-001` — `sanitize_font_family`'s comment understates its own allowlist *(security-adjacent)* ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-A (L6) · **Reference:** `src/theme/value.rs:137-184`,
  the character filter at `:146-152`
- *This finding bypassed the nitpick filter as security-adjacent and is recorded here rather than
  in the security section, because the security reviewer independently traced the same function
  and found the **sanitiser sound and unbypassable** (see Cleared surfaces). The finding is the
  comment, not the code.*

The comment describes a narrower allowlist than the code admits (`is_alphanumeric()` is
Unicode-wide). The security reviewer checked that deliberately and confirmed no character that
can end a CSS string or start a comment is alphanumeric, so nothing is wrong — but a reader
reasoning from the comment would reach a different conclusion about what is admitted than the
code implements. **Fix:** state the actual predicate, and record the Unicode reasoning the
security review performed so the next reviewer does not re-derive it.

### `F-INKMATCH-001` — Non-exhaustive match with a catch-all in `marker_ink` ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-B (#10) · **Reference:**
  `src/codeview/gutter.rs:204-212`, the `_ =>` at `:211`

`marker_substitute` (`:158-186`), thirty lines above, matches the same enum **exhaustively** — so
a new `ListMarkerKind` forces a decision there and is silently answered `list_marker_color` here.
Two functions answering "which key does this marker kind read?" with opposite failure modes is
the drift the schema's one-key rule exists to prevent. The enum has three variants today, so the
`_` covers precisely `Ordered` and buys nothing. **Fix:** spell `Ordered` and delete the catch-all.

*Already done by the Medium batch's `F-MARKERKEY-001`: the eight open-coded dispatches collapsed
onto `Theme::marker_ink` (`src/theme/decor.rs`), which matches `MarkerKind` exhaustively. There
is no `_ =>` left in `codeview/gutter.rs` at all.*

### `F-CHIPRECT-001` — The annotation chip's sprite rect is rounded; its flat rect, hit-box and count numeral are not ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-B (#12) · **Reference:** `src/codeview/mod.rs:1044-1067`
- **Spec:** GTK4Rs/**AP-78** — ink and hit-box must be computed from the same geometry.

A mismatch between what is drawn and what is clickable is an interaction-correctness issue, not a
cosmetic one. **Fix:** round once and use the same rect for the sprite, the flat fill, the numeral
and the hit-box.

### `F-DEBUGASSERT-001` — `debug_assert!(false, …)` is reachable from a GTK `snapshot` callback, and makes a documented degradation path untestable ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-B (#14), testability-B (T-12) · **Reference:**
  `src/sprite.rs` (the `debug_assert!(false, …)` on the unresolved-`Named` arm)

Two consequences from one construct. A panic inside a draw vfunc unwinds through C and aborts the
process — in a debug build, which is where tests and developer runs live. And because the arm
panics rather than returning, the *documented* graceful-degradation contract on that path cannot
be asserted by any test: the path is inert in release and unassertable in debug.
**Fix:** replace with `log::error!` + `return None`. The contract then becomes testable, and the
release behaviour is unchanged.

### `F-CAIROPATH-001` — A cairo path is left uncleared when `set_source` fails ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-C (#19) · **Reference:**
  `src/export/pdf/ink.rs:169` (the rectangle) and `:179` (the guarded source)

The heading-band block appends a rectangle to the path *before* the sprite/gradient/flat `match`.
The gradient and flat arms consume it with `fill()`; the sprite arm fills only
`if cr.set_source(&pattern).is_ok()`. When that fails, **neither** rectangle is consumed and the
path survives `cr.restore()` (cairo's save/restore does not save the path), so the next
`show_layout_line` at `:300` fills the stale rectangle in the text colour — a solid block over the
heading. `set_source` fails only when the context is already in an error state, and the module
argues persuasively at `:340-351` that one status check per page is the right discipline for
*reporting* — but that argument does not cover the path being left behind. The rule branch
(`:236-243`) gets this right by building its rectangle inside the `if`.
**Fix:** build the rectangle inside each arm, matching the rule branch. That also removes the
duplicate-rectangle oddity where the sprite arm appends a second, device-space-identical rect
(spec-C F-16, which did not survive the filter on its own).

### `F-FLOATEQ-001` — Float equality in layout code and in a layout test ✅ Done
- **Severity:** Low · **Found by:** anti-pattern-C (#20) · **Reference:**
  `src/export/pdf/ink.rs:401`; `src/export/pdf/measure/tests.rs:855`; `:1171-1178`

`ink.rs:401` writes an equality test against `1.0` as a tolerance (`(row.scale - 1.0).abs() >
f64::EPSILON`). `f64::EPSILON` happens to be the correct ULP *at* 1.0, so it is right today — but
the intent ("don't emit an identity transform") is not what the code says, and a scale that is
`1.0 + 3ε` from a different derivation takes the wrong branch with no visible effect until it
does. `tests.rs:855` uses exact `assert_eq!` on a value produced by `pdftable::fit`'s arithmetic,
and `:1171-1178` has three `assert_eq!` on `f64` of which the third is entailed by the first two.
**Fix:** at `ink.rs:401`, say what is meant (`row.scale != 1.0`) or make `Grid::scale` an
`Option<f64>` where `None` means unscaled. In the tests, use an explicit epsilon and drop the
entailed assertion.

### `F-COLORTEST-001` — `parse_color` is tested for two of the three documented colour spellings ✅ Done
- **Severity:** Low · **Found by:** testability-A (L1) · **Reference:** `src/theme/value.rs:47-59`
- **Spec:** SCHEMA promises `#RRGGBB`, `#RRGGBBAA` and CSS colour names.

Neither `#RRGGBBAA` nor the CSS-name path is asserted, despite both being delegated to GDK — i.e.
the two spellings whose behaviour this crate does not own are the two nothing pins.
(`F-COLORDOC-001` adds a fourth spelling to document and test.) **Fix:** three assertions.

### `F-PAPER-001` — `Palette::for_paper`'s "paper has no dark mode" fall-through is untested ✅ Done
- **Severity:** Low · **Found by:** testability-B (T-11) · **Reference:** `src/palette.rs`
- **Spec:** a SCHEMA-stated rule with zero guard test on the fall-through case.

The PDF sink's entire colour basis rests on this, and `F-PDF-001`'s fix will touch it.
**Fix:** one test asserting a dark theme's paper palette is still light.

### `F-NEWINSTANCE-001` — `run()`'s `--new-instance` decision is pure, inline, and untestable in a coverage-excluded function ✅ Done
- **Severity:** Low · **Found by:** testability-C (T-12) · **Reference:** `src/lib.rs`
- **Spec:** the decision has a documented defect history (**AP-17**).

Pure logic with a recorded past failure, sitting inline in a function the coverage gate cannot
see. **Fix:** extract to `fn wants_new_instance(args: &[String]) -> bool` and test it — trivial,
and the kind of extraction POLICY step 6 says is how the floor rises.

### `F-PIXELHARNESS-001` — `pdf/measure/tests.rs` carries two verbatim pixel-scan blocks and fourteen literal page dimensions ✅ Done
- **Severity:** Low · **Found by:** DRY-C (C-18) · **Reference:**
  `src/export/pdf/measure/tests.rs:540-552` and `:606-618` (two identical 12-line blocks);
  `magenta` spelled three times; page dimensions as literals ×14; fixture setup repeated 11×

Quantified duplication in the very file `F-SPRITEPAINT-001` asks to grow: changing the fixture
silently desynchronises eleven assertions if they diverge. **Fix:** one `fn scan(…)` helper and a
`const PAGE: (f64, f64)` — the file is one step away from having both.

### `F-TUPLE-001` — Positional tuple access at eleven sites, against the project's own convention ✅ Done
- **Severity:** Low · **Found by:** DRY-B (F-11), anti-pattern-A (L1), DRY-C (C-15, C-17),
  DRY-A (T-1, T-2, T-3) — all kept at score 2 except T-3
- **Reference:** `src/palette.rs:213-214` (crosses a tuple boundary), `src/codeview/mod.rs:2390`
  (reaches through an array then positionally), `src/sprite.rs:527`,
  `src/export/pdf/measure/table.rs:128`, `:135`, `:186` (`layout.extents().1`, while siblings in
  the same module destructure by name), `src/theme/model.rs:421`,
  `src/theme/tests/system.rs:87`, `:88`, `:91`, `:93`, `:94`, `:152`, `:153`,
  `src/theme/tests/merge.rs:176`, `src/theme/tests/headings.rs:313`
- **Spec:** the project convention is to destructure by name, never positionally.

The API-boundary instances (`chooser_list`, `Renderer`'s eight bare tuple vectors, the clamp
ranges) are raised separately as `F-CHOOSER-001` and `F-CLAMP-001` because they force the
violation on every caller. This entry is the remaining local sites. **Fix:** destructure by name
at each site; where the tuple crosses a module boundary, name the fields instead.

---

## 🧹 Tidy (Optional)

**These are optional suggestions. They require no verification and no reply.** Take them or
leave them; nothing in this review depends on any of them.

- **`T-001` ✅ — `ThemeColor` is a positional newtype read as `.0` five times.**
  `src/theme/model.rs`. Found by DRY-A (T-1). A named accessor would stop future refactorers of
  `ThemeColor` being misled by inconsistent field access within one module.
- **`T-002` ✅ — `list[0].0` / `list[0].1` in two test modules.** *(Already resolved by `F-CHOOSER-001`'s named struct, as the entry anticipated; verified — the sites now read `list[0].id` / `.label`.)*
  `src/theme/tests/system.rs`, `src/theme/tests/merge.rs`. Found by DRY-A (T-2). Resolved
  automatically by `F-CHOOSER-001`'s named struct if that is taken.
- **`T-003` ✅ — `pad = w + 6` is an unexplained literal, twice.** *(One site left after `F-HTMLDUP-001`; now `SPRITE_MARKER_TEXT_GAP_PX` with its reason.)*
  `src/export/html.rs`. Found by anti-pattern-C (#23). An unexplained magic literal repeated at
  more than one site will drift; a named `const` with a one-line reason costs nothing.
- **`T-004` ✅ — `_ => {}` inside `gtk_suite::parse_args`.** *(Now a rejecting arm: a third `VALUE_FLAGS` entry with no handler is an error rather than a silently discarded instruction.)*
  `src/gtk_suite.rs`, `parse_args` at `:264`. Found by anti-pattern-C (#24). The function's whole
  doc is about arguments being silently discarded, and its catch-all would silently ignore a new
  test flag. Test-infrastructure correctness only.
- **`T-SEC-001` — skipped — the marker glyph is HTML-escaped *and* CSS-escaped into a `content:` string.** *(Not trivial, and the suggested call is not obviously right: `<style>` is raw text, so a glyph reaching it as `as_plain()` could spell `</style>` and close the element — the HTML pass is what removes `<`. Deciding how to neutralise `<` without HTML-escaping is a real design call, and the entry itself says security posture is unaffected either way. Tidy is terminal, so the code is left alone — but `escaped_for_css_string`'s rustdoc now states exactly what the HTML pass buys (`<`, not entity decoding) so the next reader does not re-derive it.)*
  `src/export/html.rs:761`, `:807`. Full entry in the Security section; repeated here so the
  tidy list is complete. Security posture is unaffected either way.

---

## ⏸️ Deferred / by design (accepted)

The developer (`linux-decor`) sent a pre-review declaration of deliberate scope limits and
already-registered items, received over ToasterTalk before any reviewer reported
(`docs/code-review-round-1-deferred-input.md`, tree confirmed clean and final at `7f6b09d`).
**These are accepted. They are not re-litigated below and no reviewer's restatement of them is
carried forward as a finding.** Where a finding proved the limit *wider* than declared, the
excess — and only the excess — is raised as a real defect in the sections above; those three
cases are cross-referenced here.

### Deliberate scope limits

| # | Limit | Status |
|---|---|---|
| **D1** | Annotation-chip sprite reaches preview + HTML; PDF gets colour only — `export/markup.rs` Pango markup carries no inline image | **Accepted.** Stated in `sdd/PLAN.preview-decoration.md` and in SCHEMA.md's `annotation_chip_sprite` row ("a stated scope limit (TDD 18.19)"), and stated *in code* beside the colour-only implementation at `src/export/markup.rs:102-107`. Spec-C independently verified it as **correct** and recorded it as such rather than as a finding. |
| **D2** | No heading-overline **colour** key at any level — GTK 4.6.9 double-free in `gtk_text_attributes_unref`, fixed only in 4.16.13+ | **Accepted.** `clippy.toml` bans `set_overline_rgba`; spec-B verified the guard survives the module split intact (`src/tags.rs:297-299` sets `set_overline` only; the live tag-table walk at `src/preview/build.rs:2156-2196` still holds). Only the *pointers* rot — see `F-STALEREF-001`, which matters here because `HeadingRule`'s rustdoc is the only place the measurement lives. |
| **D3** | Blockquote ink rides a **second**, separately-prioritised `GtkTextTag` from the one carrying its margin | **Accepted for the mechanism.** ⚠️ **Partial exception:** SCHEMA.md's `blockquote_fg` row guarantees three constructs keep their own colour and the code guarantees one. See **`F-BQ-001`** — the priority ladder is correct and not in dispute; the unconditional-vs-`if let` asymmetry at `src/tags.rs:306`/`:370` versus `:449` is. |
| **D4** | Sprite-vs-flat precedence is an explicit branch per renderer — the dev explicitly invited the generalisation finding *"if you can show a seam"* | **Invitation taken.** ⚠️ See **`F-SPRITE-BRANCH-001`** — the seam exists in the repo already (`MarkerSubstitute`, `src/codeview/gutter.rs:140`), and the measured spread is 8 sprite kinds against 36 non-test call sites across 8 files, with `MarkerSubstitute` used in `gutter.rs` only. The per-renderer *branch* SCHEMA mandates is not disputed; what is raised is that the branch is re-derived rather than answered. |
| **D5** | Rule is a bespoke widget, not a `GtkSeparator` subclass — gtk4-rs ships no `SeparatorImpl` | **Accepted, and independently endorsed.** Anti-pattern-B examined it specifically and concluded *"That is a justified widget, not a special case"*; the security reviewer cleared it as sound (`gtk_snapshot_push_repeat`, one GSK node, zero/negative guards, no panic path). The only residue raised is the duplicated *paint* idiom (`F-TILE-001`), which is not about the widget. |

### Declined during mitigation — deferred to their own change

Raised by reviewers, accepted as real, and **not fixed in this campaign**. Both are
structural rather than defective: each was partly landed where the part stood on its own,
and each has a stated reason why the remainder does not belong at the tail end of a branch
this size. Recorded here so they are carried as scheduled work rather than re-raised as
findings in a later round. Deferred by the operator, 2026-08-27.

| # | Finding | What landed | Why the rest is deferred |
|---|---|---|---|
| **D6** | `F-GOD-001` — `snapshot_layer` is a 735-line god function in a GTK draw callback | Two of three parts. The draw gate is now derived from one `DRAWN_VECTORS` table with a per-decoration sweep, and the three duplicated render-and-download blocks fold into `framebuffer_of`. | The decomposition itself. The function is verified **only** by the pixel suite, and the property most at risk under decomposition is the compositing **ORDER** between decorations — a heading band inside a blockquote must land ON the panel — which is precisely what that suite asserts least. Refactoring under a test that cannot see the thing it would break is how the breakage is found later, in pixels, by a user. The `AboveText` half is also not a paint function: it carries the pending-open state machine and wants its own extraction on its own reasoning. **Prerequisite for the deferred work:** per-decoration before/after pixel coverage pinning every overlapping pair, each assertion mutation-checked by swapping the two draws' order and confirming red — an ordering assertion that survives a swapped order is worthless and is the entire risk. |
| **D7** | `F-RENDERER-001` — `Renderer` is a god struct and the event walk has zero unit tests | The `Rc<Theme>` half. `Renderer` holds its own `Rc<Theme>` and the six markup wrappers take `&Theme`, which removes the self-referential-markup-tests half of the finding. | The buffer-free event sink — the seam that would let the pulldown-cmark event walk be unit-tested without a `GtkTextBuffer` — and the tests it exists to enable. Deferred on the finding's own advice. Note the acceptance criterion for whenever it is scheduled: the seam is not the deliverable, the tests behind it are; a seam with nothing testing through it has not closed this finding. |

### Already in the register — cited, not re-opened

| Register ID | Item | Where it surfaced this round |
|---|---|---|
| ANTI-PATTERNS **ScrAP-324** | Compiled-in sprite resolved only on the user-file load path — recorded, **not fixed** | Cited as the *pattern* by `F-BAND-001`, `F-LOG-001`, `F-BARWIDTH-001` and `F-BUILTIN-001`. The entry itself is not re-opened. Note `sdd/scrap-numbers.manifest` was changed on this branch and **no reviewer examined it** (see `coverage-map.md`) — worth confirming ScrAP-324 is allocated exactly once. |
| ISSUES **J** | PDF export ignores heading colour / font / spacing — pre-existing, Low | ⚠️ **Partial exception.** J covers the **bare** `heading_color` / `heading_font` only; those existed at the diff base (orchestrator-verified via `git show 96951f0:src/theme.rs`). It does **not** cover `heading_space_above`, nor **any** per-level `_h1…_h5` heading key — those are new on this branch and born dead in the PDF sink. See **`F-PDF-001`**. |
| ISSUES **K** | CSS-quoted font stack passed to Pango `set_family` — pre-existing, Low | Deferred. The security reviewer separately cleared the *safety* of it (`src/tags.rs:318`: the value is a `CssSafeFontStack` whose character set cannot produce a metacharacter, so stripping quotes is safe) — recorded so the deferral is not confused with an unexamined risk. |
| ISSUES **Q** | Blockquote bar drawn off the page margin — pre-existing, Low | Deferred. Not re-raised. |
| ISSUES **M** | `sprite::scaled` texture cache has no eviction policy — known, Low | Deferred. Security `VERIFY-002` asks a *different* question about the same caches — not "what is the eviction policy" but "is the entry count bounded at all", which turns on whether zoom is a discrete set. That verification is listed in the security section, not here. |
| ISSUES **O** | `px()` exists as three copies — known, Low; the dev names this as "exactly your missing-generalisation bucket" | Deferred. `F-PX-001` records one measurement O does not obviously cover — DRY-B found **four** sites and that **two use different rounding semantics**, which is a behavioural divergence rather than a duplication. Flagged for the developer's own judgement about whether O absorbs it; not re-litigated. |

### Out of scope (per the same declaration)

- `src/widgets/tab/imp.rs:153`, `:163` — `unimplemented!()`. Pre-existing, not decor. No
  reviewer raised it.
- TDD 18.29 `GtkTextTag` margin-vs-ink finding — an open **doc** task queued for routing to
  `gtk4skiller`. Not a branch defect.

---

## ⚖️ Adjudicated disagreements

Two reviewers contradicted each other outright. Both are settled; both are recorded here rather
than hidden, so the next round does not re-derive them.

### 1. `unwrap`/`expect` in non-test code — **both reviewers partly right**

DRY-B stated there is **no** `unwrap()`/`expect()` in non-test code anywhere in its assigned
files (verified by `awk` over the pre-`#[cfg(test)]` region of all eight Rust files), and
anti-pattern-B stated the same for its nine. Anti-pattern-A stated there **is** an unchecked
panic site in library code.

**Adjudication:** every candidate except one is under `#[cfg(test)]`. The exception is
**`src/theme/mod.rs:314`** — `slot.clone().expect("just initialised")` inside
`pub(crate) fn active()`. That is production code, so anti-pattern-A is literally correct — but
it is **provably unreachable**, because `*slot = Some(...)` runs immediately above it under
`if slot.is_none()`. DRY-B's sweep did not cover `src/theme/`, which is why the two reports
disagree without either being wrong.

**Severity: Low.** Removable with no behaviour change:

```rust
pub(crate) fn active() -> std::rc::Rc<Theme> {
    ACTIVE.with(|a| a.borrow_mut()
        .get_or_insert_with(|| std::rc::Rc::new(themes().resolve(SYSTEM_ID)))
        .clone())
}
```

*(This scored 1 in the nitpick filter as DRY-A's standalone L-4 and therefore does not appear in
the Low section. It appears here because the adjudication has to be recorded, not because the
finding was reinstated.)* Anti-pattern-C separately confirmed the same property for the export
paths: *"The only `expect` on a live path is `find.rs:412`, which is correct by construction and
documented; everything else is `#[cfg(test)]`."*

### 2. The coverage-ratchet mechanism — anti-pattern-A's **M9 is superseded**

Anti-pattern-A's **M9** argued that the ratchet was raised by test-only changes because
`src/theme/tests/*.rs` lines land **in the gate's denominator** (and numerator), so adding test
text moves the total up regardless of product coverage — and recommended adding
`theme[/\\]tests[/\\][a-z_]+` to `IGNORE` and re-baselining.

**That premise is backwards, and M9 is SUPERSEDED.** It is recorded rather than deleted because
the recommendation it produced would have *lowered* the floor for the wrong reason.

**Adjudication, in three parts:**

1. **DRY-A actually ran the gate.** `scripts/coverage.sh` at HEAD **passes at 80.85% against
   `FLOOR=80.33`**, and `src/theme/tests/*` is **absent from the summary entirely** — it is not
   in the denominator, so it cannot be inflating the number.
2. **The orchestrator confirmed the mechanism structurally.** `src/theme/mod.rs:54` declares
   `mod tests;`, making `src/theme/tests/*.rs` a sibling module file — structurally identical to
   `src/copymap.rs:1251`. `scripts/coverage.sh:81-87` names exactly this trap in the project's
   own words: *"cargo-llvm-cov stops reporting the file at all."* The theme tests are therefore
   invisible to the gate for the same reason the script already documents, not because of any
   `IGNORE` term.
3. **What survives from M9 is the *other* half, and it is real.** The last ratchet entry claims
   **81.74%** at `51caea6`; HEAD measures **80.85%** — a **~0.9pt drop with no log entry**, while
   the gate stays green on accumulated slack. That is carried forward as **`F-COV-001`**, along
   with testability-A's H1 (the slack exceeds the size of the change it gates) and H2 (the
   `codeview/` exclusion swallows `marker_substitute`).

**Do not apply M9's recommended `IGNORE` addition.** The theme tests are already outside the
measured set; adding the term would change nothing measured and would re-baseline the floor
downward on a false premise.

---

## ✅ Resolved

**Nothing is resolved this round.** This is Round 1 — no findings have been handed to the
developer, no fixes have been made, and no verification pass has been run. This section exists
so that its emptiness is explicit rather than an omission, and so Round 2 has a place to record
what was closed and how it was confirmed.

---

## ✅ Positives

Recorded deliberately. Several of the findings above are literally *"do what this file already
does"*, so naming the templates is part of the remedy — and a review that listed only defects
would misrepresent this branch.

**Design and abstraction**

- **`src/theme/keys.rs` is exemplary, and every reviewer family said so independently.** One
  macro-declared table answers validation, coercion, arity and sprite-ness; `spec.rs:1-16`
  argues the trade honestly and the `Key`-constant discipline buys back the compile-time check
  the map costs. Anti-pattern-A: the split *"cured the god-module rather than redistributing
  it"* — the pre-split file answered four questions from four hand-maintained lists, and
  `keys.rs` answers all four from one. DRY-A: *"the strongest single piece of abstraction on the
  branch."* DRY-C: *"the best abstraction in the branch."* Adding a key is one line and **cannot**
  miss the merge, the sprite walk or the fallback chain. `no_two_keys_claim_the_same_spelling`
  and `every_chain_ends_at_the_bare_key` (`keys.rs:289-300`) guard it.
- **Glyph validation is genuinely ONE function.** `MarkerGlyph::parse` (`src/theme/value.rs:308-326`)
  is the sole constructor: it trims, refuses empty, refuses over `MAX_GLYPH_CHARS = 8` — **refusing
  rather than truncating**, so a grapheme cluster is never split, with the reasoning stated at
  `:299-306` — and refuses control characters. The inner `String` is private with no `Display`
  and no `Deref`; the only ways out are three grammar-named projections, which is the right shape
  because a single escape would be wrong in two of the three destinations. **There are no copies.**
- **`SpriteOrigin` / `SpriteRef` make the ScrAP-324 defect unrepresentable rather than
  remembered.** A theme cannot author a `File` or a `Compiled` (the `Deserialize` impl at
  `src/sprite.rs:163-167` produces `Named` and nothing else), and no `Named` survives
  `Themes::parse`. The security reviewer verified the embedded-table boundary holds in **both**
  directions in code rather than from the prose, and that `ThemeSpec::resolve_sprites` uses
  `retain` — so an unresolvable key is *removed* and `overlay`'s `extend` cannot null out the
  built-in's resolved sprite. That is SCHEMA's "the refused override leaves the compiled-in
  sprite standing", pinned by `an_installed_themes_file_cannot_unship_a_compiled_in_sprite`.
- **The `decide` / `ink` / `measure` extraction made previously-untestable code reachable.**
  `src/export/pdf/decide.rs` is genuinely pure — every function takes an explicit `&Theme` /
  `&Sprites` / `&ListGlyphs`, none touches a `pango::Context` or a `cairo::Context` — and
  testability-C calls it *"a genuine success … the best-tested new code in the diff"*, table-driven
  with several tests carrying explicit mutation notes. That seam did its job and needs no change.
- **`PageDrawn` / `PageTally` (`src/export/pdf/mod.rs:265-287`) and `Layouter::push_line`
  (`measure.rs:184-206`)** are two invariants converted from conventions into constructions, with
  the failure each prevents written down. `PageDrawn`'s `#[must_use]` newtype is genuinely good
  type design, and `finish()` (`pdf/mod.rs:331-346`) is *"a model implementation"* of the
  truncated-export problem, with five tests covering every branch including the zero-page case.
- **Small shared seams done right, and worth copying:** `Typography::bold_attr` / `supsub_attr`
  (`model.rs:92`, `:103`), `HeadingRule::is_absent_at` (`model.rs:181` — *"the one gate every
  consumer asks, so 'absent' is a single decision rather than four"*), `theme::depth_tier`
  (`value.rs:360`), and `RenderProducts`' existence as a concept.
- **`src/widgets/rule.rs` is a textbook thin widget.** It implements both `measure` and
  `snapshot`, declines a width so the view's own bound governs it, guards zero-sized geometry,
  and is argued from a real toolkit constraint. Two reviewers examined it specifically for
  one-off-exception smell and both cleared it.
- **The module split drew real seams.** `keys` = vocabulary → `spec` = parse+validate →
  `sources` = resolution walk → `resolve` = clamp+floor → `model` = the resolved shape. Each
  module's public surface is narrow and there is essentially **no reach-through**: `spec::Value`
  is `pub(super)`, only `resolve` constructs `Sources`, and `resolve` is the sole builder of
  `model::Theme`. `resolve.rs` is pure — no filesystem — and it holds.

**Security**

- **Path containment is correct and well tested.** `src/sprite.rs:282` uses
  `real.starts_with(&real_base)` — `Path::starts_with`, **component-wise, not a string prefix** —
  with both operands canonicalised first, so the classic `/home/u/themes-evil` matching
  `/home/u/themes` trap does not apply and a symlinked base cannot defeat it either. The
  non-`Normal` component filter refuses rather than interprets, and
  `resolve_refuses_parent_traversal` tests it against a target that genuinely exists one level
  up, so the refusal is proved to be about the *component* and not about a missing file.
- **No injection was found anywhere** — not CSS, not HTML, not Pango markup — across
  `preview/css.rs`, `export/html.rs`, `export/markup.rs` and `tags.rs`. Every one of the thirteen
  `Metrics` fields was checked individually against its clamp range. `CssSafeFontStack` and
  `MarkerGlyph` are proof-of-sanitisation newtypes whose sole constructors are validators, so the
  guarantee is enforced by rustc rather than by a doc comment. The `javascript:` vector is closed
  upstream by `links::is_allowed_url`, and `data:` URI construction cannot be steered.
- **Clamping is total and happens before use** — `Value::int` saturates *before* clamping so
  `heading_weight = 99999999999` is a value to clamp rather than a wrap or a panic; `clamp_f64`
  maps non-finite to the floor; `depth_tier` is tested at `usize::MAX`.
- **No `unwrap` / `expect` / `panic!` / `unreachable!` on any production paint or export path**
  across seventeen assigned Rust files, verified by two reviewers independently against the
  pre-`#[cfg(test)]` region of each. For a branch whose main surface is a GTK `snapshot` vfunc,
  that is the single most important property and it holds (see *Adjudicated disagreements* for
  the one provably-unreachable exception).

**Tests and gates**

- **`src/theme/tests/sprites.rs:100`
  (`a_compiled_in_sprite_reaches_every_slot_it_can_be_named_in`) already drives a registry-driven
  conformance sweep**, and `:129-134` asserts the resolved `Sprites` struct and the registry
  agree on slot count — so a new sprite key cannot be added to one and forgotten in the other.
  Testability-A calls it *"the single best test on the branch"*; DRY-C calls it *"the answer to
  C-1; it just needs to be pointed at the sinks as well as the model."* Its sibling at `:56`
  drives off the registry too and carries an explicit **anti-vacuity** assertion (`checked > 0`,
  `:80-83`) so an empty iteration cannot pass silently.
- **`sources.rs:315-354`** walks the whole heading family rather than one key, *and its docstring
  says why*. **`sources.rs:216`** pins the five-row resolution order's subtlest claim in both
  directions — the one rule testability-A *"most expected to find untested"*.
- **`src/codeview/gutter.rs` is the model for theme→pixels code**: `marker_substitute`,
  `marker_ink`, `checkbox_rect`, `first_display_line` and `marker_gap_px` are all pure and take
  `&Metrics` as a parameter *specifically* so they stay display-free (`:37-38` says so), with 17
  unit tests and no display. **`src/preview/css.rs`** is the same: `theme_css` is a pure
  `fn(&Theme, &Palette) -> String` that never touches a `CssProvider`, with 14 tests including a
  CSS-injection negative.
- **Unusually careful test design, repeatedly.** The blockquote-bar test uses a
  **half-transparent** tile specifically so a fill-then-tile-over cannot pass — and its fixture
  comment records that the first, opaque version *passed the mutation it was written to catch*.
  `only_a_banded_heading_level_is_inset_from_its_band` (`preview/build.rs:2210`) asserts
  `is_left_margin_set()` rather than a value, at two zooms, because a tag that sets the margin to
  the view's own number is a *different tag* from one that never set it. Several
  `preview/build.rs` tests assert on resolved geometry from a realized view rather than tag
  properties, with a comment explaining the GTK4Rs/AP-96 lesson.
  `every_heading_rule_spelling_parses_as_pango_markup` sweeps all 16 overline×underline
  combinations through `parse_markup` — the parse being the load-bearing half, per ScrAP-163.
  `pdf/measure/tests.rs` carries real **negative controls** (`:518`, `:596`, `:653` each open by
  asserting the unthemed case draws nothing) and a real positive control at `:275`. **No
  `#[ignore]` anywhere in `src/`**, and no assertion-free tests in the reviewed set.
- **`clippy.toml` was tightened, not loosened** — one ban added (`TextTagExt::set_overline_rgba`)
  with a measured root cause and an upstream commit id, no threshold moved, no lint disabled. And
  there is **not one** `#[allow(...)]`, `#[expect(...)]` or `#[ignore]` anywhere under
  `src/theme/`.
- **The coverage gate's scope was not widened** — the ScrAP-294 failure mode was avoided, and
  neither `src/sprite.rs` (the new security boundary) nor `src/widgets/rule.rs` (the new paint
  widget) is in the `IGNORE` regex, so both are inside the gate.
- **The ratchet log is honest about its own past.** The 2026-08-25 entry states outright *"No
  code moved between files this time — every gain is test-side"*, and the script's header records
  a previous scope narrowing in the project's own words: *"Skipping a move because it is small is
  how a ratchet quietly stops tracking."* `F-COV-001` exists **because** that log makes the drift
  visible — a gate that documented itself less well would have hidden it.

**Documentation**

- This is unusually well-documented code, and anti-pattern-C says the quiet part: most of its
  findings were found *because* the comments are so specific. Several findings in this review are
  a doc comment stating a rule the neighbouring branch does not keep — which is only possible
  because someone wrote the rule down.

---

## Checklist Assessment

| Area | Verdict | Note |
|---|---|---|
| **Functionality** | ⚠️ | The preview is complete and correct. The **PDF sink is not**: eleven documented keys never reach it (`F-PDF-001`), colour alpha is dropped (`F-ALPHA-001`), and three metrics are read as points (`F-METRIC-001`). Two behaviours contradict `sdd/SCHEMA.md` and need an explicit direction chosen — `heading_band_sprite`'s fill precondition (`F-BAND-001`) and `blockquote_fg`'s three-construct guarantee (`F-BQ-001`). One decoration disappears entirely on a decode failure (`F-MARKER-001`). `F-PKG-001` will fail an RPM build. |
| **Code Quality** | ⚠️ | The data layer is excellent and the module split drew real seams. The **consumption layer is where the abstraction stops**: the sprite-outranks-flat rule (`F-SPRITE-BRANCH-001`), the marker-kind dispatch (`F-MARKERKEY-001`), the h6→h5 fold (`F-H6-001`) and the Pango span builders (`F-SPAN-001`) are each one rule with many hand-written copies, and one of those has already diverged in production. Two god functions (`F-GOD-001`, `F-GOD-002`) and one catalogued anti-pattern recurrence (`F-EVENT-001`). |
| **Testing** | ⚠️ | Genuinely strong in places — registry-driven sweeps, real negative controls, mutation-checked fixtures, no `#[ignore]` anywhere. But **seven tests pass without asserting what they name** (`F-TEST-001`), half the sprite paint branches have no test at all (`F-SPRITEPAINT-001`), nine `warn` sites are unobservable (`F-WARN-001`), and the entire XDG search path is untested (`F-XDG-001`). The coverage ratchet did not move for this branch and the measured number fell ~0.9pt unlogged (`F-COV-001`). |
| **Security** | ⚠️ | The design is genuinely strong — no injection of any kind, containment correct and component-wise, sanitisation enforced by the type system. But **four findings, all in one place**: the byte-level admission of an untrusted sprite file. One High (`F-SEC-001`, a reproduced 1.2 GB decompression bomb), two Medium (`F-SEC-002` FIFO hang — POLICY names this exact bug and ships the helper; `F-SEC-003` check-then-use), one Low. Two items need verification on Windows and on zoom discreteness. |
| **Performance** | ⚠️ | No hot-path regression found, and the deliberate hoists in `ink.rs` are correct. Two per-item costs are not: the PDF list walk decodes the marker sprite **per item** and retains each surface (`F-DECODE-001`, which is also `F-SEC-001`'s amplification path), and the HTML task marker embeds a full base64 data URI **per item** — ~340 MB from one 512 KiB PNG on a 500-item list (`F-TASKSPRITE-001`). Both have a sibling in the same tree doing it correctly and saying why. |
| **Documentation** | ⚠️ | SCHEMA.md and THEMING.md were kept genuinely current for the new vocabulary, and `TECH.md`'s module map was partly updated. But sixteen live doc comments describe mechanisms this branch deleted (`F-DEADDOC-001`), five spec files and two register files still cite the deleted `src/theme.rs` (`F-STALEREF-001`), `TECH.md` has no entry for the new `widgets/rule.rs` (`F-TECHDOC-001`), two PDF scope limits exist only in a rustdoc where a theme author will never see them (`F-RADIUS-001`), and `data/themes.toml` teaches four dead key spellings (`F-RETIRED-001`). |

**Overall:** ⚠️ **Changes requested.** Nothing here is Critical and nothing blocks on a
correctness catastrophe. The branch's data layer is the best-designed code in the review and
several of its tests are models. The work to do is concentrated and legible: close the PDF sink's
key gaps, choose a direction on the two SCHEMA divergences, give the seven mis-named tests the
assertions their names promise, fix the FIFO and pixel-cap security gaps, and — the item that
prevents all of this recurring — build `F-SINK-001`'s registry-driven cross-sink sweep, which is
the single change that turns "a completeness obligation on the author" into a gate.

**If only five things are done:** `F-SEC-002` (three lines, closes a hard hang, POLICY already
ships the helper) → `F-SEC-001` (the highest-impact security gap) → `F-PDF-001`'s heading ink and
`mark_fg` (a `<span foreground>` and one `write!`) → `F-TEST-001` (seven assertions, each
mutation-checked) → `F-SINK-001` (the sweep that makes `F-PDF-001`'s class impossible to
reintroduce).
