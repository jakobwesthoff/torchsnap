# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/release-feed. The tool has no `.py` suffix, so it is
# loaded by path.

import base64
import importlib.machinery
import importlib.util
import pathlib
import unittest

_path = pathlib.Path(__file__).resolve().parent.parent / "release-feed"
_loader = importlib.machinery.SourceFileLoader("release_feed", str(_path))
_spec = importlib.util.spec_from_loader("release_feed", _loader)
release_feed = importlib.util.module_from_spec(_spec)
_loader.exec_module(release_feed)

CHANGELOG = """# Changelog

Intro text.

## [Unreleased]

### Added

- Something not released yet.

## [0.12.0] - 2026-10-02

### Added

- Updates.

  With a second paragraph.

## [0.12.0-beta.1] - 2026-09-30

### Added

- Beta only.

## [0.11.1] - 2026-09-24

### Fixed

- A costume.

[0.12.0]: https://example.org/v0.12.0
[0.11.1]: https://example.org/v0.11.1
"""


def minisign_text(key_id, comment=None):
    """Base64 of a minisign-shaped text file with the given 8-byte key id."""
    lines = ["untrusted comment: test", base64.b64encode(b"Ed" + key_id + b"\0" * 32).decode()]
    if comment is not None:
        lines += [f"trusted comment: {comment}", base64.b64encode(b"\0" * 64).decode()]
    return base64.b64encode("\n".join(lines).encode()).decode()


KEY = b"\x01" * 8
PUBKEY = minisign_text(KEY)


def signature(version, key=KEY):
    return minisign_text(key, f"timestamp:1\tfile:Torchsnap.app.tar.gz\tversion:{version}")


class ChangelogSections(unittest.TestCase):
    def test_dated_sections_newest_first_without_unreleased(self):
        versions = [v for v, _, _ in release_feed.changelog_sections(CHANGELOG)]
        self.assertEqual(versions, ["0.12.0", "0.12.0-beta.1", "0.11.1"])

    def test_notes_keep_inner_blank_lines_and_drop_outer_ones(self):
        notes = dict((v, n) for v, _, n in release_feed.changelog_sections(CHANGELOG))
        self.assertEqual(notes["0.12.0"], "### Added\n\n- Updates.\n\n  With a second paragraph.")

    def test_last_section_stops_at_link_definitions(self):
        notes = dict((v, n) for v, _, n in release_feed.changelog_sections(CHANGELOG))
        self.assertEqual(notes["0.11.1"], "### Fixed\n\n- A costume.")


class BuildFeed(unittest.TestCase):
    def feed(self, version="0.12.0"):
        return release_feed.build_feed(
            CHANGELOG, version, "2026-10-02T14:03:00Z", "abc123", "f00d", signature(version) + "\n", "owner/repo"
        )

    def test_plugin_fields(self):
        feed = self.feed()
        self.assertEqual(feed["version"], "0.12.0")
        self.assertEqual(feed["pub_date"], "2026-10-02T14:03:00Z")
        self.assertTrue(feed["notes"].startswith("### Added"))

    def test_archive_url_is_pinned_to_the_version(self):
        platform = self.feed()["platforms"]["darwin-aarch64"]
        self.assertEqual(
            platform["url"],
            "https://github.com/owner/repo/releases/download/v0.12.0/Torchsnap.app.tar.gz",
        )
        self.assertEqual(platform["signature"], signature("0.12.0"))

    def test_releases_leave_out_prereleases(self):
        releases = [r["version"] for r in self.feed()["releases"]]
        self.assertEqual(releases, ["0.12.0", "0.11.1"])

    def test_receipt_fields(self):
        feed = self.feed()
        self.assertEqual((feed["commit"], feed["dmg_sha256"]), ("abc123", "f00d"))

    def test_missing_section_is_an_error(self):
        with self.assertRaisesRegex(release_feed.FeedError, "no dated section for 0.13.0"):
            self.feed("0.13.0")

    def test_version_must_be_semver(self):
        with self.assertRaisesRegex(release_feed.FeedError, "not a semver"):
            self.feed("v0.12")


class CheckSignature(unittest.TestCase):
    def test_matching_key_and_version_pass(self):
        release_feed.check_signature(signature("0.12.0"), PUBKEY, "0.12.0")

    def test_other_version_fails(self):
        with self.assertRaisesRegex(release_feed.FeedError, "version:0.12.0"):
            release_feed.check_signature(signature("0.11.1"), PUBKEY, "0.12.0")

    def test_version_prefix_does_not_count(self):
        with self.assertRaisesRegex(release_feed.FeedError, "version:0.12.0"):
            release_feed.check_signature(signature("0.12.0-beta.1"), PUBKEY, "0.12.0")

    def test_signature_without_version_fails(self):
        unversioned = minisign_text(KEY, "timestamp:1\tfile:Torchsnap.app.tar.gz")
        with self.assertRaisesRegex(release_feed.FeedError, "version:0.12.0"):
            release_feed.check_signature(unversioned, PUBKEY, "0.12.0")

    def test_other_key_fails(self):
        with self.assertRaisesRegex(release_feed.FeedError, "another key"):
            release_feed.check_signature(signature("0.12.0", key=b"\x02" * 8), PUBKEY, "0.12.0")


if __name__ == "__main__":
    unittest.main()
