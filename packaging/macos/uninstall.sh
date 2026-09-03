#!/usr/bin/env bash
#
# Undoes packaging/macos/install.sh.
#
# WHAT IT REMOVES is exactly what that script created, and nothing else:
#   the `scribobulate` symlink in Homebrew's bin/ (the thing on PATH), the manual-page
#   symlinks in Homebrew's share/man/man{1,5}/, the anchored bundle at
#   ~/Applications/Scribobulate.app that they all resolve into, and any bundle left in
#   the build directory. install.sh removes its own build copy on success, so that last
#   one is normally already gone; it is swept here for the run that failed part-way and
#   for a bare `bundle.sh` invocation, because Launch Services registers a bundle sitting
#   in a build directory like any other and a second registration for one identifier is
#   the state both scripts exist to prevent.
#
# WHAT IT DELIBERATELY LEAVES: a copy dragged to /Applications from the .dmg. That is the
# redistributable route (dmg.sh), installed by the user rather than by this script, and a
# developer uninstall that quietly deletes it would be removing something it never put
# there. It is REPORTED instead, because the failure this script exists to end is an
# uninstaller that announces success while a working copy of the app is still installed.
#
# THE TWO ARE NOW DISTINCT BY CONSTRUCTION, which is what makes that split honest. The
# developer install anchors at ~/Applications and the .dmg route lands in /Applications,
# so "remove what I created, report what I did not" names two different bundles rather
# than two claims about one. It used to anchor inside the build directory, where a Finder
# drag to /Applications MOVED the bundle out from under the symlinks and left them
# dangling — at which point this script's account of what it had installed was wrong.
# install.sh's own gate now refuses to build a second bundle while another is installed,
# so the state this script is asked to clean up is the one it describes.
#
# It is idempotent: every removal is guarded, so a second run reports what is already gone
# rather than failing on it.
#
# Usage: packaging/macos/uninstall.sh [OUTPUT_DIR]   (default: target/macos, as install.sh)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
BUILT="$OUT_DIR/Scribobulate.app"
ANCHOR="$HOME/Applications/Scribobulate.app"

[ "$(uname -s)" = "Darwin" ] || { echo "error: macOS only (see packaging/linux/uninstall.sh)" >&2; exit 1; }

# The symlink lives in Homebrew's bin/ because install.sh put it there; without brew we
# cannot know which prefix that was, so say so rather than guessing at /opt/homebrew and
# /usr/local in turn and reporting a clean removal after checking the wrong one.
if command -v brew >/dev/null 2>&1; then
    LINK="$(brew --prefix)/bin/scribobulate"
    if [ -L "$LINK" ]; then
        echo ":: Removing $LINK -> $(readlink "$LINK")"
        rm -f "$LINK"
    elif [ -e "$LINK" ]; then
        # install.sh only ever creates a symlink here, so a regular file at this path came
        # from somewhere else and is not ours to delete.
        echo "warning: $LINK exists and is not a symlink; leaving it alone" >&2
    else
        echo ":: No symlink at $LINK"
    fi

    # The manual-page links install.sh created, under the same prefix.
    #
    # `[ -L ]` FIRST AND ALONE decides these, and the ordering is load-bearing rather than
    # stylistic: these point INTO the .app, so once the bundle is gone they are dangling,
    # and `[ -e ]` follows the link and is FALSE for a dangling one. An uninstall run after
    # the .app was already removed -- the ordinary case, since this script removes it below
    # and a re-run then finds it missing -- would report "nothing to remove" and leave the
    # links behind forever. `[ -L ]` is true for a link whether or not its target exists.
    MAN_DIR="$(brew --prefix)/share/man"
    for section in 1 5; do
        man_link="$MAN_DIR/man$section/scribobulate.$section.gz"
        if [ -L "$man_link" ]; then
            echo ":: Removing $man_link -> $(readlink "$man_link")"
            rm -f "$man_link"
        elif [ -e "$man_link" ]; then
            # install.sh only ever creates a symlink here; a real file came from elsewhere
            # (a future Homebrew formula, say) and is not ours to delete.
            echo "warning: $man_link exists and is not a symlink; leaving it alone" >&2
        else
            echo ":: No symlink at $man_link"
        fi
    done
else
    echo "warning: 'brew' not found, so the PATH symlink was not looked for." >&2
    echo "  install.sh places it at \$(brew --prefix)/bin/scribobulate, and the manual" >&2
    echo "  pages at \$(brew --prefix)/share/man/man{1,5}/scribobulate.{1,5}.gz." >&2
fi

# DO THE UNREGISTRATION, DO NOT HAND THE USER A COMMAND FOR IT. This script knows
# exactly which bundle paths it installed, which is the one thing a general-purpose
# instruction cannot know, and `lsregister -u` accepts a path whose directory is already
# gone (MEASURED: 20 registrations for deleted paths, all cleared this way). So the stale
# entry never outlives the uninstall and there is nothing left to advise about.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

