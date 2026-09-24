---
kind: plan
status: open
---

# Check for and install updates

Coordinates `todos/product/features/01kmh1ah0j1c2cx7pa39rmp7k0-auto-updater.md`.
The todo holds the background (current release flow, what Tauri's
updater plugin offers, the cross-platform notes). This file holds the
decisions and the steps.

**Progress (2026-09-24):** decisions made, no step started. Work
happens on branch `auto-updater` in three git worktrees next to the
main checkouts: `../torchsnap--auto-updater`,
`../torchsnap-docs--auto-updater` and `../torchsnap-web--auto-updater`.

## Goal

A Torchsnap installed from `Torchsnap.dmg` notices that a newer release
exists, shows its version and notes, and installs it and restarts when
the user agrees. A welcome window introduces Torchsnap and asks whether
to check for updates automatically. torchsnap.app serves the update
feed and shows the latest version next to the download button. macOS
on Apple silicon only. Ships with 0.12.0.

## Decisions

Decided with the maintainer on 2026-09-24.

| Topic | Decision |
|---|---|
| Engine | `tauri-plugin-updater`. |
| First version with the updater | 0.12.0. Users of 0.11.x download it by hand once. |
| Automatic checks | Asked once; the answer is stored and can be changed in Settings. |
| Manual check | Always available, independent of the automatic-check setting. |
| Check timing (when automatic) | A few seconds after startup, then every 24 hours while running. The time of the last check is stored, so a restart within 24 hours does not check again. |
| Download timing | Only after the user clicks "Install and Restart", with progress. |
| Where a found update appears | A small dedicated update window with the new version, the notes, **Install and Restart**, **Later** and **Skip This Version**. |
| Later | Closes the window. The tray menu shows an item for the pending update until it is installed or skipped. The next automatic check opens the window again. |
| Skip This Version | Hides that version for good; a newer one is offered again. |
| Release notes | Rendered as Markdown, for every version between the installed and the new one. |
| First-launch question | Lives in a separate welcome window, not in the update window. |
| Welcome window flow | 1. Welcome: mascot and one sentence. 2. How it works: a visual. 3. Shortcut: the Settings shortcut recorder with the current shortcut filled in. 4. Launch at login and automatic update checks: two switches, with a short text on what the update check sends where. 5. Done: "Press <shortcut> now". Every step comes prefilled, so continuing through all steps is valid. Appearance, Control API and gadgets stay in Settings only. |
| Who sees the welcome window | Everyone running a version that has it and has not completed it yet, new users and users upgrading from 0.11.x alike. |
| Showing it again | A button in Settings opens the welcome window again at any time. |
| Welcome window state | Stored persistently, so a later version does not show it again to someone who has seen it. |
| "How it works" visual, timing | Built within this plan, before the first release that has the updater. |
| Manual check and Show Welcome | Settings → General gets an area next to the build line with Check for Updates, the automatic-check switch and Show Welcome. The tray menu gets "Check for Updates...". |
| Website copy | torchsnap-web stays unchanged. |
| Endpoint | The update feed is served from torchsnap.app. |
| Getting the feed onto torchsnap.app | `release-publish` uploads the feed file to the GitHub release. The site build downloads it and serves it. `release-publish` then starts the site deploy with `gh workflow run deploy.yml -R jakobwesthoff/torchsnap-web`. |
| Feed download fails during the site build | The build fails, and the previous deployment stays live. |
| Version near the download button | Part of this plan, from the same build-time download. It replaces option A/B of torchsnap-web `todos/01m39tc7baqfjta27mqhh4796q-show-latest-version-near-download.md`. |
| Welcome window counts as seen | When the user clicks Done or closes the window. |
| Welcome state storage | A revision number in `settings.json` (for example `welcome.seenRevision`). The window shows while the stored number is lower than the app's welcome revision. Bumping the revision shows a reworked welcome once more. |
| App in a place it cannot update | Before offering Install, the app checks its bundle path (a mounted volume under `/Volumes/`, or an App Translocation path). There the window explains that Torchsnap has to be moved to Applications to install updates and offers the download link instead of Install. |
| Failed automatic check | Only logged; the next scheduled check tries again. A failed manual check shows its error in the update window. |
| "Later" across a restart | The postponed update is kept in memory only. After a restart the tray item returns with the next check that finds the update. |
| Pending gadget changes | The update window says in one line that the restart also applies the pending gadget changes, with their count. |
| Test feed | An environment variable (for example `TORCHSNAP_UPDATE_FEED`) overrides the feed URL in every build, including signed release builds. Its use is logged. The minisign check against the compiled-in public key still applies. |
| "How it works" visual | A storyboard comes first; the medium is picked after it. Candidate to look at then: the scripted React demo torchsnap-web removed from its hero in `1e02fd0` as too distracting there (`web/src/launcher/DemoLauncher.tsx`, `useScriptedDemo.ts`, `scenarios.ts`, `candidates.ts`, `match.ts`), reduced to a minimal example. |
| Notes of versions in between | `release-publish` writes the notes of every version from `CHANGELOG.md` into an extra field of the feed. The update window shows all versions newer than the installed one. The plugin exposes the whole feed as `Update::raw_json`. |
| Feed content | `version`, `pub_date` (RFC 3339), `notes` (newest CHANGELOG section), `platforms.darwin-aarch64` with `url` pinned to the version (`releases/download/v<version>/…`, not `latest/download`) and `signature` (contents of the `.sig`), and a custom `releases` array with version, date and Markdown notes of every CHANGELOG section, without a cutoff. No forced-update flag, no rollout, no minimum macOS version until a release raises it. |
| Feed URL on torchsnap.app | `https://torchsnap.app/updates/latest.json`. |
| Feed file in the release | `release.json`. It merges with the staging receipt `release-build` writes today (`src-tauri/target/release/dist/release.json`: version, commit, DMG SHA-256): `release-build` writes the full file with the feed content plus commit and DMG checksum, `release-publish` checks against it as today and uploads it as a release asset. |
| Pre-releases | Never offered by the updater. They stay manual downloads. |
| Onboarding todo | `todos/product/features/01kmh2c7pem81px3twgqhsz4th-onboarding-first-run.md` is deleted when this plan is cleaned up. |
| Updater signing key | Key file under `~/.config/torchsnap/`, its path and password in `release.env` next to the Apple credentials. Key and password are also kept in the maintainer's password manager. |

