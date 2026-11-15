"""Splits a restored Mac cache into what the current builds use and what they don't, from the JSON cargo prints for
each unit, fresh or not, and prints the size of each part.

Usage: python3 cache-probe.py DIR MESSAGES...
"""
import json, os, sys, collections

out, messages = sys.argv[1], sys.argv[2:]
build = os.path.abspath("target/debug/build")
used = set()
for path in messages:
    for line in open(path):
        try:
            m = json.loads(line)
        except ValueError:
            continue
        paths = list(m.get("filenames") or []) + ([m["out_dir"]] if m.get("out_dir") else [])
        if m.get("executable"):
            paths.append(m["executable"])
        for p in paths:
            p = os.path.abspath(p)
            if p.startswith(build + "/"):
                parts = p[len(build) + 1:].split("/")
                used.add(os.path.join(build, *parts[:2]))

def size(path):
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            try:
                total += os.lstat(os.path.join(root, f)).st_size
            except OSError:
                pass
    return total

lists = collections.defaultdict(list)
sizes = collections.Counter()
for pkg in sorted(os.listdir(build)):
    for unit in sorted(os.listdir(os.path.join(build, pkg))):
        path = os.path.join(build, pkg, unit)
        kind = "build-used" if path in used else "build-unused"
        lists[kind].append(path)
        sizes[kind] += size(path)
top = collections.Counter()
for name in os.listdir("target"):
    p = os.path.join("target", name)
    if name == "debug":
        for sub in os.listdir(p):
            if sub != "build":
                top[f"target/debug/{sub}"] += size(os.path.join(p, sub)) if os.path.isdir(os.path.join(p, sub)) else os.lstat(os.path.join(p, sub)).st_size
    else:
        top[p] += size(p) if os.path.isdir(p) else os.lstat(p).st_size
for p in top:
    lists["target-other"].append(os.path.abspath(p))
sizes["target-other"] = sum(top.values())
home = os.path.expanduser("~")
for p in (f"{home}/.cargo/registry/index", f"{home}/.cargo/registry/cache", f"{home}/.cargo/git/db"):
    lists["cargo-home"].append(p)
    sizes["cargo-home"] += size(p)
os.makedirs(out, exist_ok=True)
for kind, paths in lists.items():
    with open(os.path.join(out, kind + ".txt"), "w") as f:
        f.write("\n".join(paths) + "\n")
print(f"units the builds use: {len(used)}")
for kind, s in sizes.most_common():
    print(f"  {s/1e6:8.0f} MB raw  {kind} ({len(lists[kind])} entries)")
print("outside target/debug/build:")
for p, s in top.most_common(10):
    print(f"  {s/1e6:8.0f} MB raw  {p}")
