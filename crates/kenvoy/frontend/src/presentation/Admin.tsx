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
import { useI18n } from './i18n';
import { translateError } from '../domain/i18n/error';
import type { LocalizedMessage } from '../domain/i18n/error';

function ResourceWorkspace({ resource }: { resource: ResourceDefinition }) {
  const { t } = useI18n();
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
  const [notice, setNotice] = useState<LocalizedMessage | null>(null);
  const [deleting, setDeleting] = useState<RecordData | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteFailure, setDeleteFailure] = useState<unknown>(null);
  const [deleteUnknown, setDeleteUnknown] = useState(false);
  const [deleteInfo, setDeleteInfo] = useState<LocalizedMessage | null>(null);
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
    setNotice({ message: '', message_key: creating ? 'ui.admin.created' : 'ui.admin.updated', params: { id: value.id } });
    navigate(route);
    requestAnimationFrame(() => title.current?.focus());
  };
  const finishDelete = (deletedId: number) => {
    setDeleting(null); setDeleteFailure(null); setDeleteUnknown(false);
    setRevision((current) => current + 1); setNotice({ message: '', message_key: 'ui.admin.deleted', params: { id: deletedId } });
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
      setDeleteInfo({ message: '', message_key: 'ui.admin.still_exists', params: { id: existing.id } });
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) finishDelete(deleting.id);
      else setDeleteFailure(error);
    } finally { setDeleteBusy(false); }
  };
  const closeDelete = () => {
    if (deleteBusy) return;
    if (deleteUnknown) setRevision((current) => current + 1);
    setDeleting(null); setDeleteInfo(null);
  };
  const heading = creating ? t('ui.admin.create') : mode === 'edit' ? t('ui.admin.edit', { id: id ?? '' }) : mode === 'detail' ? t('ui.admin.detail', { id: id ?? '' }) : resource.label ?? resource.name;
  return <div className="kv-stack spacious kv-resource-workspace">
    <div className="kv-workspace-heading">
      <div><h1 className="kv-title" ref={title} tabIndex={-1}>{heading}</h1>
        {listVisible && <p className="kv-muted kv-small kv-workspace-intro">{t('ui.admin.intro')}</p>}
      </div>
      {listVisible ? <button className="kv-button primary" type="button" onClick={() => { setNotice(null); navigate(route + '?view=create'); }}>{t('ui.admin.create')}</button>
        : <button className="kv-button kv-self-start" type="button" onClick={back}>{t('ui.form.back')}</button>}
    </div>
    {notice && <Notice kind="success" onClose={() => setNotice(null)} role="status">{translateError(notice, t)}</Notice>}
    <div hidden={!listVisible}>
      <ResourceTable resource={resource} highlightedId={highlighted} refreshToken={revision}
        onEdit={(value) => { setNotice(null); navigate(route + '?view=edit&id=' + value.id); }}
        onInspect={(value) => { setNotice(null); navigate(route + '?view=detail&id=' + value.id); }}
        onDelete={(value) => { setDeleting(value); setDeleteFailure(null); setDeleteUnknown(false); setDeleteInfo(null); }} />
    </div>
    {creating && <section className="kv-panel kv-record-panel">
      <ResourceForm key={resource.name + '-create'} resource={resource} onSaved={saved} onCancel={back} />
    </section>}
    {editing && <>
      {id === null ? <Notice kind="error">{t('ui.admin.invalid_id')}</Notice>
        : failure != null ? <RequestError error={failure} retry={() => setReadAttempt((value) => value + 1)} />
        : busy || record?.id !== id ? <Loading height={320} label={t('ui.table.loading')} />
        : <section className="kv-panel kv-record-panel">
          {mode === 'edit' ? <ResourceForm key={resource.name + '-' + record.id} resource={resource} initial={record} mode="edit" onSaved={saved} onCancel={back} />
            : <div className="kv-stack spacious">
              <dl className="kv-details">
                {[{ name: 'id', label: t('ui.record.id') }, ...resource.fields,
                  { name: 'created_at', label: t('ui.admin.created_at'), kind: 'datetime' as const, options: [] },
                  { name: 'updated_at', label: t('ui.admin.updated_at'), kind: 'datetime' as const, options: [] }].map((field) =>
                  <div key={field.name} className="kv-details-row">
                    <dt>{field.label || field.name}</dt>
                    <dd><RecordValueDisplay value={record[field.name]} field={field.kind === undefined ? undefined : field} readable={canReadFile(application, resource.name, field.name)} /></dd>
                  </div>)}
              </dl>
              <button className="kv-button outlined kv-self-start" type="button" onClick={() => navigate(route + '?view=edit&id=' + record.id)}>{t('ui.admin.edit_record')}</button>
            </div>}
        </section>}
    </>}
    <NativeDialog open={deleting !== null} onClose={closeDelete} closeDisabled={deleteBusy} title={t('ui.admin.delete_title', { id: deleting?.id ?? '' })} className="kv-delete-dialog"
      footer={<>
        <button className="kv-button" type="button" onClick={closeDelete} disabled={deleteBusy} data-autofocus>{t(deleteUnknown ? 'ui.admin.close_verify' : 'ui.cancel')}</button>
        <button className="kv-button primary danger" type="button" disabled={deleteBusy || deleteUnknown} onClick={() => { void remove(); }}>{t(deleteBusy ? 'ui.admin.processing' : 'ui.admin.confirm_delete')}</button>
      </>}>
      <div className="kv-stack">
        <p className="kv-muted">{t('ui.admin.delete_warning')}</p>
        {deleting && <dl className="kv-delete-details">
          {resource.fields.slice(0, 3).map((field) => <div key={field.name}>
            <dt>{field.label || field.name}</dt><dd><RecordValueDisplay value={deleting[field.name]} field={field} compact readable={canReadFile(application, resource.name, field.name)} /></dd>
          </div>)}
        </dl>}
        {deleteFailure != null && <RequestError error={deleteFailure} />}
        {deleteInfo && <Notice kind="info">{translateError(deleteInfo, t)}</Notice>}
        {deleteUnknown && <button className="kv-button outlined" type="button" onClick={() => { void reconcileDelete(); }} disabled={deleteBusy}>{t('ui.admin.query_state')}</button>}
      </div>
    </NativeDialog>
  </div>;
}

