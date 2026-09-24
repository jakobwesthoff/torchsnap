# 51. Install gadgets through a review queue with staged copies

Date: 2026-09-24

## Status

Accepted

Amends [35. Plugin distribution via bundled and user-installable archives](0035-plugin-distribution-via-bundled-and-user-installable-archives.md)

Amends [36. Plugin trust model and deferred signing](0036-plugin-trust-model-and-deferred-signing.md)

## Context

ADR 35 installs a `.torchsnap` chosen in the file picker or dropped on
the Gadgets settings panel directly: the archive is validated, then
copied from the path the user picked into `gadgets/<id>.torchsnap`. It
rejects every id that is already registered, so a newer version of an
installed gadget cannot be installed over it, and uninstall deletes the
gadget's `gadget-home/<id>/` tree and settings while the gadget is still
running until the required restart.

Gadget archives are also meant to reach Torchsnap from outside the
settings window: files opened from Finder and paths on the command line.
Gadgets can declare permissions to run commands, reach HTTP origins,
read files and open paths.

## Decision

### One queue for every entry point

Every archive, from the settings file picker, the settings drop zone,
files macOS asks the app to open, and paths on the command line
(including those a second launch hands to the running instance), is
submitted to one install queue. The control socket does not submit
installs while it has no authentication.

The queue is created before the Tauri app is built and only buffers
inputs until `setup` starts it with the install context. tao forwards
macOS open events without buffering them, and whether one can arrive
before `setup` on a cold start is not established.

### Staged copies

Each request copies its archive into
`<app_cache_dir>/install-staging/<ulid>.torchsnap` and parses that copy.
The review and the install both use the staged copy. Archives larger
than 16 MiB are rejected at staging. Leftover staged files are removed
at startup.

### A review for every request

Every request is shown to the user in the Gadgets settings section
before anything is installed, one request at a time in arrival order.
The settings window opens on that section when requests arrive from
outside it. The review shows:

- name, version, id, description and the file path;
- on macOS, where the file was downloaded from, read from the
  `com.apple.metadata:kMDItemWhereFroms` and `com.apple.quarantine`
  extended attributes when present;
- every permission as its own item (one per HTTP origin, filesystem
  pattern, opener scheme, command rule and flag), with a severity:
  command rules, HTTP origin `*` and opening paths are warnings;
- for a replace, which permissions are new and which the new version no
  longer asks for.

Per-rule command limits (`cwd`, `timeout-ms-max`, `max-output-bytes`,
`max-stdin-bytes`) are not shown, because the host does not enforce
them.

Confirming a request decides again under the queue and pending-changes
locks. If the result differs from what the review showed, the request
fails and nothing is installed.

### Replace, downgrade and undo

Installing an id that is already installed as a user gadget replaces
it and keeps its data and settings. The review names the installed and
incoming version and whether it is an upgrade, the same version or a
downgrade; downgrades are allowed with a warning. A replace is refused
when:

- the installed gadget is a directory under `gadgets/` rather than an
  archive;
- the incoming version declares fewer SQL storage migrations than the
  installed one. Opening a database whose version is ahead of the
  migration list fails in `rusqlite_migration`, and that failure stops
  the gadget from loading.

Before the first replace of an id in a session, the current archive is
kept as `gadgets/.<id>.torchsnap.prev`. Each install and replace can be
undone from the result list until that list is dismissed or the app
restarts. Leftover backups are removed at startup.

Installs, replaces, undos and uninstalls made since startup are tracked
next to the registry snapshot taken at startup, so uninstalling and
reinstalling an id works before the restart.

### Deferred uninstall cleanup

Uninstall removes the archive (and a hand-placed directory form) and
writes `gadgets/.<id>.uninstall`. At the next startup, before settings
are initialized and before any gadget loads, each marker's
`gadget-home/<id>/` tree and `enabled.<id>` / `gadgets.<id>.*` settings
are deleted and the marker is removed.

### Signing

Signing stays deferred as in ADR 36; the review is shown for every
install regardless of where the file came from.

## Consequences

- The `install_gadget_archive` command no longer exists. The settings
  panel calls `install_queue_submit`, and installing takes a
  confirmation in the review.
- Installs and uninstalls still take effect only after a restart.
- A gadget that is uninstalled keeps its data on disk until the next
  start.
