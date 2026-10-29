#!/usr/bin/env bash
#
# The X11 half of window-place restore (TDD 15.10a/c/d), checked against a REAL KWin:
# a nested `kwin_x11` on a private Xvfb, with its own activity manager, config and
# session bus. The pipeline's Xvfb has no window manager, and the whole mechanism is
# about what a window manager reads when it first manages a window, so this is the
# only automated check that can see it. Linux only, opt-in, not a pipeline step.
#
#     scripts/winplace-rig.sh
#
# Shows `winplace/examples/probe` in a series of places and, for each, reads the
# window's X properties back with `xprop`/`xwininfo` — independently of the crate —
# plus the place the crate captures off the same window.
#
# ── ISOLATION (each line here is load-bearing) ───────────────────────────────────
# * A private XDG_RUNTIME_DIR, so the a11y bus this session's GTK might start never
#   touches the operator's /run/user/<uid>/at-spi socket.
# * A private XDG_CONFIG_HOME, so the nested KWin never reads or writes the operator's
#   kwinrc, and a private XDG_DATA_HOME / XDG_CACHE_HOME for the activity database.
# * `dbus-run-session`, so kactivitymanagerd and KWin register on a throwaway bus.
# * kwin_x11 is started with DISPLAY set to the private Xvfb and WITHOUT --replace.
# * Teardown kills only PIDs this script started, never by name.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
KAMD=/usr/lib/x86_64-linux-gnu/libexec/kactivitymanagerd

if [[ "${1:-}" != "--inner" ]]; then
    for tool in Xvfb kwin_x11 xprop xwininfo dbus-run-session qdbus; do
        command -v "$tool" >/dev/null || { echo "winplace-rig: missing $tool" >&2; exit 2; }
    done
    [[ -x "$KAMD" ]] || { echo "winplace-rig: missing $KAMD" >&2; exit 2; }
    (cd "$ROOT" && cargo build -q -p winplace --example probe && cargo build -q --bin scribobulate)
    SCRATCH=$(mktemp -d /tmp/winplace-rig.XXXXXX)
    trap 'rm -rf "$SCRATCH"' EXIT
    mkdir -p "$SCRATCH"/{run,config,data,cache,state}
    chmod 700 "$SCRATCH/run"
    # Four desktops in one row, so desktop indices 0-3 exist.
    printf '[Desktops]\nNumber=4\nRows=1\n' >"$SCRATCH/config/kwinrc"
    env -i HOME="$HOME" PATH="$PATH" USER="${USER:-}" \
        XDG_RUNTIME_DIR="$SCRATCH/run" XDG_CONFIG_HOME="$SCRATCH/config" \
        XDG_DATA_HOME="$SCRATCH/data" XDG_CACHE_HOME="$SCRATCH/cache" \
        GTK_A11Y=none NO_AT_BRIDGE=1 QT_ACCESSIBILITY=0 GSK_RENDERER=cairo \
        GTK_USE_PORTAL=0 QT_QPA_PLATFORM=xcb \
        XDG_STATE_HOME="$SCRATCH/state" \
        PROBE="$ROOT/target/debug/examples/probe" APP="$ROOT/target/debug/scribobulate" \
        SCRATCH="$SCRATCH" \
        dbus-run-session -- "$0" --inner
    exit $?
fi

