"""生成 Kenvoy Linux 渠道包配置：nfpm v2 四格式配置与 Arch PKGBUILD。"""

import argparse
import hashlib
import json
import stat
import sys
import zipfile
from pathlib import Path

from manifests import ARCHES, ChannelError, load_assets

ELF_MACHINES = {"x86_64": 62, "ARM64": 183}

# 包格式、nfpm 配置前缀、归档架构到包管理器架构的映射。
FORMATS = (
    ("deb", "nfpm-deb", {"x86_64": "amd64", "ARM64": "arm64"}),
    ("rpm", "nfpm-rpm", {"x86_64": "x86_64", "ARM64": "aarch64"}),
    ("apk", "nfpm-apk", {"x86_64": "x86_64", "ARM64": "aarch64"}),
    ("archlinux", "nfpm-archlinux", {"x86_64": "x86_64", "ARM64": "aarch64"}),
)
LICENSE = "MIT"
DESCRIPTION = "Kenvoy command-line interface"


def _quote(value):
    """以 YAML 双引号标量输出字符串，JSON 转义与 YAML 双引号兼容。"""
    return json.dumps(value, ensure_ascii=False)


def _maintainer(assets):
    owner = assets.repository.split("/", 1)[0]
    return f"{owner} <{owner}@users.noreply.github.com>"


def _linux_assets(assets):
    return {arch: assets.select("linux", arch) for arch in ARCHES}


def _output_path(output, relative):
    """把相对路径固定到输出目录内，拒绝绝对路径与 .. 穿越。"""
    base = output.resolve()
    path = (base / relative).resolve()
    if not path.is_relative_to(base):
        raise ChannelError(f"输出路径越界：{relative}")
    return path


def _archive(asset, path):
    if not path.is_file():
        raise ChannelError(f"缺少已下载归档：{path}")
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != asset.sha256:
        raise ChannelError(f"{asset.file} 的 sha256 与资产清单不一致")
    try:
        return zipfile.ZipFile(path)
    except (OSError, zipfile.BadZipFile) as error:
        raise ChannelError(f"{asset.file} 不是有效 ZIP：{error}") from error


def _member(archive, name, asset):
    try:
        info = archive.getinfo(name)
    except KeyError:
        raise ChannelError(f"{asset.file} 根目录缺少 {name}") from None
    if info.is_dir() or stat.S_ISLNK(info.external_attr >> 16):
        raise ChannelError(f"{asset.file} 的 {name} 必须是普通文件")
    return archive.read(info)


def _check_binary(payload, asset):
    """确认根目录 kenvoy 是 64 位小端 ELF，且机器类型与归档架构一致。"""
    if len(payload) < 20 or payload[:4] != b"\x7fELF":
        raise ChannelError(f"{asset.file} 的 kenvoy 不是 ELF 可执行文件")
    if payload[4] != 2 or payload[5] != 1:
        raise ChannelError(f"{asset.file} 的 kenvoy 必须是 64 位小端 ELF")
    machine = int.from_bytes(payload[18:20], "little")
    if machine != ELF_MACHINES[asset.arch]:
        raise ChannelError(f"{asset.file} 的 ELF 架构与 {asset.arch} 不一致")


def _stage(asset, zips, output):
    with _archive(asset, zips / asset.file) as archive:
        binary = _member(archive, asset.binary, asset)
        license_text = _member(archive, "LICENSE", asset)
    _check_binary(binary, asset)
    if not license_text.strip():
        raise ChannelError(f"{asset.file} 的 LICENSE 不能为空")
    binary_path = _output_path(output, f"pkgroot/{asset.arch}/usr/bin/kenvoy")
    license_path = _output_path(output, f"pkgroot/{asset.arch}/usr/share/doc/kenvoy/LICENSE")
    binary_path.parent.mkdir(parents=True, exist_ok=True)
    license_path.parent.mkdir(parents=True, exist_ok=True)
    binary_path.write_bytes(binary)
    license_path.write_bytes(license_text)
    binary_path.chmod(0o755)
    license_path.chmod(0o644)


def _arch_note(base, mapping):
    """在每份配置头注明两个归档架构到包架构及文件名，避免误用另一架构文件。"""
    pairs = [f"{arch} -> {mapping[arch]}（{_config_name(base, mapping[arch], arch)}）"
             for arch in ARCHES]
    return "# 架构映射：" + "；".join(pairs)


