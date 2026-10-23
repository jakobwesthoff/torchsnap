# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `just coverage`, `just coverage-crates` and `just coverage-frontend`
  write test coverage reports for the Rust crate and the frontend to
  `target/coverage/`.
- Control API: `status` reports `focused`, whether the launcher window
  has keyboard focus; `ready`, true once the launcher has sent its
  layout and its frontend is connected, so `show`, `query` and `dismiss`
  work; and `targetMonitor`, the monitor Torchsnap last placed the
  launcher on.
- Control API: `windows` lists Torchsnap's open windows with their
  label, title, visibility and focus.
- Control API: `info` reports the process id, version, git hash, data
  folder, socket path, the tray icon's id and, on Linux, the display
  Torchsnap is connected to and its backend (Wayland or X11).
- Control API: `snapshot` reports what the launcher shows: the query,
  whether its search is still running, the results, the selected
  result, the active gadget view, whether the content is visible and
  whether the search input is the active element. It answers with error
  `-4` when the launcher does not reply within two seconds or reloads
  before replying.
- `just lint-shell` runs shellcheck on every shell script in the
  repository, found by file extension and shebang. `just lint`, and so
  `just fullcycle`, runs it. Shell recipes inside the justfiles are not
  checked.
- `just linux-build [--release]` builds Torchsnap for Linux arm64
  (Debian 13) in a Docker container on an Apple silicon Mac, without
  packaging; the binary and its bundled gadgets land in
  `target/linux-build/`, and a debug build also holds the archives of
  the gadgets `gadgets/bundled.toml` does not list, for the debug app's
  development gadget folder. `just linux-build-lint [path]` runs
  `cargo check` and clippy for Linux on any checkout in the same
  container. `just linux-build-status` shows what its image, build cache
  and volumes take on disk, `just linux-build-clean [--all]` removes
  them, and each build and lint keeps them within a budget.
  `just linux-build-check` copies a build to a Linux machine and checks
  its glibc version and libraries there; `just linux-build-run-check`
  starts it there in a session of its own and asks its status over the
  control socket. See `docs/Howto-build-linux-on-macos.md`.
- A Linux testbed, driven from the Mac: `just testbed-sync` mirrors the
  working tree to a Raspberry Pi named by `TORCHSNAP_LINUX_HOST`,
  `just testbed-provision` installs the build tools, ydotool and GNOME
  with automatic login and RDP access there, `just testbed-reboot` and
  `just testbed-exec` reboot it and run commands on it,
  `just testbed-build` builds a debug app in the Mac's Linux container
  and copies it there (`--on-machine`: builds it there),
  `just testbed-app-start`, `testbed-app-stop` and `testbed-app-log` run
  it in the GNOME session, `just testbed-control` drives it through its
  control socket, `just testbed-screenshot`, `testbed-launcher-state`,
  `testbed-windows` and `testbed-eval` observe it, `just testbed-type`,
  `testbed-key`, `testbed-pointer` and `testbed-click` give it keyboard
  and pointer input, `just testbed-clipboard-set` and
  `testbed-clipboard-get` set and read the clipboard, `just testbed-tray`,
  `testbed-tray-icon` and `testbed-tray-click` switch GNOME's tray
  support and use the app's tray icon, `just testbed-record-start` and
  `testbed-record-stop` record the screen, `just testbed-headless-start`
  and `testbed-headless-stop` run a headless GNOME session with virtual
  monitors for the app, `just testbed-stop-all` stops everything the
  testbed started, and
  `just testbed-selftest` checks the testbed. Provisioning lowers the
  Pi's security for remote control, for example with an unencrypted
  default keyring; `docs/Howto-test-on-a-raspberry-pi.md` lists what it
  opens up.
- `tools/memsnap` measures on Linux: PSS and RSS from `/proc` for
  Torchsnap and every process below it, with each process's CPU time,
  its DRM clients' GPU memory and its dma-buf count and size, and the
  system's free memory. `--also PID` records another process apart from
  Torchsnap's, such as the compositor, and `memsnap pids` lists the
  measured processes. `tools/memsnap-report` reports such files in PSS.
- `tools/bench-memory` runs on Linux: `--app PATH` names the binary to
  start (no build), in a systemd user scope when `systemd-run` exists;
  it waits until the launcher is ready, runs the same scenario, and
  writes an events file with each control request's time, the app's
  `info` and its `WEBKIT_*` and `GDK_*` variables; `--env-prefix PREFIX`
  (repeatable) records the variables with that prefix as well.
  `--compositor-pid` records the compositor apart, `--cycles N` adds N
  measured show/dismiss cycles, `--timing-cycles N` adds N unmeasured
  ones that time `show` until the content is visible
  (`tools/bench-memory-cycles`). `--output-dir` and `--log` work on both
  platforms. `just testbed-stop-all` also stops an app a benchmark left
  running.
- `just testbed-bench-memory DESCRIPTION [--cycles N] [--timing-cycles N] [--env-prefix PREFIX] [--headless]`
  runs `tools/bench-memory` on the Linux testbed against its current
  build, with GNOME Shell recorded apart, and copies the data files to
  `target/linux-testbed/`.

### Changed

- `just doctor` lists shellcheck as a required tool.
- Control API: `query` and `dismiss` answer with error `-1` when the
  launcher's frontend is not connected, instead of reporting success
  for a command it never received. `dismiss` then leaves the launcher
  as it is; `hide` still hides it.
