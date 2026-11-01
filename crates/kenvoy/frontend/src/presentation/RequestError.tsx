import { ApiError } from '../application/errors';
import { Notice } from './ui';

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : '暂时无法完成操作，请稍后再试。';
}

export function RequestError({ error, retry }: { error: unknown; retry?: () => void }) {
  const uncertain = error instanceof ApiError && error.outcomeUnknown;
  return <Notice kind={uncertain ? 'warning' : 'error'} title={uncertain ? '操作结果待核对' : '操作未完成'} role="alert">
    <p>{uncertain ? '网络响应中断，无法确认提交结果。请联系管理员核对，避免重复提交。' : errorMessage(error)}</p>
    {error instanceof ApiError && error.request_id && <p className="kv-hint">核对编号：{error.request_id}</p>}
    {retry && !uncertain && <button type="button" className="kv-button" onClick={retry}>重新加载</button>}
  </Notice>;
}
