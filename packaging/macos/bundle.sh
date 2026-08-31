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

# --- Bundled GTK runtime ------------------------------------------------------
#
# WHY: step 10's intent is an artefact a NON-DEVELOPER can install with no toolchain.
# Linked against Homebrew, this bundle does not launch for such a person at all -- it
# dies in dyld before main(). So the runtime travels with it.
#
# THE REWRITE IS TWO-SIDED, and the second side is the one that is easy to miss. Every
# dependent's load command must be repointed, AND every staged library's own install ID
# must be rewritten: Homebrew builds these with an absolute ID
# (`otool -D libgtk-4.1.dylib` reports /opt/homebrew/opt/gtk4/lib/libgtk-4.1.dylib). A
# bundle with rewritten load commands but original IDs still LAUNCHES, because dyld uses
# the load command -- so testing only the launch would call this done while every library
# still advertises a path outside the bundle to anything that re-resolves it.
#
# `@rpath/<name>` in the libraries rather than `@executable_path/../Frameworks/<name>`:
# it keeps them position-independent relative to whatever loads them, and puts the one
# absolute assumption in a single LC_RPATH on the executable.
#
# Hand-rolled rather than `dylibbundler`: it is not installed here, and this is the
# amount of code that would be spent making it a build dependency.
echo ":: Bundling the GTK runtime"
FRAMEWORKS="$APP/Contents/Frameworks"
mkdir -p "$FRAMEWORKS"

# Every dependency of $1 that this bundle must carry, as an ABSOLUTE SOURCE PATH.
#
# `@rpath/` DEPENDENCIES ARE NOT OPTIONAL TO HANDLE, and skipping them is the trap this
# function exists to avoid. Most Homebrew libraries reference their siblings that way --
# `libwebp.7.dylib` needs `@rpath/libsharpyuv.0.dylib` -- with `LC_RPATH` of
# `@loader_path/../lib`, which resolves inside that formula's own Cellar directory. A walk
# that follows only absolute paths therefore MISSES them, and misses them INVISIBLY: the
# resulting bundle passes a static audit, because `@rpath/…` reads as an internal
# reference whether or not the file behind it was ever staged. MEASURED here -- the first
# version of this script shipped exactly that bundle, condition 1 of
# verify-selfcontained.sh passed it, and only the sandboxed launch caught it.
resolve_deps() {
    local obj="$1" real dir rpath dep cand
    real="$(/usr/bin/readlink -f "$obj")"
    dir="$(dirname "$real")"
    otool -L "$obj" | tail -n +2 | awk '{print $1}' | while IFS= read -r dep; do
        case "$dep" in
            /usr/lib/*|/System/Library/*) continue ;;                 # the OS itself
            /opt/homebrew/*|/usr/local/*) printf '%s\n' "$dep" ;;
            @loader_path/*)               printf '%s\n' "$dir/${dep#@loader_path/}" ;;
            @rpath/*)
                # Resolve against this object's own LC_RPATH entries, with
                # @loader_path taken relative to where the object really lives.
                otool -l "$obj" | awk '/LC_RPATH/{f=1} f&&/ path /{print $2; f=0}' \
                | while IFS= read -r rpath; do
                    cand="${rpath//@loader_path/$dir}/${dep#@rpath/}"
                    [ -e "$cand" ] && printf '%s\n' "$cand" && break
                done
                ;;
        esac
    done
}

declare -a QUEUE=("$APP/Contents/MacOS/scribobulate")
declare -a STAGED=()
while [ ${#QUEUE[@]} -gt 0 ]; do
    current="${QUEUE[0]}"; QUEUE=("${QUEUE[@]:1}")
    while IFS= read -r dep; do
        [ -n "$dep" ] || continue
        name="$(basename "$dep")"
        target="$FRAMEWORKS/$name"
        if [ ! -e "$target" ]; then
            src="$(/usr/bin/readlink -f "$dep" 2>/dev/null || echo "$dep")"
            [ -e "$src" ] || { echo "error: dependency not found: $dep (from $current)" >&2; exit 1; }
            cp "$src" "$target"
            chmod u+w "$target"          # Homebrew ships these read-only
            STAGED+=("$target")
            # Queue the ORIGINAL, never the staged copy. `@loader_path` is resolved
            # relative to where the object actually sits, so walking the copy would
            # resolve `@loader_path/../lib` against Contents/Frameworks/../lib -- which
            # does not exist, yielding no candidate and silently dropping that whole
            # subtree from the closure. That is precisely how libsharpyuv went missing.
            QUEUE+=("$src")
        fi
    done < <(resolve_deps "$current")
done
echo "   staged ${#STAGED[@]} libraries into Contents/Frameworks"

# Side one: every staged library's own ID.
for lib in "${STAGED[@]}"; do
    install_name_tool -id "@rpath/$(basename "$lib")" "$lib" 2>/dev/null
done

# Side two: every load command, in the executable and in every staged library.
#
# The strings rewritten are the ones AS WRITTEN in the load command, which is what
# `install_name_tool -change` matches on -- so this lists raw dependency text rather than
# the resolved source paths used to compute the closure above.
#
# `@loader_path/…` is rewritten as well as the absolute forms, and it is the subtle one:
# it still *reads* as an internal reference after relocation, while meaning something
# different, because the loader path is now Contents/Frameworks instead of the formula's
# own lib directory. `@rpath/…` is left alone -- already the form we want, and the
# executable's single LC_RPATH is what resolves it.
needs_rewrite() {
    otool -L "$1" | tail -n +2 | awk '{print $1}' \
        | grep -E '^(/opt/homebrew|/usr/local|@loader_path)/' || true
}

for obj in "$APP/Contents/MacOS/scribobulate" "${STAGED[@]}"; do
    while IFS= read -r dep; do
        [ -n "$dep" ] || continue
        install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$obj" 2>/dev/null
    done < <(needs_rewrite "$obj")
done

# The one absolute assumption, in one place.
install_name_tool -add_rpath "@executable_path/../Frameworks" \
    "$APP/Contents/MacOS/scribobulate" 2>/dev/null || true

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

# ANNOUNCE THE LIMITATION HERE, where whoever holds the artefact will read it.
#
# The bundle is self-contained (verify-selfcontained.sh gates that), so it carries its own
# GTK runtime and needs no Homebrew on the recipient's machine. It is NOT notarized, and
# the signature above is ad-hoc, so Gatekeeper refuses it on any machine that did not
# build it -- reported to the user as "is damaged and can't be opened", which is a
# security decision wearing the words of a corrupt download.
#
# This prints unconditionally and on success, which is the point: a limitation mentioned
# only in a README is one the person holding the .app has already walked past. When
# notarization lands, this block comes out IN THE SAME CHANGE as the stapling -- an
# artefact that is notarized and still warns that it is not is this same defect pointed
# the other way.
echo
echo "   NOTE: ad-hoc signed, not notarized. On any Mac that did not build it, this"
echo "   bundle is refused by Gatekeeper and reported as \"damaged\". It is not damaged."
echo "   The recipient must override that decision deliberately: right-click > Open, or"
echo "   xattr -dr com.apple.quarantine '<bundle>'."
