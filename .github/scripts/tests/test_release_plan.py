"""发布可见行为回归：只发布 main 当前版本、跳过/补齐与标签前置条件。"""

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from publish_release import local_digest
from release_common import GitHub, ReleaseError
from release_plan import plan_release
from type_package_fixture import crate as source_crate, type_files

TARGETS = [
    "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin", "aarch64-apple-darwin",
]
REPOSITORY = "fixture/kenvoy"
URL = "{ repo }/releases/download/v{ version }/{ name }-{ version }-{ target }.zip"


class RegistryFixture:
    def __init__(self, existing=(), published_manifest=None, types_present=True):
        self.existing = set(existing)
        self.published_manifest = published_manifest
        self.queries = []
        self.types_present = types_present

    def registry_version(self, crate, version):
        self.queries.append((crate, version))
        if (crate, version) not in self.existing:
            return None
        return {"crate": crate, "num": version, "yanked": False}

    def download_crate(self, crate, version, metadata):
        files = type_files(version, REPOSITORY) if self.types_present else {}
        return source_crate(version, files=files)

    def package_manifest(self, payload, crate, version):
        return self.published_manifest


class GitHubFixture:
    def __init__(self, sha, tags=None, release=None):
        self.sha = sha
        self.tags = tags or {}
        self.record = release
        self.calls = []

    def main_sha(self):
        self.calls.append("main")
        return self.sha

    def release(self, tag):
        self.calls.append(tag)
        return self.record

    def tag_sha(self, tag):
        if tag not in self.tags:
            raise ReleaseError(f"远端缺少 {tag}；先推标签")
        return self.tags[tag]


class ReleasePlanTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="kenvoy-release-test-")
        self.root = Path(self.directory.name)
        self.git("init", "--initial-branch=main")
        self.git("config", "user.name", "发布规则测试")
        self.git("config", "user.email", "release-test@example.invalid")
        self.git("config", "core.hooksPath", str(self.root / "no-hooks"))
        (self.root / ".github").mkdir()
        (self.root / ".github/distribution-targets.json").write_text(
            json.dumps([{"target": target, "runner": "fixture"} for target in TARGETS]),
            encoding="utf-8",
        )

    def tearDown(self):
        self.directory.cleanup()

    def git(self, *arguments):
        result = subprocess.run(
            ["git", *arguments], cwd=self.root, capture_output=True,
            text=True, encoding="utf-8", check=True,
        )
        return result.stdout.strip()

    def version(self, version, tag=True, url=URL):
        for crate in ("kengine", "kenvoy"):
            path = self.root / "crates" / crate
            path.mkdir(parents=True, exist_ok=True)
            text = f'[package]\nname = "{crate}"\nversion = "{version}"\n'
            if crate == "kenvoy":
                text += f'[package.metadata.binstall]\npkg-url = "{url}"\n'
            (path / "Cargo.toml").write_text(text, encoding="utf-8")
        self.git("add", ".")
        self.git("commit", "--no-gpg-sign", "-m", f"版本 {version}")
        sha = self.git("rev-parse", "HEAD")
        if tag:
            self.git("tag", "-a", f"v{version}", "-m", version)
        return sha

    def environ(self, sha, ref="refs/heads/main"):
        return {"GITHUB_REF": ref, "GITHUB_EVENT_NAME": "push", "GITHUB_SHA": sha,
                "GITHUB_REPOSITORY": REPOSITORY}

    def published_manifest(self, version):
        return {"package": {"name": "kenvoy", "version": version,
                            "metadata": {"binstall": {"pkg-url": URL}}}}

    def assets(self, version):
        binaries = [
            {"name": f"kenvoy-{version}-{target}.zip{suffix}", "state": "uploaded", "size": 100}
            for target in TARGETS for suffix in ("", ".sha256")
        ]
        return binaries + [
            {"name": f"kenvoy-types-{version}.tgz{suffix}", "state": "uploaded", "size": 100}
            for suffix in ("", ".sha256")
        ]

    def test_complete_current_version_is_noop_without_scanning_older_release(self):
        self.version("0.1.0")
        sha = self.version("0.2.0")
        github = GitHubFixture(sha, release={"draft": False, "assets": self.assets("0.2.0")})
        registry = RegistryFixture(
            [("kenvoy", "0.2.0"), ("kengine", "0.2.0")], self.published_manifest("0.2.0")
        )
        plan = plan_release(self.root, self.environ(sha), github, registry)
        self.assertFalse(plan["active"])
        self.assertEqual(plan["matrix"]["include"], [])
        self.assertEqual(registry.queries, [("kenvoy", "0.2.0"), ("kengine", "0.2.0")])
        self.assertEqual(github.calls, ["main", "v0.2.0"])

    def test_current_patch_publishes_but_missing_older_versions_are_not_candidates(self):
        self.version("0.1.0")
        self.version("0.2.0")
        sha = self.version("0.2.1")
        github = GitHubFixture(sha, {"v0.2.1": sha})
        registry = RegistryFixture()
        plan = plan_release(self.root, self.environ(sha), github, registry)
        self.assertEqual(plan["version"], "0.2.1")
        self.assertTrue(plan["publish_engine"])
        self.assertTrue(plan["publish_kenvoy"])
        self.assertTrue(plan["release_required"])
        self.assertEqual({item["target"] for item in plan["matrix"]["include"]}, set(TARGETS))
        self.assertEqual({version for _, version in registry.queries}, {"0.2.1"})
        self.assertEqual(plan["types_archive"], "kenvoy-types-0.2.1.tgz")

    def test_dev_is_rejected_before_any_remote_lookup(self):
        sha = self.version("0.2.1")
        github = GitHubFixture(sha)
        registry = RegistryFixture()
        with self.assertRaises(ReleaseError):
            plan_release(self.root, self.environ(sha, "refs/heads/dev"), github, registry)
        self.assertEqual(github.calls, [])
        self.assertEqual(registry.queries, [])

    def test_superseded_main_run_is_noop(self):
        sha = self.version("0.2.1")
        github = GitHubFixture("1" * 40)
        registry = RegistryFixture()
        plan = plan_release(self.root, self.environ(sha), github, registry)
        self.assertFalse(plan["active"])
        self.assertEqual(registry.queries, [])
        self.assertIn("最新提交", plan["reason"])

    def test_missing_remote_tag_blocks_unpublished_version(self):
        sha = self.version("0.2.1")
        with self.assertRaises(ReleaseError):
            plan_release(self.root, self.environ(sha), GitHubFixture(sha), RegistryFixture())

    def test_source_changes_after_tag_cannot_publish_under_old_tag(self):
        tag_sha = self.version("0.2.1")
        (self.root / "new-source.rs").write_text("fn changed() {}\n", encoding="utf-8")
        self.git("add", ".")
        self.git("commit", "--no-gpg-sign", "-m", "标签之后继续修改")
        sha = self.git("rev-parse", "HEAD")
        with self.assertRaises(ReleaseError):
            plan_release(self.root, self.environ(sha), GitHubFixture(sha, {"v0.2.1": tag_sha}), RegistryFixture())

    def test_existing_release_uses_published_names_and_builds_only_missing_zip(self):
        tag_sha = self.version("0.2.0")
        (self.root / "note.txt").write_text("工作流修订不修改既有发布包\n", encoding="utf-8")
        manifest = self.root / "crates/kenvoy/Cargo.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace("{ target }.zip", "{ os-name }-{ target-arch }.zip"),
            encoding="utf-8",
        )
        self.git("add", ".")
        self.git("commit", "--no-gpg-sign", "-m", "已发布后修订文档")
        sha = self.git("rev-parse", "HEAD")
        missing = f"kenvoy-0.2.0-{TARGETS[1]}.zip"
        assets = [item for item in self.assets("0.2.0") if item["name"] not in (missing, missing + ".sha256")]
        github = GitHubFixture(sha, {"v0.2.0": tag_sha}, {"draft": False, "assets": assets})
        registry = RegistryFixture(
            [("kenvoy", "0.2.0"), ("kengine", "0.2.0")], self.published_manifest("0.2.0")
        )
        plan = plan_release(self.root, self.environ(sha), github, registry)
        self.assertFalse(plan["publish_kenvoy"])
        self.assertEqual([item["archive"] for item in plan["matrix"]["include"]], [missing])

    def test_missing_sidecar_does_not_rebuild_or_replace_existing_zip(self):
        sha = self.version("0.2.0")
        missing = f"kenvoy-0.2.0-{TARGETS[-1]}.zip.sha256"
        assets = [item for item in self.assets("0.2.0") if item["name"] != missing]
        github = GitHubFixture(sha, {"v0.2.0": sha}, {"draft": False, "assets": assets})
        registry = RegistryFixture(
            [("kenvoy", "0.2.0"), ("kengine", "0.2.0")], self.published_manifest("0.2.0")
        )
        plan = plan_release(self.root, self.environ(sha), github, registry)
        self.assertTrue(plan["release_required"])
        self.assertEqual(plan["matrix"]["include"], [])

    def test_type_tarball_missing_does_not_rebuild_existing_binary_archives(self):
        sha = self.version("0.2.0")
        assets = [item for item in self.assets("0.2.0") if ".tgz" not in item["name"]]
        github = GitHubFixture(sha, {"v0.2.0": sha}, {"draft": False, "assets": assets})
        registry = RegistryFixture(
            [("kenvoy", "0.2.0"), ("kengine", "0.2.0")], self.published_manifest("0.2.0")
        )
        plan = plan_release(self.root, self.environ(sha), github, registry)
        self.assertTrue(plan["release_required"])
        self.assertFalse(plan["publish_kenvoy"])
        self.assertEqual(plan["matrix"]["include"], [])
        self.assertEqual(plan["types_archive"], "kenvoy-types-0.2.0.tgz")

    def test_old_published_crate_without_types_fails_instead_of_using_checkout(self):
        sha = self.version("0.2.0")
        registry = RegistryFixture(
            [("kenvoy", "0.2.0"), ("kengine", "0.2.0")], self.published_manifest("0.2.0"),
            types_present=False,
        )
        with self.assertRaises(ValueError):
            plan_release(self.root, self.environ(sha), GitHubFixture(sha), registry)

    def test_network_authorization_failure_is_not_treated_as_missing_release(self):
        error = urllib.error.HTTPError("https://api.github.com", 403, "Forbidden", {}, None)
        with patch("urllib.request.urlopen", side_effect=error):
            with self.assertRaises(ReleaseError):
                GitHub(REPOSITORY, token="").release("v0.2.1")

    def test_checksum_must_cover_exact_archive_bytes_and_filename(self):
        name = "kenvoy-0.2.1-windows-ARM64.zip"
        archive = self.root / name
        archive.write_bytes(b"immutable archive")
        sidecar = self.root / (name + ".sha256")
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        sidecar.write_text(f"{digest}  wrong.zip\n", encoding="ascii")
        with self.assertRaises(ReleaseError):
            local_digest(self.root, name)
        sidecar.write_text(f"{'0' * 64}  {name}\n", encoding="ascii")
        with self.assertRaises(ReleaseError):
            local_digest(self.root, name)
        sidecar.write_text(f"{digest}  {name}\n", encoding="ascii")
        self.assertEqual(local_digest(self.root, name), digest)


if __name__ == "__main__":
    unittest.main()
