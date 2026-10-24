# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for the Linux container build in tools/linux-build/. The scripts
# drive Docker, so every external command they call is replaced by a
# stub on PATH that records its arguments and answers from environment
# variables; nothing here needs Docker or the network.
#
# Each case runs the scripts from a copy in a temporary tree, so the
# repository's own `target/` is never touched. The Mac-side script runs
# under `/bin/bash`, the bash 3.2 that macOS ships, because that is the
# shell it meets on a Mac and on CI's macOS runner.

import hashlib
import json
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import tempfile
import time
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
TOOL_DIR = REPO / "tools" / "linux-build"
HAVE_JUST = shutil.which("just") is not None


def pinned_channel():
    """The Rust release `rust-toolchain.toml` pins, which the image is built with."""
    for line in (REPO / "rust-toolchain.toml").read_text().splitlines():
        if line.startswith("channel"):
            return line.split("=", 1)[1].strip().strip('"')
    raise AssertionError("rust-toolchain.toml has no channel")

# A stub logs one line per call: its name and arguments, separated by
# the ASCII unit separator, which no argument here contains.
SEP = "\x1f"

# Stubs answer from `STUB_<NAME>_<ARG1>_…_<ARG5>_OUT`/`_RC`, trying the
# longest prefix of the arguments first and ending with `STUB_<NAME>_…`;
# names and arguments are upper-cased and reduced to letters, digits
# and `_` (`stub_key` builds the same names). So one `docker` stub can
# fail `docker build`, answer `docker run`, and report one volume as
# present and another as missing.
STUB = """#!/bin/bash
name=$(basename "$0")
{ printf '%s' "$name"; for a in "$@"; do printf '\\037%s' "$a"; done; printf '\\n'; } >> "$STUB_LOG"
if [ -n "${STUB_CWD_LOG:-}" ]; then printf '%s %s\\n' "$name" "$PWD" >> "$STUB_CWD_LOG"; fi
norm() { printf '%s' "$1" | tr 'a-z-' 'A-Z_' | tr -cd 'A-Z0-9_'; }
key="STUB_$(norm "$name")"
prefixes=("$key")
n=0
for a in "$@"; do
    n=$((n + 1))
    [ "$n" -gt 5 ] && break
    key="${key}_$(norm "$a")"
    prefixes=("$key" "${prefixes[@]}")
done
for prefix in "${prefixes[@]}"; do
    out="${prefix}_OUT"
    rc="${prefix}_RC"
    if [ -n "${!out+x}" ] || [ -n "${!rc+x}" ]; then
        [ -n "${!out+x}" ] && printf '%s\\n' "${!out}"
        exit "${!rc:-0}"
    fi
done
exit 0
"""


def stub_key(*argv):
    """The environment variable prefix a stub reads for this call."""
    def norm(value):
        value = value.upper().replace("-", "_")
        return "".join(c for c in value if c.isalnum() or c == "_")
    return "STUB_" + "_".join(norm(a) for a in argv[:6])


STUBBED = ["docker", "git", "rsync", "just", "objdump", "dpkg-query"]


class Tree:
    """A temporary copy of what the scripts read from the repository,
    with stubs on PATH and a log of every stubbed call."""

    def __init__(self, stubs=STUBBED):
        self._tmp = tempfile.TemporaryDirectory()
        # The scripts work with physical paths (`pwd -P`); macOS keeps
        # temporary directories behind the /var -> /private/var link.
        self.root = pathlib.Path(os.path.realpath(self._tmp.name)) / "repo"
        shutil.copytree(TOOL_DIR, self.root / "tools" / "linux-build")
        (self.root / "src-tauri" / "src").mkdir(parents=True)
        shutil.copy(REPO / "rust-toolchain.toml", self.root)
        shutil.copy(REPO / "src-tauri" / "Cargo.toml", self.root / "src-tauri")
        shutil.copy(REPO / "src-tauri" / "tauri.conf.json", self.root / "src-tauri")
        shutil.copy(REPO / "src-tauri" / "src" / "welcome.rs", self.root / "src-tauri" / "src")
        self.bin = pathlib.Path(self._tmp.name) / "stubs"
        self.bin.mkdir()
        for name in stubs:
            path = self.bin / name
            path.write_text(STUB)
            path.chmod(0o755)
        self.log = pathlib.Path(self._tmp.name) / "calls.log"
        self.cwd_log = pathlib.Path(self._tmp.name) / "cwd.log"
        self.env = {
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "HOME": self._tmp.name,
            "STUB_LOG": str(self.log),
            "STUB_CWD_LOG": str(self.cwd_log),
            # By default Docker holds none of the Linux build's objects: no image, no
            # volume, no container. Cases add what they need.
            "STUB_DOCKER_CONTEXT_SHOW_OUT": "testctx",
            "STUB_DOCKER_VOLUME_INSPECT_RC": "1",
            "STUB_DOCKER_IMAGE_INSPECT_RC": "1",
        }

    def close(self):
        self._tmp.cleanup()

    def run(self, script, *args, env=None, bash="/bin/bash"):
        merged = dict(self.env)
        merged.update(env or {})
        return subprocess.run(
            [bash, str(self.root / "tools" / "linux-build" / script), *args],
            env=merged,
            capture_output=True,
            text=True,
            timeout=60,
        )

    def calls(self, name=None):
        if not self.log.exists():
            return []
        rows = [line.split(SEP) for line in self.log.read_text().splitlines()]
        return [row for row in rows if name is None or row[0] == name]

    def docker(self, subcommand):
        return [row[1:] for row in self.calls("docker") if len(row) > 1 and row[1] == subcommand]

    def cwds(self, name):
        if not self.cwd_log.exists():
            return []
        return [line.split(" ", 1)[1] for line in self.cwd_log.read_text().splitlines()
                if line.split(" ", 1)[0] == name]


class TreeCase(unittest.TestCase):
    def setUp(self):
        self.tree = Tree()
        self.addCleanup(self.tree.close)


def just_dry_run(*args, cwd=REPO):
    """The command lines `just --dry-run` would run, which it writes to
    stderr. `cwd` is the directory just is invoked from."""
    return subprocess.run(
        ["just", "--justfile", str(REPO / "Justfile"), "--dry-run", *args],
        capture_output=True, text=True, timeout=60, cwd=cwd,
    )


# =========================================================================
# Recipes
# =========================================================================

@unittest.skipUnless(HAVE_JUST, "just is not installed")
class Recipes(unittest.TestCase):
    def assert_runs(self, args, expected, cwd=REPO):
        result = just_dry_run(*args, cwd=cwd)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(expected, [line.strip() for line in result.stderr.splitlines()])

    def test_image(self):
        self.assert_runs(["linux-build-image"], "tools/linux-build/linux-build image")

    def test_lint_defaults_to_the_repository(self):
        self.assert_runs(["linux-build-lint"], f"tools/linux-build/linux-build lint --path '{REPO}'")

    def test_lint_takes_a_path(self):
        self.assert_runs(["linux-build-lint", "/elsewhere/checkout"],
                         "tools/linux-build/linux-build lint --path '/elsewhere/checkout'")

    def test_lint_resolves_a_relative_path_against_the_invocation_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            elsewhere = os.path.realpath(tmp)
            self.assert_runs(["linux-build-lint", "sub"],
                             f"tools/linux-build/linux-build lint --path '{elsewhere}/sub'", cwd=elsewhere)

    def test_status(self):
        self.assert_runs(["linux-build-status"], "tools/linux-build/linux-build status")

    def test_build_debug(self):
        self.assert_runs(["linux-build"], "tools/linux-build/linux-build build")

    def test_build_release(self):
        self.assert_runs(["linux-build", "--release"], "tools/linux-build/linux-build build --release")

    def test_build_takes_the_profile_only_as_a_flag(self):
        # `profile` is a `--release` flag, so a misspelt profile is read as
        # a recipe name and nothing runs.
        result = just_dry_run("linux-build", "relase")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("tools/linux-build/linux-build", result.stderr)

    def test_clean(self):
        self.assert_runs(["linux-build-clean"], "tools/linux-build/linux-build clean")

    def test_clean_all(self):
        self.assert_runs(["linux-build-clean", "--all"], "tools/linux-build/linux-build clean --all")


# =========================================================================
# linux-build: arguments and the image
# =========================================================================

class Arguments(TreeCase):
    def test_unknown_command_prints_usage_and_exits_2(self):
        result = self.tree.run("linux-build", "bogus")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: linux-build", result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])

    def test_no_command_prints_usage_and_exits_2(self):
        result = self.tree.run("linux-build")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: linux-build", result.stderr)

    def test_unknown_option_prints_usage_and_exits_2(self):
        result = self.tree.run("linux-build", "clean", "--bogus")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: linux-build", result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])

    def test_build_takes_only_release(self):
        result = self.tree.run("linux-build", "build", "--bogus")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: linux-build", result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])


DANGLING = ["image", "ls", "--no-trunc", "--filter", "dangling=true",
            "--filter", "label=org.torchsnap.linux-build=1", "--format", "{{.ID}} {{.Size}}"]
IMAGE_ID = stub_key("docker", "image", "inspect", "torchsnap-linux-build:latest", "--format", "{{.Id}}")


