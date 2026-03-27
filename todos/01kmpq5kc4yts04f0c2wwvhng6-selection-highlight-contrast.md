# Increase selection highlight contrast globally

The selected row background (`bg-accent/10`) is too subtle in dark
mode. This affects every selection in the app — the main launcher
result list, clipboard entry list, emoji grid, and any future plugin
with selectable rows.

## What to do

- Introduce a dedicated design token for selection background
  (e.g. `bg-selection` / `--color-selection`) instead of ad-hoc
  `bg-accent/10` scattered across components.
- Tune the token per color scheme: dark mode needs noticeably more
  contrast, light mode may already be fine.
- Replace all current `bg-accent/10` selection usages with the new
  token in one pass.
