"""同版 npm 发布：先 @kenvoy/types 后 kenvoy；默认只读，逐包跳过已存在版本。"""

import argparse
import json
import os
import subprocess
import sys
import tarfile
from pathlib import Path

import publish_npm
import type_assets
from manifests import ChannelError, load_assets


def load_release(assets_path, directory, installer_path):
    assets = load_assets(assets_path)
    installer = publish_npm.load_package(installer_path)
    if installer.version != assets.version:
        raise ChannelError("kenvoy 安装器、类型资产和 CLI 必须使用同一版本")
    downloads = json.loads((installer.directory / "download-manifest.json").read_text(encoding="utf-8"))
    if downloads["repository"] != assets.repository:
        raise ChannelError("kenvoy 安装器和类型资产必须来自同一正式 Release 仓库")
    manifest = json.loads(Path(assets_path).read_text(encoding="utf-8"))
    tarball = type_assets.verify(directory, manifest)
    return publish_npm.Package("@kenvoy/types", assets.version, tarball.parent), tarball


def publish_tarball(package, tarball):
    # 原样发布已经过正式 Release 官方摘要校验的 tarball；不在渠道重新构建或 pack。
    try:
        result = subprocess.run(
            [publish_npm.npm_executable(), "publish", str(tarball.resolve()), "--access", "public",
             "--provenance", "--ignore-scripts", "--registry", publish_npm.REGISTRY],
            cwd=tarball.parent, capture_output=True, text=True, encoding="utf-8", errors="replace",
        )
    except OSError:
        raise ChannelError(f"{package.spec} 无法启动 npm；已停止，未重试") from None
    if result.returncode:
        raise ChannelError(f"{package.spec} 发布失败（npm 退出码 {result.returncode}）；已停止，未重试")


def run(assets_path, directory, installer_path, publish=False):
    if publish and os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ChannelError("实际 npm 发布仅允许 GITHUB_REF=refs/heads/main")
    # 两包所有本地校验都先完成，不能先上传一包再发现另一包与 CLI 版本不符。
    package, tarball = load_release(assets_path, directory, installer_path)
    if publish_npm.version_exists(package):
        print(f"{package.spec} 已存在，跳过")
    elif not publish:
        print(f"{package.spec} 计划发布（只读，未上传）")
    else:
        publish_tarball(package, tarball)
        if not publish_npm.version_exists(package):
            raise ChannelError(f"{package.spec} 上传后尚不可见；停止安装器发布，不重复上传，请稍后重跑正式入口")
        print(f"{package.spec} 已发布")
    publish_npm.run(installer_path, publish)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", required=True, type=Path, help="正式 Release 的 assets.json")
    parser.add_argument("--types", required=True, type=Path, help="已下载类型 tarball 与侧车的目录")
    parser.add_argument("--package", required=True, type=Path, help="严格校验的 kenvoy 安装器目录")
    parser.add_argument("--publish", action="store_true", help="仅 main：先类型包，再安装器；不重试写请求")
    arguments = parser.parse_args(argv)
    try:
        run(arguments.assets, arguments.types, arguments.package, arguments.publish)
    except ChannelError as error:
        print(f"同版 npm 发布失败：{error}", file=sys.stderr)
        return 1
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
        print(f"同版 npm 发布预检失败：{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
