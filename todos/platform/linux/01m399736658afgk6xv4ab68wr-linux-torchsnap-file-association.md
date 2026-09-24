---
kind: feature
status: blocked
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Linux: register `.torchsnap` for deb and rpm packages

Blocked on Linux packaging. There is no deb, rpm or AppImage config
yet; `torchsnap-docs/src/content/docs/start/installation.mdx` lists
them as planned.

## Goal

Opening a `.torchsnap` file in the file manager hands it to Torchsnap,
as it already does on macOS (ADR 52). The `bundle.fileAssociations`
entry in `src-tauri/tauri.conf.json` already carries the MIME type and
feeds the `.desktop` file. The Linux side needs more on top.

## What Tauri does and does not generate

Checked against the bundler source
(`crates/tauri-bundler/src/bundle/linux/freedesktop/mod.rs` on
`tauri-apps/tauri` `dev`, 2026-09-24):

- It writes `MimeType=` into the `.desktop` file from each
  association's `mimeType`, plus `x-scheme-handler/<scheme>` for
  deep-link schemes.
- It does **not** write a shared-mime-info XML. Without one the
  system does not know that `*.torchsnap` has our MIME type, so the
  `MimeType=` line never matches anything.

## What to add

- **A shared-mime-info file**, e.g. `app.torchsnap.xml`, installed to
  `/usr/share/mime/packages/`:

  ```xml
  <mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
    <mime-type type="application/vnd.torchsnap.gadget+zip">
      <comment>Torchsnap gadget</comment>
      <glob pattern="*.torchsnap"/>
    </mime-type>
  </mime-info>
  ```

  Decided 2026-09-24: no `<sub-class-of type="application/zip"/>`,
  matching `public.data` on macOS, while the name keeps `+zip`.
  shared-mime-info may still treat a `+zip` type as a zip. Check
  whether archive managers then offer to open `.torchsnap` files, and
  accept it if they do.
- **Ship it** via `bundle.linux.deb.files` (a
  `HashMap<PathBuf, PathBuf>`, `tauri-utils-2.9.3/src/config.rs:349-351`).
  Check whether the rpm config has an equivalent.
- **Refresh caches.** `DebConfig` has `post_install_script` and
  `post_remove_script` (`config.rs:365-380`) for
  `update-mime-database /usr/share/mime` and `update-desktop-database`.
  Debian-based distros often run these through dpkg triggers already,
  so the scripts may be unnecessary. Verify on a target distro before
  adding them.
- **Check the `Exec=` line.** The desktop file must pass files with
  `%F` or `%f`, or the file manager starts the app without the path.
  I have not verified what Tauri's default template writes. If it is
  missing, `DebConfig::desktop_template` (a Handlebars template with
  `categories`, `comment`, `exec`, `icon` and `name`) can override it.

## Receiving files

Paths arrive in argv, and on an already running instance through the
single-instance plugin. Both are in place (`submit_command_line` in
`src-tauri/src/gadget_install/commands.rs`, wired in `run()` and
`setup`). The plugin uses DBus on Linux, which snap and flatpak block
unless the package manifest declares the name.

## Not covered here

- **AppImage** gets no install step, so nothing registers.
  `todos/platform/linux/01m399736658afgk6xv4ab68ws-linux-runtime-mime-self-registration.md`
  covers it.
- **Flatpak** declares MIME types and the DBus name in its own
  manifest. Handle it if Flatpak ever becomes a target.

## Testing

- `xdg-mime query filetype some.torchsnap` returns our MIME type.
- `xdg-mime query default application/vnd.torchsnap.gadget+zip` returns
  Torchsnap's desktop file.
- `gio open some.torchsnap` with the app stopped and with it running.
- Double-click in at least GNOME Files and KDE Dolphin.

## Related

The other Linux todos in `todos/platform/linux/` (launcher display,
tray, Wayland shortcuts) decide whether Linux ships at all. This one
only matters once it does.
