"""通过真实 Cargo 和仅限本机的 registry 验证源包上传鉴权，不接触真实凭据。"""

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


@unittest.skipUnless(shutil.which("cargo") and shutil.which("git"), "需要已有 Cargo 和 Git")
class CrateAuthenticationTests(unittest.TestCase):
    def test_default_registry_upload_authenticates_with_the_publish_secret(self):
        requests = []
        token = "kenvoy-local-registry-test-not-a-real-token"

        class RegistryHandler(BaseHTTPRequestHandler):
            def log_message(self, *_arguments):
                pass

            def do_PUT(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                metadata_length = struct.unpack("<I", body[:4])[0]
                metadata = json.loads(body[4:4 + metadata_length])
                requests.append((self.path, self.headers.get("Authorization"),
                                 metadata["name"], metadata["vers"]))
                # 验证请求后立即拒绝，不落库、不触发 Cargo 的索引可见性等待。
                payload = json.dumps({"errors": [{"detail": "本地探针不接收发布"}]}).encode()
                self.send_response(403)
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
                    ["add", "config.json"],
                    ["-c", "user.name=发布鉴权测试", "-c", "user.email=release-test@example.invalid",
                     "commit", "--no-gpg-sign", "-m", "本地 registry"],
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
                    return real_run(arguments, **kwargs, capture_output=True, timeout=60)

                def registry_version(crate, _version):
                    return {"num": "0.0.0"} if crate == "kengine" else None

                plan = {"version": "0.0.0", "engine_version": "0.0.0", "sha": "fixture"}
                with (
                    patch.object(publish_crates, "snapshot", return_value=source),
                    patch.object(publish_crates, "prepare_frontend"),
                    patch.object(publish_crates, "require_tag"),
                    patch.object(publish_crates, "require_latest"),
                    patch.object(publish_crates.registry, "registry_version", registry_version),
                    patch.object(publish_crates.subprocess, "run", run_cargo),
                    self.assertRaises(subprocess.CalledProcessError) as rejected,
                ):
                    publish_crates.publish(root, plan, object(), environment)

                self.assertEqual(
                    requests, [("/api/v1/crates/new", token, "kenvoy", "0.0.0")],
                    rejected.exception.stderr.decode("utf-8", errors="replace"),
                )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main()
