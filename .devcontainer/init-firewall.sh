#!/bin/bash
# =========================================================
# Attribution
# =========================================================
#
# Adapted from the Anthropic Claude Code reference devcontainer:
#   https://github.com/anthropics/claude-code/tree/main/.devcontainer
#
# The upstream script is © Anthropic PBC, "All rights reserved" under
# Anthropic's Commercial Terms of Service. We reproduce and adapt it
# here in reliance on the implied license granted by Anthropic's public
# documentation at https://docs.claude.com/en/docs/claude-code/devcontainer,
# which directs users to adopt and customize these files.
#
# Torchsnap-specific additions (extended allowlist, extra verification
# probe) are original work; this file as a whole is therefore NOT
# covered by the project's MPL-2.0 license. See .devcontainer/NOTICE.md
# for full provenance details.

# =========================================================
# Torchsnap devcontainer firewall
# =========================================================
#
# Runs once from postStartCommand inside the container. Sets an egress
# allowlist via iptables + ipset so that even a compromised Claude
# session cannot exfiltrate to arbitrary destinations.
#
# Derived from the Anthropic reference container, extended with the
# domains Torchsnap specifically needs: crates.io (two hosts for the
# sparse-index + static tarballs), the Rust static distribution server
# used by rustup, and nothing else on top of the baseline.
#
# The script is fail-closed: if any step errors out, default policies
# stay at DROP and the container has no network.

set -euo pipefail
IFS=$'\n\t'

# ---------------------------------------------------------
# Preserve Docker's internal DNS rules across the flush.
# ---------------------------------------------------------
#
# Docker installs NAT rules for 127.0.0.11 so containers can resolve
# other containers by name. Flushing iptables below would delete them,
# so we capture them first and restore them afterwards.

DOCKER_DNS_RULES=$(iptables-save -t nat | grep "127\.0\.0\.11" || true)

iptables -F
iptables -X
iptables -t nat -F
iptables -t nat -X
iptables -t mangle -F
iptables -t mangle -X
ipset destroy allowed-domains 2>/dev/null || true

if [ -n "$DOCKER_DNS_RULES" ]; then
    echo "Restoring Docker DNS rules..."
    iptables -t nat -N DOCKER_OUTPUT 2>/dev/null || true
    iptables -t nat -N DOCKER_POSTROUTING 2>/dev/null || true
    echo "$DOCKER_DNS_RULES" | xargs -L 1 iptables -t nat
else
    echo "No Docker DNS rules to restore"
fi

# ---------------------------------------------------------
# Baseline: DNS, SSH, loopback
# ---------------------------------------------------------

iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
iptables -A INPUT  -p udp --sport 53 -j ACCEPT
iptables -A OUTPUT -p tcp --dport 22 -j ACCEPT
iptables -A INPUT  -p tcp --sport 22 -m state --state ESTABLISHED -j ACCEPT
iptables -A INPUT  -i lo -j ACCEPT
iptables -A OUTPUT -o lo -j ACCEPT

# ---------------------------------------------------------
# Allowlist set
# ---------------------------------------------------------

ipset create allowed-domains hash:net

# GitHub (web, api, git). The meta endpoint publishes the canonical
# CIDR list, so we pull it and load every range into the ipset.
echo "Fetching GitHub IP ranges..."
gh_ranges=$(curl -s https://api.github.com/meta)
if [ -z "$gh_ranges" ]; then
    echo "ERROR: Failed to fetch GitHub IP ranges"
    exit 1
fi

if ! echo "$gh_ranges" | jq -e '.web and .api and .git' >/dev/null; then
    echo "ERROR: GitHub API response missing required fields"
    exit 1
fi

echo "Processing GitHub IPs..."
while read -r cidr; do
    if [[ ! "$cidr" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}/[0-9]{1,2}$ ]]; then
        echo "ERROR: Invalid CIDR range from GitHub meta: $cidr"
        exit 1
    fi
    echo "Adding GitHub range $cidr"
    # Use `-exist` so overlapping CIDRs or duplicate entries in the
    # upstream feed don't abort the script under `set -e`.
    ipset add allowed-domains "$cidr" -exist
done < <(echo "$gh_ranges" | jq -r '(.web + .api + .git)[]' | aggregate -q)

