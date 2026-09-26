#!/usr/bin/env bash
#
# Undoes packaging/macos/install.sh.
#
# MIRROR THE MODE YOU INSTALLED WITH, because the mode is what chooses the anchor:
#
#     ./uninstall.sh          -> per-user, anchor ~/Applications
#     sudo ./uninstall.sh     -> global,   anchor /Applications
#
# It shares packaging/macos/mode.sh with install.sh rather than restating any of it. An
# uninstaller that resolved the anchor one line differently from the installer would
# report a clean removal having looked in the wrong place — the exact failure this script
# exists to end — and nothing could have caught the divergence.
#
# WHAT IT REMOVES is exactly what that script created, and nothing else:
#   the `scribobulate` symlink in the mode's bin/ (the thing on PATH), the manual-page
#   symlinks in its share/man/man{1,5}/ (see mode.sh), the anchored bundle those resolve into,
#   and any bundle left in the build directory. install.sh removes its own build copy on
#   success, so that last one is normally already gone; it is swept here for the run that
#   failed part-way and for a bare `bundle.sh` invocation, because Launch Services
#   registers a bundle sitting in a build directory like any other and a second
#   registration for one identifier is the state both scripts exist to prevent.
#
# WHAT IT DELIBERATELY LEAVES: the OTHER mode's bundle, and any /Applications copy this
# install did not put there. A developer uninstall that quietly deleted a bundle another
# route installed would be overreaching. Those are REPORTED instead, because the failure
# this script exists to end is an uninstaller that announces success while a working copy
# of the app is still installed.
#
# THE SPLIT USED TO REST ON GEOGRAPHY AND NO LONGER CAN. While the developer install could
# only anchor at ~/Applications and the .dmg route only landed in /Applications, "remove
# what I created, report what I did not" named two different bundles by construction. With
# a global mode both routes target /Applications, so the location no longer says who put
# it there and the question has to be ASKED:
#
#   THE AUTHORSHIP TEST — in global mode, /Applications/Scribobulate.app is removed only
#   if the PATH symlink resolves INTO it. That symlink is created by install.sh and by
#   nothing else, so it pointing at that bundle is proof this script's installer put it
#   there, rather than an inference from where it happens to sit. A .dmg copy has no such
#   symlink aimed at it and survives a developer uninstall untouched. The link target is
#   therefore read BEFORE the link is removed — the order is load-bearing, since removing
#   it first would destroy the only evidence of authorship.
#
# EVERY REMOVAL IS VERIFIED, not merely attempted. Printing ":: Removing X" and exiting 0
# without re-testing X is how an uninstaller comes to claim a removal it did not perform —
# and in global mode there is a live way for that to happen (a root-owned artefact and a
# non-sudo run). The re-test is two syscalls and is mode-independent, so it runs in both.
#
# It is idempotent: every removal is guarded, so a second run reports what is already gone
# rather than failing on it.
#
# Usage: packaging/macos/uninstall.sh [OUTPUT_DIR]   (default: target/macos, as install.sh)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/target/macos}"
BUILT="$OUT_DIR/Scribobulate.app"

[ "$(uname -s)" = "Darwin" ] || { echo "error: macOS only (see packaging/linux/uninstall.sh)" >&2; exit 1; }

# Same resolver install.sh uses — MODE, ANCHOR, FOREIGN, USER_HOME, and the global-mode
# PATH fix that has to land before the brew lookup below.
# shellcheck source=packaging/macos/mode.sh
. "$REPO_ROOT/packaging/macos/mode.sh"

announce_mode

# THE POSTCONDITION, CHECKED. `rm` reports its own success; this asks the filesystem.
# `[ -L ]` as well as `[ -e ]`, for the same reason install.sh's PATH gate needs both: a
# dangling symlink is invisible to `[ -e ]`, so an `[ -e ]`-only check would call a link
# that is still sitting there "gone".
REMOVAL_FAILED=""
verify_gone() {
    if [ -e "$1" ] || [ -L "$1" ]; then
        REMOVAL_FAILED="$REMOVAL_FAILED$1"$'\n'
        echo "error: $1 is still present after removal" >&2
        return 1
    fi
    return 0
}

