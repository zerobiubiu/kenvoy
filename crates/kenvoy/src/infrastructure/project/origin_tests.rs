use super::config::ServerConfig;
use super::error::{ProjectError as Error, Result};

#[test]
fn deployment_origin_is_fixed_and_https_is_required_outside_loopback() -> Result<()> {
    let mut config = ServerConfig::default();
    assert_eq!(config.resolve_origin(41234)?, "http://127.0.0.1:41234");
    config.host = "::1"
        .parse()
        .map_err(|_| Error::InvalidConfig("测试地址无效".into()))?;
    assert_eq!(config.resolve_origin(41234)?, "http://[::1]:41234");
    for invalid in [
        "https://user:secret@example.com",
        "https://example.com/path",
        "https://example.com?q=1",
        "https://example.com#x",
        "ftp://example.com",
        "http://example.com",
        "http://127.0.0.1:0",
        " https://example.com",
    ] {
        config.public_origin = Some(invalid.into());
        assert!(
            config.resolve_origin(3000).is_err(),
            "错误配置被接受：{invalid}"
        );
    }
    config.public_origin = Some("http://localhost:3000".into());
    assert_eq!(config.resolve_origin(3000)?, "http://localhost:3000");
    config.host = "0.0.0.0"
        .parse()
        .map_err(|_| Error::InvalidConfig("测试地址无效".into()))?;
    assert!(config.resolve_origin(3000).is_err());
    config.public_origin = None;
    assert!(config.resolve_origin(3000).is_err());
    config.public_origin = Some("https://app.example.com:443/".into());
    assert_eq!(config.resolve_origin(3000)?, "https://app.example.com");
    Ok(())
}
