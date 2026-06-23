#!/usr/bin/env bash
# gen-splash.sh — regenerate the animated marketing splash (looping WebP + GIF)
# from purpose-built showcase fixtures. Build-time developer tool; NOT part of
# the shipped binary.
#
# Pipeline: capture (headless Xvfb) -> composite (magick) -> tween (ffmpeg
# xfade) -> encode (webp + gif).
#
# Usage:
#   scripts/gen-splash.sh capture     # stage 1: render every per-frame shot
#   scripts/gen-splash.sh composite   # stage 2: composite every keyframe
#   scripts/gen-splash.sh <frameID>   # capture + composite ONE frame (A|B|C|D)
#   scripts/gen-splash.sh encode      # stage 3+4: tween + encode webp/gif
#   scripts/gen-splash.sh all         # the whole pipeline (default)
#
# ═══ PREREQUISITES — Linux + X11 only ═══════════════════════════════════════
# This drives a REAL Scribobulate window on a headless X server, so it needs an
# X stack. It will not run on Windows or macOS (nor an Amiga). It is fine to run
# from a Wayland session — Xvfb is its own X server and this never touches your
# display — but the tools themselves are X11.
#
#   tool     provided by (Debian/Ubuntu)   tested version   used for
#   ------   --------------------------    --------------   ------------------
#   Xvfb     xvfb                          (Ubuntu 22.04)   headless X server
#   xdotool  xdotool                       3.20160805.1     find/size/focus win
#   xwd      x11-apps                      (x11-apps)       window capture
#   magick   ImageMagick **7** (see below) 7.1.1-27 Q16     composite/crop/scale
#   ffmpeg   ffmpeg (>= 4.3)               4.4.2            xfade + webp/gif
#   the app  cargo build --release         —                target/release/scribobulate
#
#   sudo apt install xvfb xdotool x11-apps ffmpeg
#
# ⚠ ImageMagick 7 is REQUIRED and apt alone is NOT enough on Ubuntu 22.04:
#   the `imagemagick` package there is version **6**, which ships `convert` and
#   `import` but NO `magick` binary — the command this script calls throughout.
#   Install IM7 from source or a PPA (newer distros may package 7 directly).
#   Verify:  magick -version   → must report "ImageMagick 7.x"
#
# ffmpeg needs the `xfade` filter (added in 4.3) and the `libwebp_anim` encoder
# (a libwebp-enabled build). Verify:
#   ffmpeg -hide_banner -h filter=xfade    | head -1
#   ffmpeg -hide_banner -encoders | grep libwebp_anim
#
# Tested end-to-end on Ubuntu 22.04.5 LTS with the versions above.
# ════════════════════════════════════════════════════════════════════════════
set -euo pipefail

# ─── paths ──────────────────────────────────────────────────────────────────
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/release/scribobulate"
FIX="$ROOT/data/splash"                 # purpose-built marketing fixtures
OUT="$ROOT/target/splash"               # scratch + artifacts (under git-ignored target/)
SHOTS="$OUT/shots"                      # raw window captures
FRAMES="$OUT/frames"                    # composited keyframes
mkdir -p "$SHOTS" "$FRAMES" "$ROOT/assets"

# ═════════════════════════════════════════════════════════════════════════════
# CONFIG — edit here. Everything visual is a knob; layouts are per-frame below.
# ═════════════════════════════════════════════════════════════════════════════
CANVAS_W=1346                           # final splash canvas (kept from the retired splash.jpeg)
CANVAS_H=1002
GRAD_FROM="#613373"                     # purple, top-left  (sampled from the retired splash.jpeg)
GRAD_TO="#8c7857"                       # gold,  bottom-right
SHADOW_ALPHA=70                         # drop-shadow opacity % (applied to EVERY shot)

WIN_W=1100                              # app window size for a captured shot
WIN_H=740
SETTLE_SECS=2.5                         # let GTK finish first-frame layout + render
SHADOW_SIGMA=22                         # drop-shadow blur radius
SHADOW_OFF=16                           # drop-shadow downward offset (px)

