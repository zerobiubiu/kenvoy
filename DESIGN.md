---
name: Kenvoy
description: 以清楚标识、可扫描记录和真实操作反馈组织的 React 原生 CSS 界面
colors:
  primary: "#1558ba"
  primary-dark: "#10458f"
  primary-light: "#e8f0fc"
  background: "#f4f6f9"
  paper: "#ffffff"
  text-primary: "#1e293b"
  text-secondary: "#526176"
  divider: "#dce2ea"
  table-head: "#f8fafc"
  success: "#237344"
  error: "#b42332"
  warning: "#945600"
  selection-background: "#d6e6ff"
  selection-text: "#153a6c"
  saved-row: "#e8f3ec"
typography:
  headline:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "1.875rem"
    fontWeight: 700
    lineHeight: 1.35
  section:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "1.375rem"
    fontWeight: 700
    lineHeight: 1.4
  title:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "1.125rem"
    fontWeight: 650
    lineHeight: 1.45
  body:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.65
  body-small:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.6
  label:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "0.875rem"
    fontWeight: 600
    lineHeight: 1.75
  caption:
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
    fontSize: "0.75rem"
    fontWeight: 400
    lineHeight: 1.66
rounded:
  button: "7px"
  control: "8px"
  surface: "12px"
spacing:
  "0.5": "4px"
  "1": "8px"
  "1.5": "12px"
  "2": "16px"
  "2.5": "20px"
  "3": "24px"
  "4": "32px"
  "5": "40px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.paper}"
    typography: "{typography.label}"
    rounded: "{rounded.button}"
    padding: "6px 18px"
  button-primary-hover:
    backgroundColor: "{colors.primary-dark}"
    textColor: "{colors.paper}"
  button-outlined:
    backgroundColor: "transparent"
    textColor: "{colors.primary}"
    typography: "{typography.label}"
    rounded: "{rounded.button}"
    padding: "5px 18px"
  button-text:
    backgroundColor: "transparent"
    textColor: "{colors.primary}"
    typography: "{typography.label}"
    rounded: "{rounded.button}"
    padding: "6px 18px"
  input-outlined:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.text-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "11px 13px"
  navigation-item:
    textColor: "{colors.text-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
  surface-form:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.surface}"
    padding: "20px"
  surface-form-wide:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.surface}"
    padding: "32px"
---

# Design System: Kenvoy

## Overview

**Creative North Star: "公共信息目录"**

界面用一致的标识、可扫描的记录和明确的操作反馈帮助用户完成登记与数据维护。蓝色主操作、深色文字、白色工作区与浅灰背景构成当前视觉系统；使用浏览器原生控件、系统中文字体和明确的可见标签。

信息层级服务于实际操作。字段、记录编号、错误原因和下一步动作应当可辨认；不使用没有产品依据的品牌素材、装饰图表或性能承诺。

**Key Characteristics:**

- 清楚的标题与业务字段，默认中文，并按项目词典提供原生多语言展示。
- 白色工作区、细边界与均匀间距。
- 蓝色主操作，状态色与明确文字共同表达结果。
- 桌面保留表格密度，窄屏保留可见的记录管理入口。

状态：现行。事实来源为 [theme.ts](crates/kenvoy/frontend/src/presentation/theme.ts)、[styles.css](crates/kenvoy/frontend/src/presentation/styles.css)、[ui.tsx](crates/kenvoy/frontend/src/presentation/ui.tsx)、[RuntimePage.tsx](crates/kenvoy/frontend/src/presentation/runtime/RuntimePage.tsx)、[ResourceForm.tsx](crates/kenvoy/frontend/src/presentation/forms/ResourceForm.tsx)、[FieldControl.tsx](crates/kenvoy/frontend/src/presentation/forms/FieldControl.tsx)、[ResourceTable.tsx](crates/kenvoy/frontend/src/presentation/records/ResourceTable.tsx)、[Admin.tsx](crates/kenvoy/frontend/src/presentation/Admin.tsx) 和 [App.tsx](crates/kenvoy/frontend/src/app/App.tsx)。默认组件使用 React、TypeScript 和原生 CSS，不依赖第三方 UI 库；theme.ts 只定义 CSS 变量，无 ThemeProvider。设计方向沿用 [PRODUCT.md](PRODUCT.md) 与[当前界面方向](.impeccable/surfaces/crates-kenvoy-frontend.md)，不引入新的跨项目规则。

