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
# ---------------------------------------------------------------------------------
# THE ANCHOR IS ~/Applications, NOT THE BUILD DIRECTORY, AND THAT IS THE POINT.
#
# This script used to point the PATH symlink and both manual-page symlinks straight
# at `$OUT_DIR/Scribobulate.app` — inside `target/`. Everything that legitimately
# empties a build directory then silently broke the install: `cargo clean`,
# `rm -rf target`, or moving the bundle to /Applications (a Finder drag WITHIN one
# volume is a move, not a copy).
#
# It broke SILENTLY, which is the part worth understanding, because the same trap is
# waiting for any future script that links into a directory it does not own. A
# dangling symlink is not executable, so the shell does not error on it — it SKIPS the
# entry and keeps walking PATH. MEASURED on this machine (macOS 26, Darwin 25.0.5):
# with `/opt/homebrew/bin/scribobulate` dangling, `command -v scribobulate` resolved
# to `~/.local/bin/scribobulate` — a five-day-old binary left behind by an old run of
# the LINUX installer, which nothing on this platform has ever known about. The
# operator built the current tree, ran this script, and got the stale binary with no
# diagnostic of any kind.
#
# So the bundle is COPIED out of the build directory to a stable anchor and everything
# resolves there. `~/Applications` is per-user (no sudo), and Launch Services scans it:
# MEASURED by dropping an unlaunched, never-`lsregister`-ed bundle there and finding it
# in `lsregister -dump` and resolvable by `osascript -e 'id of app "…"'` within three
# seconds. A bundle launched from there reports `type="Foreground"` with its own bundle
# path to `lsappinfo`, i.e. a real Dock tile and Cmd-Tab entry. No explicit registration
# call is needed and none is made.
#
# WHY NOT JUST POINT AT /Applications WHEN A COPY IS ALREADY THERE: because this
# script's first act is a release build, and it must never put anything on PATH other
# than the artefact it just produced. Pointing at a bundle it did not build reproduces
# the original defect exactly — build the latest, type `scribobulate`, run something
# older, no diagnostic — with a different mechanism and identical silence.
#
# THE TWO GATES BELOW EXIST BECAUSE THE ANCHOR ALONE DOES NOT GIVE ONE COPY.
# CFBundleIdentifier, not the path, is the app's identity, so a second bundle anywhere
# is a second registration for the same identity: with a copy in both /Applications and
# ~/Applications, `open -a Scribobulate` and `open -b com.extollit.scribobulate` both
# MEASURED as launching the /Applications one, while the PATH command ran ours. Two
# copies, silently divergent, decided by which route the user took. The gates run
# BEFORE the release build so a refusal costs seconds rather than minutes, and the PATH
# one runs again at the end, because the second half of what it checks is the thing this
# script just created.
#
# THEY REPORT AND REFUSE; THEY NEVER DELETE. Removing a bundle another route installed
# is the overreach uninstall.sh already declines to make for /Applications, and a
# developer install is not entitled to it just because it noticed.
#
# Usage: packaging/macos/install.sh [OUTPUT_DIR]   (default: target/macos, same as bundle.sh)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
BUILT="$OUT_DIR/Scribobulate.app"
ANCHOR_DIR="$HOME/Applications"
ANCHOR="$ANCHOR_DIR/Scribobulate.app"
DRAGGED="/Applications/Scribobulate.app"

[[ "$(uname)" == "Darwin" ]] || { echo "error: macOS only" >&2; exit 1; }

command -v brew >/dev/null 2>&1 || {
    echo "error: 'brew' not found. This project's macOS build already requires" >&2
    echo "  Homebrew for gtk4/gtksourceview5/adwaita-icon-theme (see README" >&2
    echo "  Quickstart), and this script uses its bin/ directory to put" >&2
    echo "  'scribobulate' on PATH without needing sudo." >&2
    exit 1
}
BIN_DIR="$(brew --prefix)/bin"
LINK="$BIN_DIR/scribobulate"
MAN_DIR="$(brew --prefix)/share/man"

