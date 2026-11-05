import { missingSdkExport } from '@kenvoy/client'; // @error unknown-sdk-export
import { RuntimeContext } from '@kenvoy/client'; // @error private-runtime-export
import { AttachmentList, Image, callAction, fileUrl, useField, useI18n, useResource } from '@kenvoy/client';
import type { FieldBinding } from '@kenvoy/client';

function StrictCard(props: { count: number; onSelect: (value: number) => void }) { return <span>{props.count}</span>; }
function FieldView(props: FieldBinding) { return <span>{props.draft}</span>; }
const resource = useResource('entries');
callAction(42); // @error action-name
callAction('calculate', { amount: 1 }); // @error action-args
resource.get('1'); // @error record-id
resource.list({ page: 'first' }); // @error list-query
resource.options('status', { dependencies: { active: [] } }); // @error options-query
resource.create({ title: () => '不能序列化' }); // @error record-value
useField(1); // @error field-name
useI18n().setLocale(42); // @error locale-code
fileUrl({ id: 'file' }); // @error uploaded-file
const sdkImage = <Image src="/a.png" alt="图片" fit="stretch" />; // @error sdk-image-props
const attachments = <AttachmentList files="file" />; // @error sdk-attachment-props
const wrongCard = <StrictCard count="1" onSelect={() => {}} />; // @error component-props
const wrongCallback = <StrictCard count={1} onSelect={(value: string) => value.toUpperCase()} />; // @error component-callback
const unknown = <unknownWidget />; // @error unknown-tag
const app = <app name="应用" title="不支持" />; // @error app-attribute
const page = <page path="/entries" permission="owner" />; // @error permission
const source = <resource name="entries" label="不支持" />; // @error resource-attribute
const text = <text name="title" maxLength="long" />; // @error text-constraint
const textarea = <textarea name="note" integer />; // @error textarea-constraint
const number = <number name="amount" min="0" />; // @error number-constraint
const numberDefault = <number name="amount" default="0" />; // @error number-default
const boolean = <boolean name="active" default="true" />; // @error boolean-default
const date = <date name="day" index="yes" />; // @error date-index
const datetime = <datetime name="at" default={new Date()} />; // @error datetime-default
const fileIndex = <file name="files" index />; // @error file-index
const fileDefault = <file name="files" default={[{ id: 'existing' }]} />; // @error file-default
const fileAccept = <file name="files" accept="image/*" />; // @error file-accept
const select = <select name="city" optionsSource={{ resource: 'cities', value: 'code', label: 'name', permission: 'owner' }} />; // @error select-source
const option = <option value="open" when={{ active: [] }} />; // @error option-condition
const formMode = <form resource="entries" mode="remove" />; // @error form-mode
const formFields = <form resource="entries" fields="title" />; // @error form-fields
const formResource = <form mode="create" />; // @error form-resource
const mixedForm = <form resource="entries" mode="create" onSubmit={() => {}} />; // @error mixed-intrinsic-props
const table = <table resource="entries" columns={[1]} />; // @error table-columns
const image = <image src="/a.png" alt="图片" fit="stretch" />; // @error image-fit
const actionHandler = <action name="calculate" handler="calculate" runtime="server" permission="admin" />; // @error action-handler
const actionRuntime = <action name="calculate" handler={() => 1} runtime="worker" permission="admin" />; // @error action-runtime
const navigation = <navigation path="/entries" />; // @error navigation-attribute
const item = <item path="/entries" />; // @error item-label
const group = <group unknown="属性" />; // @error group-attribute
const overrideName = <override name="unknown" component={FieldView} />; // @error override-name
const overrideProps = <override name="form" component={FieldView} />; // @error override-binding
const selectEvent = <select onChange={event => { const value: number = event.currentTarget.value; void value; }} />; // @error native-select-event
const textareaEvent = <textarea onChange={event => { const value: number = event.currentTarget.value; void value; }} />; // @error native-textarea-event
const nativeFormEvent = <form onSubmit={event => { const value: number = event.currentTarget.method; void value; }} />; // @error native-form-event
const nativeSvg = <svg><text x={{ invalid: true }}>坐标错误</text></svg>; // @error native-svg-props
const noChildren = <file name="files"><span>不可声明子节点</span></file>; // @error leaf-children
