"""渠道清单生成：校验 assets.json 并输出 brew、scoop 与 winget 清单。

资产契约只此一份实现，npm_packages.py 复用同一套校验，避免两个脚本各写一份导致漂移。
所有 URL 与 sha256 只取自 assets.json，脚本不联网、不自行下载、不推测发布地址。
"""

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

# 分发注册表面向国际用户，这里使用固定英文描述；名称、版本与仓库地址一律来自 assets.json。
DESCRIPTION = "Kenvoy turns TypeScript/JSX declarations into pages, controlled APIs and SQLite-backed apps"
LICENSE = "MIT"
MANIFEST_VERSION = "1.6.0"
IDENTIFIER = "Kenvoy.Kenvoy"
PUBLISHER = "Kenvoy"

VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
REPOSITORY = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]{0,38}/[A-Za-z0-9._-]{1,100}\Z")
TARGET = re.compile(r"[a-z0-9_]+(?:-[a-z0-9_]+){2,}\Z")
ARCHIVE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._+-]*\.zip\Z")
RESERVED = re.compile(r"(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?\Z")

PLATFORMS = ("windows", "linux", "macos")
ARCHES = ("x86_64", "ARM64")
COMBINATIONS = tuple((platform, arch) for platform in PLATFORMS for arch in ARCHES)
BINARIES = {"windows": "kenvoy.exe", "linux": "kenvoy", "macos": "kenvoy"}


class ChannelError(RuntimeError):
    """资产不符合契约或渠道清单无法生成；调用方必须失败退出，不得降级产出。"""


@dataclass(frozen=True)
class Asset:
    """assets.json 中的单个平台资产，字段已通过契约校验。"""

    platform: str
    arch: str
    target: str
    file: str
    sha256: str
    url: str
    binary: str

    @property
    def combination(self) -> str:
        return f"{self.platform}/{self.arch}"


@dataclass(frozen=True)
class Assets:
    """完整的六平台资产清单。"""

    version: str
    tag: str
    repository: str
    assets: tuple[Asset, ...]

    @property
    def homepage(self) -> str:
        return f"https://github.com/{self.repository}"

    def select(self, platform: str, arch: str) -> Asset:
        for asset in self.assets:
            if (asset.platform, asset.arch) == (platform, arch):
                return asset
        raise ChannelError(f"assets.json 缺少 {platform}/{arch} 资产")


def _asset(entry: object, index: int, version: str, repository: str) -> Asset:
    label = f"assets[{index}]"
    if not isinstance(entry, dict):
        raise ChannelError(f"{label} 必须是 JSON 对象")
    values = {}
    for key in ("platform", "arch", "target", "file", "sha256", "url", "binary"):
        value = entry.get(key)
        if not isinstance(value, str) or not value:
            raise ChannelError(f"{label}.{key} 必须是非空字符串")
        values[key] = value

    platform, arch = values["platform"], values["arch"]
    if (platform, arch) not in COMBINATIONS:
        raise ChannelError(
            f"{label} 的平台组合 {platform}/{arch} 非法；仅允许 windows|linux|macos × x86_64|ARM64"
        )
    if not TARGET.fullmatch(values["target"]):
        raise ChannelError(f"{label}.target 必须是 Rust 目标三元组，实际为 {values['target']!r}")

    name = values["file"]
    if not ARCHIVE.fullmatch(name) or RESERVED.fullmatch(name.split(".", 1)[0].upper()):
        raise ChannelError(f"{label}.file 必须是 .zip 普通文件名，不能含路径、设备名或控制字符")
    expected = f"kenvoy-{version}-{platform}-{arch}.zip"
    if not _matches_file(name, version, platform, arch, values["target"]):
        raise ChannelError(
            f"{label}.file 与 platform/arch 不匹配：{name}；应为 {expected} 或 kenvoy-{version}-<target>.zip"
        )

    if values["binary"] != BINARIES[platform]:
        raise ChannelError(
            f"{label}.binary 必须是 {BINARIES[platform]}，实际为 {values['binary']!r}"
        )
    if not SHA256.fullmatch(values["sha256"]):
        raise ChannelError(f"{label}.sha256 必须是 64 位小写十六进制，实际为 {values['sha256']!r}")

    expected_url = f"https://github.com/{repository}/releases/download/v{version}/{name}"
    if values["url"] != expected_url:
        raise ChannelError(
            f"{label}.url 必须指向本仓库 v{version} Release；应为 {expected_url}，实际为 {values['url']!r}"
        )
    return Asset(
        platform=platform,
        arch=arch,
        target=values["target"],
        file=name,
        sha256=values["sha256"],
        url=values["url"],
        binary=values["binary"],
    )


