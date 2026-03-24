# Torchsnap task runner — split into domain-specific files under just/.
# Run `just --list` to see all available recipes.

import 'just/install.just'
import 'just/build.just'
import 'just/start.just'
import 'just/quality.just'
import 'just/assets.just'
import 'just/doctor.just'
import 'just/maintenance.just'

# Show available recipes
default:
    @just --list
