/** 默认界面的原生 CSS 变量；用户组件库无需依赖任何 ThemeProvider。 */
const theme = {
  '--kv-primary': '#1558ba', '--kv-primary-dark': '#10458f', '--kv-primary-light': '#e8f0fc',
  '--kv-background': '#f4f6f9', '--kv-paper': '#ffffff', '--kv-text': '#1e293b',
  '--kv-muted': '#526176', '--kv-border': '#dce2ea', '--kv-table-head': '#f8fafc',
  '--kv-success': '#237344', '--kv-error': '#b42332', '--kv-warning': '#945600',
  '--kv-selection': '#d6e6ff', '--kv-selection-text': '#153a6c', '--kv-saved': '#e8f3ec',
} as const;

export default theme;