- On macOS, `just build` without `--release` and `--sign` makes only
  `Torchsnap.app`, no DMG. No recipe reads the DMG of such a build.
  Making it took about half a minute, and its Finder step could hang.
  A `just build` with nothing changed went from about 50 s to 15 s.
  Release builds and `--sign` builds still make the DMG.
- The host library is built only as an `rlib`, no longer also as a
  static and a dynamic library. On an Apple silicon Mac, `cargo build`
  after a one-line change went from about 2.95 s to 2.31 s and
  `cargo test --no-run` from about 3.04 s to 2.54 s.
- The launcher window is 64 logical pixels shorter. The card's shadow
  above the card fits in the space kept for the mascot, so no separate
  pad sits above it. Where the window manager places the window
  itself, as GNOME does, the card sits up to 64 px higher.
- The launcher window is placed in the monitor's work area, the part
  that the menu bar, Dock, task bar and panels leave free. The card
  sits a quarter of the way down that area, and the window's top stays
  inside it. Wayland desktops place the window themselves and are not
  affected.

### Removed

- The devcontainer for running Claude Code in isolation, with
  `just devcontainer` and `just devcontainer-reset-caches`. The Docker
  volumes it created (`torchsnap-*`: shell history, Claude login, cargo
  and bun caches) stay on a machine until removed by hand.

### Fixed

- The `template` gadget declares the `opener` permission its code needs.
  Before, the host refused to enable it with "imports interfaces without
  matching permissions: [opener]", and so refused every gadget copied
  from it.
- `tools/memsnap sample` without `-d` runs under the macOS system bash
  (3.2) instead of stopping with "duration_args[@]: unbound variable".
- Switching between `just build` and plain `cargo` or `just start` no
  longer recompiles the 64 crates that build C code. `.cargo/config.toml`
  now gives cargo the same `MACOSX_DEPLOYMENT_TARGET` as `tauri build`.
  On an Apple silicon Mac, `cargo build` after `just build` went from
  about 25 s to 3 s, and the Tauri build after `cargo build` from about
  29 s to 9 s.
- A `just build` in which neither the frontend nor a bundled gadget
  changed no longer recompiles the app to embed them again; on an Apple
  silicon Mac a debug build went from about 15 s to 9 s. `vite build`
  keeps the timestamps of files in `dist/` whose content did not change,
  and staging keeps gadget archives whose content did not change.
- Control API: the socket file has mode `0600` from the moment it
  appears. Its mode came from the umask, so on a Linux system with a
  permissive umask and a home folder other users can enter, they could
  connect to it.
- Replacing a query that has a gadget prefix with text that does not
  start with that prefix, by typing over the input or with the control
  API's `query`, no longer puts the old prefix back in the input. After
  `=2+3`, a `query` with `!b2` showed `=b2`, typing `a` over the input
  showed `=`, and deleting the leading `=` showed `=+3`.
- `tools/memsnap-report`: a phase cell holds every process of its name;
  with several WebKit processes of one name it counted only the first.
  The TOTAL row of the samples table is now the min, max, average, first
  and last of the per-sample totals; it summed each process's own
  values, so a process that started during the run was missing from the
  total's change. Reports of runs with either case differ from earlier
  ones in `memory-benchmark.log`.

## [0.15.0] - 2026-10-21

### Changed

- Command grants now bound what a gadget can do to what the install
  review shows. A rule names its program by absolute path or by a bare
  name; a relative path such as `bin/tool` is rejected. A gadget can
  pass a working directory only inside the `cwd` its rule declares;
  without one, the program runs in an empty folder that exists only for
  that run. Programs get locale settings, `TZ`, a fixed `PATH`
  (`/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin`) and
  `HOME` and `TMPDIR` pointing at empty folders of their own, instead of
  Torchsnap's environment; a program outside that `PATH` needs an
  absolute path in the rule. A gadget can set only the environment
  variables its rule declares in `[permissions.command.env]`, never
  `PATH`, `HOME`, `TMPDIR`, `LD_*`, `DYLD_*` or the interpreter and tool
  variables that make a program run other code. Folders, paths and fixed
  values in a rule cannot point into Torchsnap's data folder, which holds
  every gadget's data, and a program cannot run in or be handed a folder
  that lies in or contains it; the install review refuses a gadget whose
  rules do. The install review shows each rule's folder and variables,
  says that programs run outside the gadget's sandbox with your rights,
  and marks rules that accept any argument or any variable value.

### Fixed

- A gadget's SQL can no longer attach other SQLite databases or write
  copies of its database to other places with `VACUUM INTO`, nor change
  SQLite settings that apply to the whole app (`temp_store_directory`,
  `data_store_directory`, `soft_heap_limit`, `hard_heap_limit`), nor
  write SQLite's internal schema or index tables directly; it reaches
  only the gadget's own database. Plain `VACUUM` is no longer available
  to gadgets.
- When "Install and Restart" fails while downloading the update, the
  update window says that Torchsnap could not download the update from
  github.com. Before, it said Torchsnap could not get update information
  from torchsnap.app.
- A clipboard history entry no longer shows, or is found in search by,
  text from a copy it does not contain. Copying twice in quick
  succession could cause this.
- Clipboard history no longer keeps entries it cannot paste or entries
  without content. Pasting an entry while old entries were being
  removed, a failed write while storing a copy, or a failure while
  deleting or clearing could leave such entries, or leave stored copies
  on disk that nothing removed. Leftover files are removed when the
  clipboard gadget starts.
