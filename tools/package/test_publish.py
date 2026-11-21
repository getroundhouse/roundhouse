"""Tests for publish.py and feed-signature.sh, against a directory standing in for the bucket, served over HTTP for
`check`, and packages standing in for vpk's.

    python3 -m unittest discover -s tools/package

Needs OpenSSL 3, as feed-signature.sh does, and fails without it.
"""

from __future__ import annotations

import functools
import hashlib
import http.server
import json
import shutil
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

import publish
from publish import DirStore, Refused

HERE = Path(__file__).resolve().parent
# A throwaway key, and a feed it signed, which the app's tests check too, so the two sides can't disagree on the form.
FIXTURE = HERE.parent.parent / "crates" / "update-feed" / "fixture"
THROWAWAY_KEY = FIXTURE / "throwaway-key.pem"
# Which versions of the worker protocol a release's hub speaks, unless a test says otherwise.
SPEAKS = {"oldest": 3, "newest": 4}


def feed_signature(*args: str) -> str:
    """Runs feed-signature.sh, and returns what it printed."""
    return subprocess.run([str(HERE / "feed-signature.sh"), *args], capture_output=True, text=True,
                          check=True).stdout.strip()


def make_key(directory: Path) -> tuple[Path, str]:
    """A new private key file, and its public key the way the app holds it."""
    private = directory / "feed.pem"
    subprocess.run([feed_signature("openssl"), "genpkey", "-algorithm", "ed25519", "-out", str(private)], check=True)
    return private, feed_signature("public", str(private))


def package_name(version: str, kind: str) -> str:
    """As vpk names a package, which says its channel."""
    channel = publish.FEED.removeprefix("releases.").removesuffix(".json")
    return f"Roundhouse-{version}-{channel}-{kind.lower()}.nupkg"


def hub_name(version: str) -> str:
    """As `hub_archive` in vpk.sh names the archive of a release's hub."""
    return f"Roundhouse-{version}-{publish.CHANNEL}-hub.tar.gz"


def publishes_hubs() -> bool:
    return publish.CHANNEL in publish.HUBS


def asset(version: str, kind: str, content: bytes) -> dict:
    return {
        "PackageId": "Roundhouse",
        "Version": version,
        "Type": kind,
        "FileName": package_name(version, kind),
        "SHA1": hashlib.sha1(content).hexdigest().upper(),
        "SHA256": hashlib.sha256(content).hexdigest().upper(),
        "Size": len(content),
    }


class FailingStore(DirStore):
    """A bucket whose connection drops when a run uploads one object."""

    def __init__(self, root: str, failing: str) -> None:
        super().__init__(root)
        self.failing = failing

    def put(self, key: str, source: Path, content_type: str) -> None:
        if key == self.failing:
            raise ConnectionError(f"dropped while uploading {key}")
        super().put(key, source, content_type)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:  # noqa: A002, which is the name the base class gives it
        del format, args


