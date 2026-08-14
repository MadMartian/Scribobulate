#!/usr/bin/env bash
#
# Build an .rpm an ordinary user can install with no Rust toolchain and no GTK
# development packages. Companion to build-deb.sh; the payload is defined once in
# payload.sh and shared, so the two packages cannot drift in what they install.
#
# BUILT FROM A PRE-STAGED BUILDROOT, not from a %build section. The spec below has no
# %prep or %build: the binary is already compiled by the pipeline's step 3, and having
# rpmbuild compile it again would mean the packaged artefact is not the one the gates
# ran against. `%install` is a copy of the staged tree, nothing more.
#
# DEPENDENCY NAMES ARE RPM-WORLD NAMES. `gtk4` and `gtksourceview5` are what Fedora and
# openSUSE call these; the deb's `libgtk-4-1`/`libgtksourceview-5-0` are Debian names
# for the same libraries. This is the one place the two packages legitimately differ in
# substance rather than in format, which is why it is not in payload.sh.
#
# CROSS-BUILDING CAVEAT, stated because it is easy to over-claim: this can be built on
# Debian/Ubuntu with the `rpm` package installed, and the result is a well-formed rpm.
# That is NOT the same as having been tested on an RPM distribution — no dependency here
# has been resolved against a real Fedora/openSUSE repository, and the package has never
# been installed on one. Treat a successful build as "the artefact is well-formed", not
# as "the artefact installs".
#
# Usage:
#   packaging/linux/build-rpm.sh            # build (expects a release binary)
#   packaging/linux/build-rpm.sh --build    # cargo build --release first
#
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
# shellcheck source=packaging/linux/payload.sh
. "$repo/packaging/linux/payload.sh"

command -v rpmbuild >/dev/null 2>&1 || {
    echo "build-rpm: rpmbuild not found. On Debian/Ubuntu: apt install rpm" >&2
    exit 1
}

VERSION="$(read_version)"
[ -n "$VERSION" ] || { echo "build-rpm: could not read version from Cargo.toml" >&2; exit 1; }

BIN="target/release/$PKG"
[ "${1:-}" = "--build" ] && cargo build --release
require_fresh_binary "$BIN"

# rpm's arch names differ from dpkg's; ask uname rather than translating dpkg's answer.
ARCH="$(uname -m)"

top="$PWD/target/rpm"
# Staged OUTSIDE the buildroot on purpose. rpmbuild runs `rm -rf %{buildroot}` as part
# of %install's preamble (%__spec_install_pre), so a tree pre-staged directly into the
# buildroot is deleted before %files ever looks for it — which presents as "File not
# found" for every payload entry while the identical stage_payload call succeeds for the
# deb. So stage here, and let %install copy it in.
payload="$top/payload"
rm -rf "$top"
mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS} "$payload"

stage_payload "$payload" "$BIN" "$VERSION"

spec="$top/SPECS/$PKG.spec"
cat > "$spec" <<EOF
Name:           $PKG
Version:        $VERSION
Release:        1
Summary:        Native Markdown viewer and editor that renders on the CPU
License:        Apache-2.0
BuildArch:      $ARCH

Requires:       gtk4 >= 4.6
Requires:       gtksourceview5

# The binary is built by the pipeline's step 3 and staged into BUILDROOT before
# rpmbuild runs, so there is deliberately no %prep and no %build. Re-compiling here
# would package an artefact the gates never ran against.
%global debug_package %{nil}

%description
Scribobulate renders Markdown into native GTK4 widgets and forces the GSK Cairo
software renderer, so it holds no GL context and no video memory. It offers live
reload, split editing, a document outline, preview reading themes, find and
replace, and CriticMarkup annotation review.

%install
# Copy the tree payload.sh staged. It is staged outside the buildroot because rpmbuild
# clears the buildroot immediately before this section runs.
mkdir -p %{buildroot}
cp -a $payload/. %{buildroot}/

%files
%{_bindir}/$PKG
%{_datadir}/applications/$PKG.desktop
%{_datadir}/icons/hicolor/scalable/apps/$APP_ID.svg
%dir %{_datadir}/$PKG
%{_datadir}/$PKG/themes.toml
%{_mandir}/man1/$PKG.1.gz
# %license, not %doc: rpm marks these so they survive an --excludedocs install, which
# is exactly the case a licence notice must not be dropped by. The syntect grammars are
# statically linked into the binary and their terms require the notice to travel with it.
%license %{_datadir}/doc/$PKG/THIRD-PARTY-LICENSES.md

%post
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q %{_datadir}/applications || :
command -v gtk-update-icon-cache   >/dev/null 2>&1 && gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor 2>/dev/null || :
exit 0

%postun
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q %{_datadir}/applications || :
command -v gtk-update-icon-cache   >/dev/null 2>&1 && gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor 2>/dev/null || :
exit 0

%changelog
* $(date '+%a %b %d %Y') extollIT Enterprises <sales@extollit.com> - $VERSION-1
- Packaged release.
EOF

rpmbuild --define "_topdir $top" \
         --define "_rpmdir $top/RPMS" \
         -bb "$spec" >"$top/rpmbuild.log" 2>&1 || {
    echo "build-rpm: rpmbuild failed — see $top/rpmbuild.log" >&2
    tail -20 "$top/rpmbuild.log" >&2
    exit 1
}

rpm_file="$(find "$top/RPMS" -name '*.rpm' -print -quit)"
echo "built $rpm_file"
