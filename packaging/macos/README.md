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

`adwaita-icon-theme` is named explicitly because Homebrew's `gtk4` does not pull in
an icon theme. Without it the build succeeds and roughly half the toolbar renders as
broken-image placeholders.

Build on a Mac. Cross-compiling from Linux does not work: with the
`x86_64-apple-darwin` target installed, `cargo check --target` fails inside `gtk4-sys`
on pkg-config cross-compilation. A successful native build says nothing about the
cross one, so only a macOS machine can produce or check a macOS artefact.

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
claim, and `bundle.sh` runs it on every build, after signing.

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

### librsvg has a security floor

The bundled librsvg must be **2.63.2 or newer** (CVE-2026-96889, a use-after-free reachable
from any SVG through the pixbuf loader). The app's XInclude screen stands down when it runs
from the bundle, because the bundle carries the fix, so `bundle.sh` enforces it (a binary run
outside the bundle, such as `./target/release/scribobulate`, loads Homebrew's librsvg and is
screened like Linux): after signing,
`packaging/macos/verify-librsvg.sh` loads the **staged** dylib, reads its runtime version
and fails the build below the floor. It reads the artefact, not Homebrew: `brew upgrade`
leaves the old keg in the Cellar, and only the staged copy is delivered.

The staged copy is only the one loaded if **no `LC_RPATH` in the bundle points outside it**.
dyld tries a library's own rpaths before the executable's, and Homebrew's came along with
the copies: the SVG loader's `@loader_path/../../..` named the directory holding the `.app`,
so a `librsvg-2.2.dylib` placed beside the bundle was loaded instead of the staged one
(measured with `DYLD_PRINT_LIBRARIES`). `bundle.sh` deletes every such rpath, and
`verify-librsvg.sh` (given a `.app`) and condition 1 of `verify-selfcontained.sh` fail a
bundle that still has one. The resolver is `packaging/macos/rpath.sh`, and every reader in
it fails closed: a tool that fails or prints nothing fails the bundle. The app checks the
same thing at run time, from dyld's own list of loaded images: its screen stands down only
when the librsvg the SVG loader bound is the staged file
(`platform::mac::bundle::carries_verified_librsvg`).

If the build stops there, run `brew upgrade librsvg` and build again. The gate runs on
its own as well: `verify-librsvg.sh <.app | librsvg-2.2.dylib>`, and `--self-test`.

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

