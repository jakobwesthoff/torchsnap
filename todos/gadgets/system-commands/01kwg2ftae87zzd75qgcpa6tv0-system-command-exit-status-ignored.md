---
kind: bug
severity: low
status: open
area: [src-tauri/src/gadgets/system_commands/macos_commands/power.rs, src-tauri/src/gadgets/system_commands/macos_commands/utilities.rs]
tags: [unconfirmed, error-handling]
---

# System commands ignore child exit status — failures dismiss the launcher silently

## Problem

Three system commands spawn a subprocess with `.status()` and
propagate only the *spawn* error; a non-zero exit status is
discarded and the command reports success:

- Lock Screen (`power.rs:55-59`):
  ```rust
  Command::new("pmset")
      .arg("displaysleepnow")
      .status()
      .context("lock screen via pmset")?;
  ```
- Start Screen Saver (`utilities.rs:92-97`): `open -a ScreenSaverEngine`
- Eject Disc (`utilities.rs:123-127`): `drutil eject`

`.status()` returns `Ok(ExitStatus)` even when the tool exits
non-zero, so the `?` never fires for command-level failures. Each
implementation then returns `Ok(PostAction::Dismiss)`: the launcher
closes as if the command succeeded.

The AppleScript-based commands in the same module do this
correctly — `osascript::eval` checks `output.status.success()` and
bails with stderr (`src-tauri/src/platform/macos/osascript.rs:26-29`).

## Impact

When `pmset`/`open`/`drutil` fail (no optical drive, tool missing
from PATH, permission issues), the user gets no feedback: the
launcher dismisses and nothing happens. The error also is not
surfaced anywhere else (the gadget execute path would have shown a
returned `Err`).

## Suggested fix

Check the returned `ExitStatus` and bail on failure, mirroring
`osascript::eval`:

```rust
let status = Command::new("pmset").arg("displaysleepnow").status()
    .context("lock screen via pmset")?;
anyhow::ensure!(status.success(), "pmset exited with {status}");
```

Consider capturing stderr via `.output()` instead of `.status()`
for a useful error message (as `osascript::eval` does).