class Signature(unittest.TestCase):
    def test_the_fixture_the_app_reads_names_the_worker_versions_as_this_writes_them(self) -> None:
        """`crates/update-feed` reads the same file, so the two can't come to call the field different things, which
        would have every app that steps drop every release."""
        assets = json.loads((FIXTURE / publish.FEED).read_bytes())["Assets"]
        full = next(asset for asset in assets if asset["Type"] == "Full")
        self.assertEqual(full[publish.WORKER_VERSIONS], SPEAKS)

    def test_the_fixture_the_app_and_a_hub_read_lists_the_hub_s_archive_as_this_writes_it(self) -> None:
        """`crates/update-feed` reads the same file, so a hub can't come to look for its archive under another name."""
        full = next(asset for asset in json.loads((FIXTURE / publish.FEED).read_bytes())["Assets"]
                    if asset["Type"] == "Full")
        with tempfile.TemporaryDirectory() as work:
            archive = Path(work) / f"Roundhouse-{full['Version']}-osx-hub.tar.gz"
            archive.write_bytes(b"hub")
            self.assertEqual(set(full[publish.HUB]), set(publish.described(archive)))
        self.assertEqual(full[publish.HUB]["FileName"], archive.name)

    def test_openssl_signs_the_fixture_as_the_app_expects(self) -> None:
        key = (FIXTURE / "throwaway-key.pub").read_text().strip()
        self.assertEqual(feed_signature("public", str(THROWAWAY_KEY)), key)
        feed_signature("verify", str(FIXTURE / publish.FEED), key)

        # Ed25519 signs the same bytes the same way every time, so signing again gives the committed signature.
        with tempfile.TemporaryDirectory() as work:
            feed = Path(work) / publish.FEED
            shutil.copyfile(FIXTURE / publish.FEED, feed)
            feed_signature("sign", str(feed), str(THROWAWAY_KEY), key)
            self.assertEqual(Path(str(feed) + ".sig").read_bytes(), (FIXTURE / (publish.FEED + ".sig")).read_bytes())

    def test_a_changed_feed_or_another_key_fails_to_verify(self) -> None:
        key = (FIXTURE / "throwaway-key.pub").read_text().strip()
        with tempfile.TemporaryDirectory() as work:
            feed = Path(work) / publish.FEED
            feed.write_bytes((FIXTURE / publish.FEED).read_bytes() + b" ")
            shutil.copyfile(FIXTURE / (publish.FEED + ".sig"), Path(str(feed) + ".sig"))
            with self.assertRaises(subprocess.CalledProcessError):
                feed_signature("verify", str(feed), key)

            _, other = make_key(Path(work))
            with self.assertRaises(subprocess.CalledProcessError):
                feed_signature("verify", str(FIXTURE / publish.FEED), other)