class Image(TreeCase):
    def builds(self):
        return [row for row in self.tree.docker("buildx") if row[1] == "build"]

    def image_removals(self):
        return [row[1:] for row in self.tree.calls("docker") if row[1:3] == ["image", "rm"]]

    def test_builds_the_arm64_image_with_the_pinned_toolchain(self):
        result = self.tree.run("linux-build", "image")
        self.assertEqual(result.returncode, 0, result.stderr)
        tools = str(self.tree.root / "tools" / "linux-build")
        self.assertEqual(self.builds(), [[
            "buildx", "build", "--builder", "testctx", "--platform", "linux/arm64", "--load",
            "--build-arg", f"RUST_TOOLCHAIN={pinned_channel()}",
            "-t", "torchsnap-linux-build:latest",
            "-f", f"{tools}/Dockerfile", tools,
        ]])

    def test_removes_exactly_the_image_it_replaced(self):
        # Another dangling image with the label, built FROM this one by
        # someone else, inherits the label; it stays.
        result = self.tree.run("linux-build", "image", env={
            IMAGE_ID + "_OUT": "sha256:old1",
            "STUB_DOCKER_IMAGE_LS_OUT": "sha256:other 1GB\nsha256:old1 2GB",
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(DANGLING, [row[1:] for row in self.tree.calls("docker")])
        self.assertEqual(self.image_removals(), [["image", "rm", "sha256:old1"]])
        inspect = next(i for i, row in enumerate(self.tree.calls("docker"))
                       if row[1:3] == ["image", "inspect"] and "{{.Id}}" in row)
        build = next(i for i, row in enumerate(self.tree.calls("docker")) if row[1:3] == ["buildx", "build"])
        self.assertLess(inspect, build)

    def test_an_unchanged_image_is_not_removed(self):
        # The rebuild produced the same image, so it is still tagged and
        # not in the dangling list.
        result = self.tree.run("linux-build", "image", env={
            IMAGE_ID + "_OUT": "sha256:same",
            "STUB_DOCKER_IMAGE_LS_OUT": "sha256:other 1GB",
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.image_removals(), [])

    def test_a_first_build_removes_nothing(self):
        result = self.tree.run("linux-build", "image", env={"STUB_DOCKER_IMAGE_LS_OUT": "sha256:other 1GB"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.image_removals(), [])

    def test_keeps_the_predecessor_while_a_container_of_this_tool_runs(self):
        result = self.tree.run("linux-build", "image", env={
            IMAGE_ID + "_OUT": "sha256:old1",
            "STUB_DOCKER_IMAGE_LS_OUT": "sha256:old1 2GB",
            "STUB_DOCKER_PS_OUT": "c0ffee",
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.image_removals(), [])

    def test_keeps_everything_after_a_failed_build(self):
        result = self.tree.run("linux-build", "image", env={
            "STUB_DOCKER_BUILDX_BUILD_RC": "1",
            IMAGE_ID + "_OUT": "sha256:old1",
            "STUB_DOCKER_IMAGE_LS_OUT": "sha256:old1 2GB",
        })
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.image_removals(), [])

    def test_an_image_still_in_use_is_kept_and_reported(self):
        result = self.tree.run("linux-build", "image", env={
            IMAGE_ID + "_OUT": "sha256:old1",
            "STUB_DOCKER_IMAGE_LS_OUT": "sha256:old1 2GB",
            "STUB_DOCKER_IMAGE_RM_RC": "1",
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("sha256:old1 in use, kept", result.stderr)

    def test_a_toolchain_file_without_a_channel_stops_before_docker(self):
        (self.tree.root / "rust-toolchain.toml").write_text('[toolchain]\ntargets = ["wasm32-wasip2"]\n')
        result = self.tree.run("linux-build", "image")
        self.assertEqual(result.returncode, 1)
        self.assertIn("rust-toolchain.toml", result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])


# =========================================================================
# linux-build lint
# =========================================================================

def lint_volume(path):
    """The work volume `linux-build lint` uses for a checkout: one per
    checkout, named after the hash of its physical path."""
    digest = hashlib.sha256(os.path.realpath(path).encode()).hexdigest()[:8]
    return f"torchsnap-linux-build-lint-{digest}"


def contains_run(argv, expected):
    return expected in [argv[i:i + len(expected)] for i in range(len(argv))]


class Lint(TreeCase):
    def setUp(self):
        super().setUp()
        # Another checkout, deliberately without tools/linux-build/: the
        # scripts always come from the repository that runs linux-build.
        self.checkout = pathlib.Path(self.tree._tmp.name) / "other-checkout"
        (self.checkout / "src-tauri").mkdir(parents=True)
        (self.checkout / "src-tauri" / "Cargo.toml").write_text("[package]\n")
        shutil.copy(REPO / "rust-toolchain.toml", self.checkout)

    def lint_run(self):
        runs = [run for run in self.tree.docker("run") if run[1:3] == ["--name", "torchsnap-linux-build-lint"]]
        self.assertEqual(len(runs), 1)
        return runs[0]

    def test_runs_the_lint_container_on_the_given_checkout(self):
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len([row for row in self.tree.docker("buildx") if row[1] == "build"]), 1)
        run = self.lint_run()
        checkout = os.path.realpath(self.checkout)
        tools = self.tree.root / "tools" / "linux-build"
        for expected in (
            ["--name", "torchsnap-linux-build-lint"],
            ["--platform", "linux/arm64"],
            ["--mount", f"type=bind,src={checkout},dst=/src,readonly"],
            ["--mount", f"type=bind,src={tools},dst=/linux-build,readonly"],
            ["--mount", f"type=volume,src={lint_volume(self.checkout)},dst=/work"],
            ["--mount", "type=volume,src=torchsnap-linux-build-cargo,dst=/usr/local/cargo/registry"],
            ["--mount", "type=volume,src=torchsnap-linux-build-bun,dst=/var/cache/bun"],
            ["-e", "LINUX_BUILD_WORK=/work"],
        ):
            self.assertTrue(contains_run(run, expected), f"{expected} not in {run}")
        self.assertIn("--rm", run)
        self.assertIn("--init", run)
        self.assertEqual(run[-3:], ["torchsnap-linux-build:latest", "/linux-build/in-container", "lint"])
        self.assertFalse(any(arg.startswith("GIT_DIR=") for arg in run))

    def test_each_checkout_gets_its_own_work_volume(self):
        second = pathlib.Path(self.tree._tmp.name) / "second-checkout"
        shutil.copytree(self.checkout, second)
        for path in (self.checkout, second, self.checkout):
            result = self.tree.run("linux-build", "lint", "--path", str(path))
            self.assertEqual(result.returncode, 0, result.stderr)
        volumes = [arg for run in self.tree.docker("run") for arg in run
                   if run[1:3] == ["--name", "torchsnap-linux-build-lint"]
                   and arg.startswith("type=volume,src=torchsnap-linux-build-lint-")]
        self.assertEqual(len(volumes), 3)
        self.assertNotEqual(volumes[0], volumes[1])
        self.assertEqual(volumes[0], volumes[2])

    def test_lints_the_repository_without_a_path(self):
        result = self.tree.run("linux-build", "lint")
        self.assertEqual(result.returncode, 0, result.stderr)
        run = self.lint_run()
        self.assertIn(f"type=bind,src={os.path.realpath(self.tree.root)},dst=/src,readonly", run)

    def test_a_checkout_pinning_another_toolchain_exits_2_before_docker(self):
        (self.checkout / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.90.0"\n')
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout))
        self.assertEqual(result.returncode, 2)
        self.assertIn("1.90.0", result.stderr)
        self.assertIn(pinned_channel(), result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])

    def test_a_path_without_the_host_crate_exits_2_before_docker(self):
        result = self.tree.run("linux-build", "lint", "--path", str(self.tree._tmp.name))
        self.assertEqual(result.returncode, 2)
        self.assertIn("src-tauri/Cargo.toml", result.stderr)
        self.assertEqual(self.tree.docker("run"), [])

    def test_a_path_that_does_not_exist_exits_2_before_docker(self):
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout / "missing"))
        self.assertEqual(result.returncode, 2)
        self.assertIn("missing", result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])

    def test_path_without_a_value_prints_usage_and_exits_2(self):
        result = self.tree.run("linux-build", "lint", "--path")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: linux-build", result.stderr)
        self.assertEqual(self.tree.calls("docker"), [])

    def test_a_failed_image_build_stops_before_the_container(self):
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout),
                               env={"STUB_DOCKER_BUILDX_BUILD_RC": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len([row for row in self.tree.docker("buildx") if row[1] == "build"]), 1)
        self.assertEqual(self.tree.docker("run"), [])

    def test_creates_its_volumes_with_labels_before_the_run(self):
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout))
        self.assertEqual(result.returncode, 0, result.stderr)
        creates = [row for row in self.tree.docker("volume") if row[1] == "create"]
        checkout = os.path.realpath(self.checkout)
        self.assertIn(["volume", "create", "--label", "org.torchsnap.linux-build=1",
                       "--label", f"org.torchsnap.linux-build.checkout={checkout}",
                       lint_volume(self.checkout)], creates)
        for name in ("torchsnap-linux-build-cargo", "torchsnap-linux-build-bun"):
            self.assertIn(["volume", "create", "--label", "org.torchsnap.linux-build=1", name], creates)
        calls = self.tree.calls("docker")
        last_create = max(i for i, row in enumerate(calls) if row[1:3] == ["volume", "create"])
        main_run = next(i for i, row in enumerate(calls) if row[1:4] == ["run", "--name", "torchsnap-linux-build-lint"])
        self.assertLess(last_create, main_run)

    def test_existing_volumes_are_not_created_again(self):
        exists = {stub_key("docker", "volume", "inspect", name) + "_RC": "0"
                  for name in (lint_volume(self.checkout), "torchsnap-linux-build-cargo", "torchsnap-linux-build-bun")}
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout), env=exists)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row for row in self.tree.docker("volume") if row[1] == "create"], [])


# =========================================================================
# linux-build build
# =========================================================================

