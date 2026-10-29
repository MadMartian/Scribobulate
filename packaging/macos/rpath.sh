# Reading a bundle's Mach-O load commands, and resolving the paths they name. Sourced,
# not run. The ONE place this packaging parses LC_RPATH, expands @loader_path and
# @executable_path, or decides which files in a bundle are Mach-O: bundle.sh uses it to
# walk Homebrew's closure and to delete escaping rpaths, and verify-librsvg.sh and
# verify-selfcontained.sh use it to fail a bundle that still references anything outside
# itself.
#
# WHY AN RPATH OUTSIDE THE BUNDLE IS A HOLE, NOT CLUTTER. dyld resolves an `@rpath/<name>`
# dependency by trying the loading image's OWN LC_RPATH entries first, and only then those
# of the images that loaded it, the executable's `@executable_path/../Frameworks` last.
# Homebrew builds its libraries with rpaths relative to its own tree, and copying a library
# into Contents/Frameworks carries them along unchanged. The SVG pixbuf loader arrived with
# `@loader_path/../../..`, which from Contents/Frameworks is the directory that CONTAINS the
# .app. MEASURED on the bundle (GTK 4.24.1, ad-hoc signed, so no library validation): with a
# librsvg-2.2.dylib placed beside the .app, DYLD_PRINT_LIBRARIES showed the loader binding
# that sibling, not Contents/Frameworks/librsvg-2.2.dylib. The app's XInclude screen stands
# down because the bundle carries a fixed librsvg (verify-librsvg.sh), so a sibling file
# could put an unfixed one behind a disabled screen. Anything that can write beside the .app,
# such as an unpacked download, gets that.
#
# RESOLVED LEXICALLY, never with realpath: the directory an rpath names usually does not
# exist (`@loader_path/../lib` in Frameworks/ names Contents/lib), and the question is
# where dyld WOULD look, not whether something is there today.
#
# EVERY READER FAILS CLOSED, returning 2 when a tool fails or answers nothing, so a caller
# must take the result through `$(...) || <fail>`. An empty answer is NEVER "nothing found":
# a gate that read an otool crash as "no rpaths" passed a bundle it never looked at. Two
# measured ways that happened before this file existed: output read through `< <(...)`
# loses the tool's exit status entirely, and `file "$f" | grep -q Mach-O` under pipefail
# reads a universal binary as NOT Mach-O, because `file` prints one line per architecture
# and is killed by SIGPIPE when grep quits at the first, so the pipeline returns 141.
#
# EVERY ARCHITECTURE IS READ (`otool -arch all`). Without it otool prints the host's slice
# of a universal binary only, so an rpath in its x86_64 slice, the one Rosetta runs, would
# never be seen. The per-slice header lines (`<path> (architecture x86_64):`) are dropped
# and the answers merged.

# otool $1 (a flag) over every slice of $2, header lines dropped.
otool_all() {
    local out
    out="$(otool -arch all "$1" "$2")" || { echo "rpath.sh: otool $1 failed on $2" >&2; return 2; }
    printf '%s\n' "$out" | awk -v f="$2" 'index($0, f) == 1 && /:$/ { next } { print }'
}

# 0 when $1 is a Mach-O (thin or universal), 1 when it is not, 2 when `file` fails.
is_macho() {
    local out
    out="$(file -b "$1")" || { echo "rpath.sh: file failed on $1" >&2; return 2; }
    [ -n "$out" ] || { echo "rpath.sh: file said nothing about $1" >&2; return 2; }
    case "$out" in *Mach-O*) return 0 ;; *) return 1 ;; esac
}

# Every Mach-O under bundle $1, one per line.
bundle_machos() {
    local f rc list
    list="$(find "$1" -type f)" || { echo "rpath.sh: find failed under $1" >&2; return 2; }
    [ -n "$list" ] || { echo "rpath.sh: no files under $1" >&2; return 2; }
    while IFS= read -r f; do
        rc=0; is_macho "$f" || rc=$?
        case "$rc" in 0) printf '%s\n' "$f" ;; 1) ;; *) return 2 ;; esac
    done <<< "$list"
}

# `otool -l` of Mach-O $1, refused unless it lists at least one load command.
load_commands() {
    local out
    out="$(otool_all -l "$1")" || return 2
    case "$out" in
        *"Load command 0"*) printf '%s\n' "$out" ;;
        *) echo "rpath.sh: otool -l listed no load commands for $1" >&2; return 2 ;;
    esac
}

