"""组装 APT、RPM、Alpine 与 Arch 软件包仓库索引，缺失工具时明确失败。"""

import argparse
import gzip
import hashlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import dataclass
from email.utils import formatdate
from pathlib import Path, PurePosixPath

from manifests import VERSION

PACKAGE_NAME = "kenvoy"
# 与 docs/operations/distribution-channels.md 及 channels.yml 的 APK 源路径保持一致。
ALPINE_BRANCH = "v3.20"
DEB_ARCHES = ("amd64", "arm64")
APK_ARCHES = ("x86_64", "aarch64")
RPM_ARCHES = ("x86_64", "aarch64")
ARCH_ARCHES = ("x86_64", "aarch64")
PKG_TAR_SUFFIXES = (".pkg.tar.zst", ".pkg.tar.xz", ".pkg.tar.gz")
PACKAGE_VERSION = re.compile(r"\d+\.\d+\.\d+(?:[-+~][0-9A-Za-z.+~]+)?\Z")

DEB_NAME = re.compile(
    r"(?P<name>[a-z0-9][a-z0-9+.-]*)_(?P<version>[^_]+)_(?P<arch>amd64|arm64|all)\.deb\Z"
)
RPM_NAME = re.compile(
    r"(?P<name>.+)[-_](?P<version>\d+\.\d+\.\d+)(?:[-_](?P<release>[^_-]+))?"
    r"[-_.](?P<arch>x86_64|aarch64)\.rpm\Z"
)
ARCH_NAME = re.compile(
    r"(?P<name>.+)[-_](?P<version>\d+\.\d+\.\d+)(?:[-_](?P<release>[^_-]+))?"
    r"[-_](?P<arch>x86_64|aarch64)\.pkg\.tar\.(?:zst|xz|gz)\Z"
)


class RepoError(RuntimeError):
    """软件包目录、索引工具或输出路径不满足仓库组装前置条件。"""


@dataclass(frozen=True)
class Package:
    path: Path
    kind: str
    version: str
    arch: str


def _inside(root, relative):
    """把相对路径固定到根目录内，拒绝绝对路径与 .. 穿越。"""
    base = root.resolve()
    path = (base / relative).resolve()
    if not path.is_relative_to(base):
        raise RepoError(f"路径越界：{relative}")
    return path


def _version_matches(value, version):
    """允许包管理器附加版本后缀，例如 0.3.0-1 或 0.3.0-r0。"""
    return value == version or value.startswith(f"{version}-")


def _apk_metadata(path):
    try:
        with tarfile.open(path, "r:gz") as archive:
            try:
                member = archive.getmember(".PKGINFO")
            except KeyError:
                raise RepoError(f"{path.name} 缺少 .PKGINFO") from None
            stream = archive.extractfile(member)
            payload = stream.read() if stream is not None else b""
    except (tarfile.TarError, OSError) as error:
        raise RepoError(f"{path.name} 不是有效的 APK 包：{error}") from error
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RepoError(f"{path.name} 的 .PKGINFO 不是 UTF-8") from error
    fields = {}
    for line in text.splitlines():
        if "=" in line and not line.lstrip().startswith("#"):
            key, _, value = line.partition("=")
            fields[key.strip()] = value.strip()
    return fields


def _matches(path, pattern, arches):
    match = pattern.fullmatch(path.name)
    if match is None:
        raise RepoError(f"无法识别的软件包文件名：{path.name}")
    if match["name"] != PACKAGE_NAME:
        raise RepoError(f"仓库只接受 {PACKAGE_NAME} 包，实际 {match['name']}")
    if not PACKAGE_VERSION.fullmatch(match["version"]):
        raise RepoError(f"{path.name} 的版本不是 X.Y.Z 形式")
    if match["arch"] not in arches:
        raise RepoError(f"{path.name} 的架构不受支持：{match['arch']}")
    return match["version"], match["arch"]