# THE AUTHORSHIP PREDICATE, and it governs every artefact this script removes -- the
# bundle AND the symlinks. install.sh points all three links into the anchor it just
# built, so a link resolving into $ANCHOR is proof this mode's install created it. A link
# pointing anywhere else belongs to the other mode, and removing it is the same overreach
# the bundle test exists to prevent, on a smaller artefact.
#
# IT IS NOT COSMETIC. Without it, running the WRONG mode half-destroys a live install:
# with a per-user install live, `sudo packaging/macos/uninstall.sh` would correctly
# decline the /Applications bundle and still have deleted the PATH symlink and both
# manual-page links on the way there -- leaving the ~/Applications bundle installed with
# no `scribobulate` command and no manual pages, and a run that reported success. A
# wrong-mode uninstall must be a no-op with a remedy, not a partial teardown.
points_into_anchor() {
    case "$1" in
        "$ANCHOR"/*) return 0 ;;
        *)           return 1 ;;
    esac
}

# Links left in place because they belong to the other mode. Reported at the end with the
# command that does remove them.
FOREIGN_LINKS=""

# NO `brew` ANYWHERE IN THIS SCRIPT ANY MORE. It used to derive both directories from
# `brew --prefix`, and carried a caveat that without brew it could not know which prefix
# to look in -- so a machine that had removed Homebrew could not be cleaned up by the
# script that had installed there. mode.sh now names BIN_DIR and MAN_DIR outright, the
# caveat is gone, and the uninstall works on a machine with no Homebrew at all.

# Captured before the link is removed, because the anchor's authorship test below reads
# it. Empty when there is no link, which classify_link_target reports as `orphan` -- and
# orphan correctly means "not proven ours", so the anchor is left alone.
LINK_TARGET=""

FOREIGN_LINKS=""
ORPHAN_LINKS=""

# ANYTHING AT ALL ACTUALLY REMOVED BY THIS RUN. The no-op message at the bottom asserts
# "Nothing was removed", and an assertion about an effect has to be gated on the same
# condition as the effect. Derived from the anchor alone, it claimed nothing was removed
# in a run that had just deleted a PATH symlink -- true about the bundle, false as
# printed, and printed is what the operator reads.
REMOVED_ANY=""

# ONE ROUTINE FOR ALL THREE LINKS, because they are the same decision three times and the
# only thing that differs is the path. Each is classified on its OWN readlink: they are
# independent artefacts, a half-migrated machine can genuinely have them pointing at
# different bundles, and deciding all three from one of them would be guessing about the
# other two.
handle_link() {
    link_path="$1"
    link_target=""

    if [ -L "$link_path" ]; then
        link_target="$(readlink "$link_path" 2>/dev/null || true)"
        case "$(classify_link_target "$link_target")" in
            ours)
                echo ":: Removing $link_path -> $link_target"
                rm -f "$link_path"
                verify_gone "$link_path" || true
                REMOVED_ANY=1
                ;;
            foreign)
                echo ":: Leaving $link_path -> $link_target"
                echo "   It resolves into the $OTHER_LABEL install, which this run does not touch."
                FOREIGN_LINKS="$FOREIGN_LINKS$link_path"$'\n'
                ;;
            orphan)
                echo ":: Leaving $link_path -> $link_target"
                echo "   It resolves into neither Applications directory, so no uninstall"
                echo "   owns it."
                ORPHAN_LINKS="$ORPHAN_LINKS$link_path"$'\n'
                ;;
        esac
    elif [ -e "$link_path" ]; then
        # install.sh only ever creates a symlink at these paths, so a regular file came
        # from somewhere else and is not ours to delete.
        echo "warning: $link_path exists and is not a symlink; leaving it alone" >&2
    else
        echo ":: No symlink at $link_path"
    fi
}

[ -L "$LINK" ] && LINK_TARGET="$(readlink "$LINK" 2>/dev/null || true)"
handle_link "$LINK"

for section in 1 5; do
    handle_link "$MAN_DIR/man$section/scribobulate.$section.gz"
done

# DO THE UNREGISTRATION, DO NOT HAND THE USER A COMMAND FOR IT. This script knows
# exactly which bundle paths it installed, which is the one thing a general-purpose
# instruction cannot know, and `lsregister -u` accepts a path whose directory is already
# gone (MEASURED: 20 registrations for deleted paths, all cleared this way). So the stale
# entry never outlives the uninstall and there is nothing left to advise about.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

# WHAT DID NOT HAPPEN IS TRACKED, because the closing message asserts that it did.
#
# The unregistration is best-effort by design -- failing to clear a Launch Services entry
# is not worth failing an uninstall over -- but "best-effort" and "silently claimed as
# done" are different things, and this file has already shipped the second one once: the
# `-kill` advice it used to print was a remedy that no-opped while reading as a remedy.
# A guard on an absolute path into a system framework is exactly the assumption that
# expires (this same version REMOVED `lsregister -kill`), so the failure is recorded and
# reported rather than swallowed.
LSREGISTER_MISSING=""
UNREG_FAILED=""

# --- THE AUTHORSHIP TEST ----------------------------------------------------------
#
# Only in global mode, and only because the answer stopped being free there. In per-user
# mode the anchor is ~/Applications/Scribobulate.app, a path nothing but install.sh ever
# writes, so "did we install this?" is answered by the location. /Applications is not that
# path: the .dmg lands there too, so the location is no longer evidence and the PATH
# symlink is asked instead. It is created by install.sh and by nothing else, so a link
# aimed into this bundle is proof of authorship rather than an inference from geography.
#
# A missing or foreign LINK_TARGET means NOT PROVEN, and not-proven leaves the bundle
# alone. That is the safe direction: refusing to delete a bundle we did install costs a
# printed line, while deleting one we did not is unrecoverable.
ANCHOR_UNPROVEN=""
REMOVE_LIST="$BUILT"$'\n'

if [ -d "$ANCHOR" ]; then
    if [ "$MODE" != "global" ]; then
        REMOVE_LIST="$ANCHOR"$'\n'"$REMOVE_LIST"
    else
        if points_into_anchor "$LINK_TARGET"; then
            REMOVE_LIST="$ANCHOR"$'\n'"$REMOVE_LIST"
        else
            ANCHOR_UNPROVEN=1
        fi
    fi
else
    # Kept in the list when absent: the removal loop is also what unregisters, and a
    # Launch Services entry outliving its bundle is exactly the case that needs clearing.
    REMOVE_LIST="$ANCHOR"$'\n'"$REMOVE_LIST"
fi

if [ -n "$ANCHOR_UNPROVEN" ]; then
    echo ":: Leaving $ANCHOR"
    echo "   The PATH symlink does not resolve into it, so this install did not put it"
    echo "   there — it is a .dmg copy or another route's. Not removed, and its Launch"
    echo "   Services registration is left intact because the bundle is still installed."
fi

while IFS= read -r app; do
    [ -n "$app" ] || continue
    if [ -d "$app" ]; then
        echo ":: Removing $app"
        # UNGUARDED ON PURPOSE, and it is `set -e` that makes it safe: this is a plain
        # statement rather than a condition, so a removal that fails (a permission error,
        # say) aborts the script here and never reaches the success message below. `rm -rf`
        # already exits 0 for a path that is not there, so the only non-zero it can return
        # is a real failure. Do not "tidy" a `|| true` onto it -- that is precisely what
        # would let a failed removal be reported as a completed one.
        rm -rf "$app"
        REMOVED_ANY=1
        # AND THEN ASK THE FILESYSTEM. `rm -rf` exiting 0 is its account of itself; this
        # is the postcondition. It costs two syscalls and it is the difference between
        # "removed" and "attempted to remove", which is the whole claim of this script.
        verify_gone "$app" || true
    else
        echo ":: No bundle at $app"
    fi
    # Unconditionally, not only when the directory was there: a registration outliving
    # its bundle is exactly the case this exists for, and a previous run that removed the
    # bundle without unregistering it leaves one behind.
    if [ ! -x "$LSREGISTER" ]; then
        LSREGISTER_MISSING=1
    else
        "$LSREGISTER" -u "$app" 2>/dev/null || true
    fi
done < <(printf '%s' "$REMOVE_LIST")

# THE EXIT CODE IS NOT THE ANSWER, THE DATABASE IS. `lsregister -u` returns non-zero for
# a path it holds no registration for, which is the ORDINARY case here -- $BUILT is
# normally absent because install.sh removes it on success. Believing the exit code made
# this script report a failed unregistration on a completely clean run. MEASURED: the
# entry cleared (dump count 1 -> 0) while the status said otherwise.
#
# So the effect is verified rather than the claim: read the database back and see whether
# the path is still in it. That is the same rule the bundle.sh signing step follows for
# the same reason -- an acting verb's exit status is a claim the tool makes about itself,
# and here it is a claim about the wrong question.
lsregister_bundle_paths() {
    "$LSREGISTER" -dump 2>/dev/null \
        | grep -o "^[[:space:]]*path:.*Scribobulate\.app" \
        | sed 's/^[[:space:]]*path:[[:space:]]*//' \
        | sort -u
}

