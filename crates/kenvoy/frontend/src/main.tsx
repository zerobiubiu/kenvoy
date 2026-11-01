import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router';
import { CssBaseline, ThemeProvider } from '@mui/material';
import App from './App';
import theme from './theme';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('页面缺少应用容器');

createRoot(root).render(
  <ThemeProvider theme={theme}>
    <CssBaseline />
    <BrowserRouter><App /></BrowserRouter>
  </ThemeProvider>,
);
