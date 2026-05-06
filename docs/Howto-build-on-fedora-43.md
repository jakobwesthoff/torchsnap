# How to build Torchsnap on Fedora 43

This guide walks through everything required to get a clean Fedora 43
install (Workstation or Server, both `x86_64` and `aarch64` verified)
to the point where `just start` launches the app and `just build`
produces a release bundle.

Torchsnap is a cross-platform Tauri 2 project, so most of the setup is
generic — the Fedora-specific parts are the native C libraries Tauri's
WebKit backend links against. Those are covered first.

> **Why GTK3 and not GTK4?** Tauri's webview backend (`wry`) still
> binds to the `webkit2gtk-4.1` flavor of WebKitGTK, which is built on
> GTK 3. A newer `webkitgtk-6.0` (GTK 4) flavor exists upstream, but
> the Rust bindings crate used by `wry` does not target it yet. GTK 3
> is feature-frozen upstream but still receives security fixes and is
> current in Fedora 43.

## 1. System packages (`dnf`)

### 1.1 Tauri / WebKit native dependencies

These are the libraries the Rust `*-sys` crates resolve through
`pkg-config` at build time, plus one runtime `dlopen` dependency for
the tray icon. Without them the first `cargo` build under `src-tauri/`
fails with `package 'gdk-3.0' not found`, or — if the runtime piece is
missing — the app compiles but panics at launch with
`Failed to load ayatana-appindicator3 or appindicator3 dynamic library`.

```sh
sudo dnf install \
    gtk3-devel \
    webkit2gtk4.1-devel \
    javascriptcoregtk4.1-devel \
    libsoup3-devel \
    glib2-devel \
    librsvg2-devel \
    libayatana-appindicator-gtk3
```

What each one provides:

| Package                          | Needed by                                                        |
| -------------------------------- | ---------------------------------------------------------------- |
| `gtk3-devel`                     | `gdk-sys`, `gtk-sys` — GTK 3 headers and `.pc`                   |
| `webkit2gtk4.1-devel`            | `webkit2gtk-sys` — WebKit backend for `wry`                      |
| `javascriptcoregtk4.1-devel`     | `webkit2gtk-sys` — JSC bindings                                  |
| `libsoup3-devel`                 | `soup3-sys` — HTTP used by WebKit                                |
| `glib2-devel`                    | `glib-sys` and friends                                           |
| `librsvg2-devel`                 | `tauri build` icon/SVG processing at bundle time                 |
| `libayatana-appindicator-gtk3`   | runtime tray icon — `libappindicator-sys` dlopens `libayatana-appindicator3.so.1` |

In practice `webkit2gtk4.1-devel` pulls most of the other `-devel`
packages transitively, but listing them explicitly makes the failure
mode obvious if a mirror lags. `libayatana-appindicator-gtk3` is the
runtime shared library only (no `-devel` suffix) — it is not pulled in
by anything else and is only consulted at app launch, which is why a
compile-clean run can still panic the first time you hit `just start`.

### 1.2 GNOME-only: enable the AppIndicator shell extension

`libayatana-appindicator-gtk3` publishes the tray icon over the
freedesktop **StatusNotifierItem** D-Bus protocol. KDE, XFCE, MATE,
Cinnamon, Budgie, and most Wayland status bars (Waybar, ironbar, …)
subscribe to that protocol natively and display the icon with no extra
steps.

**GNOME Shell does not.** It dropped system-tray support in 3.26 and
takes the stance that persistent status icons are a UX anti-pattern.
Without a bridge, Torchsnap runs fine on Fedora Workstation but its
tray icon is silently invisible.

The standard bridge is the shell extension *AppIndicator and
KStatusNotifierItem Support*, packaged on Fedora 43. Install and
enable it:

