# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for `tools/lint-shell`, the shellcheck gate that `just lint`
# runs, and for its wiring into the recipes, `just doctor` and CI. Each
# script test builds a scratch git repository, runs the script in it and
# reads which files shellcheck checked from the `In <path> line N:`
# headers of its report.

import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
SCRIPT = REPO / "tools/lint-shell"
CI_WORKFLOW = REPO / ".github/workflows/ci.yml"

# shellcheck reports this line as SC2086, so every file that carries it
# and gets checked shows up in the report.
FINDING = "echo $1\n"
CLEAN = 'echo "$1"\n'

# Files the gate checks. Those without a shebang name their dialect, so
# the planted SC2086 is the only reason shellcheck mentions them.
CHECKED = {
    "run": "#!/usr/bin/env bash\n" + FINDING,
    "posix": "#!/bin/sh\n" + FINDING,
    "spaced-shebang": "#! /bin/sh\n" + FINDING,
    "env-split": "#!/usr/bin/env -S bash -eu\n" + FINDING,
    "dash-script": "#!/bin/dash\n" + FINDING,
    "ksh-script": "#!/usr/bin/ksh\n" + FINDING,
    "homebrew-bash": "#!/opt/homebrew/bin/bash\n" + FINDING,
    "crlf": "#!/bin/bash\r\n" + FINDING,
    "lib/helper.sh": "# shellcheck shell=bash\n" + FINDING,
    "lib/helper.bash": "# shellcheck shell=bash\n" + FINDING,
    "lib/helper.dash": "# shellcheck shell=dash\n" + FINDING,
    "lib/helper.ksh": "# shellcheck shell=ksh\n" + FINDING,
    "dir with space/my script.sh": "#!/bin/sh\n" + FINDING,
    "-leading-dash.sh": "#!/bin/sh\n" + FINDING,
}

# Untracked but not ignored, so a new script is checked before `git add`.
CHECKED_UNTRACKED = {
    "new-script": "#!/usr/bin/env bash\n" + FINDING,
}

# Files that would draw a finding if the gate checked them.
SKIPPED = {
    "python-tool": "#!/usr/bin/env python3\n" + FINDING,
    "uv-tool": "#!/usr/bin/env -S uv run --script\n" + FINDING,
    "bun-tool": "#!/usr/bin/env bun\n" + FINDING,
    "zsh-script": "#!/bin/zsh\n" + FINDING,
    "bashful": "#!/usr/bin/env bashful\n" + FINDING,
    "notes.md": "# Notes\n\n#!/bin/bash\n" + FINDING,
    # `read` returns 1 on an empty file; the run must go on past it.
    "empty": "",
}


def require(*tools):
    # A skipped test would read as a pass, so a missing tool fails.
    for tool in tools:
        if shutil.which(tool) is None:
            raise AssertionError(f"{tool} is not on PATH")


def scratch_env(base):
    """The environment for git and the script inside the scratch area.

    The user's git configuration stays out (signing, hooks, global
    excludes), and git does not climb out of `base` to find a repository
    around the platform's temp directory. `TMPDIR` points at an empty
    directory so the tests can see whether the script leaves files.
    """
    env = {key: value for key, value in os.environ.items()
           if key not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE")}
    env["GIT_CONFIG_GLOBAL"] = os.devnull
    env["GIT_CONFIG_NOSYSTEM"] = "1"
    env["GIT_CEILING_DIRECTORIES"] = str(base)
    env["TMPDIR"] = str(base / "tmp")
    return env


def git(base, cwd, *args):
    subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    *args], cwd=cwd, env=scratch_env(base), check=True, capture_output=True)


def write(root, relative, text):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    # Bytes keep the CRLF and missing-newline cases as written.
    path.write_bytes(text.encode())
    path.chmod(0o755)
    return path


def make_scratch():
    """A scratch area with an empty `TMPDIR` and a fresh repository `repo/`."""
    # Resolved, so `GIT_CEILING_DIRECTORIES` matches the path git sees
    # (macOS puts temp directories under the `/var` symlink).
    base = pathlib.Path(tempfile.mkdtemp()).resolve()
    (base / "tmp").mkdir()
    repo = base / "repo"
    repo.mkdir()
    git(base, repo, "init", "-q")
    git(base, repo, "commit", "-q", "--allow-empty", "-m", "start")
    return base, repo


