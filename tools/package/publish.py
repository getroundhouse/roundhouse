#!/usr/bin/env python3
"""Publishes a packaged release as an update, so that the only feed the release workflow signs is one it computed.

The app trusts the update feed because the workflow signed it, so the workflow never signs what it reads from the
bucket. It reads the published feed, uses it only once its signature matches the app's key, adds this release's
packages to it, and signs and uploads exactly that. Whoever can write to the bucket can then only offer apps a feed
the workflow signed, such as an older one, and can't get a feed of theirs signed.

Each step takes `--channel` first, for which platform's releases it publishes: `macos-arm64`, `win-amd64` for Windows
on amd64, `win-arm64`, `linux-amd64` for Linux on amd64, or `linux-arm64`. It's macos-arm64 if it's left out. A platform
has a feed of its own, `releases.<channel>.json`, and all are in the one bucket.

  publish.py prepare --store STORE --key KEY --version VERSION --work DIR
      Reads the published feed and checks its signature. If it lists VERSION, a run already published it: this makes
      sure apps get that feed, and prints `published`. Otherwise it refuses a VERSION that isn't newer than every
      published release, which no app would take. It puts the feed in DIR/base, and in DIR/releases with its newest
      full package, checked against it, for vpk to make a delta from, and prints `new`. On a channel that took over
      from an older one and has published nothing, the feed in DIR/base is the older channel's, so the releases it
      keeps carry over, and DIR/releases is left empty, since no app could use a delta from a package of the older
      channel.
  publish.py merge --version VERSION --keep COUNT --worker-versions FILE --work DIR
      Writes the feed to sign and prints its path: the feed in DIR/base and the packages vpk made for VERSION in
      DIR/releases, checked against their files, with VERSION's full package saying its two floors, from FILE, which
      `tools/build.sh --package` wrote beside the binaries: which versions of the worker protocol its hub speaks, and
      the oldest release it updates from, if it says one. On a channel that publishes the hub as an archive of its
      own, it also says which archive that is. It refuses a release that would leave an app on the newest published
      one no release to step to, as `steps_from` says. It leaves out releases older than the newest COUNT, but for
      the newest of each floor, as its full package, since that's the release an app steps to while it can't take a
      newer one, so the update stops none of its workers and its hub can migrate its database.
  publish.py bridge --version VERSION --work DIR
      For a channel that took over from an older one, writes the older channel's feed to sign, listing only the full
      package of VERSION the feed in DIR/feed has and, on a channel whose apps step, the full packages that feed keeps
      for them to step to, and prints its path. Prints nothing for any other channel.
  publish.py upload --store STORE --key KEY --version VERSION --work DIR
      Uploads the new packages, the hub's archive and what people download, then the signed feed, reads the feed
      back, then the older channel's feed, if `bridge` wrote one, and then deletes the packages and archives that left
      the feed.
  publish.py check --url URL --key KEY --version VERSION --work DIR
      Checks that URL, where apps read the feed, serves the one in DIR/feed, VERSION's full package and hub archive,
      and the older channel's feed, if this run published one, with each full package and hub archive it lists. A host
      with a cache can take a while to, so it tries for a minute.

STORE is s3://BUCKET, with S3_ENDPOINT, S3_REGION and the AWS CLI's usual credentials, or a directory, to try it
without a bucket. KEY is the app's public key, in the form `crates/update-feed/src/signature.rs` says.
`test_publish.py` runs this against a directory, and serves it for `check`. Needs Python 3.9 or newer, and OpenSSL 3,
as `feed-signature.sh` does.

The feed and its signature are two uploads, so a run that stops between them leaves a pair that doesn't match. Each
run uploads the pair under `staging/` first, so whichever of the two matches its signature and is newer becomes the
base, and running again finishes the job.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

if sys.version_info < (3, 9):
    sys.exit("error: publish.py needs Python 3.9 or newer")

HERE = Path(__file__).resolve().parent
PACK_ID = "Roundhouse"
VERSION = re.compile(r"^\d+\.\d+\.\d+$")
# On a release's full package in the feed, which versions of the worker protocol its hub speaks, as
# `{"oldest": 3, "newest": 4}`. Velopack reads only the fields it knows, so an app that doesn't read this reads the feed
# as before.
WORKER_VERSIONS = "WorkerVersions"
# On a release's full package in the feed, the oldest release it updates from, as `"0.1.300"`, if it says one, since
# its hub can't migrate the database of a release before it. An app doesn't take a release that doesn't update from its
# own. Velopack doesn't read it either.
UPDATES_FROM = "UpdatesFrom"
# On a release's full package in the feed, the archive a hub on a machine of its own is installed and updated from, as
# `docs/decisions/hub-package.md` says, described as the feed describes a package: `FileName`, `Size`, `SHA1` and
# `SHA256`. The feed's signature covers it, so a hub can check what it downloads. Velopack doesn't read it either.
HUB = "Hub"


def windows(channel: str) -> dict[str, str]:
    """What people download for Windows: the installer, and a portable zip."""
    return {f"{PACK_ID}-{channel}-Setup.exe": "application/octet-stream",
            f"{PACK_ID}-{channel}-Portable.zip": "application/zip"}


def linux(channel: str) -> dict[str, str]:
    """What people download for Linux: the AppImage, which is the app."""
    return {f"{PACK_ID}-{channel}.AppImage": "application/octet-stream"}


# What people download for each channel, which vpk makes anew for each release under the same names.
CHANNELS = {
    "macos-arm64": {PACK_ID + "-macos-arm64-Portable.zip": "application/zip"},
    "win-amd64": windows("win-amd64"),
    "win-arm64": windows("win-arm64"),
    "linux-amd64": linux("linux-amd64"),
    "linux-arm64": linux("linux-arm64"),
}


# The channels whose apps step through each break in the worker protocol: those whose hubs keep their workers through
# an update, as KEEPS_WORKERS in crates/hub/src/hub/coordinator.rs has it. A Windows hub's workers stop with it, so its
# apps take the newest. tools/update-steps checks the same ones in the merge queue.
STEPS = {"macos-arm64", "linux-amd64", "linux-arm64"}


# The channels whose releases publish the hub as an archive of its own, those where `roundhouse hub install` installs it
# as a service. Each archive is `Roundhouse-<version>-<channel>-hub.tar.gz`, as `hub_archive` in vpk.sh makes it, and
# the newest is also what people download, without the version.
HUBS = {"macos-arm64", "linux-amd64", "linux-arm64"}


# The channel each one took over from. An app looks for updates in the channel it was packaged for, and a hub installed
# as a service in the one it was built for, so one from before the change still reads the older channel's feed. Each
# release also signs that feed, listing its own full package, which was packaged for the newer channel, so such an app
# or hub takes it and looks there from then on. Where apps step, it also lists the full packages the newer channel keeps
# for them to step to, so one with older workers at work steps as it would have. Those can be the older channel's,
# since the newer channel's first release starts from that one's feed. amd64 Windows was `win` before 0.1.245, and the
# Mac was `osx`, Velopack's default, before `macos-arm64`. *Unconfirmed.*
TAKES_OVER = {"win-amd64": "win", "macos-arm64": "osx"}


def use_channel(channel: str) -> None:
    """Chooses which platform's releases every step below publishes. A run publishes one."""
    global CHANNEL, FEED, STAGED_FEED, DOWNLOADS, OLDER_FEED  # noqa: PLW0603, since a run has one, chosen as it starts
    CHANNEL = channel
    FEED = f"releases.{channel}.json"
    STAGED_FEED = "staging/" + FEED
    DOWNLOADS = CHANNELS[channel]
    OLDER_FEED = f"releases.{TAKES_OVER[channel]}.json" if channel in TAKES_OVER else None


