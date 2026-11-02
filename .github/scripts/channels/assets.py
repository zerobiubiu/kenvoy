"""从已发布 Release 生成渠道发布所需的 assets.json，并校验六个归档。"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

PLATFORMS = {"windows", "linux", "macos"}
ARCHES = {"x86_64", "ARM64"}
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
DIGEST = re.compile(r"sha256:([0-9a-f]{64})\Z")


class AssetError(RuntimeError):
    """资产清单不完整或与 Release 不一致。"""


def release_assets(repository, tag, token=None):
    command = ["gh", "api", f"repos/{repository}/releases/tags/{tag}"]
    environment = dict(os.environ)
    if token:
        environment["GH_TOKEN"] = token
    result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", env=environment)
    if result.returncode:
        raise AssetError(f"读取 Release {tag} 失败：{result.stderr.strip()}")
    return json.loads(result.stdout).get("assets", [])


def build(version, repository, tag, assets):
    if not VERSION.fullmatch(version):
        raise AssetError(f"版本必须是正式 X.Y.Z，实际 {version}")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise AssetError("仓库标识无效")
    uploaded = {item["name"]: item for item in assets if item.get("state") == "uploaded"}
    entries = []
    for platform in sorted(PLATFORMS):
        for arch in sorted(ARCHES):
            name = f"kenvoy-{version}-{platform}-{arch}.zip"
            item = uploaded.get(name)
            if item is None or item.get("size", 0) <= 0:
                raise AssetError(f"Release 缺少 {name}")
            match = DIGEST.fullmatch(item.get("digest", "") or "")
            if not match:
                raise AssetError(f"{name} 缺少 GitHub 官方 sha256 digest")
            entries.append({
                "platform": platform,
                "arch": arch,
                "target": {
                    ("windows", "x86_64"): "x86_64-pc-windows-msvc",
                    ("windows", "ARM64"): "aarch64-pc-windows-msvc",
                    ("linux", "x86_64"): "x86_64-unknown-linux-musl",
                    ("linux", "ARM64"): "aarch64-unknown-linux-musl",
                    ("macos", "x86_64"): "x86_64-apple-darwin",
                    ("macos", "ARM64"): "aarch64-apple-darwin",
                }[(platform, arch)],
                "file": name,
                "sha256": match.group(1),
                "url": f"https://github.com/{repository}/releases/download/{tag}/{name}",
                "binary": "kenvoy.exe" if platform == "windows" else "kenvoy",
            })
    return {"version": version, "tag": tag, "repository": repository, "assets": entries}


def download(zips, assets):
    zips.mkdir(parents=True, exist_ok=True)
    for item in assets["assets"]:
        if not item["url"].startswith(f"https://github.com/{assets['repository']}/releases/download/{assets['tag']}/"):
            raise AssetError("下载地址必须属于本仓库对应 Release")
        target = zips / item["file"]
        subprocess.run(["gh", "release", "download", assets["tag"], "--repo", assets["repository"],
                        "--pattern", item["file"], "--dir", str(zips), "--clobber"], check=True)
        if not target.is_file():
            raise AssetError(f"未能下载 {item['file']}")
    return assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--zips", help="同时下载六个 ZIP 到该目录")
    arguments = parser.parse_args()
    tag = f"v{arguments.version}"
    manifest = build(arguments.version, arguments.repository, tag,
                     release_assets(arguments.repository, tag, os.environ.get("GH_TOKEN")))
    if arguments.zips:
        download(Path(arguments.zips), manifest)
    Path(arguments.output).write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"资产清单完成：{arguments.version}，6 个归档")


if __name__ == "__main__":
    try:
        main()
    except (AssetError, OSError, ValueError, KeyError) as error:
        print(f"资产清单失败：{error}", file=sys.stderr)
        sys.exit(1)
