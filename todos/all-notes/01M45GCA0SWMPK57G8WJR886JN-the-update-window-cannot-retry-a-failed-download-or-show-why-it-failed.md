---
title: "The update window cannot retry a failed download or show why it failed"
kind: improvement
component: backend
status: needs-discussion
impact: low
origin: incident
tags: [error-handling, ux]
---
# The update window cannot retry a failed download or show why it failed

When "Install and Restart" fails, "Try Again" in the update window starts a new check instead of retrying the download, and the window shows only a worded message without the underlying error, which the bundled app loses. Seen in the field on 2026-10-05: 0.13.0 offered 0.14.0, and the download failed several times in a row at the DNS lookup of `github.com` (the router's resolvers answered SERVFAIL, and mDNSResponder served the cached empty answer for about a minute), while the feed lookups for torchsnap.app kept working. A few minutes later the update installed. The cause had to be read from the macOS unified log.

## Facts the proposal relies on

- `Phase::Failed` (`src-tauri/src/updates/mod.rs`) carries only `message`, not the step that failed.
- "Try Again" in the failed phase calls `actions.check` (`src/update/UpdateWindow.tsx`). After a failed download, the new check offers the same update again.
- `update_install` calls `Inner::install_failed`, which drops the plugin's `Update` handle, so a retry needs a new check today unless the handle is kept.
- `is_busy` and `settles_on_close` treat `Phase::Failed` as settled: a new check may start, and closing the window returns to idle.
- The window message comes from `describe_plugin_error` (check) or `describe_install_error` (download and install). A network failure of the download names the host of the archive URL in the feed.
- The underlying error goes only to `eprintln!`: "update check failed: …" in `run_check`, "installing the update failed: …" in `update_install`. The bundled app's stderr is attached to nothing.
- github.com answers the archive URL with a 302 to `release-assets.githubusercontent.com`, and the plugin's reqwest client follows it; the window names github.com either way. Read in reqwest 0.13.5 (`src/async_impl/client.rs`, `PendingRequest::poll`), not run: a failed send gets the original request URL, so the error chain would not name the redirect target either.

## Proposal

- Give `Phase::Failed` the step it failed in, so the window can word its text and its actions per step.
- After a failed download, offer to retry the download instead of a new check.
- Keep the plugin's error chain (`{e:#}`) reachable for diagnosis, for example as an expandable detail in the window.
- Word the remaining plugin errors while touching the per-step wording. Both were found on 2026-10-20 while fixing the download wording (commit 8e888607), read in the plugin's source, not reproduced:
  - `Base64` (a feed signature that is not valid base64) still shows the generic "Updating failed: <plugin message>"; it belongs with the "could not be verified" text. A test needs a dev-dependency on base64 0.22.1, the plugin's version (src-tauri uses 0.23.1).
  - The "administrator password" wording for `AuthenticationFailed` looks unreachable on macOS: only the plugin's Linux install returns that error, while the macOS install reports a failed admin move as `Io`.

## Open questions

- Retry with the kept `Update` handle, or always check again before downloading.
- Whether the error detail belongs in the window or in a host log.

## Relations

- Relates to: [Host errors go only to stderr and are invisible in a packaged app](01KWG2FTAE87ZZD75QGCPA6TTZ-host-errors-go-only-to-stderr-and-are-invisible-in-a-packaged-app.md), the error chain of the field case went to `eprintln!` and was lost