use_channel("macos-arm64")


class Refused(Exception):
    """Something that publishing mustn't go past."""


# Where releases are published.


class DirStore:
    """A directory standing in for the bucket."""

    def __init__(self, root: str) -> None:
        self.root = Path(root)

    def get(self, key: str) -> bytes | None:
        path = self.root / key
        return path.read_bytes() if path.exists() else None

    def size(self, key: str) -> int | None:
        path = self.root / key
        return path.stat().st_size if path.exists() else None

    def put(self, key: str, source: Path, content_type: str) -> None:
        del content_type
        path = self.root / key
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, path)

    def delete(self, key: str) -> None:
        (self.root / key).unlink(missing_ok=True)


class S3Store:
    """A bucket, through the AWS CLI and its S3 API, which no cache sits in front of."""

    def __init__(self, bucket: str) -> None:
        self.base = ["aws", "s3api", "--endpoint-url", os.environ["S3_ENDPOINT"], "--region", os.environ["S3_REGION"]]
        self.bucket = bucket
        # B2's advice for the AWS CLI, whose newer default checksums not every S3-compatible store takes.
        self.env = dict(os.environ, AWS_REQUEST_CHECKSUM_CALCULATION="when_required",
                        AWS_RESPONSE_CHECKSUM_VALIDATION="when_required")

    def _run(self, *args: str) -> subprocess.CompletedProcess[str]:
        command = [*self.base, *args, "--bucket", self.bucket]
        return subprocess.run(command, env=self.env, capture_output=True, text=True, check=False)

    def get(self, key: str) -> bytes | None:
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "object"
            done = self._run("get-object", "--key", key, str(path))
            if done.returncode != 0:
                if "(NoSuchKey)" in done.stderr:
                    return None
                raise Refused(f"couldn't read {key}: {done.stderr.strip()}")
            return path.read_bytes()

    def size(self, key: str) -> int | None:
        done = self._run("head-object", "--key", key)
        if done.returncode != 0:
            if "(404)" in done.stderr or "(NoSuchKey)" in done.stderr:
                return None
            raise Refused(f"couldn't look up {key}: {done.stderr.strip()}")
        return int(json.loads(done.stdout)["ContentLength"])

    def put(self, key: str, source: Path, content_type: str) -> None:
        # Not cached, since a run that stopped early uploads a package again, and the feed changes in place.
        done = self._run("put-object", "--key", key, "--body", str(source), "--content-type", content_type,
                         "--cache-control", "no-cache")
        if done.returncode != 0:
            raise Refused(f"couldn't upload {key}: {done.stderr.strip()}")

    def delete(self, key: str) -> None:
        done = self._run("delete-object", "--key", key)
        if done.returncode != 0:
            raise Refused(f"couldn't delete {key}: {done.stderr.strip()}")


