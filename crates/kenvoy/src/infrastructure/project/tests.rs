use super::*;
#[test]
fn unknown_configuration_and_unsupported_scalar_types_are_rejected() {
    assert!(toml::from_str::<Config>("[server]\nporrt=3000").is_err());
    assert!(toml::from_str::<Config>("[server]\nport='3000'").is_err());
    assert!(toml::from_str::<Config>("[unknown]\nvalue=1").is_err());
    assert_eq!(
        toml::from_str::<Config>("").expect("默认配置").server.port,
        3000
    );
}
#[test]
fn paths_reject_parent_absolute_and_overlapping_boundaries() -> Result<()> {
    let root = std::env::current_dir()
        .map_err(|error| io(".", error))?
        .canonicalize()
        .map_err(|error| io(".", error))?;
    assert!(safe_path(&root, Path::new("../outside")).is_err());
    assert!(safe_path(&root, &root).is_err());
    assert!(overlaps(&root.join("src"), &root.join("src/dist")));
    assert!(!overlaps(&root.join("src"), &root.join("src-other")));
    Ok(())
}
