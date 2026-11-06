"""同版 registry 类型来源、真实本机 npm pack、安全拒绝与不可覆盖恢复。"""

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock
from urllib.error import HTTPError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import package_types
import registry
import type_package
from release_common import ReleaseError
from type_package_fixture import archive, crate, type_files

VERSION = "1.2.3"
REPOSITORY = "fixture/kenvoy"
SECRET = "unrelated-domain-secret-must-not-reach-pack"


class RegistryTypePackageTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="kenvoy-types-test-")))
        self.output = self.root / "assets"
        self.plan = {"version": VERSION, "repository": REPOSITORY, "sha": "current-main",
                     "tag": f"v{VERSION}", "types_archive": type_package.archive_name(VERSION)}
        self.github = mock.Mock()
        self.github.release.return_value = None
        self.payload = crate()
        self.requests = []
        self.environment = dict(os.environ) | {
            "RUNNER_TEMP": str(self.root), "GH_TOKEN": SECRET, "CARGO_TOKEN": SECRET,
            "GITHUB_TOKEN": SECRET, "CARGO_REGISTRY_TOKEN": SECRET,
            "CARGO_REGISTRIES_CRATES_IO_TOKEN": SECRET, "NODE_AUTH_TOKEN": SECRET,
            "NPM_TOKEN": SECRET, "ACTIONS_ID_TOKEN_REQUEST_TOKEN": SECRET,
            "NPM_CONFIG_REGISTRY": "https://must-not-use.invalid",
        }
        self.enterContext(mock.patch.object(package_types, "require_latest"))
        self.enterContext(mock.patch.object(package_types, "require_tag"))
        self.network = self.enterContext(mock.patch.object(registry.urllib.request, "urlopen", side_effect=self.get))

    def get(self, request, timeout):
        headers = {key.lower(): value for key, value in request.header_items()}
        self.assertNotIn("authorization", headers)
        self.assertNotIn("cookie", headers)
        self.assertNotIn(SECRET, str(headers))
        self.assertEqual(request.get_method(), "GET")
        self.requests.append(request.full_url)
        if request.full_url == f"https://crates.io/api/v1/crates/kenvoy/{VERSION}":
            return io.BytesIO(json.dumps({"version": {
                "crate": "kenvoy", "num": VERSION, "yanked": False,
                "checksum": hashlib.sha256(self.payload).hexdigest(),
            }}).encode())
        self.assertEqual(request.full_url, f"https://static.crates.io/crates/kenvoy/kenvoy-{VERSION}.crate")
        return io.BytesIO(self.payload)

    def prepare(self):
        package_types.prepare(self.root, self.plan, self.output, self.github, self.environment)

    def test_real_npm_pack_uses_only_the_exact_published_crate_and_isolates_credentials(self):
        if shutil.which("npm.cmd" if os.name == "nt" else "npm") is None:
            self.skipTest("真实 npm pack 回归需要 Node.js 配套 npm")
        checkout = self.root / "frontend/dist/types-package"
        checkout.mkdir(parents=True)
        (checkout / "sdk.d.ts").write_text("not the published declaration", encoding="utf-8")
        real_run = subprocess.run
        pack_environments = []

        def run(arguments, **options):
            self.assertIn("pack", arguments)
            self.assertIn("--ignore-scripts", arguments)
            self.assertNotIn(SECRET, str(options["env"]))
            self.assertNotIn("NPM_CONFIG_REGISTRY", options["env"])
            self.assertNotEqual(Path(options["cwd"]).resolve(), checkout.resolve())
            pack_environments.append(dict(options["env"]))
            return real_run(arguments, **options)

        with mock.patch.object(package_types.subprocess, "run", side_effect=run):
            self.prepare()
        target = self.output / self.plan["types_archive"]
        actual = type_package.tarball_files(target, VERSION, REPOSITORY)
        type_package.require_same_files(actual, type_files())
        self.assertIn(b"registry declaration", actual["sdk/sdk.d.ts"])
        self.assertEqual(type_package.read_checksum(target.with_name(target.name + ".sha256"), target.name),
                         hashlib.sha256(target.read_bytes()).hexdigest())
        self.assertEqual(len(self.requests), 2)
        self.assertEqual(len(pack_environments), 1)

    def test_old_crate_without_types_fails_before_starting_npm(self):
        self.payload = crate(files={})
        with mock.patch.object(package_types, "pack") as pack:
            with self.assertRaises(ValueError):
                self.prepare()
        pack.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_type_version_cannot_follow_the_current_checkout_instead_of_the_crate(self):
        self.payload = crate(files=type_files("9.9.9"))
        with mock.patch.object(package_types, "pack") as pack:
            with self.assertRaises(ValueError):
                self.prepare()
        pack.assert_not_called()

    def test_official_crate_checksum_mismatch_is_not_repacked(self):
        metadata = {"crate": "kenvoy", "num": VERSION, "checksum": "0" * 64}
        with (mock.patch.object(registry, "registry_version", return_value=metadata),
              mock.patch.object(package_types, "pack") as pack):
            with self.assertRaises(ValueError):
                self.prepare()
        pack.assert_not_called()

    def test_registry_authorization_failure_is_not_a_missing_crate(self):
        self.network.side_effect = HTTPError("https://crates.io", 403, "Forbidden", {}, None)
        with mock.patch.object(package_types, "pack") as pack:
            with self.assertRaises(HTTPError):
                self.prepare()
        self.assertEqual(self.network.call_count, 1)
        pack.assert_not_called()

    def test_source_tar_traversal_and_links_are_rejected_by_the_shared_unpacker(self):
        for kind in ("traversal", "link", "duplicate"):
            with self.subTest(kind=kind):
                buffer = io.BytesIO()
                with tarfile.open(fileobj=buffer, mode="w:gz") as bundle:
                    original = registry.archive_files(crate(), f"kenvoy-{VERSION}", "")
                    for name, payload in original.items():
                        info = tarfile.TarInfo(f"kenvoy-{VERSION}/{name}")
                        info.size = len(payload)
                        bundle.addfile(info, io.BytesIO(payload))
                    name = {"traversal": "../escape", "link": "link", "duplicate": "Cargo.toml"}[kind]
                    info = tarfile.TarInfo(f"kenvoy-{VERSION}/{name}")
                    if kind == "link":
                        info.type, info.linkname = tarfile.SYMTYPE, "../../escape"
                    bundle.addfile(info)
                self.payload = buffer.getvalue()
                with mock.patch.object(package_types, "pack") as pack:
                    with self.assertRaises(ValueError):
                        self.prepare()
                pack.assert_not_called()
                self.assertFalse((self.root / "escape").exists())

    def existing(self, files):
        payload = archive(files, "package")
        name = self.plan["types_archive"]
        self.github.release.return_value = {"assets": [{
            "name": name, "state": "uploaded", "size": len(payload),
            "digest": f"sha256:{hashlib.sha256(payload).hexdigest()}",
        }]}

        def download(arguments, **options):
            self.assertEqual(arguments[:3], ["gh", "release", "download"])
            target = Path(arguments[arguments.index("--dir") + 1]) / name
            target.write_bytes(payload)
            return subprocess.CompletedProcess(arguments, 0)

        return mock.patch.object(package_types.subprocess, "run", side_effect=download)

    def test_existing_type_tarball_without_sidecar_is_reused_without_repacking(self):
        with self.existing(type_files()), mock.patch.object(package_types, "pack") as pack:
            self.prepare()
        pack.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_same_name_and_version_with_different_declarations_cannot_be_replaced(self):
        with self.existing(type_files(marker="wrong declaration")), mock.patch.object(package_types, "pack") as pack:
            with self.assertRaises(ValueError):
                self.prepare()
        pack.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_mismatched_plan_type_version_is_rejected_before_network_access(self):
        self.plan["types_archive"] = "kenvoy-types-9.9.9.tgz"
        with self.assertRaises(ReleaseError):
            self.prepare()
        self.network.assert_not_called()


class PureTypeContractTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.target = self.root / type_package.archive_name(VERSION)

    def test_runtime_files_and_manifest_capabilities_are_not_publishable(self):
        for field in ("scripts", "bin", "dependencies", "optionalDependencies", "devDependencies", "main"):
            with self.subTest(field=field):
                files = type_files()
                data = json.loads(files["package.json"])
                data[field] = {} if field != "main" else "index.js"
                files["package.json"] = json.dumps(data).encode()
                self.target.write_bytes(archive(files, "package"))
                with self.assertRaises(ValueError):
                    type_package.tarball_files(self.target, VERSION, REPOSITORY)
        for name in ("sdk/run.js", ".npmrc", "../escape.d.ts", "sdk/CON.d.ts"):
            with self.subTest(name=name):
                self.target.write_bytes(archive(type_files() | {name: b"unexpected"}, "package"))
                with self.assertRaises(ValueError):
                    type_package.tarball_files(self.target, VERSION, REPOSITORY)

    def test_empty_sdk_entry_and_non_type_exports_fail_closed(self):
        files = type_files()
        files["sdk/sdk.d.ts"] = b""
        with self.assertRaises(ValueError):
            type_package.validate_files(files, VERSION, REPOSITORY)
        files = type_files()
        data = json.loads(files["package.json"])
        data["exports"]["./sdk"]["default"] = "./sdk/sdk.js"
        files["package.json"] = json.dumps(data).encode()
        with self.assertRaises(ValueError):
            type_package.validate_files(files, VERSION, REPOSITORY)


