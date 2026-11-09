#!/usr/bin/env python3
"""Publishes a packaged release as an update, so that the only feed the release workflow signs is one it computed.

The app trusts the update feed because the workflow signed it, so the workflow never signs what it reads from the
bucket. It reads the published feed, uses it only once its signature matches the app's key, adds this release's
packages to it, and signs and uploads exactly that. Whoever can write to the bucket can then only offer apps a feed
the workflow signed, such as an older one, and can't get a feed of theirs signed.

  publish.py prepare --store STORE --key KEY --version VERSION --work DIR
      Reads the published feed and checks its signature. If it lists VERSION, a run already published it: this makes
      sure apps get that feed, and prints `published`. Otherwise it puts the feed in DIR/base, and in DIR/releases
      with its newest full package, checked against it, for vpk to make a delta from, and prints `new`.
  publish.py merge --version VERSION --keep COUNT --work DIR
      Writes DIR/feed/releases.osx.json: the feed in DIR/base and the packages vpk made for VERSION in DIR/releases,
      checked against their files, without releases older than the newest COUNT. The workflow signs it next.
  publish.py upload --store STORE --key KEY --version VERSION --work DIR
      Uploads the new packages and the portable zip, then the signed feed, reads the feed back, and then deletes the
      packages that left it.

STORE is s3://BUCKET, with S3_ENDPOINT, S3_REGION and the AWS CLI's usual credentials, or a directory, to try it
without a bucket. KEY is the app's public key: the raw 32 bytes, in base64. `test_publish.py` runs it against a
directory.

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
from pathlib import Path

HERE = Path(__file__).resolve().parent
PACK_ID = "Roundhouse"
FEED = "releases.osx.json"
STAGED_FEED = "staging/" + FEED
PORTABLE = PACK_ID + "-osx-Portable.zip"
VERSION = re.compile(r"^\d+\.\d+\.\d+$")


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


def version_of(asset: dict) -> tuple[int, ...]:
    version = asset["Version"]
    if not VERSION.match(version):
        raise Refused(f"{version} isn't a version this publishes, which is three numbers")
    return tuple(int(part) for part in version.split("."))


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


def merge(version: str, keep: int, work: Path) -> None:
    if not VERSION.match(version):
        raise Refused(f"{version} isn't a version this publishes, which is three numbers")
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
    if assets and version_of(made[0]) <= newest(assets):
        raise Refused(f"{version} isn't newer than every published release, so no app would take it")
    every = assets + made
    kept = sorted({version_of(asset) for asset in every}, reverse=True)[:keep]
    feed = {"Assets": [asset for asset in every if version_of(asset) in kept]}
    (work / "feed").mkdir(parents=True, exist_ok=True)
    (work / "feed" / FEED).write_text(json.dumps(feed, separators=(",", ":")))


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
    store.put(PORTABLE, work / "releases" / PORTABLE, "application/zip")
    put_pair(store, STAGED_FEED, work / "feed")
    put_pair(store, FEED, work / "feed")
    if (store.get(FEED), store.get(FEED + ".sig")) != pair:
        raise Refused("the published feed doesn't read back as what was signed")
    for name in before.keys() - {asset["FileName"] for asset in after}:
        store.delete(name)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("step", choices=["prepare", "merge", "upload"])
    parser.add_argument("--store")
    parser.add_argument("--key")
    parser.add_argument("--version", required=True)
    parser.add_argument("--keep", type=int)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.step == "prepare":
            print(prepare(open_store(args.store), args.key, args.version, args.work))
        elif args.step == "merge":
            merge(args.version, args.keep or 0, args.work)
        else:
            upload(open_store(args.store), args.key, args.version, args.work)
    except Refused as refused:
        print(f"error: {refused}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
