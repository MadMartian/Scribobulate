#!/usr/bin/env bash
#
# Undoes packaging/macos/install.sh.
#
# WHAT IT REMOVES is exactly what that script created, and nothing else:
#   the `scribobulate` symlink in Homebrew's bin/ (the thing on PATH), and the
#   Scribobulate.app the symlink resolves into.
#
# WHAT IT DELIBERATELY LEAVES: a copy dragged to /Applications from the .dmg. That is the
# redistributable route (dmg.sh), installed by the user rather than by this script, and a
# developer uninstall that quietly deletes it would be removing something it never put
# there. It is REPORTED instead, because the failure this script exists to end is an
# uninstaller that announces success while a working copy of the app is still installed.
#
# It is idempotent: every removal is guarded, so a second run reports what is already gone
# rather than failing on it.
#
# Usage: packaging/macos/uninstall.sh [OUTPUT_DIR]   (default: target/macos, as install.sh)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
APP="$OUT_DIR/Scribobulate.app"

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
else
    echo "warning: 'brew' not found, so the PATH symlink was not looked for." >&2
    echo "  install.sh places it at \$(brew --prefix)/bin/scribobulate." >&2
fi

if [ -d "$APP" ]; then
    echo ":: Removing $APP"
    rm -rf "$APP"
else
    echo ":: No bundle at $APP"
fi

DRAGGED="/Applications/Scribobulate.app"
if [ -d "$DRAGGED" ]; then
    echo
    echo "NOTE: $DRAGGED is also present."
    echo "  That copy came from the .dmg, which this script did not install and does not"
    echo "  remove. Drag it to the Trash, or: rm -rf '$DRAGGED'"
fi

echo
echo "Removed the developer install."
echo "Launch Services can keep a stale entry for a bundle that no longer exists; it"
echo "clears on its own rescan, or immediately with:"
echo "  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -kill -r -domain local -domain user"
