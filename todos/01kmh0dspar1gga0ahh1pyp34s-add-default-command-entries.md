# Add default command entries

The launcher should show built-in command entries that are always
available regardless of query, filtered by fuzzy match when the user
types.

## Reference

Nutty's `createCommands()` in `src/launcher/commands.ts` defines:
- Settings (opens settings window)
- Quit (exits app)
- Toggle theme (light/dark switch)

Nutty uses `useCommandFilter` from acornkit to fuzzy-filter commands
against the current query.

## What needs to happen

1. Create a commands system with a `Command` type:
   - `id`, `label`, `icon`, `execute()`, `keepOpen?: boolean`
2. Define default commands: Quit, Settings, Light/Dark Switch
3. Show commands in the results area, filtered by query
4. Commands should appear alongside plugin results in the unified
   item list
