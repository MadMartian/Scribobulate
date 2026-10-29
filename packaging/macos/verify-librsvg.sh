#!/usr/bin/env bash
#
# Is the librsvg this bundle delivers at or above the security floor?
#
# CVE-2026-96889 / RUSTSEC-2026-0305 (librsvg #1241): a use-after-free when a nested
# XInclude redefines an XML entity, fixed in librsvg 2.63.2. Every SVG a document references
# reaches librsvg through the gdk-pixbuf SVG loader, and that loader follows a `data:`
# XInclude with no base URL. MEASURED on 2.62.3: a `data:` include drew into the pixbuf. The
# application's own XInclude screen stands down when it runs from a bundle carrying a staged
# librsvg (`platform::svg_decoder_is_delivered_fixed`), because this bundle carries the fix,
# so THIS GATE IS THE ONLY THING standing between a stale Homebrew keg and a delivered
# use-after-free.
#
# THE VERSION IS READ FROM THE STAGED DYLIB, by loading it and reading its exported
# `rsvg_{major,minor,micro}_version`. Not from `pkg-config` and not from `brew list`: both
# describe Homebrew, and the artefact is what is delivered. `brew upgrade` leaves the old keg in
# the Cellar beside the new one, so "Homebrew has 2.63.2" and "the bundle has 2.63.2" are two
# different claims.
#
# RUN IT ONLY ON A SIGNED BUNDLE. The staged copies have had install_name_tool run over
# them, which invalidates their signatures, and on Apple Silicon the kernel SIGKILLs a
# process that loads one (see the loader-cache step in bundle.sh). bundle.sh therefore calls
# this AFTER codesign. A reader killed that way exits non-zero and this gate fails closed.
#
# FAILS CLOSED in every other way too: no librsvg in the bundle, no python3, a dylib that
# will not load, or missing version symbols are all failures, never a skipped check. A
# bundle without librsvg also has no working SVG loader, so there is nothing to pass.
#
# Usage: packaging/macos/verify-librsvg.sh <path-to-.app | path-to-librsvg-2.2.dylib>
#        packaging/macos/verify-librsvg.sh --self-test
#
# Negative control (not in --self-test, because it needs an old keg that comes and goes):
#        packaging/macos/verify-librsvg.sh /opt/homebrew/Cellar/librsvg/2.62.3/lib/librsvg-2.2.dylib
# must FAIL.
set -euo pipefail

FLOOR="2.63.2"

# 0 when $1 >= $2, numerically per component (so 2.100.0 > 2.63.2, which a string compare
# gets wrong).
version_at_least() {
    local IFS=.
    local -a have=($1) want=($2)
    local i h w
    for i in 0 1 2; do
        h="${have[$i]:-0}"; w="${want[$i]:-0}"
        [[ "$h" =~ ^[0-9]+$ && "$w" =~ ^[0-9]+$ ]] || return 2
        if [ "$h" -gt "$w" ]; then return 0; fi
        if [ "$h" -lt "$w" ]; then return 1; fi
    done
    return 0
}

# Prints the runtime version of the librsvg at $1, or fails.
#
# Through Python's ctypes, NOT a compiled C probe. MEASURED on this seat (Xcode 26.5 with the
# macOS 27 Command Line Tools SDK installed beside it): `cc` could not link even a hello-world
# ("tapi error: malformed file ... unknown architecture arm64e.x1") while cargo still built
# the app, so a C probe turned a toolchain mismatch into a false "librsvg unreadable".
# `-I` keeps the interpreter from importing anything out of the working directory.
read_version() {
    command -v python3 >/dev/null 2>&1 \
        || { echo "error: python3 is needed to read the librsvg version" >&2; return 1; }
    python3 -I -c '
import ctypes, sys
lib = ctypes.CDLL(sys.argv[1])
print(".".join(str(ctypes.c_uint.in_dll(lib, "rsvg_%s_version" % k).value)
               for k in ("major", "minor", "micro")))
' "$1"
}

