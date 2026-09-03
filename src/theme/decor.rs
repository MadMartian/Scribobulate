//! **The resolved appearance of every decoration in the vocabulary — the one place a
//! sprite's precedence over its flat sibling is decided.**
//!
//! `sdd/SCHEMA.md` § Key naming states the rule and says why the renderers must
//! *branch* rather than paint over: *"filling first and tiling over looks identical
//! for an opaque sprite and lets the flat colour bleed through a transparent one,
//! which is a defect that appears only for the sprites nobody tested."* `sdd/THEMING.md`
//! names the intended shape — the precedence decided in **one** pure function that the
//! drawn preview and both export sinks read.
//!
//! That shape existed for exactly one decoration ([`MarkerChoice`], originally
//! `codeview::gutter::marker_substitute`). Every other one open-coded it: **8** sprite
//! kinds across **36** non-test call sites in 8 files. The mandate was honoured
//! everywhere — every site really did branch — but by convention, not by construction,
//! and *the precedence semantics differ per decoration with nothing recording that*.
//! Reading them off one by one was the only way to learn that the band is three-way,
//! the bar two-way, and the chip two-way but with its ink still painting on top.
//!
//! # Three shapes, and each is a type
//!
//! | Shape | Decorations | Precedence |
//! |---|---|---|
//! | [`Fill`] | blockquote bar, horizontal rule, annotation chip | sprite → flat → the consumer's own default |
//! | [`Band`] | heading band, per level | sprite → gradient → flat → absent |
//! | [`MarkerChoice`] | list markers (bullet · ordered · task · task-checked) | sprite → glyph → drawn |
//!
//! **All three are structs of ORDERED CANDIDATES, not enums of one winner**, and that
//! uniformity is load-bearing rather than tidy. Resolving to a winner discards the
//! rungs below it — so a paint site handed `Sprite(_)` has nothing left to fall back
//! to the moment that sprite will not decode, which is precisely how the list marker
//! became the one renderer that *erased* its decoration instead of degrading. Every
//! consumer here tries the sprite, and reaches for the next rung if it could not be
//! produced.
//!
//! The chip shares [`Fill`] rather than earning a type of its own, and the difference
//! that made it look like a third shape is documented where it belongs — at
//! [`Theme::annotation_chip_decor`]: the chip's **ink is independent of its fill**, so
//! a sprite replaces the fill and the count numeral still draws on top. That is a fact
//! about the chip's ink key, not about how its fill is chosen.
//!
//! # Why these are functions and not fields
//!
//! Every one is a pure function of a `Theme` the caller already holds, so there is
//! nothing to keep in step and no second representation to drift. Storing them would
//! make `Theme` self-referential (a resolved decoration borrows the `SpriteRef` the
//! same struct owns) for no gain: the decision is a handful of `Option` tests, and the
//! expensive half — decoding the sprite — is cached in `crate::sprite`.

use super::model::{ListGlyphs, Sprites, Theme};
use super::MarkerGlyph;
use crate::sprite::SpriteRef;
use gtk::gdk;

/// Which marker a list item wears, with **no source span and no widget** — the
/// display-free discriminant every sink shares.
///
/// The preview walks `renderer::ListMarkerKind` (which additionally carries a task
/// checkbox's source span, for the toggle); the PDF sink carries a `(task, start)`
/// pair; the HTML sink dispatches on neither and hand-wrote a table per surface. Eight
/// tables encoded this one fact, four of them inside a single ~100-line file, each
/// re-stating the arm order and each carrying its own copy of the "task arms come
/// first" comment. A new marker kind was eight coordinated edits with zero compiler or
/// test enforcement, and six of the eight still compiled after a partial change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    /// An unordered item's bullet. **The only kind whose decoration varies by nesting
    /// depth** (TDD 18.26).
    Bullet,
    /// An ordered item's numeral.
    Ordered,
    /// A task item's empty checkbox.
    Task,
    /// A task item's ticked checkbox.
    TaskChecked,
}

