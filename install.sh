#!/usr/bin/env bash
#
# Developer install for Scribobulate -- a router, and deliberately nothing else.
#
# Each platform's install is a different job in a different vocabulary: Linux drops an XDG
# tree and registers a .desktop entry with the shell's MIME database; macOS builds an .app
# bundle and puts a symlink on PATH. This file owns neither. It picks one and hands over:
#
#   Linux   -> packaging/linux/install.sh
#   macOS   -> packaging/macos/install.sh
#
# so each platform's install sits in that platform's packaging directory beside the
# redistributable-artefact scripts already there, while the repository root keeps the one
# obvious entry point people actually type. Holding no install logic here is the point:
# the three platforms cannot answer the same install question two ways by accident when
# no file is in a position to answer it twice.
#
# WHY IT ROUTES RATHER THAN REFUSING. The Linux script's first act is a release build.
# Run on a Mac before this dispatch existed, it spent that build and only then died on the
# first line to touch the filesystem -- MEASURED on macOS 26.6.2 (Darwin 25.6.0, arm64):
# `install -Dm755` exits 71 with "No such file or directory", because macOS ships BSD
# install, which has no `-D` (create leading directories) flag. That is an error naming a
# path rather than a platform, arriving minutes after the mistake that caused it. The
# platform question has to be asked before the build, not left to whichever command
# happens to be the first incompatible one.
#
# Arguments pass through, which is what gives the macOS script its optional OUTPUT_DIR.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "$(uname -s)" in
    Linux)  exec "$REPO_DIR/packaging/linux/install.sh" "$@" ;;
    Darwin) exec "$REPO_DIR/packaging/macos/install.sh" "$@" ;;
    # A bash on Windows is the one non-Linux, non-macOS case with somewhere to go, so it
    # gets its own arm. Everything else gets a plain refusal rather than that pointer:
    # sending a *BSD user to the Windows packaging directory would be this script
    # committing the fault it exists to prevent -- an error naming the wrong platform.
    MINGW* | MSYS* | CYGWIN*)
        echo "error: this router is for the Linux and macOS developer installs." >&2
        echo "  Windows builds and packages from packaging/windows/ (PowerShell, not" >&2
        echo "  this script) -- see packaging/windows/README.md." >&2
        exit 1
        ;;
    *)
        echo "error: no developer install for $(uname -s)." >&2
        echo "  Supported: Linux (packaging/linux/install.sh) and macOS" >&2
        echo "  (packaging/macos/install.sh). Building from source directly is" >&2
        echo "  'cargo build --release' -- see README.md." >&2
        exit 1
        ;;
esac
