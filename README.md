# Torchsnap

**Light. Find. Launch.**

Strike a key, light the way. A cross-platform launcher to find, launch, and automate anything across every system.

## Installing plugins

Third-party plugins ship as `.torchsnap` files — single-file
archives you can hand-share. To install one:

1. Open **Settings → Plugins**.
2. Click **Choose a file…** and pick the `.torchsnap`, or drop the
   file onto the drop zone.
3. When prompted, click **Restart now**. The plugin is active after
   restart.

Installed user plugins live under
`<app_data_dir>/plugins/<id>.torchsnap` and their host-managed state
(SQLite databases, caches) under
`<app_data_dir>/plugin-home/<id>/`. Uninstalling a plugin from the
Plugins panel removes both.

Plugins bundled with the app (like the calculator) cannot be
uninstalled — they are upgraded along with Torchsnap itself.

## Developing plugins

See [`docs/api/plugin-development.md`](docs/api/plugin-development.md)
for the plugin author guide: the WIT interface, manifest format,
discovery rules, and packaging instructions.
