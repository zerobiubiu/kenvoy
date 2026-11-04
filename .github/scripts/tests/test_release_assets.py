"""验证草稿不可按标签查询时的完整发布及已有资产恢复，禁止重复创建或覆盖。"""

import hashlib
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import publish_release
from release_common import GitHub


class DraftReleaseFixture:
    def __init__(self, tag, preceding=0):
        self.tag = tag
        self.record = None
        self.payloads = {}
        self.preceding = preceding
        self.created = 0
        self.uploaded = []

    def get(self, suffix):
        if suffix == f"releases/tags/{self.tag}":
            return self.view() if self.record and not self.record["draft"] else None
        if suffix.startswith("releases?per_page=100&page="):
            page = int(suffix.rsplit("=", 1)[1])
            records = [{"id": index + 10, "tag_name": f"unrelated-{index}"}
                       for index in range(self.preceding)]
            if self.record:
                records.append(self.view())
            return records[(page - 1) * 100:page * 100]
        raise AssertionError(f"意外 GitHub 查询：{suffix}")

    def view(self):
        return {**self.record, "assets": [
            {"name": name, "size": len(payload), "state": "uploaded",
             "digest": "sha256:" + hashlib.sha256(payload).hexdigest()}
            for name, payload in self.payloads.items()
        ]}

    def gh(self, _repository, operation, tag, *arguments):
        if tag != self.tag:
            raise AssertionError("操作了错误版本")
        if operation == "create":
            if self.record:
                raise AssertionError("重复创建已有草稿")
            self.created += 1
            self.record = {"id": 1, "tag_name": tag, "draft": True}
        elif operation == "upload":
            path = Path(arguments[0])
            if path.name in self.payloads:
                raise AssertionError("覆盖了既有资产")
            self.payloads[path.name] = path.read_bytes()
            self.uploaded.append(path.name)
        elif operation == "download":
            name = arguments[arguments.index("--pattern") + 1]
            directory = Path(arguments[arguments.index("--dir") + 1])
            (directory / name).write_bytes(self.payloads[name])
        elif operation == "edit":
            self.record["draft"] = False
        else:
            raise AssertionError(f"意外 Release 操作：{operation}")


class ReleaseAssetsTests(unittest.TestCase):
    def publish_fixture(self, existing=False, preceding=0):
        tag = "v1.2.3"
        fixture = DraftReleaseFixture(tag, preceding)
        names = [f"kenvoy-1.2.3-{platform}-{arch}.zip"
                 for platform in ("windows", "linux", "macos")
                 for arch in ("x86_64", "ARM64")]
        payloads = {name: f"归档内容:{name}".encode() for name in names}
        if existing:
            fixture.record = {"id": 1, "tag_name": tag, "draft": True}
            fixture.payloads[names[0]] = payloads[names[0]]
        with tempfile.TemporaryDirectory(prefix="kenvoy-release-assets-test-") as temporary:
            root = Path(temporary)
            for name, payload in payloads.items():
                if existing and name == names[0]:
                    # 已有 ZIP 没有本地构建输入：恢复只能复用它，不能要求重新构建。
                    continue
                (root / name).write_bytes(payload)
                (root / f"{name}.sha256").write_text(
                    publish_release.checksum_text(hashlib.sha256(payload).hexdigest(), name),
                    encoding="ascii",
                )
            github = GitHub("fixture/kenvoy", token="")
            plan = {"repository": "fixture/kenvoy", "sha": "fixture", "version": "1.2.3",
                    "tag": tag, "archives": [{"name": name} for name in names]}
            with (patch.object(github, "get", fixture.get),
                  patch.object(publish_release, "gh", fixture.gh),
                  patch.object(publish_release, "require_latest"),
                  patch.object(publish_release, "require_tag")):
                publish_release.complete_release(root, plan, root, github, {})

        self.assertFalse(fixture.record["draft"])
        self.assertEqual(fixture.created, 0 if existing else 1)
        self.assertEqual(set(fixture.payloads),
                         {entry for name in names for entry in (name, f"{name}.sha256")})
        for name, payload in payloads.items():
            self.assertEqual(fixture.payloads[name], payload)
            self.assertEqual(fixture.payloads[f"{name}.sha256"],
                             publish_release.checksum_text(
                                 hashlib.sha256(payload).hexdigest(), name).encode("ascii"))
        if existing:
            self.assertNotIn(names[0], fixture.uploaded)

    def test_new_draft_remains_discoverable_through_all_asset_uploads(self):
        self.publish_fixture()

    def test_existing_draft_on_later_page_reuses_the_original_zip(self):
        self.publish_fixture(existing=True, preceding=100)


if __name__ == "__main__":
    unittest.main()