def open_store(name: str) -> DirStore | S3Store:
    if name.startswith("s3://"):
        return S3Store(name.removeprefix("s3://"))
    return DirStore(name)


# The feed.


def signed(feed: bytes, signature: bytes, key: str) -> bool:
    """Whether `signature` is the app's key's signature of exactly `feed`, which `feed-signature.sh` checks."""
    with tempfile.TemporaryDirectory() as work:
        path = Path(work) / FEED
        path.write_bytes(feed)
        Path(str(path) + ".sig").write_bytes(signature)
        done = subprocess.run([str(HERE / "feed-signature.sh"), "verify", str(path), key], capture_output=True,
                              check=False)
        return done.returncode == 0


def parse_version(version: str) -> tuple[int, ...]:
    if not VERSION.match(version):
        raise Refused(f"{version} isn't a version this publishes, which is three numbers")
    return tuple(int(part) for part in version.split("."))


def version_of(asset: dict) -> tuple[int, ...]:
    return parse_version(asset["Version"])


def newest(assets: list[dict]) -> tuple[int, ...]:
    return max((version_of(asset) for asset in assets), default=())


def assets_of(feed: bytes) -> list[dict]:
    return json.loads(feed)["Assets"]


def published(store: DirStore | S3Store, key: str, feed_name: str | None = None) -> tuple[bytes, bytes] | None:
    """The published feed, this channel's unless `feed_name` names another, and its signature, once they match. None
    only when nothing has been published."""
    feed_name = feed_name or FEED
    pairs, found = [], False
    for name in (feed_name, "staging/" + feed_name):
        feed, signature = store.get(name), store.get(name + ".sig")
        found = found or feed is not None or signature is not None
        if feed is not None and signature is not None and signed(feed, signature, key):
            pairs.append((feed, signature))
        elif feed is not None or signature is not None:
            print(f"{name} and its signature don't match, or one is missing", file=sys.stderr)
    if not pairs:
        if found:
            raise Refused("no published feed matches its signature, so nothing in the bucket can be trusted")
        return None
    # The first is the feed apps read, which wins a tie.
    return max(pairs, key=lambda pair: newest(assets_of(pair[0])))


