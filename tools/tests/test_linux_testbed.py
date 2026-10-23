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
import re
import shutil
import socket
import stat
import subprocess
import tempfile
import textwrap
import threading
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
TESTBED = REPO / "tools" / "linux-testbed" / "testbed"
SELFTEST = REPO / "tools" / "linux-testbed" / "selftest"
PROVISION_DIR = REPO / "tools" / "linux-testbed" / "provision"
TESTBED_PI = REPO / "tools" / "linux-testbed" / "pi" / "testbed-pi"

# Tests that stub `bash` itself start their shell by this path, found on
# the PATH the tests run with.
BASH = shutil.which("bash")

# A stub reads STUB_<NAME>_STDOUT and STUB_<NAME>_EXIT, with NAME the
# command name in upper case and `-` replaced by `_`. With
# STUB_<NAME>_FAIL_ON set, a call whose arguments contain that text exits
# 1 instead.
STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    name = os.path.basename(sys.argv[0])
    key = name.upper().replace("-", "_")
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(sys.argv) + "\\n")
    sys.stdout.write(os.environ.get(f"STUB_{key}_STDOUT", ""))
    fail_on = os.environ.get(f"STUB_{key}_FAIL_ON")
    if fail_on and fail_on in " ".join(sys.argv[1:]):
        sys.exit(1)
    sys.exit(int(os.environ.get(f"STUB_{key}_EXIT", "0")))
    """
)

# sudo for tests of steps that change system files: it runs nothing. It
# logs its argv like STUB and, for the commands that get a file's content
# on stdin, that content as a last element `stdin:<content>`.
# STUB_SUDO_FAIL_ON works as in STUB.
SUDO_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    args = sys.argv[1:]
    entry = ["sudo", *args]
    if args and args[0] in ("tee", "debconf-set-selections"):
        entry.append("stdin:" + sys.stdin.read())
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(entry) + "\\n")
    fail_on = os.environ.get("STUB_SUDO_FAIL_ON")
    sys.exit(1 if fail_on and fail_on in " ".join(args) else 0)
    """
)

# busctl for the Secret Service. A call or get-property is answered by
# its method or property name (the sixth argument after `busctl`) from
# STUB_BUSCTL_<NAME>: answers separated by `|`, one per call, the last
# one repeating.
BUSCTL_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, pathlib, sys
    log = pathlib.Path(os.environ["STUB_LOG"])
    with log.open("a") as f:
        f.write(json.dumps(sys.argv) + "\\n")
    name = sys.argv[6].upper()
    answers = os.environ.get(f"STUB_BUSCTL_{name}", "").split("|")
    counter = log.with_name(f"busctl-{name}.count")
    n = int(counter.read_text()) if counter.exists() else 0
    counter.write_text(str(n + 1))
    print(answers[min(n, len(answers) - 1)])
    """
)

# gsettings: `get SCHEMA KEY` prints STUB_GSETTINGS_<KEY>.
GSETTINGS_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(sys.argv) + "\\n")
    if sys.argv[1] == "get":
        print(os.environ["STUB_GSETTINGS_" + sys.argv[3].upper().replace("-", "_")])
    """
)


class Stubs:
    """A temporary stub directory and the log its stubs write to."""

    def __init__(self, names):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="testbed-stubs-"))
        self.log = self.dir / "calls.jsonl"
        for name in names:
            self.add(name, STUB)

    def add(self, name, content):
        path = self.dir / name
        path.write_text(content)
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


