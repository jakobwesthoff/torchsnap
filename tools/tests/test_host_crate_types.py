# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the crate types of the host library, as ADR 0070 decides
# them. Cargo reports them through `cargo metadata`, which reads the
# manifest without building anything.

import json
import pathlib
import shutil
import subprocess
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
MANIFEST = REPO / "src-tauri/Cargo.toml"

# A `[lib]` target's `kind` lists its crate types, or `lib` when the
# manifest leaves them at the default.
LIBRARY_KINDS = {"lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"}


def host_targets():
    """The build targets of the host package, as `cargo metadata` lists them."""
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--offline", "--format-version", "1",
         "--manifest-path", str(MANIFEST)],
        cwd=REPO, check=True, capture_output=True, text=True)
    [package] = json.loads(result.stdout)["packages"]
    return package["targets"]


class HostCrateTypesTest(unittest.TestCase):
    def setUp(self):
        # Without cargo the pin cannot be checked, and a skipped test
        # would read as a pass.
        self.assertIsNotNone(shutil.which("cargo"), "cargo is not on PATH")
        libs = [target for target in host_targets()
                if LIBRARY_KINDS.intersection(target["kind"])]
        self.assertEqual([lib["name"] for lib in libs], ["torchsnap_lib"])
        self.lib = libs[0]

    def test_the_host_library_is_built_as_rlib_only(self):
        self.assertEqual(self.lib["crate_types"], ["rlib"])


if __name__ == "__main__":
    unittest.main()
