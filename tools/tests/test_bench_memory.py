# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/bench-memory and tools/bench-memory-cycles. The bench
# runs from a copy of the tools in a temporary tree, so its log and data
# files land there and not in the repository; memsnap is a stub next to
# it, socat and the macOS commands are stubs on PATH, and "the app" is a
# small script the test starts through the bench. Every PID a stub hands
# to the bench belongs to a process the test started itself.

import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

from memtools_support import (
    FAKE_APP,
    MEMSNAP_STUB,
    SOCAT_STUB,
    SYSTEMD_RUN_STUB,
    TOOLS,
    MethodControlSocket,
    Stubs,
)

# The scenario's phases, in the order bench-memory takes its snapshots.
LABELS = [
    "app started (idle)",
    "launcher visible (empty)",
    "after first dismiss",
    "query: Safari",
    "query: Terminal",
    "after app search dismiss",
    "query: :rocket (emoji)",
    "query: :fire (emoji)",
    "after emoji dismiss",
    "query: uni",
    "query: able",
    "after hello-world-wasm search dismiss",
    "after 5 rapid cycles",
    "query: settings",
    "final idle",
]


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def wait_gone(pid, seconds=5):
    deadline = time.monotonic() + seconds
    while alive(pid) and time.monotonic() < deadline:
        time.sleep(0.05)
    return not alive(pid)


class BenchTree:
    """A temporary tree with copies of the bench and its tools, a stub
    memsnap, a fake app, a home directory and an output directory. It
    lives under /tmp because the control socket's path must stay short
    (104 bytes on macOS)."""

    def __init__(self):
        self.root = pathlib.Path(tempfile.mkdtemp(prefix="bm-", dir="/tmp"))
        tools = self.root / "tools"
        tools.mkdir()
        for name in ("bench-memory", "memsnap-report", "bench-memory-cycles"):
            if (TOOLS / name).exists():
                shutil.copy2(TOOLS / name, tools / name)
        (tools / "memsnap").write_text(MEMSNAP_STUB)
        (tools / "memsnap").chmod(0o755)
        self.app = self.root / "fake-app"
        self.app.write_text(FAKE_APP)
        self.app.chmod(0o755)
        self.app_record = self.root / "fake-app.json"
        self.home = self.root / "h"
        self.home.mkdir()
        self.out = self.root / "out"

    def linux_socket_path(self):
        directory = self.home / ".local" / "share" / "app.torchsnap"
        directory.mkdir(parents=True, exist_ok=True)
        return directory / "control.sock"

    def macos_socket_path(self):
        directory = self.home / "Library" / "Application Support" / "app.torchsnap"
        directory.mkdir(parents=True, exist_ok=True)
        return directory / "control.sock"

    def app_pid(self):
        return json.loads(self.app_record.read_text())["pid"]

    def app_env(self):
        return json.loads(self.app_record.read_text())["env"]

    def cleanup(self):
        if self.app_record.exists() and alive(self.app_pid()):
            os.kill(self.app_pid(), 9)
        shutil.rmtree(self.root)


