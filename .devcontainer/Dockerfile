# =========================================================
# Attribution
# =========================================================
#
# Adapted from the Anthropic Claude Code reference devcontainer:
#   https://github.com/anthropics/claude-code/tree/main/.devcontainer
#
# The upstream files are © Anthropic PBC, "All rights reserved" under
# Anthropic's Commercial Terms of Service. We reproduce and adapt them
# here in reliance on the implied license granted by Anthropic's public
# documentation at https://docs.claude.com/en/docs/claude-code/devcontainer,
# which directs users to adopt and customize these files.
#
# Torchsnap-specific additions are original work; this file as a whole
# is therefore NOT covered by the project's MPL-2.0 license. See
# .devcontainer/NOTICE.md for full provenance details.

# =========================================================
# Base image
# =========================================================
#
# The official Rust image already ships cargo, rustc and rustup configured
# in /usr/local/{cargo,rustup}. Starting from it saves a large chunk of
# build time compared to installing the toolchain from scratch on top of
# a minimal Debian base.

ARG RUST_VERSION=1
FROM rust:${RUST_VERSION}-bookworm

ARG TZ
ENV TZ="$TZ"

ARG CLAUDE_CODE_VERSION=latest
ARG GIT_DELTA_VERSION=0.18.2
ARG ZSH_IN_DOCKER_VERSION=1.2.0

# =========================================================
# System packages
# =========================================================
#
# Three groups here:
#  1. Shell conveniences and the Anthropic-reference dev tools (zsh, fzf,
#     gh, git, nano, vim, man).
#  2. The firewall toolchain (iptables, ipset, dnsutils, aggregate, jq) —
#     identical to the Anthropic reference because init-firewall.sh
#     depends on all of them.
#  3. Rust build prerequisites for pure-Rust and WASM builds
#     (build-essential, pkg-config, libssl-dev, xz-utils).
#  4. Tauri v2 Linux runtime libraries (libwebkit2gtk-4.1 and friends).
#     The Rust compiler links against them when building against the
#     `tauri` crate, even though we can't run a windowed Tauri binary
#     inside a headless container. Installing them lets `cargo check`
#     and `cargo build` succeed on the src-tauri workspace — the actual
#     windowed app still has to be run on the host.

RUN apt-get update && apt-get install -y --no-install-recommends \
        less \
        git \
        procps \
        sudo \
        fzf \
        zsh \
        man-db \
        unzip \
        gnupg2 \
        gh \
        iptables \
        ipset \
        iproute2 \
        dnsutils \
        aggregate \
        jq \
        nano \
        vim \
        curl \
        wget \
        ca-certificates \
        file \
        shellcheck \
        build-essential \
        pkg-config \
        libssl-dev \
        xz-utils \
        libwebkit2gtk-4.1-dev \
        libjavascriptcoregtk-4.1-dev \
        libsoup-3.0-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        libxdo-dev \
        libxcb1-dev \
        libxcb-render0-dev \
        libxcb-shape0-dev \
        libxcb-xfixes0-dev \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# =========================================================
# just (task runner)
# =========================================================
#
# `just` is not in Debian bookworm's main repository. The official
# install script fetches a prebuilt binary appropriate to the host
# architecture and drops it in /usr/local/bin. This is faster than
# `cargo install just` (no from-source compilation).

RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
    | bash -s -- --to /usr/local/bin

# =========================================================
# Node.js (host-level install, before the user switch)
# =========================================================
#
# Bun is the project's primary JS runtime, but a real Node install is
# still useful for editor extensions and the occasional tool that
# assumes `node` is on $PATH. NodeSource provides a modern LTS Debian
# package that's simpler than juggling nvm inside a container.

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# =========================================================
# git-delta for prettier git diffs
# =========================================================

RUN ARCH=$(dpkg --print-architecture) \
    && wget "https://github.com/dandavison/delta/releases/download/${GIT_DELTA_VERSION}/git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" \
    && dpkg -i "git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" \
    && rm "git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb"

# =========================================================
# Non-root user
# =========================================================
#
# Running Claude as a normal user means any Bash command it issues is
# already un-privileged at the kernel level. The firewall script is the
# only thing that needs root, and we grant it via a narrowly-scoped
# sudoers rule below — not blanket NOPASSWD.

ARG USERNAME=dev
ARG USER_UID=1000
ARG USER_GID=${USER_UID}

RUN groupadd --gid ${USER_GID} ${USERNAME} \
    && useradd --uid ${USER_UID} --gid ${USER_GID} -m --shell /usr/bin/zsh ${USERNAME}

# The rust base image installs the toolchain into /usr/local/{cargo,rustup}
# owned by root. Hand ownership to the dev user so `rustup target add` and
# `cargo install` work without sudo during the rest of the build.
RUN chown -R ${USERNAME}:${USERNAME} /usr/local/cargo /usr/local/rustup

# Workspace, bash history, and Claude config dirs. The two named volumes
# declared in devcontainer.json mount over /commandhistory and
# /home/${USERNAME}/.claude; creating them here ensures correct ownership
# on first start before the volume is populated.
RUN mkdir -p /workspace /commandhistory /home/${USERNAME}/.claude \
    && chown -R ${USERNAME}:${USERNAME} /workspace /commandhistory /home/${USERNAME}/.claude

