# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Torchsnap task runner — split into domain-specific files under just/.
# Run `just --list` to see all available recipes.

import 'just/install.just'
import 'just/build.just'
import 'just/plugins.just'
import 'just/start.just'
import 'just/quality.just'
import 'just/assets.just'
import 'just/bangs.just'
import 'just/doctor.just'
import 'just/maintenance.just'
import 'just/tools.just'
import 'just/devcontainer.just'

# Show available recipes
default:
    @just --list
