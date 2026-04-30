# Control API

The Control API lets external programs drive Torchsnap over a local Unix
socket. You can show and hide the launcher, set the search query, check
status, and more — all from shell scripts, automation tools, or custom
integrations.

## Enabling the Control API

The Control API is disabled by default. To enable it:

1. Open Torchsnap Settings (right-click the tray icon → Settings).
2. Navigate to the **General** section.
3. Toggle **Control API** on.

Once enabled, Torchsnap opens a Unix domain socket that accepts connections
immediately. Turning the toggle off closes the socket and disconnects all
clients.

## Connecting

The socket is located in the Torchsnap application data directory:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/app.torchsnap/control.sock` |
| Linux | `~/.local/share/app.torchsnap/control.sock` (XDG default) |

Connect with any tool that speaks Unix sockets. Examples use `socat`, which
is available via Homebrew (`brew install socat`) or most Linux package
managers.

### Interactive session

```bash
socat - UNIX-CONNECT:"$HOME/Library/Application Support/app.torchsnap/control.sock"
```

Type JSON-RPC requests line by line. Press Ctrl-D to disconnect.

### One-shot command

```bash
SOCK="$HOME/Library/Application Support/app.torchsnap/control.sock"
echo '{"jsonrpc":"2.0","id":1,"method":"status"}' | socat - UNIX-CONNECT:"$SOCK"
```

## Protocol

The Control API uses [JSON-RPC 2.0](https://www.jsonrpc.org/specification)
over newline-delimited JSON:

- Each **request** is a single JSON object on one line, terminated by `\n`.
- Each **response** is a single JSON object on one line, terminated by `\n`.
- The client picks an `id` (string or number); the server echoes it in the
  response so you can match requests to responses.

### Request format

```json
{"jsonrpc": "2.0", "id": 1, "method": "show"}
{"jsonrpc": "2.0", "id": 2, "method": "query", "params": {"text": "firefox"}}
```

### Response format

Success:
```json
{"jsonrpc": "2.0", "id": 1, "result": {"ok": true}}
```

Error:
```json
{"jsonrpc": "2.0", "id": 1, "error": {"code": -32601, "message": "Method not found: foo"}}
```

## API Reference

### `show`

Make the launcher window visible. Positions it on the monitor under the
cursor, matching the behavior of the keyboard shortcut.

**Parameters:** none

**Response:**
```json
{"ok": true}
```

**Example:**
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"show"}' | socat - UNIX-CONNECT:"$SOCK"
```

---

### `hide`

Hide the launcher window without resetting its state. The current query
and selection are preserved for the next time the launcher is shown.

**Parameters:** none

**Response:**
```json
{"ok": true}
```

---

### `toggle`

Toggle the launcher's visibility. If visible, hides it. If hidden, shows
it (same as `show`).

**Parameters:** none

**Response:**
```json
{"ok": true}
```

---

### `dismiss`

Hide the launcher and reset its state — clears the search query,
selection, and any active plugin view. This matches the behavior of
pressing Escape in the launcher.

**Parameters:** none

**Response:**
```json
{"ok": true}
```

---

### `query`

Set the search input text. The launcher's search runs automatically
after the text is set, just as if the user had typed it.

**Parameters:**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `text` | string | yes | The search query text |

**Response:**
```json
{"ok": true}
```

**Example:**
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"query","params":{"text":"firefox"}}' \
  | socat - UNIX-CONNECT:"$SOCK"
```

---

### `status`

Report the current launcher state.

**Parameters:** none

**Response:**

| Field | Type | Description |
|-------|------|-------------|
| `visible` | boolean | Whether the launcher window is currently visible |

**Example:**
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"status"}' | socat - UNIX-CONNECT:"$SOCK"
# → {"jsonrpc":"2.0","id":1,"result":{"visible":false}}
```

## Error Codes

### Protocol errors

These are returned by the JSON-RPC framing layer before any handler runs.

| Code | Meaning |
|------|---------|
| `-32700` | Parse error — the request is not valid JSON |
| `-32600` | Invalid request — missing `method` field or malformed envelope |
| `-32601` | Method not found — no handler registered for this method name |

### Application errors

These are returned by individual handlers.

| Code | Meaning |
|------|---------|
| `-1` | Invalid state — a precondition is not met or a required parameter is missing/invalid |
| `-3` | Internal error — an unexpected failure in the handler |

## Multiple Clients

The Control API supports multiple simultaneous connections. Each client
operates independently — there is no shared state between connections
beyond the launcher itself.

## Scripting Examples

### Show the launcher, search, then dismiss

```bash
SOCK="$HOME/Library/Application Support/app.torchsnap/control.sock"

# Open a persistent connection with socat in bidirectional mode
{
  echo '{"jsonrpc":"2.0","id":1,"method":"show"}'
  sleep 0.5
  echo '{"jsonrpc":"2.0","id":2,"method":"query","params":{"text":"calculator"}}'
  sleep 2
  echo '{"jsonrpc":"2.0","id":3,"method":"dismiss"}'
} | socat - UNIX-CONNECT:"$SOCK"
```

### Check if the launcher is visible

```bash
SOCK="$HOME/Library/Application Support/app.torchsnap/control.sock"
VISIBLE=$(echo '{"jsonrpc":"2.0","id":1,"method":"status"}' \
  | socat - UNIX-CONNECT:"$SOCK" \
  | jq -r '.result.visible')

if [ "$VISIBLE" = "true" ]; then
  echo "Launcher is visible"
else
  echo "Launcher is hidden"
fi
```