class BenchCase(unittest.TestCase):
    platform = "Linux"
    stub_names = ["uname", "pgrep", "pkill", "open", "bun", "sleep"]

    def setUp(self):
        if shutil.which("jq") is None:
            self.skipTest("jq is not installed")
        self.tree = BenchTree()
        self.stubs = Stubs(self.stub_names, custom={"socat": SOCAT_STUB})
        self.children = []

    def tearDown(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
                child.wait()
        self.tree.cleanup()
        self.stubs.cleanup()

    def spawn(self, *command):
        """A throwaway process of the test's own; stubs hand out only
        PIDs of these."""
        child = subprocess.Popen(list(command))
        self.children.append(child)
        return child

    def bench(self, *args, interpreter="bash", **env):
        environment = self.stubs.environment(
            HOME=str(self.tree.home),
            FAKE_APP_RECORD=str(self.tree.app_record),
            STUB_UNAME_STDOUT=f"{self.platform}\n",
            STUB_PGREP_EXIT="1",
        )
        environment.pop("XDG_DATA_HOME", None)
        environment.update(env)
        return subprocess.run(
            [interpreter, str(self.tree.root / "tools" / "bench-memory"), *map(str, args)],
            capture_output=True,
            text=True,
            env=environment,
            cwd=self.tree.root,
            timeout=60,
        )

    def memsnap_calls(self, command):
        return [c[2:] for c in self.stubs.calls("memsnap") if c[1] == command]

    def labels(self):
        return [args[args.index("-l") + 1] for args in self.memsnap_calls("snapshot")]

    def socat_methods(self):
        return [c[1] for c in self.stubs.calls("socat-request")]

    def printed_path(self, output, prefix):
        for line in output.splitlines():
            if line.startswith(prefix):
                return pathlib.Path(line[len(prefix):])
        self.fail(f"no line {prefix!r} in:\n{output}")


# =========================================================
# Linux
# =========================================================


class BenchMemoryLinux(BenchCase):
    def test_the_app_is_required(self):
        result = self.bench("-d", "x")
        self.assertEqual(result.returncode, 1)
        self.assertIn("--app is required on Linux", result.stderr)
        self.assertFalse(self.tree.app_record.exists())
        self.assertEqual(self.stubs.calls("bun"), [])

    def test_a_full_run(self):
        result = self.bench(
            "-d", "linux run", "--app", self.tree.app, "--compositor-pid", "4242",
            "--output-dir", self.tree.out, "--log", self.tree.out / "bench.log",
            "--env-prefix", "EXPERIMENT_",
            STUB_SOCAT_NOT_READY="2", EXPERIMENT_MAXIMIZE="1", WEBKIT_DISABLE_DMABUF_RENDERER="1",
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        app_pid = self.tree.app_pid()

        # The caller's environment reaches the app.
        self.assertEqual(self.tree.app_env().get("EXPERIMENT_MAXIMIZE"), "1")

        # `dismiss` goes out only once `status` reports ready.
        methods = self.socat_methods()
        first_dismiss = methods.index("dismiss")
        self.assertEqual(methods[:first_dismiss].count("status"), 3)
        self.assertNotIn("show", methods[:first_dismiss])

        self.assertEqual(self.labels(), LABELS)
        for args in self.memsnap_calls("snapshot") + self.memsnap_calls("sample"):
            self.assertIn(["-p", str(app_pid)], [args[i:i + 2] for i in range(len(args))])
            self.assertIn(["--also", "4242"], [args[i:i + 2] for i in range(len(args))])
        sample = self.memsnap_calls("sample")[0]
        self.assertEqual(sample[sample.index("-d") + 1], "0")

        self.assertIn("Phase snapshots: 15", (self.tree.out / "bench.log").read_text())
        self.assertFalse((self.tree.root / "memory-benchmark.log").exists())
        samples = self.printed_path(result.stdout, "Continuous data: ")
        snapshots = self.printed_path(result.stdout, "Snapshot data: ")
        events = self.printed_path(result.stdout, "Events data: ")
        for path in (samples, snapshots, events):
            self.assertEqual(path.parent, self.tree.out)
            self.assertTrue(path.exists(), path)
        self.assertNotIn("Timing data: ", result.stdout)

        records = [json.loads(line) for line in events.read_text().splitlines()]
        environment = next(r for r in records if r.get("event") == "environment")
        self.assertEqual(environment["variables"]["EXPERIMENT_MAXIMIZE"], "1")
        self.assertEqual(environment["variables"]["WEBKIT_DISABLE_DMABUF_RENDERER"], "1")
        info = next(r for r in records if r.get("event") == "info")
        self.assertEqual(info["info"]["version"], "0.0.0-test")
        requests = [r for r in records if "method" in r]
        self.assertEqual([r["method"] for r in requests][:3], ["dismiss", "show", "dismiss"])
        self.assertTrue(all(isinstance(r["t"], (int, float)) and r["ok"] for r in requests))
        self.assertIn({"text": "Safari"}, [r["params"] for r in requests])

        self.assertTrue(wait_gone(app_pid), "the app still runs")
        self.assertNotIn("Warning", result.stderr)

    def environment_record(self, result):
        events = self.printed_path(result.stdout, "Events data: ")
        records = [json.loads(line) for line in events.read_text().splitlines()]
        return next(r for r in records if r.get("event") == "environment")["variables"]

    # Only WEBKIT_ and GDK_ variables and the prefixes named with
    # --env-prefix go into the events file; the app still gets every
    # variable.
    def test_only_listed_prefixes_are_recorded(self):
        result = self.bench(
            "-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out,
            GDK_SCALE="2", EXPERIMENT_MAXIMIZE="1", OTHER_SWITCH="1",
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.environment_record(result), {"GDK_SCALE": "2"})
        self.assertEqual(self.tree.app_env().get("EXPERIMENT_MAXIMIZE"), "1")

    def test_env_prefix_can_be_given_several_times(self):
        result = self.bench(
            "-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out,
            "--env-prefix", "EXPERIMENT_", "--env-prefix", "L1_",
            EXPERIMENT_MAXIMIZE="1", L1_SWITCH="on", OTHER_SWITCH="1",
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.environment_record(result), {"EXPERIMENT_MAXIMIZE": "1", "L1_SWITCH": "on"})

    def test_the_app_runs_in_a_user_scope_when_systemd_run_is_there(self):
        self.stubs.add("systemd-run", SYSTEMD_RUN_STUB)
        result = self.bench("-d", "scope", "--app", self.tree.app, "--output-dir", self.tree.out)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        call = self.stubs.calls("systemd-run")[0][1:]
        self.assertEqual(call[:4], ["--user", "--scope", "--collect", "--quiet"])
        self.assertEqual(call[4], "--unit")
        self.assertRegex(call[5], r"^torchsnap-bench-\d{8}-\d{6}$")
        self.assertEqual(call[6:], ["--", str(self.tree.app)])
        # In scope mode the command runs in systemd-run's place, so the
        # bench measures and stops the app's own PID.
        snapshot = self.memsnap_calls("snapshot")[0]
        self.assertEqual(snapshot[snapshot.index("-p") + 1], str(self.tree.app_pid()))
        self.assertTrue(wait_gone(self.tree.app_pid()))

    def test_refuses_while_torchsnap_runs(self):
        running = self.spawn(sys.executable, "-c", "import time; time.sleep(60)")
        result = self.bench(
            "-d", "x", "--app", self.tree.app,
            STUB_PGREP_EXIT="0", STUB_PGREP_STDOUT=f"{running.pid}\n",
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"Torchsnap is already running (PID {running.pid})", result.stderr)
        self.assertIn("just testbed-app-stop", result.stderr)
        self.assertEqual(self.stubs.calls("pkill"), [])
        self.assertFalse(self.tree.app_record.exists())
        self.assertIsNone(running.poll())

    def test_gives_up_when_the_app_never_gets_ready(self):
        result = self.bench(
            "-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out,
            STUB_SOCAT_NEVER_READY="1", BENCH_MEMORY_READY_TIMEOUT="1",
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not become ready within 1 s", result.stderr)
        self.assertTrue(wait_gone(self.tree.app_pid()))

    def test_reports_an_app_that_exits_at_once(self):
        result = self.bench(
            "-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out,
            FAKE_APP_EXIT="3", STUB_SOCAT_NEVER_READY="1", BENCH_MEMORY_READY_TIMEOUT="10",
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("exited before it got ready", result.stderr)
        self.assertIn("fake app gives up", result.stderr)

    def test_kills_a_process_left_over_after_the_stop(self):
        stubborn = self.spawn(
            sys.executable, "-c",
            "import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(60)",
        )
        time.sleep(0.3)
        result = self.bench(
            "-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out,
            STUB_MEMSNAP_PIDS=str(stubborn.pid), BENCH_MEMORY_STOP_TIMEOUT="1",
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"Warning: still running 1 s after the stop, killed: {stubborn.pid}", result.stderr)
        self.assertEqual(stubborn.wait(timeout=5), -9)

    def test_memory_cycles_after_the_scenario(self):
        server = MethodControlSocket(self.tree.linux_socket_path())
        try:
            result = self.bench("-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out, "--cycles", "2")
        finally:
            server.close()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(server.methods(), ["show", "dismiss", "show", "dismiss"])
        self.assertEqual(self.labels(), LABELS + ["after 2 cycles"])
        events = self.printed_path(result.stdout, "Events data: ")
        cycles = [json.loads(line) for line in events.read_text().splitlines() if '"cycle"' in line]
        self.assertEqual([c["cycle"] for c in cycles], [1, 2])

    def test_timing_cycles_after_the_last_snapshot(self):
        server = MethodControlSocket(
            self.tree.linux_socket_path(),
            {"snapshot": [{"result": {"contentVisible": True}}], "status": [{"result": {"visible": False}}]},
        )
        try:
            result = self.bench("-d", "x", "--app", self.tree.app, "--output-dir", self.tree.out, "--timing-cycles", "2")
        finally:
            server.close()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        timing = self.printed_path(result.stdout, "Timing data: ")
        self.assertEqual(timing.parent, self.tree.out)
        lines = [json.loads(line) for line in timing.read_text().splitlines()]
        self.assertEqual(len(lines), 2)
        self.assertTrue(all(isinstance(line["show_ms"], (int, float)) for line in lines))
        self.assertEqual(server.methods().count("show"), 2)

    def test_stock_macos_bash_runs_the_linux_path(self):
        if not os.path.exists("/bin/bash"):
            self.skipTest("no /bin/bash")
        result = self.bench("-d", "x", "--app", self.tree.app, interpreter="/bin/bash")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.labels(), LABELS)


# =========================================================
# macOS
# =========================================================


class BenchMemoryMacOS(BenchCase):
    platform = "Darwin"

    def macos_run(self, *args):
        app = self.spawn("sleep", "60")
        server = MethodControlSocket(self.tree.macos_socket_path())
        try:
            return app, self.bench(*args, STUB_PGREP_EXIT="0", STUB_PGREP_STDOUT=f"{app.pid}\n")
        finally:
            server.close()

    # Pinning test: the macOS run builds, opens the bundle, takes the
    # same snapshots and writes the log into the project, as before the
    # Linux path existed.
    def test_pinned_macos_run(self):
        app, result = self.macos_run("-d", "mac run")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.stubs.calls("bun")[0][1:], ["run", "tauri", "build", "-b", "app"])
        bundle = self.tree.root / "src-tauri" / "target" / "release" / "bundle" / "macos" / "Torchsnap.app"
        self.assertEqual(self.stubs.calls("open")[0][1:], [str(bundle)])
        self.assertEqual(self.labels(), LABELS)
        sample = self.memsnap_calls("sample")[0]
        self.assertEqual(sample[:4], ["-i", "0.2", "-d", "55"])
        settings = self.tree.home / "Library" / "Application Support" / "app.torchsnap" / "settings.json"
        self.assertEqual(json.loads(settings.read_text()), {"controlChannel.enabled": True})
        self.assertIn("Phase snapshots: 15", (self.tree.root / "memory-benchmark.log").read_text())
        # The bench killed "the app"; it is the test's child, so the test
        # reaps it.
        self.assertEqual(app.wait(timeout=5), -15)

    def test_log_and_output_dir_options(self):
        _, result = self.macos_run("-d", "mac run", "--output-dir", self.tree.out, "--log", self.tree.out / "bench.log")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Phase snapshots: 15", (self.tree.out / "bench.log").read_text())
        self.assertFalse((self.tree.root / "memory-benchmark.log").exists())
        for prefix in ("Continuous data: ", "Snapshot data: "):
            path = self.printed_path(result.stdout, prefix)
            self.assertEqual(path.parent, self.tree.out)
            self.assertTrue(path.exists(), path)

    def test_linux_options_are_refused(self):
        for option, value in (("--app", "/x"), ("--compositor-pid", "1"), ("--cycles", "2"), ("--timing-cycles", "2"), ("--env-prefix", "EXPERIMENT_")):
            result = self.bench("-d", "x", option, value)
            self.assertEqual(result.returncode, 1, option)
            self.assertIn(f"{option} is Linux only", result.stderr)
        self.assertEqual(self.stubs.calls("bun"), [])


# =========================================================
# bench-memory-cycles
# =========================================================


class BenchMemoryCycles(unittest.TestCase):
    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="bmc-", dir="/tmp"))

    def tearDown(self):
        shutil.rmtree(self.dir)

    def cycles(self, answers, *args):
        server = MethodControlSocket(self.dir / "c.sock", answers)
        try:
            result = subprocess.run(
                [sys.executable, str(TOOLS / "bench-memory-cycles"), str(self.dir / "c.sock"), *map(str, args)],
                capture_output=True, text=True, timeout=60,
            )
        finally:
            server.close()
        lines = [json.loads(line) for line in result.stdout.splitlines()]
        return result, server.methods(), lines

    def test_timing_waits_for_visible_content_then_for_the_hide(self):
        visible = {"result": {"contentVisible": True}}
        blank = {"result": {"contentVisible": False}}
        result, methods, lines = self.cycles(
            {"snapshot": [blank, blank, visible], "status": [{"result": {"visible": False}}]},
            "2", "--timing", "--hold", "0",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            methods,
            ["show", "snapshot", "snapshot", "snapshot", "dismiss", "status",
             "show", "snapshot", "dismiss", "status"],
        )
        self.assertEqual([line["cycle"] for line in lines], [1, 2])
        for line in lines:
            self.assertGreaterEqual(line["t_visible"], line["t_show"])
            self.assertGreaterEqual(line["t_hidden"], line["t_visible"])
            self.assertAlmostEqual(line["show_ms"], (line["t_visible"] - line["t_show"]) * 1000, places=3)

    def test_timing_records_content_that_never_shows(self):
        result, methods, lines = self.cycles(
            {"snapshot": [{"result": {"contentVisible": False}}], "status": [{"result": {"visible": False}}]},
            "1", "--timing", "--hold", "0", "--visible-timeout", "0.3",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(lines[0]["timeout"])
        self.assertIsNone(lines[0]["show_ms"])
        self.assertIn("dismiss", methods)

    def test_memory_cycles_send_only_show_and_dismiss(self):
        result, methods, lines = self.cycles({}, "3", "--hold", "0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(methods, ["show", "dismiss"] * 3)
        self.assertEqual([line["cycle"] for line in lines], [1, 2, 3])
        self.assertTrue(all(line["ok"] for line in lines))


if __name__ == "__main__":
    unittest.main()
