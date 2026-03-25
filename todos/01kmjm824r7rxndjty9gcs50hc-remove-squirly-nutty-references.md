# Remove squirly/nutty references from codebase

Comments and documentation throughout the codebase reference squirly
and nutty as the origin of ported patterns (keybinding engine, theme
toggle, mouse hover suppression, etc.). Once torchsnap has diverged
enough that these references are no longer useful context, clean them
up so the codebase stands on its own.

## Scope

- Code comments mentioning nutty, acornkit, or squirly
- Todo files referencing nutty as a source
- ADR context sections citing nutty patterns
