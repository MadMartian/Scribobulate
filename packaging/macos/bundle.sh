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
# THE RUNTIME TRAVELS WITH IT. The GTK closure is copied into Contents/Frameworks and
# every load path rewritten, so the bundle needs no Homebrew on the machine that runs it;
# `verify-selfcontained.sh` is the gate on that claim and is not optional. The closure is
# 42 FILES reached by 49 distinct load paths — eight basenames are reached through two
# Homebrew aliases each, so "49 dylibs" counts paths and overcounts files.
#
# WHAT IT IS STILL NOT: notarized. The signature below is ad-hoc, so Gatekeeper refuses
# the bundle on any Mac that did not build it and tells the user it is "damaged". That is
# a deferred scope decision rather than an oversight, and the run announces it — see the
# note printed at the end, and packaging/macos/README.md.
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

# --- gdk-pixbuf loaders, staged BEFORE the closure is walked --------------------
#
# THESE ARE NOT IN THE STATIC GRAPH. They are `dlopen`ed at runtime, so nothing reachable
# from the executable's load commands mentions them, and a closure seeded only from the
# executable stages neither the loaders nor -- the part that bites -- THEIR OWN
# dependencies. The SVG loader alone pulls in librsvg, which the application does not link
# and which would therefore be absent from a bundle that looked complete.
#
# So they are copied in first and their ORIGINALS are seeded into the work list below,
# which makes their dependencies part of the same closure rather than a second staging
# list that has to be kept in step with it.
PIXBUF_VER="2.10.0"
PIXBUF_SRC="/opt/homebrew/lib/gdk-pixbuf-2.0/$PIXBUF_VER/loaders"
# FLAT IN Contents/Frameworks, with no directory of its own, and that is not a style
# choice. `codesign --deep` treats ANY DIRECTORY under Frameworks/ as a nested bundle and
# refuses one that is not: "bundle format unrecognized, invalid, or unsuitable -- In
# subcomponent: .../Frameworks/gdk-pixbuf-2.0". The bundle is then left UNSIGNED (no
# Contents/_CodeSignature at all) and the kernel kills it at launch with no output, which
# reads as a missing library and is not one. Flat Mach-O files sign exactly like the 44
# dylibs already beside them.
#
# SCOPE OF THAT MEASUREMENT, stated because the first version of this comment overclaimed:
# the "In subcomponent" refusal WAS observed directly with a directory here. Whether
# Contents/Resources would also have worked was NOT established -- that build died earlier
# for an unrelated reason (see the loader-cache step below), so its failure proved nothing
# about placement. Flat under Frameworks is chosen because it is known to sign, not
# because the alternative is known to fail.
PIXBUF_DST="$APP/Contents/Frameworks"
declare -a LOADER_SRCS=()
if [ -d "$PIXBUF_SRC" ]; then
    mkdir -p "$PIXBUF_DST"
    # Enumerated, never matched against a guessed filename: the set includes both `.so`
    # and `.dylib` spellings and is not predictable from the format list.
    while IFS= read -r loader; do
        cp "$(/usr/bin/readlink -f "$loader")" "$PIXBUF_DST/$(basename "$loader")"
        chmod u+w "$PIXBUF_DST/$(basename "$loader")"
        LOADER_SRCS+=("$loader")
    done < <(find -L "$PIXBUF_SRC" -type f \( -name '*.so' -o -name '*.dylib' \))
    echo "   staged ${#LOADER_SRCS[@]} gdk-pixbuf loaders"
fi

declare -a QUEUE=("$APP/Contents/MacOS/scribobulate" "${LOADER_SRCS[@]}")
declare -a STAGED=()

# The staged loader copies are Mach-O objects inside the bundle and need their load
# commands rewritten like everything else -- condition 1 walks every file in the bundle,
# so a missed one fails the gate rather than shipping. They are kept SEPARATE from
# $STAGED because they must NOT receive an `@rpath/<name>` install ID: a loader is
# `dlopen`ed by the path written in loaders.cache, not resolved through an rpath, so
# rewriting its ID would describe it as something it is not.
declare -a LOADER_OBJS=()
while IFS= read -r staged_loader; do
    [ -n "$staged_loader" ] && LOADER_OBJS+=("$staged_loader")
done < <(find "$PIXBUF_DST" -type f 2>/dev/null || true)
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

