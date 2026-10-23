# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the memory tools under tools/.
#
# memsnap is run for real with stub `footprint`, `lsappinfo` and `ps`
# programs on PATH, so the tests need neither macOS nor a running
# Torchsnap. It runs under `/bin/bash`, the bash 3.2 of macOS where that
# is what the system has, because that is the interpreter that rejects
# an empty array expanded under `set -u`.
#
# memsnap-report turns memsnap's samples and phase snapshots into
# tables. Its inputs are written here in the shape `footprint` produces
# on macOS; nothing is measured, so the tests run on any machine with jq.

import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import textwrap
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
MEMSNAP_REPORT = REPO / "tools" / "memsnap-report"
MEMSNAP = REPO / "tools" / "memsnap"

MB = 1048576


def footprint_process(name, pid, mb):
    """One process record as `footprint -j` writes it."""
    return {"name": name, "pid": pid, "auxiliary": {"phys_footprint": int(mb * MB)}}


class ReportFiles:
    """A temporary directory for the report's input files."""

    def __init__(self):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="memsnap-report-"))

    def samples(self, samples):
        path = self.dir / "samples.json"
        path.write_text(json.dumps({"samples": [{"processes": s} for s in samples]}))
        return path

    def snapshots(self, snapshots):
        path = self.dir / "snapshots.jsonl"
        with path.open("w") as f:
            for phase, processes in snapshots:
                f.write(json.dumps({"processes": processes, "phase": phase}) + "\n")
        return path

    def cleanup(self):
        shutil.rmtree(self.dir)


def report(*args):
    return subprocess.run(
        ["bash", str(MEMSNAP_REPORT), *map(str, args)],
        capture_output=True,
        text=True,
        timeout=60,
    )


def table_row(output, first_cell):
    """The cells of the table row that starts with FIRST_CELL."""
    for line in output.splitlines():
        if line.startswith(first_cell):
            return line[len(first_cell):].split()
    raise AssertionError(f"no row {first_cell!r} in:\n{output}")


class MemsnapReportMacOS(unittest.TestCase):
    def setUp(self):
        if shutil.which("jq") is None:
            self.skipTest("jq is not installed")
        self.files = ReportFiles()

    def tearDown(self):
        self.files.cleanup()

    # Pinning test: the report of a run whose processes stay the same and
    # have distinct names is the same text before and after the fixes for
    # same-named and mid-run processes. Only the date line differs from
    # run to run.
    def test_pinned_report_of_a_plain_macos_run(self):
        samples = self.files.samples(
            [
                [footprint_process("torchsnap", 100, 50), footprint_process("com.apple.WebKit.WebContent", 200, 80)],
                [footprint_process("torchsnap", 100, 52), footprint_process("com.apple.WebKit.WebContent", 200, 90)],
                [footprint_process("torchsnap", 100, 51), footprint_process("com.apple.WebKit.WebContent", 200, 85.5)],
            ]
        )
        snapshots = self.files.snapshots(
            [
                (phase, [footprint_process("torchsnap", 100, a), footprint_process("com.apple.WebKit.WebContent", 200, b)])
                for phase, a, b in (
                    ("app started (idle)", 50, 80),
                    ("launcher visible (empty)", 55, 120),
                    ("final idle", 51, 85.5),
                )
            ]
        )
        result = report("-d", "pinned", "-s", samples, "-n", snapshots)
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = result.stdout.splitlines()
        self.assertRegex(lines[1], r"^  \d{4}-\d\d-\d\dT\d\d:\d\d:\d\d$")
        lines[1] = "  <date>"
        expected = textwrap.dedent(
            """\
              pinned
              <date>

              Continuous samples: 3

            ═══════════════════════════════════════════════════════════════════════
            Process                      PID  Min       Max       Avg       Delta
            ───────────────────────────────────────────────────────────────────────
            com.apple.WebKit.WebContent  200  80.0 MB   90.0 MB   85.2 MB   +5.5 MB
            torchsnap                    100  50.0 MB   52.0 MB   51.0 MB   +1.0 MB
            ───────────────────────────────────────────────────────────────────────
            TOTAL                             130.0 MB  142.0 MB  136.2 MB  +6.5 MB
            ═══════════════════════════════════════════════════════════════════════

              Phase snapshots: 3

            ═════════════════════════════════════════════════════════
            Phase                     WebContent  torchsnap  TOTAL
            ─────────────────────────────────────────────────────────
            app started (idle)        80 MB       50 MB      130 MB
            launcher visible (empty)  120 MB      55 MB      175 MB
            final idle                85.5 MB     51 MB      136.5 MB
            ─────────────────────────────────────────────────────────
            Δ first → last            +5.5 MB     +1 MB      +6.5 MB
            ═════════════════════════════════════════════════════════
            """
        )
        self.assertEqual("\n".join(lines) + "\n", expected)

    # WebKit may run several processes under one name (several
    # WebContent processes on macOS, several WebKitWebProcess on Linux);
    # each phase cell holds all of them.
    def test_phase_table_sums_processes_that_share_a_name(self):
        snapshots = self.files.snapshots(
            [
                (
                    "launcher visible (empty)",
                    [
                        footprint_process("torchsnap", 100, 50),
                        footprint_process("com.apple.WebKit.WebContent", 200, 10),
                        footprint_process("com.apple.WebKit.WebContent", 201, 20),
                    ],
                ),
                (
                    "final idle",
                    [
                        footprint_process("torchsnap", 100, 50),
                        footprint_process("com.apple.WebKit.WebContent", 200, 12),
                        footprint_process("com.apple.WebKit.WebContent", 201, 23),
                    ],
                ),
            ]
        )
        result = report("-n", snapshots)
        self.assertEqual(result.returncode, 0, result.stderr)
        # Columns: torchsnap, WebContent (by their footprint), TOTAL.
        self.assertEqual(table_row(result.stdout, "launcher visible (empty)"), ["50", "MB", "30", "MB", "80", "MB"])
        self.assertEqual(table_row(result.stdout, "Δ first → last"), ["+0", "MB", "+5", "MB", "+5", "MB"])

    # A process that starts during the run (a web process for a window
    # opened later) counts in the totals from its first sample on. The
    # TOTAL row is the statistic of the per-sample totals.
    def test_samples_total_follows_a_process_that_starts_mid_run(self):
        samples = self.files.samples(
            [
                [footprint_process("torchsnap", 100, 500)],
                [footprint_process("torchsnap", 100, 500)],
                [footprint_process("torchsnap", 100, 500), footprint_process("com.apple.WebKit.WebContent", 200, 100)],
                [footprint_process("torchsnap", 100, 500), footprint_process("com.apple.WebKit.WebContent", 200, 100)],
            ]
        )
        result = report("-s", samples)
        self.assertEqual(result.returncode, 0, result.stderr)
        # min, max, avg (500, 500, 600, 600), delta
        self.assertEqual(
            table_row(result.stdout, "TOTAL"),
            ["500.0", "MB", "600.0", "MB", "550.0", "MB", "+100.0", "MB"],
        )


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