# Spawned PIDs are torn down on ANY exit (a mid-capture failure must never
# orphan an Xvfb — an orphan holds its display and later captures silently
# collide onto its stale window, rendering the wrong theme).
SPAWNED_PIDS=()
cleanup_spawned() { local p; for p in "${SPAWNED_PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup_spawned EXIT

# The reading theme owns the PREVIEW; GTK_THEME only styles the chrome
# (menubar/toolbar/tabs). Adwaita ships with GTK.
export GTK_THEME="${GTK_THEME:-Adwaita:dark}"

# ── Per-frame declarative config ──────────────────────────────────────────────
# SHOT_SPEC entries:  id | fixture.md | theme | view_mode | unsafe | outline
#   theme:     system | sepia | synthwave | terminal
#   view_mode: preview | edit | split
#   outline:   on | off   (win.outline defaults ON; "off" sends a focused F9)
declare -A SHOT_SPEC=(
  [a_syn]="hero.md|synthwave|preview|true|off"
  [a_term]="hero.md|terminal|preview|true|off"
  [b_outline]="outline-showcase.md|system|preview|true|on"
  [c_arch]="architecture.md|terminal|preview|true|off"
  [d_sepia]="editing.md|sepia|split|true|off"
)

# SHOT_CROP: OPTIONAL zoom. A shot listed here is cropped to "WxH+X+Y" (in the
# captured 1100x740 window's own pixels) before scaling — a magnified detail card
# rather than a whole window. Use it where the full-window shot is too small to
# read the very thing the frame is selling. Omit a shot to use the whole window.
declare -A SHOT_CROP=(
  [b_outline]="720x540+0+118"      # the nested outline tree + the headings beside it
  [d_sepia]="720x540+380+118"      # straddles the split seam: editor source | serif page
)

# FRAME_SHOTS: which shot(s) each keyframe draws (space-separated, back-to-front).
declare -A FRAME_SHOTS=(
  [A]="a_term a_syn"      # two overlapping (terminal behind, synthwave in front)
  [B]="b_outline"
  [C]="c_arch"
  [D]="d_sepia"
)
FRAME_ORDER=(A B C D)     # animation cycle order (loops back A after D)

# place_shots FRAME_ID — echo one "<shot> <x> <y> <scalePct>" line per placement.
# Frame A overlaps two shots diagonally; every other frame centers its single
# shot. Editing a frame's layout = editing its case here (that is the whole
# point of keeping placement declarative).
place_shots() {
  case "$1" in
    A)
      local s=86                                    # scale each shot to 86%
      local sw=$((WIN_W*s/100)) sh=$((WIN_H*s/100))
      # back shot: upper-right; front shot: lower-left — mirrors the retired splash.jpeg
      echo "a_term $(( CANVAS_W - sw - 40 )) 40 $s"
      echo "a_syn 40 $(( CANVAS_H - sh - 40 )) $s"
      ;;
    # Single-shot frames are STAGGERED — each takes its own scale and a nudge off
    # dead-centre, so B->C->D drift rather than snapping to the same rectangle.
    # B and D are CROPPED zooms (see SHOT_CROP), so their scale is a % of the
    # 720x540 crop — hence the larger numbers, not a mistake.
    # The zooms fill most of the canvas — only a modest border of gradient shows.
    # Their nudges are small BECAUSE they are large: a ±30 nudge on a card this
    # size crowds one edge out of the frame.
    B) place_centered "$1" 172 -18 -14 ;;
    C) place_centered "$1" 113  10   6 ;;
    D) place_centered "$1" 170  16  18 ;;
    *) place_centered "$1" 92 0 0 ;;
  esac
}

# shot_dims SHOT_ID — the shot's effective source "W H": its SHOT_CROP size when it
# has one, else the captured window size. Every placement/size calc goes through
# this, so adding or changing a crop needs no other edit.
shot_dims() {
  local c="${SHOT_CROP[$1]:-}"
  if [ -n "$c" ]; then echo "${c%%+*}" | tr 'x' ' '; else echo "$WIN_W $WIN_H"; fi
}

# place_centered FRAME_ID SCALE DX DY — centre this frame's single shot on the
# canvas at SCALE%, nudged by (DX,DY). SCALE is relative to the shot's own source
# size, so a cropped shot needs a LARGER percentage to fill the same area.
place_centered() {
  local shot; shot="${FRAME_SHOTS[$1]}"
  local s="$2" dx="$3" dy="$4"
  local w h; read -r w h <<<"$(shot_dims "$shot")"
  local sw=$((w*s/100)) sh=$((h*s/100))
  echo "$shot $(( (CANVAS_W - sw)/2 + dx )) $(( (CANVAS_H - sh)/2 + dy )) $s"
}

