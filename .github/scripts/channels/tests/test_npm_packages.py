"""单包生成回归：可消费的运行时布局、下载元数据与写入前失败边界。"""

import contextlib
import hashlib
import io
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import npm_packages
from manifests import ChannelError, load_assets
from npm_packages import archive_member, build, main, require_zips

COMBINATIONS = (
    ("windows", "x86_64", "x86_64-pc-windows-msvc", "kenvoy.exe", "win32-x64"),
    ("windows", "ARM64", "aarch64-pc-windows-msvc", "kenvoy.exe", "win32-arm64"),
    ("linux", "x86_64", "x86_64-unknown-linux-musl", "kenvoy", "linux-x64"),
    ("linux", "ARM64", "aarch64-unknown-linux-musl", "kenvoy", "linux-arm64"),
    ("macos", "x86_64", "x86_64-apple-darwin", "kenvoy", "darwin-x64"),
    ("macos", "ARM64", "aarch64-apple-darwin", "kenvoy", "darwin-arm64"),
)
VERSION = "0.3.0"
REPOSITORY = "fixture/kenvoy"


def make_fixture(root: Path, payloads=None, *, legacy=False, version=VERSION):
    """ZIP 内容与摘要独立构造；可覆盖旧三元组资产名。"""
    zips = root / "zips"
    zips.mkdir()
    contents, entries = {}, []
    for platform, arch, target, binary, _ in COMBINATIONS:
        suffix = target if legacy else f"{platform}-{arch}"
        name = f"kenvoy-{version}-{suffix}.zip"
        payload = (payloads or {}).get((platform, arch), f"{version} {target}\n".encode() * 20)
        contents[(platform, arch)] = payload
        archive = zips / name
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr(f"release/bin/{binary}", payload)
            bundle.writestr("release/LICENSE", "MIT License\r\nCopyright (c) Kenvoy fixture\r\n")
        entries.append({
            "platform": platform, "arch": arch, "target": target, "file": name,
            "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "url": f"https://github.com/{REPOSITORY}/releases/download/v{version}/{name}",
            "binary": binary,
        })
    assets = root / "assets.json"
    assets.write_text(json.dumps({
        "version": version, "tag": f"v{version}", "repository": REPOSITORY, "assets": entries,
    }), encoding="utf-8")
    return assets, zips, contents


def make_runtime(root: Path) -> Path:
    """独立工程输入，检验生成后入口能解析包根及共享模块；不代替真实安装器测试。"""
    runtime = root / "runtime"
    (runtime / "lib").mkdir(parents=True)
    (runtime / "package.json").write_text(json.dumps({
        "private": True, "engines": {"node": ">=18"},
        "dependencies": {"fflate": "1.2.3", "https-proxy-agent": "4.5.6", "proxy-from-env": "7.8.9"},
    }), encoding="utf-8")
    (runtime / "lib" / "config.cjs").write_text(
        "const fs = require('node:fs');\n"
        "exports.lookup = (root, key) => {\n"
        "  const data = JSON.parse(fs.readFileSync(root + '/download-manifest.json', 'utf8'));\n"
        "  if (!data.targets[key]) throw new Error('unsupported fixture target');\n"
        "  return {version: data.version, ...data.targets[key]};\n"
        "};\n", encoding="utf-8",
    )
    (runtime / "install.cjs").write_text(
        "const {lookup} = require('./lib/config.cjs');\n"
        "console.log(JSON.stringify(lookup(__dirname, process.argv[2])));\n", encoding="utf-8",
    )
    (runtime / "run.cjs").write_text(
        "#!/usr/bin/env node\n"
        "const path = require('node:path');\n"
        "const {lookup} = require('../lib/config.cjs');\n"
        "const data = lookup(path.resolve(__dirname, '..'), process.argv[2]);\n"
        "console.log(JSON.stringify({...data, args: process.argv.slice(3)}));\n", encoding="utf-8",
    )
    return runtime


