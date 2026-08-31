#!/usr/bin/env bash
#
# Does this .app actually ship the notices for the runtime it redistributes? (TDD 26.6)
#
# FIVE CONDITIONS. The first two are about the SET — that it matches what is shipped, in
# both directions — and the last three are about each row's text. They are separated
# because they fail for different reasons and a reader needs to know which.
#
#   1. Every staged Mach-O maps to a row. An unattributed binary is the breach itself.
#   2. Every row maps to a staged Mach-O. Attribution for something not shipped is not
#      harmless: it is a false statement about the artefact, and it is how a manifest
#      keeps a row after the binary behind it is dropped.
#   3. Every row staged at least one file.
#   4. No staged file is EMPTY. Presence is not content — a path that resolves and a file
#      with nothing in it are the same breach wearing a directory listing. (Windows seat:
#      gvsbuild ships share\doc directories for three projects that are present and empty.)
#   5. Every row has at least one staged file containing a licence anchor, so what was
#      shipped IS licence text rather than a README that mentions one.
#
# WHAT THIS GATE DOES NOT CHECK, AND WILL NOT: WHICH LICENCE COVERS WHICH BINARY. That is
# a determination, it is not derivable, and the manifest marks every row
# NOT-GATE-ENFORCED to say so out loud. A third of these formulae declare AND/OR
# expressions: an OR needs an election we make, an AND does not say which part covers the
# artefact staged. The instance that proves it is gettext — GPL for the tools, LGPL for
# the libintl actually shipped — where a derived row would be confidently wrong about the
# product rather than merely incomplete.
#
# SO A GREEN RUN MEANS "the notices are shipped", NEVER "the licensing is correct". An
# obligation that reads as discharged when it is not is the failure this whole area keeps
# producing, which is why the field is a column rather than a footnote.
#
# Usage: packaging/macos/verify-licenses.sh <path-to-.app>
#        packaging/macos/verify-licenses.sh --self-test
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANCHORS="$HERE/license-anchors.txt"

# Anchor lines, comments and blanks stripped, lower-cased once.
anchor_list() { grep -v '^[[:space:]]*#' "$ANCHORS" | grep -v '^[[:space:]]*$' | tr '[:upper:]' '[:lower:]'; }

has_anchor() {
    local text; text="$(tr '[:upper:]' '[:lower:]' < "$1")"
    while IFS= read -r a; do
        [ -n "$a" ] || continue
        case "$text" in *"$a"*) return 0 ;; esac
    done < <(anchor_list)
    return 1
}

# The formula behind a staged file name, or empty.
formula_of() {
    local name="$1" cand real
    for cand in "/opt/homebrew/lib/$name" \
                "/opt/homebrew/lib/gdk-pixbuf-2.0/2.10.0/loaders/$name"; do
        [ -e "$cand" ] || continue
        real="$(/usr/bin/readlink -f "$cand" 2>/dev/null || true)"
        case "$real" in
            */Cellar/*) printf '%s' "$real" | sed 's|.*/Cellar/||; s|/.*||'; return 0 ;;
        esac
    done
    return 1
}

