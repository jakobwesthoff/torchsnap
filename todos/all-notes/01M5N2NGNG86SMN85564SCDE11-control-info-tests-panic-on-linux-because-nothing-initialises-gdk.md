---
title: "Control info tests panic on Linux because nothing initialises GDK"
kind: bug
component: backend
origin: implementation
tags: [linux, testing]
---
# Control info tests panic on Linux because nothing initialises GDK

Four tests of the control API's `info` method fail on Linux, with or without a display. Found on 2026-10-23: `cargo test --lib control::` in the Linux arm64 build container (`docs/Howto-build-linux-on-macos.md`, no display) panicked with "GDK has not been initialized" in four `control::handlers::info` tests. The other control tests passed. On macOS they pass. On 2026-10-24 the same four panicked in the container under `xvfb-run`, with a display (`gdk-0.18.2/src/auto/display.rs:301`: "GDK has not been initialized. Call `gdk::init` or `gtk::init` first."): nothing in these tests initialises GDK, so a display does not help.

## Problem

On Linux, `display()` in `src-tauri/src/control/handlers/info.rs` (around line 79) reads `gdk::Display::default()`. The tests reach it through the MockRuntime app; without an initialised GDK (no `gtk::init`) the gdk call panics instead of returning `None`.

## Impact

`cargo test` for the host crate cannot pass on Linux: not in the container, on a CI runner or over SSH, and not under Xvfb either.

## Suggested fix

Put the gdk read behind a seam the tests can replace, or skip the GDK read when GDK is not initialised (check `gdk::Display::default()` only after `gtk::is_initialized()`, returning no display), and keep the backend naming (`display_backend`) tested as a pure function. Check the four tests in the container afterwards.
