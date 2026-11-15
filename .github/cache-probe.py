"""Sorts what a restored Mac cache holds into kinds, writes a file list for each to DIR/<kind>.txt, and prints sizes.

Usage: python3 cache-probe.py DIR
"""
import os, sys, collections

out = sys.argv[1]
home = os.path.expanduser("~")
kinds = collections.defaultdict(list)
sizes = collections.Counter()
units = []

def size(path):
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            try:
                total += os.lstat(os.path.join(root, f)).st_size
            except OSError:
                pass
    return total

for name, path in (("registry-index", f"{home}/.cargo/registry/index"), ("registry-cache", f"{home}/.cargo/registry/cache"), ("git-db", f"{home}/.cargo/git/db")):
    if os.path.isdir(path):
        kinds[name].append(path); sizes[name] += size(path)

build = "target/debug/build"
seen = set()
if os.path.isdir(build):
    for pkg in sorted(os.listdir(build)):
        for unit in sorted(os.listdir(os.path.join(build, pkg))):
            path = os.path.join(build, pkg, unit)
            if not os.path.isdir(path):
                continue
            files = []
            for root, _, fs in os.walk(path):
                files += fs
            if any(f.endswith(".rlib") for f in files):
                kind = "lib-build"
            elif any(f.endswith(".dylib") for f in files):
                kind = "proc-macro"
            elif any(f.startswith("build_script_build") or f.startswith("build-script-build") for f in files):
                kind = "build-script-compile"
            elif any(f.endswith(".rmeta") for f in files):
                kind = "check"
            elif any(f in ("output", "root-output") for f in files) or os.path.isdir(os.path.join(path, "out")):
                kind = "build-script-run"
            else:
                kind = "other-unit"
            if pkg.startswith("libghostty"):
                kind = "ghostty-" + kind
            s = size(path)
            kinds[kind].append(path); sizes[kind] += s; units.append((s, kind, path)); seen.add(path)

rest = 0
for root, dirs, fs in os.walk("target"):
    if root.startswith(build):
        dirs[:] = []
        continue
    for f in fs:
        p = os.path.join(root, f)
        try:
            rest += os.lstat(p).st_size
        except OSError:
            pass
        kinds["target-other"].append(p)
sizes["target-other"] = rest

os.makedirs(out, exist_ok=True)
for kind, paths in kinds.items():
    with open(os.path.join(out, kind + ".txt"), "w") as f:
        f.write("\n".join(paths) + "\n")
print("raw sizes:")
for kind, s in sizes.most_common():
    print(f"  {s/1e6:9.0f} MB  {kind} ({len(kinds[kind])} entries)")
print(f"  {sum(sizes.values())/1e6:9.0f} MB  total")
print("largest units:")
for s, kind, path in sorted(units, reverse=True)[:25]:
    print(f"  {s/1e6:9.0f} MB  {kind}  {path}")
