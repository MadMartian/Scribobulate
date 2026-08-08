#!/usr/bin/env bash
#
# Build a .deb an ordinary user can install with no Rust toolchain and no GTK
# development packages.
#
# This is NOT install.sh. That script builds from source into ~/.local and is a
# DEVELOPER convenience — it needs cargo and the -dev libraries, so it cannot be handed
# to anyone. This produces a transferable artefact, which is the point
# no toolchain, which install.sh cannot be.
#
# The payload — what goes where — is defined ONCE in payload.sh and shared with
# build-rpm.sh. Only Debian's metadata lives here.
#
# Usage:
#   packaging/linux/build-deb.sh            # build (expects a release binary)
#   packaging/linux/build-deb.sh --build    # cargo build --release first
#
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
# shellcheck source=packaging/linux/payload.sh
. "$repo/packaging/linux/payload.sh"

VERSION="$(read_version)"
[ -n "$VERSION" ] || { echo "build-deb: could not read version from Cargo.toml" >&2; exit 1; }

ARCH="$(dpkg --print-architecture)"
BIN="target/release/$PKG"

[ "${1:-}" = "--build" ] && cargo build --release
require_fresh_binary "$BIN"

GLIBC_MIN="$(glibc_floor "$BIN")"
[ -n "$GLIBC_MIN" ] || { echo "build-deb: could not derive a libc floor from $BIN" >&2; exit 1; }

out="target/deb"
stage="$out/${PKG}_${VERSION}_${ARCH}"
rm -rf "$stage"
mkdir -p "$stage/DEBIAN" "$stage/usr/share/doc/$PKG"

stage_payload "$stage" "$BIN" "$VERSION"

# Debian copyright: REFERENCE the common licence, never embed it. Policy requires
# /usr/share/common-licenses/Apache-2.0 be cited rather than copied — a package
# shipping its own copy of a common licence is both larger and harder to audit.
cat > "$stage/usr/share/doc/$PKG/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: $PKG

Files: *
Copyright: $(date +%Y) extollIT Enterprises <sales@extollit.com>
License: Apache-2.0
 On Debian systems the full text of the Apache License version 2.0 can be
 found in /usr/share/common-licenses/Apache-2.0
EOF

# Mandatory for a native package. `-9n` keeps the artefact reproducible.
cat > "$stage/usr/share/doc/$PKG/changelog" <<EOF
$PKG ($VERSION) unstable; urgency=medium

  * Packaged release.

 -- extollIT Enterprises <sales@extollit.com>  $(date -R)
EOF
gzip -9n "$stage/usr/share/doc/$PKG/changelog"
chmod 644 "$stage/usr/share/doc/$PKG/copyright" "$stage/usr/share/doc/$PKG/changelog.gz"
find "$stage" -type d -exec chmod 755 {} +

# RUNTIME DEPENDENCIES ARE DECLARED, NOT DISCOVERED. dpkg-shlibdeps would compute them
# but needs a debian/ source tree we deliberately do not carry. This is the SHORT list —
# the direct toolkit dependencies whose absence is the realistic failure; their own
# chains pull in glib, pango, cairo and gdk-pixbuf, so naming those adds noise without
# adding protection. The libc floor is derived, not written down. Re-derive from
# `ldd target/release/scribobulate` if the toolkit set changes.
#
# NO ICON THEME DEPENDENCY, deliberately rather than by oversight: the app bundles its
# own symbolic icons in its GResource and renders correctly with no icon theme present.
# Declaring adwaita-icon-theme would claim a requirement we do not have.
cat > "$stage/DEBIAN/control" <<EOF
Package: $PKG
Version: $VERSION
Section: editors
Priority: optional
Architecture: $ARCH
Depends: libc6 (>= $GLIBC_MIN), libgtk-4-1 (>= 4.6), libgtksourceview-5-0
Installed-Size: $(du -ks "$stage" | cut -f1)
Maintainer: extollIT Enterprises <sales@extollit.com>
Description: Native Markdown viewer and editor that renders on the CPU
 Scribobulate renders Markdown into native GTK4 widgets and forces the GSK
 Cairo software renderer, so it holds no GL context and no VRAM. It offers
 live reload, split editing, an outline, reading themes and annotation
 review.
EOF

# Cache refreshes, best-effort: a missing tool is not a failed install, and `|| true`
# keeps a container or minimal system from failing the package over a cache it lacks.
for script in postinst postrm; do
    cat > "$stage/DEBIAN/$script" <<'EOF'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache   >/dev/null 2>&1 && gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
exit 0
EOF
    chmod 755 "$stage/DEBIAN/$script"
done

# fakeroot so the payload is owned by root:root. Without it every file carries the
# building user's uid, which dpkg installs verbatim — the package "works" on the
# builder's machine and ships wrong ownership everywhere else.
deb="$out/${PKG}_${VERSION}_${ARCH}.deb"
if command -v fakeroot >/dev/null 2>&1; then
    fakeroot dpkg-deb --build "$stage" "$deb" >/dev/null
else
    echo "build-deb: fakeroot not found — payload will carry the building user's ownership" >&2
    dpkg-deb --build "$stage" "$deb" >/dev/null
fi

echo "built $deb"
