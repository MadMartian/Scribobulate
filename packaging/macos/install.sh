#!/usr/bin/env bash
#
# Put a `scribobulate` command on PATH, backed by the built .app bundle.
#
# WHAT THIS FIXES: bundle.sh produces Scribobulate.app, but nothing after that puts a
# `scribobulate` command in a terminal — launching means `open Scribobulate.app` or
# spelling out the full path to the binary inside it. This script symlinks the
# bundle's own executable into Homebrew's bin/ directory, which this project already
# requires for GTK4/GtkSourceView 5 (see README Quickstart) and which Homebrew's own
# installer already puts on PATH — so it reuses the one directory a macOS build of
# this project is already guaranteed to have writable and on PATH, rather than
# inventing a second one. `/usr/local/bin` would need sudo on a stock macOS install
# (prohibited, POLICY.md "Prohibited actions"); `~/.local/bin`, the Linux install.sh
# location, is not on PATH on macOS by default and nothing here puts it there.
#
# The symlink resolves to `Scribobulate.app/Contents/MacOS/scribobulate` — inside the
# bundle, not a separate copy — so a terminal launch runs the exact executable Finder
# or the Dock would, with its Dock/Cmd-Tab identity intact (bundle.sh's whole point);
# a bare copy sitting outside `Contents/MacOS/` would lose that identity.
#
# WHAT THIS IS NOT: the redistributable installer — that is dmg.sh (pipeline step
# 10). This is the developer-convenience counterpart to the top-level `install.sh` on
# Linux: it needs cargo and the Homebrew GTK libraries already on this machine.
#
# Usage: packaging/macos/install.sh [OUTPUT_DIR]   (default: target/macos, same as bundle.sh)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
APP="$OUT_DIR/Scribobulate.app"

[[ "$(uname)" == "Darwin" ]] || { echo "error: macOS only" >&2; exit 1; }

echo ":: Building Scribobulate.app"
"$REPO_ROOT/packaging/macos/bundle.sh" "$OUT_DIR"

command -v brew >/dev/null 2>&1 || {
    echo "error: 'brew' not found. This project's macOS build already requires" >&2
    echo "  Homebrew for gtk4/gtksourceview5/adwaita-icon-theme (see README" >&2
    echo "  Quickstart), and this script uses its bin/ directory to put" >&2
    echo "  'scribobulate' on PATH without needing sudo." >&2
    exit 1
}
BIN_DIR="$(brew --prefix)/bin"
LINK="$BIN_DIR/scribobulate"
TARGET="$APP/Contents/MacOS/scribobulate"

echo ":: Linking $LINK -> $TARGET"
mkdir -p "$BIN_DIR"
ln -sf "$TARGET" "$LINK"

# --- Manual pages ---------------------------------------------------------------
#
# THE DIRECTORY WAS MEASURED, not assumed. `manpath` on a stock Mac reports Homebrew's
# prefix (`/opt/homebrew/share/man` here) among the searched directories, and reports NO
# per-user one -- `~/.local/share/man`, where the Linux install.sh puts these, is an XDG
# convention that macOS's man does not search. So the pages go under the same Homebrew
# prefix already justified for the executable above: writable without sudo, and already
# searched. Check it on any host in doubt with `manpath`.
#
# SYMLINKS INTO THE BUNDLE, for the same reason the executable is one: bundle.sh already
# staged the substituted, compressed pages into Contents/Resources/man, and a copy here
# would be a second original to drift. It inherits the same dangling-link failure mode as
# the CLI symlink when the .app is deleted -- uninstall.sh tests with `[ -L ]`, which is
# true for a dangling link where `[ -e ]` is false.
MAN_DIR="$(brew --prefix)/share/man"
for section in 1 5; do
    man_src="$APP/Contents/Resources/man/man$section/scribobulate.$section.gz"
    man_link="$MAN_DIR/man$section/scribobulate.$section.gz"
    [ -f "$man_src" ] || { echo "error: bundle.sh did not stage $man_src" >&2; exit 1; }
    echo ":: Linking $man_link -> $man_src"
    mkdir -p "$MAN_DIR/man$section"
    ln -sf "$man_src" "$man_link"
done

# REPORTED, not assumed to have worked. A link in a directory man does not search is
# indistinguishable from a successful install until someone runs `man scribobulate` and
# gets nothing -- the same failure the PATH check below exists to pre-empt.
if command -v manpath >/dev/null 2>&1; then
    case ":$(manpath 2>/dev/null):" in
        *":$MAN_DIR:"*) ;;
        *)
            echo
            echo "NOTE: $MAN_DIR is not in your 'manpath' output, so 'man scribobulate'"
            echo "may not find the pages. Read them directly with:"
            echo "  man $APP/Contents/Resources/man/man1/scribobulate.1.gz"
            ;;
    esac
fi

echo
echo "Installed."
echo "  app : $APP"
echo "  cli : $LINK"
echo "  man : $MAN_DIR/man{1,5}/scribobulate.{1,5}.gz"
case ":$PATH:" in
    *":$BIN_DIR:"*)
        echo
        echo "Open a new terminal and run: scribobulate path/to/document.md"
        ;;
    *)
        echo
        echo "NOTE: $BIN_DIR is not on your PATH. Homebrew's own installer normally adds"
        echo "it (see 'brew shellenv' in Homebrew's post-install instructions); until it"
        echo "is, run the command by its full path: $LINK"
        ;;
esac
