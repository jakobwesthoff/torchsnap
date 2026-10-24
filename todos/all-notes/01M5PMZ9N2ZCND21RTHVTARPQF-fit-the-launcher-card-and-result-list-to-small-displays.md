---
title: "Fit the launcher card and result list to small displays"
kind: improvement
component: frontend
status: needs-discussion
origin: discussion
tags: [ux]
---
# Fit the launcher card and result list to small displays

On displays less than about 900 logical pixels high the launcher's result list is cut off at the bottom: the window has a fixed size, and the page places the card at a fixed offset with a fixed maximum content height, whatever room the window really has. Measured on GNOME (Linux testbed, 2026-10-24); the layout code is shared, so it applies on every platform, though Mac displays rarely have so few logical pixels. The direction is decided (cap the content to the room and scroll the rest); how the available height reaches the launcher and the gadget views is open and needs a design discussion before anyone builds it.

## Decisions

- 2026-10-24: keep the fixed-size window on Linux (no maximized window) and adapt the page instead.
- 2026-10-24: when not all rows fit, cap the list to the room there is and let the rest scroll, on all platforms. This holds for gadget views as well, such as the emoji grid. Rejected: showing fewer results with no way to reach the others.
- 2026-10-24: not now. The maintainer: it "hört sich nämlich gar nicht so unkompliziert an, weil es auch Gadgets und die Art, wie die Rendern und so weiter betrifft, da müssen wir uns definitiv über die API Gedanken machen und wie wir diese Informationen, die da zur Verfügung steht, überhaupt in die UI transportieren und dann auch zu den Gadget-Renderern wie Emoji und so weiter."
- Related work decided the same day, separate from this todo: the 64 px shadow pad above the card is dropped on every platform (the card's upward shadow fits inside the mascot headroom), and whether the card is placed at 25 % of the usable screen area instead of the full monitor is being researched (adopted only if getting the work area is simple). Both change the numbers below; recompute them before building this.

## Open questions

- How does the page learn how much room it has: the window's inner height (`window.innerHeight`, which on GNOME follows Mutter's shortening and maximizing), the monitor's work area from the Rust side, or both? On GNOME's first show after app start the window is still at full size and sticks out below the screen, so the window height alone is wrong there.
- How does that height reach the content: a CSS variable replacing `max-h-[448px]`, a value in the launcher state, a prop?
- What do gadget views get: a maximum height through the view props or the gadget SDK, or only a scrolling container around them? Which views assume today that 448 px are available (the emoji grid is 449 px with search bar, measured on GNOME), and how does keyboard selection keep the selected cell or row visible when a view scrolls?
- Does the measure dummy (state 1 in `Launcher.tsx`, `h-[448px]`) keep sizing the window for the full 8 rows?

## How the launcher is laid out (main, 2026-10-24)

- The window is `SHADOW_PADDING + MASCOT_HEADROOM + card + SHADOW_PADDING` high (`src/launcher/main.tsx`, around line 66; constants in `src/launcher/layout.ts`), measured once for the largest card: 64 + 181.9 + about 550 + 64, so 860 logical pixels, and 680 + 2 x 64 = 808 wide. It is the same empty and with results (ADR 0020).
- The card's top sits `CARD_TOP_OFFSET` = 245.9 px below the window top. The card is 61 px high empty, 449 px with the emoji grid (`:fire`), about 550 px with a full list.
- The content area is at most 448 px high (`max-h-[448px]` on the content wrapper in `src/launcher/Launcher.tsx`, around line 875; the comment above the content states says all views must fit within it). It holds the result list, inline gadget views above the list, and full gadget views (`PluginViewContainer`). A result row is 56 px (`py-2.5` around a 20 px title line and a 16 px subtitle line, `src/launcher/ResultRow.tsx`), so 448 px hold 8 rows; screenshots confirm 56 px row spacing.
- The Rust side places the card top at 25 % of the full monitor height on every platform (`src-tauri/src/monitor_placement.rs`, around lines 157-166); no code subtracts a menu bar, dock, top bar or task bar.

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
- On the first show after the app starts, Mutter does not shorten the window: on work areas under 860 px it sticks out below the screen (1280 x 720: window from y 77 to 937 on a 720 px monitor). From the second show on the shortening applies.
- Two monitors (1280 x 720 and 1920 x 1080) with the pointer on the small one: the window stays there at full size and is cut like on a single 1280 x 720 monitor.
- On small displays the search field sits at 35 to 43 % of the screen height instead of about 25 %.

## Rows that fit (computed, not measured)

From a layout model checked against the GNOME measurements above (analysis of 2026-10-24): full rows of 8 that fit today on GNOME are 6 at 1024 x 768, 6 at 1280 x 720, 7 at 1280 x 800 and 6 at 1366 x 768; without the top shadow pad 8, 7, 8 and 8 (at 1366 x 768 the card then ends 1 px above the screen edge, part of its bottom shadow outside). On macOS, with the card at 25 % of the full monitor, 8 rows fit at every common size except 7 at 1280 x 720. So after the top pad goes, capping and scrolling matters mainly below about 800 logical pixels, for scaled 4K setups such as 1920 x 1080 at scale 2, and for gadget views taller than the list.

## Tests to plan

Cover each room size the cap can hit (full 8 rows, a partial row, less than the empty card), keyboard selection scrolling the selected row and grid cell into view, an inline view above a capped list, a full gadget view under the cap, and the measure dummy still sizing the window.
