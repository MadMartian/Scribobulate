#!/usr/bin/env bash
#
# Removes a user-local Scribobulate install (see install.sh).
set -euo pipefail

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
