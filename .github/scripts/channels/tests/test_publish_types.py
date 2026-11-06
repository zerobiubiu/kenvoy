"""真实包内容的同版 npm 编排：纯类型先行、逐包恢复、只读默认与失败关闭。"""

import contextlib
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock
from urllib.error import HTTPError, URLError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.append(str(Path(__file__).resolve().parents[2] / "tests"))

import npm_packages
import publish_npm
import publish_types
import type_assets
from manifests import load_assets
from test_npm_packages import REPOSITORY, VERSION, make_fixture, make_runtime
from type_package_fixture import archive, type_files

SECRET = "paired-publisher-secret-must-not-appear"


class Response(io.BytesIO):
    status = 200


class PairedNpmPublisherTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.assets, zips, _ = make_fixture(self.root)
        runtime = make_runtime(self.root)
        self.enterContext(mock.patch.object(npm_packages, "RUNTIME_ROOT", runtime))
        npm_packages.build(load_assets(self.assets), zips, self.root / "npm")
        self.installer = self.root / "npm/kenvoy"
        self.types = self.root / "types"
        self.types.mkdir()
        self.tarball = self.types / f"kenvoy-types-{VERSION}.tgz"
        self.tarball.write_bytes(archive(type_files(VERSION), "package"))
        self.refresh_type_digests()
        self.exists = set()
        self.events = []
        self.failure = None
        self.failed_package = None
        self.accepted_only = False
        self.enterContext(mock.patch.dict(os.environ, {
            "GITHUB_REF": "refs/heads/main", "NODE_AUTH_TOKEN": SECRET,
        }, clear=True))
        self.network = self.enterContext(mock.patch.object(publish_npm, "urlopen", side_effect=self.probe))
        self.launcher = self.enterContext(mock.patch.object(publish_npm.shutil, "which", return_value="npm"))
        self.upload = self.enterContext(mock.patch.object(publish_types.subprocess, "run", side_effect=self.publish))

    def refresh_type_digests(self, checksum=None):
        digest = hashlib.sha256(self.tarball.read_bytes()).hexdigest()
        sidecar = self.tarball.with_name(self.tarball.name + ".sha256")
        sidecar.write_text(f"{checksum or digest}  {self.tarball.name}\n", encoding="ascii")
        data = json.loads(self.assets.read_text(encoding="utf-8"))
        data["types"] = {
            "file": self.tarball.name, "sha256": digest,
            "checksumSha256": hashlib.sha256(sidecar.read_bytes()).hexdigest(),
            "url": f"https://github.com/{REPOSITORY}/releases/download/v{VERSION}/{self.tarball.name}",
        }
        self.assets.write_text(json.dumps(data), encoding="utf-8")

    def probe(self, request, timeout):
        self.assertEqual(request.get_method(), "GET")
        self.assertEqual(timeout, 30)
        if request.full_url == f"https://registry.npmjs.org/%40kenvoy%2Ftypes/{VERSION}":
            name = "@kenvoy/types"
        else:
            self.assertEqual(request.full_url, f"https://registry.npmjs.org/kenvoy/{VERSION}")
            name = "kenvoy"
        headers = {key.lower(): value for key, value in request.header_items()}
        self.assertNotIn("authorization", headers)
        self.assertNotIn("cookie", headers)
        self.assertNotIn(SECRET, str(headers))
        self.events.append(("probe", name))
        if self.failure:
            raise self.failure
        if name not in self.exists:
            raise HTTPError(request.full_url, 404, SECRET, {}, None)
        return Response(json.dumps({"name": name, "version": VERSION}).encode())

    def publish(self, arguments, **options):
        self.assertEqual(arguments[0:2], ["npm", "publish"])
        self.assertIn("--ignore-scripts", arguments)
        self.assertIn("--provenance", arguments)
        self.assertEqual(arguments[-2:], ["--registry", "https://registry.npmjs.org"])
        self.assertNotIn("pack", arguments)
        self.assertFalse(options.get("shell", False))
        if str(self.tarball.resolve()) in arguments:
            name = "@kenvoy/types"
            self.assertEqual(Path(options["cwd"]), self.types)
        else:
            name = "kenvoy"
            self.assertEqual(options["cwd"], self.installer.resolve())
            self.assertIn("@kenvoy/types", self.exists)
        self.events.append(("publish", name))
        if name == self.failed_package:
            return subprocess.CompletedProcess(arguments, 17, SECRET, SECRET)
        if not self.accepted_only:
            self.exists.add(name)
        return subprocess.CompletedProcess(arguments, 0, SECRET, SECRET)

    def cli(self, publish=True, installer=None):
        stdout, stderr = io.StringIO(), io.StringIO()
        arguments = ["--assets", str(self.assets), "--types", str(self.types),
                     "--package", str(installer or self.installer)]
        if publish:
            arguments.append("--publish")
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = publish_types.main(arguments)
        self.assertNotIn(SECRET, stdout.getvalue() + stderr.getvalue())
        return code, stdout.getvalue(), stderr.getvalue()

    def assert_preflight_rejected(self):
        code, _, error = self.cli()
        self.assertEqual(code, 1)
        self.assertTrue(error)
        self.network.assert_not_called()
        self.upload.assert_not_called()
        self.launcher.assert_not_called()

    def test_default_on_dev_is_readonly_for_both_packages_without_npm_or_authentication(self):
        with mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/dev"}, clear=True):
            code, output, error = self.cli(publish=False)
        self.assertEqual((code, error), (0, ""))
        self.assertIn(f"@kenvoy/types@{VERSION} 计划发布（只读，未上传）", output)
        self.assertIn(f"kenvoy@{VERSION} 计划发布（只读，未上传）", output)
        self.assertEqual(self.events, [("probe", "@kenvoy/types"), ("probe", "kenvoy")])
        self.launcher.assert_not_called()
        self.upload.assert_not_called()

    def test_two_packages_publish_the_same_version_types_first_without_token_gate(self):
        with mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main"}, clear=True):
            code, _, error = self.cli()
        self.assertEqual((code, error), (0, ""))
        self.assertEqual(self.events, [("probe", "@kenvoy/types"), ("publish", "@kenvoy/types"),
                                       ("probe", "@kenvoy/types"), ("probe", "kenvoy"), ("publish", "kenvoy")])
        self.assertEqual(self.exists, {"@kenvoy/types", "kenvoy"})

    def test_each_existing_package_skips_independently_without_restoring_platform_packages(self):
        for existing in ({"@kenvoy/types"}, {"kenvoy"}, {"@kenvoy/types", "kenvoy"}):
            with self.subTest(existing=existing):
                self.exists = set(existing)
                self.events.clear()
                code, _, error = self.cli()
                self.assertEqual((code, error), (0, ""))
                self.assertEqual({name for action, name in self.events if action == "publish"},
                                 {"@kenvoy/types", "kenvoy"} - existing)

    def test_type_failure_exits_without_installing_and_never_retries_the_write(self):
        self.failed_package = "@kenvoy/types"
        code, _, error = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("退出码 17", error)
        self.assertIn("未重试", error)
        self.assertEqual(self.events, [("probe", "@kenvoy/types"), ("publish", "@kenvoy/types")])
        self.upload.assert_called_once()

    def test_accepted_but_not_public_type_version_stops_before_installer_publication(self):
        self.accepted_only = True
        code, output, error = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("上传后尚不可见", error)
        self.assertNotIn("已发布", output)
        self.assertEqual(self.events, [("probe", "@kenvoy/types"), ("publish", "@kenvoy/types"),
                                       ("probe", "@kenvoy/types")])
        self.upload.assert_called_once()

    def test_partial_success_resumes_only_the_missing_installer(self):
        self.failed_package = "kenvoy"
        self.assertEqual(self.cli()[0], 1)
        self.assertEqual(self.exists, {"@kenvoy/types"})
        self.failed_package = None
        self.events.clear()
        self.assertEqual(self.cli()[0], 0)
        self.assertEqual(self.events, [("probe", "@kenvoy/types"), ("probe", "kenvoy"), ("publish", "kenvoy")])

    def test_non_main_actual_publication_stops_before_registry_queries(self):
        for ref in ("refs/heads/dev", f"refs/tags/v{VERSION}", ""):
            with self.subTest(ref=ref), mock.patch.dict(os.environ, {"GITHUB_REF": ref}, clear=True):
                self.assert_preflight_rejected()

    def test_only_exact_404_allows_scoped_publication_other_failures_stop(self):
        for failure in ([HTTPError("https://registry.npmjs.org", status, SECRET, {}, None)
                         for status in (401, 403, 429, 500)] + [URLError(SECRET)]):
            with self.subTest(failure=type(failure).__name__, status=getattr(failure, "code", None)):
                self.failure = failure
                self.events.clear()
                code, _, error = self.cli()
                self.assertEqual(code, 1)
                self.assertIn("停止发布", error)
                self.assertEqual(self.events, [("probe", "@kenvoy/types")])
                self.upload.assert_not_called()

    def test_registry_identity_mismatch_is_not_a_missing_scoped_package(self):
        self.network.side_effect = lambda *args, **kwargs: Response(
            json.dumps({"name": "kenvoy", "version": VERSION}).encode())
        code, _, error = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("包名或版本不符", error)
        self.upload.assert_not_called()

    def test_type_tarball_and_sidecar_must_both_match_official_digest(self):
        for path in (self.tarball, self.tarball.with_name(self.tarball.name + ".sha256")):
            original = path.read_bytes()
            path.write_bytes(b"corrupt retained cache")
            try:
                self.assert_preflight_rejected()
                self.assertEqual(path.read_bytes(), b"corrupt retained cache")
            finally:
                path.write_bytes(original)

    def test_official_sidecar_cannot_claim_a_different_archive_checksum(self):
        self.refresh_type_digests(checksum="0" * 64)
        self.assert_preflight_rejected()

    def test_type_name_version_scripts_and_runtime_payloads_are_checked_before_either_publish(self):
        for kind in ("name", "version", "scripts", "runtime"):
            with self.subTest(kind=kind):
                files = type_files(VERSION)
                data = json.loads(files["package.json"])
                if kind == "runtime":
                    files["sdk/runtime.js"] = b"do not execute"
                else:
                    data[kind] = {"prepare": "do not execute"} if kind == "scripts" else "unrelated"
                    files["package.json"] = json.dumps(data).encode()
                self.tarball.write_bytes(archive(files, "package"))
                self.refresh_type_digests()
                self.assert_preflight_rejected()

    def test_installer_validation_is_not_relaxed_for_the_types_package(self):
        manifest = self.installer / "package.json"
        data = json.loads(manifest.read_text(encoding="utf-8"))
        data["scripts"] = {"postinstall": "node different.cjs"}
        manifest.write_text(json.dumps(data), encoding="utf-8")
        self.assert_preflight_rejected()

    def test_different_installer_and_cli_versions_fail_before_any_registry_read(self):
        other = self.root / "other-version"
        other.mkdir()
        assets, zips, _ = make_fixture(other, version="9.9.9")
        npm_packages.build(load_assets(assets), zips, other / "npm")
        code, _, error = self.cli(installer=other / "npm/kenvoy")
        self.assertEqual(code, 1)
        self.assertIn("同一版本", error)
        self.network.assert_not_called()
        self.upload.assert_not_called()

    def test_type_metadata_cannot_point_to_another_release_or_repository(self):
        data = json.loads(self.assets.read_text(encoding="utf-8"))
        data["types"]["url"] = data["types"]["url"].replace(REPOSITORY, "another/repository")
        self.assets.write_text(json.dumps(data), encoding="utf-8")
        self.assert_preflight_rejected()


if __name__ == "__main__":
    unittest.main()
