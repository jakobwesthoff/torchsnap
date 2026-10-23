---
title: "The template gadget fails to enable because its manifest lacks the opener permission"
kind: bug
component: gadgets
origin: implementation
---
# The template gadget fails to enable because its manifest lacks the opener permission

The `template` gadget calls the opener interface but does not declare it, so the host refuses to enable it. Seen on 2026-10-23 in a debug build on Linux, where unlisted gadgets load from the development folder: the app log repeats "gadget template: enable failed: ... imports interfaces without matching permissions: [opener]".

## Problem

`gadgets/template/src/lib.rs:274-275` handles `Command::OpenUrl` with `opener::open_url(&url)`, so the component imports the opener interface. `gadgets/template/manifest.toml` lists only `settings = true` and `sql-storage = true` under `[permissions]`. The host checks imports against permissions and refuses the gadget.

The template is the starting point for new gadgets, so authors copy a manifest that does not match its code.

## Suggested fix

Add the opener permission to the template's `[permissions]` (check the exact key in the manifest schema and the other gadgets, for example `gadgets/open-url/manifest.toml`), rebuild `gadgets/template.torchsnap`, and confirm the gadget enables in a debug build. Consider a test or a build-time check that each in-tree gadget's imports match its declared permissions.
