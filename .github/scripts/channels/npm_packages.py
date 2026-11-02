"""npm 发布包生成：主包 JS shim 与六个平台包（内含从 Release ZIP 提取的真实二进制）。

资产契约校验复用 manifests.py，保证 npm 与系统级渠道对同一份 assets.json 的理解一致。
"""

import argparse
import hashlib
import json
import os
import sys
import zipfile
from pathlib import Path, PurePosixPath

from manifests import BINARIES, Asset, Assets, ChannelError, load_assets

DESCRIPTION = "Kenvoy CLI：把 TypeScript/JSX 声明解释为页面、受控 API 与 SQLite 持久化应用"
LICENSE = "MIT"
MAIN_PACKAGE = "kenvoy"
SHIM_PATH = "bin/kenvoy.js"

NPM_OS = {"windows": "win32", "linux": "linux", "macos": "darwin"}
NPM_CPU = {"x86_64": "x64", "ARM64": "arm64"}

# 主包 shim：只负责按 process.platform/arch 找到平台包，转发 argv 与 stdio，并透传退出码。
SHIM_TEMPLATE = """#!/usr/bin/env node
"use strict";

// 该文件由 .github/scripts/channels/npm_packages.py 生成，请勿手工修改。
// 主包不携带二进制：按 platform/arch 解析对应的可选依赖包并转发参数、stdio 与退出码。

const { spawn } = require("node:child_process");

const VERSION = "__VERSION__";
const PACKAGES = __PACKAGES__;
const BINARIES = __BINARIES__;

/**
 * 把 platform/arch 映射到平台包内的二进制绝对路径。
 * 找不到映射或平台包缺失时抛出可读错误，避免调用方看到 spawn ENOENT。
 */
function resolveBinary(platform, arch) {
  const name = PACKAGES[`${platform}-${arch}`];
  if (!name) {
    throw new Error(
      `不支持 ${platform}-${arch}；可用平台：${Object.keys(PACKAGES).join("、")}`,
    );
  }
  try {
    return require.resolve(`${name}/bin/${BINARIES[platform]}`);
  } catch (error) {
    throw new Error(
      `缺少可选依赖 ${name}@${VERSION}（${error.code || error.message}）；` +
        "请重新安装：npm install -g kenvoy --force，并确认没有使用 --omit=optional",
    );
  }
}

function main() {
  let binary;
  try {
    binary = resolveBinary(process.platform, process.arch);
  } catch (error) {
    console.error(`kenvoy: ${error.message}`);
    process.exit(1);
  }

  const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });

  child.on("error", (error) => {
    console.error(`kenvoy: 无法启动 ${binary}：${error.message}`);
    process.exit(1);
  });

  child.on("close", (code, signal) => {
    if (signal) {
      // 子进程被信号终止时，让外层进程报告同一信号，保持调用方看到的行为一致。
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code === null ? 1 : code);
  });
}

main();
"""


def npm_os(platform: str) -> str:
    try:
        return NPM_OS[platform]
    except KeyError:
        raise ChannelError(f"不支持的平台：{platform!r}") from None


def npm_cpu(arch: str) -> str:
    try:
        return NPM_CPU[arch]
    except KeyError:
        raise ChannelError(f"不支持的架构：{arch!r}") from None


def package_name(platform: str, arch: str) -> str:
    """npm 平台包名（不含 scope），与 assets.json 的平台/架构一一对应。"""
    return f"{MAIN_PACKAGE}-{npm_os(platform)}-{npm_cpu(arch)}"


def dependency_spec(assets: Assets) -> dict[str, str]:
    """主包 optionalDependencies：精确锁定同版本，避免 npm 解析到其它版本的二进制。"""
    return {package_name(asset.platform, asset.arch): assets.version for asset in assets.assets}


def shim_source(assets: Assets) -> str:
    packages = {}
    for asset in assets.assets:
        packages[f"{npm_os(asset.platform)}-{npm_cpu(asset.arch)}"] = package_name(
            asset.platform, asset.arch
        )
    binaries = {npm_os(platform): binary for platform, binary in BINARIES.items()}
    return (
        SHIM_TEMPLATE.replace("__VERSION__", assets.version)
        .replace("__PACKAGES__", json.dumps(packages, ensure_ascii=False, indent=2))
        .replace("__BINARIES__", json.dumps(binaries, ensure_ascii=False, indent=2))
    )


def main_package(assets: Assets) -> dict:
    return {
        "name": MAIN_PACKAGE,
        "version": assets.version,
        "description": DESCRIPTION,
        "keywords": ["cli", "low-code", "typescript", "jsx", "sqlite"],
        "homepage": assets.homepage,
        "repository": {"type": "git", "url": f"git+{assets.homepage}.git"},
        "bugs": {"url": f"{assets.homepage}/issues"},
        "license": LICENSE,
        "bin": {"kenvoy": SHIM_PATH},
        "files": [SHIM_PATH],
        "engines": {"node": ">=18"},
        "optionalDependencies": dependency_spec(assets),
    }


