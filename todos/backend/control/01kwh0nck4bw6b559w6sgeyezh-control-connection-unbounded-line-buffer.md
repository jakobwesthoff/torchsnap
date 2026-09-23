# Control connection reads request lines into an unbounded buffer

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/control/mod.rs

## Problem

`handle_connection` (`src-tauri/src/control/mod.rs:182-207`)
reads each request with `BufReader::read_line` into a `String`:

```rust
let mut line = String::new();
loop {
    line.clear();
    match reader.read_line(&mut line).await {
        Ok(0) => break,
        Ok(_) => { /* process */ }
        Err(_) => break,
    }
}
```

`read_line` appends until it sees `\n` with no length limit. A
client that never sends a newline (or sends one gigantic request
line) grows the buffer without bound; the host allocates as much
memory as the client cares to stream. There is also no cap on
concurrent connections, so the per-connection buffers multiply.

The socket is same-user local (mode-default Unix socket in
`app_data_dir`), so this is a robustness issue rather than a
privilege boundary, but a misbehaving script (e.g. accidentally
`cat`-ing a large file into `socat`) takes the whole app down via
memory pressure rather than getting an error.

## Impact

Runaway memory growth in the main app process from a single
misbehaving local client; worst case OOM kill of the launcher.

## Suggested fix

Use a length-limited read (e.g. `AsyncBufReadExt::take` around
the reader, or check `line.len()` against a cap such as 1 MiB and
reply with JSON-RPC `-32700`/`-32600` then drop the connection).
Document the request-size limit in `docs/control-api.md` once one
exists.
