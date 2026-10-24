---
title: "Measure the smallest space the launcher needs above its card"
kind: investigation
component: frontend
origin: discussion
tags: [ux]
---
# Measure the smallest space the launcher needs above its card

The launcher window reserves 245.9 px above the card for the mascot and the card's shadow. That value was chosen generously, and every pixel of it is missing on small displays, where the result list gets cut off. Work out the real minimum from the mascot set and the shadows, so the window has no unused space above the card. Raised by the maintainer on 2026-10-24 after the small-display measurements on GNOME.

## How the space is made up

`CARD_TOP_OFFSET = SHADOW_PADDING + MASCOT_HEADROOM` (`src/launcher/layout.ts`):

- `SHADOW_PADDING` = 64: the card's largest shadow is `0 16px 48px`; 48 px blur plus 16 px y-offset is 64 px below the card, and the file uses 64 on all sides. Above the card the shadow reaches only about 48 - 16 = 32 px. That region lies inside the mascot headroom anyway, so adding the full 64 on top of the headroom may count the same pixels twice.
- `MASCOT_HEADROOM` = `MASCOT_SIZE.center - CENTER_FEET_Y` = 192 - 10.1 = 181.9: the worst case of a ground anchor on the image's bottom edge.

## What the mascot data already says (lead, 2026-10-24)

From `groundAnchor.y` in `src/derived/mascots.json` (626 mascots), the top of the 192 px image lies `y * 192 - 10.1` px above the card's top edge: minimum 146.9, median 169.3, 90th percentile 178.0, maximum 181.5. So the image top of the worst mascot almost reaches the full headroom. Not measured: how many transparent rows each image has at its top, which would lower the real visible height, and whether the mascot carries a drop shadow or other effect that extends beyond its image.

## What to measure

- The highest visible pixel of every mascot above the card top, in center and sidekick mode, from the images' alpha channel (`public/images/mascot/*.webp`) and the placement in `src/launcher/placement.ts`.
- How far the card's shadow and any mascot effect really extend above the card.
- The resulting minimum offset, and whether the top and bottom shadow paddings can differ.

Then lower `CARD_TOP_OFFSET` (and so the window height) to that minimum. Small displays gain the difference as room for the result list.

## Relations

- Relates to: [Fit the launcher card and result list to small displays](01M5PMZ9N2ZCND21RTHVTARPQF-fit-the-launcher-card-and-result-list-to-small-displays.md)
