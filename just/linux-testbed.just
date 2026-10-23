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

# Run the testbed self-test against the Linux testbed (default items, or the given ones)
testbed-selftest *ITEMS:
    tools/linux-testbed/selftest {{ ITEMS }}
