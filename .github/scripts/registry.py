"""只使用 crates.io 官方精确版本与 checksum 的无令牌源码读取。"""

import hashlib
import io
import json
from pathlib import Path, PureWindowsPath
import re
import shutil
import tarfile
import tomllib
import urllib.error
import urllib.request


_USER_AGENT = "kenvoy-distribution (https://github.com/zerobiubiu/kenvoy)"
_VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)")


def _identity(crate: str, version: str) -> str:
    if not isinstance(crate, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", crate):
        raise ValueError("crate 名称无效")
    if not isinstance(version, str) or not _VERSION.fullmatch(version):
        raise ValueError("crate 版本必须是无前缀的 major.minor.patch 正式版本")
    return f"{crate}-{version}"


def _metadata(metadata: dict, crate: str, version: str) -> None:
    if not isinstance(metadata, dict):
        raise ValueError("crates.io 精确版本响应不是对象")
    if metadata.get("crate") != crate or metadata.get("num") != version:
        raise ValueError(f"crates.io 响应与请求的 {crate} {version} 不一致")


def _download(url: str, accept: str) -> bytes:
    request = urllib.request.Request(
        url, headers={"User-Agent": _USER_AGENT, "Accept": accept}
    )
    # 不读取任何 token 环境变量，也不转发 GitHub 或 Cargo 凭据。
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def registry_version(crate: str, version: str) -> dict | None:
    """查询唯一精确版本；只有官方接口的 HTTP 404 表示未发布。"""
    _identity(crate, version)
    try:
        payload = _download(
            f"https://crates.io/api/v1/crates/{crate}/{version}", "application/json"
        )
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    document = json.loads(payload)
    metadata = document.get("version") if isinstance(document, dict) else None
    _metadata(metadata, crate, version)
    return metadata


def download_crate(crate: str, version: str, metadata: dict) -> bytes:
    """从官方静态地址下载精确 .crate，必须与该版本官方 checksum 一致。"""
    prefix = _identity(crate, version)
    _metadata(metadata, crate, version)
    checksum = metadata.get("checksum")
    if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", checksum):
        raise ValueError(f"crates.io 未提供 {crate} {version} 的有效 SHA256 checksum")
    payload = _download(
        f"https://static.crates.io/crates/{crate}/{prefix}.crate", "application/octet-stream"
    )
    if hashlib.sha256(payload).hexdigest() != checksum.lower():
        raise ValueError(f"{crate} {version} 源码包与 crates.io 官方 checksum 不一致")
    return payload


def _members(archive: tarfile.TarFile, prefix: str) -> dict[tuple[str, ...], tarfile.TarInfo]:
    members = {}
    portable_names = set()
    for member in archive.getmembers():
        if member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE):
            raise ValueError(f"源码包包含链接或特殊条目：{member.name}")
        name = member.name.removesuffix("/") if member.isdir() else member.name
        parts = tuple(name.split("/"))
        if (
            not parts
            or parts[0] != prefix
            or (len(parts) == 1 and not member.isdir())
            or any(
                part in ("", ".", "..")
                or "\\" in part
                or ":" in part
                or part.endswith((" ", "."))
                or any(ord(character) < 32 or ord(character) == 127 for character in part)
                or PureWindowsPath(part).is_reserved()
                for part in parts
            )
        ):
            raise ValueError(f"源码包路径不安全或不属于 {prefix}：{member.name}")
        portable = tuple(part.casefold() for part in parts)
        if portable in portable_names or member.size < 0:
            raise ValueError(f"源码包包含重复路径或非法大小：{member.name}")
        portable_names.add(portable)
        members[parts] = member
    for parts in members:
        for length in range(1, len(parts)):
            parent = members.get(parts[:length])
            if parent is not None and not parent.isdir():
                raise ValueError(f"源码包文件不能作为父目录：{parent.name}")
    return members


def _manifest(
    archive: tarfile.TarFile,
    members: dict[tuple[str, ...], tarfile.TarInfo],
    crate: str,
    version: str,
) -> dict:
    member = members.get((f"{crate}-{version}", "Cargo.toml"))
    if member is None or not member.isfile():
        raise ValueError("源码包缺少根 Cargo.toml 普通文件")
    source = archive.extractfile(member)
    if source is None:
        raise ValueError("无法读取源码包的 Cargo.toml")
    with source:
        manifest = tomllib.loads(source.read().decode("utf-8"))
    package = manifest.get("package")
    if not isinstance(package, dict) or package.get("name") != crate or package.get("version") != version:
        raise ValueError(f"源码包 Cargo.toml 与 {crate} {version} 不一致")
    return manifest


def package_manifest(payload: bytes, crate: str, version: str) -> dict:
    """读取已校验字节中的规范化 manifest，同时拒绝不安全 tar 条目。"""
    prefix = _identity(crate, version)
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        return _manifest(archive, _members(archive, prefix), crate, version)


def extract_crate(payload: bytes, crate: str, version: str, output: Path) -> Path:
    """在 output 下独占创建 crate-version，不覆盖、不提取链接或越界路径。"""
    prefix = _identity(crate, version)
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        members = _members(archive, prefix)
        _manifest(archive, members, crate, version)
        output = Path(output).resolve()
        output.mkdir(parents=True, exist_ok=True)
        root = output / prefix
        root.mkdir()
        try:
            for parts, member in members.items():
                destination = root.joinpath(*parts[1:])
                if member.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                    continue
                destination.parent.mkdir(parents=True, exist_ok=True)
                source = archive.extractfile(member)
                if source is None:
                    raise ValueError(f"无法读取源码包文件：{member.name}")
                with source, destination.open("xb") as target:
                    shutil.copyfileobj(source, target)
                destination.chmod(0o755 if member.mode & 0o111 else 0o644)
        except BaseException:
            shutil.rmtree(root)
            raise
    return root
