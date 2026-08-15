# Provenance of the vendored licence texts

Every file under `packaging/windows/licenses/` that is not authored here was
fetched from the source recorded below and is **byte-verbatim**. Nothing was
reformatted, re-wrapped, or annotated — a provenance header inside a licence
text would be a modification of the licence text, which is why that information
lives in this file instead.

Fetched **2026-08-14** onto a Windows host, and pinned to the version of the
component actually staged by `packaging/windows/stage.ps1` wherever upstream
offered a tag. Versions were read from the shipped `lib/pkgconfig/*.pc` in the
GTK prefix, not assumed.

The two FreeType texts are the exception and are **not** fetched: they are copied
from the source tree gvsbuild compiled, which is a strictly stronger provenance
than a tagged download. A tag asserts that upstream published these bytes under
that name; the source tree is the bytes the shipped DLL was built from. Both were
checked to agree — `bin/freetype-6.dll` reports FileVersion **2.14.3** and
`include/freetype/freetype.h` in that tree defines MAJOR 2, MINOR 14, PATCH 3.

**This file records the licence *texts* we ship. It does not record the *source*
some of those licences additionally oblige us to publish** — that is
`SOURCE-AVAILABILITY.md`, beside this one, and it is a release-process duty no
staging step and no condition of `verify-licenses.ps1` can discharge or observe.
Two dual-licence elections it raises, cairo's and adwaita-icon-theme's, are
unmade and belong here once ruled, the way FreeType's was.

## Why this file exists at all

Four of the first nine fetches returned **HTTP 200 with a body that was not the
licence** — `gitlab.freedesktop.org` served an anti-bot interstitial page, all
four byte-identical at 4626 bytes, to requests that `Invoke-WebRequest` reported
as successful. A fetch step that checks only the exit status would have vendored
four HTML bot-check pages as licence texts and passed.

That is the same failure the gate's fourth condition exists for, arriving from a
new direction: a downloaded file is exactly as capable of being the wrong
document as `pcre2/COPYING` was. So every fetch here was accepted only after its
content was asserted, and every row in `licenses.psd1` names a string that must
occur in these bytes — the gate re-checks at build time what the fetch checked
once.

## The files

| File | SHA-256 | Bytes | Source |
|---|---|---|---|
| `cairo/COPYING` | `67228A9F…55ABDF` | 1,576 | Copied from the staged GTK prefix, `share/doc/cairo/COPYING`, cairo 1.18.4. This is cairo's own summary of its dual licensing; it is **not** a licence, which is why the elected text below ships with it. |
| `cairo/COPYING-LGPL-2.1` | `5749785C…EF81D2` | 26,001 | SPDX license-list-data, `text/LGPL-2.1-only.txt`. |
| `freetype/FTL.TXT` | `5A5EE54C…02967F` | 6,743 | `docs/FTL.TXT` from the FreeType **source tree gvsbuild built**, `C:\gtk-build\build\x64\release\freetype`. Not fetched — see below. |
| `freetype/LICENSE.TXT` | `BD36C8B4…FD2D3B` | 2,149 | Root `LICENSE.TXT` from the same source tree. Discloses the sub-licences; not a duplicate of the FTL. |
| `gettext/COPYING.LIB` | `5749785C…EF81D2` | 26,001 | SPDX license-list-data, `text/LGPL-2.1-only.txt` — the same document as cairo's, hence the same hash. |
| `graphene/LICENSE.txt` | `CFD9FD7B…88DF9` | 1,077 | `github.com/ebassi/graphene`, tag `1.10.8`. |
| `hicolor-icon-theme/COPYING` | `B0A64377…8C4526` | 17,992 | `COPYING` from the hicolor-icon-theme **0.18 source tree gvsbuild built**, `C:\gtk-build\build\x64\release\hicolor-icon-theme`. Not fetched — the earlier attempt was blocked, and the build tree is better provenance anyway. |
| `gtksourceview-icons/CC-BY-SA-3.0.txt` | `3F941B3B…D7F72F` | 22,240 | SPDX license-list-data, `text/CC-BY-SA-3.0.txt`. |
| `libtiff/LICENSE.md` | `0E27C238…3C4ADE` | 2,416 | `gitlab.com/libtiff/libtiff`, tag `v4.7.1`. |
| `libxml2/Copyright` | `5D487388…217626` | 1,314 | `github.com/GNOME/libxml2` (the mirror), tag `v2.15.3`. `gitlab.gnome.org` answered 406 to a plain request. |
| `pcre2/LICENCE.md` | `197D8A73…10D811` | 4,011 | `github.com/PCRE2Project/pcre2`, tag `pcre2-10.47`. |

