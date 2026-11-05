use std::{
    error::Error,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{cache, files};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn checksum(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(value, "{byte:02x}").expect("写入字符串不会失败");
    }
    value
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let number = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kenvoy-editor-types-{}-{stamp}-{number}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn cache(&self) -> Result<PathBuf> {
        Ok(files::directory(
            &self.0.canonicalize()?,
            Path::new(".kenvoy/types"),
        )?)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn upgrade_updates_only_owned_bytes_and_preserves_user_files() -> Result {
    let scratch = Scratch::new()?;
    let root = scratch.cache()?;
    let old: &[(&str, &[u8])] = &[
        ("sdk.d.ts", b"export declare const version: 1;\n"),
        ("obsolete.d.ts", b"export declare const obsolete: true;\n"),
        (
            "modified-obsolete.d.ts",
            b"export declare const changed: false;\n",
        ),
    ];
    cache::refresh(&root, old)?;
    fs::write(root.join("user.d.ts"), "用户自己的声明")?;
    fs::write(root.join("modified-obsolete.d.ts"), "用户已接管这份旧文件")?;
    let new: &[(&str, &[u8])] = &[
        ("sdk.d.ts", b"export declare const version: 2;\n"),
        (
            "presentation/added.d.ts",
            b"export declare const added: true;\n",
        ),
    ];
    let result = cache::refresh(&root, new)?;
    assert_eq!(result.created, 1);
    assert_eq!(result.updated, 1);
    assert_eq!(result.removed, 1);
    assert_eq!(result.preserved_user_files, ["modified-obsolete.d.ts"]);
    assert_eq!(fs::read(root.join("sdk.d.ts"))?, new[0].1);
    assert!(!root.join("obsolete.d.ts").exists());
    assert_eq!(
        fs::read_to_string(root.join("user.d.ts"))?,
        "用户自己的声明"
    );
    assert_eq!(
        fs::read_to_string(root.join("modified-obsolete.d.ts"))?,
        "用户已接管这份旧文件"
    );
    let manifest: Value = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    assert_eq!(
        manifest["files"].as_object().ok_or("缺少生成清单")?.len(),
        2
    );
    let again = cache::refresh(&root, new)?;
    assert_eq!(again.unchanged, 2);
    assert_eq!(again.updated + again.created + again.removed, 0);
    Ok(())
}

#[test]
fn unknown_collisions_and_modified_current_declarations_are_not_overwritten() -> Result {
    let scratch = Scratch::new()?;
    let root = scratch.cache()?;
    let sources: &[(&str, &[u8])] = &[("sdk.d.ts", b"export declare const sdk: true;\n")];
    fs::write(root.join("sdk.d.ts"), "用户文件")?;
    assert!(cache::refresh(&root, sources).is_err());
    assert_eq!(fs::read_to_string(root.join("sdk.d.ts"))?, "用户文件");
    assert!(!root.join("manifest.json").exists());
    fs::remove_file(root.join("sdk.d.ts"))?;
    cache::refresh(&root, sources)?;
    let manifest = fs::read(root.join("manifest.json"))?;
    fs::write(root.join("sdk.d.ts"), "已改动生成文件")?;
    assert!(cache::refresh(&root, sources).is_err());
    assert_eq!(fs::read_to_string(root.join("sdk.d.ts"))?, "已改动生成文件");
    assert_eq!(fs::read(root.join("manifest.json"))?, manifest);
    Ok(())
}

#[test]
fn interrupted_refresh_accepts_recorded_old_or_new_hashes_and_finishes() -> Result {
    let scratch = Scratch::new()?;
    let root = scratch.cache()?;
    let old: &[(&str, &[u8])] = &[("sdk.d.ts", b"export declare const version: 1;\n")];
    let new: &[(&str, &[u8])] = &[
        ("sdk.d.ts", b"export declare const version: 2;\n"),
        ("new.d.ts", b"export declare const added: true;\n"),
    ];
    cache::refresh(&root, old)?;
    let manifest_path = root.join("manifest.json");
    let mut pending: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    pending["files"]["sdk.d.ts"]
        .as_array_mut()
        .ok_or("缺少摘要")?
        .push(json!(checksum(new[0].1)));
    pending["files"]["new.d.ts"] = json!([checksum(new[1].1)]);
    fs::write(&manifest_path, serde_json::to_vec_pretty(&pending)?)?;
    // 模拟声明替换完成一半后进程退出；清单仍保留旧/新两个可接受摘要。
    fs::write(root.join("sdk.d.ts"), new[0].1)?;
    let refreshed = cache::refresh(&root, new)?;
    assert_eq!(refreshed.unchanged, 1);
    assert_eq!(refreshed.created, 1);
    let finalized: Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
    assert_eq!(
        finalized["files"]["sdk.d.ts"]
            .as_array()
            .ok_or("缺少摘要")?
            .len(),
        1
    );
    assert_eq!(fs::read(root.join("new.d.ts"))?, new[1].1);
    Ok(())
}

#[test]
fn paths_in_sources_or_manifests_cannot_escape_or_alias() -> Result {
    let scratch = Scratch::new()?;
    let root = scratch.cache()?;
    let outside = scratch.0.join("outside.d.ts");
    fs::write(&outside, "不能改写")?;
    for name in [
        "../outside.d.ts",
        "/outside.d.ts",
        "C:/outside.d.ts",
        "a\\outside.d.ts",
        "x/../outside.d.ts",
    ] {
        assert!(
            cache::refresh(&root, &[(name, "覆盖".as_bytes())]).is_err(),
            "{name}"
        );
        assert!(!root.join("manifest.json").exists());
        let manifest = json!({
            "format": 1, "generator": "kenvoy-editor-types",
            "files": {(name): [checksum("不能改写".as_bytes())]}
        });
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        fs::write(root.join("manifest.json"), &bytes)?;
        assert!(cache::refresh(&root, &[("sdk.d.ts", b"export {};")]).is_err());
        assert_eq!(fs::read(root.join("manifest.json"))?, bytes);
        fs::remove_file(root.join("manifest.json"))?;
    }
    assert!(cache::refresh(&root, &[("sdk.d.ts", b"a"), ("SDK.d.ts", b"b")]).is_err());
    assert_eq!(fs::read_to_string(outside)?, "不能改写");
    Ok(())
}

#[test]
fn replacement_rechecks_expected_bytes_and_cleans_only_its_temporary_file() -> Result {
    let scratch = Scratch::new()?;
    let root = scratch.cache()?;
    fs::write(root.join("sdk.d.ts"), "用户刚刚改动")?;
    let keep = root.join(".kenvoy-editor-user.tmp");
    fs::write(&keep, "不是本次临时文件")?;
    assert!(files::replace(&root, "sdk.d.ts", Some(b"old"), b"new").is_err());
    assert_eq!(fs::read_to_string(root.join("sdk.d.ts"))?, "用户刚刚改动");
    assert_eq!(fs::read_to_string(keep)?, "不是本次临时文件");
    assert_eq!(fs::read_dir(root)?.count(), 2);
    Ok(())
}
