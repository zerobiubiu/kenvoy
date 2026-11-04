"""验证已发布包不会因重跑重新构建，以及本地失败不会改写旧仓库。"""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import build_linux_packages
from manifests import ChannelError


class LinuxBuildTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.configs = self.root / "configs"
        self.previous = self.root / "previous"
        self.output = self.root / "output"
        self.configs.mkdir()
        self.previous.mkdir()
        self.plan = list(build_linux_packages.jobs("1.0.0"))
        for _, config, _, _ in self.plan:
            (self.configs / config).write_text("name: kenvoy\nversion: 1.0.0\n", encoding="utf-8")

    def run_build(self, output=None):
        build_linux_packages.build(self.configs, self.previous, output or self.output,
                                   "1.0.0", "nfpm")

    def test_existing_release_packages_and_signatures_are_not_regenerated(self):
        original = {}
        for kind, _, name, relative in self.plan:
            path = self.previous / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            original[name] = f"immutable signed {name}".encode()
            path.write_bytes(original[name])
            if kind == "archlinux":
                original[name + ".sig"] = f"original signature for {name}".encode()
                path.with_name(name + ".sig").write_bytes(original[name + ".sig"])

        def repack(command, **kwargs):
            Path(command[command.index("--target") + 1]).write_bytes(b"new build and signature")
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(build_linux_packages.subprocess, "run", side_effect=repack):
            self.run_build()
        self.assertEqual({path.name: path.read_bytes() for path in self.output.iterdir()}, original)

    def test_failed_new_package_does_not_modify_existing_release(self):
        _, _, _, relative = self.plan[0]
        old = self.previous / relative
        old.parent.mkdir(parents=True)
        old.write_bytes(b"previous release remains available")
        with mock.patch.object(build_linux_packages.subprocess, "run",
                               return_value=subprocess.CompletedProcess([], 17)):
            with self.assertRaises(ChannelError):
                self.run_build()
        self.assertEqual(old.read_bytes(), b"previous release remains available")
        self.assertFalse((self.output / self.plan[1][2]).exists())

    def test_output_cannot_reuse_a_previous_or_configuration_directory(self):
        marker = self.previous / "keep"
        marker.write_bytes(b"user-owned repository data")
        for target in [self.previous, self.previous / "nested", self.configs, self.root]:
            with self.subTest(target=target), self.assertRaises(ChannelError):
                self.run_build(target)
        self.assertEqual(marker.read_bytes(), b"user-owned repository data")
        self.assertFalse((self.previous / "nested").exists())


if __name__ == "__main__":
    unittest.main()
