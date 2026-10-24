# 71. Reserve no shadow pad above the launcher card

Date: 2026-10-24

## Status

Accepted

Amends [20. Size launcher webview to content and shrink on hide](0020-size-launcher-webview-to-content-and-shrink-on-hide.md)

## Context

ADR 20 sizes the launcher window to its largest card plus a pad for the
card's box-shadow on every side and headroom for the mascot above the
card. In `src/launcher/layout.ts` the pad is `SHADOW_PADDING` (64 px)
and the headroom `MASCOT_HEADROOM` (181.9 px). The card sat
`SHADOW_PADDING + MASCOT_HEADROOM` = 245.9 px below the window's top,
and the window was 64 + 181.9 + card + 64 high: 859.9 logical pixels
for the largest card (about 550 px), and 680 + 2 × 64 = 808 wide.

The mascot headroom is the full image box of the centre mascot. The
192 px image is placed so that its ground anchor lies 10.1 px below the
card's top, so its top lies `192 × y − 10.1` above the card for an
anchor at height `y`, a fraction of the image. The mascot data allows
`y` up to 1.0, which gives 181.9 px. The 626 mascots in the set have
`y` from 0.8179 to 0.9981: their image tops lie 146.9 to 181.5 px above
the card, median 169.3, 90th percentile 178.0. The 96 px sidekick
reaches at most 84.8 px above the card. Mirrored, the sidekick
overhangs the card's right edge by up to 20.2 px.

The card's shadow has four layers: a 1 px ring, an inset layer,
`0 4px 16px` and `0 16px 48px`. A CSS blur radius B is a Gaussian with
a standard deviation of B / 2. Its tail ends three deviations, 1.5 × B,
past the shadow's edge, which the y offset moves down. The largest
layer's alpha drops below one 8-bit step (1/255) at about two
deviations.

| Layer | Above the card | Sides | Below |
|---|---|---|---|
| `0 0 0 1px` ring | 1 px | 1 px | 1 px |
| `0 4px 16px`, three deviations | 20 px | 24 px | 28 px |
| `0 16px 48px`, three deviations | 56 px | 72 px | 88 px |
| `0 16px 48px`, below one 8-bit step | about 31 px | about 47 px | about 63 px |

Above the card, every layer ends inside the mascot headroom, so nothing
was drawn in the 64 px pad above it.

## Decision

The card's top lies `MASCOT_HEADROOM` below the window's top, with no
shadow pad above it. The window is `MASCOT_HEADROOM + card +
SHADOW_PADDING` high and `card + 2 × SHADOW_PADDING` wide: 808 × 795.9
logical pixels for the largest card.

The headroom stays the full image box for any anchor `y` up to 1.0, not
the mascots' visible pixels, so every image a mascot can have fits,
including mascots added later.

The sides and the bottom keep the 64 px pad. It holds the shadow past
the point where the largest layer drops below one 8-bit step, though
not to its three-deviation tail, and on the right the sidekick's
overhang.

The constants and `launcherWindowLayout`, which computes the window
size from the measured card, live in `src/launcher/layout.ts`. The
frontend sends the result with `launcher_set_layout`; the backend holds
no layout constants.

## Consequences

- The launcher window is 64 px shorter on every platform.
- On macOS and Windows the backend places the window so that the
  card's top lies at a quarter of the monitor's height. The card stays
  where it was; the window's top moves down 64 px.
- On GNOME, Mutter places the window itself and the card follows the
  window: 64 px higher where Mutter puts the window at the top of the
  work area, 32 px higher where it centres the window.
- A centre mascot anchored at `y` = 1.0 reaches the window's top
  exactly.
- `src/launcher/layout.test.ts` fails when a mascot in the data, or an
  anchor at `y` = 1.0, would reach above the window's top, and when a
  shadow layer's three-deviation reach above the card exceeds the
  card's top offset.