# REMOVE_LIST, NOT A FIXED PAIR. An anchor the authorship test declined to remove is
# still installed and therefore still legitimately registered; naming it here would
# report a deliberate decision as an unregistration failure.
if [ -n "$LSREGISTER_MISSING" ]; then
    UNREG_FAILED="$REMOVE_LIST"
else
    still_registered="$(lsregister_bundle_paths)"
    while IFS= read -r app; do
        [ -n "$app" ] || continue
        if printf '%s\n' "$still_registered" | grep -qxF "$app"; then
            UNREG_FAILED="$UNREG_FAILED$app"$'\n'
        fi
    done < <(printf '%s' "$REMOVE_LIST")
fi

# THE OTHER MODE'S LOCATION, reported and never touched. In per-user mode this is
# /Applications (a .dmg copy, or a global install); in global mode it is the operator's
# ~/Applications (a per-user install). Either way it belongs to a different run and the
# remedy is that run's own uninstall, which also clears its Launch Services registration
# — something the `rm -rf` fallback leaves stranded.
# LINKS LEFT BEHIND ARE REPORTED, and the report is the whole point of leaving them.
# Silently declining to remove an artefact is indistinguishable from not having looked.
if [ -n "$FOREIGN_LINKS" ]; then
    echo
    echo "NOTE: these were left in place because they resolve into a bundle this mode did"
    echo "not install:"
    printf '%s' "$FOREIGN_LINKS" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p -> $(readlink "$p" 2>/dev/null || echo '?')"
    done
    echo "  They belong to the $OTHER_LABEL install. Remove that install, links and all, with:"
    echo "    cd '$REPO_ROOT' && $FOREIGN_UNINSTALL"
