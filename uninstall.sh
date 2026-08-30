#!/usr/bin/env bash
#
# Removes a Scribobulate developer install -- a router, and deliberately nothing else.
# The counterpart to install.sh at this level, and the same shape for the same reason:
#
#   Linux   -> packaging/linux/uninstall.sh
#   macOS   -> packaging/macos/uninstall.sh
#
# ROUTING MATTERS MORE HERE THAN IT DOES FOR install.sh, because the failure is silent.
# Before this dispatch existed, this file was the Linux uninstaller unconditionally: run
# on a Mac it removed XDG paths that had never been created, the `rm -f` calls all
# succeeded on absent files, and it printed "Removed Scribobulate." while the .app and the
# `scribobulate` symlink in Homebrew's bin/ were both still installed and still working.
# An uninstaller that reports success and uninstalls nothing is worse than one that fails,
# because nothing prompts the user to look.
#
# Arguments pass through, which is what gives the macOS script its optional OUTPUT_DIR.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "$(uname -s)" in
    Linux)  exec "$REPO_DIR/packaging/linux/uninstall.sh" "$@" ;;
    Darwin) exec "$REPO_DIR/packaging/macos/uninstall.sh" "$@" ;;
    MINGW* | MSYS* | CYGWIN*)
        echo "error: this router is for the Linux and macOS developer installs." >&2
        echo "  Windows installs through Inno Setup and uninstalls through" >&2
        echo "  Apps & Features -- see packaging/windows/README.md." >&2
        exit 1
        ;;
    *)
        echo "error: no developer install to remove on $(uname -s)." >&2
        echo "  Supported: Linux (packaging/linux/uninstall.sh) and macOS" >&2
        echo "  (packaging/macos/uninstall.sh)." >&2
        exit 1
        ;;
esac