class Publishing(unittest.TestCase):
    # Whether this platform's apps step through each break in the worker protocol, since its hubs keep their workers
    # through an update.
    APPS_STEP = True

    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.bucket = self.dir / "bucket"
        self.bucket.mkdir()
        self.store = DirStore(str(self.bucket))
        self.private, self.key = THROWAWAY_KEY, feed_signature("public", str(THROWAWAY_KEY))
        self.runs = 0

    def work(self) -> Path:
        self.runs += 1
        return self.dir / f"run-{self.runs}"

    def pack(self, work: Path, version: str) -> None:
        """What vpk does: adds a full package and, given an earlier one, a delta, to the feed in releases/."""
        releases = work / "releases"
        feed = releases / publish.FEED
        assets = json.loads(feed.read_bytes())["Assets"] if feed.exists() else []
        made = [asset(version, "Full", f"full {version} of run {self.runs}".encode())]
        if any(entry["Type"] == "Full" for entry in assets):
            made.append(asset(version, "Delta", f"delta {version} of run {self.runs}".encode()))
        for entry in made:
            (releases / entry["FileName"]).write_bytes(f"{entry['Type'].lower()} {version} of run {self.runs}".encode())
        for download in publish.DOWNLOADS:
            (releases / download).write_bytes(f"{download} {version}".encode())
        # What the packaging script makes beside vpk's.
        if publishes_hubs():
            (releases / hub_name(version)).write_bytes(f"hub {version} of run {self.runs}".encode())
        feed.write_text(json.dumps({"Assets": assets + made}))

    def sign(self, work: Path) -> None:
        self.sign_file(work / "feed" / publish.FEED)

    def sign_file(self, feed: Path) -> None:
        subprocess.run([str(HERE / "feed-signature.sh"), "sign", str(feed), str(self.private), self.key], check=True,
                       capture_output=True)

    def release(self, version: str, keep: int = 10, store: DirStore | None = None,
                speaks: dict[str, int] = SPEAKS) -> Path:
        """One run of the release job, of a build whose hub speaks the worker versions `speaks`."""
        store = store or self.store
        work = self.work()
        if publish.prepare(store, self.key, version, work) == "new":
            self.pack(work, version)
            self.assertEqual(publish.merge(version, keep, speaks, work), work / "feed" / publish.FEED)
            self.sign(work)
            older = publish.bridge(version, work)
            if older is not None:
                self.sign_file(older)
            publish.upload(store, self.key, version, work)
        return work

    def serve(self) -> str:
        """The bucket's URL, served the way apps read it."""
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(self.bucket)))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        return f"http://127.0.0.1:{server.server_address[1]}"

    def published(self) -> list[tuple[str, str]]:
        feed, signature = self.store.get(publish.FEED), self.store.get(publish.FEED + ".sig")
        self.assertIsNotNone(feed)
        self.assertTrue(publish.signed(feed, signature, self.key))
        return sorted((entry["Version"], entry["Type"]) for entry in json.loads(feed)["Assets"])

    def assert_packages_match(self) -> None:
        for entry in json.loads(self.store.get(publish.FEED))["Assets"]:
            for file in publish.files_of(entry):
                self.assertTrue(publish.file_matches(self.bucket / file["FileName"], file), file["FileName"])

    def hub_of(self, version: str) -> dict | None:
        """What the published feed says of `version`'s hub archive."""
        assets = json.loads(self.store.get(publish.FEED))["Assets"]
        return next(entry for entry in assets if entry["Version"] == version and entry["Type"] == "Full").get(
            publish.HUB)

    def test_the_first_release_publishes_a_signed_feed_with_its_package(self) -> None:
        work = self.release("0.1.1")

        self.assertEqual(self.published(), [("0.1.1", "Full")])
        self.assertEqual(self.store.get(publish.FEED), (work / "feed" / publish.FEED).read_bytes())
        self.assert_packages_match()
        self.assertEqual(list(publish.DOWNLOADS), ["Roundhouse-osx-Portable.zip"])
        for download in ("Roundhouse-osx-Portable.zip", "Roundhouse-osx-hub.tar.gz"):
            self.assertTrue((self.bucket / download).exists(), download)

    def test_the_next_release_starts_from_the_newest_full_package_and_adds_to_the_feed(self) -> None:
        self.release("0.1.1")
        work = self.work()

        self.assertEqual(publish.prepare(self.store, self.key, "0.1.2", work), "new")

        newest = work / "releases" / package_name("0.1.1", "Full")
        self.assertEqual(newest.read_bytes(), (self.bucket / newest.name).read_bytes())
        self.assertEqual((work / "releases" / publish.FEED).read_bytes(), self.store.get(publish.FEED))
        self.pack(work, "0.1.2")
        publish.merge("0.1.2", 10, SPEAKS, work)
        self.sign(work)
        publish.upload(self.store, self.key, "0.1.2", work)
        self.assertEqual(self.published(), [("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.2", "Full")])
        self.assert_packages_match()

    def test_releases_past_the_newest_few_leave_the_feed_and_the_bucket(self) -> None:
        for version in ("0.1.1", "0.1.2", "0.1.3"):
            self.release(version, keep=2)

        self.assertEqual(self.published(), [("0.1.2", "Delta"), ("0.1.2", "Full"), ("0.1.3", "Delta"),
                                            ("0.1.3", "Full")])
        self.assertFalse((self.bucket / package_name("0.1.1", "Full")).exists())
        self.assertFalse((self.bucket / hub_name("0.1.1")).exists())
        self.assert_packages_match()

    def test_a_release_s_full_package_lists_its_hub_s_archive_which_people_also_download(self) -> None:
        work = self.release("0.1.1")
        self.release("0.1.2")

        if not publishes_hubs():
            self.assertIsNone(self.hub_of("0.1.2"))
            self.assertEqual(list(self.bucket.glob("*-hub.tar.gz")), [])
            return
        hub = self.hub_of("0.1.2")
        self.assertEqual(hub, publish.described(self.bucket / hub_name("0.1.2")))
        self.assertEqual(hub["FileName"], hub_name("0.1.2"))
        self.assertEqual(self.hub_of("0.1.1"), publish.described(work / "releases" / hub_name("0.1.1")))
        # The newest, under the name that stays the same.
        self.assertEqual(publish.hub_download(), f"Roundhouse-{publish.CHANNEL}-hub.tar.gz")
        self.assertEqual((self.bucket / publish.hub_download()).read_bytes(),
                         (self.bucket / hub_name("0.1.2")).read_bytes())

    def test_a_release_after_ones_from_before_hubs_were_published_lists_its_own(self) -> None:
        with mock.patch.object(publish, "HUBS", set()):
            self.release("0.1.1")
        self.release("0.1.2")
        work = self.release("0.1.3", keep=1)

        self.assertEqual(self.published(), [("0.1.3", "Delta"), ("0.1.3", "Full")])
        self.assertEqual(self.hub_of("0.1.3") is not None, publishes_hubs())
        self.assertFalse((self.bucket / hub_name("0.1.2")).exists())
        self.assert_packages_match()
        self.assertEqual(publish.prepare(self.store, self.key, "0.1.3", self.work()), "published")
        publish.check(self.serve(), self.key, "0.1.3", work, wait=0)

    def test_a_hub_archive_packaging_didnt_make_or_that_changed_is_refused(self) -> None:
        if not publishes_hubs():
            self.skipTest("this channel publishes no hub archive")
        work = self.work()
        publish.prepare(self.store, self.key, "0.1.1", work)
        self.pack(work, "0.1.1")
        archive = work / "releases" / hub_name("0.1.1")
        archive.unlink()
        with self.assertRaisesRegex(Refused, f"packaging made no {hub_name('0.1.1')}"):
            publish.merge("0.1.1", 10, SPEAKS, work)

        archive.write_bytes(b"hub")
        publish.merge("0.1.1", 10, SPEAKS, work)
        self.sign(work)
        archive.write_bytes(b"changed")
        with self.assertRaisesRegex(Refused, "doesn't match the signed feed"):
            publish.upload(self.store, self.key, "0.1.1", work)
        self.assertIsNone(self.store.get(publish.FEED))

    def test_a_release_s_full_package_says_which_worker_versions_its_hub_speaks(self) -> None:
        self.release("0.1.1")
        self.release("0.1.2", speaks={"oldest": 4, "newest": 5})

        said = {(entry["Version"], entry["Type"]): entry.get(publish.WORKER_VERSIONS)
                for entry in json.loads(self.store.get(publish.FEED))["Assets"]}
        self.assertEqual(said, {("0.1.1", "Full"): SPEAKS, ("0.1.2", "Delta"): None,
                                ("0.1.2", "Full"): {"oldest": 4, "newest": 5}})

    def test_the_last_release_before_each_break_stays_as_its_full_package(self) -> None:
        for version, oldest in (("0.1.1", 3), ("0.1.2", 3), ("0.1.3", 4), ("0.1.4", 4), ("0.1.5", 5), ("0.1.6", 5)):
            self.release(version, keep=2, speaks={"oldest": oldest, "newest": oldest + 1})

        # 0.1.2's hub is the newest that speaks workers of protocol 3, and 0.1.4's the newest that speaks 4.
        self.assertEqual(self.published(), [("0.1.2", "Full"), ("0.1.4", "Full"), ("0.1.5", "Delta"),
                                            ("0.1.5", "Full"), ("0.1.6", "Delta"), ("0.1.6", "Full")])
        for gone in ("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.3", "Full"), ("0.1.4", "Delta"):
            self.assertFalse((self.bucket / package_name(*gone)).exists(), gone)
        self.assert_packages_match()

    def test_a_release_whose_hub_cant_talk_to_the_newest_one_s_workers_is_refused_where_apps_step(self) -> None:
        self.release("0.1.1", speaks={"oldest": 3, "newest": 4})
        work = self.work()
        publish.prepare(self.store, self.key, "0.1.2", work)
        self.pack(work, "0.1.2")

        if self.APPS_STEP:
            with self.assertRaises(Refused):
                publish.merge("0.1.2", 10, {"oldest": 5, "newest": 6}, work)
            self.assertEqual(self.published(), [("0.1.1", "Full")])
        else:
            publish.merge("0.1.2", 10, {"oldest": 5, "newest": 6}, work)

    def test_what_a_build_says_it_speaks_is_used_only_as_a_range(self) -> None:
        path = self.dir / "worker-versions.json"
        path.write_text('{"oldest":3,"newest":4}\n')
        self.assertEqual(publish.worker_versions(path), SPEAKS)

        for said in ('{"oldest":4,"newest":3}', '{"newest":4}', '{"oldest":"3","newest":4}',
                     '{"oldest":true,"newest":4}', '{"oldest":0,"newest":4}', '{"oldest":3,"newest":4,"more":5}',
                     "[3,4]", ""):
            path.write_text(said)
            with self.assertRaises(Refused, msg=said):
                publish.worker_versions(path)
        with self.assertRaises(Refused):
            publish.worker_versions(self.dir / "missing.json")

    def test_a_feed_changed_in_the_bucket_is_never_signed(self) -> None:
        self.release("0.1.1")
        planted = json.loads(self.store.get(publish.FEED))
        planted["Assets"].append(asset("9.0.0", "Full", b"not ours"))
        (self.bucket / publish.FEED).write_text(json.dumps(planted))

        # The copy under staging/ still matches its signature, so the release goes ahead from that.
        self.release("0.1.2")
        self.assertEqual(self.published(), [("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.2", "Full")])

        (self.bucket / publish.FEED).write_text(json.dumps(planted))
        (self.bucket / publish.STAGED_FEED).write_text(json.dumps(planted))
        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, self.key, "0.1.3", self.work())

    def test_a_feed_signed_with_another_key_is_refused(self) -> None:
        self.release("0.1.1")
        _, other = make_key(self.dir / "run-1")

        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, other, "0.1.2", self.work())

    def test_an_unsigned_feed_is_refused_unless_nothing_was_ever_published(self) -> None:
        (self.bucket / publish.FEED).write_text(json.dumps({"Assets": [asset("0.1.1", "Full", b"x")]}))
        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, self.key, "0.1.2", self.work())

        (self.bucket / publish.FEED).unlink()
        (self.bucket / (publish.FEED + ".sig")).write_bytes(bytes(64))
        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, self.key, "0.1.2", self.work())

    def test_a_newest_package_that_isnt_the_one_in_the_feed_is_refused(self) -> None:
        self.release("0.1.1")
        package = self.bucket / package_name("0.1.1", "Full")
        package.write_bytes(b"changed")
        with self.assertRaisesRegex(Refused, "doesn't match the feed"):
            publish.prepare(self.store, self.key, "0.1.2", self.work())

        package.unlink()
        with self.assertRaisesRegex(Refused, "isn't in the bucket"):
            publish.prepare(self.store, self.key, "0.1.2", self.work())

    def test_a_version_already_published_is_left_as_it_is(self) -> None:
        self.release("0.1.1")
        before = sorted((path.name, path.read_bytes()) for path in self.bucket.rglob("*") if path.is_file())

        work = self.work()
        self.assertEqual(publish.prepare(self.store, self.key, "0.1.1", work), "published")

        after = sorted((path.name, path.read_bytes()) for path in self.bucket.rglob("*") if path.is_file())
        self.assertEqual(before, after)
        self.assertEqual((work / "feed" / publish.FEED).read_bytes(), self.store.get(publish.FEED))

    def test_a_version_published_without_its_hub_archive_in_the_bucket_is_refused(self) -> None:
        if not publishes_hubs():
            self.skipTest("this channel publishes no hub archive")
        self.release("0.1.1")
        (self.bucket / hub_name("0.1.1")).unlink()

        with self.assertRaisesRegex(Refused, f"{hub_name('0.1.1')} is published in the feed but missing"):
            publish.prepare(self.store, self.key, "0.1.1", self.work())

    def test_running_again_after_only_the_packages_went_up_publishes_the_new_ones(self) -> None:
        self.release("0.1.1")
        with self.assertRaises(ConnectionError):
            self.release("0.1.2", store=FailingStore(str(self.bucket), publish.STAGED_FEED))
        self.assertEqual(self.published(), [("0.1.1", "Full")])

        self.release("0.1.2")

        self.assertEqual(self.published(), [("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.2", "Full")])
        self.assert_packages_match()

    def test_running_again_after_the_feed_went_up_without_its_signature_finishes(self) -> None:
        self.release("0.1.1")
        for failing in (publish.FEED, publish.FEED + ".sig"):
            version = "0.1.2" if failing == publish.FEED else "0.1.3"
            with self.assertRaises(ConnectionError):
                self.release(version, store=FailingStore(str(self.bucket), failing))

            self.assertEqual(publish.prepare(self.store, self.key, version, self.work()), "published")

            self.assertEqual(self.store.get(publish.FEED), self.store.get(publish.STAGED_FEED))
            self.assertIn((version, "Full"), self.published())
            self.assert_packages_match()

    def test_a_version_older_than_one_published_is_refused_before_anything_is_built(self) -> None:
        self.release("0.1.5")
        work = self.work()
        with self.assertRaisesRegex(Refused, "isn't newer than every published release"):
            publish.prepare(self.store, self.key, "0.1.4", work)
        self.assertFalse((work / "releases").exists())

    def test_a_package_that_isnt_what_vpk_made_is_refused(self) -> None:
        work = self.work()
        publish.prepare(self.store, self.key, "0.1.1", work)
        self.pack(work, "0.1.1")
        (work / "releases" / package_name("0.1.1", "Full")).write_bytes(b"changed after vpk")

        with self.assertRaisesRegex(Refused, "doesn't match what vpk's feed says"):
            publish.merge("0.1.1", 10, SPEAKS, work)

    def test_only_the_feed_as_merged_and_signed_is_uploaded(self) -> None:
        self.release("0.1.1")
        work = self.work()
        publish.prepare(self.store, self.key, "0.1.2", work)
        self.pack(work, "0.1.2")
        publish.merge("0.1.2", 10, SPEAKS, work)
        feed = work / "feed" / publish.FEED
        merged = feed.read_bytes()

        feed.write_bytes(merged + b" ")
        with self.assertRaisesRegex(Refused, "isn't signed"):
            publish.upload(self.store, self.key, "0.1.2", work)

        changed = json.loads(merged)
        changed["Assets"][0]["SHA256"] = "0" * 64
        feed.write_text(json.dumps(changed))
        self.sign(work)
        with self.assertRaisesRegex(Refused, "change nothing else"):
            publish.upload(self.store, self.key, "0.1.2", work)
        self.assertEqual(self.published(), [("0.1.1", "Full")])

    def test_only_a_channel_that_took_over_from_another_publishes_that_one_s_feed_too(self) -> None:
        self.release("0.1.1")

        feeds = sorted(path.name for path in self.bucket.glob("releases.*.json"))
        self.assertEqual(feeds, sorted([publish.FEED, *([publish.OLDER_FEED] if publish.OLDER_FEED else [])]))

    def test_a_run_starts_from_an_empty_directory(self) -> None:
        work = self.release("0.1.1")
        with self.assertRaisesRegex(Refused, "start from an empty directory"):
            publish.prepare(self.store, self.key, "0.1.2", work)

    def test_the_check_reads_what_apps_read(self) -> None:
        self.release("0.1.1")
        work = self.release("0.1.2")

        hubs = publishes_hubs()
        self.assertEqual(publish.check(self.serve(), self.key, "0.1.2", work, wait=0), [
            f"0.1.1 Full {package_name('0.1.1', 'Full')} 19",
            *([f"0.1.1 Hub {hub_name('0.1.1')} 18"] if hubs else []),
            f"0.1.2 Full {package_name('0.1.2', 'Full')} 19",
            *([f"0.1.2 Hub {hub_name('0.1.2')} 18"] if hubs else []),
            f"0.1.2 Delta {package_name('0.1.2', 'Delta')} 20",
        ])

    def test_the_check_refuses_a_feed_or_package_apps_wouldnt_get(self) -> None:
        first = self.release("0.1.1")
        work = self.release("0.1.2")
        url = self.serve()

        # Such as a cache still serving the last feed.
        for name in (publish.FEED, publish.FEED + ".sig"):
            shutil.copyfile(first / "feed" / name, self.bucket / name)
        with self.assertRaisesRegex(Refused, "doesn't serve the feed this run published"):
            publish.check(url, self.key, "0.1.2", work, attempts=2, wait=0)

        for name in (publish.FEED, publish.FEED + ".sig"):
            shutil.copyfile(work / "feed" / name, self.bucket / name)
        (self.bucket / package_name("0.1.2", "Full")).unlink()
        with self.assertRaisesRegex(Refused, f"{package_name('0.1.2', 'Full')} is missing"):
            publish.check(url, self.key, "0.1.2", work, wait=0)

        if publishes_hubs():
            shutil.copyfile(work / "releases" / package_name("0.1.2", "Full"),
                            self.bucket / package_name("0.1.2", "Full"))
            (self.bucket / hub_name("0.1.2")).unlink()
            with self.assertRaisesRegex(Refused, f"{hub_name('0.1.2')} is missing"):
                publish.check(url, self.key, "0.1.2", work, wait=0)

        _, other = make_key(self.dir)
        with self.assertRaisesRegex(Refused, "doesn't match the app's key"):
            publish.check(url, other, "0.1.2", work, wait=0)