阶段 06/07 的登记与后台截图保留为迁移前的视觉参考，不能替代原生组件迁移后的验收。此次迁移保留页面层级、颜色、间距、操作反馈及窄屏管理入口；新实现的功能与视觉证据以 [docs/verification/README.md](docs/verification/README.md) 为准。

## Colors

主色用于动作与焦点，中性色承担内容结构；成功、错误、警告各有语义，不能只靠颜色区分。

- **主操作蓝**：使用 `primary`，按钮悬停使用 `primary-dark`；`primary-light` 是现有主题中的浅色值。
- **浅灰背景与白色工作区**：分别使用 `background` 与 `paper`。
- **深色正文与辅助文字**：分别使用 `text-primary` 与 `text-secondary`；辅助文字包括标签、说明和表头。
- **边界与表头底色**：使用 `divider` 和 `table-head`，建立表格与页面背景的分界。
- **状态色**：`success`、`error`、`warning` 分别表示成功、确定的错误与待核对状态。
- **浏览器表面**：文字选择使用 `selection-background` 与 `selection-text`；插入光标使用主色。
- **保存定位**：`saved-row` 是更新行短暂淡出的起始底色。

原生 CSS 明确定义悬停、选中、禁用与提示背景。主题主色和辅助色沿用迁移前数值：辅助文字对白色为 6.31:1，主按钮白字为 6.69:1；这两项颜色计算不替代完整页面的对比度检查。

## Typography

所有层级使用前言中的同一系统字体栈。当前没有独立品牌字体或装饰性等宽字体。

- `headline` 对应页面 h1；`section` 对应分区 h2；`title` 对应导航和局部 h3。
- `body` 用于字段与正文；`body-small` 用于补充说明和紧凑列表；`label` 用于按钮；`caption` 用于帮助文字与核对编号。
- 表格数字使用等宽数字字形（tabular-nums），长值允许断行；数据单元格最大宽度为 360px。
- h1、h2 允许长单词或资源名换行。按钮保留原始文字大小写，不强制英文大写。

## Layout

间距保留 8px 基础步长及半步值，直接由原生 CSS 定义。表单字段之间为 24px；公共页面横向留白在窄屏为 16px、宽屏为 32px。

登记页容器最大宽度为 720px，含表格的普通页面为 1200px；后台总容器最大宽度为 1500px，桌面资源导航宽 210px、与内容间距 32px。表单工作区在窄屏使用 20px 内边距，宽屏使用 32px。顶部工具栏在窄屏至少 64px、宽屏至少 72px。

当前布局断点为 600px 与 900px。小于 600px 时，顶部导航收进菜单，表格搜索框独占一行，其余查询动作在下一行按可用宽度排布；小于 900px 时，后台资源导航改为抽屉入口。

多语言时顶部使用带可见标签的原生语言 select，与应用名、登录/账号操作同组；单语言时隐藏，不增设独立设置页。选择保留未提交状态，lang/dir 与标题同步更新，RTL 使用逻辑方向布局；语言不决定时区或改写数据。实现与桌面/窄屏证据见[原生国际化记录](docs/changes/0027-native-project-i18n.md)。

窄屏数据表仍使用表格：可横向滚动区域提供提示、名称和键盘焦点；操作列固定在右侧，显示“管理”按钮及详情、编辑、删除菜单。固定列宽 96px，管理按钮至少 72px 宽、44px 高。桌面继续显示直接操作按钮，不改为卡片列表。

## Elevation & Depth

