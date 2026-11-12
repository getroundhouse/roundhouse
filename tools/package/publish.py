#!/usr/bin/env python3
"""Publishes a packaged release as an update, so that the only feed the release workflow signs is one it computed.

The app trusts the update feed because the workflow signed it, so the workflow never signs what it reads from the
bucket. It reads the published feed, uses it only once its signature matches the app's key, adds this release's
packages to it, and signs and uploads exactly that. Whoever can write to the bucket can then only offer apps a feed
the workflow signed, such as an older one, and can't get a feed of theirs signed.

Each step takes `--channel` first, for which platform's releases it publishes: `osx`, `win-amd64` for Windows on amd64,
`win-arm64`, `linux-amd64` for Linux on amd64, or `linux-arm64`. It's osx if it's left out. A platform has a feed of its own,
`releases.<channel>.json`, and all are in the one bucket.

  publish.py prepare --store STORE --key KEY --version VERSION --work DIR
      Reads the published feed and checks its signature. If it lists VERSION, a run already published it: this makes
      sure apps get that feed, and prints `published`. Otherwise it refuses a VERSION that isn't newer than every
      published release, which no app would take. It puts the feed in DIR/base, and in DIR/releases with its newest
      full package, checked against it, for vpk to make a delta from, and prints `new`.
  publish.py merge --version VERSION --keep COUNT --work DIR
      Writes the feed to sign and prints its path: the feed in DIR/base and the packages vpk made for VERSION in
      DIR/releases, checked against their files, without releases older than the newest COUNT.
  publish.py upload --store STORE --key KEY --version VERSION --work DIR
      Uploads the new packages and what people download, then the signed feed, reads the feed back, and then deletes
      the packages that left it.
  publish.py check --url URL --key KEY --version VERSION --work DIR
      Checks that URL, where apps read the feed, serves the one in DIR/feed, and VERSION's full package. A host with a
      cache can take a while to, so it tries for a minute.

STORE is s3://BUCKET, with S3_ENDPOINT, S3_REGION and the AWS CLI's usual credentials, or a directory, to try it
without a bucket. KEY is the app's public key, in the form `apps/desktop/src/updates/source.rs` says.
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


def windows(channel: str) -> dict[str, str]:
    """What people download for Windows: the installer, and a portable zip."""
    return {f"{PACK_ID}-{channel}-Setup.exe": "application/octet-stream",
            f"{PACK_ID}-{channel}-Portable.zip": "application/zip"}


def linux(channel: str) -> dict[str, str]:
    """What people download for Linux: the AppImage, which is the app."""
    return {f"{PACK_ID}-{channel}.AppImage": "application/octet-stream"}


# What people download for each channel, which vpk makes anew for each release under the same names.
CHANNELS = {
    "osx": {PACK_ID + "-osx-Portable.zip": "application/zip"},
    "win-amd64": windows("win-amd64"),
    "win-arm64": windows("win-arm64"),
    "linux-amd64": linux("linux-amd64"),
    "linux-arm64": linux("linux-arm64"),
}


def use_channel(channel: str) -> None:
    """Chooses which platform's releases every step below publishes. A run publishes one."""
    global FEED, STAGED_FEED, DOWNLOADS  # noqa: PLW0603, since a run has one channel, chosen once as it starts
    FEED = f"releases.{channel}.json"
    STAGED_FEED = "staging/" + FEED
    DOWNLOADS = CHANNELS[channel]


use_channel("osx")


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


def published(store: DirStore | S3Store, key: str) -> tuple[bytes, bytes] | None:
    """The published feed and its signature, once they match. None only when nothing has been published."""
    pairs, found = [], False
    for name in (FEED, STAGED_FEED):
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


def file_matches(path: Path, asset: dict) -> bool:
    if not path.is_file() or path.stat().st_size != asset["Size"]:
        return False
    sha1, sha256 = hashlib.sha1(), hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            sha1.update(block)
            sha256.update(block)
    return (sha1.hexdigest().upper() == asset["SHA1"].upper()
            and sha256.hexdigest().upper() == asset.get("SHA256", "").upper())


def write_pair(directory: Path, pair: tuple[bytes, bytes]) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / FEED).write_bytes(pair[0])
    (directory / (FEED + ".sig")).write_bytes(pair[1])


