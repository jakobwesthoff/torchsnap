# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the memory tools under tools/: memsnap, which measures
# Torchsnap's processes (with `footprint` on macOS, from /proc through
# memsnap-linux on Linux), and memsnap-report, which turns its samples
# and phase snapshots into tables. memsnap-linux reads a fake /proc tree
# (memtools_support.FakeProc) and the macOS commands are stubs, so
# nothing is measured and the tests run on the macOS CI runner.
#
# MemsnapSampleUnderSystemBash runs memsnap under `/bin/bash`, the bash
# 3.2 of macOS where that is what the system has, because that is the
# interpreter that rejects an empty array expanded under `set -u`.

import json
import os
import pathlib
import shutil
import subprocess
import signal
import tempfile
import textwrap
import time
import unittest

from memtools_support import FOOTPRINT_STUB, FakeProc, Stubs

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
MEMSNAP = REPO / "tools" / "memsnap"
MEMSNAP_LINUX = REPO / "tools" / "memsnap-linux"
MEMSNAP_REPORT = REPO / "tools" / "memsnap-report"

MB = 1048576


def footprint_process(name, pid, mb):
    """One process record as `footprint -j` writes it."""
    return {"name": name, "pid": pid, "auxiliary": {"phys_footprint": int(mb * MB)}}


class ReportFiles:
    """A temporary directory for the report's input files."""

    def __init__(self):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="memsnap-report-"))

    def samples(self, samples, metric=None):
        path = self.dir / "samples.json"
        data = {"samples": [{"processes": s} for s in samples]}
        if metric:
            data["metric"] = metric
        path.write_text(json.dumps(data))
        return path

    def snapshots(self, snapshots, metric=None):
        path = self.dir / "snapshots.jsonl"
        with path.open("w") as f:
            for phase, processes in snapshots:
                line = {"processes": processes, "phase": phase}
                if metric:
                    line["metric"] = metric
                f.write(json.dumps(line) + "\n")
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


# =========================================================
# memsnap-report with Linux input (PSS)
# =========================================================


def pss_process(name, pid, mb):
    """One process record as memsnap-linux writes it."""
    return {"name": name, "pid": pid, "ppid": 1, "pss": int(mb * MB), "rss": int(mb * 2 * MB)}


class MemsnapReportLinux(unittest.TestCase):
    def setUp(self):
        if shutil.which("jq") is None:
            self.skipTest("jq is not installed")
        self.files = ReportFiles()

    def tearDown(self):
        self.files.cleanup()

    def test_snapshots_report_pss_and_name_the_metric(self):
        snapshots = self.files.snapshots(
            [
                ("app started (idle)", [pss_process("torchsnap", 100, 40), pss_process("WebKitWebProcess", 200, 60)]),
                ("final idle", [pss_process("torchsnap", 100, 42), pss_process("WebKitWebProcess", 200, 70)]),
            ],
            metric="pss",
        )
        result = report("-n", snapshots)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("  Metric: PSS\n", result.stdout)
        self.assertEqual(table_row(result.stdout, "final idle"), ["70", "MB", "42", "MB", "112", "MB"])

    def test_samples_report_pss(self):
        samples = self.files.samples(
            [
                [pss_process("torchsnap", 100, 40), pss_process("WebKitWebProcess", 200, 60)],
                [pss_process("torchsnap", 100, 44), pss_process("WebKitWebProcess", 200, 66)],
            ],
            metric="pss",
        )
        result = report("-s", samples)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("  Metric: PSS\n", result.stdout)
        self.assertEqual(
            table_row(result.stdout, "WebKitWebProcess"),
            ["200", "60.0", "MB", "66.0", "MB", "63.0", "MB", "+6.0", "MB"],
        )
        self.assertEqual(
            table_row(result.stdout, "TOTAL"),
            ["100.0", "MB", "110.0", "MB", "105.0", "MB", "+10.0", "MB"],
        )


# =========================================================
# memsnap-linux
# =========================================================


def kb_to_bytes(kb):
    return kb * 1024


def memsnap_linux(proc, *args, env=None):
    environment = dict(os.environ)
    environment["MEMSNAP_PROC_ROOT"] = str(proc.root)
    environment.update(env or {})
    return subprocess.run(
        ["python3", str(MEMSNAP_LINUX), *map(str, args)],
        capture_output=True,
        text=True,
        env=environment,
        timeout=60,
    )


