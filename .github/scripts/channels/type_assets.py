"""渠道只消费正式 Release 的类型 tarball 与官方摘要，不读取或编译 frontend 源码。"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import type_package


DIGEST = re.compile(r"sha256:([0-9a-f]{64})\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")


def build(version, repository, tag, assets):
    name = type_package.archive_name(version)
    entries = {}
    for filename in (name, f"{name}.sha256"):
        matches = [item for item in assets if item.get("name") == filename and item.get("state") == "uploaded"]
        if len(matches) != 1 or matches[0].get("size", 0) <= 0:
            raise ValueError(f"正式 Release 必须包含唯一的非空类型资产 {filename}")
        match = DIGEST.fullmatch(matches[0].get("digest", "") or "")
        if not match:
            raise ValueError(f"{filename} 缺少 GitHub 官方 SHA256 digest")
        entries[filename] = match.group(1)
    return {
        "file": name, "sha256": entries[name], "checksumSha256": entries[f"{name}.sha256"],
        "url": f"https://github.com/{repository}/releases/download/{tag}/{name}",
    }


def load(manifest):
    version, repository = manifest["version"], manifest["repository"]
    name = type_package.archive_name(version)
    if (manifest.get("tag") != f"v{version}"
            or not isinstance(repository, str)
            or not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)):
        raise ValueError("类型资产必须属于相同仓库与 CLI 正式版本 Release")
    entry = manifest.get("types")
    if (not isinstance(entry, dict) or set(entry) != {"file", "sha256", "checksumSha256", "url"}
            or entry.get("file") != name
            or entry.get("url") != f"https://github.com/{repository}/releases/download/v{version}/{name}"):
        raise ValueError("assets.json 缺少同版本正式类型资产或下载地址不符")
    if any(not isinstance(entry[key], str) or not SHA256.fullmatch(entry[key])
           for key in ("sha256", "checksumSha256")):
        raise ValueError("类型 tarball 与侧车必须具有官方 SHA256")
    return entry


def verify(directory, manifest):
    entry = load(manifest)
    directory = Path(directory)
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("类型资产目录必须是普通目录")
    target = directory / entry["file"]
    sidecar = directory / f"{entry['file']}.sha256"
    for path, checksum in ((target, entry["sha256"]), (sidecar, entry["checksumSha256"])):
        if path.is_symlink() or not path.is_file() or type_package.digest(path) != checksum:
            raise ValueError(f"{path.name} 与 GitHub 官方 digest 不符，拒绝替换或发布")
    if type_package.read_checksum(sidecar, target.name) != entry["sha256"]:
        raise ValueError("类型 tarball 与 Release SHA256 侧车不一致")
    type_package.tarball_files(target, manifest["version"], manifest["repository"])
    return target


def download(directory, manifest, downloader):
    entry = load(manifest)
    # 六平台下载器同样拒绝覆盖损坏缓存；这里复用它的下载和官方摘要门禁。
    downloader(directory, {
        "repository": manifest["repository"], "tag": manifest["tag"],
        "assets": [
            {"file": entry["file"], "sha256": entry["sha256"], "url": entry["url"]},
            {"file": f"{entry['file']}.sha256", "sha256": entry["checksumSha256"],
             "url": f"{entry['url']}.sha256"},
        ],
    })
    return verify(directory, manifest)
