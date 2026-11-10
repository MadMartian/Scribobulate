# Plan: Distribution channels beyond GitHub

## Problem

The only way to obtain a release is to download an installer from the GitHub release
page ([RELEASE.md](RELEASE.md)). That is enough to cut the first release, and it is
how every artefact will continue to be *produced*; but it is not how most users on any
of the three platforms expect to find and update software. A macOS user looks in
Homebrew, a Windows user in winget or the Store, a Linux user in a distribution
repository or Flathub. Each channel is a second place the same bytes have to be
registered and kept current, and each one forgotten after a release is a stale version
with the project's name on it.

Nothing here blocks a release. The plan exists so the question has a home outside the
release procedure, and so that "which channels, in what order, and what each costs
per release" is decided once rather than re-asked at every cut.

### Root cause

No channel has been set up because none was needed before a release existed. Every
channel also carries a per-release maintenance cost that the single-maintainer model
has to absorb, and some (signing, notarisation, Store submission) carry prerequisites
the project has not met (POLICY § Artefact signing).

## Previously attempted

Nothing. `packaging/macos/homebrew.sh` is unrelated: it locates the Homebrew *build*
dependencies the bundle is staged from, not a tap that would distribute the bundle.

## Possible approaches

Each is a channel, not an alternative to the others; the decision is which to adopt and
when. Every one of them except Flathub re-publishes the GitHub release's asset — none
builds anything of its own — so each needs the asset's SHA-256. RELEASE.md publishes
no checksum file by decision; GitHub's per-asset digest serves a human reading the
release page, but a manifest needs the digest as data, so the first channel adopted
brings with it a `SHA256SUMS` emitted by the packaging run and attached beside the
installers. That file is this plan's deliverable, not the release procedure's.

### ✅ 1. GitHub releases only, for the first release

The release page is the sole channel; README's install section links the three assets.

**Pros**: zero additional work; the artefact set is already the release's contents by
construction; no second place to go stale.
**Cons**: no discoverability; users update by hand; unsigned installers draw a warning
on macOS and Windows with no channel-side trust to offset it.

### 💡 2. Homebrew cask (own tap)

A `Casks/scribobulate.rb` in a `MadMartian/homebrew-scribobulate` tap, pointing at the
release `.dmg` by URL and SHA-256. Homebrew core's cask repository has acceptance
criteria (notability, a signed/notarised app in practice) the project does not yet
meet; an own tap has none.

**Pros**: the install a macOS developer expects (`brew install --cask`); `brew upgrade`
follows new versions; a cask is a few lines and can be bumped by a script in the
release's wake.
**Cons**: one more repository to keep; Gatekeeper still warns on an unsigned app, which
the cask cannot suppress; a tap is not discoverable the way core is.

### 💡 3. winget manifest

A manifest under `manifests/m/MadMartian/Scribobulate/<version>/` in
`microsoft/winget-pkgs`, pointing at the release `-setup.exe` by URL and SHA-256.
Submitted as a pull request per version (`wingetcreate` automates it).

**Pros**: the Windows install a developer expects, with `winget upgrade`; no repository
of our own; the community repository accepts unsigned installers.
**Cons**: each version is a PR reviewed by a bot and sometimes a person, so publication
lags the release; SmartScreen still warns on first run; the manifest must match the
installer's exact product and version strings from `scribobulate.iss`.

### 💡 4. Flathub

A Flatpak manifest building from the release tag against the GNOME runtime, submitted to
Flathub and then maintained in its own repository under the Flathub organisation.

**Pros**: the Linux desktop's primary discovery channel; the runtime brings GTK and
librsvg at a version the manifest pins, so the librsvg floor is met by construction;
reaches every distribution at once.
**Cons**: Flathub builds from source, so this is the one channel that does not
re-publish the GitHub asset — a fourth build whose output no gate here has verified;
the sandbox changes file-monitor and portal behaviour that live reload, rename handling
and the file chooser depend on, which is untested; the single-instance and multi-window
model needs checking under the portal; the submission review is substantive.

### 💡 5. Distribution repositories (apt, rpm)

A hosted apt repository and an rpm repository carrying the release's `.deb` and
`.rpm` (GitHub Pages with signed indexes, or OBS/COPR/PPA, which build from source).

**Pros**: `apt upgrade` and `dnf upgrade` follow new versions; the packages already
exist.
**Cons**: repository metadata must be GPG-signed to be installable without a warning,
and the project has no signing identity; the alternatives that build from source
(PPA, COPR) are the Flathub objection again; a repository per distribution family is
the highest ongoing cost on the list for the smallest audience gain.

### ❌ 6. Microsoft Store, Mac App Store, Snap

All three require a signing or developer identity and a store-side review; Snap
additionally confines GTK apps in ways the project has not examined. Ruled out until
POLICY § Artefact signing changes, at which point 2 and 3 are still the cheaper route.

## Recommendation

Approach 1 for the first release. Then 2 and 3, in that order, because each is a
manifest pointing at the asset the release already produced — no new build, no new
gate — and each can be bumped by a script run in the release's wake (RELEASE.md gate
13). Adopting either turns RELEASE.md's checksum decision into a settled one: the
manifest needs the asset's SHA-256, so the packaging run should emit it.

Flathub (4) is the channel with the largest Linux audience and the largest unknowns;
it is worth a spike — build the manifest, run `tests/MANUAL-TEST.md`'s live-reload,
rename and file-chooser items inside the sandbox — before it is promised. Repositories
(5) wait on a signing identity.

## Proposed TDD Rubrics

To enter TDD.md when the first channel beyond GitHub is delivered.

### A channel offers the release's own bytes

- **Given** a distribution channel the project has adopted
- **When** a release is published
- **Then** what the channel installs is byte-identical to the asset attached to that
  GitHub release, and reports the same version in the application's About dialog
- **And** the channel never offers a version the GitHub release page does not

### A channel install satisfies the same obligations as a direct one

- **Given** an installation made through any adopted channel
- **When** the user opens the About dialog's licence notices, or uninstalls through the
  channel
- **Then** the third-party notices are present where the dialog says they are, and the
  uninstall leaves nothing the install put there — the same checks a direct install
  passes (RELEASE.md gates 6 and 9)

### A release updates every adopted channel

- **Given** more than one adopted channel
- **When** a release is cut
- **Then** every channel is updated in that release's wake, from the published asset,
  and the release is not considered complete until each has been

## Technical details preserved

- A Homebrew cask and a winget manifest both key on the asset URL and SHA-256; neither
  can be written before the packaging run has attached the asset, so the bump is a
  post-publish step by construction.
- winget's manifest validates the installer's `ProductCode` and version against what
  Inno Setup wrote; those come from `packaging/windows/scribobulate.iss`, which reads
  the version from `Cargo.toml`, so the manifest's version is the Cargo version.
- `LSMinimumSystemVersion` in `packaging/macos/Info.plist.in` is the cask's
  `depends_on macos:` floor; the two must agree, and the plist is the owner.
