import { ApiError } from '../application/errors';
import { Notice } from './ui';
import type { Translate } from '../domain/i18n/model';
import { translateError } from '../domain/i18n/error';
import { useI18n } from './i18n';

export function errorMessage(error: unknown, t: Translate): string {
  if (error instanceof ApiError) return translateError(error, t);
  return error instanceof Error ? error.message : t('ui.error.unknown');
}

export function RequestError({ error, retry }: { error: unknown; retry?: () => void }) {
  const { t } = useI18n();
  const uncertain = error instanceof ApiError && error.outcomeUnknown;
  return <Notice kind={uncertain ? 'warning' : 'error'} title={t(uncertain ? 'ui.error.uncertain_title' : 'ui.error.failed_title')} role="alert">
    <p>{errorMessage(error, t)}</p>
    {error instanceof ApiError && error.request_id && <p className="kv-hint">{t('ui.error.request', { id: error.request_id })}</p>}
    {retry && !uncertain && <button type="button" className="kv-button" onClick={retry}>{t('ui.reload')}</button>}
  </Notice>;
}
