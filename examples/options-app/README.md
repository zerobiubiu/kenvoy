# 联动 select 与系统组件重载

项目格式为 0.4.0，运行使用本仓库 dev 源码构建的 Kenvoy CLI；编辑器由 `@kenvoy/types` 开发依赖提供声明，先按[示例总览](../README.md)构建同 CLI 版本的本地 tarball。项目依赖统一由 Bun 1.4.3 管理，框架内部客户端工具链仍需 Node.js 24.22+ 与配套 npm，不在本次切换范围。从仓库根目录可使用：

```text
bun install --cwd examples/options-app --ignore-scripts --omit=peer
target/debug/kenvoy --project examples/options-app admin init --username admin
target/debug/kenvoy --project examples/options-app dev
```

本地生成的 bun.lock 不提交 Git；不要对本示例使用当前仍走 npm 的 `--install-deps`。没有共享锁文件时，不同环境可能解析出不同的间接依赖。

先登录 `/admin/cities`，新增来源记录，例如 `{code:"hz",name:"杭州",province:"zj"}` 和 `{code:"nj",name:"南京",province:"js"}`；内部备注不会进入选项投影。然后访问 `/entries`，切换省份会更新服务区与远程城市选项。

示例用应用自己的 Radix Slot 包重载 select，用原生 CSS 定义样式；表单包装器继续渲染框架 children。`useResource` 从当前权限下的原生资源模型读取字段，`useField` 绑定系统字段，既不复制 Schema，也不绕过 API 权限。select 存储城市代码字符串，不能提交完整选项对象。

本例通过 `[languages]` 显式启用 `zh-CN`，显示名称为“简体中文”。根目录 `translations.json` 仅含必需的 `$schema` 引用，界面沿用源码默认文案和框架内置中文，原生国际化仍然启用。在示例目录执行 `kenvoy i18n` 会按配置更新 `language.schema.json` 并校验译文，不覆盖已有词条或自动翻译。完整规则见[示例总览](../README.md)。

详情见[组件与选项契约](../../docs/contracts/component-bindings.md)。本示例不自动创建管理员密码或写入来源数据。
