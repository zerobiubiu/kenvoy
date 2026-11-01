use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
#[test]
fn embedded_runtime_is_real_reusable_and_tampering_is_not_overwritten()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("kenvoy-assets-{}-{stamp}", std::process::id()));
    fs::create_dir(&root)?;
    let outcome = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
        export(&root)?;
        let assets = &root;
        let index = fs::read_to_string(assets.join("index.html"))?;
        assert!(index.contains("/assets/") && index.contains("type=\"module\""));
        assert!(
            BUNDLED_ASSETS
                .iter()
                .any(|(name, data)| name.ends_with(".js") && data.len() > 10_000)
        );
        fs::write(assets.join("index.html"), "用户修改")?;
        assert!(matches!(verify(&root), Err(FrontendError::Corrupted(_))));
        assert!(export(&root).is_err());
        assert_eq!(fs::read_to_string(assets.join("index.html"))?, "用户修改");
        Ok(())
    })();
    if root.parent() == Some(std::env::temp_dir().as_path()) {
        fs::remove_dir_all(&root)?;
    }
    outcome
}
