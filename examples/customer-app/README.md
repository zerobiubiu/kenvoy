# 客户登记示例

要求 Kenvoy CLI 0.4.0 或以上兼容版本，项目格式为 0.4.0。执行 `kenvoy dev` 启动，`kenvoy run` 用于生产运行。

源码入口为 src/app.tsx，SQLite 保存在 data/app.sqlite；构建不会清理数据。

本例通过 `[languages]` 显式启用 `zh-CN`，显示名称为“简体中文”。根目录 `translations.json` 仅含指向 `language.schema.json` 的 `$schema` 引用，界面沿用 DSL 默认文案和框架内置中文。原生国际化不能关闭；`kenvoy i18n` 更新 Schema 并校验译文，不覆盖已有文件、不提取或生成业务翻译。完整规则见[示例总览](../README.md)。
