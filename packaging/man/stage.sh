# Staging the manual pages. SOURCED, never executed.
#
# WHY THIS FILE EXISTS, and why it is not inside packaging/linux/payload.sh where it
# started. The pages in this directory are platform-neutral -- one option set, one config
# format, one set of theme keys -- so every platform that installs them performs the same
# two substitutions and the same compression. Written once per platform, those drift, and
# the drift is invisible: each artefact installs cleanly on its own and nothing compares
# them. That is the same argument payload.sh's own header makes for sharing the Linux
# payload, one level up, so it gets the same answer rather than a second opinion.
#
# EVERYTHING IS AN ARGUMENT, no caller globals. payload.sh has $PKG and $REPO_DIR in
# scope and a macOS caller does not; a shared file that silently depends on one caller's
# environment is shared in name only.
#
# PORTABILITY IS THE POINT OF THE DATE HELPER, not an afterthought. `date -d` is GNU and
# `date -r <epoch>` is BSD, `stat -c` is GNU and `stat -f` is BSD -- and the GNU spellings
# are the ones a Linux author writes without noticing. Each is tried in turn, so this runs
# on a macOS host without a `uname` branch: a platform test would state which spelling each
# platform has, and be wrong the first time someone installs GNU coreutils on a Mac.

# The date a manual page carries in its `.TH` line: man_date <file> <repo-dir>
#
# NOT `date +%F`, which is what this replaced and what made every build produce a
# different page -- directly against the reproducibility `gzip -9n` below is chosen for.
# In descending order of authority: SOURCE_DATE_EPOCH (the reproducible-builds standard,
# set by every distribution builder that cares about this), then the date of the commit
# that last touched the page (meaningful in its own right -- it is when the documentation
# was last revised), then the file's own mtime, for a build from an exported tarball with
# no git to ask.
man_date() {
    local file="$1" repo="$2"
    if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
        # SHAPE-CHECKED BEFORE USE, and this is not belt-and-braces: BSD `date -r` accepts
        # "filename|seconds", so a value that happens to name a readable file converts to
        # THAT FILE'S mtime and passes the output check below -- a wrong answer rather than
        # a failure, in the one code path whose entire purpose is to honour a date the
        # caller named exactly. Raised by the macOS seat against real BSD userland.
        case "$SOURCE_DATE_EPOCH" in
            "" | *[!0-9]*)
                echo "man: SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH is not a whole number of seconds" >&2
                return 1
                ;;
        esac
        # A HARD FAILURE, not a fall-through to the next source. A builder that sets this
        # has asked for a reproducible artefact by name, and quietly substituting a
        # different date would answer a question nobody asked -- MEASURED during a
        # BSD-spelling simulation, where an unconvertible value slid silently down to the
        # git date and the page looked perfectly correct.
        _epoch_to_date "$SOURCE_DATE_EPOCH" && return 0
        echo "man: SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH is not a date this host can convert" >&2
        return 1
    fi
    local committed
    committed="$(git -C "$repo" log -1 --format=%cs -- "$file" 2>/dev/null || true)"
    if [ -n "$committed" ]; then
        printf '%s\n' "$committed"
        return 0
    fi
    local mtime
    mtime="$(_file_mtime "$file")" && _epoch_to_date "$mtime" && return 0
    echo "man: cannot determine a date for $file (no SOURCE_DATE_EPOCH, no git, no mtime)" >&2
    return 1
}

# An epoch as YYYY-MM-DD, UTC so that one commit cannot produce two dates on two hosts
# either side of midnight.
#
# GNU spelling first, then BSD, and THE OUTPUT IS VALIDATED rather than the exit status
# trusted -- because the two spellings collide. GNU `date -r` takes a reference FILE where
# BSD's takes an epoch, and GNU `stat -f` reports FILESYSTEM status where BSD's is a
# format string: on a GNU host `stat -f %m <file>` exits 0 and prints a block of filesystem
# statistics, which an exit-status test would accept as a timestamp. Each helper therefore
# asserts the SHAPE of what came back, so a wrong-dialect success is a failure here.
_epoch_to_date() {
    local out
    out="$(date -u -d "@$1" +%Y-%m-%d 2>/dev/null)" || out=""
    case "$out" in [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) printf '%s\n' "$out"; return 0;; esac
    out="$(date -u -r "$1" +%Y-%m-%d 2>/dev/null)" || out=""
    case "$out" in [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) printf '%s\n' "$out"; return 0;; esac
    return 1
}

_file_mtime() {
    local out
    out="$(stat -c %Y "$1" 2>/dev/null)" || out=""
    case "$out" in *[!0-9]* | "") ;; *) printf '%s\n' "$out"; return 0;; esac
    out="$(stat -f %m "$1" 2>/dev/null)" || out=""
    case "$out" in *[!0-9]* | "") ;; *) printf '%s\n' "$out"; return 0;; esac
    return 1
}

# Stage one manual page: install_man_page <src> <dst> <version> <repo-dir>
#
# `dst` is the UNCOMPRESSED destination path; this function creates its directory,
# substitutes, compresses in place and leaves `<dst>.gz` mode 644.
#
# @VERSION@ and @DATE@ are the ONLY substitutions, deliberately -- a template with logic
# in it is a generator again, and the point of carrying these pages as source is that what
# ships is what a reviewer read (and what `cargo xtask lint-references` checks 16 and 17
# were able to gate). `gzip -9n` because the timestamp gzip would otherwise embed makes
# the artefact non-reproducible.
install_man_page() {
    local src="$1" dst="$2" version="$3" repo="$4"
    [ -f "$src" ] || { echo "man: $src is missing" >&2; return 1; }
    local when
    when="$(man_date "$src" "$repo")" || return 1
    mkdir -p "$(dirname "$dst")"
    sed -e "s|@VERSION@|$version|g" -e "s|@DATE@|$when|g" "$src" > "$dst"
    # A placeholder that survives means the substitution silently missed one, and a page
    # whose footer reads "scribobulate @VERSION@" ships looking deliberate.
    if grep -q '@[A-Z_]*@' "$dst"; then
        echo "man: $dst still carries an unsubstituted placeholder" >&2
        return 1
    fi
    rm -f "$dst.gz"
    gzip -9n "$dst"
    chmod 644 "$dst.gz"
}
