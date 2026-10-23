---
title: "Control socket relies on the umask and startup unlinks an existing socket"
kind: bug
component: backend
impact: low
tags: [security]
---
# Control socket relies on the umask and startup unlinks an existing socket

The control server binds a Unix socket at `<app_data_dir>/control.sock` with no explicit mode, no peer-credential check, and an unconditional unlink before bind. Access today rests on the umask and the directory tree, and a second instance can take the socket over. Severity is low: the server is opt-in (`controlChannel.enabled`, default off), and its handler set has no execute capability. Since 2026-10-23 a peer can also read the launcher's state back (`snapshot`), which widens what a peer outside the user would get on a Linux setup where the socket is reachable.

## Problem

Read in the code. The socket mode was checked empirically (`srwxr-xr-x` under umask 022). The rest comes from the code and the platform documentation.

The control server in `src-tauri/src/control/mod.rs` binds the socket with no explicit mode and no peer-credential check, after an unconditional `remove_file`:

```rust
let path = socket_path(app);
let _ = std::fs::remove_file(&path);
let listener = UnixListener::bind(&path)...;
```

`handle_connection` reads newline-delimited JSON-RPC and dispatches with no authentication.

## Impact

### Permissions and same-user determination

- **Socket mode.** `UnixListener::bind` creates the inode with `0777 & ~umask`, which gives 0755 under the usual umask 022 (verified empirically: `srwxr-xr-x`; confirmed in XNU `unp_bind` and Linux `unix(7)`).
- **Connect semantics.** The "macOS ignores socket file permission bits" lore is outdated (4.2BSD-era). Modern XNU `unp_connect` runs `vnode_authorize(..., KAUTH_VNODE_WRITE_DATA, ...)`, so connect requires write permission on the socket inode, as on Linux (`unix(7)`). Both also require search (`x`) on every parent directory. POSIX leaves socket-mode enforcement unspecified, so portable hardening should not rely on the socket mode alone. Both target platforms enforce it today.
- **Parent directories.** macOS `~/Library` is 0700 by platform default (verified), so other local users are stopped at the directory walk regardless of the socket mode. Linux `~/.local/share/app.torchsnap` is 0755 (`create_dir_all` under umask 022). Whether `~` is 0700 or 0750 (Fedora, Ubuntu 21.04 and later, Debian 12 and later) or 0755 (older or other distros) depends on the distro and its history, so it is **not guaranteed**.

Verdict: same-user-only access holds in practice on both platforms. On Linux it rests on two accidents: the umask-022 socket mode (0755 lacks the write bit others would need to connect) and a possibly-0700 home dir. Nothing in the code guarantees it. A user with a permissive umask (002 on a system without user-private groups, or 0) on a 0755-home Linux box exposes the socket to other local users. macOS is robust through the 0700 `~/Library`.

### Blast radius

Nine handlers are registered (`register_all` in `control/handlers/mod.rs`, read 2026-10-23). None runs gadget actions, opens paths or executes commands:

