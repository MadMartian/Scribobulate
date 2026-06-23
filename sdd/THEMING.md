# Theme awareness

How the preview's appearance is sourced, resolved, applied to the screen, and
re-applied when the desktop changes. [`TECH.md`](TECH.md) places this in the
architecture and names the modules that own it (`theme.rs`, `palette.rs`,
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
| [Three application mechanisms](#three-application-mechanisms) | How a key reaches the screen |
| [Zoom, and why the theme owns SCALE but never SIZE](#zoom-and-why-the-theme-owns-scale-but-never-size) | The theme/zoom CSS boundary |
| [Pixel metrics and zoom](#pixel-metrics-and-zoom) | Design-time px, scaled explicitly |
| [Untrusted input](#untrusted-input) | A theme is data from disk |
| [Syntax palette: page luminance vs desktop luminance](#syntax-palette-page-luminance-vs-desktop-luminance) | Which lightness each surface follows |
| [Theme change detection and re-render](#theme-change-detection-and-re-render) | Where the change notification comes from per platform |

---

The preview's appearance is **data**. Everything it renders — colour, typography,
and decoration geometry — is sourced from the active *reading theme*, and the
engine (`theme.rs`) holds no per-theme knowledge: no colour constants, no
`if theme == "sepia"` branches. Themes live in `data/themes.toml`, installed
alongside the app and compiled in (`include_str!`) as the last-resort fallback, so
a missing or malformed themes file can never prevent startup.

The rest of the app — toolbar, tab strip, outline sidebar, editor — stays on the
desktop GTK theme. Theming those is the window manager's job (POLICY scope
decision), so the reading theme is **preview-only**.

## Resolution order

Each key resolves by the first link that supplies it:

1. **The selected theme's key.**
2. **`[themes.system]`'s key** — the register of every styling value the app would
   otherwise hardcode. Reading that one table answers "what does this app hardcode?".
3. **The desktop GTK theme probe + derivation** — `Palette::from_base`'s blend math
   and WCAG contrast walk.

Selecting **System** means links 2 and 3 only, which is byte-identical to the
pre-theming rendering (TDD 18.2). A theme therefore states only what makes it
distinctive; Sepia sets a page, a face, an accent and three highlights, and
*derives* its links, code fills, bar and table chrome from those for free.

## Search path

First match wins; a user file is merged over the built-in **per theme id and per
key**, so a user can override one key of a shipped theme, override what the app
hardcodes (via `[themes.system]`), or add a whole new theme.

| Order | Path |
|---|---|
| 1 | `${XDG_CONFIG_HOME:-~/.config}/scribobulate/themes.toml` (user override) |
| 2 | `${XDG_DATA_HOME:-~/.local/share}/scribobulate/themes.toml` (per-user install) |
| 3 | `$XDG_DATA_DIRS` → `…/scribobulate/themes.toml` (system install) |

⚠️ Row 1 is resolved by `config::user_config_dir()` (an `std::env` snapshot taken
*before* the XCompose workaround redirects `XDG_CONFIG_HOME`), never by
`glib::user_config_dir()` — the GLib helper's process-global cache makes that
redirect and an honest config read mutually exclusive (ScrAP-128; a `clippy.toml`
`disallowed-methods` ban enforces it). Rows 2–3 use the GLib data-dir helpers
freely (the redirect touches `XDG_CONFIG_HOME` only) and iterate
`system_data_dirs()` rather than hard-coding `/usr/share` (KDE's first entry is
`/usr/share/plasma`).

## Three application mechanisms

A theme key reaches the screen by one of three paths, chosen by what the surface
*is* — not by the key. **One key feeds every path it appears on**; the
representations differ, the source does not (POLICY "One theme key, every
application path").

| Mechanism | Covers | Notes |
|---|---|---|
| **A — `GtkTextTag`** | headings, bold/italic/strike, links, inline code, sub/superscript, list indents, the annotation highlight, find-all | Pango attributes, not CSS nodes: **GTK cannot style a `GtkTextTag` with CSS**, which is why themes are TOML and not a stylesheet. |
| **B — self-drawn** | code-block panel, blockquote accent bar, list-marker gutter (bullet / numeral / task checkbox) | `CodePreviewView::snapshot_layer`, from `Palette` + `theme.metrics`. The marker *glyph* takes the optional `list_marker` colour key (one key for all three kinds; unset ⇒ the widget foreground); it colours the glyph only — the item text is buffer content, unaffected. |
| **C — generated CSS** | the page (background/`color`/`font-family`), table cells, the rule separator, the image-selection tint, the preview's floating cards | GTK4 removed the GTK3 widget style overrides, so a widget's background/font is **CSS-only**. CSS is a *generated artifact* here, never a source of truth — `preview/css.rs::theme_css` `format!`s it from the theme, as `zoom_css_rule` already did. |

Mechanism C's page rule must style **both** the widget node (`color`,
`font-family` — also read by GTK's own caret/text paths) and its `> text` child
(`background-color`): styling the widget node's background alone works on GTK's
Default theme but is silently overpainted by an opaque system theme like
Breeze-Dark (ScrAP-126). The `text` fill spans `MAX(screen, layout)`, covering the
non-node margins, so no white frame remains once `> text` is styled.

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
by provider add-order, not selector specificity (ScrAP-127), so keeping the
property sets disjoint is the only reliable defence. A test asserts the theme sheet
never emits `font-size` or the `font:` shorthand (which expands to include size).

## Pixel metrics and zoom

A theme states decoration geometry as **design-time px at zoom 1.0**. Pixel metrics
are widget/Pango properties and do **not** follow the CSS `font-size` rule, so they
are scaled explicitly on every render/zoom through `theme::px(n, zoom) =
(n * zoom).round()`. Theming swapped the *source* of the number; the zoom machinery
is unchanged. A theme never expresses "pixels at the current zoom".

`list_step`/`list_item_gap` resolve once and feed **both** `tags.rs` (the
`li-{depth}` tag's `left_margin`) and `codeview/gutter.rs` (the drawn marker's x); a
themed step reaching one but not the other strands every marker (ScrAP-121), so
themed-geometry tests assert the **resolved** pixel position on a realized view.

## Untrusted input

A theme is data from disk. Geometry is typed `i32` and **clamped**, so it cannot
carry a `}` or `;` into a rule — injection is impossible by construction rather
than by validation. Colours are re-emitted from a parsed `RGBA`, never echoed. The
one free-form string, `font_family`, is sanitised, and is **guaranteed to end in a
generic family**: fontconfig resolves an unknown family to the SANS default, not
serif, so a stack without a generic terminator silently lands on sans and defeats
the theme (`fc-match Charter` → Noto Sans).

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

Something must *emit* those notifications, and on macOS nothing does: the Quartz
backend never reads the system appearance, so `platform/mac/appearance.rs`
supplies the signal. It writes the settings the desktop would have written and
changes nothing else, so this path is identical on both platforms from
`re_render_all_windows()` onward.

Document source text is retained per-window in the `winstate` registry
(`TabState.source`, keyed by tab id). File-monitor callbacks and the
editor buffer keep it in sync, so theme re-renders never re-read disk.

XDG Desktop Portal `SettingChanged` subscription is deferred — `GtkSettings`
covers the common GTK/GNOME case. Libadwaita is intentionally excluded:
`AdwStyleManager` would shorten detection but pulls in a large dependency against
the project's measured minimal footprint.

**Those notifications need a source, and on Windows GTK supplies none.** GTK 4.22
never reads the Windows light/dark preference, so `prefer-dark` would never change
and the app stayed light whatever the user had chosen. `platform::win32::track_system_dark_mode`
supplies the missing source: it polls `AppsUseLightTheme` (0 = dark) on the main
loop, change-gated, and hands the answer to `colorscheme` — the same writer
`platform/mac/appearance.rs` uses — so everything above runs unmodified, and the
detection layer holds no theming knowledge of its own. The one
surface GTK cannot repaint is the DWM-owned title bar, which
`platform::win32::sync_caption_theme` sets from the same `desktop_is_dark()` probe on realize
and on every re-render.

The KDE/X11 live desktop dark↔light toggle (tracked in `sdd/ISSUES.md`) is the same
gap with a different missing source, and its fix belongs in the same shape:
subscribe to the portal signal, write `prefer-dark`, change nothing downstream.
