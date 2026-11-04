"""从 main 精确提交的仓库外快照打包，按依赖顺序发布缺失源包。"""

import argparse
import hashlib
import os
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path

import registry
from release_common import (
    GitHub, ReleaseError, load_current_plan, release_context, require_latest, require_tag,
)


SECRET_KEYS = (
    "CARGO_TOKEN", "CARGO_REGISTRY_TOKEN", "CARGO_REGISTRIES_CRATES_IO_TOKEN",
    "GH_TOKEN", "GITHUB_TOKEN",
)


def build_environment(environ):
    return {key: value for key, value in environ.items() if key not in SECRET_KEYS}


def snapshot(root, sha, directory):
    archive = directory / "source.tar"
    source = directory / "source"
    subprocess.run(
        ["git", "archive", "--format=tar", "--output", str(archive), sha],
        cwd=root, check=True,
    )
    source.mkdir()
    with tarfile.open(archive) as stream:
        stream.extractall(source, filter="data")
    archive.unlink()
    return source


def prepare_frontend(source, environment):
    frontend = source / "crates" / "kenvoy" / "frontend"
    for arguments in (
        ["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        ["npm", "run", "build"],
    ):
        subprocess.run(arguments, cwd=frontend, check=True, env=environment)


def await_registry(crate, version, expected_checksum):
    # 只等待只读 registry 可见性，不重试 cargo publish 写请求。
    for attempt in range(12):
        metadata = registry.registry_version(crate, version)
        if metadata is not None:
            if metadata["checksum"] != expected_checksum:
                raise ReleaseError(f"{crate} {version} 的 registry 校验和与本次源包不同")
            print(f"{crate} {version} 已在 registry 可见，SHA256 一致")
            return
        if attempt != 11:
            time.sleep(5)
    raise ReleaseError(f"{crate} {version} 上传后尚未可见；不重复上传，请稍后使用 main 手动兜底")


def publish(root, plan, github, environ):
    versions = {"kengine": plan["engine_version"], "kenvoy": plan["version"]}
    missing = [crate for crate, version in versions.items()
               if registry.registry_version(crate, version) is None]
    if not missing:
        print("两个源包均已存在，跳过 cargo publish")
        return
    token = environ.get("CARGO_TOKEN", "").strip()
    if not token:
        raise ReleaseError("缺少仓库 Secret CARGO_TOKEN；未执行任何上传")
    require_tag(root, plan["sha"], plan["version"], github, source_required=True)
    environment = build_environment(environ)
    temporary_root = environ.get("RUNNER_TEMP")
    with tempfile.TemporaryDirectory(prefix="kenvoy-publish-", dir=temporary_root) as directory:
        source = snapshot(root, plan["sha"], Path(directory))
        # 发行资产在独占快照生成，不污染 checkout，不用 allow-dirty/no-verify。
        if "kenvoy" in missing:
            prepare_frontend(source, environment)
        packages = [argument for crate in missing for argument in ("--package", crate)]
        subprocess.run(
            ["cargo", "publish", "--dry-run", "--locked", *packages],
            cwd=source, env=environment, check=True,
        )
        for crate in missing:
            require_latest(root, environ, github)
            version = versions[crate]
            # 重跑/并发完成的源包只跳过，不覆盖，也不尝试用新内容占用旧版本号。
            if registry.registry_version(crate, version) is not None:
                print(f"{crate} {version} 已存在，跳过")
                continue
            # 默认 crates.io 使用 registry.token，而不是命名仓库的 registries.<name>.token。
            subprocess.run(
                ["cargo", "publish", "--locked", "--package", crate],
                cwd=source, check=True,
                env={**environment, "CARGO_REGISTRY_TOKEN": token},
            )
            package = source / "target" / "package" / f"{crate}-{version}.crate"
            digest = hashlib.sha256(package.read_bytes()).hexdigest()
            await_registry(crate, version, digest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    repository, _ = release_context(root, os.environ)
    github = GitHub(repository)
    plan = load_current_plan(root, arguments.plan, os.environ, github)
    if not plan["active"]:
        print("本次没有发布任务")
        return
    publish(root, plan, github, os.environ)


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"源包发布失败：{error}", file=sys.stderr)
        sys.exit(1)
