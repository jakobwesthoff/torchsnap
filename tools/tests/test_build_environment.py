# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the build environment that `.cargo/config.toml` gives cargo.
# The file is read as text: `tomllib` needs Python 3.11, and the CI
# runner's `python3` is not pinned.

import json
import pathlib
import re
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
CARGO_CONFIG = REPO / ".cargo/config.toml"
TAURI_CONF = REPO / "src-tauri/tauri.conf.json"


def env_section(text):
    """The lines of the `[env]` table, up to the next table header."""
    match = re.search(r"^\[env\]\n(.*?)(?=^\[|\Z)", text, re.MULTILINE | re.DOTALL)
    return match.group(1) if match else ""


class DeploymentTargetTest(unittest.TestCase):
    def setUp(self):
        self.assertTrue(CARGO_CONFIG.is_file(), ".cargo/config.toml is missing")
        self.env = env_section(CARGO_CONFIG.read_text())

    def test_cargo_pins_the_bundle_minimum_macos_version(self):
        match = re.search(r'^MACOSX_DEPLOYMENT_TARGET = "([^"]+)"$', self.env, re.MULTILINE)
        pinned = match.group(1) if match else None
        conf = json.loads(TAURI_CONF.read_text())
        self.assertEqual(pinned, conf["bundle"]["macOS"].get("minimumSystemVersion"))

    def test_the_pin_yields_to_the_environment(self):
        # `tauri build` sets the variable itself. A plain string value
        # leaves cargo's `force` off, so that value wins over this one.
        line = re.search(r"^MACOSX_DEPLOYMENT_TARGET = .*$", self.env, re.MULTILINE)
        self.assertIsNotNone(line, "[env] does not set MACOSX_DEPLOYMENT_TARGET")
        self.assertRegex(line.group(0), r'^MACOSX_DEPLOYMENT_TARGET = "[^"]+"$')


if __name__ == "__main__":
    unittest.main()
