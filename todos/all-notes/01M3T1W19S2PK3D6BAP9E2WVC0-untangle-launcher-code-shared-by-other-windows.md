---
kind: improvement
status: open
---

# Untangle the launcher code the other windows share

Every window (launcher, settings, devtools, update, welcome) loads the
mascot code and its data, although only the launcher and the welcome
window draw mascots. `vite.config.ts` puts `src/mascot/` and the JSON
files in `src/derived/` (the mascot set, its groups, the time zone
coordinates) into a `mascot` chunk (255 kB, 74 kB gzip; the `shared`
chunk shrank from 743 kB to 488 kB). The mascot code is imported by
`src/launcher/Launcher.tsx`, `LauncherMascot.tsx`,
`hooks/useLauncherMascotPlacement.ts`, `src/welcome/WelcomeWindow.tsx`
and `LauncherPreview.tsx`. Those sit in the `shared` chunk, which
imports the `mascot` chunk, so every window's page preloads it.

Measured on test builds (2026-09-30), not committed:

- With only the JSON files in their own chunk, all five pages still
  loaded it; `shared` imported it at the top.
- With `src/launcher/`, `src/welcome/` and `src/mascot/` in one chunk
  (entry files excluded), that chunk grew to 524 kB, still over the
  500 kB warning, and all five pages still loaded it; the settings,
  devtools and update entry chunks imported it directly.

Imports of `src/launcher/` from outside that folder:

- `src/lib/sdk.ts` imports `launcher/hooks/useWindowedList`, and
  `src/lib/sdk.ts` is imported by `src/launcher/main.tsx` and
  `src/settings/main.tsx`.
- `src/gadgets/clipboard/ClipboardView.tsx` imports
  `launcher/hooks/useWindowedList`.
- `src/mascot/variants.ts` imports `launcher/placement`.

Nothing outside `src/welcome/` imports from it.

## To check

- Whether moving `useWindowedList` out of `src/launcher/` (it is used by
  the gadget SDK and a gadget, not only by the launcher) is enough for
  the launcher, welcome and mascot code to form a chunk that only the
  launcher and welcome pages load.
- Whether that keeps the rule in `vite.config.ts` that entry chunks hold
  no shared exports.
