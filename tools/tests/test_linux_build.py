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
import shutil
import subprocess
import tempfile
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
            ["-a", "--delete", "--filter=:- .gitignore", "--exclude=/.git", "--exclude=/.claude",
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
            ["-a", "--delete", "--filter=:- .gitignore", "--exclude=/.git", "--exclude=/.claude",
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


if __name__ == "__main__":
    unittest.main()
