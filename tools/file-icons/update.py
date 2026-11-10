#!/usr/bin/env python3
"""Replaces apps/desktop/file-icons/ with the icons, the manifest, the colours and the license of a pinned release of
Catppuccin's icons.

The desktop app draws these icons beside files and folders, and gives each name the icon the manifest does, so they
match what an editor that uses them shows. The release is the extension Catppuccin publishes on GitHub, since that's
where the manifest is built, and it has to match the checksum pinned here. Only the icons a name can get are kept:
those the manifest gives a file's name or extension, and the plain file, folder and open folder.

Each icon is kept with its colours named rather than filled in, as `var(--vscode-ctp-blue)`, and `palette.json` says
what each name is in the Mocha flavour, which `build.rs` fills in. The palette is read from the release's own Mocha
icons, and the release is refused unless filling it in gives back every one of them exactly, so the app draws what
Catppuccin does. Each icon is also checked for anything a picture shouldn't hold, such as a script or a link out,
before anything is written.

To update, set VERSION and INTEGRITY to the new release's, from https://github.com/catppuccin/vscode-icons/releases,
run this, then `cargo test -p desktop`, and accept the gallery's screens if the icons look different.
"""
import base64
import hashlib
import io
import json
import pathlib
import re
import sys
import tempfile
import urllib.request
import xml.etree.ElementTree as ElementTree
import zipfile

VERSION = "1.26.0"
INTEGRITY = "sha512-0te0Ryg3vFrCPE1RKnwUL1pKlqyrZvqfEhoDXK3eUFCpbxZQ8yIb3dHYDFBkmWaB/XI71fOnHgQlfA1pf3kPJw=="
RELEASE = (
    f"https://github.com/catppuccin/vscode-icons/releases/download/v{VERSION}/catppuccin-vsc-icons-{VERSION}.vsix"
)
FLAVOR = "mocha"

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "apps/desktop/file-icons"
MANIFEST = "theme.json"
PALETTE = "palette.json"

# What the app gives a name an icon by, which are the only icons it can show.
TABLES = ("fileNames", "fileExtensions")
DEFAULTS = ("file", "folder", "folderExpanded")

ICON_NAME = re.compile(r"[a-z0-9_][a-z0-9_.-]*")
COLOUR = re.compile(r"var\(--vscode-ctp-(\w+)\)")
HEX = re.compile(r"#[0-9a-f]{6}")
# The elements a drawing of an icon is made of: those Catppuccin's are, and the few more a drawing like them would use.
# Anything else, such as a script, text, a style sheet, a picture from elsewhere, or a `<use>`, which can repeat a part
# of the drawing until it takes minutes to draw, is more than an icon needs, so an icon with one is turned away rather
# than trusted. `apps/desktop/build.rs` checks the same, with the same limits, so a change to one belongs in both.
DRAWING = {
    "svg", "g", "path", "circle", "ellipse", "rect", "line", "polyline", "polygon", "defs", "clipPath", "mask",
    "linearGradient", "radialGradient", "stop", "title", "desc",
}
# Far more than any of Catppuccin's needs, whose biggest is about 3 KB, with 18 elements.
MAX_ICON_BYTES = 16 * 1024
MAX_ELEMENTS = 64
# A reference within the drawing, such as `url(#a)` for a gradient, which is the only kind an icon needs.
LOCAL_URL = re.compile(r"url\(\s*['\"]?#", re.IGNORECASE)


