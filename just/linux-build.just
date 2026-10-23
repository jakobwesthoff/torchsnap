# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file builds and checks Torchsnap for Linux arm64 in a Docker
# container on an Apple silicon Mac. The work happens in
# `tools/linux-build/linux-build`; see docs/Howto-build-linux-on-macos.md.

# Build the Docker image for Linux arm64 builds
linux-build-image:
    tools/linux-build/linux-build image

# The output, `target/linux-build/<profile>/`, holds `bin/torchsnap`
# and `lib/Torchsnap/` with the bundled gadgets; it runs from wherever
# the whole directory is copied.

# Build Torchsnap for Linux arm64 in a container, without packaging
[arg('profile', long='release', value='release')]
linux-build profile="debug":
    tools/linux-build/linux-build build {{ if profile == "release" { "--release" } else { "" } }}

# The checks copy a build to `~/linux-build-check/<profile>/` on the
# machine `TORCHSNAP_LINUX_HOST` names, through ssh.

# Copy a Linux build to TORCHSNAP_LINUX_HOST and check its glibc and libraries
[arg('profile', long='release', value='release')]
linux-build-check profile="debug":
    tools/linux-build/linux-build check {{ if profile == "release" { "--release" } else { "" } }}

# Start a Linux build on TORCHSNAP_LINUX_HOST in a session of its own and ask its status
[arg('profile', long='release', value='release')]
linux-build-run-check profile="debug":
    tools/linux-build/linux-build run-check {{ if profile == "release" { "--release" } else { "" } }}

# Recipes run from the repository root, so a relative path is taken
# from the directory `just` was started in.

# Run cargo check and clippy for Linux on a checkout (default: this one)
linux-build-lint path=".":
    tools/linux-build/linux-build lint --path {{ quote(clean(join(invocation_directory(), path))) }}

# Show what the Linux build's Docker objects and outputs take on disk
linux-build-status:
    tools/linux-build/linux-build status

# Remove Linux build output (--all: everything the Linux build owns in Docker)
[arg('all', long='all', value='all')]
linux-build-clean all="":
    tools/linux-build/linux-build clean {{ if all == "all" { "--all" } else { "" } }}
