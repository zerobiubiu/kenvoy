"""只规划 main 当前版本；扫描 Git 历史和发布旧版本都不属于此入口。"""

import argparse
import json
import os
import sys
from pathlib import Path

import registry
from release_common import (
    ROOT_COMMIT, GitHub, ReleaseError, asset_name, git, manifest_at,
    release_assets, release_context, require_tag, write_outputs,
)


def plan_release(root, environ, github=None, registry_client=registry):
    repository, sha = release_context(root, environ)
    github = github or GitHub(repository)
    cli = manifest_at(root, sha, "kenvoy")
    engine = manifest_at(root, sha, "kengine")
    version = cli["package"]["version"]
    engine_version = engine["package"]["version"]
    plan = {
        "repository": repository, "sha": sha, "version": version,
        "engine_version": engine_version, "tag": f"v{version}",
        "active": False, "publish_engine": False, "publish_kenvoy": False,
        "release_required": False, "matrix": {"include": []},
        "archives": [], "reason": "当前版本的源包与二进制资产均已发布，本次跳过。",
    }
    if github.main_sha() != sha:
        plan["reason"] = "此运行已不是远端 main 最新提交，跳过，不补发历史版本。"
        return plan
    if sha == ROOT_COMMIT or version == "0.0.0":
        plan["reason"] = "0.0.0 init 仅标记共同根节点，不是发布版本。"
        return plan

    cli_state = registry_client.registry_version("kenvoy", version)
    engine_state = registry_client.registry_version("kengine", engine_version)
    for crate, state in (("kenvoy", cli_state), ("kengine", engine_state)):
        if state and state.get("yanked"):
            raise ReleaseError(f"{crate} 的当前版本已被 yank，不会覆盖或自动恢复")
    release = github.release(plan["tag"])
    assets = release_assets(release)

    # 已发布源包不可改写。补资产时以该版本实际发布的下载元数据为准，
    # 不因当前仓库修改命名规范而覆盖旧版本资产。
    published_manifest = cli
    if cli_state is not None:
        payload = registry_client.download_crate("kenvoy", version, cli_state)
        published_manifest = registry_client.package_manifest(payload, "kenvoy", version)
    targets = json.loads(git(root, "show", f"{sha}:.github/distribution-targets.json"))
    if len(targets) != 6 or len({item["target"] for item in targets}) != 6:
        raise ReleaseError("发布目标必须是六个互不重复的系统/架构组合")
    expected = set()
    for target in targets:
        name = asset_name(published_manifest, target["target"], version, repository)
        if name in expected:
            raise ReleaseError("不同目标不能使用同一分发文件名")
        plan["archives"].append({"target": target["target"], "name": name})
        expected.update((name, f"{name}.sha256"))
        if name not in assets:
            plan["matrix"]["include"].append({**target, "archive": name})

    plan["publish_engine"] = engine_state is None
    plan["publish_kenvoy"] = cli_state is None
    plan["release_required"] = (
        release is None or release.get("draft", False) or not expected.issubset(assets)
    )
    plan["active"] = plan["publish_engine"] or plan["publish_kenvoy"] or plan["release_required"]
    if not plan["active"]:
        return plan

    plan["tag_sha"] = require_tag(
        root, sha, version, github,
        source_required=plan["publish_engine"] or plan["publish_kenvoy"],
    )
    plan["reason"] = (
        f"仅处理 main 当前版本 {version}："
        f"kengine {'待发布' if plan['publish_engine'] else '已存在'}，"
        f"kenvoy {'待发布' if plan['publish_kenvoy'] else '已存在'}，"
        f"需构建 {len(plan['matrix']['include'])} 个目标；不扫描历史版本。"
    )
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", help="发布计划 JSON 文件")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    write_outputs(plan_release(root, os.environ), arguments.output)


if __name__ == "__main__":
    try:
        main()
    except (ReleaseError, OSError, ValueError, KeyError) as error:
        print(f"发布计划失败：{error}", file=sys.stderr)
        sys.exit(1)