impl MarkerKind {
    /// Normalise the PDF sink's `(task, start)` pair.
    ///
    /// **The task arms come first, and that is the whole of what this encodes**: a
    /// task item inside an ordered list is still a checkbox — `1. [x] done` is a
    /// checkbox, not a number (SCHEMA § Lists). That sentence used to appear as a
    /// comment beside four separate `match (task, start)` tables in `pdf/decide.rs`
    /// alone, which is four chances to write the arms in the other order.
    pub(crate) fn from_task_and_start(task: Option<bool>, start: Option<u64>) -> MarkerKind {
        match (task, start) {
            (Some(true), _) => MarkerKind::TaskChecked,
            (Some(false), _) => MarkerKind::Task,
            (None, Some(_)) => MarkerKind::Ordered,
            (None, None) => MarkerKind::Bullet,
        }
    }
}

/// A decoration whose sprite stands in for a flat colour, with nothing between them.
///
/// **A struct of ordered candidates, not an enum of one winner** — the same shape
/// [`MarkerChoice`] has, and for the same reason. Precedence between two STATED values
/// is one question; what to do when the winner cannot be PRODUCED is another. An enum
/// that resolved to `Sprite(_)` would discard the flat colour a paint site needs the
/// moment that sprite fails to decode, which is exactly how the list marker came to be
/// the one renderer that *erased* its decoration instead of degrading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fill<'a> {
    /// The sprite this decoration's key names. **Outranks the flat colour** — and the
    /// flat colour is not painted underneath it, because a transparent sprite would
    /// let it bleed through (SCHEMA § Key naming).
    pub sprite: Option<&'a SpriteRef>,
    /// The theme's own flat colour. `None` means the theme states none, so the
    /// consumer's own pre-theming default applies — which is what keeps System
    /// byte-identical (TDD 18.2).
    pub flat: Option<gdk::RGBA>,
}

impl Fill<'_> {
    /// The flat colour to paint **when the sprite is absent or could not be
    /// produced**, falling back to the consumer's own default.
    ///
    /// A caller renders the sprite first and reaches for this only if that failed, so
    /// this deliberately does NOT ask whether a sprite outranks it: by the time it is
    /// called, the question is settled.
    pub(crate) fn flat_or(&self, default: gdk::RGBA) -> gdk::RGBA {
        self.flat.unwrap_or(default)
    }
}

/// A heading band's appearance at one level — the **three-way** shape.
///
/// The extra rung over [`Fill`] is the gradient, which sits between the sprite and the
/// flat value, and whose own SCHEMA row states the precondition the sprite's does not:
/// *"Ignored where the level states no fill"* — a gradient is a second stop, so it
/// needs a first one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Band<'a> {
    /// A sprite tiled across the band at its natural size, **in place of** the fill
    /// and the gradient. SCHEMA's `heading_band_sprite` row: *"Outranks the fill and
    /// the gradient."*
    pub sprite: Option<&'a SpriteRef>,
    /// A vertical two-stop gradient, `(from, to)`. Present only where the level
    /// states **both** a fill and a `gradient_to`.
    pub gradient: Option<(gdk::RGBA, gdk::RGBA)>,
    /// The level's flat fill.
    pub flat: Option<gdk::RGBA>,
}

impl Band<'_> {
    /// Whether this level carries a band at all — the gate every renderer applies
    /// before reserving the band's padding or drawing anything.
    ///
    /// **A sprite alone is a band.** It used to require a stated `heading_band_color`
    /// as well: all three renderers agreed with each other and all three disagreed
    /// with SCHEMA, whose `heading_band_sprite` row carries no fill precondition while
    /// the *gradient* row beside it does. A theme stating `heading_band_sprite_h1` and
    /// nothing else got no band, no sprite, no heading inset and **no log line** —
    /// ScrAP-324's class with a third case added, where "the reference resolved fine
    /// and was then discarded for want of an unrelated key" is pixel-identical to "the
    /// theme stated no sprite".
    ///
    /// **Absent, not transparent**: nothing is drawn and no space is reserved, which
    /// is what keeps a theme that bands nothing identical to one from before the key
    /// existed (TDD 18.2).
    pub(crate) fn is_present(&self) -> bool {
        self.sprite.is_some() || self.flat.is_some()
    }

    /// The flat appearance to paint when the sprite is absent or could not be
    /// produced: the gradient where the level states one, else the flat fill, else
    /// nothing.
    pub(crate) fn without_sprite(&self) -> Option<BandPaint> {
        match (self.gradient, self.flat) {
            (Some((from, to)), _) => Some(BandPaint::Gradient { from, to }),
            (None, Some(c)) => Some(BandPaint::Flat(c)),
            (None, None) => None,
        }
    }
}