- When a gadget fails to start, Settings say so under its switch, with
  the reason, and the switch stays on. A Retry button starts the gadget
  again. Before, the gadget looked enabled but did nothing, and a gadget
  that failed while Torchsnap started kept its global shortcut.
- Gadget settings pages get the styles their gadget ships for them.
  Before, those styles applied to the gadget's row under Installed
  Gadgets instead.

## [0.14.1] - 2026-10-20

### Fixed

- On Macs with displays of different scale factors, such as a Retina
  MacBook next to a non-Retina display, the launcher and the settings,
  developer tools, update and welcome windows open on the screen with
  the mouse pointer, and those windows open centered on it. On setups
  with displays of different heights, the launcher opens a quarter of
  the way down the screen with the mouse pointer, whichever screen holds
  the window with keyboard focus.

## [0.14.0] - 2026-10-03

### Added

- Torchsnap installs with Homebrew:
  `brew install --cask jakobwesthoff/tap/torchsnap`.
- When an action in the launcher fails, the launcher stays open and its
  footer shows what failed and why, for example `Eject Disc failed: …`,
  until the next key press or action. Before, nothing showed. This also
  covers:
  - Lock Screen, Start Screen Saver and Eject Disc when the tool behind
    them exits with an error. Before, the launcher closed as if the
    command had worked.
  - Copying a calculator result, and copying or removing a clipboard
    history entry. Before, the calculator only logged the failure and
    the clipboard showed nothing.
- Gadget views can report and react to failed actions:
  - `useLauncher().showError(message)` shows a failure of the view's own
    action, such as a `sendMessage` call, in the launcher footer.
  - `useLauncher().onExecute()` returns a promise that rejects with the
    error message when the action fails. The launcher already shows the
    error, so a view may ignore the promise. In tests,
    `MockGadgetContextProvider` still accepts an `onExecute` override
    that returns nothing.

### Changed

- New artwork:
  - The app icon shows Snappy's face rising under a night sky with an
    aurora. On macOS 26 it follows the dark, clear and tinted icon
    styles.
  - The DMG window shows a winter night with an aurora over a snowfield,
    with Torchsnap and the Applications folder on the snow and a
    campfire between them. The window is a little larger.
  - The General page of the settings shows Snappy's emblem above the
    build version. Snappy peeks up a little on hover, and clicking opens
    torchsnap.app in the browser.
- Snappy's costumes:
  - Snappy comes in 626 costumes instead of 189. A newer image model
    redrew most of the earlier ones close to their old look. Every
    mascot has a reviewed alt text, and the owls inside the costumes are
    drawn at about the same size.
  - The costumes are sorted into new themed groups. Mascots that only
    relate to a holiday, such as the cartoon rabbits at Easter, now show
    all year and also turn up more often on the holiday. The mascots
    that are the holiday itself, such as pumpkins, Santa and Easter
    bunnies, still show only then.
  - The costume is picked per character: a character with several
    versions, such as an SFW and an NSFW one, shows as often as one with
    a single version. The launcher holds back the characters it showed
    recently, so a costume seldom comes twice in a row. While a holiday
    or a full-moon night lasts, its costumes take a fixed share of the
    launches: 40% at Christmas, Halloween and Easter, 90% on New Year's
    Eve and on full-moon nights.
  - Each mascot stands on the launcher card at a point chosen for it,
    centred on the card in centre mode. As sidekick it keeps a fixed
    distance from the card's right edge without reaching past it, and
    the Escape hint sits centred below it.

### Fixed

- A search match on an emoji, a flag or an accented letter built from a
  combining mark highlights the whole character. Before, the launcher
  showed `�` in its place.
- Typing or pasting an email address such as `jane.doe@gmail.com` no
  longer offers "Open https://jane.doe@gmail.com" as a result.
- Opening an entry's settings, or an action that runs nothing because
  its entry or slot is gone, no longer counts as using the entry, so it
  no longer raises the entry's ranking.
- The "Launch at login" switch in Settings moves back when turning
  launching at login on or off fails, and stays usable when its state
  cannot be read. Before, it showed the requested state anyway, or
  stayed disabled for good.
- When the Settings, DevTools or Welcome window fails to load its
  settings or gadget list, it opens and shows the error with a button to
  reload. Before, the window stayed invisible, and opening it again
  showed it blank.
- The green window button of Torchsnap's own windows goes fullscreen
  again after the window lost focus while Option was held. Before, it
  kept zooming until Option was pressed in the window again.
- In the devtools console:
  - Ctrl+K clears and Ctrl+F focuses the filter on Linux and Windows,
    where they needed the Super key before. On every platform the two
    shortcuts do nothing while Shift or Alt is also held, and they work
    with Caps Lock on.
  - The tree view shows the end of a span whose start has left the log
    buffer, with its duration, as a top-level row. Before, it hid that
    row while the flat view showed it.
- The Control API reads at most 1 MiB per request line. A longer line
  gets a `-32600` error and the connection closes. Before, a client that
  never sent a newline made Torchsnap buffer without limit.
- A gadget manifest fails to load, with an error naming the offending
  entry, when:
  - a `[permissions.filesystem]` `read` pattern is a relative path.
    Patterns start with `/`, a substitution variable or a Windows drive
    letter. Before, the pattern never matched.
  - a `[[permissions.command]]` rule has a `glob` pattern that does not
    compile, or `timeout-ms-max`, `max-output-bytes` or
    `max-stdin-bytes` set to 0. Before, a bad glob failed only when the
    gadget was enabled.
  - a `[permissions.opener]` scheme is no URL scheme, such as `""` or
    `"https://"`. Before, it never matched a URL.
  - a `[permissions.http]` origin has no host, such as `file:///etc`, or
    has a path, query or fragment, such as `https://api.example.com/v1`.
    Before, the first became the meaningless origin `null` and the
    second granted the whole host.
  - `permissions.sql-storage = true` and the `[storage.sql]` table do
    not appear together. Before, such a gadget loaded without a
    database.
  - `manifest.toml` in a gadget directory is a symlink to a file outside
    the directory. Before, the host read the outside file as the
    manifest.
