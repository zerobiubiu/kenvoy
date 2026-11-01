import { createRoot } from 'react-dom/client';
import { BrowserRouter, Link, Route, Routes } from 'react-router';
import { CssBaseline, ThemeProvider, createTheme } from '@mui/material';
import { PageInterpreter } from './runtime';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('探针页面缺少 root');

createRoot(root).render(
  <ThemeProvider theme={createTheme()}>
    <CssBaseline />
    <BrowserRouter>
      <Routes>
        <Route path="/customers" element={<PageInterpreter />} />
        <Route path="/" element={<Link to="/customers">进入探针</Link>} />
        <Route path="*" element={<p>页面不存在</p>} />
      </Routes>
    </BrowserRouter>
  </ThemeProvider>,
);
