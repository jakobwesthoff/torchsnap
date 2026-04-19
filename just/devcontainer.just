# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Drives the sandboxed Docker-backed Claude development environment
# defined under `.devcontainer/`. We use @devcontainers/cli (the
# official devcontainer spec implementation) instead of invoking
# `docker build`/`docker run` directly, because it reads and respects
# every field in `devcontainer.json` — mounts, `runArgs`,
# `postStartCommand`, `waitFor`, etc. — the same way VS Code does.
#
# `bunx` runs @devcontainers/cli transiently, matching the project's
# "use bun, not npm" convention. First invocation pays a small
# download cost; subsequent runs use bun's cache.
#
# `--remove-existing-container` is the fix for a subtle devcontainer
# CLI behavior: without it, `up` reuses any existing container by
# label and never swaps to a rebuilt image, so edits to the Dockerfile
# or `init-firewall.sh` appear to be ignored. Removing-and-recreating
# the container is cheap (~1s); the image build itself stays fully
# layer-cached, so only changed Dockerfile steps re-execute. All
# persistent state (bash history, the claude login token, cargo/bun
# caches) lives in named volumes and survives the container
# recreation.
#
# First-run authentication: after the shell drops you into the
# container, run `claude login` once to authenticate Claude Code. The
# token persists across restarts in the container-local `.claude`
# volume and is never written to the host.

# Build (if needed), start, and enter the Torchsnap devcontainer
devcontainer:
    bunx @devcontainers/cli up --workspace-folder . --remove-existing-container
    bunx @devcontainers/cli exec --workspace-folder . zsh

# Wipe the container's build caches (cargo registry, cargo git, bun
# cache) while preserving the auth state (`claude login` token) and
# shell history. Useful after a Dockerfile change that alters volume
# mountpoint ownership — Docker only copies image contents into a
# named volume on its first mount, so an already-initialized volume
# with wrong ownership must be removed before a fix takes effect.
#
# The force-remove on the container ensures no volume is held busy.

# Remove cargo/bun cache volumes (keeps claude login and bash history)
devcontainer-reset-caches:
    #!/usr/bin/env bash
    set -euo pipefail
    bunx @devcontainers/cli down --workspace-folder . 2>/dev/null || true
    docker ps -aq --filter "label=devcontainer.local_folder=$(pwd)" | xargs -r docker rm -f
    for vol in $(docker volume ls -q --filter "name=torchsnap-cargo-" --filter "name=torchsnap-bun-"); do
        echo "Removing volume: $vol"
        docker volume rm "$vol"
    done
    echo "Caches cleared. Run 'just devcontainer' to rebuild with fresh caches."
