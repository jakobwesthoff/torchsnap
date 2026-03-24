# Decide: General launcher UX concepts

Open architectural decision about the overall user experience
model for the launcher.

## Questions to resolve

- **Query lifecycle**: Does the query persist after executing an
  action (like a terminal) or reset on every show (like Spotlight)?
  What about when the user dismisses without acting?
- **Navigation model**: Strictly keyboard-driven? Mouse as
  secondary? Touch support for tablets?
- **Result ordering**: How are results from different plugins
  ranked? Recency? Frequency of use? Plugin priority? ML-based
  relevance scoring?
- **Prefix vs universal search**: Do plugins require prefixes
  (`= ` for calc, `: ` for emoji) or does everything fuzzy-match
  against one universal query? Or both — universal by default,
  prefix to narrow?
- **History**: Should the launcher remember past queries and
  results? Show recent actions? Frecency-based suggestion?
- **Pinning / favorites**: Can users pin items to always appear at
  the top? Favorites list when the query is empty?
- **Animated transitions**: Should results animate in/out? Card
  entrance animation? Or instant for speed perception?
- **Window size**: Fixed height? Dynamic height based on result
  count? Maximum height with scroll? Does the card grow/shrink
  as results change?
- **Multiple monitors**: Always appear on the monitor with the
  cursor (current behavior)? Or remember last position? Follow
  focus?
- **Accessibility**: Screen reader support (ARIA roles on result
  list)? High contrast theme? Reduced motion support?
- **Onboarding**: First-run experience? Tutorial? Shortcut hint
  overlay?

## UX principles to establish

- **Speed over features**: Every interaction should feel instant.
  If it can't be instant, show progress. Never block the input.
- **Keyboard-first**: The mouse should work but never be required.
  Every action reachable via keyboard.
- **Minimal chrome**: The launcher should feel like it's part of
  the OS, not a separate application. No unnecessary UI elements.
- **Progressive disclosure**: Simple for basic use, powerful
  features discoverable through exploration (prefixes, modifier
  keys, action palette).
- **Consistent behavior**: Same patterns across all plugins.
  Enter always executes. Escape always dismisses. Arrow keys
  always navigate.
