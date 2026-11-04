"""kenvoy 单包发布：默认只读探测，显式授权且位于 main 才上传。"""

import argparse
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from http.client import HTTPException
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import Request, urlopen

from manifests import ARCHIVE, REPOSITORY, SHA256, VERSION, ChannelError
from npm_packages import (
    DOWNLOAD_TARGETS,
    PACKAGE_FILES,
    PACKAGE_NAME,
    RUNNER_PATH,
    runtime_dependencies,
    runtime_files,
    valid_member,
)

REGISTRY = "https://registry.npmjs.org"


@dataclass(frozen=True)
class Package:
    name: str
    version: str
    directory: Path

    @property
    def spec(self) -> str:
        return f"{self.name}@{self.version}"


def _read_json(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ChannelError("npm 包缺少非空的普通 JSON 文件")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeError, json.JSONDecodeError):
        raise ChannelError("npm 包清单不是合法 UTF-8 JSON") from None
    if not isinstance(value, dict):
        raise ChannelError("npm 包清单必须是 JSON 对象")
    return value


def _validate_downloads(data: dict, version: str) -> str:
    if set(data) != {"formatVersion", "version", "repository", "targets"}:
        raise ChannelError("下载清单字段与 formatVersion=1 契约不符")
    if type(data.get("formatVersion")) is not int or data["formatVersion"] != 1:
        raise ChannelError("下载清单 formatVersion 必须是 1")
    if data.get("version") != version:
        raise ChannelError("下载清单与 npm 包的 version 必须一致")
    repository = data.get("repository")
    if not isinstance(repository, str) or not REPOSITORY.fullmatch(repository):
        raise ChannelError("下载清单 repository 必须是 GitHub owner/name")
    targets = data.get("targets")
    if not isinstance(targets, dict) or set(targets) != set(DOWNLOAD_TARGETS):
        raise ChannelError("下载清单必须包含全部六个受支持的系统架构组合")
    urls = set()
    fields = {"url", "archiveSha256", "archiveSize", "member", "binarySha256", "binarySize", "executable"}
    prefix = f"https://github.com/{repository}/releases/download/v{version}/"
    for key, executable in DOWNLOAD_TARGETS.items():
        target = targets[key]
        if not isinstance(target, dict) or set(target) != fields:
            raise ChannelError(f"下载清单 {key} 字段不完整或含未支持字段")
        for field in ("archiveSha256", "binarySha256"):
            value = target[field]
            if not isinstance(value, str) or not SHA256.fullmatch(value):
                raise ChannelError(f"下载清单 {key} 的摘要必须是小写 SHA-256")
        for field in ("archiveSize", "binarySize"):
            value = target[field]
            if type(value) is not int or not 0 < value <= 2**53 - 1:
                raise ChannelError(f"下载清单 {key} 的长度必须是正安全整数")
        member = target["member"]
        if (target["executable"] != executable or not valid_member(member)
                or member.rsplit("/", 1)[-1] != executable):
            raise ChannelError(f"下载清单 {key} 的程序名或 ZIP 成员路径非法")
        url = target["url"]
        if not isinstance(url, str) or not url.startswith(prefix):
            raise ChannelError(f"下载清单 {key} 必须指向相同仓库和版本的 GitHub Release")
        filename = url[len(prefix):]
        if (not ARCHIVE.fullmatch(filename) or not filename.startswith(f"kenvoy-{version}-")
                or url in urls):
            raise ChannelError(f"下载清单 {key} 的 Release ZIP 地址非法或重复")
        parsed = urlsplit(url)
        if parsed.netloc != "github.com" or parsed.query or parsed.fragment:
            raise ChannelError(f"下载清单 {key} 的 Release ZIP 地址非法")
        urls.add(url)
    return repository


def _validate_files(directory: Path) -> None:
    expected = set(runtime_files()) | {"package.json", "download-manifest.json", "LICENSE", "README.md"}
    found = set()
    for entry in directory.rglob("*"):
        if entry.is_symlink() or not entry.resolve().is_relative_to(directory):
            raise ChannelError("npm 发布包不能包含符号链接或越界路径")
        relative = entry.relative_to(directory).as_posix()
        if entry.is_dir():
            if relative not in ("bin", "lib"):
                raise ChannelError("npm 发布包只能包含 bin 与 lib 目录，不得内嵌二进制或依赖目录")
            continue
        if not entry.is_file() or entry.stat().st_size == 0:
            raise ChannelError("npm 安装入口、共享模块、清单和许可证必须是非空普通文件")
        found.add(relative)
    if found != expected:
        raise ChannelError("npm 发布包缺少运行时文件、LICENSE、README，或混入了额外文件")