def described(path: Path) -> dict:
    """A file as the feed describes a package: its name, size and checksums, the checksums in capitals as vpk writes
    them."""
    sha1, sha256 = hashlib.sha1(), hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            sha1.update(block)
            sha256.update(block)
    return {"FileName": path.name, "Size": path.stat().st_size, "SHA1": sha1.hexdigest().upper(),
            "SHA256": sha256.hexdigest().upper()}


def file_matches(path: Path, asset: dict) -> bool:
    if not path.is_file() or path.stat().st_size != asset["Size"]:
        return False
    found = described(path)
    return found["SHA1"] == asset["SHA1"].upper() and found["SHA256"] == asset.get("SHA256", "").upper()


def files_of(asset: dict) -> list[dict]:
    """The files the feed lists for `asset`: its package, and the archive of its hub if it's a full package with one,
    each with its `FileName`, `Size` and checksums. A release from before releases published the hub has none."""
    return [asset, *([asset[HUB]] if HUB in asset else [])]


def names_of(assets: list[dict]) -> set[str]:
    return {file["FileName"] for asset in assets for file in files_of(asset)}


def write_pair(directory: Path, pair: tuple[bytes, bytes]) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / FEED).write_bytes(pair[0])
    (directory / (FEED + ".sig")).write_bytes(pair[1])


def read_pair(directory: Path, file: str | None = None) -> tuple[bytes, bytes] | None:
    """The feed in `directory`, this channel's unless `file` names another, and its signature."""
    file = file or FEED
    feed, signature = directory / file, directory / (file + ".sig")
    if not feed.exists():
        return None
    return feed.read_bytes(), signature.read_bytes() if signature.exists() else b""


def put_pair(store: DirStore | S3Store, name: str, directory: Path, file: str | None = None) -> None:
    file = file or FEED
    store.put(name, directory / file, "application/json")
    store.put(name + ".sig", directory / (file + ".sig"), "application/octet-stream")


