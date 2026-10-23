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
