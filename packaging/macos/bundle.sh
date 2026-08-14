#!/usr/bin/env bash
#
# Build Scribobulate.app — a macOS application bundle around the release binary.
#
# WHAT THIS FIXES: run as a bare Unix executable, the app has no Dock or
# Cmd-Tab identity — macOS shows a generic "exec" placeholder and names it
# "scribobulate" in lowercase, because the Dock icon comes from the bundle's
# Info.plist (CFBundleIconFile), NOT from GTK's icon theme. No amount of
# GTK-side icon work reaches it. A bundle is the only fix.
#
# WHAT THIS IS NOT: a redistributable. The bundled binary still links against
# Homebrew's dylibs in /opt/homebrew (49 of them, ~35 MB — see `otool -L`), and
# a self-contained bundle additionally needs those copied into
# Contents/Frameworks with their load paths rewritten, plus the icon theme and
# GLib schemas staged inside. See packaging/macos/README.md for the gap list.
# On a machine with the Homebrew dependencies installed, this bundle works.
#
# Usage:  packaging/macos/bundle.sh [OUTPUT_DIR]   (default: target/macos)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
APP="$OUT_DIR/Scribobulate.app"
BIN="$REPO_ROOT/target/release/scribobulate"
SVG="$REPO_ROOT/data/icons/scalable/apps/com.extollit.scribobulate.svg"

[[ "$(uname)" == "Darwin" ]] || { echo "error: macOS only" >&2; exit 1; }

# Version comes from Cargo.toml so the plist can never drift from the crate.
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$REPO_ROOT/Cargo.toml" | head -1)"
[[ -n "$VERSION" ]] || { echo "error: could not read version from Cargo.toml" >&2; exit 1; }

# Always rebuilds, like install.sh — a stale binary bundled without warning
# (QA finding R1-15) is worse than the cost of a rebuild that turns out to be
# a no-op.
echo ":: Building release binary"
(cd "$REPO_ROOT" && cargo build --release)

# --- Icon: SVG -> .iconset -> .icns -------------------------------------------
# `iconutil` is part of macOS; the SVG rasterizer is not, so accept either of
# the two common ones. The @2x entries must be rendered at DOUBLE their nominal
# size — iconutil derives the logical size from the filename, not the pixels,
# and silently produces a blurry icon if they are rendered at face value.
if command -v rsvg-convert >/dev/null 2>&1; then
    render() { rsvg-convert -w "$1" -h "$1" "$SVG" -o "$2"; }
elif command -v inkscape >/dev/null 2>&1; then
    render() { inkscape "$SVG" -w "$1" -h "$1" -o "$2" >/dev/null 2>&1; }
else
    echo "error: need rsvg-convert (brew install librsvg) or inkscape to rasterize the icon" >&2
    exit 1
fi

echo ":: Generating scribobulate.icns from $(basename "$SVG")"
mkdir -p "$OUT_DIR"
ICONSET="$(mktemp -d)/scribobulate.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    render "$size"           "$ICONSET/icon_${size}x${size}.png"
    render "$((size * 2))"   "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$OUT_DIR/scribobulate.icns"

# --- Bundle layout ------------------------------------------------------------
echo ":: Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/scribobulate"
mv "$OUT_DIR/scribobulate.icns" "$APP/Contents/Resources/scribobulate.icns"
sed "s/@VERSION@/$VERSION/g" "$REPO_ROOT/packaging/macos/Info.plist.in" \
    > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# --- Third-party notices -------------------------------------------------------
# The syntect grammar assets `two-face` compiles into the binary (MIT, Apache-2.0,
# BSD-2-Clause, BSD-3-Clause) require the notice to travel with every binary
# distribution, on every platform -- independent of the dylib-bundling gap noted
# above (this app links Homebrew's dylibs rather than bundling them, so it owes no
# runtime-attribution obligation, but it owes this one). The About dialog already
# claims "Full notices: THIRD-PARTY-LICENSES.md (in the distribution)"; without
# this the claim is false on macOS. No librsvg notice is staged here -- that one
# exists only because the Windows installer bundles a statically-linked librsvg
# with its Rust crate graph, which this bundle does not do.
cp "$REPO_ROOT/LICENSE" "$APP/Contents/Resources/LICENSE"
cp "$REPO_ROOT/THIRD-PARTY-LICENSES.md" "$APP/Contents/Resources/THIRD-PARTY-LICENSES.md"

# Ad-hoc signature. Unsigned bundles are increasingly refused outright on Apple
# Silicon; this is enough to launch locally and is NOT distribution signing
# (that needs a Developer ID certificate plus notarization).
#
# A failed signature is a hard failure, not a warning: a bundle whose
# signature codesign refused to write is not "slightly worse", it is one
# Gatekeeper will very likely refuse to launch outright. QA finding R1-14 was
# that the previous version of this script downgraded this to a logged
# warning and still exited 0 — a broken build looked identical to a working
# one in script/CI output.
if ! codesign --force --deep --sign - "$APP"; then
    echo "error: ad-hoc codesign failed; the bundle will likely refuse to launch" >&2
    exit 1
fi

# Refresh Launch Services so the Dock/Finder pick up the icon immediately rather
# than after an unpredictable cache delay.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
    -f "$APP" 2>/dev/null || true

echo ":: Done — $APP"
echo "   open '$APP' --args path/to/document.md"
