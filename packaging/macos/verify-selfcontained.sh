#!/usr/bin/env bash
#
# Does this .app resolve everything it needs from inside itself? (TDD 26.1, 26.2, 26.3)
#
# STANDALONE ON PURPOSE. It takes a bundle path rather than reaching for the one
# bundle.sh just built, because a gate that can only run against its own producer's
# output cannot be shown to FAIL. Point it at a pre-bundling .app and it must reject it;
# that mutation is the only thing separating this from a check that always passes.
#
# TWO CONDITIONS, and neither implies the other:
#
#   1. STATIC. Every Mach-O in the bundle, walked with `otool -L`, references only
#      /usr/lib and /System/Library (the OS itself, present on every Mac by definition),
#      `@rpath/<name>` with <name> staged in Frameworks/, or an `@loader_path/…` /
#      `@executable_path/…` path that RESOLVES to an existing file inside the bundle.
#      Install IDs are checked as well as load commands: an absolute ID left behind still
#      launches, so a launch test alone reports success on a half-done rewrite. And no
#      LC_RPATH may point outside the bundle (packaging/macos/rpath.sh's header).
#
#   2. DYNAMIC. The binary starts with the Homebrew prefix made unreadable. This is what
#      the static check cannot see -- a library found through some path the walk did not
#      model still fails here -- and it is run under `sandbox-exec` rather than by
#      manipulating DYLD_* variables, WHICH WOULD BE VACUOUS: an absolute load command
#      never consults DYLD_LIBRARY_PATH, so that form of the check passes a bundle that
#      is not self-contained at all. Do not "simplify" condition 2 into an env var.
#
# The development machine HAS Homebrew GTK, so a bundle that still links it launches here
# perfectly. That is the whole reason condition 2 hides the prefix instead of trusting a
# plain launch, and the reason TDD 26.3 makes the control's own falsifiability a rubric.
#
# Usage: packaging/macos/verify-selfcontained.sh <path-to-.app>
set -euo pipefail

APP="${1:?usage: verify-selfcontained.sh <path-to-.app>}"
[ -d "$APP" ] || { echo "error: no bundle at $APP" >&2; exit 1; }
[ "$(uname -s)" = "Darwin" ] || { echo "error: macOS only" >&2; exit 1; }

# shellcheck source=rpath.sh
. "$(dirname "${BASH_SOURCE[0]}")/rpath.sh"

BIN="$APP/Contents/MacOS/scribobulate"
[ -x "$BIN" ] || { echo "error: no executable at $BIN" >&2; exit 1; }

