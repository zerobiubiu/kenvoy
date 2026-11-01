import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
import { Link, Navigate, Route, Routes, useLocation, useNavigate } from 'react-router';
import {
  Alert, AlertTitle, AppBar, Box, Button, Divider, Drawer, List, ListItemButton, ListItemText,
  Paper, Skeleton, Stack, TextField, Toolbar, Typography,
} from '@mui/material';
import { api, ApiError, developmentChange } from './api';
import { RequestError, RuntimeContext, RuntimePage, errorMessage, useRuntime } from './Runtime';
import type { ApplicationDescription, DevelopmentStatus, NavigationItem, SessionView } from './types';

const Admin = lazy(() => import('./Admin'));

function navigationLinks(items: NavigationItem[], parents: string[] = []): { path: string; title: string }[] {
  return items.flatMap((item) => item.type === 'link'
    ? [{ path: item.path, title: [...parents, item.label].join(' / ') }]
    : navigationLinks(item.items, item.label ? [...parents, item.label] : parents));
}

function DevelopmentNotice() {
  const { application, client, refresh } = useRuntime();
  const generation = useRef(application.generation);
  generation.current = application.generation;
  const enabled = application.development === true;
  const [compilationError, setCompilationError] = useState<string | null>(null);
  const [connectionLost, setConnectionLost] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let previous: DevelopmentStatus | null = null;
    let reconnected = false;
    const poll = async () => {
      try {
        const status = await client.getDevelopment();
        if (!current) return;
        const change = developmentChange(previous, status, generation.current, reconnected);
        if (change === 'reload') {
          previous = status;
          // 页面启动会重新读取描述；首次轮询只建立基线，不重复触发刷新。
          window.location.reload();
          return;
        }
        if (change === 'refresh') {
          await refresh();
          if (!current) return;
        }
        if (status.state === 'error') setCompilationError(status.error || '源码或构建检查未通过，请查看终端诊断。');
        if (status.state === 'ready') setCompilationError(null);
        previous = status;
        reconnected = false;
        setConnectionLost(false);
      } catch {
        if (!current) return;
        reconnected = true;
        setConnectionLost(true);
      } finally {
        // 完成一次请求后再安排下一次，慢请求不会积累；只读取状态，不重试业务写入。
        if (current) timer = setTimeout(() => { void poll(); }, 1000);
      }
    };
    void poll();
    return () => { current = false; if (timer !== undefined) clearTimeout(timer); };
  }, [client, enabled, refresh]);
  if (!enabled || (!compilationError && !connectionLost)) return null;
  return <Stack spacing={1.5} sx={{ maxWidth: 1200, mx: 'auto', mb: 3 }}>
    {compilationError && <Alert severity="error" sx={{ overflowWrap: 'anywhere' }}>
      <AlertTitle>开发更新未通过</AlertTitle>
      <Box sx={{ whiteSpace: 'pre-wrap' }}>{compilationError}</Box>
      当前继续使用上一次有效页面。修复源码后将自动恢复。
    </Alert>}
    {connectionLost && <Alert severity="warning">开发服务暂时无法连接，当前页面和填写内容已保留；连接恢复后将自动同步。</Alert>}
  </Stack>;
}

