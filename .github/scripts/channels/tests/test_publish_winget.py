"""WinGet 发布边界：只读计划、权限失败、真实本地 Git 恢复与 PR 幂等。"""

import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock
from urllib.parse import parse_qs, urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import publish_winget
from manifests import Asset, Assets, winget_files

VERSION = "0.4.0"
OWNER = "fixture-owner"
FORK = f"{OWNER}/winget-pkgs"
BRANCH = f"kenvoy-{VERSION}"
RELATIVE = f"manifests/k/Kenvoy/Kenvoy/{VERSION}"
UPSTREAM = "microsoft/winget-pkgs"
PR_URL = f"https://github.com/{UPSTREAM}/pull/123"
REAL_RUN = subprocess.run


def manifests(root):
    assets = []
    for arch in ("x86_64", "ARM64"):
        name = f"kenvoy-{VERSION}-windows-{arch}.zip"
        assets.append(Asset("windows", arch, "unused-target", name, "a" * 64,
                            f"https://github.com/fixture/kenvoy/releases/download/v{VERSION}/{name}",
                            "kenvoy.exe"))
    result = root / "input"
    result.mkdir()
    for relative, text in winget_files(Assets(VERSION, f"v{VERSION}", "fixture/kenvoy", tuple(assets))).items():
        (result / Path(relative).name).write_bytes(text.encode("utf-8"))
    return result


class GitHubFixture:
    """只替换 GitHub HTTP 边界；所有 clone/fetch/commit/push 都落到独占本地仓库。"""

    def __init__(self, root):
        self.root = root
        self.upstream = root / "upstream.git"
        self.fork = root / "fork.git"
        self.fork_exists = False
        self.collected = None
        self.pr = None
        self.api_calls = []
        self.git_calls = []
        self.responses = {}
        self.fail_git = None
        self.fail_after_pr = False
        self.fork_metadata = {"full_name": FORK, "fork": True,
                              "parent": {"full_name": UPSTREAM}, "permissions": {"push": True}}

    def git(self, *arguments, cwd=None, required=True):
        result = REAL_RUN(["git", "-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
                           "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false",
                           *map(str, arguments)], cwd=cwd, capture_output=True, text=True, encoding="utf-8")
        if required and result.returncode:
            raise AssertionError(result.stderr)
        return result

    def initialize(self):
        source = self.root / "seed"
        self.git("init", "--initial-branch=master", source)
        (source / "README").write_text("上游初始提交\n", encoding="utf-8")
        self.git("add", "README", cwd=source)
        self.git("commit", "-m", "fixture", cwd=source)
        self.git("clone", "--bare", source, self.upstream)

    def create_fork(self):
        self.git("clone", "--bare", self.upstream, self.fork)
        self.fork_exists = True

    def seed_branch(self, files):
        work = self.root / "previous-run"
        self.git("clone", self.fork, work)
        self.git("checkout", "-b", BRANCH, cwd=work)
        for relative, payload in files.items():
            target = work / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(payload)
        self.git("add", ".", cwd=work)
        self.git("commit", "-m", "上次运行", cwd=work)
        self.git("push", "origin", f"HEAD:refs/heads/{BRANCH}", cwd=work)
        return self.branch_sha()

    def branch_sha(self):
        if not self.fork.exists():
            return None
        result = self.git("--git-dir", self.fork, "rev-parse", "--verify", f"refs/heads/{BRANCH}", required=False)
        return result.stdout.strip() if result.returncode == 0 else None

    def pull(self):
        return {"state": "open", "html_url": PR_URL,
                "head": {"ref": BRANCH, "repo": {"full_name": FORK}},
                "base": {"ref": "master", "repo": {"full_name": UPSTREAM}}}

    def posts(self, suffix):
        return [call for call in self.api_calls if call[0] == "POST" and call[1].endswith(suffix)]

    def response(self, command, status, value):
        if status is None:
            return subprocess.CompletedProcess(command, 1, "", str(value))
        stdout = f"HTTP/2.0 {status} status\nContent-Type: application/json\n\n{json.dumps(value)}"
        error = "" if 200 <= status < 300 else f"gh: {value} (HTTP {status})"
        return subprocess.CompletedProcess(command, 0 if not error else 1, stdout, error)

    def __call__(self, command, **kwargs):
        if command[0] == "git":
            self.git_calls.append(command)
            if self.fail_git is not None and self.fail_git in command:
                return subprocess.CompletedProcess(command, 1, "", "remote: Permission denied")
            replacements = {f"https://github.com/{UPSTREAM}.git": str(self.upstream),
                            f"https://github.com/{FORK}.git": str(self.fork)}
            for item in command:
                if item.startswith("https://") and item not in replacements:
                    raise AssertionError(f"测试禁止访问远端：{item}")
            return REAL_RUN([replacements.get(item, item) for item in command], **kwargs)
        if command[:2] != ["gh", "api"] or command[command.index("--hostname") + 1] != "github.com":
            raise AssertionError(f"意外命令：{command}")
        method = command[command.index("--method") + 1]
        endpoint = command[command.index("--header") + 2]
        fields = dict(command[index + 1].split("=", 1) for index, part in enumerate(command) if part == "--raw-field")
        path, query = urlsplit(endpoint).path, parse_qs(urlsplit(endpoint).query)
        self.api_calls.append((method, path, fields))
        override = self.responses.get((method, path))
        if override is not None:
            return self.response(command, *override)
        if method == "GET":
            if path == f"repos/{UPSTREAM}":
                value = {"full_name": UPSTREAM, "default_branch": "master"}
            elif path == f"repos/{UPSTREAM}/contents/{RELATIVE}":
                assert query == {"ref": ["master"]}
                return self.response(command, 404 if self.collected is None else 200,
                                     {"message": "Not Found"} if self.collected is None else self.collected)
            elif path == f"repos/{FORK}":
                return self.response(command, 200 if self.fork_exists else 404,
                                     self.fork_metadata if self.fork_exists else {"message": "Not Found"})
            elif path == f"repos/{UPSTREAM}/pulls":
                assert query == {"state": ["open"], "head": [f"{OWNER}:{BRANCH}"],
                                 "base": ["master"], "per_page": ["100"]}
                value = [self.pr] if self.pr else []
            elif path == f"repos/{FORK}/git/ref/heads/{BRANCH}":
                sha = self.branch_sha()
                return self.response(command, 200 if sha else 404,
                                     {"ref": f"refs/heads/{BRANCH}", "object": {"type": "commit", "sha": sha}})
            elif path in (f"users/{OWNER}", "user"):
                value = {"login": OWNER, "type": "User"}
            else:
                raise AssertionError(f"意外读取：{endpoint}")
        elif method == "POST" and path == f"repos/{UPSTREAM}/forks":
            self.create_fork()
            value = self.fork_metadata
        elif method == "POST" and path == f"repos/{UPSTREAM}/pulls":
            assert fields["head"] == f"{OWNER}:{BRANCH}" and fields["base"] == "master"
            if self.pr or not self.branch_sha():
                return self.response(command, 422, {"message": "PR 已存在或 head 不存在"})
            self.pr = self.pull()
            if self.fail_after_pr:
                return self.response(command, None, "PR 已创建，但连接中断")
            value = self.pr
        else:
            raise AssertionError(f"意外写入：{method} {endpoint}")
        return self.response(command, 201 if method == "POST" else 200, value)


class PublisherTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="kenvoy-winget-test-")))
        self.manifests = manifests(self.root)
        self.remote = GitHubFixture(self.root)
        self.enterContext(mock.patch.object(publish_winget.subprocess, "run", self.remote))

    def invoke(self, *, publish=True, ref="refs/heads/main", version=VERSION, owner=OWNER):
        arguments = ["--manifests", str(self.manifests), "--version", version, "--owner", owner]
        if publish:
            arguments.append("--publish")
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.dict(os.environ, {"GITHUB_REF": ref}), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = publish_winget.main(arguments)
        return code, stdout.getvalue(), stderr.getvalue()

    def assert_failed_without_writes(self, message):
        code, output, error = self.invoke()
        self.assertEqual(code, 1)
        self.assertEqual(output, "")
        self.assertIn(message, error)
        self.assertFalse(self.remote.git_calls)
        self.assertFalse(self.remote.posts("forks"))
        self.assertFalse(self.remote.posts("pulls"))

    def test_default_is_read_only_even_on_dev(self):
        code, output, error = self.invoke(publish=False, ref="refs/heads/dev")
        self.assertEqual((code, error), (0, ""))
        self.assertIn("只读计划", output)
        self.assertIn("创建 fork", output)
        self.assertFalse(self.remote.git_calls)
        self.assertTrue(all(method == "GET" for method, _, _ in self.remote.api_calls))

    def test_non_main_fails_before_any_command(self):
        for ref in ("", "refs/heads/dev", "refs/tags/v0.4.0", "refs/pull/123/merge"):
            with self.subTest(ref=ref):
                code, _, error = self.invoke(ref=ref)
                self.assertEqual(code, 1)
                self.assertIn("仅允许", error)
        self.assertFalse(self.remote.api_calls)
        self.assertFalse(self.remote.git_calls)

    def test_invalid_owner_and_version_cannot_reach_commands(self):
        for owner in ("../other", "has space", "owner/other", "bad--name", "trailing-", "x" * 40):
            with self.subTest(owner=owner):
                self.assertEqual(self.invoke(owner=owner)[0], 1)
        for version in ("v0.4.0", "0.4", "01.4.0", "0.4.0-rc.1", "../../outside"):
            with self.subTest(version=version):
                self.assertEqual(self.invoke(version=version)[0], 1)
        self.assertFalse(self.remote.api_calls)

    def test_manifest_mismatch_is_rejected_before_network(self):
        path = self.manifests / "Kenvoy.Kenvoy.installer.yaml"
        path.write_bytes(path.read_bytes().replace(b"PackageVersion: 0.4.0", b"PackageVersion: 9.9.9"))
        self.assert_failed_without_writes("PackageVersion")
        self.assertFalse(self.remote.api_calls)

    def test_collected_version_skips_fork_and_pr(self):
        self.remote.collected = [{"name": name, "path": f"{RELATIVE}/{name}", "type": "file"}
                                 for name in publish_winget.FILES]
        code, output, error = self.invoke()
        self.assertEqual((code, error), (0, ""))
        self.assertIn("上游已收录", output)
        self.assertEqual(len(self.remote.api_calls), 2)
        self.assertFalse(self.remote.git_calls)

    def test_incomplete_upstream_directory_is_not_success(self):
        self.remote.collected = []
        self.assert_failed_without_writes("缺少完整三清单")

    def test_open_pr_is_returned_without_writes(self):
        self.remote.fork_exists = True
        self.remote.pr = self.remote.pull()
        code, output, error = self.invoke()
        self.assertEqual((code, error), (0, ""))
        self.assertIn(PR_URL, output)
        self.assertIn("不代表可安装", output)
        self.assertFalse(self.remote.git_calls)
        self.assertFalse(self.remote.posts("pulls"))

    def test_api_errors_never_become_missing_resources(self):
        paths = [f"repos/{UPSTREAM}", f"repos/{UPSTREAM}/contents/{RELATIVE}", f"repos/{FORK}",
                 f"repos/{UPSTREAM}/pulls", f"repos/{FORK}/git/ref/heads/{BRANCH}"]
        self.remote.fork_exists = True
        for path in paths:
            for status in (401, 403, 429, 500, 503):
                with self.subTest(path=path, status=status):
                    self.remote.responses = {("GET", path): (status, {"message": "Not Found"})}
                    self.assert_failed_without_writes(f"HTTP {status}")
        self.remote.responses = {("GET", f"repos/{FORK}"): (None, "404 Not Found: connection reset")}
        self.assert_failed_without_writes("未收到明确 HTTP 状态")

    def test_successful_null_response_is_not_treated_as_a_404(self):
        self.remote.fork_exists = True
        for path in (f"repos/{UPSTREAM}/contents/{RELATIVE}", f"repos/{FORK}",
                     f"repos/{FORK}/git/ref/heads/{BRANCH}"):
            with self.subTest(path=path):
                self.remote.responses = {("GET", path): (200, None)}
                self.assert_failed_without_writes("不能将损坏响应当作 HTTP 404")

    def test_malformed_fork_relationship_is_rejected(self):
        self.remote.fork_exists = True
        for parent in (None, "microsoft/winget-pkgs", {"full_name": None}):
            with self.subTest(parent=parent):
                self.remote.fork_metadata["parent"] = parent
                self.assert_failed_without_writes("不是 microsoft/winget-pkgs")

    def test_wrong_fork_and_permission_are_rejected(self):
        self.remote.fork_exists = True
        self.remote.fork_metadata["parent"] = {"full_name": "other/winget-pkgs"}
        self.assert_failed_without_writes("不是 microsoft/winget-pkgs")
        self.remote.fork_metadata["parent"] = {"full_name": UPSTREAM}
        self.remote.fork_metadata["permissions"]["push"] = False
        self.assert_failed_without_writes("push 权限")

    def test_wrong_owner_cannot_create_someone_elses_fork(self):
        self.remote.responses[("GET", "user")] = (200, {"login": "another-user"})
        self.assert_failed_without_writes("与认证用户不符")

    def test_invalid_pr_response_cannot_claim_success_or_create_duplicate(self):
        self.remote.fork_exists = True
        self.remote.pr = self.remote.pull()
        self.remote.pr["head"]["repo"]["full_name"] = "other/winget-pkgs"
        self.assert_failed_without_writes("PR 响应与指定")

    def test_clone_failure_creates_no_fork(self):
        self.remote.initialize()
        self.remote.fail_git = "clone"
        code, output, error = self.invoke()
        self.assertEqual((code, output), (1, ""))
        self.assertIn("git clone失败", error)
        self.assertFalse(self.remote.posts("forks"))
        self.assertFalse(self.remote.posts("pulls"))

    def test_fork_creation_failure_does_not_push_or_create_pr(self):
        self.remote.initialize()
        self.remote.responses[("POST", f"repos/{UPSTREAM}/forks")] = (403, {"message": "Resource not accessible"})
        code, output, error = self.invoke()
        self.assertEqual((code, output), (1, ""))
        self.assertIn("HTTP 403", error)
        self.assertFalse(self.remote.fork_exists)
        self.assertFalse(any("push" in call for call in self.remote.git_calls))
        self.assertFalse(self.remote.posts("pulls"))

    def test_push_failure_recovers_without_creating_another_fork(self):
        self.remote.initialize()
        self.remote.fail_git = "push"
        code, output, error = self.invoke()
        self.assertEqual((code, output), (1, ""))
        self.assertIn("Permission denied", error)
        self.assertTrue(self.remote.fork_exists)
        self.assertIsNone(self.remote.branch_sha())
        self.assertFalse(self.remote.posts("pulls"))
        self.remote.fail_git = None
        self.assertEqual(self.invoke()[0], 0)
        self.assertEqual(len(self.remote.posts("forks")), 1)
        self.assertEqual(len(self.remote.posts("pulls")), 1)
        self.assertIsNotNone(self.remote.branch_sha())

    def test_pr_failure_resumes_pushed_branch_and_duplicate_runs_add_nothing(self):
        self.remote.initialize()
        key = ("POST", f"repos/{UPSTREAM}/pulls")
        self.remote.responses[key] = (403, {"message": "Resource not accessible"})
        code, output, error = self.invoke()
        self.assertEqual((code, output), (1, ""))
        self.assertIn("HTTP 403", error)
        sha = self.remote.branch_sha()
        self.assertIsNotNone(sha)
        del self.remote.responses[key]
        self.assertEqual(self.invoke()[0], 0)
        self.assertEqual(self.remote.branch_sha(), sha)
        git_count = len(self.remote.git_calls)
        code, output, _ = self.invoke()
        self.assertEqual(code, 0)
        self.assertIn(PR_URL, output)
        self.assertEqual(len(self.remote.git_calls), git_count)
        self.assertEqual(len(self.remote.posts("forks")), 1)
        self.assertEqual(len(self.remote.posts("pulls")), 2)
        self.assertEqual(sum("push" in call for call in self.remote.git_calls), 1)

    def test_pr_committed_before_connection_failure_is_found_on_rerun(self):
        self.remote.initialize()
        self.remote.fail_after_pr = True
        code, output, error = self.invoke()
        self.assertEqual((code, output), (1, ""))
        self.assertIn("连接中断", error)
        self.assertIsNotNone(self.remote.pr)
        code, output, _ = self.invoke()
        self.assertEqual(code, 0)
        self.assertIn(PR_URL, output)
        self.assertEqual(len(self.remote.posts("pulls")), 1)

    def test_partial_branch_is_completed_by_fast_forward(self):
        self.remote.initialize()
        self.remote.create_fork()
        name = "Kenvoy.Kenvoy.yaml"
        previous = self.remote.seed_branch({f"{RELATIVE}/{name}": (self.manifests / name).read_bytes()})
        code, output, error = self.invoke()
        self.assertEqual((code, error), (0, ""))
        self.assertIn(PR_URL, output)
        parent = self.remote.git("--git-dir", self.remote.fork, "rev-parse", f"{BRANCH}^").stdout.strip()
        self.assertEqual(parent, previous)
        for name in publish_winget.FILES:
            payload = self.remote.git("--git-dir", self.remote.fork, "show", f"{BRANCH}:{RELATIVE}/{name}").stdout
            self.assertEqual(payload, (self.manifests / name).read_text(encoding="utf-8"))
        self.assertFalse(self.remote.posts("forks"))
        self.assertEqual(len(self.remote.posts("pulls")), 1)

    def test_unrelated_existing_branch_history_is_preserved_and_not_submitted(self):
        self.remote.initialize()
        self.remote.create_fork()
        previous = self.remote.seed_branch({"unrelated.txt": b"unrelated\n"})
        code, output, error = self.invoke()
        self.assertEqual((code, output), (1, ""))
        self.assertIn("三份清单之外", error)
        self.assertEqual(self.remote.branch_sha(), previous)
        self.assertFalse(self.remote.posts("pulls"))
        self.assertFalse(any("push" in call for call in self.remote.git_calls))


if __name__ == "__main__":
    unittest.main()