class FixtureCase(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.assets_path, self.zips, self.contents = make_fixture(self.root)
        self.assets = load_assets(self.assets_path)
        self.output = self.root / "npm"
        self.runtime = make_runtime(self.root)
        self.enterContext(mock.patch.object(npm_packages, "RUNTIME_ROOT", self.runtime))

    def replace_archive(self, entries):
        asset = self.assets.select("linux", "x86_64")
        archive = self.zips / asset.file
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(archive, "w") as bundle:
                for name, value in entries:
                    bundle.writestr(name, value)
        self.refresh_archive_digest(archive)
        return archive

    def refresh_archive_digest(self, archive):
        data = json.loads(self.assets_path.read_text(encoding="utf-8"))
        for item in data["assets"]:
            if item["file"] == archive.name:
                item["sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
        self.assets_path.write_text(json.dumps(data), encoding="utf-8")
        self.assets = load_assets(self.assets_path)

    def assert_rejected_without_output(self):
        with self.assertRaises(ChannelError):
            build(self.assets, self.zips, self.output)
        self.assertFalse(self.output.exists())


class NpmPackages(FixtureCase):
    def test_package_has_install_contract_without_bundled_binaries(self):
        build(self.assets, self.zips, self.output)
        package = self.output / "kenvoy"
        data = json.loads((package / "package.json").read_text(encoding="utf-8"))
        self.assertEqual({entry.name for entry in self.output.iterdir()}, {"kenvoy"})
        self.assertEqual((data["name"], data["version"]), ("kenvoy", VERSION))
        self.assertEqual(data["bin"], {"kenvoy": "bin/kenvoy.cjs"})
        self.assertEqual(data["scripts"], {"postinstall": "node install.cjs"})
        self.assertEqual(data["engines"], {"node": ">=18"})
        self.assertEqual(set(data["files"]), {"bin", "lib", "install.cjs", "download-manifest.json"})
        self.assertNotIn("optionalDependencies", data)
        self.assertFalse(any(name.startswith("kenvoy") for name in data["dependencies"]))
        self.assertFalse((package / "vendor").exists())
        self.assertFalse(any(path.name in ("kenvoy", "kenvoy.exe") for path in package.rglob("*")
                             if path.is_file()))
        if os.name != "nt":
            self.assertTrue(os.access(package / "bin" / "kenvoy.cjs", os.X_OK))

    def test_downloads_bind_each_target_to_verified_archive_and_member(self):
        build(self.assets, self.zips, self.output)
        data = json.loads((self.output / "kenvoy" / "download-manifest.json").read_text(encoding="utf-8"))
        self.assertEqual((data["formatVersion"], data["version"], data["repository"]),
                         (1, VERSION, REPOSITORY))
        self.assertEqual(set(data["targets"]), {entry[4] for entry in COMBINATIONS})
        for platform, arch, _, binary, key in COMBINATIONS:
            with self.subTest(target=key):
                asset = self.assets.select(platform, arch)
                target = data["targets"][key]
                payload = self.contents[(platform, arch)]
                self.assertEqual(target["url"], asset.url)
                self.assertEqual(target["archiveSha256"], asset.sha256)
                self.assertEqual(target["archiveSize"], (self.zips / asset.file).stat().st_size)
                self.assertEqual(target["member"], f"release/bin/{binary}")
                self.assertEqual(target["binarySha256"], hashlib.sha256(payload).hexdigest())
                self.assertEqual(target["binarySize"], len(payload))
                self.assertEqual(target["executable"], binary)

    def test_generated_entries_resolve_shared_module_from_unrelated_cwd(self):
        node = shutil.which("node")
        if node is None:
            self.skipTest("需要 Node.js 验证生成后运行时布局")
        build(self.assets, self.zips, self.output)
        package = self.output / "kenvoy"
        for entry in ("install.cjs", "bin/kenvoy.cjs"):
            with self.subTest(entry=entry):
                result = subprocess.run(
                    [node, str(package / entry), "linux-arm64", "空 格", "--help"],
                    cwd=self.root, capture_output=True, text=True, encoding="utf-8",
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                selected = json.loads(result.stdout)
                self.assertEqual(selected["version"], VERSION)
                self.assertEqual(selected["executable"], "kenvoy")
                self.assertIn("linux-ARM64.zip", selected["url"])
                if entry.startswith("bin/"):
                    self.assertEqual(selected["args"], ["空 格", "--help"])

    def test_dependency_version_comes_from_runtime_manifest(self):
        source = self.runtime / "package.json"
        data = json.loads(source.read_text(encoding="utf-8"))
        data["dependencies"]["fflate"] = "9.8.7"
        source.write_text(json.dumps(data), encoding="utf-8")
        build(self.assets, self.zips, self.output)
        package = json.loads((self.output / "kenvoy" / "package.json").read_text(encoding="utf-8"))
        self.assertEqual(package["dependencies"]["fflate"], "9.8.7")
        self.assertEqual(set(package["dependencies"]), {"fflate", "https-proxy-agent", "proxy-from-env"})

    def test_historical_release_keeps_actual_triple_urls(self):
        historical = self.root / "historical"
        historical.mkdir()
        assets, zips, _ = make_fixture(historical, legacy=True, version="0.2.0")
        build(load_assets(assets), zips, self.output)
        data = json.loads((self.output / "kenvoy" / "download-manifest.json").read_text(encoding="utf-8"))
        for _, _, triple, _, key in COMBINATIONS:
            self.assertTrue(data["targets"][key]["url"].endswith(f"v0.2.0/kenvoy-0.2.0-{triple}.zip"))
        self.assertEqual(data["version"], "0.2.0")

    def test_cli_produces_the_single_package_directory(self):
        with contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = main(["--assets", str(self.assets_path), "--zips", str(self.zips),
                         "--output", str(self.output)])
        self.assertEqual(code, 0)
        self.assertIn(f"kenvoy@{VERSION}", stdout.getvalue())
        self.assertTrue((self.output / "kenvoy" / "download-manifest.json").is_file())
        self.assertFalse((self.output / "package.json").exists())

    def test_runtime_dependency_and_entry_failures_leave_no_package(self):
        manifest = self.runtime / "package.json"
        original = manifest.read_text(encoding="utf-8")
        cases = [("private", False), ("engines", {"node": ">=14"}),
                 ("dependencies", {}), ("dependencies", {"fflate": "^1.2.3"})]
        unlocked = json.loads(original)["dependencies"]
        unlocked["fflate"] = "^1.2.3"
        cases.append(("dependencies", unlocked))
        for field, value in cases:
            with self.subTest(field=field, value=value):
                data = json.loads(original)
                data[field] = value
                manifest.write_text(json.dumps(data), encoding="utf-8")
                self.assert_rejected_without_output()
        manifest.write_text(original, encoding="utf-8")
        (self.runtime / "run.cjs").unlink()
        self.assert_rejected_without_output()

    def test_nonempty_output_is_not_overwritten_or_mixed_with_old_packages(self):
        self.output.mkdir()
        sentinel = self.output / "existing-user-file"
        sentinel.write_text("preserve", encoding="utf-8")
        with self.assertRaises(ChannelError):
            build(self.assets, self.zips, self.output)
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "preserve")
        self.assertFalse((self.output / "kenvoy").exists())


class ArchiveFailures(FixtureCase):
    def test_missing_empty_or_ambiguous_members_fail_before_writing(self):
        cases = [
            [("LICENSE", b"MIT")],
            [("kenvoy", b"program")],
            [("kenvoy", b""), ("LICENSE", b"MIT")],
            [("kenvoy", b"program"), ("LICENSE", b"")],
            [("kenvoy", b"one"), ("nested/kenvoy", b"two")],
            [("kenvoy", b"one"), ("kenvoy", b"two")],
            [("kenvoy", b"program"), ("LICENSE", b"one"), ("sub/LICENSE", b"two")],
        ]
        for entries in cases:
            with self.subTest(entries=entries):
                self.replace_archive(entries)
                self.assert_rejected_without_output()

    def test_unsafe_paths_including_unrelated_directory_entries_are_rejected(self):
        for name in ("../kenvoy", "/kenvoy", "a/../b", "./kenvoy", "a//b", "C:/kenvoy",
                     "a\\kenvoy", "../escape/"):
            with self.subTest(name=name):
                # Windows ZIP 写入会规范化反斜杠；替换等长原始名称，确保负例真实存在。
                stored = name.replace("\\", "?")
                archive = self.replace_archive([
                    ("kenvoy", b"program"), ("LICENSE", b"MIT"), (stored, b"bad"),
                ])
                if stored != name:
                    archive.write_bytes(archive.read_bytes().replace(stored.encode(), name.encode()))
                    self.refresh_archive_digest(archive)
                with zipfile.ZipFile(archive) as bundle:
                    self.assertIn(name, [entry.orig_filename for entry in bundle.infolist()])
                self.assert_rejected_without_output()

    def test_nul_in_original_zip_member_name_is_rejected(self):
        archive = self.replace_archive([
            ("kenvoy", b"program"), ("LICENSE", b"MIT"), ("bad-name", b"data"),
        ])
        archive.write_bytes(archive.read_bytes().replace(b"bad-name", b"bad\x00name"))
        self.refresh_archive_digest(archive)
        self.assert_rejected_without_output()

    def test_symlink_member_is_not_treated_as_binary(self):
        link = zipfile.ZipInfo("kenvoy")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        self.replace_archive([(link, b"outside"), ("LICENSE", b"MIT")])
        self.assert_rejected_without_output()

    def test_unsupported_compression_is_rejected_before_generating_installer(self):
        member = zipfile.ZipInfo("kenvoy")
        member.compress_type = zipfile.ZIP_BZIP2
        self.replace_archive([(member, b"program"), ("LICENSE", b"MIT")])
        self.assert_rejected_without_output()

    def test_last_archive_checksum_failure_leaves_no_package(self):
        archive = self.zips / self.assets.select("macos", "ARM64").file
        with zipfile.ZipFile(archive, "a") as bundle:
            bundle.writestr("extra.txt", b"tampered")
        self.assert_rejected_without_output()

    def test_missing_archive_leaves_no_package(self):
        (self.zips / self.assets.select("windows", "x86_64").file).unlink()
        self.assert_rejected_without_output()

    def test_bad_zip_has_controlled_error(self):
        archive = self.root / "broken.zip"
        archive.write_bytes(b"not a zip")
        with self.assertRaises(ChannelError):
            archive_member(archive, "kenvoy")

    def test_crc_error_in_member_does_not_emit_partial_package(self):
        archive = self.replace_archive([("kenvoy", b"unique-binary-payload"), ("LICENSE", b"MIT")])
        content = archive.read_bytes().replace(b"unique-binary-payload", b"broken-binary-payload", 1)
        archive.write_bytes(content)
        self.refresh_archive_digest(archive)
        self.assert_rejected_without_output()

    def test_missing_zips_fails_without_writing(self):
        for missing in (None, self.root / "absent"):
            with self.subTest(path=missing), self.assertRaises(ChannelError):
                require_zips(missing)
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            code = main(["--assets", str(self.assets_path), "--output", str(self.output)])
        self.assertEqual(code, 1)
        self.assertIn("--zips", stderr.getvalue())
        self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
