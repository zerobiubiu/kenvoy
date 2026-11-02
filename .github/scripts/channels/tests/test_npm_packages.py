"""npm 包回归：七个包目录、主包 shim 解析与转发、二进制提取及资产校验拒绝行为。"""

import contextlib
import hashlib
import io
import json
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from manifests import Assets, ChannelError, load_assets
from npm_packages import (
    binary_member,
    build,
    dependency_spec,
    main,
    npm_cpu,
    npm_os,
    package_name,
    require_zips,
)

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
NAMES = [
    "kenvoy",
    "kenvoy-win32-x64",
    "kenvoy-win32-arm64",
    "kenvoy-linux-x64",
    "kenvoy-linux-arm64",
    "kenvoy-darwin-x64",
    "kenvoy-darwin-arm64",
]


def make_fixture(root: Path, payloads=None):
    """造六份假 ZIP 与配套 assets.json，sha256 由真实 ZIP 计算。"""
    root = Path(root)
    zips = root / "zips"
    zips.mkdir()
    payloads = payloads or {}
    contents = {}
    entries = []
    for platform, arch, target, binary in COMBINATIONS:
        name = f"kenvoy-{VERSION}-{platform}-{arch}.zip"
        payload = payloads.get((platform, arch), f"kenvoy {VERSION} {platform} {arch}\n".encode())
        contents[(platform, arch)] = payload
        archive = zips / name
        with zipfile.ZipFile(archive, "w") as bundle:
            bundle.writestr(binary, payload)
        entries.append(
            {
                "platform": platform,
                "arch": arch,
                "target": target,
                "file": name,
                "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                "url": f"https://github.com/{REPOSITORY}/releases/download/v{VERSION}/{name}",
                "binary": binary,
            }
        )
    assets = root / "assets.json"
    assets.write_text(
        json.dumps(
            {"version": VERSION, "tag": f"v{VERSION}", "repository": REPOSITORY, "assets": entries}
        ),
        encoding="utf-8",
    )
    return assets, zips, contents


