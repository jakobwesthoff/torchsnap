---
kind: feature
status: needs-discussion
area: [src/welcome/LauncherPreview.tsx, src/welcome/WelcomeWindow.tsx]
tags: [ux]
---

# Animate the launcher preview in the welcome window

The first page of the welcome window shows a still picture of the
launcher (`src/welcome/LauncherPreview.tsx`): the query "gho", three
results with the matched letters highlighted, Snappy on top. It is the
static launcher preview from the torchsnap.app hero, rebuilt from the
app's components, and it fades in with the page. The maintainer wants
it animated later; for 0.12.0 the still picture is enough (decided
2026-09-24 after testing the first welcome window).

## Starting point

torchsnap-web had an animated version in its hero and removed it in
commit `1e02fd0` ("Drop the launcher React island, render the hero
statically") because it distracted there. Its files, retrievable with
`git show 1e02fd0^:<path>` in torchsnap-web:

- `web/src/launcher/useScriptedDemo.ts`: types each query letter by
  letter (80 to 140 ms per letter), waits 3 s, clears, moves on;
  keeps the first frame for `prefers-reduced-motion: reduce`.
- `web/src/launcher/scenarios.ts`: four queries, `gho`, `zen`, a URL
  and a bang search (`pulp fiction !g`).
- `web/src/launcher/candidates.ts`, `match.ts`: the canned results and
  a fuzzy matcher for them.
- `web/src/launcher/DemoLauncher.tsx`: the card.

## To decide

- Storyboard: which queries, how many, and whether the results change
  while typing or only at the end.
- Whether to reuse the web demo code or build on the app's real
  launcher components.
- Reduced motion keeps the still picture.
