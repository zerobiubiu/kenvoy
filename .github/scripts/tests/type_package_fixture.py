"""发布回归的隔离源码包与 npm tarball；不调用 registry 或发布写入口。"""

import io
import json
import tarfile

from type_package import EXPORTS, PACKAGE_FILES


def type_files(version="1.2.3", repository="fixture/kenvoy", marker="registry declaration"):
    manifest = {
        "name": "@kenvoy/types", "version": version, "type": "module", "license": "MIT",
        "types": "./index.d.ts", "exports": EXPORTS, "files": PACKAGE_FILES,
        "publishConfig": {"access": "public"}, "peerDependencies": {"@types/react": "^19.0.0"},
        "repository": f"https://github.com/{repository}",
    }
    return {
        "package.json": json.dumps(manifest).encode(), "LICENSE": b"MIT license\n",
        "README.md": b"# Kenvoy types\n", "index.d.ts": (
            'declare module "@kenvoy/client" { export * from "@kenvoy/types/sdk"; }\n'
            'declare module "@kenvoy/client/jsx-runtime" { export * from "@kenvoy/types/jsx-runtime"; }\n'
            'declare module "@kenvoy/client/jsx-dev-runtime" { export * from "@kenvoy/types/jsx-dev-runtime"; }\n'
        ).encode(),
        "sdk/sdk.d.ts": f'export interface Model {{ label: "{marker}"; }}\n'.encode(),
        "sdk/presentation/jsx-runtime.d.ts": b"export declare const Fragment: unique symbol;\n",
        "sdk/presentation/jsx-dev-runtime.d.ts": b"export { Fragment } from './jsx-runtime';\n",
    }


def archive(files, prefix):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as bundle:
        for name, payload in files.items():
            info = tarfile.TarInfo(f"{prefix}/{name}")
            info.size = len(payload)
            bundle.addfile(info, io.BytesIO(payload))
    return buffer.getvalue()


def crate(version="1.2.3", files=None, cargo=None):
    source = {f"frontend/dist/types-package/{name}": content
              for name, content in (files if files is not None else type_files(version)).items()}
    source["Cargo.toml"] = (cargo or f'[package]\nname = "kenvoy"\nversion = "{version}"\n').encode()
    return archive(source, f"kenvoy-{version}")
