---
title: "Let testbed-record-start take a frame rate and a screen area"
kind: improvement
component: project
impact: low
origin: implementation
tags: [linux, testing, tooling]
---
# Let testbed-record-start take a frame rate and a screen area

`just testbed-record-start NAME` always records the whole screen of the Linux testbed at GNOME Shell's default frame rate. That is too coarse to catch a flash of one or two frames when the launcher is shown, which is what a flicker check needs. The testbed lives on branch `linux-testbed` until it lands.

## Problem

`tools/linux-testbed/pi/testbed-pi`, `cmd_record_start`, takes only the file name and starts `tools/linux-testbed/pi/gnome-record` with no options. A frame-rate trial on the Raspberry Pi 5 testbed (GNOME 48, one 4K portrait monitor, 2026-10-23) got about 10 fps for the full 4K screen, in bursts. An area recording through Shell's Screencast `ScreencastArea` with `framerate` 60 got about 25 fps in bursts on a 1280x800 headless monitor, and only 3 to 5 frames per show on an area of the 4K desktop. The trial needed one-off scripts outside the recipes to set the area and frame rate.

## Suggested fix

Give `record-start` (Mac side `tools/linux-testbed/testbed`, Pi side `testbed-pi` and `gnome-record`) the options `--framerate N` and `--area X,Y,W,H`, the latter as `testbed-screenshot` already takes it. Area recording goes through `ScreencastArea`, the full screen through `Screencast`, each with the `framerate` option. Add a self-test item that records a short area clip at a set frame rate and checks the frame count with `ffprobe`.

## Caveat

On the 4K desktop the encoder, not the option, limits the rate: 60 requested gave 3 to 5 frames per show. A content flash shorter than the frame gap stays possible to miss, so flicker checks should keep combining the recording with GNOME Shell's after-paint log and the page's frame log.