def _nfpm_config(assets, asset, packager, package_arch, output, note, apk_key=None):
    repository = assets.homepage
    binary = _output_path(output, f"pkgroot/{asset.arch}/usr/bin/kenvoy")
    license_path = _output_path(output, f"pkgroot/{asset.arch}/usr/share/doc/kenvoy/LICENSE")
    lines = [
        "# 由 .github/scripts/channels/linux_packages.py 生成，禁止手工修改。",
        f"# 归档：{asset.file}；包格式：{packager}；包架构：{package_arch}；仓库：{repository}",
        note,
        "# 构建：nfpm package --config 本文件 --target <输出目录>",
        'name: "kenvoy"',
        f"arch: {_quote(package_arch)}",
        'platform: "linux"',
        f"version: {_quote(assets.version)}",
        'section: "utils"',
        'priority: "optional"',
        f"maintainer: {_quote(_maintainer(assets))}",
        f"description: {_quote(DESCRIPTION)}",
        'vendor: "Kenvoy"',
        f"homepage: {_quote(repository)}",
        f"license: {_quote(LICENSE)}",
        "contents:",
        f"  - src: {_quote(binary.as_posix())}",
        '    dst: "/usr/bin/kenvoy"',
        "    file_info:",
        "      mode: 0755",
        f"  - src: {_quote(license_path.as_posix())}",
        '    dst: "/usr/share/doc/kenvoy/LICENSE"',
        "    file_info:",
        "      mode: 0644",
        "",
    ]
    if packager == "apk" and apk_key is not None:
        lines.extend([
            "apk:",
            "  signature:",
            f"    key_file: {_quote(Path(apk_key).resolve().as_posix())}",
            '    key_name: "kenvoy.rsa.pub"',
            "",
        ])
    return "\n".join(lines)


def _config_name(base, package_arch, arch):
    """x86_64 使用契约文件名，ARM64 追加各自包架构后缀。"""
    return f"{base}.yaml" if arch == "x86_64" else f"{base}-{package_arch}.yaml"


def _pkgbuild(assets, linux):
    lines = [
        "# 由 .github/scripts/channels/linux_packages.py 生成，禁止手工修改。",
        f"# Maintainer: {_maintainer(assets)}",
        "pkgname=kenvoy",
        f"pkgver={assets.version}",
        "pkgrel=1",
        f'pkgdesc="{DESCRIPTION}"',
        "arch=('x86_64' 'aarch64')",
        f'url="{assets.homepage}"',
        f"license=('{LICENSE}')",
        "conflicts=('kenvoy-bin')",
        "",
        f'source_x86_64=("{linux["x86_64"].url}")',
        f"sha256sums_x86_64=('{linux['x86_64'].sha256}')",
        f'source_aarch64=("{linux["ARM64"].url}")',
        f"sha256sums_aarch64=('{linux['ARM64'].sha256}')",
        "",
        "package() {",
        '  install -Dm755 "$srcdir/kenvoy" "$pkgdir/usr/bin/kenvoy"',
        '  install -Dm644 "$srcdir/LICENSE" "$pkgdir/usr/share/doc/kenvoy/LICENSE"',
        "}",
        "",
    ]
    return "\n".join(lines)


def generate(assets, zips, output, apk_key=None):
    """解压 Linux 归档并写出八份 nfpm 配置与 arch/PKGBUILD。"""
    linux = _linux_assets(assets)
    zips = Path(zips).resolve()
    if not zips.is_dir():
        raise ChannelError(f"ZIP 目录不存在：{zips}")
    output = Path(output).resolve()
    if output.is_file():
        raise ChannelError(f"输出路径必须是目录：{output}")
    if output == zips:
        raise ChannelError("输出目录不能与 ZIP 目录相同")
    output.mkdir(parents=True, exist_ok=True)
    for arch in ARCHES:
        _stage(linux[arch], zips, output)
    for _packager, base, mapping in FORMATS:
        note = _arch_note(base, mapping)
        for arch in ARCHES:
            name = _config_name(base, mapping[arch], arch)
            path = _output_path(output, name)
            path.write_text(
                _nfpm_config(assets, linux[arch], _packager, mapping[arch], output, note, apk_key),
                encoding="utf-8",
                newline="\n",
            )
    _output_path(output, "arch/PKGBUILD").parent.mkdir(parents=True, exist_ok=True)
    _output_path(output, "arch/PKGBUILD").write_text(
        _pkgbuild(assets, linux), encoding="utf-8", newline="\n"
    )
    print(f"已生成 8 份 nfpm 配置与 arch/PKGBUILD，版本 {assets.version}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", required=True, help="工作流准备的 assets.json")
    parser.add_argument("--zips", required=True, help="六归档 ZIP 所在目录（至少含 Linux 两个）")
    parser.add_argument("--output", required=True, help="配置、PKGBUILD 与包根目录的输出目录")
    parser.add_argument("--apk-key", help="APK RSA 私钥路径；提供时在配置中启用包签名")
    arguments = parser.parse_args()
    generate(load_assets(arguments.assets), arguments.zips, arguments.output, arguments.apk_key)


if __name__ == "__main__":
    try:
        main()
    except ChannelError as error:
        print(f"错误：{error}", file=sys.stderr)
        sys.exit(1)
