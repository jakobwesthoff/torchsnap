---
kind: bug
severity: low
status: open
area: [src/settings/sections/GeneralSection.tsx]
tags: [unconfirmed, error-handling]
---

# GeneralSection: autostart and build-info promises have no error handling

## Problem
Three promise chains in `GeneralSection` lack rejection handling:

1. `src/settings/sections/GeneralSection.tsx:24-26`:

   ```ts
   useEffect(() => {
     command("build_info").then(setBuildInfo);
   }, []);
   ```

2. `GeneralSection.tsx:28-33`:

   ```ts
   useEffect(() => {
     isEnabled().then((enabled) => {
       setLaunchAtLogin(enabled);
       setAutoStartLoading(false);
     });
   }, []);
   ```

   If `isEnabled()` (autostart plugin) rejects, `autoStartLoading`
   stays `true` forever, leaving the "Launch at login" switch
   permanently disabled with no explanation, plus an unhandled
   rejection.

3. `GeneralSection.tsx:35-42` — `handleLaunchAtLoginChange` sets the
   switch state optimistically, then `await enable()` / `disable()`
   with no catch. On failure the UI shows the new state while the
   real autostart state is unchanged, and the rejection is unhandled
   (the `onChange` caller does not await it).

Other sections in the same directory handle this correctly, e.g.
`FrecencySection.tsx:44-48` attaches `.catch(...)` with a logger.

## Impact
Any autostart plugin error (missing entitlement, sandbox denial,
plugin not registered) yields a stuck-disabled or silently wrong
"Launch at login" toggle and unhandled promise rejections; a
`build_info` failure is swallowed invisibly.

## Suggested fix
Add `.catch` handlers mirroring the `FrecencySection` pattern: log
via `createLogger`, clear `autoStartLoading` on failure, and revert
the optimistic switch state when `enable()`/`disable()` throws.
