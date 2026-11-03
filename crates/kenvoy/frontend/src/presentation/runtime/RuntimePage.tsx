import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router';
import { components, expressions, functions } from 'virtual:kenvoy-registry';
import { immutable } from '../../domain/options';
import type { ComponentDefinition, PageDefinition, RecordData, ResourceDefinition } from '../../domain/types';
import { ResourceForm } from '../forms/ResourceForm';
import { Image } from '../images';
import { ResourceTable } from '../records/ResourceTable';
import { RequestError } from '../RequestError';
import { Loading, Notice } from '../ui';
import type { GroupBinding, PageBinding } from './bindings';
import { useRuntime } from './context';
import { ComponentBoundary, OverrideView } from './overrides';

import { useI18n } from '../i18n';
function BoundForm({ node, resource }: { node: Extract<ComponentDefinition, { type: 'form' }>; resource: ResourceDefinition }) {
  const { t } = useI18n();
  const { client } = useRuntime();
  const [search] = useSearchParams();
  const rawId = search.get('id');
  const id = rawId && /^[1-9]\d*$/.test(rawId) && Number.isSafeInteger(Number(rawId)) ? Number(rawId) : null;
  const [record, setRecord] = useState<RecordData | null>(null);
  const [failure, setFailure] = useState<unknown>(null);
  const [busy, setBusy] = useState(node.mode === 'edit' && id !== null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (node.mode !== 'edit' || id === null) return;
    let current = true;
    setBusy(true); setFailure(null); setRecord(null);
    client.get(resource.name, id).then(result => { if (current) setRecord(result); })
      .catch(error => { if (current) setFailure(error); }).finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, node.mode, id, attempt]);
  if (node.mode === 'edit') {
    if (id === null) return <Notice>{t('ui.form.edit_link')}</Notice>;
    if (busy) return <Loading label={t('ui.form.loading_edit')} height={260} />;
    if (failure != null) return <RequestError error={failure} retry={() => setAttempt(value => value + 1)} />;
    if (!record) return null;
  }
  return <ResourceForm key={node.id + '-' + (record?.id ?? 'new')} resource={resource} formId={node.id} fields={node.fields} labels={node.labels} mode={node.mode} initial={record ?? undefined} />;
}

function RegisteredComponent({ node }: { node: Extract<ComponentDefinition, { type: 'custom' }> }) {
  const { t } = useI18n();
  const Custom = components[node.module];
  if (!Custom) return <Notice kind="error">{t('ui.component.unavailable')}</Notice>;
  const props = Object.fromEntries(Object.entries(node.props).map(([name, value]) => {
    if (value.kind === 'literal') return [name, value.value];
    if (Object.hasOwn(expressions, value.value)) return [name, expressions[value.value]()];
    if (Object.hasOwn(functions, value.value)) return [name, functions[value.value]];
    throw new Error('页面组件引用尚未登记');
  }));
  return <Custom {...props}>{node.children.map((child, index) => <RenderNode key={index} node={child} />)}</Custom>;
}

export function RenderNode({ node }: { node: ComponentDefinition }) {
  const { t } = useI18n();
  const { application } = useRuntime();
  if (node.type === 'text') return <p className="kv-text">{node.value}</p>;
  if (node.type === 'image') {
    const props = { src: node.src, alt: node.alt, width: node.width ?? undefined, height: node.height ?? undefined, fit: node.fit };
    const content = <Image {...props} />;
    return <OverrideView name="image" model={{ ...props, children: content }}>{content}</OverrideView>;
  }
  if (node.type === 'group') {
    const content = <section className="kv-stack">{node.label && <h2>{node.label}</h2>}{node.children.map((child, index) => <RenderNode key={index} node={child} />)}</section>;
    const model: GroupBinding = { label: node.label, children: content };
    return <OverrideView name="group" model={model}>{content}</OverrideView>;
  }
  if (node.type === 'custom') return <ComponentBoundary key={node.module}><RegisteredComponent node={node} /></ComponentBoundary>;
  const resource = application.resources.find(item => item.name === node.resource);
  if (!resource) return <Notice kind="error">{t('ui.resource.unavailable')}</Notice>;
  if (node.type === 'form') return <BoundForm resource={resource} node={node} />;
  if (node.type === 'table') return <ResourceTable resource={resource} columns={node.columns} />;
  return null;
}
export function RuntimePage({ page }: { page: PageDefinition }) {
  const hasTable = page.components.some(node => node.type === 'table');
  const content = <div className={'page-content' + (hasTable ? ' kv-page-wide' : '')}><h1>{page.title}</h1>
    <div className="kv-panel kv-page-panel kv-stack">{page.components.map((node, index) => <RenderNode key={page.path + '-' + index} node={node} />)}</div></div>;
  const model: PageBinding = { page: immutable(page), children: content };
  return <OverrideView name="page" model={model}>{content}</OverrideView>;
}
