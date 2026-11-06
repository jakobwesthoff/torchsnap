---
title: "Upgrade rusqlite to 0.41 once rusqlite_migration supports it"
kind: chore
component: backend
status: blocked
origin: upstream
tags: [dependencies]
---
# Upgrade rusqlite to 0.41 once rusqlite_migration supports it

The 0.16.0 dependency round on 2026-11-06 left `rusqlite` on 0.40.2 while
0.41.1 is out. `rusqlite_migration` 2.6.0, its newest release, requires
`rusqlite ^0.40`, and the two versions cannot share a build: both pull
`libsqlite3-sys`, which links the native library, so cargo allows only
one.

## Scope

Move `rusqlite` in `src-tauri/Cargo.toml` to 0.41 as soon as a
`rusqlite_migration` release allows it. Checked on crates.io on
2026-11-06: its last release is from 2026-05-28.

If no release follows, the options are a pull request upstream, or
replacing the crate with the project's own migration runner. The project
must not stay behind on `rusqlite` only because of the extra work.

## Done when

- `rusqlite` is on the newest release and `just fullcycle` passes.

## Relations

- Blocked by: a `rusqlite_migration` release that supports `rusqlite` 0.41
