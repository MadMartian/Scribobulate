//! The Format command row shared by the toolbar section and the caret overlay, so
//! the two surfaces can never drift (Bold … HR, then Link/Image/Table, with the
//! H1–H6 heading `GtkMenuButton`).

use super::super::*;

/// Build one Format-toolbar button.  Tries the freedesktop symbolic icon; if the
/// theme lacks it, falls back to the short `glyph` label so the bar never shows a
/// broken-image placeholder.  `set_focus_on_click(false)` is the precise tool
/// (NOT `focusable(false)`, which would break Tab nav / a11y): the click never
/// grabs focus, so the GtkSourceView keeps keyboard focus and the focus-gated
/// win.format action stays enabled at activation time.
fn format_button(cmd: &crate::app::FmtCmd) -> gtk::Button {
    // `cmd.icon` is `None` for commands with no freedesktop icon (Code Span, Quote,
    // Code Block, Task List, Horizontal Bar) — fall back to the short glyph. When
    // present, still verify the theme actually resolves it before using it, so a
    // missing name never renders the broken-image placeholder (GTK4Rs/AP-48).
    let icon_name = cmd.icon.map(crate::icons::Icon::name).filter(|name| {
        gtk::gdk::Display::default()
            .map(|d| gtk::IconTheme::for_display(&d).has_icon(name))
            .unwrap_or(false)
    });
    let btn = match icon_name {
        Some(name) => gtk::Button::from_icon_name(name),
        None => gtk::Button::with_label(cmd.glyph),
    };
    crate::a11y::name_with_accel(&btn, cmd.label, cmd.accel);
    btn.add_css_class("flat");
    btn.set_focus_on_click(false);
    btn.set_action_name(Some("win.format"));
    btn.set_action_target_value(Some(&cmd.target.to_variant()));
    btn
}

/// Build the Format commands as a flat, ordered list of individually-packable
/// widgets — Bold, Italic, Heading (MenuButton), Strikethrough, Code Span,
/// Superscript, Subscript, Code Block, Quote, Bulleted List, Numbered List,
/// Task List, Horizontal Bar, Link/Image/Table, then Annotate — in the same
/// order as the menu. Shared by the toolbar section and the Stage-2 caret
/// overlay so the two surfaces can never drift. Returns the items, the heading
/// `MenuButton` (the caller binds its sensitivity to the action) and the
/// Link/Image buttons whose tooltip flips to "Edit …".
///
/// **A list rather than a container, because the two consumers need different
/// packing.** The caret overlay wants one horizontal row and takes it from
/// [`build_format_bar`] below. The toolbar hands each item to its wrap box
/// separately, so a narrow window wraps Format per button like every other
/// section instead of as one ~555px block — which was the single largest
/// contributor to the window's minimum width, and so to what a narrow display
/// could show at all.
///
/// The heading picker is a GtkMenuButton, NOT a GtkDropDown: GtkDropDown's empty/
/// no-selection caption is a hardcoded translatable "(None)" GtkLabel baked into
/// its private button_stack template — no factory, property, or CSS overrides it
/// (see sdd/ANTI-PATTERNS.md).  A MenuButton's label is fully ours ("(Hn)") and
/// its menu items drive win.format::h{1..6} directly, so there is no selection
/// state to manage.
pub(crate) fn format_items() -> (
    Vec<gtk::Widget>,
    gtk::MenuButton,
    Vec<(FmtInsertKind, gtk::Button)>,
) {
    let mut items: Vec<gtk::Widget> = Vec::new();
    items.push(format_button(&FORMAT_CMDS[0]).upcast()); // Bold
    items.push(format_button(&FORMAT_CMDS[1]).upcast()); // Italic

    let heading_menu = gtk::gio::Menu::new();
    for n in 1..=6u8 {
        heading_menu.append(Some(&format!("H{n}")), Some(&format!("win.format::h{n}")));
    }
    let heading_btn = gtk::MenuButton::builder()
        .label("(Hn)")
        .focus_on_click(false)
        .menu_model(&heading_menu)
        .build();
    crate::a11y::name(&heading_btn, "Heading level");
    heading_btn.add_css_class("flat");
    items.push(heading_btn.clone().upcast());

    // Capture the Link/Image buttons so their tooltip can flip to "Edit …" when the
    // selection is exactly that markup (update_format_edit_surfaces).
    let mut edit_btns = Vec::new();
    for cmd in FORMAT_CMDS.iter().skip(2) {
        let btn = format_button(cmd); // Strike … HR, then Link/Image/Table
        items.push(btn.clone().upcast());
        if let Some(kind) = FmtInsertKind::from_target(cmd.target) {
            edit_btns.push((kind, btn));
        }
    }
    // Annotate — a first-class command, NOT a `win.format`
    // target, so it drives the parameterless `win.annotate` action. Placed here in the
    // SHARED format row so the ONE button serves BOTH the toolbar Format section AND the
    // editor caret/format overlay (SSOT). No standard "add comment" symbolic icon exists
    // on the common themes (GTK4Rs/AP-48), so it uses the same 💬 glyph as the preview selection
    // popover for a consistent mark; its enabled state is `win.annotate`'s (an editor or
    // preview selection). `set_focus_on_click(false)` keeps editor focus so the action
    // stays enabled at activation (same reasoning as `format_button`).
    let annotate_btn = gtk::Button::with_label("\u{1f4ac}");
    crate::a11y::name_from_action(&annotate_btn, "win.annotate");
    annotate_btn.add_css_class("flat");
    annotate_btn.set_focus_on_click(false);
    annotate_btn.set_action_name(Some("win.annotate"));
    items.push(annotate_btn.upcast());
    (items, heading_btn, edit_btns)
}

