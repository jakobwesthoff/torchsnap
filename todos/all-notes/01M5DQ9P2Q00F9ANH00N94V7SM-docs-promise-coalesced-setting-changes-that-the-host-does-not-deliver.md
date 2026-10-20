---
title: "Docs promise coalesced setting changes that the host does not deliver"
kind: bug
component: gadget-host
status: needs-discussion
origin: review
tags: [api-design]
---
# Docs promise coalesced setting changes that the host does not deliver

The published docs say that rapid writes to one setting reach `on_setting_changed` as one call with the final value. Each settings-changed listener call drains only its own item, so a slider drag gives one call per write. The claim is false before and after the dispatcher fix of 2026-10-21. Found in that fix's review, read in the code.

## Where the claim is

- torchsnap-docs `src/content/docs/development/interfaces/exports.mdx`, the Aside on `on_setting_changed` ("receives one call with the final value").
- torchsnap-docs `src/content/docs/development/frontend/settings-panels.mdx`, around lines 66-67.
- `docs/api/gadget-development.md`, around lines 282-285.
- ADR 26, Consequences (on main; only a new ADR may change it).

## Open question

Make the host coalesce (deliver only the latest value per key when calls pile up), or correct the docs to say each write is delivered. Coalescing matches what authors were told; correcting the docs is smaller.
