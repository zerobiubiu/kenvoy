"""构建八个 Linux 包；同版本已发布文件只复用，不因重跑改变签名或包字节。"""

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

from linux_packages import FORMATS
from manifests import ARCHES, VERSION, ChannelError


def jobs(version):
    for packager, base, mapping in FORMATS:
        for source_arch in ARCHES:
            arch = mapping[source_arch]
            config = f"{base}.yaml" if source_arch == "x86_64" else f"{base}-{arch}.yaml"
            if packager == "deb":
                name, directory = f"kenvoy_{version}_{arch}.deb", "apt/pool/main/k/kenvoy"
            elif packager == "rpm":
                name, directory = f"kenvoy-{version}-1.{arch}.rpm", f"rpm/{arch}"
            elif packager == "apk":
                name, directory = f"kenvoy_{version}_{arch}.apk", f"alpine/v3.20/main/{arch}"
            else:
                name, directory = f"kenvoy-{version}-1-{arch}.pkg.tar.zst", f"arch/{arch}"
            yield packager, config, name, f"{directory}/{name}"


def regular(path, root):
    if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root):
        raise ChannelError(f"软件包输入不是目录内的普通文件：{path.name}")


def build(configs, previous, output, version, nfpm):
    if not VERSION.fullmatch(version):
        raise ChannelError("Linux 包版本必须是正式 X.Y.Z")
    configs, previous, output = (Path(path).resolve() for path in (configs, previous, output))
    if not configs.is_dir() or not previous.is_dir():
        raise ChannelError("配置目录与已成功克隆的旧仓库目录都必须存在")
    if any(output == root or output.is_relative_to(root) or root.is_relative_to(output)
           for root in (configs, previous)):
        raise ChannelError("安装包输出不能与配置或旧仓库目录重叠")
    plan = list(jobs(version))
    for _, config, _, _ in plan:
        regular(configs / config, configs)
    output.mkdir(parents=True, exist_ok=False)
    for packager, config, name, old_relative in plan:
        source, target = previous / old_relative, output / name
        if source.exists() or source.is_symlink():
            regular(source, previous)
            shutil.copy2(source, target)
            if packager == "archlinux":
                signature = source.with_name(source.name + ".sig")
                if signature.exists() or signature.is_symlink():
                    regular(signature, previous)
                    shutil.copy2(signature, output / signature.name)
            print(f"复用已发布包：{name}")
            continue
        result = subprocess.run(
            [str(nfpm), "package", "--config", str(configs / config),
             "--packager", packager, "--target", str(target)],
            cwd=configs, capture_output=True, text=True, encoding="utf-8", errors="replace",
        )
        if result.returncode:
            raise ChannelError(f"nfpm 构建 {name} 失败（退出码 {result.returncode}），未重试")
        regular(target, output)
        print(f"已构建：{name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configs", required=True, type=Path)
    parser.add_argument("--previous", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--nfpm", required=True, type=Path)
    arguments = parser.parse_args()
    try:
        build(arguments.configs, arguments.previous, arguments.output,
              arguments.version, arguments.nfpm)
    except (ChannelError, OSError) as error:
        print(f"Linux 包构建失败：{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
