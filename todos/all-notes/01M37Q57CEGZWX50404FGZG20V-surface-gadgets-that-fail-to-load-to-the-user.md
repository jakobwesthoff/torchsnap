---
title: "Surface gadgets that fail to load to the user"
kind: improvement
component: gadget-host
tags: [error-handling]
---
# Surface gadgets that fail to load to the user

When a gadget's compiled code cannot be loaded, the app keeps running without that gadget and tells the user nothing. The gadget load path and the frontend gadget settings are both involved.

## Problem

Observed on 2026-09-22 with the hardened runtime and none or only some of
the ADR 0046 entitlements: every bundled gadget failed to load. The
process stayed alive, control-socket requests were answered, the launcher
opened, but no gadget worked. Only `vmmap` showed it: no gadget `.cwasm`
file was mapped executable (0 of 6, against 6 of 6 without the hardened
runtime).

The path: `CachedComponent::first_acquire` (`cached_component.rs`) fails
to deserialize the cached `.cwasm`, logs "corrupt compile cache,
recompiling" at warn level to the gadget's devtools log, deletes the
file, recompiles, writes a new cache file and loads that, which fails the
same way. Where the final error goes after that was not traced. In the
experiment nothing user-visible appeared, and the gadgets were simply absent.

ADR 0046's entitlements fix that cause, but any future load failure
(entitlement change, wasmtime upgrade, disk problem) would be just as
silent in a release build, where nobody reads devtools.

## Proposal

- Surface a failed gadget in the UI, e.g. a failed state with the error
  in Settings, Gadgets, and a notice in the launcher on startup.
- Treat "deserialize failed, recompiled, deserialize failed again" as a
  load failure with a clear message instead of a corrupt-cache warning.
- Consider a startup self-check that counts loaded vs enabled gadgets.