# ── inside the private session ─────────────────────────────────────────────────────
PIDS=()
cleanup() { for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap cleanup EXIT

N=90
while [[ -e /tmp/.X$N-lock || -e /tmp/.X11-unix/X$N ]]; do N=$((N + 1)); done
Xvfb ":$N" -screen 0 1920x1080x24 -nolisten tcp >/dev/null 2>&1 &
PIDS+=($!)
export DISPLAY=":$N"
# The bus was started before the display existed; bus-activated helpers (kglobalaccel)
# need it too, or they abort in a loop.
dbus-update-activation-environment DISPLAY
for _ in $(seq 50); do xwininfo -root >/dev/null 2>&1 && break; sleep 0.1; done

"$KAMD" >"$SCRATCH/kamd.log" 2>&1 &
PIDS+=($!)
kwin_x11 >"$SCRATCH/kwin.log" 2>&1 &
KWIN_PID=$!
PIDS+=("$KWIN_PID")
for _ in $(seq 100); do
    xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q 'window id' && break
    sleep 0.1
done
xprop -root _NET_SUPPORTING_WM_CHECK | grep -q 'window id' || { echo "FAIL: kwin_x11 did not start"; cat "$SCRATCH/kwin.log"; exit 1; }
sleep 1

# A second activity, so "on these activities" is distinguishable from "on all".
for _ in $(seq 50); do
    qdbus org.kde.ActivityManager /ActivityManager/Activities ListActivities >/dev/null 2>&1 && break
    sleep 0.1
done
ACT1=$(qdbus org.kde.ActivityManager /ActivityManager/Activities CurrentActivity)
ACT2=$(qdbus org.kde.ActivityManager /ActivityManager/Activities AddActivity second)
MONITOR=$(xrandr --listmonitors 2>/dev/null | awk 'NR==2 {print $NF}')
echo "rig: display $DISPLAY, desktops $(xprop -root _NET_NUMBER_OF_DESKTOPS | awk '{print $NF}'), activities $ACT1 $ACT2, monitor ${MONITOR:-?}"

FAILS=0
# managed_windows <pid>: the toplevels of <pid> the window manager has taken on (they
# carry WM_STATE). Not `--onlyvisible`: a shaded window's client is unmapped.
managed_windows() {
    local w
    for w in $(xdotool search --pid "$1" 2>/dev/null || true); do
        if xprop -id "$w" WM_STATE 2>/dev/null | grep -q 'window state'; then echo "$w"; fi
    done
}
check() { # <label> <haystack> <needle>
    if grep -qF -- "$3" <<<"$2"; then echo "  ok   $1"; else echo "  FAIL $1: wanted '$3' in: $2"; FAILS=$((FAILS + 1)); fi
}
check_not() {
    if grep -qF -- "$3" <<<"$2"; then echo "  FAIL $1: did not want '$3' in: $2"; FAILS=$((FAILS + 1)); else echo "  ok   $1"; fi
}

# run_case <name> <place toml> [<hook>]: show the probe, inspect its window, print the
# capture. <hook>, if given, is run with the window id while the probe still holds the
# window, after its capture.
run_case() {
    local name=$1 place=$2 hook=${3:-}
    echo "case: $name"
    printf '%s\n' "$place" >"$SCRATCH/place.toml"
    "$PROBE" "$SCRATCH/place.toml" --hold 3 >"$SCRATCH/capture.toml" 2>"$SCRATCH/probe.err" &
    local pid=$!
    local wid=""
    for _ in $(seq 60); do
        wid=$(managed_windows "$pid" | head -1)
        [[ -n "$wid" ]] && break
        sleep 0.05
    done
    if [[ -z "$wid" ]]; then
        wait "$pid" || true
        PROPS="(no window)" GEOM="(no window)" CAPTURE=$(cat "$SCRATCH/capture.toml")
        echo "  FAIL no window appeared"; sed 's/^/  stderr: /' "$SCRATCH/probe.err" | head -5
        FAILS=$((FAILS + 1))
        return
    fi
    sleep 1
    PROPS=$(xprop -id "$wid" _NET_WM_DESKTOP _NET_WM_STATE _KDE_NET_WM_ACTIVITIES _NET_FRAME_EXTENTS 2>&1 || true)
    GEOM=$(xwininfo -id "$wid" 2>&1 | grep -E 'Absolute upper-left|Width|Height' | tr -s ' ' | tr '\n' ' ')
    if [[ -n "$hook" ]]; then "$hook" "$wid"; fi
    wait "$pid" || true
    CAPTURE=$(cat "$SCRATCH/capture.toml")
    [[ -s "$SCRATCH/probe.err" ]] && sed 's/^/  stderr: /' "$SCRATCH/probe.err" | head -5
    echo "  props: $(tr '\n' ';' <<<"$PROPS")"
    echo "  geom:  $GEOM"
    echo "  capture: $(tr '\n' ' ' <<<"$CAPTURE")"
}

run_case "desktop 2, kept above, second activity, position" "$(cat <<EOF
width = 640
height = 400
monitor = "$MONITOR"
x = 300
y = 200
desktop = 2
activities = ["$ACT2"]
above = true
EOF
)"
check "desktop written" "$PROPS" "_NET_WM_DESKTOP(CARDINAL) = 2"
check "above written" "$PROPS" "_NET_WM_STATE_ABOVE"
check "activity written" "$PROPS" "\"$ACT2\""
check "capture desktop" "$CAPTURE" "desktop = 2"
check "capture above" "$CAPTURE" "above = true"
check "capture activity" "$CAPTURE" "$ACT2"
check "position applied" "$GEOM" "Absolute upper-left X: 300  Absolute upper-left Y: 236"
check "capture position x" "$CAPTURE" "x = 300"
check "capture position y" "$CAPTURE" "y = 200"
check "capture size" "$CAPTURE" "width = 640"