def fetch(url: str, method: str = "GET") -> tuple[bytes, int] | None:
    """The body and length at `url`, past any cache, or None if it can't be read."""
    request = urllib.request.Request(url, method=method, headers={"Cache-Control": "no-cache"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.read(), int(response.headers.get("Content-Length", -1))
    except (urllib.error.URLError, OSError, ValueError):
        return None


# The steps.


def prepare(store: DirStore | S3Store, key: str, version: str, work: Path) -> str:
    if any((work / name).exists() for name in ("base", "feed", "releases")):
        raise Refused(f"{work} has files from an earlier run, so start from an empty directory")
    base = published(store, key)
    # A channel that took over from another and has published nothing starts from that one's feed, so the releases it
    # keeps for apps to step to carry over, and the older channel's feed can go on listing them. Only once nothing is
    # published, so a feed that doesn't match its signature is still refused.
    seeded = base is None and OLDER_FEED is not None and (base := published(store, key, OLDER_FEED)) is not None
    assets = assets_of(base[0]) if base else []
    if not seeded and any(asset["Version"] == version for asset in assets):
        # A run published it and stopped before it finished, maybe with the feed only under staging/.
        assert base is not None
        write_pair(work / "feed", base)
        if (store.get(FEED), store.get(FEED + ".sig")) != base:
            put_pair(store, FEED, work / "feed")
        for file in (file for asset in assets if asset["Version"] == version for file in files_of(asset)):
            if store.size(file["FileName"]) != file["Size"]:
                raise Refused(f"{file['FileName']} is published in the feed but missing from the bucket")
        return "published"
    # Publishing's one check that the version is newer, before the job builds anything or sets up a key. package.sh
    # has its own for a local --out, and vpk checks too, but only after a build.
    if assets and parse_version(version) <= newest(assets):
        raise Refused(f"{version} isn't newer than every published release, so no app would take it")
    releases = work / "releases"
    releases.mkdir(parents=True, exist_ok=True)
    if base is None:
        return "new"
    write_pair(work / "base", base)
    # vpk would make a delta from the older channel's newest package, which an app on it never takes, since the older
    # channel's feed lists only full packages, and an app on this one is past it.
    if seeded:
        return "new"
    (releases / FEED).write_bytes(base[0])
    fulls = [asset for asset in assets if asset["Type"] == "Full"]
    if fulls:
        latest = max(fulls, key=version_of)
        package = store.get(latest["FileName"])
        if package is None:
            raise Refused(f"the newest full package, {latest['FileName']}, isn't in the bucket")
        path = releases / latest["FileName"]
        path.write_bytes(package)
        if not file_matches(path, latest):
            raise Refused(f"{latest['FileName']} in the bucket doesn't match the feed")
    return "new"


def floors(path: Path) -> dict:
    """The build's floors, as its full package says them in the feed, from what `roundhoused worker-versions` wrote at
    `path`: which versions of the worker protocol its hub speaks, once they're a range, and the oldest release it
    updates from, once it's a release's version, if it says one."""
    try:
        said = json.loads(path.read_text())
    except (OSError, ValueError) as e:
        raise Refused(f"couldn't read which worker versions the build speaks from {path}: {e}") from e
    if not (isinstance(said, dict) and said.keys() in ({"oldest", "newest"}, {"oldest", "newest", "updatesFrom"})):
        raise Refused(f"{path} doesn't say which worker versions the build speaks, as oldest and newest: {said}")
    speaks = {end: said[end] for end in ("oldest", "newest")}
    if not (all(type(speaks[end]) is int for end in speaks) and 1 <= speaks["oldest"] <= speaks["newest"]):
        raise Refused(f"{path} doesn't say which worker versions the build speaks, as oldest and newest: {said}")
    if "updatesFrom" not in said:
        return {WORKER_VERSIONS: speaks}
    if not (isinstance(said["updatesFrom"], str) and VERSION.match(said["updatesFrom"])):
        raise Refused(f"{path} doesn't say the oldest release the build updates from as a release's version: {said}")
    return {WORKER_VERSIONS: speaks, UPDATES_FROM: said["updatesFrom"]}


def floors_of(asset: dict) -> tuple[int, tuple[int, ...]]:
    """A release's two floors: the oldest worker version its hub speaks, and the oldest release it updates from, which
    is `()` for one that says none."""
    return asset[WORKER_VERSIONS]["oldest"], parse_version(asset[UPDATES_FROM]) if UPDATES_FROM in asset else ()


def stepped_to(assets: list[dict]) -> set[tuple[int, ...]]:
    """The version of the newest release of each floor, which an app steps to while it can't take a newer one: with
    workers as old as its oldest at work, or on a release older than the next one's floor. A hub speaks no worker
    version older than the one before its own, so those include the last release before each break."""
    steps: dict[tuple[int, tuple[int, ...]], tuple[int, ...]] = {}
    for asset in assets:
        if asset["Type"] == "Full" and WORKER_VERSIONS in asset:
            floor = floors_of(asset)
            steps[floor] = max(steps.get(floor, ()), version_of(asset))
    return set(steps.values())


def kept(assets: list[dict], keep: int) -> list[dict]:
    """The newest `keep` releases, and the full package of each release an app steps to, on every channel."""
    newest = set(sorted({version_of(asset) for asset in assets}, reverse=True)[:keep])
    steps = stepped_to(assets)
    return [asset for asset in assets
            if version_of(asset) in newest or (version_of(asset) in steps and asset["Type"] == "Full")]


def steps_from(assets: list[dict], version: str, floors: dict) -> None:
    """Refuses `version`, with these floors, if an app on the newest published release couldn't take it, so it would
    have no release to step to, as "A break in the worker protocol" in `docs/standards/merging.md` says. That's where
    its hub doesn't speak that release's workers, on a channel whose apps step, or where that release is older than the
    oldest one it updates from. `tools/update-steps` holds such a change back in the merge queue, so this only catches
    one that got past it. A release from before releases said what they speak says nothing to check."""
    fulls = [asset for asset in assets if asset["Type"] == "Full"]
    if not fulls:
        return
    latest = max(fulls, key=version_of)
    if UPDATES_FROM in floors:
        floor_is_a_step(assets, version, floors)
    if CHANNEL not in STEPS or WORKER_VERSIONS not in latest:
        return
    if floors[WORKER_VERSIONS]["oldest"] > latest[WORKER_VERSIONS]["newest"]:
        raise Refused(f"{version}'s hub doesn't speak workers of protocol {latest[WORKER_VERSIONS]['newest']}, which "
                      f"{latest['Version']}'s do, so an app with those at work could never update. tools/update-steps "
                      "should have held the newer break back in the merge queue. \"A break in the worker protocol\" "
                      "in docs/standards/merging.md says what's left to do")


def floor_is_a_step(assets: list[dict], version: str, floors: dict) -> None:
    """Refuses `version` unless the oldest release it updates from is one this channel's feed keeps for apps to step
    to, so a person whose hub refuses a database that old can still install it. Where apps step, that release can be
    no newer than the last whose hub doesn't speak the oldest workers `version`'s hub does, since only those carry an
    app from before releases said what they update from through it: such an app takes `version` only once its hub's
    workers are that new, and the database it has is then from a release at least as new."""
    floor = floors[UPDATES_FROM]
    if parse_version(floor) not in stepped_to(assets):
        raise Refused(f"{version} updates from {floor} and nothing older, but the feed doesn't keep {floor} for apps to "
                      "step to, so an app on a release before it would have none")
    if CHANNEL not in STEPS:
        return
    oldest = floors[WORKER_VERSIONS]["oldest"]
    before = [asset for asset in assets if asset["Type"] == "Full" and WORKER_VERSIONS in asset
              and asset[WORKER_VERSIONS]["newest"] < oldest]
    if not before or parse_version(floor) > max(version_of(asset) for asset in before):
        raise Refused(f"{version} updates from {floor} and nothing older, which is past the last release whose hub "
                      f"doesn't speak workers of protocol {oldest}, so an app from before releases said what they "
                      "update from could step past it")


def merge(version: str, keep: int, floors: dict, work: Path) -> Path:
    parse_version(version)
    if keep < 1:
        raise Refused("the feed has to keep at least the release it publishes")
    base = read_pair(work / "base")
    assets = assets_of(base[0]) if base else []
    made = [asset for asset in assets_of((work / "releases" / FEED).read_bytes()) if asset["Version"] == version]
    if [asset["Type"] for asset in made if asset["Type"] == "Full"] != ["Full"]:
        raise Refused(f"vpk didn't make one full package for {version}")
    for asset in made:
        if asset["PackageId"] != PACK_ID or not file_matches(work / "releases" / asset["FileName"], asset):
            raise Refused(f"{asset['FileName']} doesn't match what vpk's feed says of it")
        if asset["Type"] == "Full":
            asset.update(floors)
            if CHANNEL in HUBS:
                asset[HUB] = packaged_hub(version, work)
    steps_from(assets, version, floors)
    feed = {"Assets": kept(assets + made, keep)}
    path = work / "feed" / FEED
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(feed, separators=(",", ":")))
    return path


def hub_download() -> str:
    """What people download to install the newest release's hub, by a name that stays the same."""
    return f"{PACK_ID}-{CHANNEL}-hub.tar.gz"


def packaged_hub(version: str, work: Path) -> dict:
    """The archive of `version`'s hub that packaging put in DIR/releases, as the feed describes it."""
    path = work / "releases" / f"{PACK_ID}-{version}-{CHANNEL}-hub.tar.gz"
    if not path.is_file():
        raise Refused(f"packaging made no {path.name}, the archive of the hub this channel publishes")
    return described(path)


def full_of(feed: bytes, version: str) -> list[dict]:
    return [asset for asset in assets_of(feed) if asset["Version"] == version and asset["Type"] == "Full"]


def offered_on_older(feed: bytes, version: str) -> list[dict]:
    """What the older channel's feed offers, as `feed` has it: `version`'s full package and, on a channel whose apps
    step, each release's they step to, which an app with older workers at work takes first. Only full packages, since a
    delta starts from the release before it in this channel, not the app's."""
    if len(full_of(feed, version)) != 1:
        raise Refused(f"the feed to publish doesn't list one full package for {version}")
    assets = assets_of(feed)
    steps = stepped_to(assets) if CHANNEL in STEPS else set()
    return [asset for asset in assets
            if asset["Type"] == "Full" and (asset["Version"] == version or version_of(asset) in steps)]


def bridge(version: str, work: Path) -> Path | None:
    if OLDER_FEED is None:
        return None
    offered = offered_on_older((work / "feed" / FEED).read_bytes(), version)
    path = work / "bridge" / OLDER_FEED
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"Assets": offered}, separators=(",", ":")))
    return path


