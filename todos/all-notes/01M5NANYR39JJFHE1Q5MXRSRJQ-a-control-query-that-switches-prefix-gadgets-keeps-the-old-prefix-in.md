---
title: "A control query that switches prefix gadgets keeps the old prefix in the launcher input"
kind: bug
component: frontend
origin: implementation
---
# A control query that switches prefix gadgets keeps the old prefix in the launcher input

Setting the query through the control API from one prefix gadget's query to another's leaves the old prefix in the input field. Seen on 2026-10-23 on the Linux testbed: after `query` with `=2+3` (calculator, prefix `=`) a `query` with `!b2` reached the hello-world gadget, but the launcher input showed `=b2`. Not investigated, not checked on macOS. The testbed's self-test now sends an empty query before each check to avoid it.

## Problem

Unknown. Likely in how the launcher applies `setQuery` (`src/launcher/hooks/useControlChannel.ts` calls the Launcher's `setQuery`) while a prefix mode is active: the search goes by the new text, while the input keeps the active prefix and replaces only the rest.

## Reproduce

With the control channel enabled: `query {"text":"=2+3"}`, then `query {"text":"!b2"}`, then `snapshot` (field `query`) or a screenshot of the input.

## Suggested fix

Write a frontend test that sets one prefix query and then another through the control channel's `setQuery`, assert the input shows the second text, then fix the prefix handling.