def load_package(path: str | Path) -> Package:
    """只接受生成器的单包安装契约；不执行任何 npm 或安装脚本。"""
    try:
        directory = Path(path)
        if directory.is_symlink() or not directory.is_dir():
            raise ChannelError("--package 必须是 kenvoy 的普通包目录")
        directory = directory.resolve()
        data = _read_json(directory / "package.json")
        if data.get("name") != PACKAGE_NAME:
            raise ChannelError("--package 的 package.json 必须声明 name=kenvoy")
        version = data.get("version")
        if not isinstance(version, str) or not VERSION.fullmatch(version):
            raise ChannelError("kenvoy 的 version 必须是 X.Y.Z 正式版本")
        if (data.get("bin") != {"kenvoy": RUNNER_PATH} or data.get("files") != PACKAGE_FILES
                or data.get("scripts") != {"postinstall": "node install.cjs"}
                or data.get("engines") != {"node": ">=18"}):
            raise ChannelError("kenvoy 的安装入口、运行入口或文件声明与单包契约不符")
        forbidden = {"optionalDependencies", "devDependencies", "peerDependencies",
                     "bundledDependencies", "bundleDependencies"}
        if forbidden.intersection(data) or data.get("dependencies") != runtime_dependencies():
            raise ChannelError("kenvoy 只能声明下载运行时的精确依赖，不能引用平台分包")
        if ("private" in data and data["private"] is not False) or data.get("license") != "MIT":
            raise ChannelError("kenvoy 必须是可发布的 MIT 软件包")
        if data.get("publishConfig") not in (None, {}):
            raise ChannelError("kenvoy 不允许通过 publishConfig 覆盖发布配置")
        for field, expected in (("os", ["win32", "linux", "darwin"]), ("cpu", ["x64", "arm64"])):
            if field in data and data[field] != expected:
                raise ChannelError("kenvoy 的 os/cpu 必须覆盖全部受支持平台")
        repository = _validate_downloads(_read_json(directory / "download-manifest.json"), version)
        expected_repository = {"type": "git", "url": f"git+https://github.com/{repository}.git"}
        if data.get("repository") != expected_repository:
            raise ChannelError("kenvoy 的 repository 必须与下载清单一致")
        _validate_files(directory)
        return Package(PACKAGE_NAME, version, directory)
    except (OSError, ValueError):
        raise ChannelError("无法读取 npm 包目录或文件；尚未发布任何包") from None


def version_exists(package: Package) -> bool:
    """公开包只做匿名 GET；只有明确 HTTP 404 才表示该版本不存在。"""
    request = Request(
        f"{REGISTRY}/{package.name}/{package.version}",
        headers={"Accept": "application/json", "User-Agent": "kenvoy-channel-publisher"},
    )
    try:
        with urlopen(request, timeout=30) as response:
            if response.status != 200:
                raise ChannelError(f"{package.spec} 查询未返回 HTTP 200；停止发布")
            data = json.loads(response.read().decode("utf-8"))
    except HTTPError as error:
        status = error.code
        error.close()
        if status == 404:
            return False
        raise ChannelError(f"{package.spec} 查询失败（HTTP {status}）；停止发布") from None
    except (URLError, OSError, HTTPException):
        raise ChannelError(f"{package.spec} 查询发生网络错误；停止发布") from None
    except (UnicodeError, json.JSONDecodeError):
        raise ChannelError(f"{package.spec} 查询响应不是合法 UTF-8 JSON；停止发布") from None
    if not isinstance(data, dict) or data.get("name") != package.name or data.get("version") != package.version:
        raise ChannelError(f"{package.spec} 查询响应的包名或版本不符；停止发布")
    return True


def npm_executable() -> str:
    executable = shutil.which("npm.cmd" if os.name == "nt" else "npm")
    if executable is None:
        raise ChannelError("缺少 npm，无法发布；请安装 Node.js 配套 npm")
    return executable


def publish_package(package: Package, executable: str) -> None:
    # 认证交给 npm 的 OIDC/令牌机制；不读取、打印或自行转发凭据。
    try:
        result = subprocess.run(
            [executable, "publish", "--access", "public", "--provenance", "--ignore-scripts",
             "--registry", REGISTRY],
            cwd=package.directory,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except OSError:
        raise ChannelError(f"{package.spec} 无法启动 npm；已停止，未重试") from None
    if result.returncode:
        raise ChannelError(
            f"{package.spec} 发布失败（npm 退出码 {result.returncode}）；已停止，未重试"
        )


def run(path: str | Path, publish: bool = False) -> None:
    if publish and os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ChannelError("实际 npm 发布仅允许 GITHUB_REF=refs/heads/main")
    package = load_package(path)
    if version_exists(package):
        print(f"{package.spec} 已存在，跳过")
    elif not publish:
        print(f"{package.spec} 计划发布（只读，未上传）")
    else:
        publish_package(package, npm_executable())
        print(f"{package.spec} 已发布")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="只读探测 kenvoy npm 包；显式 --publish 才上传")
    parser.add_argument("--package", required=True, type=Path, help="npm_packages.py 生成的 kenvoy 目录")
    parser.add_argument("--publish", action="store_true", help="仅允许 main：发布缺失版本，已有版本跳过")
    arguments = parser.parse_args(argv)
    try:
        run(arguments.package, arguments.publish)
    except ChannelError as error:
        print(f"npm 发布失败：{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
