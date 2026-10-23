---
title: "Control info tests panic on Linux without a display"
kind: bug
component: backend
origin: implementation
tags: [linux, testing]
---
# Control info tests panic on Linux without a display

Four tests of the control API's `info` method fail on Linux when no display is available. Found on 2026-10-23: `cargo test --lib control::` in the Linux arm64 build container (`docs/Howto-build-linux-on-macos.md`, no display) panicked with "GDK has not been initialized" in four `control::handlers::info` tests. The other control tests passed. On macOS they pass.

## Problem

On Linux, `display()` in `src-tauri/src/control/handlers/info.rs` (around line 79) reads `gdk::Display::default()`. The tests reach it through the MockRuntime app; without an initialised GDK (no `gtk::init`, no display) the gdk call panics instead of returning `None`.

## Impact

`cargo test` for the host crate cannot pass on headless Linux: in the container, on a CI runner, or over SSH without a session.

## Suggested fix

Put the gdk read behind a seam the tests can replace, or skip the GDK read when GDK is not initialised (check `gdk::Display::default()` only after `gtk::is_initialized()`, returning no display), and keep the backend naming (`display_backend`) tested as a pure function. Check the four tests in the container afterwards.