/// What a band paints once its sprite is out of the running.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BandPaint {
    Gradient { from: gdk::RGBA, to: gdk::RGBA },
    Flat(gdk::RGBA),
}

/// What a list marker may be painted as, **in precedence order and all of it** —
/// rather than the single winner.
///
/// **Precedence between two STATED values is a different question from what to do
/// when the winner cannot be produced**, and collapsing the two is what made the list
/// marker the one renderer where a decode failure *erased* the decoration. Every
/// sibling degrades correctly — the band falls to gradient then fill, the bar to
/// `blockquote_bar_color`, the rule to a stock `GtkSeparator`, the chip to the flat
/// `annotation_chip_bg`. The list marker fell to **nothing**: not the theme's glyph,
/// not the drawn bullet, numeral or checkbox. A theme stating both a sprite and a
/// glyph lost the glyph too, because the winner was picked before any decode was
/// attempted.
///
/// The consequence was sharper than a missing decoration: a task checkbox that failed
/// to resample was *gone* while its hit-box was still recorded, leaving an invisible,
/// clickable checkbox — with no log line anywhere in the process.
///
/// `sdd/THEMING.md` § Untrusted input states the rule this restores: *"A reference
/// that fails any check is dropped to 'this decoration is absent' — the same
/// inert-by-default behaviour an unset key gets, never a partial render."*
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MarkerChoice<'a> {
    /// The sprite this kind's key names, if any. First choice.
    pub sprite: Option<&'a SpriteRef>,
    /// The glyph this kind's key names, if any. Second choice.
    pub glyph: Option<&'a MarkerGlyph>,
}

/// One rung of [`MarkerChoice`]: what a paint site should try next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MarkerSubstitute<'a> {
    Sprite(&'a SpriteRef),
    Glyph(&'a MarkerGlyph),
    /// No substitution: the bullet arc, the ordered numeral, or the drawn checkbox.
    Drawn,
}

impl<'a> MarkerChoice<'a> {
    /// Every candidate, **most preferred first, always ending at [`MarkerSubstitute::Drawn`]**.
    ///
    /// A consumer walks this until one produces ink; the first element is the winner
    /// among the values the theme *stated*. There is deliberately no `winner()`
    /// accessor beside it — every consumer in this codebase can degrade, and an
    /// accessor that returns one rung and discards the rest is exactly the shape that
    /// made the list marker erase its own decoration.
    ///
    /// The terminal `Drawn` is what makes the walk total: there is always something
    /// to draw.
    pub(crate) fn candidates(&self) -> Vec<MarkerSubstitute<'a>> {
        let mut out = Vec::with_capacity(3);
        if let Some(s) = self.sprite {
            out.push(MarkerSubstitute::Sprite(s));
        }
        if let Some(g) = self.glyph {
            out.push(MarkerSubstitute::Glyph(g));
        }
        out.push(MarkerSubstitute::Drawn);
        out
    }
}