## Findings that shape the design

Checked in the repositories on 2026-09-24.

- **Windows.** The app creates the windows `main` (launcher),
  `settings` and `devtools` (`src-tauri/src/lib.rs`). The capability
  `src-tauri/capabilities/default.json` covers all three.
- **Tray.** The menu has Open Launcher, Settings..., Developer Tools...
  and Quit Torchsnap (`src-tauri/src/platform/macos/tray.rs`).
- **Settings → General** shows `Build: v<version> (<git hash>)` at the
  bottom and has the sections Startup, Shortcut and Advanced
  (`src/settings/sections/GeneralSection.tsx`).
- **Restart.** `restart_to_apply_gadget_changes` restarts with
  `app.restart()`. Pending gadget installs and uninstalls are applied
  at startup (`src-tauri/src/gadget_install/mod.rs`, `lib.rs`).
- **Launch at login** uses `tauri_plugin_autostart` with
  `MacosLauncher::LaunchAgent` (`src-tauri/src/lib.rs`).
- **No TCC permissions.** The entitlements contain only the two
  hardened-runtime exceptions for gadget code (ADR 0046). The code
  asks for no Accessibility or Screen Recording access.
- **Compile cache.** Cached gadget code is keyed by
  `Engine::precompile_compatibility_hash()` (ADR 0044), so a new
  wasmtime version in an update does not load stale code.