# `install_name_tool` warns "changes being made to the file will invalidate the code
# signature" on EVERY call. It is benign HERE and only here, because signing happens after
# all rewriting (below) -- if that order ever reverses, this warning stops being noise and
# starts being the report of a real defect.
#
# Filter that one line rather than discarding stderr: `2>/dev/null` on the whole stream
# also hides the failures worth seeing, and a rewrite that silently did not happen is
# exactly what condition 1 of verify-selfcontained.sh then has to catch for us.
retool() {
    local err
    if ! err="$(install_name_tool "$@" 2>&1 >/dev/null)"; then
        printf '%s\n' "$err" >&2
        return 1
    fi
    printf '%s\n' "$err" | grep -v 'will invalidate the code signature' | grep . >&2 || true
}

# Side one: every staged library's own ID.
for lib in "${STAGED[@]}"; do
    retool -id "@rpath/$(basename "$lib")" "$lib"
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

# WHERE a dependency ends up decides the form it is rewritten to, and getting this wrong
# is invisible until something dlopens. A library staged in Frameworks/ is reached through
# the executable's LC_RPATH, so `@rpath/<name>`. A pixbuf loader's SIBLING loader is NOT in
# Frameworks -- it sits beside it in the loaders directory -- so `@rpath/<name>` would send
# dyld to Frameworks/ and find nothing. That one is `@loader_path/<name>`, which is also
# the truthful description: it is found relative to the object loading it.
rewrite_target() {
    local dep="$1" name
    name="$(basename "$dep")"
    if [ -e "$FRAMEWORKS/$name" ]; then
        printf '@rpath/%s\n' "$name"
    else
        printf '@loader_path/%s\n' "$name"
    fi
}

for obj in "$APP/Contents/MacOS/scribobulate" "${STAGED[@]}" ${LOADER_OBJS[@]+"${LOADER_OBJS[@]}"}; do
    while IFS= read -r dep; do
        [ -n "$dep" ] || continue
        retool -change "$dep" "$(rewrite_target "$dep")" "$obj"
    done < <(needs_rewrite "$obj")
done

# A loader's own ID is absolute as Homebrew ships it, which condition 1 rejects and which
# describes the file as living somewhere it does not. `@loader_path/<name>` rather than
# `@rpath/<name>`: a loader is dlopen'ed by the path in loaders.cache, never resolved
# through the executable's rpath, so @rpath would be a claim that is simply untrue.
for obj in ${LOADER_OBJS[@]+"${LOADER_OBJS[@]}"}; do
    retool -id "@loader_path/$(basename "$obj")" "$obj"
done

# The one absolute assumption, in one place.
# Conditional rather than `|| true`: adding an rpath that is already present is an error,
# and blanket-ignoring the result would also swallow a genuine failure to add it.
if ! otool -l "$APP/Contents/MacOS/scribobulate" \
     | awk '/LC_RPATH/{f=1} f&&/ path /{print $2; f=0}' \
     | grep -qx '@executable_path/../Frameworks'; then
    retool -add_rpath "@executable_path/../Frameworks" "$APP/Contents/MacOS/scribobulate"
fi

# --- Runtime data that fails LATE and SILENTLY ---------------------------------
#
# None of this is in any link graph, so nothing above stages it and no dyld error reports
# it missing. Each one degrades the running window instead: icons become broken-image
# placeholders, a file dialog aborts on a missing GSettings schema, images fail to decode.
# That is the whole reason TDD 26.4 asserts them as OUTCOMES from inside the bundle rather
# than as a staging checklist -- a checklist is satisfied by copying the wrong thing.
#
# THE gtksourceview VALIDATORS ARE STAGED; the .lang files and style schemes are NOT.
# That split is measured, and the obvious reading of it is wrong in both directions.
#
# The .lang files and schemes genuinely are a GResource inside libgtksourceview-5.0.dylib,
# which Frameworks/ already carries, so restaging them would be copying what the library
# already has. But loading one VALIDATES it against language2.rng, and that is read from a
# real filesystem path -- libxml takes a filename, so a GResource cannot satisfy it. The
# path is the compile-time PACKAGE_DATADIR, i.e. the Homebrew Cellar, which the recipient
# does not have. Validation then fails, the .lang is DROPPED, and the editor silently
# loses Markdown highlighting with no error the user sees.
#
# XDG_DATA_DIRS does not fix it, and the reason is search ORDER rather than the variable
# being ignored: GtkSourceView walks user-data-dir, then the compiled DATADIR, then its
# GResource, then XDG_DATA_DIRS -- so on a machine that HAS the Cellar the walk stops at
# entry two and never reaches the staged copy. The seam prepends this directory to the
# LanguageManager search path instead, which is the supported way in front of DATADIR.
GTKSV_SRC="/opt/homebrew/share/gtksourceview-5/language-specs"
if [ -d "$GTKSV_SRC" ]; then
    GTKSV_DST="$APP/Contents/Resources/gtksourceview-5/language-specs"
    mkdir -p "$GTKSV_DST"
    # Enumerated rather than named: the validator set is language2.rng today, with
    # language.rng and language.dtd beside it, and which of them a given .lang pulls in is
    # not something to hard-code.
    find -L "$GTKSV_SRC" -maxdepth 1 -type f \( -name '*.rng' -o -name '*.dtd' \) \
        -exec cp {} "$GTKSV_DST/" \;
    echo "   staged $(find "$GTKSV_DST" -type f | wc -l | tr -d ' ') gtksourceview validators"