class MemsnapLinux(unittest.TestCase):
    """memsnap-linux against a fake /proc: Torchsnap (100) with its web
    and network processes, a sandbox wrapper with a child, and an
    unrelated process."""

    def setUp(self):
        self.proc = FakeProc()
        self.out = pathlib.Path(tempfile.mkdtemp(prefix="memsnap-out-"))
        self.proc.process(1, 0, "systemd", ["/sbin/init"], pss_kb=1000)
        self.proc.process(100, 1, "torchsnap", ["/home/u/torchsnap/src-tauri/target/debug/torchsnap"],
                          utime=150, stime=50, rss_kb=253888, pss_kb=180946, pss_anon_kb=70000,
                          pss_file_kb=100000, pss_shmem_kb=10946, swap_pss_kb=0)
        self.proc.process(200, 100, "WebKitWebProces",
                          ["/usr/lib/aarch64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess", "4", "76"],
                          rss_kb=215616, pss_kb=146285)
        self.proc.process(300, 100, "WebKitNetworkPr",
                          ["/usr/lib/aarch64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess", "1", "69"],
                          rss_kb=66176, pss_kb=29863)
        self.proc.process(350, 100, "bwrap", ["bwrap", "--args", "27"], pss_kb=500)
        self.proc.process(400, 350, "xdg-dbus-proxy", ["xdg-dbus-proxy"], pss_kb=700)
        self.proc.process(500, 1, "foot", ["foot"], pss_kb=9999)

    def tearDown(self):
        self.proc.cleanup()
        shutil.rmtree(self.out)

    def snapshot(self, *args):
        output = self.out / "snapshots.jsonl"
        result = memsnap_linux(self.proc, "snapshot", "-o", output, *args)
        lines = output.read_text().splitlines() if output.exists() else []
        return result, [json.loads(line) for line in lines]

    def by_pid(self, record):
        return {p["pid"]: p for p in record["processes"]}

    def test_measures_torchsnap_and_all_its_descendants(self):
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sorted(self.by_pid(lines[0])), [100, 200, 300, 350, 400])

    def test_a_command_name_with_spaces_and_parentheses_keeps_its_parent(self):
        self.proc.process(600, 200, "a) (b c", ["helper"], pss_kb=10)
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.by_pid(lines[0])[600]["ppid"], 200)

    def test_smaps_rollup_and_cpu_time_in_bytes_and_seconds(self):
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        root = self.by_pid(lines[0])[100]
        self.assertEqual(root["rss"], kb_to_bytes(253888))
        self.assertEqual(root["pss"], kb_to_bytes(180946))
        self.assertEqual(root["pss_anon"], kb_to_bytes(70000))
        self.assertEqual(root["pss_file"], kb_to_bytes(100000))
        self.assertEqual(root["pss_shmem"], kb_to_bytes(10946))
        self.assertEqual(root["swap_pss"], 0)
        self.assertAlmostEqual(root["cpu_s"], 200 / os.sysconf("SC_CLK_TCK"))

    def test_name_from_the_command_line_or_the_command_name(self):
        self.proc.process(610, 100, "kworker-ish", None, pss_kb=1)
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        processes = self.by_pid(lines[0])
        self.assertEqual(processes[200]["name"], "WebKitWebProcess")
        self.assertEqual(processes[300]["name"], "WebKitNetworkProcess")
        self.assertEqual(processes[610]["name"], "kworker-ish")

    def test_a_descendant_without_smaps_is_skipped_and_a_missing_root_fails(self):
        self.proc.process(620, 100, "gone", ["gone"], smaps=False)
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(620, self.by_pid(lines[0]))
        result, _ = self.snapshot("-l", "idle", "-p", "999")
        self.assertEqual(result.returncode, 1)
        self.assertIn("PID 999 is not a running process", result.stderr)

    def test_a_zombie_descendant_is_skipped(self):
        self.proc.process(630, 100, "exited", None, state="Z", smaps=False)
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(630, self.by_pid(lines[0]))

    def test_an_unreadable_smaps_rollup_fails_the_reading(self):
        if os.geteuid() == 0:
            self.skipTest("root reads files of mode 000")
        (self.proc.root / "200" / "smaps_rollup").chmod(0)
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 1)
        self.assertIn("PID 200", result.stderr)
        self.assertEqual(lines, [])

    def test_finds_torchsnap_by_its_command_name(self):
        result, lines = self.snapshot("-l", "idle")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(lines[0]["processes"][0]["pid"], 100)

    def test_no_or_several_torchsnap_processes_need_a_pid(self):
        self.proc.process(700, 1, "torchsnap", ["torchsnap"], pss_kb=1)
        result, _ = self.snapshot("-l", "idle")
        self.assertEqual(result.returncode, 1)
        self.assertIn("100 700", result.stderr)
        self.assertIn("-p", result.stderr)
        shutil.rmtree(self.proc.root / "700")
        shutil.rmtree(self.proc.root / "100")
        result, _ = self.snapshot("-l", "idle")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Torchsnap is not running", result.stderr)

    def test_drm_clients_once_per_client_in_bytes(self):
        drm = """\
            pos:	0
            flags:	02100002
            drm-driver:	v3d
            drm-client-id:	473
            drm-engine-render: 	926967740136 ns
            drm-total-memory:	158800 KiB
            drm-shared-memory:	21952 KiB
            drm-resident-memory:	2 MiB
            drm-purgeable-memory:	4096
            """
        self.proc.fd(200, 14, "/dev/dri/renderD128", drm)
        self.proc.fd(200, 15, "/dev/dri/renderD128", drm)
        self.proc.fd(200, 16, "socket:[1234]", "pos:	0\nflags:	02\n")
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.by_pid(lines[0])[200]["gpu"],
            [{"driver": "v3d", "client_id": "473", "memory": {
                "drm-total-memory": 158800 * 1024,
                "drm-shared-memory": 21952 * 1024,
                "drm-resident-memory": 2 * 1024 * 1024,
                "drm-purgeable-memory": 4096,
            }}],
        )
        self.assertEqual(self.by_pid(lines[0])[300]["gpu"], [])

    def test_dmabufs_once_per_buffer(self):
        def dmabuf(ino, size, exporter):
            return f"pos:	0\nflags:	02000002\nmnt_id:	15\nino:	{ino}\nsize:	{size}\ncount:	1\nexp_name:	{exporter}\n"
        self.proc.fd(100, 20, "anon_inode:dmabuf", dmabuf(5001, 33554432, "v3d"))
        self.proc.fd(100, 21, "anon_inode:dmabuf", dmabuf(5001, 33554432, "v3d"))
        self.proc.fd(100, 22, "anon_inode:dmabuf", dmabuf(5002, 1048576, "drm"))
        result, lines = self.snapshot("-l", "idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.by_pid(lines[0])[100]["dmabuf"],
            {"count": 2, "bytes": 33554432 + 1048576, "exp_names": {"v3d": 1, "drm": 1}},
        )

    def test_snapshot_lines_carry_phase_time_metric_system_and_external(self):
        self.proc.process(1666, 1, "gnome-shell", ["/usr/bin/gnome-shell"], pss_kb=300000)
        before = time.time()
        result, lines = self.snapshot("-l", "app started (idle)", "-p", "100", "--also", "1666", "--also", "4242")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("[app started (idle)]", result.stderr)
        result, lines = self.snapshot("-l", "final idle", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([line["phase"] for line in lines], ["app started (idle)", "final idle"])
        first = lines[0]
        self.assertEqual(first["metric"], "pss")
        self.assertGreaterEqual(first["t"], before)
        self.assertEqual(first["system"]["MemTotal"], 8251728 * 1024)
        self.assertEqual(first["system"]["CmaFree"], 336 * 1024)
        self.assertEqual(first["external"][0]["name"], "gnome-shell")
        self.assertEqual(first["external"][0]["pss"], 300000 * 1024)
        self.assertEqual(first["external"][1], {"pid": 4242, "gone": True})
        self.assertNotIn(1666, self.by_pid(first))

    def test_sample_writes_samples_and_survives_sigterm(self):
        self.proc.process(1666, 1, "gnome-shell", ["/usr/bin/gnome-shell"], utime=500, pss_kb=300000)
        output = self.out / "samples.json"
        result = memsnap_linux(self.proc, "sample", "-i", "0.05", "-d", "0.3", "-p", "100", "--also", "1666", "-o", output)
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(output.read_text())
        self.assertEqual(data["metric"], "pss")
        self.assertGreaterEqual(len(data["samples"]), 3)
        self.assertIn("sampler_cpu_s", data)
        first = data["samples"][0]
        self.assertEqual(sorted(p["pid"] for p in first["processes"]), [100, 200, 300, 350, 400])
        # The compositor is sampled for its CPU time only.
        self.assertEqual(first["external"], [{"pid": 1666, "name": "gnome-shell", "cpu_s": 500 / os.sysconf("SC_CLK_TCK")}])

        output.unlink()
        environment = dict(os.environ, MEMSNAP_PROC_ROOT=str(self.proc.root))
        sampler = subprocess.Popen(
            ["python3", str(MEMSNAP_LINUX), "sample", "-i", "0.05", "-d", "0", "-p", "100", "-o", str(output)],
            env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        time.sleep(0.5)
        sampler.send_signal(signal.SIGTERM)
        _, stderr = sampler.communicate(timeout=30)
        self.assertEqual(sampler.returncode, 0, stderr)
        self.assertGreaterEqual(len(json.loads(output.read_text())["samples"]), 1)

    def test_pids_prints_the_measured_tree(self):
        result = memsnap_linux(self.proc, "pids", "-p", "100")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sorted(int(p) for p in result.stdout.split()), [100, 200, 300, 350, 400])

    def test_snapshot_without_a_label_fails(self):
        result, _ = self.snapshot("-p", "100")
        self.assertEqual(result.returncode, 1)
        self.assertIn("-l/--label is required", result.stderr)

    def test_an_unknown_option_fails(self):
        result, _ = self.snapshot("-l", "idle", "--frobnicate")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Unknown option: --frobnicate", result.stderr)

    def test_an_output_in_a_missing_directory_fails(self):
        result = memsnap_linux(self.proc, "snapshot", "-l", "idle", "-p", "100", "-o", self.out / "missing" / "s.jsonl")
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing", result.stderr)


# =========================================================
# memsnap: the dispatch between macOS and Linux
# =========================================================


# lsappinfo prints the coalition of the given PID with one more member.
STUB_LSAPPINFO = textwrap.dedent(
    """\
    #!/usr/bin/env bash
    echo '"Torchsnap" ASN:0x0-0x1:'
    echo '    coalition: 1234 {{ {pid} 4711 }}'
    """
)


class MemsnapDispatch(unittest.TestCase):
    def setUp(self):
        self.proc = FakeProc()
        self.proc.process(100, 1, "torchsnap", ["torchsnap"], pss_kb=1000)
        self.out = pathlib.Path(tempfile.mkdtemp(prefix="memsnap-out-"))

    def tearDown(self):
        self.proc.cleanup()
        shutil.rmtree(self.out)

    def run_memsnap(self, stubs, uname, interpreter, *args):
        environment = stubs.environment(
            MEMSNAP_PROC_ROOT=str(self.proc.root), STUB_UNAME_STDOUT=f"{uname}\n"
        )
        return subprocess.run(
            [interpreter, str(MEMSNAP), *map(str, args)],
            capture_output=True, text=True, env=environment, timeout=60,
        )

    def test_on_linux_memsnap_runs_memsnap_linux(self):
        stubs = Stubs(["uname", "lsappinfo", "footprint"])
        try:
            output = self.out / "s.jsonl"
            result = self.run_memsnap(stubs, "Linux", "bash", "snapshot", "-l", "idle", "-p", "100", "-o", output)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(output.read_text())["processes"][0]["pid"], 100)
            self.assertEqual(stubs.calls("footprint"), [])
        finally:
            stubs.cleanup()

    # macOS ships bash 3.2 as /bin/bash; the dispatch must run there.
    def test_stock_macos_bash_runs_the_linux_dispatch(self):
        if not os.path.exists("/bin/bash"):
            self.skipTest("no /bin/bash")
        stubs = Stubs(["uname"])
        try:
            output = self.out / "s.jsonl"
            result = self.run_memsnap(stubs, "Linux", "/bin/bash", "snapshot", "-l", "idle", "-p", "100", "-o", output)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(output.exists())
        finally:
            stubs.cleanup()

    # Pinning test: on macOS, memsnap measures the coalition's processes
    # with footprint, as before the Linux path existed.
    def test_on_macos_memsnap_measures_the_coalition_with_footprint(self):
        child = subprocess.Popen(["sleep", "60"])
        stubs = Stubs(["uname"], custom={"footprint": FOOTPRINT_STUB})
        try:
            stubs.add("lsappinfo", STUB_LSAPPINFO.format(pid=child.pid))
            output = self.out / "s.jsonl"
            result = self.run_memsnap(stubs, "Darwin", "bash", "snapshot", "-l", "idle", "-p", child.pid, "-o", output)
            self.assertEqual(result.returncode, 0, result.stderr)
            footprint = stubs.calls("footprint")[0]
            self.assertEqual(footprint[1:5], ["-p", str(child.pid), "-p", "4711"])
            line = json.loads(output.read_text())
            self.assertEqual(line["phase"], "idle")
            self.assertEqual([p["pid"] for p in line["processes"]], [child.pid, 4711])
        finally:
            child.kill()
            child.wait()
            stubs.cleanup()


# Each stub records its arguments, one per line, in `<name>.args`.
STUB = """#!/bin/sh
printf '%s\\n' "$@" > "{dir}/{name}.args"
"""


class MemsnapSampleUnderSystemBash(unittest.TestCase):
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
