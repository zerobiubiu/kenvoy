"""npm 七包发布：默认只读探测，全部检查通过后才按平台包、主包顺序上传。"""

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
from urllib.request import Request, urlopen

from manifests import BINARIES, COMBINATIONS, VERSION, ChannelError
from npm_packages import MAIN_PACKAGE, SHIM_PATH, npm_cpu, npm_os, package_name

REGISTRY = "https://registry.npmjs.org"
PLATFORM_METADATA = {
    package_name(platform, arch): (npm_os(platform), npm_cpu(arch), f"bin/{BINARIES[platform]}")
    for platform, arch in COMBINATIONS
}
PACKAGE_NAMES = (*PLATFORM_METADATA, MAIN_PACKAGE)


@dataclass(frozen=True)
class Package:
    name: str
    version: str
    directory: Path

    @property
    def spec(self) -> str:
        return f"{self.name}@{self.version}"


def load_packages(path: str | Path) -> tuple[Package, ...]:
    """校验生成器的七包契约；目录名不能替代 package.json 中的实际元数据。"""
    try:
        root = Path(path).resolve()
        if not root.is_dir() or {entry.name for entry in root.iterdir()} != set(PACKAGE_NAMES):
            raise ChannelError("--packages 必须恰好包含 kenvoy 主包与六个平台包目录")
        packages = []
        version = None
        for name in PACKAGE_NAMES:
            directory = root / name
            manifest = directory / "package.json"
            if directory.is_symlink() or not directory.is_dir() or manifest.is_symlink():
                raise ChannelError(f"{name} 必须是普通包目录，package.json 不能是符号链接")
            try:
                data = json.loads(manifest.read_text(encoding="utf-8"))
            except (UnicodeError, json.JSONDecodeError):
                raise ChannelError(f"{name}/package.json 不是合法 UTF-8 JSON") from None
            if not isinstance(data, dict) or data.get("name") != name:
                raise ChannelError(f"{name}/package.json 的 name 与指定包不符")
            current = data.get("version")
            if not isinstance(current, str) or not VERSION.fullmatch(current):
                raise ChannelError(f"{name} 的 version 必须是 X.Y.Z 正式版本")
            if version is not None and current != version:
                raise ChannelError("七个 npm 包的 version 必须完全一致")
            version = current
            if name == MAIN_PACKAGE:
                binary = SHIM_PATH
                expected = {dependency: version for dependency in PLATFORM_METADATA}
                if data.get("optionalDependencies") != expected:
                    raise ChannelError("kenvoy 必须通过 optionalDependencies 精确依赖同版本六个平台包")
            else:
                platform, arch, binary = PLATFORM_METADATA[name]
                if data.get("os") != [platform] or data.get("cpu") != [arch]:
                    raise ChannelError(f"{name} 的 os/cpu 与平台包名不符")
            if data.get("bin") != {"kenvoy": binary} or data.get("files") != [binary]:
                raise ChannelError(f"{name} 的 bin/files 与生成器约定不符")
            payload = directory / binary
            if payload.is_symlink() or not payload.is_file() or payload.stat().st_size == 0:
                raise ChannelError(f"{name} 缺少非空的 {binary} 普通文件")
            license_path = directory / "LICENSE"
            if license_path.is_symlink() or not license_path.is_file() or license_path.stat().st_size == 0:
                raise ChannelError(f"{name} 缺少发行归档携带的 LICENSE")
            if data.get("publishConfig") not in (None, {}):
                raise ChannelError(f"{name} 不允许通过 publishConfig 覆盖发布配置")
            packages.append(Package(name, version, directory))
        return tuple(packages)
    except OSError:
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
    # 沿用工程命令的 Windows npm.cmd 入口；参数始终通过数组传递。
    executable = shutil.which("npm.cmd" if os.name == "nt" else "npm")
    if executable is None:
        raise ChannelError("缺少 npm，无法发布；请安装 Node.js 配套 npm")
    return executable


def publish_package(package: Package, executable: str) -> None:
    # 认证交给 npm：保留 OIDC/可信发布与 NODE_AUTH_TOKEN 的官方选择逻辑，不预设令牌门禁。
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
    # npm 原始输出可能包含认证上下文；只报告经过验证的包名、版本和退出码。
    if result.returncode:
        raise ChannelError(
            f"{package.spec} 发布失败（npm 退出码 {result.returncode}）；已停止，未重试"
        )


def run(path: str | Path, publish: bool = False) -> None:
    if publish and os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ChannelError("实际 npm 发布仅允许 GITHUB_REF=refs/heads/main")
    packages = load_packages(path)
    # 先探测全部七包，防止后续包的鉴权或网络错误发生前已经上传了前面的包。
    existing = [version_exists(package) for package in packages]
    executable = npm_executable() if publish and not all(existing) else None
    for package, exists in zip(packages, existing):
        if exists:
            print(f"{package.spec} 已存在，跳过")
        elif not publish:
            print(f"{package.spec} 计划发布（只读，未上传）")
        else:
            publish_package(package, executable)
            print(f"{package.spec} 已发布")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="只读探测 npm 七包；显式 --publish 才执行上传")
    parser.add_argument("--packages", required=True, type=Path, help="npm_packages.py 生成的七包根目录")
    parser.add_argument("--publish", action="store_true", help="仅允许 main：发布缺失版本，已有版本跳过")
    arguments = parser.parse_args(argv)
    try:
        run(arguments.packages, arguments.publish)
    except ChannelError as error:
        print(f"npm 发布失败：{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
