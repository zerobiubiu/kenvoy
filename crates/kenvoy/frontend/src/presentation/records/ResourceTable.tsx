import { useEffect, useId, useState } from 'react';
import { canReadFile } from '../../domain/files';
import { immutable } from '../../domain/options';
import { normalizeInput, validateField } from '../../domain/validation';
import type { JsonScalar, RecordData, ResourceDefinition } from '../../domain/types';
import { FieldControl } from '../forms/FieldControl';
import type { TableBinding } from '../runtime/bindings';
import { useRuntime } from '../runtime/context';
import { OverrideView } from '../runtime/overrides';
import { RequestError } from '../RequestError';
import { Loading, NativeDialog } from '../ui';
import { displayValue, RecordValueDisplay } from './RecordValueDisplay';

export function ResourceTable({ resource, columns, onEdit, onInspect, onDelete, highlightedId, refreshToken = 0 }: {
  resource: ResourceDefinition; columns?: string[]; onEdit?: (record: RecordData) => void; onInspect?: (record: RecordData) => void;
  onDelete?: (record: RecordData) => void; highlightedId?: number; refreshToken?: number;
}) {
  const { client, application } = useRuntime();
  const [items, setItems] = useState<RecordData[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(10);
  const [search, setSearch] = useState('');
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState('id');
  const [direction, setDirection] = useState<'asc' | 'desc'>('desc');
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState<unknown>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [filterOpen, setFilterOpen] = useState(false);
  const filterable = resource.fields.filter(field => field.kind !== 'file');
  const [filterField, setFilterField] = useState(filterable[0]?.name ?? '');
  const [filterDraft, setFilterDraft] = useState('');
  const [filterError, setFilterError] = useState('');
  const [filters, setFilters] = useState<Record<string, JsonScalar>>({});
  const [actionRecord, setActionRecord] = useState<RecordData | null>(null);
  const tableId = useId();
  const filterDefinition = filterable.find(field => field.name === filterField);
  const hasConditions = Boolean(query) || Object.keys(filters).length > 0;
  const selected = resource.fields.filter(field => !columns || columns.includes(field.name));
  const actions = Boolean(onEdit || onInspect || onDelete);
  const cellCount = selected.length + 1 + Number(actions);
  useEffect(() => {
    let current = true;
    setBusy(true); setFailure(null);
    client.list(resource.name, { page: page + 1, page_size: pageSize, search: query, sort, descending: direction === 'desc', filters }).then(result => {
      if (!current) return;
      setItems(result.data); setTotal(result.total);
      if (page > 0 && result.data.length === 0) setPage(Math.max(0, Math.ceil(result.total / pageSize) - 1));
    }).catch(error => { if (current) setFailure(error); }).finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, page, pageSize, query, sort, direction, filters, refreshKey, refreshToken, highlightedId, application.generation]);
  const sortColumn = (name: string) => { setDirection(sort === name && direction === 'asc' ? 'desc' : 'asc'); setSort(name); setPage(0); };
  const chooseAction = (action: ((record: RecordData) => void) | undefined) => { if (actionRecord) { const record = actionRecord; setActionRecord(null); action?.(record); } };
  const refresh = () => setRefreshKey(value => value + 1);
  const content = <div className="kv-stack">
    <form className="kv-row kv-table-toolbar" onSubmit={event => { event.preventDefault(); setQuery(search); setPage(0); }}>
      <label className="kv-field kv-search"><span>搜索记录</span><input value={search} maxLength={256} onChange={event => setSearch(event.target.value)} /></label>
      <button type="submit" className="kv-button" disabled={busy}>搜索</button>
      {query && <button type="button" className="kv-button" onClick={() => { setSearch(''); setQuery(''); setPage(0); }}>清除搜索</button>}
      <button type="button" className="kv-button" onClick={() => setFilterOpen(value => !value)} disabled={filterable.length === 0} aria-expanded={filterOpen}>筛选</button>
      <button type="button" className="kv-button" onClick={refresh} disabled={busy}>刷新列表</button>
    </form>
    {filterOpen && <div className="kv-panel kv-stack"><div className="kv-row">
      <label className="kv-field"><span>筛选字段</span><select aria-label="筛选字段" value={filterField} onChange={event => { setFilterField(event.target.value); setFilterDraft(''); setFilterError(''); }}>
        {filterable.map(field => <option key={field.name} value={field.name}>{field.label || field.name}</option>)}</select></label>
      {filterDefinition && <FieldControl field={{ ...filterDefinition, label: '筛选值' }} resource={resource} values={filters} value={filterDraft} onChange={setFilterDraft} error={filterError} />}
    </div><div className="kv-row">
      <button type="button" className="kv-button" disabled={busy} onClick={() => {
        if (!filterDefinition) return;
        const result = validateField(filterDefinition, normalizeInput(filterDefinition, filterDraft));
        if (!result.ok) { setFilterError(result.errors.map(error => error.message).join('；')); return; }
        if (typeof result.value === 'object' && result.value !== null) return;
        setFilterError(''); setFilters(current => ({ ...current, [filterField]: result.value as JsonScalar })); setPage(0);
      }}>应用筛选</button>
      <button type="button" className="kv-button" onClick={() => { setFilters({}); setFilterDraft(''); setFilterError(''); setPage(0); }}>清除筛选</button>
    </div></div>}
    {Object.keys(filters).length > 0 && <p className="kv-hint" role="status">当前筛选：{Object.entries(filters).map(([name, value]) => (resource.fields.find(field => field.name === name)?.label || name) + ' = ' + displayValue(value, resource.fields.find(field => field.name === name))).join('，')}</p>}
    {failure != null && <RequestError error={failure} retry={refresh} />}
    <p id={tableId + '-scroll-hint'} className="kv-hint mobile-only">左右滑动查看完整字段{actions ? '，右侧可管理记录。' : '。'}</p>
    <div tabIndex={0} role="region" aria-label={resource.name + ' 数据表，可左右滚动'} aria-describedby={tableId + '-scroll-hint'} className="kv-table-scroll">
      <table className="kv-table" aria-label={resource.name + ' 数据列表'} aria-busy={busy}>
        <thead><tr>{[{ name: 'id', label: '编号' }, ...selected].map(field => <th key={field.name} aria-sort={sort === field.name ? direction === 'asc' ? 'ascending' : 'descending' : undefined}>
          {'kind' in field && field.kind === 'file' ? field.label || field.name : <button type="button" className="kv-sort" disabled={busy} onClick={() => sortColumn(field.name)}>{field.label || field.name}{sort === field.name && <svg aria-hidden="true" width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5"><path d={direction === 'asc' ? 'M9 14V4M5 8l4-4 4 4' : 'M9 4v10M5 10l4 4 4-4'} /></svg>}</button>}</th>)}
          {actions && <th className="kv-actions-cell">操作</th>}</tr></thead>
        <tbody>{busy ? <tr><td colSpan={cellCount}><Loading label="正在加载记录" /></td></tr>
          : failure ? <tr><td colSpan={cellCount} className="kv-empty">列表暂时无法加载，请查看上方提示。</td></tr>
          : items.length === 0 ? <tr><td colSpan={cellCount} className="kv-empty"><strong>{hasConditions ? '没有找到匹配的记录' : '还没有记录'}</strong><p>{hasConditions ? '试试其他关键词或筛选条件，或清除查询条件查看全部记录。' : '提交表单后，已保存的数据会显示在这里。'}</p></td></tr>
          : items.map(record => <tr key={record.id} className={record.id === highlightedId ? 'row-updated' : undefined}>
            <td className="data-cell">{record.id}</td>{selected.map(field => <td key={field.name} className="data-cell"><RecordValueDisplay value={record[field.name]} field={field} compact readable={canReadFile(application, resource.name, field.name)} /></td>)}
            {actions && <td className="kv-actions-cell"><div className="kv-row desktop-only">
              {onInspect && <button type="button" className="kv-button" onClick={() => onInspect(record)} aria-label={'查看记录 ' + record.id}>详情</button>}
              {onEdit && <button type="button" className="kv-button" onClick={() => onEdit(record)} aria-label={'编辑记录 ' + record.id}>编辑</button>}
              {onDelete && <button type="button" className="kv-button danger" onClick={() => onDelete(record)} aria-label={'删除记录 ' + record.id}>删除</button>}
            </div><button type="button" className="kv-button mobile-only" aria-label={'管理记录 ' + record.id} aria-haspopup="dialog" onClick={() => setActionRecord(record)}>管理</button></td>}
          </tr>)}</tbody>
      </table>
    </div>
    <NativeDialog open={Boolean(actionRecord)} title={'管理记录 ' + (actionRecord?.id ?? '')} onClose={() => setActionRecord(null)}>
      <div role="menu" className="kv-stack" aria-label="记录操作">
        {onInspect && <button type="button" role="menuitem" className="kv-button" aria-label={'查看记录 ' + actionRecord?.id} onClick={() => chooseAction(onInspect)}>详情</button>}
        {onEdit && <button type="button" role="menuitem" className="kv-button" aria-label={'编辑记录 ' + actionRecord?.id} onClick={() => chooseAction(onEdit)}>编辑</button>}
        {onDelete && <button type="button" role="menuitem" className="kv-button danger" aria-label={'删除记录 ' + actionRecord?.id} onClick={() => chooseAction(onDelete)}>删除</button>}
      </div>
    </NativeDialog>
    <div className="kv-pagination">
      <label>每页 <select aria-label="每页" value={pageSize} onChange={event => { setPageSize(Number(event.target.value)); setPage(0); }}>{[10, 20, 50].map(size => <option key={size} value={size}>{size}</option>)}</select></label>
      <span>{total ? page * pageSize + 1 : 0}–{Math.min((page + 1) * pageSize, total)} 条，共 {total} 条</span>
      <button type="button" className="kv-button" aria-label="上一页" disabled={busy || page === 0} onClick={() => setPage(value => value - 1)}>上一页</button>
      <button type="button" className="kv-button" aria-label="下一页" disabled={busy || (page + 1) * pageSize >= total} onClick={() => setPage(value => value + 1)}>下一页</button>
    </div>
  </div>;
  const binding: TableBinding = { resource: immutable(resource), records: immutable(items), columns: selected.map(field => field.name), busy, page: page + 1, pageSize, total, refresh, children: content };
  return <OverrideView name="table" model={binding}>{content}</OverrideView>;
}
