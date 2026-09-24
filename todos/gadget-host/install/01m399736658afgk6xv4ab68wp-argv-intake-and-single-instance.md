---
kind: feature
status: open
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Accept gadget archives from the command line and from a second launch

Ships together with the macOS file association (decided 2026-09-24).
Plan step 17.

## Why

On Linux and Windows a file association starts the app with the path
in argv. If Torchsnap already runs, a second process starts and has to
hand its argv over and exit. macOS does neither for bundle launches:
LaunchServices keeps one instance and sends an Apple Event
(`RunEvent::Opened`). argv only reaches Torchsnap on macOS when
someone starts `Torchsnap.app/Contents/MacOS/<binary>` directly.

## Decisions

- **First instance:** at the end of `setup`, pass
  `std::env::args_os()` (`args()` panics on non-UTF-8) through
  `intake::paths_from_args` (skip the program name and anything
  starting with `-`, accept paths and `file://` URLs) and submit with
  origin `CommandLine`.
- **Later launches:** `tauri-plugin-single-instance` (2.4.3),
  registered first in `run()` as its docs require. The callback gets
  `(app, args, cwd)`; relative paths resolve against `cwd`.
- **Second launch without files shows the launcher.** A new
  show-only `show_launcher_window`, extracted from the show branch of
  `toggle_launcher_window`, so a visible launcher is not hidden.
- **Plain paths only**, no `torchsnap install <file>` subcommand.
  File managers pass plain paths.

## Findings

- On macOS the plugin hands args over through a Unix socket
  (`/tmp/<identifier>_si.sock`). It detects and exits the second
  process; it does not stop it from starting. As a side effect it
  stops a directly started second binary from running a full second
  instance that would load all gadgets again and fight over
  `<app_data_dir>/control.sock`.
- On Linux it uses DBus, which snap and flatpak block unless the
  package manifest declares the name.
- The crate has a `deep-link` feature for forwarding URLs on
  Windows/Linux. Relevant only when the parked URL scheme comes back.
- `tauri_plugin_autostart` is initialized with no extra args, so
  autostart launches carry nothing the parser must skip.