Two `CREDITS.txt` files are authored here, not fetched, because CC BY-SA 3.0 §4(c)
wants an attribution and a licence text is not one. **They rest on different
footing, and the difference is worth keeping:**
`gtksourceview-icons/CREDITS.txt` had to be **reconstructed** — author and title
were recovered from git history, upstream saying nothing about how it wants to be
credited. `adwaita-icon-theme/CREDITS.txt` is **quoted** — upstream's own `COPYING`
states *"When attributing the artwork, using 'GNOME Project' is enough. Please link
to http://www.gnome.org where available."* A credit the rights-holder specifies
beats one we compose, so that file quotes rather than paraphrases it.

## The two licence ELECTIONS, ruled and recorded

Both were dual offers sitting in exactly the state the FreeType row was ruled out
of — where **whichever text happens to be present decides the question by
accident**. Each is now a choice, made here rather than implied by the tree.

| Component | Offer, read from upstream's own file | **Elected** | Why |
|---|---|---|---|
| **cairo 1.18.4** | `COPYING`: every file in `src/` — the whole of what `cairo-2.dll` is built from — under **either** LGPL-2.1 **or** MPL-1.1 | **`LGPL-2.1-only`** | Every other copyleft component in this installer is LGPL-2.0/2.1. One regime: one §6(b) DLL-replaceability argument, one §4 source-publication process. MPL-1.1 would add a second compliance mechanism for exactly one DLL and buy nothing. |
| **adwaita-icon-theme 50.0** | `COPYING`: *"either the GNU LGPL v3 or Creative Commons Attribution-Share Alike 3.0"* | **`CC-BY-SA-3.0`** | The licence written for artwork, and it carries **no source-code obligation** where LGPL-3.0 would drag a source-publication duty onto a set of SVGs. Icons ship unmodified, so no Adaptation is created and ShareAlike §4(b) is not engaged. |

**`cairo/COPYING-MPL-1.1` was deliberately REMOVED**, not merely unused: shipping
both texts states we redistribute under either, which is the ambiguity the election
exists to remove. Upstream's `COPYING` still ships and still describes the dual
offer — a true statement about cairo — with our election recorded here beside it.

**The adwaita election was already being made, by accident, and that is why it
needed ruling.** The GTK prefix installs only `COPYING_CCBYSA3` (measured: the sole
file in `share/doc/adwaita-icon-theme/`), so the row cited it and that was the only
text that shipped. The outcome happens to be the one we would have chosen; it was
still not chosen. Read from the 50.0 source tree's `COPYING`, not from which file
the prefix happened to install.

## These texts now SHIP — they did not before

`licenses.psd1` opens by observing that *"the installer currently ships not one line
of their licence text"*, and that stayed true through all the vendoring recorded
above. This file's `Source` column says where each text **comes from**; nothing
copied it anywhere. `verify-licenses.ps1` reads those paths off the **build
machine**, so every condition could pass while the installed product carried no LGPL
text at all — a gate green about the build tree and silent about the artefact.

`stage.ps1` now stages every row's text to `share\licenses\<row Id>\`, driven by
this manifest rather than by a second list, so adding a component here ships its
text with no separate step to forget. **Measured after the change: 902 staged files,
35 rows, 38 licence texts across 34 component directories, gate down to its one
deliberate red row (`msvc-runtime`).**

**The premise above is MEASURED, not argued — and the control is the valuable half.**
The `windows` seat ran the gate on the commit *before* this change and got the
identical verdict:

| | staged files | rows | gate verdict |
|---|---|---|---|
| before (`26ea7d9`) | 865 | 34 | 1 problem — `msvc-runtime`, conditions 1/2/4 clean |
| after | 902 | 35 | 1 problem — `msvc-runtime`, conditions 1/2/4 clean |

