import { Component, useEffect, useId, useRef, useState } from 'react';
import type { ErrorInfo, FormEvent, ReactNode } from 'react';
import { useSearchParams } from 'react-router';
import {
  Alert, AlertTitle, Box, Button, Menu, MenuItem, Paper, Skeleton, Stack,
  Table, TableBody, TableCell, TableContainer, TableHead, TablePagination, TableRow,
  TableSortLabel, TextField, Typography,
} from '@mui/material';
import { components, expressions, functions } from 'virtual:kenvoy-registry';
import { ApiError } from './api';
import { useRuntime } from './context';
export { RuntimeContext, useRuntime } from './context';
export type { RuntimeState } from './context';
import { normalizeInput, validateField } from './validation';
import type {
  ComponentDefinition, FieldDefinition, JsonScalar, JsonValue,
  PageDefinition, RecordData, ResourceDefinition,
} from './types';

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : '暂时无法完成操作，请稍后再试。';
}

export function RequestError({ error, retry }: { error: unknown; retry?: () => void }) {
  const uncertain = error instanceof ApiError && error.outcomeUnknown;
  return <Alert severity={uncertain ? 'warning' : 'error'} className="form-feedback" role="alert"
    action={retry && !uncertain ? <Button color="inherit" onClick={retry}>重新加载</Button> : undefined}>
    <AlertTitle>{uncertain ? '操作结果待核对' : '操作未完成'}</AlertTitle>
    {uncertain ? '网络响应中断，无法确认提交结果。请联系管理员核对，避免重复提交。' : errorMessage(error)}
    {error instanceof ApiError && error.request_id &&
      <Typography variant="caption" component="p" sx={{ mt: 0.5 }}>核对编号：{error.request_id}</Typography>}
  </Alert>;
}

function draftValue(field: FieldDefinition, value: unknown): string {
  if (value === null || value === undefined) return '';
  if (field.kind === 'boolean') return value === true ? 'true' : 'false';
  return String(value);
}

export function FieldControl({ field, value, onChange, error, disabled = false }: {
  field: FieldDefinition; value: string; onChange: (value: string) => void;
  error?: string; disabled?: boolean;
}) {
  const common = {
    name: field.name, label: field.label || field.name, required: field.required,
    value, onChange: (event: { target: { value: string } }) => onChange(event.target.value),
    disabled, error: Boolean(error), fullWidth: true,
  };
  if (field.kind === 'boolean') {
    return <TextField {...common} select helperText={error || '选择是或否；“否”也是有效值。'}>
      <MenuItem value="">请选择</MenuItem><MenuItem value="true">是</MenuItem><MenuItem value="false">否</MenuItem>
    </TextField>;
  }
  if (field.kind === 'select') {
    return <TextField {...common} select helperText={error}>
      {!field.options.some((option) => option.value === '') && <MenuItem value="">请选择</MenuItem>}
      {field.options.map((option) => <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>)}
    </TextField>;
  }
  if (field.kind === 'date') {
    return <TextField {...common} type="date" helperText={error}
      slotProps={{ inputLabel: { shrink: true }, htmlInput: { min: '0001-01-01', max: '9999-12-31' } }} />;
  }
  if (field.kind === 'datetime') {
    return <TextField {...common} placeholder="2026-11-01T09:00:00+08:00"
      helperText={error || '填写含时区的日期时间，例如 2026-11-01T09:00:00+08:00。'}
      slotProps={{ htmlInput: { spellCheck: false, autoCapitalize: 'off' } }} />;
  }
  if (field.kind === 'number') {
    const bounds = [field.integer ? '请输入整数' : '请输入数字', field.min != null ? `最小 ${field.min}` : '', field.max != null ? `最大 ${field.max}` : ''].filter(Boolean).join('，');
    return <TextField {...common} helperText={error || bounds}
      slotProps={{ htmlInput: { inputMode: field.integer ? 'numeric' : 'decimal' } }} />;
  }
  return <TextField {...common} multiline={field.kind === 'textarea'} minRows={field.kind === 'textarea' ? 4 : undefined}
    helperText={error || (field.max_length != null ? `最多 ${field.max_length} 个字符` : undefined)} />;
}

