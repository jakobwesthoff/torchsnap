# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Releases are built, signed, notarized and published from a developer
# Mac, so no signing credentials ever reach GitHub (ADR 0050). A release
# takes two steps:
#
#   just release-build <version>    checks, builds and stages the DMG
#   just release-publish <version>  tags, pushes and creates the release
#
# The split leaves room to try the staged DMG before anything becomes
# public. `release-build` records version, commit and checksum of what it
# staged in `release.json`, and `release-publish` refuses to publish
# anything else.
#
# Version bump and CHANGELOG section are prepared by hand and committed
# before `release-build`; both recipes only check them.

RELEASE_DIST := "src-tauri/target/release/dist"
RELEASE_REPO := "jakobwesthoff/torchsnap"
WEBSITE_REPO := "jakobwesthoff/torchsnap-web"

# Signing identity, notarization key and updater key live outside the
# repository. The file sets APPLE_SIGNING_IDENTITY, APPLE_API_ISSUER,
# APPLE_API_KEY, APPLE_API_KEY_PATH, TORCHSNAP_UPDATER_KEY_PATH and
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD.
RELEASE_ENV := env('TORCHSNAP_RELEASE_ENV', env('HOME', '') / ".config/torchsnap/release.env")

# Build, notarize and stage a release DMG (checks git, versions, CHANGELOG)
release-build version:
    #!/usr/bin/env bash
    set -euo pipefail
    version="{{ version }}"
    fail() { echo "release-build: $*" >&2; exit 1; }

    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
        || fail "'$version' is not a semver version such as 0.10.0 or 0.10.0-beta.1"

    # The release must be built from a commit that is already public, so
    # anyone can check out exactly what was shipped.
    [ "$(git rev-parse --abbrev-ref HEAD)" = main ] || fail "not on main"
    [ -z "$(git status --porcelain)" ] || fail "working tree has uncommitted or untracked changes"
    git fetch --quiet origin main
    [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] \
        || fail "main differs from origin/main; pull or push first"
    if git rev-parse -q --verify "refs/tags/v$version" >/dev/null \
        || git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null; then
        fail "tag v$version already exists"
    fi

    # Every place that carries the app version must agree with the
    # requested one.
    pkg_version=$(jq -r .version package.json)
    conf_version=$(jq -r .version src-tauri/tauri.conf.json)
    cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -1)
    for pair in "package.json:$pkg_version" "src-tauri/tauri.conf.json:$conf_version" "src-tauri/Cargo.toml:$cargo_version"; do
        [ "${pair#*:}" = "$version" ] || fail "${pair%%:*} has version ${pair#*:}, expected $version"
    done

    # The section date has to be today, so a section prepared for an
    # earlier attempt gets looked at again.
    today=$(date +%F)
    grep -qxF "## [$version] - $today" CHANGELOG.md \
        || fail "CHANGELOG.md has no section '## [$version] - $today'"
    grep -qF "[$version]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v$version" CHANGELOG.md \
        || fail "CHANGELOG.md has no link definition for [$version]"

    env_file="{{ RELEASE_ENV }}"
    [ -f "$env_file" ] || fail "$env_file not found (see README, 'Releasing')"
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
    for var in APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH; do
        [ -n "${!var:-}" ] || fail "$var is not set in $env_file"
    done
    [ "$APPLE_SIGNING_IDENTITY" != "-" ] || fail "a release needs a Developer ID, not ad-hoc signing"
    security find-identity -v -p codesigning | grep -qF "\"$APPLE_SIGNING_IDENTITY\"" \
        || fail "signing identity '$APPLE_SIGNING_IDENTITY' is not in the keychain"
    [ -f "$APPLE_API_KEY_PATH" ] || fail "API key file $APPLE_API_KEY_PATH not found"
    for var in TORCHSNAP_UPDATER_KEY_PATH TAURI_SIGNING_PRIVATE_KEY_PASSWORD; do
        [ -n "${!var:-}" ] || fail "$var is not set in $env_file"
    done
    [ -f "$TORCHSNAP_UPDATER_KEY_PATH" ] || fail "updater key $TORCHSNAP_UPDATER_KEY_PATH not found"
    # The Tauri CLI takes the key content from this variable and ignores
    # TAURI_SIGNING_PRIVATE_KEY_PATH (ADR 0053).
    TAURI_SIGNING_PRIVATE_KEY=$(cat "$TORCHSNAP_UPDATER_KEY_PATH")
    export TAURI_SIGNING_PRIVATE_KEY

    just install
    just fullcycle
    # The update archive and its signature are only made for releases,
    # so everyday builds need no updater key (ADR 0053).
    just build --release --sign --config '{"bundle":{"createUpdaterArtifacts":true}}'

    # `bun install` or a generator could have rewritten a tracked file;
    # the shipped build would then not match the tagged commit.
    [ -z "$(git status --porcelain)" ] || fail "the build changed tracked files: $(git status --porcelain | head -5)"

    name=$(jq -r .productName src-tauri/tauri.conf.json)
    app="src-tauri/target/release/bundle/macos/$name.app"
    dmg="src-tauri/target/release/bundle/dmg/${name}_${version}_aarch64.dmg"
    [ -d "$app" ] || fail "$app not found"
    [ -f "$dmg" ] || fail "$dmg not found"

    # Check what a downloader's Gatekeeper will check, plus the facts
    # ADR 0048 promises about the release.
    spctl -a -vv -t exec "$app" 2>&1 | grep -q 'source=Notarized Developer ID' \
        || fail "Gatekeeper does not accept $app as notarized"
    spctl -a -vv -t open --context context:primary-signature "$dmg" 2>&1 | grep -q 'source=Notarized Developer ID' \
        || fail "Gatekeeper does not accept $dmg as notarized"
    xcrun stapler validate -q "$app" || fail "$app has no stapled ticket"
    xcrun stapler validate -q "$dmg" || fail "$dmg has no stapled ticket"
    executable=$(/usr/libexec/PlistBuddy -c 'Print CFBundleExecutable' "$app/Contents/Info.plist")
    [ "$(lipo -archs "$app/Contents/MacOS/$executable")" = arm64 ] || fail "$app is not an arm64-only build"
    bundle_version=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$app/Contents/Info.plist")
    [ "$bundle_version" = "$version" ] || fail "$app reports version $bundle_version"
    tools/verify-bundle-plist "$app" >/dev/null || fail "$app does not register .torchsnap files as decided"

    # Installed apps replace themselves with the app inside the update
    # archive, so that copy has to pass the same Gatekeeper checks.
    archive="src-tauri/target/release/bundle/macos/$name.app.tar.gz"
    [ -f "$archive" ] || fail "$archive not found"
    [ -f "$archive.sig" ] || fail "$archive.sig not found"
    unpacked=$(mktemp -d)
    trap 'rm -rf "$unpacked"' EXIT
    tar -xzf "$archive" -C "$unpacked"
    [ -d "$unpacked/$name.app" ] || fail "$archive does not contain $name.app"
    spctl -a -vv -t exec "$unpacked/$name.app" 2>&1 | grep -q 'source=Notarized Developer ID' \
        || fail "Gatekeeper does not accept the app in $archive as notarized"
    xcrun stapler validate -q "$unpacked/$name.app" || fail "the app in $archive has no stapled ticket"
    archived_version=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$unpacked/$name.app/Contents/Info.plist")
    [ "$archived_version" = "$version" ] || fail "the app in $archive reports version $archived_version"

    dist="{{ RELEASE_DIST }}"
    rm -rf "$dist"
    mkdir -p "$dist"
    cp "$dmg" "$dist/Torchsnap.dmg"
    cp "$archive" "$dist/Torchsnap.app.tar.gz"
    cp "$archive.sig" "$dist/Torchsnap.app.tar.gz.sig"
    sha256=$(shasum -a 256 "$dist/Torchsnap.dmg" | cut -d' ' -f1)
    # `release-feed` also checks that the signature names this version
    # and comes from the key in `plugins.updater.pubkey`.
    tools/release-feed \
        --changelog CHANGELOG.md \
        --version "$version" \
        --pub-date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --commit "$(git rev-parse HEAD)" \
        --dmg-sha256 "$sha256" \
        --signature "$dist/Torchsnap.app.tar.gz.sig" \
        --pubkey "$(jq -r .plugins.updater.pubkey src-tauri/tauri.conf.json)" \
        --repo "{{ RELEASE_REPO }}" \
        > "$dist/release.json" \
        || fail "could not write $dist/release.json"

    echo
    echo "Staged Torchsnap.dmg, the update archive and release.json for v$version in $dist (DMG sha256 $sha256)."
    echo "Try it, then run: just release-publish $version"

