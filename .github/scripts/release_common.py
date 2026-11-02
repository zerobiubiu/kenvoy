"""发布脚本的只读契约：分支、精确版本、远端标签与资产命名。"""

import json
import os
import re
import subprocess
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT_COMMIT = "7d3b5c2860d6e3c64948612a53d0df74ddf47883"
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")


class ReleaseError(RuntimeError):
    """发布前置条件不满足；不得降级成已发布或继续写入。"""


def version_parts(value):
    match = VERSION.fullmatch(value) if isinstance(value, str) else None
    if not match:
        raise ReleaseError(f"要求正式 MAJOR.MINOR.PATCH 版本，实际 {value!r}")
    return tuple(map(int, match.groups()))


def git(root, *arguments):
    result = subprocess.run(
        ["git", *arguments], cwd=root, capture_output=True, text=True, encoding="utf-8"
    )
    if result.returncode:
        raise ReleaseError(f"git {arguments[0]} 失败：{result.stderr.strip()}")
    return result.stdout.strip()


def manifest_at(root, sha, crate):
    manifest = tomllib.loads(git(root, "show", f"{sha}:crates/{crate}/Cargo.toml"))
    package = manifest.get("package", {})
    if package.get("name") != crate:
        raise ReleaseError(f"{crate} manifest 的包名不匹配")
    version_parts(package.get("version"))
    return manifest


def release_context(root, environ):
    if environ.get("GITHUB_REF") != "refs/heads/main":
        raise ReleaseError("仅 main 分支允许发布；dev 和版本 tag 均不能发布")
    if environ.get("GITHUB_EVENT_NAME") not in ("push", "workflow_dispatch"):
        raise ReleaseError("只接受 main 推送或 main 手动兜底事件")
    repository = environ.get("GITHUB_REPOSITORY", "")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ReleaseError("GitHub 仓库标识无效")
    sha = environ.get("GITHUB_SHA", "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or git(root, "rev-parse", "HEAD") != sha:
        raise ReleaseError("checkout 必须是本次 main 事件的精确提交")
    return repository, sha


class GitHub:
    def __init__(self, repository, token=None):
        self.repository = repository
        self.token = os.environ.get("GH_TOKEN", "") if token is None else token

    def get(self, suffix):
        # 此认证头只发送给 api.github.com；crates.io 请求由 registry.py 独立处理。
        headers = {
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "kenvoy-release",
        }
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        request = urllib.request.Request(
            f"https://api.github.com/repos/{self.repository}/{suffix}", headers=headers
        )
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code == 404:
                return None
            raise ReleaseError(f"GitHub 查询失败（HTTP {error.code}）：{suffix}") from None

    def main_sha(self):
        reference = self.get("git/ref/heads/main")
        if reference is None:
            raise ReleaseError("远端 main 不存在或无读取权限")
        return reference["object"]["sha"]

    def release(self, tag):
        return self.get(f"releases/tags/{urllib.parse.quote(tag, safe='')}")

    def tag_sha(self, tag):
        reference = self.get(f"git/ref/tags/{urllib.parse.quote(tag, safe='')}")
        if reference is None:
            raise ReleaseError(f"远端缺少 {tag}；先逐个推送版本 tag，再合并/推送 main")
        target = reference["object"]
        for _ in range(8):
            if target["type"] == "commit":
                return target["sha"]
            if target["type"] != "tag":
                break
            annotated = self.get(f"git/tags/{target['sha']}")
            if annotated is None:
                break
            target = annotated["object"]
        raise ReleaseError(f"{tag} 没有指向有效提交")


def require_latest(root, environ, github):
    _, sha = release_context(root, environ)
    if github.main_sha() != sha:
        raise ReleaseError("远端 main 已更新，本次旧提交不得继续发布；由最新运行接续")
    return sha


def require_tag(root, sha, version, github, source_required=False):
    tag = f"v{version}"
    tag_sha = github.tag_sha(tag)
    local = git(root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    if local != tag_sha:
        raise ReleaseError(f"本地 {tag} 与远端不同，不移动或覆盖既有标签")
    git(root, "merge-base", "--is-ancestor", tag_sha, sha)
    if manifest_at(root, tag_sha, "kenvoy")["package"]["version"] != version:
        raise ReleaseError(f"{tag} 与标签提交的 Kenvoy CLI 版本不符")
    if source_required and git(root, "rev-parse", f"{tag_sha}^{{tree}}") != git(
        root, "rev-parse", f"{sha}^{{tree}}"
    ):
        raise ReleaseError("待发布源码与版本 tag 的树不同；请完成新版本并打新 tag，不移动旧 tag")
    return tag_sha


def asset_name(manifest, target, version, repository):
    metadata = manifest["package"].get("metadata", {}).get("binstall", {})
    template = metadata.get("overrides", {}).get(target, {}).get("pkg-url", metadata.get("pkg-url"))
    if not isinstance(template, str):
        raise ReleaseError("发布包缺少 cargo-binstall 的 pkg-url")
    if "-windows-" in target:
        operating_system = "windows"
    elif "-linux-" in target:
        operating_system = "linux"
    elif target.endswith("-apple-darwin"):
        operating_system = "macos"
    else:
        raise ReleaseError(f"不支持的分发目标：{target}")
    values = {"repo": f"https://github.com/{repository}", "version": version,
              "name": "kenvoy", "target": target, "os-name": operating_system,
              "target-arch": target.split("-", 1)[0]}

    def substitute(match):
        key = match.group(1).strip()
        if key not in values:
            raise ReleaseError(f"不支持的 binstall URL 占位符：{key}")
        return values[key]

    url = re.sub(r"\{([^{}]+)\}", substitute, template)
    prefix = f"https://github.com/{repository}/releases/download/v{version}/"
    if not url.startswith(prefix):
        raise ReleaseError("binstall 下载地址必须指向本仓库的精确版本 Release")
    name = url.removeprefix(prefix)
    if not re.fullmatch(r"kenvoy-[A-Za-z0-9_.-]+\.zip", name):
        raise ReleaseError(f"分发文件名不安全或不是 ZIP：{name}")
    return name


def release_assets(release):
    return {asset["name"]: asset for asset in (release or {}).get("assets", [])
            if asset.get("state") == "uploaded" and asset.get("size", 0) > 0}


def write_outputs(plan, output_path=None):
    if output_path:
        Path(output_path).write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    fields = ("version", "engine_version", "tag", "active", "publish_engine", "publish_kenvoy", "release_required")
    output = {key: plan[key] for key in fields}
    output["build_required"] = bool(plan["matrix"]["include"])
    output["matrix"] = plan["matrix"]
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
            for key, value in output.items():
                stream.write(f"{key}={json.dumps(value, separators=(',', ':')) if not isinstance(value, str) else value}\n")
    print(json.dumps(plan, ensure_ascii=False, indent=2))
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(f"## Kenvoy {plan['version']}\n\n{plan['reason']}\n")


def load_current_plan(root, path, environ, github):
    plan = json.loads(Path(path).read_text(encoding="utf-8"))
    repository, sha = release_context(root, environ)
    if plan["repository"] != repository or plan["sha"] != sha:
        raise ReleaseError("发布计划不属于当前仓库及提交")
    require_latest(root, environ, github)
    if manifest_at(root, sha, "kenvoy")["package"]["version"] != plan["version"]:
        raise ReleaseError("计划版本与 main 当前 Kenvoy CLI 不一致")
    if manifest_at(root, sha, "kengine")["package"]["version"] != plan["engine_version"]:
        raise ReleaseError("计划版本与 main 当前 KEngine 不一致")
    return plan
