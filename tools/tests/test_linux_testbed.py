# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the Linux testbed scripts under tools/linux-testbed/. They run
# the scripts as subprocesses with a PATH whose first directory holds
# stubs for the commands that would reach the Pi (ssh, rsync, ...). Each
# stub appends its argv to a log as one JSON line and answers with the
# output and exit code the test sets, so the tests need no Pi and run on
# the macOS CI runner as well.

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import textwrap
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
TESTBED = REPO / "tools" / "linux-testbed" / "testbed"

# A stub reads STUB_<NAME>_STDOUT and STUB_<NAME>_EXIT, with NAME the
# command name in upper case and `-` replaced by `_`.
STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    name = os.path.basename(sys.argv[0])
    key = name.upper().replace("-", "_")
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(sys.argv) + "\\n")
    sys.stdout.write(os.environ.get(f"STUB_{key}_STDOUT", ""))
    sys.exit(int(os.environ.get(f"STUB_{key}_EXIT", "0")))
    """
)


class Stubs:
    """A temporary stub directory and the log its stubs write to."""

    def __init__(self, names):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="testbed-stubs-"))
        self.log = self.dir / "calls.jsonl"
        for name in names:
            path = self.dir / name
            path.write_text(STUB)
            path.chmod(0o755)

    def calls(self, name=None):
        if not self.log.exists():
            return []
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        if name is None:
            return calls
        return [c for c in calls if os.path.basename(c[0]) == name]

    def cleanup(self):
        shutil.rmtree(self.dir)


def run(args, stubs, env=None, unset=()):
    environment = dict(os.environ)
    for name in unset:
        environment.pop(name, None)
    environment["PATH"] = f"{stubs.dir}{os.pathsep}{os.environ['PATH']}"
    environment["STUB_LOG"] = str(stubs.log)
    environment.update(env or {})
    return subprocess.run(
        ["bash", str(TESTBED), *args],
        capture_output=True,
        text=True,
        env=environment,
        timeout=60,
    )


class TestbedCommandLine(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["ssh", "rsync", "scp"])

    def tearDown(self):
        self.stubs.cleanup()

    def test_no_subcommand_exits_2_with_usage(self):
        result = run([], self.stubs, env={"TORCHSNAP_LINUX_HOST": "user@pi.local"})
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage:", result.stderr.lower())

    def test_unknown_subcommand_exits_2_with_usage(self):
        result = run(["frobnicate"], self.stubs, env={"TORCHSNAP_LINUX_HOST": "user@pi.local"})
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage:", result.stderr.lower())
        self.assertEqual(self.stubs.calls(), [])


class TestbedHostCheck(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["ssh", "rsync", "scp"])

    def tearDown(self):
        self.stubs.cleanup()

    def assert_host_refused(self, result):
        self.assertEqual(result.returncode, 2)
        self.assertIn("TORCHSNAP_LINUX_HOST is not set", result.stderr)
        self.assertEqual(self.stubs.calls(), [])

    def test_sync_refuses_without_host(self):
        self.assert_host_refused(run(["sync"], self.stubs, unset=["TORCHSNAP_LINUX_HOST"]))

    def test_sync_refuses_an_empty_host(self):
        self.assert_host_refused(run(["sync"], self.stubs, env={"TORCHSNAP_LINUX_HOST": ""}))


class TestbedSync(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["ssh", "rsync", "scp"])

    def tearDown(self):
        self.stubs.cleanup()

    def test_sync_mirrors_the_repository_without_ignored_files(self):
        result = run(["sync"], self.stubs, env={"TORCHSNAP_LINUX_HOST": "user@pi.local"})
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.stubs.calls("rsync")
        self.assertEqual(len(calls), 1)
        argv = calls[0][1:]
        self.assertIn("-a", argv)
        self.assertIn("--delete", argv)
        self.assertIn("--filter=:- .gitignore", argv)
        self.assertIn("--exclude=.git", argv)
        self.assertEqual(argv[-2], f"{REPO}/")
        self.assertEqual(argv[-1], "user@pi.local:torchsnap/")

    def test_sync_passes_an_rsync_failure_through(self):
        result = run(
            ["sync"],
            self.stubs,
            env={"TORCHSNAP_LINUX_HOST": "user@pi.local", "STUB_RSYNC_EXIT": "23"},
        )
        self.assertEqual(result.returncode, 23)


class TestbedRecipes(unittest.TestCase):
    """The recipes in just/linux-testbed.just only forward to the script."""

    def dry_run(self, *args):
        if shutil.which("just") is None:
            self.skipTest("just is not installed")
        return subprocess.run(
            ["just", "--justfile", str(REPO / "Justfile"), "--dry-run", *args],
            capture_output=True,
            text=True,
            cwd=REPO,
            timeout=60,
        )

    def test_testbed_sync_calls_the_script(self):
        result = self.dry_run("testbed-sync")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr.strip(), "tools/linux-testbed/testbed sync")


if __name__ == "__main__":
    unittest.main()
