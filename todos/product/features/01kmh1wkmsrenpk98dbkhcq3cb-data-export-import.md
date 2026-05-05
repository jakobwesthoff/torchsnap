# Data export and import

Users switching machines, reinstalling, or backing up should be
able to export and restore their entire torchsnap configuration.

## What needs to be exportable

- **Settings**: Global shortcut, theme preference, search engine
  config, all user preferences from `settings.json`
- **Frecency data**: Usage history that drives result ranking.
  Without this, a fresh install feels like starting over.
- **Gadget settings**: Per-gadget configuration (namespaced in
  the settings store or separate files)
- **Pinned items / favorites**: If we implement pinning
- **Clipboard history**: If the clipboard manager gadget stores
  history locally
- **Custom keybinds**: User-configured shortcut overrides
- **Installed gadgets**: List of installed gadgets (paths or
  registry references) so they can be re-fetched on import
- **Custom themes**: User-installed theme files

## Export format

- Single archive file (`.torchsnap-backup` or `.zip`)
- Contains JSON/TOML config files in a known directory structure
- Version field in the archive for migration compatibility
- Human-readable where possible so users can inspect/edit

## Import behavior

- Validate archive structure and version before applying
- Merge vs replace strategy: should importing overwrite all
  settings or merge with existing? Probably offer both.
- Handle missing gadgets gracefully (import settings but warn
  that gadget X is not installed)
- Handle version differences (older export → newer app) with
  migration logic

## UX

- Settings panel: "Export Data" and "Import Data" buttons
- Built-in launcher commands: "Export settings", "Import settings"
- Drag-and-drop import of `.torchsnap-backup` files?
- Confirmation dialog before import overwrites existing data

## Privacy

- Export should warn if it contains sensitive data (API keys,
  clipboard history with passwords)
- Option to exclude specific categories from export
- Never include system credentials or keychain data
