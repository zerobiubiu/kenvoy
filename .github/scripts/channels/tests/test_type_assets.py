"""正式 Release 类型资产的发现、双摘要下载与不可覆盖缓存回归。"""

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.append(str(Path(__file__).resolve().parents[2] / "tests"))

import assets
import type_assets
from manifests import load_assets
from type_package_fixture import archive, type_files

VERSION = "1.2.3"
REPOSITORY = "fixture/kenvoy"


class OfficialTypeAssetsTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.name = f"kenvoy-types-{VERSION}.tgz"
        self.payloads = {
            f"kenvoy-{VERSION}-{platform}-{arch}.zip": f"official {platform} {arch}".encode()
            for platform in ("windows", "linux", "macos") for arch in ("x86_64", "ARM64")
        }
        self.payloads[self.name] = archive(type_files(), "package")
        digest = hashlib.sha256(self.payloads[self.name]).hexdigest()
        self.payloads[f"{self.name}.sha256"] = f"{digest}  {self.name}\n".encode()
        self.records = [{"name": name, "state": "uploaded", "size": len(payload),
                         "digest": f"sha256:{hashlib.sha256(payload).hexdigest()}"}
                        for name, payload in self.payloads.items()]
        self.downloaded = []

    def gh(self, arguments, **options):
        if arguments[:2] == ["gh", "api"]:
            return subprocess.CompletedProcess(arguments, 0, json.dumps({
                "tag_name": f"v{VERSION}", "draft": False, "prerelease": False,
                "published_at": "2026-11-06T00:00:00Z", "assets": self.records,
            }), "")
        self.assertEqual(arguments[:3], ["gh", "release", "download"])
        name = arguments[arguments.index("--pattern") + 1]
        directory = Path(arguments[arguments.index("--dir") + 1])
        self.downloaded.append(name)
        (directory / name).write_bytes(self.payloads[name])
        return subprocess.CompletedProcess(arguments, 0)

    def test_formal_release_preparation_fetches_and_validates_all_eight_archive_inputs(self):
        output = self.root / "assets.json"
        arguments = ["assets.py", "--version", VERSION, "--repository", REPOSITORY,
                     "--output", str(output), "--zips", str(self.root / "zips"),
                     "--types", str(self.root / "types")]
        with mock.patch.object(sys, "argv", arguments), mock.patch.object(assets.subprocess, "run", side_effect=self.gh):
            assets.main()
        manifest = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(set(self.downloaded), set(self.payloads))
        self.assertEqual(type_assets.verify(self.root / "types", manifest).name, self.name)
        self.assertEqual(load_assets(output).version, VERSION)
        self.assertEqual(len(load_assets(output).assets), 6)

    def test_missing_duplicate_or_unverifiable_type_asset_prevents_channel_manifest_creation(self):
        cases = [
            [item for item in self.records if item["name"] != self.name],
            [item for item in self.records if item["name"] != f"{self.name}.sha256"],
            self.records + [next(item for item in self.records if item["name"] == self.name)],
            [item | {"digest": None} if item["name"] == self.name else item for item in self.records],
            [item | {"size": 0} if item["name"] == self.name else item for item in self.records],
        ]
        for records in cases:
            with self.subTest(records=len(records)):
                with self.assertRaises(ValueError):
                    assets.build(VERSION, REPOSITORY, f"v{VERSION}", records)

    def test_damaged_existing_type_tarball_is_retained_and_never_replaced(self):
        directory = self.root / "types"
        directory.mkdir()
        original = b"retain damaged cached type asset"
        (directory / self.name).write_bytes(original)
        manifest = assets.build(VERSION, REPOSITORY, f"v{VERSION}", self.records)
        with mock.patch.object(assets.subprocess, "run", side_effect=self.gh):
            with self.assertRaises(assets.AssetError):
                type_assets.download(directory, manifest, assets.download)
        self.assertEqual((directory / self.name).read_bytes(), original)
        self.assertEqual(self.downloaded, [])

    def test_wrong_version_release_tag_cannot_supply_the_types_package(self):
        with self.assertRaises(assets.AssetError):
            assets.build(VERSION, REPOSITORY, "v9.9.9", self.records)


if __name__ == "__main__":
    unittest.main()
