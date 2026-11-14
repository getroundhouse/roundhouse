"""Says which units of one cargo unit graph a build of another wouldn't already have made.

A unit is keyed by everything that goes into its artifact's hash that the graph shows: package, target, profile
(without its name, which cargo leaves out of the hash), platform, mode, features, and the keys of what it depends on.
"""
import json
import sys
from functools import lru_cache

def keys(path):
    units = json.load(open(path))["units"]

    @lru_cache(maxsize=None)
    def key(i):
        u = units[i]
        profile = {k: v for k, v in u["profile"].items() if k != "name"}
        deps = tuple(sorted((d["extern_crate_name"], key(d["index"])) for d in u["dependencies"]))
        target = (u["target"]["name"], tuple(u["target"]["kind"]), u["target"]["src_path"])
        return hash((u["pkg_id"], target, json.dumps(profile, sort_keys=True), u["platform"], u["mode"],
                     tuple(u["features"]), deps))

    return {key(i): units[i] for i in range(len(units))}

made = keys(sys.argv[1])
for path in sys.argv[2:]:
    wanted = keys(path)
    missing = [u for k, u in wanted.items() if k not in made]
    names = sorted({u["pkg_id"].split("#")[-1].split("@")[0] if "#" in u["pkg_id"] else u["pkg_id"] for u in missing})
    print(f"{path.split('/')[-1]}: {len(missing)} of {len(wanted)} units not made by {sys.argv[1].split('/')[-1]}")
    if missing:
        print("   ", ", ".join(names)[:600])
