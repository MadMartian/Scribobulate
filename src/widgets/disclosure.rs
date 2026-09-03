//! The disclosure toggle — the small anchored control on a `<details>` summary line.
//!
//! # Why this is a real widget rather than drawn chrome
//!
//! Every other self-drawn affordance in the preview (task checkboxes, list markers,
//! annotation chips) is painted in `snapshot_layer` with a click hit-box, and none of
//! them has an accessible object: GTK's AT-SPI tree is built solely by walking the
//! WIDGET tree, so a range of buffer text can carry no role and no expanded state.
//! A disclosure whose only affordance was drawn would be unreachable by keyboard and
//! invisible to a screen reader, permanently, on our GTK floor.
//!
//! [`PLAN.accessibility.md`](../../sdd/PLAN.accessibility.md) forbids anchoring
//! widgets for the elements it covers, and that rule was narrowed — on measurement,
//! not argument — to the two mechanisms that actually bite: an anchored child sets a
//! floor under the view's minimum width equal to its OWN minimum, and each anchor
//! costs a `U+FFFC` that shifts every buffer-offset consumer at density. A table
//! contributes ~900px and one per list item is density; one ~30px control per
//! disclosure block is neither. MEASURED (`probes/textview-anchored-toggle.c`, GTK
//! 4.6.9): eight such children leave the view's minimum at 30 against 900 for a
//! table-sized child, with zero horizontal overflow.
//!
//! # Three things that are not optional, each measured on a live session
//!
//! 1. **Compose `GtkToggleButton`.** A hand-rolled widget that only calls
//!    `gtk_widget_class_set_activate_signal()` gets Space unconditionally but Enter
//!    only while the window has no default widget — `activate-default` falls back to
//!    the FOCUS widget when no default exists, so Enter appears to work during
//!    development and silently stops in any real dialog. `GtkToggleButton` sets
//!    `receives-default` and installs the five activation keyvals itself. Verified:
//!    Space and Enter both activate with a default button present.
//! 2. **Swap the indicator.** The icon IS the feedback channel. A build that emitted
//!    `toggled` 29 times without changing its arrow was reported as "did not do
//!    anything when clicking on it" — the signal is not the affordance.
//! 3. **Carry its own cursor.** An anchored child sits inside the text area and does
//!    NOT participate in the view's hover machinery (`hover_at_point`/`apply_hover`,
//!    and the cursor set from the view's motion handler in `preview::interactions`),
//!    so without this it hovers as an I-beam and reads as unclickable.

use gtk::prelude::*;

/// Collapsed and expanded indicator icons.
///
/// ⚠ The RTL variant is a SEPARATE ICON NAME, not a mirror — a self-drawn triangle
/// would have to handle direction itself, which is one more reason this is an icon on
/// a real widget rather than chrome.
const ICON_COLLAPSED: &str = "pan-end-symbolic";
const ICON_EXPANDED: &str = "pan-down-symbolic";

/// The control's own CSS class.
///
/// Named rather than written out twice: it marks the widget for styling AND is what
/// the view's line-wide click hit-test resolves a press against, so a press that
/// landed on the control is left to the control (ScrAP-79). Two spellings of it would
/// mean a click that toggles twice, which looks exactly like a click that does
/// nothing.
pub(crate) const CSS_CLASS: &str = "scrib-disclosure";

/// Build a disclosure toggle showing `expanded`'s state, named for `summary`.
///
/// The caller owns what activation MEANS (the fold state and the re-render it drives);
/// this owns only how the control looks, what it announces and how it is reached.
///
/// **`summary` is not optional and the parameter exists to make that structural.** The
/// control publishes an interactive accessible role and draws a bare chevron, so with no
/// accessible name a screen reader announces it as an unnamed toggle button — and a
/// document with six disclosures then offers six controls a user cannot tell apart. The
/// caller already holds the block's summary label (it is about to write it as the line's
/// text), so the name costs nothing but had to be asked for.
pub(crate) fn build(expanded: bool, zoom: f64, summary: &str) -> gtk::ToggleButton {
    let toggle = gtk::ToggleButton::new();
    set_expanded(&toggle, expanded, zoom);
    toggle.set_active(expanded);
    // Through `a11y::name`, the project's one naming choke point — it writes the tooltip
    // and the accessible `Label` together, which is what stops the pointer and the
    // screen reader ever describing this control differently.
    crate::a11y::name(&toggle, summary);

    // Flat: the control reads as an indicator in prose, not as a button dropped into
    // a paragraph.
    toggle.add_css_class("flat");
    toggle.add_css_class(CSS_CLASS);
    toggle.set_halign(gtk::Align::Start);
    // Sit on the text baseline like a glyph. GTK_ALIGN_START pins the child to the top
    // of the line box, which reads as the indicator floating above its own summary
    // text — reported from a live session before this was set.
    toggle.set_valign(gtk::Align::Baseline);
    toggle.set_cursor_from_name(Some("pointer"));

    toggle
}

