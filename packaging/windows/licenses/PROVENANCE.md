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
| `cairo/COPYING` | `67228A9F…55ABDF` | 1,576 | Copied from the staged GTK prefix, `share/doc/cairo/COPYING`, cairo 1.18.4. This is cairo's own summary of its dual licensing; it is **not** a licence, which is why the two texts below ship with it. |
| `cairo/COPYING-LGPL-2.1` | `5749785C…EF81D2` | 26,001 | SPDX license-list-data, `text/LGPL-2.1-only.txt`. |
| `cairo/COPYING-MPL-1.1` | `6214F8B1…4A8795` | 23,669 | SPDX license-list-data, `text/MPL-1.1.txt`. |
| `freetype/FTL.TXT` | `5A5EE54C…02967F` | 6,743 | `docs/FTL.TXT` from the FreeType **source tree gvsbuild built**, `C:\gtk-build\build\x64\release\freetype`. Not fetched — see below. |
| `freetype/LICENSE.TXT` | `BD36C8B4…FD2D3B` | 2,149 | Root `LICENSE.TXT` from the same source tree. Discloses the sub-licences; not a duplicate of the FTL. |
| `gettext/COPYING.LIB` | `5749785C…EF81D2` | 26,001 | SPDX license-list-data, `text/LGPL-2.1-only.txt` — the same document as cairo's, hence the same hash. |
| `graphene/LICENSE.txt` | `CFD9FD7B…88DF9` | 1,077 | `github.com/ebassi/graphene`, tag `1.10.8`. |
| `hicolor-icon-theme/COPYING` | `B0A64377…8C4526` | 17,992 | `COPYING` from the hicolor-icon-theme **0.18 source tree gvsbuild built**, `C:\gtk-build\build\x64\release\hicolor-icon-theme`. Not fetched — the earlier attempt was blocked, and the build tree is better provenance anyway. |
| `gtksourceview-icons/CC-BY-SA-3.0.txt` | `3F941B3B…D7F72F` | 22,240 | SPDX license-list-data, `text/CC-BY-SA-3.0.txt`. |
| `libtiff/LICENSE.md` | `0E27C238…3C4ADE` | 2,416 | `gitlab.com/libtiff/libtiff`, tag `v4.7.1`. |
| `libxml2/Copyright` | `5D487388…217626` | 1,314 | `github.com/GNOME/libxml2` (the mirror), tag `v2.15.3`. `gitlab.gnome.org` answered 406 to a plain request. |
| `pcre2/LICENCE.md` | `197D8A73…10D811` | 4,011 | `github.com/PCRE2Project/pcre2`, tag `pcre2-10.47`. |

`gtksourceview-icons/CREDITS.txt` is authored here, not fetched: CC BY-SA 3.0
§4(c) wants an attribution, and there is no upstream file to copy one from.

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
