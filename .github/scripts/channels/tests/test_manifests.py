"""渠道清单回归：assets.json 严格校验与 brew/scoop 交付契约。"""

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from manifests import ChannelError, channel_files, load_assets, main, write_files

COMBINATIONS = (
    ("windows", "x86_64", "x86_64-pc-windows-msvc", "kenvoy.exe"),
    ("windows", "ARM64", "aarch64-pc-windows-msvc", "kenvoy.exe"),
    ("linux", "x86_64", "x86_64-unknown-linux-musl", "kenvoy"),
    ("linux", "ARM64", "aarch64-unknown-linux-musl", "kenvoy"),
    ("macos", "x86_64", "x86_64-apple-darwin", "kenvoy"),
    ("macos", "ARM64", "aarch64-apple-darwin", "kenvoy"),
)
VERSION = "0.3.0"
REPOSITORY = "fixture/kenvoy"


def make_assets(version=VERSION, repository=REPOSITORY):
    """构造契约内的六平台清单；每个组合用不同的 sha256 便于断言落位。"""
    entries = []
    for index, (platform, arch, target, binary) in enumerate(COMBINATIONS):
        name = f"kenvoy-{version}-{platform}-{arch}.zip"
        entries.append(
            {
                "platform": platform,
                "arch": arch,
                "target": target,
                "file": name,
                "sha256": f"{index + 1:02x}" * 32,
                "url": f"https://github.com/{repository}/releases/download/v{version}/{name}",
                "binary": binary,
            }
        )
    return {"version": version, "tag": f"v{version}", "repository": repository, "assets": entries}


def write_assets(directory, payload=None):
    path = Path(directory) / "assets.json"
    path.write_text(json.dumps(payload if payload is not None else make_assets()), encoding="utf-8")
    return path


class ChannelOutputs(unittest.TestCase):
    def setUp(self):
        self.assets = load_assets(write_assets(self.enterContext(tempfile.TemporaryDirectory())))
        self.files = channel_files(self.assets)

    def test_bundle_covers_every_channel(self):
        self.assertEqual(
            sorted(self.files),
            sorted(
                [
                    "brew/Formula/kenvoy.rb",
                    "scoop/bucket/kenvoy.json",
                ]
            ),
        )
        for relative, content in self.files.items():
            self.assertTrue(content.endswith("\n"), relative)
            self.assertNotIn("\r", content, relative)

    def test_brew_binds_url_and_sha_per_platform(self):
        formula = self.files["brew/Formula/kenvoy.rb"]
        self.assertIn("class Kenvoy < Formula", formula)
        self.assertIn('version "0.3.0"', formula)
        self.assertIn('bin.install "kenvoy"', formula)

        macos = formula.split("  on_macos do", 1)[1].split("  on_linux do", 1)[0]
        linux = formula.split("  on_linux do", 1)[1]
        for section, platform in ((macos, "macos"), (linux, "linux")):
            for label, arch in (("on_arm", "ARM64"), ("on_intel", "x86_64")):
                block = section.split(f"    {label} do", 1)[1].split("    end", 1)[0]
                asset = self.assets.select(platform, arch)
                self.assertIn(f'url "{asset.url}"', block)
                self.assertIn(f'sha256 "{asset.sha256}"', block)

    def test_scoop_selects_windows_architectures(self):
        manifest = json.loads(self.files["scoop/bucket/kenvoy.json"])
        self.assertEqual(manifest["version"], VERSION)
        self.assertEqual(manifest["bin"], "kenvoy.exe")
        self.assertEqual(manifest["checkver"]["github"], self.assets.homepage)
        for key, arch in (("64bit", "x86_64"), ("arm64", "ARM64")):
            asset = self.assets.select("windows", arch)
            self.assertEqual(manifest["architecture"][key]["url"], asset.url)
            self.assertEqual(manifest["architecture"][key]["hash"], f"sha256:{asset.sha256}")
            template = manifest["autoupdate"]["architecture"][key]["url"]
            self.assertEqual(template, asset.url.replace(VERSION, "$version"))



