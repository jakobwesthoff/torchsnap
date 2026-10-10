---
title: "Move the logger code into a src/logger vertical"
kind: refactor
component: frontend
---
# Move the logger code into a src/logger vertical

Logging infrastructure is split across `src/lib/`, `src/contexts/` and `src/hooks/`, although it is one coherent feature with a clear public interface. The worker, the context and provider, and the consumer hook exist only to serve each other, so understanding one feature means opening three folders. Consolidate them into `src/logger/`.

Medium priority: the change is small and low risk, and a good next step now that the mascot vertical is done (`src/mascot/`, `e469a5ab`).

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
- The `Logger` type, if it is re-exported from `logger.ts` for typed usage

Internal: `logWorker.ts` and `LoggerContext.tsx` are not part of the public surface.

## Known callers to update

- Each window's `main.tsx` (`launcher/`, `settings/`, `devtools/`) mounts `LoggerProvider`.
- All components and hooks that call `useLogger()`.
- `src/devtools/console/useLogStream.ts` likely imports from the logger context or lib.

## Suggested approach

1. Create `src/logger/` and move the files with `git mv`.
2. Fix the imports and keep the build green throughout.
3. Add the barrel `index.ts`.
