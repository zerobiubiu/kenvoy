"""正式发布状态和归档摘要是写入渠道前的边界，缓存不能被盲目覆盖。"""

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import assets


class ReleaseAssetsTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.payload = b"verified official archive"
        self.name = "kenvoy-1.0.0-linux-x86_64.zip"
        self.target = self.root / self.name
        self.manifest = {
            "repository": "fixture/kenvoy", "tag": "v1.0.0",
            "assets": [{"file": self.name,
                        "url": f"https://github.com/fixture/kenvoy/releases/download/v1.0.0/{self.name}",
                        "sha256": hashlib.sha256(self.payload).hexdigest()}],
        }

    def test_unpublished_or_wrong_release_cannot_supply_channel_assets(self):
        release = {"tag_name": "v1.0.0", "draft": False, "prerelease": False,
                   "published_at": "2026-11-04T00:00:00Z", "assets": []}
        for changed in [{"draft": True}, {"prerelease": True}, {"published_at": None},
                        {"tag_name": "v2.0.0"}]:
            response = subprocess.CompletedProcess([], 0, json.dumps(release | changed), "")
            with self.subTest(changed=changed), mock.patch.object(assets.subprocess, "run", return_value=response):
                with self.assertRaises(assets.AssetError):
                    assets.release_assets("fixture/kenvoy", "v1.0.0")

    def test_matching_cached_archive_is_preserved_instead_of_redownloaded(self):
        self.target.write_bytes(self.payload)

        def unsafe_download(*args, **kwargs):
            self.target.write_bytes(b"different remote bytes")
            return subprocess.CompletedProcess([], 0)

        with mock.patch.object(assets.subprocess, "run", side_effect=unsafe_download):
            assets.download(self.root, self.manifest)
        self.assertEqual(self.target.read_bytes(), self.payload)

    def test_mismatched_cached_archive_is_rejected_without_replacement(self):
        original = b"retain unexpected cached data for diagnosis"
        self.target.write_bytes(original)

        def replacing_download(*args, **kwargs):
            self.target.write_bytes(self.payload)
            return subprocess.CompletedProcess([], 0)

        with mock.patch.object(assets.subprocess, "run", side_effect=replacing_download):
            with self.assertRaises(assets.AssetError):
                assets.download(self.root, self.manifest)
        self.assertEqual(self.target.read_bytes(), original)

    def test_downloaded_bytes_must_match_the_official_digest(self):
        def corrupted_download(*args, **kwargs):
            self.target.write_bytes(b"corrupt download")
            return subprocess.CompletedProcess([], 0)

        with mock.patch.object(assets.subprocess, "run", side_effect=corrupted_download):
            with self.assertRaises(assets.AssetError):
                assets.download(self.root, self.manifest)
        self.assertEqual(self.target.read_bytes(), b"corrupt download")


if __name__ == "__main__":
    unittest.main()
