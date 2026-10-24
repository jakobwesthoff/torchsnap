---
title: "Shrink the launcher while it is still mapped before hiding it on Linux"
kind: improvement
component: platform
status: needs-discussion
origin: investigation
tags: [linux, memory]
---
# Shrink the launcher while it is still mapped before hiding it on Linux

On Linux the hidden launcher keeps its window buffers. Today's 1x1 shrink after the hide (ADR 0020) frees nothing there. Shrinking to 1x1 while the window is still mapped, and hiding it afterwards, frees 35 to 40 MiB while hidden. This needs a decision together with the GNOME placement work (maximized launcher), which has its own plan for releasing the window's buffers.

## Measurement

Raspberry Pi 5 testbed, GNOME 48 Wayland, one 4K monitor at scale 2, WebKitGTK 2.54, release build, three paired rounds (Linux memory review, 2026-10-24):

- Torchsnap's memory (system Shmem plus the anonymous PSS of its processes) fell by 40 MiB at start and 35 MiB after the first dismiss. Shown phases did not change.
- What is freed: the window's buffers shared between the UI process, the web process and GNOME Shell (UI shared GPU memory 40.9 to 8.8 MiB, web shared 21.4 to 0). The web process's own 93 MiB of GPU memory stays.
- Show to visible content: 75 ms median against 61 ms.
- Screenshots and recordings of five show/dismiss cycles: card, mascot and emoji grid correct on every show, a new mascot each time.
- Without any change, the hidden launcher still holds 146 to 157 MiB of GPU buffers three minutes after a dismiss; nothing releases on its own.

## Open

- The experiment shrank after the page had blanked, waited a fixed 150 ms off the main thread and then hid. What to wait for instead of a fixed time is open.
- The GNOME placement design maximizes the launcher on GNOME and plans its own `release_backing_store` step for the hidden window, with no shrink under a maximized window. Decide both together.
