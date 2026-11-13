#!/usr/bin/env python3
"""Puts the marketing site together as it's published: what's in site/, without its README, and with the
link-preview tags that need the site's address, which the deploy passes in. The guide's pages aren't its to make:
tools/guide builds them into OUT/docs afterwards.

    tools/site/assemble.py OUT [ADDRESS]

OUT must not exist. With no ADDRESS, the page goes out without those tags."""
import shutil
import sys
from pathlib import Path

SITE = Path(__file__).resolve().parents[2] / "site"
# Where the page says the tags go. A page without it is refused, so the page and this can't drift apart unnoticed.
MARKER = "    <!-- The deploy adds the tags that need the site's address here. -->\n"


def tags(address: str) -> str:
    address = address.rstrip("/")
    return (f'    <meta property="og:url" content="{address}/" />\n'
            f'    <meta property="og:image" content="{address}/assets/card.png" />\n'
            '    <meta property="og:image:width" content="1200" />\n'
            '    <meta property="og:image:height" content="630" />\n'
            f'    <link rel="canonical" href="{address}/" />\n')


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(sys.argv[1])
    address = sys.argv[2] if len(sys.argv) == 3 else ""
    if address and not address.startswith("https://"):
        print("the address must start with https://", file=sys.stderr)
        return 1
    shutil.copytree(SITE, out, ignore=shutil.ignore_patterns("README.md", ".DS_Store"))
    index = out / "index.html"
    page = index.read_text()
    if page.count(MARKER) != 1:
        print("index.html doesn't say where the tags that need the address go", file=sys.stderr)
        return 1
    index.write_text(page.replace(MARKER, tags(address) if address else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