# ═════════════════════════════════════════════════════════════════════════════
# STAGE 1 — capture
# ═════════════════════════════════════════════════════════════════════════════
# What provides each tool, so a failure is actionable rather than a bare name.
# (See the PREREQUISITES block at the top of this file.)
declare -A TOOL_PKG=(
  [Xvfb]="apt install xvfb"
  [xdotool]="apt install xdotool"
  [xwd]="apt install x11-apps"
  [ffmpeg]="apt install ffmpeg   (needs >= 4.3 for xfade, and a libwebp build)"
  [magick]="ImageMagick 7 — NOT apt's 'imagemagick', which is v6 and has no 'magick' binary; install IM7 from source or a PPA"
)
require() {
  command -v "$1" >/dev/null 2>&1 && return 0
  echo "gen-splash: required tool '$1' not found." >&2
  echo "            ${TOOL_PKG[$1]:-see the PREREQUISITES block at the top of this script}" >&2
  exit 1
}

# The pipeline drives the real app, so the release binary must exist. Without
# this check a missing binary shows up only as "no window found" — a confusing
# symptom for a trivial cause.
require_binary() {
  [ -x "$BIN" ] && return 0
  echo "gen-splash: no Scribobulate binary at $BIN" >&2
  echo "            build it first:  cargo build --release" >&2
  exit 1
}

write_session() {  # statedir theme path mode unsafe
  local statedir="$1" theme="$2" path="$3" mode="$4" unsafe="$5"
  mkdir -p "$statedir/scribobulate"
  cat >"$statedir/scribobulate/session.toml" <<EOF
show_toolbar = true
show_statusbar = true
preview_theme = "$theme"

[[windows]]
width = $WIN_W
height = $WIN_H
zoom_level = 1.0
active_tab = 0

[[windows.tabs]]
path = "$path"
view_mode = "$mode"
split_swap = false
split_vertical = false
show_unsafe_images = $unsafe
EOF
}

# capture_shot <id> — render the app headless per SHOT_SPEC[id] -> $SHOTS/<id>.png.
# Kills ONLY the PID it launched (never a name match — the operator's own
# instance may be running in parallel).
capture_shot() {
  local id="$1"
  IFS='|' read -r fixture theme mode unsafe outline <<<"${SHOT_SPEC[$id]}"
  local path="$FIX/$fixture"
  [ -f "$path" ] || { echo "WARN: missing fixture $path (skipping $id)" >&2; return 0; }
  local statedir; statedir="$(mktemp -d /tmp/scrib-splash-state.XXXXXX)"
  write_session "$statedir" "$theme" "$path" "$mode" "$unsafe"

  # Let Xvfb PICK a free display (-displayfd) and report it — never hard-code a
  # number (a stale server on that number is the collision trap that renders the
  # wrong theme). It writes the chosen display number to the given fd.
  local dfile; dfile="$(mktemp)"
  Xvfb -displayfd 1 -screen 0 "$((WIN_W+120))x$((WIN_H+120))x24" >"$dfile" 2>/dev/null &
  local xvfb_pid=$!; SPAWNED_PIDS+=("$xvfb_pid")
  local dnum="" n=0
  while [ -z "$dnum" ] && [ $n -lt 60 ]; do dnum="$(tr -dc '0-9' <"$dfile")"; [ -z "$dnum" ] && sleep 0.1; n=$((n+1)); done
  rm -f "$dfile"
  [ -z "$dnum" ] && { echo "WARN: Xvfb never reported a display for $id" >&2; kill "$xvfb_pid" 2>/dev/null||true; rm -rf "$statedir"; return 0; }
  local DISP=":$dnum"

  # -n = new instance: a fresh process (so on_activate -> restore_session) that
  # never forwards to the operator's real running instance.
  XDG_STATE_HOME="$statedir" DISPLAY="$DISP" RUST_LOG=warn \
    "$BIN" -n >/dev/null 2>&1 &
  local app_pid=$!; SPAWNED_PIDS+=("$app_pid")

  # A GTK4 app maps several X windows (1x1 util, IM, the real toplevel); the
  # content window is the largest-area one.
  local wid="" tries=0
  while [ -z "$wid" ] && [ $tries -lt 40 ]; do
    local best="" bestarea=0 w
    for w in $(DISPLAY="$DISP" xdotool search --name "Scribobulate" 2>/dev/null || true); do
      # WIDTH/HEIGHT come from `eval`ing xdotool's --shell output, so they are
      # LEFTOVERS from the previous iteration if this call fails — and a window can
      # disappear between `search` and `getwindowgeometry` routinely, since GTK maps
      # and unmaps helper windows during startup. Unguarded, the dead window is then
      # scored with the live one's area and can win, so the script screenshots the
      # wrong window (or a 1x1 utility window) and reports success. Clear first, and
      # skip the window unless this call actually produced both values.
      local geom; WIDTH=""; HEIGHT=""
      geom="$(DISPLAY="$DISP" xdotool getwindowgeometry --shell "$w" 2>/dev/null)" || continue
      eval "$geom"
      [ -n "$WIDTH" ] && [ -n "$HEIGHT" ] || continue
      local a=$((WIDTH*HEIGHT)); [ "$a" -gt "$bestarea" ] && { bestarea=$a; best=$w; }
    done
    [ -n "$best" ] && [ "$bestarea" -gt 40000 ] && wid="$best"
    tries=$((tries+1)); [ -z "$wid" ] && sleep 0.2
  done

  if [ -n "$wid" ]; then
    # Bare Xvfb has no WM to enforce the requested size, so GTK allocates the
    # content's NATURAL width. Force the size and let GTK re-layout.
    DISPLAY="$DISP" xdotool windowsize "$wid" "$WIN_W" "$WIN_H"; sleep 1
    if [ "$outline" = "off" ]; then
      # Focus first: a focused XTEST key is "real" input GTK honours (a synthetic
      # --window event is not). win.outline defaults ON, so one F9 hides it.
      DISPLAY="$DISP" xdotool windowfocus "$wid"; sleep 0.3
      DISPLAY="$DISP" xdotool key F9
    fi
    sleep "$SETTLE_SECS"
    # xwd reads the window directly, avoiding ImageMagick's flaky XShm under Xvfb.
    DISPLAY="$DISP" xwd -id "$wid" 2>/dev/null | magick xwd:- "$SHOTS/$id.png"
    echo "captured $id.png ($theme/$mode, outline=$outline) on $DISP"
  else
    # FATAL, not a warning: with no window there is no shot, and the composite stage
    # would go on to build a blank frame from it and encode that. Failing here names
    # the real cause (the app never mapped a window under Xvfb) instead of leaving a
    # blank slide to be discovered in the published animation.
    echo "ERROR: no window found for $id ($fixture/$theme) after 40 tries on $DISP" >&2
    return 1
  fi

  kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true
  kill "$xvfb_pid" 2>/dev/null || true; wait "$xvfb_pid" 2>/dev/null || true
  rm -rf "$statedir"
}

