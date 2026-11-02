"""补齐 main 当前版本 Release 的缺失资产；已有 ZIP 和标签一律不覆盖。"""

import argparse
import hashlib
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from release_common import (
    GitHub, ReleaseError, load_current_plan, release_assets, release_context,
    require_latest, require_tag,
)


def gh(repository, *arguments):
    subprocess.run(["gh", "release", *arguments, "--repo", repository], check=True)


def checksum_text(digest, name):
    return f"{digest}  {name}\n"


def read_checksum(path, name):
    parts = path.read_text(encoding="ascii").split()
    if len(parts) != 2 or parts[1] != name or not re.fullmatch(r"[0-9a-f]{64}", parts[0]):
        raise ReleaseError(f"{path.name} 内容必须是对应 ZIP 的 SHA256 与精确文件名")
    return parts[0]


def local_digest(directory, name):
    archive = directory / name
    if not archive.is_file():
        raise ReleaseError(f"缺少构建结果 {name}")
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if read_checksum(directory / f"{name}.sha256", name) != digest:
        raise ReleaseError(f"{name} 与 SHA256 侧车不一致")
    return digest


def download_asset(repository, tag, name, directory):
    gh(repository, "download", tag, "--pattern", name, "--dir", str(directory))
    path = directory / name
    if not path.is_file():
        raise ReleaseError(f"未能下载既有资产 {name}")
    return path


def existing_digest(repository, tag, asset, directory):
    digest = asset.get("digest", "") or ""
    if re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
        return digest.removeprefix("sha256:")
    path = download_asset(repository, tag, asset["name"], directory)
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def complete_release(root, plan, directory, github, environ):
    require_latest(root, environ, github)
    tag = plan["tag"]
    require_tag(root, plan["sha"], plan["version"], github)
    repository = plan["repository"]
    release = github.release(tag)
    assets = release_assets(release)
    names = [item["name"] for item in plan["archives"]]
    # 先核对全部本次需要上传的 ZIP，避免把坏包上传后再发现校验失败。
    for name in names:
        if name not in assets:
            local_digest(directory, name)
    title = f"Kenvoy {plan['version']}"
    if release is None:
        gh(repository, "create", tag, "--verify-tag", "--draft", "--title", title,
           "--notes", f"{title}：Linux、Windows、macOS 各提供 ARM64 与 x86_64 二进制，ZIP 附 SHA256。")

    with tempfile.TemporaryDirectory(prefix="kenvoy-release-assets-", dir=environ.get("RUNNER_TEMP")) as temporary:
        temporary = Path(temporary)
        for index, name in enumerate(names):
            require_latest(root, environ, github)
            assets = release_assets(github.release(tag))
            work = temporary / str(index)
            work.mkdir()
            sidecar_name = f"{name}.sha256"
            if name not in assets:
                gh(repository, "upload", tag, str(directory / name))
                assets = release_assets(github.release(tag))
                if name not in assets:
                    raise ReleaseError(f"{name} 上传后未出现在 Release 中")
            digest = existing_digest(repository, tag, assets[name], work)
            if sidecar_name in assets:
                sidecar = download_asset(repository, tag, sidecar_name, work)
                if read_checksum(sidecar, name) != digest:
                    raise ReleaseError(f"既有资产 {name} 的 SHA256 不一致，拒绝覆盖；需人工核查")
            else:
                # ZIP 已存在但侧车缺失时，校验值来自既有 ZIP，不是重新构建的 ZIP。
                sidecar = work / sidecar_name
                sidecar.write_text(checksum_text(digest, name), encoding="ascii", newline="\n")
                gh(repository, "upload", tag, str(sidecar))

    require_latest(root, environ, github)
    final = github.release(tag)
    assets = release_assets(final)
    expected = {entry for name in names for entry in (name, f"{name}.sha256")}
    if not expected.issubset(assets):
        raise ReleaseError("六目标资产未齐，Release 保持原状态")
    gh(repository, "edit", tag, "--title", title, "--draft=false", "--latest")
    print(f"{title} 已发布；六目标 ZIP/SHA256 完整，未覆盖既有资产")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True)
    parser.add_argument("--artifacts", required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    repository, _ = release_context(root, os.environ)
    github = GitHub(repository)
    plan = load_current_plan(root, arguments.plan, os.environ, github)
    if not plan["release_required"]:
        print("当前版本 Release 无需更新")
        return
    complete_release(root, plan, Path(arguments.artifacts), github, os.environ)


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"二进制发布失败：{error}", file=sys.stderr)
        sys.exit(1)
