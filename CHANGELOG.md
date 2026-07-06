# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Torchsnap is a Tauri 2 desktop launcher for macOS with a sandboxed WebAssembly
gadget system; gadgets ship as single-file `.torchsnap` archives, and the set
bundled with release builds is whitelisted in `gadgets/bundled.toml`.

### Added

- The `app-icon` entry icon: gadgets can show an installed application's real
  icon by passing a platform-native identifier (a macOS bundle id), and the
  launcher renders that application's actual icon. Requires the `icon-cache`
  manifest permission.
- The awake gadget shows Amphetamine's real application icon
  (`com.if.Amphetamine`) on its entries; the backend-error entry keeps the
  bolt icon.
- Quality gates cover the gadgets workspace: new `lint-gadgets`,
  `fmt-gadgets`, and `fmt-check-gadgets` recipes wired into `just lint` /
  `just fmt` / `just fmt-check`; `lint-crates` lints all cargo targets.