- Gadget frontend files whose names contain spaces, `%` or non-ASCII
  characters load. Before, the request failed with "file not found".
  A request that hits an internal error answers with status 500 instead
  of never finishing.
- Running an entry or sending a message to a gadget that was turned off
  in the meantime fails with "gadget is disabled". Before, the call
  reached the turned-off gadget and failed with whatever error its
  torn-down state produced.

## [0.13.0] - 2026-09-25

### Added

- Rust gadgets can implement `Messaging` from the gadget SDK instead of
  `MessagingGuest`: one enum lists every method the frontend calls,
  and the SDK decodes each call into it and rejects unknown methods
  and mismatched payloads before gadget code runs. The JSON helpers
  `parse_payload` and `to_response` are no longer in the prelude.
  Import them from `torchsnap_gadget_sdk::messaging`.
- Gadget views get `openSettings()` from `useLauncher()`. It opens the
  Settings window on the gadget's own section and closes the launcher,
  as returning `open-settings` from `execute()` does.
- Settings explains a global shortcut that does not work below its
  row: no shortcut set, not a valid shortcut, already used by another
  Torchsnap shortcut, or refused by the system because another app
  may hold it.

### Fixed

- Enter on a calculator result, inline or after `=`, and a click on a
  calculator history row copy the result and close the launcher.
  Before, nothing was copied and the launcher stayed open.
- The gadget SDK's test helpers `setupSdkGlobalsForTesting` and
  `MockGadgetContextProvider` load. They imported the host sources from
  a wrong path. `MockGadgetContextProvider` also provides the
  keybinding context, so components that register key bindings render
  in tests.
- While typing fast, a slow gadget's results for an earlier keystroke
  no longer replace the results of the current one in the host. Before,
  an action could run with the older entry, or do nothing because the
  entry was gone.
- The gadget interface docs name the filesystem permission section
  `[permissions.filesystem]`, the name the manifest parser accepts.
  They said `[permissions.fs]`, which grants nothing.
- Gadget views opened in the launcher see whether their gadget is
  enabled. Before, they got the flag of whatever the launcher showed
  earlier, usually no value at all, which reads as disabled.
- One global shortcut that cannot be registered no longer takes the
  others with it. Before, an invalid launcher shortcut, two shortcuts
  on the same keys, or a combination another app holds left no
  shortcut working at all, and a missing launcher shortcut stopped
  shortcut updates until restart. Each shortcut now registers on its
  own; on a collision the launcher shortcut wins, then the gadget
  registered first.
- A gadget command permission with a `path-under` constraint no longer
  accepts a path that goes through a symlink whose target does not
  exist. Writing to such a path would have created the target outside
  the allowed folder.
- A Clipboard Manager retention of 0 days, or a value too large to
  store, no longer deletes the whole clipboard history. Retention is
  at least one day.
- A gadget whose `manifest.toml` names a view or settings component
  that its bundle does not export shows an error card naming the
  missing export instead of "Loading…" forever.
- Recording a shortcut no longer accepts a single key or Shift plus a
  key, which took that key away from every other app. A shortcut needs
  Cmd, Ctrl or Alt, or is an F-key on its own. While recording, a hint
  says so, and a combination that does not qualify keeps the recorder
  listening instead of ending it.
- Letter shortcuts inside the launcher, such as Ctrl+D and Ctrl+U for
  scrolling, keep working while CapsLock is on. Shortcuts with Shift
  and a letter work at all now, including the Cmd+Shift+R (Reveal) and
  Cmd+Shift+O (Open With) hints shown in the launcher footer.
- On macOS, a recorded shortcut keeps Ctrl and Cmd apart. Before,
  pressing Ctrl+Space saved Cmd+Space, and Cmd+Ctrl+K saved Cmd+K.
