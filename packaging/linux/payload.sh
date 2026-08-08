# Shared payload definition for the Linux packages. SOURCED, never executed.
#
# WHY THIS FILE EXISTS. build-deb.sh and build-rpm.sh install the same five things into
# the same five places; only the metadata around them differs. Written twice, the two
# layouts drift — and a drifted layout is invisible, because each package installs
# cleanly on its own and nothing compares them. That is the same defect the build-step
# contract exists to prevent (ScrAP-207), one level down, so
# the same answer applies: ONE definition, both consumers read it.
#
# What is NOT shared, deliberately: the deb's copyright/changelog/control and the rpm's
# spec. Those are genuinely per-format — Debian requires a referenced common-licence
# copyright and a changelog, rpm carries its licence and summary in the spec header.
# Forcing them into a common shape would be pretending two different formats are one.

PKG="scribobulate"
APP_ID="com.extollit.scribobulate"

# The [package] version. `exit` after the first hit is load-bearing: Cargo.toml carries
# several [[test]] tables further down, and a bare grep eventually matches the wrong one.
read_version() {
    awk '/^\[package\]/{p=1;next} /^\[/{p=0} p && /^version[[:space:]]*=/{gsub(/[",]/,"");print $3;exit}' Cargo.toml
}

# The libc floor, DERIVED from the binary's own versioned symbols rather than guessed.
# The highest GLIBC_x.y it references is the oldest libc that can load it. Hard-coding a
# number would be a second copy of a fact the binary already states, and it would be
# wrong the first time the toolchain moves.
glibc_floor() {
    objdump -T "$1" 2>/dev/null \
        | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' \
        | sed 's/GLIBC_//' | sort -V | tail -1
}

# Refuse to package a binary older than the sources it claims to be. A stale artefact
# that installs cleanly is worse than a missing one: it is indistinguishable from a
# fresh build at every later step.
require_fresh_binary() {
    local bin="$1"
    if [ ! -x "$bin" ]; then
        echo "package: $bin not found — run 'cargo build --release' or pass --build" >&2
        return 1
    fi
    local newer
    newer="$(find src Cargo.toml -newer "$bin" -print -quit 2>/dev/null || true)"
    if [ -n "$newer" ]; then
        echo "package: $bin is older than $newer — rebuild before packaging" >&2
        return 1
    fi
}

# Install the payload into $1 as a filesystem root. Both packages get exactly this.
stage_payload() {
    local root="$1" bin="$2" version="$3"

    install -Dm755 "$bin" "$root/usr/bin/$PKG"

    # Ships VERBATIM. install.sh rewrites Exec/TryExec to an absolute path because
    # ~/.local/bin may not be on the launcher's PATH; /usr/bin always is, so the
    # unmodified entry is correct here and stays byte-identical to the one in data/.
    install -Dm644 "data/$PKG.desktop" "$root/usr/share/applications/$PKG.desktop"

    # The same SVG the binary compiles into its GResource, so the icon the window
    # resolves internally and the one the shell reads off disk cannot diverge. The
    # shell is a separate process and cannot see our GResource, which is why this
    # copy exists at all.
    install -Dm644 "data/icons/scalable/apps/$APP_ID.svg" \
        "$root/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"

    # Preview reading themes. Found via glib::system_data_dirs() — never hard-code
    # /usr/share on the READING side; on a KDE box its first entry is
    # /usr/share/plasma. The same file is compiled into the binary as a fallback, so
    # this copy is an override rather than a requirement.
    install -Dm644 "data/themes.toml" "$root/usr/share/$PKG/themes.toml"

    # A binary on $PATH with no `man` entry is an incomplete install on a system where
    # `man` is how you ask. Generated rather than carried as a source file: everything
    # in it is already stated in Cargo.toml or the desktop entry, and a second
    # hand-maintained copy would drift. `gzip -9n` because the timestamp gzip would
    # otherwise embed makes the artefact non-reproducible.
    mkdir -p "$root/usr/share/man/man1"
    cat > "$root/usr/share/man/man1/$PKG.1" <<EOF
.TH SCRIBOBULATE 1 "$(date +%Y-%m-%d)" "$PKG $version" "User Commands"
.SH NAME
$PKG \- native Markdown viewer and editor that renders on the CPU
.SH SYNOPSIS
.B $PKG
.RI [ FILE .\|.\|.]
.SH DESCRIPTION
Renders Markdown into native GTK4 widgets and forces the GSK Cairo software
renderer, so the process holds no GL context and no video memory. Opening a
second document reuses the running instance rather than starting a new process.
.PP
Supports live reload of externally edited files, split editing, a document
outline, preview reading themes, find and replace, and CriticMarkup annotation
review.
.SH ENVIRONMENT
.TP
.B RUST_LOG
Log filter, e.g. \fBinfo\fR or \fB$PKG=debug\fR. Application and GTK
diagnostics share one sink.
.SH FILES
.TP
.I ~/.config/$PKG/themes.toml
User overrides for preview reading themes, merged over the installed
.I /usr/share/$PKG/themes.toml
per theme id.
.SH LICENSE
Apache License 2.0.
EOF
    gzip -9n "$root/usr/share/man/man1/$PKG.1"
    chmod 644 "$root/usr/share/man/man1/$PKG.1.gz"

    # Directory permissions set EXPLICITLY, not inherited. `mkdir -p`/`install -D`
    # apply the building user's umask, and on a 002 umask that yields group-writable
    # /usr, /usr/bin and /usr/share which the package manager then applies verbatim to
    # the installed system. It builds, installs and runs; only lintian says otherwise.
    find "$root" -type d -exec chmod 755 {} +
}
