# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Tests for tools/build-app-icons. The tool has no `.py` suffix, so it
# is loaded by path. Everything but the raster step (rsvg-convert,
# ictool, magick) runs with the standard library, so these tests cover
# it without those tools.

import contextlib
import importlib.machinery
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest
import xml.etree.ElementTree as ET

_path = pathlib.Path(__file__).resolve().parent.parent / "build-app-icons"
_loader = importlib.machinery.SourceFileLoader("build_app_icons", str(_path))
_spec = importlib.util.spec_from_loader("build_app_icons", _loader)
icons = importlib.util.module_from_spec(_spec)
_loader.exec_module(icons)

# A stand-in for the flattened emblem: the head's black, the cream face,
# and the white eye highlight, as the real file has them.
OWL = [("#120a07", "M0 0H10V10H0Z"), ("#fdf8e4", "M2 2H8V8H2Z"), ("#ffffff", "M4 4H5V5H4Z")]
FLAT_SVG = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
            + "".join(f'<path fill="{f}" d="{d}"/>' for f, d in OWL) + "</svg>\n")


def svg_root(text):
    return ET.fromstring(text)


def build(*argv):
    """Runs the tool's main without its progress lines on stdout."""
    with contextlib.redirect_stdout(io.StringIO()):
        return icons.main(list(argv))


class Tones(unittest.TestCase):
    def test_ink_prints_dark_tones_solid_and_the_face_as_paper(self):
        self.assertEqual(icons.ink_opacity("#120a07"), 1.0)
        self.assertEqual(icons.ink_opacity("#fdf8e4"), 0.0)
        self.assertEqual(icons.ink_opacity("#ffffff"), 0.0)

    def test_ink_grades_the_middle_tones(self):
        value = icons.ink_opacity("#fa8a06")
        self.assertGreater(value, 0.0)
        self.assertLess(value, 1.0)

    def test_negative_is_the_other_way_round(self):
        self.assertEqual(icons.negative_opacity("#120a07"), 0.0)
        self.assertEqual(icons.negative_opacity("#ffffff"), 1.0)
        self.assertGreater(icons.negative_opacity("#fdf8e4"), 0.9)


class Owl(unittest.TestCase):
    def test_reads_fill_and_path_per_colour(self):
        with tempfile.TemporaryDirectory() as work:
            flat = pathlib.Path(work) / "flat.svg"
            flat.write_text(FLAT_SVG)
            self.assertEqual(icons.read_owl(flat), OWL)

    def test_colour_style_keeps_every_colour(self):
        paths = svg_root(icons.svg(icons.owl_paths(OWL, "colour"))).findall("{http://www.w3.org/2000/svg}path")
        self.assertEqual([p.get("fill") for p in paths], ["#120a07", "#fdf8e4", "#ffffff"])

    def test_ink_style_drops_tones_that_print_nothing(self):
        paths = svg_root(icons.svg(icons.owl_paths(OWL, "ink"))).findall("{http://www.w3.org/2000/svg}path")
        self.assertEqual(len(paths), 1)
        self.assertEqual(paths[0].get("fill"), icons.NAVY)
        self.assertEqual(paths[0].get("d"), OWL[0][1])

    def test_negative_style_prints_the_light_tones_in_pale_ink(self):
        paths = svg_root(icons.svg(icons.owl_paths(OWL, "negative"))).findall("{http://www.w3.org/2000/svg}path")
        self.assertEqual([p.get("d") for p in paths], [OWL[1][1], OWL[2][1]])
        self.assertTrue(all(p.get("fill") == icons.PALE for p in paths))

    def test_transform_centres_the_drawing_box_and_scales_it_to_the_width(self):
        tx, ty, s = icons.owl_transform(412, (412, 412))
        box_centre_x = tx + s * (icons.BOX_X + icons.BOX_W / 2)
        box_centre_y = ty + s * (icons.BOX_Y + icons.BOX_H / 2)
        self.assertAlmostEqual(box_centre_x, icons.CANVAS / 2)
        self.assertAlmostEqual(box_centre_y, icons.CANVAS / 2)
        self.assertAlmostEqual(s * icons.BOX_W, icons.CANVAS / 2)


