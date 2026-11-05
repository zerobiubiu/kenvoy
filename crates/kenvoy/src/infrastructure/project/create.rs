use std::{fs, io::Write, path::Path};

use super::{
    config::Config,
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

const CUSTOMER_TRANSLATIONS: &str = r#"{
  "$schema": "./language.schema.json",
  "app.name": {
    "zh-CN": "客户管理系统",
    "en-US": "Customer management",
    "ko-KR": "고객 관리"
  },
  "resources.customers.label": {
    "zh-CN": "客户",
    "en-US": "Customers",
    "ko-KR": "고객"
  },
  "resources.customers.fields.name.label": {
    "zh-CN": "客户姓名",
    "en-US": "Customer name",
    "ko-KR": "고객 이름"
  },
  "resources.customers.fields.phone.label": {
    "zh-CN": "联系电话",
    "en-US": "Phone number",
    "ko-KR": "전화번호"
  },
  "resources.customers.fields.age.label": {
    "zh-CN": "年龄",
    "en-US": "Age",
    "ko-KR": "나이"
  },
  "pages./customers.title": {
    "zh-CN": "客户登记",
    "en-US": "Customer registration",
    "ko-KR": "고객 등록"
  }
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
    let encoded =
        toml::to_string_pretty(&config).map_err(|error| Error::InvalidConfig(error.to_string()))?;
    let mut schema = serde_json::to_string_pretty(&super::i18n::schema(&config.languages)?)
        .map_err(|error| Error::InvalidConfig(error.to_string()))?;
    schema.push('\n');
    // 原子保留目标目录，跨平台都不会用 rename 替换已存在的空目录。
    // 中途失败保留本次部分文件供排查，不删除未知内容或再次覆盖。
    fs::create_dir(&target).map_err(|error| io(&target, error))?;
    for directory in ["src", "data"] {
        let path = target.join(directory);
        fs::create_dir(&path).map_err(|error| io(path, error))?;
    }
    for (path, content) in [
        ("kenvoy.toml", encoded.as_str()),
        ("src/app.tsx", CUSTOMER_APP),
        ("translations.json", CUSTOMER_TRANSLATIONS),
        ("language.schema.json", schema.as_str()),
        (
            ".gitignore",
            "# translations.json 与 language.schema.json 随配置和源码提交。\ndata/*.sqlite*\n*.kenvoy.lock\n*.kenvoy-snapshot.json\n*.kenvoy-startups.jsonl\ndist/\n.kenvoy/\nnode_modules/\n.env*\n",
        ),
        (
            "README.md",
            "# Kenvoy 应用\n\n`kenvoy create` 已固化初始迁移，尚未写入业务数据库。执行 `kenvoy dev` 启动，访问 http://127.0.0.1:3000/customers。\n\n通过 `kenvoy admin init --username admin` 隐藏输入密码初始化管理员，再访问 /admin/customers；没有默认密码。\n\n`kenvoy check` 检查声明，`kenvoy inspect customers` 查看资源，`kenvoy migrate status` 查看数据库迁移位置，`kenvoy migrate history` 查看历史。\n\n原生国际化始终启用，无需安装国际化库或自行装配 Provider。kenvoy.toml 的 [languages] 声明 fallback 与每种语言的 display、可选 alias；模板默认中文、英文、韩文三种语言，业务译文已完整提供，系统内置文案仍只有中文和英文并按 fallback 回退。每次打开或刷新优先精确匹配设备默认语言，不支持时使用 fallback；页面内可手动切换，label 使用配置 display。\n\n根 translations.json 是集中业务词典，必须保留 \"$schema\":\"./language.schema.json\"；每条消息都必须覆盖全部配置语言，每种语言的 code 或 alias 恰好写一个，不能同时写、漏写或使用未知语言。没有业务词条也合法；没有对应词条的原生 DSL 文案继续使用源码文字。词典会交付客户端，请勿写入密钥。\n\n修改 [languages] 后执行 `kenvoy i18n` 刷新 language.schema.json；它从不覆盖已有 translations.json，首次缺失时仅创建 Schema 引用，不伪造译文。dev/run 也会刷新 Schema 并严格校验，build/check 只读校验，词典缺失或无效会阻断；无需源码即可校验。配置、词典与 Schema 请一同提交，切换语言不会修改业务数据或未提交草稿。\n\n数据库 Schema 变化后执行 `kenvoy migrate generate --description \"说明本次变化\"`，审查候选并通过 `kenvoy migrate seal` 固化，再停服重启 `kenvoy dev` 或 `kenvoy run`。构建和热重载不会隐式生成正式迁移。\n\n`kenvoy build` 准备包含完整迁移链的产物；`kenvoy run` 会按需自动构建并启动生产服务，不需要事先执行 build。自定义组件的 npm 依赖已在 package.json 声明时，首次可明确使用 `kenvoy dev --install-deps`。\n\n源码入口为 src/app.tsx，SQLite 保存在 data/app.sqlite；构建不会清理数据。\n",
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
    super::editor::prepare_at(&target, &config)?;
    Ok(())
}
