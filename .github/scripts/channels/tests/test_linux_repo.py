"""Linux 仓库组装：四类索引、工具缺失失败、版本/架构校验与路径边界。"""

import gzip
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import linux_repo
from linux_repo import RepoError, assemble

VERSION = "0.3.0"


def write_apk(path, arch, version="0.3.0-r0", name="kenvoy"):
    text = f"pkgname = {name}\npkgver = {version}\narch = {arch}\n"
    payload = text.encode("utf-8")
    member = tarfile.TarInfo(".PKGINFO")
    member.size = len(payload)
    with tarfile.open(path, "w:gz") as archive:
        archive.addfile(member, io.BytesIO(payload))


def make_tar(path, member, content):
    payload = content.encode("utf-8")
    info = tarfile.TarInfo(member)
    info.size = len(payload)
    with tarfile.open(path, "w:gz") as archive:
        archive.addfile(info, io.BytesIO(payload))


class FakeTools:
    DEFAULT = ("apt-ftparchive", "createrepo_c", "apk", "repo-add")

    def __init__(self, enabled=None, create_outputs=True):
        self.enabled = set(self.DEFAULT if enabled is None else enabled)
        self.create_outputs = create_outputs
        self.calls = []

    def available(self, name):
        return name in self.enabled

    def stanzas(self, cwd, only_arch=None):
        pool = Path(cwd) / "pool/main/k/kenvoy"
        blocks = []
        for deb in sorted(pool.glob("*.deb")):
            name, version, arch = deb.name[:-4].rsplit("_", 2)
            if only_arch is not None and arch not in (only_arch, "all"):
                continue
            blocks.append(
                f"Package: {name}\nVersion: {version}\nArchitecture: {arch}\n"
                f"Filename: pool/main/k/kenvoy/{deb.name}\nSize: {deb.stat().st_size}\n"
                f"SHA256: {'a' * 64}"
            )
        return "\n\n".join(blocks) + ("\n" if blocks else "")

    def run(self, name, arguments, cwd=None):
        self.calls.append((name, list(arguments), Path(cwd) if cwd else None))
        if name == "apt-ftparchive":
            return subprocess.CompletedProcess([name], 0, self.stanzas(cwd), "")
        if name == "dpkg-scanpackages":
            arch = arguments[arguments.index("--arch") + 1]
            return subprocess.CompletedProcess([name], 0, self.stanzas(cwd, arch), "")
        if name == "createrepo_c" and self.create_outputs:
            target = Path(arguments[-1])
            (target / "repodata").mkdir(parents=True, exist_ok=True)
            (target / "repodata/repomd.xml").write_text("<repomd/>\n", encoding="utf-8")
        if name == "apk" and self.create_outputs:
            names = [value for value in arguments if value.endswith(".apk")]
            make_tar(Path(cwd) / "APKINDEX.tar.gz", "APKINDEX", "\n".join(names))
        if name == "repo-add" and self.create_outputs:
            make_tar(Path(cwd) / "kenvoy.db.tar.gz", "kenvoy-0.3.0-1/desc", "%DESC%\n")
        return subprocess.CompletedProcess([name], 0, "", "")


class LinuxRepoTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="kenvoy-linux-repo-")
        self.root = Path(self.directory.name)
        self.packages = self.root / "packages"
        self.packages.mkdir()
        self.repo = self.root / "site"
        self.write_packages()

    def tearDown(self):
        self.directory.cleanup()

    def write_packages(self):
        (self.packages / "debs").mkdir()
        (self.packages / "debs/kenvoy_0.3.0_amd64.deb").write_bytes(b"!<arch>\nfixture")
        (self.packages / "debs/kenvoy_0.3.0_arm64.deb").write_bytes(b"!<arch>\nfixture")
        (self.packages / "rpms").mkdir()
        (self.packages / "rpms/kenvoy-0.3.0-1.x86_64.rpm").write_bytes(b"rpm fixture")
        (self.packages / "rpms/kenvoy-0.3.0-1.aarch64.rpm").write_bytes(b"rpm fixture")
        for arch in ("x86_64", "aarch64"):
            directory = self.packages / "apks" / arch
            directory.mkdir(parents=True)
            write_apk(directory / "kenvoy-0.3.0-r0.apk", arch)
            arch_dir = self.packages / "arch" / arch
            arch_dir.mkdir(parents=True)
            (arch_dir / f"kenvoy-0.3.0-1-{arch}.pkg.tar.zst").write_bytes(b"arch fixture")

    def assemble_with(self, tools):
        with mock.patch.object(linux_repo, "_available", tools.available), \
                mock.patch.object(linux_repo, "_run", tools.run):
            assemble(self.packages, self.repo, VERSION)

    def test_builds_all_four_indexes(self):
        tools = FakeTools()
        self.assemble_with(tools)
        for arch in ("amd64", "arm64"):
            index = self.repo / f"apt/dists/stable/main/binary-{arch}/Packages"
            compressed = self.repo / f"apt/dists/stable/main/binary-{arch}/Packages.gz"
            self.assertTrue(index.is_file())
            self.assertEqual(index.read_bytes(), gzip.decompress(compressed.read_bytes()))
            self.assertIn(f"Architecture: {arch}", index.read_text(encoding="utf-8"))
            self.assertIn(f"kenvoy_0.3.0_{arch}.deb", index.read_text(encoding="utf-8"))
        release = (self.repo / "apt/dists/stable/Release").read_text(encoding="utf-8")
        self.assertIn("Architectures: amd64 arm64", release)
        self.assertIn("SHA256:", release)
        for arch in ("amd64", "arm64"):
            digest = hashlib.sha256(
                (self.repo / f"apt/dists/stable/main/binary-{arch}/Packages").read_bytes()
            ).hexdigest()
            self.assertIn(digest, release)
        self.assertTrue((self.repo / "rpm/x86_64/repodata/repomd.xml").is_file())
        self.assertTrue((self.repo / "rpm/aarch64/repodata/repomd.xml").is_file())
        self.assertTrue((self.repo / "rpm/x86_64/kenvoy-0.3.0-1.x86_64.rpm").is_file())
        for arch in ("x86_64", "aarch64"):
            index = self.repo / f"alpine/v3.20/main/{arch}/APKINDEX.tar.gz"
            self.assertTrue(index.is_file())
            with tarfile.open(index, "r:gz") as archive:
                self.assertIn("kenvoy-0.3.0-r0.apk", archive.extractfile("APKINDEX").read().decode())
            self.assertTrue((self.repo / f"arch/{arch}/kenvoy.db.tar.gz").is_file())
            with tarfile.open(self.repo / f"arch/{arch}/kenvoy.db.tar.gz", "r:gz") as archive:
                self.assertTrue(any(name.endswith("/desc") for name in archive.getnames()))
        commands = [call[0] for call in tools.calls]
        self.assertIn("apt-ftparchive", commands)
        self.assertEqual(commands.count("createrepo_c"), 2)
        self.assertIn("apk", commands)
        self.assertIn("repo-add", commands)
        apt_call = next(call for call in tools.calls if call[0] == "apt-ftparchive")
        self.assertEqual(apt_call[1], ["packages", "pool"])
        self.assertEqual(apt_call[2].name, "apt")

    def test_dpkg_scanpackages_fallback(self):
        tools = FakeTools(enabled=("dpkg-scanpackages", "createrepo_c", "apk", "repo-add"))
        self.assemble_with(tools)
        scans = [call for call in tools.calls if call[0] == "dpkg-scanpackages"]
        self.assertEqual(len(scans), 2)
        self.assertIn("--arch", scans[0][1])
        self.assertTrue((self.repo / "apt/dists/stable/main/binary-arm64/Packages").is_file())

    def test_missing_tools_fail_and_leave_repo_untouched(self):
        self.repo.mkdir()
        (self.repo / "CNAME").write_text("packages.example\n", encoding="utf-8")
        with self.assertRaises(RepoError) as caught:
            self.assemble_with(FakeTools(enabled=()))
        self.assertIn("apt-ftparchive", str(caught.exception))
        self.assertEqual(sorted(path.name for path in self.repo.iterdir()), ["CNAME"])

    def test_tool_without_output_fails_without_partial_repo(self):
        with self.assertRaises(RepoError) as caught:
            self.assemble_with(FakeTools(create_outputs=False))
        self.assertIn("repomd.xml", str(caught.exception))
        self.assertFalse(self.repo.exists())

    def test_existing_repo_subtrees_are_replaced(self):
        self.repo.mkdir()
        (self.repo / "CNAME").write_text("packages.example\n", encoding="utf-8")
        stale = self.repo / "apt/old.txt"
        stale.parent.mkdir(parents=True)
        stale.write_text("stale\n", encoding="utf-8")
        self.assemble_with(FakeTools())
        self.assertFalse(stale.exists())
        self.assertTrue((self.repo / "CNAME").is_file())
        self.assertTrue((self.repo / "apt/dists/stable/Release").is_file())

    def test_requested_version_must_be_present(self):
        with self.assertRaises(RepoError) as caught:
            assemble(self.packages, self.repo, "0.4.0")
        self.assertIn("0.4.0", str(caught.exception))
        self.assertFalse(self.repo.exists())

    def test_older_packages_accumulate(self):
        (self.packages / "debs/kenvoy_0.2.0_amd64.deb").write_bytes(b"!<arch>\nolder")
        self.assemble_with(FakeTools())
        index = (self.repo / "apt/dists/stable/main/binary-amd64/Packages").read_text(encoding="utf-8")
        self.assertIn("kenvoy_0.3.0_amd64.deb", index)
        self.assertIn("kenvoy_0.2.0_amd64.deb", index)

    def test_nfpm_generic_filenames_are_classified(self):
        directory = self.packages / "generic"
        directory.mkdir()
        expected = {
            "kenvoy_0.3.0_amd64.deb": ("deb", "amd64"),
            "kenvoy_0.3.0_x86_64.rpm": ("rpm", "x86_64"),
            "kenvoy_0.3.0_x86_64.pkg.tar.zst": ("arch", "x86_64"),
        }
        for name, (kind, arch) in expected.items():
            path = directory / name
            path.write_bytes(b"fixture")
            package = linux_repo.classify(path)
            self.assertEqual((package.kind, package.arch), (kind, arch), name)

    def test_unknown_architecture_is_rejected(self):
        (self.packages / "debs/kenvoy_0.3.0_armhf.deb").write_bytes(b"!<arch>\nfixture")
        with self.assertRaises(RepoError):
            self.assemble_with(FakeTools())

    def test_corrupt_apk_is_rejected(self):
        (self.packages / "apks/kenvoy-broken.apk").write_bytes(b"not a tar")
        with self.assertRaises(RepoError) as caught:
            self.assemble_with(FakeTools())
        self.assertIn("APK", str(caught.exception))

    def test_duplicate_package_is_rejected(self):
        other = self.packages / "debs-copy"
        other.mkdir()
        shutil.copy2(self.packages / "debs/kenvoy_0.3.0_amd64.deb",
                     other / "kenvoy_0.3.0_amd64.deb")
        with self.assertRaises(RepoError) as caught:
            self.assemble_with(FakeTools())
        self.assertIn("重名", str(caught.exception))

    def test_signature_files_are_ignored(self):
        (self.packages / "arch/kenvoy-0.3.0-1-x86_64.pkg.tar.zst.sig").write_bytes(b"sig")
        self.assemble_with(FakeTools())
        self.assertTrue((self.repo / "arch/x86_64/kenvoy.db.tar.gz").is_file())

    def test_repo_inside_packages_is_rejected(self):
        with self.assertRaises(RepoError):
            assemble(self.packages, self.packages / "site", VERSION)

    def test_path_escape_is_rejected(self):
        with self.assertRaises(RepoError):
            linux_repo._inside(self.root, "../escape")

    def test_symlinked_package_is_rejected(self):
        link = self.packages / "debs/linked.deb"
        try:
            os.symlink(self.packages / "debs/kenvoy_0.3.0_amd64.deb", link)
        except (OSError, NotImplementedError):
            self.skipTest("当前平台不支持创建符号链接")
        with self.assertRaises(RepoError):
            self.assemble_with(FakeTools())

    def test_empty_package_directory_is_rejected(self):
        empty = self.root / "empty"
        empty.mkdir()
        with self.assertRaises(RepoError):
            assemble(empty, self.repo, VERSION)


if __name__ == "__main__":
    unittest.main()
