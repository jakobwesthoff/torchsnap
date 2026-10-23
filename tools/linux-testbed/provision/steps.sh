# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# shellcheck shell=bash

# Step bookkeeping for the provision script, sourced by `provision`.
#
# A step is a check and an apply function. The check decides whether the
# machine is already in the step's state; only then is the step skipped,
# which makes a second provision run change nothing. After an apply the
# same check runs again, so a step that claims success but leaves the
# machine unchanged fails the run instead of passing silently.
#
# An apply runs in a subshell with errexit on and fails at its first
# failing command. Variables it sets stay in that subshell; what it
# changes is the machine.

steps_changed=0
steps_ok=0
steps_deferred=0

# Some steps talk to the running desktop session (its settings, its
# keyring). A desktop file overrides this with a real test; without one,
# no session counts as running.
session_running() {
    return 1
}

# step NAME CHECK APPLY [session]
step() {
    local name="$1"
    local check="$2"
    local apply="$3"
    local needs="${4:-}"

    # A session step's check may itself need the session, so the session
    # comes first.
    if [ "$needs" = "session" ] && ! session_running; then
        echo "deferred $name (needs the desktop session)"
        steps_deferred=$((steps_deferred + 1))
        return 0
    fi

    if "$check"; then
        echo "ok $name"
        steps_ok=$((steps_ok + 1))
        return 0
    fi

    # Bash turns errexit off for everything an `if`, `!`, `&&` or `||`
    # tests, down into the functions and subshells it calls, so an apply
    # run there would fail only when its last command does. Run as a
    # plain command with errexit off around it, the subshell's own
    # `set -e` holds.
    local status errexit_was_on=false
    [[ $- == *e* ]] && errexit_was_on=true
    set +e
    (
        set -e
        "$apply"
    )
    status=$?
    if "$errexit_was_on"; then
        set -e
    fi
    if [ "$status" -ne 0 ]; then
        echo "failed $name (apply failed)" >&2
        exit 1
    fi
    if ! "$check"; then
        echo "failed $name (check still fails after apply)" >&2
        exit 1
    fi
    echo "changed $name"
    steps_changed=$((steps_changed + 1))
}

# wait_for TIMEOUT INTERVAL COMMAND...: runs COMMAND every INTERVAL
# seconds until it succeeds; fails after TIMEOUT seconds.
wait_for() {
    local timeout="$1"
    local interval="$2"
    shift 2
    local deadline=$((SECONDS + timeout))
    until "$@" >/dev/null 2>&1; do
        if [ "$SECONDS" -ge "$deadline" ]; then
            return 1
        fi
        sleep "$interval"
    done
}

# Prints the summary line and returns 3 when steps were deferred, so the
# caller knows a second run inside the desktop session is due.
steps_summary() {
    echo "provision: $steps_changed changed, $steps_ok ok, $steps_deferred deferred"
    if [ "$steps_deferred" -gt 0 ]; then
        return 3
    fi
    return 0
}
