"""从已校验的 Release ZIP 生成单个 npm 包及按平台下载清单。"""

import argparse
import hashlib
import json
import os
import shutil
import stat
import sys
import zipfile
import zlib
from pathlib import Path

from manifests import BINARIES, COMBINATIONS, VERSION, Asset, Assets, ChannelError, load_assets

PACKAGE_NAME = "kenvoy"
RUNNER_PATH = "bin/kenvoy.cjs"
PACKAGE_FILES = ["bin", "lib", "install.cjs", "download-manifest.json"]
RUNTIME_ROOT = Path(__file__).resolve().with_name("npm_runtime")
RUNTIME_DEPENDENCIES = frozenset(("fflate", "https-proxy-agent", "proxy-from-env"))
_NPM_OS = {"windows": "win32", "linux": "linux", "macos": "darwin"}
_NPM_CPU = {"x86_64": "x64", "ARM64": "arm64"}
DOWNLOAD_TARGETS = {
    f"{_NPM_OS[platform]}-{_NPM_CPU[arch]}": BINARIES[platform]
    for platform, arch in COMBINATIONS
}
INSTALL_README = """# Kenvoy CLI

需要 Node.js 18 或更高版本：

```text
npm install -g kenvoy
kenvoy --version
```

npm 12 默认阻止依赖安装脚本，安装本包时请明确授权：

```text
npm install -g kenvoy --allow-scripts=kenvoy
```

不要全局放行所有依赖脚本；该参数只允许 Kenvoy 的安装器。

安装脚本按当前系统与架构选择 Windows、Linux、macOS 的 x64 或 ARM64
官方 Release ZIP，只下载当前平台；校验归档与程序的大小和 SHA-256 后安装。
包内不携带平台二进制，也不依赖其它 Kenvoy npm 分包。

代理沿用 npm 代理配置以及 HTTP_PROXY、HTTPS_PROXY、ALL_PROXY、NO_PROXY
环境变量；保持 HTTPS 证书校验，不需要另装解压工具或 Rust。

若安装时使用了 --ignore-scripts，请执行 `npm rebuild kenvoy`；
全局安装使用 `npm rebuild -g kenvoy`。npm 12 的局部项目须先执行
`npm install-scripts approve kenvoy`；全局重建可执行
`npm rebuild -g kenvoy --allow-scripts=kenvoy`。运行命令不会绕过脚本审批自行联网补装。
"""