class Build(TreeCase):
    def build_run(self):
        runs = [run for run in self.tree.docker("run") if run[1:3] == ["--name", "torchsnap-linux-build"]]
        self.assertEqual(len(runs), 1)
        return runs[0]

    def test_runs_the_build_container_on_this_checkout(self):
        result = self.tree.run("linux-build", "build")
        self.assertEqual(result.returncode, 0, result.stderr)
        run = self.build_run()
        root = self.tree.root
        out = root / "target" / "linux-build" / "debug"
        for expected in (
            ["--platform", "linux/arm64"],
            ["--mount", f"type=bind,src={root},dst=/src,readonly"],
            ["--mount", f"type=bind,src={root}/tools/linux-build,dst=/linux-build,readonly"],
            ["--mount", "type=volume,src=torchsnap-linux-build-work,dst=/work"],
            ["--mount", "type=volume,src=torchsnap-linux-build-cargo,dst=/usr/local/cargo/registry"],
            ["--mount", "type=volume,src=torchsnap-linux-build-bun,dst=/var/cache/bun"],
            ["--mount", f"type=bind,src={out},dst=/out"],
            ["-e", "LINUX_BUILD_WORK=/work"],
        ):
            self.assertTrue(contains_run(run, expected), f"{expected} not in {run}")
        self.assertIn("--rm", run)
        self.assertIn("--init", run)
        self.assertEqual(run[-4:], ["torchsnap-linux-build:latest", "/linux-build/in-container", "build", "debug"])
        self.assertTrue(out.is_dir())

    def test_release(self):
        result = self.tree.run("linux-build", "build", "--release")
        self.assertEqual(result.returncode, 0, result.stderr)
        run = self.build_run()
        self.assertIn(f"type=bind,src={self.tree.root}/target/linux-build/release,dst=/out", run)
        self.assertEqual(run[-2:], ["build", "release"])

    def test_creates_the_work_volume_with_the_label(self):
        result = self.tree.run("linux-build", "build")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(["volume", "create", "--label", "org.torchsnap.linux-build=1", "torchsnap-linux-build-work"],
                      [row for row in self.tree.docker("volume") if row[1] == "create"])

    def test_a_failed_image_build_stops_before_the_container(self):
        result = self.tree.run("linux-build", "build", env={"STUB_DOCKER_BUILDX_BUILD_RC": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.tree.docker("run"), [])

    def test_mounts_the_git_dir_so_the_build_knows_its_commit(self):
        git_out = "/Users/someone/repo/.git/worktrees/w\n/Users/someone/repo/.git"
        result = self.tree.run("linux-build", "build", env={"STUB_GIT_OUT": git_out})
        self.assertEqual(result.returncode, 0, result.stderr)
        run = self.build_run()
        self.assertTrue(contains_run(run, ["--mount", "type=bind,src=/Users/someone/repo/.git,dst=/Users/someone/repo/.git,readonly"]))
        self.assertTrue(contains_run(run, ["-e", "GIT_DIR=/Users/someone/repo/.git/worktrees/w"]))
        self.assertIn(["-C", str(self.tree.root), "rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"],
                      [row[1:] for row in self.tree.calls("git")])

    def test_builds_without_git_outside_a_checkout(self):
        result = self.tree.run("linux-build", "build", env={"STUB_GIT_RC": "128"})
        self.assertEqual(result.returncode, 0, result.stderr)
        run = self.build_run()
        self.assertFalse(any(arg.startswith("GIT_DIR=") for arg in run))

    def test_the_work_path_can_be_set(self):
        result = self.tree.run("linux-build", "build", env={"TORCHSNAP_LINUX_BUILD_WORKDIR": "/home/someone/tree"})
        self.assertEqual(result.returncode, 0, result.stderr)
        run = self.build_run()
        self.assertTrue(contains_run(run, ["--mount", "type=volume,src=torchsnap-linux-build-work,dst=/home/someone/tree"]))
        self.assertTrue(contains_run(run, ["-e", "LINUX_BUILD_WORK=/home/someone/tree"]))

    def test_a_relative_or_reserved_work_path_exits_2_before_docker(self):
        for value in ("rel/path", "", "/", "/src", "/out", "/linux-build"):
            with self.subTest(value=value):
                self.tree.log.unlink(missing_ok=True)
                result = self.tree.run("linux-build", "build", env={"TORCHSNAP_LINUX_BUILD_WORKDIR": value})
                self.assertEqual(result.returncode, 2)
                self.assertIn("TORCHSNAP_LINUX_BUILD_WORKDIR", result.stderr)
                self.assertEqual(self.tree.calls("docker"), [])


# =========================================================================
# Disk use: what the Linux build owns, status, limits, clean
# =========================================================================

HELPER = stub_key("docker", "run", "--name", "torchsnap-linux-build-helper") + "_OUT"
IMAGE_SIZE = stub_key("docker", "image", "inspect", "torchsnap-linux-build:latest", "--format", "{{.Size}}") + "_OUT"
OWNED_LABEL = "org.torchsnap.linux-build=1"
# The whole command line the Dockerfile's SHELL puts into each record's
# description, so a record that merely contains the variable name does
# not match.
CACHE_FILTER = 'description~="from exec /usr/bin/env TORCHSNAP_LINUX_BUILD=1 /bin/sh -c "'
CACHE_DU = ["buildx", "du", "--builder", "testctx", "--filter", CACHE_FILTER, "--format", "{{.ID}} {{.Size}}"]

# Every Docker subcommand the scripts may run. Anything else, a prune
# of everything for example, fails the guard.
ALLOWED_DOCKER = {
    ("context", "show"), ("buildx", "build"), ("buildx", "du"), ("buildx", "prune"),
    ("ps", "-q"), ("image", "ls"), ("image", "inspect"), ("image", "rm"),
    ("volume", "ls"), ("volume", "inspect"), ("volume", "create"), ("volume", "rm"),
    ("system", "df"), ("run", "--name"),
}
OWNED_VOLUME = re.compile(r"^torchsnap-linux-build-(work|cargo|bun|lint-[0-9a-f]{8})$")


class Docker:
    """Builds the stub environment for a Docker that holds some of the
    Linux build's objects, and some of other projects'."""

    def __init__(self, tmp):
        self.tmp = pathlib.Path(tmp)
        self.env = {}
        self.labelled = []   # (name, checkout) of labelled volumes
        self.sizes = []      # "name size" lines of `docker system df -v`
        self.last_use = []   # "name epoch" lines of the helper

    def image(self, size_bytes):
        self.env[IMAGE_SIZE] = str(size_bytes)
        return self

    def cache(self, *sizes):
        self.env["STUB_DOCKER_BUILDX_DU_OUT"] = "\n".join(f"rec{i} {size}" for i, size in enumerate(sizes))
        return self

    def dangling(self, *lines):
        self.env["STUB_DOCKER_IMAGE_LS_OUT"] = "\n".join(lines)
        return self

    def volume(self, name, size, last_use, checkout=None, labelled=True):
        if labelled:
            self.labelled.append((name, checkout or ""))
        self.env[stub_key("docker", "volume", "inspect", name) + "_RC"] = "0"
        self.sizes.append(f"{name} {size}")
        self.last_use.append(f"{name} {last_use}")
        return self

    def checkout(self, name):
        path = self.tmp / name
        path.mkdir(exist_ok=True)
        return os.path.realpath(path)

    def build(self):
        env = dict(self.env)
        env["STUB_DOCKER_VOLUME_LS_OUT"] = "\n".join(f"{n}\t{c}" for n, c in self.labelled)
        env["STUB_DOCKER_SYSTEM_DF_OUT"] = "\n".join(self.sizes)
        env[HELPER] = "\n".join(self.last_use)
        return env


def removed(tree, kind):
    return [row[3] for row in tree.calls("docker") if row[1:3] == [kind, "rm"]]


def cache_prunes(tree):
    return [row[1:] for row in tree.calls("docker") if row[1:3] == ["buildx", "prune"]]


class Status(TreeCase):
    def test_lists_every_owned_object_with_its_size_and_the_total(self):
        docker = Docker(self.tree._tmp.name)
        alive = docker.checkout("alive")
        env = (docker.image(2_260_000_000).cache("1.5GB", "600MB")
               .volume("torchsnap-linux-build-cargo", "500MB", 1761234000)
               .volume("torchsnap-linux-build-lint-0a1b2c3d", "3GB", 1761234500, checkout=alive)
               .volume("torchsnap-linux-build-lint-4e5f6a7b", "1GB", 1761230000, checkout="/gone/checkout")
               .build())
        (self.tree.root / "target" / "linux-build" / "debug").mkdir(parents=True)
        result = self.tree.run("linux-build", "status", env=dict(env, TORCHSNAP_LINUX_BUILD_BUDGET_GB="20"))
        self.assertEqual(result.returncode, 0, result.stderr)
        out = result.stdout
        self.assertIn("torchsnap-linux-build:latest", out)
        self.assertIn("2.26 GB", out)
        self.assertIn("2.10 GB", out)                        # 1.5 GB + 600 MB of marked cache
        self.assertIn("torchsnap-linux-build-cargo", out)
        self.assertIn(f"{alive} (exists)", out)
        self.assertIn("/gone/checkout (gone)", out)
        self.assertIn("target/linux-build/debug", out)
        self.assertRegex(out, r"total\s+8\.86 GB")          # 2.26 + 2.10 + 0.5 + 3 + 1, output dirs empty
        self.assertIn("budget 20 GB", out)
        self.assertIn("outputs in other checkouts are not counted", out)
        self.assertIn(CACHE_DU, [row[1:] for row in self.tree.calls("docker")])
        self.assertNotIn("B1", out)

    def test_counts_untagged_predecessors_of_the_image(self):
        docker = Docker(self.tree._tmp.name).image(2_000_000_000).dangling("sha256:old1 1.5GB")
        result = self.tree.run("linux-build", "status", env=docker.build())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("sha256:old1", result.stdout)
        self.assertRegex(result.stdout, r"total\s+3\.50 GB")
        self.assertIn(DANGLING, [row[1:] for row in self.tree.calls("docker")])

    def test_shows_when_each_work_volume_was_last_used(self):
        docker = Docker(self.tree._tmp.name)
        env = (docker.image(1_000_000_000)
               .volume("torchsnap-linux-build-cargo", "500MB", 0)
               .volume("torchsnap-linux-build-lint-0a1b2c3d", "3GB", 1761234500, checkout="/c")
               .build())
        result = self.tree.run("linux-build", "status", env=dict(env, TZ="UTC"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("last use 2025-10-23 15:48", result.stdout)
        self.assertRegex(result.stdout, r"torchsnap-linux-build-cargo .*download cache")
        self.assertEqual(result.stderr, "")

    def test_never_lists_volumes_it_does_not_own(self):
        docker = Docker(self.tree._tmp.name)
        env = docker.image(1_000_000_000).volume("torchsnap-linux-build-cargo", "1GB", 1).build()
        env["STUB_DOCKER_VOLUME_LS_OUT"] += "\nmy-torchsnap-linux-build-lint-x\t\ntorchsnap-linux-build-lint-zzz\t"
        env["STUB_DOCKER_SYSTEM_DF_OUT"] += "\nother-project-data 9GB\nmy-torchsnap-linux-build-lint-x 9GB"
        result = self.tree.run("linux-build", "status", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("other-project-data", result.stdout)
        self.assertNotIn("lint-x", result.stdout)
        self.assertNotIn("lint-zzz", result.stdout)

    def test_without_the_image_shows_sizes_but_no_last_use(self):
        docker = Docker(self.tree._tmp.name)
        env = docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 1761234000, checkout="/c").build()
        result = self.tree.run("linux-build", "status", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("torchsnap-linux-build-lint-0a1b2c3d", result.stdout)
        self.assertIn("1.00 GB", result.stdout)
        self.assertIn("last use unknown", result.stdout)
        self.assertEqual([run for run in self.tree.docker("run")], [])


class Limits(TreeCase):
    """Limits run at the start of `lint`, before its container."""

    def setUp(self):
        super().setUp()
        self.checkout = pathlib.Path(self.tree._tmp.name) / "this-checkout"
        (self.checkout / "src-tauri").mkdir(parents=True)
        (self.checkout / "src-tauri" / "Cargo.toml").write_text("[package]\n")
        self.docker = Docker(self.tree._tmp.name).image(2_000_000_000)

    def lint(self, **extra):
        env = self.docker.build()
        env.update(extra)
        return self.tree.run("linux-build", "lint", "--path", str(self.checkout), env=env)

    def main_run_index(self):
        calls = self.tree.calls("docker")
        return next(i for i, row in enumerate(calls) if row[1:4] == ["run", "--name", "torchsnap-linux-build-lint"])

    def test_a_lint_volume_whose_checkout_is_gone_is_evicted(self):
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 100, checkout="/gone/checkout")
        result = self.lint()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), ["torchsnap-linux-build-lint-0a1b2c3d"])
        self.assertIn("evicted torchsnap-linux-build-lint-0a1b2c3d (checkout gone), freed 1.00 GB", result.stderr)
        rm_index = next(i for i, row in enumerate(self.tree.calls("docker")) if row[1:3] == ["volume", "rm"])
        self.assertLess(rm_index, self.main_run_index())

    def test_keeps_at_most_the_configured_number_of_lint_volumes(self):
        for name, last in (("0a1b2c3d", 100), ("1b2c3d4e", 300), ("2c3d4e5f", 200)):
            self.docker.volume(f"torchsnap-linux-build-lint-{name}", "1GB", last,
                               checkout=self.docker.checkout(name))
        # With this run's volume that makes four; two may stay.
        result = self.lint(TORCHSNAP_LINUX_BUILD_MAX_LINT_VOLUMES="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"),
                         ["torchsnap-linux-build-lint-0a1b2c3d", "torchsnap-linux-build-lint-2c3d4e5f"])
        self.assertIn("(more than 2 lint volumes)", result.stderr)

    def test_never_evicts_the_volume_of_this_run(self):
        own = lint_volume(self.checkout)
        self.docker.volume(own, "5GB", 1, checkout=os.path.realpath(self.checkout))
        result = self.lint(TORCHSNAP_LINUX_BUILD_MAX_LINT_VOLUMES="1", TORCHSNAP_LINUX_BUILD_BUDGET_GB="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(own, removed(self.tree, "volume"))

    def test_over_budget_evicts_other_lint_volumes_first_then_the_cache(self):
        self.docker.cache("3GB")
        self.docker.volume("torchsnap-linux-build-cargo", "1GB", 1)
        self.docker.volume("torchsnap-linux-build-bun", "1GB", 1)
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "4GB", 500, checkout=self.docker.checkout("a"))
        self.docker.volume("torchsnap-linux-build-lint-1b2c3d4e", "4GB", 100, checkout=self.docker.checkout("b"))
        # image 2 + cache 3 + caches 2 + lint 8 = 15 GB against 6 GB: both
        # lint volumes go, older first (to 7 GB), then the cache (to 4 GB).
        # The image and the cargo and bun caches stay.
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="6")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"),
                         ["torchsnap-linux-build-lint-1b2c3d4e", "torchsnap-linux-build-lint-0a1b2c3d"])
        self.assertEqual(cache_prunes(self.tree), [["buildx", "prune", "-f", "--all", "--builder", "testctx",
                                                    "--filter", CACHE_FILTER]])
        self.assertIn("(over budget)", result.stderr)
        self.assertEqual(removed(self.tree, "image"), [])

    def test_a_lint_evicts_the_build_work_volume_last(self):
        self.docker.cache("1GB")
        self.docker.volume("torchsnap-linux-build-work", "8GB", 50)
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "2GB", 100, checkout=self.docker.checkout("a"))
        # 13 GB against 3 GB: lint volume, cache, then the build's volume.
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="3")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"),
                         ["torchsnap-linux-build-lint-0a1b2c3d", "torchsnap-linux-build-work"])
        self.assertEqual(len(cache_prunes(self.tree)), 1)
        calls = [row[1:3] for row in self.tree.calls("docker")]
        prune = calls.index(["buildx", "prune"])
        work_rm = [i for i, row in enumerate(self.tree.calls("docker"))
                   if row[1:4] == ["volume", "rm", "torchsnap-linux-build-work"]][0]
        self.assertLess(prune, work_rm)

    def test_a_lint_keeps_the_build_work_volume_while_a_build_runs(self):
        self.docker.volume("torchsnap-linux-build-work", "8GB", 50)
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="3", STUB_DOCKER_PS_OUT="c0ffee")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("torchsnap-linux-build-work", removed(self.tree, "volume"))
        self.assertIn("warning", result.stderr)

    def test_a_build_never_evicts_its_own_work_volume(self):
        self.docker.cache("1GB")
        self.docker.volume("torchsnap-linux-build-work", "8GB", 50)
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "2GB", 100, checkout=self.docker.checkout("a"))
        env = self.docker.build()
        env["TORCHSNAP_LINUX_BUILD_BUDGET_GB"] = "3"
        result = self.tree.run("linux-build", "build", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), ["torchsnap-linux-build-lint-0a1b2c3d"])
        self.assertEqual(len(cache_prunes(self.tree)), 1)
        self.assertIn("warning", result.stderr)
        calls = self.tree.calls("docker")
        rm_index = next(i for i, row in enumerate(calls) if row[1:3] == ["volume", "rm"])
        run_index = next(i for i, row in enumerate(calls) if row[1:4] == ["run", "--name", "torchsnap-linux-build"])
        self.assertLess(rm_index, run_index)

    def test_stops_evicting_once_within_budget(self):
        self.docker.cache("3GB")
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "4GB", 500, checkout=self.docker.checkout("a"))
        self.docker.volume("torchsnap-linux-build-lint-1b2c3d4e", "4GB", 100, checkout=self.docker.checkout("b"))
        # 13 GB against 10 GB: the older lint volume is enough.
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="10")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), ["torchsnap-linux-build-lint-1b2c3d4e"])
        self.assertEqual(cache_prunes(self.tree), [])

    def test_a_failed_cache_listing_does_not_prune(self):
        self.docker.cache("3GB")
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="1", STUB_DOCKER_BUILDX_DU_RC="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(cache_prunes(self.tree), [])
        self.assertIn("cannot measure the build cache", result.stderr)
        self.main_run_index()

    def test_an_empty_cache_listing_does_not_prune(self):
        self.docker.volume("torchsnap-linux-build-cargo", "30GB", 1)
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="1", STUB_DOCKER_BUILDX_DU_OUT="")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(cache_prunes(self.tree), [])

    def test_within_budget_evicts_nothing(self):
        self.docker.cache("1GB")
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 100, checkout=self.docker.checkout("a"))
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="10")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), [])
        self.assertEqual(cache_prunes(self.tree), [])

    def test_an_unreachable_budget_warns_and_the_run_goes_on(self):
        self.docker.volume("torchsnap-linux-build-cargo", "30GB", 1)
        result = self.lint(TORCHSNAP_LINUX_BUILD_BUDGET_GB="5")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("warning", result.stderr)
        self.assertEqual(removed(self.tree, "volume"), [])
        self.main_run_index()

    def test_a_volume_in_use_is_kept_and_the_run_goes_on(self):
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 100, checkout="/gone/checkout")
        result = self.lint(**{stub_key("docker", "volume", "rm", "torchsnap-linux-build-lint-0a1b2c3d") + "_RC": "1"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("torchsnap-linux-build-lint-0a1b2c3d in use, kept", result.stderr)
        self.main_run_index()

    def test_without_the_image_nothing_is_evicted(self):
        docker = Docker(self.tree._tmp.name)
        docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 100, checkout="/gone/checkout")
        env = docker.build()
        result = self.tree.run("linux-build", "lint", "--path", str(self.checkout), env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), [])

    def test_invalid_limits_exit_2_before_docker(self):
        for name in ("TORCHSNAP_LINUX_BUILD_BUDGET_GB", "TORCHSNAP_LINUX_BUILD_MAX_LINT_VOLUMES"):
            for value in ("0", "-1", "1.5", "lots", "", "100000", "99999999999"):
                with self.subTest(name=name, value=value):
                    self.tree.log.unlink(missing_ok=True)
                    result = self.tree.run("linux-build", "lint", "--path", str(self.checkout), env={name: value})
                    self.assertEqual(result.returncode, 2)
                    self.assertIn(name, result.stderr)
                    self.assertEqual(self.tree.calls("docker"), [])


