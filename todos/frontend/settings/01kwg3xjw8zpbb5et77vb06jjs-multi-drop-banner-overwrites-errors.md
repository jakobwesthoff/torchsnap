---
kind: bug
severity: low
status: open
area: [src/settings/sections/GadgetsManagementPanel.tsx]
tags: [unconfirmed]
---

# Multi-archive drop: later install banners silently overwrite earlier errors

## Problem
Dropping several `.torchsnap` files installs them sequentially, each
call reporting through the single banner slot
(`src/settings/sections/GadgetsManagementPanel.tsx:140-146`):

```ts
// Sequentially install each dropped archive so
// collision errors for one do not block the others.
(async () => {
  for (const path of paths) {
    await runInstall(path, setBanner);
  }
})();
```

`runInstall` unconditionally calls `setBanner` with either success or
error (`GadgetsManagementPanel.tsx:387-401`). With N files, only the
last result survives; an error for file 2 of 3 is replaced by the
success banner of file 3. The mixed-drop guard a few lines above
exists precisely so "the user sees a clear error instead of silent
partial success" (`GadgetsManagementPanel.tsx:130-132`), but the
multi-archive path reintroduces exactly that silent partial success.

## Impact
When installing multiple gadgets at once, a failed install can be
completely invisible: the final banner reads
"Installed <name> <version>." while an earlier archive was rejected.
The user believes all drops succeeded.

## Suggested fix
Collect per-file results and render one aggregate banner (e.g.
"Installed 2 gadgets; failed: foo.torchsnap — <reason>"), or make the
banner a list that stacks one entry per file instead of a single
slot.
