# Onboarding / first-run experience

After installation, the app lives invisibly in the tray with no
dock icon. Without guidance, users have no idea how to open it.

## The core problem

1. User installs torchsnap and launches it
2. Nothing visible happens (Accessory policy, no dock icon)
3. A tray icon appears but the user may not notice it
4. The user doesn't know the global shortcut
5. The app feels broken

## What needs to happen

- Detect first launch (no settings file exists yet)
- Show a one-time welcome experience that teaches:
  - The global shortcut (Cmd+Shift+Space or whatever is configured)
  - That the app lives in the tray
  - How to access settings
  - How to quit

## Options for the welcome experience

1. **Welcome window**: A small dedicated window (not the launcher)
   with a brief walkthrough. 2–3 steps max. Closes into normal
   tray-only mode.
2. **System notification**: "Torchsnap is running! Press
   Cmd+Shift+Space to open the launcher." Minimal but might be
   dismissed without reading.
3. **First-show overlay**: Show the launcher automatically on first
   run with a hint overlay on top ("This is your launcher. Press
   ESC to dismiss, Cmd+Shift+Space to reopen anytime.")
4. **Guided launcher**: First launch shows the launcher with
   placeholder content explaining the UI ("Try typing something",
   "Press Enter to execute", etc.)

## Considerations

- Should be skippable / dismissable instantly
- Should not feel like a tutorial — keep it to one key message:
  "your shortcut is X"
- Store a `onboardingComplete` flag in settings to never show again
- What about re-onboarding after a shortcut change?