class BesideAnother:
    """A platform's releases, which go in the bucket macOS's are in, under a feed of their own. Mixed into a
    `Publishing` for each platform, so every test above runs again for it, and the ones here are about two platforms
    side by side."""

    # The platform's channel, and another's, published into the same bucket first.
    CHANNEL = ""
    BESIDE = ""
    # Its first full package, and what people download, as vpk names them.
    PACKAGE = ""
    DOWNLOADS: tuple[str, ...] = ()

    def setUp(self) -> None:
        # Publishing's, which comes after this in each platform's bases.
        super().setUp()
        publish.use_channel(self.CHANNEL)
        self.addCleanup(publish.use_channel, "osx")

    def test_the_first_release_publishes_a_signed_feed_with_its_package(self) -> None:
        self.release("0.1.1")

        self.assertEqual(self.published(), [("0.1.1", "Full")])
        self.assert_packages_match()
        feed = f"releases.{self.CHANNEL}.json"
        for name in (feed, feed + ".sig", *self.DOWNLOADS, self.PACKAGE):
            self.assertTrue((self.bucket / name).exists(), name)

    def test_a_platform_s_releases_leave_the_other_s_alone(self) -> None:
        publish.use_channel(self.BESIDE)
        self.release("0.1.1")
        self.release("0.1.2")
        beside = {path.name: path.read_bytes() for path in self.bucket.iterdir() if path.is_file()}

        # The same versions, since each platform publishes every merge, and few enough kept that some are deleted.
        publish.use_channel(self.CHANNEL)
        self.release("0.1.1", keep=1)
        self.release("0.1.2", keep=1)

        self.assertEqual(self.published(), [("0.1.2", "Delta"), ("0.1.2", "Full")])
        self.assertFalse((self.bucket / self.PACKAGE).exists())
        for name, content in beside.items():
            self.assertEqual((self.bucket / name).read_bytes(), content, name)


