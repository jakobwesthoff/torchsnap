# matchesCombo breaks on CapsLock and makes Shift+letter combos inexpressible

**Kind:** bug
**Severity:** high
**Area:** src/keybindings/matching.ts, src-tauri/src/wasm/bindings.rs

> Severity raised medium → high (2026-07-02, cross-boundary
> pass): the inexpressible Shift+letter case is not just a
> hypothetical gadget-declared combo — the host's *own default
> keybindings* fall into it, see the addendum at the bottom.

## Problem
`matchesCombo` requires exact `event.key` equality before any
modifier logic runs (`src/keybindings/matching.ts:100-102`):

```ts
if (event.key !== combo.key) {
  return false;
}
```

`KeyboardEvent.key` for letter keys reflects the *produced
character*: with Shift held or CapsLock on, pressing the `k` key
yields `event.key === "K"`. Two consequences:

1. **CapsLock breaks every lowercase-letter binding.** Registered
   combos use lowercase letters, e.g. the half-page-scroll bindings
   `{ modifiers: ["Ctrl"], key: "d" }` / `key: "u"`
   (`src/hooks/useHalfPageScroll.ts:57,70`, also
   `src/gadgets/clipboard/ClipboardView.tsx:347,351`). With CapsLock
   active, Ctrl+D produces `event.key === "D"`, the equality check
   fails, and the shortcut silently stops working.

2. **The dedicated Shift-enforcement branch for lowercase letters is
   unreachable for its own use case.** Lines 150-157 exist to make
   `Shift` a strict modifier for single lowercase letters:

   ```ts
   const isSingleLetter = combo.key.length === 1 && combo.key >= "a" && combo.key <= "z";
   if (isSingleLetter) {
     if (wantsShift !== event.shiftKey) {
       return false;
     }
   ```

   But a combo like `{ modifiers: ["Shift"], key: "k" }` can never
   reach this check with `shiftKey === true`, because Shift+k
   produces `event.key === "K"` and the function already returned at
   the key-equality gate. `wantsShift === true` with a lowercase
   letter key is therefore dead configuration — such a binding never
   fires. Gadget-supplied action keybindings flow into this matcher
   verbatim (`src/launcher/hooks/useKeyboardNavigation.ts:200-203`:
   `key: action.keybinding.key`), so a gadget declaring Shift+letter
   hits exactly this.

   The doc comment (`matching.ts:92-94`) claims Shift is "enforced as
   a strict modifier for single lowercase letter keys (a-z)" — the
   enforcement exists, but the combination it is meant to gate cannot
   match at all.

## Impact
All lowercase-letter shortcuts (launcher scrolling, clipboard view
bindings, gadget action combos) silently stop working while CapsLock
is on. Shift+letter combos, which the matcher's own code and docs
present as supported, never fire regardless of CapsLock.

## Suggested fix
For single-letter combos, compare case-insensitively
(`event.key.toLowerCase() === combo.key.toLowerCase()` when
`combo.key.length === 1`) and keep the explicit `wantsShift ===
event.shiftKey` check as the sole Shift arbiter. Alternatively match
on `event.code` ("KeyK") for letters, which is layout-dependent in
the other direction — case-insensitive `key` comparison is the
smaller change. Normalize combo keys at registration so uppercase
declarations like `key: "K"` behave identically.

## Addendum (2026-07-02 cross-boundary pass): host defaults hit this

The host fills default keybindings for well-known actions on
every WASM gadget entry (`default_keybinding_for`,
`src-tauri/src/wasm/bindings.rs:155-170`):

```rust
ActionId::Reveal   => (&["Meta", "Shift"], "r"),
ActionId::OpenWith => (&["Meta", "Shift"], "o"),
```

Both are Shift+lowercase-letter combos — the exact class shown
above to be unmatchable (Shift makes `event.key` uppercase, the
equality gate at `matching.ts:100` fails before Shift handling
runs). Consequences on every WASM gadget entry that carries
Reveal or OpenWith:

- The launcher footer *renders* the Cmd+Shift+R / Cmd+Shift+O
  hints (the footer shows actions that carry a keybinding,
  `src/launcher/Launcher.tsx:40-44`).
- Pressing the advertised shortcut does nothing
  (`useKeyboardNavigation` registers the combo verbatim and it
  never matches).

So the UI advertises shortcuts that are dead on arrival — a
user-visible broken promise on standard actions, not an edge
case. The native `app-launcher` gadget is unaffected (its
Reveal binding is `Meta+Enter`,
`src-tauri/src/gadgets/app_launcher.rs:221-224`).

When fixing, add a regression test that a `["Meta","Shift"] + "r"`
combo matches a synthetic `KeyboardEvent { key: "R", metaKey,
shiftKey }`, since that is the shape the host defaults produce.