# Shaded is requested only once the window is managed: given as an initial state, GTK
# remembered a 93x36 default size and the window came back tiny once unshaded (x11.rs's
# header). So its size is checked on screen after unshading.
UNSHADED=""
unshade() { # <wid>
    sleep 1
    wmctrl -i -r "$1" -b remove,shaded
    sleep 0.5
    UNSHADED=$(xwininfo -id "$1" 2>&1 | grep -E 'Width|Height' | tr -s ' ' | tr '\n' ' ')
}
run_case "all desktops, kept below, shaded, all activities" "$(cat <<EOF
width = 500
height = 300
all_desktops = true
below = true
shaded = true
activities = ["00000000-0000-0000-0000-000000000000"]
EOF
)" unshade
echo "  unshaded: $UNSHADED"
check "all desktops written" "$PROPS" "_NET_WM_DESKTOP(CARDINAL) = 4294967295"
check "below written" "$PROPS" "_NET_WM_STATE_BELOW"
check "shaded written" "$PROPS" "_NET_WM_STATE_SHADED"
check "capture all desktops" "$CAPTURE" "all_desktops = true"
check "capture shaded" "$CAPTURE" "shaded = true"
check "capture below" "$CAPTURE" "below = true"
# On screen, not in the capture: with shaded given as an initial state the capture
# still read 500x300 while the unshaded window was 93x36 (MEASURED).
check "unshaded at its size" "$UNSHADED" "Width: 500  Height: 300"

run_case "no titlebar and frame, at a position" "$(cat <<EOF
width = 700
height = 500
monitor = "$MONITOR"
x = 50
y = 60
no_border = true
EOF
)"
# KWin gives a frameless window no _NET_FRAME_EXTENTS at all (MEASURED), and its
# client area then starts exactly at the requested frame position.
check "no frame" "$PROPS" "_NET_FRAME_EXTENTS:  not found"
check "frameless position" "$GEOM" "Absolute upper-left X: 50  Absolute upper-left Y: 60"
check "capture no_border" "$CAPTURE" "no_border = true"

run_case "an activity that no longer exists, a desktop out of range" "$(cat <<EOF
width = 600
height = 400
desktop = 9
activities = ["11111111-2222-3333-4444-555555555555"]
EOF
)"
check_not "unknown desktop not applied" "$PROPS" "_NET_WM_DESKTOP(CARDINAL) = 9"
check "lands on the current activity" "$PROPS" "\"$ACT1\""

run_case "maximized on the monitor" "$(cat <<EOF
width = 600
height = 400
maximized = true
monitor = "$MONITOR"
x = 0
y = 0
EOF
)"
check "maximized" "$PROPS" "_NET_WM_STATE_MAXIMIZED_VERT"
check "capture maximized keeps the default size" "$CAPTURE" "width = 600"

run_case "full screen" "$(cat <<EOF
width = 600
height = 400
fullscreen = true
monitor = "$MONITOR"
EOF
)"
check "full screen" "$PROPS" "_NET_WM_STATE_FULLSCREEN"

run_case "nothing but a size (a new window's place)" "$(cat <<EOF
width = 600
height = 400
EOF
)"
check "current activity" "$PROPS" "\"$ACT1\""
check_not "no forced state" "$PROPS" "_NET_WM_STATE_ABOVE"

# ── Leaving maximized brings back the size it had before ──────────────────────────
echo "case: unmaximize returns to the saved size"
printf 'width = 600\nheight = 400\nmaximized = true\nmonitor = "%s"\nx = 100\ny = 100\n' "$MONITOR" >"$SCRATCH/place.toml"
"$PROBE" "$SCRATCH/place.toml" --hold 3 >/dev/null 2>&1 &
pid=$!
wid=""
for _ in $(seq 60); do
    wid=$(managed_windows "$pid" | head -1)
    [[ -n "$wid" ]] && break
    sleep 0.05
done
if [[ -n "$wid" ]]; then
    sleep 0.8
    wmctrl -i -r "$wid" -b remove,maximized_vert,maximized_horz
    sleep 0.5
    GEOM=$(xwininfo -id "$wid" 2>&1 | grep -E 'Width|Height' | tr -s ' ' | tr '\n' ' ')
else
    GEOM="(no window)"
fi
wait "$pid" || true
echo "  geom:  $GEOM"
check "size it returns to" "$GEOM" "Width: 600  Height: 400"

