---
title: "Add a platform host interface for runtime OS and architecture detection"
kind: feature
component: gadget-host
---
# Add a platform host interface for runtime OS and architecture detection

Gadgets cannot tell which platform they run on. WASM and WASI do not provide this, so it would have to be a new host-imported function. Any gadget making platform-conditional decisions (different SQL pragmas, different default paths, different feature availability) currently has no way to do so. Gadget-bundled executables also need it, so they can pick the right binary path or pass the right argv; that work is split out into its own todo (see Relations).

## Goal

Gadgets can query the host's operating system and CPU architecture at runtime.

## Proposal

A new host-imported WIT interface:

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

This interface is useful independently of bundled binaries. It is worth landing as its own small feature even if bundled-binary support takes longer.
