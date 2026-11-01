# 0006：React + MUI 的通用 CSR Runtime

- 状态：参考；阶段 00 设计记录，React、Rolldown、HMR 集成均待实际验证。
- 影响范围：前端解释器、组件注册、资源构建和开发适配层。

## 背景

框架既要解释内置 DSL，也要运行带 Hooks、Props、CSS 和第三方依赖的普通 React 组件。CSR 是第一版明确要求。

## 问题

页面描述不是可直接执行的 React 组件；打包能力也不自动包含完整开发服务器和 Fast Refresh。

## 可选方案

1. 每个页面生成独立 React 业务源码：重复模型，不符合解释优先。
2. 仅提供静态 HTML 或远程 CDN 页面：不足以验证完整交互和受控依赖。
3. 通用 React Runtime 解释描述，构建用户组件并通过注册表引用。

## 最终选择

采用方案 3，以 React、MUI、React Router 和 CSR 为默认。Runtime 将受控组件标识解析为真实 React 组件；通过模块/导出注册 ID 绑定用户组件及客户端表达式。保持 React 与 React DOM 单一实例，普通组件由 React 正常渲染。[React createElement](https://react.dev/reference/react/createElement)、[React Hooks 限制](https://react.dev/warnings/invalid-hook-call-warning)

MUI 默认 Emotion 依赖与 React peer dependencies 一并锁定和验证；路由先采用 React Router Declarative Mode，由 Kenvoy 保持数据与服务器控制权。[MUI 安装](https://mui.com/material-ui/getting-started/installation/)、[React Router 模式](https://reactrouter.com/start/modes)

基础 DSL 项目离线使用框架随发行版提供的预构建前端资源，这是实施计划。框架维护时批准并锁定依赖、构建与校验资产；用户自定义组件仍需批准并准备对应项目依赖。首次使用缺依赖时明确报告，不能静默下载执行。

Rolldown 为生产打包候选。官方列出 TS/JSX 转换、模块解析并将 HMR 标为 WIP；官网建议应用通过 Vite 获得完整开发体验。因此 Vite + 官方 React 插件仅作为待批准、待探针的开发适配候选。Axum 负责生产 HTTP，不将 Vite 开发服务器用于生产托管。[Rolldown 能力](https://rolldown.rs/guide/introduction)、[Rolldown 推荐集成](https://rolldown.rs/guide/getting-started)、[Vite 构建说明](https://vite.dev/guide/)

## 原因

通用渲染器保留声明模型，标准 React 模块构建保留用户扩展能力。对辅助开发工具的使用不改变 Rust 原生宿主及业务解释边界。

## 已知缺点

自定义组件引入包管理和平台构建依赖；Fast Refresh 有导出和组件边界限制。Axum 输出 HTML 时须处理开发资源与 Refresh preamble。[Vite 后端集成](https://vite.dev/guide/backend-integration)、[官方 React 插件](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react/README.md)

## 影响

客户端构建不得从完整声明入口无区别地打包全部依赖；服务端图与客户端图分离。生产 manifest 指向已构建资产；`run` 自动构建仍须已有获准的工具和依赖。构建不能清理数据库。

## 验证与状态

本地 React 等前端包不可解析，本轮未安装，也未运行浏览器、打包或 HMR 探针。阶段 06 前须真实验证页面描述渲染、MUI/CSS、跨文件 Hooks、同源 API、深层 URL 刷新、语法错误恢复和 Fast Refresh 状态保留；阶段 08 检查客户端产物及 sourcemap 不含服务端源码。