class Clean(TreeCase):
    def setUp(self):
        super().setUp()
        self.docker = Docker(self.tree._tmp.name).image(2_000_000_000).cache("1GB")
        self.docker.volume("torchsnap-linux-build-cargo", "1GB", 1)
        self.docker.volume("torchsnap-linux-build-bun", "1GB", 1)
        self.docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 1, checkout="/a")
        self.docker.volume("torchsnap-linux-build-lint-4e5f6a7b", "1GB", 1, checkout="/b")

    def test_shows_status_then_drops_build_output_and_keeps_the_caches(self):
        (self.tree.root / "target" / "linux-build" / "debug").mkdir(parents=True)
        result = self.tree.run("linux-build", "clean", env=self.docker.build())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("total", result.stdout)
        self.assertEqual(removed(self.tree, "volume"),
                         ["torchsnap-linux-build-lint-0a1b2c3d", "torchsnap-linux-build-lint-4e5f6a7b"])
        self.assertEqual(cache_prunes(self.tree), [])
        self.assertEqual(removed(self.tree, "image"), [])
        self.assertFalse((self.tree.root / "target" / "linux-build").exists())
        self.assertIn("freed", result.stderr)

    def test_all_removes_everything_b1_owns(self):
        result = self.tree.run("linux-build", "clean", "--all", env=self.docker.build())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), [
            "torchsnap-linux-build-lint-0a1b2c3d", "torchsnap-linux-build-lint-4e5f6a7b",
            "torchsnap-linux-build-cargo", "torchsnap-linux-build-bun",
        ])
        self.assertEqual(cache_prunes(self.tree), [["buildx", "prune", "-f", "--all", "--builder", "testctx",
                                                    "--filter", CACHE_FILTER]])
        self.assertEqual(removed(self.tree, "image"), ["torchsnap-linux-build:latest"])

    def test_all_also_removes_untagged_predecessors_with_the_label(self):
        env = self.docker.dangling("sha256:old1 1GB", "sha256:old2 1GB").build()
        result = self.tree.run("linux-build", "clean", "--all", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "image"), ["torchsnap-linux-build:latest", "sha256:old1", "sha256:old2"])

    def test_all_does_not_prune_when_the_cache_listing_fails(self):
        env = self.docker.build()
        env["STUB_DOCKER_BUILDX_DU_RC"] = "1"
        result = self.tree.run("linux-build", "clean", "--all", env=env)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(cache_prunes(self.tree), [])
        self.assertIn("cannot measure the build cache", result.stderr)

    def test_adopts_unlabelled_volumes_only_by_their_exact_names(self):
        docker = Docker(self.tree._tmp.name)
        docker.volume("torchsnap-linux-build-work", "1GB", 1, labelled=False)
        env = docker.build()
        result = self.tree.run("linux-build", "clean", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(removed(self.tree, "volume"), ["torchsnap-linux-build-work"])

    def test_a_volume_that_cannot_be_removed_is_kept_reported_and_fails_clean(self):
        env = self.docker.build()
        env[stub_key("docker", "volume", "rm", "torchsnap-linux-build-lint-0a1b2c3d") + "_RC"] = "1"
        result = self.tree.run("linux-build", "clean", env=env)
        self.assertEqual(result.returncode, 1)
        self.assertIn("torchsnap-linux-build-lint-0a1b2c3d in use, kept", result.stderr)
        self.assertIn("torchsnap-linux-build-lint-4e5f6a7b", removed(self.tree, "volume"))


class Guard(TreeCase):
    """Only the Linux build's own Docker objects are ever removed."""

    def foreign_env(self):
        docker = Docker(self.tree._tmp.name).image(1_000_000_000).cache("1GB")
        docker.volume("torchsnap-linux-build-lint-0a1b2c3d", "1GB", 1, checkout="/gone")
        env = docker.build()
        # Names a careless prefix or substring match would take.
        env["STUB_DOCKER_VOLUME_LS_OUT"] += ("\nmy-torchsnap-linux-build-lint-x\t/gone"
                                            "\ntorchsnap-linux-build-lint-zzz\t/gone"
                                            "\nother-project-data\t/gone")
        env["STUB_DOCKER_SYSTEM_DF_OUT"] += "\nother-project-data 50GB\nmy-torchsnap-linux-build-lint-x 50GB"
        env[HELPER] += "\nother-project-data 1\nmy-torchsnap-linux-build-lint-x 1"
        env["STUB_DOCKER_IMAGE_LS_OUT"] = "sha256:mine 1GB"
        env[IMAGE_ID + "_OUT"] = "sha256:mine"
        env["TORCHSNAP_LINUX_BUILD_BUDGET_GB"] = "1"
        env["TORCHSNAP_LINUX_BUILD_MAX_LINT_VOLUMES"] = "1"
        return env

    def test_every_docker_call_is_allowed_and_removes_only_owned_objects(self):
        checkout = pathlib.Path(self.tree._tmp.name) / "c"
        (checkout / "src-tauri").mkdir(parents=True)
        (checkout / "src-tauri" / "Cargo.toml").write_text("[package]\n")
        env = self.foreign_env()
        for argv in (["image"], ["status"], ["lint", "--path", str(checkout)], ["build"],
                     ["clean"], ["clean", "--all"]):
            self.tree.run("linux-build", *argv, env=env)
        calls = [row[1:] for row in self.tree.calls("docker")]
        self.assertTrue(calls)
        for call in calls:
            self.assertIn(tuple(call[:2]), ALLOWED_DOCKER, call)
            if call[:2] == ["volume", "rm"]:
                self.assertRegex(call[2], OWNED_VOLUME)
                self.assertEqual(len(call), 3, call)
            if call[:2] == ["image", "rm"]:
                self.assertIn(call[2], ("torchsnap-linux-build:latest", "sha256:mine"))
                self.assertEqual(len(call), 3, call)
            if call[:2] == ["buildx", "prune"]:
                self.assertEqual(call[-2:], ["--filter", CACHE_FILTER])
            if call[:2] == ["buildx", "du"]:
                self.assertEqual(call, CACHE_DU)
            if call[:2] == ["volume", "ls"]:
                self.assertIn(f"label={OWNED_LABEL}", call)

    def test_the_scripts_contain_no_unfiltered_removal(self):
        for script in ("linux-build", "in-container"):
            # A command split over lines with a trailing backslash is
            # checked as one line.
            text = (TOOL_DIR / script).read_text().replace("\\\n", " ")
            for forbidden in ("system prune", "volume prune", "container prune", "image prune",
                              "builder prune", "docker rmi", " rm -f", "volume rm -f", "image rm -f"):
                self.assertNotIn(forbidden, text, f"{script} contains {forbidden!r}")
            for line in text.splitlines():
                if line.strip().startswith("#"):
                    continue
                if "buildx prune" in line:
                    self.assertIn('--filter "$cache_filter"', line, line)
                if "docker volume rm" in line:
                    self.assertIn('docker volume rm "$', line, line)
                if "docker image rm" in line:
                    self.assertIn('docker image rm "$', line, line)


# =========================================================================
# in-container lint
# =========================================================================

class InContainerLint(TreeCase):
    def setUp(self):
        super().setUp()
        base = pathlib.Path(self.tree._tmp.name)
        self.src = base / "src"
        self.work = base / "work"
        self.src.mkdir()
        self.work.mkdir()
        self.env = {"LINUX_BUILD_SRC": str(self.src), "LINUX_BUILD_WORK": str(self.work)}

    def test_syncs_the_tree_then_installs_checks_and_lints(self):
        result = self.tree.run("in-container", "lint", env=self.env, bash="bash")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row[1:] for row in self.tree.calls() if row[0] in ("rsync", "just")], [
            ["-rlpgoD", "--checksum", "--delete", "--filter=:- .gitignore", "--exclude=/.git", "--exclude=/.claude",
             "--exclude=/target", "--exclude=/.linux-build-last-use", f"{self.src}/", f"{self.work}/"],
            ["install"],
            ["check-crates"],
            ["lint-crates"],
        ])
        self.assertEqual(set(self.tree.cwds("just")), {str(self.work)})

    def test_a_failed_check_stops_before_clippy(self):
        result = self.tree.run("in-container", "lint", env=dict(self.env, STUB_JUST_CHECK_CRATES_RC="4"),
                               bash="bash")
        self.assertEqual(result.returncode, 4)
        self.assertIn(["just", "check-crates"], self.tree.calls("just"))
        self.assertNotIn(["just", "lint-crates"], self.tree.calls("just"))

    def test_records_the_last_use_in_the_work_volume(self):
        result = self.tree.run("in-container", "lint", env=self.env, bash="bash")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.work / ".linux-build-last-use").is_file())

    def test_unknown_command_exits_2(self):
        result = self.tree.run("in-container", "bogus", env=self.env, bash="bash")
        self.assertEqual(result.returncode, 2)