class Windows(BesideAnother, Publishing):
    """amd64 Windows' releases, beside macOS's."""

    CHANNEL = "win-amd64"
    # A Windows hub's workers stop with it, so its apps take the newest.
    APPS_STEP = False
    BESIDE = "osx"
    PACKAGE = "Roundhouse-0.1.1-win-amd64-full.nupkg"
    DOWNLOADS = ("Roundhouse-win-amd64-Setup.exe", "Roundhouse-win-amd64-Portable.zip")
    # What amd64 Windows was called before, where an app packaged then still looks.
    OLDER = "releases.win.json"

    def full(self, version: str) -> list[dict]:
        assets = json.loads(self.store.get(publish.FEED))["Assets"]
        return [entry for entry in assets if entry["Version"] == version and entry["Type"] == "Full"]

    def offered_on_the_older_channel(self) -> list[dict]:
        older, signature = self.store.get(self.OLDER), self.store.get(self.OLDER + ".sig")
        self.assertIsNotNone(older)
        self.assertTrue(publish.signed(older, signature, self.key))
        return json.loads(older)["Assets"]

    def merged(self, version: str) -> Path:
        """A run of the release job that stops before it uploads, with the older channel's feed not signed yet."""
        work = self.work()
        publish.prepare(self.store, self.key, version, work)
        self.pack(work, version)
        publish.merge(version, 10, SPEAKS, work)
        self.sign(work)
        return work

    def test_an_app_on_the_older_channel_is_offered_only_the_newest_full_package(self) -> None:
        self.release("0.1.1")
        self.release("0.1.2")

        # Not the delta, which starts from 0.1.1, where such an app isn't. The package is this channel's, so an app
        # that takes it looks here from then on.
        self.assertEqual(self.offered_on_the_older_channel(), self.full("0.1.2"))
        self.assertEqual(self.full("0.1.2")[0]["FileName"], "Roundhouse-0.1.2-win-amd64-full.nupkg")
        self.assertTrue(publish.file_matches(self.bucket / self.full("0.1.2")[0]["FileName"], self.full("0.1.2")[0]))

    def test_an_older_channel_s_feed_that_isnt_signed_or_offers_more_is_refused_before_anything_goes_up(self) -> None:
        self.release("0.1.1")
        work = self.merged("0.1.2")
        older = publish.bridge("0.1.2", work)
        assert older is not None

        with self.assertRaisesRegex(Refused, "releases.win.json isn't signed"):
            publish.upload(self.store, self.key, "0.1.2", work)

        merged = json.loads((work / "feed" / publish.FEED).read_bytes())["Assets"]
        older.write_text(json.dumps({"Assets": [entry for entry in merged if entry["Version"] == "0.1.2"]}))
        self.sign_file(older)
        with self.assertRaisesRegex(Refused, "should offer only 0.1.2's full package"):
            publish.upload(self.store, self.key, "0.1.2", work)

        self.assertEqual(self.published(), [("0.1.1", "Full")])
        self.assertEqual(self.offered_on_the_older_channel(), self.full("0.1.1"))
        self.assertFalse((self.bucket / package_name("0.1.2", "Full")).exists())

    def test_the_check_reads_the_older_channel_s_feed_as_apps_do(self) -> None:
        first = self.release("0.1.1")
        work = self.release("0.1.2")
        url = self.serve()
        publish.check(url, self.key, "0.1.2", work, wait=0)

        for name in (self.OLDER, self.OLDER + ".sig"):
            shutil.copyfile(first / "bridge" / name, self.bucket / name)
        with self.assertRaisesRegex(Refused, "doesn't serve the releases.win.json this run published"):
            publish.check(url, self.key, "0.1.2", work, attempts=2, wait=0)


