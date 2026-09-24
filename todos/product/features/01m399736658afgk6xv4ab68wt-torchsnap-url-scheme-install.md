---
kind: feature
status: deferred
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
depends-on: [todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md, todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md, todos/gadget-host/install/01m399736658afgk6xv4ab68wp-argv-intake-and-single-instance.md]
---

# Install gadgets from a `torchsnap://` link

Needs an ADR before implementation.

## Goal

An "Install in Torchsnap" button on a web page (the docs site, a
gadget's README, a future gadget directory) opens Torchsnap. Torchsnap
downloads the archive, shows the review dialog, and installs on
confirmation. It saves the user the download, find-in-Finder and
double-click steps.

## Depends on

- `todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`,
  with `InstallSource::Remote(Url)` and origin `UrlScheme`. The
  download happens in the intake's staging step.
- `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`.
  A link-triggered install must never go through only the minimal
  confirm step.
- On Linux,
  `todos/gadget-host/install/01m399736658afgk6xv4ab68wp-argv-intake-and-single-instance.md`,
  since URLs arrive in argv there.

## How Tauri handles it

From the deep-link plugin docs (`v2.tauri.app/plugin/deep-linking/`,
read 2026-09-24):

- **Config.** Schemes go under `plugins.deep-link.desktop.schemes` in
  `tauri.conf.json`. The bundler turns them into `CFBundleURLTypes` on
  macOS and `x-scheme-handler/<scheme>` in the Linux `.desktop` file.
  The latter is verified in the bundler source; see the Linux file
  association todo.
- **macOS.** Registration cannot happen at runtime. Per the docs,
  deep links "can only be tested on the bundled application, which
  must be installed in the `/Applications` directory." URLs arrive
  through the same `RunEvent::Opened` event as opened files; the
  plugin exposes them as `on_open_url` and `get_current()`.
- **Linux and Windows.** `register()` and `register_all()` register at
  runtime, which dev builds and AppImage need. For AppImage the docs
  warn that moving the file invalidates the registration (see
  `todos/platform/linux/01m399736658afgk6xv4ab68ws-linux-runtime-mime-self-registration.md`).
  The URL arrives in argv, and on a running instance through
  single-instance.

One plugin handles both platforms, and the bulk of this work (URL
parsing, download, trust) is platform-independent. That is why this is
one todo with platform notes rather than a macOS/Linux split.

## URL design (open)

- **Option A, direct URL:** `torchsnap://install?url=https%3A%2F%2F…/foo.torchsnap`.
  Works today with GitHub release assets. Anyone can link any
  archive.
- **Option B, registry id:** `torchsnap://install/<gadget-id>`. The
  app resolves the id against a registry Torchsnap controls. Safer and
  shorter links, but no registry exists, and running one is a product
  decision of its own.
- **Optional integrity parameter:** `&sha256=<hex>`, checked after
  download. This only protects against a swapped file between link and
  download. The page that shows the link also picks the hash, so it
  proves nothing about who built the gadget.
- **Path namespace.** Keep `install` as one verb among possible future
  ones (e.g. `torchsnap://settings/<gadget-id>`), and reject unknown
  verbs.

## Trust and security points to settle in the ADR

- **ADR 0036 asks for this.** It defers signing and names "a
  distribution channel beyond direct download" as a revisit trigger.
  Its other trigger, network and filesystem imports, has already
  fired: the HTTP (ADR 0038) and command (ADR 0040) APIs exist. The
  ADR for this feature has to revisit 0036, not route around it.
- **Any web page can fire the link.** Browsers ask the first time,
  and many offer "always allow" for the site. After that, a page can
  trigger installs without a click. So the review dialog is
  mandatory for this origin. Collapse repeated requests while a
  dialog is open, and never queue unbounded requests from links.
- **Download before or after asking?** Showing permissions requires
  the manifest, which requires the archive. Options: a first dialog
  with just the URL and host ("Download from github.com?"), then the
  full review; or download immediately into staging with strict
  limits and show one dialog. The second is smoother but lets any web
  page make Torchsnap download things.
- **Download rules.**
  - HTTPS only.
  - Redirects allowed, since GitHub release asset links redirect to
    another host. Show the final host in the dialog.
  - Size cap enforced while streaming, tied to
    `todos/gadget-host/wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`.
  - Timeout and progress UI.
  - Never trust `Content-Type`.
  - `reqwest` with rustls is already a dependency
    (`src-tauri/Cargo.toml`).
- **Host allowlist?** Only allow downloads from a fixed list (e.g.
  `github.com`, `torchsnap.app`) or from anywhere with the host shown
  prominently. An allowlist is safer and gets in the way of
  third-party gadget authors.
- **Scheme hijacking.** Any other app can also claim `torchsnap://`,
  and the OS then picks one. Links carry only public download URLs,
  so the impact is low as long as nothing secret ever goes into a
  link. Write that down as a rule.

## Web and docs side

- `torchsnap-web` (landing page, `web/src/landing/hero/Hero.astro`
  links the DMG) and `torchsnap-docs` (gadget docs) contain no
  `.torchsnap` downloads today. Whoever adds install buttons needs a
  fallback: if Torchsnap is not installed, the link does nothing
  visible. Show a plain "download the `.torchsnap`" link next to the
  button.
- Document the URL format for gadget authors in
  `torchsnap-docs/src/content/docs/development/packaging.mdx`.

## Done when

- The ADR is accepted.
- A link on a test page installs a gadget from a GitHub release on
  macOS through the review dialog, with the app running and not
  running.
- The same works on Linux once Linux ships.
- A page firing the link repeatedly causes no dialog pile-up or
  unbounded downloads.
