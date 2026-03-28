#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# =========================================================
# Control API Demo
#
# Demonstrates driving the Torchsnap launcher through the
# Control API. Shows the launcher, types a query, waits a
# moment, then dismisses.
#
# Prerequisites:
#   - Torchsnap running with Control API enabled in settings
#   - socat installed (brew install socat)
# =========================================================

set -euo pipefail

SOCK="$HOME/Library/Application Support/app.torchsnap/control.sock"

if [[ ! -S "$SOCK" ]]; then
    echo "Error: Control socket not found at $SOCK"
    echo "Make sure Torchsnap is running and the Control API is enabled in settings."
    exit 1
fi

if ! command -v socat &>/dev/null; then
    echo "Error: socat is not installed. Install it with: brew install socat"
    exit 1
fi

# Helper: send a JSON-RPC request and print the response.
LAST_ID=0
send() {
    local method="$1"
    local params="${2:-}"

    local id="$((LAST_ID++))"

    local request
    if [[ -n "$params" ]]; then
        request="{\"jsonrpc\":\"2.0\",\"id\":$id,\"method\":\"$method\",\"params\":$params}"
    else
        request="{\"jsonrpc\":\"2.0\",\"id\":$id,\"method\":\"$method\"}"
    fi

    echo "→ $method"
    local response
    response=$(echo "$request" | socat - UNIX-CONNECT:"$SOCK")
    echo "← $response"
    echo
}

echo "=== Torchsnap Control API Demo ==="
echo

# Check current status
send "status"

# Show the launcher
send "show"
sleep 0.5

# Type a search query
send "query" '{"text":"settings"}'
sleep 2

# Type a search query
send "query" '{"text":""}'
sleep 2

# Type a search query
send "query" '{"text":":rocket"}'
sleep 2

# Dismiss (reset state + hide)
send "dismiss"

echo "Done."
