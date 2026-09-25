# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Recording a shortcut no longer accepts a single key or Shift plus a
  key, which took that key away from every other app. A shortcut needs
  Cmd, Ctrl or Alt, or is an F-key on its own. While recording, a hint
  says so, and a combination that does not qualify keeps the recorder
  listening instead of ending it.
- Letter shortcuts inside the launcher, such as Ctrl+D and Ctrl+U for
  scrolling, keep working while CapsLock is on. Shortcuts with Shift
  and a letter work at all now, including the Cmd+Shift+R (Reveal) and
  Cmd+Shift+O (Open With) hints shown in the launcher footer.
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

### Changed

- Gadget API: `OpenSettings` actions reach the gadget's `execute()`
  like every other action. To open its settings, a gadget returns the
  new `open-settings` post-action (`PostAction::OpenSettings` in the
  Rust SDK). Gadgets built against the previous interface need a
  rebuild.
- Settings → General lists Updates before Advanced, and every settings
  row keeps a gap between its text and its switch.

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

[0.12.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.12.0
[0.11.1]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.11.1
[0.11.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.11.0
[0.10.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.10.0
[0.9.3]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.3
[0.9.2]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.2
[0.9.1]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.1
[0.9.0]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v0.9.0
