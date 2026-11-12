#!/usr/bin/env python3
"""Makes the Windows icon, an .ico, from the drawings the Mac's icon is made from.

  ico.py ICONSET OUT.ico

An .ico holds a picture for each size, and since Windows Vista each can be a PNG as it is, so this only writes the
directory in front of them. It takes the drawing made for each size on a display that isn't Retina, and the 64-pixel
one, which the iconset holds as 32 points at twice the scale. An .ico can't hold one wider than 256.

The app's is checked in as apps/desktop/icons/AppIcon.ico, which the installer has, and which the desktop app's build
script puts in the program, so neither needs Python or knows the drawings. test_ico.py fails when it isn't what this
makes from working.iconset now. To make it again, from the repo's root:

  python3 tools/package/ico.py apps/desktop/icons/working.iconset apps/desktop/icons/AppIcon.ico
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

DRAWINGS = ("icon_16x16.png", "icon_32x32.png", "icon_32x32@2x.png", "icon_128x128.png", "icon_256x256.png")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def side_of(png: bytes, name: str) -> int:
    """How many pixels wide a square PNG is, from its header."""
    if png[:8] != PNG_SIGNATURE or png[12:16] != b"IHDR":
        raise ValueError(f"{name} isn't a PNG")
    width, height = struct.unpack(">II", png[16:24])
    if width != height or not 0 < width <= 256:
        raise ValueError(f"{name} is {width} by {height}, and an icon is square and at most 256 across")
    return width


def ico(pngs: list[tuple[str, bytes]]) -> bytes:
    """The .ico that holds each PNG, smallest first."""
    pictures = sorted((side_of(png, name), png) for name, png in pngs)
    directory, offset = [], 6 + 16 * len(pictures)
    for side, png in pictures:
        # A side of 256 is written as 0. One plane, 32 bits a pixel, and no palette.
        directory.append(struct.pack("<BBBBHHII", side % 256, side % 256, 0, 0, 1, 32, len(png), offset))
        offset += len(png)
    return struct.pack("<HHH", 0, 1, len(pictures)) + b"".join(directory) + b"".join(png for _, png in pictures)


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: ico.py ICONSET OUT.ico", file=sys.stderr)
        return 1
    iconset, out = Path(sys.argv[1]), Path(sys.argv[2])
    try:
        out.write_bytes(ico([(name, (iconset / name).read_bytes()) for name in DRAWINGS]))
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
