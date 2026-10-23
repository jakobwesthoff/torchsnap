---
title: "Control socket has no peer check and startup unlinks an existing socket"
kind: bug
component: backend
status: needs-discussion
impact: low
tags: [security]
---
# Control socket has no peer check and startup unlinks an existing socket

The control server binds a Unix socket at `<app_data_dir>/control.sock`. It gives the socket mode 0600 inside a private 0700 directory before renaming it to that path, so the umask no longer decides who can connect. It has no peer-credential check and unlinks any file at the path before it opens the socket, so a second instance can take the socket over. Whether the socket moves to `$XDG_RUNTIME_DIR` is an open decision. Severity is low: the server is opt-in (`controlChannel.enabled`, default off), and its handler set has no execute capability. Since 2026-10-23 a peer can also read the launcher's state back (`snapshot`), which widens what a peer outside the user would get if it could connect.

## Problem

Read in the code. The socket mode before the explicit 0600 was checked empirically (`srwxr-xr-x` under umask 022), the rename and the mode on macOS (Python) and Linux (Perl in a Debian container, Linux 7.2, overlayfs, umask 000). The rest comes from the code and the platform documentation.

`ControlServer::start` in `src-tauri/src/control/mod.rs` opens the socket through `bind_restricted` after an unconditional `remove_file`, and checks no peer credentials:

```rust
let path = socket_path(app);
let _ = std::fs::remove_file(&path);
let listener = match bind_restricted(&path, restrict_to_owner) { ... };
```

`handle_connection` reads newline-delimited JSON-RPC and dispatches with no authentication.

## Impact

### Permissions and same-user determination

- **Socket mode.** `bind` creates the inode with `0777 & ~umask` (0755 under umask 022, verified empirically: `srwxr-xr-x`; XNU `unp_bind`, Linux `unix(7)`). `bind_restricted` binds it in `<app_data_dir>/.control-<pid>.tmp/`, created with mode 0700, sets 0600 by path there and then renames it to `control.sock`, so the socket is reachable at the path only with mode 0600. Connect finds a socket by its inode, which rename keeps (Linux `unix_find_bsd`, XNU `unp_connect`). Checked on Linux: after rename and removal of the private dir the socket is 0600, the owner connects, a connect as `nobody` gets `Permission denied`.
- **macOS ACLs.** chmod leaves extended ACL entries in place: an inheritable allow ACE on the data dir is inherited by the private dir and the socket, and still grants connect after the chmod (checked on macOS: a socket bound below a dir with `group:everyone allow write,file_inherit` shows `group:everyone inherited allow write,append` after chmod 0600). The 0700 `~/Library` stops other users before that, unless it carries such entries too. Linux is not affected: on a file with a POSIX ACL, chmod sets the mask entry from the group bits (`acl(5)`), so 0600 gives a `---` mask that voids named entries.
- **Connect semantics.** The "macOS ignores socket file permission bits" lore is outdated (4.2BSD-era). Modern XNU `unp_connect` runs `vnode_authorize(..., KAUTH_VNODE_WRITE_DATA, ...)`, so connect requires write permission on the socket inode, as on Linux (`unix(7)`). Both also require search (`x`) on every parent directory. POSIX leaves socket-mode enforcement unspecified, so portable hardening should not rely on the socket mode alone. Both target platforms enforce it today.
- **Parent directories.** macOS `~/Library` is 0700 by platform default (verified), so other local users are stopped at the directory walk regardless of the socket mode. Linux `~/.local/share/app.torchsnap` is 0755 (`create_dir_all` under umask 022). Whether `~` is 0700 or 0750 (Fedora, Ubuntu 21.04 and later, Debian 12 and later) or 0755 (older or other distros) depends on the distro and its history, so it is **not guaranteed**.

Verdict: same-user-only access rests on the socket's 0600 mode, which both kernels enforce on connect, whatever the umask, and on macOS also on the 0700 `~/Library`.

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

