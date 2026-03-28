# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Sample memory footprint of Torchsnap and its WebKit processes
memsnap *ARGS:
    tools/memsnap {{ARGS}}

# Run an automated memory benchmark scenario
bench-memory *ARGS:
    tools/bench-memory {{ARGS}}