def _matches_file(name: str, version: str, platform: str, arch: str, target: str) -> bool:
    """归档名必须标识同一平台架构：kenvoy-<version>-<platform>-<arch>.zip 或按目标三元组命名。"""
    prefix = f"kenvoy-{version}-"
    if not name.endswith(".zip") or not name.startswith(prefix):
        return False
    tail = name[len(prefix) : -len(".zip")]
    return tail == target or tail.lower() == f"{platform}-{arch}".lower()


def load_assets(path: str | Path) -> Assets:
    """读取并严格校验 assets.json；任何偏离契约的输入都以 ChannelError 拒绝。"""
    source = Path(path)
    try:
        payload = source.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise ChannelError(f"找不到资产清单：{source}") from None
    except (OSError, UnicodeDecodeError) as error:
        raise ChannelError(f"无法读取资产清单 {source}：{error}") from None
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as error:
        raise ChannelError(f"资产清单不是合法 JSON：{error}") from None

    if not isinstance(data, dict):
        raise ChannelError("资产清单必须是 JSON 对象")
    version = data.get("version")
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        raise ChannelError(f"资产清单 version 必须是 X.Y.Z 正式版本，实际为 {version!r}")
    tag = data.get("tag")
    if tag != f"v{version}":
        raise ChannelError(f"资产清单 tag 必须等于 v{version}，实际为 {tag!r}")
    repository = data.get("repository")
    if not isinstance(repository, str) or not REPOSITORY.fullmatch(repository):
        raise ChannelError(f"资产清单 repository 必须是 owner/name，实际为 {repository!r}")

    entries = data.get("assets")
    if not isinstance(entries, list) or len(entries) != len(COMBINATIONS):
        actual = len(entries) if isinstance(entries, list) else type(entries).__name__
        raise ChannelError(f"assets 必须恰好包含 {len(COMBINATIONS)} 个平台资产，实际为 {actual}")

    assets = tuple(_asset(entry, index, version, repository) for index, entry in enumerate(entries))
    found = [asset.combination for asset in assets]
    duplicated = sorted({item for item in found if found.count(item) > 1})
    if duplicated:
        raise ChannelError("assets 存在重复的平台组合：" + "、".join(duplicated))
    missing = [f"{platform}/{arch}" for platform, arch in COMBINATIONS if f"{platform}/{arch}" not in found]
    if missing:
        raise ChannelError("assets 缺少平台组合：" + "、".join(missing))
    names = [asset.file for asset in assets]
    if len(set(names)) != len(names):
        raise ChannelError("assets 存在重复的归档文件名")
    return Assets(version=version, tag=tag, repository=repository, assets=assets)


def _template_url(assets: Assets, asset: Asset) -> str:
    """把发布 URL 里的版本替换成 scoop 的 $version 占位符，保持与当前命名一致。"""
    base = asset.url.rsplit("/", 1)[0]
    marker = f"/v{assets.version}"
    if not base.endswith(marker):
        raise ChannelError(f"无法从 {asset.url} 推导自动更新模板")
    stem = asset.file[: -len(".zip")].replace(f"-{assets.version}-", "-$version-", 1)
    return f"{base[: -len(marker)]}/v$version/{stem}.zip"


def brew_formula(assets: Assets) -> str:
    """二进制 formula：四种平台/架构组合各自声明 url 与 sha256，安装裸二进制。"""
    lines = [
        "# 该文件由 .github/scripts/channels/manifests.py 生成，请勿手工修改。",
        "class Kenvoy < Formula",
        f'  desc "{DESCRIPTION}"',
        f'  homepage "{assets.homepage}"',
        f'  version "{assets.version}"',
        f'  license "{LICENSE}"',
        "",
    ]
    for platform, outer in (("macos", "on_macos"), ("linux", "on_linux")):
        lines.append(f"  {outer} do")
        for arch, inner in (("ARM64", "on_arm"), ("x86_64", "on_intel")):
            asset = assets.select(platform, arch)
            lines.extend(
                [
                    f"    {inner} do",
                    f'      url "{asset.url}"',
                    f'      sha256 "{asset.sha256}"',
                    "    end",
                ]
            )
        lines.extend(["  end", ""])
    lines.extend(
        [
            "  def install",
            '    bin.install "kenvoy"',
            "  end",
            "",
            "  test do",
            '    assert_match version.to_s, shell_output("#{bin}/kenvoy --version")',
            "  end",
            "end",
            "",
        ]
    )
    return "\n".join(lines)