/// Make an EXISTING toggle show `expanded` — its indicator and its accessible state,
/// which are the two channels this control has and the two that must never disagree.
///
/// **Deliberately does not touch `active`, and that is the whole safety property.**
/// `gtk_toggle_button_set_active` emits `toggled`, and this control's `toggled`
/// handler splices the document — so a refresh that also wrote `active` would be a
/// re-entrant toggle, and the one call site that needs this is running *inside* the
/// consequences of a toggle. `active` is already correct there: it is what the user's
/// click set, and the fold state was derived from it.
///
/// # Why this exists, and why its absence used to be correct
///
/// This function was deliberately absent, on the reasoning that "a toggle re-renders
/// the preview rather than mutating widgets in place, so every toggle is a freshly
/// `build`-ed control already carrying the right state", and that an in-place setter
/// would be a second way for the indicator and the accessible state to get out of
/// step. The first half was a true statement about the *implementation* that had
/// frozen into a statement of *requirement*: `preview::splice` changes only the
/// toggled block's own region, and the control sits on the summary line ABOVE that
/// region, so the toggle the reader clicked is now a survivor rather than a rebuild.
/// Left alone it would keep showing the arrow for the state the reader just left —
/// which rubric 2.26a forbids, and which is the exact "emitted `toggled` 29 times
/// without changing its arrow" report this widget's own module docs already record.
///
/// The second half — the drift worry — is answered by SHARING rather than by absence:
/// [`build`] calls this, so there is one definition of how the control shows a state
/// and no second place for the two channels to be set independently.
pub(crate) fn set_expanded(toggle: &gtk::ToggleButton, expanded: bool, zoom: f64) {
    let theme = crate::theme::active();
    // Design-time px at zoom 1.0, scaled explicitly — a widget property, so it does
    // NOT follow the CSS `font-size` rule (THEMING § Pixel metrics and zoom).
    let size = crate::theme::px(theme.metrics.disclosure_marker_size, zoom);
    toggle.set_child(Some(&indicator(&theme, expanded, size)));
    // ARIA's disclosure pattern is "a button with aria-expanded", which is exactly what
    // `GtkExpander` itself reports (`gtkexpander.c` — ACCESSIBLE_ROLE_BUTTON plus
    // ACCESSIBLE_STATE_EXPANDED updated on every toggle), so this maps cleanly onto a
    // role GTK already has rather than inventing one.
    toggle.update_state(&[gtk::accessible::State::Expanded(Some(expanded))]);
}

/// The control's indicator: a themed sprite, else a themed glyph, else the stock icon.
///
/// **The precedence is the engine's, not this function's.** `theme::decor` owns
/// sprite-outranks-glyph-outranks-drawn for every decoration in the vocabulary, and
/// hands back ORDERED CANDIDATES rather than a winner — so a sprite that will not
/// decode falls to the glyph instead of erasing the arrow, which is the failure mode
/// that made the list marker the one renderer that erased its own decoration.
///
/// The indicator is the whole feedback channel for this control (a build that emitted
/// `toggled` without changing its arrow was reported as doing nothing at all), so a
/// theme that restyles it must not be able to make it silent.
fn indicator(theme: &crate::theme::Theme, expanded: bool, size: i32) -> gtk::Widget {
    use crate::theme::MarkerSubstitute;
    for candidate in theme.disclosure_marker_decor(expanded).candidates() {
        match candidate {
            MarkerSubstitute::Sprite(sprite) => {
                // An exact-size texture rather than a paint pass: this is a WIDGET,
                // so the toolkit already has somewhere to put a picture, and the
                // resample seam the drawn markers take is for a snapshot they do not.
                if let Some(tex) = crate::sprite::scaled(sprite, size, size) {
                    let pic = gtk::Picture::for_paintable(&tex);
                    pic.set_size_request(size, size);
                    return pic.upcast();
                }
            }
            MarkerSubstitute::Glyph(glyph) => {
                // `as_plain()`, because a label built with `set_text` parses no markup
                // — the same projection the drawn gutter takes, and for the same
                // reason (see `MarkerGlyph`).
                let label = gtk::Label::new(Some(glyph.as_plain()));
                label.set_size_request(size, size);
                return label.upcast();
            }
            MarkerSubstitute::Drawn => {
                let icon = gtk::Image::from_icon_name(if expanded {
                    ICON_EXPANDED
                } else {
                    ICON_COLLAPSED
                });
                icon.set_pixel_size(size);
                return icon.upcast();
            }
        }
    }
    // `candidates()` always ends in `Drawn`, so the loop returns. Kept total rather
    // than unwrapped: a silent indicator is this control's worst failure.
    gtk::Image::from_icon_name(ICON_COLLAPSED).upcast()
}

