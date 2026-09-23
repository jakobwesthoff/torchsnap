# Install→uninstall and uninstall→reinstall are blocked until restart, with misleading errors

**Kind:** bug
**Severity:** low
**Area:** src-tauri/src/gadget_install.rs

## Problem

Both install and uninstall validate against
`host.gadget_sources()`, which is frozen at startup (the
`GadgetHost` slot list does not change until restart —
`gadget_install.rs:8-12`). On-disk state changes immediately,
so the registry and the disk diverge until the user restarts.
Two user flows hit this divergence:

1. **Uninstall → reinstall (the upgrade flow).** After
   uninstalling a user gadget, its id is still registered as
   `GadgetSourceKind::User` in the frozen registry. Installing
   a new version of the same gadget before restarting is
   rejected at `gadget_install.rs:137-139` with:

   > A user gadget with id `<id>` is already installed.
   > Uninstall the existing version, then retry.

   The user *just did* uninstall it. The only remediation is a
   restart, which the message does not mention. Since there is
   no in-place upgrade path, uninstall→install is the natural
   way to update a gadget, and it always dead-ends here.

2. **Install → immediate uninstall.** A freshly installed
   gadget is not in the frozen registry, so uninstalling it
   right away (e.g. "wrong file") fails at
   `gadget_install.rs:202-206` with
   `unknown gadget id `<id>``. The archive stays in
   `<app_data_dir>/gadgets/` until the user restarts and
   uninstalls again.

## Impact

The two most common correction flows in the gadget management
panel require an extra restart each, and the error messages
point the user at remediations that cannot work
pre-restart. Worst case for flow 1: user uninstalls v1,
gets the "already installed" rejection for v2, restarts —
and now the gadget is fully gone, so they must install v2 and
restart *again* (two restarts for one upgrade).

## Suggested fix

Until hot lifecycle lands
(`todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`),
track pending installs/uninstalls next to the frozen registry
(e.g. a small `RwLock<HashMap<String, PendingChange>>` on
`GadgetHost` updated by the two commands):

- Reinstall over a pending uninstall of the same id: allow it
  (the disk state is already gone; the copy just lands the new
  archive).
- Uninstall of a pending install: allow it (delete the archive
  that was just copied; the registry never knew about it).
- Where a restart genuinely is required first, say so in the
  error message instead of suggesting an action that will be
  rejected.
