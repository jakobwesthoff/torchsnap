# Random mascot variants on launcher invocation

Generate additional Snappy owl mascot variants (different poses,
expressions, accessories, seasonal variants) and randomly pick one
each time the launcher is shown.

## Reference

Squirly uses a `Mascot` component in acornkit that accepts a list
of variant image paths and randomly selects one on each mount/show.

## What needs to happen

1. Generate additional mascot source images at 1024px
   (different poses/expressions of Snappy)
2. Run them through the asset pipeline (`just asset-mascots`) to
   produce all size/format variants
3. Create a `Mascot` component (or hook) that:
   - Accepts a list of available variants
   - Randomly selects one on each launcher show (not mount —
     the launcher window persists)
   - Uses the `tauri://focus` event or a prop signal to trigger
     re-randomization
4. Replace the inline `<img>` in `Launcher.tsx` with the component
