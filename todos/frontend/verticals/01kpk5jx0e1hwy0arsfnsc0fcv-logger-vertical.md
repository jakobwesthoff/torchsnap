---
kind: improvement
status: open
---

# Logger feature vertical (`src/logger/`)

Consolidate logging infrastructure into a single `src/logger/` vertical.
Currently split across `lib/`, `contexts/`, and `hooks/` despite being a
coherent feature domain with a clear public interface.

## Why

The three pieces (worker, context/provider, consumer hook) only exist to
serve each other. Scattering them horizontally means opening three folders to
understand one feature.

## Files to move

| Current path | Target path |
|---|---|
| `src/lib/logger.ts` | `src/logger/logger.ts` |
| `src/lib/logWorker.ts` | `src/logger/logWorker.ts` |
| `src/contexts/LoggerContext.tsx` | `src/logger/LoggerContext.tsx` |
| `src/contexts/LoggerProvider.tsx` | `src/logger/LoggerProvider.tsx` |
| `src/hooks/useLogger.ts` | `src/logger/useLogger.ts` |

## Barrel export

Add `src/logger/index.ts` exporting:
- `LoggerProvider` (mounted in each window's `main.tsx`)
- `useLogger` (used throughout components)
- The `Logger` type (if re-exported from `logger.ts` for typed usage)

Internal: `logWorker.ts`, `LoggerContext.tsx` — not part of public surface.

## Known callers to update

- Each window's `main.tsx` (`launcher/`, `settings/`, `devtools/`) mounts `LoggerProvider`
- All components/hooks that call `useLogger()`
- `src/devtools/console/useLogStream.ts` — likely imports from logger context or lib

## Suggested approach

1. Create `src/logger/`, move files with `git mv`
2. Fix imports, keep build green throughout
3. Add barrel `index.ts`

## Priority: MEDIUM (small, low risk — good second step after mascot)
