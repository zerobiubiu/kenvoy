import zh from './ui.zh-CN.json' with { type: 'json' };
import en from './ui.en-US.json' with { type: 'json' };
import { ERROR_MESSAGES } from './errors.ts';

export const SYSTEM_MESSAGES: Record<'zh-CN' | 'en-US', Record<string, string>> = {
  'zh-CN': { ...zh, ...ERROR_MESSAGES['zh-CN'] },
  'en-US': { ...en, ...ERROR_MESSAGES['en-US'] },
};
