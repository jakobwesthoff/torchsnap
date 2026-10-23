# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Shared pieces for the tests of the memory tools (memsnap, memsnap-linux,
# memsnap-report, bench-memory): a fake /proc tree for memsnap-linux and
# stub commands put first on PATH. Nothing here measures or starts a real
# app; every PID a stub prints is one the test passes in.

import json
import os
import pathlib
import shutil
import tempfile
import textwrap

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
TOOLS = REPO / "tools"


class FakeProc:
    """A directory laid out like the parts of /proc that memsnap-linux
    reads: per PID `stat`, `cmdline`, `smaps_rollup`, `fd/` and
    `fdinfo/`, and the system-wide `meminfo`."""

    def __init__(self):
        self.root = pathlib.Path(tempfile.mkdtemp(prefix="fake-proc-"))
        self.meminfo(
            MemTotal=8251728, MemAvailable=6315488, Shmem=859808, CmaTotal=65536,
            CmaFree=336, SwapTotal=2097136, SwapFree=1337200,
        )

    def meminfo(self, **fields_kb):
        lines = [f"{name}:{value:>16} kB" for name, value in fields_kb.items()]
        (self.root / "meminfo").write_text("\n".join(lines) + "\n")

    def process(self, pid, ppid, comm, cmdline=None, state="S", utime=0, stime=0,
                rss_kb=0, pss_kb=0, pss_anon_kb=0, pss_file_kb=0, pss_shmem_kb=0,
                swap_pss_kb=0, smaps=True):
        """Adds a process. `cmdline` is a list of arguments; None leaves
        the file empty, as for a kernel thread or a zombie."""
        directory = self.root / str(pid)
        (directory / "fd").mkdir(parents=True)
        (directory / "fdinfo").mkdir()
        # Fields after the command name, from field 3 (state) on; utime
        # and stime are fields 14 and 15.
        rest = [state, str(ppid)] + ["0"] * 9 + [str(utime), str(stime)] + ["0"] * 37
        (directory / "stat").write_text(f"{pid} ({comm}) {' '.join(rest)}\n")
        (directory / "comm").write_text(comm[:15] + "\n")
        (directory / "cmdline").write_bytes(b"".join(a.encode() + b"\0" for a in cmdline or []))
        if smaps:
            (directory / "smaps_rollup").write_text(
                textwrap.dedent(
                    f"""\
                    5555dfac0000-7fffcd770000 ---p 00000000 00:00 0                          [rollup]
                    Rss:            {rss_kb:>8} kB
                    Pss:            {pss_kb:>8} kB
                    Pss_Dirty:             0 kB
                    Pss_Anon:       {pss_anon_kb:>8} kB
                    Pss_File:       {pss_file_kb:>8} kB
                    Pss_Shmem:      {pss_shmem_kb:>8} kB
                    Shared_Clean:          0 kB
                    Private_Dirty:         0 kB
                    Swap:                  0 kB
                    SwapPss:        {swap_pss_kb:>8} kB
                    Locked:                0 kB
                    """
                )
            )
        return directory

    def fd(self, pid, fd, link, fdinfo):
        """Adds an open file descriptor: its link target and its fdinfo
        text."""
        directory = self.root / str(pid)
        os.symlink(link, directory / "fd" / str(fd))
        (directory / "fdinfo" / str(fd)).write_text(textwrap.dedent(fdinfo))

    def cleanup(self):
        shutil.rmtree(self.root)


# A stub appends its argv to $STUB_LOG as one JSON line, prints
# $STUB_<NAME>_STDOUT and exits with $STUB_<NAME>_EXIT (NAME in upper case,
# `-` as `_`), the same contract as the testbed tests' stubs.
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

# footprint writes the `-j FILE` it is given: one record per `-p PID`,
# with STUB_FOOTPRINT_BYTES as each process's footprint.
FOOTPRINT_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(sys.argv) + "\\n")
    args = sys.argv[1:]
    pids = [int(args[i + 1]) for i, a in enumerate(args) if a == "-p"]
    output = args[args.index("-j") + 1]
    size = int(os.environ.get("STUB_FOOTPRINT_BYTES", "1048576"))
    processes = [{"name": f"process-{pid}", "pid": pid, "auxiliary": {"phys_footprint": size}} for pid in pids]
    with open(output, "w") as f:
        json.dump({"processes": processes}, f)
    """
)


class Stubs:
    """A temporary directory of stub commands and the log they write."""

    def __init__(self, names=(), custom=None):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="memtools-stubs-"))
        self.log = self.dir / "calls.jsonl"
        for name in names:
            self.add(name, STUB)
        for name, content in (custom or {}).items():
            self.add(name, content)

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

    def environment(self, **extra):
        environment = dict(os.environ)
        environment["PATH"] = f"{self.dir}{os.pathsep}{os.environ['PATH']}"
        environment["STUB_LOG"] = str(self.log)
        environment.update(extra)
        return environment

    def cleanup(self):
        shutil.rmtree(self.dir)

