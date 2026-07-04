# TitleBar Alt-pressed state sticks when the window loses focus with Alt held

**Kind:** possible-bug
**Severity:** low

**Area:** src/components/TitleBar.tsx

## Problem
`MacTitleBar` tracks the Alt/Option key with paired window keydown /
keyup listeners (`src/components/TitleBar.tsx:88-101`):

```ts
const down = (e: KeyboardEvent) => {
  if (e.key === "Alt") setIsAltPressed(true);
};
const up = (e: KeyboardEvent) => {
  if (e.key === "Alt") setIsAltPressed(false);
};
```

If the window loses focus while Alt is held (app switch via
Option-containing shortcut, mission-control gestures, clicking
another window), the matching keyup is delivered to whichever window
has focus then — never to this one. `isAltPressed` stays `true`
until the user happens to press and release Alt inside this window
again.

While stuck, the green traffic light permanently acts as Zoom
(`toggleMaximize`) instead of fullscreen and shows the `+` glyph
(`TitleBar.tsx:150-156`), inverting the intended
native-matching behavior the comment describes ("Track Alt so
hovering the green button shows the zoom (`+`) glyph …, matching
native macOS behavior", `TitleBar.tsx:85-87`).

## Impact
After any focus loss with Option held, clicking the green button
maximizes instead of entering fullscreen with no visual explanation
beyond the glyph. Self-heals only on the next Alt press inside the
window.

## Suggested fix
Reset the state on focus loss: add a `window.addEventListener("blur",
() => setIsAltPressed(false))` alongside the key listeners (and
optionally sync from `e.altKey` on any keydown/mouseenter, which
reflects the live modifier state).