def bridged(key: str, version: str, feed: bytes, work: Path) -> tuple[bytes, bytes] | None:
    """The older channel's feed that `bridge` wrote, and its signature, once it's signed and offers exactly what
    `offered_on_older` takes from `feed`. None if it wrote none. What the older channel's feed held before isn't kept,
    or read."""
    older = read_pair(work / "bridge", OLDER_FEED) if OLDER_FEED else None
    if older is None:
        return None
    if not signed(*older, key):
        raise Refused(f"{OLDER_FEED} isn't signed with the app's key")
    if assets_of(older[0]) != offered_on_older(feed, version):
        raise Refused(f"{OLDER_FEED} should offer only {version}'s full package, and where apps step the ones they "
                      "step to, as the signed feed has them")
    return older


def upload(store: DirStore | S3Store, key: str, version: str, work: Path) -> None:
    pair = read_pair(work / "feed")
    if pair is None or not signed(*pair, key):
        raise Refused("the feed to publish isn't signed with the app's key")
    base = read_pair(work / "base")
    before = {asset["FileName"]: asset for asset in assets_of(base[0])} if base else {}
    after = assets_of(pair[0])
    added = [asset for asset in after if asset["FileName"] not in before]
    unchanged = all(asset == before[asset["FileName"]] for asset in after if asset["FileName"] in before)
    adds_this = any(asset["Type"] == "Full" for asset in added) and all(asset["Version"] == version for asset in added)
    if not (unchanged and adds_this):
        raise Refused(f"the feed to publish should add {version}'s packages to the published one, and change "
                      "nothing else")
    older = bridged(key, version, pair[0], work)
    for file in (file for asset in added for file in files_of(asset)):
        path = work / "releases" / file["FileName"]
        if not file_matches(path, file):
            raise Refused(f"{path} doesn't match the signed feed")
        store.put(file["FileName"], path, "application/octet-stream")
    for name, content_type in DOWNLOADS.items():
        store.put(name, work / "releases" / name, content_type)
    # The archive just checked against the signed feed, under the name that stays the same from release to release.
    for asset in added:
        if HUB in asset:
            store.put(hub_download(), work / "releases" / asset[HUB]["FileName"], "application/gzip")
    put_pair(store, STAGED_FEED, work / "feed")
    put_pair(store, FEED, work / "feed")
    if (store.get(FEED), store.get(FEED + ".sig")) != pair:
        raise Refused("the published feed doesn't read back as what was signed")
    # After this channel's feed, which apps on it read for the release before. A run that stops before this leaves the
    # older channel on the release before, until the next one.
    if older is not None:
        assert OLDER_FEED is not None
        put_pair(store, OLDER_FEED, work / "bridge", OLDER_FEED)
        if (store.get(OLDER_FEED), store.get(OLDER_FEED + ".sig")) != older:
            raise Refused(f"{OLDER_FEED} doesn't read back as what was signed")
    for name in names_of(list(before.values())) - names_of(after):
        store.delete(name)


