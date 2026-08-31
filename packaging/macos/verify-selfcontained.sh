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
#   1. STATIC. Every Mach-O in the bundle, walked with `otool -L`, resolves only under
#      @rpath / @executable_path / @loader_path / /usr/lib / /System/Library. The last
#      two are the OS itself, present on every Mac by definition. Install IDs are checked
#      as well as load commands: an absolute ID left behind still launches, so a launch
#      test alone reports success on a half-done rewrite.
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

BIN="$APP/Contents/MacOS/scribobulate"
[ -x "$BIN" ] || { echo "error: no executable at $BIN" >&2; exit 1; }

# A path that is allowed to appear in a load command or an install ID.
is_internal() {
    case "$1" in
        @rpath/*|@executable_path/*|@loader_path/*) return 0 ;;
        /usr/lib/*|/System/Library/*)               return 0 ;;
        *)                                          return 1 ;;
    esac
}

echo ":: Condition 1 — no library resolved from outside the bundle"
findings=0
checked=0
while IFS= read -r obj; do
    file "$obj" | grep -q 'Mach-O' || continue
    checked=$((checked + 1))
    # The install ID, which is the half a launch test cannot see.
    id="$(otool -D "$obj" | tail -n +2 | tr -d '\t')"
    if [ -n "$id" ] && ! is_internal "$id"; then
        echo "   EXTERNAL ID   $(basename "$obj"): $id"
        findings=$((findings + 1))
    fi
    while IFS= read -r dep; do
        [ -n "$dep" ] || continue
        if ! is_internal "$dep"; then
            echo "   EXTERNAL LOAD $(basename "$obj"): $dep"
            findings=$((findings + 1))
            continue
        fi
        # AN INTERNAL-LOOKING REFERENCE IS NOT AN INTERNAL ONE. `@rpath/libfoo.dylib`
        # reads as internal whether or not libfoo was ever staged, so without this the
        # static condition passes a bundle that cannot load -- MEASURED, and it took the
        # sandboxed launch in condition 2 to catch it (libwebp needing
        # @rpath/libsharpyuv.0.dylib, which the closure had not followed). Resolving the
        # name against Frameworks/ is what makes condition 1 able to see that class.
        case "$dep" in
            @rpath/*)
                if [ ! -e "$APP/Contents/Frameworks/${dep#@rpath/}" ]; then
                    echo "   DANGLING      $(basename "$obj"): $dep is not in Frameworks/"
                    findings=$((findings + 1))
                fi
                ;;
        esac
    done < <(otool -L "$obj" | tail -n +2 | awk '{print $1}')
done < <(find "$APP" -type f)

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

# `--probe-startup` would be ideal; absent one, an unrecognised argument makes the app
# parse its command line and exit, which is enough to prove every library loaded --
# dyld resolves the whole graph before main() runs, so a missing library fails BEFORE
# any argument is looked at.
out="$(sandbox-exec -f "$PROFILE" "$BIN" --verify-startup 2>&1 || true)"
rm -f "$PROFILE"

if printf '%s' "$out" | grep -q 'Library not loaded\|dyld\[' ; then
    echo "   FAIL — dyld could not resolve the graph with the prefix hidden:" >&2
    printf '%s\n' "$out" | head -4 >&2
    exit 1
fi

# ABSENCE OF A dyld ERROR IS NOT EVIDENCE THE BINARY RAN. A process that died for some
# other reason -- or never started -- also prints no dyld error, so a check that stops at
# the line above passes on silence. Require the application's OWN voice: its argument
# parser rejecting the probe flag proves main() was entered, which in turn proves dyld
# resolved every library first, since the graph is bound before main().
#
# This couples the gate to that diagnostic string, which is the weakest part of it. A
# first-class `--probe-startup` that exits 0 with a known marker would be better and is
# application-side work; until then the coupling is stated here rather than left for
# someone to discover when the message is reworded.
if ! printf '%s' "$out" | grep -q 'Unknown option'; then
    echo "   FAIL — no dyld error, but the application never spoke either." >&2
    echo "          Silence is not a pass; it did not reach its own argument parsing." >&2
    printf '   output: %s\n' "${out:-<empty>}" | head -3 >&2
    exit 1
fi
echo "   PASS — dyld resolved the whole graph and main() was reached"

echo ":: self-contained"
