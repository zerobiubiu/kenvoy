import { useEffect, useRef, useState } from 'react';
import { Link, Navigate, useNavigate, useParams, useSearchParams } from 'react-router';
import {
  Alert, Box, Button, Dialog, DialogActions, DialogContent, DialogContentText,
  DialogTitle, Divider, Drawer, List, ListItemButton, ListItemText, Paper, Skeleton,
  Stack, Typography,
} from '@mui/material';
import { ApiError } from './api';
import { RequestError, ResourceForm, ResourceTable, displayValue, useRuntime } from './Runtime';
import type { RecordData, ResourceDefinition } from './types';

function ResourceWorkspace({ resource }: { resource: ResourceDefinition }) {
  const { client } = useRuntime();
  const navigate = useNavigate();
  const [search] = useSearchParams();
  const mode = search.get('view');
  const editing = mode === 'edit' || mode === 'detail';
  const creating = mode === 'create';
  const listVisible = !editing && !creating;
  const rawId = search.get('id');
  const id = rawId && /^[1-9]\d*$/.test(rawId) && Number.isSafeInteger(Number(rawId)) ? Number(rawId) : null;
  const route = `/admin/${resource.name}`;
  const [record, setRecord] = useState<RecordData | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<unknown>(null);
  const [readAttempt, setReadAttempt] = useState(0);
  const [revision, setRevision] = useState(0);
  const [highlighted, setHighlighted] = useState<number>();
  const [notice, setNotice] = useState('');
  const [deleting, setDeleting] = useState<RecordData | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteFailure, setDeleteFailure] = useState<unknown>(null);
  const [deleteUnknown, setDeleteUnknown] = useState(false);
  const [deleteInfo, setDeleteInfo] = useState('');
  const title = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (!editing || id === null) return;
    let current = true;
    setBusy(true); setFailure(null); setRecord(null);
    client.get(resource.name, id).then((value) => { if (current) setRecord(value); })
      .catch((error) => { if (current) setFailure(error); })
      .finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [client, resource.name, editing, id, readAttempt]);
  const back = () => navigate(route);
  const saved = (value: RecordData) => {
    setHighlighted(value.id); setRevision((current) => current + 1);
    setNotice(`记录 ${value.id} ${creating ? '已新增' : '已更新'}。`);
    navigate(route);
    requestAnimationFrame(() => title.current?.focus());
  };
  const finishDelete = (deletedId: number) => {
    setDeleting(null); setDeleteFailure(null); setDeleteUnknown(false);
    setRevision((current) => current + 1); setNotice(`记录 ${deletedId} 已删除。`);
  };
  const remove = async () => {
    if (!deleting || deleteBusy || deleteUnknown) return;
    setDeleteBusy(true); setDeleteFailure(null);
    try { await client.delete(resource.name, deleting.id); finishDelete(deleting.id); }
    catch (error) { setDeleteFailure(error); setDeleteUnknown(error instanceof ApiError && error.outcomeUnknown); }
    finally { setDeleteBusy(false); }
  };
  const reconcileDelete = async () => {
    if (!deleting || deleteBusy) return;
    setDeleteBusy(true);
    try {
      const existing = await client.get(resource.name, deleting.id);
      setDeleting(existing); setDeleteUnknown(false); setDeleteFailure(null);
      setDeleteInfo(`已核对：记录 ${existing.id} 仍然存在，请核对内容后决定是否删除。`);
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) finishDelete(deleting.id);
      else setDeleteFailure(error);
    } finally { setDeleteBusy(false); }
  };
  const closeDelete = () => {
    if (deleteBusy) return;
    if (deleteUnknown) setRevision((current) => current + 1);
    setDeleting(null); setDeleteInfo('');
  };
  const heading = creating ? '新增记录' : mode === 'edit' ? `编辑记录 ${id ?? ''}` : mode === 'detail' ? `记录 ${id ?? ''} 的详情` : resource.name;
  return <Stack spacing={3} sx={{ minWidth: 0, flex: 1 }}>
    <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2} sx={{ justifyContent: 'space-between', alignItems: { sm: 'center' } }}>
      <Box><Typography component="h1" variant="h1" ref={title} tabIndex={-1}>{heading}</Typography>
        {listVisible && <Typography color="text.secondary" variant="body2" sx={{ mt: 0.75 }}>查询和维护已保存的数据。</Typography>}
      </Box>
      {listVisible ? <Button variant="contained" onClick={() => { setNotice(''); navigate(`${route}?view=create`); }}>新增记录</Button>
        : <Button onClick={back} sx={{ alignSelf: { xs: 'flex-start', sm: 'auto' } }}>返回列表</Button>}
    </Stack>
    {notice && <Alert severity="success" onClose={() => setNotice('')} role="status">{notice}</Alert>}
    <Box sx={{ display: listVisible ? 'block' : 'none' }}>
      <ResourceTable resource={resource} highlightedId={highlighted} refreshToken={revision}
        onEdit={(value) => { setNotice(''); navigate(`${route}?view=edit&id=${value.id}`); }}
        onInspect={(value) => { setNotice(''); navigate(`${route}?view=detail&id=${value.id}`); }}
        onDelete={(value) => { setDeleting(value); setDeleteFailure(null); setDeleteUnknown(false); setDeleteInfo(''); }} />
    </Box>
    {creating && <Paper sx={{ p: { xs: 2.5, sm: 4 }, maxWidth: 760, border: '1px solid', borderColor: 'divider', borderRadius: 1.5 }}>
      <ResourceForm key={`${resource.name}-create`} resource={resource} onSaved={saved} onCancel={back} />
    </Paper>}
    {editing && <>
      {id === null ? <Alert severity="error">记录编号无效，请返回列表重新选择。</Alert>
        : failure != null ? <RequestError error={failure} retry={() => setReadAttempt((value) => value + 1)} />
        : busy || record?.id !== id ? <Skeleton variant="rounded" height={320} aria-label="正在加载记录" />
        : <Paper sx={{ p: { xs: 2.5, sm: 4 }, maxWidth: 760, border: '1px solid', borderColor: 'divider', borderRadius: 1.5 }}>
          {mode === 'edit' ? <ResourceForm key={`${resource.name}-${record.id}`} resource={resource} initial={record} mode="edit" onSaved={saved} onCancel={back} />
            : <Stack spacing={3}>
              <Box component="dl" sx={{ m: 0, display: 'grid', gridTemplateColumns: { xs: '1fr', sm: 'minmax(120px, 1fr) 3fr' }, columnGap: 3, rowGap: { xs: 1, sm: 2 } }}>
                {[{ name: 'id', label: '编号' }, ...resource.fields, { name: 'created_at', label: '创建时间（UTC）' }, { name: 'updated_at', label: '更新时间（UTC）' }].map((field) =>
                  <Box key={field.name} sx={{ display: 'contents' }}>
                    <Typography component="dt" variant="body2" color="text.secondary">{field.label || field.name}</Typography>
                    <Typography component="dd" sx={{ m: 0, mb: { xs: 1.5, sm: 0 }, overflowWrap: 'anywhere', whiteSpace: 'pre-wrap', fontVariantNumeric: 'tabular-nums' }}>
                      {displayValue(record[field.name], resource.fields.find((item) => item.name === field.name))}
                    </Typography>
                  </Box>)}
              </Box>
              <Button variant="outlined" sx={{ alignSelf: 'flex-start' }} onClick={() => navigate(`${route}?view=edit&id=${record.id}`)}>编辑这条记录</Button>
            </Stack>}
        </Paper>}
    </>}
    <Dialog open={deleting !== null} onClose={closeDelete} fullWidth maxWidth="xs" aria-labelledby="delete-title">
      <DialogTitle id="delete-title">删除记录 {deleting?.id}？</DialogTitle>
      <DialogContent><Stack spacing={2}>
        <DialogContentText>删除后无法在页面中撤销，请核对记录内容。</DialogContentText>
        {deleting && <Box component="dl" sx={{ m: 0 }}>
          {resource.fields.slice(0, 3).map((field) => <Box key={field.name} sx={{ mb: 1.5 }}>
            <Typography component="dt" variant="body2" color="text.secondary">{field.label || field.name}</Typography>
            <Typography component="dd" sx={{ m: 0, overflowWrap: 'anywhere' }}>{displayValue(deleting[field.name], field)}</Typography>
          </Box>)}
        </Box>}
        {deleteFailure != null && <RequestError error={deleteFailure} />}
        {deleteInfo && <Alert severity="info">{deleteInfo}</Alert>}
        {deleteUnknown && <Button variant="outlined" onClick={() => { void reconcileDelete(); }} disabled={deleteBusy}>查询记录状态</Button>}
      </Stack></DialogContent>
      <DialogActions sx={{ p: 3, pt: 1 }}>
        <Button onClick={closeDelete} disabled={deleteBusy}>{deleteUnknown ? '关闭并核对' : '取消'}</Button>
        <Button color="error" variant="contained" disabled={deleteBusy || deleteUnknown} onClick={() => { void remove(); }}>{deleteBusy ? '正在处理…' : '确认删除'}</Button>
      </DialogActions>
    </Dialog>
  </Stack>;
}

