# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/stage-bundled-archives. The tool has no `.py` suffix,
# so it is loaded by path. Every archive is written here with explicit
# entry dates and every file gets an explicit mtime, so no test waits
# for the clock.

import contextlib
import importlib.machinery
import importlib.util
import io
import os
import pathlib
import tempfile
import unittest
import zipfile

_path = pathlib.Path(__file__).resolve().parent.parent / "stage-bundled-archives"
_loader = importlib.machinery.SourceFileLoader("stage_bundled_archives", str(_path))
_spec = importlib.util.spec_from_loader("stage_bundled_archives", _loader)
stage_tool = importlib.util.module_from_spec(_spec)
_loader.exec_module(stage_tool)

OLD = 978307200  # 2001-01-01T00:00:00Z
ENTRIES = {"manifest.toml": b'[gadget]\nid = "calculator"\n', "calculator.wasm": b"\0asm" + b"x" * 64}


def write_archive(path, entries, date_time=(2001, 1, 1, 0, 0, 0)):
    """A zip with the given entries, every entry dated `date_time`."""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in entries.items():
            archive.writestr(zipfile.ZipInfo(name, date_time), data)


def run(*argv):
    """Runs the tool's main without its progress lines on stdout."""
    with contextlib.redirect_stdout(io.StringIO()):
        return stage_tool.main([str(arg) for arg in argv])


class StageBundledArchivesTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self._tmp.name)
        self.stage = self.dir / "bundled-gadgets"
        self.stage.mkdir()
        self.built = self.dir / "gadgets"
        self.built.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def staged(self, name, entries):
        """An archive already in the stage, its file and the stage set to OLD."""
        path = self.stage / name
        write_archive(path, entries)
        os.utime(path, (OLD, OLD))
        os.utime(self.stage, (OLD, OLD))
        return path

    def fresh(self, name, entries):
        """An archive as the gadget build just wrote it: same name, later entry dates."""
        path = self.built / name
        write_archive(path, entries, date_time=(2026, 10, 23, 18, 30, 0))
        return path

    def test_unchanged_archive_keeps_its_staged_copy_and_mtime(self):
        staged = self.staged("calculator.torchsnap", ENTRIES)
        before = staged.read_bytes()
        new = self.fresh("calculator.torchsnap", ENTRIES)
        self.assertNotEqual(new.read_bytes(), before)
        run(self.stage, new)
        self.assertEqual(staged.read_bytes(), before)
        self.assertEqual(staged.stat().st_mtime, OLD)
        self.assertEqual(self.stage.stat().st_mtime, OLD)

    def test_changed_entry_content_replaces_the_staged_copy(self):
        staged = self.staged("calculator.torchsnap", ENTRIES)
        new = self.fresh("calculator.torchsnap", {**ENTRIES, "calculator.wasm": b"\0asm" + b"y" * 64})
        run(self.stage, new)
        self.assertEqual(staged.read_bytes(), new.read_bytes())
        self.assertGreater(staged.stat().st_mtime, OLD)

    def test_added_entry_replaces_the_staged_copy(self):
        staged = self.staged("calculator.torchsnap", ENTRIES)
        new = self.fresh("calculator.torchsnap", {**ENTRIES, "frontend/dist/launcher.js": b"export {}"})
        run(self.stage, new)
        self.assertEqual(staged.read_bytes(), new.read_bytes())

    def test_removed_entry_replaces_the_staged_copy(self):
        staged = self.staged("calculator.torchsnap", ENTRIES)
        new = self.fresh("calculator.torchsnap", {"manifest.toml": ENTRIES["manifest.toml"]})
        run(self.stage, new)
        self.assertEqual(staged.read_bytes(), new.read_bytes())

    def test_new_archive_is_copied(self):
        new = self.fresh("awake.torchsnap", ENTRIES)
        run(self.stage, new)
        self.assertEqual((self.stage / "awake.torchsnap").read_bytes(), new.read_bytes())

    def test_unlisted_archive_and_stray_entries_are_removed(self):
        self.staged("calculator.torchsnap", ENTRIES)
        self.staged("retired.torchsnap", ENTRIES)
        (self.stage / ".stray").write_text("left over")
        (self.stage / "leftover-dir").mkdir()
        (self.stage / "leftover-dir/file").write_text("left over")
        new = self.fresh("calculator.torchsnap", ENTRIES)
        run(self.stage, new)
        self.assertEqual(sorted(p.name for p in self.stage.iterdir()), ["calculator.torchsnap"])

    def test_corrupt_staged_copy_is_replaced(self):
        staged = self.stage / "calculator.torchsnap"
        staged.write_bytes(b"not a zip archive")
        new = self.fresh("calculator.torchsnap", ENTRIES)
        run(self.stage, new)
        self.assertEqual(staged.read_bytes(), new.read_bytes())

    def test_staged_copy_with_damaged_entry_data_is_replaced(self):
        # Stored (uncompressed) entries put the data verbatim after the
        # local header, so one flipped byte damages the entry while the
        # central directory, with its CRC-32, stays as it was.
        staged = self.stage / "calculator.torchsnap"
        with zipfile.ZipFile(staged, "w", zipfile.ZIP_STORED) as archive:
            for name, data in ENTRIES.items():
                archive.writestr(zipfile.ZipInfo(name, (2001, 1, 1, 0, 0, 0)), data)
        raw = bytearray(staged.read_bytes())
        offset = raw.index(b"x" * 64)
        raw[offset] ^= 0xFF
        staged.write_bytes(bytes(raw))
        new = self.fresh("calculator.torchsnap", ENTRIES)
        run(self.stage, new)
        self.assertEqual(staged.read_bytes(), new.read_bytes())

    def test_unreadable_new_archive_is_still_copied(self):
        self.staged("calculator.torchsnap", ENTRIES)
        new = self.built / "calculator.torchsnap"
        new.write_bytes(b"truncated")
        run(self.stage, new)
        self.assertEqual((self.stage / "calculator.torchsnap").read_bytes(), b"truncated")

    def test_no_archives_empties_the_stage(self):
        self.staged("calculator.torchsnap", ENTRIES)
        run(self.stage)
        self.assertEqual(list(self.stage.iterdir()), [])

    def test_missing_stage_dir_is_created(self):
        stage = self.dir / "target/bundled-gadgets"
        new = self.fresh("awake.torchsnap", ENTRIES)
        run(stage, new)
        self.assertEqual((stage / "awake.torchsnap").read_bytes(), new.read_bytes())


if __name__ == "__main__":
    unittest.main()