export interface ResourceFormProps {
  resource: ResourceDefinition;
  fields?: string[];
  labels?: Record<string, string>;
  formId?: string;
  initial?: RecordData;
  mode?: 'create' | 'edit';
  onSaved?: (record: RecordData) => void;
  onCancel?: () => void;
}

export function ResourceForm({ resource, fields, labels, formId, initial, mode = 'create', onSaved, onCancel }: ResourceFormProps) {
  const { client, refresh } = useRuntime();
  const selected = resource.fields.filter((field) => !fields || fields.includes(field.name));
  const makeValues = () => Object.fromEntries(selected.map((field) => [field.name, draftValue(field, initial ? initial[field.name] : field.default)]));
  const [values, setValues] = useState<Record<string, string>>(makeValues);
  const [dirty, setDirty] = useState<Set<string>>(() => new Set());
  const [unchanged, setUnchanged] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [failure, setFailure] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<RecordData | null>(null);
  const [uncertain, setUncertain] = useState(false);
  const [copyState, setCopyState] = useState('');
  const form = useRef<HTMLFormElement>(null);
  const feedback = useRef<HTMLDivElement>(null);

  const focusFirst = (names: string[]) => {
    requestAnimationFrame(() => {
      for (const name of names) {
        const input = form.current?.elements.namedItem(name);
        if (input instanceof HTMLElement) {
          const control = input.closest('.MuiFormControl-root')?.querySelector<HTMLElement>('[role="combobox"]') ?? input;
          control.focus(); break;
        }
      }
    });
  };
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || saved || uncertain) return;
    if (mode === 'edit' && !initial) { setFailure(new Error('请先选择需要编辑的记录。')); return; }
    if (mode === 'edit' && dirty.size === 0) { setUnchanged(true); return; }
    const nextErrors: Record<string, string> = {};
    const input: Record<string, JsonValue> = {};
    for (const field of selected) {
      if (mode === 'edit' && !dirty.has(field.name)) continue;
      const touched = dirty.has(field.name);
      const value = touched ? normalizeInput(field, values[field.name] ?? '') : field.default ?? null;
      const result = validateField(field, value);
      if (result.ok) { if (touched) input[field.name] = result.value; }
      else nextErrors[field.name] = result.errors.map((error) => error.message).join('；');
    }
    setErrors(nextErrors); setFailure(null);
    if (Object.keys(nextErrors).length) { focusFirst(Object.keys(nextErrors)); return; }
    setBusy(true);
    try {
      const record = mode === 'edit'
        ? await client.update(resource.name, Number(initial!.id), input)
        : await client.create(resource.name, input, formId);
      setSaved(record);
      onSaved?.(record);
      requestAnimationFrame(() => feedback.current?.focus());
    } catch (error) {
      setFailure(error);
      if (error instanceof ApiError) {
        setUncertain(error.outcomeUnknown);
        const serverErrors: Record<string, string> = {};
        for (const item of error.fields ?? []) if (item.field) serverErrors[item.field] = item.message;
        setErrors(serverErrors); focusFirst(Object.keys(serverErrors));
      }
    } finally { setBusy(false); }
  };

  return <Stack component="form" ref={form} noValidate onSubmit={submit} spacing={3} aria-busy={busy}>
    {mode === 'edit' && !initial
      ? <Alert severity="info">请先从数据列表选择需要编辑的记录。</Alert>
      : <>
        <Box className="field-grid">
          {selected.map((field) => <FieldControl key={field.name} field={{ ...field, label: labels?.[field.name] ?? field.label }}
            value={values[field.name] ?? ''} disabled={busy || Boolean(saved) || uncertain} error={errors[field.name]}
            onChange={(value) => {
              setValues((current) => ({ ...current, [field.name]: value }));
              setDirty((current) => new Set(current).add(field.name)); setUnchanged(false);
              setErrors((current) => ({ ...current, [field.name]: '' }));
            }} />)}
        </Box>
        {failure != null && <RequestError error={failure} />}
        {unchanged && <Alert severity="info">内容没有变化，无需保存。</Alert>}
        {failure instanceof ApiError && !uncertain && [401, 403, 409].includes(failure.status) &&
          <Button onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }} variant="outlined">刷新登录与页面状态</Button>}
        {saved && <Box ref={feedback} tabIndex={-1} className="form-feedback">
          <Alert severity="success" role="status"><AlertTitle>{mode === 'edit' ? '修改已保存' : '提交成功'}</AlertTitle>
            记录编号：{String(saved.id)}。{mode === 'create' ? '您的信息已保存。' : '记录已更新。'}
          </Alert>
        </Box>}
        <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1.5} sx={{ pt: 0.5 }}>
          {!saved && <Button type="submit" variant="contained" disabled={busy || uncertain}>
            {busy ? '正在保存…' : mode === 'edit' ? '保存修改' : '提交信息'}
          </Button>}
          {saved && !onSaved && mode === 'create' && <Button variant="outlined" onClick={() => {
            setValues(makeValues()); setDirty(new Set()); setUnchanged(false); setSaved(null); setErrors({}); setCopyState(''); focusFirst(selected.map((field) => field.name));
          }}>再登记一条</Button>}
          {onCancel && <Button onClick={onCancel} disabled={busy}>{saved ? '返回列表' : '取消'}</Button>}
          {uncertain && <Button variant="outlined" onClick={() => {
            void navigator.clipboard.writeText(JSON.stringify(values, null, 2)).then(() => setCopyState('填写内容已复制，可交给管理员核对。')).catch(() => setCopyState('无法访问剪贴板，请保留当前页面并联系管理员。'));
          }}>复制填写内容</Button>}
        </Stack>
        {copyState && <Typography role="status" variant="body2" color="text.secondary">{copyState}</Typography>}
      </>}
  </Stack>;
}