/// [`format_items`] packed into one horizontal row — what the Stage-2 caret
/// overlay shows. The overlay is a popover sized to its content, so it wants a
/// single row and never wraps; the toolbar takes the items instead.
pub(crate) fn build_format_bar() -> (
    crate::widgets::wrapbox::ToolbarWrapBox,
    gtk::MenuButton,
    Vec<(FmtInsertKind, gtk::Button)>,
) {
    let (items, heading_btn, edit_btns) = format_items();
    // Wraps onto a second row in a pane narrower than the whole bar: the bar floats
    // inside the editor pane and is clipped to it, so a single row would leave its
    // last buttons unreachable in a narrow window or a split.
    let bx = crate::widgets::wrapbox::ToolbarWrapBox::new(2, 2);
    for it in &items {
        bx.append(it);
    }
    (bx, heading_btn, edit_btns)
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// TDD 16.4: every Format button's tooltip carries the shortcut its descriptor
    /// declares — read from `FORMAT_CMDS`, never restated. Insert Link…, Increase Indent
    /// and Horizontal Bar showed the bare label because `format_button` named every
    /// button without its accelerator.
    /// Mutation check: naming the button with `a11y::name` again fails here.
    #[gtktest::test]
    fn every_format_button_tooltip_shows_its_declared_shortcut() {
        let (items, _heading, _edit) = format_items();
        let buttons: Vec<gtk::Button> = items
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::Button>().ok())
            .collect();
        for cmd in FORMAT_CMDS.iter().filter(|c| !c.accel.is_empty()) {
            let want = crate::app::tooltip_with_accel(cmd.label, cmd.accel);
            let btn = buttons
                .iter()
                .find(|b| {
                    b.action_target_value()
                        .and_then(|v| v.str().map(str::to_owned))
                        .as_deref()
                        == Some(cmd.target)
                })
                .unwrap_or_else(|| panic!("no toolbar button for format target {}", cmd.target));
            assert_eq!(btn.tooltip_text().as_deref(), Some(want.as_str()));
            assert!(want.ends_with(')'), "{want}: no accelerator in the tooltip");
        }
    }

    /// The Insert/Edit relabel rewrites the tooltip, and must keep the shortcut.
    #[gtktest::test]
    fn the_edit_relabel_keeps_the_shortcut_in_the_tooltip() {
        let (_items, _heading, edit_btns) = format_items();
        for (kind, btn) in &edit_btns {
            crate::a11y::name_with_accel(btn, kind.label(true), kind.accel());
            let tip = btn.tooltip_text().expect("tooltip");
            assert!(tip.starts_with("Edit "), "{tip}");
            assert!(tip.ends_with(')'), "{tip}");
        }
    }
}
