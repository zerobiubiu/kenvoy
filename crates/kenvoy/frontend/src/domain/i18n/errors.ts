import zhCN from './errors.zh-CN.json' with { type: 'json' };
import enUS from './errors.en-US.json' with { type: 'json' };

/** Rust 词条提取与原生 UI 使用同份系统错误目录，参数只来自机器校验结果。 */
export const ERROR_MESSAGES: Record<'zh-CN' | 'en-US', Record<string, string>> = {
  'zh-CN': zhCN,
  'en-US': enUS,
};
