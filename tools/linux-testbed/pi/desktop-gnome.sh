# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# shellcheck shell=bash

# GNOME side of testbed-pi. GNOME Shell answers its Eval method only in
# unsafe mode, which the testbed's Shell extension switches on
# (tools/linux-testbed/provision/gnome/).

# shell_eval JS: prints GNOME Shell's answer, `(true, '<JSON>')` on
# success, `(false, '<error>')` when the code threw.
shell_eval() {
    gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
        --method org.gnome.Shell.Eval "$1"
}

# desk_eval JS: prints the value of JS as JSON. GNOME Shell returns the
# value JSON-encoded inside a GVariant string; Python's literal syntax
# reads that string form, escapes included.
desk_eval() {
    local answer
    answer="$(shell_eval "$1")" || return 1
    python3 -c '
import ast, sys
answer = sys.argv[1].strip()
ok, value = ast.literal_eval(answer.replace("(true,", "(True,", 1).replace("(false,", "(False,", 1))
if not ok:
    sys.exit("testbed-pi: GNOME Shell refused Eval: " + value)
print(value)
' "$answer"
}

# The app's windows. GNOME Shell knows them by their WM class, `torchsnap`;
# the launcher is the one without a title (src-tauri/src/lib.rs creates it
# with `.title("")`).
readonly app_windows_js='global.get_window_actors().map(a => a.meta_window).filter(w => w.get_wm_class() === "torchsnap").map(w => { const r = w.get_frame_rect(); return {title: w.get_title(), wm_class: w.get_wm_class(), rect: {x: r.x, y: r.y, width: r.width, height: r.height}, monitor: w.get_monitor(), focused: w.has_focus()}; })'

desk_app_windows() {
    desk_eval "$app_windows_js"
}

# desk_window_rect REGEX: prints "X Y WIDTH HEIGHT" of the first window
# whose title or WM class matches REGEX (Python syntax).
desk_window_rect() {
    desk_eval 'global.get_window_actors().map(a => a.meta_window).map(w => { const r = w.get_frame_rect(); return {title: w.get_title(), wm_class: w.get_wm_class(), x: r.x, y: r.y, width: r.width, height: r.height}; })' \
        | python3 -c '
import json, re, sys
pattern = re.compile(sys.argv[1])
for w in json.load(sys.stdin):
    if pattern.search(w["title"] or "") or pattern.search(w["wm_class"] or ""):
        print(w["x"], w["y"], w["width"], w["height"])
        break
else:
    sys.exit("testbed-pi: no window matches " + sys.argv[1])
' "$1"
}

# desk_monitor_rect N: prints "X Y WIDTH HEIGHT" of monitor N in logical
# pixels.
desk_monitor_rect() {
    desk_eval "(m => ({x: m.x, y: m.y, width: m.width, height: m.height}))(Main.layoutManager.monitors[$1])" \
        | python3 -c 'import json, sys; m = json.load(sys.stdin); print(m["x"], m["y"], m["width"], m["height"])'
}

# desk_screenshot FILE [X Y WIDTH HEIGHT]: the whole screen, or an area in
# logical pixels; the file has the screen's physical pixels.
desk_screenshot() {
    local file="$1"
    shift
    local answer
    if [ $# -eq 4 ]; then
        answer="$(gdbus call --session --dest org.gnome.Shell.Screenshot --object-path /org/gnome/Shell/Screenshot \
            --method org.gnome.Shell.Screenshot.ScreenshotArea "$1" "$2" "$3" "$4" false "$file")" || return 1
    else
        answer="$(gdbus call --session --dest org.gnome.Shell.Screenshot --object-path /org/gnome/Shell/Screenshot \
            --method org.gnome.Shell.Screenshot.Screenshot false false "$file")" || return 1
    fi
    case "$answer" in
        "(true, "*) ;;
        *)
            echo "testbed-pi: GNOME Shell did not take the screenshot: $answer" >&2
            return 1
            ;;
    esac
}

# =========================================================
# Input support
# =========================================================

