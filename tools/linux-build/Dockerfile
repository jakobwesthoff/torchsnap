# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Build environment for Torchsnap on Linux arm64, Debian 13 (trixie),
# the base of Raspberry Pi OS. `tools/linux-build/linux-build image`
# builds it with `--platform linux/arm64`, which on an Apple silicon
# Mac runs natively.
#
# The dated tag says which trixie snapshot this is, the digest makes the
# pull exact. Packages come from the live trixie archive, the one a
# Raspberry Pi OS machine updates from.
FROM debian:trixie-20261020@sha256:4c25275446ca371766b8b093c696782eabce06263fccacb8d5d813b3dc44b308

# The Docker on the Mac is shared with other projects, so this image
# marks what it creates: the label lets `linux-build` find untagged
# images of this tool, and every RUN below goes through a shell command
# line naming TORCHSNAP_LINUX_BUILD=1, which BuildKit copies into the
# description of each build cache record. `linux-build` measures and
# prunes the cache by that command line and touches no other record.
LABEL org.torchsnap.linux-build=1
SHELL ["/usr/bin/env", "TORCHSNAP_LINUX_BUILD=1", "/bin/sh", "-c"]

ARG RUST_TOOLCHAIN
ARG RUSTUP_VERSION=1.29.2
ARG RUSTUP_SHA256=a9c168dbea81a84671c6b7a5c1795cb0eb699020110d107ddfca427bd797e697
ARG BUN_VERSION=1.4.4
ARG BUN_SHA256=baa6bb36f51d6e58561e02bbea825038a9df454f673d6d776e8bbf86f8c7850e
ARG JUST_VERSION=1.58.0
ARG JUST_SHA256=748237128c4c40cbdabc65e841d05ceba13cc23a91eaba395495894c1d9764df
ARG WASM_TOOLS_VERSION=1.246.2
ARG WASM_TOOLS_SHA256=cd28f8086297b491d857a5a8118d0de3b910f785d0b465defb4ca4d3826937dc

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    BUN_INSTALL_CACHE_DIR=/var/cache/bun \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Tauri's Debian build dependencies, then what `just install` and
# `just build` call: zip packs gadget archives, ImageMagick (`magick`)
# makes the icons, tzdata provides the zone table the timezone data is
# generated from, curl downloads the bang database, git gives
# `src-tauri/build.rs` the commit hash, rsync copies the checkout into
# the work volume. binutils, part of build-essential, has `objdump`.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev \
        libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
        libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
        libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
        zip unzip imagemagick jq curl ca-certificates tzdata git rsync xz-utils \
    && rm -rf /var/lib/apt/lists/*

# rustup with the release `rust-toolchain.toml` pins, plus the target and
# components that file lists, so cargo downloads nothing at build time.
RUN test -n "$RUST_TOOLCHAIN" \
    && curl -fsSL -o /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/aarch64-unknown-linux-gnu/rustup-init" \
    && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN" \
    && rm /tmp/rustup-init \
    && rustup target add wasm32-wasip2 \
    && rustup component add clippy rustfmt

RUN curl -fsSL -o /tmp/bun.zip \
        "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-aarch64.zip" \
    && echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c - \
    && unzip -q -j /tmp/bun.zip bun-linux-aarch64/bun -d /usr/local/bin \
    && rm /tmp/bun.zip

RUN curl -fsSL -o /tmp/just.tar.gz \
        "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-aarch64-unknown-linux-musl.tar.gz" \
    && echo "${JUST_SHA256}  /tmp/just.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/just.tar.gz -C /usr/local/bin just \
    && rm /tmp/just.tar.gz

RUN curl -fsSL -o /tmp/wasm-tools.tar.gz \
        "https://github.com/bytecodealliance/wasm-tools/releases/download/v${WASM_TOOLS_VERSION}/wasm-tools-${WASM_TOOLS_VERSION}-aarch64-linux.tar.gz" \
    && echo "${WASM_TOOLS_SHA256}  /tmp/wasm-tools.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/wasm-tools.tar.gz -C /usr/local/bin --strip-components=1 \
        "wasm-tools-${WASM_TOOLS_VERSION}-aarch64-linux/wasm-tools" \
    && rm /tmp/wasm-tools.tar.gz

# The image is only usable with exactly these versions; a mismatch
# fails the image build here rather than a Torchsnap build later.
RUN rustc --version | grep -q "^rustc ${RUST_TOOLCHAIN} " \
    && rustup target list --installed | grep -qx wasm32-wasip2 \
    && test "$(bun --version)" = "${BUN_VERSION}" \
    && test "$(just --version)" = "just ${JUST_VERSION}" \
    && wasm-tools --version | grep -q "^wasm-tools ${WASM_TOOLS_VERSION} " \
    && magick -version | grep -q "^Version: ImageMagick 7\."
