# 联动 select 与系统组件重载

要求 Kenvoy CLI 0.4.0 或以上兼容版本，项目格式为 0.4.0。首次构建需要 Node.js 24.22+ 与 npm。从仓库根目录执行：

```text
target/debug/kenvoy --project examples/options-app admin init --username admin
target/debug/kenvoy --project examples/options-app dev --install-deps
```

先登录 `/admin/cities`，新增来源记录，例如 `{code:"hz",name:"杭州",province:"zj"}` 和 `{code:"nj",name:"南京",province:"js"}`；内部备注不会进入选项投影。然后访问 `/entries`，切换省份会更新服务区与远程城市选项。

示例用应用自己的 Radix Slot 包重载 select，用原生 CSS 定义样式；表单包装器继续渲染框架 children。`useResource` 从当前权限下的原生资源模型读取字段，`useField` 绑定系统字段，既不复制 Schema，也不绕过 API 权限。select 存储城市代码字符串，不能提交完整选项对象。

本例通过 `[languages]` 显式启用 `zh-CN`，显示名称为“简体中文”。根目录 `translations.json` 仅含必需的 `$schema` 引用，界面沿用源码默认文案和框架内置中文，原生国际化仍然启用。在示例目录执行 `kenvoy i18n` 会按配置更新 `language.schema.json` 并校验译文，不覆盖已有词条或自动翻译。完整规则见[示例总览](../README.md)。

详情见[组件与选项契约](../../docs/contracts/component-bindings.md)。本示例不自动创建管理员密码或写入来源数据。
