# Plugin-bundled executables + runtime platform detection

## Context

The v1 `command` host interface (see ADR draft for the WASM
command/process exec feature) supports system binaries only — either
absolute paths (`/usr/bin/mdfind`) or names resolved against the host
process `$PATH` (`mdfind`). Plugin-bundled binaries
(`${plugin-archive}/helper`) are deliberately excluded from v1 because
they require additional infrastructure that wasn't worth gating the
v1 release on.

That capability is still desirable: it lets plugins ship narrow
purpose-built tools (a Rust binary that scrapes a particular API, a
Go binary that handles an OS-specific quirk, a packaged `ffmpeg`
subset) rather than depending on the user having the right tool
installed at the right version. It's especially important for plugins
that legitimately need binaries macOS/Linux distributions don't ship
by default.

## What's missing for bundled executables

1. **Archive format honours executable bits.** The `.torchsnap`
   archive format and its extraction code path must preserve and
   apply Unix file modes on extract. Today extraction may strip
   modes; needs verification + fix.
2. **`binary` field accepts `${plugin-archive}/...` paths.** Manifest
   parser must canonicalize the path against the plugin's archive
   root (using the same lexical+canonicalize idiom as `path-under`
   constraints) and verify the binary exists and is marked executable
   at plugin load time. Reject loudly if not.
3. **Cross-platform binary selection.** Plugins shipping bundled
   binaries inherently need per-platform builds. Two patterns to
   choose between:
   - **Manifest-level platform tables**: separate `[[permissions.command]]`
     entries per platform, host picks the matching one at load time.
   - **Path templating with platform tokens**: `binary =
     "${plugin-archive}/bin/${platform}/${arch}/helper"`, host
     substitutes the tokens. Simpler manifest, more conventional
     layout requirement on the plugin side.
4. **Quarantine attribute handling on macOS.** Binaries extracted
   from a `.torchsnap` archive are `com.apple.quarantine`-tagged on
   macOS; LaunchServices/`execve` will refuse to run them without
   user approval. Extraction must strip the attribute (`xattr -d`
   equivalent) for plugin-bundled binaries the user has already
   consented to install.
5. **Code signing on macOS.** Apple Silicon will refuse to execute
   unsigned binaries in many configurations. Plugins shipping
   binaries on macOS will likely need ad-hoc signing at build time
   (`codesign --sign -`) or the host has to sign on extraction.
   Worth investigating before committing to a pattern.

## Runtime platform detection (related, blocking dependency)

Once bundled binaries land, plugins need to know which platform they
are running on so they can pick the right binary path / pass the
right argv. WASM/WASI does not provide this — it would have to be a
new host-imported function:

```wit
interface platform {
  variant os {
    macos,
    linux,
    windows,
    other(string),
  }

  variant arch {
    x86-64,
    aarch64,
    other(string),
  }

  os: func() -> os;
  arch: func() -> arch;
}
```

This interface is useful independent of bundled binaries — any plugin
making platform-conditional decisions (different SQL pragmas,
different default paths, different feature availability) currently
has no way to do so. Worth landing as its own small feature even if
bundled-binary support takes longer.

## Out of scope for this todo

- General read/write filesystem access for plugins. Bundled binaries
  read their own argv inputs via normal `command::run`; plugins do not
  gain new filesystem capabilities through this work.
- Dynamically downloaded binaries. The bundled binary must come from
  the plugin archive (signed/consented as part of the plugin install
  flow). A plugin pulling an executable down at runtime via
  `http::fetch` and trying to run it should remain *not* possible.

## Blocked-by / enables

- Depends on: v1 `command` host interface landing first
- Depends on: archive-format mode-preservation audit
- Enables: plugins that ship purpose-built tools rather than relying
  on system-installed binaries
- Pairs naturally with: a `platform` interface for runtime OS
  detection (which itself is useful independently)
