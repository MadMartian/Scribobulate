#!/usr/bin/env bash
#
# Stage a licence text for every Homebrew formula the bundle redistributes, and write the
# manifest that records what was staged and what is NOT being claimed. (TDD 26.6)
#
# WHY THIS EXISTS: the .app now carries a GTK runtime it did not build. Redistributing an
# LGPL-family runtime without its notices is a licence breach, and the About dialog
# already tells the user the full notices are in the distribution — so the staging is what
# makes that sentence true rather than a decoration on top of it.
#
# THE ROW SET IS DERIVED, THE DETERMINATION IS NOT. Every staged Mach-O is mapped back to
# its formula through the Cellar, so the set cannot drift from what is shipped and nobody
# maintains a list by hand. What is NOT derivable is which licence covers which binary,
# and the SBOM data says so itself: a third of these formulae declare expressions like
# `LGPL-2.1-only OR MPL-1.1` or `GPL-3.0-or-later AND LGPL-2.1-or-later`. An OR needs an
# ELECTION, which is a decision we make rather than a fact to read; an AND does not say
# which part covers the artefact we staged. The GPL/LGPL pair is the sharp case — it is
# gettext, where the GPL covers the tools and the LGPL covers the libintl we actually
# ship, so a derived row would carry GPL-3.0 with perfect provenance and be WRONG ABOUT
# THE PRODUCT. The manifest therefore records the declared expression verbatim and marks
# the determination NOT-GATE-ENFORCED. Do not "finish" this by deriving it.
#
# WHAT IS STAGED: every regular file at the formula's prefix root, minus a denylist of
# history files. That is deliberately over-inclusive and has NO heuristic for picking the
# licence, because a filename heuristic on the INCLUDE side fails dangerous — it drops a
# licence silently — while one on the EXCLUDE side fails safe, over-shipping a NEWS file.
# Four filename-pattern misses in one session are the evidence for that asymmetry.
#
# Usage: packaging/macos/stage-licenses.sh <path-to-.app>
set -euo pipefail

APP="${1:?usage: stage-licenses.sh <path-to-.app> | --self-test}"

# The fail-closed arms, run against throwaway bundles by invoking this script on them: a
# Frameworks/ holding no Mach-O, and a `file` that crashes, must each refuse to produce a
# manifest. ANTI-VACUITY first: a bundle holding one Homebrew dylib stages.
if [ "$APP" = "--self-test" ]; then
    # shellcheck source=rpath.sh
    . "$(dirname "${BASH_SOURCE[0]}")/rpath.sh"
    tmp="$(mktemp -d)"; failures=0
    mkb() {  # $1 = case; a bundle with libglib in Frameworks/
        mkdir -p "$tmp/$1/Contents/Frameworks" "$tmp/$1/Contents/Resources"
        cp /opt/homebrew/lib/libglib-2.0.0.dylib "$tmp/$1/Contents/Frameworks/"
    }
    stages() {  # $1 = case, $2 = expected (ok|fail), $3 = stub dir (optional)
        rc=0; ( [ -z "${3:-}" ] || PATH="$3:$PATH"; bash "${BASH_SOURCE[0]}" "$tmp/$1" >/dev/null 2>&1 ) || rc=$?
        if { [ "$2" = ok ] && [ "$rc" -eq 0 ]; } || { [ "$2" = fail ] && [ "$rc" -ne 0 ]; }; then
            echo "   ok    $1"
        else
            echo "   FAILED $1 (expected $2, got $rc)"; failures=$((failures + 1))
        fi
    }
    mkb agreeing;  stages agreeing ok
    mkb no-machos; rm -f "$tmp/no-machos/Contents/Frameworks/"*.dylib
    printf 'text\n' > "$tmp/no-machos/Contents/Frameworks/readme"
    stages no-machos fail
    stub_tool "$tmp/file-crashes" file 'exit 1'
    mkb file-unreadable; stages file-unreadable fail "$tmp/file-crashes"
    rm -rf "$tmp"
    [ "$failures" -eq 0 ] || { echo ":: self-test FAILED ($failures)"; exit 1; }
    echo ":: self-test passed"; exit 0
fi

[ -d "$APP/Contents/Frameworks" ] || { echo "error: no Frameworks in $APP" >&2; exit 1; }

DEST="$APP/Contents/Resources/licenses"
MANIFEST="$APP/Contents/Resources/licenses/MANIFEST.tsv"
rm -rf "$DEST"
mkdir -p "$DEST"

# Pure history, and nothing a licence obligation is ever discharged by. AUTHORS and README
# are deliberately NOT here: copyright notices live in them often enough that dropping one
# could lose an attribution the licence requires.
is_history() {
    case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
        changelog*|news|changes|todo|install_receipt.json|sbom.spdx.json) return 0 ;;
        *) return 1 ;;
    esac
}

# formula_of, and Mach-O detection that fails closed (is_macho), each written once.
# shellcheck source=rpath.sh
. "$(dirname "${BASH_SOURCE[0]}")/rpath.sh"
# shellcheck source=homebrew.sh
. "$(dirname "${BASH_SOURCE[0]}")/homebrew.sh"

printf 'formula\tspdx_declared\tdetermination\tstaged_files\n' > "$MANIFEST"

declare -a SEEN=()
count=0
# Listed through rpath.sh, which fails closed, never `< <(find …)`, which loses the exit
# status of whatever produced the list.
objs="$(bundle_machos "$APP/Contents/Frameworks")" \
    || { echo "error: could not list the Mach-O files in $APP/Contents/Frameworks" >&2; exit 1; }
while IFS= read -r obj; do
    [ -n "$obj" ] || continue
    name="$(basename "$obj")"
    if ! formula="$(formula_of "$name")"; then
        echo "error: no Homebrew formula maps to staged binary '$name'." >&2
        echo "       Every redistributed binary must be attributable; refusing to" >&2
        echo "       produce a manifest that silently omits one." >&2
        exit 1
    fi
    case " ${SEEN[*]-} " in *" $formula "*) continue ;; esac
    SEEN+=("$formula")

    root="/opt/homebrew/opt/$formula"
    mkdir -p "$DEST/$formula"
    staged=""
    for entry in "$root"/*; do
        [ -f "$entry" ] || continue
        base="$(basename "$entry")"
        is_history "$base" && continue
        [ "$(stat -f%z "$entry")" -le 1000000 ] || continue
        cp "$entry" "$DEST/$formula/$base"
        staged="${staged:+$staged,}$base"
    done
    [ -n "$staged" ] || { echo "error: $formula staged no files from $root" >&2; exit 1; }

    # The declared expression, verbatim from Homebrew's own SBOM. Recorded as evidence of
    # what upstream says, never as our determination of what covers the binary.
    spdx="$(/usr/bin/python3 -c '
import json,sys
try:
    d=json.load(open(sys.argv[1]))
except Exception:
    print("NO-SBOM"); raise SystemExit
for p in d.get("packages",[]):
    if p.get("name")==sys.argv[2]:
        print(p.get("licenseConcluded") or p.get("licenseDeclared") or "NOASSERTION"); raise SystemExit
print("NOT-IN-SBOM")' "$root/sbom.spdx.json" "$formula" 2>/dev/null || echo "NO-SBOM")"

    printf '%s\t%s\tNOT-GATE-ENFORCED\t%s\n' "$formula" "$spdx" "$staged" >> "$MANIFEST"
    count=$((count + 1))
done <<< "$objs"

echo "   staged licence texts for $count formulae"
