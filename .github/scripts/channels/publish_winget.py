"""只读规划或在 main 提交 WinGet 清单；已有版本、PR 与部分完成分支可安全重跑。"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlencode

from manifests import ChannelError, IDENTIFIER, MANIFEST_VERSION, VERSION

UPSTREAM = "microsoft/winget-pkgs"
OWNER = re.compile(r"[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*\Z")
FILES = {
    f"{IDENTIFIER}.yaml": "version",
    f"{IDENTIFIER}.installer.yaml": "installer",
    f"{IDENTIFIER}.locale.en-US.yaml": "locale",
}
PR_URL = re.compile(r"https://github\.com/microsoft/winget-pkgs/pull/[1-9][0-9]*\Z")


def _run(command, cwd=None):
    environment = dict(os.environ)
    environment.update(GH_HOST="github.com", GH_PROMPT_DISABLED="1", GIT_TERMINAL_PROMPT="0")
    environment.pop("GH_DEBUG", None)
    try:
        return subprocess.run(command, cwd=cwd, env=environment, capture_output=True,
                              text=True, encoding="utf-8", errors="replace")
    except OSError as error:
        raise ChannelError(f"无法执行 {command[0]}：{error}") from None


def _failure(result, operation):
    detail = result.stderr.strip() or f"退出码 {result.returncode}，没有错误详情"
    raise ChannelError(f"{operation}失败：{detail}")


def _api(endpoint, *, method="GET", fields=None, missing=False):
    command = ["gh", "api", "--hostname", "github.com", "--method", method,
               "--include", "--header", "Accept: application/vnd.github+json", endpoint]
    for key, value in (fields or {}).items():
        command.extend(["--raw-field", f"{key}={value}"])
    result = _run(command)
    headers, separator, body = result.stdout.replace("\r\n", "\n").partition("\n\n")
    status = re.match(r"\AHTTP/\S+ ([0-9]{3})(?: |\n|$)", headers)
    if not separator or status is None:
        if result.returncode:
            _failure(result, f"GitHub {method} {endpoint}（未收到明确 HTTP 状态）")
        raise ChannelError(f"GitHub {endpoint} 响应缺少 HTTP 状态或头部边界")
    code = int(status[1])
    if missing and method == "GET" and code == 404:
        return None
    if result.returncode or not 200 <= code < 300:
        _failure(result, f"GitHub {method} {endpoint}（HTTP {code}）")
    try:
        value = json.loads(body)
    except json.JSONDecodeError:
        raise ChannelError(f"GitHub {endpoint} 响应不是合法 JSON") from None
    if value is None:
        raise ChannelError(f"GitHub {endpoint} 返回 null，不能将损坏响应当作 HTTP 404")
    return value


def _git(arguments, cwd=None, *, quiet_diff=False):
    # 认证仅交给 gh 的 credential helper，不读取令牌，也不修改全局 Git 配置。
    command = ["git", "-c", "credential.helper=", "-c", "credential.helper=!gh auth git-credential",
               "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false",
               "-c", "user.name=github-actions[bot]",
               "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com", *arguments]
    result = _run(command, cwd)
    if result.returncode and not (quiet_diff and result.returncode == 1):
        _failure(result, f"git {arguments[0]}")
    return result


def _manifests(directory, version):
    root = Path(directory)
    if not root.is_dir() or {path.name for path in root.iterdir()} != set(FILES):
        raise ChannelError("--manifests 必须是仅含 Kenvoy.Kenvoy 三份 YAML 的版本目录")
    contents = {}
    for name, kind in FILES.items():
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ChannelError(f"清单必须是普通文件，不能是符号链接：{name}")
        payload = path.read_bytes()
        lines = payload.decode("utf-8").splitlines()
        # 只接收本仓库生成器的元数据格式；不是通用 YAML 解析器。
        for key, expected in {"PackageIdentifier": IDENTIFIER, "PackageVersion": version,
                              "ManifestType": kind, "ManifestVersion": MANIFEST_VERSION}.items():
            values = [line.split(":", 1)[1].strip() for line in lines if line.startswith(f"{key}:")]
            if values != [expected]:
                raise ChannelError(f"{name} 的 {key} 必须唯一且等于 {expected}")
        contents[name] = payload
    return contents


def _repository(value, expected, *, fork=False):
    if not isinstance(value, dict) or str(value.get("full_name", "")).lower() != expected.lower():
        raise ChannelError(f"GitHub 仓库响应与目标 {expected} 不符")
    parent = value.get("parent")
    if fork and (value.get("fork") is not True or not isinstance(parent, dict)
                 or str(parent.get("full_name", "")).lower() != UPSTREAM.lower()):
        raise ChannelError(f"{expected} 不是 {UPSTREAM} 的直接 fork，拒绝写入")
    return value


def _pull(value, owner, branch, base):
    if not isinstance(value, dict):
        raise ChannelError("GitHub PR 响应不是对象")
    head, target = value.get("head"), value.get("base")
    if not isinstance(head, dict) or not isinstance(target, dict):
        raise ChannelError("GitHub PR 响应缺少 head/base 对象")
    source_repo, target_repo = head.get("repo"), target.get("repo")
    url = value.get("html_url", "")
    if (value.get("state") != "open" or head.get("ref") != branch or target.get("ref") != base
            or not isinstance(source_repo, dict) or not isinstance(target_repo, dict)
            or str(source_repo.get("full_name", "")).lower() != f"{owner}/winget-pkgs".lower()
            or str(target_repo.get("full_name", "")).lower() != UPSTREAM.lower()
            or not isinstance(url, str) or not PR_URL.fullmatch(url)):
        raise ChannelError("GitHub PR 响应与指定上游、fork、分支或 open 状态不符")
    return url


def _open_pull(owner, branch, base):
    query = urlencode({"state": "open", "head": f"{owner}:{branch}", "base": base, "per_page": 100})
    values = _api(f"repos/{UPSTREAM}/pulls?{query}")
    if not isinstance(values, list) or len(values) > 1:
        raise ChannelError("GitHub open PR 查询不是唯一分支对应的列表，拒绝重复创建")
    return _pull(values[0], owner, branch, base) if values else None


def _fork_fields(owner):
    account = _api(f"users/{owner}")
    if not isinstance(account, dict) or str(account.get("login", "")).lower() != owner.lower():
        raise ChannelError("GitHub owner 查询与 --owner 不符")
    fields = {"name": "winget-pkgs"}
    if account.get("type") == "Organization":
        fields["organization"] = owner
    elif account.get("type") == "User":
        actor = _api("user")
        if not isinstance(actor, dict) or str(actor.get("login", "")).lower() != owner.lower():
            raise ChannelError("只能为当前认证用户或其有权限的组织创建 fork，--owner 与认证用户不符")
    else:
        raise ChannelError("--owner 必须是 GitHub 用户或组织")
    return fields


def _prepare(root, owner, base, branch, relative, contents, branch_exists):
    checkout = root / "checkout"
    _git(["clone", "--filter=blob:none", "--no-checkout", "--single-branch", "--branch", base,
          f"https://github.com/{UPSTREAM}.git", str(checkout)])
    _git(["remote", "add", "fork", f"https://github.com/{owner}/winget-pkgs.git"], checkout)
    _git(["sparse-checkout", "set", "--cone", relative], checkout)
    start = f"refs/remotes/origin/{base}"
    if branch_exists:
        _git(["fetch", "--no-tags", "--filter=blob:none", "fork", f"refs/heads/{branch}"], checkout)
        start = "FETCH_HEAD"
    _git(["checkout", "-b", branch, start], checkout)
    if branch_exists:
        changed = _git(["diff", "--name-only", "-z", f"refs/remotes/origin/{base}...HEAD"], checkout)
        paths = set(filter(None, changed.stdout.split("\0")))
        if not paths.issubset({f"{relative}/{name}" for name in contents}):
            raise ChannelError("已有 WinGet 分支含本次三份清单之外的改动，拒绝继续提交 PR")
    destination = checkout / relative
    for path in [destination, *destination.parents]:
        if path == checkout:
            break
        if path.is_symlink():
            raise ChannelError("清单目标目录含符号链接，拒绝写入")
    destination.mkdir(parents=True, exist_ok=True)
    for name, payload in contents.items():
        path = destination / name
        if path.is_symlink():
            raise ChannelError(f"已有分支清单是符号链接，拒绝覆盖：{name}")
        path.write_bytes(payload)
    _git(["add", "--", relative], checkout)
    changed = _git(["diff", "--cached", "--quiet"], checkout, quiet_diff=True).returncode == 1
    if changed:
        _git(["commit", "-m", f"New version: {IDENTIFIER} {branch.removeprefix('kenvoy-')}"], checkout)
    elif not branch_exists:
        raise ChannelError("上游内容已变化且没有可提交差异，请重新运行只读计划")
    return checkout, changed


def publish(manifests, version, owner, *, apply=False):
    if not VERSION.fullmatch(version):
        raise ChannelError("--version 必须是 X.Y.Z 正式版本")
    if not 1 <= len(owner) <= 39 or not OWNER.fullmatch(owner):
        raise ChannelError("--owner 必须是有效 GitHub 账号名，不允许路径、连续或首尾连字符")
    if apply and os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ChannelError("WinGet 发布仅允许 GITHUB_REF=refs/heads/main；其他分支只能只读规划")
    contents = _manifests(manifests, version)
    branch, fork = f"kenvoy-{version}", f"{owner}/winget-pkgs"
    relative = f"manifests/k/Kenvoy/Kenvoy/{version}"
    upstream = _repository(_api(f"repos/{UPSTREAM}"), UPSTREAM)
    base = upstream.get("default_branch")
    if not isinstance(base, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]*", base):
        raise ChannelError("上游 default_branch 缺失或无效")
    upstream_files = _api(f"repos/{UPSTREAM}/contents/{relative}?{urlencode({'ref': base})}", missing=True)
    if upstream_files is not None:
        if not isinstance(upstream_files, list) or not set(FILES).issubset({
            item.get("name") for item in upstream_files if isinstance(item, dict)
            and isinstance(item.get("name"), str) and item.get("type") == "file"
            and item.get("path") == f"{relative}/{item.get('name')}"
        }):
            raise ChannelError("上游版本路径存在，但缺少完整三清单，不能视为已收录")
        return f"WinGet {version}：上游已收录，跳过"
    repository = _api(f"repos/{fork}", missing=True)
    if repository is not None:
        _repository(repository, fork, fork=True)
    url = _open_pull(owner, branch, base)
    if url:
        if repository is None:
            raise ChannelError("已有 open PR，但目标 fork 不可读，拒绝判为成功")
        return f"WinGet {version}：已有 PR {url}（仍需上游审核，不代表可安装）"
    existing = None
    if repository is not None:
        existing = _api(f"repos/{fork}/git/ref/heads/{branch}", missing=True)
        if existing is not None and (not isinstance(existing, dict)
                or existing.get("ref") != f"refs/heads/{branch}"
                or not isinstance(existing.get("object"), dict)
                or existing["object"].get("type") != "commit"):
            raise ChannelError("fork 分支查询响应无效")
    if not apply:
        action = "创建 fork、分支与 PR" if repository is None else (
            "续接已有分支并创建 PR" if existing is not None else "创建分支与 PR")
        return f"WinGet {version}：只读计划，将{action}；{fork}:{branch} → {UPSTREAM}:{base}"
    if repository is not None and (not isinstance(repository.get("permissions"), dict)
                                   or repository["permissions"].get("push") is not True):
        raise ChannelError(f"当前认证没有 {fork} 的 push 权限，未执行写入")
    fields = _fork_fields(owner) if repository is None else None
    with tempfile.TemporaryDirectory(prefix="kenvoy-winget-") as directory:
        checkout, changed = _prepare(Path(directory), owner, base, branch, relative, contents, existing is not None)
        if repository is None:
            _repository(_api(f"repos/{UPSTREAM}/forks", method="POST", fields=fields), fork, fork=True)
        if changed:
            _git(["push", "fork", f"HEAD:refs/heads/{branch}"], checkout)
        # 再读一次以接住其他运行已创建的 PR；写请求失败仍立即报错，交由下次运行恢复。
        url = _open_pull(owner, branch, base)
        if url:
            return f"WinGet {version}：已有 PR {url}（仍需上游审核，不代表可安装）"
        result = _api(f"repos/{UPSTREAM}/pulls", method="POST", fields={
            "head": f"{owner}:{branch}", "base": base,
            "title": f"New version: {IDENTIFIER} version {version}",
            "body": "Generated from the published Kenvoy release archives and their official SHA256 digests.",
        })
        url = _pull(result, owner, branch, base)
    return f"WinGet {version}：已创建 PR {url}（仍需上游审核，不代表可安装）"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifests", required=True, type=Path, help="生成器输出的三 YAML 版本目录")
    parser.add_argument("--version", required=True)
    parser.add_argument("--owner", required=True, help="GitHub fork 所属用户或组织")
    parser.add_argument("--publish", action="store_true", help="仅 main：创建所需 fork、提交分支与 PR")
    arguments = parser.parse_args(argv)
    try:
        message = publish(arguments.manifests, arguments.version, arguments.owner, apply=arguments.publish)
    except (ChannelError, OSError, UnicodeError) as error:
        print(f"WinGet 发布失败：{error}", file=sys.stderr)
        return 1
    print(message)
    return 0


if __name__ == "__main__":
    sys.exit(main())
