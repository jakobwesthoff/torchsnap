---
kind: feature
status: open
tags: [accessibility, ux]
---

# Accessibility

The launcher should be usable by everyone, including users who
rely on screen readers, keyboard-only navigation, high-contrast
themes, or reduced motion.

## Areas to address

### Screen reader support
- Result list needs proper ARIA roles: `role="listbox"` on the
  container, `role="option"` on each result row, `aria-selected`
  on the active item
- Search input: `role="combobox"`, `aria-expanded`,
  `aria-activedescendant` pointing to the selected result
- Live region (`aria-live="polite"`) to announce result count
  changes ("5 results" / "No results")
- Action hints should be announced (not just visual)

### Keyboard navigation
- Already keyboard-first by design — verify all actions are
  reachable without mouse
- Focus indicators must be visible (not just relying on the
  selection highlight)
- Tab order should be logical in the settings panel
- Escape behavior should be consistent everywhere

### Reduced motion
- Respect `prefers-reduced-motion: reduce`
- Disable or simplify: transition animations, opacity fades,
  any future entrance/exit animations
- The launcher show/hide should still work, just without
  animation

### High contrast
- Test with macOS "Increase contrast" setting
- Ensure all text meets WCAG AA contrast ratios (4.5:1 for
  normal text, 3:1 for large text) in both light and dark themes
- Border visibility: some of our borders use low-opacity values
  that may disappear in high contrast mode
- Consider a dedicated high-contrast theme variant

### Color independence
- Don't rely on color alone to convey information (e.g. error
  states should have icons or text, not just red color)
- Accent color (orange) on surface backgrounds — verify contrast

## When to do this

- ARIA roles on the result list: as soon as the result display
  is built
- Reduced motion: when adding any animations
- Contrast audit: after the color palette is finalized
- High-contrast theme: after the theming system supports user
  themes
- Gets exponentially harder to retrofit — better to build in
  incrementally as each feature is added