def checked_paths(report):
    return set(re.findall(r"^In (.+) line \d+:$", report, re.MULTILINE))


def bash_major(path):
    if not os.path.exists(path):
        return None
    result = subprocess.run([path, "-c", "echo ${BASH_VERSINFO[0]}"],
                            capture_output=True, text=True)
    return result.stdout.strip()


# macOS ships bash 3.2 as `/bin/bash`, and that is the bash the script's
# `env bash` finds on a Mac without Homebrew bash.
BIN_BASH_IS_3 = bash_major("/bin/bash") == "3"


class LintShellRunner:
    # How the script is started; the bash 3 subclasses run it through
    # `/bin/bash` instead of its shebang.
    launcher = ()

    @classmethod
    def lint(cls, base, cwd, env=None):
        return subprocess.run([*cls.launcher, str(SCRIPT)], cwd=cwd,
                              env=env or scratch_env(base), capture_output=True, text=True)


class DiscoveryTest(LintShellRunner, unittest.TestCase):
    """Which files of one repository the gate hands to shellcheck."""

    @classmethod
    def setUpClass(cls):
        require("git", "shellcheck")
        cls.base, repo = make_scratch()
        cls.addClassCleanup(shutil.rmtree, cls.base)

        for relative, text in {**CHECKED, **SKIPPED}.items():
            write(repo, relative, text)
        write(repo, ".gitignore", "build/\n")
        write(repo, "build/ignored.sh", "#!/bin/sh\n" + FINDING)
        git(cls.base, repo, "add", "-A")
        for relative, text in CHECKED_UNTRACKED.items():
            write(repo, relative, text)

        # A symlink to a script outside the repository.
        outside = write(cls.base, "outside/target.sh", "#!/bin/sh\n" + FINDING)
        (repo / "link.sh").symlink_to(outside)

        # A nested repository and a linked worktree, which git lists as
        # one directory entry each. The main checkout keeps agent
        # worktrees under `.claude/worktrees/`.
        nested = repo / "nested"
        nested.mkdir()
        git(cls.base, nested, "init", "-q")
        write(nested, "n.sh", "#!/bin/sh\n" + FINDING)
        git(cls.base, repo, "worktree", "add", "-q", "--detach", ".claude/worktrees/wt", "HEAD")
        write(repo, ".claude/worktrees/wt/inner.sh", "#!/bin/sh\n" + FINDING)

        cls.result = cls.lint(cls.base, repo)
        cls.checked = checked_paths(cls.result.stdout)

    def test_checks_tracked_shell_scripts_by_shebang_and_extension(self):
        for relative in CHECKED:
            with self.subTest(relative):
                self.assertIn(relative, self.checked, self.result.stdout + self.result.stderr)

    def test_checks_untracked_scripts_that_are_not_ignored(self):
        for relative in CHECKED_UNTRACKED:
            with self.subTest(relative):
                self.assertIn(relative, self.checked, self.result.stdout + self.result.stderr)

    def test_skips_files_that_are_not_shell_scripts(self):
        for relative in SKIPPED:
            with self.subTest(relative):
                self.assertNotIn(relative, self.checked)

    def test_skips_ignored_files(self):
        self.assertNotIn("build/ignored.sh", self.checked)

    def test_skips_symlinks(self):
        self.assertNotIn("link.sh", self.checked)

    def test_skips_nested_repositories_and_worktrees(self):
        self.assertNotIn("nested/n.sh", self.checked)
        self.assertNotIn(".claude/worktrees/wt/inner.sh", self.checked)

    def test_counts_every_checked_file(self):
        expected = len(CHECKED) + len(CHECKED_UNTRACKED)
        self.assertIn(f"lint-shell: checking {expected} files", self.result.stdout)


