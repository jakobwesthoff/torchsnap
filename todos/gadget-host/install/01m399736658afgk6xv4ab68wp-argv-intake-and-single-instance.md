---
kind: feature
status: blocked
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
depends-on: [todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md]
---

# Accept gadget archives from the command line and from a second launch

Ready once the intake exists. Can ship with the macOS file
association.

## Why

On Linux and Windows, a file association launches the app with the
file path in argv. If Torchsnap is already running, the OS starts a
second process, and that process has to hand its argv to the first
instance and exit. macOS does neither of these for bundle launches:
LaunchServices keeps one instance and sends an Apple Event
(`RunEvent::Opened`). argv only reaches Torchsnap on macOS when
someone runs `Torchsnap.app/Contents/MacOS/<binary>` directly.

This code is platform-independent, the Linux association cannot work
without it, and it is small once the intake
(`todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`)
exists. Building it together with the macOS work tests the intake
with two different origins from the start.

## What to build

- **First instance.** In `setup` (`src-tauri/src/lib.rs:648`), read
  `std::env::args().skip(1)`, skip anything starting with `-`, accept
  plain paths and `file://` URLs, and push `InstallRequest`s with
  origin `CommandLine`. Tauri's `examples/file-associations` does the
  same in its `setup`.
- **Second instance.** Add `tauri-plugin-single-instance`. Its
  callback receives `(app, args, cwd)`. Resolve relative paths
  against `cwd`, then push requests like above. With no file
  arguments, bring up the launcher or settings (decide which) so a
  second launch is not a silent no-op.
- **Plugin order.** The plugin docs say single-instance must be
  registered first, before the other plugins in `run()`
  (`lib.rs:586-625`).

## Things found during research

- **Linux uses DBus** for single-instance detection. Per the plugin
  docs it does not work in snap or flatpak without declaring the DBus
  name in the package manifest.
- **The plugin runs on macOS too.** Today nothing stops a second
  process when the binary is started directly. That second process
  would load all gadgets again and fight over the control socket at
  `<app_data_dir>/control.sock` (cleanup at `lib.rs:1011-1013`).
  Single-instance fixes that as a side effect.
- **Deep links.** The URL scheme todo will route `torchsnap://` URLs
  through the same argv path on Linux. The deep-link plugin docs say
  it works together with single-instance. I remember a `deep-link`
  feature flag on the single-instance crate that forwards URLs, but
  have not verified it.
- **Autostart.** `tauri_plugin_autostart` is initialized with `None`
  as its extra args (`lib.rs:622-625`), so autostart launches carry no
  arguments the parser needs to ignore. Recheck this if autostart
  args are ever added.

## Open points

- Accept only `*.torchsnap`, or pass everything to the intake and let
  it reject with a log line? The intake normalization step already
  filters, so the adapter can stay dumb.
- Is a `torchsnap install <file>` style subcommand wanted, or are
  plain paths enough? Plain paths match what file managers send.

## Done when

- `Torchsnap <path>.torchsnap` on a cold start and on a running
  instance both end in the confirm step.
- Relative paths from a second instance resolve against its cwd.
- Launching the app a second time does not start a second full
  instance.