# Why reference $3 of object $2 does NOT resolve inside bundle $1, or nothing when it does.
#
# AN INTERNAL-LOOKING REFERENCE IS NOT AN INTERNAL ONE, in either spelling.
# `@rpath/libfoo.dylib` reads as internal whether or not libfoo was ever staged -- MEASURED,
# and it took the sandboxed launch in condition 2 to catch it (libwebp needing
# @rpath/libsharpyuv.0.dylib, which the closure had not followed) -- so it is resolved
# against Frameworks/. `@loader_path/…` and `@executable_path/…` are resolved with the same
# resolver as the rpath check, so `@loader_path/../../../libfoo.dylib`, which names a file
# beside the .app, is the external reference it is and not a pass on its prefix.
reference_problem() {
    local app="$1" obj="$2" ref="$3" loc
    case "$ref" in
        /usr/lib/*|/System/Library/*) return 0 ;;     # the OS itself, on every Mac
        @rpath/*)
            [ -e "$app/Contents/Frameworks/${ref#@rpath/}" ] || echo "is not in Frameworks/"
            return 0 ;;
        @loader_path/*|@executable_path/*)
            loc="$(bundle_location "$app" "$obj" "$ref")"
            if ! inside_bundle "$app" "$loc"; then
                echo "resolves outside the bundle, to ${loc:-<unresolvable>}"
            elif [ ! -e "$loc" ]; then
                echo "resolves to $loc, which does not exist"
            fi
            return 0 ;;
        *) echo "is outside the bundle" ;;
    esac
}

echo ":: Condition 1 — no library resolved from outside the bundle"
findings=0
checked=0
# Read through rpath.sh, which FAILS CLOSED: a tool that crashed or answered nothing stops
# the gate here instead of reading as a bundle with nothing in it (its header has the two
# ways that happened).
exe_rpath_control "$APP" || { echo "   FAIL — the positive control failed; nothing below could be trusted" >&2; exit 1; }
objs="$(bundle_machos "$APP")" || { echo "   FAIL — could not list the bundle's Mach-O files" >&2; exit 1; }
while IFS= read -r obj; do
    [ -n "$obj" ] || continue
    checked=$((checked + 1))
    name="$(basename "$obj")"
    # The install ID, which is the half a launch test cannot see.
    id="$(install_id "$obj")" || { echo "   FAIL — could not read $name" >&2; exit 1; }
    if [ -n "$id" ]; then
        why="$(reference_problem "$APP" "$obj" "$id")"
        if [ -n "$why" ]; then
            echo "   EXTERNAL ID    $name: $id $why"
            findings=$((findings + 1))
        fi
    fi
    deps="$(load_deps "$obj")" || { echo "   FAIL — could not read $name" >&2; exit 1; }
    while IFS= read -r dep; do
        [ -n "$dep" ] || continue
        why="$(reference_problem "$APP" "$obj" "$dep")"
        if [ -n "$why" ]; then
            echo "   EXTERNAL LOAD  $name: $dep $why"
            findings=$((findings + 1))
        fi
    done <<< "$deps"
    # An rpath outside the bundle is searched BEFORE the executable's, so a file there
    # replaces a staged library even though every load command above reads as internal.
    rps="$(rpaths_of "$obj")" || { echo "   FAIL — could not read $name" >&2; exit 1; }
    while IFS= read -r rp; do
        [ -n "$rp" ] || continue
        if ! rpath_is_internal "$APP" "$obj" "$rp"; then
            echo "   ESCAPING RPATH $name: $rp"
            findings=$((findings + 1))
        fi
    done <<< "$rps"
done <<< "$objs"
[ "$checked" -gt 0 ] || { echo "   FAIL — no Mach-O files found; nothing was checked" >&2; exit 1; }

if [ "$findings" -ne 0 ]; then
    echo "   FAIL — $findings external reference(s) across $checked Mach-O files" >&2
    exit 1
fi
echo "   PASS — $checked Mach-O files, every reference internal"

echo ":: Condition 2 — launches with the Homebrew prefix unreadable"
PROFILE="$(mktemp -t scribo-selfcontained).sb"
cat > "$PROFILE" <<'PROFILE_EOF'
(version 1)
(allow default)
(deny file-read* (subpath "/opt/homebrew"))
(deny file-read* (subpath "/usr/local"))
PROFILE_EOF

# `--probe-startup` prints a marker this project owns and exits 0. dyld binds the whole
# library graph before main() runs, so a missing library fails BEFORE the flag is looked
# at -- which is what makes a successful probe evidence about the graph.
#
# LC_ALL=C is retained deliberately though it is no longer load-bearing: the marker is
# ours and untranslated. It costs nothing and keeps this invocation independent of the
# tester's environment, which is the property the previous version of this check lost.
out="$(LC_ALL=C sandbox-exec -f "$PROFILE" "$BIN" --probe-startup 2>&1 || true)"
rm -f "$PROFILE"

if printf '%s' "$out" | grep -q 'Library not loaded\|dyld\[' ; then
    echo "   FAIL — dyld could not resolve the graph with the prefix hidden:" >&2
    printf '%s\n' "$out" | head -4 >&2
    exit 1
fi

# ABSENCE OF A dyld ERROR IS NOT EVIDENCE THE BINARY RAN. A process that died for some
# other reason -- or never started -- also prints no dyld error, so a check that stops at
# the line above passes on silence. Requiring a rejected probe flag proves main() was
# entered, which in turn proves dyld resolved every library first, since the graph is
# bound before main().
#
# THE MARKER IS OURS, and that is the point of it. `app::STARTUP_PROBE_MARKER` is a
# constant with this gate named beside it, so there is one definition and whoever changes
# it is told what they break. It replaces a grep for GLib's `Unknown option`, which was
# never the application's string at all and which GLib TRANSLATES -- measured on one
# binary as `Unbekannte Option`, `Option inconnue`, `Opción desconocida`, i.e. a gate that
# went red on a good bundle in German. LC_ALL=C above is now belt-and-braces rather than
# load-bearing, and stays because it costs nothing.
#
# WHAT REACHING THIS PROVES, AND WHAT IT DOES NOT. The probe is the first statement in
# run(), ahead of the renderer pin, the logger and GTK, so reaching it proves the LIBRARY
# GRAPH bound and nothing else -- which is exactly the question this gate asks. It says
# nothing about anything dlopen'ed later: pixbuf loaders, GIO modules, GSettings schemas.
# A zero exit here is not "the bundle is complete", the same way an internal-LOOKING
# @rpath reference is not an internal one. The resource assertions are what cover that.
if ! printf '%s' "$out" | grep -qF 'scribobulate: startup-probe ok'; then
    echo "   FAIL — no dyld error, but the application never spoke either." >&2
    echo "          Silence is not a pass; it never reached the probe in run()." >&2
    printf '   output: %s\n' "${out:-<empty>}" | head -3 >&2
    exit 1
fi
echo "   PASS — dyld resolved the whole graph and main() was reached"

echo ":: self-contained"
