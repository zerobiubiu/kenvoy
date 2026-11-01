import { useEffect, useRef, useState } from 'react';
import { Link, Navigate, useNavigate, useParams, useSearchParams } from 'react-router';
import { ApiError } from '../application/errors';
import { RequestError } from './RequestError';
import { ResourceForm } from './forms/ResourceForm';
import { ResourceTable } from './records/ResourceTable';
import { RecordValueDisplay } from './records/RecordValueDisplay';
import { useRuntime } from './runtime/context';
import { canReadFile } from '../domain/files';
import { Loading, NativeDialog, Notice } from './ui';
import type { RecordData, ResourceDefinition } from '../domain/types';

function ResourceWorkspace({ resource }: { resource: ResourceDefinition }) {
  const { client, application } = useRuntime();
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
  return <div className="kv-stack spacious kv-resource-workspace">
    <div className="kv-workspace-heading">
      <div><h1 className="kv-title" ref={title} tabIndex={-1}>{heading}</h1>
        {listVisible && <p className="kv-muted kv-small kv-workspace-intro">查询和维护已保存的数据。</p>}
      </div>
      {listVisible ? <button className="kv-button primary" type="button" onClick={() => { setNotice(''); navigate(route + '?view=create'); }}>新增记录</button>
        : <button className="kv-button kv-self-start" type="button" onClick={back}>返回列表</button>}
    </div>
    {notice && <Notice kind="success" onClose={() => setNotice('')} role="status">{notice}</Notice>}
    <div hidden={!listVisible}>
      <ResourceTable resource={resource} highlightedId={highlighted} refreshToken={revision}
        onEdit={(value) => { setNotice(''); navigate(route + '?view=edit&id=' + value.id); }}
        onInspect={(value) => { setNotice(''); navigate(route + '?view=detail&id=' + value.id); }}
        onDelete={(value) => { setDeleting(value); setDeleteFailure(null); setDeleteUnknown(false); setDeleteInfo(''); }} />
    </div>
    {creating && <section className="kv-panel kv-record-panel">
      <ResourceForm key={resource.name + '-create'} resource={resource} onSaved={saved} onCancel={back} />
    </section>}
    {editing && <>
      {id === null ? <Notice kind="error">记录编号无效，请返回列表重新选择。</Notice>
        : failure != null ? <RequestError error={failure} retry={() => setReadAttempt((value) => value + 1)} />
        : busy || record?.id !== id ? <Loading height={320} label="正在加载记录" />
        : <section className="kv-panel kv-record-panel">
          {mode === 'edit' ? <ResourceForm key={resource.name + '-' + record.id} resource={resource} initial={record} mode="edit" onSaved={saved} onCancel={back} />
            : <div className="kv-stack spacious">
              <dl className="kv-details">
                {[{ name: 'id', label: '编号' }, ...resource.fields, { name: 'created_at', label: '创建时间（UTC）' }, { name: 'updated_at', label: '更新时间（UTC）' }].map((field) =>
                  <div key={field.name} className="kv-details-row">
                    <dt>{field.label || field.name}</dt>
                    <dd><RecordValueDisplay value={record[field.name]} field={resource.fields.find((item) => item.name === field.name)} readable={canReadFile(application, resource.name, field.name)} /></dd>
                  </div>)}
              </dl>
              <button className="kv-button outlined kv-self-start" type="button" onClick={() => navigate(route + '?view=edit&id=' + record.id)}>编辑这条记录</button>
            </div>}
        </section>}
    </>}
    <NativeDialog open={deleting !== null} onClose={closeDelete} closeDisabled={deleteBusy} title={'删除记录 ' + (deleting?.id ?? '') + '？'} className="kv-delete-dialog"
      footer={<>
        <button className="kv-button" type="button" onClick={closeDelete} disabled={deleteBusy} data-autofocus>{deleteUnknown ? '关闭并核对' : '取消'}</button>
        <button className="kv-button primary danger" type="button" disabled={deleteBusy || deleteUnknown} onClick={() => { void remove(); }}>{deleteBusy ? '正在处理…' : '确认删除'}</button>
      </>}>
      <div className="kv-stack">
        <p className="kv-muted">删除后无法在页面中撤销，请核对记录内容。</p>
        {deleting && <dl className="kv-delete-details">
          {resource.fields.slice(0, 3).map((field) => <div key={field.name}>
            <dt>{field.label || field.name}</dt><dd><RecordValueDisplay value={deleting[field.name]} field={field} compact readable={canReadFile(application, resource.name, field.name)} /></dd>
          </div>)}
        </dl>}
        {deleteFailure != null && <RequestError error={deleteFailure} />}
        {deleteInfo && <Notice kind="info">{deleteInfo}</Notice>}
        {deleteUnknown && <button className="kv-button outlined" type="button" onClick={() => { void reconcileDelete(); }} disabled={deleteBusy}>查询记录状态</button>}
      </div>
    </NativeDialog>
  </div>;
}

export default function Admin() {
  const { application, session } = useRuntime();
  const params = useParams();
  const [navigation, setNavigation] = useState(false);
  const resource = application.resources.find((item) => item.name === params.resource);
  useEffect(() => { setNavigation(false); }, [params.resource]);
  if (session.user?.role !== 'admin') return <div className="kv-stack kv-access-required">
    <h1 className="kv-title">需要管理员权限</h1>
    <Notice kind="warning">请使用管理员账号登录后查看和管理数据。</Notice>
    <Link className="kv-button outlined" to="/admin/login">前往登录</Link>
  </div>;
  if (!params.resource && application.resources[0]) return <Navigate replace to={'/admin/' + application.resources[0].name} />;
  const links = <nav className="kv-navigation" aria-label="管理资源">
    {application.resources.map((item) => <Link className="kv-navigation-link" key={item.name} to={'/admin/' + item.name}
      aria-current={item.name === params.resource ? 'page' : undefined} onClick={() => setNavigation(false)}>{item.name}</Link>)}
  </nav>;
  return <div className="kv-admin">
    <aside className="kv-sidebar">
      <h2 className="kv-sidebar-title">数据管理</h2>
      {links}<hr className="kv-divider" />
      <p className="kv-muted kv-small kv-sidebar-help">选择资源，查看和维护记录。</p>
    </aside>
    <div className="kv-stack spacious kv-admin-content">
      <button className="kv-button outlined kv-resource-switch" type="button" onClick={() => setNavigation(true)} aria-haspopup="dialog" aria-expanded={navigation}>选择数据资源</button>
      {resource ? <ResourceWorkspace key={resource.name} resource={resource} /> : <div className="kv-stack">
        <h1 className="kv-title">{params.resource ? '无法找到此资源' : '暂无可管理的数据'}</h1>
        <p className="kv-muted">{params.resource ? '请选择导航中的有效资源。' : '应用中尚未开放可管理的数据资源。'}</p>
      </div>}
    </div>
    <NativeDialog open={navigation} onClose={() => setNavigation(false)} title="数据管理" className="kv-drawer"
      footer={<button className="kv-button outlined" type="button" data-autofocus onClick={() => setNavigation(false)}>关闭</button>}>
      {links}
    </NativeDialog>
  </div>;
}
