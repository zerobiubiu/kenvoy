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
        format!(
            "[project]\nname = \"{name}\"\n\n# 注册模式：open（公开注册）| review（审核注册）| hr（人事绑定式，关闭公开注册）\n# [auth]\n# mode = \"open\"\n# 密码强度：none | weak | medium | strong | custom（默认 none）\n# [auth.password]\n# strength = \"none\"\n\n# 对象存储（上传文件走 S3，写入 public/ 是被禁止的）\n# [storage]\n# endpoint = \"http://127.0.0.1:9000\"\n# region = \"us-east-1\"\n# bucket = \"kenvoy-dev\"\n# access_key = \"minioadmin\"\n# secret_key = \"minioadmin\"\n# max_size_mb = 10                  # 上传大小上限（缺省不限）\n# allowed_types = [\"image/*\", \"application/pdf\"]   # 类型白名单（缺省不限）\n"
        ),
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