class Sky(unittest.TestCase):
    def test_stars_are_the_same_for_a_seed(self):
        self.assertEqual(icons.stars(10, 400, 5), icons.stars(10, 400, 5))
        self.assertNotEqual(icons.stars(10, 400, 5), icons.stars(10, 400, 6))

    def test_stars_stay_on_the_canvas_above_their_limit(self):
        root = svg_root(icons.svg(icons.stars(50, 420, 9)))
        circles = root.findall("{http://www.w3.org/2000/svg}circle")
        self.assertEqual(len(circles), 50)
        for c in circles:
            self.assertTrue(0 <= float(c.get("cx")) <= icons.CANVAS)
            self.assertTrue(0 <= float(c.get("cy")) <= 420 * icons.K + 0.1)

    def test_ribbons_draw_one_blurred_band_per_colour(self):
        root = svg_root(icons.svg(icons.ribbons(icons.AURORA_BANDS)))
        ns = "{http://www.w3.org/2000/svg}"
        self.assertIsNotNone(root.find(f"{ns}filter/{ns}feGaussianBlur"))
        polygons = root.findall(f"{ns}g/{ns}polygon")
        self.assertEqual([p.get("fill") for p in polygons], [band[0] for band in icons.AURORA_BANDS])

    def test_ribbons_run_past_both_tile_edges(self):
        root = svg_root(icons.svg(icons.ribbons(icons.RISE_BANDS)))
        polygon = root.find("{http://www.w3.org/2000/svg}g/{http://www.w3.org/2000/svg}polygon")
        xs = [float(point.split(",")[0]) for point in polygon.get("points").split()]
        self.assertLess(min(xs), 0)
        self.assertGreater(max(xs), icons.CANVAS)