- The emoji picker finds ©, ® and the keycap emoji (#, * and 0 to 9),
  which never showed up before.
- "Open settings" on a gadget's result, such as ZeroTier's "token not
  configured" entry, opens the Settings window on that gadget's page.
  Before, it only closed the launcher.
- A gadget whose launcher view, inline result or settings panel fails
  to render shows an error card naming the gadget instead of blanking
  the whole window. The launcher card offers "Go back". Any other
  render error in a window shows the error and a "Reload window"
  button.
- A gadget whose WASM binary does not compile no longer stops
  Torchsnap from starting. It is skipped, the error is logged, and the
  other gadgets load.
- The Clipboard Manager no longer panics when its database cannot be
  opened. The gadget is disabled instead. Turning the gadget off closes
  its database. A clipboard history opened before the gadget has
  finished starting gets an error instead of a panic.

### Changed

- Shortcuts show their modifiers in one order on every platform: Ctrl,
  Alt, Shift, then Cmd (⌃⌥⇧⌘ on macOS). On Windows and Linux the
  recorder also accepts the Windows or Super key, shown as "Win" or
  "Super".
- The Clipboard Manager's settings show statistics and offer "Clear
  All" only while the gadget is on, and "Clear All" stays unavailable
  while the history is empty.
- Gadget API 0.2.0: the WIT package `torchsnap:gadget`, the Rust SDK
  and the TypeScript SDK move to version 0.2.0 together. An entry's
  actions sit in fixed slots, and the slot decides the key: `primary`
  (Enter or a click), `secondary` (Cmd+Enter), `copy` (Cmd+C),
  `reveal` (Cmd+Shift+R), `delete` (Cmd+Backspace) and `open-settings`
  (Cmd+,). Each action carries a command that the host hands back
  unchanged to `execute(command)`. Rust gadgets implement `Search`
  with their own `Command` type, and gadget views run an entry's
  action with `onExecute(entryId, slot)`. Gadgets built against 0.1.0
  need changes and a rebuild.
- Gadget API: to open its settings, a gadget returns the new
  `open-settings` post-action from `execute()`
  (`PostAction::OpenSettings` in the Rust SDK).
- Settings → General lists Updates before Advanced, and every settings
  row keeps a gap between its text and its switch.

### Removed

- Gadget API: the `custom` and `open-with` action ids, and the entry
  `data` field. A command on each action replaces `data`, and the
  Rust SDK's `data` module is no longer public.

## [0.12.0] - 2026-09-24

### Added

- Torchsnap updates itself. "Check for Updates..." in the menu-bar
  menu, or the button in Settings → General, opens a window with the
  notes of every version since yours. "Install and Restart" downloads
  the update, checks that it is signed by the Torchsnap project,
  replaces the app and restarts it, showing the launcher once it is
  back. "Later" keeps the update in the menu-bar menu, "Skip This
  Version" stops automatic checks from offering it again. Coming from
  0.11.1 or earlier, install this version by hand once; later versions
  arrive through the update window.
- Automatic update checks: once a day, Torchsnap asks torchsnap.app
  for the newest version. They run only after you agreed in the
  welcome window or turned them on in Settings → General. The request
  carries your IP address and nothing else about you.
- A welcome window introduces Torchsnap on the first start after
  installing or updating to this version and sets up the shortcut,
  launch at login and whether to check for updates automatically. It
  finishes when you press the shortcut, and opens again from Settings →
  General → "Show Welcome".

### Fixed

- Opening Torchsnap again while it was still starting showed the
  launcher and hid it again at once. The launcher now stays open.

## [0.11.1] - 2026-09-24

### Fixed

- One non-family-friendly Snappy costume was shown stretched wide and
  misplaced above the launcher, because its image was not square. The
  asset build now rejects mascot images that are not 1024×1024.
- Snappy appeared larger in most costumes added in 0.11.0 than in the
  others. Those costumes are scaled down to the usual size.

## [0.11.0] - 2026-09-24

### Added

- `.torchsnap` gadget files open in Torchsnap from Finder (double-click
  or "Open With"), and Finder shows them with their own document icon.
  Paths passed on the command line open them too, also when Torchsnap
  is already running.
- Installing another version of an installed gadget updates it and
  keeps its data and settings. Installing an older version shows a
  warning first. A version that could not open the installed version's
  stored data (fewer storage migrations) is refused with instructions.
- Each gadget in Settings → Gadgets shows its version and a permission
  line naming what it touches (programs, network, files, links,
  clipboard, its own data), which expands into the details. Groups that
  reach beyond the gadget's own data are labelled "Broad access", and
  paths that only apply to another operating system are set aside.
- Settings → Gadgets lists installs, updates and uninstalls that wait
  for a restart, each with an Undo, and a bar with the number of
  pending changes and "Restart now". Uninstalls can be undone as well
  until Torchsnap restarts. The list keeps showing pending changes
  after Settings is closed and reopened, and after "Restart now"
  Torchsnap opens Settings → Gadgets again.
- Starting Torchsnap a second time shows the launcher of the running
  instance instead of starting another one.
- Nine new Snappy costumes in the regular rotation, and five
  Christmas-movie costumes that join Santa during the Christmas window.

### Changed

- Every gadget install shows a review first: the gadget, where the file
  came from, and every permission it asks for, grouped the same way as
  in the permission line. For an update, the review marks permissions
  that are new or removed. Nothing is installed until you confirm, and
  several files are reviewed one after another.
- Gadget archives larger than 16 MiB are rejected at install.
- Gadgets that ship with the app are labelled "Bundled" instead of
  "System" in Settings → Gadgets.

### Fixed

- Uninstalling a running gadget no longer leaves its data behind, lets
  the still-running gadget recreate it, or claims success while its
  settings reappear on the next start. Its data and settings are
  removed on the next start, before any gadget loads.
- A gadget can be uninstalled and installed again, or installed and
  removed again, without restarting Torchsnap in between.
- Dropping gadget files after switching settings sections no longer
  installs them twice, and a failed install among several dropped
  files is no longer hidden behind the next file.
- The ZeroTier gadget's warning results (token missing, token rejected,
  service not running) appeared under every search. They now appear
  only when the query starts with `zerotier` (or at least `zer`), is a
  network ID, or matches a remembered network.
- The ZeroTier gadget kept reporting the ZeroTier service as not running
  after it started, until the gadget was re-enabled. It now checks again
  on the next ZeroTier search, at most every 5 seconds.
- Pressing Enter on the ZeroTier "daemon not running" result showed an
  error. The result is now informational and has no action.

## [0.10.0] - 2026-09-23

### Added

