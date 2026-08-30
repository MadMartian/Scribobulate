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
├── Resources/
│   ├── scribobulate.icns         rasterized from the app-icon SVG, via iconutil
│   ├── LICENSE
│   └── THIRD-PARTY-LICENSES.md
└── _CodeSignature/               written by the ad-hoc codesign
```

**The two notice files are a shipping obligation rather than documentation, and this is
the paragraph to read before editing `bundle.sh`.** Several crates this binary links
(MIT, BSD-2-Clause, BSD-3-Clause) require their notice to travel with every binary
distribution, and the About dialog tells the user in as many words that the full notices
are in `THIRD-PARTY-LICENSES.md` "in the distribution". Those two `cp` lines in
`bundle.sh` are what make that sentence true on macOS. Dropping either as redundant, or
assembling a bundle some other way and not carrying them, does not merely lose a file —
it falsifies a claim the running application makes about itself.

## What this bundle is not: a redistributable

It runs on a machine that already has the Homebrew dependencies. It is **not**
self-contained, and handing it to someone without Homebrew GTK will fail. The
dylib counts/sizes and the icon-theme row below are measured on a real machine;
the schema and signing rows are reasoned from how those libraries behave and
have **not** been reproduced — they are leads to verify while closing the gap,
not findings to quote.

| Gap | Detail | Cost |
|---|---|---|
| **Dylibs** | The binary links Homebrew paths directly (`otool -L`); the transitive closure is **49 dylibs, ~35 MB**. A self-contained bundle copies them into `Contents/Frameworks` and rewrites every load path (`install_name_tool -change` / `-add_rpath @executable_path/../Frameworks`, or `dylibbundler`). | ~35 MB |
| **Icon theme** | GTK finds Adwaita via `XDG_DATA_DIRS`, which points into `/opt/homebrew`. **Measured**: with that path removed, 11 icon names fall back to the broken-image placeholder — i.e. the packaged app reproduces the exact defect the icon audit exists to catch. Stage the theme into `Contents/Resources/share/icons` and point `XDG_DATA_DIRS` there. | ~15 MB |
| **GLib schemas** | GTK4 defines `org.gtk.gtk4.Settings.FileChooser` and friends (confirmed), and GLib aborts on a missing schema — so a bundle without `gschemas.compiled` is *expected* to die when a file dialog opens. Not yet reproduced. Stage it into `Contents/Resources/share/glib-2.0/schemas`. | ~3 KB |
| **gdk-pixbuf loaders** | Non-native image formats resolve through the loader cache of the *process*, not the file. Stage `lib/gdk-pixbuf-2.0/**` and set `GDK_PIXBUF_MODULE_FILE` if the packaged app must render WebP/AVIF. | small |
| **Signing** | `bundle.sh` signs ad-hoc (`codesign --sign -`), which is enough to launch locally. Distribution needs a Developer ID certificate, `--options runtime`, and notarization. **Gatekeeper refuses an ad-hoc-signed `.dmg` that carries a quarantine flag — measured, not assumed:** with `com.apple.quarantine` set, `spctl -a -t open` rejects the image (`source=no usable signature`) and `spctl -a -t exec` rejects the `.app` (exit 3). That was tested on the machine that *built* it, which is the case most likely to be allowed — so anyone who downloads it will be refused too, and must right-click ▸ Open or clear the attribute. The `.dmg` is a transfer format here, not a distribution channel. | — |

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
