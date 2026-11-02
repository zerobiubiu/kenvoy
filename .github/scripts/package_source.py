"""共享打包 action 的目标选择、源码准备、归档与独占目录清理。"""

import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile
import tomllib

from registry import download_crate, extract_crate, registry_version


ROOT = Path(__file__).resolve().parents[2]


def _version(value: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(
        r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", value
    ):
        raise ValueError("分发版本必须是无前缀的 major.minor.patch 正式版本")
    return value


def _archive_name(value: str) -> str:
    stem = value.split(".", 1)[0].upper()
    reserved = stem in {"CON", "PRN", "AUX", "NUL"} or re.fullmatch(r"(?:COM|LPT)[1-9]", stem)
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*\.zip", value) or reserved:
        raise ValueError("归档文件名必须是普通 .zip basename，不能含路径、特殊字符或 Windows 设备名")
    return value


def load_targets() -> list[dict]:
    targets = json.loads((ROOT / ".github/distribution-targets.json").read_text(encoding="utf-8"))
    required = {"target", "runner", "host", "host_arch", "extension", "platform", "arch"}
    if not isinstance(targets, list) or len(targets) != 6:
        raise ValueError("分发目标清单必须包含六个目标")
    for target in targets:
        if not isinstance(target, dict) or not required.issubset(target):
            raise ValueError("分发目标缺少 runner、host 或文件名字段")
        if any(not isinstance(target[key], str) for key in required):
            raise ValueError("分发目标字段必须是字符串")
        if target["platform"] == "windows" and not target.get("msvc_arch"):
            raise ValueError("Windows 目标缺少 MSVC 架构")
    expected = {(system, arch) for system in ("windows", "linux", "macos") for arch in ("x86_64", "ARM64")}
    if len({target["target"] for target in targets}) != 6 or {
        (target["platform"], target["arch"]) for target in targets
    } != expected:
        raise ValueError("分发目标必须唯一覆盖三个系统与两种架构")
    return targets


def _append(variable: str, values: dict[str, str]) -> None:
    with open(os.environ[variable], "a", encoding="utf-8", newline="\n") as output:
        for key, value in values.items():
            if "\n" in value or "\r" in value:
                raise ValueError(f"工作流字段不能包含换行：{key}")
            output.write(f"{key}={value}\n")


def select(system: str, arch: str) -> None:
    selected = [target for target in load_targets() if target["platform"] == system and target["arch"] == arch]
    if len(selected) != 1:
        raise ValueError("手动打包必须选择唯一的系统与架构")
    manifest = tomllib.loads((ROOT / "crates/kenvoy/Cargo.toml").read_text(encoding="utf-8"))
    package = manifest["package"]
    if package["name"] != "kenvoy":
        raise ValueError("手动打包 manifest 必须属于 kenvoy")
    version = _version(package["version"])
    _append("GITHUB_OUTPUT", {"matrix": json.dumps({"include": selected}, separators=(",", ":")), "version": version})
    print(f'本次仅打包 {selected[0]["target"]}，使用 {selected[0]["runner"]}，版本 {version}')


def _package_file(source: Path, relative: str) -> Path:
    if not isinstance(relative, str) or not relative:
        raise ValueError("源码包的 README/LICENSE 路径必须是字符串")
    path = (source / relative).resolve()
    if not path.is_relative_to(source.resolve()) or not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"源码包缺少包内非空普通文件：{relative}")
    return path


def prepare_source(mode: str, version: str, destination: Path) -> tuple[Path, Path, Path]:
    if mode == "checkout":
        source = ROOT / "crates/kenvoy"
    elif mode == "registry":
        metadata = registry_version("kenvoy", version)
        if metadata is None:
            raise ValueError(f"crates.io 尚未发布 kenvoy {version}，不能构建 registry 源码")
        payload = download_crate("kenvoy", version, metadata)
        source = extract_crate(payload, "kenvoy", version, destination)
    else:
        raise ValueError("source-mode 只能是 checkout 或 registry")
    manifest = tomllib.loads((source / "Cargo.toml").read_text(encoding="utf-8"))
    package = manifest["package"]
    if package.get("name") != "kenvoy" or package.get("version") != version:
        raise ValueError("产品 Cargo.toml 与精确分发版本不匹配")
    readme = _package_file(source, package.get("readme", "README.md"))
    if mode == "checkout":
        license_file = _package_file(ROOT, "LICENSE")
    else:
        license_file = _package_file(source, package.get("license-file", "LICENSE"))
        _package_file(source, "Cargo.lock")
        _package_file(source, "frontend/dist/index.html")
        if not any(path.is_file() and path.stat().st_size for path in (source / "frontend/dist/assets").rglob("*.js")):
            raise ValueError("registry 源码包缺少已发布的前端 JavaScript 资产，禁止重跑 npm 补造")
    return source, readme, license_file


