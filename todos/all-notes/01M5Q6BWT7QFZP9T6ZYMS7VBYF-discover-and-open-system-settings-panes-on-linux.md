---
title: "Discover and open system settings panes on Linux"
kind: feature
component: platform
origin: implementation
tags: [linux]
---
# Discover and open system settings panes on Linux

The system-preferences gadget lists and opens the system's settings panes on macOS. On Linux it registers but shows nothing: `FallbackSettingsDiscovery::discover` returns an empty list (`src-tauri/src/platform/fallback/settings_discovery.rs`, with a TODO to scan GNOME and KDE settings `.desktop` files), and `execute` in `src-tauri/src/gadgets/system_preferences.rs` bails with "not implemented on this platform". Found on 2026-10-24 while analysing the warnings of the Linux build.

## Proposal

- Add opening to the `SettingsDiscovery` trait next to `discover` and `icon`, so the gadget's `execute` calls the platform instead of a `cfg` block that names `MacosSettingsDiscovery::pane_url` directly. The fallback bails inside the platform module.
- Move `bundle_path` out of the shared `SettingsPane` into `MacosSettingsDiscovery`: it is read only to extract the `.appex` icon on macOS.
- GNOME first (the order of the Linux support plan: GNOME, labwc, X11, KDE): discover the settings panels from their `.desktop` files and open them by `.desktop` id. `SettingsPane::id` is documented as the `.desktop` file path on Linux already.

## Open questions

- Which `.desktop` files GNOME 48 ships for its panels and how they launch (`gnome-control-center <panel>`, `gio launch`, or the opener capability); not checked on the testbed.
- KDE's counterpart (`systemsettings`, `kcmshell6`) when KDE support starts.
