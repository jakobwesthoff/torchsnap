# Replace fixed nighttime window with astronomical sunset/sunrise calculation

## Current state

`src/hooks/useNighttime.ts` uses a hardcoded 19:00–06:00 local time window.
This is adequate for mid-latitudes but wrong at extremes: Stockholm has sunset
at ~22:00 in June and ~15:00 in December. Tropical locations have near-constant
~18:00 sunsets year-round.

## Goal

Compute civil twilight times (sun 6° below horizon) from the user's approximate
location, derived from their system timezone. No network requests, no
permissions — pure math, same approach as the existing lunar phase code.

## Implementation plan

### 1. Solar position calculation

The existing `useFullMoonDistance.ts` already implements the low-precision Meeus
algorithm for sun coordinates (`sunCoords`, `solarMeanAnomaly`,
`eclipticLongitude`). Sunrise/sunset requires:

- **Solar declination** — already computed by `declination(L, 0)` in the
  existing code.
- **Hour angle at a given altitude** — the angle at which the sun crosses a
  specific altitude threshold. For civil twilight (altitude = -6°):

  ```
  cos(H) = (sin(altitude) - sin(lat) * sin(dec)) / (cos(lat) * cos(dec))
  ```

  where `lat` is the observer's latitude, `dec` is solar declination, and `H`
  is the hour angle in radians.

- **Transit time** — solar noon in UTC, from which sunrise/sunset are offset
  by ±H:

  ```
  J_transit = J2000 + 0.0009 + lon/360 + n
  ```

  where `n` is the Julian cycle number and `lon` is observer longitude (west
  negative).

The full calculation (Meeus chapter 15) yields sunrise and sunset times in UTC,
which can be compared to the current local time.

### 2. Timezone-to-coordinates mapping

The `Intl.DateTimeFormat().resolvedOptions().timeZone` API returns the user's
IANA timezone (e.g. `Europe/Berlin`, `America/New_York`). We need to map this
to an approximate (latitude, longitude) pair.

**Option A: Explicit lookup table (~100-150 entries)**

A hand-curated map covering all major IANA zones. Most zones map to their
principal city. This is the most reliable approach.

```typescript
const TIMEZONE_COORDS: Record<string, [lat: number, lon: number]> = {
  "Europe/Berlin":    [52.52,  13.41],
  "America/New_York": [40.71, -74.01],
  "Asia/Tokyo":       [35.68, 139.69],
  // ...
};
```

**Option B: Heuristic from UTC offset + hemisphere**

Less accurate, doesn't distinguish timezones at the same offset (e.g.
`Europe/Stockholm` vs `Africa/Lagos` are both UTC+1 but at 59°N vs 6°N).
Not recommended as the primary approach.

**Recommendation: Option A.** The IANA timezone database has ~450 zones, but
most are aliases or uninhabited regions. A table of ~100-150 entries covers
>99% of users. For unknown zones, fall back to a mid-latitude default (e.g.
48°N, the approximate population-weighted mean for the Northern Hemisphere) —
this gives reasonable twilight times for most people.

### 3. Edge cases

- **Polar regions**: Above the Arctic/Antarctic circle, civil twilight may not
  end (midnight sun) or not begin (polar night). When `cos(H) > 1` or
  `cos(H) < -1`, the sun never reaches the threshold. In these cases:
  - Midnight sun → `isNighttime()` returns `false` (it's never truly dark).
  - Polar night → `isNighttime()` returns `true` (it's always dark).

- **Performance**: The calculation is trivially cheap (a few trig calls), same
  as the lunar phase code. No caching needed.

- **Testing**: The existing pattern of `(now: Date = new Date())` parameters
  makes it easy to test specific dates/times. Add a `coordinates` override
  parameter for testing different latitudes.

### 4. Refactoring opportunity

The solar position functions (`sunCoords`, `solarMeanAnomaly`,
`eclipticLongitude`, `toDays`, Julian date helpers, `declination`,
`rightAscension`) are already in `useFullMoonDistance.ts`. Since the sunset
calculation needs the same functions, extract them into a shared
`solarPosition.ts` or `astronomy.ts` module that both `useFullMoonDistance.ts`
and `useNighttime.ts` import from.

### 5. Civil twilight vs sunset

Three common thresholds:

| Threshold         | Sun altitude | Description                          |
|-------------------|-------------|--------------------------------------|
| Sunset/sunrise    | 0° (−0.83° with refraction) | Sun touching horizon |
| Civil twilight    | −6°         | Sky noticeably dark, street lights on |
| Nautical twilight | −12°        | Horizon barely visible                |

**Civil twilight (−6°)** is the right choice for "is it dark enough for a
werewolf?" — it's when most people would say "it's dark outside."
