# Linux packaging

**Three ways to install on Linux**, all defined here. Two produce a transferable
artefact for someone with no Rust toolchain and no GTK development packages; the third
builds from source into `~/.local` and needs both.

```bash
packaging/linux/build-deb.sh          # -> target/deb/scribobulate_<version>_<arch>.deb
packaging/linux/build-rpm.sh          # -> target/rpm/RPMS/<arch>/scribobulate-<version>-1.<arch>.rpm
packaging/linux/build-deb.sh --build  # cargo build --release first

packaging/linux/install.sh            # build from source -> ~/.local, no root
packaging/linux/install.sh --no-build # install an existing release binary
```

The two builders are also step 10 of the build pipeline (`scripts/pipeline.sh
--package`), which is opt-in — see `scripts/pipeline.steps`. `install.sh` is not part
of any gate: it is a developer convenience, and the property step 10 defends is
precisely the one it cannot demonstrate.

## `payload.sh` is the single definition — add there, not to a route

`payload.sh` says what gets installed and where, and **all three routes read it**.
Only the per-format metadata differs (Debian needs a referenced-common-licence
copyright file and a changelog; rpm carries its licence and summary in the spec
header; `install.sh` adds the MIME registration and the cache refresh a package's
`postinst` would do). Written more than once, the layouts drift — and a drifted layout
is invisible, because each route installs cleanly on its own and nothing compares them.

**One layout, two anchors.** `stage_payload` takes a prefix: the packages pass `usr`
and write `<root>/usr/share/…`, while `install.sh` passes an empty prefix with
`root=~/.local` and writes `~/.local/share/…`. That works because XDG's user tree is
deliberately shaped like `/usr`, so this is one payload relocated rather than two
payloads that happen to resemble each other. Add a file to `stage_payload` and every
route gains it in the same change.

**The one thing that is NOT derived from it is `uninstall.sh`**, which must be updated
by hand whenever `stage_payload` gains a file — there is no reverse direction to a
payload definition, because the packages get their removal from dpkg/rpm's own file
manifests and only the user-local route needs a remover. Its removal list is written in
the same order as `stage_payload`'s installs so the two can be read side by side.

## `install.sh` used to live at the repo root

It was moved here, and this file previously argued it should not be — on the grounds
that it is "not this": a from-source developer install rather than a transferable
artefact. That distinction is real and still holds; what it does not support is a
*location*. The script installs Linux software into Linux paths using the same payload
the packages use, so the directory that owns Linux installation is where it belongs, and
keeping it at the root advertised a difference in kind that was really a difference in
audience. Sharing `payload.sh` is what makes the point concrete: the three routes now
cannot disagree about what an installed Scribobulate consists of.

## Why `.deb` and `.rpm`, and not AppImage or Flatpak

**This was decided, not defaulted.** The alternatives and their real trade-offs:

| | |
|---|---|
| **AppImage** | Single file, bundles GTK, needs no root — but ships a second copy of the toolkit and sidesteps the distribution's own security updates. |
| **Flatpak** | Sandboxed and handles the GTK runtime, but needs a manifest and a portal story for file access, which this app's whole job (opening arbitrary Markdown files and watching them change) makes non-trivial. |
| **`.deb` / `.rpm`** | Native, and the user's package manager handles updates and dependencies. Costs one package per distribution family, and each inherits the host's GTK version. |

The deciding question was *who the Linux user is*, and the answer taken was
someone on a mainstream distribution who expects `apt install` or `dnf install`
to work and their system GTK to be used. Inheriting the host's GTK is a real
constraint that follows from that — the app targets GTK 4.6+, so a distribution
older than that is out of scope rather than accommodated by bundling.

`install.sh` at the repo root is **not** this and should stay as it is: a
from-source developer install into `~/.local`, requiring cargo and the `-dev`
libraries.

## Notes

- **Version comes from `Cargo.toml`**, parsed once in `payload.sh`. Never write a
  version anywhere in this directory.
- **The libc floor is derived** from the built binary's own `GLIBC_` symbol
  versions, not hard-coded, so it cannot go stale when the toolchain moves.
- **The deb is lintian-clean**, and it is worth keeping that way — it caught four
  real defects that would otherwise have shipped, including directories inheriting
  the building user's umask into `/usr`.
- **The rpm is well-formed but untested.** It builds on Debian/Ubuntu with the
  `rpm` package installed; no dependency here has been resolved against a real
  Fedora or openSUSE repository and the package has never been installed on one.
  Treat a successful build as "the artefact is well-formed", not "it installs".