# Everything else is resolved via DNS. Keep this list short and
# reviewable: every entry is a potential exfil channel.
#
# - registry.npmjs.org:      bun / npm package downloads
# - api.anthropic.com:       Claude Code's primary endpoint
# - sentry.io, statsig.*:    Claude Code telemetry
# - marketplace.visualstudio.com,
#   vscode.blob.core.windows.net,
#   update.code.visualstudio.com: VS Code extension installs
# - index.crates.io:         cargo sparse protocol index
# - static.crates.io:        cargo crate tarball downloads
# - crates.io:               fallback / metadata queries
# - static.rust-lang.org:    rustup toolchain updates
# - sh.rustup.rs:            rustup installer (rarely needed post-image)
# - duckduckgo.com:          `just asset-bang-data` downloads bang.js
for domain in \
    "registry.npmjs.org" \
    "api.anthropic.com" \
    "sentry.io" \
    "statsig.anthropic.com" \
    "statsig.com" \
    "marketplace.visualstudio.com" \
    "vscode.blob.core.windows.net" \
    "update.code.visualstudio.com" \
    "index.crates.io" \
    "static.crates.io" \
    "crates.io" \
    "static.rust-lang.org" \
    "sh.rustup.rs" \
    "duckduckgo.com"; do
    echo "Resolving $domain..."
    ips=$(dig +noall +answer A "$domain" | awk '$4 == "A" {print $5}')
    if [ -z "$ips" ]; then
        echo "ERROR: Failed to resolve $domain"
        exit 1
    fi

    while read -r ip; do
        if [[ ! "$ip" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
            echo "ERROR: Invalid IP from DNS for $domain: $ip"
            exit 1
        fi
        echo "Adding $ip for $domain"
        # `-exist` absorbs the common case of two domains resolving to
        # the same CDN IP (e.g. `index.crates.io` and `static.crates.io`
        # both live on Fastly) without tripping `set -e`.
        ipset add allowed-domains "$ip" -exist
    done < <(echo "$ips")
done

# ---------------------------------------------------------
# Host-network allowance
# ---------------------------------------------------------
#
# Allow traffic to the Docker host's /24. This keeps IDE features like
# VS Code's dev-container server working — the VS Code server on the
# host talks to the container over that network.

HOST_IP=$(ip route | grep default | cut -d" " -f3)
if [ -z "$HOST_IP" ]; then
    echo "ERROR: Failed to detect host IP"
    exit 1
fi

HOST_NETWORK=$(echo "$HOST_IP" | sed "s/\.[0-9]*$/.0\/24/")
echo "Host network detected as: $HOST_NETWORK"

iptables -A INPUT  -s "$HOST_NETWORK" -j ACCEPT
iptables -A OUTPUT -d "$HOST_NETWORK" -j ACCEPT

# ---------------------------------------------------------
# Default DROP + allowlist ACCEPT
# ---------------------------------------------------------

iptables -P INPUT   DROP
iptables -P FORWARD DROP
iptables -P OUTPUT  DROP

iptables -A INPUT  -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

iptables -A OUTPUT -m set --match-set allowed-domains dst -j ACCEPT

# REJECT (rather than silently DROP) anything else so Rust / bun errors
# surface immediately instead of hanging on a TCP timeout.
iptables -A OUTPUT -j REJECT --reject-with icmp-admin-prohibited

# ---------------------------------------------------------
# Post-install verification
# ---------------------------------------------------------
#
# Asserting both a negative and a positive probe catches the worst
# failure mode: a broken firewall that is *more* permissive than
# intended. If either probe disagrees with expectation, exit non-zero
# so the container is clearly marked as misconfigured.

echo "Firewall configuration complete"
echo "Verifying firewall rules..."
if curl --connect-timeout 5 https://example.com >/dev/null 2>&1; then
    echo "ERROR: Firewall verification failed - was able to reach https://example.com"
    exit 1
else
    echo "Firewall verification passed - unable to reach https://example.com as expected"
fi

if ! curl --connect-timeout 5 https://api.github.com/zen >/dev/null 2>&1; then
    echo "ERROR: Firewall verification failed - unable to reach https://api.github.com"
    exit 1
else
    echo "Firewall verification passed - able to reach https://api.github.com as expected"
fi

if ! curl --connect-timeout 5 https://index.crates.io/config.json >/dev/null 2>&1; then
    echo "ERROR: Firewall verification failed - unable to reach https://index.crates.io"
    exit 1
else
    echo "Firewall verification passed - able to reach https://index.crates.io as expected"
fi
