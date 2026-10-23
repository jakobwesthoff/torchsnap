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
SELFTEST = REPO / "tools" / "linux-testbed" / "selftest"
PROVISION_DIR = REPO / "tools" / "linux-testbed" / "provision"
TESTBED_PI = REPO / "tools" / "linux-testbed" / "pi" / "testbed-pi"

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
        result = with_steps(
            "state=0; check() { [ $state = 1 ]; }; apply() { state=1; }; "
            "step demo check apply; steps_summary"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("changed demo", result.stdout)
        self.assertIn("provision: 1 changed, 0 ok, 0 deferred", result.stdout)

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
        path = self.dir / "jakob"
        if content is not None:
            path.write_text(content)
        result = with_gnome_steps(f"set_accountsservice_session '{path}'", self.stubs)
        self.assertEqual(result.returncode, 0, result.stderr)
        return path.read_text()

    def test_session_in_a_missing_file(self):
        self.assertEqual(self.set_session(None), "[User]\nSession=gnome\n")

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


if __name__ == "__main__":
    unittest.main()