- **Build and notarization.** `just build --release --sign` runs
  `tauri build`, which notarizes and staples the `.app`, then
  `just notarize-dmg` notarizes and staples the DMG
  (`just/build.just`). `release-build` requires `main` and checks
  Gatekeeper, stapled tickets, `arm64` and the bundle version
  (`just/release.just`).
- **Markdown.** The frontend has no Markdown renderer
  (`package.json`). Release notes are CHANGELOG Markdown.
- **Privacy claims.** torchsnap-web says "No account. No telemetry. No
  catch." and "Nothing leaves your machine."
  (`web/src/landing/Convictions.astro`). An update check sends a
  request to the update host on every check.
- **Plugin version.** `tauri-plugin-updater` and
  `@tauri-apps/plugin-updater` are both at 2.12.0 (crates.io, npm).
- **How the plugin installs on macOS** (`plugins/updater/src/updater.rs`
  in tauri-apps/plugins-workspace, branch `v2`, `install_inner`): it
  extracts the `.app.tar.gz` into a temporary directory, renames the
  running `.app` into a backup directory and renames the new one into
  place. When the first rename fails with `PermissionDenied`, it runs
  `rm -rf` and `mv` through AppleScript `with administrator
  privileges`, which shows the macOS password prompt. The file neither
  sets nor removes `com.apple.quarantine` and runs no signature check
  beyond the minisign verification of the download.
- **Quarantine after update.** tauri-apps/plugins-workspace#3082 (open,
  November 2025) reports an app that macOS quarantined after an update.
- **Artifacts need the key.** With `createUpdaterArtifacts: true`,
  `tauri build` requires `TAURI_SIGNING_PRIVATE_KEY`. The key password
  may be empty. The option can be passed only at release time through
  `tauri build --config`, so development builds keep working without
  the key.
- **Rust-only use.** `app.updater()?.check()` and the install calls
  are plain Rust methods. The capabilities `updater:*` only gate the JS
  commands. A Rust-driven updater needs no new webview capability.
- **Endpoints and format.** `endpoints` is an ordered list; the plugin
  tries the next one only after a non-2xx answer. The feed needs
  `version` and per platform (`darwin-aarch64`) `url` and `signature`;
  `notes` and `pub_date` (RFC 3339) are optional. The plugin passes
  `notes` through as a string.
- **Only full archives.** No delta updates, no DMG as update format.
- **Sparkle for Tauri** exists only as the third-party
  `ahonn/tauri-plugin-sparkle-updater` (v0.3.0, Sparkle 2.9.6). No
  plugin from the tauri-apps organisation.
- **Feed fields the plugin reads.** `version`, `notes`, `pub_date` and
  `platforms`; it looks up `darwin-aarch64-app` first, then
  `darwin-aarch64`. Unknown fields are ignored by the parser and stay
  readable through `Update::raw_json`.
- **Signed version.** The plugin rejects an archive whose signature was
  made for another version than the feed announces
  (`SignedVersionMismatch`).
- **What a check sends.** A GET to the configured URL with the user
  agent `tauri-plugin-updater/<plugin version>`. Without
  `{{current_version}}` in the URL, the request carries nothing about
  the installed app beyond the IP address.
- **Existing `release.json`.** `release-build` writes a staging receipt
  (version, commit, DMG SHA-256) to
  `src-tauri/target/release/dist/release.json`; `release-publish`
  checks it and uploads only `Torchsnap.dmg`.
- **Site deploy.** torchsnap-web deploys through
  `.github/workflows/deploy.yml` on pushes to `main` and on
  `workflow_dispatch`.
- **Removed web demo.** torchsnap-web removed a scripted React launcher
  demo from its hero in `1e02fd0` because it distracted there.
- **Welcome window base.** New windows go through
  `show_auxiliary_window` with an `AuxiliaryWindowConfig`, an HTML entry
  in `vite.config.ts` and a React root that emits `react-ready`
  (`src-tauri/src/lib.rs`). The shortcut recorder is the component
  `ShortcutRecorder`, used by `src/settings/ShortcutSection.tsx`.
