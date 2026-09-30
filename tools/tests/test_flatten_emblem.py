# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/flatten-emblem. The tool needs skia-pathops and
# fonttools, which `just test-tools` (standard library only, as on CI)
# does not have, so there these tests are skipped;
# `just test-app-icon-tools` runs them with both.

import contextlib
import importlib.machinery
import importlib.util
import io
import pathlib
import unittest
import xml.etree.ElementTree as ET

try:
    import pathops
    from fontTools.svgLib.path import parse_path
except ImportError:
    pathops = None


def load_tool():
    path = pathlib.Path(__file__).resolve().parent.parent / "flatten-emblem"
    loader = importlib.machinery.SourceFileLoader("flatten_emblem", str(path))
    spec = importlib.util.spec_from_loader("flatten_emblem", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def area(d):
    path = pathops.Path()
    parse_path(d, path.getPen())
    return abs(path.area)


def paths_of(svg_text):
    root = ET.fromstring(svg_text)
    return [(p.get("fill"), p.get("d")) for p in root.iter("{http://www.w3.org/2000/svg}path")]


@unittest.skipIf(pathops is None, "needs skia-pathops and fonttools (just test-app-icon-tools)")
class Flatten(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tool = load_tool()

    def test_later_shapes_cut_their_area_out_of_earlier_ones(self):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">'
               '<path fill="#000000" d="M0 0H40V40H0Z"/><path fill="#ffffff" d="M20 20H60V60H20Z"/></svg>')
        paths = paths_of(self.tool.flattened_svg(svg))
        self.assertEqual([fill for fill, _ in paths], ["#000000", "#ffffff"])
        self.assertAlmostEqual(area(paths[0][1]), 40 * 40 - 20 * 20)
        self.assertAlmostEqual(area(paths[1][1]), 40 * 40)

    def test_parts_of_one_fill_become_one_path(self):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">'
               '<path fill="#000000" d="M0 0H10V10H0Z"/><path fill="#ffffff" d="M50 0H60V10H50Z"/>'
               '<path fill="#000000" d="M80 0H90V10H80Z"/></svg>')
        paths = paths_of(self.tool.flattened_svg(svg))
        self.assertEqual([fill for fill, _ in paths], ["#000000", "#ffffff"])
        self.assertAlmostEqual(area(paths[0][1]), 200)

    def test_group_transform_and_inherited_fill_are_applied(self):
        # The emblem's own form: a group that moves the origin to the
        # bottom and flips y, at a tenth of the path units.
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">'
               '<g transform="translate(0,100) scale(0.1,-0.1)" fill="#123456">'
               '<path d="M0 0H200V200H0Z"/></g></svg>')
        (fill, d), = paths_of(self.tool.flattened_svg(svg))
        self.assertEqual(fill, "#123456")
        path = pathops.Path()
        parse_path(d, path.getPen())
        self.assertEqual(tuple(round(v) for v in path.bounds), (0, 80, 20, 100))

    def test_clip_paths_are_applied(self):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">'
               '<defs><clipPath id="half"><path d="M0 0H50V100H0Z"/></clipPath></defs>'
               '<path fill="#000000" clip-path="url(#half)" d="M0 0H100V100H0Z"/></svg>')
        (_, d), = paths_of(self.tool.flattened_svg(svg))
        self.assertAlmostEqual(area(d), 50 * 100)

    def test_keeps_the_view_box(self):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
               '<path fill="#000000" d="M0 0H1V1H0Z"/></svg>')
        self.assertEqual(ET.fromstring(self.tool.flattened_svg(svg)).get("viewBox"), "0 0 1024 1024")

    def test_main_rejects_wrong_arguments(self):
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(self.tool.main([]), 1)


if __name__ == "__main__":
    unittest.main()
