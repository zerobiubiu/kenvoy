//! `kv init`：创建新 Kenvoy 项目骨架。

use std::path::Path;

use anyhow::Result;

/// 在指定目录创建项目骨架（拒绝覆盖已有项目）。
pub fn run(dir: &Path, title: Option<&str>) -> Result<()> {
    if dir.join("kenvoy.toml").exists() || dir.join("apps").exists() {
        anyhow::bail!("{} 已存在 Kenvoy 项目，拒绝初始化", dir.display());
    }

    let name = title
        .map(str::to_string)
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "my-app".to_string());

    std::fs::create_dir_all(dir.join("apps/dashboard"))?;
    std::fs::create_dir_all(dir.join("public"))?;

    std::fs::write(
        dir.join("kenvoy.toml"),
        format!("[project]\nname = \"{name}\"\n"),
    )?;
    std::fs::write(dir.join("apps/layout.kenvoy.xml"), ROOT_LAYOUT)?;
    std::fs::write(dir.join("apps/dashboard/page.kenvoy.xml"), DASHBOARD_PAGE)?;
    std::fs::write(dir.join(".gitignore"), ".kenvoy/\ndist/\n")?;

    println!("已初始化 Kenvoy 项目：{}", dir.display());
    println!("下一步：编辑 apps/ 下的 XML，然后运行 `kv dev`");
    Ok(())
}

const ROOT_LAYOUT: &str = r#"<Layout>
  <Title>Kenvoy 项目</Title>
  <Root>
    <Row>
      <Section title="导航">
        <Text value="Kenvoy" />
      </Section>
      <Slot />
    </Row>
  </Root>
</Layout>
"#;

const DASHBOARD_PAGE: &str = r#"<Page>
  <Title>工作台</Title>
  <Content>
    <Text value="欢迎使用 Kenvoy，开始编写你的第一个业务页面。" />
  </Content>
</Page>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_skeleton() {
        let dir = std::env::temp_dir().join(format!("kenvoy-init-test-{}", std::process::id()));
        run(&dir, Some("测试项目")).expect("初始化失败");

        assert!(dir.join("kenvoy.toml").is_file());
        assert!(dir.join("apps/layout.kenvoy.xml").is_file());
        assert!(dir.join("apps/dashboard/page.kenvoy.xml").is_file());
        assert!(dir.join("public").is_dir());
        let toml = std::fs::read_to_string(dir.join("kenvoy.toml")).unwrap();
        assert!(toml.contains("测试项目"));

        // 拒绝重复初始化
        assert!(run(&dir, Some("再次")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