def classify(path):
    """按扩展名识别软件包，未知文件返回 None；已知格式但内容不符则报错。"""
    name = path.name
    if name.endswith(".deb"):
        package_version, arch = _matches(path, DEB_NAME, DEB_ARCHES)
        return Package(path, "deb", package_version, arch)
    if name.endswith(".rpm"):
        package_version, arch = _matches(path, RPM_NAME, RPM_ARCHES)
        return Package(path, "rpm", package_version, arch)
    if name.endswith(".apk"):
        fields = _apk_metadata(path)
        if fields.get("pkgname") != PACKAGE_NAME:
            raise RepoError(f"{name} 的包名必须是 {PACKAGE_NAME}")
        package_version = fields.get("pkgver", "")
        if not PACKAGE_VERSION.fullmatch(package_version):
            raise RepoError(f"{name} 的版本不是 X.Y.Z 形式")
        arch = fields.get("arch", "")
        if arch not in APK_ARCHES:
            raise RepoError(f"{name} 的架构不受支持：{arch}")
        return Package(path, "apk", package_version, arch)
    if name.endswith(PKG_TAR_SUFFIXES):
        package_version, arch = _matches(path, ARCH_NAME, ARCH_ARCHES)
        return Package(path, "arch", package_version, arch)
    return None


def discover(packages):
    root = packages.resolve()
    found = []
    seen = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise RepoError(f"软件包目录不允许符号链接：{path.name}")
        if not path.is_file():
            continue
        if not path.resolve().is_relative_to(root):
            raise RepoError(f"软件包路径越界：{path.name}")
        package = classify(path)
        if package is None:
            continue
        key = (package.kind, package.arch, path.name)
        if key in seen:
            raise RepoError(f"软件包重名：{path.name}")
        seen[key] = package
        found.append(package)
    if not found:
        raise RepoError("软件包目录没有可组装的 kenvoy 包")
    return found


def _stanzas(text):
    blocks = []
    for block in text.replace("\r\n", "\n").strip().split("\n\n"):
        lines = [line for line in block.splitlines() if line.strip()]
        if lines:
            blocks.append("\n".join(lines))
    return blocks


def _field(stanza, name):
    for line in stanza.splitlines():
        if line.startswith(f"{name}:"):
            return line.split(":", 1)[1].strip()
    return None


def _require_indexed(packages, stanzas, arches):
    names = set()
    for stanza in stanzas:
        filename = _field(stanza, "Filename")
        if filename:
            names.add(PurePosixPath(filename).name)
        architecture = _field(stanza, "Architecture")
        if architecture not in set(arches) | {"all"}:
            raise RepoError(f"APT 索引包含未知架构：{architecture}")
    missing = sorted({package.path.name for package in packages} - names)
    if missing:
        raise RepoError(f"APT 索引遗漏软件包：{', '.join(missing)}")


def _build_apt(work, debs, version):
    apt = work / "apt"
    pool = _inside(apt, "pool/main/k/kenvoy")
    pool.mkdir(parents=True)
    for package in debs:
        shutil.copy2(package.path, _inside(apt, f"pool/main/k/kenvoy/{package.path.name}"))
    arches = sorted({package.arch for package in debs if package.arch != "all"}) or list(DEB_ARCHES)
    indexes = {}
    if _available("apt-ftparchive"):
        stanzas = _stanzas(_run("apt-ftparchive", ["packages", "pool"], cwd=apt).stdout)
        _require_indexed(debs, stanzas, arches)
        grouped = {arch: [] for arch in arches}
        for stanza in stanzas:
            architecture = _field(stanza, "Architecture")
            if architecture == "all":
                for arch in arches:
                    grouped[arch].append(stanza)
            else:
                grouped[architecture].append(stanza)
        indexes = {
            arch: "\n\n".join(grouped[arch]) + "\n" if grouped[arch] else "" for arch in arches
        }
    elif _available("dpkg-scanpackages"):
        for arch in arches:
            selected = [package for package in debs if package.arch in (arch, "all")]
            text = _run("dpkg-scanpackages", ["--arch", arch, "pool"], cwd=apt).stdout
            stanzas = _stanzas(text)
            _require_indexed(selected, stanzas, arches)
            indexes[arch] = "\n\n".join(stanzas) + "\n" if stanzas else ""
    else:
        raise RepoError("缺少 apt-ftparchive 或 dpkg-scanpackages，无法生成 APT 索引")
    for arch in arches:
        directory = _inside(apt, f"dists/stable/main/binary-{arch}")
        directory.mkdir(parents=True, exist_ok=True)
        payload = indexes[arch].encode("utf-8")
        (directory / "Packages").write_bytes(payload)
        (directory / "Packages.gz").write_bytes(gzip.compress(payload, mtime=0))
    _write_release(apt, version, arches)