def prepare() -> None:
    target_name = os.environ["DIST_INPUT_TARGET"]
    selected = [target for target in load_targets() if target["target"] == target_name]
    if len(selected) != 1:
        raise ValueError(f"不支持的分发目标：{target_name}")
    target = selected[0]
    expected_system = {"windows": "Windows", "linux": "Linux", "macos": "Darwin"}[target["platform"]]
    aliases = {"amd64": "x86_64", "aarch64": "arm64"}
    machine = platform.machine().lower()
    actual_arch = aliases.get(machine, machine)
    expected_arch = aliases.get(target["host_arch"], target["host_arch"])
    if platform.system() != expected_system or actual_arch != expected_arch:
        raise ValueError(f'当前 runner {platform.system()} {actual_arch} 与 {target["runner"]} 的 OS/host 契约不符')
    version = _version(os.environ["DIST_INPUT_VERSION"])
    mode = os.environ.get("DIST_INPUT_SOURCE_MODE", "checkout")
    if mode not in ("checkout", "registry"):
        raise ValueError("source-mode 只能是 checkout 或 registry")
    archive_name = _archive_name(os.environ.get("DIST_INPUT_ARCHIVE_NAME") or f'kenvoy-{version}-{target["platform"]}-{target["arch"]}.zip')
    work = Path(tempfile.mkdtemp(prefix="kenvoy-package-", dir=os.environ["RUNNER_TEMP"])).resolve()
    _append("GITHUB_ENV", {"DIST_WORK_DIR": work.as_posix()})
    paths = {
        "DIST_BUILD_DIR": work / "build",
        "DIST_ARTIFACT_DIR": work / "artifacts",
        "DIST_ZIG_TOOLS": work / "zig-tools",
        "DIST_ZIGBUILD_ROOT": work / "zigbuild",
        "DIST_SOURCE_DIR": work / "source",
    }
    for path in paths.values():
        path.mkdir()
    source, readme, license_file = prepare_source(mode, version, paths["DIST_SOURCE_DIR"])
    values = {key: path.as_posix() for key, path in paths.items()}
    values.update({
        "DIST_TARGET": target_name,
        "DIST_HOST": target["host"],
        "DIST_HOST_ARCH": target["host_arch"],
        "DIST_EXTENSION": target["extension"],
        "DIST_MSVC_ARCH": target.get("msvc_arch", ""),
        "DIST_VERSION": version,
        "DIST_ARCHIVE_NAME": archive_name,
        "DIST_PACKAGE_DIR": source.as_posix(),
        "DIST_MANIFEST": (source / "Cargo.toml").as_posix(),
        "DIST_README": readme.as_posix(),
        "DIST_LICENSE": license_file.as_posix(),
        "RUSTUP_TOOLCHAIN": "1.98.0",
        "CARGO_TERM_COLOR": "always",
        "CARGO_BUILD_JOBS": "2",
        "PYTHONUTF8": "1",
        "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS": "-C target-feature=+crt-static",
        "CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_RUSTFLAGS": "-C target-feature=+crt-static",
    })
    if target["platform"] != "windows":
        values["DIST_ZIG_HOST"] = {
            "x86_64-unknown-linux-gnu": "x86_64-linux-gnu",
            "aarch64-apple-darwin": "aarch64-macos",
        }[target["host"]]
    _append("GITHUB_ENV", values)
    _append("GITHUB_OUTPUT", {"artifact-name": f"kenvoy-{version}-{target_name}", "archive-name": archive_name})
    print(f"源码模式：{mode}；目标：{target_name}；归档：{archive_name}")


def verify_host() -> None:
    result = subprocess.run(["rustc", "-vV"], check=True, capture_output=True, text=True)
    hosts = [line.removeprefix("host: ") for line in result.stdout.splitlines() if line.startswith("host: ")]
    if hosts != [os.environ["DIST_HOST"]]:
        raise ValueError(f"实际 Rust host {hosts} 与原生 runner 契约不符")


def archive() -> None:
    env = os.environ
    build = Path(env["DIST_BUILD_DIR"])
    extension = env["DIST_EXTENSION"]
    subprocess.run([
        str(build / env["DIST_HOST"] / "release" / f"xtask{extension}"), "dist",
        "--binary", str(build / env["DIST_TARGET"] / "release" / f"kenvoy{extension}"),
        "--target", env["DIST_TARGET"], "--version", env["DIST_VERSION"],
        "--output", env["DIST_ARTIFACT_DIR"], "--archive-name", env["DIST_ARCHIVE_NAME"],
        "--readme", env["DIST_README"], "--license", env["DIST_LICENSE"],
    ], check=True)


def cleanup() -> None:
    value = os.environ.get("DIST_WORK_DIR")
    if not value:
        return
    path = Path(value)
    root = Path(os.environ["RUNNER_TEMP"]).resolve()
    if path.is_symlink() or path.resolve().parent != root or not path.name.startswith("kenvoy-package-"):
        raise ValueError(f"拒绝清理非本次独占目录：{path}")
    if path.exists():
        shutil.rmtree(path)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    selection = commands.add_parser("select")
    selection.add_argument("--platform", required=True, choices=("windows", "linux", "macos"))
    selection.add_argument("--arch", required=True, choices=("x86_64", "ARM64"))
    for command in ("prepare", "verify-host", "archive", "cleanup"):
        commands.add_parser(command)
    arguments = parser.parse_args()
    if arguments.command == "select":
        select(arguments.platform, arguments.arch)
    else:
        {"prepare": prepare, "verify-host": verify_host, "archive": archive, "cleanup": cleanup}[arguments.command]()


if __name__ == "__main__":
    main()