# =========================================================================
# in-container build, assemble, glibc
# =========================================================================

def fake_work_tree(work, profile="debug"):
    """A work tree as `just build` leaves it: the binary, the staged
    gadgets and the resources `tauri.conf.json` names."""
    tauri = work / "src-tauri"
    (tauri / "target" / profile).mkdir(parents=True)
    (tauri / "target" / profile / "torchsnap").write_text("ELF")
    (tauri / "icons").mkdir()
    (tauri / "icons" / "gadget-document.icns").write_text("icns")
    (tauri / "icons" / "Assets.car").write_text("car")
    shutil.copy(REPO / "src-tauri" / "tauri.conf.json", tauri)
    staged = work / "target" / "bundled-gadgets"
    staged.mkdir(parents=True)
    (staged / "a.torchsnap").write_text("a")
    (staged / "b.torchsnap").write_text("b")
    add_gadgets(work)


def add_gadgets(work, listed=(), unlisted=(), without_archive=(), stale_archives=()):
    """Gadget sources and the archives `just build` leaves next to them
    in `gadgets/`. `listed` ids are in `bundled.toml`, which a fake
    `tools/list-bundled-gadgets` prints; `without_archive` gadgets have
    a manifest but no archive; `stale_archives` are archives without a
    gadget directory. Each archive's mtime is set to a fixed past time,
    so a copy that keeps it is told from one that does not."""
    gadgets = work / "gadgets"
    gadgets.mkdir(exist_ok=True)
    for gadget in (*listed, *unlisted, *without_archive):
        (gadgets / gadget).mkdir()
        (gadgets / gadget / "manifest.toml").write_text(f'[gadget]\nid = "{gadget}"\n')
    for archive in (*listed, *unlisted, *stale_archives):
        path = gadgets / f"{archive}.torchsnap"
        path.write_text(archive)
        os.utime(path, (1_600_000_000, 1_600_000_000))
    (gadgets / "bundled.toml").write_text(
        "gadgets = [\n" + "".join(f'    "{gadget}",\n' for gadget in listed) + "]\n")
    lister = work / "tools" / "list-bundled-gadgets"
    lister.parent.mkdir(exist_ok=True)
    lister.write_text("#!/bin/bash\n" + "".join(f"echo {gadget}\n" for gadget in listed))
    lister.chmod(0o755)