# READ, NOT RESTATED. `packaging/macos/Info.plist.in` is where the bundle's
# CFBundleIdentifier is declared, and `cargo xtask lint-references` check 7 already holds
# that file against `src/icons.rs`. Deriving the value here keeps this script out of the
# set of files that can drift from the canonical ID — a literal copy here would be a new
# restatement with nothing checking it, which is the defect that check exists to prevent.
APP_ID="$(plutil -extract CFBundleIdentifier raw "$REPO_ROOT/packaging/macos/Info.plist.in")"
[ -n "$APP_ID" ] || {
    echo "error: could not read CFBundleIdentifier from packaging/macos/Info.plist.in" >&2
    exit 1
}

# --- Gate 1: exactly one Scribobulate bundle on this machine ----------------------
#
# TWO TIERS, AND THE MANDATORY ONE DOES NOT DEPEND ON AN INDEXER. The direct test of the
# two fixed locations is the gate; Spotlight is an extra that can widen it and must never
# be able to narrow it. `mdfind` was chosen over `lsregister -dump` on measurement, not
# taste: mdfind answered in 0.06s with exactly the live path, while lsregister took 2.1s
# and returned a pile of registrations for bundles that no longer exist (deleted /tmp
# staging directories, an unmounted .dmg volume) — a gate built on it would refuse to run
# over copies that are not there. Every mdfind hit is therefore existence-tested anyway.
#
# KNOW WHERE THE mdfind TIER STOPS. It is a Spotlight-INDEX query, so its coverage is
# whatever Spotlight indexes and it is blind everywhere Spotlight is excluded — which is
# not a corner case. MEASURED on this machine: /private/tmp held FIFTEEN real bundles
# (841 MB, executables present, every one carrying this identifier and registered with
# Launch Services) left by an earlier porting session, and mdfind returned exactly ONE
# path. Spotlight does not index /private/tmp; Launch Services registers it.
#
# That is a bound on the tier, not a hole in the gate, and the distinction is the reason
# the mandatory tier is a direct test rather than a query. The two fixed locations are the
# ones that decide the outcome — Launch Services ranks by location, and a /private/tmp
# bundle cannot outrank /Applications — so what mdfind misses cannot win. Do not promote
# mdfind to the mandatory tier on the strength of it usually agreeing.
found_foreign=""
note_transient=""

add_foreign() {
    case "$found_foreign" in
        *"$1"$'\n'*) ;;
        *) found_foreign="$found_foreign$1"$'\n' ;;
    esac
}

[ -d "$DRAGGED" ] && add_foreign "$DRAGGED"

