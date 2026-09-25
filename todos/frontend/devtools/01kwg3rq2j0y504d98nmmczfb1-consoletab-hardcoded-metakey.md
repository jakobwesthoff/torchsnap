---
kind: bug
severity: low
status: open
area: [src/devtools/console/ConsoleTab.tsx]
tags: [unconfirmed]
---

# ConsoleTab shortcuts hardcode metaKey — dead on non-macOS

## Problem
The devtools console registers its own raw keydown listener instead
of the keybinding system (`src/devtools/console/ConsoleTab.tsx:50-61`):

```ts
const handleKeyDown = useCallback(
  (e: KeyboardEvent) => {
    if (e.metaKey && e.key === "k") {
      e.preventDefault();
      clear();
    } else if (e.metaKey && e.key === "f") {
      e.preventDefault();
      searchInputRef.current?.focus();
    }
  },
  [clear],
);
```

`e.metaKey` is the Cmd key on macOS and the Windows/Super key
elsewhere. The rest of the codebase deliberately maps `Meta` to
`ctrlKey` on non-macOS (`src/keybindings/matching.ts:112-113`:
`const metaPressed = isMacOS ? event.metaKey : event.ctrlKey;`), and
the backend ships a non-macOS `fallback` platform backend
(`src-tauri/src/platform/mod.rs:43-47`), so non-macOS is a supported
target. The clear button's tooltip advertises the shortcut
unconditionally (`ConsoleToolbar.tsx:312`: `title="Clear log (⌘K)"`).

Additionally, no other modifiers are checked: Cmd+Shift+K,
Cmd+Alt+K etc. also trigger clear, and the lowercase `e.key === "k"`
comparison fails under CapsLock, since `event.key` carries the
produced character (`matchesCombo` compares letters case-insensitively
for this reason).

## Impact
On Windows/Linux builds the devtools clear/search shortcuts do not
work with Ctrl (only with the Super/Win key, which the OS typically
intercepts), while the UI advertises ⌘K.

## Suggested fix
Route these through the existing keybinding system
(`KeyBindingProvider` + `useKeyBindings` with `Meta` combos), which
already handles the platform mapping and stray-modifier rejection.
The devtools window currently mounts no `KeyBindingProvider`
(`src/devtools/main.tsx`), so either add one or reuse
`matchesCombo(e, combo, platform === "macos")` directly.