def _write_release(apt, version, arches):
    entries = []
    for arch in arches:
        for name in ("Packages", "Packages.gz"):
            path = _inside(apt, f"dists/stable/main/binary-{arch}/{name}")
            payload = path.read_bytes()
            entries.append((
                hashlib.md5(payload, usedforsecurity=False).hexdigest(),
                len(payload),
                f"main/binary-{arch}/{name}",
                hashlib.sha256(payload).hexdigest(),
            ))
    lines = [
        "Origin: kenvoy",
        "Label: kenvoy",
        "Suite: stable",
        "Codename: stable",
        f"Architectures: {' '.join(arches)}",
        "Components: main",
        f"Description: Kenvoy {version} APT repository",
        f"Date: {formatdate(usegmt=True)}",
        "MD5Sum:",
    ]
    lines.extend(f" {md5} {size:16d} {name}" for md5, size, name, _ in entries)
    lines.append("SHA256:")
    lines.extend(f" {sha} {size:16d} {name}" for _, size, name, sha in entries)
    release = _inside(apt, "dists/stable/Release")
    release.parent.mkdir(parents=True, exist_ok=True)
    release.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def _build_rpm(work, rpms):
    if not _available("createrepo_c"):
        raise RepoError("缺少 createrepo_c，无法生成 RPM repodata")
    for arch in sorted({package.arch for package in rpms}):
        directory = work / "rpm" / arch
        directory.mkdir(parents=True)
        for package in rpms:
            if package.arch == arch:
                shutil.copy2(package.path, _inside(directory, package.path.name))
        _run("createrepo_c", [str(directory)])
        if not (directory / "repodata/repomd.xml").is_file():
            raise RepoError(f"createrepo_c 未生成 rpm/{arch}/repodata/repomd.xml")


def _require_member(path, member):
    try:
        with tarfile.open(path, "r:gz") as archive:
            names = archive.getnames()
    except (tarfile.TarError, OSError) as error:
        raise RepoError(f"{path.name} 不是有效 tar.gz：{error}") from error
    if not any(PurePosixPath(name).name == member for name in names):
        raise RepoError(f"{path.name} 缺少 {member}")


def _build_alpine(work, apks, version):
    root = work / "alpine" / ALPINE_BRANCH
    if not _available("apk"):
        raise RepoError("缺少 apk，无法生成 Alpine APKINDEX")
    for arch in sorted({package.arch for package in apks}):
        directory = root / "main" / arch
        directory.mkdir(parents=True)
        names = []
        for package in apks:
            if package.arch == arch:
                shutil.copy2(package.path, _inside(root, f"main/{arch}/{package.path.name}"))
                names.append(package.path.name)
        _run("apk", ["index", "-o", "APKINDEX.tar.gz", "-d", f"Kenvoy {version}",
                     *sorted(names)], cwd=directory)
        target = directory / "APKINDEX.tar.gz"
        if not target.is_file():
            raise RepoError("apk 未生成 APKINDEX.tar.gz")
        _require_member(target, "APKINDEX")


def _build_arch(work, packages):
    if not _available("repo-add"):
        raise RepoError("缺少 repo-add，无法生成 Arch 数据库")
    for arch in sorted({package.arch for package in packages}):
        directory = work / "arch" / arch
        directory.mkdir(parents=True)
        names = []
        for package in packages:
            if package.arch == arch:
                shutil.copy2(package.path, _inside(work / "arch", f"{arch}/{package.path.name}"))
                signature = package.path.with_name(package.path.name + ".sig")
                if signature.is_symlink():
                    raise RepoError("Arch 软件包签名不能是符号链接")
                if signature.is_file():
                    shutil.copy2(signature, _inside(directory, signature.name))
                names.append(package.path.name)
        _run("repo-add", ["kenvoy.db.tar.gz", *sorted(names)], cwd=directory)
        target = directory / "kenvoy.db.tar.gz"
        if not target.is_file():
            raise RepoError("repo-add 未生成 kenvoy.db.tar.gz")
        _require_member(target, "desc")


def _available(name):
    return shutil.which(name) is not None