class WindowsOnArm(BesideAnother, Publishing):
    """ARM64 Windows' releases, beside amd64 Windows'."""

    CHANNEL = "win-arm64"
    BESIDE = "win-amd64"
    APPS_STEP = False
    PACKAGE = "Roundhouse-0.1.1-win-arm64-full.nupkg"
    DOWNLOADS = ("Roundhouse-win-arm64-Setup.exe", "Roundhouse-win-arm64-Portable.zip")


class Linux(BesideAnother, Publishing):
    """amd64 Linux's releases, beside Windows', with the AppImage to download."""

    CHANNEL = "linux-amd64"
    BESIDE = "win-amd64"
    PACKAGE = "Roundhouse-0.1.1-linux-amd64-full.nupkg"
    DOWNLOADS = ("Roundhouse-linux-amd64.AppImage", "Roundhouse-linux-amd64-hub.tar.gz")


class LinuxOnArm(BesideAnother, Publishing):
    """ARM64 Linux's releases, beside amd64 Linux's."""

    CHANNEL = "linux-arm64"
    BESIDE = "linux-amd64"
    PACKAGE = "Roundhouse-0.1.1-linux-arm64-full.nupkg"
    DOWNLOADS = ("Roundhouse-linux-arm64.AppImage", "Roundhouse-linux-arm64-hub.tar.gz")


if __name__ == "__main__":
    unittest.main()
