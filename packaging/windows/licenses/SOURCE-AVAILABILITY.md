# Source availability for the copyleft libraries the Windows installer redistributes

**This is a release-process obligation, not a packaging one.** Nothing `stage.ps1` does
can discharge it, and no condition of `verify-licenses.ps1` can observe it — every one of
those four conditions is a predicate over *staged files*, and what is owed here is a file
that never enters the stage tree. The gate can be green while this is unmet. It has been.

**Scope is Windows alone, and that is measured rather than assumed.** The `.deb`/`.rpm`
declare `Depends:` on the distribution's own GTK and bundle no runtime; the macOS `.app`
links `/opt/homebrew` directly rather than bundling (`packaging/macos/bundle.sh` says so in
its own header). Only the Windows installer conveys these libraries in binary form, so only
the Windows installer triggers the clause.

**Do not read this scope claim more widely than the thing it was measured over.** It is a
statement about *who redistributes the GTK runtime*, not about *who owes attribution* —
that conflation was made twice during this work and is worth stating plainly rather than
cited: *"only Windows bundles the runtime"* is true and does NOT imply *"only Windows owes
attribution"*, because the statically linked `two-face` grammars owe attribution on all
three platforms while owing nothing under this clause. The two obligations are separated in
POLICY § "Third-party attribution".

## What the clause actually requires

All the LGPL components in this artefact are `LGPL-2.1-or-later` or `LGPL-2.0-or-later`;
there is no LGPL-3.0 component. Two clauses fire, not one:

- **§6 (linking)** — discharged by §6(b). The libraries are separate, replaceable DLLs, so
  the relinking limb is satisfied and no object-code drop of Scribobulate is owed.
- **§4 (conveying the library binaries)** — **not excused by §6(b)**, and this is the half
  that gets missed. LGPL-2.1 §4 has **no three-year written offer**, unlike GPLv2 §3(b).
  It offers "accompany with source", or the *designated place* route: if the object code is
  offered from a designated place, equivalent access to the source **from that same place**
  satisfies it.

Practically, for a GitHub release: the page carrying `scribobulate-*.exe` must also carry
the sources listed below, at the versions listed below.

## READ THIS BEFORE PUBLISHING ANYTHING: the dev box is not what ships

**The Windows installer is not built from a locally built GTK.** `pipeline.yml` pins
`GVSBUILD_VERSION: 2026.8.0` and downloads the prebuilt `GTK4_Gvsbuild_2026.8.0_x64.zip`;
CI never runs gvsbuild and never sees a source tarball. The operator's box, meanwhile, has
gvsbuild **2026.6.0** installed and a prefix built by it.

**Those two recipes do not pin the same component versions. Three of ten differ:**

| Component | Dev box (gvsbuild 2026.6.0) | **Shipped (gvsbuild 2026.8.0)** |
|---|---|---|
| GLib | 2.88.1 | **2.88.3** |
| gdk-pixbuf | 2.44.6 | **2.44.7** |
| Pango | 1.57.1 | **1.58.0** |

Publishing source for GLib 2.88.1 when the installer carries 2.88.3 discharges nothing, and
it is a mistake that looks like diligence — the same shape as the `cargo-about` default
resolve over-attributing librsvg by 132 crates.