/// The CSS node an indicator SHAPE draws its mark on, or `None` where the shape carries
/// its own pixels and takes no ink at all.
///
/// Test-only, and it exists to answer one question structurally rather than by
/// enumeration: `preview::css`'s `DISCLOSURE_MARKER_SELECTORS` has to name every node an
/// indicator can be, and a list of three names is only correct until [`indicator`] grows
/// a fourth shape. The `match` here is **exhaustive over the decoration vocabulary**, so
/// a new `MarkerSubstitute` variant does not compile until someone says which node it
/// draws on — and the guard beside it then requires a selector for that node. The ink
/// going quietly wrong on backdrop is exactly the failure this control cannot afford:
/// the indicator is its entire feedback channel.
#[cfg(test)]
fn marker_css_node(shape: &crate::theme::MarkerSubstitute<'_>) -> Option<&'static str> {
    use crate::theme::MarkerSubstitute;
    match shape {
        // A `GtkPicture` of a decoded texture — its colours are in the file, which is
        // the sprite-outranks-flat rule doing its job.
        MarkerSubstitute::Sprite(_) => None,
        MarkerSubstitute::Glyph(_) => Some("label"),
        MarkerSubstitute::Drawn => Some("image"),
    }
}

// NOTE: [`set_expanded`] above is the in-place refresh this file once argued should not
// exist. Read its rustdoc before removing it again: the argument was sound and its
// premise ("a toggle re-renders") stopped being true when `preview::splice` landed.

/// Display-free guards over the indicator's SHAPE vocabulary — no widget needed, so
/// they run under a plain `cargo test` rather than only under the integration feature.
#[cfg(test)]
mod vocabulary {
    use super::marker_css_node;
    use crate::preview::DISCLOSURE_MARKER_SELECTORS;

    /// **Every shape the indicator can wear has a selector inking it.**
    ///
    /// The exhaustiveness is [`marker_css_node`]'s `match` — a new shape does not
    /// compile until it is mapped — and this closes the other half: that the node it maps
    /// to is one the theme sheet actually states an ink for. Without both, a themed
    /// indicator keeps its colour focused and takes the desktop's `label:backdrop` ink the
    /// moment the window goes to the back, which is a silent degradation of the one
    /// channel this control has (TDD 18.52).
    ///
    /// Driven off the SHIPPED themes, so a theme that dresses its fold in a way no
    /// existing one does is covered by having been shipped rather than by anyone
    /// remembering this test.
    #[test]
    fn every_indicator_shape_a_shipped_theme_produces_has_a_selector() {
        let themes = crate::theme::Themes::builtin();
        let mut inked = 0usize;
        for entry in themes.chooser_list() {
            let id = entry.id;
            let theme = themes.resolve(&id);
            for expanded in [false, true] {
                for shape in theme.disclosure_marker_decor(expanded).candidates() {
                    let Some(node) = marker_css_node(&shape) else {
                        continue;
                    };
                    inked += 1;
                    let selector = format!("button.scrib-disclosure {node}");
                    assert!(
                        DISCLOSURE_MARKER_SELECTORS.contains(&selector.as_str()),
                        "theme {id:?} can draw its {} indicator on a `{node}` node, and \
                         the theme sheet states no ink for it — the mark then follows the \
                         desktop theme, including into backdrop",
                        if expanded { "expanded" } else { "collapsed" }
                    );
                }
            }
        }
        assert!(
            inked > 0,
            "no shipped theme produces an inkable indicator shape — this guard is vacuous"
        );
    }

    /// **The other direction: every selector the sheet states is a shape something
    /// PRODUCES**, and the bare button node is asserted on its own terms.
    ///
    /// The guard above proves each produced shape has a selector; it says nothing about
    /// a selector for a shape nothing produces — a rule inking a node that never
    /// appears, which reads as coverage and is not. Two of the three selectors were
    /// covered; the third, `button.scrib-disclosure` itself, is not a shape at all and
    /// so was reached by neither direction (F-TEST-A-009).
    #[test]
    fn every_selector_the_sheet_states_is_a_node_something_draws_on() {
        use std::collections::BTreeSet;

        let themes = crate::theme::Themes::builtin();
        let mut produced: BTreeSet<&'static str> = BTreeSet::new();
        for entry in themes.chooser_list() {
            let theme = themes.resolve(&entry.id);
            for expanded in [false, true] {
                for shape in theme.disclosure_marker_decor(expanded).candidates() {
                    if let Some(node) = marker_css_node(&shape) {
                        produced.insert(node);
                    }
                }
            }
        }
        assert_eq!(
            produced,
            ["image", "label"].into_iter().collect::<BTreeSet<_>>(),
            "the shipped themes no longer produce every indicator shape this guard \
             covers — a selector for a shape nothing produces is a selector nothing \
             checks, and the sheet is then inking a node that never appears"
        );

        // The BASE node, which is not a shape and so is reached by neither sweep. It
        // inks the button itself: the stock chevron GTK draws when a theme states no
        // shape of its own is not a child node at all, so without this selector a themed
        // page's default indicator keeps the desktop's ink (TDD 18.53).
        assert!(
            DISCLOSURE_MARKER_SELECTORS.contains(&"button.scrib-disclosure"),
            "the bare button node must be inked — it is what a theme stating no glyph \
             and no sprite draws on"
        );
        assert_eq!(
            DISCLOSURE_MARKER_SELECTORS.len(),
            produced.len() + 1,
            "and the sheet states exactly the produced shapes plus that base node — a \
             fourth selector would be one nothing here has an opinion about"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;

    fn icon_of(toggle: &gtk::ToggleButton) -> Option<String> {
        toggle
            .child()
            .and_downcast::<gtk::Image>()
            .and_then(|i| i.icon_name())
            .map(|s| s.to_string())
    }

    /// The whole accessibility case for making this a widget rests on it carrying a real
    /// role; a drawn affordance has none at all.
    ///
    /// **Which role is a property of the RUNTIME, not of this widget.** GTK 4.10 gave
    /// `GtkToggleButton` an accessible role of its own, so a toggle publishes
    /// `TOGGLE_BUTTON` from 4.10 and `BUTTON` below it — both are correct, and both are
    /// interactive, which is the contract this test is actually about. The name says
    /// "an interactive role" for that reason; the previous name said "the button role"
    /// and froze one runtime's answer into a universal claim, which is why this failed on
    /// GTK 4.22.4 while passing on 4.6.9 with the widget behaving identically.
    ///
    /// Asked of the runtime rather than the target, and not weakened to "any role" — the
    /// point is that whatever it publishes must be the role the accessible-name walk
    /// recognises (`a11y`'s `is_interactive_role`, whose own recurrence guard is the
    /// companion to this one).
    #[gtktest::test]
    fn a_new_toggle_publishes_an_interactive_role() {
        use gtk::glib::translate::IntoGlib;
        let toggle = build(false, 1.0, "Summary");
        let (major, minor) = (gtk::major_version(), gtk::minor_version());
        let role = toggle.accessible_role();

        if major > 4 || (major == 4 && minor >= 10) {
            assert_eq!(
                role.into_glib(),
                crate::a11y::ROLE_TOGGLE_BUTTON,
                "on GTK {major}.{minor} the disclosure affordance must present as a \
                 toggle button (the binding cannot NAME that variant at this project's \
                 v4_6 floor — see crate::a11y::ROLE_TOGGLE_BUTTON)"
            );
        } else {
            assert_eq!(
                role,
                gtk::AccessibleRole::Button,
                "on GTK {major}.{minor}, which predates the TOGGLE_BUTTON role, the \
                 disclosure affordance must present as a button"
            );
        }
    }

    #[gtktest::test]
    fn a_collapsed_toggle_shows_the_collapsed_indicator() {
        // The indicator IS the feedback channel: a build that fired `toggled` without
        // changing its arrow was reported from a live session as doing nothing at all.
        let toggle = build(false, 1.0, "Summary");
        assert!(!toggle.is_active());
        assert_eq!(icon_of(&toggle).as_deref(), Some(ICON_COLLAPSED));
    }

    #[gtktest::test]
    fn a_toggle_built_expanded_starts_expanded() {
        // `<details open>` renders expanded without user action (rubric 2.26b), so the
        // initial state must come from the document rather than from a default.
        let toggle = build(true, 1.0, "Summary");
        assert!(toggle.is_active());
        assert_eq!(icon_of(&toggle).as_deref(), Some(ICON_EXPANDED));
    }

    #[gtktest::test]
    fn the_indicator_scales_with_zoom() {
        // Pixel metrics are widget properties and do not follow the CSS font-size rule,
        // so they must be scaled explicitly on every render (POLICY, themed geometry).
        let small = build(false, 1.0, "Summary");
        let large = build(false, 2.0, "Summary");
        let px = |t: &gtk::ToggleButton| {
            t.child()
                .and_downcast::<gtk::Image>()
                .map(|i| i.pixel_size())
                .unwrap_or_default()
        };
        assert!(
            px(&large) > px(&small),
            "a zoomed indicator must grow: {} vs {}",
            px(&large),
            px(&small)
        );
    }
    /// **The indicator follows the reading theme, in all three of its appearances.**
    ///
    /// The arrow is the whole feedback channel for this control, so a theme that
    /// restyles it must not be able to make it silent — which is why the engine hands
    /// back ORDERED candidates and this asserts the fall-through, not just the winner.
    #[gtktest::test]
    fn a_theme_may_state_the_indicators_glyph_and_it_outranks_the_stock_icon() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.glyphy]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n\
             disclosure_glyph = \"+\"\ndisclosure_expanded_glyph = \"-\"\n",
        );
        let _guard = crate::theme::activate_for_test(themes.resolve("glyphy"));

        for (expanded, want) in [(false, "+"), (true, "-")] {
            let toggle = build(expanded, 1.0, "Summary");
            let label = toggle
                .child()
                .and_downcast::<gtk::Label>()
                .unwrap_or_else(|| panic!("expanded={expanded} should carry a glyph label"));
            assert_eq!(label.text().as_str(), want, "expanded={expanded}");
        }
    }

    /// **Pixel Quest's shipped indicator reaches the control as its PLATE, not as the
    /// glyph rung beneath it.**
    ///
    /// The theme-side guards prove the sprite resolves, decodes and resamples; none of
    /// them proves the *widget* takes it, and the failure is invisible from every one of
    /// them — falling to the glyph is the designed behaviour, so a plate that never
    /// reaches this function renders a perfectly reasonable arrow and logs nothing.
    /// MEASURED as a real regression while this theme was being dressed: the fold drew
    /// its glyph rung on a live Xvfb run with every theme-side test green, and only a
    /// screenshot said so (GTK4Rs/AP-168 — assert the OUTERMOST representation).
    ///
    /// Asserted on the shipped theme rather than on an inline fragment, because a
    /// compiled-in sprite is the case that broke: an inline fragment resolves through a
    /// directory origin and exercises a different arm of `sprite::resolve`.
    #[gtktest::test]
    fn pixel_quests_indicator_reaches_the_control_as_a_plate() {
        let _guard = crate::theme::activate_for_test(crate::theme::themes().resolve("pixelquest"));
        for expanded in [false, true] {
            let toggle = build(expanded, 1.0, "Summary");
            assert!(
                toggle.child().and_downcast::<gtk::Picture>().is_some(),
                "expanded={expanded}: Pixel Quest's plate did not reach the control — it \
                 fell to a rung beneath, which renders an arrow and says nothing"
            );
        }
    }

    /// The states resolve INDEPENDENTLY, so a theme may restyle one and leave the
    /// other stock — the same promise `list_task_glyph`/`list_task_checked_glyph`
    /// make, and the half a shared lookup would silently break.
    #[gtktest::test]
    fn stating_one_states_glyph_leaves_the_other_on_its_stock_icon() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.halfglyph]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n\
             disclosure_glyph = \"+\"\n",
        );
        let _guard = crate::theme::activate_for_test(themes.resolve("halfglyph"));

        assert!(
            build(false, 1.0, "Summary")
                .child()
                .and_downcast::<gtk::Label>()
                .is_some(),
            "the stated state takes the glyph"
        );
        assert_eq!(
            icon_of(&build(true, 1.0, "Summary")).as_deref(),
            Some(ICON_EXPANDED),
            "the unstated state keeps its stock icon"
        );
    }

    /// **TDD 18.2** — a theme stating none of these keys renders byte-identically to
    /// what the control always drew: the stock icon, at the size the old constant set.
    #[gtktest::test]
    fn a_theme_that_states_nothing_leaves_the_control_exactly_as_it_was() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.plain]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n",
        );
        let t = themes.resolve("plain");
        assert_eq!(
            t.metrics.disclosure_marker_size, 16,
            "Adwaita's 16px disclosure metric, which is what the old constant was"
        );
        let _guard = crate::theme::activate_for_test(t);
        let toggle = build(false, 1.0, "Summary");
        assert_eq!(icon_of(&toggle).as_deref(), Some(ICON_COLLAPSED));
        let image = toggle
            .child()
            .and_downcast::<gtk::Image>()
            .expect("an icon");
        assert_eq!(image.pixel_size(), 16);
    }

    /// The size is a design-time px at zoom 1.0 and is scaled explicitly, like every
    /// other themed metric applied through a widget property.
    #[gtktest::test]
    fn the_indicator_size_is_themed_and_follows_zoom() {
        let mut themes = crate::theme::themes();
        themes.merge_over_for_test(
            "[themes.big]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n\
             disclosure_marker_size = 24\n",
        );
        let _guard = crate::theme::activate_for_test(themes.resolve("big"));
        let at = |zoom: f64| {
            build(false, zoom, "Summary")
                .child()
                .and_downcast::<gtk::Image>()
                .expect("an icon")
                .pixel_size()
        };
        assert_eq!(at(1.0), 24, "the theme's design-time value");
        assert_eq!(at(2.0), 48, "scaled explicitly, not left at design size");
    }

    /// **A surviving control can be re-pointed at the state it now shows.**
    ///
    /// The splice keeps the toggle the reader clicked (it sits on the summary line,
    /// ABOVE the region a toggle changes), so the arrow is only right if something
    /// updates it. Both channels are asserted, because a control whose arrow moved and
    /// whose accessible state did not is broken for exactly the audience the widget
    /// exists for.
    #[gtktest::test]
    fn refreshing_a_live_toggle_moves_its_indicator_and_its_accessible_state() {
        let toggle = build(false, 1.0, "Summary");
        assert_eq!(icon_of(&toggle).as_deref(), Some(ICON_COLLAPSED));

        set_expanded(&toggle, true, 1.0);
        assert_eq!(
            icon_of(&toggle).as_deref(),
            Some(ICON_EXPANDED),
            "the indicator is the whole feedback channel and must follow the state"
        );

        set_expanded(&toggle, false, 1.0);
        assert_eq!(
            icon_of(&toggle).as_deref(),
            Some(ICON_COLLAPSED),
            "and back — an apply that works while its undo does not still passes a \
             one-way test"
        );
    }

    /// **A refresh must not emit `toggled`.**
    ///
    /// The one caller runs inside the consequences of a toggle, so a refresh that also
    /// wrote `active` would re-enter the splice — an infinite fold. This is why
    /// [`set_expanded`] leaves `active` alone rather than "keeping the two in sync",
    /// which is the obvious-looking spelling.
    #[gtktest::test]
    fn refreshing_a_toggle_does_not_re_emit_its_activation() {
        let toggle = build(false, 1.0, "Summary");
        let fired = std::rc::Rc::new(std::cell::Cell::new(0u32));
        {
            let f = std::rc::Rc::clone(&fired);
            toggle.connect_toggled(move |_| f.set(f.get() + 1));
        }
        set_expanded(&toggle, true, 1.0);
        assert_eq!(
            fired.get(),
            0,
            "a refresh writes the indicator, never the activation — otherwise the \
             disclosure toggle's own handler re-enters through it"
        );
        // Control: the signal DOES fire when the activation really changes, so the
        // zero above is a property of `set_expanded` and not of a dead oracle.
        toggle.set_active(true);
        assert_eq!(fired.get(), 1, "the oracle discriminates");
    }

    /// **A glyph indicator keeps its themed ink when the window goes to the back.**
    ///
    /// The sibling of the table cell's backdrop regression, and it reaches this control
    /// through its own widget shape: a themed glyph indicator is a `GtkLabel`
    /// ([`indicator`]), so the desktop theme's `label:backdrop { color: … }` matches the
    /// node the mark is drawn on, and the theme sheet's `button.scrib-disclosure` ink —
    /// which only INHERITS down to it — loses. MEASURED on a driven session before the
    /// child selectors existed: Synthwave's magenta ▶ turned white the moment focus left.
    ///
    /// The hostile rule sits below this test's own sheet — but both go ABOVE the app's
    /// process-global theme provider at `APPLICATION + 1`, which is never removed once
    /// installed and would otherwise decide the control's reading in a full-suite run
    /// (the table cell's twin records the measurement). Priority is not what decides this
    /// test; matching is.
    ///
    /// Mutation: cutting `button.scrib-disclosure label` from `DISCLOSURE_MARKER_SELECTORS`
    /// fails this. TDD 18.52.
    #[gtktest::test]
    fn a_glyph_indicator_keeps_its_themed_ink_in_the_backdrop_state() {
        let display = gtk::gdk::Display::default().expect("this test needs a display");
        let add = |css: &str, priority: u32| {
            let provider = gtk::CssProvider::new();
            provider.load_from_data(css);
            gtk::style_context_add_provider_for_display(&display, &provider, priority);
            provider
        };
        // Removed again on the way out: a display provider is PROCESS-global and libtest
        // shares one process (POLICY § Unit tests).
        struct Installed(gtk::gdk::Display, Vec<gtk::CssProvider>);
        impl Drop for Installed {
            fn drop(&mut self) {
                for p in &self.1 {
                    gtk::style_context_remove_provider_for_display(&self.0, p);
                }
            }
        }

        let mut themes = crate::theme::Themes::builtin();
        themes.merge_over_for_test(
            "[themes.probe]\ndisclosure_glyph = \"▶\"\ndisclosure_marker_color = \"#33ddaa\"\n",
        );
        let theme = themes.resolve("probe");
        let want = theme
            .disclosure_marker_color
            .expect("the probe theme states a marker ink");
        let palette = crate::palette::Palette::for_paper(&theme);
        let sheet = crate::preview::theme_css(&theme, &palette);
        let _active = crate::theme::activate_for_test(theme);

        // A backdrop toggle built exactly as the renderer builds one. Each reading gets
        // its own: an unrooted widget caches its computed style at the first read, and a
        // provider added afterwards does not invalidate it.
        let backdrop_toggle = || {
            let toggle = build(false, 1.0, "Summary");
            toggle.set_state_flags(gtk::StateFlags::BACKDROP, false);
            toggle
        };

        let _hostile = Installed(
            display.clone(),
            vec![add(
                "label:backdrop { color: #ff0000; }",
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2,
            )],
        );
        let control = backdrop_toggle();
        let glyph = control
            .child()
            .and_downcast::<gtk::Label>()
            .expect("a theme stating a glyph gets a GtkLabel indicator");
        let hijacked = glyph.style_context().color();
        assert_eq!(
            (hijacked.red(), hijacked.green(), hijacked.blue()),
            (1.0, 0.0, 0.0),
            "fixture no longer discriminates: the desktop theme's `label:backdrop` rule \
             must actually reach the glyph, or the assertion below proves nothing"
        );

        let _themed = Installed(
            display.clone(),
            vec![add(&sheet, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 3)],
        );
        let inked = backdrop_toggle()
            .child()
            .and_downcast::<gtk::Label>()
            .expect("a theme stating a glyph gets a GtkLabel indicator")
            .style_context()
            .color();
        assert_eq!(
            (inked.red(), inked.green(), inked.blue()),
            (want.red(), want.green(), want.blue()),
            "an unfocused window's disclosure glyph fell back to the desktop theme's \
             backdrop ink — the button's `color` only INHERITS to the glyph"
        );
    }
}