def run(args, stubs, env=None, unset=(), script=TESTBED):
    environment = dict(os.environ)
    for name in unset:
        environment.pop(name, None)
    environment["PATH"] = f"{stubs.dir}{os.pathsep}{os.environ['PATH']}"
    environment["STUB_LOG"] = str(stubs.log)
    environment.update(env or {})
    return subprocess.run(
        ["bash", str(script), *args],
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

    def test_provision_refuses_without_host(self):
        self.assert_host_refused(run(["provision"], self.stubs, unset=["TORCHSNAP_LINUX_HOST"]))


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

    def test_testbed_provision_passes_its_arguments(self):
        result = self.dry_run("testbed-provision", "--monitor", "HDMI-1:90:2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stderr.strip(), "tools/linux-testbed/testbed provision --monitor HDMI-1:90:2"
        )

    def test_testbed_reboot_calls_the_script(self):
        result = self.dry_run("testbed-reboot")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr.strip(), "tools/linux-testbed/testbed reboot")

    def test_testbed_exec_passes_the_command_as_one_argument(self):
        result = self.dry_run("testbed-exec", "ls -la ~")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr.strip(), "tools/linux-testbed/testbed exec 'ls -la ~'")

    def test_testbed_build_calls_the_script(self):
        result = self.dry_run("testbed-build")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr.strip(), "tools/linux-testbed/testbed build")

    def test_testbed_control_passes_json_as_one_argument(self):
        result = self.dry_run("testbed-control", "query", '{"text": "a b"}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stderr.strip(), "tools/linux-testbed/testbed control query '{\"text\": \"a b\"}'"
        )

    def test_observation_recipes_call_the_script(self):
        for recipe, args, expected in (
            ("testbed-screenshot", ["shot", "--monitor", "0"], "tools/linux-testbed/testbed screenshot shot --monitor 0"),
            ("testbed-windows", [], "tools/linux-testbed/testbed windows"),
            ("testbed-launcher-state", [], "tools/linux-testbed/testbed launcher-state"),
            ("testbed-eval", ["Main.overview.visible"], "tools/linux-testbed/testbed eval 'Main.overview.visible'"),
        ):
            result = self.dry_run(recipe, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr.strip(), expected)

    def test_input_recipes_keep_text_with_spaces_together(self):
        if shutil.which("just") is None:
            self.skipTest("just is not installed")
        stubs = Stubs(["ssh"])
        try:
            environment = dict(os.environ)
            environment.update(
                {
                    "PATH": f"{stubs.dir}{os.pathsep}{os.environ['PATH']}",
                    "STUB_LOG": str(stubs.log),
                    "TORCHSNAP_LINUX_HOST": "user@pi.local",
                    "TORCHSNAP_LINUX_RUNNER": "",
                }
            )
            for recipe, args in (
                ("testbed-type", ["--window", "^torchsnap$", "a b $HOME"]),
                ("testbed-key", ["--shell-modal", "ctrl+u"]),
            ):
                result = subprocess.run(
                    ["just", "--justfile", str(REPO / "Justfile"), recipe, *args],
                    capture_output=True, text=True, cwd=REPO, env=environment, timeout=60,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
            remote = [c[-1] for c in stubs.calls("ssh")]
            self.assertIn(r"a\\\ b\\\ \\\$HOME", remote[0])
            self.assertIn("--shell-modal", remote[1])
        finally:
            stubs.cleanup()

    def test_testbed_stop_all_calls_the_script(self):
        result = self.dry_run("testbed-stop-all")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr.strip(), "tools/linux-testbed/testbed stop-all")

    def test_headless_recipes_call_the_script(self):
        for recipe, args, expected in (
            ("testbed-headless-start", ["1280x800@1"], "tools/linux-testbed/testbed headless-start 1280x800@1"),
            ("testbed-headless-stop", [], "tools/linux-testbed/testbed headless-stop"),
        ):
            result = self.dry_run(recipe, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr.strip(), expected)

    def test_recording_recipes_call_the_script(self):
        for recipe, args, expected in (
            ("testbed-record-start", ["opening"], "tools/linux-testbed/testbed record-start opening"),
            ("testbed-record-stop", [], "tools/linux-testbed/testbed record-stop"),
        ):
            result = self.dry_run(recipe, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr.strip(), expected)

    def test_tray_recipes_call_the_script(self):
        for recipe, args, expected in (
            ("testbed-tray", ["on"], "tools/linux-testbed/testbed tray on"),
            ("testbed-tray-icon", [], "tools/linux-testbed/testbed tray-icon"),
            ("testbed-tray-click", [], "tools/linux-testbed/testbed tray-click left"),
        ):
            result = self.dry_run(recipe, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr.strip(), expected)

    def test_clipboard_recipes_call_the_script(self):
        for recipe, args, expected in (
            ("testbed-clipboard-set", ["a b"], "tools/linux-testbed/testbed clipboard-set 'a b'"),
            ("testbed-clipboard-get", [], "tools/linux-testbed/testbed clipboard-get"),
        ):
            result = self.dry_run(recipe, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr.strip(), expected)

    def test_pointer_and_click_recipes_call_the_script(self):
        for recipe, args, expected in (
            ("testbed-pointer", ["10", "20"], "tools/linux-testbed/testbed pointer 10 20"),
            ("testbed-click", ["10", "20"], "tools/linux-testbed/testbed click 10 20 left"),
        ):
            result = self.dry_run(recipe, *args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr.strip(), expected)

    def test_testbed_selftest_passes_its_items(self):
        result = self.dry_run("testbed-selftest", "1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr.strip(), "tools/linux-testbed/selftest 1")


def remote_ssh_stub(home):
    """An ssh stub that behaves like the far end: it logs its argv, then
    joins the arguments after the host and hands them to bash, as sshd
    does with the login shell, with HOME set to a temporary directory."""
    return textwrap.dedent(
        f"""\
        #!/usr/bin/env bash
        python3 -c 'import json, sys; print(json.dumps(sys.argv[1:]))' ssh "$@" >>"$STUB_LOG"
        shift
        HOME={home} exec bash -c "$*"
        """
    )


class TestbedTransport(unittest.TestCase):
    """Commands reach the machine through `bash -c` or through the runner
    prefix, with every argument intact."""

    # Spaces, quotes, `$` and a backtick: anything a shell on either side
    # would split or expand if the quoting were wrong.
    awkward = ["--monitor", "HDMI-1:90:2", "a b", "$HOME", "`date`", "it's", 'say "hi"']

    def setUp(self):
        self.stubs = Stubs(["rsync", "scp"])
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="testbed-home-"))
        ssh = self.stubs.dir / "ssh"
        ssh.write_text(remote_ssh_stub(self.home))
        ssh.chmod(0o755)
        # The provision script on the far end records the argv it got.
        fake = self.home / "torchsnap" / "tools" / "linux-testbed" / "provision" / "provision"
        fake.parent.mkdir(parents=True)
        fake.write_text(
            "#!/usr/bin/env bash\n"
            "python3 -c 'import json, sys; print(json.dumps(sys.argv[1:]))' \"$@\" >\"$HOME/argv.json\"\n"
            'exit "${FAKE_EXIT:-0}"\n'
        )
        fake.chmod(0o755)
        # A runner like the visible terminal's: it takes the command after
        # `--`, joins its arguments and evals them.
        runner = self.stubs.dir / "fake-runner"
        runner.write_text('#!/usr/bin/env bash\n[ "$1" = "--" ] && shift\neval "$*"\n')
        runner.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def provision(self, args, env=None):
        environment = {"TORCHSNAP_LINUX_HOST": "user@pi.local", "TORCHSNAP_LINUX_RUNNER": ""}
        environment.update(env or {})
        return run(["provision", *args], self.stubs, env=environment)

    def received_argv(self):
        return json.loads((self.home / "argv.json").read_text())

    def test_arguments_survive_bash_c(self):
        result = self.provision(self.awkward)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.received_argv(), self.awkward)
        remote = self.stubs.calls("ssh")[-1]
        self.assertTrue(remote[2].startswith("bash -c "), remote)

    def test_arguments_survive_the_runner(self):
        result = self.provision(self.awkward, env={"TORCHSNAP_LINUX_RUNNER": "fake-runner --"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.received_argv(), self.awkward)
        remote = self.stubs.calls("ssh")[-1]
        self.assertTrue(remote[2].startswith("fake-runner -- "), remote)

    def test_provision_syncs_before_running(self):
        result = self.provision([])
        self.assertEqual(result.returncode, 0, result.stderr)
        names = [os.path.basename(c[0]) for c in self.stubs.calls()]
        self.assertEqual(names, ["rsync", "ssh"])

    def test_remote_exit_status_passes_through(self):
        result = self.provision([], env={"FAKE_EXIT": "4"})
        self.assertEqual(result.returncode, 4)

    def test_runner_refusal_is_explained(self):
        result = self.provision(
            [], env={"FAKE_EXIT": "75", "TORCHSNAP_LINUX_RUNNER": "fake-runner --"}
        )
        self.assertEqual(result.returncode, 75)
        self.assertIn("runner refused (exit 75)", result.stderr)

    def test_exec_runs_a_command_line_on_the_machine(self):
        result = run(
            ["exec", "printf '%s|' \"$HOME\" 'a b'"],
            self.stubs,
            env={"TORCHSNAP_LINUX_HOST": "user@pi.local", "TORCHSNAP_LINUX_RUNNER": "fake-runner --"},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, f"{self.home}|a b|")

    def test_exit_75_without_runner_is_not_called_a_refusal(self):
        result = self.provision([], env={"FAKE_EXIT": "75"})
        self.assertEqual(result.returncode, 75)
        self.assertNotIn("runner refused", result.stderr)


def with_steps(snippet):
    """Runs a bash snippet with provision/steps.sh sourced."""
    return subprocess.run(
        ["bash", "-c", f"set -euo pipefail; source '{PROVISION_DIR / 'steps.sh'}'; {snippet}"],
        capture_output=True,
        text=True,
        timeout=60,
    )


class ProvisionSteps(unittest.TestCase):
    def test_a_passing_check_is_ok(self):
        result = with_steps("step demo true false; steps_summary")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ok demo", result.stdout)
        self.assertIn("provision: 0 changed, 1 ok, 0 deferred", result.stdout)

    def test_a_check_that_passes_after_apply_is_changed(self):
        # An apply runs in a subshell; what it changes is the machine, here
        # a file.
        result = with_steps(
            'marker="$(mktemp -u)"; check() { [ -e "$marker" ]; }; apply() { touch "$marker"; }; '
            'step demo check apply; steps_summary; rm "$marker"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("changed demo", result.stdout)
        self.assertIn("provision: 1 changed, 0 ok, 0 deferred", result.stdout)

    def test_an_apply_stops_at_its_first_failing_command(self):
        result = with_steps(
            'marker="$(mktemp -u)"; check() { [ -e "$marker" ]; }; '
            'apply() { false; touch "$marker"; }; step demo check apply; echo after'
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("failed demo (apply failed)", result.stderr)
        self.assertNotIn("after", result.stdout)

    def test_a_check_that_still_fails_after_apply_stops_the_run(self):
        result = with_steps("step demo false true; echo after")
        self.assertEqual(result.returncode, 1)
        self.assertIn("failed demo (check still fails after apply)", result.stderr)
        self.assertNotIn("after", result.stdout)

    def test_a_failing_apply_stops_the_run(self):
        result = with_steps("step demo false false; echo after")
        self.assertEqual(result.returncode, 1)
        self.assertIn("failed demo (apply failed)", result.stderr)
        self.assertNotIn("after", result.stdout)

    def test_a_session_step_without_a_session_is_deferred(self):
        result = with_steps("step demo false true session; steps_summary")
        self.assertEqual(result.returncode, 3)
        self.assertIn("deferred demo", result.stdout)
        self.assertIn("provision: 0 changed, 0 ok, 1 deferred", result.stdout)

    def test_wait_for_gives_up_after_its_timeout(self):
        result = with_steps(
            "start=$SECONDS; if wait_for 2 1 false; then echo waited; "
            "else echo \"gave-up after $((SECONDS - start))\"; fi"
        )
        self.assertRegex(result.stdout, r"gave-up after [2-4]\b")

    def test_wait_for_returns_once_the_command_succeeds(self):
        result = with_steps(
            "marker=$(mktemp); (sleep 1; rm \"$marker\") & "
            "wait_for 10 1 test ! -e \"$marker\" && echo done"
        )
        self.assertIn("done", result.stdout)


class SelftestRunner(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["ssh", "rsync", "scp"])

    def tearDown(self):
        self.stubs.cleanup()

    def selftest(self, args):
        return run(args, self.stubs, env={"TORCHSNAP_LINUX_HOST": "user@pi.local"}, script=SELFTEST)

    def test_an_unknown_item_exits_2_before_anything_runs(self):
        result = self.selftest(["1", "nope"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("unknown item: nope", result.stderr)
        self.assertEqual(self.stubs.calls(), [])

    def test_a_failing_item_fails_the_run(self):
        # The stub ssh prints nothing, so provision never reports
        # "0 changed".
        result = self.selftest(["1"])
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL item 1", result.stdout)


# An ssh stub for item U. It logs the command line that would run on the
# machine. Provision runs answer like a provisioned machine: the first
# with `0 changed`, each later one with `changed` for the next step in
# STUB_UNDONE_STEPS, the step item U has just undone.
UNDO_SSH = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, pathlib, shlex, sys
    log = pathlib.Path(os.environ["STUB_LOG"])
    args = sys.argv[1:]
    while args and args[0] == "-o":
        args = args[2:]
    command = shlex.split(" ".join(args[1:]))[-1]
    with log.open("a") as f:
        f.write(json.dumps(["ssh", command]) + "\\n")
    if "provision/provision" in command:
        counter = log.with_name("provision.count")
        n = int(counter.read_text()) if counter.exists() else 0
        counter.write_text(str(n + 1))
        if n == 0:
            print("provision: 0 changed, 18 ok, 0 deferred")
        else:
            print("changed " + os.environ["STUB_UNDONE_STEPS"].split(",")[n - 1])
            print("provision: 1 changed, 17 ok, 0 deferred")
    """
)


class SelftestUndo(unittest.TestCase):
    STEPS = [
        "testbed-extension",
        "dconf-defaults",
        "gdm",
        "accountsservice-session",
        "uinput",
        "ydotool-service",
        "sleep-masked",
        "user-settings",
        "keyring",
        "rdp",
    ]

    def setUp(self):
        self.stubs = Stubs(["rsync", "scp"])
        self.stubs.add("ssh", UNDO_SSH)

    def tearDown(self):
        self.stubs.cleanup()

    def undo_commands(self):
        result = run(
            ["U"],
            self.stubs,
            env={"TORCHSNAP_LINUX_HOST": "user@pi.local", "STUB_UNDONE_STEPS": ",".join(self.STEPS)},
            unset=["TORCHSNAP_LINUX_RUNNER"],
            script=SELFTEST,
        )
        self.assertIn("PASS item U", result.stdout, result.stdout + result.stderr)
        commands = [call[1] for call in self.stubs.calls("ssh") if "provision/provision" not in call[1]]
        # The first command records the desktop's state before the items.
        return dict(zip(self.STEPS, commands[1:], strict=True))

    def test_the_sleep_undo_unmasks_every_masked_target(self):
        undo = self.undo_commands()["sleep-masked"].split()
        self.assertIn("unmask", undo)
        for target in ("sleep.target", "suspend.target", "hibernate.target", "hybrid-sleep.target"):
            self.assertIn(target, undo)

    def test_the_keyring_undo_restores_the_recorded_default(self):
        undo = self.undo_commands()["keyring"]
        self.assertIn("~/.config/torchsnap-testbed/previous-default-keyring", undo)
        self.assertIn("SetAlias", undo)
        self.assertNotIn("collection/login", undo)


# An ssh stub for the reboot and the provision orchestration. It answers by
# what the remote command asks for: boot ids from STUB_BOOT_IDS (one per
# call, the last one repeating), exit codes for provision runs from
# STUB_PROVISION_EXITS (same), and STUB_READY_EXIT for session-ready.
MACHINE_SSH = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, pathlib, sys
    log = pathlib.Path(os.environ["STUB_LOG"])
    with log.open("a") as f:
        f.write(json.dumps(sys.argv) + "\\n")
    args = sys.argv[1:]
    while args and args[0] == "-o":
        args = args[2:]
    command = " ".join(args[1:])

    def next_of(name, default):
        values = os.environ.get(name, default).split(",")
        counter = log.with_name(name + ".count")
        n = int(counter.read_text()) if counter.exists() else 0
        counter.write_text(str(n + 1))
        return values[min(n, len(values) - 1)]

    if "boot_id" in command:
        print(next_of("STUB_BOOT_IDS", "boot-a"))
    elif "provision/provision" in command:
        sys.exit(int(next_of("STUB_PROVISION_EXITS", "0")))
    elif "session-ready" in command:
        sys.exit(int(os.environ.get("STUB_READY_EXIT", "0")))
    elif "reboot" in command:
        sys.exit(int(os.environ.get("STUB_REBOOT_EXIT", "0")))
    """
)


class TestbedReboot(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["rsync", "scp"])
        ssh = self.stubs.dir / "ssh"
        ssh.write_text(MACHINE_SSH)
        ssh.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()

    def call_testbed(self, args, env):
        environment = {
            "TORCHSNAP_LINUX_HOST": "user@pi.local",
            "TORCHSNAP_LINUX_RUNNER": "fake-runner --",
            "TORCHSNAP_LINUX_REBOOT_TIMEOUT": "3",
            "TORCHSNAP_LINUX_POLL_INTERVAL": "1",
        }
        environment.update(env)
        return run(args, self.stubs, env=environment)

    def remote_commands(self):
        commands = []
        for call in self.stubs.calls("ssh"):
            args = call[1:]
            while args and args[0] == "-o":
                args = args[2:]
            commands.append(" ".join(args[1:]))
        return commands

    def test_reboot_waits_for_a_new_boot_and_a_ready_session(self):
        result = self.call_testbed(["reboot"], {"STUB_BOOT_IDS": "boot-a,boot-a,boot-b"})
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = self.remote_commands()
        self.assertEqual(sum("reboot" in c for c in commands), 1)
        self.assertTrue(any("session-ready" in c for c in commands))

    def test_reboot_never_goes_through_the_runner(self):
        self.call_testbed(["reboot"], {"STUB_BOOT_IDS": "boot-a,boot-b"})
        commands = self.remote_commands()
        self.assertTrue(any("--no-block" in c and "reboot" in c for c in commands), commands)
        for command in commands:
            self.assertNotIn("fake-runner", command)

    def test_reboot_survives_the_machine_closing_the_connection(self):
        # ssh exits 255 when the machine goes down before the reboot
        # command has returned.
        result = self.call_testbed(
            ["reboot"], {"STUB_BOOT_IDS": "boot-a,boot-b", "STUB_REBOOT_EXIT": "255"}
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_closed_connection_without_a_reboot_still_fails(self):
        # ssh also exits 255 when it cannot connect at all; without a new
        # boot id that is no reboot.
        result = self.call_testbed(["reboot"], {"STUB_BOOT_IDS": "boot-a", "STUB_REBOOT_EXIT": "255"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not come back", result.stderr)

    def test_reboot_fails_when_the_reboot_command_fails(self):
        result = self.call_testbed(
            ["reboot"], {"STUB_BOOT_IDS": "boot-a,boot-b", "STUB_REBOOT_EXIT": "1"}
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("reboot command failed", result.stderr)

    def test_reboot_gives_up_when_the_machine_does_not_come_back(self):
        result = self.call_testbed(["reboot"], {"STUB_BOOT_IDS": "boot-a"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not come back", result.stderr)

    def test_reboot_gives_up_when_the_session_is_not_ready(self):
        result = self.call_testbed(["reboot"], {"STUB_BOOT_IDS": "boot-a,boot-b", "STUB_READY_EXIT": "1"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("session not ready", result.stderr)

    def test_deferred_steps_get_a_reboot_and_a_second_run(self):
        result = self.call_testbed(
            ["provision"], {"STUB_PROVISION_EXITS": "3,0", "STUB_BOOT_IDS": "boot-a,boot-b"}
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = self.remote_commands()
        self.assertEqual(sum("provision/provision" in c for c in commands), 2)
        self.assertEqual(sum("--no-block" in c and "reboot" in c for c in commands), 1)

    def test_steps_deferred_twice_fail_the_run(self):
        result = self.call_testbed(
            ["provision"], {"STUB_PROVISION_EXITS": "3,3", "STUB_BOOT_IDS": "boot-a,boot-b"}
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("still deferred", result.stderr)


def with_gnome_steps(snippet, stubs):
    """Runs a bash snippet with the GNOME provision steps sourced and the
    stub directory first on PATH."""
    environment = dict(os.environ)
    environment["PATH"] = f"{stubs.dir}{os.pathsep}{os.environ['PATH']}"
    environment["STUB_LOG"] = str(stubs.log)
    return subprocess.run(
        [
            "bash",
            "-c",
            f"set -euo pipefail; provision_dir='{PROVISION_DIR}'; monitor_spec=''; "
            f"source '{PROVISION_DIR / 'steps.sh'}'; "
            f"source '{PROVISION_DIR / 'desktop-gnome.sh'}'; {snippet}",
        ],
        capture_output=True,
        text=True,
        env=environment,
        timeout=60,
    )


class ProvisionGnome(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["gdctl"])
        # sudo runs its arguments as the test user.
        sudo = self.stubs.dir / "sudo"
        sudo.write_text('#!/usr/bin/env bash\nexec "$@"\n')
        sudo.chmod(0o755)
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="testbed-accounts-"))

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.dir)

    def set_session(self, content):
        path = self.dir / "testuser"
        if content is not None:
            path.write_text(content)
        result = with_gnome_steps(f"set_accountsservice_session '{path}'", self.stubs)
        self.assertEqual(result.returncode, 0, result.stderr)
        return path.read_text()

    def test_session_in_a_missing_file(self):
        self.assertEqual(self.set_session(None), "[User]\nSession=gnome\n")
        # AccountsService keeps these files readable by root only.
        self.assertEqual(stat.S_IMODE((self.dir / "testuser").stat().st_mode), 0o600)

    def test_session_added_to_a_user_group_without_one(self):
        self.assertEqual(
            self.set_session("[User]\nXSession=rpd-labwc\nIcon=/x\n"),
            "[User]\nSession=gnome\nXSession=rpd-labwc\nIcon=/x\n",
        )

    def test_session_replaced_and_other_keys_kept(self):
        self.assertEqual(
            self.set_session("[User]\nLanguages=en_GB.utf8;\nSession=\nXSession=rpd-labwc\n"),
            "[User]\nLanguages=en_GB.utf8;\nSession=gnome\nXSession=rpd-labwc\n",
        )

    def test_session_already_set_stays(self):
        self.assertEqual(self.set_session("[User]\nSession=gnome\n"), "[User]\nSession=gnome\n")

    def test_session_in_a_file_without_user_group(self):
        self.assertEqual(self.set_session("Other=1\n"), "Other=1\n[User]\nSession=gnome\n")

    def append(self, value, item):
        result = with_gnome_steps(f"append_to_gvariant_list \"{value}\" \"{item}\"", self.stubs)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_append_to_an_empty_typed_list(self):
        self.assertEqual(self.append("@as []", "'x'"), "['x']")

    def test_append_to_an_empty_list(self):
        self.assertEqual(self.append("[]", "'x'"), "['x']")

    def test_append_to_a_list(self):
        self.assertEqual(
            self.append("[('xkb', 'de')]", "('xkb', 'us')"), "[('xkb', 'de'), ('xkb', 'us')]"
        )

    def test_logical_monitor_of_a_connector(self):
        gdctl_show = textwrap.dedent(
            """\
            Monitors:
            └──Monitor HDMI-1 (LG Electronics 32")
               └──Current mode
            Logical monitors:
            ├──Logical monitor #1
            │  ├──Scale: 1.0
            │  ├──Transform: normal
            │  └──Monitors: (1)
            │      └──DP-1 (Other)
            └──Logical monitor #2
               ├──Scale: 2.0
               ├──Transform: 90
               └──Monitors: (1)
                   └──HDMI-1 (LG Electronics 32")
            """
        )
        result = subprocess.run(
            [
                "bash",
                "-c",
                f"provision_dir='{PROVISION_DIR}'; source '{PROVISION_DIR / 'steps.sh'}'; "
                f"source '{PROVISION_DIR / 'desktop-gnome.sh'}'; gdctl_logical_monitor HDMI-1",
            ],
            capture_output=True,
            text=True,
            env={
                **os.environ,
                "PATH": f"{self.stubs.dir}{os.pathsep}{os.environ['PATH']}",
                "STUB_LOG": str(self.stubs.log),
                "STUB_GDCTL_STDOUT": gdctl_show,
            },
            timeout=60,
        )
        self.assertEqual(result.stdout.strip(), "2.0 90", result.stderr)

    def test_the_keyring_template_holds_no_secret(self):
        template = (PROVISION_DIR / "gnome" / "testbed.keyring").read_text()
        groups = [line for line in template.splitlines() if line.startswith("[")]
        self.assertEqual(groups, ["[keyring]"])
        self.assertNotIn("secret", template)


YDOTOOL_COMMIT = "57ba7d0af525e82da2de0e275d169477f293b197"
TESTBED_KEYRING = "/org/freedesktop/secrets/collection/testbed"
LOGIN_KEYRING = "/org/freedesktop/secrets/collection/login"


class ProvisionApplies(unittest.TestCase):
    """The commands each provision step's apply runs, and the order of the
    steps. Every command that would change this machine is stubbed, sudo
    runs nothing, and HOME is a temporary directory."""

    def setUp(self):
        self.stubs = Stubs(
            ["git", "cmake", "curl", "sh", "bash", "cargo", "systemctl", "grdctl", "openssl", "gdctl"]
        )
        self.stubs.add("sudo", SUDO_STUB)
        self.stubs.add("busctl", BUSCTL_STUB)
        self.stubs.add("gsettings", GSETTINGS_STUB)
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="testbed-home-"))
        self.tree = pathlib.Path(tempfile.mkdtemp(prefix="testbed-tree-"))

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)
        shutil.rmtree(self.tree)

    def shell(self, snippet, env=None, monitor_spec=""):
        environment = dict(os.environ)
        environment.update(
            {
                "PATH": f"{self.stubs.dir}{os.pathsep}{os.environ['PATH']}",
                "STUB_LOG": str(self.stubs.log),
                "HOME": str(self.home),
                "USER": "testuser",
            }
        )
        environment.update(env or {})
        return subprocess.run(
            [
                BASH,
                "-c",
                f"set -euo pipefail; provision_dir='{PROVISION_DIR}'; tree_dir='{self.tree}'; "
                f"monitor_spec='{monitor_spec}'; source '{PROVISION_DIR / 'steps.sh'}'; "
                f"source '{PROVISION_DIR / 'base.sh'}'; source '{PROVISION_DIR / 'desktop-gnome.sh'}'; "
                f"{snippet}",
            ],
            capture_output=True,
            text=True,
            env=environment,
            stdin=subprocess.DEVNULL,
            timeout=60,
        )

    def calls(self, name):
        return [call[1:] for call in self.stubs.calls(name)]

    # =========================================================
    # Desktop-independent steps
    # =========================================================

    def test_base_packages_are_installed_after_an_update(self):
        result = self.shell("apply_base_packages")
        self.assertEqual(result.returncode, 0, result.stderr)
        update, install = self.calls("sudo")
        self.assertEqual(update, ["apt-get", "update"])
        self.assertEqual(install[:4], ["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y"])
        self.assertIn("libwebkit2gtk-4.1-dev", install)
        self.assertIn("scdoc", install)

    def test_rustup_installs_without_a_default_toolchain(self):
        result = self.shell("apply_rustup")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("https://sh.rustup.rs", self.calls("curl")[0])
        self.assertEqual(
            self.calls("sh"), [["-s", "--", "-y", "--default-toolchain", "none", "--profile", "minimal"]]
        )

    def test_bun_installs_its_pinned_version(self):
        result = self.shell("apply_bun")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("https://bun.sh/install", self.calls("curl")[0])
        self.assertEqual(self.calls("bash"), [["-s", "bun-v1.4.4"]])

    def test_just_installs_its_pinned_version_into_local_bin(self):
        result = self.shell("apply_just")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("https://just.systems/install.sh", self.calls("curl")[0])
        self.assertEqual(
            self.calls("bash"),
            [["-s", "--", "--tag", "1.58.0", "--to", f"{self.home}/.local/bin", "--force"]],
        )
        self.assertTrue((self.home / ".local" / "bin").is_dir())

    def test_wasm_tools_installs_its_pinned_version(self):
        result = self.shell("apply_wasm_tools")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("cargo"), [["install", "wasm-tools", "--version", "1.246.2", "--locked", "--force"]]
        )

    def wasm_tools_check(self, version_line):
        tool = self.home / ".cargo" / "bin" / "wasm-tools"
        tool.parent.mkdir(parents=True, exist_ok=True)
        tool.write_text(f"#!/bin/sh\necho '{version_line}'\n")
        tool.chmod(0o755)
        return self.shell("check_wasm_tools").returncode

    def test_the_wasm_tools_check_needs_the_exact_version(self):
        self.assertEqual(self.wasm_tools_check("wasm-tools 1.246.2"), 0)
        self.assertNotEqual(self.wasm_tools_check("wasm-tools 1.246.20"), 0)
        self.assertNotEqual(self.wasm_tools_check("wasm-tools 1.246.1"), 0)

    def test_ydotool_is_built_from_the_pinned_commit(self):
        result = self.shell("apply_ydotool", env={"STUB_GIT_STDOUT": YDOTOOL_COMMIT + "\n"})
        self.assertEqual(result.returncode, 0, result.stderr)
        src = f"{self.home}/.cache/torchsnap-testbed/ydotool-src"
        self.assertEqual(
            self.calls("git"),
            [
                ["clone", "--depth", "1", "--branch", "v1.0.4", "https://github.com/ReimuNotMoe/ydotool", src],
                ["-C", src, "rev-parse", "HEAD"],
            ],
        )
        self.assertEqual(
            self.calls("cmake"),
            [["-S", src, "-B", f"{src}/build", "-DCMAKE_BUILD_TYPE=Release"], ["--build", f"{src}/build", "-j4"]],
        )
        self.assertEqual(
            self.calls("sudo"),
            [
                ["cmake", "--install", f"{src}/build"],
                ["mkdir", "-p", "/usr/local/share/torchsnap-testbed"],
                ["touch", "/usr/local/share/torchsnap-testbed/ydotool-v1.0.4"],
            ],
        )

    def test_ydotool_from_another_commit_is_not_built(self):
        result = self.shell("apply_ydotool", env={"STUB_GIT_STDOUT": "0" * 40 + "\n"})
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls("cmake"), [])
        self.assertEqual(self.calls("sudo"), [])

    def test_a_failing_build_leaves_ydotool_without_its_stamp(self):
        result = self.shell(
            "step ydotool false apply_ydotool",
            env={"STUB_GIT_STDOUT": YDOTOOL_COMMIT + "\n", "STUB_CMAKE_FAIL_ON": "--build"},
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("failed ydotool (apply failed)", result.stderr)
        self.assertEqual(self.calls("sudo"), [])

    def test_uinput_rule_and_module_are_installed_and_applied(self):
        result = self.shell("apply_uinput")
        self.assertEqual(result.returncode, 0, result.stderr)
        base = PROVISION_DIR / "base"
        self.assertEqual(
            self.calls("sudo"),
            [
                [
                    "install",
                    "-m",
                    "0644",
                    f"{base}/60-torchsnap-testbed-uinput.rules",
                    "/etc/udev/rules.d/60-torchsnap-testbed-uinput.rules",
                ],
                [
                    "install",
                    "-m",
                    "0644",
                    f"{base}/torchsnap-testbed-uinput.conf",
                    "/etc/modules-load.d/torchsnap-testbed-uinput.conf",
                ],
                ["udevadm", "control", "--reload"],
                ["modprobe", "uinput"],
                ["udevadm", "trigger", "--action=add", "--subsystem-match=misc", "--sysname-match=uinput"],
                ["udevadm", "settle"],
            ],
        )

    def test_ydotool_service_is_enabled_and_restarted(self):
        result = self.shell("apply_ydotool_service")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("systemctl"),
            [
                ["--user", "daemon-reload"],
                ["--user", "reset-failed", "ydotool.service"],
                ["--user", "enable", "--now", "ydotool.service"],
                ["--user", "restart", "ydotool.service"],
            ],
        )

    def test_all_four_sleep_targets_are_masked(self):
        result = self.shell("apply_sleep_masked")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("sudo"),
            [["systemctl", "mask", "sleep.target", "suspend.target", "hibernate.target", "hybrid-sleep.target"]],
        )

    # =========================================================
    # GNOME steps
    # =========================================================

    def test_gnome_packages_keep_the_current_display_manager(self):
        marker = pathlib.Path("/etc/X11/default-display-manager")
        current = os.path.basename(marker.read_text().strip()) if marker.exists() else "lightdm"
        result = self.shell("apply_gnome_packages")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("sudo"),
            [
                ["debconf-set-selections", f"stdin:gdm3 shared/default-x-display-manager select {current}\n"],
                [
                    "debconf-set-selections",
                    f"stdin:{current} shared/default-x-display-manager select {current}\n",
                ],
                ["apt-get", "update"],
                [
                    "DEBIAN_FRONTEND=noninteractive",
                    "apt-get",
                    "install",
                    "-y",
                    "task-gnome-desktop",
                    "gnome-shell-extension-appindicator",
                    "gstreamer1.0-pipewire",
                ],
            ],
        )

    def test_the_extension_is_installed_system_wide_and_the_user_copy_removed(self):
        uuid = "unsafe-mode@testbed.torchsnap.app"
        user_copy = self.home / ".local" / "share" / "gnome-shell" / "extensions" / uuid
        user_copy.mkdir(parents=True)
        (user_copy / "extension.js").write_text("// an older copy\n")
        result = self.shell("apply_extension")
        self.assertEqual(result.returncode, 0, result.stderr)
        source = PROVISION_DIR / "gnome" / uuid
        target = f"/usr/local/share/gnome-shell/extensions/{uuid}"
        self.assertEqual(
            self.calls("sudo"),
            [
                ["install", "-d", target],
                ["install", "-m", "0644", f"{source}/metadata.json", f"{source}/extension.js", f"{target}/"],
            ],
        )
        self.assertFalse(user_copy.exists())

    def test_dconf_defaults_are_installed_and_compiled(self):
        result = self.shell("apply_dconf_defaults")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("sudo"),
            [
                [
                    "install",
                    "-m",
                    "0644",
                    f"{PROVISION_DIR}/gnome/50-torchsnap-testbed",
                    "/etc/dconf/db/local.d/50-torchsnap-testbed",
                ],
                ["dconf", "update"],
            ],
        )

    def test_gdm_logs_the_user_in_and_becomes_the_display_manager(self):
        result = self.shell("apply_gdm")
        self.assertEqual(result.returncode, 0, result.stderr)
        daemon_conf = (PROVISION_DIR / "gnome" / "daemon.conf").read_text().replace("@USER@", "testuser")
        self.assertIn("AutomaticLogin=testuser\n", daemon_conf)
        self.assertEqual(
            self.calls("sudo"),
            [
                ["tee", "/etc/gdm3/daemon.conf", "stdin:" + daemon_conf],
                ["tee", "/etc/X11/default-display-manager", "stdin:/usr/sbin/gdm3\n"],
                ["ln", "-sf", "/usr/lib/systemd/system/gdm.service", "/etc/systemd/system/display-manager.service"],
                ["systemctl", "daemon-reload"],
            ],
        )

    def test_a_failing_daemon_conf_write_leaves_the_display_manager(self):
        result = self.shell("step gdm false apply_gdm", env={"STUB_SUDO_FAIL_ON": "tee /etc/gdm3/daemon.conf"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("failed gdm (apply failed)", result.stderr)
        self.assertEqual([call[:2] for call in self.calls("sudo")], [["tee", "/etc/gdm3/daemon.conf"]])

    def test_user_settings_append_the_extension_and_the_us_layout(self):
        result = self.shell(
            "apply_user_settings",
            env={"STUB_GSETTINGS_ENABLED_EXTENSIONS": "@as []", "STUB_GSETTINGS_SOURCES": "[('xkb', 'de')]"},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [call for call in self.calls("gsettings") if call[0] == "set"],
            [
                ["set", "org.gnome.shell", "enabled-extensions", "['unsafe-mode@testbed.torchsnap.app']"],
                ["set", "org.gnome.desktop.input-sources", "sources", "[('xkb', 'de'), ('xkb', 'us')]"],
            ],
        )

    def test_user_settings_in_place_are_not_written(self):
        result = self.shell(
            "apply_user_settings",
            env={
                "STUB_GSETTINGS_ENABLED_EXTENSIONS": "['unsafe-mode@testbed.torchsnap.app']",
                "STUB_GSETTINGS_SOURCES": "[('xkb', 'de'), ('xkb', 'us')]",
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call for call in self.calls("gsettings") if call[0] == "set"], [])

    def secret_service_requests(self):
        # `--user call|get-property DEST PATH INTERFACE NAME ARGS...`
        return [[call[1], *call[5:]] for call in self.calls("busctl")]

    def previous_default_keyring(self):
        return self.home / ".config" / "torchsnap-testbed" / "previous-default-keyring"

    def test_the_keyring_is_installed_and_made_the_default(self):
        result = self.shell(
            "apply_keyring",
            env={
                "STUB_BUSCTL_COLLECTIONS": f'ao 2 "{LOGIN_KEYRING}" "{TESTBED_KEYRING}"',
                "STUB_BUSCTL_READALIAS": f'o "{LOGIN_KEYRING}"',
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        keyrings = self.home / ".local" / "share" / "keyrings"
        installed = keyrings / "testbed.keyring"
        self.assertEqual(installed.read_text(), (PROVISION_DIR / "gnome" / "testbed.keyring").read_text())
        self.assertEqual(stat.S_IMODE(installed.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(keyrings.stat().st_mode), 0o700)
        self.assertEqual(
            self.secret_service_requests(),
            [
                ["get-property", "Collections"],
                ["call", "Unlock", "ao", "1", TESTBED_KEYRING],
                ["call", "ReadAlias", "s", "default"],
                ["call", "SetAlias", "so", "default", TESTBED_KEYRING],
            ],
        )
        self.assertEqual(self.previous_default_keyring().read_text(), LOGIN_KEYRING + "\n")
        self.assertEqual(self.calls("systemctl"), [])

    def test_the_keyring_daemon_restarts_until_it_knows_the_keyring(self):
        result = self.shell(
            "apply_keyring",
            env={
                "STUB_BUSCTL_COLLECTIONS": f'ao 1 "{LOGIN_KEYRING}"|ao 2 "{LOGIN_KEYRING}" "{TESTBED_KEYRING}"',
                "STUB_BUSCTL_READALIAS": f'o "{LOGIN_KEYRING}"',
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls("systemctl"), [["--user", "restart", "gnome-keyring-daemon.service"]])

    def test_the_recorded_default_survives_a_second_apply(self):
        record = self.previous_default_keyring()
        record.parent.mkdir(parents=True)
        record.write_text(LOGIN_KEYRING + "\n")
        result = self.shell(
            "apply_keyring",
            env={
                "STUB_BUSCTL_COLLECTIONS": f'ao 2 "{LOGIN_KEYRING}" "{TESTBED_KEYRING}"',
                "STUB_BUSCTL_READALIAS": f'o "{TESTBED_KEYRING}"',
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(record.read_text(), LOGIN_KEYRING + "\n")

    def test_no_default_keyring_before_is_recorded_as_none(self):
        # The Secret Service names no collection with "/", which SetAlias
        # takes to remove the alias again.
        result = self.shell(
            "apply_keyring",
            env={
                "STUB_BUSCTL_COLLECTIONS": f'ao 1 "{TESTBED_KEYRING}"',
                "STUB_BUSCTL_READALIAS": 'o "/"',
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.previous_default_keyring().read_text(), "/\n")

    def rdp_in_place(self):
        password = self.home / ".config" / "torchsnap-testbed" / "rdp-password"
        password.parent.mkdir(parents=True)
        password.write_text("password-for-tests")
        rdp_dir = self.home / ".local" / "share" / "gnome-remote-desktop"
        rdp_dir.mkdir(parents=True)
        (rdp_dir / "rdp-tls.key").write_text("")
        (rdp_dir / "rdp-tls.crt").write_text("")
        return rdp_dir

    def test_rdp_is_configured_and_started(self):
        rdp_dir = self.rdp_in_place()
        result = self.shell("apply_rdp")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("grdctl"),
            [
                ["rdp", "set-tls-cert", f"{rdp_dir}/rdp-tls.crt"],
                ["rdp", "set-tls-key", f"{rdp_dir}/rdp-tls.key"],
                ["rdp", "set-credentials", "testbed", "password-for-tests"],
                ["rdp", "disable-view-only"],
                ["rdp", "enable"],
            ],
        )
        self.assertEqual(self.calls("systemctl"), [["--user", "enable", "--now", "gnome-remote-desktop.service"]])
        self.assertEqual(self.calls("openssl"), [])

    def test_rdp_generates_its_password_and_certificate(self):
        result = self.shell("apply_rdp", env={"STUB_OPENSSL_STDOUT": "ab/c+d=e\n"})
        self.assertEqual(result.returncode, 0, result.stderr)
        password = self.home / ".config" / "torchsnap-testbed" / "rdp-password"
        self.assertEqual(password.read_text(), "abcde")
        self.assertEqual(stat.S_IMODE(password.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(password.parent.stat().st_mode), 0o700)
        rdp_dir = self.home / ".local" / "share" / "gnome-remote-desktop"
        self.assertEqual(stat.S_IMODE(rdp_dir.stat().st_mode), 0o700)
        self.assertEqual(
            self.calls("openssl"),
            [
                ["rand", "-base64", "18"],
                [
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-days",
                    "3650",
                    "-subj",
                    "/CN=torchsnap-testbed",
                    "-keyout",
                    f"{rdp_dir}/rdp-tls.key",
                    "-out",
                    f"{rdp_dir}/rdp-tls.crt",
                ],
            ],
        )

    def test_failing_credentials_leave_rdp_off(self):
        self.rdp_in_place()
        result = self.shell("step rdp false apply_rdp", env={"STUB_GRDCTL_FAIL_ON": "set-credentials"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("failed rdp (apply failed)", result.stderr)
        self.assertEqual(self.calls("grdctl")[-1][:2], ["rdp", "set-credentials"])
        self.assertEqual(self.calls("systemctl"), [])

    def test_the_monitor_layout_is_stored(self):
        result = self.shell("apply_monitor", monitor_spec="HDMI-1:90:2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls("gdctl"),
            [
                [
                    "set",
                    "--persistent",
                    "--logical-monitor",
                    "--primary",
                    "--scale",
                    "2",
                    "--transform",
                    "90",
                    "--monitor",
                    "HDMI-1",
                ]
            ],
        )

    # =========================================================
    # Order
    # =========================================================

    def test_the_steps_run_in_order(self):
        result = self.shell('step() { echo "$1${4:+ $4}"; }; base_steps; desktop_steps', monitor_spec="HDMI-1:90:2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.splitlines(),
            [
                "base-packages",
                "rustup",
                "bun",
                "just",
                "wasm-tools",
                "ydotool",
                "uinput",
                "ydotool-service",
                "sleep-masked",
                "gnome-packages",
                "testbed-extension",
                "dconf-defaults",
                "gdm",
                "accountsservice-session",
                "user-settings session",
                "keyring session",
                "rdp session",
                "monitor session",
            ],
        )

    def test_the_monitor_step_needs_the_option(self):
        result = self.shell('step() { echo "$1"; }; desktop_steps')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("monitor", result.stdout.splitlines())


class ProvisionOptions(unittest.TestCase):
    def provision(self, *args):
        return subprocess.run(
            ["bash", str(PROVISION_DIR / "provision"), *args],
            capture_output=True,
            text=True,
            timeout=60,
        )

    def test_an_unknown_desktop_is_refused(self):
        result = self.provision("--desktop", "nope")
        self.assertEqual(result.returncode, 2)
        self.assertIn("no provision steps for desktop nope", result.stderr)

    def test_a_malformed_monitor_is_refused(self):
        result = self.provision("--monitor", "HDMI-1:90")
        self.assertEqual(result.returncode, 2)
        self.assertIn("--monitor expects CONNECTOR:TRANSFORM:SCALE", result.stderr)

    def test_an_unknown_option_is_refused(self):
        result = self.provision("--frobnicate")
        self.assertEqual(result.returncode, 2)


SHOW_ENVIRONMENT = (
    "HOME=/home/testuser\n"
    "WAYLAND_DISPLAY=wayland-0\n"
    "DISPLAY=:0\n"
    "XAUTHORITY=/run/user/1000/.mutter-Xwaylandauth.ABC\n"
    "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\n"
    "XDG_CURRENT_DESKTOP=GNOME\n"
    "LANG=en_GB.UTF-8\n"
)


class TestbedPi(unittest.TestCase):
    """The machine-side script, run here with systemctl and gdbus stubbed."""

    def setUp(self):
        self.stubs = Stubs(["systemctl", "gdbus"])

    def tearDown(self):
        self.stubs.cleanup()

    def call_pi(self, args, env=None, unset=()):
        environment = {"STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT}
        environment.update(env or {})
        return run(args, self.stubs, env=environment, unset=unset, script=TESTBED_PI)

    def test_session_env_takes_the_display_variables_from_the_user_manager(self):
        result = self.call_pi(
            ["env"], unset=["WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "XDG_CURRENT_DESKTOP"]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = set(result.stdout.splitlines())
        self.assertIn("WAYLAND_DISPLAY=wayland-0", lines)
        self.assertIn("DISPLAY=:0", lines)
        self.assertIn("XAUTHORITY=/run/user/1000/.mutter-Xwaylandauth.ABC", lines)
        self.assertIn("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus", lines)
        self.assertIn("XDG_CURRENT_DESKTOP=GNOME", lines)
        self.assertNotIn("LANG=en_GB.UTF-8", lines)

    def test_another_desktop_is_refused(self):
        result = self.call_pi(
            ["session-ready"],
            env={"STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT.replace("GNOME", "labwc:wlroots")},
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("no testbed support for desktop labwc:wlroots", result.stderr)

    def test_session_ready_when_startup_is_done_and_the_overview_closed(self):
        result = self.call_pi(
            ["session-ready"],
            env={"STUB_GDBUS_STDOUT": "(true, '\"{\\\\\"startingUp\\\\\":false,\\\\\"overview\\\\\":false}\"')\n"},
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_session_not_ready_while_the_overview_is_open(self):
        result = self.call_pi(
            ["session-ready"],
            env={"STUB_GDBUS_STDOUT": "(true, '\"{\\\\\"startingUp\\\\\":false,\\\\\"overview\\\\\":true}\"')\n"},
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("overview", result.stderr)

    def test_session_not_ready_without_eval(self):
        result = self.call_pi(["session-ready"], env={"STUB_GDBUS_STDOUT": "(false, '')\n"})
        self.assertEqual(result.returncode, 1)


CONTROL_REQUEST = REPO / "tools" / "linux-testbed" / "pi" / "control-request"
WELCOME_RS = REPO / "src-tauri" / "src" / "welcome.rs"
LAYOUT_ERROR = "launcher layout not yet received from frontend"


class FakeControlSocket:
    """A Unix socket that answers each request line with the next of the
    given responses (the last one repeating) and records the requests."""

    def __init__(self, path, responses):
        self.path = str(path)
        self.responses = responses
        self.requests = []
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(self.path)
        self.server.listen()
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def serve(self):
        while True:
            try:
                connection, _ = self.server.accept()
            except OSError:
                return
            with connection:
                data = b""
                while not data.endswith(b"\n"):
                    chunk = connection.recv(65536)
                    if not chunk:
                        break
                    data += chunk
                if not data:
                    continue
                request = json.loads(data)
                self.requests.append(request)
                index = min(len(self.requests) - 1, len(self.responses) - 1)
                response = dict(self.responses[index], jsonrpc="2.0", id=request["id"])
                connection.sendall((json.dumps(response) + "\n").encode())

    def close(self):
        self.server.close()


class TestbedForwarding(unittest.TestCase):
    """Subcommands that run on the machine go to testbed-pi in the synced
    tree, through the runner."""

    def setUp(self):
        self.stubs = Stubs(["rsync", "scp"])
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="testbed-home-"))
        ssh = self.stubs.dir / "ssh"
        ssh.write_text(remote_ssh_stub(self.home))
        ssh.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def install_fake_pi(self):
        fake = self.home / "torchsnap" / "tools" / "linux-testbed" / "pi" / "testbed-pi"
        fake.parent.mkdir(parents=True)
        fake.write_text(
            "#!/usr/bin/env bash\n"
            "python3 -c 'import json, sys; print(json.dumps(sys.argv[1:]))' \"$@\" >>\"$HOME/pi-calls.jsonl\"\n"
        )
        fake.chmod(0o755)

    def pi_calls(self):
        path = self.home / "pi-calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def call(self, *args):
        return run(
            list(args),
            self.stubs,
            env={"TORCHSNAP_LINUX_HOST": "user@pi.local", "TORCHSNAP_LINUX_RUNNER": ""},
        )

    def test_build_syncs_then_builds_on_the_machine(self):
        self.install_fake_pi()
        result = self.call("build")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([os.path.basename(c[0]) for c in self.stubs.calls()], ["rsync", "ssh"])
        self.assertEqual(self.pi_calls(), [["build"]])

    def test_app_and_control_subcommands_pass_their_arguments(self):
        self.install_fake_pi()
        for args in (
            ["app-start"],
            ["app-stop"],
            ["app-log", "50"],
            ["control", "query", '{"text": "a b"}'],
        ):
            result = self.call(*args)
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.pi_calls(),
            [["app-start"], ["app-stop"], ["app-log", "50"], ["control", "query", '{"text": "a b"}']],
        )

    def test_a_missing_tree_asks_for_a_sync(self):
        result = self.call("app-start")
        self.assertEqual(result.returncode, 2)
        self.assertIn("run just testbed-sync first", result.stderr)


class ControlRequests(unittest.TestCase):
    def setUp(self):
        # A Unix socket path has to stay short (104 bytes on macOS), and
        # the per-user temporary directory there is long.
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.data = self.home / ".local" / "share" / "app.torchsnap"
        self.data.mkdir(parents=True)
        self.stubs = Stubs(["systemctl", "gdbus"])

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def control(self, responses, *args):
        fake = FakeControlSocket(self.data / "control.sock", responses)
        try:
            result = run(
                ["control", *args],
                self.stubs,
                env={"HOME": str(self.home), "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
                     "TORCHSNAP_TESTBED_POLL_INTERVAL": "0.2"},
                script=TESTBED_PI,
            )
        finally:
            fake.close()
        return result, fake.requests

    def test_a_result_is_printed_and_exits_0(self):
        result, requests = self.control([{"result": {"visible": False}}], "status")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["result"], {"visible": False})
        self.assertEqual(requests[0]["method"], "status")

    def test_params_are_sent_as_json(self):
        result, requests = self.control([{"result": {"ok": True}}], "query", '{"text": "a b"}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(requests[0]["params"], {"text": "a b"})

    def test_an_error_member_exits_1(self):
        result, _ = self.control([{"error": {"code": -32601, "message": "Method not found: x"}}], "x")
        self.assertEqual(result.returncode, 1)

    def test_show_waits_for_the_launcher_layout(self):
        layout_error = {"error": {"code": -3, "message": LAYOUT_ERROR}}
        result, requests = self.control([layout_error, layout_error, {"result": {"ok": True}}], "show")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(requests), 3)

    def test_show_fails_at_once_on_another_error(self):
        result, requests = self.control([{"error": {"code": -3, "message": "boom"}}], "show")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(requests), 1)

    def test_no_socket_exits_2(self):
        result = run(
            ["control", "status"],
            self.stubs,
            env={"HOME": str(self.home), "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT},
            script=TESTBED_PI,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("control.sock", result.stderr)
        self.assertNotIn("Usage", result.stderr)


class AppSettings(unittest.TestCase):
    """app-start enables the control socket and skips the welcome by
    merging two flat keys into the app's settings.json."""

    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="testbed-home-"))
        self.settings = self.home / ".local" / "share" / "app.torchsnap" / "settings.json"
        self.stubs = Stubs(["systemctl", "systemd-run", "gdbus"])

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def merge(self):
        return run(
            ["merge-app-settings"],
            self.stubs,
            env={"HOME": str(self.home), "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT},
            script=TESTBED_PI,
        )

    def test_the_merge_creates_the_file(self):
        result = self.merge()
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = json.loads(self.settings.read_text())
        self.assertIs(settings["controlChannel.enabled"], True)
        self.assertNotIn("controlChannel", settings)

    def test_the_merge_keeps_other_keys(self):
        self.settings.parent.mkdir(parents=True)
        self.settings.write_text(json.dumps({"mascotMode": "corner", "controlChannel.enabled": False}))
        result = self.merge()
        self.assertEqual(result.returncode, 0, result.stderr)
        settings = json.loads(self.settings.read_text())
        self.assertEqual(settings["mascotMode"], "corner")
        self.assertIs(settings["controlChannel.enabled"], True)

    def test_the_welcome_revision_matches_the_app(self):
        revision = int(re.search(r"pub const WELCOME_REVISION: u64 = (\d+);", WELCOME_RS.read_text()).group(1))
        self.merge()
        settings = json.loads(self.settings.read_text())
        self.assertEqual(settings["welcome.seenRevision"], revision)

    def test_app_start_refuses_while_the_app_runs(self):
        # The stub systemctl exits 0, so `is-active` says the unit runs.
        result = run(
            ["app-start"],
            self.stubs,
            env={"HOME": str(self.home), "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT},
            script=TESTBED_PI,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("already running", result.stderr)
        self.assertEqual(self.stubs.calls("systemd-run"), [])


def eval_answer(value):
    """GNOME Shell's Eval answer for a value, as gdbus prints it: the
    value as JSON inside a GVariant string."""
    text = json.dumps(value).replace("\\", "\\\\").replace("'", "\\'")
    return f"(true, '{text}')\n"


# Two windows of the app (the launcher has an empty title) and one other.
WINDOWS = [
    {"title": "", "wm_class": "torchsnap", "rect": {"x": 140, "y": 677, "width": 808, "height": 860}, "monitor": 0, "focused": True},
    {"title": "Torchsnap Settings", "wm_class": "torchsnap", "rect": {"x": 0, "y": 0, "width": 900, "height": 700}, "monitor": 0, "focused": False},
]


class Observation(unittest.TestCase):
    """Screenshots and window state through GNOME Shell, with gdbus
    stubbed."""

    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.stubs = Stubs(["systemctl"])
        # gdbus answers Eval calls with STUB_GDBUS_EVAL_STDOUT when set,
        # every other call with STUB_GDBUS_STDOUT.
        gdbus = self.stubs.dir / "gdbus"
        gdbus.write_text(
            STUB.replace(
                'sys.stdout.write(os.environ.get(f"STUB_{key}_STDOUT", ""))',
                'is_eval = "org.gnome.Shell.Eval" in sys.argv\n'
                'default = os.environ.get("STUB_GDBUS_STDOUT", "")\n'
                'sys.stdout.write(os.environ.get("STUB_GDBUS_EVAL_STDOUT", default) if is_eval else default)',
            )
        )
        gdbus.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def call_pi(self, args, gdbus_stdout="", env=None):
        environment = {
            "HOME": str(self.home),
            "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
            "STUB_GDBUS_STDOUT": gdbus_stdout,
        }
        environment.update(env or {})
        return run(args, self.stubs, env=environment, script=TESTBED_PI)

    def gdbus_calls(self):
        return [c[1:] for c in self.stubs.calls("gdbus")]

    def test_eval_prints_the_value(self):
        result = self.call_pi(["eval", "global.context.unsafe_mode"], eval_answer(True))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), True)
        self.assertEqual(self.gdbus_calls()[0][-1], "global.context.unsafe_mode")

    def test_eval_decodes_quotes_and_backslashes(self):
        value = {"title": "it's \"quoted\" \\ here"}
        result = self.call_pi(["eval", "x"], eval_answer(value))
        self.assertEqual(json.loads(result.stdout), value)

    def test_eval_fails_when_the_shell_refuses(self):
        result = self.call_pi(["eval", "x"], "(false, '')\n")
        self.assertEqual(result.returncode, 1)
        self.assertIn("GNOME Shell refused Eval", result.stderr)

    def test_windows_lists_the_apps_windows(self):
        result = self.call_pi(["windows"], eval_answer(WINDOWS))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), WINDOWS)
        self.assertIn("torchsnap", self.gdbus_calls()[0][-1])

    def test_launcher_state_combines_status_and_the_launcher_window(self):
        data = self.home / ".local" / "share" / "app.torchsnap"
        data.mkdir(parents=True)
        fake = FakeControlSocket(data / "control.sock", [{"result": {"visible": True}}])
        try:
            result = self.call_pi(["launcher-state"], eval_answer(WINDOWS))
        finally:
            fake.close()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"visible": True, "focused": True, "monitor": 0})

    def test_launcher_state_without_a_launcher_window(self):
        data = self.home / ".local" / "share" / "app.torchsnap"
        data.mkdir(parents=True)
        fake = FakeControlSocket(data / "control.sock", [{"result": {"visible": False}}])
        try:
            result = self.call_pi(["launcher-state"], eval_answer(WINDOWS[1:]))
        finally:
            fake.close()
        self.assertEqual(json.loads(result.stdout), {"visible": False, "focused": False, "monitor": None})

    def screenshot_call(self, *args):
        answer = f"(true, '{self.home}/.cache/torchsnap-testbed/shots/x.png')\n"
        result = self.call_pi(["screenshot", "x", *args], answer)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), f"{self.home}/.cache/torchsnap-testbed/shots/x.png")
        return self.gdbus_calls()

    def test_a_full_screenshot(self):
        calls = self.screenshot_call()
        self.assertIn("org.gnome.Shell.Screenshot.Screenshot", calls[-1])
        self.assertEqual(calls[-1][-3:], ["false", "false", f"{self.home}/.cache/torchsnap-testbed/shots/x.png"])

    def test_an_area_screenshot(self):
        calls = self.screenshot_call("--area", "10,20,300,400")
        self.assertIn("org.gnome.Shell.Screenshot.ScreenshotArea", calls[-1])
        self.assertEqual(calls[-1][-6:-1], ["10", "20", "300", "400", "false"])

    def test_a_monitor_screenshot_takes_the_monitors_rect(self):
        answer = f"(true, '{self.home}/.cache/torchsnap-testbed/shots/x.png')\n"
        result = self.call_pi(
            ["screenshot", "x", "--monitor", "1"],
            answer,
            env={"STUB_GDBUS_EVAL_STDOUT": eval_answer({"x": 1080, "y": 0, "width": 1280, "height": 800})},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.gdbus_calls()
        self.assertIn("Main.layoutManager.monitors[1]", calls[0][-1])
        self.assertIn("org.gnome.Shell.Screenshot.ScreenshotArea", calls[-1])
        self.assertEqual(calls[-1][-6:-1], ["1080", "0", "1280", "800", "false"])

    def test_a_bad_option_is_refused(self):
        result = self.call_pi(["screenshot", "x", "--area", "1,2,3"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("--area expects X,Y,WIDTH,HEIGHT", result.stderr)


class ScreenshotCopy(unittest.TestCase):
    """The Mac side copies a screenshot into target/linux-testbed/."""

    def setUp(self):
        self.stubs = Stubs(["ssh", "rsync", "scp"])

    def tearDown(self):
        self.stubs.cleanup()

    def test_the_screenshot_is_copied_next_to_the_repository(self):
        result = run(
            ["screenshot", "launcher", "--area", "0,0,10,10"],
            self.stubs,
            env={
                "TORCHSNAP_LINUX_HOST": "user@pi.local",
                "TORCHSNAP_LINUX_RUNNER": "",
                "STUB_SSH_STDOUT": "/home/u/.cache/torchsnap-testbed/shots/launcher.png\n",
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        scp = self.stubs.calls("scp")[-1]
        self.assertEqual(scp[-2], "user@pi.local:/home/u/.cache/torchsnap-testbed/shots/launcher.png")
        self.assertEqual(scp[-1], str(REPO / "target" / "linux-testbed" / "launcher.png"))
        self.assertEqual(result.stdout.strip(), str(REPO / "target" / "linux-testbed" / "launcher.png"))

# gdbus answering Eval calls by rules: STUB_EVAL_RULES is a JSON list of
# [substring, [answer, ...]]; the first rule whose substring is in the
# JavaScript answers with its next answer (the last one repeating).
# Other gdbus calls answer "(true, '')".
RULE_GDBUS = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, pathlib, sys
    log = pathlib.Path(os.environ["STUB_LOG"])
    with log.open("a") as f:
        f.write(json.dumps(sys.argv) + "\\n")
    if "org.gnome.Shell.Eval" not in sys.argv:
        print("(true, '')")
        sys.exit(0)
    js = sys.argv[-1]
    for n, (needle, answers) in enumerate(json.loads(os.environ.get("STUB_EVAL_RULES", "[]"))):
        if needle in js:
            counter = log.with_name(f"rule{n}.count")
            i = int(counter.read_text()) if counter.exists() else 0
            counter.write_text(str(i + 1))
            value = answers[min(i, len(answers) - 1)]
            text = json.dumps(value).replace("\\\\", "\\\\\\\\").replace("'", "\\\\'")
            print(f"(true, '{text}')")
            sys.exit(0)
    print("(true, 'null')")
    """
)

# ydotool that logs its argv and, for `type -f -`, what it read.
YDOTOOL = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    argv = sys.argv[1:]
    stdin = sys.stdin.read() if "-f" in argv and "-" in argv else None
    with open(os.environ["STUB_LOG"], "a") as f:
        f.write(json.dumps(["ydotool", *argv, *(["<stdin>", stdin] if stdin is not None else [])]) + "\\n")
    sys.exit(int(os.environ.get("STUB_YDOTOOL_EXIT", "0")))
    """
)

FOCUS_TERMINAL = {"title": "testbed", "wm_class": "testbed-terminal"}
FOCUS_LAUNCHER = {"title": "", "wm_class": "torchsnap"}
LAYOUTS = {"current": 0, "us": 1}


# The machine-side script marks each kind of Eval with a comment such as
# /*focus*/; the rules answer by those marks.
# The pointer answers: parked in the bottom left corner (read twice: the
# left edge reached, then its height), one pixel to the right for the
# barrier, and back.
PARKED = ([0, 1919], [0, 1919], [1, 1919], [0, 1919])


def input_rules(focus=FOCUS_LAUNCHER, modal=False, current=(1, 0), pointer=PARKED, windows=()):
    return json.dumps(
        [
            ["/*modal*/", [modal]],
            ["/*focus*/", [focus]],
            ["/*pointer*/", list(pointer)],
            ["/*current*/", list(current)],
            ["/*layouts*/", [LAYOUTS]],
            ['get_wm_class() === "torchsnap"', [list(windows)]],
        ]
    )


class Input(unittest.TestCase):
    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.runtime = self.home / "run"
        self.runtime.mkdir()
        self.stubs = Stubs(["systemctl"])
        for name, text in (("gdbus", RULE_GDBUS), ("ydotool", YDOTOOL), ("id", "#!/bin/sh\necho 1000\n")):
            path = self.stubs.dir / name
            path.write_text(text)
            path.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def call_pi(self, args, rules=None, env=None):
        environment = {
            "HOME": str(self.home),
            "TORCHSNAP_TESTBED_RUNTIME_DIR": str(self.runtime),
            "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
            "STUB_EVAL_RULES": rules or input_rules(),
            "TORCHSNAP_TESTBED_POLL_INTERVAL": "0.1",
        }
        environment.update(env or {})
        return run(args, self.stubs, env=environment, script=TESTBED_PI)

    def ydotool_calls(self):
        return [c[1:] for c in self.stubs.calls("ydotool")]

    def eval_scripts(self):
        return [c[-1] for c in self.stubs.calls("gdbus") if "org.gnome.Shell.Eval" in c]

    def test_type_needs_a_guard(self):
        result = self.call_pi(["type", "hello"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("--window REGEX or --shell-modal", result.stderr)
        self.assertEqual(self.ydotool_calls(), [])

    def test_type_refuses_when_the_wrong_window_has_focus(self):
        result = self.call_pi(["type", "--window", "torchsnap", "hello"], input_rules(focus=FOCUS_TERMINAL))
        self.assertEqual(result.returncode, 4)
        self.assertIn("focused window is testbed (testbed-terminal)", result.stderr)
        self.assertEqual(self.ydotool_calls(), [])

    def test_type_refuses_without_a_shell_modal(self):
        result = self.call_pi(["type", "--shell-modal", "hello"], input_rules(modal=False))
        self.assertEqual(result.returncode, 4)
        self.assertIn("no Shell dialog or menu holds the keyboard", result.stderr)

    def test_type_switches_to_us_types_and_switches_back(self):
        result = self.call_pi(["type", "--window", "^torchsnap$", "-yz @[]"])
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.ydotool_calls()
        self.assertEqual(calls[1][:3], ["type", "-f", "-"])
        self.assertEqual(calls[1][-1], "-yz @[]")
        self.assertEqual(calls[0], ["mousemove", "-x", "-100000", "-y", "100000"])
        self.assertEqual(calls[2:], [["mousemove", "-x", "1", "-y", "0"], ["mousemove", "-x", "-1", "-y", "0"]])
        scripts = self.eval_scripts()
        activations = [s for s in scripts if "activate(" in s]
        self.assertIn("inputSources[1].activate", activations[0])
        self.assertIn("inputSources[0].activate", activations[-1])
        # The layout goes back only after the barrier saw the pointer move.
        self.assertGreater(scripts.index(activations[-1]), max(i for i, s in enumerate(scripts) if "/*pointer*/" in s))

    def test_type_with_a_shell_modal(self):
        result = self.call_pi(["type", "--shell-modal", "abc"], input_rules(modal=True))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_the_layout_is_restored_when_typing_fails(self):
        result = self.call_pi(["type", "--window", "torchsnap", "abc"], env={"STUB_YDOTOOL_EXIT": "1"})
        self.assertNotEqual(result.returncode, 0)
        activations = [s for s in self.eval_scripts() if "activate(" in s]
        self.assertIn("inputSources[0].activate", activations[-1])

    def test_type_fails_when_the_keys_never_arrive(self):
        result = self.call_pi(
            ["type", "--window", "torchsnap", "abc"], input_rules(pointer=([0, 1919],)),
            env={"TORCHSNAP_TESTBED_INPUT_TIMEOUT": "1"},
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not arrive", result.stderr)

    def test_typing_refuses_while_an_app_window_covers_the_parking_corner(self):
        covering = {"title": "Torchsnap Settings", "wm_class": "torchsnap", "rect": {"x": 0, "y": 1000, "width": 900, "height": 920}, "monitor": 0, "focused": False}
        result = self.call_pi(["type", "--window", "torchsnap", "abc"], input_rules(windows=[covering]))
        self.assertEqual(result.returncode, 1)
        self.assertIn("covers the bottom left corner", result.stderr)
        self.assertFalse(any(c[0] == "type" for c in self.ydotool_calls()))

    def test_key_combinations_become_key_codes(self):
        result = self.call_pi(["key", "--window", "torchsnap", "ctrl+u", "Escape"])
        self.assertEqual(result.returncode, 0, result.stderr)
        keys = [c for c in self.ydotool_calls() if c[0] == "key"]
        self.assertEqual(keys, [["key", "29:1", "22:1", "22:0", "29:0", "1:1", "1:0"]])

    def test_an_unknown_key_name_is_refused(self):
        result = self.call_pi(["key", "--window", "torchsnap", "Hyper"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("unknown key: Hyper", result.stderr)

    def test_a_stale_input_lock_is_taken_over(self):
        lock = self.runtime / "torchsnap-testbed" / "input.lock"
        lock.mkdir(parents=True)
        (lock / "pid").write_text("999999\n")
        result = self.call_pi(["key", "--window", "torchsnap", "Escape"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(lock.exists())

    def test_a_held_input_lock_is_waited_for_then_refused(self):
        lock = self.runtime / "torchsnap-testbed" / "input.lock"
        lock.mkdir(parents=True)
        (lock / "pid").write_text(f"{os.getpid()}\n")
        result = self.call_pi(
            ["key", "--window", "torchsnap", "Escape"], env={"TORCHSNAP_TESTBED_LOCK_TIMEOUT": "1"}
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("input is in use", result.stderr)
        self.assertEqual(self.ydotool_calls(), [])

    def test_pointer_homes_then_moves(self):
        rules = json.dumps([["/*pointer*/", [[0, 1919], [0, 1919], [300, 1919], [300, 420]]]])
        result = self.call_pi(["pointer", "300", "420"], rules)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.ydotool_calls(),
            [
                ["mousemove", "-x", "-100000", "-y", "100000"],
                ["mousemove", "-x", "300", "-y", "0"],
                ["mousemove", "-x", "0", "-y", "-1499"],
            ],
        )

    def test_pointer_fails_when_it_does_not_land(self):
        rules = json.dumps([["/*pointer*/", [[0, 1919], [0, 1919], [290, 1919], [290, 420]]]])
        result = self.call_pi(["pointer", "300", "420"], rules, env={"TORCHSNAP_TESTBED_INPUT_TIMEOUT": "1"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("pointer at [290, 420]", result.stderr)

    def test_click_presses_the_button_where_the_pointer_landed(self):
        rules = json.dumps([["/*pointer*/", [[0, 1919], [0, 1919], [10, 1919], [10, 20]]]])
        result = self.call_pi(["click", "10", "20", "right"], rules)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.ydotool_calls()[-1], ["click", "0xC1"])


TRAY_ITEMS = [
    {"key": "appindicator-:1.9@/org/ayatana/NotificationItem/other", "title": "nm-applet", "x": 700, "y": 0, "width": 48, "height": 35, "menuOpen": False},
    {"key": "appindicator-:1.113@/org/ayatana/NotificationItem/tray_icon_tray_app_4892_1", "title": "torchsnap", "x": 785, "y": 0, "width": 48, "height": 35, "menuOpen": False},
]


class Tray(unittest.TestCase):
    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.stubs = Stubs(["systemctl"])
        for name, text in (("gdbus", RULE_GDBUS), ("ydotool", YDOTOOL)):
            path = self.stubs.dir / name
            path.write_text(text)
            path.chmod(0o755)
        # gnome-extensions reports the state STUB_EXTENSION_STATE.
        extensions = self.stubs.dir / "gnome-extensions"
        extensions.write_text(
            STUB.replace(
                'sys.stdout.write(os.environ.get(f"STUB_{key}_STDOUT", ""))',
                'sys.stdout.write("  State: " + os.environ.get("STUB_EXTENSION_STATE", "ACTIVE") + "\\n" if sys.argv[1] == "info" else "")',
            )
        )
        extensions.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def call_pi(self, args, rules="[]", env=None):
        environment = {
            "HOME": str(self.home),
            "TORCHSNAP_TESTBED_RUNTIME_DIR": str(self.home),
            "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
            "STUB_EVAL_RULES": rules,
            "TORCHSNAP_TESTBED_POLL_INTERVAL": "0.1",
            "TORCHSNAP_TESTBED_INPUT_TIMEOUT": "1",
        }
        environment.update(env or {})
        return run(args, self.stubs, env=environment, script=TESTBED_PI)

    def test_tray_on_enables_the_extension(self):
        result = self.call_pi(["tray", "on"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.stubs.calls("gnome-extensions")[0][1:], ["enable", "ubuntu-appindicators@ubuntu.com"])

    def test_tray_off_waits_for_the_extension_to_stop(self):
        result = self.call_pi(["tray", "off"], env={"STUB_EXTENSION_STATE": "ACTIVE"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not become INACTIVE", result.stderr)

    def test_tray_status(self):
        result = self.call_pi(["tray", "status"], env={"STUB_EXTENSION_STATE": "INACTIVE"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "off")

    def test_tray_icon_finds_torchsnaps_item(self):
        result = self.call_pi(["tray-icon"], json.dumps([["/*tray*/", [TRAY_ITEMS]]]))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout), {"x": 785, "y": 0, "width": 48, "height": 35, "menuOpen": False}
        )

    def test_no_tray_icon(self):
        result = self.call_pi(["tray-icon"], json.dumps([["/*tray*/", [TRAY_ITEMS[:1]]]]))
        self.assertEqual(result.returncode, 1)
        self.assertIn("no Torchsnap tray icon", result.stderr)

    def test_tray_click_clicks_the_icons_centre(self):
        rules = json.dumps(
            [["/*tray*/", [TRAY_ITEMS]], ["/*pointer*/", [[0, 1919], [0, 1919], [809, 1919], [809, 17]]]]
        )
        result = self.call_pi(["tray-click"], rules)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [c[1:] for c in self.stubs.calls("ydotool")],
            [
                ["mousemove", "-x", "-100000", "-y", "100000"],
                ["mousemove", "-x", "809", "-y", "0"],
                ["mousemove", "-x", "0", "-y", "-1902"],
                ["click", "0xC0"],
            ],
        )


# systemctl that answers `is-active` with STUB_ACTIVE_EXIT (default 3:
# inactive) and everything else with the user manager's environment.
SYSTEMCTL = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ["STUB_LOG"], "a") as f:
        f.write(json.dumps(sys.argv) + "\\n")
    if "is-active" in sys.argv:
        units = os.environ.get("STUB_ACTIVE_UNITS")
        if units is not None:
            sys.exit(0 if sys.argv[-1] in units.split(",") else 3)
        sys.exit(int(os.environ.get("STUB_ACTIVE_EXIT", "3")))
    sys.stdout.write(os.environ.get("STUB_SYSTEMCTL_STDOUT", ""))
    """
)

# systemd-run that, like gnome-record, writes "started FILE" to the unit's
# log when STUB_RECORD_STARTS is set; FILE is the last argument + ".mp4".
SYSTEMD_RUN = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ["STUB_LOG"], "a") as f:
        f.write(json.dumps(sys.argv) + "\\n")
    log = next(a.split("append:", 1)[1] for a in sys.argv if a.startswith("StandardOutput=append:"))
    if os.environ.get("STUB_RECORD_STARTS"):
        with open(log, "a") as f:
            f.write("started " + sys.argv[-1] + ".mp4\\n")
    """
)


class Recording(unittest.TestCase):
    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.stubs = Stubs([])
        for name, text in (("systemctl", SYSTEMCTL), ("systemd-run", SYSTEMD_RUN)):
            path = self.stubs.dir / name
            path.write_text(text)
            path.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def call_pi(self, args, env=None):
        environment = {
            "HOME": str(self.home),
            "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
            "TORCHSNAP_TESTBED_POLL_INTERVAL": "0.1",
            "TORCHSNAP_TESTBED_RECORD_TIMEOUT": "1",
        }
        environment.update(env or {})
        return run(args, self.stubs, env=environment, script=TESTBED_PI)

    def test_record_start_runs_gnome_record_as_a_unit(self):
        result = self.call_pi(["record-start", "opening"], env={"STUB_RECORD_STARTS": "1"})
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = f"{self.home}/.cache/torchsnap-testbed/recordings/opening"
        self.assertEqual(result.stdout.strip(), expected + ".mp4")
        run_call = self.stubs.calls("systemd-run")[0]
        self.assertIn("torchsnap-testbed-record", run_call)
        self.assertTrue(run_call[-2].endswith("gnome-record"))
        self.assertEqual(run_call[-1], expected)

    def test_record_start_refuses_while_recording(self):
        result = self.call_pi(["record-start", "x"], env={"STUB_ACTIVE_EXIT": "0"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("already recording", result.stderr)
        self.assertEqual(self.stubs.calls("systemd-run"), [])

    def test_record_start_fails_when_the_recording_does_not_start(self):
        result = self.call_pi(["record-start", "x"])
        self.assertEqual(result.returncode, 1)
        self.assertIn("did not start", result.stderr)

    def test_record_stop_without_a_recording(self):
        result = self.call_pi(["record-stop"])
        self.assertEqual(result.returncode, 1)
        self.assertIn("no recording", result.stderr)


# systemd-run for headless sessions: for the shell's unit it writes a bus
# address into the bus file, as the session's inner script does.
HEADLESS_SYSTEMD_RUN = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ["STUB_LOG"], "a") as f:
        f.write(json.dumps(sys.argv) + "\\n")
    if "torchsnap-testbed-headless" in sys.argv:
        path = os.path.join(os.environ["HOME"], ".cache/torchsnap-testbed/headless/bus")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("unix:path=/run/user/1000/headless-bus\\n")
    """
)


class Headless(unittest.TestCase):
    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.stubs = Stubs([])
        # gdctl that also logs which bus it would talk to.
        gdctl = (
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "with open(os.environ['STUB_LOG'], 'a') as f:\n"
            "    f.write(json.dumps(sys.argv + ['bus=' + os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')]) + '\\n')\n"
        )
        for name, text in (
            ("systemctl", SYSTEMCTL), ("systemd-run", HEADLESS_SYSTEMD_RUN), ("gdbus", RULE_GDBUS), ("gdctl", gdctl),
        ):
            path = self.stubs.dir / name
            path.write_text(text)
            path.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def call_pi(self, args, env=None):
        environment = {
            "HOME": str(self.home),
            "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
            "TORCHSNAP_TESTBED_POLL_INTERVAL": "0.1",
            "TORCHSNAP_TESTBED_HEADLESS_TIMEOUT": "2",
            "TORCHSNAP_TESTBED_APP_TIMEOUT": "1",
            "STUB_EVAL_RULES": json.dumps([["/*scales*/", [[1, 2]]], ["/*ready*/", [True]]]),
        }
        environment.update(env or {})
        return run(args, self.stubs, env=environment, script=TESTBED_PI)

    def test_headless_start_runs_gnome_shell_with_the_monitors_and_scales(self):
        result = self.call_pi(["headless-start", "1280x800@1", "2560x1600@2"])
        self.assertEqual(result.returncode, 0, result.stderr)
        run_call = " ".join(self.stubs.calls("systemd-run")[0])
        self.assertIn("torchsnap-testbed-headless", run_call)
        self.assertIn("env -u WAYLAND_DISPLAY -u DISPLAY -u XAUTHORITY", run_call)
        self.assertIn("--virtual-monitor 1280x800 --virtual-monitor 2560x1600", run_call)
        self.assertIn("--no-x11", run_call)
        self.assertEqual(
            self.stubs.calls("gdctl")[0][1:],
            ["set", "--logical-monitor", "--primary", "--scale", "1", "--monitor", "Meta-0",
             "--logical-monitor", "--scale", "2", "--right-of", "Meta-0", "--monitor", "Meta-1",
             # gdctl talks to the headless session's bus.
             "bus=unix:path=/run/user/1000/headless-bus"],
        )

    def test_headless_start_defaults_to_two_monitors_at_scale_1_and_2(self):
        result = self.call_pi(["headless-start"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--virtual-monitor 1280x800 --virtual-monitor 2560x1600", " ".join(self.stubs.calls("systemd-run")[0]))

    def test_a_malformed_monitor_spec_is_refused(self):
        result = self.call_pi(["headless-start", "1280x800"])
        self.assertEqual(result.returncode, 2)
        self.assertIn("WIDTHxHEIGHT@SCALE", result.stderr)
        self.assertEqual(self.stubs.calls("systemd-run"), [])

    def test_headless_start_refuses_while_a_session_runs(self):
        result = self.call_pi(["headless-start"], env={"STUB_ACTIVE_UNITS": "torchsnap-testbed-headless"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("already running", result.stderr)

    def test_headless_commands_need_a_session(self):
        result = self.call_pi(["control", "--headless", "status"], env={"STUB_ACTIVE_UNITS": ""})
        self.assertEqual(result.returncode, 1)
        self.assertIn("no headless session", result.stderr)

    def test_the_headless_app_gets_the_headless_session_and_its_own_data(self):
        bus = self.home / ".cache" / "torchsnap-testbed" / "headless" / "bus"
        bus.parent.mkdir(parents=True)
        bus.write_text("unix:path=/run/user/1000/headless-bus\n")
        binary = self.home / "bin" / "torchsnap"
        binary.parent.mkdir()
        binary.write_text("#!/bin/sh\n")
        binary.chmod(0o755)
        result = self.call_pi(
            ["app-start", "--headless"],
            env={"STUB_ACTIVE_UNITS": "torchsnap-testbed-headless", "TORCHSNAP_TESTBED_APP_BINARY": str(binary)},
        )
        run_call = self.stubs.calls("systemd-run")[0]
        self.assertIn("torchsnap-testbed-app-headless", run_call)
        joined = " ".join(run_call)
        self.assertIn("env -u DISPLAY -u XAUTHORITY", joined)
        self.assertIn("WAYLAND_DISPLAY=torchsnap-headless", joined)
        self.assertIn("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/headless-bus", joined)
        data = self.home / ".cache" / "torchsnap-testbed" / "headless" / "app" / "data"
        self.assertIn(f"XDG_DATA_HOME={data}", joined)
        settings = json.loads((data / "app.torchsnap" / "settings.json").read_text())
        self.assertIs(settings["controlChannel.enabled"], True)
        # No app answers in the test, so the start times out.
        self.assertEqual(result.returncode, 1)

    def test_headless_stop_stops_both_units_and_forgets_the_bus(self):
        bus = self.home / ".cache" / "torchsnap-testbed" / "headless" / "bus"
        bus.parent.mkdir(parents=True)
        bus.write_text("x\n")
        result = self.call_pi(
            ["headless-stop"],
            env={"STUB_ACTIVE_UNITS": "torchsnap-testbed-headless,torchsnap-testbed-app-headless"},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        stops = [c[-1] for c in self.stubs.calls("systemctl") if "stop" in c]
        self.assertEqual(stops, ["torchsnap-testbed-app-headless", "torchsnap-testbed-headless"])
        self.assertFalse(bus.exists())


class StopAll(unittest.TestCase):
    def setUp(self):
        self.home = pathlib.Path(tempfile.mkdtemp(prefix="tb-", dir="/tmp"))
        self.stubs = Stubs([])
        path = self.stubs.dir / "systemctl"
        path.write_text(SYSTEMCTL)
        path.chmod(0o755)
        extensions = self.stubs.dir / "gnome-extensions"
        extensions.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "with open(os.environ['STUB_LOG'], 'a') as f:\n"
            "    f.write(json.dumps(sys.argv) + '\\n')\n"
            "if sys.argv[1] == 'info':\n"
            "    print('  State: ' + os.environ.get('STUB_EXTENSION_STATE', 'ACTIVE'))\n"
        )
        extensions.chmod(0o755)

    def tearDown(self):
        self.stubs.cleanup()
        shutil.rmtree(self.home)

    def call_pi(self, env=None):
        environment = {
            "HOME": str(self.home),
            "STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT,
            "TORCHSNAP_TESTBED_POLL_INTERVAL": "0.1",
            "STUB_ACTIVE_UNITS": "torchsnap-testbed-app,torchsnap-testbed-record",
        }
        environment.update(env or {})
        return run(["stop-all"], self.stubs, env=environment, script=TESTBED_PI)

    def test_stop_all_stops_every_testbed_unit(self):
        result = self.call_pi()
        self.assertEqual(result.returncode, 0, result.stderr)
        stops = [c[-1] for c in self.stubs.calls("systemctl") if "stop" in c]
        self.assertEqual(stops, ["torchsnap-testbed-record", "torchsnap-testbed-app"])

    def test_stop_all_puts_the_recorded_tray_state_back(self):
        state = self.home / ".cache" / "torchsnap-testbed" / "selftest-initial"
        state.parent.mkdir(parents=True)
        state.write_text("tray=off\nmonitors=abc\n")
        self.call_pi(env={"STUB_EXTENSION_STATE": "INACTIVE"})
        disables = [c for c in self.stubs.calls("gnome-extensions") if c[1] == "disable"]
        self.assertEqual(len(disables), 1)

    def test_stop_all_leaves_the_tray_alone_without_a_record(self):
        result = self.call_pi()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [c for c in self.stubs.calls("gnome-extensions") if c[1] in ("enable", "disable")], []
        )


class SelftestRepeat(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["ssh", "rsync", "scp"])

    def tearDown(self):
        self.stubs.cleanup()

    def test_repeat_counts_the_passes_per_item(self):
        result = run(
            ["--repeat", "2", "1"], self.stubs,
            env={"TORCHSNAP_LINUX_HOST": "user@pi.local"}, script=SELFTEST,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("item 1: passed 0/2", result.stdout)

    def test_the_initial_state_is_recorded_before_the_first_item(self):
        run(["1"], self.stubs, env={"TORCHSNAP_LINUX_HOST": "user@pi.local"}, script=SELFTEST)
        first_remote = " ".join(self.stubs.calls("ssh")[0][2:])
        self.assertIn("selftest-initial", first_remote)


class Clipboard(unittest.TestCase):
    def setUp(self):
        self.stubs = Stubs(["systemctl", "wl-paste"])

    def tearDown(self):
        self.stubs.cleanup()

    def wl_copy(self, body):
        path = self.stubs.dir / "wl-copy"
        path.write_text("#!/usr/bin/env bash\n" + body)
        path.chmod(0o755)

    def call_pi(self, args, env=None):
        environment = {"STUB_SYSTEMCTL_STDOUT": SHOW_ENVIRONMENT}
        environment.update(env or {})
        return run(args, self.stubs, env=environment, script=TESTBED_PI)

    def test_set_returns_although_wl_copy_leaves_a_server_behind(self):
        # wl-copy forks a process that serves the clipboard and keeps the
        # standard output it inherited.
        self.wl_copy('cat >"$STUB_LOG.copied"\n(sleep 30) &\nexit 0\n')
        result = self.call_pi(["clipboard-set", "a b $HOME `x`"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(pathlib.Path(str(self.stubs.log) + ".copied").read_text(), "a b $HOME `x`")

    def test_set_gives_up_on_a_wl_copy_that_never_returns(self):
        self.wl_copy("sleep 30\n")
        result = self.call_pi(["clipboard-set", "x"], env={"TORCHSNAP_TESTBED_CLIPBOARD_TIMEOUT": "1"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("wl-copy did not return", result.stderr)

    def test_get_prints_the_clipboard(self):
        result = self.call_pi(["clipboard-get"], env={"STUB_WL_PASTE_STDOUT": "a b"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "a b")
        self.assertEqual(self.stubs.calls("wl-paste")[0][1:], ["--no-newline"])


if __name__ == "__main__":
    unittest.main()
