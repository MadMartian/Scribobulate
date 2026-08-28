#!/usr/bin/env bash
#
# User-local installer for Scribobulate (no root required).
#
# Installs into ~/.local so it stays per-user and needs no sudo:
#   binary  -> ~/.local/bin/scribobulate
#   icon    -> ~/.local/share/icons/hicolor/scalable/apps/com.extollit.scribobulate.svg
#              (from data/icons/scalable/apps/, the same file bundled in-binary)
#   desktop -> ~/.local/share/applications/scribobulate.desktop
#   themes  -> ~/.local/share/scribobulate/themes.toml
#              (plus data/sprites/ beside it, for that copy's own references)
#
# Then registers Scribobulate as the default handler for Markdown files so a
# double-click in Dolphin (or any file manager) opens it here.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ID="com.extollit.scribobulate"

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"
APP_DIR="$DATA_DIR/applications"
THEME_DIR="$DATA_DIR/scribobulate"
BIN_PATH="$BIN_DIR/scribobulate"
DESKTOP_PATH="$APP_DIR/scribobulate.desktop"

echo ":: Building release binary"
cargo build --release --manifest-path "$REPO_DIR/Cargo.toml"

echo ":: Installing files"
install -Dm755 "$REPO_DIR/target/release/scribobulate" "$BIN_PATH"
# Sourced from data/icons/, the tree build.rs compiles into the binary's
# GResource — so the app icon the window resolves internally and the one the
# desktop shell reads off disk are byte-for-byte the same file. (The shell is a
# separate process and cannot see our GResource, so this copy is still needed.)
install -Dm644 "$REPO_DIR/data/icons/scalable/apps/$APP_ID.svg" "$ICON_DIR/$APP_ID.svg"
# Preview reading themes. The same file is compiled into the binary as a fallback,
# so this copy is what makes the themes readable/editable — and it is installed under
# XDG_DATA_HOME (never XDG_CONFIG_HOME, which the app redirects at startup for the
# XCompose workaround). A user override goes in ~/.config/scribobulate/themes.toml
# and is merged over this one per theme id.
install -Dm644 "$REPO_DIR/data/themes.toml" "$THEME_DIR/themes.toml"
# CANONICAL: why EVERY platform ships this copy of data/sprites/.
#
# The sprites that themes.toml's shipped themes name. NOT what makes those themes
# work -- a built-in theme's sprite is compiled into the binary (`include_bytes!`,
# src/sprite.rs) precisely so no install step can take a shipped decoration away.
# This copy exists because the installed themes.toml is itself read as a themes file
# (it lands on the themes search path), and its own sprite references resolve against
# its own directory; without the sprites beside it every launch would log a resolution
# failure for a decoration that is in fact rendering perfectly from the binary.
#
# packaging/linux/payload.sh and packaging/windows/stage.ps1 ship the same copy and
# point HERE instead of restating this. They used to carry it verbatim, all three,
# and that is precisely how the three commands underneath the three copies drifted
# into three different behaviours (empty directory, subdirectory, filename with a
# space) while the prose above them stayed identical and said nothing about it.
#
# `find ... -exec install -Dm644 -t ... {} +` rather than a glob: a glob with nothing
# to match stays literal and aborts the install, a glob that matches a subdirectory
# hands `install` an operand it refuses, and an unquoted glob word-splits a filename
# containing a space. This form survives all three, and the two shell scripts run the
# SAME command so they cannot answer those three cases differently again.
find "$REPO_DIR/data/sprites" -type f \
    -exec install -Dm644 -t "$THEME_DIR/sprites" {} +

# Copy the desktop entry, pinning Exec/TryExec to the absolute binary path so it
# launches regardless of whether $BIN_DIR is on the launcher's PATH.
mkdir -p "$APP_DIR"
sed -e "s|^Exec=scribobulate|Exec=$BIN_PATH|" \
    -e "s|^TryExec=scribobulate|TryExec=$BIN_PATH|" \
    "$REPO_DIR/data/scribobulate.desktop" > "$DESKTOP_PATH"
chmod 644 "$DESKTOP_PATH"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_PATH" && echo "   desktop entry valid"
fi

echo ":: Refreshing desktop & icon caches"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APP_DIR" || true
command -v gtk-update-icon-cache  >/dev/null 2>&1 && \
    gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true

echo ":: Registering as default handler for Markdown"
# text/markdown covers *.md / *.markdown via shared-mime-info; x-markdown is the
# legacy alias some setups still emit.
for mime in text/markdown text/x-markdown; do
    xdg-mime default scribobulate.desktop "$mime" || true
done

echo
echo "Installed Scribobulate."
echo "  binary  : $BIN_PATH"
echo "  desktop : $DESKTOP_PATH"
echo "  default : $(xdg-mime query default text/markdown 2>/dev/null || echo '?') for text/markdown"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "  NOTE: $BIN_DIR is not on your PATH; the launcher still works (absolute Exec)," \
            "but the 'scribobulate' command in a terminal will not until you add it." ;;
esac
echo
echo "Double-click a .md file in Dolphin to open it in Scribobulate."
echo "(If Dolphin still shows the old app, log out/in or restart it to reload the MIME cache.)"
