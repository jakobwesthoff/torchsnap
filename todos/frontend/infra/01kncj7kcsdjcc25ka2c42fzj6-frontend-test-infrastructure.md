---
kind: chore
status: open
tags: [testing]
---

# Frontend Test Infrastructure and Tests

## Problem

The project has no frontend test infrastructure — no test runner, no test
files, no mocking utilities. All testing is currently backend-only (Rust
`cargo test`). Frontend code is verified manually.

## Scope

Set up a test runner and write tests for the primary frontend features.

### Infrastructure

- **Test runner**: Vitest (integrates with the existing Vite build, fast,
  supports TypeScript natively).
- **React testing**: `@testing-library/react` for component tests.
- **Tauri mock**: Mock `@tauri-apps/api/core` `invoke` for command tests.
  The `command()` wrapper in `src/lib/command.ts` is the single entry point
  — mock it at that level.

### Priority test targets

1. **Log worker** (`src/lib/logWorker.ts`) — the async queue, local→backend
   ID mapping, sequential processing, edge cases (unknown span IDs, queue
   drain ordering).

2. **Logger class** (`src/lib/logger.ts`) — level methods enqueue correct
   commands, spanStart returns local IDs, spanEnd references correct IDs.

3. **useLogFilters** (`src/devtools/console/useLogFilters.ts`) — level
   filtering, span filtering, source filtering, text search, counts.

4. **useTreeView** (`src/devtools/console/useTreeView.ts`) — tree building
   from flat stream, collapse/expand, flatten for virtualization.

5. **formatters** (`src/devtools/console/formatters.ts`) — timestamp
   formatting, duration formatting, clipboard formatting.

6. **Gadget view integration** — LoggerProvider provides logger via context,
   useLogger returns it, gadget views receive logger prop.

### Non-priority (defer)

- Visual component rendering tests (LogItemRow, TreeLogList, ConsoleToolbar)
  — these are better verified visually.
- E2E tests with real Tauri backend — requires Tauri test driver setup.
