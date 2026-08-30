#!/usr/bin/env bash
#
# User-local installer for Scribobulate (no root required).
#
# THE THIRD LINUX INSTALL ROUTE, and the one that needs a toolchain. build-deb.sh and
# build-rpm.sh beside it produce transferable artefacts for someone with no Rust and no
# GTK development packages; this one builds from source straight into ~/.local, so it
# needs cargo and the -dev libraries and cannot be handed to anyone. That is the whole
# difference, and it is why it lives here rather than being folded into them: same
# destination shape, same payload, different audience.
#
#   binary   -> ~/.local/bin/scribobulate
#   desktop  -> ~/.local/share/applications/scribobulate.desktop
#   icon     -> ~/.local/share/icons/hicolor/scalable/apps/<app-id>.svg
#   themes   -> ~/.local/share/scribobulate/themes.toml
#   man page -> ~/.local/share/man/man1/scribobulate.1.gz
#   notices  -> ~/.local/share/doc/scribobulate/THIRD-PARTY-LICENSES.md
#
# WHAT GOES WHERE IS NOT DECIDED HERE. payload.sh owns the layout and all three routes
# read it, because XDG's user tree is shaped like /usr — so this is one payload with a
# different anchor, not a second one. Add a file there, and the packages and this script
# gain it together. Written twice, they drift, and a drifted layout is invisible: each
# route installs cleanly on its own and nothing compares them.
#
# Then registers Scribobulate as the default handler for Markdown files so a
# double-click in Dolphin (or any file manager) opens it here.
#
# Usage:
#   packaging/linux/install.sh          # build, then install into ~/.local
#   packaging/linux/install.sh --no-build   # install an existing release binary
#
# REACHED BY `./install.sh` AT THE REPO ROOT, which is a router that dispatches on
# `uname -s`; running this file directly is equivalent. The root script holds no install
# logic of its own, so the three platforms cannot answer an install question differently
# by accident — each one's answer lives in its own packaging/<os>/ directory.
set -euo pipefail

# The router already dispatched on the platform, but this script is directly runnable and
# a direct run must not be the lenient path. `install -D`, `xdg-mime` and the desktop
# database are all Linux here: on macOS the first of them fails with a message naming a
# path rather than the platform, minutes into a release build.
[ "$(uname -s)" = "Linux" ] || { echo "error: Linux only (see packaging/macos/install.sh)" >&2; exit 1; }

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
# shellcheck source=packaging/linux/payload.sh
. "$repo/packaging/linux/payload.sh"

VERSION="$(read_version)"
[ -n "$VERSION" ] || { echo "install: could not read version from Cargo.toml" >&2; exit 1; }

# XDG_BIN_HOME is not a real XDG variable — there is no such key in the basedir spec,
# which stops at DATA/CONFIG/STATE/CACHE. It is honoured anyway because it is a common
# convention and someone who sets it means it; ~/.local/bin is the default either way.
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_PATH="$BIN_DIR/$PKG"
APP_DIR="$DATA_DIR/applications"
DESKTOP_PATH="$APP_DIR/$PKG.desktop"

BIN="target/release/$PKG"
if [ "${1:-}" = "--no-build" ]; then
    require_fresh_binary "$BIN"
else
    echo ":: Building release binary"
    cargo build --release
fi

# The two anchors have to agree, and they only do by default. payload.sh writes
# <root>/bin and <root>/share/…, so it can honour a redirected XDG_DATA_HOME or a
# redirected XDG_BIN_HOME, but not both at once when they disagree about their parent.
# Refuse rather than install half the payload somewhere the user did not ask for.
root="${DATA_DIR%/share}"
if [ "$BIN_DIR" != "$root/bin" ] || [ "$DATA_DIR" != "$root/share" ]; then
    echo "install: XDG_BIN_HOME ($BIN_DIR) and XDG_DATA_HOME ($DATA_DIR) do not share a" >&2
    echo "         parent, so one payload cannot satisfy both. Unset one, or set them to" >&2
    echo "         <prefix>/bin and <prefix>/share." >&2
    exit 1
fi

echo ":: Installing files"
# Empty prefix: root IS the anchor, so paths land at ~/.local/bin and ~/.local/share/…
# rather than ~/.local/usr/…. The absolute Exec path is the other user-local difference
# — ~/.local/bin is frequently absent from the launcher's PATH where /usr/bin never is.
stage_payload "$root" "$BIN" "$VERSION" "" "$BIN_PATH"

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
    xdg-mime default "$PKG.desktop" "$mime" || true
done

echo
echo "Installed Scribobulate $VERSION."
echo "  binary  : $BIN_PATH"
echo "  desktop : $DESKTOP_PATH"
echo "  notices : $DATA_DIR/doc/$PKG/THIRD-PARTY-LICENSES.md"
echo "  default : $(xdg-mime query default text/markdown 2>/dev/null || echo '?') for text/markdown"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "  NOTE: $BIN_DIR is not on your PATH; the launcher still works (absolute Exec)," \
            "but the '$PKG' command in a terminal will not until you add it." ;;
esac
echo
echo "Double-click a .md file in Dolphin to open it in Scribobulate."
echo "(If Dolphin still shows the old app, log out/in or restart it to reload the MIME cache.)"