def runtime_dependencies() -> dict[str, str]:
    """依赖版本只读取工程运行时声明，不在生成器维护第二份版本表。"""
    try:
        source = RUNTIME_ROOT / "package.json"
        if source.is_symlink() or not source.is_file():
            raise ChannelError("npm_runtime/package.json 必须是普通文件")
        data = json.loads(source.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        raise ChannelError("无法读取 npm_runtime/package.json 的 UTF-8 JSON") from None
    if not isinstance(data, dict) or data.get("private") is not True:
        raise ChannelError("npm_runtime 必须声明 private=true")
    if data.get("engines") != {"node": ">=18"}:
        raise ChannelError("npm_runtime 必须声明 Node.js >=18")
    dependencies = data.get("dependencies")
    if not isinstance(dependencies, dict) or set(dependencies) != RUNTIME_DEPENDENCIES:
        raise ChannelError("npm_runtime 的 dependencies 与支持的下载运行时不符")
    if any(not isinstance(value, str) or not VERSION.fullmatch(value)
           for value in dependencies.values()):
        raise ChannelError("npm_runtime 的依赖必须锁定精确正式版本")
    return dependencies


def runtime_files() -> dict[str, Path]:
    """将源运行时映射到 npm 的安装、命令和共享模块布局。"""
    library = RUNTIME_ROOT / "lib"
    if RUNTIME_ROOT.is_symlink() or library.is_symlink() or not library.is_dir():
        raise ChannelError("npm_runtime/lib 必须是普通目录")
    modules = sorted(library.glob("*.cjs"))
    if not modules:
        raise ChannelError("npm_runtime/lib 缺少共享模块")
    files = {"install.cjs": RUNTIME_ROOT / "install.cjs", RUNNER_PATH: RUNTIME_ROOT / "run.cjs"}
    files.update((f"lib/{source.name}", source) for source in modules)
    for source in files.values():
        if source.is_symlink() or not source.is_file() or source.stat().st_size == 0:
            raise ChannelError("npm_runtime 的安装入口、命令入口与共享模块必须是非空普通文件")
    return files


def package_manifest(assets: Assets, dependencies: dict[str, str]) -> dict:
    return {
        "name": PACKAGE_NAME,
        "version": assets.version,
        "description": "Kenvoy CLI 官方二进制安装器",
        "homepage": assets.homepage,
        "repository": {"type": "git", "url": f"git+{assets.homepage}.git"},
        "bugs": {"url": f"{assets.homepage}/issues"},
        "license": "MIT",
        "bin": {"kenvoy": RUNNER_PATH},
        "scripts": {"postinstall": "node install.cjs"},
        "files": PACKAGE_FILES,
        "engines": {"node": ">=18"},
        "dependencies": dependencies,
    }


def verify_archive(archive: Path, asset: Asset) -> None:
    """生成前核对官方摘要，不覆盖或重新下载损坏的缓存。"""
    if not archive.is_file():
        raise ChannelError(f"缺少资产归档：{archive}（{asset.combination} 需要 {asset.file}）")
    with archive.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != asset.sha256:
        raise ChannelError(
            f"{asset.file} 的 sha256 与 assets.json 不一致：期望 {asset.sha256}，实际 {actual}"
        )


def valid_member(name: object) -> bool:
    """归档成员只能使用无穿越、设备路径或歧义片段的相对 POSIX 路径。"""
    return (
        isinstance(name, str)
        and bool(name)
        and not any(ord(character) < 32 or character in "\\:" for character in name)
        and all(part not in ("", ".", "..") for part in name.split("/"))
    )


def archive_member(archive: Path, binary: str) -> str:
    """唯一定位普通交付文件；拒绝歧义、空文件和任意危险路径。"""
    try:
        with zipfile.ZipFile(archive) as bundle:
            matches = []
            for info in bundle.infolist():
                name = info.orig_filename
                if not valid_member(name[:-1] if info.is_dir() else name):
                    raise ChannelError(f"{archive.name} 含可疑路径条目")
                mode = stat.S_IFMT(info.external_attr >> 16)
                if mode not in (0, stat.S_IFDIR if info.is_dir() else stat.S_IFREG):
                    raise ChannelError(f"{archive.name} 含非普通文件条目")
                if info.flag_bits & 1 or info.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                    raise ChannelError(f"{archive.name} 含加密或不受支持的压缩条目")
                if not info.is_dir() and name.rsplit("/", 1)[-1] == binary:
                    matches.append(info)
            if not matches:
                raise ChannelError(f"{archive.name} 内缺少交付文件 {binary}")
            if len(matches) != 1:
                raise ChannelError(f"{archive.name} 内含多个 {binary}")
            if matches[0].file_size == 0:
                raise ChannelError(f"{archive.name} 内的 {binary} 是空文件")
            return matches[0].filename
    except FileNotFoundError:
        raise ChannelError(f"缺少资产归档：{archive}") from None
    except zipfile.BadZipFile:
        raise ChannelError(f"{archive.name} 不是合法 ZIP") from None


def _downloads(assets: Assets, archive_root: Path) -> tuple[dict, bytes]:
    targets = {}
    license_payload = None
    for asset in assets.assets:
        archive = archive_root / asset.file
        verify_archive(archive, asset)
        member = archive_member(archive, asset.binary)
        license_member = archive_member(archive, "LICENSE")
        with zipfile.ZipFile(archive) as bundle:
            digest = hashlib.sha256()
            size = 0
            with bundle.open(member) as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
                    size += len(chunk)
            if size != bundle.getinfo(member).file_size:
                raise ChannelError(f"{archive.name} 的程序长度与 ZIP 元数据不符")
            original_license = bundle.read(license_member)
            if license_payload is None:
                license_payload = original_license
        key = f"{_NPM_OS[asset.platform]}-{_NPM_CPU[asset.arch]}"
        targets[key] = {
            "url": asset.url,
            "archiveSha256": asset.sha256,
            "archiveSize": archive.stat().st_size,
            "member": member,
            "binarySha256": digest.hexdigest(),
            "binarySize": size,
            "executable": asset.binary,
        }
    return {
        "formatVersion": 1,
        "version": assets.version,
        "repository": assets.repository,
        "targets": targets,
    }, license_payload


def _write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write(content)


def build(assets: Assets, zips: str | Path, output: str | Path) -> list[Path]:
    """只生成 output/kenvoy；所有输入校验通过前不写输出。"""
    try:
        root = Path(output)
        if root.is_symlink() or (root.exists() and (not root.is_dir() or any(root.iterdir()))):
            raise ChannelError("npm 输出目录必须不存在或为空，拒绝混入旧包与其它文件")
        archive_root = Path(zips).resolve()
        if not archive_root.is_dir():
            raise ChannelError("--zips 必须是已下载 Release ZIP 的目录")
        dependencies = runtime_dependencies()
        sources = runtime_files()
        downloads, license_payload = _downloads(assets, archive_root)
        package = root.resolve() / PACKAGE_NAME
        contents = {
            "package.json": package_manifest(assets, dependencies),
            "download-manifest.json": downloads,
        }
        written = []
        for name, data in contents.items():
            destination = package / name
            _write_text(destination, json.dumps(data, ensure_ascii=False, indent=2) + "\n")
            written.append(destination)
        for name, source in sources.items():
            destination = package / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            if os.name != "nt":
                destination.chmod(0o755 if name == RUNNER_PATH else 0o644)
            written.append(destination)
        license_path = package / "LICENSE"
        license_path.write_bytes(license_payload)
        readme = package / "README.md"
        _write_text(readme, INSTALL_README)
        return written + [license_path, readme]
    except (OSError, EOFError, zipfile.BadZipFile, zlib.error, NotImplementedError):
        raise ChannelError("无法读取发行归档、运行时或写入 npm 包；停止生成") from None


def require_zips(zips: str | Path | None) -> Path:
    if zips is None:
        raise ChannelError("生成 npm 下载清单必须提供 --zips（已下载 Release ZIP 的目录）")
    directory = Path(zips)
    if not directory.is_dir():
        raise ChannelError(f"--zips 目录不存在：{directory}")
    return directory


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="生成按平台下载官方二进制的 kenvoy npm 包")
    parser.add_argument("--assets", required=True, type=Path, help="工作流准备的 assets.json")
    parser.add_argument("--zips", type=Path, help="已下载 Release ZIP 的目录")
    parser.add_argument("--output", required=True, type=Path, help="仅含 kenvoy 的输出目录")
    arguments = parser.parse_args(argv)
    try:
        assets = load_assets(arguments.assets)
        build(assets, require_zips(arguments.zips), arguments.output)
    except ChannelError as error:
        print(f"npm 包生成失败：{error}", file=sys.stderr)
        return 1
    print(f"已生成 kenvoy@{assets.version} npm 下载包")
    return 0


if __name__ == "__main__":
    sys.exit(main())