def scoop_manifest(assets: Assets) -> str:
    """Scoop bucket 清单：64bit/arm64 两套 url+hash，checkver 跟随 GitHub Release。"""
    windows = {asset.arch: asset for asset in assets.assets if asset.platform == "windows"}
    architecture = {}
    autoupdate = {}
    for arch, key in (("x86_64", "64bit"), ("ARM64", "arm64")):
        asset = windows[arch]
        architecture[key] = {"url": asset.url, "hash": f"sha256:{asset.sha256}"}
        autoupdate[key] = {"url": _template_url(assets, asset)}
    manifest = {
        "version": assets.version,
        "description": DESCRIPTION,
        "homepage": assets.homepage,
        "license": LICENSE,
        "architecture": architecture,
        "bin": "kenvoy.exe",
        "checkver": {"github": assets.homepage},
        "autoupdate": {"architecture": autoupdate},
    }
    return json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"


def winget_files(assets: Assets) -> dict[str, str]:
    """WinGet 三清单：zip 安装器 + portable 嵌套文件，x64 与 arm64 各自落位。"""
    windows = {asset.arch: asset for asset in assets.assets if asset.platform == "windows"}
    root = f"winget/manifests/k/Kenvoy/Kenvoy/{assets.version}"
    version_manifest = "\n".join(
        [
            f"PackageIdentifier: {IDENTIFIER}",
            f"PackageVersion: {assets.version}",
            "DefaultLocale: en-US",
            "ManifestType: version",
            f"ManifestVersion: {MANIFEST_VERSION}",
            "",
        ]
    )
    installer = [
        f"PackageIdentifier: {IDENTIFIER}",
        f"PackageVersion: {assets.version}",
        "Platform:",
        "- Windows.Desktop",
        "MinimumOSVersion: 10.0.0.0",
        "InstallerType: zip",
        "NestedInstallerType: portable",
        "NestedInstallerFiles:",
        f"- RelativeFilePath: {windows['x86_64'].binary}",
        "  PortableCommandAlias: kenvoy",
        "InstallModes:",
        "- silent",
        "UpgradeBehavior: install",
        "Installers:",
    ]
    for arch, key in (("x86_64", "x64"), ("ARM64", "arm64")):
        asset = windows[arch]
        installer.extend(
            [
                f"- Architecture: {key}",
                f"  InstallerUrl: {asset.url}",
                f"  InstallerSha256: {asset.sha256}",
            ]
        )
    installer.extend(["ManifestType: installer", f"ManifestVersion: {MANIFEST_VERSION}", ""])
    locale = "\n".join(
        [
            f"PackageIdentifier: {IDENTIFIER}",
            f"PackageVersion: {assets.version}",
            "PackageLocale: en-US",
            f"Publisher: {PUBLISHER}",
            f"PublisherUrl: {assets.homepage}",
            f"PublisherSupportUrl: {assets.homepage}/issues",
            "PackageName: KenVoy",
            f"PackageUrl: {assets.homepage}",
            f"License: {LICENSE}",
            f"LicenseUrl: {assets.homepage}/blob/main/LICENSE",
            f'ShortDescription: "{DESCRIPTION}"',
            "Moniker: kenvoy",
            "Tags:",
            "- cli",
            "- low-code",
            "- typescript",
            "ManifestType: locale",
            f"ManifestVersion: {MANIFEST_VERSION}",
            "",
        ]
    )
    return {
        f"{root}/Kenvoy.Kenvoy.yaml": version_manifest,
        f"{root}/Kenvoy.Kenvoy.installer.yaml": "\n".join(installer),
        f"{root}/Kenvoy.Kenvoy.locale.en-US.yaml": locale,
    }


def channel_files(assets: Assets) -> dict[str, str]:
    """全部渠道文件，键为相对输出目录的路径。"""
    files = {
        "brew/Formula/kenvoy.rb": brew_formula(assets),
        "scoop/bucket/kenvoy.json": scoop_manifest(assets),
    }
    files.update(winget_files(assets))
    return files


def write_files(files: dict[str, str], output: str | Path) -> list[Path]:
    """把渠道文件写入输出目录，路径必须落在目录内部。"""
    root = Path(output).resolve()
    written = []
    for relative, content in sorted(files.items()):
        target = (root / relative).resolve()
        if not target.is_relative_to(root) or target == root:
            raise ChannelError(f"输出路径越界：{relative}")
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("w", encoding="utf-8", newline="\n") as stream:
            stream.write(content)
        written.append(target)
    return written


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="生成 brew/scoop/winget 渠道清单")
    parser.add_argument("--assets", required=True, type=Path, help="工作流准备的 assets.json")
    parser.add_argument("--output", required=True, type=Path, help="渠道文件输出目录")
    arguments = parser.parse_args(argv)
    try:
        assets = load_assets(arguments.assets)
        written = write_files(channel_files(assets), arguments.output)
    except ChannelError as error:
        print(f"渠道清单生成失败：{error}", file=sys.stderr)
        return 1
    print(f"已生成 {len(written)} 个渠道文件，版本 {assets.version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