class ExitStatusTest(LintShellRunner, unittest.TestCase):
    """The gate's exit status and what it leaves behind."""

    def setUp(self):
        require("git", "shellcheck")
        self.base, self.repo = make_scratch()
        self.addCleanup(shutil.rmtree, self.base)

    def assert_tmpdir_empty(self):
        self.assertEqual(list((self.base / "tmp").iterdir()), [])

    def test_passes_on_clean_scripts(self):
        write(self.repo, "ok.sh", "#!/bin/sh\n" + CLEAN)
        result = self.lint(self.base, self.repo)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("lint-shell: checking 1 files", result.stdout)
        self.assert_tmpdir_empty()

    def test_fails_on_a_finding(self):
        write(self.repo, "ok.sh", "#!/bin/sh\n" + CLEAN)
        write(self.repo, "bad.sh", "#!/bin/sh\n" + FINDING)
        result = self.lint(self.base, self.repo)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(checked_paths(result.stdout), {"bad.sh"})
        self.assert_tmpdir_empty()

    def test_passes_without_shell_scripts(self):
        write(self.repo, "README.md", "# Readme\n")
        result = self.lint(self.base, self.repo)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("lint-shell: no shell scripts found", result.stdout)

    def test_checks_a_script_whose_only_line_is_the_shebang_without_newline(self):
        write(self.repo, "only-shebang", "#!/bin/bash")
        result = self.lint(self.base, self.repo)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("lint-shell: checking 1 files", result.stdout)

    def test_skips_tracked_files_deleted_from_disk(self):
        write(self.repo, "ok.sh", "#!/bin/sh\n" + CLEAN)
        write(self.repo, "gone.sh", "#!/bin/sh\n" + CLEAN)
        git(self.base, self.repo, "add", "-A")
        (self.repo / "gone.sh").unlink()
        result = self.lint(self.base, self.repo)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("lint-shell: checking 1 files", result.stdout)

    def test_checks_the_whole_repository_from_a_subdirectory(self):
        write(self.repo, "bad.sh", "#!/bin/sh\n" + FINDING)
        (self.repo / "sub").mkdir()
        result = self.lint(self.base, self.repo / "sub")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(checked_paths(result.stdout), {"bad.sh"})

    def test_fails_outside_a_git_work_tree(self):
        plain = self.base / "plain"
        plain.mkdir()
        write(plain, "bad.sh", "#!/bin/sh\n" + FINDING)
        result = self.lint(self.base, plain)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not a git repository", result.stderr)

    def test_fails_with_a_pointer_to_doctor_without_shellcheck(self):
        write(self.repo, "ok.sh", "#!/bin/sh\n" + CLEAN)
        # `PATH` holds bash alone: the check comes before any other tool.
        bin_dir = self.base / "bin"
        bin_dir.mkdir()
        (bin_dir / "bash").symlink_to(shutil.which("bash"))
        env = scratch_env(self.base)
        env["PATH"] = str(bin_dir)
        result = self.lint(self.base, self.repo, env)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("shellcheck not found", result.stderr)
        self.assertIn("just doctor", result.stderr)


@unittest.skipUnless(BIN_BASH_IS_3, "/bin/bash is not bash 3")
class DiscoveryBash3Test(DiscoveryTest):
    launcher = ("/bin/bash",)


@unittest.skipUnless(BIN_BASH_IS_3, "/bin/bash is not bash 3")
class ExitStatusBash3Test(ExitStatusTest):
    launcher = ("/bin/bash",)


def just(*args):
    require("just")
    return subprocess.run(["just", *args], cwd=REPO, check=True, capture_output=True, text=True)


class WiringTest(unittest.TestCase):
    def test_lint_runs_lint_shell(self):
        recipes = json.loads(just("--dump", "--dump-format", "json").stdout)["recipes"]
        dependencies = [dependency["recipe"] for dependency in recipes["lint"]["dependencies"]]
        self.assertIn("lint-shell", dependencies)

    def test_lint_shell_runs_the_script(self):
        # `just --dry-run` prints the commands it would run on stderr.
        lines = just("--dry-run", "lint-shell").stderr.splitlines()
        self.assertIn("tools/lint-shell", [line.strip() for line in lines])

    def test_doctor_lists_shellcheck_as_required(self):
        doctor = just("--show", "doctor").stdout
        start = doctor.index('echo "Required:"')
        end = doctor.index('echo ""', start)
        required = [line.strip() for line in doctor[start:end].splitlines()]
        self.assertTrue(any(line.startswith("check shellcheck ") for line in required), required)

    def test_ci_installs_shellcheck(self):
        installs = [line for line in CI_WORKFLOW.read_text().splitlines() if "brew install" in line]
        self.assertTrue(any("shellcheck" in line.split() for line in installs), installs)


if __name__ == "__main__":
    unittest.main()