fi
echo ":: Staging runtime data"
SHARE="$APP/Contents/Resources/share"
mkdir -p "$SHARE"

# GSettings schemas. `gschemas.compiled` is what GLib actually reads; the .xml sources
# come too so the directory is inspectable rather than opaque.
if [ -d /opt/homebrew/share/glib-2.0/schemas ]; then
    mkdir -p "$SHARE/glib-2.0/schemas"
    cp -RL /opt/homebrew/share/glib-2.0/schemas/. "$SHARE/glib-2.0/schemas/"
    [ -f "$SHARE/glib-2.0/schemas/gschemas.compiled" ] \
        || glib-compile-schemas "$SHARE/glib-2.0/schemas"
fi

# The icon theme. `-L` because Homebrew's share/icons entries are symlinks into the
# Cellar, and copying the links would produce a bundle whose icons resolve only on a
# machine that has the Cellar -- i.e. exactly the bundle this work exists to stop
# shipping.
for theme in Adwaita hicolor; do
    if [ -d "/opt/homebrew/share/icons/$theme" ]; then
        mkdir -p "$SHARE/icons/$theme"
        cp -RL "/opt/homebrew/share/icons/$theme/." "$SHARE/icons/$theme/"
    fi
done

# The loader cache, REGENERATED rather than copied. Homebrew's own cache names absolute
# /opt/homebrew paths, so shipping it would point a self-contained bundle straight back
# out of itself. Paths are made relative to the cache file's own directory, which is how
# the seam then resolves them at runtime.
# QUERIED AGAINST THE ORIGINALS, NEVER THE STAGED COPIES. `gdk-pixbuf-query-loaders`
# dlopens every module it scans, and the staged copies have had install_name_tool run over
# them, which invalidates their signatures -- on Apple Silicon the kernel SIGKILLs a
# process that loads such a dylib. Pointed at the bundle this step died with "Killed: 9",
# taking the whole build down BEFORE the signing step, which is why the resulting bundle
# had no signature and the app was killed at launch with no output. Every symptom in that
# cascade pointed somewhere other than here.
if [ -d "$PIXBUF_SRC" ] && command -v gdk-pixbuf-query-loaders >/dev/null 2>&1; then
    GDK_PIXBUF_MODULEDIR="$PIXBUF_SRC" gdk-pixbuf-query-loaders \
        | sed "s|$PIXBUF_SRC/||g" \
        > "$APP/Contents/Resources/loaders.cache"
fi

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

# --- Licence attribution for the redistributed runtime --------------------------
#
# The bundle now carries a GTK runtime it did not build, so the notices travel with it.
# Staged and gated in separate scripts, both runnable standalone: a gate that can only be
# invoked by its own producer cannot be pointed at a subject known to be bad.
"$REPO_ROOT/packaging/macos/stage-licenses.sh" "$APP"
"$REPO_ROOT/packaging/macos/verify-licenses.sh" "$APP" >/dev/null

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

# CODESIGN EXITS 0 WHILE FAILING, so the check above is not sufficient on its own and was
# not sufficient in practice. MEASURED: with a directory under Frameworks/ it printed
# "bundle format unrecognized, invalid, or unsuitable / In subcomponent: ..." and RETURNED
# ZERO, leaving the bundle with no Contents/_CodeSignature and no signature at all. The
# guard that exists precisely to stop a broken signature shipping (QA finding R1-14) saw
# exit 0 and passed it on. VERIFY THE ARTEFACT, never the exit code -- the same rule this
# project applies to every other gate.
if ! codesign --verify --deep --strict "$APP" 2>/dev/null; then
    echo "error: the bundle is not validly signed after signing it:" >&2
    codesign --verify --deep --strict "$APP" 2>&1 | sed 's/^/       /' >&2
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