function Shell({ children }: { children: ReactNode }) {
  const { application, session, client, refresh } = useRuntime();
  const [menu, setMenu] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<unknown>(null);
  const navigate = useNavigate();
  const location = useLocation();
  const first = application.pages[0]?.path || '/admin/login';
  const declaredLinks = application.navigation.flatMap((group) => navigationLinks(group.items, group.label ? [group.label] : []));
  const pages = declaredLinks.length ? declaredLinks : application.pages;
  useEffect(() => { setMenu(false); }, [location.pathname]);
  const logout = async () => {
    setBusy(true); setFailure(null);
    try { await client.logout(); await refresh(); navigate('/'); }
    catch (error) { setFailure(error); }
    finally { setBusy(false); }
  };
  const links = pages.map((page, index) => <ListItemButton key={`${page.path}-${index}`} component={Link} to={page.path}
    selected={location.pathname === page.path} aria-current={location.pathname === page.path ? 'page' : undefined} onClick={() => setMenu(false)}>
    <ListItemText primary={page.title} />
  </ListItemButton>);
  return <>
    <a className="skip-link" href="#main-content">跳到主要内容</a>
    <AppBar position="sticky" color="inherit" elevation={0} sx={{ borderBottom: '1px solid', borderColor: 'divider' }}>
      <Toolbar sx={{ minHeight: { xs: 64, sm: 72 }, gap: 2, px: { xs: 2, sm: 4 } }}>
        <Typography component={Link} to={first} sx={{ fontSize: '1.0625rem', fontWeight: 700, color: 'text.primary', textDecoration: 'none', overflowWrap: 'anywhere', flex: 1 }}>
          {application.name}
        </Typography>
        <Stack direction="row" spacing={1} sx={{ display: { xs: 'none', sm: 'flex' }, alignItems: 'center' }}>
          {session.user ? <>
            <Typography variant="body2" color="text.secondary" title={session.user.username} sx={{ maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis' }}>{session.user.username}</Typography>
            {session.user.role === 'admin' && <Button component={Link} to="/admin" color="primary">管理后台</Button>}
            <Button onClick={() => { void logout(); }} disabled={busy}>{busy ? '正在退出…' : '退出登录'}</Button>
          </> : <Button component={Link} to="/admin/login">管理员登录</Button>}
        </Stack>
        <Button onClick={() => setMenu(true)} aria-label="打开导航" aria-expanded={menu} sx={{ display: { xs: 'inline-flex', sm: 'none' }, minWidth: 44 }}>菜单</Button>
      </Toolbar>
      {(declaredLinks.length > 0 || pages.length > 1) && <Box component="nav" aria-label="页面导航" sx={{ display: { xs: 'none', sm: 'flex' }, px: 4, gap: 1, overflowX: 'auto', pb: 1.5 }}>
        {pages.map((page, index) => <Button key={`${page.path}-${index}`} component={Link} to={page.path} aria-current={location.pathname === page.path ? 'page' : undefined}
          variant={location.pathname === page.path ? 'contained' : 'text'} size="small" sx={{ whiteSpace: 'nowrap' }}>{page.title}</Button>)}
      </Box>}
    </AppBar>
    <Drawer anchor="right" open={menu} onClose={() => setMenu(false)}>
      <Box sx={{ width: 280, maxWidth: '85vw', pt: 2 }}>
        <Stack direction="row" sx={{ px: 2, pb: 1, alignItems: 'center', justifyContent: 'space-between' }}>
          <Typography sx={{ fontWeight: 700 }}>导航</Typography><Button onClick={() => setMenu(false)}>关闭</Button>
        </Stack>
        <List component="nav" aria-label="移动端页面导航">{links}</List><Divider />
        <Stack spacing={1.5} sx={{ p: 2 }}>
          {session.user ? <>
            <Typography variant="body2">当前账号：{session.user.username}</Typography>
            {session.user.role === 'admin' && <Button component={Link} to="/admin">管理后台</Button>}
            <Button variant="outlined" onClick={() => { void logout(); }} disabled={busy}>退出登录</Button>
          </> : <Button component={Link} to="/admin/login" variant="outlined">管理员登录</Button>}
        </Stack>
      </Box>
    </Drawer>
    <Box component="main" id="main-content" tabIndex={-1} sx={{ px: { xs: 2, sm: 4 }, py: { xs: 3, sm: 5 }, minHeight: 'calc(100dvh - 72px)' }}>
      {failure != null && <Box sx={{ maxWidth: 720, mx: 'auto', mb: 3 }}><RequestError error={failure} />
        <Button onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }} sx={{ mt: 1 }}>刷新登录状态</Button>
      </Box>}
      <DevelopmentNotice />
      {children}
    </Box>
  </>;
}