# The LC_RPATH entries of $1, one per line, in load-command order. Empty is a real answer
# here: load_commands has already proved otool read the file.
rpaths_of() {
    local out
    out="$(load_commands "$1")" || return 2
    printf '%s\n' "$out" | awk '/cmd LC_RPATH/{f=1} f&&/ path /{if (!seen[$2]++) print $2; f=0}'
}

# The dependencies Mach-O $1's load commands name (`otool -L`, without the header line).
# Every Mach-O links at least libSystem, so an empty list is a misread, not an answer.
load_deps() {
    local out
    out="$(otool_all -L "$1")" || return 2
    out="$(printf '%s\n' "$out" | awk 'NF && !seen[$1]++ {print $1}')"
    [ -n "$out" ] || { echo "rpath.sh: otool -L listed no dependencies for $1" >&2; return 2; }
    printf '%s\n' "$out"
}

# The install ID of Mach-O $1, or nothing for an executable, which has none.
install_id() {
    local out
    out="$(otool_all -D "$1")" || return 2
    printf '%s\n' "$out" | tr -d '\t' | awk 'NF && !seen[$0]++'
}

# $1 as an absolute path with `.` and `..` folded, without touching the filesystem.
normalize_path() {
    local p="$1" part out=""
    case "$p" in /*) ;; *) p="$PWD/$p" ;; esac
    local IFS=/
    for part in $p; do
        case "$part" in
            ''|.) ;;
            ..)   out="${out%/*}" ;;
            *)    out="$out/$part" ;;
        esac
    done
    printf '%s\n' "${out:-/}"
}

# Where path $3, as written in a load command of an object in directory $1, points, when
# the executable is in directory $2 (empty when it is not known). Prints nothing for what
# dyld would resolve against something no bundle controls: a relative path is taken from
# the process's working directory, and `@rpath/…` names no single place.
expand_path() {
    local objdir="$1" exedir="$2" p="$3"
    case "$p" in
        @loader_path|@loader_path/*)
            normalize_path "$objdir/${p#@loader_path}" ;;
        @executable_path|@executable_path/*)
            if [ -n "$exedir" ]; then normalize_path "$exedir/${p#@executable_path}"; fi ;;
        /*) normalize_path "$p" ;;
        *)  ;;
    esac
}

# Where path $3 of object $2 inside bundle $1 points; `@executable_path` is the bundle's
# own executable directory.
bundle_location() {
    expand_path "$(dirname "$2")" "$1/Contents/MacOS" "$3"
}

# 0 when location $2 is inside bundle $1 (the .app directory itself included).
inside_bundle() {
    local root
    root="$(normalize_path "$1")"
    [ -n "$2" ] && case "$2/" in "$root"/*) return 0 ;; *) return 1 ;; esac
}

# 0 when rpath $3 of object $2 points inside bundle $1.
rpath_is_internal() {
    inside_bundle "$1" "$(bundle_location "$1" "$2" "$3")"
}

# Every rpath in bundle $1 that points outside it, as `<object><TAB><rpath>` lines.
escaping_rpaths() {
    local app="$1" objs obj rps rp
    objs="$(bundle_machos "$app")" || return 2
    while IFS= read -r obj; do
        [ -n "$obj" ] || continue
        rps="$(rpaths_of "$obj")" || return 2
        while IFS= read -r rp; do
            [ -n "$rp" ] || continue
            rpath_is_internal "$app" "$obj" "$rp" || printf '%s\t%s\n' "$obj" "$rp"
        done <<< "$rps"
    done <<< "$objs"
}

# POSITIVE CONTROL for every reader above: bundle.sh gives the executable exactly one rpath
# it can be checked for, `@executable_path/../Frameworks`. A parse that silently reads
# nothing, for any reason the fail-closed checks did not anticipate, cannot find it, so a
# gate that runs this first cannot pass a bundle it never actually read.
exe_rpath_control() {
    local rps
    rps="$(rpaths_of "$1/Contents/MacOS/scribobulate")" || return 2
    grep -qx '@executable_path/../Frameworks' <<< "$rps" || {
        echo "rpath.sh: the executable does not report @executable_path/../Frameworks, so this read of the bundle cannot be trusted" >&2
        return 2
    }
}