# ═════════════════════════════════════════════════════════════════════════════
# STAGE 2 — composite (magick)
# ═════════════════════════════════════════════════════════════════════════════
# gradient_bg <out> — the diagonal purple->gold canvas. `-sparse-color
# barycentric` with two control points is a clean linear gradient along the line
# between them (top-left -> bottom-right).
gradient_bg() {
  magick -size "${CANVAS_W}x${CANVAS_H}" xc: \
    -sparse-color barycentric \
      "0,0 $GRAD_FROM $((CANVAS_W-1)),$((CANVAS_H-1)) $GRAD_TO" \
    "$1"
}

# shadowed <in> <scalePct> <out> — scale a shot and give it a soft drop shadow on
# transparency (BOTH Frame-A shots get one — the defect fixed from the retired splash.jpeg).
# The result is larger than the shot by the blur margin; its caller measures the
# size to keep placement referring to the SHOT's own top-left.
shadowed() {
  local in="$1" s="$2" out="$3" crop="${4:-}"
  local pre=(); [ -n "$crop" ] && pre=(-crop "$crop" +repage)
  magick "$in" "${pre[@]}" -resize "${s}%" \
    \( +clone -background black -shadow "${SHADOW_ALPHA}x${SHADOW_SIGMA}+0+${SHADOW_OFF}" \) \
    +swap -background none -layers merge +repage "$out"
}