# Each Eval below starts with a comment naming what it asks, which keeps
# them apart in the tests' stubs.

# The focused window as {"title", "wm_class"}, or null.
desk_focused_window() {
    desk_eval '/*focus*/ (w => w ? {title: w.get_title(), wm_class: w.get_wm_class()} : null)(global.display.focus_window)'
}

# Whether a GNOME Shell dialog or menu holds the keyboard: Shell UI that
# takes keys does so modally, with a stage actor in key focus, and no
# window is the target then.
desk_shell_modal() {
    [ "$(desk_eval '/*modal*/ Main.modalCount > 0 && global.stage.get_key_focus() !== null')" = "true" ]
}

# The keyboard layouts live in GNOME Shell's input source manager. It is
# not public API: the keyboard indicator in the top bar holds it
# (GNOME Shell 48).
readonly input_sources='Main.panel.statusArea.keyboard._inputSourceManager'

# {"current": INDEX, "us": INDEX or null}
desk_layouts() {
    desk_eval "/*layouts*/ (m => ({current: m.currentSource.index, us: (Object.values(m.inputSources).find(s => s.id === \"us\") || {index: null}).index}))($input_sources)"
}

desk_current_layout() {
    desk_eval "/*current*/ $input_sources.currentSource.index"
}

desk_activate_layout() {
    desk_eval "/*activate*/ $input_sources.inputSources[$1].activate(false)" >/dev/null
}

# The pointer position in logical pixels, as [x, y].
desk_pointer() {
    desk_eval '/*pointer*/ global.get_pointer().slice(0, 2)'
}

# =========================================================
# Headless sessions
# =========================================================

# A second gnome-shell without a screen, with virtual monitors, on a D-Bus
# session bus of its own (dbus-run-session) and with XDG directories of
# its own, so it shares no settings or state with the desktop session.
# The testbed extension is installed system-wide, so it runs there too.
# The variables of the desktop session the user manager hands down are
# removed first; with them the new shell would connect to the desktop's
# display. Without Xwayland (--no-x11), the app's X11-backed clipboard
# gadget does not start in it.
desk_headless_start() {
    local monitors="" gdctl_args=() index=0 spec previous=""
    local scales=()
    for spec in "$@"; do
        monitors="$monitors --virtual-monitor ${spec%@*}"
        scales+=("${spec#*@}")
        if [ "$index" -eq 0 ]; then
            gdctl_args+=(--logical-monitor --primary --scale "${spec#*@}" --monitor "Meta-$index")
        else
            gdctl_args+=(--logical-monitor --scale "${spec#*@}" --right-of "$previous" --monitor "Meta-$index")
        fi
        previous="Meta-$index"
        index=$((index + 1))
    done
    local shell_dir="$headless_dir/shell"
    mkdir -p "$shell_dir"/{config,data,cache,state}
    systemd-run --user --unit "$headless_unit" --collect --quiet \
        -p "StandardOutput=append:$headless_dir/shell.log" -p "StandardError=append:$headless_dir/shell.log" \
        env -u WAYLAND_DISPLAY -u DISPLAY -u XAUTHORITY \
        "XDG_CONFIG_HOME=$shell_dir/config" "XDG_DATA_HOME=$shell_dir/data" \
        "XDG_CACHE_HOME=$shell_dir/cache" "XDG_STATE_HOME=$shell_dir/state" \
        dbus-run-session -- bash -c "echo \"\$DBUS_SESSION_BUS_ADDRESS\" >'$headless_bus_file'; exec gnome-shell --headless --wayland --no-x11 --wayland-display $headless_display$monitors"

    wait_for "$headless_timeout" test -s "$headless_bus_file" \
        || { echo "testbed-pi: the headless session did not start; see $headless_dir/shell.log" >&2; return 1; }
    DBUS_SESSION_BUS_ADDRESS="$(cat "$headless_bus_file")"
    export DBUS_SESSION_BUS_ADDRESS
    wait_for "$headless_timeout" headless_shell_answers \
        || { echo "testbed-pi: the headless GNOME Shell does not answer" >&2; return 1; }
    gdctl set "${gdctl_args[@]}"
    wait_for "$headless_timeout" headless_scales_are "${scales[@]}" \
        || { echo "testbed-pi: the headless monitors did not get the scales ${scales[*]}" >&2; return 1; }
}