impl Theme {
    /// The blockquote accent bar (TDD 18.28).
    pub(crate) fn blockquote_bar_decor(&self) -> Fill<'_> {
        Fill {
            sprite: self.sprites.blockquote_bar.as_ref(),
            flat: self.blockquote_bar_color,
        }
    }

    /// The horizontal rule (TDD 18.31).
    pub(crate) fn rule_decor(&self) -> Fill<'_> {
        Fill {
            sprite: self.sprites.rule.as_ref(),
            flat: self.rule_color,
        }
    }

    /// The annotation chip's **fill** (TDD 18.19).
    ///
    /// **The chip's ink is independent of its fill**, which is the one place this
    /// vocabulary's decorations differ from each other in a way a shared type cannot
    /// express: a sprite replaces the fill and the count numeral still draws in
    /// `annotation_chip_fg` on top of it. Read `annotation_chip_fg` separately; it is
    /// not part of this decision and never was.
    pub(crate) fn annotation_chip_decor(&self) -> Fill<'_> {
        Fill {
            sprite: self.sprites.annotation_chip.as_ref(),
            flat: self.annotation_chip_bg,
        }
    }

    /// The heading band at `level` (TDD 18.25, 18.32).
    ///
    /// **A sprite alone is a band.** It used to require a stated `heading_band_color`
    /// as well — all three renderers agreed with each other and all three disagreed
    /// with SCHEMA, whose `heading_band_sprite` row says "Outranks the fill and the
    /// gradient" with no fill precondition, while the *gradient* row beside it does
    /// carry one. A theme stating `heading_band_sprite_h1` and nothing else got no
    /// band, no sprite, no heading inset and **no log line**: ScrAP-324's class with a
    /// third case added, where "the reference resolved fine and was then discarded for
    /// want of an unrelated key" is pixel-identical to "the theme stated no sprite".
    ///
    /// The gradient keeps its precondition, because a gradient is a second stop and
    /// needs a first one — and a discarded one is now diagnosed rather than silent.
    pub(crate) fn heading_band_decor(&self, level: usize) -> Band<'_> {
        let level = level.min(super::HEADING_LEVELS - 1);
        let flat = self.heading_band.fills[level];
        Band {
            sprite: self.sprites.heading_band[level].as_ref(),
            // The gradient keeps its precondition, because a gradient is a second stop
            // and needs a first one — SCHEMA's `heading_band_gradient_to_color` row
            // states it, unlike the sprite row.
            gradient: flat.zip(self.heading_band.gradient_to[level]),
            flat,
        }
    }

    /// Whether **any** level carries a band — the document-wide gate a paint pass
    /// takes before walking headings at all.
    pub(crate) fn bands_nothing(&self) -> bool {
        (0..super::HEADING_LEVELS).all(|i| !self.heading_band_decor(i).is_present())
    }

    /// The band behind a disclosure's summary LINE (TDD 18.48).
    ///
    /// The same three-way [`Band`] the heading band resolves to, and deliberately the
    /// same type rather than a parallel one: the precedence (sprite, else gradient,
    /// else flat), the "a sprite alone is a band" rule and the degradation when a
    /// sprite will not decode are properties of the SHAPE, not of the decoration —
    /// and the drawn gutter's history is what a second copy of them would repeat.
    ///
    /// Flat rather than per level: a disclosure has no levels. Its gradient keeps the
    /// same precondition a heading band's does, because a gradient is a second stop.
    pub(crate) fn disclosure_band_decor(&self) -> Band<'_> {
        let flat = self.disclosure_band_color;
        Band {
            sprite: self.sprites.disclosure_band.as_ref(),
            gradient: flat.zip(self.disclosure_band_gradient_to),
            flat,
        }
    }

    /// What the disclosure indicator may be painted as, in the state `expanded`.
    ///
    /// A [`MarkerChoice`] — sprite → glyph → the stock icon — because it is the same
    /// shape of decision as a list marker: a themed picture, else a themed character,
    /// else what the engine draws. Two states resolving independently, exactly as the
    /// task checkbox's do, so a theme may restyle one and leave the other stock.
    pub(crate) fn disclosure_marker_decor(&self, expanded: bool) -> MarkerChoice<'_> {
        let (sprite, glyph) = if expanded {
            (
                &self.sprites.disclosure_expanded,
                &self.disclosure_glyphs.expanded,
            )
        } else {
            (&self.sprites.disclosure, &self.disclosure_glyphs.collapsed)
        };
        MarkerChoice {
            sprite: sprite.as_ref(),
            glyph: glyph.as_ref(),
        }
    }

    /// What a list marker of `kind` at 1-based nesting `depth` may be painted as.
    ///
    /// Only the **bullet** varies by depth (TDD 18.26); an ordered numeral at depth 3
    /// is still a numeral and a task box is still a box. The tier index is applied
    /// here rather than by the caller picking a value, because the decision about
    /// *which kinds are depth-varying* belongs with the decision about which key wins.
    pub(crate) fn marker_decor(&self, kind: MarkerKind, depth: usize) -> MarkerChoice<'_> {
        marker_choice(kind, depth, &self.list_glyphs, &self.sprites)
    }

    /// The ink one list marker is drawn in: the **bullet's** depth-tiered colour where
    /// the theme states one (TDD 18.26), the shared `list_marker` otherwise, and
    /// `None` where the theme states neither — which the caller answers with its own
    /// pre-theming default.
    ///
    /// Both task states share one colour (TDD 18.27): a checked and an unchecked box
    /// are the same control in two positions. The fold to `list_marker` already
    /// happened in `Theme::resolve`, so a theme that states no task colour is answered
    /// by that arm identically to the shared one.
    pub(crate) fn marker_ink(&self, kind: MarkerKind, depth: usize) -> Option<gdk::RGBA> {
        match kind {
            MarkerKind::Bullet => self.list_bullet_colors[super::depth_tier(depth)],
            MarkerKind::Task | MarkerKind::TaskChecked => self.list_task_color,
            MarkerKind::Ordered => self.list_marker_color,
        }
    }
}