def read_pair(directory: Path) -> tuple[bytes, bytes] | None:
    feed, signature = directory / FEED, directory / (FEED + ".sig")
    if not feed.exists():
        return None
    return feed.read_bytes(), signature.read_bytes() if signature.exists() else b""


def put_pair(store: DirStore | S3Store, name: str, directory: Path) -> None:
    store.put(name, directory / FEED, "application/json")
    store.put(name + ".sig", directory / (FEED + ".sig"), "application/octet-stream")


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
    assets = assets_of(base[0]) if base else []
    if any(asset["Version"] == version for asset in assets):
        # A run published it and stopped before it finished, maybe with the feed only under staging/.
        assert base is not None
        write_pair(work / "feed", base)
        if (store.get(FEED), store.get(FEED + ".sig")) != base:
            put_pair(store, FEED, work / "feed")
        for asset in assets:
            if asset["Version"] == version and store.size(asset["FileName"]) != asset["Size"]:
                raise Refused(f"{asset['FileName']} is published in the feed but missing from the bucket")
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


def merge(version: str, keep: int, work: Path) -> Path:
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
    every = assets + made
    kept = sorted({version_of(asset) for asset in every}, reverse=True)[:keep]
    feed = {"Assets": [asset for asset in every if version_of(asset) in kept]}
    path = work / "feed" / FEED
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(feed, separators=(",", ":")))
    return path


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
    for asset in added:
        path = work / "releases" / asset["FileName"]
        if not file_matches(path, asset):
            raise Refused(f"{path} doesn't match the signed feed")
        store.put(asset["FileName"], path, "application/octet-stream")
    for name, content_type in DOWNLOADS.items():
        store.put(name, work / "releases" / name, content_type)
    put_pair(store, STAGED_FEED, work / "feed")
    put_pair(store, FEED, work / "feed")
    if (store.get(FEED), store.get(FEED + ".sig")) != pair:
        raise Refused("the published feed doesn't read back as what was signed")
    for name in before.keys() - {asset["FileName"] for asset in after}:
        store.delete(name)


def check(url: str, key: str, version: str, work: Path, attempts: int = 6, wait: float = 10) -> list[str]:
    """Checks what apps read at `url`, and returns a line for each release it lists."""
    pair = read_pair(work / "feed")
    if pair is None:
        raise Refused(f"{work / 'feed'} has no feed, so this run published nothing to check")
    for attempt in range(attempts):
        feed, signature = fetch(f"{url}/{FEED}"), fetch(f"{url}/{FEED}.sig")
        if feed and signature and (feed[0], signature[0]) == pair:
            break
        if attempt + 1 == attempts:
            raise Refused(f"{url} doesn't serve the feed this run published")
        time.sleep(wait)
    if not signed(*pair, key):
        raise Refused("the published feed doesn't match the app's key")
    assets = assets_of(pair[0])
    fulls = [asset for asset in assets if asset["Version"] == version and asset["Type"] == "Full"]
    if not fulls:
        raise Refused(f"the published feed doesn't list {version}")
    package = fetch(f"{url}/{fulls[0]['FileName']}", method="HEAD")
    if package is None or package[1] != fulls[0]["Size"]:
        size = "missing" if package is None else f"{package[1]} bytes"
        raise Refused(f"{fulls[0]['FileName']} is {size} at {url}, not {fulls[0]['Size']} bytes")
    return [f"{asset['Version']} {asset['Type']} {asset['FileName']} {asset['Size']}" for asset in assets]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--channel", choices=sorted(CHANNELS), default="osx")
    steps = parser.add_subparsers(dest="step", required=True)
    for name, options in {
        "prepare": ("--store", "--key", "--version", "--work"),
        "merge": ("--version", "--keep", "--work"),
        "upload": ("--store", "--key", "--version", "--work"),
        "check": ("--url", "--key", "--version", "--work"),
    }.items():
        step = steps.add_parser(name)
        for option in options:
            step.add_argument(option, required=True, type={"--keep": int, "--work": Path}.get(option, str))
    args = parser.parse_args()
    use_channel(args.channel)
    try:
        if args.step == "prepare":
            print(prepare(open_store(args.store), args.key, args.version, args.work))
        elif args.step == "merge":
            print(merge(args.version, args.keep, args.work))
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