# WHAT DID NOT HAPPEN IS TRACKED, because the closing message asserts that it did.
#
# The unregistration is best-effort by design -- failing to clear a Launch Services entry
# is not worth failing an uninstall over -- but "best-effort" and "silently claimed as
# done" are different things, and this file has already shipped the second one once: the
# `-kill` advice it used to print was a remedy that no-opped while reading as a remedy.
# A guard on an absolute path into a system framework is exactly the assumption that
# expires (this same version REMOVED `lsregister -kill`), so the failure is recorded and
# reported rather than swallowed.
LSREGISTER_MISSING=""
UNREG_FAILED=""

for app in "$ANCHOR" "$BUILT"; do
    if [ -d "$app" ]; then
        echo ":: Removing $app"
        # UNGUARDED ON PURPOSE, and it is `set -e` that makes it safe: this is a plain
        # statement rather than a condition, so a removal that fails (a permission error,
        # say) aborts the script here and never reaches the success message below. `rm -rf`
        # already exits 0 for a path that is not there, so the only non-zero it can return
        # is a real failure. Do not "tidy" a `|| true` onto it -- that is precisely what
        # would let a failed removal be reported as a completed one.
        rm -rf "$app"
    else
        echo ":: No bundle at $app"
    fi
    # Unconditionally, not only when the directory was there: a registration outliving
    # its bundle is exactly the case this exists for, and a previous run that removed the
    # bundle without unregistering it leaves one behind.
    if [ ! -x "$LSREGISTER" ]; then
        LSREGISTER_MISSING=1
    else
        "$LSREGISTER" -u "$app" 2>/dev/null || true
    fi
done

# THE EXIT CODE IS NOT THE ANSWER, THE DATABASE IS. `lsregister -u` returns non-zero for
# a path it holds no registration for, which is the ORDINARY case here -- $BUILT is
# normally absent because install.sh removes it on success. Believing the exit code made
# this script report a failed unregistration on a completely clean run. MEASURED: the
# entry cleared (dump count 1 -> 0) while the status said otherwise.
#
# So the effect is verified rather than the claim: read the database back and see whether
# the path is still in it. That is the same rule the bundle.sh signing step follows for
# the same reason -- an acting verb's exit status is a claim the tool makes about itself,
# and here it is a claim about the wrong question.
lsregister_bundle_paths() {
    "$LSREGISTER" -dump 2>/dev/null \
        | grep -o "^[[:space:]]*path:.*Scribobulate\.app" \
        | sed 's/^[[:space:]]*path:[[:space:]]*//' \
        | sort -u
}

if [ -n "$LSREGISTER_MISSING" ]; then
    UNREG_FAILED="$ANCHOR"$'\n'"$BUILT"$'\n'
else
    still_registered="$(lsregister_bundle_paths)"
    for app in "$ANCHOR" "$BUILT"; do
        if printf '%s\n' "$still_registered" | grep -qxF "$app"; then
            UNREG_FAILED="$UNREG_FAILED$app"$'\n'
        fi
    done
fi

DRAGGED="/Applications/Scribobulate.app"
if [ -d "$DRAGGED" ]; then
    echo
    echo "NOTE: $DRAGGED is also present."
    echo "  That copy came from the .dmg, which this script did not install and does not"
    echo "  remove. Drag it to the Trash, or: rm -rf '$DRAGGED'"
fi

echo
if [ -z "$UNREG_FAILED" ]; then
    echo "Removed the developer install, and unregistered both bundle paths from Launch"
    echo "Services, so no stale Dock or 'Open With' entry survives this run."
else
    echo "Removed the developer install."
    echo
    echo "BUT the Launch Services unregistration did NOT run for:"
    printf '%s' "$UNREG_FAILED" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p"
    done
    if [ -n "$LSREGISTER_MISSING" ]; then
        echo "  because lsregister was not found at"
        echo "    $LSREGISTER"
        echo "  Locate it under the LaunchServices framework's Support directory on this"
        echo "  version of macOS and run the command below with that path."
    else
        echo "  because the path is still in the Launch Services database after the"
        echo "  unregistration was attempted."
    fi
    echo "  So a stale Dock or 'Open With' entry MAY survive this run. Clear it with the"
    echo "  per-path command below; it works even though the bundle is already gone."
fi
echo
echo "To unregister any Scribobulate.app BY PATH -- a copy installed some other way, or"
echo "one this run could not clear:"
echo "  $LSREGISTER -u '/path/to/Scribobulate.app'"
echo "That works even when the path is already deleted. Do not reach for"
echo "'lsregister -kill': the option was REMOVED (MEASURED on macOS 26 / Darwin 25.0.5,"
echo "which answers \"the -kill option has been removed because it was dangerous and no"
echo "longer useful\" and changes nothing), and this message used to recommend it."
