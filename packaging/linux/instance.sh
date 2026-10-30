# Sourced, not run: the desktop launcher for a named instance (TDD 8.17), shared by
# install.sh and uninstall.sh. The caller sets APP_ID and APP_DIR.
#
# A named instance (`scribobulate --instance NAME`) is a separate reusable copy of the
# app with its own application ID, `$APP_ID.<name>`. Its launcher is a desktop entry
# whose FILE NAME is that ID, because that is how the desktop matches a running window
# to its launcher: Wayland's app_id and X11's _GTK_APPLICATION_ID both carry the ID, and
# KWin and the Plasma task manager resolve it to `<ID>.desktop` before they consult
# StartupWMClass. StartupWMClass is still set, to the instance's program name
# (`scribobulate-<name>`), so a desktop that only matches by window class agrees.
#
# The launcher registers no MIME types: opening a Markdown file from the file manager
# keeps using the default instance.
#
# THE NAME RULES ARE NOT WRITTEN HERE. The installer asks the installed binary
# (`--instance NAME --version`), so the shell cannot accept a name the app refuses.

# The lower-cased name: the ID element and the launcher's file name. Called only on a
# name the binary has accepted, which is ASCII letters and digits.
instance_key() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

instance_desktop_path() {
    printf '%s/%s.%s.desktop' "$APP_DIR" "$APP_ID" "$(instance_key "$1")"
}

# A directory written into Exec must be absolute and free of the characters the Desktop
# Entry Specification makes special inside Exec (quote, backquote, dollar, backslash,
# percent) or a newline. Refused rather than escaped: two levels of escaping apply there,
# and a launcher that silently points an instance at the wrong folder is worse than an
# installer that asks for a plainer path.
instance_check_dir() {
    local what="$1" dir="$2"
    case "$dir" in
        /*) ;;
        *) echo "install: $what must be an absolute path: $dir" >&2; return 1 ;;
    esac
    case "$dir" in
        *'"'* | *'`'* | *'$'* | *'\'* | *'%'* | *$'\n'*)
            echo "install: $what contains a character a desktop entry cannot carry" \
                 "(\" \` \$ \\ % or a newline): $dir" >&2
            return 1 ;;
    esac
}

# write_instance_launcher NAME BIN STATE_HOME CONFIG_HOME
# STATE_HOME and CONFIG_HOME may be empty: the instance then uses the usual XDG
# directories, keeping its state in <state home>/scribobulate/instances/<name>/.
write_instance_launcher() {
    local name="$1" bin="$2" state_home="$3" config_home="$4"
    local key path exec_line
    key="$(instance_key "$name")"
    path="$(instance_desktop_path "$name")"
    exec_line="\"$bin\" --instance $name %U"
    if [ -n "$state_home" ] || [ -n "$config_home" ]; then
        local env_args="env"
        [ -n "$state_home" ] && env_args="$env_args \"XDG_STATE_HOME=$state_home\""
        [ -n "$config_home" ] && env_args="$env_args \"XDG_CONFIG_HOME=$config_home\""
        exec_line="$env_args $exec_line"
    fi
    mkdir -p "$APP_DIR"
    cat > "$path" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Scribobulate ($name)
GenericName=Markdown Editor
Comment=The "$name" instance of Scribobulate, with its own windows and session
Exec=$exec_line
TryExec=$bin
Icon=$APP_ID
Terminal=false
Categories=Office;Development;Utility;TextEditor;GTK;
Keywords=markdown;md;editor;viewer;
StartupNotify=true
StartupWMClass=$(basename "$bin")-$key
EOF
    printf '%s\n' "$path"
}

# Let the desktop see a launcher that was added or removed. KDE keeps its own index,
# which update-desktop-database does not refresh.
refresh_launcher_index() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APP_DIR" || true
    local sycoca
    for sycoca in kbuildsycoca6 kbuildsycoca5; do
        if command -v "$sycoca" >/dev/null 2>&1; then
            "$sycoca" >/dev/null 2>&1 || true
            break
        fi
    done
}
