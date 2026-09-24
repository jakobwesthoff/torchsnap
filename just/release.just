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

# Signing identity and notarization key live outside the repository.
# The file sets APPLE_SIGNING_IDENTITY, APPLE_API_ISSUER, APPLE_API_KEY
# and APPLE_API_KEY_PATH.
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

    just install
    just fullcycle
    just build --release --sign

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

    rm -rf "{{ RELEASE_DIST }}"
    mkdir -p "{{ RELEASE_DIST }}"
    cp "$dmg" "{{ RELEASE_DIST }}/Torchsnap.dmg"
    sha256=$(shasum -a 256 "{{ RELEASE_DIST }}/Torchsnap.dmg" | cut -d' ' -f1)
    jq -n --arg version "$version" --arg commit "$(git rev-parse HEAD)" --arg sha256 "$sha256" \
        '{version: $version, commit: $commit, sha256: $sha256}' > "{{ RELEASE_DIST }}/release.json"

    echo
    echo "Staged {{ RELEASE_DIST }}/Torchsnap.dmg for v$version (sha256 $sha256)."
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
    [ "$sha256" = "$(jq -r .sha256 "$dist/release.json")" ] || fail "$dist/Torchsnap.dmg changed since release-build"

    [ "$(git rev-parse --abbrev-ref HEAD)" = main ] || fail "not on main"
    [ -z "$(git status --porcelain)" ] || fail "working tree has uncommitted or untracked changes"
    git fetch --quiet origin main
    [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || fail "main differs from origin/main"
    if git rev-parse -q --verify "refs/tags/v$version" >/dev/null \
        || git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null; then
        fail "tag v$version already exists"
    fi

    # Release notes: the CHANGELOG section of this version, up to the next
    # section or the link definitions at the end of the file, followed by
    # the checksum and a pointer to the installation guide.
    notes="$dist/notes.md"
    awk -v head="## [$version] - " '
        index($0, head) == 1 { found = 1; next }
        found && (/^## \[/ || /^\[[^]]+\]: /) { exit }
        found { print }
    ' CHANGELOG.md | sed -e '/./,$!d' | awk 'NF { while (blank > 0) { print ""; blank-- } print; next } { blank++ }' > "$notes"
    [ -s "$notes" ] || fail "CHANGELOG section for $version is empty"
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
    gh release create "v$version" "$dist/Torchsnap.dmg" --verify-tag \
        --title "Torchsnap $version" --notes-file "$notes" "${release_kind[@]}"

    assets=$(gh release view "v$version" --json assets -q '[.assets[].name] | join(",")')
    [ "$assets" = "Torchsnap.dmg" ] || fail "release v$version has assets '$assets', expected Torchsnap.dmg"
    repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
    if [[ "$version" != *-* ]]; then
        latest=$(gh api "repos/$repo/releases/latest" -q .tag_name)
        [ "$latest" = "v$version" ] || fail "latest release is $latest, not v$version"
        # The public download URL only answers for a public repository.
        if [ "$(gh repo view --json visibility -q .visibility)" = PUBLIC ]; then
            status=$(curl -sIL -o /dev/null -w '%{http_code}' "https://github.com/$repo/releases/latest/download/Torchsnap.dmg")
            [ "$status" = 200 ] || fail "releases/latest/download/Torchsnap.dmg answers $status"
        else
            echo "Repository is private; skipped the public download URL check."
        fi
    fi

    echo
    echo "Published https://github.com/$repo/releases/tag/v$version"
