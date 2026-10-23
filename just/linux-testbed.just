# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file drives the Linux testbed: a Raspberry Pi with GNOME that the
# recipes reach over SSH. TORCHSNAP_LINUX_HOST names the machine (for
# example `user@pi.local`). Each recipe forwards to
# tools/linux-testbed/testbed; docs/Howto-test-on-a-raspberry-pi.md
# describes the setup.

# Mirror the working tree to ~/torchsnap on the Linux testbed
testbed-sync:
    tools/linux-testbed/testbed sync

# Provisioning is idempotent: every step checks first and changes only
# what is missing, so running it again reports `0 changed`.

# Sync, then take the Linux testbed to the testbed state
testbed-provision *ARGS:
    tools/linux-testbed/testbed provision {{ ARGS }}

# The reboot goes over plain SSH, never through TORCHSNAP_LINUX_RUNNER:
# a runner on the machine's screen goes down with the desktop.

# Reboot the Linux testbed and wait until its desktop session takes input
testbed-reboot:
    tools/linux-testbed/testbed reboot

# Run one shell command line on the Linux testbed
testbed-exec COMMAND:
    tools/linux-testbed/testbed exec {{ quote(COMMAND) }}

# The build installs dependencies, builds every gadget and a debug app
# without bundling: `just install`, then `just build` with
# `bundle.active` off.

# Sync, then build gadgets and a debug app on the Linux testbed
testbed-build:
    tools/linux-testbed/testbed build

# Start the debug app in the Linux testbed's desktop session (control socket on)
testbed-app-start:
    tools/linux-testbed/testbed app-start

# Stop the debug app on the Linux testbed
testbed-app-stop:
    tools/linux-testbed/testbed app-stop

# Print the end of the debug app's output on the Linux testbed
testbed-app-log *LINES:
    tools/linux-testbed/testbed app-log {{ LINES }}

# Send one request to the app's control socket on the Linux testbed
testbed-control METHOD *PARAMS:
    tools/linux-testbed/testbed control {{ METHOD }} {{ if PARAMS == "" { "" } else { quote(PARAMS) } }}

# Take a screenshot on the Linux testbed into target/linux-testbed/NAME.png
testbed-screenshot NAME *OPTIONS:
    tools/linux-testbed/testbed screenshot {{ NAME }} {{ OPTIONS }}

# List the app's windows on the Linux testbed as JSON
testbed-windows:
    tools/linux-testbed/testbed windows

# Print whether the launcher is visible and focused on the Linux testbed, and its monitor
testbed-launcher-state:
    tools/linux-testbed/testbed launcher-state

# Evaluate JavaScript in the Linux testbed's GNOME Shell and print the value
testbed-eval JS:
    tools/linux-testbed/testbed eval {{ quote(JS) }}

# `type` and `key` need a target, `--window REGEX` (title or WM class of
# the focused window) or `--shell-modal` (a GNOME Shell dialog or menu),
# and refuse with exit 4 when the target does not have the keyboard.

# The arguments go on as they were given (positional arguments), so text
# with spaces stays one argument.

# Type text on the Linux testbed: testbed-type --window REGEX 'text'
[positional-arguments]
testbed-type *ARGS:
    tools/linux-testbed/testbed type "$@"

# Press keys on the Linux testbed: testbed-key --window REGEX Escape ctrl+u
[positional-arguments]
testbed-key *ARGS:
    tools/linux-testbed/testbed key "$@"

# Move the pointer on the Linux testbed to X Y (logical pixels)
testbed-pointer X Y:
    tools/linux-testbed/testbed pointer {{ X }} {{ Y }}

# Click on the Linux testbed at X Y (left, right or middle button)
testbed-click X Y BUTTON="left":
    tools/linux-testbed/testbed click {{ X }} {{ Y }} {{ BUTTON }}

# GNOME shows tray icons only with its AppIndicator extension, which
# provisioning installs but leaves off, as stock GNOME has it.

# Switch the Linux testbed's tray support on or off, or print its state (on, off, status)
testbed-tray STATE:
    tools/linux-testbed/testbed tray {{ STATE }}

# Print the position of the app's tray icon on the Linux testbed and whether its menu is open
testbed-tray-icon:
    tools/linux-testbed/testbed tray-icon

# Click the app's tray icon on the Linux testbed
testbed-tray-click BUTTON="left":
    tools/linux-testbed/testbed tray-click {{ BUTTON }}

# Put text on the Linux testbed's clipboard
testbed-clipboard-set TEXT:
    tools/linux-testbed/testbed clipboard-set {{ quote(TEXT) }}

# Print the text on the Linux testbed's clipboard
testbed-clipboard-get:
    tools/linux-testbed/testbed clipboard-get

# Run the testbed self-test against the Linux testbed (default items, or the given ones)
testbed-selftest *ITEMS:
    tools/linux-testbed/selftest {{ ITEMS }}
