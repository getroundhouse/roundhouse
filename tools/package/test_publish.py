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

import publish
from publish import FEED, PORTABLE, STAGED_FEED, DirStore, Refused

HERE = Path(__file__).resolve().parent
# A throwaway key, and a feed it signed, which the app's tests check too, so the two sides can't disagree on the form.
FIXTURE = HERE.parent.parent / "apps" / "desktop" / "src" / "updates" / "source" / "fixture"
THROWAWAY_KEY = FIXTURE / "throwaway-key.pem"


def feed_signature(*args: str) -> str:
    """Runs feed-signature.sh, and returns what it printed."""
    return subprocess.run([str(HERE / "feed-signature.sh"), *args], capture_output=True, text=True,
                          check=True).stdout.strip()


def make_key(directory: Path) -> tuple[Path, str]:
    """A new private key file, and its public key the way the app holds it."""
    private = directory / "feed.pem"
    subprocess.run([feed_signature("openssl"), "genpkey", "-algorithm", "ed25519", "-out", str(private)], check=True)
    return private, feed_signature("public", str(private))


def asset(version: str, kind: str, content: bytes) -> dict:
    return {
        "PackageId": "Roundhouse",
        "Version": version,
        "Type": kind,
        "FileName": f"Roundhouse-{version}-osx-{kind.lower()}.nupkg",
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
    def test_openssl_signs_the_fixture_as_the_app_expects(self) -> None:
        key = (FIXTURE / "throwaway-key.pub").read_text().strip()
        self.assertEqual(feed_signature("public", str(THROWAWAY_KEY)), key)
        feed_signature("verify", str(FIXTURE / FEED), key)

        # Ed25519 signs the same bytes the same way every time, so signing again gives the committed signature.
        with tempfile.TemporaryDirectory() as work:
            feed = Path(work) / FEED
            shutil.copyfile(FIXTURE / FEED, feed)
            feed_signature("sign", str(feed), str(THROWAWAY_KEY), key)
            self.assertEqual(Path(str(feed) + ".sig").read_bytes(), (FIXTURE / (FEED + ".sig")).read_bytes())

    def test_a_changed_feed_or_another_key_fails_to_verify(self) -> None:
        key = (FIXTURE / "throwaway-key.pub").read_text().strip()
        with tempfile.TemporaryDirectory() as work:
            feed = Path(work) / FEED
            feed.write_bytes((FIXTURE / FEED).read_bytes() + b" ")
            shutil.copyfile(FIXTURE / (FEED + ".sig"), Path(str(feed) + ".sig"))
            with self.assertRaises(subprocess.CalledProcessError):
                feed_signature("verify", str(feed), key)

            _, other = make_key(Path(work))
            with self.assertRaises(subprocess.CalledProcessError):
                feed_signature("verify", str(FIXTURE / FEED), other)


class Publishing(unittest.TestCase):
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
        feed = releases / FEED
        assets = json.loads(feed.read_bytes())["Assets"] if feed.exists() else []
        made = [asset(version, "Full", f"full {version} of run {self.runs}".encode())]
        if any(entry["Type"] == "Full" for entry in assets):
            made.append(asset(version, "Delta", f"delta {version} of run {self.runs}".encode()))
        for entry in made:
            (releases / entry["FileName"]).write_bytes(f"{entry['Type'].lower()} {version} of run {self.runs}".encode())
        (releases / PORTABLE).write_bytes(f"portable {version}".encode())
        feed.write_text(json.dumps({"Assets": assets + made}))

    def sign(self, work: Path) -> None:
        feed = work / "feed" / FEED
        subprocess.run([str(HERE / "feed-signature.sh"), "sign", str(feed), str(self.private), self.key], check=True,
                       capture_output=True)

    def release(self, version: str, keep: int = 10, store: DirStore | None = None) -> Path:
        """One run of the release job."""
        store = store or self.store
        work = self.work()
        if publish.prepare(store, self.key, version, work) == "new":
            self.pack(work, version)
            self.assertEqual(publish.merge(version, keep, work), work / "feed" / FEED)
            self.sign(work)
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
        feed, signature = self.store.get(FEED), self.store.get(FEED + ".sig")
        self.assertIsNotNone(feed)
        self.assertTrue(publish.signed(feed, signature, self.key))
        return sorted((entry["Version"], entry["Type"]) for entry in json.loads(feed)["Assets"])

    def assert_packages_match(self) -> None:
        for entry in json.loads(self.store.get(FEED))["Assets"]:
            self.assertTrue(publish.file_matches(self.bucket / entry["FileName"], entry), entry["FileName"])

    def test_the_first_release_publishes_a_signed_feed_with_its_package(self) -> None:
        work = self.release("0.1.1")

        self.assertEqual(self.published(), [("0.1.1", "Full")])
        self.assertEqual(self.store.get(FEED), (work / "feed" / FEED).read_bytes())
        self.assert_packages_match()
        self.assertTrue((self.bucket / PORTABLE).exists())

    def test_the_next_release_starts_from_the_newest_full_package_and_adds_to_the_feed(self) -> None:
        self.release("0.1.1")
        work = self.work()

        self.assertEqual(publish.prepare(self.store, self.key, "0.1.2", work), "new")

        newest = work / "releases" / "Roundhouse-0.1.1-osx-full.nupkg"
        self.assertEqual(newest.read_bytes(), (self.bucket / newest.name).read_bytes())
        self.assertEqual((work / "releases" / FEED).read_bytes(), self.store.get(FEED))
        self.pack(work, "0.1.2")
        publish.merge("0.1.2", 10, work)
        self.sign(work)
        publish.upload(self.store, self.key, "0.1.2", work)
        self.assertEqual(self.published(), [("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.2", "Full")])
        self.assert_packages_match()

    def test_releases_past_the_newest_few_leave_the_feed_and_the_bucket(self) -> None:
        for version in ("0.1.1", "0.1.2", "0.1.3"):
            self.release(version, keep=2)

        self.assertEqual(self.published(), [("0.1.2", "Delta"), ("0.1.2", "Full"), ("0.1.3", "Delta"),
                                            ("0.1.3", "Full")])
        self.assertFalse((self.bucket / "Roundhouse-0.1.1-osx-full.nupkg").exists())
        self.assert_packages_match()

    def test_a_feed_changed_in_the_bucket_is_never_signed(self) -> None:
        self.release("0.1.1")
        planted = json.loads(self.store.get(FEED))
        planted["Assets"].append(asset("9.0.0", "Full", b"not ours"))
        (self.bucket / FEED).write_text(json.dumps(planted))

        # The copy under staging/ still matches its signature, so the release goes ahead from that.
        self.release("0.1.2")
        self.assertEqual(self.published(), [("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.2", "Full")])

        (self.bucket / FEED).write_text(json.dumps(planted))
        (self.bucket / STAGED_FEED).write_text(json.dumps(planted))
        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, self.key, "0.1.3", self.work())

    def test_a_feed_signed_with_another_key_is_refused(self) -> None:
        self.release("0.1.1")
        _, other = make_key(self.dir / "run-1")

        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, other, "0.1.2", self.work())

    def test_an_unsigned_feed_is_refused_unless_nothing_was_ever_published(self) -> None:
        (self.bucket / FEED).write_text(json.dumps({"Assets": [asset("0.1.1", "Full", b"x")]}))
        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, self.key, "0.1.2", self.work())

        (self.bucket / FEED).unlink()
        (self.bucket / (FEED + ".sig")).write_bytes(bytes(64))
        with self.assertRaisesRegex(Refused, "no published feed matches its signature"):
            publish.prepare(self.store, self.key, "0.1.2", self.work())

    def test_a_newest_package_that_isnt_the_one_in_the_feed_is_refused(self) -> None:
        self.release("0.1.1")
        package = self.bucket / "Roundhouse-0.1.1-osx-full.nupkg"
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
        self.assertEqual((work / "feed" / FEED).read_bytes(), self.store.get(FEED))

    def test_running_again_after_only_the_packages_went_up_publishes_the_new_ones(self) -> None:
        self.release("0.1.1")
        with self.assertRaises(ConnectionError):
            self.release("0.1.2", store=FailingStore(str(self.bucket), STAGED_FEED))
        self.assertEqual(self.published(), [("0.1.1", "Full")])

        self.release("0.1.2")

        self.assertEqual(self.published(), [("0.1.1", "Full"), ("0.1.2", "Delta"), ("0.1.2", "Full")])
        self.assert_packages_match()

    def test_running_again_after_the_feed_went_up_without_its_signature_finishes(self) -> None:
        self.release("0.1.1")
        for failing in (FEED, FEED + ".sig"):
            version = "0.1.2" if failing == FEED else "0.1.3"
            with self.assertRaises(ConnectionError):
                self.release(version, store=FailingStore(str(self.bucket), failing))

            self.assertEqual(publish.prepare(self.store, self.key, version, self.work()), "published")

            self.assertEqual(self.store.get(FEED), self.store.get(STAGED_FEED))
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
        (work / "releases" / "Roundhouse-0.1.1-osx-full.nupkg").write_bytes(b"changed after vpk")

        with self.assertRaisesRegex(Refused, "doesn't match what vpk's feed says"):
            publish.merge("0.1.1", 10, work)

    def test_only_the_feed_as_merged_and_signed_is_uploaded(self) -> None:
        self.release("0.1.1")
        work = self.work()
        publish.prepare(self.store, self.key, "0.1.2", work)
        self.pack(work, "0.1.2")
        publish.merge("0.1.2", 10, work)
        feed = work / "feed" / FEED
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

    def test_a_run_starts_from_an_empty_directory(self) -> None:
        work = self.release("0.1.1")
        with self.assertRaisesRegex(Refused, "start from an empty directory"):
            publish.prepare(self.store, self.key, "0.1.2", work)

    def test_the_check_reads_what_apps_read(self) -> None:
        self.release("0.1.1")
        work = self.release("0.1.2")

        self.assertEqual(publish.check(self.serve(), self.key, "0.1.2", work, wait=0), [
            "0.1.1 Full Roundhouse-0.1.1-osx-full.nupkg 19",
            "0.1.2 Full Roundhouse-0.1.2-osx-full.nupkg 19",
            "0.1.2 Delta Roundhouse-0.1.2-osx-delta.nupkg 20",
        ])

    def test_the_check_refuses_a_feed_or_package_apps_wouldnt_get(self) -> None:
        first = self.release("0.1.1")
        work = self.release("0.1.2")
        url = self.serve()

        # Such as a cache still serving the last feed.
        for name in (FEED, FEED + ".sig"):
            shutil.copyfile(first / "feed" / name, self.bucket / name)
        with self.assertRaisesRegex(Refused, "doesn't serve the feed this run published"):
            publish.check(url, self.key, "0.1.2", work, attempts=2, wait=0)

        for name in (FEED, FEED + ".sig"):
            shutil.copyfile(work / "feed" / name, self.bucket / name)
        (self.bucket / "Roundhouse-0.1.2-osx-full.nupkg").unlink()
        with self.assertRaisesRegex(Refused, "Roundhouse-0.1.2-osx-full.nupkg is missing"):
            publish.check(url, self.key, "0.1.2", work, wait=0)

        _, other = make_key(self.dir)
        with self.assertRaisesRegex(Refused, "doesn't match the app's key"):
            publish.check(url, other, "0.1.2", work, wait=0)


if __name__ == "__main__":
    unittest.main()
