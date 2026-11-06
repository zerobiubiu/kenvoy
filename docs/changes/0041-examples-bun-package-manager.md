# 0041：examples 切换为 Bun 包管理

状态：已执行；范围：五个示例的包管理器声明、锁文件与使用说明。前置记录：[0040：示例开发类型与检查](0040-example-typecheck-dependencies.md)。用户明确要求仅执行切换，不做验证；本轮未运行安装验证、冻结锁校验、类型检查、测试、构建或应用。

## 切换内容

- customer-app、extensions-app、options-app、uploads-app、i18n-app 的 package.json 均增加 `packageManager: bun@1.4.3`。
- 通过 Bun 将各自已有 npm 锁转换为文本 bun.lock，保留并提交新的锁文件。
- 删除五个示例根目录的 package-lock.json，不并存两套包管理器锁。
- 更新六份示例 README：项目依赖用 Bun，脚本使用 bun run；不再通过当前仍走 npm 的 `--install-deps` 准备 Bun 示例。
- 不修改示例业务源码、迁移、数据或用户已有 i18n 暂存源码；不修改 CLI 内部受控 npm 工具链、依赖安装器和构建指纹实现。

## lock 文件处理

bun.lock 有必要保留。这些是可运行应用，package.json 中直接依赖固定版本并不能固定完整传递依赖树。提交 Bun 锁用于后续可复现安装；node_modules 和 .kenvoy/types 仍不入库。

后续使用命令为 `bun install --frozen-lockfile --ignore-scripts --omit=peer`；该命令只是使用说明，本轮没有执行。依赖升级时由 Bun 更新锁，不手工编辑 bun.lock，也不再生成 npm 锁文件。

## 执行记录（无额外验证）

本机已有 Bun 版本为 1.4.3。分别在五个示例目录执行锁文件转换：

```text
bun install --lockfile-only --save-text-lockfile --ignore-scripts --omit=peer
```

该操作只生成锁文件，不安装依赖。五次均输出从 package-lock.json 迁移并保存 bun.lock，退出 0；Bun 输出的锁定包数依次为 customer-app 25、extensions-app 29、options-app 28、uploads-app 25、i18n-app 25。随后删除旧 npm 锁文件。

以上是执行结果，不是验证结论。未进行锁内容比对、依赖重装、冻结安装、typecheck、测试、Cargo 检查、构建或浏览器操作；0040 的既有 npm 检查记录保留原历史含义，不冒充本次 Bun 结果。

## 保持的边界

本轮仅改变 examples 的包管理方式。自定义客户端的框架缓存仍由 Kenvoy 使用 Node/npm 管理，当前 types 报告和 `--install-deps` 仍面向默认 npm 安装器；Bun 示例按各 README 手动准备项目依赖。

当前 CLI 未把 bun.lock 纳入构建输入指纹。本轮不扩展其实现；仅更新 Bun 锁后，应显式执行 kenvoy build 再使用 run，避免沿用旧客户端资产。此为现有代码边界说明，不代表本轮执行了该流程。

产品版本和已有版本标签不变，未对外发布。