def _run(name, arguments, cwd=None):
    executable = shutil.which(name)
    if executable is None:
        raise RepoError(f"缺少 {name}，无法继续生成仓库")
    try:
        result = subprocess.run(
            [executable, *arguments], cwd=cwd, capture_output=True, text=True, encoding="utf-8"
        )
    except OSError as error:
        raise RepoError(f"无法执行 {name}：{error}") from error
    if result.returncode:
        raise RepoError(f"{name} 执行失败（退出 {result.returncode}）：{result.stderr.strip()}")
    return result


def _install(work, repo):
    """逐个替换仓库子树；任一步失败时回滚已替换的目录。"""
    repo.mkdir(parents=True, exist_ok=True)
    ecosystems = sorted(path.name for path in work.iterdir() if path.is_dir())
    backups = []
    placed = []
    try:
        for ecosystem in ecosystems:
            target = _inside(repo, ecosystem)
            source = work / ecosystem
            if target.exists():
                if target.is_symlink() or not target.is_dir():
                    raise RepoError(f"仓库目标不是普通目录：{ecosystem}")
                backup = work / f"backup-{ecosystem}"
                target.rename(backup)
                backups.append((target, backup))
            shutil.move(str(source), str(target))
            placed.append((target, source))
    except Exception:
        for target, source in reversed(placed):
            if target.exists() and not source.exists():
                shutil.move(str(target), str(source))
        for target, backup in reversed(backups):
            if backup.exists() and not target.exists():
                backup.rename(target)
        raise


def assemble(packages, repo, version):
    """在临时目录完整组装后原子替换四个仓库子树，失败不触碰目标仓库。

    packages 是本次要发布的完整包集合；保留旧版本时由调用方把历史包一并放入，
    脚本只要求其中至少有一个包属于 version。
    """
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        raise RepoError(f"版本必须是正式 X.Y.Z，实际 {version}")
    packages_root = Path(packages).resolve()
    repo_root = Path(repo).resolve()
    if not packages_root.is_dir():
        raise RepoError(f"软件包目录不存在：{packages_root}")
    if repo_root == Path(repo_root.anchor):
        raise RepoError("仓库目录不能是文件系统根目录")
    if repo_root.is_file():
        raise RepoError(f"仓库路径必须是目录：{repo_root}")
    if (repo_root == packages_root or repo_root.is_relative_to(packages_root)
            or packages_root.is_relative_to(repo_root)):
        raise RepoError("仓库目录与软件包目录不能互相包含")
    found = discover(packages_root)
    if not any(_version_matches(package.version, version) for package in found):
        raise RepoError(f"软件包目录没有 {version} 版本的 kenvoy 包")
    groups = {"deb": [], "rpm": [], "apk": [], "arch": []}
    for package in found:
        groups[package.kind].append(package)
    repo_root.parent.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix=f".{repo_root.name}-staging-", dir=repo_root.parent))
    installed = False
    try:
        if groups["deb"]:
            _build_apt(work, groups["deb"], version)
        if groups["rpm"]:
            _build_rpm(work, groups["rpm"])
        if groups["apk"]:
            _build_alpine(work, groups["apk"], version)
        if groups["arch"]:
            _build_arch(work, groups["arch"])
        _install(work, repo_root)
        installed = True
    finally:
        backups = [path for path in work.iterdir() if path.name.startswith("backup-")]
        if installed or not backups:
            shutil.rmtree(work, ignore_errors=True)
        else:
            print(f"警告：仓库替换回滚未完成，保留 {work}", file=sys.stderr)
    print(f"仓库索引完成：{version}，DEB {len(groups['deb'])}、RPM {len(groups['rpm'])}、"
          f"APK {len(groups['apk'])}、Arch {len(groups['arch'])}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="严格 X.Y.Z 版本")
    parser.add_argument("--packages", required=True, help="已构建的 .deb/.rpm/.apk/.pkg.tar.* 目录")
    parser.add_argument("--repo", required=True, help="仓库站点根目录，生成 apt/rpm/alpine/arch")
    arguments = parser.parse_args()
    assemble(arguments.packages, arguments.repo, arguments.version)


if __name__ == "__main__":
    try:
        main()
    except RepoError as error:
        print(f"错误：{error}", file=sys.stderr)
        sys.exit(1)
