import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router';
import App from './App';
import theme from './theme';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('页面缺少应用容器');
for (const [name, value] of Object.entries(theme)) document.documentElement.style.setProperty(name, value);

createRoot(root).render(
  <BrowserRouter><App /></BrowserRouter>,
);