class InContainerCase(TreeCase):
    def setUp(self):
        super().setUp()
        base = pathlib.Path(self.tree._tmp.name)
        self.src = base / "src"
        self.work = base / "work"
        self.out = base / "out"
        for path in (self.src, self.work, self.out):
            path.mkdir()
        self.env = {"LINUX_BUILD_SRC": str(self.src), "LINUX_BUILD_WORK": str(self.work),
                    "LINUX_BUILD_OUT": str(self.out)}

    def run_in_container(self, *args, **extra):
        return self.tree.run("in-container", *args, env=dict(self.env, **extra), bash="bash")


@unittest.skipUnless(shutil.which("jq"), "jq is not installed")
class InContainerBuild(InContainerCase):
    def test_syncs_installs_builds_without_bundling_and_assembles(self):
        fake_work_tree(self.work)
        result = self.run_in_container("build", "debug", STUB_DPKG_QUERY_OUT="libc6 2.41-12")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row[1:] for row in self.tree.calls() if row[0] in ("rsync", "just")], [
            ["-rlpgoD", "--checksum", "--delete", "--filter=:- .gitignore", "--exclude=/.git", "--exclude=/.claude",
             "--exclude=/target", "--exclude=/.linux-build-last-use", f"{self.src}/", f"{self.work}/"],
            ["install"],
            ["build", "--config", '{"bundle":{"active":false}}'],
        ])
        self.assertEqual(set(self.tree.cwds("just")), {str(self.work)})
        self.assertTrue((self.out / "bin" / "torchsnap").is_file())
        self.assertTrue((self.work / ".linux-build-last-use").is_file())
        for phase in ("sync", "install", "build", "assemble"):
            self.assertRegex(result.stderr, rf"linux-build: {phase} \d+ s")

    def test_release_builds_the_release_profile(self):
        fake_work_tree(self.work, "release")
        result = self.run_in_container("build", "release")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(["build", "--release", "--config", '{"bundle":{"active":false}}'],
                      [row[1:] for row in self.tree.calls("just")])
        self.assertTrue((self.out / "bin" / "torchsnap").is_file())

    def test_records_the_image_packages_next_to_the_binary(self):
        fake_work_tree(self.work)
        result = self.run_in_container("build", "debug", STUB_DPKG_QUERY_OUT="libc6 2.41-12\nlibgtk-3-0t64 3.24.49-3")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.out / "build-packages.txt").read_text(), "libc6 2.41-12\nlibgtk-3-0t64 3.24.49-3\n")
        self.assertIn(["-W", "-f", "${Package} ${Version}\\n"], [row[1:] for row in self.tree.calls("dpkg-query")])

    def test_a_failed_install_stops_before_the_build(self):
        result = self.run_in_container("build", "debug", STUB_JUST_INSTALL_RC="5")
        self.assertEqual(result.returncode, 5)
        self.assertNotIn("build", [row[1] for row in self.tree.calls("just")])

    def test_an_unknown_profile_exits_2(self):
        result = self.run_in_container("build", "fast")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.tree.calls("rsync"), [])


# A `just` that builds like cargo, for one source file: `build` writes
# `src-tauri/src/main.rs`'s content into the debug binary only when the
# binary is missing or older than `main.rs`, and then sets the binary's
# mtime to FAKE_BUILD_MTIME, so the order of the mtimes never depends on
# the clock's resolution. Every other recipe does nothing.
CARGO_LIKE_JUST = """#!/usr/bin/env python3
import os, pathlib, sys
if sys.argv[1:2] == ["build"]:
    source = pathlib.Path("src-tauri/src/main.rs")
    binary = pathlib.Path("src-tauri/target/debug/torchsnap")
    if not binary.exists() or binary.stat().st_mtime_ns < source.stat().st_mtime_ns:
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text(source.read_text())
        mtime = int(os.environ["FAKE_BUILD_MTIME"])
        os.utime(binary, (mtime, mtime))
"""


@unittest.skipUnless(shutil.which("jq"), "jq is not installed")
class InContainerTwoCheckouts(TreeCase):
    """Builds from two checkouts in turn into one work volume, with the
    real rsync and a `just` that rebuilds by mtime as cargo does."""

    def setUp(self):
        self.tree = Tree(stubs=[name for name in STUBBED if name not in ("rsync", "just")])
        self.addCleanup(self.tree.close)
        (self.tree.bin / "just").write_text(CARGO_LIKE_JUST)
        (self.tree.bin / "just").chmod(0o755)
        base = pathlib.Path(self.tree._tmp.name)
        self.work = base / "work"
        self.out = base / "out"
        self.work.mkdir()
        self.out.mkdir()
        self.now = int(time.time())
        self.a = self.checkout(base / "a")
        self.b = self.checkout(base / "b")

    def checkout(self, root):
        """A checkout with what `build debug` needs to get through
        `assemble`: a product name, no resources, no gadgets."""
        (root / "src-tauri" / "src").mkdir(parents=True)
        (root / ".gitignore").write_text("/src-tauri/target/\n")
        (root / "src-tauri" / "tauri.conf.json").write_text(
            json.dumps({"productName": "Torchsnap", "bundle": {"resources": None}}))
        add_gadgets(root)
        return root

    def write(self, checkout, relative, text, age):
        path = checkout / relative
        path.write_text(text)
        os.utime(path, (self.now - age, self.now - age))
        return path

    def build_from(self, checkout):
        result = self.tree.run("in-container", "build", "debug", bash="bash", env={
            "LINUX_BUILD_SRC": str(checkout), "LINUX_BUILD_WORK": str(self.work),
            "LINUX_BUILD_OUT": str(self.out), "FAKE_BUILD_MTIME": str(self.now - 100),
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        return (self.out / "bin" / "torchsnap").read_text()

    def test_a_change_older_than_the_last_build_from_another_checkout_is_built(self):
        self.write(self.a, "src-tauri/src/main.rs", "a", age=1000)
        self.assertEqual(self.build_from(self.a), "a")
        # B changed main.rs before A's build: its mtime is older than the
        # binary in the work volume.
        self.write(self.b, "src-tauri/src/main.rs", "b", age=500)
        self.assertEqual(self.build_from(self.b), "b")
        self.assertEqual(self.build_from(self.a), "a")

    def test_an_unchanged_file_keeps_the_volumes_mtime(self):
        for checkout, age in ((self.a, 1000), (self.b, 500)):
            self.write(checkout, "src-tauri/src/main.rs", "same", age=age)
            self.write(checkout, "src-tauri/src/lib.rs", "same", age=age)
        copied = self.work / "src-tauri" / "src" / "lib.rs"
        self.build_from(self.a)
        after_a = copied.stat().st_mtime_ns
        self.build_from(self.b)
        self.assertEqual(copied.stat().st_mtime_ns, after_a)


@unittest.skipUnless(shutil.which("jq"), "jq is not installed")
class Assemble(InContainerCase):
    def test_lays_out_binary_and_resources_like_the_deb(self):
        fake_work_tree(self.work)
        (self.out / "stale-from-before").write_text("old")
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 0, result.stderr)
        files = sorted(str(p.relative_to(self.out)) for p in self.out.rglob("*") if p.is_file())
        self.assertEqual(files, [
            "bin/torchsnap",
            "lib/Torchsnap/Assets.car",
            "lib/Torchsnap/gadget-document.icns",
            "lib/Torchsnap/gadgets/a.torchsnap",
            "lib/Torchsnap/gadgets/b.torchsnap",
        ])

    def test_a_debug_build_carries_the_archives_of_the_unlisted_gadgets(self):
        fake_work_tree(self.work)
        add_gadgets(self.work, listed=["calculator"], unlisted=["hello-world", "zerotier"],
                    stale_archives=["removed-gadget"])
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 0, result.stderr)
        dev = self.out / "dev-gadgets"
        self.assertEqual(sorted(p.name for p in dev.iterdir()), ["hello-world.torchsnap", "zerotier.torchsnap"])
        self.assertEqual((dev / "hello-world.torchsnap").read_text(), "hello-world")
        self.assertEqual((dev / "hello-world.torchsnap").stat().st_mtime, 1_600_000_000)

    def test_a_debug_build_without_unlisted_gadgets_has_an_empty_dev_gadgets_dir(self):
        fake_work_tree(self.work)
        add_gadgets(self.work, listed=["calculator"])
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list((self.out / "dev-gadgets").iterdir()), [])

    def test_a_release_build_has_no_dev_gadgets(self):
        fake_work_tree(self.work, "release")
        add_gadgets(self.work, unlisted=["hello-world"])
        result = self.run_in_container("assemble", "release", str(self.out))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.out / "dev-gadgets").exists())

    def test_an_unlisted_gadget_without_its_archive_exits_1_naming_it(self):
        fake_work_tree(self.work)
        add_gadgets(self.work, unlisted=["hello-world"], without_archive=["zerotier"])
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 1)
        self.assertIn("gadgets/zerotier.torchsnap", result.stderr)

    def test_a_missing_binary_exits_1_naming_it(self):
        fake_work_tree(self.work)
        (self.work / "src-tauri" / "target" / "debug" / "torchsnap").unlink()
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 1)
        self.assertIn("src-tauri/target/debug/torchsnap", result.stderr)

    def test_a_missing_resource_exits_1_naming_it(self):
        fake_work_tree(self.work)
        (self.work / "src-tauri" / "icons" / "Assets.car").unlink()
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 1)
        self.assertIn("icons/Assets.car", result.stderr)

    def test_resources_as_a_list_are_unsupported(self):
        fake_work_tree(self.work)
        conf = self.work / "src-tauri" / "tauri.conf.json"
        data = json.loads(conf.read_text())
        data["bundle"]["resources"] = ["icons/Assets.car"]
        conf.write_text(json.dumps(data))
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 1)
        self.assertIn("unsupported", result.stderr)

    def test_a_glob_resource_is_unsupported(self):
        fake_work_tree(self.work)
        conf = self.work / "src-tauri" / "tauri.conf.json"
        data = json.loads(conf.read_text())
        data["bundle"]["resources"] = {"icons/*.car": "icons/"}
        conf.write_text(json.dumps(data))
        result = self.run_in_container("assemble", "debug", str(self.out))
        self.assertEqual(result.returncode, 1)
        self.assertIn("unsupported", result.stderr)