# Tag, push and publish the release staged by release-build
release-publish version:
    #!/usr/bin/env bash
    set -euo pipefail
    version="{{ version }}"
    dist="{{ RELEASE_DIST }}"
    fail() { echo "release-publish: $*" >&2; exit 1; }

    [ -f "$dist/release.json" ] || fail "nothing staged; run: just release-build $version"
    [ "$(jq -r .version "$dist/release.json")" = "$version" ] \
        || fail "staged release is $(jq -r .version "$dist/release.json"), not $version"
    [ "$(jq -r .commit "$dist/release.json")" = "$(git rev-parse HEAD)" ] \
        || fail "HEAD moved since release-build; build again"
    sha256=$(shasum -a 256 "$dist/Torchsnap.dmg" | cut -d' ' -f1)
    [ "$sha256" = "$(jq -r .dmg_sha256 "$dist/release.json")" ] || fail "$dist/Torchsnap.dmg changed since release-build"
    for asset in Torchsnap.app.tar.gz Torchsnap.app.tar.gz.sig; do
        [ -f "$dist/$asset" ] || fail "$dist/$asset is missing; build again"
    done
    [ "$(tr -d '[:space:]' < "$dist/Torchsnap.app.tar.gz.sig")" = "$(jq -r '.platforms["darwin-aarch64"].signature' "$dist/release.json")" ] \
        || fail "$dist/Torchsnap.app.tar.gz.sig does not match release.json; build again"

    [ "$(git rev-parse --abbrev-ref HEAD)" = main ] || fail "not on main"
    [ -z "$(git status --porcelain)" ] || fail "working tree has uncommitted or untracked changes"
    git fetch --quiet origin main
    [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || fail "main differs from origin/main"
    if git rev-parse -q --verify "refs/tags/v$version" >/dev/null \
        || git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null; then
        fail "tag v$version already exists"
    fi

    # Release notes: the CHANGELOG section `release-build` put into
    # release.json, followed by a pointer to the installation guide and
    # the checksum.
    notes="$dist/notes.md"
    jq -r .notes "$dist/release.json" > "$notes"
    [ -s "$notes" ] || fail "release.json has no notes for $version"
    {
        echo
        echo "Installation: https://docs.torchsnap.app/start/installation/"
        echo
        echo "SHA-256 of \`Torchsnap.dmg\`: \`$sha256\`"
    } >> "$notes"

    # A pre-release must not become "latest": the website's download link
    # points at the latest release's Torchsnap.dmg (ADR 0048).
    if [[ "$version" == *-* ]]; then
        release_kind=(--prerelease --latest=false)
    else
        release_kind=(--latest)
    fi

    git tag -a "v$version" -m "Torchsnap $version"
    git push origin "v$version"
    gh release create "v$version" \
        "$dist/Torchsnap.dmg" "$dist/Torchsnap.app.tar.gz" "$dist/Torchsnap.app.tar.gz.sig" "$dist/release.json" \
        --verify-tag --title "Torchsnap $version" --notes-file "$notes" "${release_kind[@]}"

    expected_assets="Torchsnap.app.tar.gz,Torchsnap.app.tar.gz.sig,Torchsnap.dmg,release.json"
    assets=$(gh release view "v$version" --json assets -q '[.assets[].name] | sort | join(",")')
    [ "$assets" = "$expected_assets" ] || fail "release v$version has assets '$assets', expected $expected_assets"
    repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
    if [[ "$version" != *-* ]]; then
        latest=$(gh api "repos/$repo/releases/latest" -q .tag_name)
        [ "$latest" = "v$version" ] || fail "latest release is $latest, not v$version"
        # The public download URLs only answer for a public repository.
        if [ "$(gh repo view --json visibility -q .visibility)" = PUBLIC ]; then
            for asset in Torchsnap.dmg release.json; do
                status=$(curl -sIL -o /dev/null -w '%{http_code}' "https://github.com/$repo/releases/latest/download/$asset")
                [ "$status" = 200 ] || fail "releases/latest/download/$asset answers $status"
            done
        else
            echo "Repository is private; skipped the public download URL checks."
        fi

        # torchsnap.app serves the latest release.json as the update feed
        # and only picks up a new one when it is rebuilt (ADR 0053).
        gh workflow run deploy.yml -R "{{ WEBSITE_REPO }}" \
            || fail "published, but could not start the {{ WEBSITE_REPO }} deploy; start it by hand"
        echo "Started the {{ WEBSITE_REPO }} deploy; installed apps see v$version once it finished."
    fi

    echo
    echo "Published https://github.com/$repo/releases/tag/v$version"
