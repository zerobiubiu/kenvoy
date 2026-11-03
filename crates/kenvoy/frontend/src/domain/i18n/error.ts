import type { MessageParams, Translate } from './model.ts';

export interface LocalizedMessage { message: string; message_key?: string; code?: string; params?: MessageParams; outcomeUnknown?: boolean }

/** 只使用机器键和参数翻译，不匹配或拆解默认中文文本。 */
export function translateError(message: LocalizedMessage, t: Translate): string {
  if (message.outcomeUnknown) return t('errors.outcome_unknown', message.params, message.message);
  return message.message_key || message.code
    ? t(message.message_key ?? message.code!, message.params, message.message)
    : message.message;
}
