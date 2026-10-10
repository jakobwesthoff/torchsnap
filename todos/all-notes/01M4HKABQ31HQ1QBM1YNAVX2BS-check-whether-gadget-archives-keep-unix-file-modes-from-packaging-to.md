---
title: "Check whether gadget archives keep Unix file modes from packaging to extraction"
kind: investigation
component: gadget-host
---
# Check whether gadget archives keep Unix file modes from packaging to extraction

Gadget-bundled executables need the executable bit of a file inside a `.torchsnap` archive to survive packaging and the extraction on install. The bundled-executables todo says extraction may strip modes today and lists this audit as its prerequisite. The answer decides whether the archive format and the extraction code need a fix first.

## Context

- Archives are zip files built by the `package-gadget` recipe in `just/gadgets.just`.
- Today the host does not extract archives: `ArchiveSource` serves archive contents through `assets::read`, not through filesystem paths (`src-tauri/src/wasm/source.rs`, `root_path`). Extraction on install arrives with bundled-binary support.

## Done when

- It is known whether `package-gadget` records Unix modes in the zip.
- It is known what the planned extraction must do to apply them.
- The findings are in the bundled-executables todo, or in a fix todo it depends on.