export function displayValue(value: unknown, field?: FieldDefinition): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean') return value ? '是' : '否';
  if (field?.kind === 'select') return field.options.find((option) => option.value === value)?.label ?? String(value);
  return String(value);
}

export function ResourceTable({ resource, columns, onEdit, onInspect, onDelete, highlightedId, refreshToken = 0 }: {
  resource: ResourceDefinition; columns?: string[];
  onEdit?: (record: RecordData) => void; onInspect?: (record: RecordData) => void;
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
  const [filterField, setFilterField] = useState(resource.fields[0]?.name ?? '');
  const [filterDraft, setFilterDraft] = useState('');
  const [filterError, setFilterError] = useState('');
  const [filters, setFilters] = useState<Record<string, JsonScalar>>({});
  const [actionMenu, setActionMenu] = useState<{ anchor: HTMLElement; record: RecordData } | null>(null);
  const tableId = useId();
  const menuId = `${tableId}-actions`;
  const actionButtonId = (id: unknown) => `${tableId}-manage-${String(id)}`;
  const filterDefinition = resource.fields.find((field) => field.name === filterField);
  const hasConditions = Boolean(query) || Object.keys(filters).length > 0;
  const selected = resource.fields.filter((field) => !columns || columns.includes(field.name));
  const actions = Boolean(onEdit || onInspect || onDelete);
  const cellCount = selected.length + 1 + Number(actions);
  useEffect(() => {
    let current = true;
    setBusy(true); setFailure(null);
    client.list(resource.name, { page: page + 1, page_size: pageSize, search: query, sort, descending: direction === 'desc', filters }).then((result) => {
      if (!current) return;
      setItems(result.data); setTotal(result.total);
      if (page > 0 && result.data.length === 0) setPage(Math.max(0, Math.ceil(result.total / pageSize) - 1));
    }).catch((error) => { if (current) setFailure(error); }).finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, page, pageSize, query, sort, direction, filters, refreshKey, refreshToken, highlightedId, application.generation]);
  const sortColumn = (name: string) => {
    setDirection(sort === name && direction === 'asc' ? 'desc' : 'asc'); setSort(name); setPage(0);
  };
  const chooseAction = (action: ((record: RecordData) => void) | undefined) => {
    if (!actionMenu) return;
    const { record } = actionMenu;
    setActionMenu(null);
    action?.(record);
  };
  const actionCell = {
    position: { xs: 'sticky', sm: 'static' }, right: 0, zIndex: 1,
    backgroundColor: 'background.paper', width: { xs: 96, sm: 'auto' },
    px: { xs: 1, sm: 2 }, borderLeft: { xs: '1px solid', sm: 'none' }, borderLeftColor: 'divider',
  } as const;
  return <Stack spacing={2}>
    <Box component="form" sx={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 1.5 }} onSubmit={(event) => { event.preventDefault(); setQuery(search); setPage(0); }}>
      <TextField size="small" label="搜索记录" value={search} onChange={(event) => setSearch(event.target.value)}
        slotProps={{ htmlInput: { maxLength: 256 } }} sx={{ flex: { xs: '1 0 100%', sm: '0 1 360px' }, maxWidth: { sm: 360 } }} />
      <Button type="submit" variant="outlined" disabled={busy}>搜索</Button>
      {query && <Button onClick={() => { setSearch(''); setQuery(''); setPage(0); }}>清除搜索</Button>}
      <Button onClick={() => setFilterOpen((value) => !value)} aria-expanded={filterOpen}>筛选</Button>
      <Button onClick={() => setRefreshKey((value) => value + 1)} disabled={busy} sx={{ ml: { sm: 'auto' } }}>刷新列表</Button>
    </Box>
    {filterOpen && <Stack spacing={2} sx={{ bgcolor: 'background.paper', p: 2, border: '1px solid', borderColor: 'divider', borderRadius: 1.5 }}>
      <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
        <TextField select label="筛选字段" value={filterField} sx={{ maxWidth: { sm: 240 } }} onChange={(event) => { setFilterField(event.target.value); setFilterDraft(''); setFilterError(''); }}>
          {resource.fields.map((field) => <MenuItem key={field.name} value={field.name}>{field.label || field.name}</MenuItem>)}
        </TextField>
        {filterDefinition && <FieldControl field={{ ...filterDefinition, label: '筛选值' }} value={filterDraft} onChange={setFilterDraft} error={filterError} />}
      </Stack>
      <Stack direction="row" spacing={1}>
        <Button variant="outlined" disabled={busy} onClick={() => {
          if (!filterDefinition) return;
          const result = validateField(filterDefinition, normalizeInput(filterDefinition, filterDraft));
          if (!result.ok) { setFilterError(result.errors.map((error) => error.message).join('；')); return; }
          if (typeof result.value === 'object' && result.value !== null) return;
          setFilterError(''); setFilters({ [filterField]: result.value }); setPage(0);
        }}>应用筛选</Button>
        <Button onClick={() => { setFilters({}); setFilterDraft(''); setFilterError(''); setPage(0); }}>清除筛选</Button>
      </Stack>
    </Stack>}
    {Object.keys(filters).length > 0 && <Typography variant="body2" color="text.secondary" role="status">
      当前筛选：{Object.entries(filters).map(([name, value]) => `${resource.fields.find((field) => field.name === name)?.label || name} = ${displayValue(value, resource.fields.find((field) => field.name === name))}`).join('，')}
    </Typography>}
    {failure != null && <RequestError error={failure} retry={() => setRefreshKey((value) => value + 1)} />}
    <Typography id={`${tableId}-scroll-hint`} variant="body2" color="text.secondary" sx={{ display: { xs: 'block', sm: 'none' } }}>
      左右滑动查看完整字段{actions ? '，右侧可管理记录。' : '。'}
    </Typography>
    <TableContainer tabIndex={0} role="region" aria-label={`${resource.name} 数据表，可左右滚动`} aria-describedby={`${tableId}-scroll-hint`}
      sx={{ border: '1px solid', borderColor: 'divider', borderRadius: 1.5, backgroundColor: 'background.paper' }}>
      <Table aria-label={`${resource.name} 数据列表`} aria-busy={busy}>
        <TableHead><TableRow>
          {[{ name: 'id', label: '编号' }, ...selected].map((field) => <TableCell key={field.name} sortDirection={sort === field.name ? direction : false}>
            <TableSortLabel disabled={busy} active={sort === field.name} direction={sort === field.name ? direction : 'asc'} onClick={() => sortColumn(field.name)}>{field.label || field.name}</TableSortLabel>
          </TableCell>)}
          {actions && <TableCell align="right" sx={{ ...actionCell, bgcolor: '#f8fafc', zIndex: 2 }}>操作</TableCell>}
        </TableRow></TableHead>
        <TableBody>
          {busy ? Array.from({ length: 4 }, (_, index) => <TableRow key={index}><TableCell colSpan={cellCount}><Skeleton height={25} /></TableCell></TableRow>)
            : failure ? <TableRow><TableCell colSpan={cellCount}><Typography color="text.secondary" sx={{ py: 3 }}>列表暂时无法加载，请查看上方提示。</Typography></TableCell></TableRow>
            : items.length === 0 ? <TableRow><TableCell colSpan={cellCount} sx={{ py: 6, textAlign: 'center' }}>
              <Typography sx={{ fontWeight: 600 }}>{hasConditions ? '没有找到匹配的记录' : '还没有记录'}</Typography>
              <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>{hasConditions ? '试试其他关键词或筛选条件，或清除查询条件查看全部记录。' : '提交表单后，已保存的数据会显示在这里。'}</Typography>
            </TableCell></TableRow> : items.map((record) => <TableRow key={String(record.id)} className={record.id === highlightedId ? 'row-updated' : undefined} hover>
              <TableCell className="data-cell">{String(record.id)}</TableCell>
              {selected.map((field) => <TableCell key={field.name} className="data-cell">{displayValue(record[field.name], field)}</TableCell>)}
              {actions && <TableCell align="right" sx={actionCell}><Stack direction="row" spacing={0.5} sx={{ display: { xs: 'none', sm: 'flex' }, justifyContent: 'flex-end' }}>
                {onInspect && <Button size="small" onClick={() => onInspect(record)} aria-label={`查看记录 ${record.id}`}>详情</Button>}
                {onEdit && <Button size="small" onClick={() => onEdit(record)} aria-label={`编辑记录 ${record.id}`}>编辑</Button>}
                {onDelete && <Button size="small" color="error" onClick={() => onDelete(record)} aria-label={`删除记录 ${record.id}`}>删除</Button>}
              </Stack><Button id={actionButtonId(record.id)} size="small" variant="outlined" sx={{ display: { xs: 'inline-flex', sm: 'none' }, minHeight: 44, minWidth: 72, px: 1.5 }}
                aria-label={`管理记录 ${record.id}`} aria-haspopup="menu" aria-expanded={actionMenu?.record.id === record.id}
                aria-controls={actionMenu?.record.id === record.id ? menuId : undefined}
                onClick={(event) => setActionMenu({ anchor: event.currentTarget, record })}>管理</Button></TableCell>}
            </TableRow>)}
        </TableBody>
      </Table>
    </TableContainer>
    <Menu id={menuId} anchorEl={actionMenu?.anchor ?? null} open={Boolean(actionMenu)} onClose={() => setActionMenu(null)}
      slotProps={{ list: { 'aria-labelledby': actionMenu ? actionButtonId(actionMenu.record.id) : undefined }, paper: { sx: { minWidth: 160 } } }}>
      {onInspect && <MenuItem sx={{ minHeight: 44 }} aria-label={`查看记录 ${actionMenu?.record.id}`} onClick={() => chooseAction(onInspect)}>详情</MenuItem>}
      {onEdit && <MenuItem sx={{ minHeight: 44 }} aria-label={`编辑记录 ${actionMenu?.record.id}`} onClick={() => chooseAction(onEdit)}>编辑</MenuItem>}
      {onDelete && <MenuItem sx={{ minHeight: 44, color: 'error.main' }} aria-label={`删除记录 ${actionMenu?.record.id}`} onClick={() => chooseAction(onDelete)}>删除</MenuItem>}
    </Menu>
    <TablePagination component="div" count={total} page={page} rowsPerPage={pageSize}
      onPageChange={(_, value) => setPage(value)} onRowsPerPageChange={(event) => { setPageSize(Number(event.target.value)); setPage(0); }}
      rowsPerPageOptions={[10, 20, 50]} labelRowsPerPage="每页" labelDisplayedRows={({ from, to, count }) => `${from}–${to} 条，共 ${count} 条`}
      getItemAriaLabel={(type) => ({ first: '第一页', last: '最后一页', next: '下一页', previous: '上一页' })[type]}
      sx={{ '& .MuiTablePagination-toolbar': { flexWrap: 'wrap', px: 0 }, '& .MuiTablePagination-spacer': { display: { xs: 'none', sm: 'block' } } }} />
  </Stack>;
}

class ComponentBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(_error: Error, _info: ErrorInfo) { /* 用户组件异常只影响当前区域。 */ }
  render() { return this.state.failed ? <Alert severity="error">此页面组件暂时无法显示，请联系应用管理员。</Alert> : this.props.children; }
}

function BoundForm({ node, resource }: { node: Extract<ComponentDefinition, { type: 'form' }>; resource: ResourceDefinition }) {
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
    client.get(resource.name, id).then((result) => { if (current) setRecord(result); })
      .catch((error) => { if (current) setFailure(error); })
      .finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, node.mode, id, attempt]);
  if (node.mode === 'edit') {
    if (id === null) return <Alert severity="info">请从数据列表选择记录，或使用包含有效记录编号的编辑链接。</Alert>;
    if (busy) return <Skeleton variant="rounded" height={260} aria-label="正在加载编辑记录" />;
    if (failure != null) return <RequestError error={failure} retry={() => setAttempt((value) => value + 1)} />;
    if (!record) return null;
  }
  return <ResourceForm key={`${node.id}-${record?.id ?? 'new'}`} resource={resource} formId={node.id}
    fields={node.fields} labels={node.labels} mode={node.mode} initial={record ?? undefined} />;
}

function RegisteredComponent({ node }: { node: Extract<ComponentDefinition, { type: 'custom' }> }) {
  const Custom = components[node.module];
  if (!Custom) return <Alert severity="error">页面组件尚未准备完成，请联系应用管理员。</Alert>;
  const props = Object.fromEntries(Object.entries(node.props).map(([name, value]) => {
    if (value.kind === 'literal') return [name, value.value];
    if (Object.hasOwn(expressions, value.value)) return [name, expressions[value.value]()];
    if (Object.hasOwn(functions, value.value)) return [name, functions[value.value]];
    throw new Error('页面组件引用尚未登记');
  }));
  return <Custom {...props}>{node.children.map((child, index) => <RenderNode key={index} node={child} />)}</Custom>;
}