def fetch():
    request = urllib.request.Request(RELEASE, headers={"User-Agent": "file-icons-update"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read()
    algorithm, expected = INTEGRITY.split("-", 1)
    got = base64.b64encode(hashlib.new(algorithm, data).digest()).decode()
    if got != expected:
        sys.exit(f"{RELEASE} doesn't match the checksum pinned for {VERSION}: got {algorithm}-{got}")
    return data


def unpack(data):
    """The license, the manifest, and each icon with its colours named and as the flavour draws it, read from the
    release without writing any of it out."""
    named, flavored = {}, {}
    with zipfile.ZipFile(io.BytesIO(data)) as release:
        try:
            license_text = release.read("extension/LICENSE.txt")
            manifest = release.read(f"extension/dist/{FLAVOR}/theme.json")
        except KeyError as e:
            sys.exit(f"{RELEASE} is missing {e}")
        for entry in release.namelist():
            for prefix, icons in (("extension/dist/unflavored/", named), (f"extension/dist/{FLAVOR}/icons/", flavored)):
                if entry.startswith(prefix) and entry.endswith(".svg") and "/" not in entry.removeprefix(prefix):
                    icons[entry.removeprefix(prefix).removesuffix(".svg")] = release.read(entry)
    if not named or not flavored:
        sys.exit(f"{RELEASE} is missing its icons")
    return license_text, manifest, named, flavored


def check(license_text, manifest, named, flavored):
    """The icons a name can get, each checked, and the palette that colours them as the flavour does. Exits, naming
    every problem, if any is found."""
    if not license_text.startswith(b"MIT License"):
        sys.exit("the license isn't MIT any more, so it has to be read before these can ship")
    theme = json.loads(manifest)
    definitions = theme["iconDefinitions"]
    wanted = {icon for table in TABLES for icon in theme[table].values()} | {theme[key] for key in DEFAULTS}
    problems, used = [], {}
    for name in sorted(wanted):
        # The app finds an icon by its name, so each has to be in the file of that name.
        file = definitions.get(name, {}).get("iconPath", "").rsplit("/", 1)[-1]
        if not ICON_NAME.fullmatch(name) or file != f"{name}.svg":
            problems.append(f"{name}'s icon is in an odd file: {file or 'none'}")
        elif name not in named or name not in flavored:
            problems.append(f"{name}'s icon isn't in the release")
        elif (problem := unsafe(named[name])) is not None:
            problems.append(f"{name} {problem}")
        else:
            used[name] = named[name]
    palette, mismatched = colours(used, flavored)
    problems += mismatched
    if problems:
        sys.exit("\n".join(problems))
    return used, palette


def colours(used, flavored):
    """What each colour name is in the flavour, read by lining each icon up with the flavour's, and the icons that
    filling the names in doesn't turn into the flavour's exactly."""
    palette, problems = {}, []
    for name, svg in used.items():
        parts = COLOUR.split(svg.decode())
        pattern = "".join(re.escape(part) if i % 2 == 0 else "(#[0-9a-fA-F]{6})" for i, part in enumerate(parts))
        match = re.fullmatch(pattern, flavored[name].decode(), re.S)
        if match is None:
            problems.append(f"{name} isn't the {FLAVOR} icon with its colours filled in")
            continue
        for colour, value in zip(parts[1::2], match.groups()):
            if palette.setdefault(colour, value.lower()) != value.lower():
                problems.append(f"{colour} is both {palette[colour]} and {value.lower()} in {FLAVOR}")
    for name, svg in used.items():
        filled = COLOUR.sub(lambda m: palette.get(m.group(1), m.group(0)), svg.decode())
        if filled.lower() != flavored[name].decode().lower():
            problems.append(f"{name} doesn't come out as the {FLAVOR} icon")
    if any(not HEX.fullmatch(value) for value in palette.values()):
        problems.append(f"the palette has a colour that isn't one: {palette}")
    return dict(sorted(palette.items())), problems


def unsafe(svg):
    """Why an icon is more than a drawing, or None if it isn't."""
    if not svg.lstrip().startswith(b"<svg"):
        return "isn't an SVG"
    if len(svg) > MAX_ICON_BYTES:
        return f"is {len(svg)} bytes, more than {MAX_ICON_BYTES}"
    # Entities can make a small file expand into a huge one, and no icon needs them, or a comment or an instruction.
    if b"<!" in svg or b"<?" in svg:
        return "declares something, or has a comment or an instruction"
    try:
        root = ElementTree.fromstring(svg)
    except ElementTree.ParseError as e:
        return f"isn't well formed: {e}"
    if sum(1 for _ in root.iter()) > MAX_ELEMENTS:
        return f"has more than {MAX_ELEMENTS} elements"
    for element in root.iter():
        tag = element.tag.rsplit("}", 1)[-1]
        if tag not in DRAWING:
            return f"has a <{tag}>, which isn't part of a drawing"
        for name, value in element.attrib.items():
            name = name.rsplit("}", 1)[-1]
            if name.lower().startswith("on"):
                return f"has an {name} handler"
            if name == "href" and not value.startswith("#"):
                return f"links to {value}"
            if "url(" in value.lower() and not all(LOCAL_URL.match(value, at) for at in find_all(value.lower(), "url(")):
                return f"refers to {value}"
    return None


def find_all(text, part):
    at = text.find(part)
    while at != -1:
        yield at
        at = text.find(part, at + 1)


def write(license_text, manifest, palette, icons):
    """Writes everything beside the old directory, then swaps it in with two renames, moving the old one into the
    scratch directory first, so a failure at any point leaves one whole directory or the other."""
    with tempfile.TemporaryDirectory(dir=OUT.parent) as scratch:
        new = pathlib.Path(scratch) / "file-icons"
        new.mkdir()
        (new / "LICENSE").write_bytes(license_text)
        (new / MANIFEST).write_bytes(manifest)
        (new / PALETTE).write_text(json.dumps(palette, indent=2) + "\n")
        for name, svg in icons.items():
            (new / f"{name}.svg").write_bytes(svg)
        if not OUT.exists():
            new.rename(OUT)
            return
        old = pathlib.Path(scratch) / "old"
        OUT.rename(old)
        try:
            new.rename(OUT)
        except BaseException:
            old.rename(OUT)
            raise


def main():
    license_text, manifest, named, flavored = unpack(fetch())
    icons, palette = check(license_text, manifest, named, flavored)
    write(license_text, manifest, palette, icons)
    print(f"wrote {len(icons)} icons and {len(palette)} colours from Catppuccin {VERSION} to {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
