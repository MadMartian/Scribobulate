# macOS packaging

`bundle.sh` builds `target/macos/Scribobulate.app` around the release binary.

## Prerequisites

```bash
brew install gtk4 gtksourceview5 adwaita-icon-theme
```

`bundle.sh` runs `cargo build --release` itself, and that build links GTK4 via
`pkg-config`. Skip this step and the build fails before the script does
anything bundle-specific — `pkg-config` errors for `gtk4`, `gdk-pixbuf-2.0`,
`pango`, `cairo`, `glib-2.0`, `gio-2.0`, `gobject-2.0`, and
`graphene-gobject-1.0`, with `PKG_CONFIG_PATH` unset. That signature means the
Homebrew packages above were never installed on this machine, not that
something is broken.

## Usage

```bash
./install.sh                                 # the .app, plus `scribobulate` on PATH
./uninstall.sh                               # remove both again
packaging/macos/bundle.sh                    # -> target/macos/Scribobulate.app, nothing on PATH
packaging/macos/dmg.sh                       # -> Scribobulate-<version>-<arch>.dmg
open target/macos/Scribobulate.app --args "$PWD/path/to/document.md"
```

`./install.sh` and `./uninstall.sh` at the repository root are the canonical pair, and
they are the same command on every platform: each is a router that dispatches on
`uname -s` and hands over to `packaging/macos/install.sh` or
`packaging/macos/uninstall.sh` here. Running those two directly is equivalent, and the
sections below say what they do. Prefer the router in anything you write down —
documenting the per-platform path as the primary one is exactly how the router came to
go unmentioned in this file and in the root README at the same time.

`bundle.sh`, `dmg.sh`, `install.sh` and `uninstall.sh` each take an optional
`[OUTPUT_DIR]`, defaulting to `target/macos`, and the routers pass it through.

`dmg.sh` rebuilds via `bundle.sh` and wraps the result in a drag-install disk image.
It takes the version from the built `.app`'s `CFBundleShortVersionString` rather than
re-parsing `Cargo.toml` — one derivation, not two that can drift — and the architecture
from `uname -m` rather than assuming.

`packaging/macos/pipeline.sh` runs the build pipeline, deriving its step list from
`scripts/pipeline.steps`; `--list-steps` prints that derived list for diffing against the
Linux and Windows runners, and `--package` builds the `.dmg` as a pipeline step. Which
steps exist and how each is judged lives in the contract, deliberately not here.

**Pass an absolute path.** A bundle launched through `open` does not inherit the
shell's working directory, so a relative path resolves against `/` instead.
Measured: the app then opens a *blank new document* carrying the right filename
in the title bar — no error, nothing obviously wrong, just no content. Easy to
misread as a rendering failure in the bundle.

Structural counterpart of `packaging/windows/` (Inno Setup `.iss` + staging
script): a packaging subdirectory with its own README, invoked from a documented
build step. The mechanisms differ completely — `.app`/`Info.plist`/`.icns` and
Launch Services here, an installer and the registry there.

## Why a bundle exists at all

macOS reads an app's Dock, Cmd-Tab and Finder identity from
`Contents/Info.plist` — the icon from `CFBundleIconFile`, the name from
`CFBundleName`, the identity from `CFBundleIdentifier`. It does **not** consult
GTK's icon theme. So a bare Unix executable has no icon and no name of its own,
however complete the GTK-side icon work is: run straight from
`target/release/scribobulate` the Dock shows a generic "exec" placeholder
labelled `scribobulate`, and only a bundle changes that.

The two icon paths are genuinely separate and both are needed:

| Surface | Source | Fixed by |
|---|---|---|
| Dock, Cmd-Tab, Finder | `CFBundleIconFile` → `Contents/Resources/*.icns` | this bundle |
| About dialog logo, GTK window icon | GTK icon theme, by app-ID name | the app icon bundled into the GResource |

## What the bundle contains

```
Scribobulate.app/Contents/
├── Info.plist                    generated from Info.plist.in
├── MacOS/scribobulate            the release binary, copied in
├── Frameworks/
│   ├── lib*.dylib                the GTK closure — 43 files, load paths rewritten
│   └── libpixbufloader-*.so      the 13 gdk-pixbuf loaders, FLAT (see below)
├── Resources/
│   ├── scribobulate.icns         rasterized from the app-icon SVG, via iconutil
│   ├── loaders.cache             generated, naming the loaders above
│   ├── share/icons/{Adwaita,hicolor}/
│   ├── share/glib-2.0/schemas/   including gschemas.compiled
│   ├── LICENSE
│   └── THIRD-PARTY-LICENSES.md
└── _CodeSignature/               written by the ad-hoc codesign
```