**Adding 37 licence texts to the artefact did not move the gate's answer by one
character.** That is the proof that it never read the staged tree for them: had it
been checking what shipped, the before-run could not have been clean. So
`licenses.psd1`'s opening claim is no longer an assertion anyone has to take on
trust — it is a measurement taken from both sides of the change, on two boxes. An
asserted premise and a measured one carry different weight with a lawyer.

### A GREEN GATE ON A DEVELOPER BOX DOES NOT CERTIFY THE SHIPPED INSTALLER

**Read this before quoting a gate result as evidence about the product.** The gate
resolves every `prefix:` Source against the **local** GTK prefix. A developer box
runs gvsbuild **2026.6.0**; CI pins **2026.8.0** and downloads the prebuilt zip. So
a pass here is evidence about *2026.6.0's licence texts*, not about the ones the
installer carries.

That is not hypothetical for these rows. Three components differ across those
recipes — **GLib 2.88.1 → 2.88.3**, gdk-pixbuf 2.44.6 → 2.44.7, Pango 1.57.1 →
1.58.0 — and **GLib and gettext are LGPL rows**, precisely the ones the §4 source
duty in `SOURCE-AVAILABILITY.md` turns on.

**CLOSED — the gate has now been run against the shipped artefact.**
`GTK4_Gvsbuild_2026.8.0_x64.zip` (300,885,620 bytes) was downloaded from the URL
`pipeline.yml` uses, unpacked to its own prefix, staged from with
`stage.ps1 -GtkPrefix`, and gated with `verify-licenses.ps1 -GtkPrefix`:

| | staged files | rows | verdict |
|---|---|---|---|
| dev prefix, 2026.6.0 | 902 | 35 | 1 problem — `msvc-runtime` |
| **shipped prefix, 2026.8.0** | **903** | **35** | **1 problem — `msvc-runtime`** |

Conditions 1, 2 and 4 clean **against the GTK that actually ships**. The table
covers the shipped tree, not merely a same-named lookalike.

**The one-file difference is real and was chased rather than rounded off.**
2026.8.0 stages one extra file — `share\icons\Adwaita\symbolic\places\folder-drag-accept-symbolic.svg`
— an icon upstream added. It matched the `adwaita-icon-theme` row's pattern and so
raised no condition-1 row, which is the exhaustive table doing exactly what it is
for: a new upstream file was attributed automatically, and had it landed somewhere
unclaimed the gate would have said so.

### The shipped binaries are much larger than the dev box's — CAUSE OPEN, and an earlier version of this section wrongly called it settled

