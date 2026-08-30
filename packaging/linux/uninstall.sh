#!/usr/bin/env bash
#
# Removes a user-local Scribobulate install (see install.sh in this directory).
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
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_DIR/applications"

rm -fv "$BIN_DIR/scribobulate"
rm -fv "$DATA_DIR/icons/hicolor/scalable/apps/$APP_ID.svg"
rm -fv "$APP_DIR/scribobulate.desktop"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APP_DIR" || true
command -v gtk-update-icon-cache  >/dev/null 2>&1 && \
    gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true

echo
echo "Removed Scribobulate."
echo "Markdown files may still list it as default; pick another with:"
echo "  xdg-mime default <other>.desktop text/markdown"