**The loaders are flat in `Frameworks/`, and that is a constraint rather than a
preference.** `codesign --deep` treats any *directory* under `Frameworks/` as a nested
bundle and refuses one that is not — *"bundle format unrecognized, invalid, or unsuitable
/ In subcomponent: …/gdk-pixbuf-2.0"*. It then leaves the app **unsigned** and the kernel
kills it at launch with no output, which reads as a missing library and is not one.

**`share/gtksourceview-5` is deliberately absent.** The editor's language specs and style
schemes are not on disk in the Homebrew prefix at all — they are a GResource compiled into
`libgtksourceview-5.0.dylib`, which `Frameworks/` already carries. Staging that directory
would copy six RNG/DTD validators and a font nothing loads, and would look like the thing
that made highlighting work.

**The two notice files are a shipping obligation rather than documentation, and this is
the paragraph to read before editing `bundle.sh`.** Several crates this binary links
(MIT, BSD-2-Clause, BSD-3-Clause) require their notice to travel with every binary
distribution, and the About dialog tells the user in as many words that the full notices
are in `THIRD-PARTY-LICENSES.md` "in the distribution". Those two `cp` lines in
`bundle.sh` are what make that sentence true on macOS. Dropping either as redundant, or
assembling a bundle some other way and not carrying them, does not merely lose a file —
it falsifies a claim the running application makes about itself.

## Self-contained, and the one way it still is not

The bundle carries its own GTK runtime: **43 libraries in `Contents/Frameworks`**, plus
the **13 gdk-pixbuf loader modules** flat beside them, the Adwaita and hicolor icon
themes, and the GSettings schemas under `Contents/Resources/share`. It needs no Homebrew
on the machine that runs it. `packaging/macos/verify-selfcontained.sh` is the gate on that
claim and is run as part of every bundle build.

**"49 dylibs" counts load PATHS, not files.** The closure is 43 files reached by 49
distinct paths — eight basenames arrive through two Homebrew aliases each
(`/opt/homebrew/lib/X` and `/opt/homebrew/opt/<formula>/lib/X`). The earlier figure in
this file was the path count and overstated the file count by eight.

Two things are worth knowing about how that closure is computed, because both were
defects before they were features:

- **`@rpath` dependencies are followed.** Most Homebrew libraries reference their
  siblings as `@rpath/NAME` with an `LC_RPATH` of `@loader_path/../lib`. A walk that
  follows only absolute paths misses them *invisibly*, because `@rpath/NAME` reads as an
  internal reference whether or not the file behind it was ever staged.
- **The loaders seed the walk.** They are `dlopen`ed, so nothing in the executable's link
  graph mentions them — and neither, therefore, do their own dependencies. The SVG loader
  alone pulls in librsvg, which the application does not link.

### It is not notarized, and the recipient is told it is "damaged"

This is the remaining gap, and it is a deferred scope decision rather than an oversight.
`bundle.sh` signs ad-hoc (`codesign --sign -`), which is enough to launch on the machine
that built it. It is **not** a Developer ID signature and there is no notarization, so
Gatekeeper refuses the bundle anywhere else.

**What the recipient actually sees** — measured, with `com.apple.quarantine` set, which is
what a download or an AirDrop applies: macOS reports *"Scribobulate.app is damaged and
can't be opened. You should move it to the Trash."* `spctl -a -t exec` rejects the `.app`
and `spctl -a -t open` rejects the `.dmg` (`source=no usable signature`). **The app is not
damaged.** That sentence is Gatekeeper's wording for "signed by an identity I do not
trust", and it is actively misleading: it sends the user to the Trash when the issue is a
signing identity.

**The way in, which is an override of a security decision rather than a trick.** The
recipient is deliberately overruling their OS, and should know that is what they are
doing:

```bash
xattr -dr com.apple.quarantine /Applications/Scribobulate.app
```

or, without a terminal: right-click the app ▸ **Open** ▸ **Open**, which records a
per-app exception.

Closing this needs a Developer ID Application certificate (a paid Apple Developer
enrolment, bound to a legal identity), `codesign --options runtime`, `notarytool submit`
and `stapler staple`. None of that is an engineering decision. Until it lands, `bundle.sh`
prints this limitation at the end of every successful build, so whoever holds the artefact
meets it before a recipient does — and when notarization does land, that warning must come
out in the same change, since an artefact that is notarized and still says it is not is
the same defect pointed the other way.

