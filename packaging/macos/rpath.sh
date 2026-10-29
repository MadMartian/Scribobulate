# Where a bundled Mach-O's LC_RPATH entries point, and which of them leave the bundle.
# Sourced, not run: bundle.sh deletes the escaping entries, and verify-librsvg.sh and
# verify-selfcontained.sh fail a bundle that still has one.
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

# The LC_RPATH entries of $1, one per line, in load-command order.
rpaths_of() {
    otool -l "$1" | awk '/cmd LC_RPATH/{f=1} f&&/ path /{print $2; f=0}'
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

# Where rpath $3 of object $2 inside bundle $1 points, or nothing when dyld would resolve it
# against something outside any bundle (a relative path is taken from the process's working
# directory). `@executable_path` is the bundle's own executable directory.
rpath_location() {
    local app="$1" obj="$2" rp="$3"
    case "$rp" in
        @loader_path/*|@loader_path)
            normalize_path "$(dirname "$obj")/${rp#@loader_path}" ;;
        @executable_path/*|@executable_path)
            normalize_path "$app/Contents/MacOS/${rp#@executable_path}" ;;
        /*) normalize_path "$rp" ;;
        *)  ;;
    esac
}

# 0 when rpath $3 of object $2 points inside bundle $1.
rpath_is_internal() {
    local root loc
    root="$(normalize_path "$1")"
    loc="$(rpath_location "$1" "$2" "$3")"
    [ -n "$loc" ] && case "$loc/" in "$root"/*) return 0 ;; *) return 1 ;; esac
}

# Every rpath in bundle $1 that points outside it, as `<object><TAB><rpath>` lines.
escaping_rpaths() {
    local app="$1" obj rp
    while IFS= read -r obj; do
        file "$obj" | grep -q 'Mach-O' || continue
        while IFS= read -r rp; do
            [ -n "$rp" ] || continue
            rpath_is_internal "$app" "$obj" "$rp" || printf '%s\t%s\n' "$obj" "$rp"
        done < <(rpaths_of "$obj")
    done < <(find "$app" -type f)
}
