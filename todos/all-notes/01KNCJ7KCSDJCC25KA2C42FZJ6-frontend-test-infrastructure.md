---
kind: chore
status: open
tags: [testing]
---

# Frontend tests for the existing features

## Problem

The frontend has a test runner but almost no tests. Existing frontend
code is verified manually.

## Infrastructure (in place)

- Vitest with jsdom, `@testing-library/react`,
  `@testing-library/user-event` and `@testing-library/jest-dom`.
  Config in `vitest.config.ts`, which merges `vite.config.ts`.
- `bun run test`, and `just test-frontend` as part of `just test` and
  `just fullcycle`.
- `src/test/tauri.ts`: `mockCommands(handlers)` answers backend
  commands through `@tauri-apps/api/mocks`, typed by `CommandMap`, so
  tests run the real `command()` wrapper. `emitTauriEvent` delivers
  events to `listen()` callers.

## Priority test targets

1. **Log worker** (`src/lib/logWorker.ts`): the async queue,
   local→backend ID mapping, sequential processing, edge cases
   (unknown span IDs, queue drain ordering).
2. **Logger class** (`src/lib/logger.ts`): level methods enqueue the
   correct commands, spanStart returns local IDs, spanEnd references
   the correct IDs.
3. **useLogFilters** (`src/devtools/console/useLogFilters.ts`): level,
   span, source and text filtering, counts.
4. **useTreeView** (`src/devtools/console/useTreeView.ts`): tree
   building from a flat stream, collapse/expand, flattening for
   virtualization.
5. **formatters** (`src/devtools/console/formatters.ts`): timestamp,
   duration and clipboard formatting.
6. **Gadget view integration**: LoggerProvider provides the logger via
   context, useLogger returns it, gadget views receive the logger prop.

## Non-priority (defer)

- Visual rendering tests for LogItemRow, TreeLogList, ConsoleToolbar.
- E2E tests with a real Tauri backend (needs a Tauri test driver).
