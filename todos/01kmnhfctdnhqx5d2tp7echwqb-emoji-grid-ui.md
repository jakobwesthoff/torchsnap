# Emoji grid UI implementation

First plugin custom UI, test case for ADR 0013.

## Visual design

- **Layout**: 10 columns × 5 visible rows. Each cell 68×68px
  (680px card width ÷ 10). 50 emoji visible at once.
- **Cell content**: Emoji glyph only, `text-3xl` (~32px), centered.
  No text in cell.
- **Selection**: `bg-accent/10` fill, `border border-accent/30`,
  `rounded-lg`. Hover = selection (gated by `mouseActiveRef`).
  No separate hover state.
- **Partial rows**: Left-aligned, empty cells are empty.
- **No category headers** for now. Flat list in both filtered and
  unfiltered views.
- **Footer**: Primary hint `↵ Copy to Clipboard`. Shows selected
  emoji's shortcode (`:rocket:`) and label ("rocket") as contextual
  info.
- **Windowing**: Virtual window like the list — only visible 5 rows
  rendered. Keyboard/mouse-wheel shifts the window.

## Frontend structure

Plugin code lives in `src/plugins/emoji/`, mirroring the backend
`src-tauri/src/plugins/emoji.rs` structure.

## Keyboard navigation

- ArrowLeft/Right: move between cells (±1)
- ArrowUp/Down: move between rows (±columns)
- Enter: copy selected emoji to clipboard
- Escape: deactivate plugin (clear prefix / goBack)
- PageUp/PageDown: move by visible rows

## Related

- ADR 0013: plugin custom UI architecture
- Depends on: FooterState refactoring, host keybinding conditionalization,
  @torchsnap/* path aliases