headless_shell_answers() {
    [ "$(desk_eval '/*ready*/ true')" = "true" ]
}

headless_scales_are() {
    desk_eval '/*scales*/ Main.layoutManager.monitors.map(m => m.geometry_scale)' \
        | python3 -c 'import json, sys; sys.exit(0 if json.load(sys.stdin) == [float(s) for s in sys.argv[1:]] else 1)' "$@"
}

# =========================================================
# Tray
# =========================================================

# GNOME Shell shows tray icons (StatusNotifierItems) only with the
# AppIndicator extension, which Debian ships as this uuid.
readonly appindicator_uuid="ubuntu-appindicators@ubuntu.com"

desk_tray_state() {
    gnome-extensions info "$appindicator_uuid" | sed -n 's/^ *State: //p'
}

desk_tray_switch() {
    local want="$1"
    if [ "$want" = "on" ]; then
        gnome-extensions enable "$appindicator_uuid"
        wait_for 10 tray_state_is ACTIVE \
            || { echo "testbed-pi: the AppIndicator extension did not become ACTIVE" >&2; return 1; }
    else
        gnome-extensions disable "$appindicator_uuid"
        wait_for 10 tray_state_is INACTIVE \
            || { echo "testbed-pi: the AppIndicator extension did not become INACTIVE" >&2; return 1; }
    fi
}

tray_state_is() {
    [ "$(desk_tray_state)" = "$1" ]
}

# The extension adds one item per tray icon to the top bar's status area,
# keyed `appindicator-…`; Torchsnap's carries the title `torchsnap`. The
# item's `_indicator` is the extension's internal object (extension 59).
desk_tray_items() {
    desk_eval '/*tray*/ Object.entries(Main.panel.statusArea).filter(([k]) => k.startsWith("appindicator")).map(([k, v]) => { const [x, y] = v.get_transformed_position(); const [w, h] = v.get_transformed_size(); return {key: k, title: v._indicator ? v._indicator.title : null, x: Math.round(x), y: Math.round(y), width: Math.round(w), height: Math.round(h), menuOpen: v.menu ? v.menu.isOpen : false}; })'
}

# Prints {"x", "y", "width", "height", "menuOpen"} of Torchsnap's tray
# icon in logical pixels.
desk_tray_icon() {
    desk_tray_items | python3 -c '
import json, sys
item = next((i for i in json.load(sys.stdin) or [] if i["title"] == "torchsnap"), None)
if item is None:
    sys.exit("testbed-pi: no Torchsnap tray icon in the top bar")
print(json.dumps({k: item[k] for k in ("x", "y", "width", "height", "menuOpen")}))
'
}

# The session takes input once GNOME Shell has finished starting and the
# overview it opens after login is closed again (the testbed extension
# closes it). An open overview would take every typed key into its search.
desk_session_ready() {
    local state
    if ! state="$(shell_eval 'JSON.stringify({startingUp: Main.layoutManager._startingUp, overview: Main.overview.visible})' 2>&1)"; then
        echo "testbed-pi: GNOME Shell does not answer: $state" >&2
        return 1
    fi
    case "$state" in
        "(true, "*) ;;
        *)
            echo "testbed-pi: GNOME Shell refused Eval ($state); is the testbed extension active?" >&2
            return 1
            ;;
    esac
    case "$state" in
        *'startingUp\\":false'*) ;;
        *)
            echo "testbed-pi: GNOME Shell is still starting: $state" >&2
            return 1
            ;;
    esac
    case "$state" in
        *'overview\\":false'*) ;;
        *)
            echo "testbed-pi: the overview is open: $state" >&2
            return 1
            ;;
    esac
}