class AssetContract(unittest.TestCase):
    def assert_rejected(self, mutate, message):
        payload = make_assets()
        mutate(payload)
        with tempfile.TemporaryDirectory() as directory:
            path = write_assets(directory, payload)
            with self.assertRaises(ChannelError) as context:
                load_assets(path)
        self.assertIn(message, str(context.exception))

    def test_missing_asset_rejected(self):
        self.assert_rejected(lambda payload: payload["assets"].pop(), "必须恰好包含 6 个")

    def test_duplicate_combination_rejected(self):
        def mutate(payload):
            payload["assets"][1] = dict(payload["assets"][0])

        self.assert_rejected(mutate, "重复的平台组合")

    def test_unknown_combination_rejected(self):
        def mutate(payload):
            payload["assets"][0]["arch"] = "arm64"

        self.assert_rejected(mutate, "平台组合")

    def test_invalid_versions_rejected(self):
        for version in ("0.3", "v0.3.0", "01.2.3", "1.2.3-rc.1", ""):
            with self.subTest(version=version):
                self.assert_rejected(
                    lambda payload: payload.update(version=version), "version 必须是 X.Y.Z"
                )

    def test_tag_must_match_version(self):
        self.assert_rejected(lambda payload: payload.update(tag="v9.9.9"), "tag 必须等于 v0.3.0")

    def test_repository_must_be_owner_name(self):
        for repository in ("../kenvoy", "kenvoy", "zerobiubiu/kenvoy/extra", ""):
            with self.subTest(repository=repository):
                self.assert_rejected(
                    lambda payload: payload.update(repository=repository), "repository 必须是 owner/name"
                )

    def test_path_traversal_filename_rejected(self):
        for name in ("../../kenvoy-0.3.0-windows-x86_64.zip", "C:\\kenvoy.zip", "../kenvoy.exe"):
            with self.subTest(name=name):
                self.assert_rejected(
                    lambda payload: payload["assets"][0].update(file=name), "必须是 .zip 普通文件名"
                )

    def test_filename_must_match_platform_arch(self):
        def mutate(payload):
            payload["assets"][0]["file"] = "kenvoy-0.3.0-linux-ARM64.zip"

        self.assert_rejected(mutate, "与 platform/arch 不匹配")

    def test_target_filename_form_accepted(self):
        payload = make_assets()

        def rename(entry):
            entry["file"] = f"kenvoy-0.3.0-{entry['target']}.zip"
            entry["url"] = (
                f"https://github.com/{REPOSITORY}/releases/download/v0.3.0/{entry['file']}"
            )

        for entry in payload["assets"]:
            rename(entry)
        with tempfile.TemporaryDirectory() as directory:
            assets = load_assets(write_assets(directory, payload))
        self.assertEqual(assets.select("windows", "ARM64").file, "kenvoy-0.3.0-aarch64-pc-windows-msvc.zip")

    def test_sha256_must_be_lowercase_hex(self):
        for digest in ("A" * 64, "a" * 63, "g" * 64, "0" * 63 + "-"):
            with self.subTest(digest=digest[:8]):
                self.assert_rejected(
                    lambda payload: payload["assets"][0].update(sha256=digest), "64 位小写十六进制"
                )

    def test_url_must_point_to_this_repository_release(self):
        def other_repository(payload):
            payload["assets"][0]["url"] = payload["assets"][0]["url"].replace("fixture/", "someone/")

        def other_tag(payload):
            payload["assets"][0]["url"] = payload["assets"][0]["url"].replace("/v0.3.0/", "/v0.2.9/")

        for mutate in (other_repository, other_tag):
            with self.subTest(mutate=mutate.__name__):
                self.assert_rejected(mutate, "必须指向本仓库 v0.3.0 Release")

    def test_binary_must_match_platform(self):
        self.assert_rejected(
            lambda payload: payload["assets"][0].update(binary="kenvoy"), "binary 必须是 kenvoy.exe"
        )

    def test_non_object_assets_rejected(self):
        self.assert_rejected(lambda payload: payload.update(assets={}), "必须恰好包含 6 个")

    def test_missing_entry_field_rejected(self):
        self.assert_rejected(lambda payload: payload["assets"][2].pop("sha256"), "必须是非空字符串")

    def test_main_reports_failure_without_writing(self):
        payload = make_assets()
        payload["assets"].pop()
        output = Path(self.enterContext(tempfile.TemporaryDirectory())) / "out"
        with tempfile.TemporaryDirectory() as directory:
            path = write_assets(directory, payload)
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                self.assertEqual(main(["--assets", str(path), "--output", str(output)]), 1)
        self.assertIn("渠道清单生成失败", stderr.getvalue())
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