class Glibc(InContainerCase):
    def test_prints_the_highest_glibc_version_the_binary_needs(self):
        objdump = "\n".join([
            "0000 DF *UND* 0000 (GLIBC_2.17) memcpy",
            "0000 DF *UND* 0000 (GLIBC_2.34) pthread_create",
            "0000 DF *UND* 0000 (GLIBC_2.9) pipe2",
            "0000 DF *UND* 0000 (GLIBC_PRIVATE) __libc_x",
        ])
        result = self.run_in_container("glibc", "/out/bin/torchsnap", STUB_OBJDUMP_OUT=objdump)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "2.34")
        self.assertIn(["-T", "/out/bin/torchsnap"], [row[1:] for row in self.tree.calls("objdump")])

    def test_a_binary_without_glibc_symbols_exits_1(self):
        result = self.run_in_container("glibc", "/out/bin/torchsnap", STUB_OBJDUMP_OUT="nothing")
        self.assertEqual(result.returncode, 1)


# =========================================================================
# linux-build check and run-check (on a Linux machine over ssh)
# =========================================================================

# An ssh stub that evaluates the remote command the way the remote shell
# would, with the runner or `bash -c`, so the test sees what arrives.
SSH_EVAL = """#!/bin/bash
{ printf 'ssh'; for a in "$@"; do printf '\\037%s' "$a"; done; printf '\\n'; } >> "$STUB_LOG"
shift  # the host
printf '%s\\n' "$*" >> "$SSH_REMOTE_LOG"
# The target's own run check cannot run here; it answers from the env.
case "$*" in *target-run-check*) printf '%s\\n' "$STUB_TARGET_OUT"; exit "${STUB_TARGET_RC:-0}" ;; esac
eval "$*"
"""

# The runner the eval stub finds on PATH: records the command it got.
RUNNER = """#!/bin/bash
printf '%s\\n' "$*" >> "$RUNNER_LOG"
eval "$*"
"""

HOST = {"TORCHSNAP_LINUX_HOST": "someone@target.local"}


def remote_env(home="/home/someone", getconf="glibc 2.41", ldd="\tlinux-vdso.so.1 (0x0)\n\tlibc.so.6 => /lib/libc.so.6", ldd_rc="0"):
    """Stub answers of a target machine, by remote command."""
    return {
        "REMOTE_HOME": home,
        "REMOTE_GETCONF": getconf,
        "REMOTE_LDD": ldd,
        "REMOTE_LDD_RC": ldd_rc,
    }


class TargetCase(TreeCase):
    """`check` and `run-check` against stubbed ssh and rsync."""

    def setUp(self):
        super().setUp()
        out = self.tree.root / "target" / "linux-build" / "debug"
        (out / "bin").mkdir(parents=True)
        (out / "bin" / "torchsnap").write_text("ELF")
        self.remote_log = pathlib.Path(self.tree._tmp.name) / "remote.log"
        self.runner_log = pathlib.Path(self.tree._tmp.name) / "runner.log"
        self.ldd_log = pathlib.Path(self.tree._tmp.name) / "ldd.log"
        # Remote commands the eval stub runs, as tiny functions on PATH.
        for name, body in {
            "printenv": 'printf "%s\\n" "$REMOTE_HOME"',
            "getconf": 'printf "%s\\n" "$REMOTE_GETCONF"',
            "ldd": 'printf "%s\\n" "$1" >> "$LDD_ARG_LOG"; printf "%s\\n" "$REMOTE_LDD"; exit "${REMOTE_LDD_RC:-0}"',
        }.items():
            path = self.tree.bin / name
            path.write_text(f"#!/bin/bash\n{body}\n")
            path.chmod(0o755)
        ssh = self.tree.bin / "ssh"
        ssh.write_text(SSH_EVAL)
        ssh.chmod(0o755)
        runner = self.tree.bin / "test-runner"
        runner.write_text(RUNNER)
        runner.chmod(0o755)
        self.env = dict(HOST, **remote_env(),
                        SSH_REMOTE_LOG=str(self.remote_log), RUNNER_LOG=str(self.runner_log),
                        LDD_ARG_LOG=str(self.ldd_log),
                        **{stub_key("docker", "run", "--name", "torchsnap-linux-build-check") + "_OUT": "2.39"})

    def check(self, *args, **extra):
        return self.tree.run("linux-build", "check", *args, env=dict(self.env, **extra))


class CheckArguments(TargetCase):
    def test_no_host_exits_2_before_any_remote_call(self):
        for value in (None, ""):
            with self.subTest(value=value):
                env = dict(self.env)
                if value is None:
                    env.pop("TORCHSNAP_LINUX_HOST")
                else:
                    env["TORCHSNAP_LINUX_HOST"] = value
                for command in ("check", "run-check"):
                    self.tree.log.unlink(missing_ok=True)
                    result = self.tree.run("linux-build", command, env=env)
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("TORCHSNAP_LINUX_HOST is not set", result.stderr)
                    self.assertEqual(self.tree.calls("ssh") + self.tree.calls("rsync"), [])

    def test_without_a_build_exits_1_naming_the_recipe(self):
        shutil.rmtree(self.tree.root / "target" / "linux-build" / "debug")
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("just linux-build", result.stderr)
        self.assertEqual(self.tree.calls("rsync"), [])


