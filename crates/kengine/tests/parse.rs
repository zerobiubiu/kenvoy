//! kengine 解析器 golden 测试：fixture XML → 期望 `IR` JSON。

use std::path::{Path, PathBuf};

use kengine::{SourceFile, build_project};

fn load_fixture(dir: &str) -> Vec<SourceFile> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir);
    let mut files = Vec::new();
    collect_files(&root, &root, &mut files);
    files
}

fn collect_files(dir: &Path, base: &Path, out: &mut Vec<SourceFile>) {
    for entry in std::fs::read_dir(dir).expect("读取 fixture 目录失败") {
        let entry = entry.expect("读取目录项失败");
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, base, out);
        } else {
            let rel = path
                .strip_prefix(base)
                .expect("路径前缀不匹配")
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path).expect("读取 fixture 文件失败");
            out.push(SourceFile { path: rel, content });
        }
    }
}

fn check_fixture(dir: &str, project_name: &str) {
    let files = load_fixture(dir);
    let ir = build_project(&files, project_name).expect("解析 fixture 失败");
    let json = format!(
        "{}\n",
        serde_json::to_string_pretty(&ir).expect("IR 序列化失败")
    );
    let expected_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join("expected.json");

    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&expected_path, &json).expect("写入期望文件失败");
        return;
    }

    let expected = std::fs::read_to_string(&expected_path)
        .expect("缺少 expected.json（可设置 UPDATE_FIXTURES=1 生成）");
    let expected = expected.replace("\r\n", "\n");
    assert_eq!(
        json, expected,
        "IR 输出与期望不一致；设置 UPDATE_FIXTURES=1 更新"
    );
}

#[test]
fn leave_app_parses() {
    check_fixture("leave", "请假管理");
}