class NpmPackages(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.assets_path, self.zips, self.contents = make_fixture(self.root)
        self.assets = load_assets(self.assets_path)
        self.output = self.root / "npm"
        self.written = build(self.assets, self.zips, self.output)

    def test_generates_seven_publishable_directories(self):
        directories = sorted(path.name for path in self.output.iterdir())
        self.assertEqual(directories, sorted(NAMES))
        for name in NAMES:
            package = json.loads((self.output / name / "package.json").read_text(encoding="utf-8"))
            self.assertEqual(package["name"], name)
            self.assertEqual(package["version"], VERSION)
            self.assertIn("repository", package)
        self.assertTrue((self.output / "kenvoy" / "bin" / "kenvoy.js").is_file())

    def test_main_package_declares_platform_dependencies(self):
        package = json.loads((self.output / "kenvoy" / "package.json").read_text(encoding="utf-8"))
        self.assertEqual(package["bin"], {"kenvoy": "bin/kenvoy.js"})
        self.assertEqual(package["files"], ["bin/kenvoy.js"])
        self.assertEqual(package["engines"], {"node": ">=18"})
        self.assertEqual(sorted(package["optionalDependencies"]), sorted(NAMES[1:]))
        self.assertEqual(set(package["optionalDependencies"].values()), {VERSION})
        self.assertNotIn("os", package)
        self.assertNotIn("cpu", package)

    def test_platform_package_metadata(self):
        expected = {
            ("windows", "x86_64"): ("kenvoy-win32-x64", "win32", "x64", "kenvoy.exe"),
            ("windows", "ARM64"): ("kenvoy-win32-arm64", "win32", "arm64", "kenvoy.exe"),
            ("linux", "x86_64"): ("kenvoy-linux-x64", "linux", "x64", "kenvoy"),
            ("linux", "ARM64"): ("kenvoy-linux-arm64", "linux", "arm64", "kenvoy"),
            ("macos", "x86_64"): ("kenvoy-darwin-x64", "darwin", "x64", "kenvoy"),
            ("macos", "ARM64"): ("kenvoy-darwin-arm64", "darwin", "arm64", "kenvoy"),
        }
        for (platform, arch), (name, expected_os, expected_cpu, binary) in expected.items():
            with self.subTest(name=name):
                package = json.loads(
                    (self.output / name / "package.json").read_text(encoding="utf-8")
                )
                self.assertEqual(package["name"], name)
                self.assertEqual(package["os"], [expected_os])
                self.assertEqual(package["cpu"], [expected_cpu])
                self.assertEqual(package["bin"], {"kenvoy": f"bin/{binary}"})
                self.assertEqual(package["files"], [f"bin/{binary}"])
                self.assertNotIn("optionalDependencies", package)

    def test_binary_bytes_match_release_archive(self):
        for platform, arch, _, binary in COMBINATIONS:
            with self.subTest(platform=platform, arch=arch):
                name = package_name(platform, arch)
                extracted = (self.output / name / "bin" / binary).read_bytes()
                self.assertEqual(extracted, self.contents[(platform, arch)])
                self.assertTrue(extracted)
                if os.name != "nt":
                    self.assertTrue(os.access(self.output / name / "bin" / binary, os.X_OK))
        self.assertFalse((self.output / "kenvoy" / "bin" / "kenvoy.exe").exists())

    def test_main_shim_resolves_forwards_and_propagates_exit_code(self):
        shim = (self.output / "kenvoy" / "bin" / "kenvoy.js").read_text(encoding="utf-8")
        self.assertTrue(shim.startswith("#!/usr/bin/env node\n"))
        self.assertIn('const VERSION = "0.3.0";', shim)
        mapping = {
            "win32-x64": "kenvoy-win32-x64",
            "win32-arm64": "kenvoy-win32-arm64",
            "linux-x64": "kenvoy-linux-x64",
            "linux-arm64": "kenvoy-linux-arm64",
            "darwin-x64": "kenvoy-darwin-x64",
            "darwin-arm64": "kenvoy-darwin-arm64",
        }
        for key, name in mapping.items():
            self.assertIn(f'"{key}": "{name}"', shim)
        for os_name, binary in (("win32", "kenvoy.exe"), ("linux", "kenvoy"), ("darwin", "kenvoy")):
            self.assertIn(f'"{os_name}": "{binary}"', shim)
        self.assertIn("require.resolve(", shim)
        self.assertIn("/bin/${BINARIES[platform]}", shim)
        self.assertIn("process.argv.slice(2)", shim)
        self.assertIn('{ stdio: "inherit" }', shim)
        self.assertIn("process.exit(code === null ? 1 : code)", shim)
        self.assertIn("缺少可选依赖", shim)
        self.assertIn("process.kill(process.pid, signal)", shim)

    def test_name_mapping_is_pure_and_rejects_unknown(self):
        self.assertEqual(package_name("windows", "x86_64"), "kenvoy-win32-x64")
        self.assertEqual(package_name("windows", "ARM64"), "kenvoy-win32-arm64")
        self.assertEqual(package_name("macos", "x86_64"), "kenvoy-darwin-x64")
        self.assertEqual(package_name("linux", "ARM64"), "kenvoy-linux-arm64")
        with self.assertRaises(ChannelError):
            npm_os("freebsd")
        with self.assertRaises(ChannelError):
            npm_cpu("riscv64")
        self.assertEqual(
            dependency_spec(self.assets),
            {name: VERSION for name in NAMES[1:]},
        )

    def test_written_files_are_reported(self):
        self.assertEqual(len(self.written), 2 + 2 * len(COMBINATIONS))
        for path in self.written:
            self.assertTrue(path.is_relative_to(self.output.resolve()))

    def test_cli_publishes_all_packages(self):
        output = self.root / "npm-cli"
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = main(
                ["--assets", str(self.assets_path), "--zips", str(self.zips), "--output", str(output)]
            )
        self.assertEqual(code, 0)
        self.assertEqual(sorted(path.name for path in output.iterdir()), sorted(NAMES))
        self.assertIn("7 个 npm 包", stdout.getvalue())


class ZipExtraction(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.assets_path, self.zips, _ = make_fixture(self.root)
        self.assets: Assets = load_assets(self.assets_path)

    def archive(self, platform="linux", arch="x86_64") -> Path:
        return self.zips / f"kenvoy-{VERSION}-{platform}-{arch}.zip"

    def test_ambiguous_binary_rejected(self):
        with zipfile.ZipFile(self.archive(), "a") as bundle:
            bundle.writestr("nested/kenvoy", b"second")
        with self.assertRaises(ChannelError) as context:
            binary_member(self.archive(), "kenvoy")
        self.assertIn("内含多个", str(context.exception))

    def test_traversal_entry_rejected(self):
        with zipfile.ZipFile(self.zips / "evil.zip", "w") as bundle:
            bundle.writestr("../kenvoy", b"escape")
        with self.assertRaises(ChannelError) as context:
            binary_member(self.zips / "evil.zip", "kenvoy")
        self.assertIn("可疑路径条目", str(context.exception))

    def test_missing_binary_rejected(self):
        with zipfile.ZipFile(self.zips / "empty.zip", "w") as bundle:
            bundle.writestr("README.md", b"no binary here")
        with self.assertRaises(ChannelError) as context:
            binary_member(self.zips / "empty.zip", "kenvoy")
        self.assertIn("内缺少二进制", str(context.exception))

    def test_bad_zip_rejected(self):
        (self.zips / "broken.zip").write_bytes(b"not a zip")
        with self.assertRaises(ChannelError) as context:
            binary_member(self.zips / "broken.zip", "kenvoy")
        self.assertIn("不是合法 ZIP", str(context.exception))

    def test_checksum_mismatch_rejected_before_writing(self):
        with zipfile.ZipFile(self.archive("macos", "ARM64"), "a") as bundle:
            bundle.writestr("extra.txt", b"tampered")
        output = self.root / "npm"
        with self.assertRaises(ChannelError) as context:
            build(self.assets, self.zips, output)
        self.assertIn("sha256 与 assets.json 不一致", str(context.exception))
        self.assertFalse(output.exists())

    def test_missing_archive_rejected(self):
        self.archive("windows", "x86_64").unlink()
        with self.assertRaises(ChannelError) as context:
            build(self.assets, self.zips, self.root / "npm")
        self.assertIn("缺少资产归档", str(context.exception))

    def test_empty_binary_rejected(self):
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        assets_path, zips, _ = make_fixture(root, payloads={("linux", "x86_64"): b""})
        with self.assertRaises(ChannelError) as context:
            build(load_assets(assets_path), zips, root / "npm")
        self.assertIn("是空文件", str(context.exception))

    def test_missing_zips_argument_rejected(self):
        with self.assertRaises(ChannelError) as context:
            require_zips(None)
        self.assertIn("--zips", str(context.exception))
        with self.assertRaises(ChannelError) as context:
            require_zips(self.root / "absent")
        self.assertIn("--zips 目录不存在", str(context.exception))

    def test_cli_fails_without_zips(self):
        stderr = io.StringIO()
        output = self.root / "npm"
        with contextlib.redirect_stderr(stderr):
            code = main(["--assets", str(self.assets_path), "--output", str(output)])
        self.assertEqual(code, 1)
        self.assertIn("--zips", stderr.getvalue())
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