class NpmPackReportTests(unittest.TestCase):
    def pack_report(self, report, payload_files=None):
        with tempfile.TemporaryDirectory(prefix="kenvoy-pack-report-") as temporary:
            root = Path(temporary)
            directory, output, work = root / "package", root / "output", root / "work"
            work.mkdir()
            for name, payload in type_files().items():
                path = directory / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(payload)

            def npm(arguments, **options):
                target = Path(arguments[arguments.index("--pack-destination") + 1])
                (target / type_package.archive_name(VERSION)).write_bytes(
                    archive(payload_files if payload_files is not None else type_files(), "package"))
                return subprocess.CompletedProcess(arguments, 0, json.dumps(report), "")

            with (mock.patch.object(package_types.shutil, "which", return_value="npm"),
                  mock.patch.object(package_types.subprocess, "run", side_effect=npm) as process):
                target = package_types.pack(directory, output, VERSION, REPOSITORY, {}, work)
                process.assert_called_once()
            self.assertEqual(type_package.read_checksum(output / f"{target.name}.sha256", target.name),
                             type_package.digest(target))
            return type_package.tarball_files(target, VERSION, REPOSITORY)

    def entry(self):
        return {"name": "@kenvoy/types", "version": VERSION,
                "filename": type_package.archive_name(VERSION)}

    def test_npm_array_and_package_name_keyed_object_both_preserve_real_tarball_contents(self):
        for report in ([self.entry()], {"@kenvoy/types": self.entry()}):
            with self.subTest(shape=type(report).__name__):
                type_package.require_same_files(self.pack_report(report), type_files())

    def test_wrong_report_identity_version_filename_or_multiple_results_are_rejected(self):
        entry = self.entry()
        reports = [
            [entry | {"name": "unrelated"}], [entry | {"version": "9.9.9"}],
            [entry | {"filename": "../unexpected.tgz"}], [entry, entry],
            {"unrelated": entry}, {"@kenvoy/types": entry, "other": entry}, [],
        ]
        for report in reports:
            with self.subTest(report=report):
                with self.assertRaises(ReleaseError):
                    self.pack_report(report)

    def test_correct_pack_report_cannot_hide_different_declaration_bytes(self):
        with self.assertRaises(ValueError):
            self.pack_report({"@kenvoy/types": self.entry()},
                             payload_files=type_files(marker="not the published declaration"))


if __name__ == "__main__":
    unittest.main()
