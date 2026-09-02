//! The app's STATIC CSS: format overlay, outline rows + expander chevron, the
//! tab-strip chrome, and the base (system-themed) card chrome.
//!
//! Everything the PREVIEW draws is generated instead — see [`theme_css`] — because
//! it must follow the active reading theme rather than the desktop (POLICY "No
//! hard-coded styling"). What stays here is what stays on the desktop theme: the
//! outline sidebar, the tab strip, popover chrome, and the EDITOR's annotation card
//! (TDD 18.7 — the reading theme is preview-only).
//!
//! ⚠️ A rule must live in exactly ONE of the two. GTK's cascade scans providers by
//! priority and takes the FIRST value found; equal priority resolves by provider
//! ADD ORDER, and specificity only breaks ties WITHIN one provider (GTK4Rs/AP-101). So two
//! providers writing the same property on the same node is decided by construction
//! order, which no selector can arbitrate. The table/image-tint rules therefore MOVED
//! here→[`theme_css`] rather than being overridden from there.

pub(crate) fn css() -> &'static str {
    "popover.format-overlay > contents { border-radius: 12px; padding: 4px; }
     /* The preview selection ACTION popover (Annotate, …) — same chrome as the editor
        format overlay for a consistent look/behaviour (GTK4Rs/AP-83: non-autohide popover). */
     popover.annotation-overlay > contents { border-radius: 12px; padding: 4px; }
     /* The Annotate comment ENTRY, by contrast, floats as an IN-SURFACE GtkOverlay
        child over the preview (NOT a popover — the typing entry can't safely live in a
        popover surface on X11, GTK4Rs/AP-83), so it gets no theme popover chrome; give it its
        own opaque card so it reads over document text.
        This is the BASE (desktop-themed) card, worn by BOTH the editor's card and the
        preview's. The preview's additionally carries `.scrib-preview-card`, which
        `theme_css` recolours when a reading theme states its own page — so the two
        panes look identical under System and each follows its own pane's theme
        otherwise. The two cards share this class deliberately (window/editor_annotate). */
     .annotation-entry { background-color: @theme_bg_color;
                         border: 1px solid alpha(@theme_fg_color, 0.25);
                         border-radius: 8px; padding: 4px;
                         box-shadow: 0 2px 6px alpha(black, 0.35); }
     /* Base conflict toast — likewise recoloured by `theme_css` under a reading theme. */
     .conflict-toast { border-radius: 10px; padding: 10px 16px;
                       background-color: mix(@theme_bg_color, @theme_selected_bg_color, 0.10);
                       border: 1px solid alpha(@theme_fg_color, 0.20); }
     /* The disclosure toggle on a `<details>` summary line. STRUCTURAL only: this
        strips the button chrome so the control reads as an indicator sitting in prose
        rather than as a button dropped into a paragraph. `.flat` is not enough — a
        GtkToggleButton in its `:checked` state paints a frame regardless, so an
        EXPANDED disclosure rendered boxed while a collapsed one rendered bare. Found
        by driving the running app, not by any test: both states are individually
        correct and only look wrong beside each other.
        The indicator's STATE is carried by the icon (pan-end/pan-down), never by this
        chrome, which is why suppressing it loses nothing.
        The indicator's COLOUR is not here: it is a themed value and lives in
        `theme_css` (`disclosure_marker_color`), because a rule must live in exactly
        one of the two sheets. Unset, the icon keeps the desktop theme's ink, which is
        what it always drew with. */
     .scrib-disclosure { padding: 0; min-width: 0; min-height: 0;
                         background: none; border: none; box-shadow: none; }
     .scrib-disclosure:checked, .scrib-disclosure:hover,
     .scrib-disclosure:active { background: none; border: none; box-shadow: none; }
     listview.outline > row { padding: 1px 4px; }
     .outline-h1 { font-weight: bold; }
     .outline-h2 { font-weight: bold; }
     .outline-h3 { font-weight: normal; }
     .outline-h4 { font-weight: normal; }
     .outline-h5 { font-weight: normal; opacity: 0.85; }
     /* GtkTreeExpander's disclosure chevron is a GtkBuiltinIcon (css-name
        'expander') that paints NOTHING on its own — its glyph is whatever the
        theme's -gtk-icon-source resolves to. A system GTK theme that
        doesn't style this (relatively new) node leaves the chevron blank-but-sized
        (16px min-size still allocates it, so it stays clickable). Restore the
        glyph + a theme-aware tint explicitly so it paints under any theme, in both
        light and dark (verified against GTK 4.6 by the project researcher). */
     listview.outline expander { min-width: 16px; min-height: 16px;
                                 color: @theme_fg_color;
                                 -gtk-icon-source: -gtk-icontheme(\"pan-end-symbolic\"); }
     listview.outline expander:dir(rtl) {
                                 -gtk-icon-source: -gtk-icontheme(\"pan-end-symbolic-rtl\"); }
     listview.outline expander:checked {
                                 -gtk-icon-source: -gtk-icontheme(\"pan-down-symbolic\"); }
     /* widgets/tab: the custom TabBar (widgets/tab, css-name
        `tabbar`) replaced the GtkNotebook-based strip; its own
        GtkDragSource/GtkDropTarget pair (window/tabs/dnd.rs wire_tab_bar_dnd)
        drives both in-strip reorder and cross-window move. GTK still
        auto-toggles :drop(active) for us the whole time an eligible
        (u64-payload) drag hovers over the bar — same zero-signal-wiring
        highlight the GtkNotebook-hosted rule had, just retargeted to the new
        node name. The flat background is a static fallback for when
        gtk-enable-animations is off (the @keyframes rule already honors that
        setting on its own). */
     @keyframes tab-drop-pulse { from { box-shadow: 0 0 4px 1px yellow; }
                                 to   { box-shadow: 0 0 12px 5px yellow; } }
     tabbar:drop(active) { animation: tab-drop-pulse .7s ease-in-out infinite alternate;
                           background-color: alpha(yellow, .25); }
     /* Per-tab handle chrome (widgets/tab §E/N1): reveal the close
        button on hover or while the tab is active, keep it invisible-but-
        allocated otherwise so the label doesn't reflow when it appears. */
     /* Asymmetric horizontal padding (T R B L = 3 4 3 6): the label sits flush
        against the left inner edge whereas the right side already gains visual
        clearance from the flat close-button's own internal padding, so a
        symmetric value looked cramped on the left only. The extra left padding
        balances the label's clearance against the other three sides. */
     /* The strip is a RECESSED rail the tabs sit on, one shade below the window,
        closed by a 1px rule along its bottom edge. `shade()` is MULTIPLICATIVE on
        lightness, so 0.88 < 0.95 < 1.0 stays monotonically 'recessed < dormant <
        lifted' in BOTH light and dark themes. `@theme_base_color` would have been
        the obvious pick for the rail on a dark theme (it IS darker there) but it is
        the ENTRY/VIEW colour, and its relationship to `@theme_bg_color` INVERTS in
        light mode (#ffffff vs #eff0f1) — it would brighten the rail on a light theme. */
     tabbar { background-color: shade(@theme_bg_color, 0.88);
              box-shadow: inset 0 -1px @borders; }
     /* EVERY tab reaches the strip's bottom edge — `imp::size_allocate` hands each
        handle the full height (`Rectangle::new(x, 0, w, height)`) and nothing insets
        it — and CONTINUES the rail's baseline rule with its own 1px bottom border in
        the same colour, landing on the same pixel row. So a dormant tab looks like it
        SITS ON the line rather than stopping short of it. (Insetting the dormant tabs
        by a bottom margin instead also preserves the rule, but reads as an unfinished
        edge — a visible gap between the tab and the strip's bottom.)
        The active tab then turns that border TRANSPARENT rather than removing it: CSS
        backgrounds paint under the border box, so its own fill covers the baseline row
        and the rule breaks under it alone — the classic 'the selected tab is part of
        the page below it' notebook idiom. Transparent rather than `none` keeps the two
        box heights identical, so the label doesn't shift 1px between states. Same
        reason `border-top` is 3px on every tab (transparent when dormant): the accent
        edge on the active tab costs no vertical shift either.
        Only the ACTIVE tab is outlined. The dormant tabs are already separated from
        the rail and from each other by their fill, so a border on them too just busies
        the strip and leaves the outline meaning nothing in particular. */
     tabbar .tab-handle { padding: 3px 4px 3px 6px;
                          margin-top: 4px;
                          border: 1px solid transparent;
                          border-top: 3px solid transparent;
                          border-bottom: 1px solid @borders;
                          border-radius: 6px 6px 0 0;
                          background-color: shade(@theme_bg_color, 0.95); }
     /* Hover in OUR idiom rather than Breeze's: the same hue as the old `.active`
        wash at half strength, so it reads as 'about to become active' without
        competing with the real active tab. `:not(.active)` keeps the active tab from
        brightening under the pointer. */
     tabbar .tab-handle:hover:not(.active) {
                          background-color: alpha(@theme_selected_bg_color, 0.07); }
     tabbar .tab-handle.active { background-color: @theme_bg_color;
                          border-color: @borders;
                          border-top-color: @theme_selected_bg_color;
                          border-bottom-color: transparent; }
     tabbar .tab-handle .tab-close-btn { opacity: 0; }
     tabbar .tab-handle:hover .tab-close-btn,
     tabbar .tab-handle.active .tab-close-btn { opacity: 1; }
     /* Deferred-tab busy spinner: small, so it doesn't crowd the label. */
     tabbar .tab-handle .tab-spinner { min-width: 14px; min-height: 14px; }"
}

// ── generated preview CSS ─────────────────────────────────────────────────────

use crate::palette::{to_hex_opaque, Palette};
use crate::theme::Theme;

/// An `rgba(r,g,b,a)` literal — for the few surfaces that must stay TRANSLUCENT
/// because they overlay content (the image-selection tint sits over the image).
/// Everywhere else the alpha is pre-composited against the page in Rust, so the
/// generated rule carries a concrete colour and the derivation stays unit-testable.
fn rgba_css(c: gtk::gdk::RGBA, alpha: f32) -> String {
    format!(
        "rgba({},{},{},{alpha})",
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8
    )
}

/// The preview's generated stylesheet for `theme` / `palette`.
///
/// GTK4 removed the GTK3 widget style overrides (`gtk_widget_override_font`,
/// `override_background_color`) with no replacement, so a widget's background and
/// font can ONLY be set via CSS. That is a GTK constraint, not a design choice — and
/// it does not compromise the data-first model: **CSS is a generated artifact here,
/// never a source of truth.** TOML is the only place a human writes a colour; this
/// `format!`s a rule from it, exactly as `zoom_css_rule` already does.
///
/// Unscoped (no `.scrib-win-<id>` prefix): the theme is app-wide, and its properties
/// (`color`, `font-family`, `background-color`) are DISJOINT from zoom's
/// (`font-size`), so the two providers never write the same lookup slot and cannot
/// interact (GTK4Rs/AP-101). Only zoom, which is per-window, needs a scope.
///
/// **Every widget shape a link can wear inside a table cell.**
///
/// THREE, not two, and the third was found by looking at a driven render rather than by
/// a test: `color` inherits to a `GtkLinkButton`'s caption label but `text-decoration-*`
/// does not, so a rule on the button node alone leaves a pure-link cell wearing whatever
/// underline the DESKTOP theme drew.
///
/// The list is a `const` and both tests iterate it rather than restating it. Two
/// test-local literal arrays used to mirror this one, which meant they asserted *these
/// three exist* and not *the production list is covered* — a fourth selector added here
/// would have failed neither. The comment beside the loop records that this list grows.
///
/// ⚠️ **`link` here is GTK's OWN CSS class on `GtkLinkButton`, not this project's
/// `link_color` theme key**, and the two are one blanket rename apart. A sweep that
/// respelled the theme vocabulary turned `button.cell.link` into
/// `button.cell.link_color`, which is a well-formed selector that matches nothing: the
/// rules still generated, every test still passed (they assert on the rule TEXT), and a
/// link-only cell silently reverted to the desktop's link colour and underline while the
/// mixed cell beside it stayed themed. A theme key never appears in a selector — the
/// theme's *values* are interpolated into the declarations, and the selector names GTK's
/// node tree. `a_link_only_cell_wears_the_themes_link_colour` is the guard, and it is a
/// driven one for the same reason: only a real widget can say whether a selector matched.
pub(crate) const LINK_CELL_SELECTORS: [&str; 3] = [
    "scribtable .cell link",
    "scribtable button.cell.link",
    "scribtable button.cell.link label",
];

/// Pure — takes its inputs, touches no display. Unit-tested headlessly.
pub(crate) fn theme_css(theme: &Theme, palette: &Palette) -> String {
    let mut out = String::new();
    let m = &theme.metrics;

    // ── the page ──────────────────────────────────────────────────────────────
    //
    // TWO nodes, and BOTH are mandatory (researcher-verified against GTK 4.6.9 +
    // an empirical probe on this desktop):
    //
    //  * `background-color` MUST go on the `> text` node. Two backgrounds paint, in
    //    order: the widget node over the whole border-box (`gtkwidget.c:11563-11577`),
    //    then the `text` node painted ON TOP by TextView (`gtktextview.c:5857-5867`).
    //    Last painter wins. GTK's bundled Default theme sets `textview > text {
    //    background-color: transparent }`, under which the widget node alone WOULD
    //    work — but Breeze-Dark sets it opaque to `@theme_base_color`, painting over
    //    the sepia. Styling `> text` is correct under EVERY theme.
    //  * `color` MUST go on the `textview` (widget) node — `> text` is IGNORED for
    //    body text (`gtktextview.c:7707`→`:7711`/`:7717`; `gtktextlayout.c:4129-4143`).
    //    Bonus: the caret follows `color` from the same node, so it themes for free.
    //
    // Margins are not a CSS node — they are a layout property inside the `text`
    // node's area, and the `text` fill spans MAX(screen, layout), so it covers them:
    // no white frame, once `> text` is styled.
    //
    // NEVER emit the `font:` shorthand: it expands to six longhands (family, style,
    // variant-caps, weight, stretch, SIZE — `gtkcssshorthandpropertyimpl.c:1132`),
    // silently clobbering both the theme's family and zoom's font-size.
    let mut page = String::new();
    if let Some(fg) = theme.foreground {
        page.push_str(&format!("color: {};", to_hex_opaque(fg)));
    }
    if let Some(font) = &theme.font_family {
        // `font_family` is a `CssSafeFontStack`: the type proves it was sanitised and
        // generic-terminated (its sole constructor is `theme::sanitize_font_family`),
        // so this interpolation cannot escape the rule — a raw String cannot reach here.
        page.push_str(&format!("font-family: {};", font.as_str()));
    }
    if !page.is_empty() {
        out.push_str(&format!("textview.scrib-preview {{ {page} }}\n"));
    }
    // The disclosure indicator's ink. It lives HERE rather than in the static sheet
    // because it is a themed value, and a rule must live in exactly one of the two —
    // which is what the static sheet's note said when this was still a known gap.
    // `color` tints a symbolic icon and inks a glyph label alike; a sprite indicator
    // ignores it, which is the sprite-outranks-flat rule doing its job.
    if let Some(ink) = theme.disclosure_marker_color {
        out.push_str(&format!(
            "button.scrib-disclosure {{ color: {}; }}\n",
            to_hex_opaque(ink)
        ));
    }
    if let Some(bg) = theme.background {
        out.push_str(&format!(
            "textview.scrib-preview > text {{ background-color: {}; }}\n",
            to_hex_opaque(bg)
        ));
        // The selection follows the theme too, so a selection on a sepia page is not
        // the desktop's blue.
        //
        // BOTH properties, and the foreground is the half that is easy to miss: with
        // only the fill stated, selected text keeps the DESKTOP theme's
        // `theme_selected_fg_color`, which is the one ink on a themed page the reading
        // theme does not own. Measured under Bedtime before this line existed — every
        // selected glyph, body and heading alike, painted `#000000` at 2.1:1 on the
        // fill. `palette.selection_fg` derives it from colours the theme already owns.
        out.push_str(&format!(
            "textview.scrib-preview > text selection {{ background-color: {}; color: {}; }}\n",
            to_hex_opaque(palette.selection_bg),
            to_hex_opaque(palette.selection_fg)
        ));
        // Table cells are separate `GtkLabel`s outside the buffer (ScrAP-36/ScrAP-110), so
        // the `> text selection` rule above cannot reach them — each cell label draws
        // its own `selection` node, which otherwise falls back to the base GTK theme
        // (the desktop's blue on a sepia page). Style it from the SAME `selection_bg`
        // key so the one theme colour feeds both the body-buffer and table-cell paths.
        // Gated on `theme.background` in lockstep with the body rule above: on a themed
        // page both follow the theme; under System both emit nothing and share GTK's
        // default — never one colour in the body and another in the cells. Scoped to
        // `.cell` so it can't leak onto unrelated labels; the foreground comes from the
        // same derived key as the body's, for the same reason and by the same parity
        // argument — a cell whose selected text stayed the desktop's ink while the
        // paragraph above it took the theme's would be ScrAP-36 all over again.
        out.push_str(&format!(
            "scribtable .cell selection {{ background-color: {}; color: {}; }}\n",
            to_hex_opaque(palette.selection_bg),
            to_hex_opaque(palette.selection_fg)
        ));
    }

    // ── table cells ───────────────────────────────────────────────────────────
    //
    // Cells are `GtkLabel`s outside the buffer, so no GtkTextTag can reach them
    // (ScrAP-36/ScrAP-110) — CSS is their only path. These rules are ALWAYS generated (they
    // moved out of the static sheet entirely, so only one provider writes them). The
    // colours are the alpha()/mix() the static rules used to ask GTK for, computed
    // here instead: `alpha(fg, 0.25)` over the page is EXACTLY `mix(bg, fg, 0.25)`,
    // so System renders byte-identically while the derivation moves into Rust where
    // it is unit-testable (TDD 18.2).
    //
    // These four metrics are emitted at their DESIGN-TIME value and are deliberately
    // NOT put through `theme::px(n, zoom)`, unlike every metric applied via a widget
    // or Pango property. This sheet is the theme's provider, which is APP-WIDE, while
    // zoom's is per-window; the two are collision-free only because their property
    // sets are disjoint (ScrAP-127 — a cross-provider conflict is arbitrated by
    // add-order, not specificity). A zoom factor here would make one app-wide sheet
    // carry a per-window value. Documented as the standing exception in
    // THEMING.md § Pixel metrics and zoom, so the rule and the code agree.
    out.push_str(&format!(
        "scribtable .cell {{ padding: {}px {}px; border-width: {}px; border-style: solid; \
         border-color: {}; border-radius: {}px; }}\n",
        m.table_cell_padding_v,
        m.table_cell_padding_h,
        m.table_border_width,
        to_hex_opaque(palette.table_border),
        m.table_cell_radius,
    ));
    // Table header row (the GFM row above the `---` delimiter): bold text on a faint
    // fill, distinguishing it from body cells. Its TEXT takes `table_head_fg`, which
    // `Theme::resolve` has already folded down to `heading_color` where the theme states
    // no header ink of its own (TDD 18.30) — so a theme that colours its headings and
    // says nothing about tables gets exactly what it got before that key existed, and
    // one that says nothing at all still inherits the body colour. The fold is not
    // repeated here: this reads one resolved value, as both export sinks do.
    let head_fg = theme
        .table_head_fg
        .map(|c| format!(" color: {};", to_hex_opaque(c)))
        .unwrap_or_default();
    // A themed heading FONT reaches the table header too. `heading_font` is a
    // `CssSafeFontStack`, so it is injection-safe here by type, not by convention:
    // only `sanitize_font_family` can have produced the value being interpolated.
    let head_font = theme
        .heading_font
        .as_ref()
        .map(|f| format!(" font-family: {};", f.as_str()))
        .unwrap_or_default();
    // …and the themed BOLD WEIGHT. `bold_weight` was honoured for `**bold**` on all
    // three surfaces and for the table header on none: this rule said `bold`, the HTML
    // sink said nothing, and the PDF sink wrapped the cell in `<b>` — three independent
    // hardcodings of one key that `Typography::bold_attr` already owns (F-BOLD-001).
    out.push_str(&format!(
        "scribtable .cell-head {{ font-weight: {}; background-color: {};{}{} }}\n",
        theme.typography.bold_weight,
        to_hex_opaque(palette.table_head_bg),
        head_fg,
        head_font
    ));

    // A link inside a table cell takes the reading theme's link colour, from the SAME
    // `link_fg` key the body's `link` GtkTextTag uses (`tags.rs`) — one theme key,
    // every application path (Document Rendering CAM row 12). Without these two rules a
    // cell link keeps the *desktop* theme's blue on a sepia or neon page, which is
    // exactly the drift the CAM row exists to prevent.
    //
    // Two selectors because a link renders in two widget shapes (`linkcell`): a mixed
    // cell's `GtkLabel` draws a CSS node literally named `link` for each `<a href>`
    // range (`gtklabel.c:3447`), while a pure-link cell IS a `GtkLinkButton` — css name
    // `button`, css class `link` (`gtklinkbutton.c:223`/`:364`) — whose caption label
    // inherits `color`. Both come last so they win over the `.cell-head` colour a
    // themed heading sets: a link in a header cell is still a link.
    //
    // The underline is stated here rather than inherited from the desktop theme for
    // the same reason: the body's `link` GtkTextTag sets `Underline::Single`
    // explicitly (`tags.rs`), and a host theme that does not underline its `link` node
    // left a mixed cell's link coloured-but-flat beside a pure-link cell the same
    // theme *did* underline (MEASURED on Breeze-dark: the two cell shapes disagreed
    // while the body agreed with neither — GTK4Rs/AP-102's "your art, not the user's" in
    // reverse). Three paths, one appearance, none of them the desktop's opinion.
    //
    // The underline's STYLE and COLOUR are themed too (TDD 18.23), from the same two
    // keys the body `link` tag reads — so a `wavy` or a magenta underline reaches all
    // three link shapes or none of them. `text-decoration-line` states `none` rather
    // than being omitted when the theme turns the line off, because omitting it would
    // hand the decision back to the desktop theme, which is the drift these rules exist
    // to prevent.
    let link_fg = to_hex_opaque(palette.link_fg);
    // Through `cssfrag`, because the HTML sink decides the same thing and the two had
    // each spelled it out — including the `solid` carve-out, which is CSS's own initial
    // value and would otherwise move a rule that has never carried a style (TDD 18.2).
    let link_line = crate::cssfrag::link_underline_line(theme.link_underline);
    // `link_underline_style_decl` returns a WHOLE declaration (leading space, trailing
    // semicolon), so it is spliced after `text-decoration-line`'s semicolon, never into
    // its value. Concatenating the two produced
    // `text-decoration-line: underline text-decoration-style: double;;`, which GTK
    // rejects as "Not a valid value" and then drops the entire declaration: under Pixel
    // Quest (`link_underline = "double"`) every table-cell link lost its line while the
    // body's `GtkTextTag` kept drawing one. The text-level tests below could not see it,
    // because that malformed string still `contains` both fragments they look for; the
    // `parses` module at the bottom of this file is what catches it now.
    let link_line_style = crate::cssfrag::link_underline_style_decl(theme.link_underline);
    let link_line_color = crate::cssfrag::decl(
        "text-decoration-color",
        theme.link_underline_color.map(to_hex_opaque),
    );
    // THREE selectors, not two — see [`LINK_CELL_SELECTORS`]. `color` inherits to a
    // `GtkLinkButton`'s caption label,
    // but `text-decoration-*` does not — GTK registers those unhinherited and builds the
    // Pango attributes from the node that OWNS the text, and a button owns none. So a
    // rule on the button node styles nothing, the caption keeps whatever underline the
    // desktop theme drew on the label, and a pure-link cell reads differently from the
    // mixed cell beside it. MEASURED on a driven render (a wavy green underline in body
    // prose and in a mixed cell, a solid cyan one in the pure-link cell) — and invisible
    // to any test that asserts on the generated rule TEXT rather than on the pixels,
    // which is why this needed a look and not another assertion. The button rule stays:
    // it is what the desktop theme's own `button.link` styling has to lose against.
    for selector in LINK_CELL_SELECTORS {
        out.push_str(&format!(
            "{selector} {{ color: {link_fg}; text-decoration-line: {link_line};\
             {link_line_style}{link_line_color} }}\n"
        ));
    }

    // ── the horizontal rule ───────────────────────────────────────────────────
    //
    // A stock GtkSeparator (`renderer/events.rs` `Event::Rule`) — the third anchored
    // widget class beside tables and images, and the only one we don't build, so
    // without this it would keep the desktop's separator colour on a sepia page.
    out.push_str(&format!(
        "separator.scrib-rule {{ background-color: {}; }}\n",
        to_hex_opaque(palette.rule)
    ));
    // The flat rule's WEIGHT, from the same key the PDF sink strokes with — one theme
    // key, both application paths (POLICY). `min-height` rather than `height`: a
    // GtkSeparator's height is its CSS minimum, and stating only `height` leaves GTK's
    // own 1px minimum to win. A sprite-tiled rule is a different widget entirely
    // (`widgets::rule::SpriteRule`) and takes its tile's height, so this selector
    // cannot reach it and the two cannot disagree.
    out.push_str(&format!(
        "separator.scrib-rule {{ min-height: {}px; }}\n",
        m.rule_thickness.max(0)
    ));
    // ── the image-selection tint ──────────────────────────────────────────────
    //
    // Shown over an anchored image when it falls inside the buffer selection (GTK
    // highlights surrounding text but not the widget). Stays TRANSLUCENT — it
    // overlays the image, so it cannot be pre-composited against the page.
    out.push_str(&format!(
        ".scrib-image-sel {{ background-color: {}; border-radius: 3px; }}\n",
        rgba_css(palette.selection_bg, 0.40)
    ));

    // ── the broken-image placeholder ──────────────────────────────────────────
    //
    // TDD 18.20. Unreachable by mechanism A/B (it is a plain `GtkImage`, no tag and
    // no drawn decoration) and previously unreachable by C too — it carried no CSS
    // class at all, so it sat on desktop colours under every theme (the hole TDD
    // 18.4's "no grey island" claim had). Same treatment `export/html.rs`'s
    // `.missing-image` already gives the export's own broken-image marker — a
    // dashed border in the theme's `rule` colour — so the two renderings agree
    // rather than one being themed and the other not.
    out.push_str(&format!(
        ".scrib-broken-image {{ border: 1px dashed {}; border-radius: 3px; \
         padding: 4px; background-color: {}; }}\n",
        to_hex_opaque(palette.rule),
        rgba_css(palette.code_inline_bg, 0.60)
    ));

    // ── preview-floating cards ────────────────────────────────────────────────
    //
    // Emitted ONLY when the theme states its own page. With no injected page the
    // desktop's own `@theme_*` rules in the static sheet are correct BY DEFINITION —
    // that is resolution link 3 — and re-deriving them here would shift System's
    // cards from the window chrome colour to the page colour, breaking the
    // byte-identical bar (TDD 18.2). This is keyed on the DATA (does the theme state
    // a page?), never on a theme name — the engine holds no per-theme knowledge.
    //
    // These DO override the static sheet's rules, which is why the theme provider is
    // installed at a strictly higher priority: priority dominates, and equal-priority
    // add-order would otherwise decide it (GTK4Rs/AP-101).
    if theme.background.is_some() || theme.foreground.is_some() {
        let card_bg = to_hex_opaque(palette.page_bg);
        let card_border = to_hex_opaque(palette.table_border);
        // Only the PREVIEW's card — the editor's wears `.annotation-entry` alone and
        // stays on the desktop theme (TDD 18.7).
        out.push_str(&format!(
            ".annotation-entry.scrib-preview-card {{ background-color: {card_bg}; \
             border-color: {card_border}; }}\n"
        ));
        out.push_str(&format!(
            ".conflict-toast {{ background-color: {}; border-color: {card_border}; color: {}; }}\n",
            to_hex_opaque(crate::palette::mix_rgba(
                palette.page_bg,
                palette.selection_bg,
                0.10
            )),
            to_hex_opaque(palette.body_fg),
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::css;

    #[test]
    fn css_defines_the_expected_style_classes() {
        let c = css();
        // The table + image-tint rules deliberately MOVED to `theme_css` — a rule must
        // live in exactly one provider (GTK4Rs/AP-101), so their absence here is the contract.
        assert!(!c.contains("scribtable"));
        assert!(!c.contains(".scrib-image-sel"));
        assert!(c.contains(".conflict-toast"));
        assert!(c.contains("popover.format-overlay > contents"));
        assert!(c.contains("popover.annotation-overlay > contents"));
        assert!(c.contains(".annotation-entry"));
        assert!(c.contains("listview.outline"));
        assert!(c.contains(".outline-h1"));
        assert!(c.contains("tabbar:drop(active)"));
        assert!(c.contains("@keyframes tab-drop-pulse"));
        // The tab close-button hover-fade styles the same CSS class the
        // tab-activate guard hit-tests on (QA L-1). Assert the CSS still mentions
        // that hoisted const so a rename on either side is caught here, not by a
        // silently-reintroduced background-tab-close regression.
        assert!(c.contains(crate::widgets::tab::TAB_CLOSE_BTN_CLASS));
    }

    // ── generated theme CSS ───────────────────────────────────────────────────
    //
    // `theme_css` + `Palette::from_base` are both pure, so the whole generator is
    // testable with no display: build a theme, derive a palette, assert the text.

    use super::{theme_css, Palette};
    use crate::palette::to_hex_opaque;
    use crate::theme::{Themes, SYSTEM_ID};

    // Adwaita's real values on a light desktop. `theme_text_color` (body ink) and
    // `theme_fg_color` (chrome ink) are DIFFERENT colours, and using the same one for
    // both is what let the table-chrome drift ship (see `probe_fg` vs `probe_chrome_fg`
    // below, and `Palette::from_base`).
    const PROBE_BG: gtk::gdk::RGBA = gtk::gdk::RGBA::WHITE;
    fn probe_fg() -> gtk::gdk::RGBA {
        gtk::gdk::RGBA::BLACK // theme_text_color
    }
    fn probe_chrome_fg() -> gtk::gdk::RGBA {
        "#2e3436".parse().unwrap() // theme_fg_color
    }
    fn probe_accent() -> gtk::gdk::RGBA {
        gtk::gdk::RGBA::new(0.208, 0.518, 0.894, 1.0)
    }

    fn css_for(id: &str) -> String {
        let theme = Themes::builtin().resolve(id);
        // The bases a light desktop would probe, so the System case reproduces the
        // pre-theming derivation; a named theme injects over them.
        let palette = Palette::from_base(
            theme.background.unwrap_or(PROBE_BG),
            theme.foreground.unwrap_or_else(probe_fg),
            theme.foreground.unwrap_or_else(probe_chrome_fg),
            theme.accent_color.unwrap_or_else(probe_accent),
            &theme,
        );
        theme_css(&theme, &palette)
    }

    /// TDD 18.2 — the regression bar. System states no page of its own, so it must
    /// emit no page rule at all and leave the view on the desktop's colours/font.
    #[test]
    fn system_emits_no_page_rule() {
        let c = css_for(SYSTEM_ID);
        assert!(!c.contains("textview.scrib-preview"));
        // …and no floating-card override either: with no injected page, the static
        // sheet's @theme_* cards are correct by definition (resolution link 3).
        assert!(!c.contains(".scrib-preview-card"));
    }

    /// TDD 18.4 — a reading theme must reach the page itself, and it takes BOTH
    /// nodes to do it (researcher-verified against GTK 4.6.9 + an empirical probe).
    #[test]
    fn a_reading_theme_styles_both_page_nodes() {
        let c = css_for("sepia");
        // `background-color` on the `> text` node: the widget node paints first and
        // TextView paints `text` OVER it, so a widget-node-only background is
        // overpainted by any theme that sets `text` opaque (Breeze-Dark does).
        assert!(c.contains("textview.scrib-preview > text { background-color: #f4ecd8; }"));
        // `color` on the WIDGET node: `> text` is ignored for body text (two
        // independent GTK paths both read the widget node), and the caret follows it.
        let page = c
            .lines()
            .find(|l| l.starts_with("textview.scrib-preview {"))
            .expect("a reading theme must emit a widget-node rule");
        assert!(page.contains("color: #5b4636;"));
        assert!(page.contains("font-family:"));
    }

    /// ScrAP-36 parity — the in-cell selection highlight and the body selection
    /// highlight are one theme colour feeding two render paths. Cells are GtkLabels
    /// outside the buffer, so they carry their OWN `selection` rule, but it must draw
    /// from the same `selection_bg` key and gate identically to the body's: on a themed
    /// page both are emitted and equal; under System neither is, so both fall back to
    /// GTK's default together rather than one tinting while the other doesn't.
    ///
    /// **Both properties, not just the fill.** A rule stating only `background-color`
    /// leaves selected text on the DESKTOP theme's `theme_selected_fg_color`; measured
    /// under Bedtime, that painted every selected glyph `#000000` on the themed fill.
    /// So the foreground is asserted here beside the background, and for the same
    /// parity reason: the two paths must agree on it too.
    /// TDD 18.23 — a themed link underline reaches BOTH cell link shapes, and under a
    /// theme that states neither key the rules are byte-identical to what they always
    /// were (a solid underline, no stated colour).
    ///
    /// Both shapes are asserted because they are two different widgets — a mixed cell's
    /// `GtkLabel` `link` node and a pure-link cell's `GtkLinkButton` — and a host theme
    /// that decorates one and not the other is exactly what made two links in one table
    /// look different before these rules existed.
    #[test]
    fn a_themed_link_underline_reaches_both_cell_link_shapes() {
        // Iterated off the PRODUCTION list, never a copy of it: a mirrored array
        // asserts "these three exist", which a fourth selector added to production
        // would satisfy while going untested. ⚠️ Either way this assertion is over the
        // generated rule TEXT — it CANNOT see whether the rule matched anything on
        // screen, and the missing third selector was found by looking at a driven
        // render, not here.
        let sys = css_for(crate::theme::SYSTEM_ID);
        for prefix in super::LINK_CELL_SELECTORS {
            let rule = sys
                .lines()
                .find(|l| l.starts_with(prefix))
                .unwrap_or_else(|| panic!("no `{prefix}` rule:\n{sys}"));
            assert!(rule.contains("text-decoration-line: underline;"), "{rule}");
            assert!(!rule.contains("text-decoration-style"), "{rule}");
            assert!(!rule.contains("text-decoration-color"), "{rule}");
        }

        let mut themes = Themes::builtin();
        themes.merge_over_for_test(
            "[themes.sepia]\nlink_underline = \"double\"\nlink_underline_color = \"#00ff00\"\n",
        );
        let theme = themes.resolve("sepia");
        let palette = Palette::from_base(
            theme.background.unwrap(),
            theme.foreground.unwrap(),
            theme.foreground.unwrap(),
            theme.accent_color.unwrap(),
            &theme,
        );
        let themed = theme_css(&theme, &palette);
        for prefix in super::LINK_CELL_SELECTORS {
            let rule = themed
                .lines()
                .find(|l| l.starts_with(prefix))
                .unwrap_or_else(|| panic!("no `{prefix}` rule:\n{themed}"));
            assert!(rule.contains("text-decoration-style: double;"), "{rule}");
            assert!(rule.contains("text-decoration-color: #00ff00;"), "{rule}");
        }
    }

    #[test]
    fn cell_selection_matches_the_body_selection_and_gates_with_it() {
        let prop_of = |line: &str, prop: &str| {
            line.split(&format!("{prop}:"))
                .nth(1)
                .unwrap_or_else(|| panic!("no `{prop}` in rule: {line}"))
                .split(';')
                .next()
                .unwrap()
                .trim()
                .to_string()
        };
        let bg_of = |line: &str| prop_of(line, "background-color");
        let fg_of = |line: &str| prop_of(line, " color");

        // Themed page: both selection rules present, and the SAME colour.
        let c = css_for("sepia");
        let body = c
            .lines()
            .find(|l| l.starts_with("textview.scrib-preview > text selection"))
            .expect("a themed page must style the body selection");
        let cell = c
            .lines()
            .find(|l| l.starts_with("scribtable .cell selection"))
            .expect("a themed page must style the cell selection too");
        assert_eq!(
            bg_of(body),
            bg_of(cell),
            "cell and body selection colours diverged:\n{body}\n{cell}"
        );
        assert_eq!(
            fg_of(body),
            fg_of(cell),
            "cell and body SELECTED-TEXT colours diverged:\n{body}\n{cell}"
        );
        assert_ne!(
            fg_of(body),
            bg_of(body),
            "selected text must not be drawn in the selection fill's own colour:\n{body}"
        );

        // System states no page, so the whole selection block is skipped — the cell
        // rule must NOT appear, or cells would tint while the body keeps GTK's default.
        let s = css_for(SYSTEM_ID);
        assert!(
            !s.contains("scribtable .cell selection"),
            "System must not emit a cell-selection rule (the body doesn't either):\n{s}"
        );
    }

    /// Document Rendering CAM row 12 — a link is one theme key feeding three render
    /// paths, and the two that CSS owns must agree with the one a `GtkTextTag` owns.
    ///
    /// A link renders in the body as buffer text carrying the `link` tag (coloured from
    /// `palette.link_fg` in `tags.rs`), in a mixed table cell as a `GtkLabel`'s `link`
    /// CSS node, and in a pure-link cell as a `GtkLinkButton`. Cells are outside the
    /// buffer, so no tag reaches them (ScrAP-36) — without these rules a cell link keeps
    /// the *desktop* theme's blue on a sepia or neon page while the identical link one
    /// line above is the theme's colour. Emitted for every theme, System included,
    /// because the body's link tag is too (unlike the selection above, which gates).
    ///
    /// The underline is asserted beside the colour because it is the same claim: the
    /// body tag sets `Underline::Single`, and leaving the cells to the desktop theme
    /// made the two cell shapes disagree with each other on a real dark theme.
    #[test]
    fn cell_link_colour_follows_the_same_theme_key_as_a_body_link() {
        let colour_of = |line: &str| {
            line.split(" color:")
                .nth(1)
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .trim()
                .to_string()
        };
        for id in ["sepia", SYSTEM_ID] {
            let theme = Themes::builtin().resolve(id);
            let palette = Palette::from_base(
                theme.background.unwrap_or(PROBE_BG),
                theme.foreground.unwrap_or_else(probe_fg),
                theme.foreground.unwrap_or_else(probe_chrome_fg),
                theme.accent_color.unwrap_or_else(probe_accent),
                &theme,
            );
            let c = theme_css(&theme, &palette);
            // The one key the body's `link` GtkTextTag is set from.
            let expected = crate::preview::css::to_hex_opaque(palette.link_fg);
            // The production list, not a copy — see `LINK_CELL_SELECTORS`.
            for prefix in crate::preview::css::LINK_CELL_SELECTORS {
                let rule = c
                    .lines()
                    .find(|l| l.starts_with(prefix))
                    .unwrap_or_else(|| {
                        panic!(
                            "{id}: no `{prefix}` rule — a cell link \
                         falls back to the desktop theme's link colour on a themed page"
                        )
                    });
                assert_eq!(
                    colour_of(rule),
                    expected,
                    "{id}: `{prefix}` diverged from the body link colour"
                );
                assert!(
                    rule.contains("text-decoration-line: underline"),
                    "{id}: `{prefix}` must state its underline — the body link tag does, \
                     and a host theme that underlines one cell shape but not the other \
                     is what made two links in one table look different"
                );
            }
        }
    }

    /// TDD 18.9 / GTK4Rs/AP-101 — the load-bearing invariant of the whole design. Zoom owns
    /// CSS `font-size` EXCLUSIVELY; the theme owns Pango scale. If the theme sheet
    /// ever emits `font-size`, the two providers write the same lookup slot and the
    /// winner is decided by provider ADD ORDER — a construction-order accident that
    /// no selector can arbitrate. This test is what keeps the collision impossible
    /// rather than merely managed.
    #[test]
    fn the_theme_sheet_never_writes_a_property_zoom_owns() {
        for id in ["system", "sepia"] {
            let c = css_for(id);
            assert!(
                !c.contains("font-size"),
                "theme {id} emitted font-size — that slot belongs to the zoom provider (GTK4Rs/AP-101)"
            );
            // The `font:` SHORTHAND is the same trap by another route: GTK expands it
            // to six longhands INCLUDING size, silently clobbering zoom.
            assert!(
                !c.contains("font:"),
                "theme {id} emitted the `font:` shorthand, which expands to font-size"
            );
        }
    }

    /// The theme sheet is app-wide, so it must NOT carry zoom's per-window scope —
    /// and it doesn't need it, because the two providers' properties are disjoint.
    #[test]
    fn the_theme_sheet_is_unscoped() {
        assert!(!css_for("sepia").contains(".scrib-win-"));
    }

    /// TDD 18.4 — the surfaces the audit found were system-bound. Every one of them
    /// must be reachable, including the ones that are neither a tag nor our own
    /// drawing: table cells (GtkLabels outside the buffer, ScrAP-36) and the horizontal
    /// rule (a stock GtkSeparator).
    #[test]
    fn every_generated_surface_is_present_and_themed() {
        let c = css_for("sepia");
        for want in [
            "scribtable .cell",
            "scribtable .cell-head",
            "separator.scrib-rule",
            ".scrib-image-sel",
            ".scrib-broken-image",
            ".annotation-entry.scrib-preview-card",
            ".conflict-toast",
        ] {
            assert!(c.contains(want), "sepia sheet is missing {want}:\n{c}");
        }
        // None of them may still be asking the DESKTOP for a colour.
        assert!(
            !c.contains("@theme_"),
            "a generated rule still binds to @theme_*"
        );
    }

    /// TDD 18.30 / 18.2 — the table header's ink is its own key, and falls back to
    /// `heading_color` exactly as it always did.
    ///
    /// Three cases, because two of them look identical from inside one theme: a theme
    /// that states neither key (no `color` at all), one that states only
    /// `heading_color` (the pre-18.30 behaviour, which is what "byte-identical" means
    /// here), and one that states both (the header leaves the heading behind).
    #[test]
    fn the_table_header_ink_is_its_own_key_and_falls_back_to_heading_color() {
        let sheet = |fragment: &str| {
            let mut themes = Themes::builtin();
            themes.merge_over_for_test(fragment);
            let theme = themes.resolve("inked");
            let palette = Palette::from_base(
                theme.background.unwrap_or(PROBE_BG),
                theme.foreground.unwrap_or_else(probe_fg),
                theme.foreground.unwrap_or_else(probe_chrome_fg),
                theme.accent_color.unwrap_or_else(probe_accent),
                &theme,
            );
            theme_css(&theme, &palette)
        };
        let head_rule = |css: &str| {
            css.lines()
                .find(|l| l.starts_with("scribtable .cell-head"))
                .expect("the header rule is generated for every theme")
                .to_string()
        };

        assert!(
            !head_rule(&sheet("[themes.inked]\n")).contains(" color:"),
            "a theme stating neither key must leave the header on the body ink"
        );
        assert!(
            head_rule(&sheet("[themes.inked]\nheading_color = \"#5f1010\"\n"))
                .contains(" color: #5f1010;"),
            "stating only heading_color must still reach the header, as it always did"
        );
        let both = head_rule(&sheet(
            "[themes.inked]\nheading_color = \"#5f1010\"\ntable_head_fg = \"#ffd400\"\n",
        ));
        assert!(both.contains(" color: #ffd400;"), "{both}");
        assert!(
            !both.contains("#5f1010"),
            "the header must leave heading_color behind entirely, not layer over it: {both}"
        );
    }

    /// The table rules are generated for EVERY theme (they moved out of the static
    /// sheet entirely — one provider per rule, GTK4Rs/AP-101), including System.
    #[test]
    fn table_and_rule_chrome_is_generated_for_every_theme() {
        for id in ["system", "sepia"] {
            let c = css_for(id);
            assert!(c.contains("scribtable .cell"), "{id} lost its cell rule");
            assert!(
                c.contains("separator.scrib-rule"),
                "{id} lost its rule colour"
            );
            assert!(c.contains(".scrib-image-sel"), "{id} lost its image tint");
            assert!(
                c.contains(".scrib-broken-image"),
                "{id} lost its broken-image styling"
            );
        }
    }

    /// TDD 18.2 — the cell chrome's colours moved from GTK's `alpha()`/`mix()` into
    /// Rust. `alpha(c, t)` composited over the page is exactly `mix(page, c, t)`, so
    /// System must still paint the same greys it always did.
    ///
    /// `#eeefef` is a **byte measured off the pre-theming build's actual pixels**
    /// (a screenshot diff on a light desktop), NOT re-derived here from the same
    /// inputs the code under test uses. That distinction is the entire value of this
    /// test: its first version computed `mix(bg, fg, 0.08)` with the same `fg` the
    /// implementation had, so it asserted the code against itself and passed happily
    /// while the real fill drifted `#EEEFEF` → `#EBEBEB`. A manual screenshot diff
    /// caught that, not this test — a test that re-derives its own expectation cannot
    /// detect a wrong INPUT (ScrAP-132's family).
    ///
    /// The header fill is asserted because it is a large flat region that screenshots
    /// reliably. The 1px border is deliberately NOT pinned to a sampled byte — it
    /// antialiases against the page, so sampling it is noise; the sibling test below
    /// covers it by construction instead.
    #[test]
    fn system_cell_chrome_matches_the_pre_theming_pixels() {
        let c = css_for(SYSTEM_ID);
        assert!(
            c.contains("background-color: #eeefef;"),
            "table header fill drifted from the pre-theming pixels:\n{c}"
        );
    }

    /// The chrome ink and the body ink are different colours on a real desktop, and
    /// the table chrome must follow the CHROME one — that is the source the CSS it
    /// replaced named (`@theme_fg_color`). Guards the collapse from recurring.
    #[test]
    fn the_table_chrome_follows_the_chrome_ink_not_the_body_ink() {
        let theme = Themes::builtin().resolve(SYSTEM_ID);
        let from_chrome = Palette::from_base(
            PROBE_BG,
            probe_fg(),
            probe_chrome_fg(),
            probe_accent(),
            &theme,
        );
        // Feeding the BODY ink to both (the regression) must produce a DIFFERENT
        // result — otherwise this test, and the one above, prove nothing.
        let collapsed =
            Palette::from_base(PROBE_BG, probe_fg(), probe_fg(), probe_accent(), &theme);
        assert_ne!(
            to_hex_opaque(from_chrome.table_head_bg),
            to_hex_opaque(collapsed.table_head_bg),
            "the two inks must be distinguishable, or this guard is vacuous"
        );
        assert_eq!(to_hex_opaque(from_chrome.table_head_bg), "#eeefef");
        assert_eq!(to_hex_opaque(collapsed.table_head_bg), "#ebebeb"); // the shipped drift
    }

    /// A named theme states ONE foreground, so both inks collapse onto it — the
    /// distinction is a System-theme fidelity concern, not a burden on theme authors.
    #[test]
    fn a_named_theme_uses_its_single_foreground_for_both_inks() {
        let theme = Themes::builtin().resolve("sepia");
        let p = Palette::from_base(
            theme.background.unwrap(),
            theme.foreground.unwrap(),
            theme.foreground.unwrap(),
            gtk::gdk::RGBA::BLACK,
            &theme,
        );
        // Sepia states no table_border, so it derives from its page + its own ink.
        assert_eq!(
            to_hex_opaque(p.table_border),
            to_hex_opaque(crate::palette::mix_rgba(
                theme.background.unwrap(),
                theme.foreground.unwrap(),
                0.25
            ))
        );
    }

    /// The image tint sits OVER the image, so it is the one surface whose alpha
    /// cannot be pre-composited against the page.
    #[test]
    fn the_image_tint_stays_translucent() {
        let c = css_for("sepia");
        let line = c
            .lines()
            .find(|l| l.starts_with(".scrib-image-sel"))
            .unwrap();
        assert!(
            line.contains("rgba("),
            "the image tint must keep its alpha: {line}"
        );
        assert!(line.contains(",0.4)"), "{line}");
    }

    /// TDD 18.11 — a theme is data from disk, i.e. untrusted input. Nothing it
    /// carries may escape a rule. Colours can't (they are re-emitted from a parsed
    /// RGBA), geometry can't (it is an i32), and the font stack is sanitised.
    #[test]
    fn a_hostile_theme_cannot_inject_css() {
        let mut themes = Themes::builtin();
        themes.merge_over_for_test(
            "[themes.evil]\nbackground = \"#f4ecd8\"\nforeground = \"#000000\"\n\
             font_family = 'Georgia; } * { background-color: red; }'\nlist_step = 99999\n",
        );
        let theme = themes.resolve("evil");
        let palette = Palette::from_base(
            theme.background.unwrap(),
            theme.foreground.unwrap(),
            theme.foreground.unwrap(),
            gtk::gdk::RGBA::BLACK,
            &theme,
        );
        let c = theme_css(&theme, &palette);
        // The generated sheet must have exactly as many `{` as `}` and no stray
        // selector: a successful injection shows up as an extra brace pair.
        assert_eq!(
            c.matches('{').count(),
            c.matches('}').count(),
            "unbalanced braces — a value escaped its rule:\n{c}"
        );
        assert!(
            !c.contains("background-color: red"),
            "injection succeeded:\n{c}"
        );
    }

    /// Does GTK itself accept the sheet this module generates? Every assertion above
    /// reads the generated TEXT, and text-matching cannot see a malformed declaration:
    /// `text-decoration-line: underline text-decoration-style: double;` satisfies
    /// `contains("text-decoration-line: underline")` and
    /// `contains("text-decoration-style: double;")` both, and GTK drops it on the floor.
    /// That is not hypothetical — it is how Pixel Quest's `link_underline = "double"`
    /// shipped with a flat link in every table cell while the body underlined correctly.
    ///
    /// Loading a `GtkCssProvider` needs GTK initialised, so this lives behind
    /// `gtk-integration-tests` while its neighbours run under plain `cargo test`.
    #[cfg(feature = "gtk-integration-tests")]
    mod parses {
        use super::css_for;
        use crate::theme::Themes;
        use std::cell::RefCell;
        use std::rc::Rc;

        /// **Every builtin theme's sheet parses clean**, not just the two the text-level
        /// tests sample. A theme is the unit of variation here: the keys that change the
        /// sheet's SHAPE (an underline style, a stated colour) are exactly the ones only
        /// some themes state, so a sample of two leaves the interesting sheets unparsed.
        #[gtktest::test]
        fn every_builtin_theme_sheet_parses_without_a_css_error() {
            for entry in Themes::builtin().chooser_list() {
                let sheet = css_for(&entry.id);
                let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
                let provider = gtk::CssProvider::new();
                let sink = Rc::clone(&errors);
                provider.connect_parsing_error(move |_, section, error| {
                    sink.borrow_mut()
                        .push(format!("{section}: {}", error.message()));
                });
                provider.load_from_data(&sheet);
                assert!(
                    errors.borrow().is_empty(),
                    "theme `{}` generates CSS GTK refuses, so the declarations in the \
                     offending rule are silently dropped and the theme's intent never \
                     reaches the screen: {:?}\n{sheet}",
                    entry.id,
                    errors.borrow()
                );
            }
        }
    }
}