if command -v mdfind >/dev/null 2>&1; then
    while IFS= read -r hit; do
        [ -n "$hit" ] || continue
        [ -d "$hit" ] || continue          # a stale index entry is not an installed copy
        case "$hit" in
            "$ANCHOR" | "$BUILT") continue ;;
            # A mounted disk image and the Trash both hold a bundle that is on the machine
            # without being installed on it; naming them as a refusal would make this gate
            # fire on an ordinary "I just opened the .dmg to look at it".
            /Volumes/* | "$HOME"/.Trash/*) note_transient="$note_transient$hit"$'\n' ;;
            *) add_foreign "$hit" ;;
        esac
    done < <(mdfind "kMDItemCFBundleIdentifier == '$APP_ID'" 2>/dev/null || true)
fi

if [ -n "$found_foreign" ]; then
    echo "error: another Scribobulate.app is already installed on this machine." >&2
    echo >&2
    printf '%s' "$found_foreign" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p" >&2
    done
    echo >&2
    echo "  It carries the same CFBundleIdentifier ($APP_ID) as the bundle this" >&2
    echo "  script builds, so both would register as the same application and the Dock," >&2
    echo "  'open -a Scribobulate' and the PATH command could end up running different" >&2
    echo "  copies. Nothing warns you when they diverge." >&2
    echo >&2
    echo "  Choose one and re-run:" >&2
    printf '%s' "$found_foreign" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    rm -rf '$p'      # remove that copy, then re-run this script" >&2
    done
    echo "    (or keep it, and do not run the developer install on this machine)" >&2
    echo >&2
    echo "  This script does not delete a bundle it did not install." >&2
    exit 1
fi

# --- Gate 2: nothing else answers to `scribobulate` on PATH -----------------------
#
# `[ -e ] || [ -L ]`, NOT `[ -e ]` ALONE, and the whole gate turns on it: the entry this
# was written to catch was a DANGLING symlink, for which `[ -e ]` is false and `[ -L ]`
# true. A gate that tested existence would have reported the machine clean on exactly the
# configuration that produced the bug.
#
# POSITION IS TAKEN FROM PATH ORDER, NOT FROM FINDING OUR OWN LINK. The obvious
# implementation — walk the hits and call everything after ours "later" — is wrong on the
# FIRST run, when our link does not exist yet: every hit would then read as earlier than a
# directory that has nothing in it, and an ordinary machine with one stale binary anywhere
# on PATH could never install. BIN_DIR's position in PATH is known without looking at the
# filesystem, so the classification uses that. A BIN_DIR that is not on PATH at all leaves
# every hit classified as earlier, which is correct: nothing we install can win from a
# directory that is never searched.
scan_path() {
    local dir p pos=before
    local IFS=:
    for dir in $PATH; do
        [ -n "$dir" ] || dir="."
        p="$dir/scribobulate"
        if [ "$dir" = "$BIN_DIR" ]; then
            pos=after
            if [ -e "$p" ] || [ -L "$p" ]; then
                printf 'ours\t%s\n' "$p"
            fi
            continue
        fi
        if [ -e "$p" ] || [ -L "$p" ]; then
            printf '%s\t%s\n' "$pos" "$p"
        fi
    done
}

# Prints nothing and returns 0 when clean; otherwise reports and returns 1 for a
# shadowing entry. Called twice — before the build so a refusal is cheap, and after the
# link is made, because until then half of what it inspects does not exist yet.
check_path() {
    local shadowing="" trailing="" pos p
    while IFS="$(printf '\t')" read -r pos p; do
        [ -n "$p" ] || continue
        case "$pos" in
            # In BIN_DIR, and ours only if it is the symlink this script creates. A regular
            # file here came from somewhere else and shadows us from our own directory.
            ours)   [ -L "$p" ] || shadowing="$shadowing$p"$'\n' ;;
            before) shadowing="$shadowing$p"$'\n' ;;
            *)      trailing="$trailing$p"$'\n' ;;
        esac
    done < <(scan_path | awk -F'\t' '!seen[$2]++')

    if [ -n "$trailing" ]; then
        echo >&2
        echo "NOTE: another 'scribobulate' is on PATH after $BIN_DIR:" >&2
        printf '%s' "$trailing" | while IFS= read -r p; do
            [ -n "$p" ] && echo "    $p -> $(readlink "$p" 2>/dev/null || echo 'regular file')" >&2
        done
        echo "  Ours wins today because it comes first, which is an accident of PATH" >&2
        echo "  order rather than a decision. Remove it when convenient." >&2
    fi

    [ -n "$shadowing" ] || return 0

    echo "error: another 'scribobulate' on PATH would win over this install." >&2
    echo >&2
    printf '%s' "$shadowing" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p -> $(readlink "$p" 2>/dev/null || echo 'regular file')" >&2
    done
    echo >&2
    echo "  These are searched before $BIN_DIR, where this script puts its symlink, so" >&2
    echo "  typing 'scribobulate' would keep running one of them. A DANGLING symlink" >&2
    echo "  counts: the shell skips it silently rather than failing, so it does not" >&2
    echo "  protect you — it just moves the resolution further down PATH." >&2
    echo >&2
    echo "  Remove it and re-run:" >&2
    printf '%s' "$shadowing" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    rm -f '$p'" >&2
    done
    echo >&2
    echo "  A likely origin on this platform is packaging/linux/install.sh having been" >&2
    echo "  run here before the top-level install.sh routed by platform; it stages a real" >&2
    echo "  binary into ~/.local/bin, which no macOS script has ever removed." >&2
    return 1
}

check_path || exit 1

# --- Build, anchor, link ----------------------------------------------------------
echo ":: Building Scribobulate.app"
"$REPO_ROOT/packaging/macos/bundle.sh" "$OUT_DIR"

echo ":: Anchoring $ANCHOR"
mkdir -p "$ANCHOR_DIR"
rm -rf "$ANCHOR"
# `ditto` rather than `cp -R`: it is the macOS-native copy and preserves extended
# attributes and the ad-hoc code signature bundle.sh applies. A bundle whose signature
# did not survive the copy is refused at launch in a way that reads as corruption.
ditto "$BUILT" "$ANCHOR"
codesign --verify --deep --strict "$ANCHOR" 2>/dev/null || {
    echo "error: the code signature did not survive the copy to $ANCHOR" >&2
    echo "  bundle.sh signs the bundle ad-hoc and macOS refuses a bundle whose" >&2
    echo "  signature does not verify, reporting it as damaged." >&2
    exit 1
}

# AND THEN THE BUILD COPY GOES, or this script leaves behind exactly the second
# registration its own gate refuses. MEASURED with a probe pair sharing one identifier,
# one in ~/Applications and one in target/macos, neither ever launched and neither
# manually registered: BOTH appear in `lsregister -dump` within seconds — a build
# directory is not exempt from Launch Services by virtue of being a build directory —
# and `open -b <id>` launches the ~/Applications one. Remove the anchor and the same
# command launches the target/macos one. So the build copy does not merely sit there: it
# is a live candidate that loses while the anchor exists and takes over the moment it
# does not, which is a stale build-directory bundle silently becoming the application.
#
# Deleting it is not the overreach the /Applications rule forbids. The distinction is
# authorship, not location: bundle.sh produced this one seconds ago in this same run,
# where a dragged copy was installed by the user through a different route. Only the
# .app is removed, never OUT_DIR itself, which may be a directory the caller named.
# dmg.sh is unaffected — it invokes bundle.sh itself rather than consuming a bundle
# somebody else left.
rm -rf "$BUILT"

TARGET="$ANCHOR/Contents/MacOS/scribobulate"
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
# would be a second original to drift. They resolve into the ANCHOR, not into the build
# directory, so `cargo clean` no longer dangles them. They still inherit the dangling-link
# failure mode when the anchored .app is deleted -- uninstall.sh tests with `[ -L ]`, which
# is true for a dangling link where `[ -e ]` is false.
for section in 1 5; do
    man_src="$ANCHOR/Contents/Resources/man/man$section/scribobulate.$section.gz"
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
            echo "  man $ANCHOR/Contents/Resources/man/man1/scribobulate.1.gz"
            ;;
    esac
fi

# The second run of the PATH gate. The first proved nothing shadowed us; this one proves
# the link we just made is what resolves, which is a different claim and the one the
# operator actually cares about.
check_path || exit 1

if [ -n "$note_transient" ]; then
    echo
    echo "NOTE: a Scribobulate.app is also present at:"
    printf '%s' "$note_transient" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p"
    done
    echo "  Not installed (a mounted disk image or the Trash), so it is not a conflict"
    echo "  today. It becomes one if it is copied to /Applications."
fi

echo
echo "Installed."
echo "  app : $ANCHOR"
echo "  cli : $LINK"
echo "  man : $MAN_DIR/man{1,5}/scribobulate.{1,5}.gz"
echo
echo "  Exactly one Scribobulate.app is installed. The build copy this run produced at"
echo "  $BUILT"
echo "  has been removed: Launch Services registers a bundle in a build directory like"
echo "  any other, so leaving it would be leaving a second candidate for the same"
echo "  identifier. Nothing above resolves into the build tree, so 'cargo clean' is safe."
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
