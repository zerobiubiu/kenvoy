"""正式类型资产的纯声明契约；只读取同版源码包和 npm tarball，不执行其中脚本。"""

import hashlib
import json
import re
from pathlib import Path

import registry

PACKAGE_NAME = "@kenvoy/types"
PACKAGE_DIRECTORY = "frontend/dist/types-package"
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")
EXPORTS = {
    ".": {"types": "./index.d.ts"},
    "./sdk": {"types": "./sdk/sdk.d.ts"},
    "./jsx-runtime": {"types": "./sdk/presentation/jsx-runtime.d.ts"},
    "./jsx-dev-runtime": {"types": "./sdk/presentation/jsx-dev-runtime.d.ts"},
}
PACKAGE_FILES = ["index.d.ts", "sdk/**/*.d.ts", "README.md", "LICENSE"]


def archive_name(version):
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        raise ValueError("类型包版本必须是 CLI 的正式 X.Y.Z")
    return f"kenvoy-types-{version}.tgz"


def validate_files(files, version, repository):
    """目录和 tarball 共用清单、身份及文件门禁，不放宽安装器的独立契约。"""
    archive_name(version)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("类型包仓库标识无效")
    required = {"package.json", "README.md", "LICENSE"}
    required.update(item["types"].removeprefix("./") for item in EXPORTS.values())
    if not required.issubset(files):
        raise ValueError("类型包缺少清单、完整 SDK/JSX 声明入口、README 或 LICENSE")
    for name, content in files.items():
        if (name not in required and not (name.startswith("sdk/") and name.endswith(".d.ts"))) or not content:
            raise ValueError("类型包只能包含非空的声明、清单、README 与 LICENSE，不能携带运行代码或配置")
    try:
        data = json.loads(files["package.json"].decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError):
        raise ValueError("类型包 package.json 必须是合法 UTF-8 JSON") from None
    if not isinstance(data, dict) or data.get("name") != PACKAGE_NAME or data.get("version") != version:
        raise ValueError(f"类型包必须声明 {PACKAGE_NAME}@{version}，不能使用其它 CLI 版本")
    if (data.get("type") != "module" or data.get("license") != "MIT"
            or data.get("types") != "./index.d.ts" or data.get("exports") != EXPORTS
            or data.get("files") != PACKAGE_FILES):
        raise ValueError("类型包的纯声明入口、exports、files 或许可证与发布契约不符")
    forbidden = {"scripts", "bin", "main", "module", "browser", "dependencies", "devDependencies",
                 "optionalDependencies", "bundledDependencies", "bundleDependencies"}
    if forbidden.intersection(data) or ("private" in data and data["private"] is not False):
        raise ValueError("类型包不得声明脚本、运行入口、运行依赖或 private=true")
    if data.get("publishConfig") != {"access": "public"}:
        raise ValueError("类型包 publishConfig 仅允许 access=public，不能覆盖 registry")
    peer = data.get("peerDependencies")
    if (not isinstance(peer, dict) or set(peer) != {"@types/react"}
            or not isinstance(peer["@types/react"], str)
            or not re.fullmatch(r"\^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", peer["@types/react"])):
        raise ValueError("类型包仅允许由前端版本生成的 @types/react 同 major peerDependency")
    url = f"https://github.com/{repository}"
    allowed_repository = {"type": "git", "url": f"git+{url}.git"}
    if data.get("repository") not in (url, allowed_repository):
        raise ValueError("类型包 repository 必须对应发布所在的同一 GitHub 仓库")
    return data


def directory_files(directory, version, repository):
    directory = Path(directory)
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("已发布 CLI 源码包缺少 frontend/dist/types-package；禁止从当前 HEAD 重建冒充旧版类型")
    files = {}
    for path in directory.rglob("*"):
        if path.is_symlink() or not path.resolve().is_relative_to(directory.resolve()):
            raise ValueError("类型包目录不得含符号链接或越界路径")
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError("类型包目录只能包含普通文件")
        files[path.relative_to(directory).as_posix()] = path.read_bytes()
    validate_files(files, version, repository)
    return files


def crate_files(payload, version, repository):
    files = registry.archive_files(payload, f"kenvoy-{version}", PACKAGE_DIRECTORY)
    if "package.json" not in files:
        raise ValueError("已发布 CLI 源码包缺少 frontend/dist/types-package；禁止从当前 HEAD 重建冒充旧版类型")
    validate_files(files, version, repository)
    return files


def tarball_files(path, version, repository):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError("类型资产必须是普通 npm tarball")
    files = registry.archive_files(path.read_bytes(), "package", "")
    validate_files(files, version, repository)
    return files


def require_same_files(actual, expected):
    # npm pack 的清单格式化不影响声明内容；其余所有文件必须逐字节相同。
    if (set(actual) != set(expected)
            or any(actual[name] != expected[name] for name in expected if name != "package.json")
            or json.loads(actual["package.json"]) != json.loads(expected["package.json"])):
        raise ValueError("类型资产与同版本已发布 CLI 源码包不同，拒绝覆盖或以新源码补造")


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read_checksum(path, name):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError("类型资产缺少普通 SHA256 侧车文件")
    parts = path.read_text(encoding="ascii").split()
    if len(parts) != 2 or parts[1] != name or not re.fullmatch(r"[0-9a-f]{64}", parts[0]):
        raise ValueError("类型资产 SHA256 侧车必须包含小写摘要与精确 tarball 文件名")
    return parts[0]