def platform_package(assets: Assets, asset: Asset) -> dict:
    return {
        "name": package_name(asset.platform, asset.arch),
        "version": assets.version,
        "description": f"Kenvoy 的 {asset.platform}/{asset.arch} 平台二进制，由主包 kenvoy 调用",
        "homepage": assets.homepage,
        "repository": {"type": "git", "url": f"git+{assets.homepage}.git"},
        "license": LICENSE,
        "os": [npm_os(asset.platform)],
        "cpu": [npm_cpu(asset.arch)],
        "bin": {"kenvoy": f"bin/{asset.binary}"},
        "files": [f"bin/{asset.binary}"],
    }


def _json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, indent=2) + "\n"


def verify_archive(archive: Path, asset: Asset) -> None:
    """发布前核对本地 ZIP 与 assets.json 的 sha256，避免把错版本二进制塞进 npm 包。"""
    if not archive.is_file():
        raise ChannelError(f"缺少资产归档：{archive}（{asset.combination} 需要 {asset.file}）")
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    actual = digest.hexdigest()
    if actual != asset.sha256:
        raise ChannelError(
            f"{asset.file} 的 sha256 与 assets.json 不一致：期望 {asset.sha256}，实际 {actual}"
        )


def binary_member(archive: Path, binary: str) -> str:
    """在 ZIP 中唯一定位二进制条目；含穿越路径、多个同名或空文件时拒绝。"""
    try:
        with zipfile.ZipFile(archive) as bundle:
            matches = []
            for info in bundle.infolist():
                if info.is_dir():
                    continue
                name = info.filename
                parts = PurePosixPath(name).parts
                if name.startswith("/") or "\\" in name or ".." in parts or not parts:
                    raise ChannelError(f"{archive.name} 含可疑路径条目：{name!r}")
                if parts[-1] == binary:
                    matches.append(name)
            if not matches:
                raise ChannelError(f"{archive.name} 内缺少二进制 {binary}")
            if len(matches) > 1:
                raise ChannelError(f"{archive.name} 内含多个 {binary}：{'、'.join(matches)}")
            member = matches[0]
            if bundle.getinfo(member).file_size == 0:
                raise ChannelError(f"{archive.name} 内的 {member} 是空文件")
            return member
    except FileNotFoundError:
        raise ChannelError(f"缺少资产归档：{archive}") from None
    except zipfile.BadZipFile as error:
        raise ChannelError(f"{archive} 不是合法 ZIP：{error}") from None


def _target(root: Path, *parts: str) -> Path:
    target = (root.joinpath(*parts)).resolve()
    if not target.is_relative_to(root) or target == root:
        raise ChannelError(f"输出路径越界：{'/'.join(parts)}")
    return target


def _write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write(content)


def _write_binary(archive: Path, member: str, destination: Path) -> None:
    with zipfile.ZipFile(archive) as bundle:
        payload = bundle.read(member)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(payload)
    if os.name != "nt":
        destination.chmod(0o755)


def build(assets: Assets, zips: str | Path, output: str | Path) -> list[Path]:
    """生成 7 个可直接 npm publish 的目录。"""
    root = Path(output).resolve()
    archive_root = Path(zips).resolve()
    if not archive_root.is_dir():
        raise ChannelError(f"--zips 必须是已下载 Release ZIP 的目录：{archive_root}")

    # 先校验全部归档，再写任何文件：失败时不留半成品目录。
    plan = []
    for asset in assets.assets:
        archive = archive_root / asset.file
        verify_archive(archive, asset)
        plan.append((asset, archive, binary_member(archive, asset.binary)))

    written = []
    package_json = _target(root, MAIN_PACKAGE, "package.json")
    _write_text(package_json, _json(main_package(assets)))
    written.append(package_json)

    shim = _target(root, MAIN_PACKAGE, SHIM_PATH)
    _write_text(shim, shim_source(assets))
    if os.name != "nt":
        shim.chmod(0o755)
    written.append(shim)

    for asset, archive, member in plan:
        directory = package_name(asset.platform, asset.arch)
        package_path = _target(root, directory, "package.json")
        _write_text(package_path, _json(platform_package(assets, asset)))
        written.append(package_path)

        binary_path = _target(root, directory, "bin", asset.binary)
        _write_binary(archive, member, binary_path)
        written.append(binary_path)
    return written


def require_zips(zips: str | Path | None) -> Path:
    """平台包必须内含真实二进制，因此 --zips 虽为可选参数，缺失时仍然拒绝生成。"""
    if zips is None:
        raise ChannelError("生成 npm 平台包必须提供 --zips（已下载 Release ZIP 的目录）")
    directory = Path(zips)
    if not directory.is_dir():
        raise ChannelError(f"--zips 目录不存在：{directory}")
    return directory


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="生成 npm 主包与六个平台分包")
    parser.add_argument("--assets", required=True, type=Path, help="工作流准备的 assets.json")
    parser.add_argument("--zips", type=Path, default=None, help="已下载 Release ZIP 的目录")
    parser.add_argument("--output", required=True, type=Path, help="npm 包输出目录")
    arguments = parser.parse_args(argv)
    try:
        assets = load_assets(arguments.assets)
        written = build(assets, require_zips(arguments.zips), arguments.output)
    except ChannelError as error:
        print(f"npm 包生成失败：{error}", file=sys.stderr)
        return 1
    print(f"已生成 7 个 npm 包，共 {len(written)} 个文件，版本 {assets.version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