export default function Admin() {
  const { application, session } = useRuntime();
  const params = useParams();
  const [navigation, setNavigation] = useState(false);
  const resource = application.resources.find((item) => item.name === params.resource);
  useEffect(() => { setNavigation(false); }, [params.resource]);
  if (session.user?.role !== 'admin') return <Stack spacing={2} sx={{ maxWidth: 640, mx: 'auto' }}>
    <Typography component="h1" variant="h1">需要管理员权限</Typography>
    <Alert severity="warning">请使用管理员账号登录后查看和管理数据。</Alert>
    <Button component={Link} to="/admin/login" variant="outlined">前往登录</Button>
  </Stack>;
  if (!params.resource && application.resources[0]) return <Navigate replace to={`/admin/${application.resources[0].name}`} />;
  const links = <List component="nav" aria-label="管理资源">
    {application.resources.map((item) => <ListItemButton key={item.name} component={Link} to={`/admin/${item.name}`}
      selected={item.name === params.resource} aria-current={item.name === params.resource ? 'page' : undefined}
      sx={{ borderRadius: 1, mb: 0.5 }}><ListItemText primary={item.name} slotProps={{ primary: { sx: { overflowWrap: 'anywhere' } } }} /></ListItemButton>)}
  </List>;
  return <Box sx={{ maxWidth: 1500, mx: 'auto', display: 'flex', gap: 4, alignItems: 'flex-start' }}>
    <Box component="aside" sx={{ width: 210, flexShrink: 0, display: { xs: 'none', md: 'block' }, position: 'sticky', top: 100 }}>
      <Typography variant="h3" component="h2" sx={{ mb: 2, px: 2 }}>数据管理</Typography>
      {links}<Divider sx={{ my: 2 }} />
      <Typography variant="body2" color="text.secondary" sx={{ px: 2 }}>选择资源，查看和维护记录。</Typography>
    </Box>
    <Stack spacing={3} sx={{ flex: 1, minWidth: 0 }}>
      <Button variant="outlined" onClick={() => setNavigation(true)} sx={{ display: { md: 'none' }, alignSelf: 'flex-start' }}>选择数据资源</Button>
      {resource ? <ResourceWorkspace key={resource.name} resource={resource} /> : <Stack spacing={2}>
        <Typography variant="h1" component="h1">{params.resource ? '无法找到此资源' : '暂无可管理的数据'}</Typography>
        <Typography color="text.secondary">{params.resource ? '请选择导航中的有效资源。' : '应用中尚未开放可管理的数据资源。'}</Typography>
      </Stack>}
    </Stack>
    <Drawer open={navigation} onClose={() => setNavigation(false)}>
      <Box sx={{ width: 280, maxWidth: '85vw', p: 2 }}>
        <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between', mb: 1 }}><Typography variant="h3" component="h2">数据管理</Typography><Button onClick={() => setNavigation(false)}>关闭</Button></Stack>
        {links}
      </Box>
    </Drawer>
  </Box>;
}