**So step 10's intent is partially met.** Self-containment: met. *An artefact a
non-developer can install with no toolchain*: met only with the override above documented,
which is why it is documented here rather than in a commit message.

Nothing here needs the GSK renderer to be configured: `scribobulate::run`
(`src/lib.rs`, which `main()` delegates to) sets `GSK_RENDERER=cairo` in-process
before GTK initialises, so the bundle inherits it without a wrapper script. (`.cargo/config.toml`'s copy of that setting only covers `cargo`
invocations and does **not** reach a bundled app.)

## Default-handler registration

`Info.plist.in` declares the Markdown document type, which is what puts
Scribobulate in **Open With**. Becoming the *default* handler is a user-level
action, with no first-party CLI equivalent to Linux's `xdg-mime default`:

```bash
brew install duti
duti -s com.extollit.scribobulate net.daringfireball.markdown all
```

## Putting `scribobulate` on PATH, and taking it off again

```bash
./install.sh                 # -> target/macos/Scribobulate.app, plus a symlink on PATH
scribobulate path/to/document.md
./uninstall.sh               # removes both
```

`./install.sh` at the repository root is the canonical entry point on every
platform — a `uname -s` router holding no install logic, which dispatches here.
Running `packaging/macos/install.sh` directly is equivalent and is the form to use
when you are already working inside this directory. The router is named first
deliberately: documenting the direct path as primary is how the router came to be
unmentioned in every README that existed.

Builds the bundle (via `bundle.sh`) and symlinks its own executable —
`Scribobulate.app/Contents/MacOS/scribobulate` — into Homebrew's `bin/`
directory, which this project already requires for its GTK4 dependencies and
which is therefore already writable with no `sudo` and already on PATH. The
symlink stays inside `Contents/MacOS/`, so a terminal launch runs the identical
executable Finder or the Dock would, Dock/Cmd-Tab identity included — a copy
made outside the bundle would not carry that.

This is the developer-convenience counterpart to `packaging/linux/install.sh` on Linux,
not the redistributable installer — that is `dmg.sh` above.

`uninstall.sh` undoes exactly that and nothing more. It removes the Homebrew symlink and
the bundle, and it is idempotent, so a second run reports what is already gone instead of
failing. Two behaviours are worth knowing before you need them:

- **The symlink goes only if the path really is a symlink.** `install.sh` never creates
  anything else there, so a regular file sitting at `$(brew --prefix)/bin/scribobulate`
  came from somewhere else; it is left alone with a warning rather than deleted.
- **A copy dragged to `/Applications` is reported, never removed.** That copy comes from
  the `.dmg`, installed by the user rather than by this script. The run says it is there
  and how to remove it, because an uninstaller that prints success while a working copy
  of the app is still installed is worse than one that fails.

**The symlink is the part nobody guesses**, and it is the reason this section exists. It
lives outside the repository, in `$(brew --prefix)/bin`, so deleting the checkout without
running `./uninstall.sh` first leaves a dangling `scribobulate` on PATH that fails with
nothing useful to say. Uninstall before you remove the clone.

Launch Services may go on offering a bundle that is gone, because `bundle.sh` registers
every build with it (`lsregister -f`) so the Dock and Finder take the icon up without a
cache delay. It corrects itself on the next rescan, or immediately with the
`lsregister -kill -r -domain local -domain user` command `uninstall.sh` prints.

## Verifying a bundle

```bash
plutil -lint target/macos/Scribobulate.app/Contents/Info.plist
codesign -dv target/macos/Scribobulate.app
open target/macos/Scribobulate.app --args -n "$PWD/path/to/document.md"
lsappinfo info -only bundleid,name -app "$(pgrep -f Scribobulate.app | head -1)"
```

The path is absolute here for the reason given above, and this recipe used to get that
wrong — a verification step that silently opens a blank document is worse than no
verification step. The `-n` is the *application's* own `--new-instance` flag, passed
through `--args` on purpose so the check runs in a fresh process rather than being
absorbed by the single-instance handler; it is not `open`'s own `-n`.

`lsappinfo` reporting `CFBundleIdentifier=com.extollit.scribobulate` and
`LSDisplayName=Scribobulate` (rather than a null identifier and a lowercase
name) is the machine-checkable signal that the bundle took. The icon itself
needs eyes on the Dock — and an unlocked screen, since a locked one silently
sends synthetic clicks to the login window (ScrAP-172).
