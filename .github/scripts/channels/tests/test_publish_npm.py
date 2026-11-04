"""npm 发布回归：只读默认、失败关闭、平台依赖顺序与部分完成后的幂等续跑。"""

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
from urllib.parse import urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import publish_npm
from manifests import load_assets
from npm_packages import build
from test_npm_packages import NAMES, VERSION, make_fixture

ORDER = [*NAMES[1:], NAMES[0]]
SECRET = "npm-secret-must-not-appear"


class Response(io.BytesIO):
    def __init__(self, payload, status=200):
        super().__init__(payload)
        self.status = status


class NpmPublisher(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        assets, zips, _ = make_fixture(self.root)
        self.packages = self.root / "npm packages"
        build(load_assets(assets), zips, self.packages)
        self.published = set()
        self.responses = {}
        self.events = []
        self.fail_package = None
        self.enterContext(mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main"}, clear=True))
        self.network = self.enterContext(mock.patch.object(publish_npm, "urlopen", side_effect=self.view))
        self.launcher = self.enterContext(mock.patch.object(publish_npm.shutil, "which", return_value="npm"))
        self.npm = self.enterContext(mock.patch.object(publish_npm.subprocess, "run", side_effect=self.upload))

    def view(self, request, timeout):
        url = urlsplit(request.full_url)
        self.assertEqual(request.get_method(), "GET")
        self.assertEqual((url.scheme, url.netloc), ("https", "registry.npmjs.org"))
        name, version = url.path.lstrip("/").split("/")
        self.assertEqual(version, VERSION)
        self.events.append(("probe", name))
        response = self.responses.get(name)
        if isinstance(response, Exception):
            raise response
        if response is not None:
            return Response(response)
        if name not in self.published:
            raise HTTPError(request.full_url, 404, SECRET, {}, None)
        return Response(json.dumps({"name": name, "version": VERSION}).encode())

    def upload(self, arguments, **options):
        self.assertIsInstance(arguments, list)
        self.assertEqual(arguments[1], "publish")
        self.assertFalse(options.get("shell", False))
        self.assertIn("--ignore-scripts", arguments)
        self.assertIn("--provenance", arguments)
        registry = arguments[arguments.index("--registry") + 1]
        self.assertEqual(registry, "https://registry.npmjs.org")
        directory = options["cwd"]
        self.assertEqual(directory.parent, self.packages.resolve())
        self.events.append(("publish", directory.name))
        failed = directory.name == self.fail_package
        if not failed:
            self.published.add(directory.name)
        return subprocess.CompletedProcess(arguments, int(failed), SECRET, SECRET)

    def cli(self, publish=True):
        stdout, stderr = io.StringIO(), io.StringIO()
        arguments = ["--packages", str(self.packages)]
        if publish:
            arguments.append("--publish")
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = publish_npm.main(arguments)
        self.assertNotIn(SECRET, stdout.getvalue() + stderr.getvalue())
        return code, stdout.getvalue(), stderr.getvalue()

    def assert_no_writes(self):
        self.npm.assert_not_called()
        self.assertFalse(any(event == "publish" for event, _ in self.events))

    def test_default_probes_real_boundary_without_npm_or_token(self):
        with mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/dev"}, clear=True):
            code, stdout, stderr = self.cli(publish=False)
        self.assertEqual(code, 0)
        self.assertEqual(stderr, "")
        self.assertEqual(self.events, [("probe", name) for name in ORDER])
        self.assertEqual(self.published, set())
        self.launcher.assert_not_called()
        self.assert_no_writes()

    def test_all_probes_precede_dependency_ordered_publish_without_token(self):
        code, stdout, stderr = self.cli()
        self.assertEqual(code, 0)
        self.assertEqual(stderr, "")
        self.assertEqual(
            self.events,
            [("probe", name) for name in ORDER] + [("publish", name) for name in ORDER],
        )
        self.assertEqual(self.published, set(ORDER))
        self.assertEqual(stdout.count("已发布"), 7)
        self.assertEqual(self.npm.call_count, 7)

    def test_partial_existing_versions_are_skipped_and_rerun_is_idempotent(self):
        existing = {ORDER[0], ORDER[3], "kenvoy"}
        self.published.update(existing)
        code, stdout, _ = self.cli()
        self.assertEqual(code, 0)
        self.assertEqual(
            [name for event, name in self.events if event == "publish"],
            [name for name in ORDER if name not in existing],
        )
        for name in existing:
            self.assertIn(f"{name}@{VERSION} 已存在，跳过", stdout)
        self.events.clear()
        self.npm.reset_mock()
        self.launcher.reset_mock()
        code, stdout, _ = self.cli()
        self.assertEqual(code, 0)
        self.assertEqual(stdout.count("已存在，跳过"), 7)
        self.launcher.assert_not_called()
        self.assert_no_writes()

    def test_non_main_publication_stops_before_registry_queries(self):
        for ref in ("refs/heads/dev", "refs/tags/v0.3.0", "refs/pull/7/merge", ""):
            with self.subTest(ref=ref), mock.patch.dict(os.environ, {"GITHUB_REF": ref}, clear=True):
                code, _, stderr = self.cli()
                self.assertEqual(code, 1)
                self.assertIn("refs/heads/main", stderr)
                self.network.assert_not_called()
                self.assert_no_writes()

    def test_registry_auth_rate_server_and_network_errors_block_entire_batch(self):
        failures = [HTTPError("https://registry.npmjs.org/kenvoy", code, SECRET, {}, None)
                    for code in (400, 401, 403, 429, 500, 503)]
        failures.extend((URLError(SECRET), ConnectionError(SECRET), IncompleteRead(SECRET.encode())))
        for failure in failures:
            with self.subTest(failure=type(failure).__name__, status=getattr(failure, "code", None)):
                self.responses["kenvoy"] = failure
                self.events.clear()
                code, stdout, stderr = self.cli()
                self.assertEqual(code, 1)
                self.assertIn("停止发布", stderr)
                self.assertEqual(stdout, "")
                self.assertEqual(self.events, [("probe", name) for name in ORDER])
                self.assert_no_writes()

    def test_corrupt_or_wrong_registry_metadata_blocks_entire_batch(self):
        responses = [
            SECRET.encode(), b"\xff", b"[]", b"{}", b"null",
            json.dumps({"name": "another-package", "version": VERSION}).encode(),
            json.dumps({"name": "kenvoy", "version": "0.0.1"}).encode(),
        ]
        for response in responses:
            with self.subTest(response=response):
                self.responses["kenvoy"] = response
                code, stdout, stderr = self.cli()
                self.assertEqual(code, 1)
                self.assertEqual(stdout, "")
                self.assertIn("停止发布", stderr)
                self.assert_no_writes()

    def test_unexpected_success_status_is_not_a_missing_version(self):
        self.network.side_effect = lambda *args, **kwargs: Response(b"{}", status=204)
        code, _, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("HTTP 200", stderr)
        self.assert_no_writes()

    def test_publish_failure_stops_without_retries_or_main_upload(self):
        self.fail_package = ORDER[1]
        code, stdout, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertEqual(self.npm.call_count, 2)
        self.assertEqual(self.published, {ORDER[0]})
        self.assertEqual(
            [name for event, name in self.events if event == "publish"], ORDER[:2]
        )
        self.assertIn(f"{ORDER[0]}@{VERSION} 已发布", stdout)
        self.assertNotIn(f"{ORDER[1]}@{VERSION} 已发布", stdout)
        self.assertIn(f"{ORDER[1]}@{VERSION}", stderr)
        self.assertIn("未重试", stderr)

    def test_process_start_failure_is_sanitized_and_not_retried(self):
        self.npm.side_effect = OSError(SECRET)
        code, _, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertEqual(self.npm.call_count, 1)
        self.assertEqual(self.published, set())
        self.assertIn("无法启动 npm", stderr)

    def test_missing_npm_fails_before_upload(self):
        self.launcher.return_value = None
        code, _, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("缺少 npm", stderr)
        self.assert_no_writes()

    def test_invalid_package_metadata_never_reaches_registry(self):
        cases = [
            (ORDER[0], "name", "unrelated-package"),
            ("kenvoy", "version", "0.9.0"),
            (ORDER[0], "version", "0.3.0-beta.1"),
            (ORDER[0], "os", ["linux"]),
            (ORDER[0], "cpu", ["arm64"]),
            ("kenvoy", "optionalDependencies", {}),
            ("kenvoy", "optionalDependencies", {name: "0.0.1" for name in ORDER[:-1]}),
            ("kenvoy", "bin", {"kenvoy": "../secret"}),
            (ORDER[0], "files", []),
            (ORDER[0], "publishConfig", {"registry": "https://wrong.invalid"}),
        ]
        for name, field, value in cases:
            with self.subTest(name=name, field=field):
                manifest = self.packages / name / "package.json"
                original = manifest.read_text(encoding="utf-8")
                data = json.loads(original)
                data[field] = value
                manifest.write_text(json.dumps(data), encoding="utf-8")
                try:
                    code, _, stderr = self.cli()
                    self.assertEqual(code, 1)
                    self.assertTrue(stderr)
                    self.network.assert_not_called()
                    self.assert_no_writes()
                finally:
                    manifest.write_text(original, encoding="utf-8")

    def test_wrong_package_directory_set_never_reaches_registry(self):
        (self.packages / ORDER[0]).rename(self.packages / "unrelated-package")
        code, _, _ = self.cli()
        self.assertEqual(code, 1)
        self.network.assert_not_called()
        self.assert_no_writes()

    def test_missing_manifest_never_reaches_registry(self):
        (self.packages / "kenvoy" / "package.json").unlink()
        code, _, _ = self.cli()
        self.assertEqual(code, 1)
        self.network.assert_not_called()
        self.assert_no_writes()

    def test_corrupt_manifest_does_not_echo_content(self):
        (self.packages / "kenvoy" / "package.json").write_text(SECRET, encoding="utf-8")
        code, _, stderr = self.cli()
        self.assertEqual(code, 1)
        self.assertIn("UTF-8 JSON", stderr)
        self.network.assert_not_called()
        self.assert_no_writes()

    def test_empty_binary_never_reaches_registry(self):
        (self.packages / ORDER[0] / "bin" / "kenvoy.exe").write_bytes(b"")
        code, _, _ = self.cli()
        self.assertEqual(code, 1)
        self.network.assert_not_called()
        self.assert_no_writes()


if __name__ == "__main__":
    unittest.main()