**The near miss is the part worth keeping.** Everything on the dev box agrees with itself:
`lib/pkgconfig/*.pc`, the tarball filenames in `C:\gtk-build\src\`, and gvsbuild 2026.6.0's
own `projects/*.py` pins all report 2.88.1 / 2.44.6 / 1.57.1. Three independent readings,
mutually confirming, **all describing a build that is not the product.** And the single
component anyone had previously checked across this boundary was **librsvg**, which is one
of the seven that happen to agree — so the one check that was done returned a falsely
reassuring answer about the whole set. Agreement among measurements of the wrong subject is
not evidence about the right one.

## The components, and what each owes

Versions are **gvsbuild 2026.8.0's pins** — the recipe CI's zip is built from. Ids are
`licenses.psd1` row ids; that file remains the authority on which staged files belong to
which component, and this table deliberately does not restate its `Match` patterns.

| Row id | Component | Version shipped | Licence | Source owed |
|---|---|---|---|---|
| `glib` | GLib | **2.88.3** | LGPL-2.1-or-later | Yes — §4 |
| `gtk4`, `gtk4-schemas`, `icon-theme-cache` | GTK | 4.22.4 | LGPL-2.0-or-later | Yes — §4. One source tree covers all three rows |
| `gdk-pixbuf` | gdk-pixbuf | **2.44.7** | LGPL-2.1-or-later | Yes — §4 |
| `gtksourceview` | GtkSourceView | 5.20.0 | LGPL-2.1-or-later | Yes — §4. Also the tree the `gtksourceview-icons` CC-BY-SA artwork comes from |
| `pango` | Pango | **1.58.0** | LGPL-2.0-or-later | Yes — §4 |
| `librsvg` | librsvg | 2.62.3 | LGPL-2.1-or-later | Yes — §4, **and its `Cargo.lock`**; see below |
| `librsvg-rust` | librsvg's static Rust graph | (with librsvg 2.62.3) | includes MPL-2.0 | Yes, but under **MPL-2.0 §3.2**, not §4; see below |
| `cairo` | cairo | 1.18.4 | LGPL-2.1-only **OR** MPL-1.1 | Yes either way — §4 on one limb, MPL-1.1 §3.2 on the other. **Election unmade** |
| `fribidi` | GNU FriBidi | 1.0.16 | LGPL-2.1-or-later | Yes — §4 |
| `gettext-runtime` | GNU gettext (`intl.dll`, libintl) | 0.21 | LGPL-2.1-or-later | Yes — §4. The **runtime**, LGPL-2.1; not the GPL-3.0 tools |
| `adwaita-icon-theme` | adwaita-icon-theme | 50.0 | CC-BY-SA-3.0 **OR** LGPL-3.0-or-later | **Depends on the election** — none under CC-BY-SA, §4 under LGPL. **Election unmade** |
| `hicolor-icon-theme` | hicolor-icon-theme | 0.18 | GPL-2.0-or-later | **No.** Ruled: `index.theme` is plain text shipped verbatim, so it is already its own source and falls under §1, not §3. Nothing to publish |

Every other row is permissive or proprietary and owes notice only, which
`THIRD-PARTY-LICENSES.md` and the vendored texts already carry.

**MEASURED IN THE SHIPPED ZIP — the inference step is closed.**
`GTK4_Gvsbuild_2026.8.0_x64.zip` (300,885,620 bytes) was downloaded from the same URL
`pipeline.yml` uses, unpacked to its own prefix, and its `lib/pkgconfig/*.pc` read directly:

    glib-2.0  2.88.3    gtk4  4.22.4       gdk-pixbuf-2.0  2.44.7
    pango     1.58.0    cairo 1.18.4       gtksourceview-5 5.20.0
    librsvg-2.0 2.62.3  fribidi 1.0.16

Every figure matches what was inferred from the recipe, including all three that differ from
the dev box. The table above is now read off the artefact users receive, not off a tag.

### librsvg owes its lockfile, and that is not a restatement of the notices row

`packaging/windows/licenses/librsvg/THIRD-PARTY-RUST-NOTICES.txt` discharges *attribution*
for the 198 crates compiled into `rsvg-2-2.dll`. **Complete corresponding source is a
separate duty**: the Rust graph must be rebuildable, which means the lockfile travels with
the source, not just the notice. It is on the dev box at
`C:\gtk-build\build\x64\release\librsvg\Cargo.lock`, 84,387 bytes — measured, and matching
the figure recorded in the plan. librsvg is one of the seven components whose version is
the same on both recipes, so that lockfile is the right one.

The MPL-2.0 crates inside that graph (five, of Servo CSS lineage, unmodified) carry
**MPL-2.0 §3.2**, which is a source-availability duty over the covered *files* and is not
satisfied by librsvg's own tarball — that tarball does not vendor its dependencies. Their
sources come from crates.io at the versions the lockfile pins, which is the second reason
the lockfile is load-bearing rather than a courtesy.

## Two elections that are unmade, and one of them changes this table

The FreeType row established the rule: **a dual licence is an election the redistributor
makes, and the failure mode is not making it** — whichever text happens to get vendored
becomes the answer by accident. Two rows are currently in exactly the state FreeType was
ruled out of.

- **cairo.** `licenses.psd1` records `LGPL-2.1-only OR MPL-1.1`, and we vendor **both**
  texts (`cairo/COPYING-LGPL-2.1` and `cairo/COPYING-MPL-1.1`), which states that we
  redistribute under either. The prefix's own `share/doc/cairo/COPYING` is cairo's summary
  of the dual offer, not a licence, so it makes no election for us. **The source duty is
  unaffected** — both limbs owe source — so this does not block the table, only the texts.
- **adwaita-icon-theme.** `CC-BY-SA-3.0 OR LGPL-3.0-or-later`, and **the election has
  already been made by accident**: the prefix installs only `COPYING_CCBYSA3` (measured —
  it is the sole file in `share/doc/adwaita-icon-theme/`), so the row cites it and that is
  the only text that ships. **This one does change the duty.** Under CC-BY-SA-3.0 no source
  is owed; under LGPL-3.0-or-later it is. The table above reads "depends on the election"
  rather than resolving it, because resolving it by pointing at which file upstream happened
  to install is the precise mechanism the FreeType ruling forbids.

Ratify both in `PROVENANCE.md` beside the files, the way FreeType's was — not here, and not
by inference from what is present.

## THE UPSTREAM TARBALL IS NOT THE CORRESPONDING SOURCE FOR FOUR OF THESE

**We do not ship upstream's GTK. We ship gvsbuild's build of it, and gvsbuild patches it.**
§4 owes *complete corresponding source* — the source the shipped binary was actually built
from — so for any patched component the pristine tarball does not discharge the duty. It
discharges the duty for a library we did not distribute.

Measured against gvsbuild **2026.8.0**'s own `projects/*.py` (the recipe CI's zip is built
from), of the nine components that owe source:

| Component | Patched by gvsbuild? |
|---|---|
| GLib | **Yes** — `001-glib-package-installation-directory.patch` |
| GTK 4 | **Yes** — `0001-remove-direct-composition.patch` |
| gettext | **Yes** — four patches (`*-c99`, `gnulib-memset`, `libtextstyle-c99`) |
| librsvg | **Yes** — `001-fix-duplicate-symbols.patch` |
| Pango, GtkSourceView | No — `patches=[]`, declared and empty |
| gdk-pixbuf, cairo, FriBidi | No — no `patches` key at all |

**Build configuration counts too, not only patches.** Every recipe passes Meson flags that
change what is compiled — GTK 4 alone is built `-Dvulkan=disabled -Dmedia-gstreamer=disabled
-Dbuild-tests=false`, GLib `-Dtests=false -Ddocumentation=false`. Corresponding source means
a recipient can rebuild *our* binary, which needs those flags, not just the pristine tree.

**So the deliverable is upstream tarball + gvsbuild's patch set and recipes at the pinned
tag**, not the tarball alone. gvsbuild is itself public and GPL-2.0 licensed, so a snapshot
of its tree at `2026.8.0` supplies both halves and is small — the patches are single files
and the recipes are a few kB of Python each.

**This is the same failure shape as the version drift above, one layer deeper.** A tarball
named `glib-2.88.3.tar.xz` answers *"what did upstream publish under that name"*. The
question §4 asks is *"what were the bytes in the DLL you shipped built from"*, and those
differ by a patch nobody would see by looking at the filename. Getting the version right and
the patches wrong would have produced an archive that is correct in every visible respect
and still discharges nothing.

## Open: the mechanism, and why the obvious shortcut does not work

The route is chosen — designated place, same page as the installer — but not built.

**The tarballs are 83 MB in total**, measured on the dev box (`C:\gtk-build\src\`): the nine
LGPL components come to 79 MB, adwaita adds 4.3 MB if its election lands on LGPL. That is
small against GitHub's 2 GB per-asset limit, so size is not an argument for the fragile
alternative of publishing upstream URLs and relying on them staying reachable — which §4's
"from the same place" wording does not obviously permit anyway.

**But those tarballs cannot simply be uploaded from the dev box, and this is the trap.**
They are 2026.6.0's downloads, so seven are the right bytes and **three — GLib, gdk-pixbuf,
Pango — are the wrong versions**. A release assembled by copying that directory would
publish source for three libraries the user did not receive, and every filename would look
right. Whatever mechanism is built must take its version list from the shipped prefix, not
from whatever is sitting in `src\`.

That leaves a real choice for the operator, and it is the one thing here that is not
measurable:

1. **A CI step that assembles the bundle** — download each tarball at the version read out
   of the extracted prefix, add the gvsbuild tree at the pinned tag for the patches and
   recipes, attach the lot to the release beside the `.exe`. Self-maintaining across a
   gvsbuild bump; costs a download per release.
2. **A manual upload from a box that ran gvsbuild at the pinned version** — which today no
   box has, since the dev box is two releases behind CI's pin.

**One measured convenience for whichever route is taken:** gvsbuild's recipes carry a
`hash=` for each tarball, so the archives can be verified rather than trusted. Spot-checked
here — `gtk-4.22.4.tar.xz` on the dev box is SHA-256 `51bd9f60c7d23a66…`, byte-identical to
2026.8.0's pin. That makes the seven unchanged components re-usable from `C:\gtk-build\src\`
with proof, and isolates the re-fetch to GLib, gdk-pixbuf and Pango.

Until one route is built and run, no release should be published.
