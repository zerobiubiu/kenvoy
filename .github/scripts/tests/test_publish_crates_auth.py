"""通过真实 Cargo 和本机 registry 验证上传鉴权、归档摘要及发布恢复，不接触真实凭据。"""

import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import publish_crates
from type_package_fixture import type_files


@unittest.skipUnless(shutil.which("cargo") and shutil.which("git"), "需要已有 Cargo 和 Git")
class CrateAuthenticationTests(unittest.TestCase):
    def test_default_registry_upload_authenticates_and_verifies_published_bytes(self):
        self.verify_publication()

    def test_publication_honors_an_external_target_directory(self):
        self.verify_publication(external_target=True)

    def test_cli_archive_without_types_stops_before_any_registry_upload(self):
        self.verify_publication(missing_types=True)

    def verify_publication(self, external_target=False, missing_types=False):
        requests = []
        token = "kenvoy-local-registry-test-not-a-real-token"
        uploaded = {}

        class RegistryHandler(BaseHTTPRequestHandler):
            def log_message(self, *_arguments):
                pass

            def do_PUT(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                metadata_length = struct.unpack("<I", body[:4])[0]
                metadata = json.loads(body[4:4 + metadata_length])
                requests.append((self.path, self.headers.get("Authorization"),
                                 metadata["name"], metadata["vers"]))
                if self.headers.get("Authorization") != token:
                    self.send_error(401)
                    return
                offset = 4 + metadata_length
                archive_length = struct.unpack("<I", body[offset:offset + 4])[0]
                archive = body[offset + 4:]
                if len(archive) != archive_length:
                    self.send_error(400)
                    return
                uploaded["checksum"] = hashlib.sha256(archive).hexdigest()
                entry = index / "ke" / "nv" / "kenvoy"
                entry.parent.mkdir(parents=True, exist_ok=True)
                entry.write_text(json.dumps({
                    "name": "kenvoy", "vers": "0.0.0", "deps": [],
                    "cksum": uploaded["checksum"], "features": {}, "yanked": False,
                }) + "\n", encoding="utf-8")
                for arguments in (["add", "ke/nv/kenvoy"],
                                  ["commit", "--no-gpg-sign", "-m", "记录本机测试上传"]):
                    real_run(["git", *arguments], cwd=index, env=environment,
                             check=True, capture_output=True, timeout=30)
                payload = json.dumps({"warnings": {
                    "invalid_categories": [], "invalid_badges": [], "other": [],
                }}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        server = ThreadingHTTPServer(("127.0.0.1", 0), RegistryHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory(prefix="kenvoy-cargo-auth-test-") as temporary:
                root = Path(temporary)
                source = root / "source"
                (source / "src").mkdir(parents=True)
                (source / "Cargo.toml").write_text(
                    '[package]\nname = "kenvoy"\nversion = "0.0.0"\n'
                    'edition = "2021"\nlicense = "MIT"\n'
                    'description = "Local authentication probe"\n', encoding="utf-8",
                )
                (source / "src/lib.rs").write_text("pub fn probe() {}\n", encoding="utf-8")
                if not missing_types:
                    for name, payload in type_files("0.0.0").items():
                        path = source / "frontend/dist/types-package" / name
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_bytes(payload)
                cargo_home = root / "cargo-home"
                cargo_home.mkdir()
                environment = {
                    key: value for key, value in os.environ.items()
                    if not key.upper().startswith("CARGO_")
                    and key not in publish_crates.SECRET_KEYS
                    and not key.upper().endswith("_PROXY")
                    and key not in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")
                }
                environment.update({
                    "CARGO_HOME": str(cargo_home), "CARGO_HTTP_PROXY": "",
                    "CARGO_TOKEN": token, "GIT_CONFIG_NOSYSTEM": "1",
                    "GIT_CONFIG_GLOBAL": os.devnull,
                })
                if external_target:
                    environment["CARGO_TARGET_DIR"] = str(root / "external-target")
                real_run = subprocess.run
                real_run(["cargo", "generate-lockfile", "--offline"], cwd=source,
                         env=environment, check=True, capture_output=True, timeout=30)

                index = root / "index"
                index.mkdir()
                endpoint = f"http://127.0.0.1:{server.server_port}"
                (index / "config.json").write_text(
                    json.dumps({"dl": endpoint + "/download", "api": endpoint}),
                    encoding="utf-8",
                )
                for arguments in (
                    ["init", "--initial-branch=main"],
                    ["config", "core.hooksPath", str(root / "no-hooks")],
                    ["config", "user.name", "发布鉴权测试"],
                    ["config", "user.email", "release-test@example.invalid"],
                    ["add", "config.json"],
                    ["commit", "--no-gpg-sign", "-m", "本地 registry"],
                ):
                    real_run(["git", *arguments], cwd=index, env=environment,
                             check=True, capture_output=True, timeout=30)
                # 仅测试使用 Cargo 自身的 crates.io 替身入口；普通 index 覆盖不模拟默认仓库鉴权。
                # 依据 cargo-test-support 的 replace_crates_io，外网仍由拒绝转发的本机代理阻断。
                environment.update({
                    "__CARGO_TEST_CRATES_IO_URL_DO_NOT_USE_THIS": index.as_uri(),
                    "CARGO_HTTP_PROXY": endpoint, "NO_PROXY": "127.0.0.1,localhost",
                    "CARGO_NET_RETRY": "0",
                })

                def run_cargo(arguments, **kwargs):
                    # 限定索引与网络出口为本机，参数和鉴权环境原样交给真实 Cargo。
                    kwargs.setdefault("capture_output", True)
                    kwargs.setdefault("timeout", 60)
                    try:
                        return real_run(arguments, **kwargs)
                    except subprocess.CalledProcessError as error:
                        stderr = error.stderr or ""
                        error.add_note(stderr.decode("utf-8", errors="replace")
                                       if isinstance(stderr, bytes) else stderr)
                        raise

                def registry_version(crate, _version):
                    if crate == "kengine":
                        return {"num": "0.0.0"}
                    return dict(uploaded) if uploaded else None

                plan = {"version": "0.0.0", "engine_version": "0.0.0", "sha": "fixture",
                        "repository": "fixture/kenvoy"}
                with (
                    patch.object(publish_crates, "snapshot", return_value=source),
                    patch.object(publish_crates, "prepare_frontend"),
                    patch.object(publish_crates, "require_tag"),
                    patch.object(publish_crates, "require_latest"),
                    patch.object(publish_crates.registry, "registry_version", registry_version),
                    patch.object(publish_crates.subprocess, "run", run_cargo),
                ):
                    if missing_types:
                        with self.assertRaisesRegex(ValueError, "禁止从当前 HEAD"):
                            publish_crates.publish(root, plan, object(), environment)
                    else:
                        publish_crates.publish(root, plan, object(), environment)

                self.assertEqual(requests, [] if missing_types else
                                 [("/api/v1/crates/new", token, "kenvoy", "0.0.0")])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main()
