---
title: "Add frontend tests for the existing features"
kind: chore
component: frontend
tags: [testing]
---
# Add frontend tests for the existing features

The frontend has a test runner and tests for part of the code. This todo covers
the modules that are still untested, starting with the priority targets below.

## Infrastructure (in place)

- Vitest with jsdom, `@testing-library/react`, `@testing-library/user-event` and
  `@testing-library/jest-dom`. Config in `vitest.config.ts`, which merges
  `vite.config.ts`.
- `bun run test`, and `just test-frontend` as part of `just test` and
  `just fullcycle`.
- `src/test/tauri.ts`: `mockCommands(handlers)` answers backend commands through
  `@tauri-apps/api/mocks`, typed by `CommandMap`, so tests run the real
  `command()` wrapper. `emitTauriEvent` delivers events to `listen()` callers.

## Priority test targets

1. **Log worker** (`src/lib/logWorker.ts`, no test yet): the async queue, local→backend ID
   mapping, sequential processing, edge cases (unknown span IDs, queue drain
   ordering).
2. **Logger class** (`src/lib/logger.ts`, no test yet): level methods enqueue the correct
   commands, spanStart returns local IDs, spanEnd references the correct IDs.
3. **useLogFilters** (`src/devtools/console/useLogFilters.ts`, no test yet): level, span,
   source and text filtering, counts.
4. **formatters** (`src/devtools/console/formatters.ts`, no test yet): timestamp, duration and
   clipboard formatting.
5. **Gadget view integration** (no test yet): LoggerProvider provides the logger via context,
   useLogger returns it, gadget views receive the logger prop.

`useTreeView` is covered by `src/devtools/console/useTreeView.test.ts`.

## Non-priority (defer)

- Visual rendering tests for LogItemRow, TreeLogList, ConsoleToolbar.
- E2E tests with a real Tauri backend (needs a Tauri test driver).
