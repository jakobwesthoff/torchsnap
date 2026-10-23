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