function Login() {
  const { application, session, client, refresh } = useRuntime();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<unknown>(null);
  const [inputError, setInputError] = useState('');
  const navigate = useNavigate();
  const location = useLocation();
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    if (!username || !password) { setInputError('请输入用户名和密码。'); return; }
    setInputError(''); setFailure(null); setBusy(true);
    try {
      const context = await client.login(username, password);
      setPassword(''); await refresh();
      if (context.user?.role === 'admin') {
        const next = new URLSearchParams(location.search).get('next');
        navigate(next && /^\/admin(?:\/|$)/.test(next) && !next.includes('\\') ? next : '/admin', { replace: true });
      }
    } catch (error) { setFailure(error); }
    finally { setBusy(false); }
  };
  return <Box sx={{ width: '100%', maxWidth: 440, mx: 'auto', pt: { sm: 3 } }}>
    <Typography variant="h1" component="h1" sx={{ mb: 1 }}>管理员登录</Typography>
    <Typography color="text.secondary" sx={{ mb: 4 }}>登录后可查询和管理授权范围内的数据。</Typography>
    <Paper sx={{ p: { xs: 2.5, sm: 4 }, border: '1px solid', borderColor: 'divider', borderRadius: 1.5 }}>
      {session.user ? <Stack spacing={2}>
        <Alert severity={session.user.role === 'admin' ? 'success' : 'warning'}>
          {session.user.role === 'admin' ? `已登录：${session.user.username}` : '当前账号没有管理权限，请使用管理员账号登录。'}
        </Alert>
        {session.user.role === 'admin' && <Button component={Link} to="/admin" variant="contained">进入管理后台</Button>}
        <Button component={Link} to={application.pages[0]?.path || '/'} variant="outlined">返回应用</Button>
      </Stack> : <Stack component="form" noValidate onSubmit={submit} spacing={3} aria-busy={busy}>
        <TextField label="用户名" name="username" value={username} onChange={(event) => setUsername(event.target.value)} required
          autoComplete="username" disabled={busy} slotProps={{ htmlInput: { maxLength: 64, autoCapitalize: 'none', spellCheck: false } }} />
        <TextField label="密码" name="password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} required autoComplete="current-password" disabled={busy} />
        {inputError && <Alert severity="error">{inputError}</Alert>}
        {failure != null && <RequestError error={failure} />}
        {failure instanceof ApiError && (failure.outcomeUnknown || ['KV5001', 'KV5013'].includes(failure.code)) &&
          <Button variant="outlined" onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }}>刷新登录状态</Button>}
        <Button type="submit" variant="contained" disabled={busy}>{busy ? '正在登录…' : '登录'}</Button>
      </Stack>}
    </Paper>
  </Box>;
}

function MissingPage() {
  const { application } = useRuntime();
  return <Stack spacing={2} sx={{ maxWidth: 560, mx: 'auto', py: 5 }}>
    <Typography component="h1" variant="h1">无法找到这个页面</Typography>
    <Typography color="text.secondary">页面可能已经调整，或当前账号没有访问权限。</Typography>
    <Button component={Link} to={application.pages[0]?.path || '/admin/login'} variant="outlined" sx={{ alignSelf: 'flex-start' }}>返回可访问的页面</Button>
  </Stack>;
}

export default function App() {
  const [application, setApplication] = useState<ApplicationDescription | null>(null);
  const [session, setSession] = useState<SessionView | null>(null);
  const [failure, setFailure] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const refresh = useCallback(async () => {
    const context = await api.getSession();
    const description = await api.getApplication();
    setSession(context); setApplication(description);
    document.title = description.name;
  }, []);
  const load = useCallback(async () => {
    setLoading(true); setFailure(null);
    try { await refresh(); } catch (error) { setFailure(error); }
    finally { setLoading(false); }
  }, [refresh]);
  useEffect(() => { void load(); }, [load]);
  if (loading) return <Stack spacing={3} sx={{ maxWidth: 720, mx: 'auto', px: 3, py: 8 }} role="status" aria-label="正在加载页面">
    <Skeleton variant="text" width="55%" height={52} /><Skeleton variant="rounded" height={340} />
  </Stack>;
  if (failure != null || !application || !session) return <Stack spacing={2} sx={{ maxWidth: 640, mx: 'auto', p: 4 }}>
    <Typography variant="h1" component="h1">页面暂时无法加载</Typography>
    <Alert severity="error">{errorMessage(failure)}</Alert><Button variant="outlined" onClick={() => { void load(); }}>重新加载</Button>
  </Stack>;
  return <RuntimeContext.Provider value={{ application, session, client: api, refresh }}>
    <Shell><Routes>
      {application.pages.map((page) => <Route key={page.path} path={page.path} element={<RuntimePage page={page} />} />)}
      {!application.pages.some((page) => page.path === '/') && <Route path="/" element={<Navigate to={application.pages[0]?.path || '/admin/login'} replace />} />}
      <Route path="/admin/login" element={<Login />} />
      <Route path="/admin" element={<Suspense fallback={<Skeleton variant="rounded" height={360} />}><Admin /></Suspense>} />
      <Route path="/admin/:resource" element={<Suspense fallback={<Skeleton variant="rounded" height={360} />}><Admin /></Suspense>} />
      <Route path="*" element={<MissingPage />} />
    </Routes></Shell>
  </RuntimeContext.Provider>;
}
