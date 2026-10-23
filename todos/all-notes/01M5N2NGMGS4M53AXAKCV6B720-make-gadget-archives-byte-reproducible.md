---
title: "Make gadget archives byte-reproducible"
kind: chore
component: project
origin: implementation
tags: [tooling]
---
# Make gadget archives byte-reproducible

Two builds of an unchanged gadget give `.torchsnap` archives with different bytes. Found on 2026-10-23: `zipinfo -T target/bundled-gadgets/calculator.torchsnap` showed `frontend/dist/*` entries stamped with the build time, and two builds had different SHA-256 sums (99be0c2a… and 95fda913…) with the same content.

## Problem

`package-gadget` in `just/gadgets.just` packs with `zip -r`, which stores each entry's mtime and extra fields (uid/gid, extended timestamps). `build-gadget` rebuilds each gadget frontend with Vite and copies the `.wasm`, so those files get fresh mtimes on every build. The entry order follows the directory walk.

Staging works around it: `tools/stage-bundled-archives` compares entry names and decompressed bytes and keeps an unchanged staged archive (ADR 0068).

## Impact

Archive hashes can't identify a gadget build, a release can't be rebuilt bit for bit, and every tool that compares archives must open them.

## Suggested fix

Pack with `zip -X` (no extra fields), a fixed mtime for every entry (for example `SOURCE_DATE_EPOCH` or `touch -d` on a staging copy) and a sorted file list. Check with two builds of each bundled gadget that the archives are identical, and add a test for it.