**So step 9's intent is partially met.** Self-containment: met. *An artefact a
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
./install.sh                 # -> ~/Applications/Scribobulate.app, plus a symlink on PATH
sudo ./install.sh            # -> /Applications/Scribobulate.app, same symlink on PATH
scribobulate path/to/document.md
./uninstall.sh               # removes what the matching mode installed
sudo ./uninstall.sh          # ditto, for a global install
```

**The mode is the invoking UID, and it decides where everything goes.**

| | app | CLI | manual pages |
|---|---|---|---|
| `./install.sh` | `~/Applications` | `~/.local/bin` | `~/.local/share/man` |
| `sudo ./install.sh` | `/Applications` | `/usr/local/bin` | `/usr/local/share/man` |

**Nothing is installed into Homebrew's prefix.** It used to be — the symlinks went into
`$(brew --prefix)/bin` and `share/man`, justified as the one directory already writable
and already on PATH. That justification died with the global mode, and the cost was
always real: files Homebrew did not install, inside the prefix it manages, are unbrewed
files `brew doctor` reports, and they tie this project's install to a package manager you
may relocate or remove. Homebrew stays a **build** dependency — `bundle.sh` copies the
GTK closure out of `/opt/homebrew` — and a build dependency is not an install
destination.

`/usr/local/bin` is on the stock search path and this was measured, not assumed:
`/etc/paths` ships it as its *first* line, ahead of `/usr/bin`, and `path_helper`
composes it into `PATH` before any shell profile runs. `/etc/manpaths` ships
`/usr/local/share/man` likewise. Both hold on Apple silicon. Note `manpath` omits
directories that do not exist, so `/usr/local/share/man` is absent from its output until
the install creates it.

`~/.local/*` is a judgement call, not a convention — macOS defines no per-user `bin`
directory at all. It matches `packaging/linux/install.sh`, which is one fewer difference
between the platforms. Neither per-user directory is searched by default, so the
per-user install **says so and prints the line to add**. That note is the honest option;
writing into another project's prefix to avoid printing it was not.

**Which `Applications` folder matters more than it sounds.**
Use `sudo` when you want the app in the `Applications` that Finder's sidebar shows —
that is `/Applications`, and only `/Applications`. `~/Applications` is a different
folder, reachable only through your home directory, and this trips people up: a per-user
install succeeds, registers with Launch Services, works from Spotlight and the Dock, and
still reads as having silently done nothing, because the folder you go and look in is not
the folder it installed to. Both anchors are otherwise equivalent — same Dock tile, same
Cmd-Tab entry, same `scribobulate` on PATH.

**Uninstall in the mode you installed with.** `packaging/macos/mode.sh` resolves the
anchor for both scripts from that single rule, so they cannot disagree about where the
bundle went; but a per-user uninstall does not go looking in `/Applications`, and each
run names the other mode's bundle and the exact command that removes it.

`./install.sh` at the repository root is the canonical entry point on every
platform — a `uname -s` router holding no install logic, which dispatches here.
Running `packaging/macos/install.sh` directly is equivalent and is the form to use
when you are already working inside this directory. The router is named first
deliberately: documenting the direct path as primary is how the router came to be
unmentioned in every README that existed.

Builds the bundle (via `bundle.sh`), copies it to the mode's anchor, and symlinks that
copy's own executable — `Scribobulate.app/Contents/MacOS/scribobulate`
— into that mode's `bin` directory. The symlink stays inside `Contents/MacOS/`, so a terminal launch runs the identical
executable Finder or the Dock would, Dock/Cmd-Tab identity included — a copy
made outside the bundle would not carry that.

**Nothing resolves into `target/`, and that is deliberate.** The links used to point at
the build directory, so `cargo clean` — or a Finder drag to `/Applications`, which within
one volume is a move rather than a copy — left them dangling. A dangling symlink is not
an error the shell reports: it skips the entry and keeps walking PATH, so the next
`scribobulate` down the line runs instead, silently. Launch Services scans both anchors,
so either way the copy has a real Dock tile and Cmd-Tab entry. The build copy is removed once the anchor is verified, because Launch
Services registers a bundle in a build directory like any other and two registrations for
one `CFBundleIdentifier` is the state these scripts exist to prevent.

**The install refuses rather than adding a second copy.** Before it spends a release
build it checks that no other `Scribobulate.app` is installed and that nothing earlier on
PATH already answers to `scribobulate`; either one is a hard error naming the path and
the exact command to clear it. It reports and refuses — it never deletes a bundle it did
not install. Each mode treats the OTHER mode's anchor as foreign, so running both is
refused rather than silently producing two bundles under one `CFBundleIdentifier`; the
refusal names the other mode's `uninstall.sh` as the fix, which is one command and also
clears the Launch Services registration that a bare `rm -rf` would strand.

The one thing it does replace is its own anchor, because overwriting the destination is
what installing means — and it says so when something was already there, since in global
mode that something is plausibly a copy you dragged from the `.dmg`.

This is the developer-convenience counterpart to `packaging/linux/install.sh` on Linux,
not the redistributable installer — that is `dmg.sh` above.

`uninstall.sh` undoes exactly that and nothing more. It removes the Homebrew symlink and
the bundle, and it is idempotent, so a second run reports what is already gone instead of
failing. Two behaviours are worth knowing before you need them:

- **The symlink goes only if the path really is a symlink.** `install.sh` never creates
  anything else there, so a regular file sitting at that path came from somewhere else;
  it is left alone with a warning rather than deleted.
- **A link is removed only if it resolves into this mode's bundle**, and there are three
  answers, not two. Into the anchor: ours, removed. Into the other mode's bundle: left,
  with that mode's `uninstall.sh` named. Into *neither* — typically a leftover from when
  the install linked into `target/` — no uninstall owns it, so the run prints `rm -f`.
  Handing over a command that runs, succeeds and removes nothing is worse than handing
  over none.
- **The other mode's bundle is reported, never removed.** It belongs to a different run,
  and the uninstall says it is there and gives the command that removes it, because an
  uninstaller that prints success while a working copy of the app is still installed is
  worse than one that fails.
- **In global mode, `/Applications/Scribobulate.app` is removed only if the PATH symlink
  resolves into it.** This used to be free: while the developer install could only anchor
  at `~/Applications` and the `.dmg` only landed in `/Applications`, the location alone
  said who put a bundle there. Global mode ends that, so authorship is *tested* instead —
  `install.sh` creates that symlink and nothing else does, so a link aimed into the bundle
  is proof rather than inference, and a `.dmg` copy has none aimed at it and survives.
  Not proven means not removed.
- **Every removal is verified, not just attempted.** The run re-tests each path afterwards
  and fails loudly if it is still there, rather than printing `:: Removing …` and exiting
  `0` on a removal that did not happen.

**Both halves of the install live outside the repository**, and that is the part nobody
guesses: the symlink in the mode's `bin` directory and the bundle in its `Applications`
folder.
Deleting the checkout removes neither, so run `./uninstall.sh` before you remove the
clone or both outlive it — the bundle keeps its Dock and `Open With` entries, and the
symlink goes on resolving into it. (It no longer *dangles* when the checkout goes, which
is what the anchor bought; it is simply still installed.)

Launch Services may go on offering a bundle that is gone, because `bundle.sh` registers
every build with it (`lsregister -f`) so the Dock and Finder take the icon up without a
cache delay. `uninstall.sh` now unregisters the paths it installed rather than telling you
to do it, and it **verifies that by reading the database back** rather than trusting the
exit status — `lsregister -u` returns non-zero for a path it holds no registration for,
which is the ordinary case, so the status answers a different question than the one being
asked. When a path is still registered afterwards, or `lsregister` is not where it is
expected, the run says which paths were not cleared instead of claiming they were. For a
copy installed some other way, unregister it **by path**:

```bash
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u '/path/to/Scribobulate.app'
```

That works even when the path has already been deleted — measured against 20 such
registrations, all cleared.

⚠️ **Do not reach for `lsregister -kill`.** This README and `uninstall.sh` both used to
recommend `lsregister -kill -r -domain local -domain user`; the option has been REMOVED.
MEASURED on macOS 26 (Darwin 25.0.5): it answers *"The -kill option has been removed
because it was dangerous and no longer useful"* and changes nothing — 20 stale
registrations before, the same 20 after. A remedy that no-ops is worse than none, because
the reader has no reason to doubt it. `-f` and `-u` are both still present and both still
work; `-f` was re-verified functionally on the same version.

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
sends synthetic clicks to the login window (GEP-12).
