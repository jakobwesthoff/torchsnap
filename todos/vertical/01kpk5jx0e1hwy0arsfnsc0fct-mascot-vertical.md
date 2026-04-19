# Mascot feature vertical (`src/mascot/`)

Collapse all mascot-related code into a single `src/mascot/` vertical.
This is the most scattered feature in the frontend — spread across six
locations despite being fully self-contained and changing for the same reasons.

## Why

Horizontal folders (`hooks/`, `components/`, `lib/`) group by file type
rather than reason-to-change. The mascot is a coherent feature domain
(astronomical data → variant selection → rendering) with no good reason
to scatter it.

## Files to move

| Current path | Target path |
|---|---|
| `src/hooks/useMascotVariant.ts` | `src/mascot/useMascotVariant.ts` |
| `src/hooks/useFullMoonDistance.ts` | `src/mascot/useFullMoonDistance.ts` |
| `src/hooks/useHemisphere.ts` | `src/mascot/useHemisphere.ts` |
| `src/hooks/useHolidays.ts` | `src/mascot/useHolidays.ts` |
| `src/hooks/useNighttime.ts` | `src/mascot/useNighttime.ts` |
| `src/hooks/useSeason.ts` | `src/mascot/useSeason.ts` |
| `src/hooks/useRandomMascot.ts` | `src/mascot/useRandomMascot.ts` |
| `src/components/Mascot.tsx` | `src/mascot/Mascot.tsx` |
| `src/components/MascotInfoOverlay.tsx` | `src/mascot/MascotInfoOverlay.tsx` |
| `src/lib/astronomy.ts` | `src/mascot/astronomy.ts` |
| `src/lib/preloadMascot.ts` | `src/mascot/preloadMascot.ts` |
| `src/derived/mascots.json` | `src/mascot/mascots.json` |
| `src/mascotVariants.ts` (root) | `src/mascot/variants.ts` |
| `src/launcher/hooks/useMascotInfo.ts` | `src/mascot/useMascotInfo.ts` |
| `src/launcher/hooks/useLauncherMascotPlacement.ts` | `src/mascot/useLauncherMascotPlacement.ts` |

Note: `useMascotInfo` and `useLauncherMascotPlacement` are launcher-specific
consumers. They can stay in `src/launcher/hooks/` if keeping them closer to
their call site is preferred — judgment call at implementation time.

## Barrel export

Add `src/mascot/index.ts` exporting the public surface:
- `Mascot`, `MascotInfoOverlay` (components)
- `useMascotVariant`, `useMascotInfo`, `useLauncherMascotPlacement` (hooks used by launcher)
- `preloadMascot` (called in launcher entry)

Internal helpers (`astronomy.ts`, `useHemisphere.ts`, etc.) can be
non-exported — only the launcher imports them transitively.

## Known callers to update

- `src/launcher/Launcher.tsx` — imports `Mascot`, `MascotInfoOverlay`, `useMascotInfo`, `useLauncherMascotPlacement`
- `src/launcher/hooks/useSearch.ts` — may reference mascot variant state
- `src/launcher/LauncherMascot.tsx` — primary mascot render site
- Any place importing `mascotVariants` from the root `src/` level

## Suggested approach

1. Create `src/mascot/` and move files one-by-one (use `git mv`)
2. Fix imports after each move to keep the build green
3. Add the barrel `index.ts` last
4. Delete emptied horizontal folders if they become empty after this + other verticals

## Priority: HIGH (most scattered, zero cross-feature risk)