- `just build --sign` code-signs the macOS bundle with the hardened
  runtime and the entitlements gadget code needs to run under it: ad-hoc
  by default, or with the identity in `APPLE_SIGNING_IDENTITY`. The README
  describes ad-hoc signing and the variables for Developer ID signing and
  notarization.
- With Developer ID signing and notarization credentials set,
  `just build --sign` also notarizes the DMG and staples its ticket, so
  Gatekeeper accepts the DMG as well as the app inside it.
- `just release-build <version>` and `just release-publish <version>`
  build, notarize and publish a release from a Mac; the README section
  "Releasing" describes the setup and the steps.
- A CI workflow runs `just fullcycle` on pushes to `main` and on pull
  requests.

### Changed

- The app and the DMG are signed with a Developer ID and notarized by
  Apple. After a download, macOS asks once whether to open the app;
  removing the quarantine flag by hand is no longer needed.
- The app bundle is named `Torchsnap.app` and the DMG
  `Torchsnap_<version>_<arch>.dmg`, following the product name. The
  bundle identifier `app.torchsnap` stays, so settings and installed
  gadgets carry over.

### Removed

- The ZeroTier gadget is no longer bundled with the app. Its source stays
  under `gadgets/zerotier/`, and `just build-gadgets` still builds it.

## [0.9.3] - 2026-09-21

### Fixed

- `just install` failed on a fresh checkout: it generated the app icons
  before installing the JS packages, so the icon step fetched Tauri's legacy
  pre-1.0 `tauri` package from npm instead of using the project's Tauri CLI.

## [0.9.2] - 2026-09-21

### Added

- The README describes how to set up a fresh checkout.
- `just audit` checks the host and gadget Cargo lockfiles with
  `cargo audit` and every bun project with `bun audit`. `just doctor`
  checks for `cargo-audit`.

### Changed

- `tauri-nspanel` comes from its crates.io release 2.1.0 instead of the
  `v2.1` git branch.
- React and React DOM are updated to 19.3. Gadget frontends render with
  the host's React, so installed gadgets run on 19.3 as well.
- Gadgets run on wasmtime 49, which accepts the wide-arithmetic
  proposal's 128-bit integer instructions in gadget code.
- The gadget SDK and the test fixtures generate their bindings with
  wit-bindgen 0.62. The rebuilt fixtures, compiled with Rust 1.98, import
  WASI 0.2.9.
- The gadget crates' dependencies are updated within their ranges,
  including a newer public suffix list for the Open URL gadget's domain
  detection.
- Frontend dependencies are updated within their ranges, among them
  Vite 8.3, ESLint 10.11, typescript-eslint 8.70 and tailwind-merge 3.7.
- The devcontainer uses the Rust 1.98.1 image to match the toolchain pin,
  git-delta 0.19.2 and zsh-in-docker 1.2.1.
- Development uses Bun 1.4; `@types/bun` is updated to 1.4.2 to match,
  and `just doctor` fails on an older Bun.
- `rust-toolchain.toml` pins Rust 1.98.1 with the `wasm32-wasip2` target,
  `clippy` and `rustfmt`. rustup installs it on the first `cargo` run in
  the repository.
- `just stage-bundled-gadgets` empties `target/bundled-gadgets/` before
  staging, and the app bundle includes every file in that directory.
- `just install` also runs `bun install` in every gadget frontend, which
  `just check-gadgets`, `lint-gadgets` and `test-gadgets` need on a fresh
  checkout.
- `just doctor` checks for `rustup`, `jq` and `curl`, which the toolchain
  pin and the asset pipeline need, and for `uv` (optional). It no longer
  checks for `oxipng`, which nothing uses.

### Security

- Tauri 2.11.6 scopes IPC channel data to the webview that created it
  (GHSA-w28w-mhc8-qvjv). The Tauri plugins move to their matching patch
  releases on both the Rust and the JavaScript side.
- The gadget runtime and the network stack no longer carry known
  vulnerabilities: wasmtime (RUSTSEC-2026-0268, RUSTSEC-2026-0269, fixed
  from 47.0.4, shipped as 49), rustls 0.23.45 (RUSTSEC-2026-0285) and h2
  0.4.19 (RUSTSEC-2026-0258).
- The development tooling no longer pulls vulnerable `@babel/core`,
  `@humanfs/node`, `baseline-browser-mapping` and `browserslist`
  releases.

### Fixed

- `cargo check`, clippy and the host tests no longer fail when no gadget
  has been staged into `target/bundled-gadgets/`. A release build with
  nothing staged prints a warning instead.
- The README documented `just build` as a release build. It builds debug;
  `just build --release` builds release.

## [0.9.1] - 2026-09-21

### Added

- The menu bar menu has an **Open Launcher** entry at the top.

### Changed

- Toggling **Random mascots** or **Show NSFW mascots** now takes effect
  immediately in both directions. Turning either setting back on
  previously left the current mascot in place until the launcher was
  next dismissed.
- Switching directly from one gadget view to another no longer shows the
  previous view's footer hints until the new view publishes its own.
- `just lint-crates` and `just lint-gadgets` now fail on any clippy
  warning.
- Clipboard history search results are now ordered most recently copied
  first. Typing a search term narrows the list without reordering it;
  previously a filtered list was ordered by relevance and an unfiltered
  one by capture time, so typing reshuffled the results.

### Fixed

- Clipboard history search no longer comes up empty for terms
  containing punctuation. Searching `claude --resume`, `example.com`, a
  path, or a UUID fragment reported no matches, because the search text
  was passed to the full-text index as query syntax rather than as
  words. Since the search runs as you type, a single `-` was enough to
  blank the results.
