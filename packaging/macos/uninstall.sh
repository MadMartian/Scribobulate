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

for app in "$ANCHOR" "$BUILT"; do
    if [ -d "$app" ]; then
        echo ":: Removing $app"
        rm -rf "$app"
    else
        echo ":: No bundle at $app"
    fi
    # Unconditionally, not only when the directory was there: a registration outliving
    # its bundle is exactly the case this exists for, and a previous run that removed the
    # bundle without unregistering it leaves one behind.
    if [ -x "$LSREGISTER" ]; then
        "$LSREGISTER" -u "$app" 2>/dev/null || true
    fi
done

DRAGGED="/Applications/Scribobulate.app"
if [ -d "$DRAGGED" ]; then
    echo
    echo "NOTE: $DRAGGED is also present."
    echo "  That copy came from the .dmg, which this script did not install and does not"
    echo "  remove. Drag it to the Trash, or: rm -rf '$DRAGGED'"
fi

echo
echo "Removed the developer install, and unregistered both bundle paths from Launch"
echo "Services, so no stale Dock or 'Open With' entry survives this run."
echo
echo "If you still see one for a copy installed some other way, unregister it BY PATH:"
echo "  $LSREGISTER -u '/path/to/Scribobulate.app'"
echo "That works even when the path is already deleted. Do not reach for"
echo "'lsregister -kill': the option was REMOVED (MEASURED on macOS 26 / Darwin 25.0.5,"
echo "which answers \"the -kill option has been removed because it was dangerous and no"
echo "longer useful\" and changes nothing), and this message used to recommend it."
