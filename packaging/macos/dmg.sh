#!/usr/bin/env bash
#
# Build a distributable Scribobulate-<version>-<arch>.dmg wrapping Scribobulate.app.
#
# WHAT THIS FIXES: bundle.sh produces a signed .app, but a .app is not something you can
# hand someone as a single file — Finder shows it as a folder-like bundle most people do
# not know how to "install" (drag to /Applications). A .dmg is the minimum transferable
# container: double-click, drag the icon onto Applications, done.
#
# WHAT THIS IS NOT: notarized, and NOT a Gatekeeper pass on a machine that didn't build
# it. The bundle inside is ad-hoc signed (bundle.sh's own `codesign --sign -`, fatal on
# failure by design — this script does not soften that; bundle.sh's exit code propagates
# under `set -e`). MEASURED on this machine (macOS 26.6.1/25G76, arm64, 2026-08-07),
# simulating a browser download by tagging a real built dmg with com.apple.quarantine
# and asking Gatekeeper directly:
#
#     xattr -w com.apple.quarantine '0181;00000000;Safari;' Scribobulate-<ver>-arm64.dmg
#     spctl -a -t open --context context:primary-signature -v Scribobulate-<ver>-arm64.dmg
#     -> rejected: source=no usable signature
#     spctl -a -t exec -vv Scribobulate.app         (the .app itself, same story)
#     -> rejected (exit 3)
#
# "no usable signature" is Gatekeeper's verdict on an ad-hoc signature specifically —
# it is present (codesign -dv confirms a signature exists) but ad-hoc signing has no
# Developer ID chain for Gatekeeper to validate, so it counts as unsigned for this
# purpose. In practice this is the "Apple could not verify this app is free of
# malware" refusal on first launch. This was tested on the SAME machine that built
# the bundle — the one case Gatekeeper is most likely to be lenient toward — and it
# still refused, so a genuinely different machine is expected to refuse at least as
# often, not less. Distribution without that dialog needs a Developer ID certificate,
# `codesign --options runtime`, and notarization; none of that happens here.
#
# Usage: packaging/macos/dmg.sh [OUTPUT_DIR]   (default: target/macos)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
APP="$OUT_DIR/Scribobulate.app"

[[ "$(uname)" == "Darwin" ]] || { echo "error: macOS only" >&2; exit 1; }

# Always rebuild the bundle first — same reasoning as bundle.sh's own "always
# rebuilds": a stale .app wrapped into a shiny new .dmg is worse than the cost of a
# rebuild that turns out to be a no-op.
echo ":: Building Scribobulate.app"
"$REPO_ROOT/packaging/macos/bundle.sh" "$OUT_DIR"

# Version comes from the just-built bundle's Info.plist, not a second parse of
# Cargo.toml. bundle.sh already derived it once from Cargo.toml — the same single
# source packaging/windows/scribobulate.iss reads via ReadIni — so reading it back
# from the artefact means there is still one parse, not two implementations of the
# same derivation that could drift apart.
VERSION="$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")"
[[ -n "$VERSION" ]] || {
    echo "error: could not read CFBundleShortVersionString from $APP/Contents/Info.plist" >&2
    exit 1
}

ARCH="$(uname -m)"   # whatever this machine actually built (arm64 or x86_64) — never assumed
DMG="$OUT_DIR/Scribobulate-$VERSION-$ARCH.dmg"

# --- Staging: the app plus an /Applications symlink, the standard drag-to-install layout ---
echo ":: Staging dmg contents"
STAGE_PARENT="$(mktemp -d)"
STAGE_DIR="$STAGE_PARENT/dmg-stage"
mkdir -p "$STAGE_DIR"
cp -R "$APP" "$STAGE_DIR/"
ln -s /Applications "$STAGE_DIR/Applications"

# --- Build the disk image ---
echo ":: Building $DMG"
rm -f "$DMG"
hdiutil create -volname "Scribobulate $VERSION" -srcfolder "$STAGE_DIR" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE_PARENT"

[[ -s "$DMG" ]] || {
    echo "error: hdiutil reported success but $DMG is missing or empty" >&2
    exit 1
}

echo ":: Done — $DMG ($(du -h "$DMG" | cut -f1))"
echo "   Ad-hoc signed only, not notarized — Gatekeeper WILL refuse this on a machine"
echo "   that did not build it (measured; see this script's header). That is expected,"
echo "   not a bug in this script."