self_test() {
    local failures=0 rc
    check() {  # $1 = have, $2 = expected (ok|fail)
        set +e; version_at_least "$1" "$FLOOR"; rc=$?; set -e
        if { [ "$2" = ok ] && [ "$rc" -eq 0 ]; } || { [ "$2" = fail ] && [ "$rc" -ne 0 ]; }; then
            echo "   ok    $1 vs $FLOOR"
        else
            echo "   FAILED $1 vs $FLOOR (expected $2, got $rc)"; failures=$((failures + 1))
        fi
    }
    # ANTI-VACUITY first: a comparator that refuses everything passes every "fail" case.
    check "$FLOOR" ok
    check 2.63.10 ok
    check 2.100.0 ok
    check 3.0.0 ok
    check 2.63.1 fail
    check 2.62.3 fail
    check 1.99.99 fail
    check garbage fail
    # The rpath gate's resolver. Lexical cases only, so nothing here depends on a bundle.
    rp() {  # $1 = object, $2 = rpath, $3 = expected (in|out)
        if rpath_is_internal /B/S.app "$1" "$2"; then rc=in; else rc=out; fi
        if [ "$rc" = "$3" ]; then
            echo "   ok    rpath $2 of ${1#/B/S.app/} is $3"
        else
            echo "   FAILED rpath $2 of ${1#/B/S.app/} (expected $3, got $rc)"; failures=$((failures + 1))
        fi
    }
    local fw=/B/S.app/Contents/Frameworks/libpixbufloader_svg.so
    # ANTI-VACUITY first: a resolver that refuses everything passes every "out" case.
    rp /B/S.app/Contents/MacOS/scribobulate @executable_path/../Frameworks in
    rp "$fw" @loader_path in
    rp "$fw" @loader_path/../lib in
    rp "$fw" @loader_path/../../.. out      # the measured hole: the .app's parent
    rp "$fw" @loader_path/../.. in          # the .app directory itself: the boundary
    rp "$fw" @loader_path/../../../S.app.evil/Contents out
    rp "$fw" /opt/homebrew/Cellar/jpeg-turbo/3.2.0/lib out
    rp "$fw" lib out                        # relative: the process's working directory
    rp "$fw" @executable_path/../../.. out

    # The readers FAIL CLOSED: each tool is replaced by a stub that crashes or answers
    # nothing, and every reader must return exactly 2, never an empty "nothing found".
    # ANTI-VACUITY first: the real tools on a real universal binary read cleanly.
    local tmp; tmp="$(mktemp -d)"
    reader() {  # $1 = stub dir ("" = real tools), $2 = expected rc, rest = the call
        local stubs="$1" want="$2"; shift 2
        rc=0; ( [ -z "$stubs" ] || PATH="$stubs:$PATH"; "$@" >/dev/null 2>&1 ) || rc=$?
        if [ "$rc" -eq "$want" ]; then
            echo "   ok    $* -> $want${stubs:+ (${stubs##*/})}"
        else
            echo "   FAILED $*${stubs:+ (${stubs##*/})} (expected $want, got $rc)"; failures=$((failures + 1))
        fi
    }
    stub() {  # $1 = case dir, $2 = tool, $3 = body
        mkdir -p "$tmp/$1"; printf '#!/bin/sh\n%s\n' "$3" > "$tmp/$1/$2"; chmod +x "$tmp/$1/$2"
    }
    stub otool-crashes otool 'exit 1'
    stub otool-headers-only otool 'eval "last=\${$#}"; echo "$last:"'
    stub file-crashes file 'exit 1'
    stub file-silent file 'exit 0'
    printf 'text\n' > "$tmp/not-macho"
    reader "" 0 load_deps /bin/ls
    reader "" 0 rpaths_of /bin/ls
    reader "" 0 is_macho /bin/ls
    reader "" 1 is_macho "$tmp/not-macho"
    reader "$tmp/otool-crashes" 2 load_deps /bin/ls
    reader "$tmp/otool-crashes" 2 rpaths_of /bin/ls
    reader "$tmp/otool-crashes" 2 install_id /bin/ls
    reader "$tmp/otool-headers-only" 2 load_deps /bin/ls
    reader "$tmp/otool-headers-only" 2 rpaths_of /bin/ls
    reader "$tmp/file-crashes" 2 is_macho /bin/ls
    reader "$tmp/file-silent" 2 is_macho /bin/ls

    # Load-command and install-ID references, resolved before they are judged
    # (reference_problem). Against a real staged file, so "exists" is tested too.
    local app="$tmp/S.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks"
    : > "$app/Contents/Frameworks/liba.dylib"
    : > "$tmp/beside.dylib"                 # exists, so only resolution can refuse it
    local obj="$app/Contents/Frameworks/liba.dylib"
    ref() {  # $1 = reference, $2 = expected (in|out)
        if [ -z "$(reference_problem "$app" "$obj" "$1")" ]; then rc=in; else rc=out; fi
        if [ "$rc" = "$2" ]; then
            echo "   ok    reference $1 is $2"
        else
            echo "   FAILED reference $1 (expected $2, got $rc)"; failures=$((failures + 1))
        fi
    }
    # ANTI-VACUITY first: a judge that refuses everything passes every "out" case.
    ref @rpath/liba.dylib in
    ref /usr/lib/libSystem.B.dylib in
    ref /System/Library/Frameworks/AppKit.framework/AppKit in
    ref @loader_path/liba.dylib in
    ref @executable_path/../Frameworks/liba.dylib in
    ref @rpath/libmissing.dylib out
    ref @rpath/../../../beside.dylib out
    ref /usr/lib/../../opt/homebrew/lib/libglib-2.0.0.dylib out
    ref /System/Library/../../opt/homebrew/lib/libglib-2.0.0.dylib out
    ref @loader_path/../../../beside.dylib out
    ref /opt/homebrew/lib/libglib-2.0.0.dylib out
    ref liba.dylib out                      # relative: the process's working directory
    # A glob character in a component is a name, not a pattern.
    if [ "$(normalize_path "$tmp/*")" = "$tmp/*" ]; then
        echo "   ok    normalize_path leaves * unexpanded"
    else
        echo "   FAILED normalize_path expanded *"; failures=$((failures + 1))
    fi
    rm -rf "$tmp"
    # The reader, end to end, against whatever librsvg Homebrew links. Proves it loads the
    # dylib and reads three numbers; deliberately NOT a floor check, because the
    # self-test must not depend on which librsvg this machine happens to have.
    local linked=/opt/homebrew/opt/librsvg/lib/librsvg-2.2.dylib got
    if [ -e "$linked" ]; then
        if got="$(read_version "$linked")" && [[ "$got" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "   ok    reader got $got from $linked"
        else
            echo "   FAILED reader got no version from $linked"; failures=$((failures + 1))
        fi
    else
        echo "   SKIPPED reader case: no Homebrew librsvg at $linked"
    fi
    [ "$failures" -eq 0 ] || { echo ":: self-test FAILED ($failures)"; return 1; }
    echo ":: self-test passed"
}

# shellcheck source=rpath.sh
. "$(dirname "${BASH_SOURCE[0]}")/rpath.sh"

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

SUBJECT="${1:?usage: verify-librsvg.sh <path-to-.app | path-to-librsvg-2.2.dylib> | --self-test}"
if [ -d "$SUBJECT" ]; then
    DYLIB="$SUBJECT/Contents/Frameworks/librsvg-2.2.dylib"
    # THE STAGED LIBRSVG IS ONLY THE ONE LOADED IF NO RPATH LEAVES THE BUNDLE. dyld tries a
    # library's own rpaths before the executable's, so one pointing outside lets a file there
    # stand in for the copy whose version is read below (rpath.sh's header has the
    # measurement). Checked first, because a passing version would otherwise vouch for a
    # file the app may never load.
    exe_rpath_control "$SUBJECT" || { echo "error: positive control failed (fails closed)" >&2; exit 1; }
    escaping="$(escaping_rpaths "$SUBJECT")" || {
        echo "error: could not read the bundle's rpaths, so which librsvg loads is unknown (fails closed)" >&2
        exit 1
    }
    if [ -n "$escaping" ]; then
        echo "error: rpaths that leave the bundle, so a library outside it can replace a staged one:" >&2
        printf '%s\n' "$escaping" | awk -F'\t' '{n=split($1, p, "/"); print "       " p[n] " -> " $2}' >&2
        exit 1
    fi
    echo "   no rpath leaves the bundle"
else
    DYLIB="$SUBJECT"
fi
[ -f "$DYLIB" ] || { echo "error: no librsvg at $DYLIB (fails closed: no librsvg means no SVG loader)" >&2; exit 1; }

if ! HAVE="$(read_version "$DYLIB")"; then
    echo "error: could not read the librsvg version from $DYLIB" >&2
    exit 1
fi
if version_at_least "$HAVE" "$FLOOR"; then
    echo "   librsvg $HAVE >= $FLOOR"
else
    echo "error: $DYLIB is librsvg $HAVE, below the security floor $FLOOR (CVE-2026-96889)." >&2
    echo "       brew upgrade librsvg, then rebuild the bundle." >&2
    exit 1
fi