```sh
sudo dnf install gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

`gnome-extensions` ships with GNOME Shell itself — no additional GUI
app is required. Verify that the extension is both enabled and live in
the current session:

```sh
gnome-extensions list --enabled | grep appindicator
gnome-extensions info appindicatorsupport@rgcjonas.gmail.com | grep State
```

`State: ACTIVE` means the extension is loaded in the running shell and
new tray icons will appear immediately. `State: ENABLED` (without
`ACTIVE`) means it's configured but won't run until the next session —
log out and log back in. If you prefer a GUI, `gnome-extensions-app`
or `gnome-tweaks` both expose a toggle for the same thing.

On any non-GNOME desktop you can skip this section entirely.

### 1.3 Build and asset pipeline tools

The `just assets` recipe regenerates icons, mascots, and tray bitmaps;
the WASM gadget pipeline uses `zip` to pack `.torchsnap` archives; and
`shellcheck` gates the shell recipes during quality checks. On a
default Fedora 43 Workstation install, `zip`, `ImageMagick`, and `jq`
are usually already present — install anything missing.

```sh
sudo dnf install zip ImageMagick jq libwebp-tools shellcheck
```

`just doctor` treats `cargo`, `bun`, `zip`, `wasm-tools`, the
`wasm32-wasip2` Rust target, `magick`, `oxipng`, and `cwebp` as
required and exits non-zero if any are absent. `shellcheck` is listed
as optional — it is only invoked by the quality recipes — but install
it anyway if you intend to run the linters.

## 2. Rust toolchain

Fedora's packaged `rust` is too stale for this project; install the
official toolchain via `rustup`.

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After the installer finishes, open a new shell (or source
`~/.cargo/env`) so `cargo` and `rustup` are on `PATH`, then add the
WASM gadget target:

```sh
rustup target add wasm32-wasip2
```

## 3. Bun

The frontend uses [Bun](https://bun.sh) as runtime and package
manager. Fedora 43 packages it:

```sh
sudo dnf install bun
```

## 4. `just` task runner

The project is driven entirely through recipes in the repo-root
`Justfile` (and files under `just/`). Install `just` via Cargo so the
version is current:

```sh
cargo install just
```

## 5. Cargo-based tooling

These are consumed by recipes under `just/` and by the plugin build
pipeline.

```sh
cargo install wasm-tools   # WIT validation and formatting
cargo install oxipng       # PNG optimisation in the asset pipeline
```

## 6. Verify the environment with `just doctor`

`just doctor` prints a checklist of every external tool the project
relies on. Run it before proceeding — if everything installed
correctly you should see all green ticks:

```sh
cd /path/to/torchsnap
just doctor
```

A red cross points at the missing tool and shows how to install it.
Re-run until the summary line reports all tools found.

## 7. Fetch project dependencies

This pulls the JS packages via Bun and fetches the Rust crates for
both the host workspace (`src-tauri/`) and the gadget workspace
(`gadgets/`):

```sh
just install
```

## 8. Build and run

### 8.1 Development

Starts Vite with HMR and spawns the Tauri dev window:

```sh
just start
```

### 8.2 Release bundle

Builds the bundled gadgets, the frontend, and the native host, then
invokes `tauri build` to produce the distributable bundle under
`src-tauri/target/release/bundle/`.

```sh
just build
```

## Troubleshooting

### `The system library 'gdk-3.0' ... could not be found`

System packages from step 1.1 are missing. The most common oversight
is installing `webkit2gtk4.1-devel` without `gtk3-devel` present — on
some package-set variations the dependency is not pulled in
automatically. Re-run the full `dnf install` command from 1.1.

### `pkg-config` finds GTK 4 but build still fails

That is expected — Torchsnap does **not** use GTK 4. Having both
`gtk3-devel` and `gtk4-devel` installed side by side is fine and is
the normal state on a Fedora desktop.

### `Failed to load ayatana-appindicator3 or appindicator3 dynamic library`

The binary compiled, but the tray-icon runtime dependency is missing.
Install `libayatana-appindicator-gtk3` (step 1.1). It is a pure runtime
package — no restart or rebuild required, just launch again.

### App runs on GNOME but the tray icon is nowhere

No error, no panic — the icon just isn't drawn. Means the runtime lib
is present but nothing in the session is listening for
StatusNotifierItem items. Install and enable the shell extension from
step 1.2, then log out and back in.

### `cargo` or `rustc` reported as missing by `just doctor`

A fresh `rustup` install does not update the current shell's `PATH`.
Open a new terminal, or `source ~/.cargo/env`, then re-run
`just doctor`.

### `magick: command not found` during `just assets`

Install `ImageMagick` (step 1.2). Torchsnap requires ImageMagick 7+;
Fedora 43 ships `ImageMagick-7.1.x`, which is new enough.

### Clipboard gadget panics at startup with `SetupFailed`

Full error:

```
thread 'tokio-rt-worker' panicked at src/gadgets/clipboard/mod.rs:166:44:
clipboard context: SetupFailed(SetupFailed { .. })
```

and somewhere in the surrounding output:

```
Authorization required, but no authorization protocol specified
```

Cause: your shell has a **stale `XAUTHORITY`** pointing at a Mutter
X11 cookie file that no longer exists. Every GNOME login cycle
generates a fresh cookie under `/run/user/$UID/.mutter-Xwaylandauth.*`
with a new random suffix, and shells that were started before the
most recent log-in never see the update. The clipboard gadget's
underlying `clipboard-rs` dependency is X11-backed on Linux, so it
cannot connect to XWayland and crashes the worker thread.

Confirm:

```sh
ls /run/user/$UID/.mutter-Xwaylandauth.*   # current cookie(s)
echo "$XAUTHORITY"                          # what this shell has
```

If they differ, fix the current shell with either:

```sh
export $(systemctl --user show-environment \
  | grep -E '^(XAUTHORITY|DISPLAY|WAYLAND_DISPLAY)=')
```

or just open a fresh terminal — new interactive shells inherit the
systemd user environment.

### Global shortcut (`Ctrl+Shift+Space`) does nothing on Wayland

**Status: temporary workaround.** The in-app *Global Shortcut*
setting does not fire from a normal focus on a Wayland session. The
proper fix — registering via the freedesktop `GlobalShortcuts`
desktop portal so GNOME delivers the keypress to the app directly —
is tracked under
`todos/fedora/01kpv9hwx9rs2hrdbjcvh1ypwp-global-shortcut-wayland-portal.md`
and is not implemented yet.

Root cause: `tauri-plugin-global-shortcut` on Linux uses the
`global-hotkey` crate, whose Linux backend is X11-only (`XGrabKey`
via `x11rb`). The grab is installed against XWayland's X server,
and Mutter does not forward key events to XWayland grabbers when a
Wayland-native window holds focus.

Until the portal integration lands, bind a GNOME custom keybinding
that pokes the Control API socket. Zero code changes, works from
any focus because GNOME itself dispatches it.

1. In Torchsnap settings, enable the Control API. This exposes the
   socket at `~/.local/share/app.torchsnap/control.sock`.
2. GNOME Settings → Keyboard → *View and Customize Shortcuts* →
   *Custom Shortcuts* → `+`.
   - Name: *Torchsnap toggle*
   - Command:
     ```sh
     sh -c 'echo "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"toggle\"}" | socat - UNIX-CONNECT:"$HOME/.local/share/app.torchsnap/control.sock"'
     ```
   - Shortcut: `Ctrl+Shift+Space` (or any combo GNOME does not
     already claim).

The in-app picker still applies on pure X11 sessions (e.g. a Fedora
Xfce spin, or `GDK_BACKEND=x11 just start`) — its only limitation
is Wayland, not Linux in general.
