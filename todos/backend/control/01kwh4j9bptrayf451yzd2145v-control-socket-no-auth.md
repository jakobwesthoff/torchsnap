---
kind: bug
severity: low
status: open
area: [src-tauri/src/control/mod.rs]
tags: [unconfirmed, security]
---

# Control Unix socket: no peer authentication, umask-dependent permissions, and unconditional unlink (socket steal; no single-instance guard)

## Problem
The control server binds a Unix socket at
`<app_data_dir>/control.sock` (`mod.rs:116-128,265-270`) with no
explicit mode, no peer-credential check, and an unconditional
`remove_file` before bind:

```rust
let path = socket_path(app);
let _ = std::fs::remove_file(&path);
let listener = UnixListener::bind(&path)...;
```

`handle_connection` (`:182-207`) reads newline-delimited JSON-RPC and
dispatches with no authentication. The server is opt-in, gated by
`controlChannel.enabled` (default false, `:285-288`).

## Impact

### Permissions / same-user determination
- **Socket mode.** `UnixListener::bind` creates the inode with
  `0777 & ~umask` → 0755 under the usual umask 022 (verified
  empirically: `srwxr-xr-x`; confirmed in XNU `unp_bind` and Linux
  `unix(7)`).
- **Connect semantics.** The "macOS ignores socket file permission
  bits" lore is outdated (4.2BSD-era). Modern XNU `unp_connect` does
  `vnode_authorize(..., KAUTH_VNODE_WRITE_DATA, ...)` — connect
  requires write permission on the socket inode, same as Linux
  (`unix(7)`). Both also require search (`x`) on every parent
  directory. POSIX leaves socket-mode enforcement unspecified, so
  portable hardening should not rely on the socket mode alone, but both
  target platforms enforce it today.
- **Parent directories.** macOS `~/Library` is 0700 by platform
  default (verified), so other local users are stopped at the
  directory walk regardless of the socket mode. Linux
  `~/.local/share/app.torchsnap` is 0755 (`create_dir_all` under umask
  022); whether `~` is 0700/0750 (Fedora, Ubuntu ≥21.04, Debian ≥12)
  or 0755 (older/other distros) is distro- and history-dependent —
  **not guaranteed**.

Verdict: same-user-only access holds in practice on both platforms,
but on Linux it rests on two accidents — the umask-022 socket mode
(0755 lacks the write bit others would need to connect) and a
possibly-0700 home dir. Nothing in the code guarantees it: a user with
a permissive umask (002 on a non-user-private-group system, or 0) on a
0755-home Linux box exposes the socket to other local users. macOS is
robust via the 0700 `~/Library`.

### Blast radius
All six handlers confirmed (`control/handlers/{launcher,query,status}.rs`);
the set is complete (`handlers/mod.rs:12-19`), and none run gadget
actions / open paths / execute commands:
- `show`/`hide`/`toggle`/`dismiss`: window visibility only.
- `query`: pushes `ControlCommand::SetQuery{text}` to the frontend
  channel (`query.rs:36`) — it does not execute results, but it drives
  the normal search path, so it is equivalent to typing into the search
  box: gadget WASM `query` handlers run against attacker-chosen text,
  and URL-shaped results can trigger the host's website-metadata /
  favicon fetcher. So an unauthenticated peer gets a **weak
  network-beacon primitive** ("make the app fetch metadata for my
  URL"), not just cosmetics.
- `status`: returns only `{"visible": bool}` (`status.rs:34-36`); query
  text and selected result are deliberately excluded (`status.rs:8-10`).

Realistic impact of an unauthenticated peer: UI manipulation
(show/hide/toggle/dismiss, query injection with the beacon caveat) plus
a visibility boolean. No secrets, no execution.

### Socket steal / no single-instance guard
`mod.rs:120` unconditionally `remove_file`s before bind, and
**Torchsnap has no single-instance guard** (`tauri-plugin-single-instance`
is absent from `Cargo.toml`; no flock/lockfile/pidfile anywhere in
`src-tauri/src`). The steal is real and bidirectional:
- Instance B unlinks A's socket and binds its own; A keeps serving
  already-connected clients on the unlinked inode, while all new
  clients silently reach B.
- Reverse: `stop()` (`:169-171`), the accept-loop cleanup (`:161`), and
  the `RunEvent::Exit` cleanup (`lib.rs:1001-1004`) all unlink the path
  unconditionally, so if instance A later exits or toggles the setting
  off, it deletes **B's live socket file**, leaving B listening on an
  unreachable inode — silent DoS with no error on either side.

Same-user only (the path is inside the user's own data dir), so this is
integrity/DoS of the control channel, not privilege escalation. It is
one symptom of the broader missing single-instance story: two instances
also double-write `settings.json` and the SQLite databases, and race on
global-shortcut registration.

### Severity: low
Opt-in and default-off; the handler set has no execute capability;
`status` leaks one boolean; the same-user boundary holds on macOS by
platform defaults and on typical Linux via the umask-022 socket mode
the kernel enforces on connect. Residual exposure: (a) permissive-umask
Linux setups leaking UI control + a fetch beacon to other local users;
(b) same-user multi-instance silent takeover/DoS.

## Suggested fix (in order of value)
1. **Make the mode explicit:** `set_permissions(0600)` on the socket
   path immediately after bind (`std::os::unix::fs::PermissionsExt`),
   removing the umask dependency. The bind-to-chmod window is
   theoretical (pre-chmod 0755 grants no connect under any sane umask);
   for an airtight version, bind inside a freshly created 0700
   subdirectory (`DirBuilderExt::mode(0o700)`), which is race-free and
   umask-independent (the tmux/ssh-agent pattern). On Linux the
   canonical location for such a socket is `$XDG_RUNTIME_DIR` (0700
   tmpfs, per-session, auto-cleaned; sockets in persistent data dirs
   are an anti-pattern) — but moving the path changes the documented
   client contract, so treat that as a separate decision.
2. **Peer-cred same-uid check — not worth it as a security boundary,
   cheap as belt-and-braces.** A same-uid attacker already owns
   `settings.json`, the frecency SQLite, and the gadget code directory
   under `app_data_dir`, all far more powerful than this socket. (On
   macOS synthetic input is TCC-gated, so the socket is not strictly
   redundant with "can already send keystrokes"; the data-dir write
   access is the stronger equivalence.) If added anyway, it is two
   syscalls (`SO_PEERCRED` on Linux, `getpeereid`/`LOCAL_PEERCRED` on
   macOS); a token handshake is overkill for this handler set. **Record
   the trigger: if execute-class methods (run a result, open a path)
   are ever added to the control API, authentication must be revisited
   before shipping them.**
3. **Steal fix:** probe before unlink — `connect()` to the existing
   path; `ECONNREFUSED`/`ENOENT` means stale (safe to remove), success
   means a live instance owns it (log, refuse to start the server or
   surface the conflict). Gate the unlink in `stop()`/exit cleanup on
   still owning the file (compare `fstat` of the listener inode vs
   `lstat` of the path) so a dying first instance does not clobber the
   second's socket. Preferably, adopt `tauri-plugin-single-instance`
   (macOS + Linux on Tauri v2), which makes the steal moot and
   simultaneously fixes the `settings.json`/SQLite double-writer and
   global-shortcut races that multi-instance already causes today.

## Files
`control/mod.rs` (`:116-128` bind, `:161`/`:169-171` unlink, `:265-270`
path, `:285-288` default-off), `control/handlers/{mod,launcher,query,status}.rs`,
`lib.rs:1001-1010` (exit cleanup), `Cargo.toml` (no single-instance
plugin).
