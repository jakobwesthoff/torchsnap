---
kind: feature
status: open
tags: [ux]
---

# User-provided themes / skins

The current theme system uses hardcoded CSS custom properties for
light and dark mode. Users should be able to install and switch
between alternative visual themes beyond the built-in light/dark.

## What needs to be designed

- **Theme format**: What does a user-provided theme consist of?
  - A CSS file overriding the semantic token custom properties?
  - A JSON/TOML manifest with color values?
  - Support for custom fonts, border radii, spacing?
  - Can themes override component-level styles or only tokens?
- **Theme discovery**: Where do themes live on disk?
  - A `themes/` directory in the app data folder?
  - Each theme as a directory with a manifest + assets?
- **Theme switching**: How does the user select a theme?
  - Settings panel dropdown
  - Launcher command ("Switch theme")
  - Live preview before committing?
- **Theme authoring**: How easy is it to create a theme?
  - Provide a template/starter theme
  - Document all available tokens
  - Hot-reload during development?
- **Scope of customization**:
  - Colors only (safest, easiest)?
  - Colors + typography + spacing (more expressive, harder to
    guarantee readability)?
  - Full CSS override (maximum power, risk of breaking layout)?
- **Compatibility**: How do themes interact with gadgets?
  - Gadgets should use the same semantic tokens
  - A theme that changes tokens automatically affects gadgets
  - What if a gadget defines its own tokens?

## Prior art

- **Alfred**: Appearance preferences with importable `.alfredappearance` files
- **Raycast**: Built-in theme picker, community themes as JSON
- **VS Code**: Full theme extensions with token color customization
- **iTerm2**: Color profiles as `.itermcolors` plists

## Relationship to current system

The existing `[data-theme]` attribute + CSS custom properties
architecture is already a good foundation. User themes would
essentially be additional `[data-theme="<name>"]` blocks loaded
from external CSS files. The ThemeProvider would need to support
arbitrary theme names beyond just "light"/"dark"/"system".