页面和表单工作区以白色、浅灰底色与细边界区分层级；工作区、主按钮和顶部栏不使用阴影。确认对话框及抽屉使用原生 dialog.showModal，在浏览器顶层显示；浮层使用向下偏移 12px、模糊 40px 的阴影，与底层内容分离。

## Shapes

按钮使用轻微圆角，输入控件、提示与导航项采用统一控制圆角；表单容器、表格外框和对话框使用较大的表面圆角。对应值集中在前言的 `rounded`，不从截图估算另一套数值。

工作区边界为 1px；窄屏固定操作列使用 1px 左分界，帮助理解滚动内容与固定操作的位置。

## Components

### Buttons

主动作使用实心蓝色，次级动作使用描边或文字按钮。默认最小高度为 40px；窄屏行管理按钮和菜单项最小高度为 44px。默认水平内边距为 18px，尺寸变体继续遵守组件源码。

焦点使用主色 3px 外轮廓与 3px 偏移。禁用、加载和提交中的标签应保留真实状态；不能把结果待核对显示成可直接重试。

### Inputs / Fields

默认使用全宽原生 input、textarea、select 和 checkbox。标签位于控件上方，通过 htmlFor/id 建立关联；字段高度至少 48px。标签、禁用文字和 placeholder 保持辅助文字色，禁用值不降低为不可读的透明文本。帮助或错误位于对应字段下方，错误同时使用颜色、aria-invalid 和具体说明。

前端校验失败聚焦首个错误字段。成功后保留已提交内容和记录编号；结果未知时保留填写内容并禁止重复提交。此处记录源码行为，全部状态的运行覆盖仍以验证记录为准。

### Navigation

桌面顶部提供应用名、账号及导航，后台使用左侧资源列表。选中项使用浅蓝背景、深蓝文字及 aria-current，当前资源同时以页面标题表达。窄屏通过原生 dialog 抽屉维持可达性；浏览器约束模态焦点，Esc 可关闭，关闭后焦点返回触发按钮。处理中禁止关闭的对话框同步禁用取消、Esc 与背景关闭。

### Cards / Containers

白色工作区采用细边界和平面层次。表单外框承担字段分组，表格外框承担滚动与数据边界；不在每个字段或单元格外再套装饰容器。

### Tables / Feedback

表头使用浅底色、辅助文字色与较高字重；数据行上、下内边距各为 15px。排序、搜索、分页和记录操作延续表格结构，空列表与无匹配结果使用不同说明。

保存后使用文字通知与更新行的短暂底色，行底色在 1.8s 内以 ease-out 淡出。减少动态效果偏好下，将动画及过渡时长缩短为 0.01ms，并关闭平滑滚动。开发模式反馈的业务行为不在本次视觉结论中扩展声明。

### Development Feedback

仅开发模式显示编译失败与连接中断的原生 Notice，位置在现有主内容区域上方，复用状态色与正文排印。错误和警告使用 alert，成功或信息反馈使用 status。编译失败保留当前有效页面和填写内容，修复并成功发布后清除；普通生产页面不轮询该状态。协议和真实热更新的验证边界见 [前端 Runtime](docs/architecture/frontend-runtime.md)。

此前 detector 与 `.impeccable/design.json` 来自迁移前界面，保留为历史工具数据；当前事实以组件和 CSS 为准。本次不将旧扫描或样例当作新组件的验证结果。

## Do's and Don'ts

### Do:

- **Do** 用字段标签、记录编号和具体动作帮助定位内容。
- **Do** 保留可见键盘焦点、可读辅助文字与移动端记录管理入口。
- **Do** 让成功、错误和结果待核对同时具备文字说明与下一步动作。
- **Do** 从主题、组件源码和已验证截图更新设计文档。

### Don't:

- **Don't** 为普通登记和管理流程添加没有依据的图表、品牌素材或装饰纹理。
- **Don't** 用成功色替代真实结果，或对未知提交结果提供直接重试。
- **Don't** 将详情、编辑、删除的全部入口隐藏到窄屏初始视野之外。
- **Don't** 将静态设计样例视为真实业务或完整无障碍验收。
