# 47. Sign macOS bundles on request with --sign

Date: 2026-09-22

## Status

Accepted

## Context

The 0.9.3 release bundle was not code signed by Tauri; it only carried
the linker's signature (`flags=0x20002(adhoc,linker-signed)`).
`syspolicy_check distribution` rejects such a bundle with the fatal error
"Code has no resources but signature indicates they must be present",
and a copy downloaded in a browser is reported by macOS as "damaged and
can’t be opened", with "Move to Trash" and "Cancel" as the only choices.

An ad-hoc signed bundle with the hardened runtime and the entitlements
from ADR 0046 passes `syspolicy_check` with only a warning ("adhoc
signed apps are not suitable for distribution"). Downloaded in a
browser on macOS 26.6, it shows "Apple could not verify … is free of
malware", and the user can approve it once via System Settings → Privacy
& Security → "Open Anyway" with an administrator password. After that
it starts without a dialog and loads all gadgets. Notarization later
needs the same hardened-runtime signature, made with a Developer ID
certificate instead.

Tauri signs the bundle whenever `APPLE_SIGNING_IDENTITY` is set, `-`
included, and then enables the hardened runtime (verified with
`APPLE_SIGNING_IDENTITY=-`, no change to `tauri.conf.json`). lldb cannot
attach to a process from a bundle signed this way ("Not allowed to attach
to process"). `footprint` and `lsappinfo`, which `tools/memsnap` and
`tools/bench-memory` use, work on it. `just start` (`tauri dev`) does not
bundle and is not affected.

## Decision

`just build` gains a `--sign` flag. It sets `APPLE_SIGNING_IDENTITY` to
`-` for `tauri build` when the variable is not already set, and keeps
an identity that is set, such as a Developer ID. Without `--sign` the
bundle stays unsigned unless the environment sets an identity.

Builds for distribution use `--sign`: ad-hoc now, Developer ID with
notarization once an Apple Developer account exists, through the same
flag and the environment variables Tauri reads.

Rejected alternatives:

- Setting `bundle.macOS.signingIdentity` to `-` in `tauri.conf.json`,
  which would sign every bundle and take lldb away from local bundled
  builds.
- A separate config overlay file with the identity, which would need a
  second mechanism for the Developer ID later anyway.
- Signing with `codesign` after `tauri build`, which would re-sign an app
  that Tauri already packed into the DMG.

## Consequences

- Distribution builds are signed with the hardened runtime, which gives
  downloaders the "Open Anyway" path instead of the "damaged" dialog.
- Local builds keep working with lldb unless built with `--sign`.
- The signing identity comes from the environment, so CI and local
  builds share one recipe.
- The README section "Signing macOS builds" lists the variables for
  ad-hoc signing, Developer ID signing and notarization. Notarization
  has not been exercised yet.
