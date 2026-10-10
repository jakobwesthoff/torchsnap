---
title: "Support gadget-bundled executables"
kind: feature
component: gadget-host
status: blocked
tags: [security, macos, capabilities]
---
# Support gadget-bundled executables

Gadgets cannot yet ship and run their own executables. The v1 `command` host interface supports system binaries only, either absolute paths (`/usr/bin/mdfind`) or names resolved against the host process `$PATH` (`mdfind`). Gadget-bundled binaries (`${gadget-archive}/helper`) are deliberately excluded from v1, because they need more infrastructure than was worth gating the v1 release on. Bundled binaries also need gadgets to know which platform they run on; that runtime platform detection is its own todo, which this one depends on.

## Context

Bundled executables let gadgets ship narrow, purpose-built tools (a Rust binary that scrapes a particular API, a Go binary that handles an OS-specific quirk, a packaged `ffmpeg` subset) rather than depending on the user having the right tool at the right version. They matter most for gadgets that need binaries macOS and Linux distributions do not ship by default.

## What's missing for bundled executables

1. **Archive format honours executable bits.** The `.torchsnap` archive format and its extraction code path must preserve and apply Unix file modes on extract. Extraction may strip modes today; this needs verification and a fix.
2. **`binary` field accepts `${gadget-archive}/...` paths.** The manifest parser must canonicalize the path against the gadget's archive root (using the same lexical and canonicalize idiom as `path-under` constraints) and verify at gadget load time that the binary exists and is marked executable. Reject loudly if not.
3. **Cross-platform binary selection.** Gadgets shipping bundled binaries need per-platform builds. Two patterns to choose between:
   - **Manifest-level platform tables**: separate `[[permissions.command]]` entries per platform, with the host picking the matching one at load time.
   - **Path templating with platform tokens**: `binary = "${gadget-archive}/bin/${platform}/${arch}/helper"`, where the host substitutes the tokens. The manifest is simpler, but the gadget side must follow a more conventional layout.
4. **Quarantine attribute handling on macOS.** Binaries extracted from a `.torchsnap` archive carry the `com.apple.quarantine` attribute on macOS, and LaunchServices or `execve` refuse to run them without user approval. Extraction must strip the attribute (the `xattr -d` equivalent) for gadget-bundled binaries the user has already consented to install.
5. **Code signing on macOS.** Apple Silicon refuses to execute unsigned binaries in many configurations. Gadgets shipping binaries on macOS will likely need ad-hoc signing at build time (`codesign --sign -`), or the host has to sign on extraction. Investigate before committing to a pattern.

## Out of scope

- General read and write filesystem access for gadgets. Bundled binaries read their own argv inputs via normal `command::run`. Gadgets gain no new filesystem capabilities through this work.
- Dynamically downloaded binaries. The bundled binary must come from the gadget archive (signed and consented to as part of the gadget install flow). A gadget that pulls an executable down at runtime via `http::fetch` and tries to run it must remain *not* possible.

## Prerequisites and follow-on work

- The v1 `command` host interface must land first.
- Runtime platform detection must land first, so gadgets can pick the right binary path or pass the right argv.
- An audit of archive-format mode preservation must come before item 1 above.
- Enables gadgets that ship purpose-built tools rather than relying on system-installed binaries.

## Relations

- Depends on: [Add a platform host interface for runtime OS and architecture detection](01M4HJV77D9C01NAXBQYSP7WRN-add-a-platform-host-interface-for-runtime-os-and-architecture-detection.md)
- Depends on: [Check whether gadget archives keep Unix file modes from packaging to extraction](01M4HKABQ31HQ1QBM1YNAVX2BS-check-whether-gadget-archives-keep-unix-file-modes-from-packaging-to.md)
