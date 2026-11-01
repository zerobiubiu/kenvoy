---
version: 1
slug: "crates-kenvoy-frontend"
primary_target: "crates/kenvoy/frontend"
related_targets: []
---

# 默认 React Runtime

状态：现行；阶段 00—10 的首版基础功能已验收。本轮按用户要求将默认组件迁移为 React、TypeScript 与原生 CSS，并增加选项联动和受控组件重载；新行为以新增验证记录为准。交互模式为 Operate，沿用已实施的界面方向。

用户已明确要求剔除框架 MUI、允许用户选择 React 组件库；产品事实取自 [PRODUCT.md](../../PRODUCT.md) 与本次指令。当前技术选择替代原始工程规范中的默认 MUI，保留布局与业务流程。本轮设计决定只服务本项目，不写入跨项目偏好。

## 结构与构建

React + TypeScript + 原生 CSS + React Router 根据 `/_kenvoy/application` 的受控投影渲染通用组件；普通表单、后台及字段重载使用同一资源字段、验证和 API。函数/自定义组件只通过注册 ID 连接，不能 JSON 序列化函数或把完整应用入口打包给浏览器。用户组件库不需要依赖框架主题 Provider。

框架源码位于 `crates/kenvoy/frontend`，发行构建预先生成 Vite/Rolldown 资产并嵌入 Rust Binary。基础 DSL 使用内置资产，应用运行不要求 Node。内置与自定义客户端统一发布到默认 `dist/versions/<build_id>/client`，由 `dist/manifest.json` 选择有效版本；不使用旧 `.kenvoy/runtime` 路径。

自定义 UI 独立打包、开发适配、状态横幅与兼容组件 Fast Refresh 已通过阶段 08—10。用户不需要写 Vite 配置。构建只负责模块注册和资产，不按页面生成 React 业务源码。生产静态目录仅包含可公开资产，服务端模块位于同版本独立的 `modules/` 目录且不公开。

## Direction contract

THESIS：登记人完成准确提交，管理员快速定位和修改记录；所有状态围绕真实操作反馈组织。

OWN-WORLD：采用公共信息目录的清楚标识与一致索引，蓝色主操作、深色文字、白色工作区和浅灰背景；原生可访问控件、系统中文字体、均匀间距。界面不使用地图、纸张纹理或代码装饰。

STORY：访问者读标题、填写字段、提交后看到真实结果。管理员登录后从资源导航进入列表，搜索定位记录，在可返回的编辑表单完成修改。

FIRST VIEWPORT：桌面后台左侧资源导航，上方应用名和账号/退出，主区标题、搜索及新增按钮、排序表格和分页。登记页中央单列表单，错误位于字段下；移动端导航收起，操作不挤到屏幕外。

FORM：七个候选分别取自服务登记、分类索引、文档目录、表格排印、公共标识、资料档案、公共信息目录；采用第七项，保留用户固定的企业后台结构。参考 seed:35f73ecf88170e69。轨道图挑战方案只保留当前位置清晰的纪律；唱片编码方案不适合直接业务标签；登机牌方案只保留修改记录仍可定位的反馈。标志性交互是编辑成功后更新该行并短暂显示温和底色，同时提供文字确认。

FINISH：unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## 状态与验证

必需状态：加载、空数据、搜索无结果、字段错误、服务器错误、提交中、成功、网络中断后的结果待核对、登录失败、会话过期、普通用户无管理权限、删除确认。禁止将未知提交结果显示为失败并自动重试。

迁移前的登记、管理、自定义扩展和开发生命周期已有真实浏览器验收；原桌面与 390px 移动端截图作为视觉参考。新组件需复核可见标签、焦点、原生选择器、对话框关闭/恢复焦点和窄屏表格管理入口，不能沿用旧截图宣称新实现通过。选项联动的加载、错误、空结果及失效值提示与字段共用数据契约；重载组件不得绕过提交校验。

现行细节见 [前端 Runtime](../../docs/architecture/frontend-runtime.md) 与 [DESIGN.md](../../DESIGN.md)，实际命令、计数、截图范围和限制统一见 [验证索引](../../docs/verification/README.md)。迁移前 detector 数据不作为本轮通过证据。
