# Fedora howto references a todo path that has since moved

**Kind:** improvement (documentation)
**Severity:** low
**Area:** docs/Howto-build-on-fedora-43.md

## Problem

`docs/Howto-build-on-fedora-43.md:292-294` points readers at

```
todos/fedora/01kpv9hwx9rs2hrdbjcvh1ypwp-global-shortcut-wayland-portal.md
```

`todos/fedora/` no longer exists; the file lives at

```
todos/platform/linux/01kpv9hwx9rs2hrdbjcvh1ypwp-global-shortcut-wayland-portal.md
```

Everything else in the howto checked out against the tree
(`just doctor` required/optional tool lists match
`just/doctor.just:42-68`; `just install` / `just start` /
`just build` recipes exist; Control API socket path matches the
`app.torchsnap` identifier).

## Suggested fix

Update the path. Optionally reference the todo by ULID alone
("see todo `01kpv9hwx9rs2hrdbjcvh1ypwp`") so future todo-folder
reshuffles don't break the pointer.