> **RETRACTION, and the reasoning is the useful part.** This section first concluded
> *"a release build that retains debug information, not `debugoptimized`"*. That
> conclusion was **not supported by the evidence given for it**, and the `windows`
> seat took it apart on three grounds, all of which hold:
>
> 1. **`VCRUNTIME140.dll` vs `...140D.dll` is the CRT axis, not the meson buildtype
>    axis.** meson `debugoptimized` is `-O2 -g` against the **release** CRT, so both
>    configurations import `VCRUNTIME140.dll`. The test ruled out a debug-CRT build,
>    which nobody had proposed.
> 2. **The `\release\` in the embedded PDB path is the exact string ScrAP-279 warns is
>    not evidence** — gvsbuild rewrites the config string to `release` for install
>    pathing, so both configurations land there. It is the trap, not the discriminator.
> 3. **"A release build that retains debug info" is the definition of
>    `debugoptimized`** (`-O2 -g`). The sentence described the thing it claimed to
>    rule out.
>
> Two non-discriminating observations were read as a settled negative. **The
> buildtype question is OPEN**, and a wrong settled answer in this file costs more
> than an honest open one, because this file is read by people deciding what to trust.

**Measured**: staged 43.0 MB → 79.0 MB; `rsvg-2-2.dll` **5,277,184 → 32,069,120
bytes, a factor of six**, with `harfbuzz-subset`, `fontconfig-1`, `harfbuzz`,
`gio-2.0-0`, `gtk-4-1`, `cairo-2` and `gtksourceview-5-0` all larger. **Cause not
established.**

#### What WAS settled, by the test that actually discriminates

The reason this mattered beyond provenance is that `debugoptimized` vs `release`
can change **assertion enforcement**, and if the shipped GTK enforced `g_assert`
differently from the one we test against, every assertion-backed contract in GTK
would behave differently for users than for whoever tested it. That is ScrAP-279's
originating symptom. So it was tested, on both prefixes, by the paired-literal
probe (`assertion failed: (` plus the stringified expression sitting beside its
`G_STRFUNC` name in the string table):

| `gtk-4-1.dll`, GTK **4.22.4 in both** (control) | `assertion` | `priv->is_realized` | `g_return_if_fail` | `should not be reached` |
|---|---|---|---|---|
| dev prefix, 2026.6.0 | 7 | 1 | 1 | 1 |
| shipped prefix, 2026.8.0 | 8 | 2 | 1 | 1 |

**Assertions are compiled IN on both.** Neither build is `-DG_DISABLE_ASSERT`, so
the divergence that would have been serious is **not present** — measured on both
sides, not inferred from one. That retires the correctness worry; it does **not**
retire the buildtype question.

**One residual, recorded rather than explained away:** at *identical* GTK 4.22.4 the
shipped binary carries one assertion literal the dev one does not
(`gsk_renderer_real_render` / `!priv->is_realized`). Same source, different surviving
literals — consistent with different optimisation or inlining, and **not** evidence of
a different assertion policy. Do not read one literal as a wholesale difference.

**Still open, and needing the one test neither seat has run:** what buildtype
gvsbuild's published zip is actually produced with. Nothing on disk answers it —
that is ScrAP-279's whole point. The runtime oracle if it is ever worth settling:
realize a `gsk::CairoRenderer`, `render_texture`, then drop it **still realized**;
an assertion-enforcing build aborts at `gskrenderer.c:130`.

**Licensing is unaffected either way** — version, licence and crate graph are
unchanged, and the gate is clean on conditions 1/2/4 against the shipped tree. **But
a size or hash taken from a locally built DLL is not a fact about the shipped one.**

**Both of this section's corrections came from the `windows` seat**, and the second
was a correction of the first correction's author. **Raised by the `windows` seat,
against this document's own finding.** The version
drift was recorded here first and its consequence for the gate was not — the note
said the *versions* were measured on the wrong build and stopped there, without
asking what else read from that prefix. A finding is not finished when it is
written down; it is finished when everything it implicates has been re-read.

## Three of these replace a text the GTK prefix already ships

Not additions — **corrections**, and each was caught by the gate rather than by
reading:

- **`pcre2`.** The prefix's `share/doc/pcre2/COPYING` is four lines telling you
  to read a `LICENCE` file that gvsbuild does not install. Upstream has since
  renamed that file to `LICENCE.md`, which is what is vendored here. The 97-byte
  `COPYING` is still in the upstream tree too, so this is not gvsbuild dropping
  a file — the pointer is what upstream ships under that name.
- **`cairo`.** `share/doc/cairo/COPYING` is a summary naming
  `COPYING-LGPL-2.1` and `COPYING-MPL-1.1`; the prefix contains neither.
- **`gettext`.** `share/doc/gettext/COPYING` is **GPL-3.0**, which covers the
  gettext *tools*. The binary staged is `intl.dll` — libintl, LGPL-2.1. Shipping
  the prefix's file would not under-attribute, it would state that a component
  we ship is under GPL-3.0 when it is not.

## Still missing, deliberately

- **`msvc-runtime`** — the redistributable directory contains no terms file at
  all (measured: only `vc_redist.x64.exe` and `vc_redist.x86.exe`), because the
  terms live in the Visual Studio licence. **This row may be deleted rather than
  filled**: not shipping `vcruntime140*.dll` at all, and having the installer
  invoke Microsoft's own redistributable, removes the obligation instead of
  documenting it. That is not a packaging call — `scribobulate.iss` sets
  `PrivilegesRequired=lowest`, so the alternative costs either an admin prompt or
  the no-admin property itself. Left red pending that decision, not pending
  effort.

## Two of these were fetched from the build tree, not downloaded

`freetype/` and `hicolor-icon-theme/` come from the source trees gvsbuild
compiled, which beats a tagged download: a tag asserts upstream published those
bytes under that name; the build tree is the bytes the shipped artefact was made
from. Both were checked to be that tree and not merely a same-named one —
FreeType by version agreement (`freetype-6.dll` FileVersion 2.14.3 against
`freetype.h` MAJOR/MINOR/PATCH), hicolor by content: its `index.theme` is
byte-identical to the prefix's and to the staged copy, SHA-256
`A02DB5E1…CB9BC5`.

**`hicolor-icon-theme` carries a caveat the row also states.** Its `COPYING` is
the full GPL-2 text, measured. But upstream ships **no version-selection
statement anywhere in that tree** — no per-file header, no notice beside
`index.theme`, no copyright line at all. So `GPL-2.0-or-later` rests on Debian's
determination, not on anything in the artefact; only the licence *identity* is
measured here. Recorded rather than smoothed over, because "the tree contains no
statement" is evidence about what upstream ships, not about what the terms are —
the same distinction the icon row got wrong twice.

## A fourth replacement, and this one the gate could not catch

**The FreeType text vendored first was SPDX's `text/FTL.txt`, and it is not the
text FreeType 2.14.3 ships.** It is a re-wrapped rendering of the same licence:
every paragraph unwrapped to a single line, the section rules stripped, and the
project URL left at the older `http://www.freetype.org` where 2.14.3's own file
reads `https://freetype.org`. 5,979 bytes against 6,743.

Same licence, so nothing was misstated — but it is the pattern this whole file
was written about, one notch subtler. `pcre2/COPYING` was a pointer instead of a
licence and the anti-bot pages were HTML instead of a licence; both are the wrong
*document*. This was the right document in the wrong *edition*, sitting beside a
provenance line that said "FreeType 2.14.3 is shipped".

**The gate passed it, and would pass it again.** Condition 4 asks whether a
declared string occurs in the text; `The FreeType Project LICENSE` occurs in
both. A string anchor discriminates documents, not revisions of one — which is
the correct scope for it and worth writing down rather than fixing, because
widening it to a hash would make every row fail on any upstream whitespace
change and teach people to re-baseline instead of to read.

What caught it was not a check. It was going to the build tree for `LICENSE.TXT`
and finding `docs/FTL.TXT` sitting next to it.

## The election, ratified — and what ratifying it did not discharge

**FreeType is dual-licensed FTL or GPL-2.0-or-later. We redistribute under the
FTL**, ratified by the operator on 2026-08-14 as a deliberate election rather
than an inherited default. Only `FTL.TXT` is vendored; `docs/GPLv2.TXT` exists in
the same source tree and is deliberately absent, because shipping both would say
we redistribute under either. The row's SPDX expression is therefore `FTL`, not
the upstream disjunction — it records what we ship under, not what we were
offered.

Cairo is dual-licensed the same way and is handled the opposite way — **both**
texts ship — because cairo's own `COPYING` names both and reproducing it without
them leaves a summary pointing at nothing.

### FTL §2 is owed in the installer documentation, and is NOT gate-enforced

> *Redistribution in binary form must provide a disclaimer that states that the
> software is based in part of the work of the FreeType Team, in the
> distribution documentation.*

Note where it has to appear: **in the distribution documentation.** Not in a
licence file, not in the staged `licenses\` tree. This is the one obligation in
the whole table discharged by prose we write rather than by a file we stage, and
the consequence is specific:

**All four gate conditions are about staged files and their contents, so the
freetype row can be green on every one of them while §2 is unmet.** A reader who
takes four green conditions for completeness will be wrong about exactly this
row. The gate is not being extended to cover it — a condition reaching outside
the staged tree would be worse than a gap that is written down — so this
paragraph is the enforcement.

Wording is owed in the installer's documentation, not here. The URL to
freetype.org that §2 mentions is encouraged, not mandatory.

### FTL §3 is a name-use restriction, not an advertising clause

`LICENSE.TXT` calls the FTL *"similar to the original BSD license with an
advertising clause"* and says the FTL is GPLv2-incompatible *"due to its
advertisement clause"*. **Upstream's own summary is loose here and it has already
misled the research on this row once.** The section it refers to reads:

> *Neither the FreeType authors and contributors nor you shall use the name of
> the other for commercial, advertising, or promotional purposes without specific
> prior written permission.*

That is a restriction on using a name, symmetric between the parties. It is not
a BSD-4 clause requiring FreeType to be named in advertising materials, and the
sentence that follows it — suggesting phrases like `FreeType Project` — says
*"We suggest, but do not require"*. **There is no advertising-materials
obligation to satisfy here; do not add one.** The credit that is genuinely owed
is §2 above, and it is owed in documentation, not in advertising.
