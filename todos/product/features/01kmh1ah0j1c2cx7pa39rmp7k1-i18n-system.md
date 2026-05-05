# Internationalization (i18n) system

Evaluate and integrate an i18n framework early, even though the
initial implementation will be English-only. Retrofitting i18n is
significantly harder than baking in the infrastructure from the
start.

## What needs to be localized

- Built-in command names ("Quit", "Settings", "Toggle Theme")
- Settings panel labels and descriptions
- Placeholder text ("Type to search")
- Error messages and status text
- Gadget-provided strings (gadget names, result labels, action
  names)
- Keyboard shortcut display (Cmd vs Ctrl, platform-specific names)

## Library candidates (frontend)

- **i18next** + **react-i18next**: Industry standard. Large
  ecosystem, pluralization, interpolation, namespaces. Might be
  overkill for a desktop app.
- **FormatJS / react-intl**: ICU MessageFormat based. Good for
  plurals and complex formatting. Heavier.
- **Lingui**: Lightweight, compile-time extraction, good DX.
  Macro-based API (`t`"Hello"`) feels natural. Smaller bundle.
- **typesafe-i18n**: TypeScript-first, compile-time checks, very
  small runtime. Good fit for a TS-heavy project.
- **Simple key-value JSON**: Roll our own minimal system with
  typed key lookups. No library overhead but no pluralization
  or formatting built in.

`Lingui` or `typesafe-i18n` seem like the best fit — lightweight,
good TypeScript integration, and small enough for a desktop app.

## Rust-side strings

- Tray menu item labels, notification text, and error messages
  originate on the Rust side
- Options: `rust-i18n` crate, `fluent` (Mozilla's i18n system),
  or pass locale to Rust and have it select from a simple map
- The Rust side has fewer strings, so a simpler approach may
  suffice

## Gadget i18n

- Gadgets should be able to provide translations for their own
  strings
- Gadget manifest could declare supported locales
- Host provides the current locale to gadgets so they can select
  the right translation
- Fallback to English if a translation is missing

## What needs to happen

1. Evaluate library candidates — build a small proof of concept
   with 2–3 options
2. Pick one and integrate into the project
3. Extract all hardcoded English strings into translation keys
4. Set up the translation file structure (e.g. `locales/en.json`)
5. Document the i18n workflow for future contributors
6. Define how gadgets provide and access translations