ENV DEVCONTAINER=true
WORKDIR /workspace
USER ${USERNAME}

# =========================================================
# Rust targets and components
# =========================================================
#
# - wasm32-wasip2: the target every gadget in gadgets/ compiles to.
# - rustfmt, clippy: normal development hygiene.
# - rust-src, rust-analyzer: needed by the rust-analyzer VS Code
#   extension for std library navigation and on-the-fly diagnostics.

RUN rustup target add wasm32-wasip2 \
    && rustup component add rustfmt clippy rust-src rust-analyzer

# =========================================================
# wasm-tools
# =========================================================
#
# Used by `just check-wit` and `just fmt-wit` to inspect and format the
# WIT files that define the gadget ABI. Installing via `cargo install`
# (rather than a prebuilt release binary) avoids having to allowlist an
# extra download domain in the firewall.

RUN cargo install wasm-tools --locked

# =========================================================
# Bun (the project's preferred JS runtime)
# =========================================================

RUN curl -fsSL https://bun.sh/install | bash
ENV BUN_INSTALL=/home/dev/.bun
ENV PATH=${BUN_INSTALL}/bin:${PATH}

# =========================================================
# Claude Code
# =========================================================
#
# The `@anthropic-ai/claude-code` npm package is a Node.js wrapper
# that ships the real Claude binary via platform-specific
# `optionalDependencies` (one per os+arch). Its `postinstall` hook
# (`install.cjs`) detects the current platform, copies the matching
# ~230 MB native binary over the 500-byte `bin/claude.exe`
# placeholder, and marks it executable. Only after that runs is the
# wrapper actually usable.
#
# Bun by default skips all npm lifecycle scripts (`preinstall`,
# `postinstall`, etc.) as a security measure, so `bun add -g`
# extracts the packages but leaves the placeholder in place and
# never links a global `claude` bin. Running the postinstall
# ourselves as an explicit step restores the expected behavior.

RUN bun add -g @anthropic-ai/claude-code@${CLAUDE_CODE_VERSION} \
    && node /home/dev/.bun/install/global/node_modules/@anthropic-ai/claude-code/install.cjs

# =========================================================
# zsh + powerlevel10k
# =========================================================
#
# Same shell setup as the Anthropic reference container. Persistent
# history lives in /commandhistory (a named volume, not a bind mount).

RUN sh -c "$(wget -O- https://github.com/deluan/zsh-in-docker/releases/download/v${ZSH_IN_DOCKER_VERSION}/zsh-in-docker.sh)" -- \
        -p git \
        -p fzf \
        -a "source /usr/share/doc/fzf/examples/key-bindings.zsh" \
        -a "source /usr/share/doc/fzf/examples/completion.zsh" \
        -a "export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
        -a "export PATH=\$HOME/.bun/bin:\$PATH" \
        -x

# =========================================================
# Firewall script + narrow sudo grant
# =========================================================
#
# The dev user gets NOPASSWD permission for exactly one command:
# /usr/local/bin/init-firewall.sh. That script runs once from
# postStartCommand and then the container's network is locked down.

USER root
COPY init-firewall.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/init-firewall.sh \
    && echo "${USERNAME} ALL=(root) NOPASSWD: /usr/local/bin/init-firewall.sh" > /etc/sudoers.d/${USERNAME}-firewall \
    && chmod 0440 /etc/sudoers.d/${USERNAME}-firewall

# Final container-user setup:
#  - Symlink `claude` into /usr/local/bin so it's reachable regardless
#    of how interactive rc files manipulate PATH. The symlink targets
#    the claude-code wrapper package's bin/claude.exe directly, which
#    the postinstall step above populated with the platform-specific
#    native binary. Bun never creates a top-level ~/.bun/bin/claude
#    entry for this package, so pointing at the wrapper's bin dir is
#    both necessary and more stable.
#  - Pre-create /home/dev/.config/git so the devcontainer.json bind
#    mount of the host's XDG global gitignore has a parent dir to land
#    in.
#  - Pre-create /usr/local/cargo/{git,registry} with dev ownership.
#    These paths are mounted as named volumes at runtime. Docker
#    initializes a fresh named volume by copying the image's contents
#    (and ownership) at the mountpoint; without the pre-creation, the
#    empty mountpoint defaults to root ownership and the dev user
#    can't `cargo fetch` into it. `cargo install wasm-tools` above
#    populates `registry/` as a side effect but does not touch `git/`
#    (no git deps), which is why a plain rebuild without this step
#    leaves `git/` root-owned.
#
# Keeping all of this in one RUN step after the expensive cargo
# install preserves its layer cache when any of the one-liner fixes
# below need to change.
RUN ln -sf /home/${USERNAME}/.bun/install/global/node_modules/@anthropic-ai/claude-code/bin/claude.exe /usr/local/bin/claude \
    && mkdir -p /home/${USERNAME}/.config/git \
    && chown -R ${USERNAME}:${USERNAME} /home/${USERNAME}/.config \
    && mkdir -p /usr/local/cargo/git /usr/local/cargo/registry \
    && chown -R ${USERNAME}:${USERNAME} /usr/local/cargo/git /usr/local/cargo/registry

USER ${USERNAME}

ENV SHELL=/bin/zsh
ENV EDITOR=nano
ENV VISUAL=nano
