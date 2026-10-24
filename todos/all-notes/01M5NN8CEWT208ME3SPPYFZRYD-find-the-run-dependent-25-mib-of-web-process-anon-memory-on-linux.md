---
title: "Find the run-dependent 25 MiB of web-process anon memory on Linux"
kind: investigation
component: platform
impact: low
origin: investigation
tags: [linux, memory]
---
# Find the run-dependent 25 MiB of web-process anon memory on Linux

In some 200-cycle show/hide runs on the Linux testbed (Raspberry Pi 5, GNOME 48, WebKitGTK 2.54) the WebKitWebProcess gained 21 to 30 MiB of anonymous memory that has nothing to do with new mascot images. Other runs with the same setup did not: with WebKit's `DocumentViewer` cache model one run gave +23.9 MiB and the next +1.5 MiB. With the mascot draw off, one 500-cycle run reached +50.7 MiB at cycle 451 and fell back to +21.8 MiB at 500, so the memory is released again.

## Ruled out (Linux memory investigation, 2026-10-24)

- The JavaScript heap: WebKit's sampler (`WEBKIT_SAMPLE_MEMORY=1`) shows 4.9 to 6.4 MiB in use, flat.
- Decoded image mappings: the runs with the growth end with one or no 576 KiB mapping.
- GPU memory.

## Not established

Which allocator owns it. With the mascot draw off it sat mostly in one new 30 MiB anonymous region, 15 MiB resident. Low priority, since it is released again.
