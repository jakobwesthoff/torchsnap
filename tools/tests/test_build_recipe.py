# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the flags `just build` hands to `tauri build`. They read the
# recipe's commands from `just --dry-run`, which prints them without
# running anything.

import pathlib
import shlex
import shutil
import subprocess
import sys
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent


def dry_run(*args):
    """The lines `just build <args>` would run, as `just --dry-run` prints them."""
    result = subprocess.run(["just", "--dry-run", "build", *args],
                            cwd=REPO, check=True, capture_output=True, text=True)
    return [line for line in result.stderr.splitlines() if line.strip()]


def tauri_build_args(lines):
    """The words of the one `tauri build` line."""
    [line] = [line for line in lines if "tauri build" in line]
    return shlex.split(line)


def contains_pair(words, first, second):
    return any(a == first and b == second for a, b in zip(words, words[1:]))


@unittest.skipUnless(shutil.which("just"), "needs just")
class BuildRecipeTest(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "darwin", "the app-only bundle is the macOS default")
    def test_debug_build_bundles_only_the_app(self):
        words = tauri_build_args(dry_run())
        self.assertIn("--debug", words)
        self.assertTrue(contains_pair(words, "--bundles", "app"), words)

    @unittest.skipUnless(sys.platform == "darwin", "the app-only bundle is the macOS default")
    def test_debug_build_with_config_bundles_only_the_app(self):
        words = tauri_build_args(dry_run("--config", "{}"))
        self.assertTrue(contains_pair(words, "--config", "{}"), words)
        self.assertTrue(contains_pair(words, "--bundles", "app"), words)

    def test_release_build_keeps_every_bundle(self):
        words = tauri_build_args(dry_run("--release"))
        self.assertNotIn("--debug", words)
        self.assertNotIn("--bundles", words)

    def test_signed_debug_build_keeps_every_bundle(self):
        lines = dry_run("--sign")
        words = tauri_build_args(lines)
        self.assertIn("--debug", words)
        self.assertNotIn("--bundles", words)
        # The DMG stays because this step notarizes it.
        self.assertEqual(lines[-1], "just notarize-dmg debug")

    def test_signed_release_build_keeps_every_bundle(self):
        words = tauri_build_args(dry_run("--release", "--sign"))
        self.assertNotIn("--bundles", words)


if __name__ == "__main__":
    unittest.main()
