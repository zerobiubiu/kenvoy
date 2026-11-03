use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "kenvoy",
    version,
    about = "使用 TypeScript/JSX 声明应用的 Rust 原生框架"
)]
pub(crate) struct Cli {
    /// 项目位置；默认从当前目录向上查找 kenvoy.toml。
    #[arg(long, global = true, default_value = ".")]
    pub(crate) project: PathBuf,
    /// 输出结构化日志；不会记录密码、Cookie 或表单内容。
    #[arg(long, global = true)]
    pub(crate) json_logs: bool,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// 创建包含客户登记示例的项目。
    Create { path: PathBuf },
    /// 检查源码、模块、声明、权限与路由，不连接数据库。
    Check {
        /// 验证 JSON 记录所对应的资源。
        #[arg(long, requires = "input")]
        resource: Option<String>,
        /// 额外离线验证一条 JSON 记录，不写入数据库。
        #[arg(long, requires = "resource")]
        input: Option<PathBuf>,
        /// 按部分更新规则验证，缺失字段不会补默认值。
        #[arg(long, requires = "input")]
        patch: bool,
    },
    /// 查看应用模型或指定资源的注册规则。
    Inspect {
        resource: Option<String>,
        /// 输出通用源码 IR（包含源代码，仅供本地开发检查）。
        #[arg(long)]
        generic: bool,
    },
    /// 生成、固化、预览和执行正式迁移，或显式接入旧数据库。
    Migrate {
        #[command(subcommand)]
        action: Option<MigrationCommand>,
    },
    /// 原生词条提取和语言词典检查；不写入数据库或覆盖词典。
    I18n {
        #[command(subcommand)]
        action: Option<I18nCommand>,
    },
    /// 初始化管理员或由本机项目所有者创建账号。
    Admin {
        #[command(subcommand)]
        action: AdminCommand,
    },
    /// 构建版本化应用描述、客户端资产和服务端模块，不连接数据库。
    Build {
        /// 明确允许安装用户 package.json 已声明的依赖，不增加或改变依赖声明。
        #[arg(long)]
        install_deps: bool,
    },
    /// 加载生产产物并启动；有源码且需要更新时自动构建。
    Run {
        /// 自动构建时明确允许安装用户已声明的 npm 依赖。
        #[arg(long)]
        install_deps: bool,
        /// 确认 status 显示的完整危险迁移计划校验值。
        #[arg(long)]
        confirm: Option<String>,
    },
    /// 启动开发服务。
    Dev {
        /// 明确允许安装用户已声明的 npm 依赖。
        #[arg(long)]
        install_deps: bool,
        /// 创建独立的一次性空开发快照，例如 dev-snapshots/try.sqlite。
        #[arg(long)]
        snapshot: Option<PathBuf>,
        /// 确认 status 显示的完整危险迁移计划校验值。
        #[arg(long)]
        confirm: Option<String>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub(crate) enum AccountRole {
    User,
    Admin,
}

#[derive(Subcommand)]
pub(crate) enum AdminCommand {
    /// 首次创建管理员；已有管理员时拒绝覆盖。
    Init {
        #[arg(long)]
        username: String,
        /// 从标准输入读取一行密码，适用于受控自动化；默认隐藏交互输入。
        #[arg(long)]
        password_stdin: bool,
    },
    /// 创建普通用户或管理员；要求已完成首次初始化。
    AddUser {
        #[arg(long)]
        username: String,
        #[arg(long, value_enum, default_value = "user")]
        role: AccountRole,
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MigrationCommand {
    /// 只读预览正式迁移链与数据库状态；不创建数据库。
    Status,
    /// 按顺序执行已固化迁移；危险计划需要精确校验值。
    Apply {
        #[arg(long)]
        confirm: Option<String>,
    },
    /// 只读显示已执行迁移及校验记录。
    History,
    /// 比较上一正式 Schema 与当前声明，生成可审查候选；不连接数据库。
    Generate {
        #[arg(long)]
        description: String,
        /// 明确的数据转换、改名、回填及前后断言规则文件。
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// 验证并追加固化候选；危险候选需要精确校验值。
    Seal {
        #[arg(long)]
        confirm: Option<String>,
    },
    /// 显式核验旧库基线；省略 confirm 时只输出接入报告。
    Baseline {
        #[arg(long)]
        at: String,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long)]
        confirm: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum I18nCommand {
    /// 检查词典并报告各语言缺失条目；缺失单个词条将使用回退文案。
    Check,
    /// 输出当前 DSL 默认词条及框架文案的平面 JSON，不覆盖已有翻译文件。
    Extract {
        /// 选择已配置语言的框架内置文案，业务默认文案仍来自当前声明。
        #[arg(long)]
        locale: Option<String>,
    },
}