def wait_until_served(url: str, name: str, pair: tuple[bytes, bytes], attempts: int, wait: float, what: str) -> None:
    for attempt in range(attempts):
        feed, signature = fetch(f"{url}/{name}"), fetch(f"{url}/{name}.sig")
        if feed and signature and (feed[0], signature[0]) == pair:
            return
        if attempt + 1 == attempts:
            raise Refused(f"{url} doesn't serve the {what} this run published")
        time.sleep(wait)


def check(url: str, key: str, version: str, work: Path, attempts: int = 6, wait: float = 10) -> list[str]:
    """Checks what apps read at `url`, and returns a line for each release it lists."""
    pair = read_pair(work / "feed")
    if pair is None:
        raise Refused(f"{work / 'feed'} has no feed, so this run published nothing to check")
    wait_until_served(url, FEED, pair, attempts, wait, "feed")
    if not signed(*pair, key):
        raise Refused("the published feed doesn't match the app's key")
    # Only once this run uploaded it. One that found its version published already has none to check.
    older = read_pair(work / "bridge", OLDER_FEED) if OLDER_FEED else None
    if older is not None:
        wait_until_served(url, OLDER_FEED, older, attempts, wait, OLDER_FEED)
    assets = assets_of(pair[0])
    fulls = full_of(pair[0], version)
    if not fulls:
        raise Refused(f"the published feed doesn't list {version}")
    # And each release the older channel's feed offers, which can be one this channel's first release took over.
    offered = {file["FileName"]: file for asset in fulls[:1] + (assets_of(older[0]) if older else [])
               for file in files_of(asset)}
    for file in offered.values():
        found = fetch(f"{url}/{file['FileName']}", method="HEAD")
        if found is None or found[1] != file["Size"]:
            size = "missing" if found is None else f"{found[1]} bytes"
            raise Refused(f"{file['FileName']} is {size} at {url}, not {file['Size']} bytes")
    return [f"{asset['Version']} {kind} {file['FileName']} {file['Size']}"
            for asset in assets for kind, file in zip((asset["Type"], HUB), files_of(asset))]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--channel", choices=sorted(CHANNELS), default="macos-arm64")
    steps = parser.add_subparsers(dest="step", required=True)
    for name, options in {
        "prepare": ("--store", "--key", "--version", "--work"),
        "merge": ("--version", "--keep", "--worker-versions", "--work"),
        "bridge": ("--version", "--work"),
        "upload": ("--store", "--key", "--version", "--work"),
        "check": ("--url", "--key", "--version", "--work"),
    }.items():
        step = steps.add_parser(name)
        for option in options:
            types = {"--keep": int, "--worker-versions": Path, "--work": Path}
            step.add_argument(option, required=True, type=types.get(option, str))
    args = parser.parse_args()
    use_channel(args.channel)
    try:
        if args.step == "prepare":
            print(prepare(open_store(args.store), args.key, args.version, args.work))
        elif args.step == "merge":
            print(merge(args.version, args.keep, floors(args.worker_versions), args.work))
        elif args.step == "bridge":
            older = bridge(args.version, args.work)
            if older is not None:
                print(older)
        elif args.step == "upload":
            upload(open_store(args.store), args.key, args.version, args.work)
        else:
            print("\n".join(check(args.url.rstrip("/"), args.key, args.version, args.work)))
    except Refused as refused:
        print(f"error: {refused}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