class Manifest(unittest.TestCase):
    def setUp(self):
        self.candidates = icons.candidates(OWL)

    def owl_layer(self, manifest):
        return manifest["groups"][0]["layers"][0]

    def test_single_owl_image_uses_the_plain_name_only(self):
        candidate = dict(self.candidates["aurora"], owl={"any": "<g/>"})
        layer = self.owl_layer(icons.manifest(candidate))
        self.assertEqual(layer["image-name"], "owl.svg")
        self.assertNotIn("image-name-specializations", layer)

    def test_owl_per_appearance_uses_only_the_specializations(self):
        layer = self.owl_layer(icons.manifest(self.candidates["ink"]))
        self.assertNotIn("image-name", layer)
        self.assertEqual(layer["image-name-specializations"], [
            {"value": "owl.svg"},
            {"appearance": "dark", "value": "owl-dark.svg"},
            {"appearance": "tinted", "value": "owl-tinted.svg"},
        ])

    def test_every_candidate_shows_the_negative_owl_in_clear_and_tinted(self):
        for key, candidate in self.candidates.items():
            with self.subTest(key=key):
                self.assertNotIn(icons.NAVY, candidate["owl"]["tinted"])
                self.assertIn(icons.PALE, candidate["owl"]["tinted"])

    def test_aurora_candidates_keep_the_colour_owl_in_dark(self):
        for key in ("aurora", "aurora-rise"):
            with self.subTest(key=key):
                layer = self.owl_layer(icons.manifest(self.candidates[key]))
                self.assertEqual([s.get("appearance") for s in layer["image-name-specializations"]], [None, "tinted"])

    def test_candidates_are_the_kept_set(self):
        self.assertEqual(set(self.candidates), {"aurora", "ink", "aurora-rise", "aurora-ember", "aurora-rise-ember",
                                                "aurora-rise-dusk"})

    def test_warm_candidates_keep_the_layout_of_their_blue_original(self):
        for warm, blue in (("aurora-ember", "aurora"), ("aurora-rise-ember", "aurora-rise"),
                           ("aurora-rise-dusk", "aurora-rise")):
            with self.subTest(warm=warm):
                self.assertEqual(self.candidates[warm]["owl"], self.candidates[blue]["owl"])
                self.assertEqual(self.candidates[warm]["sky_tinted"], self.candidates[blue]["sky_tinted"])
                self.assertNotEqual(self.candidates[warm]["sky"], self.candidates[blue]["sky"])

    def test_recoloured_keeps_the_waves(self):
        bands = icons.recoloured(icons.RISE_BANDS, icons.DUSK_COLOURS)
        self.assertEqual([b[0] for b in bands], list(icons.DUSK_COLOURS))
        self.assertEqual([b[1:] for b in bands], [b[1:] for b in icons.RISE_BANDS])

    def sky_layer(self, sky_tinted):
        candidate = dict(self.candidates["aurora"], sky_tinted=sky_tinted)
        return icons.manifest(candidate)["groups"][1]["layers"][0]

    def test_aurora_sky_is_faint_and_the_rising_one_shown(self):
        self.assertEqual(self.candidates["aurora"]["sky_tinted"], "faint")
        self.assertEqual(self.candidates["aurora-rise"]["sky_tinted"], "shown")

    def test_sky_shown_in_clear_and_tinted_has_no_overrides(self):
        layer = self.sky_layer("shown")
        self.assertNotIn("opacity-specializations", layer)
        self.assertNotIn("hidden-specializations", layer)

    def test_faint_sky_lowers_its_opacity_only_in_clear_and_tinted(self):
        self.assertEqual(self.sky_layer("faint")["opacity-specializations"],
                         [{"value": 1.0}, {"appearance": "tinted", "value": icons.SKY_FAINT}])

    def test_hidden_sky_is_hidden_only_in_clear_and_tinted(self):
        self.assertEqual(self.sky_layer("hidden")["hidden-specializations"],
                         [{"value": False}, {"appearance": "tinted", "value": True}])

    def test_dark_background_uses_only_the_fill_specializations(self):
        manifest = icons.manifest(self.candidates["ink"])
        self.assertNotIn("fill", manifest)
        self.assertEqual(manifest["fill-specializations"][1]["appearance"], "dark")
        self.assertEqual(manifest["fill-specializations"][1]["value"]["linear-gradient"][0],
                         "srgb:0.09020,0.13333,0.23137,1.00000")

    def test_single_background_uses_the_plain_fill(self):
        manifest = icons.manifest(self.candidates["aurora"])
        self.assertEqual(manifest["fill"]["linear-gradient"][0], "srgb:0.03137,0.06275,0.15294,1.00000")
        self.assertNotIn("fill-specializations", manifest)

    def test_sky_group_lies_below_the_owl_and_only_where_there_is_a_sky(self):
        aurora = icons.manifest(self.candidates["aurora"])
        self.assertEqual([g["name"] for g in aurora["groups"]], ["Owl", "Sky"])
        self.assertEqual(aurora["groups"][1]["layers"][0]["image-name"], "sky.png")
        self.assertEqual([g["name"] for g in icons.manifest(self.candidates["ink"])["groups"]], ["Owl"])

    def test_owl_is_not_glass(self):
        for candidate in self.candidates.values():
            self.assertFalse(self.owl_layer(icons.manifest(candidate))["glass"])


class Build(unittest.TestCase):
    def test_no_raster_writes_the_vector_files_and_documents(self):
        with tempfile.TemporaryDirectory() as work:
            work = pathlib.Path(work)
            flat = work / "flat.svg"
            flat.write_text(FLAT_SVG)
            out = work / "app-icon"
            self.assertEqual(build("--flat", str(flat), "--out", str(out), "--no-raster"), 0)

            for key in icons.candidates(OWL):
                svg_root((out / key / f"{key}.svg").read_text())
                manifest = json.loads((out / key / f"{key}.icon" / "icon.json").read_text())
                self.assertIn("groups", manifest)
                for name in icons.owl_files(icons.candidates(OWL)[key]).values():
                    svg_root((out / key / f"{key}.icon" / "Assets" / name).read_text())
            self.assertTrue((out / "aurora" / "sky.svg").exists())
            self.assertFalse((out / "ink" / "sky.svg").exists())
            self.assertFalse((out / "previews").exists())

    def test_rebuilding_replaces_the_document(self):
        with tempfile.TemporaryDirectory() as work:
            work = pathlib.Path(work)
            flat = work / "flat.svg"
            flat.write_text(FLAT_SVG)
            out = work / "app-icon"
            build("--flat", str(flat), "--out", str(out), "--no-raster")
            stale = out / "ink" / "ink.icon" / "Assets" / "stale.svg"
            stale.write_text("<svg/>")
            build("--flat", str(flat), "--out", str(out), "--no-raster")
            self.assertFalse(stale.exists())


if __name__ == "__main__":
    unittest.main()
