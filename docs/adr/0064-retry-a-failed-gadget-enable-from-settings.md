# 64. Retry a failed gadget enable from Settings

Date: 2026-10-21

## Status

Accepted

Amends [63. Show a failed gadget enable in Settings](0063-show-a-failed-gadget-enable-in-settings.md) (a Retry button, and the error text Settings show)

## Context

ADR 63 shows a gadget whose `enable()` failed as failed in Settings and
keeps the error text `{e:#}`. The only way to try again was to turn the
gadget off and on. For a WASM gadget the text started with the bridge's
"gadget enable() returned error: " before the gadget's own message, and
for a guest trap it was the multi-line wasm backtrace.

## Decision

The failure block in Settings has a Retry button. It calls the
`gadget_retry_enable` command, which starts the gadget the same way
switching it on does: through the gadget's settings dispatcher, with
the earlier error removed when `enable()` starts. The command acts only
while the stored `enabled.<id>` setting is on, and when the user turns
the gadget off while the retry runs, the gadget is turned off again
afterwards. While the retry runs, the button reads "Retrying…" for at
least 600 ms.

The error text Settings show is the gadget's own message when the
error chain carries one (`GadgetMessage`): what a WASM gadget's
`enable()` returned, or the root cause of a guest trap. Any other error
shows as its whole chain (`{e:#}`), as before. The log keeps the whole
chain in every case.

## Consequences

Turning a gadget off and on still retries it, as ADR 63 states; the
Retry button is a second way to do it.