fi

# ORPHANS GET `rm -f`, NOT $FOREIGN_UNINSTALL. A link into neither Applications directory
# -- typically one left by a revision that pointed at target/macos before the install
# anchored outside the build tree -- is owned by no uninstall at all. Handing the operator
# the other mode's uninstall for it would be prescribing a command that runs, reports
# success, and removes nothing: a remedy that no-ops is worse than no remedy, because no
# remedy leaves them looking and an inert one ends the search.
if [ -n "$ORPHAN_LINKS" ]; then
    echo
    echo "NOTE: these resolve into neither Applications directory, so no uninstall owns"
    echo "them. They are most likely left over from an older layout that linked into the"
    echo "build tree. Remove them by hand:"
    printf '%s' "$ORPHAN_LINKS" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    rm -f '$p'      # -> $(readlink "$p" 2>/dev/null || echo '?')"
    done
fi

if [ -d "$FOREIGN" ]; then
    echo
    echo "NOTE: $FOREIGN is also present."
    echo "  That is the $OTHER_LABEL install, which this run does not remove."
    echo "  Remove it with:  cd '$REPO_ROOT' && $FOREIGN_UNINSTALL"
    echo "  Or directly (leaves a Launch Services entry behind): rm -rf '$FOREIGN'"
fi

# THE CLOSING CLAIM IS GATED ON THE POSTCONDITIONS, not on having reached this line.
# Everything below asserts "Removed the developer install", and an uninstaller that says
# that while a bundle or a symlink is still sitting there is the single failure this
# script exists to prevent. A non-zero exit as well, so a caller that chains off this
# script is not told it succeeded either.
if [ -n "$REMOVAL_FAILED" ]; then
    echo >&2
    echo "error: the uninstall did NOT complete. Still present:" >&2
    printf '%s' "$REMOVAL_FAILED" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p" >&2
    done
    echo >&2
    echo "  Most likely these were created by a run in the other mode and need it to" >&2
    echo "  remove them. Try:  cd '$REPO_ROOT' && $FOREIGN_UNINSTALL" >&2
    exit 1
