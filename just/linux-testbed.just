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
