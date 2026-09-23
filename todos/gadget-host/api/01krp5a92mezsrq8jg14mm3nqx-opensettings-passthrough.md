# Stop intercepting OpenSettings in the host

`ActionId::OpenSettings` is currently intercepted by
`GadgetHost::execute` (gadget_host.rs:969) before reaching the gadget's
`execute()`. The host emits `open-gadget-settings` and returns `Dismiss`
directly.

This should change: `OpenSettings` should behave like every other well-known
action (keybinding assignment + passthrough to the gadget's `execute`). To
still allow opening the settings panel, introduce a new `PostAction` variant
(e.g. `PostAction::OpenSettings`) that the gadget can return from `execute()`
to trigger the same navigation. This makes the behavior opt-in from the
gadget side rather than a host-level intercept, and lets gadgets do custom
work before opening settings if needed.