fi

# A WRONG-MODE RUN IS A NO-OP, AND SAYS SO. Reaching here having removed nothing while
# leaving another mode's links standing is not "removed the developer install" -- it is
# the operator having typed the wrong command, and the useful output is the right one.
# ONE CONDITION, THREE MESSAGES. Every line below asserts something about what this run
# DID, so all of them hang off REMOVED_ANY -- the same expression that is set where a
# removal actually happens. The earlier shape gated only the FOREIGN specialisation and
# left the general success line ungated, so three reachable runs announced a completed
# uninstall having removed nothing: a clean machine, a machine with only orphan links,
# and -- the one that had been "verified" -- THE IDEMPOTENT SECOND RUN, which the header
# promises will report what is already gone and which instead reported a removal.
#
# That was the third sentence in this file to be gated on something ADJACENT to its
# effect rather than on the effect: the anchor's absence, then the foreign list, then the
# fall-through. The rule that kills the family: if a line says something happened, the
# condition that prints it and the condition that makes it happen must be the same
# expression. A fourth special case would have been a fourth guess.
echo
if [ -z "$REMOVED_ANY" ]; then
    echo "Nothing belonging to the $MODE_LABEL install was found; nothing was removed."
    if [ -n "$FOREIGN_LINKS" ] || [ -d "$FOREIGN" ]; then
        echo "The $OTHER_LABEL install looks like the live one -- see the note above for"
        echo "the command that removes it."
    fi
elif [ -z "$UNREG_FAILED" ]; then
    echo "Removed the developer install ($MODE_LABEL), and unregistered every bundle path"
    echo "it touched from Launch Services, so no stale Dock or 'Open With' entry survives"
    echo "this run."
else
    echo "Removed the developer install ($MODE_LABEL)."
    echo
    echo "BUT the Launch Services unregistration did NOT run for:"
    printf '%s' "$UNREG_FAILED" | while IFS= read -r p; do
        [ -n "$p" ] && echo "    $p"
    done
    if [ -n "$LSREGISTER_MISSING" ]; then
        echo "  because lsregister was not found at"
        echo "    $LSREGISTER"
        echo "  Locate it under the LaunchServices framework's Support directory on this"
        echo "  version of macOS and run the command below with that path."
    else
        echo "  because the path is still in the Launch Services database after the"
        echo "  unregistration was attempted."
    fi
    echo "  So a stale Dock or 'Open With' entry MAY survive this run. Clear it with the"
    echo "  per-path command below; it works even though the bundle is already gone."
fi
echo
echo "To unregister any Scribobulate.app BY PATH -- a copy installed some other way, or"
echo "one this run could not clear:"
echo "  $LSREGISTER -u '/path/to/Scribobulate.app'"
echo "That works even when the path is already deleted. Do not reach for"
echo "'lsregister -kill': the option was REMOVED (MEASURED on macOS 26 / Darwin 25.0.5,"
echo "which answers \"the -kill option has been removed because it was dangerous and no"
echo "longer useful\" and changes nothing), and this message used to recommend it."
