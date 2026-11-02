"""Linux 包配置生成：架构映射、包内容、校验失败与路径边界。"""

import hashlib
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from manifests import ARCHES, PLATFORMS, ChannelError, load_assets
from linux_packages import _output_path, generate

VERSION = "0.3.0"
REPOSITORY = "zerobiubiu/kenvoy"
URL = "https://github.com/{repository}/releases/download/v{version}/{name}"
TARGETS = {
    ("windows", "x86_64"): "x86_64-pc-windows-msvc",
    ("windows", "ARM64"): "aarch64-pc-windows-msvc",
    ("linux", "x86_64"): "x86_64-unknown-linux-musl",
    ("linux", "ARM64"): "aarch64-unknown-linux-musl",
    ("macos", "x86_64"): "x86_64-apple-darwin",
    ("macos", "ARM64"): "aarch64-apple-darwin",
}
ELF_MACHINES = {"x86_64": 62, "ARM64": 183}


def elf(arch):
    header = bytearray(64)
    header[0:4] = b"\x7fELF"
    header[4] = 2
    header[5] = 1
    header[6] = 1
    header[18:20] = ELF_MACHINES[arch].to_bytes(2, "little")
    return bytes(header)


def field(text, name):
    prefix = f"{name}: "
    for line in text.splitlines():
        if line.startswith(prefix):
            return line[len(prefix):].strip().strip('"')
    raise AssertionError(f"缺少字段 {name}")


class PackageFixture:
    def __init__(self, root):
        self.root = root
        self.zips = root / "zips"
        self.zips.mkdir()
        self.output = root / "output"
        self.manifest = self.build()

    def payload(self, platform, arch):
        if platform == "windows":
            return b"MZ fixture windows"
        if platform == "linux":
            return elf(arch)
        return b"\xcf\xfa\xed\xfe fixture macos"

    def write_zip(self, platform, arch, binary=None, license_text="MIT License\n"):
        file = f"kenvoy-{VERSION}-{platform}-{arch}.zip"
        name = "kenvoy.exe" if platform == "windows" else "kenvoy"
        with zipfile.ZipFile(self.zips / file, "w") as archive:
            if binary is not None:
                archive.writestr(name, binary)
            if license_text is not None:
                archive.writestr("LICENSE", license_text)
            archive.writestr("README.md", "# Kenvoy\n")
        return file

    def build(self):
        assets = []
        for platform in PLATFORMS:
            for arch in ARCHES:
                file = self.write_zip(platform, arch, self.payload(platform, arch))
                assets.append({
                    "platform": platform,
                    "arch": arch,
                    "target": TARGETS[(platform, arch)],
                    "file": file,
                    "sha256": hashlib.sha256((self.zips / file).read_bytes()).hexdigest(),
                    "url": URL.format(repository=REPOSITORY, version=VERSION, name=file),
                    "binary": "kenvoy.exe" if platform == "windows" else "kenvoy",
                })
        return {"version": VERSION, "tag": f"v{VERSION}", "repository": REPOSITORY, "assets": assets}

    def update_digest(self, platform, arch):
        for item in self.manifest["assets"]:
            if item["platform"] == platform and item["arch"] == arch:
                item["sha256"] = hashlib.sha256((self.zips / item["file"]).read_bytes()).hexdigest()

    def write(self, manifest=None):
        path = self.root / "assets.json"
        path.write_text(json.dumps(manifest or self.manifest, ensure_ascii=False), encoding="utf-8")
        return path


class LinuxPackagesTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="kenvoy-linux-packages-")
        self.root = Path(self.directory.name)
        self.fixture = PackageFixture(self.root)
        self.assets = load_assets(self.fixture.write())

    def tearDown(self):
        self.directory.cleanup()

    def reload(self):
        self.assets = load_assets(self.fixture.write())
        return self.assets

    def test_six_assets_map_to_all_package_architectures(self):
        generate(self.assets, self.fixture.zips, self.fixture.output)
        expected = {
            "nfpm-deb.yaml": "amd64",
            "nfpm-deb-arm64.yaml": "arm64",
            "nfpm-rpm.yaml": "x86_64",
            "nfpm-rpm-aarch64.yaml": "aarch64",
            "nfpm-apk.yaml": "x86_64",
            "nfpm-apk-aarch64.yaml": "aarch64",
            "nfpm-archlinux.yaml": "x86_64",
            "nfpm-archlinux-aarch64.yaml": "aarch64",
        }
        for name, arch in expected.items():
            text = (self.fixture.output / name).read_text(encoding="utf-8")
            self.assertEqual(field(text, "arch"), arch, name)
            self.assertEqual(field(text, "version"), VERSION, name)
            self.assertEqual(field(text, "license"), "MIT", name)
            self.assertEqual(field(text, "homepage"), f"https://github.com/{REPOSITORY}", name)
            self.assertIn(f"https://github.com/{REPOSITORY}", text)
            self.assertIn("架构映射：", text)
            self.assertIn('dst: "/usr/bin/kenvoy"', text)
            self.assertIn('dst: "/usr/share/doc/kenvoy/LICENSE"', text)
            self.assertIn("mode: 0755", text)
            self.assertIn("mode: 0644", text)

    def test_staged_binary_and_license_are_real_files(self):
        generate(self.assets, self.fixture.zips, self.fixture.output)
        for arch in ARCHES:
            binary = self.fixture.output / "pkgroot" / arch / "usr/bin/kenvoy"
            license_path = self.fixture.output / "pkgroot" / arch / "usr/share/doc/kenvoy/LICENSE"
            self.assertEqual(binary.read_bytes()[:4], b"\x7fELF")
            self.assertEqual(license_path.read_text(encoding="utf-8"), "MIT License\n")

    def test_nfpm_sources_are_inside_output(self):
        generate(self.assets, self.fixture.zips, self.fixture.output)
        for name in ("nfpm-deb.yaml", "nfpm-rpm-aarch64.yaml"):
            text = (self.fixture.output / name).read_text(encoding="utf-8")
            sources = [line.split(":", 1)[1].strip().strip('"') for line in text.splitlines()
                       if line.strip().startswith("- src:")]
            self.assertEqual(len(sources), 2)
            for source in sources:
                self.assertTrue(Path(source).is_relative_to(self.fixture.output.resolve()), source)

    def test_pkgbuild_installs_both_archives(self):
        generate(self.assets, self.fixture.zips, self.fixture.output)
        text = (self.fixture.output / "arch/PKGBUILD").read_text(encoding="utf-8")
        self.assertIn(f"pkgver={VERSION}", text)
        self.assertIn("arch=('x86_64' 'aarch64')", text)
        self.assertIn("install -Dm755 \"$srcdir/kenvoy\"", text)
        self.assertIn("install -Dm644 \"$srcdir/LICENSE\"", text)
        for item in self.fixture.manifest["assets"]:
            if item["platform"] == "linux":
                self.assertIn(item["url"], text)
                self.assertIn(item["sha256"], text)

    def rejected(self, manifest):
        with self.assertRaises(ChannelError):
            load_assets(self.fixture.write(manifest))

    def test_missing_asset_is_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["assets"].pop()
        self.rejected(manifest)

    def test_duplicate_asset_is_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["assets"][-1] = dict(manifest["assets"][0])
        self.rejected(manifest)

    def test_bad_digest_is_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["assets"][0]["sha256"] = "A" * 64
        self.rejected(manifest)

    def test_foreign_url_is_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["assets"][0]["url"] = manifest["assets"][0]["url"].replace(REPOSITORY, "other/kenvoy")
        self.rejected(manifest)

    def test_bad_version_and_tag_are_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["version"] = f"v{VERSION}"
        self.rejected(manifest)
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["tag"] = "0.3.0"
        self.rejected(manifest)

    def test_traversing_asset_name_is_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["assets"][0]["file"] = "../kenvoy.zip"
        self.rejected(manifest)

    def test_wrong_binary_name_is_rejected(self):
        manifest = json.loads(json.dumps(self.fixture.manifest))
        manifest["assets"][0]["binary"] = "kenvoy"
        self.rejected(manifest)

    def test_zip_digest_mismatch_is_rejected(self):
        path = self.fixture.zips / "kenvoy-0.3.0-linux-x86_64.zip"
        path.write_bytes(path.read_bytes() + b"tampered")
        with self.assertRaises(ChannelError):
            generate(self.assets, self.fixture.zips, self.fixture.output)

    def test_wrong_elf_machine_is_rejected(self):
        self.fixture.write_zip("linux", "x86_64", binary=elf("ARM64"))
        self.fixture.update_digest("linux", "x86_64")
        with self.assertRaises(ChannelError) as caught:
            generate(self.reload(), self.fixture.zips, self.fixture.output)
        self.assertIn("ELF 架构", str(caught.exception))

    def test_missing_license_is_rejected(self):
        self.fixture.write_zip("linux", "ARM64", self.fixture.payload("linux", "ARM64"), license_text=None)
        self.fixture.update_digest("linux", "ARM64")
        with self.assertRaises(ChannelError) as caught:
            generate(self.reload(), self.fixture.zips, self.fixture.output)
        self.assertIn("LICENSE", str(caught.exception))

    def test_traversing_zip_member_is_not_used(self):
        file = "kenvoy-0.3.0-linux-x86_64.zip"
        with zipfile.ZipFile(self.fixture.zips / file, "w") as archive:
            archive.writestr("../kenvoy", elf("x86_64"))
            archive.writestr("README.md", "# Kenvoy\n")
            archive.writestr("LICENSE", "MIT License\n")
        self.fixture.update_digest("linux", "x86_64")
        with self.assertRaises(ChannelError):
            generate(self.reload(), self.fixture.zips, self.fixture.output)

    def test_output_traversal_is_rejected(self):
        with self.assertRaises(ChannelError):
            _output_path(self.fixture.output, "../escape")

    def test_output_equal_to_zips_is_rejected(self):
        with self.assertRaises(ChannelError):
            generate(self.assets, self.fixture.zips, self.fixture.zips)


if __name__ == "__main__":
    unittest.main()
