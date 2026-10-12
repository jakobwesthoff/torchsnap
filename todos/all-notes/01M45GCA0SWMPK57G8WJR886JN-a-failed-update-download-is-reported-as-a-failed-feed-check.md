---
title: "A failed update download is reported as a failed feed check"
kind: bug
component: backend
impact: low
origin: incident
tags: [error-handling, ux]
---
# A failed update download is reported as a failed feed check

When the in-app update fails while downloading the archive from github.com, the update window says Torchsnap could not get update information from torchsnap.app. The message names the wrong host and the wrong step, so the user and the maintainer look in the wrong place. Seen in the field on 2026-10-05.

## Seen in the field

On 2026-10-05, 0.13.0 offered 0.14.0, and "Install and Restart" ended in that message several times in a row. The cause, from the macOS unified log: the router's resolvers answered SERVFAIL for `github.com`, and mDNSResponder served the cached empty answer for about a minute. The archive URL in the feed points at `github.com/jakobwesthoff/torchsnap/releases/download/...`, so the download failed at the DNS lookup. The feed lookups for torchsnap.app kept working. A few minutes later the update installed.

## Problem

`src-tauri/src/updates/mod.rs`, `describe_plugin_error` (around line 493), words the errors of both steps, the check (`fetch_update`) and the download and install (`update_install`). `Error::Reqwest`, `Error::Network` and `Error::ReleaseNotFound` all become "Torchsnap could not get update information from torchsnap.app. Check the internet connection and try again later." Established from the field case above and read in the code.

- The message names the wrong host and the wrong step. The feed check had worked; the download from github.com failed.
- "Try Again" in the failed phase calls `actions.check` (`src/update/UpdateWindow.tsx`), so after a failed download it runs a new check, which offers the same update again. That is consistent, but with the message above it reads as if the check itself kept failing.
- The underlying error (`update check failed: …`, `installing the update failed: …`) only goes to `eprintln!`, which is lost in the bundled app.

## Suggested fix

- Word download errors separately from check errors, for example "Torchsnap could not download the update from github.com", by passing the step to the wording function or using a second function for `update_install`.
- Consider giving `Phase::Failed` the step it failed in, so the window can word its text and, after a failed download, offer to retry the download instead of running a new check. Open question.
- Keep the plugin's error chain (here a DNS lookup failure) reachable for diagnosis, e.g. as an expandable detail in the window.

The wording tests sit next to `unreachable_feed_is_worded_for_people` in `src-tauri/src/updates/mod.rs`.

## Relations

- Relates to: [Host errors go only to stderr and are invisible in a packaged app](01KWG2FTAE87ZZD75QGCPA6TTZ-host-errors-go-only-to-stderr-and-are-invisible-in-a-packaged-app.md), the error chain of this case went to `eprintln!` and was lost
