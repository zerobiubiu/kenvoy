"""仅从同版本已发布 CLI 源码包补齐正式类型资产，不从 checkout 重建声明。"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

import registry
import type_package
from release_common import (
    GitHub, ReleaseError, load_current_plan, release_assets, release_context,
    require_latest, require_tag,
)


def pack_environment(environ, work):
    excluded = {"CARGO_TOKEN", "CARGO_REGISTRY_TOKEN", "CARGO_REGISTRIES_CRATES_IO_TOKEN",
                "GH_TOKEN", "GITHUB_TOKEN", "NPM_TOKEN", "NODE_AUTH_TOKEN",
                "ACTIONS_ID_TOKEN_REQUEST_TOKEN", "ACTIONS_ID_TOKEN_REQUEST_URL"}
    environment = {key: value for key, value in environ.items()
                   if key.upper() not in excluded and not key.upper().startswith("NPM_CONFIG_")}
    for name in ("user.npmrc", "global.npmrc"):
        (work / name).write_text("", encoding="utf-8")
    environment.update({
        "NPM_CONFIG_USERCONFIG": str(work / "user.npmrc"),
        "NPM_CONFIG_GLOBALCONFIG": str(work / "global.npmrc"),
        "NPM_CONFIG_CACHE": str(work / "npm-cache"),
    })
    return environment


def pack(directory, output, version, repository, environ, work):
    expected = type_package.directory_files(directory, version, repository)
    name = type_package.archive_name(version)
    target = output / name
    if target.exists() or target.is_symlink() or (output / f"{name}.sha256").exists():
        raise ReleaseError("类型资产输出已存在，拒绝覆盖")
    executable = shutil.which("npm.cmd" if os.name == "nt" else "npm")
    if executable is None:
        raise ReleaseError("缺少 npm，不能打包已发布的纯类型目录")
    environment = pack_environment(environ, work)
    output.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [executable, "pack", "--ignore-scripts", "--json", "--pack-destination", str(output.resolve())],
        cwd=directory, env=environment, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if result.returncode:
        raise ReleaseError(f"纯类型 npm pack 失败（退出码 {result.returncode}）；未重试")
    try:
        report = json.loads(result.stdout)
        if isinstance(report, dict):
            if set(report) != {type_package.PACKAGE_NAME}:
                raise ValueError()
            report = list(report.values())
        if (not isinstance(report, list) or len(report) != 1
                or not isinstance(report[0], dict)
                or report[0].get("name") != type_package.PACKAGE_NAME
                or report[0].get("version") != version or report[0].get("filename") != name):
            raise ValueError()
    except (ValueError, AttributeError):
        raise ReleaseError("npm pack 未返回唯一的同版本类型 tarball") from None
    type_package.require_same_files(type_package.tarball_files(target, version, repository), expected)
    with (output / f"{name}.sha256").open("x", encoding="ascii", newline="\n") as stream:
        stream.write(f"{type_package.digest(target)}  {name}\n")
    return target


def prepare(root, plan, output, github, environ):
    require_latest(root, environ, github)
    version, repository = plan["version"], plan["repository"]
    require_tag(root, plan["sha"], version, github)
    name = type_package.archive_name(version)
    if plan["types_archive"] != name:
        raise ReleaseError("计划中的类型资产与 CLI 版本不一致")
    metadata = registry.registry_version("kenvoy", version)
    if metadata is None or metadata.get("yanked"):
        raise ReleaseError(f"需要同版本已正式发布且未 yank 的 kenvoy {version} 源码包")
    payload = registry.download_crate("kenvoy", version, metadata)
    with tempfile.TemporaryDirectory(prefix="kenvoy-types-", dir=environ.get("RUNNER_TEMP")) as temporary:
        work = Path(temporary)
        source = registry.extract_crate(payload, "kenvoy", version, work)
        directory = source / type_package.PACKAGE_DIRECTORY
        expected = type_package.directory_files(directory, version, repository)
        assets = release_assets(github.release(plan["tag"]))
        if name in assets:
            # 恢复仅复用既有 tarball；即使它带正确包名，也不能冒充另一份声明。
            subprocess.run(["gh", "release", "download", plan["tag"], "--repo", repository,
                            "--pattern", name, "--dir", str(work)], check=True)
            target = work / name
            official = assets[name].get("digest")
            if official and official != f"sha256:{type_package.digest(target)}":
                raise ReleaseError("既有类型资产与 GitHub 官方 digest 不一致，拒绝覆盖")
            type_package.require_same_files(type_package.tarball_files(target, version, repository), expected)
            print(f"{name} 已存在且与同版本 CLI 源码包一致；只补缺失侧车")
            return
        pack(directory, Path(output), version, repository, environ, work)
    print(f"{name} 已从同版本 registry 源码包生成，未读取当前 checkout 声明")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True)
    parser.add_argument("--output", required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    repository, _ = release_context(root, os.environ)
    github = GitHub(repository)
    plan = load_current_plan(root, arguments.plan, os.environ, github)
    prepare(root, plan, Path(arguments.output), github, os.environ)


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError, ValueError, KeyError, tarfile.TarError, EOFError,
            subprocess.CalledProcessError) as error:
        print(f"类型资产打包失败：{error}", file=sys.stderr)
        sys.exit(1)
