import { createTheme } from '@mui/material';

const theme = createTheme({
  palette: {
    mode: 'light',
    primary: { main: '#1558ba', dark: '#10458f', light: '#e8f0fc' },
    background: { default: '#f4f6f9', paper: '#ffffff' },
    text: { primary: '#1e293b', secondary: '#526176' },
    divider: '#dce2ea',
    success: { main: '#237344' },
    error: { main: '#b42332' },
    warning: { main: '#945600' },
  },
  typography: {
    fontFamily: '"Segoe UI", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif',
    fontSize: 14,
    h1: { fontSize: '1.875rem', fontWeight: 700, lineHeight: 1.35, overflowWrap: 'anywhere' },
    h2: { fontSize: '1.375rem', fontWeight: 700, lineHeight: 1.4, overflowWrap: 'anywhere' },
    h3: { fontSize: '1.125rem', fontWeight: 650, lineHeight: 1.45 },
    body1: { lineHeight: 1.65 },
    body2: { lineHeight: 1.6 },
    button: { textTransform: 'none', fontWeight: 600 },
  },
  shape: { borderRadius: 8 },
  components: {
    MuiButton: {
      defaultProps: { disableElevation: true },
      styleOverrides: { root: { minHeight: 40, paddingInline: 18, borderRadius: 7 } },
    },
    MuiTextField: { defaultProps: { fullWidth: true, variant: 'outlined' } },
    MuiOutlinedInput: { styleOverrides: { root: {
      backgroundColor: '#fff',
      '& .MuiInputBase-input.Mui-disabled': { WebkitTextFillColor: '#526176', color: '#526176', opacity: 1 },
      '& .MuiInputBase-input::placeholder': { color: '#526176', opacity: 1 },
    } } },
    MuiInputLabel: { styleOverrides: { root: { color: '#526176', '&.Mui-disabled': { color: '#526176' } } } },
    MuiFormHelperText: { styleOverrides: { root: { marginLeft: 0, marginRight: 0, '&.Mui-disabled': { color: '#526176' } } } },
    MuiAlert: { defaultProps: { closeText: '关闭提示' }, styleOverrides: { root: { borderRadius: 8, alignItems: 'flex-start' } } },
    MuiTableCell: {
      styleOverrides: {
        head: { color: '#526176', fontWeight: 650, backgroundColor: '#f8fafc', whiteSpace: 'nowrap' },
        body: { verticalAlign: 'top', paddingTop: 15, paddingBottom: 15 },
      },
    },
    MuiPaper: { defaultProps: { elevation: 0 } },
    MuiDialog: { styleOverrides: { paper: { borderRadius: 12 } } },
  },
});

export default theme;