- **CHANGELOG size.** 22 KB with 8 version sections (2026-09-24).

## Unverified assumptions

Settled by the spike (step 1).

- The `.app` inside `Torchsnap.app.tar.gz` carries the stapled
  notarization ticket.
- A drag-and-drop install into `/Applications` is writable by the user
  who made it, so the plain rename succeeds and no password prompt
  appears.
- Gatekeeper accepts the replaced app without a prompt (see #3082
  above), and the launch agent still starts it at login.
- The plugin follows GitHub's redirect from
  `releases/download/v<version>/Torchsnap.app.tar.gz` to the file.

## Steps

Each step ends in commits on the `auto-updater` branches and a note in
"Progress".

1. **Spike (torchsnap).** Add the plugin with the feed URL and the
   `TORCHSNAP_UPDATE_FEED` override, generate a throwaway key, build a
   signed, notarized app with `createUpdaterArtifacts` passed through
   `--config`, serve a hand-written feed from localhost, and update an
   older build installed in `/Applications`. Settles the unverified
   assumptions. What survives becomes the start of step 4.
2. **Key and ADR (torchsnap).** Generate the real key pair, store it as
   decided, put the public key into `tauri.conf.json`. ADR for the
   update feed (plugin, torchsnap.app URL, `release.json`, key
   handling), amending ADR 0050 for the release flow.
3. **Release pipeline (torchsnap).** `release-build` checks the updater
   key variables, builds with `createUpdaterArtifacts`, verifies the
   archive (stapled ticket, signature) and writes the full
   `release.json`, including `releases` from every CHANGELOG section.
   `release-publish` checks it, uploads DMG, archive, signature and
   `release.json`, then starts the torchsnap-web deploy.
4. **Updater backend (torchsnap).** Settings keys (automatic checks,
   last check, skipped version), scheduling, manual and automatic
   checks, the location check, install with the pending-gadget count,
   commands and events for the windows, the tray item. Tests for the
   scheduling, version filtering and location logic.
5. **Update window (torchsnap).** New auxiliary window: versions and
   Markdown notes newer than the installed one, Install and Restart
   with progress, Later, Skip This Version, the pending-gadget line,
   the "move to Applications" state, "up to date" and error states for
   manual checks.
6. **Settings → General (torchsnap).** The area next to the build line:
   Check for Updates, automatic-check switch, Show Welcome.
7. **Welcome window (torchsnap).** New auxiliary window with the five
   steps, step 2 still a placeholder; revision check at startup and
   the Settings button.
8. **"How it works" (torchsnap).** Storyboard, then the medium with the
   maintainer (the removed web demo is a candidate), then build it into
   step 2 of the welcome window.
9. **Website (torchsnap-web).** The build downloads
   `releases/latest/download/release.json` from GitHub and serves it as
   `/updates/latest.json`, failing when the download fails. The version
   appears next to the download button (wording and placement follow
   `docs/copywriting-guide.md`, agreed with the maintainer). Delete
   `todos/01m39tc7baqfjta27mqhh4796q-show-latest-version-near-download.md`.
   The latest release has no `release.json` until 0.12.0 is out, so
   this branch merges after that release.
10. **Docs (torchsnap-docs, torchsnap).** Installation page: updating
    and the one manual update from 0.11.x. Settings page: the new
    area. A page or section on the welcome window. What the update
    check sends where. README "Releasing" (key, new artifacts,
    `release.json`), CHANGELOG.
11. **Release.** Publish 0.12.0, merge and deploy the website, check
    `https://torchsnap.app/updates/latest.json`. Publish a follow-up
    release and update to it from an installed 0.12.0.
12. **Clean up.** Delete this plan, the auto-updater todo and the
    onboarding todo, and drop `plan:` lines that point here.

## Later

- Linux and Windows, per the todo's cross-platform section.