export function RenderNode({ node }: { node: ComponentDefinition }) {
  const { application } = useRuntime();
  if (node.type === 'text') return <Typography sx={{ whiteSpace: 'pre-wrap' }}>{node.value}</Typography>;
  if (node.type === 'group') return <Stack spacing={3} component="section">{node.label && <Typography variant="h2" component="h2">{node.label}</Typography>}{node.children.map((child, index) => <RenderNode key={index} node={child} />)}</Stack>;
  if (node.type === 'custom') {
    return <ComponentBoundary key={node.module}><RegisteredComponent node={node} /></ComponentBoundary>;
  }
  const resource = application.resources.find((item) => item.name === node.resource);
  if (!resource) return <Alert severity="error">此页面的数据资源暂不可用，请刷新页面或联系管理员。</Alert>;
  if (node.type === 'form') return <BoundForm resource={resource} node={node} />;
  if (node.type === 'table') return <ResourceTable resource={resource} columns={node.columns} />;
  return null;
}

export function RuntimePage({ page }: { page: PageDefinition }) {
  const hasTable = page.components.some((node) => node.type === 'table');
  return <Box className="page-content" sx={{ maxWidth: hasTable ? 1200 : 720, mx: 'auto' }}>
    <Box sx={{ mb: 3 }}><Typography variant="h1" component="h1">{page.title}</Typography></Box>
    <Paper sx={{ p: { xs: 2.5, sm: 4 }, border: '1px solid', borderColor: 'divider', borderRadius: 1.5 }}>
      <Stack spacing={4}>{page.components.map((node, index) => <RenderNode key={`${page.path}-${index}`} node={node} />)}</Stack>
    </Paper>
  </Box>;
}
