# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/build-dmg-background. The tool has no `.py` suffix, so
# it is loaded by path. The scene is plain text and needs nothing; the
# TIFF step runs `sips` and `tiffutil`, so its tests are skipped where
# macOS's tools are missing.

import contextlib
import importlib.machinery
import importlib.util
import io
import pathlib
import shutil
import struct
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zlib

_path = pathlib.Path(__file__).resolve().parent.parent / "build-dmg-background"
_loader = importlib.machinery.SourceFileLoader("build_dmg_background", str(_path))
_spec = importlib.util.spec_from_loader("build_dmg_background", _loader)
dmg = importlib.util.module_from_spec(_spec)
_loader.exec_module(dmg)

SVG = "{http://www.w3.org/2000/svg}"
ASSET_DIR = pathlib.Path(__file__).resolve().parent.parent.parent / "assets/dmg-background"
HAVE_MACOS_TOOLS = bool(shutil.which("sips") and shutil.which("tiffutil"))


def run(*argv):
    """Runs the tool's main without its progress line on stdout."""
    with contextlib.redirect_stdout(io.StringIO()):
        return dmg.main(list(argv))


def write_png(path, width, height, row):
    """An RGB PNG whose every row is `row` (width × 3 bytes), written with
    the standard library."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    path.write_bytes(b"\x89PNG\r\n\x1a\n"
                     + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress((b"\x00" + row) * height))
                     + chunk(b"IEND", b""))


def tiff_layers(path):
    """(width, height) of each image in a TIFF, as `tiffutil -info` lists them."""
    info = subprocess.run(["tiffutil", "-info", str(path)], check=True, capture_output=True, text=True).stdout
    layers = []
    for line in info.splitlines():
        if "Image Width:" in line:
            parts = line.split()
            layers.append((int(parts[parts.index("Width:") + 1]), int(parts[parts.index("Length:") + 1])))
    return layers


class Scene(unittest.TestCase):
    def setUp(self):
        self.svg = dmg.scene_svg()
        self.root = ET.fromstring(self.svg)

    def test_canvas_reaches_past_the_window_on_every_side(self):
        pad = dmg.PAD
        self.assertEqual(self.root.get("viewBox"), f"{-pad} {-pad} {dmg.W + 2 * pad} {dmg.H + 2 * pad}")
        self.assertEqual((self.root.get("width"), self.root.get("height")),
                         (str(dmg.W + 2 * pad), str(dmg.H + 2 * pad)))

    def test_is_the_same_for_every_run(self):
        self.assertEqual(dmg.scene_svg(), self.svg)

    def test_has_seventy_stars_above_the_snow(self):
        circles = self.root.findall(f"{SVG}circle")
        self.assertEqual(len(circles), 70)
        self.assertTrue(all(float(c.get("cy")) <= 300 for c in circles))

    def test_has_the_app_icons_three_aurora_bands(self):
        bands = [p.get("fill") for p in self.root.iter(f"{SVG}polygon") if p.get("fill", "").startswith("#")]
        self.assertEqual(bands, [colour for colour, *_ in dmg.RISE_BANDS])

    def test_snow_crest_runs_above_the_label_line(self):
        snow = next(p for p in self.root.iter(f"{SVG}polygon") if p.get("fill") == "url(#snow)")
        ridge = [tuple(map(float, point.split(","))) for point in snow.get("points").split()[:-2]]
        crests = [y for x, y in ridge if 0 <= x <= dmg.W]
        # The crest waves 11 points around its line, 26 above the labels.
        self.assertLess(max(crests), dmg.LABEL_Y - 26 + 11 + 0.1)
        self.assertGreater(min(crests), dmg.LABEL_Y - 26 - 11 - 0.1)
        self.assertLess(max(crests), dmg.LABEL_Y - 8)

    def test_snow_reaches_the_bottom_edge_and_both_sides(self):
        snow = next(p for p in self.root.iter(f"{SVG}polygon") if p.get("fill") == "url(#snow)")
        points = snow.get("points").split()
        self.assertEqual(points[-2:], [f"{dmg.W + dmg.PAD},{dmg.H + dmg.PAD}", f"{-dmg.PAD},{dmg.H + dmg.PAD}"])

    def test_matches_the_committed_scene(self):
        # The committed scene is what the image model was given; a change
        # here means the scene no longer explains the committed picture.
        self.assertEqual((ASSET_DIR / "inputs/scene.svg").read_text(), self.svg)


class Main(unittest.TestCase):
    def test_scene_step_writes_the_scene_into_inputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(run("scene", "--dir", tmp), 0)
            self.assertEqual((pathlib.Path(tmp) / "inputs/scene.svg").read_text(), dmg.scene_svg())

    def test_unknown_step_is_refused(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            run("png")

    @unittest.skipUnless(HAVE_MACOS_TOOLS, "needs sips and tiffutil (macOS)")
    def test_tiff_step_fails_without_the_models_image(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(subprocess.CalledProcessError):
                run("tiff", "--dir", tmp)
            self.assertFalse((pathlib.Path(tmp) / "dmg-background.tiff").exists())


@unittest.skipUnless(HAVE_MACOS_TOOLS, "needs sips and tiffutil (macOS)")
class Tiff(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self.tmp.name)
        width, height = dmg.MODEL_SIZE
        write_png(self.dir / "model-output.png", width, height, b"\x28\x3c\x78" * width)

    def tearDown(self):
        self.tmp.cleanup()

    def test_holds_the_1x_and_2x_content_area(self):
        run("tiff", "--dir", str(self.dir))
        self.assertEqual(tiff_layers(self.dir / "dmg-background.tiff"),
                         [(dmg.W, dmg.H), (2 * dmg.W, 2 * dmg.H)])

    def test_is_the_same_for_every_run(self):
        run("tiff", "--dir", str(self.dir))
        first = (self.dir / "dmg-background.tiff").read_bytes()
        run("tiff", "--dir", str(self.dir))
        self.assertEqual((self.dir / "dmg-background.tiff").read_bytes(), first)

    def test_keeps_the_middle_of_the_models_canvas(self):
        # A model image whose outer 68 columns per side (the widening, at
        # the model's scale) are red: none of that may reach the TIFF.
        width, height = dmg.MODEL_SIZE
        edge = 68
        red, blue = b"\xff\x00\x00", b"\x28\x3c\x78"
        write_png(self.dir / "model-output.png", width, height, red * edge + blue * (width - 2 * edge) + red * edge)
        run("tiff", "--dir", str(self.dir))
        # The 2x image's first and last columns, read back through sips as
        # a BMP: no pixel there may be strongly red.
        bmp = self.dir / "check.bmp"
        subprocess.run(["sips", "-s", "format", "bmp", str(self.dir / "dmg-background.tiff"), "--out", str(bmp)],
                       check=True, capture_output=True)
        data = bmp.read_bytes()
        offset, = struct.unpack_from("<I", data, 10)
        bmp_width, bmp_height = struct.unpack_from("<ii", data, 18)
        bits, = struct.unpack_from("<H", data, 28)
        step = bits // 8
        stride = (bmp_width * step + 3) & ~3
        for y in range(abs(bmp_height)):
            for x in (0, bmp_width - 1):
                b, g, r = data[offset + y * stride + x * step: offset + y * stride + x * step + 3]
                self.assertLess(r - b, 100, f"red at column {x}, row {y}")


if __name__ == "__main__":
    unittest.main()
