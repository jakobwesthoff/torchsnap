---
kind: investigation
status: open
tags: [performance, testing]
---

# React performance audit: stable references and memoization

Audit the frontend for missing or incorrect memoization, unstable
references causing unnecessary re-renders, and opportunities for
React.memo / useMemo / useCallback improvements.

## Areas to check

- Launcher.tsx: callback stability (handleExecute, handleGoBack,
  handleGadgetExecute), derived values, props passed to children
- EmojiGrid.tsx: GridCell should likely be React.memo'd, footer
  effect dependencies, keybinding array stability
- ResultList / ResultRow: similar memo audit
- useWindowedList / useWindowedGrid: ref vs state usage, stable
  callback references
- useKeyboardNavigation: binding array recreation frequency
- LauncherFooter: FooterState object identity on each render

## Goal

Ensure fast keyboard navigation and typing don't trigger unnecessary
React reconciliation work. Profile with React DevTools Profiler to
identify actual bottlenecks before optimizing.