# ── The application itself: restore a session, move a window, quit, read it back ──
# TDD 15.10a end to end, through the real session file and the real quit path.
echo "case: the application restores two windows and saves where they were at quit"
STATE="$XDG_STATE_HOME/scribobulate"
mkdir -p "$STATE"
chmod 700 "$STATE"
{
    printf '[[windows]]\nzoom_level = 1.0\nactive_tab = 0\n[windows.place]\n'
    printf 'width = 1000\nheight = 640\nmonitor = "%s"\nx = 100\ny = 80\ndesktop = 1\n' "$MONITOR"
    printf 'activities = ["%s"]\nabove = true\n[[windows.tabs]]\nview_mode = "preview"\n\n' "$ACT2"
    printf '[[windows]]\nzoom_level = 1.0\nactive_tab = 0\n[windows.place]\n'
    printf 'width = 900\nheight = 600\nmonitor = "%s"\nx = 800\ny = 300\ndesktop = 3\n' "$MONITOR"
    printf '[[windows.tabs]]\nview_mode = "preview"\n'
} >"$STATE/session.toml"
"$APP" >"$SCRATCH/app.log" 2>&1 &
APP_PID=$!
PIDS+=("$APP_PID")
WINS=""
for _ in $(seq 200); do
    WINS=$(managed_windows "$APP_PID")
    [[ $(wc -w <<<"$WINS") -ge 2 ]] && break
    sleep 0.05
done
sleep 1.5
W1="" W2=""
for w in $WINS; do
    d=$(xprop -id "$w" _NET_WM_DESKTOP 2>/dev/null | awk '{print $NF}')
    [[ "$d" == 1 ]] && W1=$w
    [[ "$d" == 3 ]] && W2=$w
done
if [[ -z "$W1" || -z "$W2" ]]; then
    echo "  FAIL the two windows did not come back on desktops 1 and 3 (windows: $WINS)"
    for w in $WINS; do echo "    $w $(xprop -id "$w" _NET_WM_DESKTOP WM_NAME 2>&1 | tr '\n' ' ')"; done
    sed 's/^/  app: /' "$SCRATCH/app.log" | tail -5
    FAILS=$((FAILS + 1))
else
    P1=$(xprop -id "$W1" _NET_WM_STATE _KDE_NET_WM_ACTIVITIES | tr '\n' ';')
    G1=$(xwininfo -id "$W1" | grep -E 'Absolute upper-left|Width|Height' | tr -s ' ' | tr '\n' ' ')
    G2=$(xwininfo -id "$W2" | grep -E 'Absolute upper-left|Width|Height' | tr -s ' ' | tr '\n' ' ')
    echo "  window 1: $P1 $G1"
    echo "  window 2: $G2"
    check "window 1 above" "$P1" "_NET_WM_STATE_ABOVE"
    check "window 1 activity" "$P1" "\"$ACT2\""
    check "window 1 position and size" "$G1" "Absolute upper-left X: 100  Absolute upper-left Y: 116  Width: 1000  Height: 640"
    check "window 2 position and size" "$G2" "Absolute upper-left X: 800  Absolute upper-left Y: 336  Width: 900  Height: 600"
    # The reader moves window 2 to desktop 0 and somewhere else after it opened.
    wmctrl -i -r "$W2" -t 0
    wmctrl -i -r "$W2" -e 0,250,150,-1,-1
    sleep 0.5
    gdbus call --session --dest com.extollit.scribobulate --object-path /com/extollit/scribobulate \
        --method org.gtk.Actions.Activate quit '[]' '{}' >/dev/null
    for _ in $(seq 100); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.05; done
    SAVED=$(tr '\n' ' ' <"$STATE/session.toml")
    echo "  saved: $SAVED"
    check "window 1 saved on desktop 1 at 100,80" "$SAVED" "width = 1000 height = 640 maximized = false fullscreen = false monitor = \"$MONITOR\" x = 100 y = 80 desktop = 1"
    check "window 1 saved activity and above" "$SAVED" "activities = [\"$ACT2\"] shaded = false above = true"
    check "window 2 saved where it was at quit" "$SAVED" "width = 900 height = 600 maximized = false fullscreen = false monitor = \"$MONITOR\" x = 250 y = 150 desktop = 0"
fi

# ── A window manager that died: last, since it ends KWin ──────────────────────────
# Killed outright, KWin leaves _NET_SUPPORTING_WM_CHECK on the root naming a window that
# is gone. Trusting the root alone, present waits out its whole bound for a manager that
# will never manage the window, and warns that it did not.
echo "case: a window manager that died is not waited for"
kill -9 "$KWIN_PID" 2>/dev/null || true
wait "$KWIN_PID" 2>/dev/null || true
STALE=$(xprop -root _NET_SUPPORTING_WM_CHECK 2>&1)
echo "  root: $STALE"
printf 'width = 600\nheight = 400\ndesktop = 1\n' >"$SCRATCH/place.toml"
"$PROBE" "$SCRATCH/place.toml" >/dev/null 2>"$SCRATCH/probe.err" || true
check "the root still names the dead manager's window" "$STALE" "window id"
check_not "present did not wait for it" "$(cat "$SCRATCH/probe.err")" "did not manage the window"

echo
if [[ $FAILS -eq 0 ]]; then echo "winplace-rig: PASS"; else echo "winplace-rig: $FAILS FAILED"; exit 1; fi