- `show`, `hide`, `toggle`, `dismiss`: window visibility only.
- `query`: pushes `ControlCommand::SetQuery{text}` to the frontend channel. It does not execute results, but it drives the normal search path, so it is equivalent to typing into the search box. Gadget WASM `query` handlers run against attacker-chosen text, and URL-shaped results can trigger the host's website-metadata and favicon fetcher. So an unauthenticated peer gets a **weak network-beacon primitive** ("make the app fetch metadata for my URL"), not just cosmetics.
- `status`: `visible`, `ready`, `focused` and `targetMonitor` (the monitor's name and rect).
- `windows`: label, title, visibility and focus of each app window.
- `info`: process id, version, git hash, data folder, socket path, tray id and, on Linux, the display and its backend.
- `snapshot` (ADR 0069): the launcher's query, the results' sources, ids, titles and subtitles, the selection, the active gadget view, whether the content is visible and whether the input has focus.

`query` plus `snapshot` let a peer type a query and read back what the search providers return for it: app names and paths, and whatever the gadgets answer for that text. Realistic impact of an unauthenticated peer: UI manipulation (show, hide, toggle, dismiss, query injection with the beacon caveat) and reading the launcher's search results for queries of its choice, plus process and path details. No execution.

### Socket steal

`tauri-plugin-single-instance` is now registered (first plugin on the builder in `lib.rs`): a second launch hands its arguments to the running instance and exits before its setup runs, so the steal below no longer happens through a normal second launch. Whether any other path still runs two instances is not established. The unconditional `remove_file` before bind in `mod.rs` still exists, so if two instances do run, the steal is unchanged:

- Instance B unlinks A's socket and binds its own. A keeps serving already-connected clients on the unlinked inode, while all new clients silently reach B.
- In reverse, `stop()`, the accept-loop cleanup and the `RunEvent::Exit` cleanup in `lib.rs` all unlink the path unconditionally. If instance A later exits or toggles the setting off, it deletes **B's live socket file**, leaving B listening on an unreachable inode. That is a silent denial of service with no error on either side.

This is same-user only, since the path lies inside the user's own data dir, so it is an integrity and DoS issue for the control channel, not privilege escalation. It is one symptom of the broader missing single-instance story. Two instances also double-write `settings.json` and the SQLite databases, and race on global-shortcut registration.

### Severity: low

The server is opt-in and default-off, the handler set has no execute capability, and the same-user boundary holds on macOS through platform defaults and on typical Linux through the umask-022 socket mode that the kernel enforces on connect. Residual exposure: (a) permissive-umask Linux setups leaking UI control, a fetch beacon and read access to the launcher's search results to other local users; (b) same-user multi-instance silent takeover or DoS, if two instances still run despite the single-instance plugin.

## Suggested fix (in order of value)

1. **Make the mode explicit.** Call `set_permissions(0600)` on the socket path right after bind (`std::os::unix::fs::PermissionsExt`), which removes the umask dependency. The bind-to-chmod window is theoretical, since pre-chmod 0755 grants no connect under any sane umask. For an airtight version, bind inside a freshly created 0700 subdirectory (`DirBuilderExt::mode(0o700)`). That is race-free and umask-independent, the tmux and ssh-agent pattern. On Linux the canonical location for such a socket is `$XDG_RUNTIME_DIR` (0700 tmpfs, per session, auto-cleaned). Sockets in persistent data dirs are an anti-pattern. Moving the path changes the documented client contract, so treat that as a separate decision.
2. **Peer-credential same-uid check: not worth it as a security boundary, cheap as belt and braces.** A same-uid attacker already owns `settings.json`, the frecency SQLite database and the gadget code directory under `app_data_dir`, all far more powerful than this socket. (On macOS synthetic input is TCC-gated, so the socket is not strictly redundant with "can already send keystrokes". The data-dir write access is the stronger equivalence.) If added anyway, it is two syscalls (`SO_PEERCRED` on Linux, `getpeereid` or `LOCAL_PEERCRED` on macOS). A token handshake is overkill for this handler set. **Record the trigger: if execute-class methods (run a result, open a path) are ever added to the control API, authentication must be revisited before shipping them.** With `snapshot` reading search results back, fix 1 matters more than when this todo was written.
3. **Steal fix.** Probe before unlink: `connect()` to the existing path. `ECONNREFUSED` or `ENOENT` means stale, which is safe to remove. Success means a live instance owns it, so log it and refuse to start the server or surface the conflict. Gate the unlink in `stop()` and the exit cleanup on still owning the file (compare `fstat` of the listener inode with `lstat` of the path), so a dying first instance does not clobber the second's socket. `tauri-plugin-single-instance` is in place (added for gadget install handoff). The probe and the ownership check remain a safeguard should two instances run anyway.

## Files

`control/mod.rs` (`ControlServer::start` binds and unlinks, `stop` and the accept loop's cleanup unlink, `socket_path`), `control/handlers/*.rs`, `control/snapshot.rs`, `lib.rs` (exit cleanup around lines 1001-1010), `Cargo.toml` (dependencies, including `tauri-plugin-single-instance`).
