#!/usr/bin/env bash
#
# Removes a user-local Scribobulate install (see install.sh beside it).
#
# IT MUST REMOVE EVERYTHING THAT SCRIPT INSTALLS. The payload is defined once, in
# packaging/linux/payload.sh, and this is the one place that has to be kept in step with
# it by hand — there is no "uninstall" direction to a payload definition, because the
# packages get theirs from dpkg/rpm's own file manifests and only the user-local route
# needs a remover. So when a file is added to `stage_payload`, add its removal here in
# the same change. The list below is deliberately written in the same order as that
# function, so the two can be read side by side.
#
# REACHED BY `./uninstall.sh` AT THE REPO ROOT, which is a router dispatching on
# `uname -s`; running this file directly is equivalent. It takes the XDG paths from the
# environment exactly as install.sh does, so it needs no repository anchor of its own and
# correctly removes an install made from a different checkout.
set -euo pipefail

# The router already dispatched, but a direct run must not be the lenient path: every path
# below is XDG, and on macOS this would delete nothing, report success, and leave the real
# install (an .app plus a symlink in Homebrew's bin/) in place.
[ "$(uname -s)" = "Linux" ] || { echo "error: Linux only (see packaging/macos/uninstall.sh)" >&2; exit 1; }

APP_ID="com.extollit.scribobulate"
PKG="scribobulate"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_DIR/applications"

rm -fv "$BIN_DIR/$PKG"
rm -fv "$APP_DIR/$PKG.desktop"
rm -fv "$DATA_DIR/icons/hicolor/scalable/apps/$APP_ID.svg"
rm -fv "$DATA_DIR/$PKG/themes.toml"
# `rm -rf` on the sprite directory rather than naming its files: its contents change
# between versions, so a named list would strand a sprite an older install shipped. The
# directory is wholly ours; the one ABOVE it is not, which is why that one goes through
# the `rmdir` sweep below instead. Left behind, these made an uninstall that printed
# success while a later install silently reused stale theme data.
rm -rfv "$DATA_DIR/$PKG/sprites"
rm -fv "$DATA_DIR/doc/$PKG/THIRD-PARTY-LICENSES.md"
rm -fv "$DATA_DIR/man/man1/$PKG.1.gz"
rm -fv "$DATA_DIR/man/man5/$PKG.5.gz"

# Only the directories this install created, and only when empty — `rmdir` without -p
# refuses a non-empty directory, which is the guard we want: a user who put something of
# their own in ~/.local/share/scribobulate keeps it, and nothing walks a tree deleting.
for d in "$DATA_DIR/$PKG" "$DATA_DIR/doc/$PKG"; do
    [ -d "$d" ] && rmdir "$d" 2>/dev/null || true
done

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APP_DIR" || true
command -v gtk-update-icon-cache  >/dev/null 2>&1 && \
    gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true

echo
echo "Removed Scribobulate."
echo "Markdown files may still list it as default; pick another with:"
echo "  xdg-mime default <other>.desktop text/markdown"