- A clipboard history search that fails now says so, instead of
  rendering an empty list that is indistinguishable from having no
  matches.

## [0.9.0] - 2026-08-10

First public release. Torchsnap is a keyboard-driven desktop launcher for
macOS, built on Tauri 2 with a Rust host and a React frontend. Its
functionality comes from **gadgets**: sandboxed WebAssembly components that
the host loads at runtime and reaches only through a manifest-declared
permission model. Gadgets ship as single-file `.torchsnap` archives that can
be handed around and installed without a store or an account.

This release is feature-complete for everyday use and stable enough for
daily driving. It is numbered 0.9.0 rather than 1.0.0 because the gadget
distribution story is not finished: archives are unsigned, installing one
requires an app restart, and the app itself is neither code-signed nor
self-updating. See [Known limitations](#known-limitations).

### Added

#### Launcher

- Summon the launcher with a user-configurable global shortcut, or by
  clicking the menu bar tray icon. The app runs as a macOS accessory with no
  Dock icon.
- The launcher is a non-activating panel: the app you were working in keeps
  focus while you type, and the panel floats above other windows and
  fullscreen apps across every Space.
- The launcher opens on the display under the mouse cursor, centered
  horizontally at roughly a quarter of the screen height.
- Results stream in as each source produces them, so fast gadgets render
  immediately instead of waiting for slow ones. The visible list is a
  deterministic merge ordered by score, then source, then id.
- Fuzzy-matched portions of a result's title and subtitle are highlighted.
- Keyboard navigation with arrow keys, `PageUp`/`PageDown`, `Enter` to run
  the primary action, and `Escape` to clear the query and then dismiss. The
  search field additionally accepts Emacs-style editing keys (`Ctrl+W`,
  `Ctrl+U`, `Ctrl+K`, `Ctrl+A`, `Ctrl+E`).
- Each result carries an ordered list of actions. The first is primary; the
  rest are reachable through their own key bindings, shown in a footer hint
  bar that follows the selection.
- Frecency ranking: entries you pick often and recently rise to the top.
  Tracking can be disabled, inspected, and cleared from Settings.
- Gadgets registering a query prefix take over the result area exclusively
  while that prefix is active, and receive the query with the prefix
  stripped.
- Gadgets can render their own UI in the result area, either as a full view
  replacing the list or as an inline view above it.
- Result entries display HeroIcons by name, embedded images, emoji, files on
  disk, or the real icon of an installed application.
- **Snappy**, an optional mascot beside or above the launcher, with a large
  set of themed variants and weighted random selection influenced by the
  date, holidays, and the lunar phase. Placement, re-rolling on each open,
  and inclusion of non-family-friendly variants are all configurable.

#### Bundled gadgets

- **Calculator** — evaluates expressions behind the `=` prefix, and also
  detects math-looking input without a prefix and shows the result inline.
  Results copy to the clipboard. Calculation history is stored locally, is
  searchable, and is pruned on a configurable retention schedule.
- **Emoji picker** — fuzzy emoji search behind the `:` prefix, matching
  shortcodes first and then labels and tags, presented as a grid. The chosen
  emoji is copied to the clipboard, and frequently used emoji surface first.
- **Bangs** — DuckDuckGo-style `!bang` shortcuts recognized anywhere in the
  query, resolving to the target site's search URL. Ships with a bundled
  snapshot of the bang database and can refresh it from DuckDuckGo on
  demand.
- **Open URL** — recognizes URLs and bare domains, validated against the
  Public Suffix List, and offers to open them in the browser or copy them.
  Bare domains that do not resolve are suppressed so ordinary words are not
  mistaken for hostnames.
- **ZeroTier** — search, join, connect, disconnect, and forget ZeroTier
  networks by name or network id, merging live state from the local
  ZeroTier daemon with locally remembered networks. Requires the ZeroTier
  One daemon to be installed and running.
- **Awake** — start and stop keep-awake sessions, optionally for a duration
  such as `awake 2h` and optionally keeping the display on. Requires
  Amphetamine to be installed in `/Applications`.

#### Built into the app

- **Application launcher** — finds and launches installed applications, with
  their real icons, and can reveal them in Finder with `Cmd+Enter`. The
  application list refreshes in the background.
- **Clipboard history** — captures what you copy across text, image, file,
  HTML, and RTF formats, and lets you search it and paste it back from a
  dedicated view, reachable with `Cmd+Shift+V`. History is retained for a
  configurable number of days.
- **System commands** — Lock Screen, Sleep, Restart, Shut Down, Log Out,
  Empty Trash, Start Screen Saver, Eject Disc, and Toggle Dark/Light Mode.
- **System preferences** — searches macOS settings panes and opens them
  directly.
- **App commands** — Quit Torchsnap, Settings, and Developer Tools.

#### Gadget platform

- Gadgets are WebAssembly components targeting `wasm32-wasip2`, executed by
  wasmtime. A gadget reaches the host only through the capabilities its
  manifest declares; everything is denied by default.
- Capabilities available to gadgets: structured logging, write-only
  clipboard, per-gadget SQLite storage, its own frecency scores, its own
  settings, URL and path opening, outbound HTTP restricted to allow-listed
  origins, read-only filesystem access restricted to allow-listed globs,
  reading files from its own archive, spawning subprocesses whose arguments
  are matched against declared patterns, OS and architecture detection, path
  template resolution, shared website metadata lookup, and application icon
  resolution.
- Permission grants are enforced before any guest code runs: the host
  inspects the compiled component's imports and refuses to instantiate a
  gadget that reaches for something its manifest did not declare.
- Gadgets implement lifecycle hooks, contribute catalog entries and query
  results, execute actions, answer request/response messages from their own
  frontend, and run scheduled tasks on a cron expression.
- Gadgets can ship a frontend bundle providing launcher views, inline views,
  and their own settings page. These bundles import host-provided React
  hooks and styled UI components at runtime rather than bundling their own
  copies, so they stay small and inherit host styling and fixes.
- Compiled components are cached to disk, keyed by a hash of the WebAssembly
  and the engine configuration, so cache invalidation is automatic. Cached
  code is memory-mapped, letting the OS page it out under pressure.
- The Winch baseline compiler is used to keep idle memory footprint low.
- Gadgets are discovered from three roots: those bundled with the app, those
  installed by the user, and — in debug builds only — the working tree, so a
  gadget under development is picked up without packaging.
- Manifest-referenced paths are validated at parse time and again at every
  read, rejecting absolute paths and directory traversal.
- **Rust gadget SDK** (`torchsnap-gadget-sdk`) — centralizes the WIT
  bindings, re-exports the generated types behind a prelude, and reduces
  registering a gadget to a single `define_gadget!` invocation, with no-op
  macros for the messaging and task hooks a gadget does not use.
- **Frontend gadget SDK** (`@torchsnap/gadget-sdk`) — hooks, themed
  components, keybinding helpers, a Vite plugin, and testing utilities for
  gadget frontends.

#### Settings and developer tools

- A separate settings window with a custom title bar, covering: launch at
  login, the global shortcut, appearance and mascot options, frecency
  tracking and statistics, website metadata cache retention and statistics,
  and gadget management.
- Gadget management lists every gadget with its origin, allows enabling and
  disabling each one, installs `.torchsnap` archives by file picker or drag
  and drop, and uninstalls user-installed gadgets along with their stored
  data and settings.
- Gadgets with their own settings page get a dedicated entry in the settings
  sidebar.
- Light, dark, and system themes, applied consistently across all windows
  and following the OS appearance when set to system.
- A developer tools window with a structured log console covering both host
  and gadget activity.

#### Building and packaging

- `just` drives the whole workflow: `just install` prepares the environment,
  `just doctor` checks the required toolchain, `just build` produces a
  release bundle, and `just fullcycle` runs formatting, linting, tests, and
  a frontend build.
- Release builds embed only the gadgets whitelisted in
  `gadgets/bundled.toml`; `just build-gadget <name>` builds and packages any
  single gadget into a `.torchsnap` archive.
- Asset generation for mascots, application and tray icons, timezone data,
  and the bundled bang database is reproducible from source through
  `just assets`.
- The Rust host carries an extensive unit and integration test suite,
  including WebAssembly fixture gadgets exercising the host capability
  surface end to end.

### Known limitations

- **macOS is the only supported platform.** The code compiles for Linux and
  Windows against a fallback layer, but application discovery and settings
  pane discovery return nothing there, opening and revealing paths is
  unimplemented, the launcher steals focus instead of floating without
  activation, and clipboard sensitivity detection is inert. On Linux
  specifically, the global shortcut does not reach the app under Wayland
  without a manual workaround, the tray icon does not respond to left-click
  on GNOME, and auxiliary windows are drawn with duplicate title bars.
- **The application is not code-signed or notarized**, so macOS Gatekeeper
  will warn on first launch.
- **There is no auto-updater.** New versions must be downloaded and
  installed manually.
- **`.torchsnap` archives are unsigned.** There is no cryptographic proof of
  authorship or integrity for a gadget you obtain from someone else;
  installing one is a matter of trusting its source, as with any unsigned
  application. The sandbox limits what an installed gadget can reach, but
  the argument matching on the subprocess capability is a guard against
  mistakes, not a boundary against a hostile gadget author.
- **Installing or uninstalling a gadget requires restarting the app**, and a
  gadget cannot be replaced in place — uninstalling and reinstalling within
  one session is rejected until after a restart. Enabling and disabling
  gadgets works without a restart.
- **Gadget execution is not resource-limited.** A gadget stuck in a loop or
  allocating without bound can occupy a host thread or grow the process.
- **Permissions are not surfaced in the UI.** There is no prompt at install
  time showing what a gadget is asking for, and no way to review the
  permissions of an installed gadget from Settings.
- **A gadget whose startup fails may still be shown as enabled** in the
  settings panel while being inert.
- Gadgets have SQLite storage only; there is no blob or key-value store, and
  a gadget cannot ship a helper binary reachable as a real file on disk.
- Gadget messaging is request/response only; a gadget frontend cannot
  subscribe to a stream of updates from its WebAssembly side.
- Output a gadget writes to stdout or stderr goes to the terminal rather
  than the developer tools log console.
- **There is no first-run experience.** Because the app has no Dock icon and
  lives only in the menu bar, nothing tells you the global shortcut on first
  launch.
- The interface is English-only, key bindings cannot be rebound, and no
  accessibility pass has been done.
- Settings, history, and gadgets cannot be exported or transferred between
  machines, and results cannot be pinned or favorited.

[0.15.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.15.0
[0.14.1]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.14.1
[0.14.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.14.0
[0.13.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.13.0
[0.12.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.12.0
[0.11.1]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.11.1
[0.11.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.11.0
[0.10.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.10.0
[0.9.3]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.3
[0.9.2]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.2
[0.9.1]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.1
[0.9.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.0