# compose_frame <FRAME_ID> — gradient + each placed (shadowed) shot -> $FRAMES/<id>.png.
# Composites iteratively; for each shot it measures the shadowed layer vs the
# scaled shot to recover the shot's offset inside the layer, so the x/y from
# place_shots always positions the SHOT (not the shadow bbox).
compose_frame() {
  local fid="$1"
  local out="$FRAMES/$fid.png"
  gradient_bg "$out"
  while read -r shot x y s; do
    [ -z "$shot" ] && continue
    local src="$SHOTS/$shot.png"
    # FATAL, not a warning. This used to `continue`, so a frame whose shot never got
    # captured was composited as bare gradient — a blank slide — and the script went
    # on to encode it into the animation and exit 0. The artifact this produces is
    # published marketing material, and "it ran clean" is precisely when nobody opens
    # the WebP to look. A missing shot means the capture stage failed; say so.
    [ -f "$src" ] || {
      echo "ERROR: shot $src missing for frame $fid — capture stage did not produce it." >&2
      echo "       Re-run: $0 capture   (or $0 $fid to redo this frame end-to-end)" >&2
      return 1
    }
    local sh; sh="$(mktemp /tmp/scrib-sh.XXXXXX.png)"
    shadowed "$src" "$s" "$sh" "${SHOT_CROP[$shot]:-}"
    # shot size after scaling, and the shadowed layer size -> the shot's inset.
    # Sized from the shot's OWN source (crop-aware), never from WIN_W/WIN_H.
    local sw sh_h lw lh srcw srch
    read -r srcw srch <<<"$(shot_dims "$shot")"
    sw=$(( srcw*s/100 )); sh_h=$(( srch*s/100 ))
    # here-string (not <(...)) so `read` sees a trailing newline and returns 0 —
    # magick's -format emits none, and a nonzero read aborts under `set -e`.
    read -r lw lh <<<"$(magick identify -format '%w %h' "$sh")"
    local insx=$(( (lw - sw)/2 )) insy=$(( (lh - sh_h)/2 ))
    magick "$out" "$sh" -geometry "+$((x-insx))+$((y-insy))" -composite "$out"
    rm -f "$sh"
  done < <(place_shots "$fid")
  echo "composited frame $fid -> $out"
}

# ═════════════════════════════════════════════════════════════════════════════
# STAGE 3+4 — tween (ffmpeg xfade) + encode (webp + gif)
# ═════════════════════════════════════════════════════════════════════════════
HOLD_SECS=2.2          # keyframe dwell (seconds)
XFADE_SECS=0.8         # crossfade duration (seconds)
FPS=20                 # WebP frame rate
GIF_FPS=8              # GIF frame rate — lower than WebP to cut file weight
                       # (the static holds carry it; only the crossfades lose frames)
OUT_W=900              # output width (px); height scales to keep the aspect
WEBP_Q=95              # WebP quality — the THRESHOLD for a clean drop shadow, not a
                       # cosmetic knob. libwebp encodes most frames as LOSSY DELTAS
                       # blended on the previous one (webpmux -info shows partial
                       # width/height + x/y_offset, blend:yes), so error compounds
                       # across a crossfade over the gradient and surfaces as
                       # horizontal STREAKS in the drop shadow's soft falloff —
                       # residue where a previous, differently-placed frame's window
                       # edge sat. Measured sweep on identical frames: q84 -> 2.99 MB
                       # visibly streaky · q90 -> 3.81 MB still streaky · q95 -> 4.95
                       # MB clean · lossless -> 27.4 MB perfect. 95 is the threshold;
                       # that size is the price of a clean shadow.
                       #
                       # TWO DEAD ENDS — do NOT retry (each cost real time):
                       #  (1) "It's gradient banding; dither it away." It is NOT
                       #      banding. Adding grain (-attenuate/+noise) to the gradient
                       #      or the finished canvas does NOT remove the streaks and
                       #      costs ~0.7 MB on the GIF. The lossless control proves it:
                       #      -lossless 1 on the SAME frames makes the artefact vanish
                       #      entirely -> quantization residue, not banding, so no
                       #      source dither can address it. (A temporal noise filter
                       #      also makes every hold frame differ, destroying libwebp's
                       #      hold-dedup and exploding the file.)
                       #  (2) "RMSE will tell you." RMSE is BLIND to this artefact —
                       #      hold-frame RMSE vs the pristine composite is flat
                       #      (~0.0197) while the streaks are plainly visible: the
                       #      error is tiny but STRUCTURED. Judge by eye or a lossless
                       #      control, never a global metric.
XFADE_KIND=fade        # any ffmpeg xfade transition (fade|dissolve|slideleft…)
# Final artifacts land in the tracked, versioned assets/ dir (the README points
# here); the shots/ and frames/ scratch stays under git-ignored target/.
WEBP="$ROOT/assets/splash.webp"
GIF="$ROOT/assets/splash.gif"