`tauri-plugin-single-instance` is now registered (first plugin on the builder in `lib.rs`): a second launch hands its arguments to the running instance and exits before its setup runs, so the steal below no longer happens through a normal second launch. Whether any other path still runs two instances is not established. The unconditional `remove_file` before the socket opens in `mod.rs` still exists, and the rename in `bind_restricted` replaces whatever is at the path, so if two instances do run, the steal is unchanged:

- Instance B replaces A's socket file with its own. A keeps serving already-connected clients on the unlinked inode, while all new clients silently reach B.
- In reverse, `stop()`, the accept-loop cleanup and the `RunEvent::Exit` cleanup in `lib.rs` all unlink the path unconditionally. If instance A later exits or toggles the setting off, it deletes **B's live socket file**, leaving B listening on an unreachable inode. That is a silent denial of service with no error on either side.

This is same-user only, since the path lies inside the user's own data dir, so it is an integrity and DoS issue for the control channel, not privilege escalation. It is one symptom of the broader missing single-instance story. Two instances also double-write `settings.json` and the SQLite databases, and race on global-shortcut registration.

A crash between creating the private dir and removing it leaves `.control-<pid>.tmp/` behind. The next start under the same pid removes it; one under another pid does not, so such leftovers can pile up.

### Severity: low

The server is opt-in and default-off, the handler set has no execute capability, and the same-user boundary holds through the socket's 0600 mode that both kernels enforce on connect, and on macOS also through the 0700 `~/Library`. Residual exposure: (a) a platform that would not enforce the socket mode, or an inheritable allow ACE on the data dir and `~/Library` on macOS, leaking UI control, a fetch beacon and read access to the launcher's search results to other local users; (b) same-user multi-instance silent takeover or DoS, if two instances still run despite the single-instance plugin.

## Open decision

Whether to move the socket to `$XDG_RUNTIME_DIR` on Linux: the canonical location for such a socket (0700 tmpfs, per session, auto-cleaned). Sockets in persistent data dirs are an anti-pattern. Moving the path changes the documented client contract (`docs/control-api.md`, the user docs, the `info` method's `socketPath`).

## Suggested fix (in order of value)

1. **Peer-credential same-uid check: not worth it as a security boundary, cheap as belt and braces.** A same-uid attacker already owns `settings.json`, the frecency SQLite database and the gadget code directory under `app_data_dir`, all far more powerful than this socket. (On macOS synthetic input is TCC-gated, so the socket is not strictly redundant with "can already send keystrokes". The data-dir write access is the stronger equivalence.) If added anyway, it is two syscalls (`SO_PEERCRED` on Linux, `getpeereid` or `LOCAL_PEERCRED` on macOS). It would also close the macOS ACL gap above. A token handshake is overkill for this handler set. **Record the trigger: if execute-class methods (run a result, open a path) are ever added to the control API, authentication must be revisited before shipping them.**
2. **Steal fix.** Probe before unlink: `connect()` to the existing path. `ECONNREFUSED` or `ENOENT` means stale, which is safe to remove. Success means a live instance owns it, so log it and refuse to start the server or surface the conflict. Gate the unlink in `stop()` and the exit cleanup on still owning the file (compare `fstat` of the listener inode with `lstat` of the path), so a dying first instance does not clobber the second's socket. The same stale handling can sweep `.control-*.tmp` leftovers of pids that no longer run. `tauri-plugin-single-instance` is in place (added for gadget install handoff). The probe and the ownership check remain a safeguard should two instances run anyway.

## Files

`control/mod.rs` (`ControlServer::start` unlinks and calls `bind_restricted`, `stop` and the accept loop's cleanup unlink, `socket_path`), `control/handlers/*.rs`, `control/snapshot.rs`, `lib.rs` (exit cleanup around lines 1478-1482), `Cargo.toml` (dependencies, including `tauri-plugin-single-instance`).
