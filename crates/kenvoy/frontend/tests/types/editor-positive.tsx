import { Component, memo, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';
import {
  ApiError, AttachmentList, Image, callAction, fileUrl, normalizeInput,
  useAction, useField, useForm, useI18n, useLocale, useResource, validateField, validateRecord,
} from '@kenvoy/client';
import type {
  FieldBinding, FieldDefinition, FormBinding, GroupBinding, ImageBinding,
  JsonValue, PageBinding, TableBinding,
} from '@kenvoy/client';
import type { FieldBinding as PackagedFieldBinding } from '@kenvoy/types/sdk';
import type { JSX as PackagedJsx } from '@kenvoy/types/jsx-runtime';
import type { JSX as PackagedDevJsx } from '@kenvoy/types/jsx-dev-runtime';
import type { JSX as VirtualJsx } from '@kenvoy/client/jsx-runtime';
import type { JSX as VirtualDevJsx } from '@kenvoy/client/jsx-dev-runtime';

export type DeclarationEntries = [
  PackagedFieldBinding,
  PackagedJsx.IntrinsicElements['number'],
  PackagedDevJsx.IntrinsicElements['svg'],
  VirtualJsx.IntrinsicElements['div'],
  VirtualDevJsx.IntrinsicElements['app'],
];

export function FieldView(binding: FieldBinding) {
  const current = useField(binding.field.name);
  return <input value={current.draft} disabled={binding.disabled}
    aria-invalid={Boolean(binding.error)} onChange={event => current.onDraftChange(event.currentTarget.value)} />;
}
export function FormView(binding: FormBinding) {
  const form = useForm();
  return <form onSubmit={form.submit} aria-busy={binding.busy}>{binding.children}</form>;
}
export function TableView(binding: TableBinding) {
  return <table><tbody>{binding.records.map(record => <tr key={record.id}><td>{record.id}</td></tr>)}</tbody></table>;
}
export function GroupView(binding: GroupBinding) { return <section aria-label={binding.label ?? undefined}>{binding.children}</section>; }
export function PageView(binding: PageBinding) { return <main aria-label={binding.page.title}>{binding.children}</main>; }
export function ImageView(binding: ImageBinding) { return <Image src={binding.src} alt={binding.alt} fit={binding.fit} />; }

export function StrictCard({ title, count, onSelect, children }: {
  title: string; count: number; onSelect: (value: number) => void; children?: ReactNode;
}) { return <button type="button" onClick={() => onSelect(count)}>{title}{children}</button>; }
const MemoCard = memo(StrictCard);
class ClassCard extends Component<{ title: string; children?: ReactNode }> {
  render() { return <aside>{this.props.title}{this.props.children}</aside>; }
}

export function ClientView() {
  const [total, setTotal] = useState(0);
  const ref = useRef<HTMLTextAreaElement>(null);
  const resource = useResource('entries');
  const run = useAction('calculate');
  const i18n = useI18n();
  const locale: string = useLocale();
  useEffect(() => {
    void resource.list({ page: 1, filters: { active: true } }).then(result => setTotal(result.total));
    void resource.get(1).then(record => setTotal(record.id));
    void resource.options('status', { dependencies: { active: true }, selected: 'open' });
  }, [resource]);
  const action: Promise<JsonValue> = callAction('calculate', [total]);
  void action;
  return <div lang={locale} aria-live="polite">
    <textarea ref={ref} value={String(total)} rows={2} onChange={event => setTotal(event.currentTarget.value.length)} />
    <select value="open" onChange={event => i18n.setLocale(event.currentTarget.value)}>
      <option value={1} disabled>普通 HTML 选项</option>
    </select>
    <form action="/search" method="get" onSubmit={event => event.preventDefault()}><input name="search" /></form>
    <table className="ordinary"><caption>普通 HTML 表格</caption><tbody><tr><td>{total}</td></tr></tbody></table>
    <svg viewBox="0 0 100 100" role="img" aria-label="普通 SVG">
      <text x={10} y="20" onClick={event => event.currentTarget.getBBox()}>SVG 文字</text>
      <image href="/logo.svg" x={10} y={30} width="60%" preserveAspectRatio="xMidYMid meet" />
      <circle cx={50} cy={50} r={10} fill="currentColor" />
    </svg>
    <MemoCard title={i18n.t('custom.title')} count={total} onSelect={value => { const next: number = value; setTotal(next); }}>
      <ClassCard title="子组件">保持 children 和组件 props</ClassCard>
    </MemoCard>
    <Image src="/logo.svg" alt="徽标" width={100} height={100} fit="contain" />
    <AttachmentList files={[]} onRemove={file => { const url: string = fileUrl(file, true); void url; }} />
    <button type="button" onClick={() => { void run([total]); }}>执行</button>
    {createPortal(<span>React DOM 类型</span>, document.body)}
  </div>;
}

export function checkSdk(field: FieldDefinition) {
  const error = new ApiError('校验失败', { status: 422, code: 'KV4001', fields: [] });
  const normalized: unknown = normalizeInput(field, '123');
  const result = validateField(field, normalized);
  if (result.ok) { const value: JsonValue = result.value; void value; }
  return validateRecord({ name: 'entries', fields: [field] }, {}, 'create').ok || error.status === 422;
}

export function calculate(amount: number) { return amount * 2; }
export default <app name="编辑器类型回归">
  <override name="select" component={FieldView} />
  <override name="file" component={FieldView} />
  <override name="form" component={FormView} />
  <override name="table" component={TableView} />
  <override name="group" component={GroupView} />
  <override name="page" component={PageView} />
  <override name="image" component={ImageView} />
  <resource name="cities"><text name="code" /><text name="name" /></resource>
  <resource name="regions"><text name="code" /><text name="name" /><text name="city" /></resource>
  <resource name="entries">
    <text name="title" label="标题" required maxLength={80} unique index default="初始标题" />
    <textarea name="note" label="备注" maxLength={500} index default={null} />
    <number name="amount" label="数量" integer min={0} max={100} unique index default={1} />
    <boolean name="active" required index default={true} />
    <date name="day" index default="2026-11-06" />
    <datetime name="at" index default="2026-11-06T00:00:00Z" />
    <select name="status" label="状态" required index default="open">
      <option value="open" label="开放" when={{ active: true }} />
      <option value="closed" label="关闭" />
    </select>
    <select name="city" optionsSource={{ resource: 'cities', value: 'code', label: 'name', permission: 'anonymous' }} />
    <select name="region" optionsSource={{ resource: 'regions', value: 'code', label: 'name', filters: { city: 'city' }, permission: 'user' }} />
    <file name="attachments" accept={['image/*', 'application/pdf'] as const} maxSize={1024} maxFiles={2} default={[]} />
  </resource>
  <page path="/entries" title="登记" permission="user">
    <group label="登记信息"><form resource="entries" mode="create" fields={['title', 'active', 'status'] as const} /></group>
    <table resource="entries" columns={['title', 'amount']} />
    <image src="/logo.svg" alt="徽标" width={100} height={100} fit="cover" />
    <ClientView />
  </page>
  <page path="/simple" resource="simple"><text name="name" /><number name="count" default={0} /></page>
  <action name="calculate" handler={calculate} runtime="shared" permission="user" />
  <navigation label="导航"><group label="登记"><item label="登记页面" path="/entries" /></group></navigation>
</app>;
