---
kind: improvement
severity: low
status: deferred
area: [src/mascot/holidays.ts, src/mascot/nighttime.ts, src/mascot/fullMoon.ts, src/mascot/season.ts, src/mascot/hemisphere.ts]
---

# Let the time hooks re-render when their value changes

`useHolidays`, `useNighttime`, `useTwilight`, `useFullMoonDistance`,
`useSeason` and `useHemisphere` only call their pure function on each
render. Nothing re-renders a component when night falls or a holiday
starts, so the value stays stale until something else renders it.

Nothing calls the hooks; the mascot selection uses the pure functions
(`isHalloween`, `isNighttime`, `getFullMoonDistance`, ...). They are
kept for a later React consumer, for example a plugin UI (decision
2026-09-30). Make them reactive when that consumer exists, so its needs
decide the API (minute resolution, the next switch time, other values).

## Approach discussed

- One shared clock for all hooks: a module-level timer that recomputes
  every value once a minute and serves them through
  `useSyncExternalStore`. React re-renders a component only when its
  value changed.
- Start the timer with the first subscriber and stop it with the last,
  so it costs nothing while no component uses a hook.
- Cost: a few trigonometric evaluations per minute; not measurable.

## Pitfalls

- Sleep and wake: a `setTimeout` aimed at the exact switch time fires
  late after the machine slept. A minute clock corrects itself within a
  minute; recomputing on window focus or `visibilitychange` would make
  it immediate.
- `getActiveHolidays` returns a new array on every call. The snapshot
  has to be cached and only replaced when the contents change, or every
  tick re-renders.
- Tests need fake timers (`vi.useFakeTimers`, `vi.setSystemTime`) and
  `vi.stubEnv("TZ", ...)` for the location, as in the existing tests of
  the pure functions.