/// A `ListGlyphs` that states nothing, so a caller asking only about SPRITES can reach
/// the one table without inventing a second projection of it.
///
/// `static` rather than `const`: a `const` is inlined at each use and would be a
/// temporary the returned reference outlives.
static NO_GLYPHS: ListGlyphs = ListGlyphs {
    bullet: [None, None, None],
    ordered: None,
    task: None,
    task_checked: None,
};

/// A `Sprites` that states nothing — the mirror of [`NO_GLYPHS`], for a caller asking
/// only about GLYPHS.
static NO_SPRITES: Sprites = Sprites {
    disclosure: None,
    disclosure_expanded: None,
    disclosure_band: None,
    annotation_chip: None,
    list_bullet: [None, None, None],
    list_ordered: None,
    list_task: None,
    list_task_checked: None,
    heading_band: [None, None, None, None, None],
    blockquote_bar: None,
    rule: None,
};

/// Just the SPRITE a marker of `kind` at `depth` reads.
///
/// The PDF sink needs the two halves separately: a sprite marker is a picture and
/// cannot ride inside a text run, so it is resolved first and the text markup is
/// suppressed entirely when one applies. Both halves still come off the one table.
pub(crate) fn marker_sprite(
    kind: MarkerKind,
    depth: usize,
    sprites: &Sprites,
) -> Option<&SpriteRef> {
    marker_choice(kind, depth, &NO_GLYPHS, sprites).sprite
}

/// Just the GLYPH a marker of `kind` at `depth` reads. The mirror of
/// [`marker_sprite`].
pub(crate) fn marker_glyph(
    kind: MarkerKind,
    depth: usize,
    glyphs: &ListGlyphs,
) -> Option<&MarkerGlyph> {
    marker_choice(kind, depth, glyphs, &NO_SPRITES).glyph
}

/// The marker keys one kind reads, at one depth. Separate from [`Theme::marker_decor`]
/// so a caller holding only the two key groups (as the drawn gutter's paint bundle
/// does) reaches the same table.
pub(crate) fn marker_choice<'a>(
    kind: MarkerKind,
    depth: usize,
    glyphs: &'a ListGlyphs,
    sprites: &'a Sprites,
) -> MarkerChoice<'a> {
    let tier = super::depth_tier(depth);
    let (sprite, glyph) = match kind {
        MarkerKind::Bullet => (&sprites.list_bullet[tier], &glyphs.bullet[tier]),
        MarkerKind::Ordered => (&sprites.list_ordered, &glyphs.ordered),
        MarkerKind::TaskChecked => (&sprites.list_task_checked, &glyphs.task_checked),
        MarkerKind::Task => (&sprites.list_task, &glyphs.task),
    };
    MarkerChoice {
        sprite: sprite.as_ref(),
        glyph: glyph.as_ref(),
    }
}
