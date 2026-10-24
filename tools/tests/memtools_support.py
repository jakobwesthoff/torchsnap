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
import socket
import tempfile
import textwrap
import threading
import time

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


# socat as bench-memory calls it (`socat [-T1] - UNIX-CONNECT:PATH`): it
# reads the request from stdin, logs `["socat-request", method, params]`
# and answers. `status` reports `ready: false` for the first
# $STUB_SOCAT_NOT_READY calls (default 0), then `ready: true`; with
# STUB_SOCAT_NEVER_READY set it never does. Every other method gets an
# empty result.
SOCAT_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, pathlib, sys
    request = json.loads(sys.stdin.readline())
    log = pathlib.Path(os.environ["STUB_LOG"])
    with log.open("a") as f:
        f.write(json.dumps(["socat-request", request["method"], request.get("params")]) + "\\n")
    result = {}
    if request["method"] == "status":
        counter = log.with_name("socat-status.count")
        n = int(counter.read_text()) if counter.exists() else 0
        counter.write_text(str(n + 1))
        not_ready = int(os.environ.get("STUB_SOCAT_NOT_READY", "0"))
        ready = n >= not_ready and not os.environ.get("STUB_SOCAT_NEVER_READY")
        result = {"visible": False, "focused": False, "ready": ready}
    elif request["method"] == "info":
        result = {"pid": 0, "version": "0.0.0-test", "display": {"name": "wayland-0", "backend": "wayland"}}
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}))
    """
)

# memsnap for the bench tests: logs its argv; `snapshot -o FILE -l
# LABEL` appends a minimal line, `sample -o FILE` writes a samples file
# when it gets SIGTERM or after `-d` seconds, `pids` prints
# $STUB_MEMSNAP_PIDS.
#
# With $STUB_MEMSNAP_SAMPLE_AFTER_LABEL set, `sample` instead ends 0.5 s
# after a snapshot with that label was logged and then logs
# `["memsnap-sample-end", time]`, so a test can tell whether the bench
# waited for the sampler before its next step: a step that does not wait
# starts within those 0.5 s.
MEMSNAP_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, signal, sys, time
    log_path = os.environ["STUB_LOG"]
    with open(log_path, "a") as log:
        log.write(json.dumps(["memsnap"] + sys.argv[1:]) + "\\n")
    command, args = sys.argv[1], sys.argv[2:]
    def option(*names):
        for i, a in enumerate(args):
            if a in names:
                return args[i + 1]
        return None
    def snapshot_logged(label):
        with open(log_path) as log:
            calls = [json.loads(line) for line in log if line.strip()]
        return any(c[:2] == ["memsnap", "snapshot"] and label in c for c in calls)
    process = {"name": "torchsnap", "pid": 1, "auxiliary": {"phys_footprint": 1048576}, "pss": 1048576}
    if command == "snapshot":
        with open(option("-o"), "a") as f:
            f.write(json.dumps({"metric": "pss", "phase": option("-l"), "processes": [process]}) + "\\n")
    elif command == "sample":
        # Runs until SIGTERM; a fixed duration (macOS) is cut to 0.2 s so
        # the tests do not wait for it.
        stop = []
        signal.signal(signal.SIGTERM, lambda *_: stop.append(True))
        duration = min(float(option("-d") or 0), 0.2)
        after_label = os.environ.get("STUB_MEMSNAP_SAMPLE_AFTER_LABEL")
        start = time.monotonic()
        while not stop:
            if after_label:
                if snapshot_logged(after_label):
                    time.sleep(0.5)
                    break
            elif duration and time.monotonic() - start >= duration:
                break
            time.sleep(0.05)
        with open(option("-o"), "w") as f:
            json.dump({"metric": "pss", "samples": [{"processes": [process]}]}, f)
        if after_label:
            with open(log_path, "a") as log:
                log.write(json.dumps(["memsnap-sample-end", time.time()]) + "\\n")
    elif command == "pids":
        print(os.environ.get("STUB_MEMSNAP_PIDS", ""))
    """
)

# A stand-in for the Torchsnap binary: writes its PID and environment to
# $FAKE_APP_RECORD, prints a line to its log and waits for SIGTERM. With
# FAKE_APP_EXIT set it exits at once with that status.
FAKE_APP = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, signal, sys, time
    with open(os.environ["FAKE_APP_RECORD"], "w") as f:
        json.dump({"pid": os.getpid(), "env": dict(os.environ)}, f)
    print("fake app started", flush=True)
    if os.environ.get("FAKE_APP_EXIT"):
        print("fake app gives up", flush=True)
        sys.exit(int(os.environ["FAKE_APP_EXIT"]))
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    while True:
        time.sleep(0.1)
    """
)

# systemd-run as bench-memory calls it in scope mode: logs its argv,
# skips its options up to `--` and execs the command in place, as
# `systemd-run --scope` does.
SYSTEMD_RUN_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ["STUB_LOG"], "a") as log:
        log.write(json.dumps(sys.argv) + "\\n")
    command = sys.argv[sys.argv.index("--") + 1:]
    os.execvp(command[0], command)
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


class MethodControlSocket:
    """A control socket that answers each request by its method: the
    answers for a method are taken in turn, the last one repeating; a
    method without answers gets an empty result. Records each request
    with the time it arrived."""

    def __init__(self, path, answers=None):
        self.path = str(path)
        self.answers = {method: list(values) for method, values in (answers or {}).items()}
        self.requests = []
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(self.path)
        self.server.listen()
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def answer(self, method):
        values = self.answers.get(method)
        if not values:
            return {"result": {}}
        return values.pop(0) if len(values) > 1 else values[0]

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
                self.requests.append((time.time(), request))
                response = dict(self.answer(request["method"]), jsonrpc="2.0", id=request["id"])
                connection.sendall((json.dumps(response) + "\n").encode())

    def methods(self):
        return [request["method"] for _, request in self.requests]

    def close(self):
        self.server.close()
