"""Exits 0 if every Cargo.toml that differs between two commits differs only in its package's description, its
comments or its layout, and 1 if one differs in more, is new or is gone, or if anything goes wrong. mac.yml uses it to
skip the Mac, which only a change that could build differently there needs.

Usage: python3 only-descriptions-changed.py BASE [HEAD]
"""

import subprocess
import sys
import tomllib


def changed(base, head):
    """The paths of the Cargo.toml files that differ between the two commits."""
    names = subprocess.run(
        ["git", "diff", "--name-only", "--no-renames", "-z", base, head, "--", ":(glob)**/Cargo.toml"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [path for path in names.split("\0") if path]


def meaning(commit, path):
    """What the Cargo.toml at path says in commit, but for its package's description, or None if it isn't there."""
    shown = subprocess.run(["git", "show", f"{commit}:{path}"], capture_output=True, text=True)
    if shown.returncode != 0:
        return None
    manifest = tomllib.loads(shown.stdout)
    manifest.get("package", {}).pop("description", None)
    return manifest


def main(base, head="HEAD"):
    for path in changed(base, head):
        if meaning(base, path) != meaning(head, path):
            print(f"{path} changed more than its description.")
            return 1
    print("No Cargo.toml changed more than its description.")
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
