# 68. Rebuild the app only when its inputs changed

Date: 2026-10-23

## Status

Accepted

## Context

`tauri build` runs `vite build`, which empties `dist/` and writes every
file in it anew, 1899 files, nearly all copied from `public/`. Two builds
in a row give files with the same content but new timestamps. The app
embeds `dist/` through `tauri::generate_context!`, which lists every
file it embeds in the app crate's dep-info. Cargo recompiles the crate
when a listed file is newer than the last compile or missing. So every
build recompiled `torchsnap` to embed an unchanged frontend. A file that
is not in the dep-info of the last compile, because it is new, is not
seen at all.

`just build` first stages the bundled gadgets into
`target/bundled-gadgets/`. `src-tauri/build.rs` and tauri-build watch
that directory with `cargo:rerun-if-changed`, and cargo compares the
newest timestamp under a watched directory, directories included, with
its last run. Staging removed the directory and copied every archive
into it again, so the build script reran and `torchsnap` recompiled on
every build. The `.torchsnap` archives are not byte-reproducible: `zip`
stores each entry's timestamp, and every gadget build rewrites the files
it packs.

`src-tauri/build.rs` sets `GIT_HASH`, which Settings shows, from
`git rev-parse` and watched no git file. The rerun on every build kept
the hash current as a side effect.

## Decision

A Vite plugin in the repository, `vite/keep-mtimes.ts`, records every
file in `dist/` with its hash and timestamps before the build. When the
build produced exactly the same set of files, it gives every
byte-identical file its old timestamps back; a changed file keeps its
new timestamp. With any file added, removed or renamed it restores
nothing, so every file is newer than the last compile and cargo embeds
the frontend again, also when the compile after such a build failed or
did not run. Building into a temporary directory and copying with
`rsync -c` into `dist/`, and a plugin that takes over writing `dist/`
from Vite, were rejected.

`tools/stage-bundled-archives` stages the archives. It keeps a staged
archive, with its timestamp, when the new one holds the same entry names
with the same decompressed bytes, and copies it otherwise. An archive
that cannot be read counts as different and is copied. It removes every
other entry from the staging directory.

Release builds rerun `src-tauri/build.rs` when git's `HEAD`, the branch
ref it names, `packed-refs` or the worktree's `logs/HEAD` change, so a
release build always carries the current `GIT_HASH`. Debug builds watch
no git file, because each rerun recompiles `torchsnap`; their hash can
lag behind commits, as it already did with plain `cargo` and
`tauri dev`.

## Consequences

A debug `just build` in which neither the frontend nor a bundled gadget
changed does not compile `torchsnap`. On an Apple silicon Mac it took
about 9 s, against about 15 s with the frontend embedded again. A change
to a frontend file or a `public/` file, a deleted or restored `public/`
file and a change to a bundled gadget each still compile `torchsnap`.
After a build that added, removed or renamed a file in `dist/`, the
whole frontend is embedded again even where most files did not change.
