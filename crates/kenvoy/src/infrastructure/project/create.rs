use std::{fs, io::Write, path::Path};

use super::{
    config::{Config, I18nConfig},
    error::{ProjectError as Error, Result, io},
};

pub const CUSTOMER_APP: &str = r#"export default (
  <app name="客户管理系统">
    <resource name="customers">
      <text name="name" label="客户姓名" required maxLength={100} />
      <text name="phone" label="联系电话" />
      <number name="age" label="年龄" integer min={0} />
    </resource>
    <page path="/customers" title="客户登记">
      <form resource="customers" mode="create" />
    </page>
  </app>
);
"#;

const CUSTOMER_ZH_MESSAGES: &str = r#"{
  "app.name": "客户管理系统",
  "resources.customers.label": "客户",
  "resources.customers.fields.name.label": "客户姓名",
  "resources.customers.fields.phone.label": "联系电话",
  "resources.customers.fields.age.label": "年龄",
  "pages./customers.title": "客户登记"
}
"#;

const CUSTOMER_EN_MESSAGES: &str = r#"{
  "app.name": "Customer management",
  "resources.customers.label": "Customers",
  "resources.customers.fields.name.label": "Customer name",
  "resources.customers.fields.phone.label": "Phone number",
  "resources.customers.fields.age.label": "Age",
  "pages./customers.title": "Customer registration"
}
"#;

pub fn create(target: &Path) -> Result<()> {
    if target.exists() {
        return Err(Error::InvalidConfig(format!(
            "目标已存在，不能覆盖：{}",
            target.display()
        )));
    }
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        .ok_or_else(|| Error::InvalidConfig("项目名称无效".into()))?;
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|error| io(parent, error))?;
    let target = parent.join(name);
    let mut config = Config::default();
    config.project.name = name.to_owned();
    config.project.min_cli_version = Some(env!("CARGO_PKG_VERSION").into());
    config.project.format_version = Some(super::compatibility::CURRENT_PROJECT_FORMAT.into());
    config.i18n = Some(I18nConfig::default());
    let encoded =
        toml::to_string_pretty(&config).map_err(|error| Error::InvalidConfig(error.to_string()))?;
    // 原子保留目标目录，跨平台都不会用 rename 替换已存在的空目录。
    // 中途失败保留本次部分文件供排查，不删除未知内容或再次覆盖。
    fs::create_dir(&target).map_err(|error| io(&target, error))?;
    for directory in ["src", "data", "locales"] {
        let path = target.join(directory);
        fs::create_dir(&path).map_err(|error| io(path, error))?;
    }
    for (path, content) in [
        ("kenvoy.toml", encoded.as_str()),
        ("src/app.tsx", CUSTOMER_APP),
        ("locales/zh-CN.json", CUSTOMER_ZH_MESSAGES),
        ("locales/en-US.json", CUSTOMER_EN_MESSAGES),
        (
            ".gitignore",
            "# locales/ 是项目业务词典，需与源码一同提交。\ndata/*.sqlite*\n*.kenvoy.lock\n*.kenvoy-snapshot.json\n*.kenvoy-startups.jsonl\ndist/\n.kenvoy/\nnode_modules/\n.env*\n",
        ),
        (
            "README.md",
            "# Kenvoy 应用\n\n`kenvoy create` 已固化初始迁移，尚未写入业务数据库。执行 `kenvoy dev` 启动，访问 http://127.0.0.1:3000/customers。\n\n通过 `kenvoy admin init --username admin` 隐藏输入密码初始化管理员，再访问 /admin/customers；没有默认密码。\n\n`kenvoy check` 检查声明，`kenvoy inspect customers` 查看资源，`kenvoy migrate status` 查看数据库迁移位置，`kenvoy migrate history` 查看历史。\n\n原生国际化已启用，界面可切换中文和英文，无需安装国际化库或自行装配 Provider。`locales/zh-CN.json` 与 `locales/en-US.json` 保存平面业务词典，请与源码一同提交；`kenvoy i18n extract` 只读输出当前 DSL 默认词条，`kenvoy i18n check` 检查词典并报告缺失翻译，不自动覆盖已有词典。缺失翻译回退项目默认语言及 DSL 原文；切换语言不修改业务数据或未提交草稿。词典会交付客户端，请勿写入密钥。新增语言在 kenvoy.toml 的 [i18n] 中声明并提供对应 JSON 文件。\n\nSchema 变化后执行 `kenvoy migrate generate --description \"说明本次变化\"`，审查候选并通过 `kenvoy migrate seal` 固化，再停服重启 `kenvoy dev` 或 `kenvoy run`。构建和热重载不会隐式生成正式迁移。\n\n`kenvoy build` 准备包含完整迁移链的产物；`kenvoy run` 会按需自动构建并启动生产服务，不需要事先执行 build。自定义组件的 npm 依赖已在 package.json 声明时，首次可明确使用 `kenvoy dev --install-deps`。\n\n源码入口为 src/app.tsx，SQLite 保存在 data/app.sqlite；构建不会清理数据。\n",
        ),
    ] {
        let path = target.join(path);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| io(&path, error))?;
        file.write_all(content.as_bytes())
            .map_err(|error| io(&path, error))?;
    }
    Ok(())
}