# Returns the number of problems found, and prints each one.
#
# The shipped-formula set is computed ONCE and then consulted, rather than re-walking the
# bundle inside the per-row loop. The first version did the latter with a nested
# pipeline-and-break, which was both slower and — measured — able to take the whole script
# down from inside a `set +e` region, so the self-test aborted before its first case and
# reported a failure that named nothing.
audit_bundle() {
    # SEPARATE STATEMENTS, not `local app="$1" manifest="$app/..."`. Bash 3.2 — which is
    # what macOS ships — expands every word of a `local` before performing any of its
    # assignments, so the second would read an unset `app` and die under `set -u`. The
    # failure surfaced as the self-test aborting before its first case with no output,
    # because the case harness redirects stderr.
    local app="$1"
    local manifest="$app/Contents/Resources/licenses/MANIFEST.tsv"
    local problems=0 shipped="" name formula
    if [ ! -f "$manifest" ]; then
        echo "   NO MANIFEST at $manifest"
        return 1
    fi

    while IFS= read -r obj; do
        file "$obj" 2>/dev/null | grep -q 'Mach-O' || continue
        name="$(basename "$obj")"
        if formula="$(formula_of "$name")"; then
            case " $shipped " in *" $formula "*) ;; *) shipped="$shipped $formula" ;; esac
        else
            echo "   UNMAPPABLE   $name has no Homebrew formula"
            problems=$((problems + 1))
        fi
    done < <(find "$app/Contents/Frameworks" -type f 2>/dev/null)

    # Condition 1 — every shipped formula has a row.
    for formula in $shipped; do
        if ! tail -n +2 "$manifest" | cut -f1 | grep -qx "$formula"; then
            echo "   UNATTRIBUTED $formula is shipped but has no manifest row"
            problems=$((problems + 1))
        fi
    done

    # Conditions 2-5, per row.
    local row dir f ok
    while IFS=$'\t' read -r row _spdx _det _files; do
        [ "$row" = "formula" ] && continue
        [ -n "$row" ] || continue
        dir="$app/Contents/Resources/licenses/$row"

        case " $shipped " in
            *" $row "*) ;;
            *) echo "   ORPHAN ROW   $row is attributed but nothing from it is staged"
               problems=$((problems + 1)) ;;
        esac

        if [ ! -d "$dir" ] || [ -z "$(ls -A "$dir" 2>/dev/null)" ]; then
            echo "   NO TEXT      $row staged no licence files"
            problems=$((problems + 1))
            continue
        fi

        for f in "$dir"/*; do
            [ -f "$f" ] || continue
            [ -s "$f" ] || { echo "   EMPTY        $row/$(basename "$f") is zero bytes"
                             problems=$((problems + 1)); }
        done

        ok=1
        for f in "$dir"/*; do
            [ -f "$f" ] || continue
            if has_anchor "$f"; then ok=0; break; fi
        done
        [ "$ok" -eq 0 ] || { echo "   NOT A LICENCE $row staged files, none containing a known licence anchor"
                             problems=$((problems + 1)); }
    done < "$manifest"

    return "$problems"
}

# --- self-test ----------------------------------------------------------------
#
# The cases are shaped after the Windows seat's, including the one they named as the case
# that matters most: A TREE THAT AGREES WITH ITS MANIFEST MUST REPORT ZERO PROBLEMS. Without
# it, a checker that always complains passes every other case here.
#
# The evaluator's counter is `problems` and this harness's is `failures`, deliberately not
# the same word: the Windows seat had a mutation aimed at their evaluator land on their
# self-test's own assertion, because both wore one name on identically-shaped lines, and
# the surviving mutant looked like a blind self-test rather than a disabled check.
self_test() {
    local tmp failures=0 rc
    tmp="$(mktemp -d)"
    mk() {  # $1 = case dir; builds a minimal, VALID bundle shape
        mkdir -p "$tmp/$1/Contents/Frameworks" "$tmp/$1/Contents/Resources/licenses/glib"
        cp /opt/homebrew/lib/libglib-2.0.0.dylib "$tmp/$1/Contents/Frameworks/libglib-2.0.0.dylib"
        printf 'formula\tspdx_declared\tdetermination\tstaged_files\n' \
            > "$tmp/$1/Contents/Resources/licenses/MANIFEST.tsv"
        printf 'glib\tLGPL-2.1-or-later\tNOT-GATE-ENFORCED\tCOPYING\n' \
            >> "$tmp/$1/Contents/Resources/licenses/MANIFEST.tsv"
        printf 'GNU LESSER GENERAL PUBLIC LICENSE\nVersion 2.1\n' \
            > "$tmp/$1/Contents/Resources/licenses/glib/COPYING"
    }
    check() {  # $1 = name, $2 = expected (ok|fail)
        set +e; audit_bundle "$tmp/$1" >/dev/null 2>&1; rc=$?; set -e
        if { [ "$2" = ok ] && [ "$rc" -eq 0 ]; } || { [ "$2" = fail ] && [ "$rc" -ne 0 ]; }; then
            echo "   ok    $1"
        else
            echo "   FAILED $1 (expected $2, audit returned $rc)"; failures=$((failures + 1))
        fi
    }

    # ANTI-VACUITY. Must be first: everything below is meaningless if this cannot pass.
    mk agreeing;            check agreeing ok

    mk empty-text;          : > "$tmp/empty-text/Contents/Resources/licenses/glib/COPYING"
    check empty-text fail

    mk not-a-licence;       printf 'See the website for licensing.\n' \
        > "$tmp/not-a-licence/Contents/Resources/licenses/glib/COPYING"
    check not-a-licence fail

    mk no-text;             rm -rf "$tmp/no-text/Contents/Resources/licenses/glib"
    check no-text fail

    mk orphan-row;          printf 'zzz-not-shipped\tMIT\tNOT-GATE-ENFORCED\tCOPYING\n' \
        >> "$tmp/orphan-row/Contents/Resources/licenses/MANIFEST.tsv"
    mkdir -p "$tmp/orphan-row/Contents/Resources/licenses/zzz-not-shipped"
    printf 'Permission is hereby granted\n' \
        > "$tmp/orphan-row/Contents/Resources/licenses/zzz-not-shipped/COPYING"
    check orphan-row fail

    mk unattributed;        tail -n +1 "$tmp/unattributed/Contents/Resources/licenses/MANIFEST.tsv" \
        | head -1 > "$tmp/unattributed/Contents/Resources/licenses/MANIFEST.tsv.new"
    mv "$tmp/unattributed/Contents/Resources/licenses/MANIFEST.tsv.new" \
       "$tmp/unattributed/Contents/Resources/licenses/MANIFEST.tsv"
    check unattributed fail

    mk no-manifest;         rm -f "$tmp/no-manifest/Contents/Resources/licenses/MANIFEST.tsv"
    check no-manifest fail

    rm -rf "$tmp"
    [ "$failures" -eq 0 ] || { echo ":: self-test FAILED ($failures)"; return 1; }
    echo ":: self-test passed"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

APP="${1:?usage: verify-licenses.sh <path-to-.app> | --self-test}"
[ -d "$APP" ] || { echo "error: no bundle at $APP" >&2; exit 1; }

echo ":: Licence attribution for the redistributed runtime"
set +e
audit_bundle "$APP"
problems=$?
set -e
rows=$(($(wc -l < "$APP/Contents/Resources/licenses/MANIFEST.tsv" 2>/dev/null || echo 1) - 1))
if [ "$problems" -ne 0 ]; then
    echo "   FAIL — $problems problem(s) across $rows row(s)" >&2
    exit 1
fi
echo "   PASS — $rows formulae attributed, every row carrying real licence text"
echo "   NOTE: which licence covers which binary is NOT gate-enforced; see the manifest."
