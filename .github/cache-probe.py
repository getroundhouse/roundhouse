"""Splits a restored Mac cache by which of the Mac job's builds use each unit in target/debug: clippy only, the tests
only, both, or neither, from the JSON cargo prints for each unit, fresh or not. Writes each part's paths to OUT and
prints each part's size.

Usage: python3 cache-probe.py OUT NAME=MESSAGES...
"""
import collections, json, os, re, sys

out, builds = sys.argv[1], [a.split("=", 1) for a in sys.argv[2:]]
debug = os.path.abspath("target/debug")
unit = re.compile(r"^(?:lib)?.+-([0-9a-f]{16})(?:\..*)?$")


def hash_of(name):
    m = unit.match(name)
    return m.group(1) if m else None


used = collections.defaultdict(set)
for name, path in builds:
    for line in open(path):
        try:
            m = json.loads(line)
        except ValueError:
            continue
        paths = list(m.get("filenames") or [])
        for key in ("out_dir", "executable"):
            if m.get(key):
                paths.append(m[key])
        for p in paths:
            rel = os.path.relpath(os.path.abspath(p), debug).split(os.sep)
            # deps/libfoo-HASH.rlib, build/foo-HASH/out, build/foo-HASH/build-script-build
            h = hash_of(rel[1]) if len(rel) > 1 and rel[0] in ("deps", "build") else None
            if h:
                used[h].add("clippy" if name == "clippy" else "tests")


def kind(h):
    users = used.get(h, set())
    if users == {"clippy"}:
        return "clippy-only"
    if users == {"tests"}:
        return "tests-only"
    return "both" if users else "neither"


def size(path):
    if os.path.isfile(path) or os.path.islink(path):
        return os.lstat(path).st_size
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            try:
                total += os.lstat(os.path.join(root, f)).st_size
            except OSError:
                pass
    return total


lists, sizes = collections.defaultdict(list), collections.Counter()


def add(kind, path):
    lists[kind].append(path)
    sizes[kind] += size(path)


for sub in ("deps", "build"):
    d = os.path.join(debug, sub)
    for name in sorted(os.listdir(d)) if os.path.isdir(d) else []:
        h = hash_of(name)
        add(kind(h) if h else "other", os.path.join(d, name))
for name in os.listdir("target"):
    p = os.path.abspath(os.path.join("target", name))
    if name != "debug":
        add("other", p)
for name in os.listdir(debug):
    if name not in ("deps", "build"):
        add("other", os.path.join(debug, name))
home = os.path.expanduser("~")
for p in (f"{home}/.cargo/registry/index", f"{home}/.cargo/registry/cache", f"{home}/.cargo/git/db"):
    add("cargo-home", p)

os.makedirs(out, exist_ok=True)
for k, paths in lists.items():
    with open(os.path.join(out, k + ".txt"), "w") as f:
        f.write("\n".join(paths) + "\n")
print(f"units the builds name: {len(used)}")
for k, s in sizes.most_common():
    print(f"  {s / 1e6:8.0f} MB raw  {k} ({len(lists[k])} entries)")
