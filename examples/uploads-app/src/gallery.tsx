import { useEffect, useState } from 'react';
import { AttachmentList, Image, fileUrl, useResource } from '@kenvoy/client';
import type { RecordData, UploadedFile } from '@kenvoy/client';
import './gallery.css';

function files(record: RecordData, name: string): UploadedFile[] {
  const value = record[name];
  return Array.isArray(value) ? value : [];
}

/** 只读取真实资源，不创建第二份 Schema，也不授予匿名附件读取权。 */
export function UploadGallery() {
  const materials = useResource('materials');
  const [records, setRecords] = useState<RecordData[]>([]);
  const [total, setTotal] = useState(0);
  const [busy, setBusy] = useState(true);
  const [failure, setFailure] = useState('');
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let current = true;
    setBusy(true);
    setFailure('');
    materials.list({ page_size: 12, sort: 'id', descending: true }).then(result => {
      if (current) { setRecords(result.data); setTotal(result.total); }
    }).catch(error => {
      if (current) setFailure(error instanceof Error ? error.message : '无法读取资料，请重新加载。');
    }).finally(() => { if (current) setBusy(false); });
    return () => { current = false; };
  }, [materials, revision]);

  return <section className="uploads-gallery" aria-label="已上传资料图库">
    <div className="kv-row">
      <p>图片和附件来自同一资料记录；新增、编辑或删除后可刷新查看。</p>
      <button type="button" className="kv-button" disabled={busy}
        onClick={() => setRevision(value => value + 1)}>刷新资料</button>
    </div>
    {busy && <p role="status">正在读取已上传资料…</p>}
    {failure && <div role="alert"><p>{failure}</p>
      <button type="button" className="kv-button" onClick={() => setRevision(value => value + 1)}>重新加载</button>
    </div>}
    {!busy && !failure && records.length === 0 && <p>还没有资料。先到<a href="/uploads">上传页面</a>提交图片，再返回这里刷新。</p>}
    {!busy && !failure && records.map(record => <article className="uploads-record" key={record.id}>
      <h2>{String(record.title)}</h2>
      <p className="kv-hint">记录编号：{record.id}</p>
      {record.description && <p>{String(record.description)}</p>}
      <div className="uploads-photos">{files(record, 'photos').map(file => <figure className="uploads-photo" key={file.id}>
        <Image src={fileUrl(file)} alt={`${String(record.title)}：${file.name}`}
          width={320} height={220} fit="contain" />
        <figcaption><a href={fileUrl(file, true)} download={file.name}>{file.name} · 下载图片</a></figcaption>
      </figure>)}</div>
      {files(record, 'attachments').length > 0 && <div>
        <h3>补充附件</h3>
        <AttachmentList files={files(record, 'attachments')} compact />
      </div>}
      <a href={`/admin/materials?view=detail&id=${record.id}`}>在后台查看与管理此资料</a>
    </article>)}
    {!busy && !failure && total > records.length && <p>这里只展示最新 {records.length} 条资料；全部 {total} 条记录可在<a href="/admin/materials">管理后台</a>查询。</p>}
  </section>;
}
