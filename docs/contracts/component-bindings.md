# 组件重载、数据绑定与联动选项

状态：参考（现状文档）。系统字段、表单、后台及自定义数据组件共享 Application IR；选项与原生组件证据见[0012 变更记录](../changes/0012-native-components-and-options.md)，附件与图片见[0014 变更记录](../changes/0014-file-uploads-and-images.md)，既有阶段验收不自动覆盖新增能力。

## 静态与跨资源选项

静态选项的 `when` 使用本资源字段名称；对象条件同时成立时选项可用。远程 `optionsSource.filters` 的键是来源资源字段，值是本资源依赖字段。值保存为字符串，标签只用于显示。

```tsx
<resource name="customers">
  <select name="province" required>
    <option value="zj" label="浙江" />
    <option value="js" label="江苏" />
  </select>
  <select name="category">
    <option value="east" label="浙江服务" when={{ province: 'zj' }} />
    <option value="north" label="江苏服务" when={{ province: 'js' }} />
  </select>
  <select name="city" optionsSource={{
    resource: 'cities', value: 'code', label: 'name',
    filters: { province: 'province' }, permission: 'anonymous',
  }} />
</resource>
```

来源 `cities` 必须显式声明。value 使用 `id` 或 unique 文本列，label 使用文本或 select 列；依赖名称、类型、循环和公开表单字段范围在分析阶段检查。静态子 option 与 optionsSource 互斥。`permission` 是明确的选项投影授权，不开放来源资源的 CRUD。

创建表单角色必须满足来源要求，否则分析失败；编辑及其他 HTTP 写入也独立检查来源权限，不能借提交猜测受限选项。纯显示重载不改变这些规则。

联动依赖改变时清除失效的下游值，远程请求以当前依赖快照为准；过期响应不能覆盖新选项。加载、错误、无选项、检索及分页分别显示。编辑时单独解析已有选中值的标签，不受当前搜索页影响。

选项接口为 `POST /api/{resource}/options/{field}`，请求包含 `dependencies`、可选 `search/page/page_size/selected`，使用会话、CSRF、应用代际和表单策略。响应包含 `data`、`selected`、分页信息与 generation；只返回 value/label。提交时服务器在写事务内验证来源记录和依赖关系；PATCH 合并原记录后验证，不能仅改上游字段而遗留无效下游值。

`when` 和远程筛选是运行规则；仅修改这些规则不重写 SQLite 列或自动清理存量记录。静态枚举全集仍属于数据库约束，切换静态/远程方式须按实际 Schema Diff 处理。来源记录修改或删除后不会自动级联修改历史业务记录；后续写入会按新规则重验。

## 系统组件重载

```tsx
import { CustomSelect } from './components/CustomSelect';

export default <app name="客户系统">
  <override name="select" component={CustomSelect} />
  {/* resource 和 page 保持原生声明 */}
</app>;
```

可重载 text、textarea、number、boolean、date、datetime、select、file、image、form、table、group、page。重复目标、未知目标、非组件引用或触达服务端私有模块均拒绝。字段重载作用于使用该应用描述的公共页面和管理表单；image 重载接收图片展示参数及原生 children。

字段组件收到 `FieldBinding`：只读 `field/resource`、规范 `value`、有效性 `valid`、编辑文本 `draft`、`onChange/onDraftChange`、错误、禁用及选项状态。number 仍是数字，boolean 仍是布尔，select 仍是字符串；file 使用严格的 `UploadedFile[]` 或可选 null，不使用文本草稿。选择器不能提交 UI 库返回的整条对象，应显式映射为值。框架保留核心转换、验证、联动和提交，渲染失败由错误边界报告。

无效草稿保留在 `draft`，此时 `value=null`、`valid=false`，不能解释为合法清空；可空字段的显式 `null` 则为 `valid=true`。`values/valuesValidity` 提供表单其余字段的同一规范值与有效性，无效依赖不触发远程查询，也不匹配静态 `when: null`。提交仍检查原始草稿，避免将无效输入写成 null。

form/table/group/page 的重载是展示容器：接收原生状态和 `children`，应继续渲染 children，以保留框架提供的操作和字段。`FormBinding.values/validity` 同样区分规范值和有效性。重载不生成另一份 API 或数据库模型。

file 重载额外取得 `uploadState/upload/removeUpload/cancelUpload`，复用表单独占暂存和提交保护。`Image`、`AttachmentList` 与 `fileUrl` 同时从 SDK 导出；属性、限制、权限和文件生命周期统一见[文件与图片契约](file-uploads.md)。

## 自定义组件的数据接口

从 `@kenvoy/client` 导入公开类型及 Hooks：

- `useResource(name)`：只读资源定义、当前权限、验证过的 create/update、list/get/delete、options 以及 upload/removeUpload；服务端仍是授权与验证的最终依据。
- `useField()`：字段重载中的同一字段绑定，避免额外维护脱离表单的值。
- `useForm()`：资源表单状态；只在对应上下文使用。
- `useAction/callAction`：沿用服务端 Action 通道。
- `useI18n()`：框架原生语言状态、`t`、`formatNumber/formatDate/formatDateTime` 和语言切换；无需应用 Provider 或第三方国际化库，规则见[原生国际化](i18n.md)。

普通展示组件无需绑定资源；涉及业务数据的自定义组件使用这些接口，不复制字段 Schema。原生协议要求无法靠 TypeScript 阻止恶意浏览器脚本，真正的类型、授权与选项验证始终在 Rust 服务端执行。

资源和字段标签在显示层按原生键翻译；选项 `value`、远程选项业务标签、字段草稿和规范提交值不被翻译。自定义组件的任意 props 不自动当成文案，使用 `useI18n().t('custom.<键>', params, fallback)` 明确声明。重载继续消费同一绑定，不把本地化数字、日期或布尔标签写回业务值。


## React 组件库

应用可在自己的 package.json 声明兼容当前 React、可用于浏览器的组件库，固定版本并使用 `--install-deps` 显式准备依赖。框架不再内置 MUI/Emotion，也不强制应用选择某个替代库；[扩展示例](../../examples/extensions-app/README.md) 使用 Radix 作为用户依赖示范。默认 UI 没有第三方组件库依赖。