# encode — tween the keyframes with xfade and encode a looping WebP + GIF.
# The cycle is FRAME_ORDER with the first frame appended (A B C D A) so the final
# D->A crossfade lands back on the opening frame: a seamless infinite loop.
#
# Inspecting the encoded animation:  magick <file>.webp -coalesce out_%03d.png
# extracts the composited canvas frames. A bare `[n]` extract instead yields the
# raw DELTA rect (e.g. 901x536+148+184), NOT the canvas — comparing that against a
# full frame gives a meaningless RMSE. ffmpeg CANNOT demux animated WebP; use
# ImageMagick, or extract from the GIF.
encode() {
  require ffmpeg
  local seq=("${FRAME_ORDER[@]}" "${FRAME_ORDER[0]}")     # e.g. A B C D A

  # Per-input duration MUST be HOLD + 2*XFADE, not HOLD + XFADE: a middle frame
  # has to fade IN, hold, and fade OUT. With only HOLD+XFADE its material ends at
  # exactly the moment its fade-out starts, so every transition after the first
  # has no source and hard-cuts instead of crossfading. Diagnosis tell: webpmux
  # -info then shows ONE long-duration hold frame instead of one per keyframe.
  local dur; dur="$(awk "BEGIN{print $HOLD_SECS + 2*$XFADE_SECS}")"

  local inputs=() f
  for f in "${seq[@]}"; do
    [ -f "$FRAMES/$f.png" ] || { echo "missing keyframe $FRAMES/$f.png — run composite first" >&2; return 1; }
    inputs+=(-loop 1 -t "$dur" -i "$FRAMES/$f.png")
  done

  # xfade chain. Each input holds HOLD then crossfades XFADE into the next;
  # offset_k = (k+1)*HOLD + k*XFADE keeps every hold equal.
  local filt="" v="[0:v]" k
  for ((k=0; k<${#seq[@]}-1; k++)); do
    local off; off="$(awk "BEGIN{print ($k+1)*$HOLD_SECS + $k*$XFADE_SECS}")"
    filt+="${v}[$((k+1)):v]xfade=transition=$XFADE_KIND:duration=$XFADE_SECS:offset=$off[x$k];"
    v="[x$k]"
  done

  # Trim the instant the final D->A fade COMPLETES: total = frames*(HOLD+XFADE).
  # The trailing A is then zero-length, so on loop the opening A hold supplies its
  # dwell — A appears exactly once and the wrap is seamless. Without this the
  # appended A holds on top of the opening A and the loop visibly stutters.
  local total; total="$(awk "BEGIN{print ${#FRAME_ORDER[@]}*($HOLD_SECS+$XFADE_SECS)}")"
  # shared xfade chain + scale (frame rate applied per-format below)
  local chain="${filt}${v}scale=$OUT_W:-2:flags=lanczos"

  echo "encoding WebP -> $WEBP (${total}s loop)"
  ffmpeg -y "${inputs[@]}" -filter_complex "${chain},fps=$FPS,format=yuva420p[out]" \
    -map "[out]" -t "$total" -c:v libwebp_anim -loop 0 -lossless 0 -q "$WEBP_Q" \
    -preset drawing "$WEBP" -loglevel error
  echo "encoding GIF  -> $GIF (${total}s loop)"
  ffmpeg -y "${inputs[@]}" -filter_complex \
    "${chain},fps=$GIF_FPS[z];[z]split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=3[out]" \
    -map "[out]" -t "$total" -loop 0 "$GIF" -loglevel error
  ls -la "$WEBP" "$GIF"
}

# ═════════════════════════════════════════════════════════════════════════════
# drivers
# ═════════════════════════════════════════════════════════════════════════════
# `|| return 1` on each call is belt-and-braces over `set -e`, which is famously
# unreliable about propagating a failure out of a function called from some contexts.
# The property being protected is that a stage which produced nothing must not report
# success to the next stage — the whole point of making a missing shot fatal.
stage_capture()   { require Xvfb; require xdotool; require xwd; require magick; require_binary
                    for id in "${!SHOT_SPEC[@]}"; do capture_shot "$id" || return 1; done; }
stage_composite() { require magick
                    for f in "${FRAME_ORDER[@]}"; do compose_frame "$f" || return 1; done; }

one_frame() {  # capture just this frame's shots, then composite it
  require Xvfb; require xdotool; require xwd; require magick; require_binary
  for s in ${FRAME_SHOTS[$1]}; do capture_shot "$s" || return 1; done
  compose_frame "$1"
}

case "${1:-all}" in
  capture)      stage_capture ;;
  composite)    stage_composite ;;
  encode)       encode ;;
  A|B|C|D)      one_frame "$1" ;;
  all)          stage_capture; stage_composite; encode ;;
  *) echo "usage: $0 {capture|composite|encode|A|B|C|D|all}" >&2; exit 2 ;;
esac
