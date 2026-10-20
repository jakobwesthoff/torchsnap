---
title: "Submit feature requests and bug reports from inside Torchsnap"
kind: feature
component: frontend
status: needs-discussion
origin: idea
tags: [ux]
---
# Submit feature requests and bug reports from inside Torchsnap

Idea from 2026-10-20. Users should be able to send a feature request or
a bug report from inside the app, through a dedicated, well designed
flow, instead of having to find the GitHub repository
(`github.com/jakobwesthoff/torchsnap`) and write an issue there by hand.
The entry point may be off by default and switched on by a hidden
setting.

## Goal

A short form in the app that asks for the kind of report (feature
request or bug), a title and a description, and sends it to wherever
reports are collected. The flow should feel like part of Torchsnap
and stay quick to fill in.

## Open questions

These need answers before anyone starts:

- Where reports go. Options include a prefilled GitHub "new issue" URL
  opened in the browser (no credentials in the app, needs a GitHub
  account), a GitHub issue created through an API (needs a token or a
  relay service), or an endpoint on torchsnap.app that collects them.
- What the hidden setting looks like. The settings module
  (`src-tauri/src/settings/`) has no hidden settings today, so a first
  one needs a mechanism: a key that only exists in the settings file, a
  key combination in the settings window, or something else.
- Who sees the entry point. Only people who turned on the hidden
  setting, or every user once the flow is proven.
- Where the entry point lives: the tray menu, the settings window, a
  launcher command, or several of these.
- Which diagnostics a bug report attaches (app version, macOS version,
  enabled gadgets, recent logs from the devtools log stream), and how
  the user reviews them before sending. This touches privacy.