export default function Admin() {
  const { t } = useI18n();
  const { application, session } = useRuntime();
  const params = useParams();
  const [navigation, setNavigation] = useState(false);
  const resource = application.resources.find((item) => item.name === params.resource);
  useEffect(() => { setNavigation(false); }, [params.resource]);
  if (session.user?.role !== 'admin') return <div className="kv-stack kv-access-required">
    <h1 className="kv-title">{t('ui.admin.required')}</h1>
    <Notice kind="warning">{t('ui.admin.login_hint')}</Notice>
    <Link className="kv-button outlined" to="/admin/login">{t('ui.admin.go_login')}</Link>
  </div>;
  if (!params.resource && application.resources[0]) return <Navigate replace to={'/admin/' + application.resources[0].name} />;
  const links = <nav className="kv-navigation" aria-label={t('ui.admin.resources')}>
    {application.resources.map((item) => <Link className="kv-navigation-link" key={item.name} to={'/admin/' + item.name}
      aria-current={item.name === params.resource ? 'page' : undefined} onClick={() => setNavigation(false)}>{item.label ?? item.name}</Link>)}
  </nav>;
  return <div className="kv-admin">
    <aside className="kv-sidebar">
      <h2 className="kv-sidebar-title">{t('ui.admin.management')}</h2>
      {links}<hr className="kv-divider" />
      <p className="kv-muted kv-small kv-sidebar-help">{t('ui.admin.resource_hint')}</p>
    </aside>
    <div className="kv-stack spacious kv-admin-content">
      <button className="kv-button outlined kv-resource-switch" type="button" onClick={() => setNavigation(true)} aria-haspopup="dialog" aria-expanded={navigation}>{t('ui.admin.choose_resource')}</button>
      {resource ? <ResourceWorkspace key={resource.name} resource={resource} /> : <div className="kv-stack">
        <h1 className="kv-title">{t(params.resource ? 'ui.admin.resource_missing' : 'ui.admin.no_resources')}</h1>
        <p className="kv-muted">{t(params.resource ? 'ui.admin.choose_valid' : 'ui.admin.none_available')}</p>
      </div>}
    </div>
    <NativeDialog open={navigation} onClose={() => setNavigation(false)} title={t('ui.admin.management')} className="kv-drawer"
      footer={<button className="kv-button outlined" type="button" data-autofocus onClick={() => setNavigation(false)}>{t('ui.close')}</button>}>
      {links}
    </NativeDialog>
  </div>;
}
