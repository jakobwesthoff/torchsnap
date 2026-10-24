---
title: "Discover and open system settings panes on Linux"
kind: feature
component: platform
origin: implementation
tags: [linux]
---
# Discover and open system settings panes on Linux

The system-preferences gadget lists and opens the system's settings panes on macOS. On Linux it registers but shows nothing: `FallbackSettingsDiscovery::discover` returns an empty list (`src-tauri/src/platform/fallback/settings_discovery.rs`, with a TODO to scan GNOME and KDE settings `.desktop` files), and `FallbackSettingsDiscovery::pane_url` bails with "not implemented on this platform", so opening a pane fails. Found on 2026-10-24 while analysing the warnings of the Linux build.

## Proposal

The platform seam is in place: `SettingsDiscovery` has `discover`, `icon(&PlatformPaneIcon)` and `pane_url(pane_id) -> String`, each `SettingsPane` carries a `PlatformPaneIcon`, and the gadget opens the returned URL through its opener capability without any `cfg`. What remains is the Linux implementation, GNOME first (the order of the Linux support plan: GNOME, labwc, X11, KDE):

- A `platform/linux` module (or a Linux part of `fallback/`) with a discovery that reads the settings panels' `.desktop` files. `SettingsPane::id` is documented as the `.desktop` file path on Linux already.
- A `LinuxPaneIcon` for `PlatformPaneIcon` that holds the `Icon=` value of the `.desktop` file, which `icon` resolves through the icon theme. `FallbackPaneIcon` is `()` today.
- Linux `pane_url` results. It returns the URL the gadget passes to its opener capability. If a Linux pane opens through something that is not a URL (a command, a `.desktop` id), the return type becomes an enum of the target kinds, and the gadget gets the matching capability request in `cap_requests`. The enum is not there now because a variant only macOS builds is reported as dead code in the Linux build.

## Open questions

- Which `.desktop` files GNOME 48 ships for its panels and how they launch (`gnome-control-center <panel>`, `gio launch`, or the opener capability); not checked on the testbed.
- KDE's counterpart (`systemsettings`, `kcmshell6`) when KDE support starts.
