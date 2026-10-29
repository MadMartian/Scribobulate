# Where this packaging finds things in Homebrew. Sourced, not run, by bundle.sh,
# stage-licenses.sh and verify-licenses.sh, so the loader directory and the
# file-to-formula mapping are written once.

# The gdk-pixbuf module ABI directory, which names the loaders directory.
PIXBUF_VER="2.10.0"
PIXBUF_LOADERS="/opt/homebrew/lib/gdk-pixbuf-2.0/$PIXBUF_VER/loaders"

# The formula a staged binary named $1 came from, via the Cellar path its Homebrew
# original resolves to. Loaders live in their own directory, so both roots are tried.
# Fails when nothing maps.
formula_of() {
    local name="$1" cand real
    for cand in "/opt/homebrew/lib/$name" "$PIXBUF_LOADERS/$name"; do
        [ -e "$cand" ] || continue
        real="$(/usr/bin/readlink -f "$cand" 2>/dev/null)" || continue
        case "$real" in
            */Cellar/*) real="${real#*/Cellar/}"; printf '%s\n' "${real%%/*}"; return 0 ;;
        esac
    done
    return 1
}
