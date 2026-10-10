---
title: "Audit the React frontend for unstable references and missing memoization"
kind: investigation
component: frontend
tags: [performance, testing]
---
# Audit the React frontend for unstable references and missing memoization

Audit the frontend for missing or incorrect memoization, unstable references
causing unnecessary re-renders, and opportunities for React.memo / useMemo /
useCallback improvements.

## Areas to check

- `src/launcher/Launcher.tsx`: callback stability (handleExecute, handleGoBack,
  handleGadgetExecute), derived values, props passed to children
- `gadgets/emoji-picker/frontend/src/EmojiGrid.tsx`: GridCell should likely be
  React.memo'd, footer effect dependencies, keybinding array stability
- `src/launcher/ResultList.tsx` and `src/launcher/ResultRow.tsx`: similar memo
  audit
- `src/launcher/hooks/useWindowedList.ts` and
  `gadgets/emoji-picker/frontend/src/useWindowedGrid.ts`: ref vs state usage,
  stable callback references
- `src/launcher/hooks/useKeyboardNavigation.ts`: binding array recreation
  frequency
- `src/launcher/LauncherFooter.tsx`: FooterState object identity on each render

## Goal

Ensure fast keyboard navigation and typing don't trigger unnecessary React
reconciliation work. Profile with React DevTools Profiler to identify actual
bottlenecks before optimizing.
