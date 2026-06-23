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
    // missing name never renders the broken-image placeholder (ScrAP-39).
    let icon_name = cmd.icon.map(crate::icons::Icon::name).filter(|name| {
        gtk::gdk::Display::default()
            .map(|d| gtk::IconTheme::for_display(&d).has_icon(name))
            .unwrap_or(false)
    });
    let btn = match icon_name {
        Some(name) => gtk::Button::from_icon_name(name),
        None => gtk::Button::with_label(cmd.glyph),
    };
    crate::a11y::name(&btn, cmd.label);
    btn.add_css_class("flat");
    btn.set_focus_on_click(false);
    btn.set_action_name(Some("win.format"));
    btn.set_action_target_value(Some(&cmd.target.to_variant()));
    btn
}

/// Build a horizontal row of the Format commands — Bold, Italic, Heading
/// (MenuButton), Strikethrough, Code Span, Superscript, Subscript, Code Block,
/// Quote, Bulleted List, Numbered List, Task List, Horizontal Bar, then
/// Link/Image/Table — in
/// the same order as the menu.  Shared by the toolbar section and the Stage-2
/// caret overlay so the two surfaces can never drift.  Returns the row and its
/// heading MenuButton (the caller binds the latter's sensitivity to the action).
///
/// The heading picker is a GtkMenuButton, NOT a GtkDropDown: GtkDropDown's empty/
/// no-selection caption is a hardcoded translatable "(None)" GtkLabel baked into
/// its private button_stack template — no factory, property, or CSS overrides it
/// (see sdd/ANTI-PATTERNS.md).  A MenuButton's label is fully ours ("(Hn)") and
/// its menu items drive win.format::h{1..6} directly, so there is no selection
/// state to manage.
pub(crate) fn build_format_bar() -> (gtk::Box, gtk::MenuButton, Vec<(FmtInsertKind, gtk::Button)>) {
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    bx.append(&format_button(&FORMAT_CMDS[0])); // Bold
    bx.append(&format_button(&FORMAT_CMDS[1])); // Italic

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
    bx.append(&heading_btn);

    // Capture the Link/Image buttons so their tooltip can flip to "Edit …" when the
    // selection is exactly that markup (update_format_edit_surfaces).
    let mut edit_btns = Vec::new();
    for cmd in FORMAT_CMDS.iter().skip(2) {
        let btn = format_button(cmd); // Strike … HR, then Link/Image/Table
        bx.append(&btn);
        if let Some(kind) = FmtInsertKind::from_target(cmd.target) {
            edit_btns.push((kind, btn));
        }
    }
    // Annotate — a first-class command, NOT a `win.format`
    // target, so it drives the parameterless `win.annotate` action. Placed here in the
    // SHARED format row so the ONE button serves BOTH the toolbar Format section AND the
    // editor caret/format overlay (SSOT). No standard "add comment" symbolic icon exists
    // on the common themes (ScrAP-39), so it uses the same 💬 glyph as the preview selection
    // popover for a consistent mark; its enabled state is `win.annotate`'s (an editor or
    // preview selection). `set_focus_on_click(false)` keeps editor focus so the action
    // stays enabled at activation (same reasoning as `format_button`).
    let annotate_btn = gtk::Button::with_label("\u{1f4ac}");
    crate::a11y::name_from_action(&annotate_btn, "win.annotate");
    annotate_btn.add_css_class("flat");
    annotate_btn.set_focus_on_click(false);
    annotate_btn.set_action_name(Some("win.annotate"));
    bx.append(&annotate_btn);
    (bx, heading_btn, edit_btns)
}
