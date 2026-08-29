# Linux packaging

Two artefacts, both installable by someone with no Rust toolchain and no GTK
development packages:

```bash
packaging/linux/build-deb.sh          # -> target/deb/scribobulate_<version>_<arch>.deb
packaging/linux/build-rpm.sh          # -> target/rpm/RPMS/<arch>/scribobulate-<version>-1.<arch>.rpm
packaging/linux/build-deb.sh --build  # cargo build --release first
```

Both are also step 10 of the build pipeline (`scripts/pipeline.sh --package`),
which is opt-in — see `scripts/pipeline.steps`.

## `payload.sh` is the single definition — add there, not to a builder

`payload.sh` says what gets installed and where. Both builders read it, and only
the per-format metadata differs (Debian needs a referenced-common-licence
copyright file and a changelog; rpm carries its licence and summary in the spec
header). Written twice, the two layouts drift — and a drifted layout is invisible,
because each package installs cleanly on its own and nothing compares them.

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

`install.sh` in this directory is **not** this and should stay as it is: a
from-source developer install into `~/.local`, requiring cargo and the `-dev`
libraries. `./install.sh` at the repo root reaches it — that file is a `uname -s`
router holding no install logic of its own, so each platform's answer lives in its
own `packaging/<os>/` directory rather than in a shared script's branches.

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