class Check(TargetCase):
    def test_passes_with_an_older_glibc_and_every_library_found(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS glibc: needs 2.39, target has 2.41", result.stdout)
        self.assertIn("PASS ldd: all libraries found", result.stdout)

    def test_a_newer_glibc_fails(self):
        result = self.check(**{stub_key("docker", "run", "--name", "torchsnap-linux-build-check") + "_OUT": "2.42"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL glibc: needs 2.42, newer than the target's 2.41", result.stdout)

    def test_versions_compare_as_versions(self):
        result = self.check(**{stub_key("docker", "run", "--name", "torchsnap-linux-build-check") + "_OUT": "2.9"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS glibc", result.stdout)

    def test_a_missing_library_fails_with_its_line(self):
        result = self.check(REMOTE_LDD="\tlibfoo.so.1 => not found")
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL ldd: libfoo.so.1 => not found", result.stdout)

    def test_a_missing_symbol_version_fails_with_its_line(self):
        line = "/x/torchsnap: /lib/libstdc++.so.6: version `GLIBCXX_3.4.33' not found (required by /x/torchsnap)"
        result = self.check(REMOTE_LDD=line)
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"FAIL ldd: {line}", result.stdout)

    def test_a_failing_ldd_fails(self):
        result = self.check(REMOTE_LDD="not a dynamic executable", REMOTE_LDD_RC="1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL ldd", result.stdout)

    def test_copies_the_output_with_rsync_outside_the_runner(self):
        result = self.check(TORCHSNAP_LINUX_RUNNER="test-runner")
        self.assertEqual(result.returncode, 0, result.stderr)
        out = self.tree.root / "target" / "linux-build" / "debug"
        self.assertEqual([row[1:] for row in self.tree.calls("rsync")],
                         [["-a", "--delete", "--stats", f"{out}/", "someone@target.local:linux-build-check/debug/"]])
        self.assertRegex(result.stdout, r"copied target/linux-build/debug to someone@target.local in \d+ s")
        self.assertNotIn("rsync", self.runner_log.read_text())

    def test_remote_commands_use_the_home_lookup_and_absolute_paths(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        remote = self.remote_log.read_text()
        self.assertIn("/home/someone/linux-build-check/debug/bin/torchsnap", remote)
        self.assertNotIn("~", remote)

    def test_a_home_that_is_not_absolute_exits_1(self):
        result = self.check(REMOTE_HOME="welcome banner")
        self.assertEqual(result.returncode, 1)
        self.assertIn("home", result.stderr)

    def test_without_a_runner_the_command_goes_through_bash_c(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        ssh_args = [row[2:] for row in self.tree.calls("ssh")]
        self.assertTrue(all(args[0].startswith("bash -c ") for args in ssh_args), ssh_args)

    def test_with_a_runner_every_command_goes_through_it(self):
        result = self.check(TORCHSNAP_LINUX_RUNNER="test-runner")
        self.assertEqual(result.returncode, 0, result.stderr)
        ssh_args = [row[2:] for row in self.tree.calls("ssh")]
        self.assertTrue(all(args[0].startswith("test-runner ") for args in ssh_args), ssh_args)

    def test_arguments_with_spaces_and_quotes_arrive_intact(self):
        home = "/home/some one/it's $here"
        result = self.check(REMOTE_HOME=home, TORCHSNAP_LINUX_RUNNER="test-runner")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.ldd_log.read_text(), f"{home}/linux-build-check/debug/bin/torchsnap\n")


class TargetRunCheck(TreeCase):
    """`target-run-check` on the target, with systemd, socat and /proc stubbed."""

    def setUp(self):
        super().setUp()
        base = pathlib.Path(self.tree._tmp.name)
        self.home = base / "home"
        build = self.home / "linux-build-check" / "debug" / "bin"
        build.mkdir(parents=True)
        (build / "torchsnap").write_text("ELF")
        (build / "torchsnap").chmod(0o755)
        self.proc = base / "proc"
        (self.proc / "4242").mkdir(parents=True)
        (self.proc / "4242" / "comm").write_text("torchsnap\n")
        (self.proc / "4243").mkdir(parents=True)
        (self.proc / "4243" / "comm").write_text("labwc\n")
        self.cgroups = base / "cgroup"
        unit_dir = self.cgroups / "user.slice" / "linux-build-check.service"
        unit_dir.mkdir(parents=True)
        (unit_dir / "cgroup.procs").write_text("4243\n4242\n")
        self.app_environ("WAYLAND_DISPLAY=wayland-1", "DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/dbus-own",
                         "XDG_CURRENT_DESKTOP=labwc:wlroots")
        for name in ("systemctl", "systemd-run", "socat", "sleep", "dbus-run-session"):
            path = self.tree.bin / name
            path.write_text(STUB)
            path.chmod(0o755)
        self.env = {
            "HOME": str(self.home),
            "TARGET_RUN_CHECK_PROC": str(self.proc),
            "TARGET_RUN_CHECK_CGROUP_ROOT": str(self.cgroups),
            "TARGET_RUN_CHECK_TIMEOUT": "3",
            stub_key("systemctl", "--user", "is-active") + "_RC": "3",
            stub_key("systemctl", "--user", "show", "-p", "ControlGroup") + "_OUT": "/user.slice/linux-build-check.service",
            stub_key("systemctl", "--user", "show-environment") + "_OUT":
                "WAYLAND_DISPLAY=wayland-0\nDBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\nXDG_CURRENT_DESKTOP=GNOME",
            "STUB_SOCAT_OUT": '{"jsonrpc":"2.0","id":1,"result":{"visible":false}}',
        }

    def app_environ(self, *pairs):
        (self.proc / "4242" / "environ").write_bytes(b"\0".join(p.encode() for p in pairs) + b"\0")

    def run_check(self, **extra):
        return self.tree.run("target-run-check", "debug", "1", env=dict(self.env, **extra), bash="bash")

    def test_starts_the_build_isolated_and_gets_its_status(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('PASS status: {"jsonrpc":"2.0","id":1,"result":{"visible":false}}', result.stdout)
        self.assertIn("PASS isolation", result.stdout)
        run = self.tree.calls("systemd-run")[0][1:]
        self.assertEqual(run[:5], ["--user", "--unit", "linux-build-check", "--collect", "--quiet"])
        command = run[run.index("--") + 1:]
        self.assertEqual(command[:2], ["env", "-i"])
        self.assertIn("WLR_BACKENDS=headless", command)
        tail = command[command.index("dbus-run-session"):]
        self.assertEqual(tail[:4], ["dbus-run-session", "--", "labwc", "-C"])
        self.assertEqual(tail[-2:], ["-s", str(self.home / "linux-build-check" / "debug" / "bin" / "torchsnap")])
        self.assertFalse(any(arg.startswith(("DISPLAY=", "WAYLAND_DISPLAY=", "DBUS_SESSION_BUS_ADDRESS="))
                             for arg in command))

    def test_writes_settings_with_flat_keys(self):
        self.run_check()
        settings = self.home / "linux-build-check" / "debug-home" / "data" / "app.torchsnap" / "settings.json"
        self.assertEqual(json.loads(settings.read_text()), {"controlChannel.enabled": True, "welcome.seenRevision": 1})

    def test_stops_and_resets_the_unit(self):
        self.run_check()
        calls = [row[1:] for row in self.tree.calls("systemctl")]
        self.assertIn(["--user", "stop", "linux-build-check"], calls)
        self.assertIn(["--user", "reset-failed", "linux-build-check"], calls)

    def test_no_reply_fails_and_still_stops_the_unit(self):
        result = self.run_check(STUB_SOCAT_OUT="")
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL status: no reply within 3 s", result.stdout)
        self.assertIn(["--user", "stop", "linux-build-check"], [row[1:] for row in self.tree.calls("systemctl")])

    def test_a_running_check_is_not_started_twice(self):
        result = self.run_check(**{stub_key("systemctl", "--user", "is-active") + "_RC": "0"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("already running", result.stderr)
        self.assertEqual(self.tree.calls("systemd-run"), [])

    def test_a_missing_binary_fails_before_starting(self):
        (self.home / "linux-build-check" / "debug" / "bin" / "torchsnap").unlink()
        result = self.run_check()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.tree.calls("systemd-run"), [])

    def test_the_desktop_sessions_display_is_a_leak(self):
        self.app_environ("WAYLAND_DISPLAY=wayland-0", "DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/dbus-own")
        result = self.run_check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL isolation: WAYLAND_DISPLAY", result.stdout)

    def test_the_desktop_sessions_bus_is_a_leak(self):
        self.app_environ("WAYLAND_DISPLAY=wayland-1", "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus")
        result = self.run_check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL isolation: DBUS_SESSION_BUS_ADDRESS", result.stdout)

    def test_a_gnome_desktop_name_is_a_leak(self):
        self.app_environ("WAYLAND_DISPLAY=wayland-1", "XDG_CURRENT_DESKTOP=GNOME")
        result = self.run_check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL isolation: XDG_CURRENT_DESKTOP", result.stdout)

    def test_no_torchsnap_process_fails(self):
        (self.cgroups / "user.slice" / "linux-build-check.service" / "cgroup.procs").write_text("4243\n")
        result = self.run_check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL isolation: no torchsnap process", result.stdout)


class RunCheck(TargetCase):
    def test_copies_build_and_script_then_runs_it_with_the_welcome_revision(self):
        result = self.tree.run("linux-build", "run-check", env=dict(self.env, STUB_TARGET_OUT="PASS status: {}"))
        self.assertEqual(result.returncode, 0, result.stderr)
        rsyncs = [row[1:] for row in self.tree.calls("rsync")]
        out = self.tree.root / "target" / "linux-build" / "debug"
        self.assertIn(["-a", "--delete", "--stats", f"{out}/", "someone@target.local:linux-build-check/debug/"], rsyncs)
        self.assertIn([str(self.tree.root / "tools" / "linux-build" / "target-run-check"),
                       "someone@target.local:linux-build-check/target-run-check"], [r[-2:] for r in rsyncs])
        commands = [shlex.split(shlex.split(line)[2]) for line in self.remote_log.read_text().splitlines()]
        self.assertIn(["/home/someone/linux-build-check/target-run-check", "debug", "1"], commands)

    def test_a_failed_run_check_fails(self):
        result = self.tree.run("linux-build", "run-check",
                               env=dict(self.env, STUB_TARGET_OUT="FAIL status: no reply", STUB_TARGET_RC="1"))
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL status", result.stdout)


@unittest.skipUnless(HAVE_JUST, "just is not installed")
class TargetRecipes(unittest.TestCase):
    def assert_runs(self, args, expected):
        result = just_dry_run(*args)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(expected, [line.strip() for line in result.stderr.splitlines()])

    def test_check(self):
        self.assert_runs(["linux-build-check"], "tools/linux-build/linux-build check")

    def test_check_release(self):
        self.assert_runs(["linux-build-check", "--release"], "tools/linux-build/linux-build check --release")

    def test_run_check(self):
        self.assert_runs(["linux-build-run-check"], "tools/linux-build/linux-build run-check")

    def test_run_check_release(self):
        self.assert_runs(["linux-build-run-check", "--release"], "tools/linux-build/linux-build run-check --release")


if __name__ == "__main__":
    unittest.main()
