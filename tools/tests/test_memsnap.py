# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/memsnap. The script is run for real with stub
# `footprint`, `lsappinfo` and `ps` programs on PATH, so the tests need
# neither macOS nor a running Torchsnap. It runs under `/bin/bash`, the
# bash 3.2 of macOS where that is what the system has, because that is
# the interpreter that rejects an empty array expanded under `set -u`.

import os
import pathlib
import subprocess
import tempfile
import unittest

MEMSNAP = pathlib.Path(__file__).resolve().parent.parent / "memsnap"

# Each stub records its arguments, one per line, in `<name>.args`.
STUB = """#!/bin/sh
printf '%s\\n' "$@" > "{dir}/{name}.args"
"""


class MemsnapSampleTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self._tmp.name)
        for name in ("footprint", "lsappinfo", "ps"):
            stub = self.dir / name
            stub.write_text(STUB.format(dir=self.dir, name=name))
            stub.chmod(0o755)

    def tearDown(self):
        self._tmp.cleanup()

    def sample(self, *args):
        """Runs `memsnap sample` for this process's PID; returns the completed process."""
        env = {"PATH": f"{self.dir}:/usr/bin:/bin"}
        return subprocess.run(
            ["/bin/bash", str(MEMSNAP), "sample", "-p", str(os.getpid()), *args],
            env=env,
            capture_output=True,
            text=True,
        )

    def footprint_args(self):
        return (self.dir / "footprint.args").read_text().splitlines()

    def test_sample_without_duration_runs_footprint_unlimited(self):
        result = self.sample("-i", "1", "-o", "out.json")
        self.assertEqual(result.returncode, 0, result.stderr)
        argv = self.footprint_args()
        self.assertNotIn("--sample-duration", argv)
        self.assertEqual(argv[argv.index("--sample") + 1], "1")

    def test_sample_with_duration_passes_it_to_footprint(self):
        result = self.sample("-i", "1", "-d", "7", "-o", "out.json")
        self.assertEqual(result.returncode, 0, result.stderr)
        argv = self.footprint_args()
        self.assertEqual(argv[argv.index("--sample-duration") + 1], "7")


if __name__ == "__main__":
    unittest.main()
