"""Tests for ico.py, against the app's own drawings.

    python3 -m unittest discover -s tools/package
"""

from __future__ import annotations

import struct
import unittest
from pathlib import Path

import ico

ICONSET = Path(__file__).resolve().parent.parent.parent / "apps" / "desktop" / "icons" / "working.iconset"


class Ico(unittest.TestCase):
    def test_the_icon_holds_each_drawing_whole_under_its_size(self) -> None:
        pngs = [(name, (ICONSET / name).read_bytes()) for name in ico.DRAWINGS]

        made = ico.ico(pngs)

        reserved, kind, count = struct.unpack("<HHH", made[:6])
        self.assertEqual((reserved, kind, count), (0, 1, len(pngs)))
        sides = []
        for index in range(count):
            width, height, _, _, planes, bits, size, offset = struct.unpack("<BBBBHHII", made[6 + 16 * index:][:16])
            picture = made[offset:offset + size]
            self.assertIn(picture, [png for _, png in pngs])
            self.assertEqual((width, height, planes, bits), (ico.side_of(picture, "") % 256,) * 2 + (1, 32))
            sides.append(ico.side_of(picture, ""))
        self.assertEqual(sides, [16, 32, 64, 128, 256])
        self.assertEqual(len(made), 6 + 16 * count + sum(len(png) for _, png in pngs))

    def test_a_picture_an_icon_cant_hold_is_refused(self) -> None:
        too_wide = (ICONSET / "icon_512x512.png").read_bytes()
        with self.assertRaisesRegex(ValueError, "at most 256"):
            ico.ico([("icon_512x512.png", too_wide)])
        with self.assertRaisesRegex(ValueError, "isn't a PNG"):
            ico.ico([("notes.txt", b"not a picture, but long enough to hold a header")])


if __name__ == "__main__":
    unittest.main()
