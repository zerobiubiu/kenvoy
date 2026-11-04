"""单包发布回归：只读默认、下载契约、认证隔离、失败关闭与幂等跳过。"""

import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from http.client import IncompleteRead
from pathlib import Path
from unittest import mock
from urllib.error import HTTPError, URLError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import npm_packages
import publish_npm
from manifests import load_assets
from npm_packages import build
from test_npm_packages import VERSION, make_fixture, make_runtime

SECRET = "npm-secret-must-not-appear"


class Response(io.BytesIO):
    def __init__(self, payload, status=200):
        super().__init__(payload)
        self.status = status


class NpmPublisher(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        assets, zips, _ = make_fixture(self.root)
        self.runtime = make_runtime(self.root)
        self.enterContext(mock.patch.object(npm_packages, "RUNTIME_ROOT", self.runtime))
        output = self.root / "npm packages"
        build(load_assets(assets), zips, output)
        self.package = output / "kenvoy"
        self.exists = False
        self.response = None
        self.upload_status = 0
        self.events = []
        self.enterContext(mock.patch.dict(os.environ, {
            "GITHUB_REF": "refs/heads/main", "NODE_AUTH_TOKEN": SECRET,
        }, clear=True))
        self.network = self.enterContext(mock.patch.object(publish_npm, "urlopen", side_effect=self.view))
        self.launcher = self.enterContext(mock.patch.object(publish_npm.shutil, "which", return_value="npm"))
        self.npm = self.enterContext(mock.patch.object(publish_npm.subprocess, "run", side_effect=self.upload))

    def view(self, request, timeout):
        self.assertEqual(request.get_method(), "GET")
        self.assertEqual(request.full_url, f"https://registry.npmjs.org/kenvoy/{VERSION}")
        self.assertEqual(timeout, 30)
        headers = {key.lower(): value for key, value in request.header_items()}
        self.assertNotIn("authorization", headers)
        self.assertNotIn("cookie", headers)
        self.assertNotIn(SECRET, str(headers))
        self.events.append("probe")
        if isinstance(self.response, Exception):
            raise self.response
        if self.response is not None:
            return Response(self.response)
        if not self.exists:
            raise HTTPError(request.full_url, 404, SECRET, {}, None)
        return Response(json.dumps({"name": "kenvoy", "version": VERSION}).encode())

    def upload(self, arguments, **options):
        self.assertEqual(arguments, [
            "npm", "publish", "--access", "public", "--provenance", "--ignore-scripts",
            "--registry", "https://registry.npmjs.org",
        ])
        self.assertEqual(options["cwd"], self.package.resolve())
        self.assertFalse(options.get("shell", False))
        self.assertTrue(options["capture_output"])
        self.events.append("publish")
        if self.upload_status == 0:
            self.exists = True
        return subprocess.CompletedProcess(arguments, self.upload_status, SECRET, SECRET)

    def cli(self, publish=True, directory=None):
        stdout, stderr = io.StringIO(), io.StringIO()
        arguments = ["--package", str(directory or self.package)]
        if publish:
            arguments.append("--publish")
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = publish_npm.main(arguments)
        self.assertNotIn(SECRET, stdout.getvalue() + stderr.getvalue())
        return code, stdout.getvalue(), stderr.getvalue()

    def assert_no_writes(self):
        self.npm.assert_not_called()
        self.assertNotIn("publish", self.events)

    def assert_preflight_rejection(self):
        code, stdout, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertEqual(stdout, "")
        self.assertTrue(stderr)
        self.network.assert_not_called()
        self.launcher.assert_not_called()
        self.assert_no_writes()

    @contextlib.contextmanager
    def changed_json(self, name, change):
        path = self.package / name
        original = path.read_text(encoding="utf-8")
        data = json.loads(original)
        change(data)
        path.write_text(json.dumps(data), encoding="utf-8")
        try:
            yield
        finally:
            path.write_text(original, encoding="utf-8")

    def test_default_is_readonly_on_dev_without_npm_or_authentication(self):
        with mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/dev"}, clear=True):
            code, stdout, stderr = self.cli(publish=False)
        self.assertEqual((code, stderr), (0, ""))
        self.assertIn("计划发布（只读，未上传）", stdout)
        self.assertEqual(self.events, ["probe"])
        self.assertFalse(self.exists)
        self.launcher.assert_not_called()
        self.assert_no_writes()

    def test_missing_version_publishes_only_validated_directory_without_token_gate(self):
        with mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main"}, clear=True):
            code, stdout, stderr = self.cli()
        self.assertEqual((code, stderr), (0, ""))
        self.assertEqual(self.events, ["probe", "publish"])
        self.assertTrue(self.exists)
        self.assertIn(f"kenvoy@{VERSION} 已发布", stdout)

    def test_existing_version_is_skipped_without_starting_npm(self):
        self.exists = True
        for publish in (False, True):
            with self.subTest(publish=publish):
                self.events.clear()
                code, stdout, stderr = self.cli(publish=publish)
                self.assertEqual((code, stderr), (0, ""))
                self.assertIn(f"kenvoy@{VERSION} 已存在，跳过", stdout)
                self.assertEqual(self.events, ["probe"])
                self.launcher.assert_not_called()
                self.assert_no_writes()

    def test_successful_publish_is_idempotent_on_explicit_rerun(self):
        self.assertEqual(self.cli()[0], 0)
        self.events.clear()
        self.npm.reset_mock()
        self.launcher.reset_mock()
        code, stdout, _ = self.cli()
        self.assertEqual(code, 0)
        self.assertIn("已存在，跳过", stdout)
        self.assertEqual(self.events, ["probe"])
        self.launcher.assert_not_called()
        self.assert_no_writes()

    def test_non_main_publication_stops_before_registry_queries(self):
        for ref in ("refs/heads/dev", f"refs/tags/v{VERSION}", "refs/pull/7/merge", ""):
            with self.subTest(ref=ref), mock.patch.dict(os.environ, {"GITHUB_REF": ref}, clear=True):
                code, _, stderr = self.cli()
                self.assertEqual(code, 1)
                self.assertIn("refs/heads/main", stderr)
                self.network.assert_not_called()
                self.assert_no_writes()

    def test_only_exact_404_allows_publication(self):
        failures = [HTTPError("https://registry.npmjs.org/kenvoy", code, SECRET, {}, None)
                    for code in (400, 401, 403, 410, 429, 500, 503)]
        failures.extend((URLError(SECRET), ConnectionError(SECRET), IncompleteRead(SECRET.encode())))
        for failure in failures:
            with self.subTest(failure=type(failure).__name__, status=getattr(failure, "code", None)):
                self.response = failure
                self.events.clear()
                code, stdout, stderr = self.cli()
                self.assertEqual((code, stdout), (1, ""))
                self.assertIn("停止发布", stderr)
                self.assertEqual(self.events, ["probe"])
                self.assert_no_writes()

    def test_invalid_registry_json_or_identity_is_not_a_missing_version(self):
        responses = [SECRET.encode(), b"\xff", b"[]", b"{}", b"null",
                     json.dumps({"name": "another-package", "version": VERSION}).encode(),
                     json.dumps({"name": "kenvoy", "version": "0.0.1"}).encode()]
        for response in responses:
            with self.subTest(response=response):
                self.response = response
                code, stdout, stderr = self.cli()
                self.assertEqual((code, stdout), (1, ""))
                self.assertIn("停止发布", stderr)
                self.assert_no_writes()

    def test_unexpected_success_status_is_not_a_missing_version(self):
        self.network.side_effect = lambda *args, **kwargs: Response(b"{}", status=204)
        code, _, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("HTTP 200", stderr)
        self.assert_no_writes()

    def test_publish_failure_is_sanitized_and_never_retried(self):
        self.upload_status = 17
        code, stdout, stderr = self.cli()
        self.assertEqual((code, stdout), (1, ""))
        self.assertFalse(self.exists)
        self.assertEqual(self.events, ["probe", "publish"])
        self.assertIn("退出码 17", stderr)
        self.assertIn("未重试", stderr)
        self.npm.assert_called_once()

    def test_process_start_failure_is_sanitized_and_never_retried(self):
        self.npm.side_effect = OSError(SECRET)
        code, stdout, stderr = self.cli()
        self.assertEqual((code, stdout), (1, ""))
        self.assertFalse(self.exists)
        self.assertIn("无法启动 npm", stderr)
        self.npm.assert_called_once()

    def test_missing_npm_stops_before_upload(self):
        self.launcher.return_value = None
        code, _, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("缺少 npm", stderr)
        self.assert_no_writes()

    def test_unrelated_or_unsupported_package_metadata_never_reaches_registry(self):
        cases = [("name", SECRET), ("version", "0.3.0-beta.1"), ("version", "0.9.0"),
                 ("bin", {"kenvoy": "../secret"}), ("scripts", {"postinstall": "node other.cjs"}),
                 ("scripts", {"postinstall": "node install.cjs", "prepare": "node extra.cjs"}),
                 ("files", ["bin", "vendor"]), ("engines", {"node": ">=14"}),
                 ("dependencies", {}), ("optionalDependencies", {}),
                 ("dependencies", {"kenvoy-linux-x64": VERSION}),
                 ("devDependencies", {"unrelated": "1.0.0"}), ("private", True),
                 ("license", "UNLICENSED"), ("os", ["win32"]), ("cpu", ["x64"]),
                 ("repository", {"url": f"https://{SECRET}.invalid"}),
                 ("publishConfig", {"registry": "https://wrong.invalid"})]
        for field, value in cases:
            with self.subTest(field=field, value=value):
                with self.changed_json("package.json", lambda data: data.update({field: value})):
                    self.assert_preflight_rejection()

    def test_runtime_dependency_versions_cannot_drift_from_source(self):
        with self.changed_json("package.json", lambda data: data["dependencies"].update({"fflate": "9.9.9"})):
            self.assert_preflight_rejection()

    def test_download_header_and_supported_target_set_are_required(self):
        cases = [lambda data: data.update(formatVersion=2), lambda data: data.update(formatVersion=True),
                 lambda data: data.update(version="0.0.1"), lambda data: data.update(repository="../bad"),
                 lambda data: data["targets"].pop("darwin-arm64"),
                 lambda data: data["targets"].update({"freebsd-x64": data["targets"]["linux-x64"]}),
                 lambda data: data.update(targets=[]), lambda data: data.update(extra=SECRET)]
        for change in cases:
            with self.subTest(change=change), self.changed_json("download-manifest.json", change):
                self.assert_preflight_rejection()

    def test_download_target_rejects_unsafe_or_unverifiable_payload(self):
        cases = [("archiveSha256", "A" * 64), ("binarySha256", "bad"),
                 ("archiveSha256", []), ("archiveSize", 0), ("archiveSize", True),
                 ("binarySize", -1), ("binarySize", 1.5), ("binarySize", 2**53),
                 ("member", "../kenvoy"), ("member", "/kenvoy"), ("member", "a//kenvoy"),
                 ("member", "C:/kenvoy"), ("member", "a\\kenvoy"), ("member", None),
                 ("member", "kenvoy.exe"), ("executable", "kenvoy.exe"),
                 ("url", f"https://user:{SECRET}@github.com/fixture/kenvoy/releases/download/v{VERSION}/a.zip"),
                 ("url", f"http://github.com/fixture/kenvoy/releases/download/v{VERSION}/a.zip"),
                 ("url", "https://evil.invalid/kenvoy.zip"), ("url", None)]
        data = json.loads((self.package / "download-manifest.json").read_text(encoding="utf-8"))
        url = data["targets"]["linux-x64"]["url"]
        cases.extend(("url", candidate) for candidate in (
            url + "?token=" + SECRET, url + "#fragment", url.replace(f"v{VERSION}/", "v0.0.1/"),
            url.replace("fixture/kenvoy/", "unrelated/kenvoy/"),
            url.replace(f"kenvoy-{VERSION}-", "another-"),
            data["targets"]["linux-arm64"]["url"],
        ))
        for field, value in cases:
            with self.subTest(field=field, value=value):
                change = lambda data: data["targets"]["linux-x64"].update({field: value})
                with self.changed_json("download-manifest.json", change):
                    self.assert_preflight_rejection()
        with self.changed_json("download-manifest.json", lambda data: data["targets"]["linux-x64"].pop("binarySize")):
            self.assert_preflight_rejection()

    def test_missing_or_empty_runtime_legal_and_manifest_files_block_publication(self):
        for name in ("install.cjs", "bin/kenvoy.cjs", "lib/config.cjs", "LICENSE", "README.md",
                     "download-manifest.json", "package.json"):
            path = self.package / name
            original = path.read_bytes()
            for missing in (False, True):
                with self.subTest(name=name, missing=missing):
                    if missing:
                        path.unlink()
                    else:
                        path.write_bytes(b"")
                    try:
                        self.assert_preflight_rejection()
                    finally:
                        path.write_bytes(original)

    def test_extra_binary_modules_and_npm_configuration_are_not_publishable(self):
        for name in ("bin/kenvoy", "lib/unexpected.cjs", ".npmrc", "vendor/kenvoy"):
            path = self.package / name
            path.parent.mkdir(exist_ok=True)
            path.write_text(SECRET, encoding="utf-8")
            try:
                self.assert_preflight_rejection()
            finally:
                path.unlink()
                if name.startswith("vendor/"):
                    path.parent.rmdir()

    def test_corrupt_json_never_echoes_input(self):
        for name in ("package.json", "download-manifest.json"):
            path = self.package / name
            original = path.read_bytes()
            for malformed in (SECRET.encode(), b"\xff", b"[]", b"null"):
                with self.subTest(name=name, content=malformed):
                    path.write_bytes(malformed)
                    try:
                        self.assert_preflight_rejection()
                    finally:
                        path.write_bytes(original)

    def test_package_parent_is_not_accepted_as_the_publish_directory(self):
        code, _, _ = self.cli(directory=self.package.parent)
        self.assertEqual(code, 1)
        self.network.assert_not_called()
        self.assert_no_writes()

    def test_symlinked_runtime_file_cannot_be_uploaded(self):
        path = self.package / "install.cjs"
        path.unlink()
        try:
            path.symlink_to(self.runtime / "install.cjs")
        except OSError:
            self.skipTest("当前系统未授权创建符号链接")
        self.assert_preflight_rejection()

    def test_plural_cli_argument_has_no_compatibility_alias(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as stopped:
            publish_npm.main(["--packages", str(self.package.parent)])
        self.assertNotEqual(stopped.exception.code, 0)
        self.network.assert_not_called()
        self.assert_no_writes()


if __name__ == "__main__":
    unittest.main()
