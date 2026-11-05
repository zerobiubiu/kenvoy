use std::{fs, path::Path};

use serde_json::json;

use super::support::{Fixture, Result, reject, snapshot};

#[test]
fn files_and_directories_cannot_be_substituted_for_editor_boundaries() -> Result {
    for (name, directory) in [
        (".kenvoy", false),
        (".kenvoy/types", false),
        (".kenvoy/types/sdk.d.ts", true),
        (".kenvoy/types/manifest.json", true),
        ("tsconfig.json", true),
        ("package.json", true),
    ] {
        let fixture = Fixture::new()?;
        let path = fixture.project().join(name);
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
        if directory {
            fs::create_dir(&path)?;
        } else {
            fs::write(&path, "用户保留文件")?;
        }
        let before = snapshot(&fixture.project())?;
        reject(&fixture.types()?)?;
        assert_eq!(snapshot(&fixture.project())?, before, "{name}");
        assert!(!fixture.project().join("data/app.sqlite").exists());
    }
    Ok(())
}

#[test]
fn modified_owned_declarations_unknown_collisions_and_broken_manifests_are_not_overwritten()
-> Result {
    let fixture = Fixture::new()?;
    let cache = fixture.project().join(".kenvoy/types");
    let manifest = fs::read(cache.join("manifest.json"))?;
    let sdk = fs::read(cache.join("sdk.d.ts"))?;
    fs::write(cache.join("sdk.d.ts"), "用户已改写 SDK 声明")?;
    let before = snapshot(&fixture.project())?;
    assert!(reject(&fixture.types()?)?.contains("未覆盖"));
    assert_eq!(snapshot(&fixture.project())?, before);
    fs::write(cache.join("sdk.d.ts"), sdk)?;
    fs::remove_file(cache.join("manifest.json"))?;
    let before = snapshot(&fixture.project())?;
    reject(&fixture.types()?)?;
    assert_eq!(snapshot(&fixture.project())?, before);
    fs::write(cache.join("manifest.json"), b"{ broken manifest")?;
    let before = snapshot(&fixture.project())?;
    assert!(reject(&fixture.types()?)?.contains("清单"));
    assert_eq!(snapshot(&fixture.project())?, before);
    fs::write(cache.join("manifest.json"), manifest)?;
    Ok(())
}

#[test]
fn a_user_edited_manifest_cannot_target_files_outside_the_type_cache() -> Result {
    let fixture = Fixture::new()?;
    let protected = fixture.project().join("outside.d.ts");
    fs::write(&protected, "用户自己的项目声明")?;
    let manifest = json!({
        "format": 1,
        "generator": "kenvoy-editor-types",
        "files": {"../outside.d.ts": ["0".repeat(64)]}
    });
    fs::write(
        fixture.project().join(".kenvoy/types/manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    let before = snapshot(&fixture.project())?;
    reject(&fixture.types()?)?;
    assert_eq!(snapshot(&fixture.project())?, before);
    assert_eq!(fs::read_to_string(protected)?, "用户自己的项目声明");
    Ok(())
}

#[cfg(windows)]
fn directory_link(target: &Path, link: &Path) -> Result {
    let link = link.to_string_lossy().replace('/', "\\");
    let target = target.to_string_lossy().replace('/', "\\");
    let output = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(unix)]
fn directory_link(target: &Path, link: &Path) -> Result {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

#[cfg(any(windows, unix))]
#[test]
fn directory_links_are_rejected_at_every_editor_cache_level() -> Result {
    for name in [
        ".kenvoy",
        ".kenvoy/types",
        ".kenvoy/types/presentation",
        "tsconfig.json",
        "package.json",
    ] {
        let fixture = Fixture::new()?;
        let outside = fixture.0.join("outside");
        fs::create_dir(&outside)?;
        fs::write(outside.join("keep.d.ts"), "链接目标只能保留，不得写入")?;
        let link = fixture.project().join(name);
        if link.is_dir() {
            fs::remove_dir_all(&link)?;
        } else {
            fs::remove_file(&link)?;
        }
        directory_link(&outside, &link)?;
        let before = snapshot(&outside)?;
        reject(&fixture.types()?)?;
        assert_eq!(snapshot(&outside)?, before, "{name}");
        assert!(!fixture.project().join("data/app.sqlite").exists());
        #[cfg(windows)]
        fs::remove_dir(link)?;
        #[cfg(unix)]
        fs::remove_file(link)?;
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn declaration_and_configuration_file_symlinks_are_rejected_without_touching_targets() -> Result {
    for name in [
        ".kenvoy/types/sdk.d.ts",
        ".kenvoy/types/manifest.json",
        "tsconfig.json",
        "package.json",
    ] {
        let fixture = Fixture::new()?;
        let outside = fixture.0.join("outside-file");
        let marker = b"outside target must not change";
        fs::write(&outside, marker)?;
        let link = fixture.project().join(name);
        fs::remove_file(&link)?;
        std::os::unix::fs::symlink(&outside, &link)?;
        reject(&fixture.types()?)?;
        assert_eq!(fs::read(outside)?, marker, "{name}");
        fs::remove_file(link)?;
    }
    Ok(())
}
