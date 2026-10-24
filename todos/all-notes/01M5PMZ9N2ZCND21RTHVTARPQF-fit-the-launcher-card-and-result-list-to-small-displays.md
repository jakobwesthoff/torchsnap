---
title: "Fit the launcher card and result list to small displays"
kind: improvement
component: frontend
origin: discussion
tags: [ux]
---
# Fit the launcher card and result list to small displays

On displays less than about 900 logical pixels high the launcher's result list is cut off at the bottom: the window is a fixed 808 x 860 logical pixels, and the page places the card at a fixed offset with a fixed maximum content height, whatever room the window really has. Measured on GNOME (Linux testbed, 2026-10-24); the layout code is shared, so it applies on every platform, though Mac displays rarely have so few logical pixels. The maintainer decided on 2026-10-24 to keep the fixed-size window on Linux and to adapt the page later, not now.

## How the launcher is laid out

- The window is `SHADOW_PADDING + MASCOT_HEADROOM + card + SHADOW_PADDING` high (`src/launcher/main.tsx`, around line 66; constants in `src/launcher/layout.ts`), measured once for the largest card: 64 + 181.9 + about 550 + 64, so 860 logical pixels, and 680 + 2 x 64 = 808 wide. It is the same empty and with results (ADR 0020).
- The card's top sits `CARD_TOP_OFFSET` = 245.9 px below the window top. The card is 61 px high empty, 449 px with the emoji grid (`:fire`), about 550 px with a full list.
- The content area is at most 448 px high (`max-h-[448px]` in `src/launcher/Launcher.tsx`). A result row is 56 px (`py-2.5` around a 20 px title line and a 16 px subtitle line, `src/launcher/ResultRow.tsx`), so 448 px hold 8 rows; screenshots confirm 56 px row spacing.

## Measurements (GNOME, Raspberry Pi 5 testbed, headless sessions, 2026-10-24)

Work area is the monitor minus GNOME's 35 px top bar; "cut" is how much of the card lies outside the visible window.

| Monitor (logical) | Empty | `:fire` grid | 8-row list | What Mutter does |
|---|---|---|---|---|
| 1920 x 1080 | ok | ok | ok | centres the window |
| 1440 x 900 | ok | ok | ok | centres the window |
| 1366 x 768 | ok | ok | cut 63 px | shortens the window to the work area |
| 1280 x 800 | ok | ok | cut 31 px | shortens the window |
| 1280 x 720 (also 2560 x 1440 at scale 2) | ok | cut 10 px | cut 111 px | shortens the window |
| 1024 x 768 | ok | ok | cut 63 px | maximizes the window |
| 960 x 540 (1920 x 1080 at scale 2) | ok | cut 190 px | cut 291 px, 3 to 4 of 8 rows visible | maximizes the window |

The search field and the empty launcher are fully visible everywhere; the window never ends up under the top bar or on another monitor. Further findings:

- When the window is taller than the work area, Mutter shortens it on every show after the first and puts it at the top of the work area; the card stays 245.9 px below that top, so its lower part is outside.
- When the window covers about 75 % or more of the work area in both directions, Mutter maximizes it; the page still places the card 245.9 px below the top with the same maximum height.
- On the first show after the app starts, Mutter does not shorten the window: on work areas under 860 px it sticks out below the screen (1280 x 720: window from y 77 to 937 on a 720 px monitor). From the second show on the shortening applies. Adapting the page does not cover this case, as the window itself is still 860 high then.
- Two monitors (1280 x 720 and 1920 x 1080) with the pointer on the small one: the window stays there at full size and is cut like on a single 1280 x 720 monitor.
- On small displays the search field sits at 35 to 43 % of the screen height instead of about 25 %.

## What to change

- Let the page follow the real window height: move the card up when there is little room and cap the content area at the visible height, so the list scrolls instead of being cut. This also fixes the maximized case.
- Or, the maintainer's idea, show fewer result rows when the room is small, so the launcher still works on small displays; the goal is to fit 1366 x 768 completely.
- Rough budget for 1366 x 768 on GNOME: the work area is 733 px; minus 64 + 181.9 above the card and 64 below, the card can be about 423 px high, which leaves about 321 px of content, five full rows. Less space above the card gives more rows; a separate todo measures how much is needed.
- Decide what happens on the first show after app start on GNOME (a smaller initial window, or a resize after Mutter's placement).
