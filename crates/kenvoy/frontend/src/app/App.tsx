import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
import { Link, Navigate, Route, Routes, useLocation, useNavigate } from 'react-router';
import { api, developmentChange } from '../infrastructure/api';
import { ApiError } from '../application/errors';
import { RequestError, errorMessage } from '../presentation/RequestError';
import { RuntimeContext, useRuntime } from '../presentation/runtime/context';
import { RuntimePage } from '../presentation/runtime/RuntimePage';
import { Loading, NativeDialog, Notice } from '../presentation/ui';
import type { ApplicationDescription, DevelopmentStatus, NavigationItem, SessionView } from '../domain/types';

const Admin = lazy(() => import('../presentation/Admin'));

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
        // 完成一次请求后再安排下一次，慢请求不会积累；不重试业务写入。
        if (current) timer = setTimeout(() => { void poll(); }, 1000);
      }
    };
    void poll();
    return () => { current = false; if (timer !== undefined) clearTimeout(timer); };
  }, [client, enabled, refresh]);
  if (!enabled || (!compilationError && !connectionLost)) return null;
  return <div className="kv-stack kv-development-notice">
    {compilationError && <Notice kind="error" title="开发更新未通过">
      <p className="kv-pre-wrap">{compilationError}</p>
      当前继续使用上一次有效页面。修复源码后将自动恢复。
    </Notice>}
    {connectionLost && <Notice kind="warning">开发服务暂时无法连接，当前页面和填写内容已保留；连接恢复后将自动同步。</Notice>}
  </div>;
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
  return <>
    <a className="skip-link" href="#main-content">跳到主要内容</a>
    <header className="kv-header">
      <div className="kv-toolbar">
        <Link className="kv-brand" to={first}>{application.name}</Link>
        <div className="kv-row desktop-only">
          {session.user ? <>
            <span className="kv-account kv-muted" title={session.user.username}>{session.user.username}</span>
            {session.user.role === 'admin' && <Link className="kv-button" to="/admin">管理后台</Link>}
            <button className="kv-button" type="button" onClick={() => { void logout(); }} disabled={busy}>{busy ? '正在退出…' : '退出登录'}</button>
          </> : <Link className="kv-button" to="/admin/login">管理员登录</Link>}
        </div>
        <button className="kv-button mobile-only" type="button" onClick={() => setMenu(true)} aria-label="打开导航" aria-expanded={menu} aria-haspopup="dialog">菜单</button>
      </div>
      {(declaredLinks.length > 0 || pages.length > 1) && <nav aria-label="页面导航" className="kv-page-navigation desktop-only">
        {pages.map((page, index) => <Link key={page.path + '-' + index} to={page.path} aria-current={location.pathname === page.path ? 'page' : undefined}
          className={'kv-button compact' + (location.pathname === page.path ? ' primary' : '')}>{page.title}</Link>)}
      </nav>}
    </header>
    <NativeDialog open={menu} onClose={() => setMenu(false)} title="导航" className="kv-drawer kv-drawer-right" closeDisabled={busy}
      footer={<button className="kv-button outlined" type="button" data-autofocus disabled={busy} onClick={() => setMenu(false)}>关闭</button>}>
      <nav className="kv-navigation" aria-label="移动端页面导航">
        {pages.map((page, index) => <Link className="kv-navigation-link" key={page.path + '-' + index} to={page.path}
          aria-current={location.pathname === page.path ? 'page' : undefined} onClick={() => setMenu(false)}>{page.title}</Link>)}
      </nav>
      <hr className="kv-divider" />
      <div className="kv-stack">
        {session.user ? <>
          <p className="kv-small kv-wrap">当前账号：{session.user.username}</p>
          {session.user.role === 'admin' && <Link className="kv-button" to="/admin" onClick={() => setMenu(false)}>管理后台</Link>}
          <button className="kv-button outlined" type="button" onClick={() => { void logout(); }} disabled={busy}>{busy ? '正在退出…' : '退出登录'}</button>
        </> : <Link className="kv-button outlined" to="/admin/login" onClick={() => setMenu(false)}>管理员登录</Link>}
      </div>
    </NativeDialog>
    <main id="main-content" tabIndex={-1} className="kv-main">
      {failure != null && <div className="kv-stack kv-shell-failure"><RequestError error={failure} />
        <button className="kv-button" type="button" onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }}>刷新登录状态</button>
      </div>}
      <DevelopmentNotice />
      {children}
    </main>
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
  return <div className="kv-login">
    <h1 className="kv-title">管理员登录</h1>
    <p className="kv-muted kv-login-intro">登录后可查询和管理授权范围内的数据。</p>
    <section className="kv-panel">
      {session.user ? <div className="kv-stack">
        <Notice kind={session.user.role === 'admin' ? 'success' : 'warning'}>
          {session.user.role === 'admin' ? '已登录：' + session.user.username : '当前账号没有管理权限，请使用管理员账号登录。'}
        </Notice>
        {session.user.role === 'admin' && <Link className="kv-button primary" to="/admin">进入管理后台</Link>}
        <Link className="kv-button outlined" to={application.pages[0]?.path || '/'}>返回应用</Link>
      </div> : <form className="kv-stack spacious" noValidate onSubmit={submit} aria-busy={busy}>
        <div className="kv-field">
          <label htmlFor="login-username">用户名 <span aria-hidden="true">*</span></label>
          <input id="login-username" name="username" value={username} onChange={(event) => setUsername(event.target.value)} required
            autoComplete="username" disabled={busy} maxLength={64} autoCapitalize="none" spellCheck={false}
            aria-invalid={Boolean(inputError && !username)} aria-describedby={inputError ? 'login-input-error' : undefined} />
        </div>
        <div className="kv-field">
          <label htmlFor="login-password">密码 <span aria-hidden="true">*</span></label>
          <input id="login-password" name="password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} required
            autoComplete="current-password" disabled={busy} aria-invalid={Boolean(inputError && !password)} aria-describedby={inputError ? 'login-input-error' : undefined} />
        </div>
        {inputError && <div id="login-input-error"><Notice kind="error">{inputError}</Notice></div>}
        {failure != null && <RequestError error={failure} />}
        {failure instanceof ApiError && (failure.outcomeUnknown || ['KV5001', 'KV5013'].includes(failure.code)) &&
          <button className="kv-button outlined" type="button" onClick={() => { void refresh().then(() => setFailure(null)).catch(setFailure); }}>刷新登录状态</button>}
        <button className="kv-button primary" type="submit" disabled={busy}>{busy ? '正在登录…' : '登录'}</button>
      </form>}
    </section>
  </div>;
}

function MissingPage() {
  const { application } = useRuntime();
  return <div className="kv-stack kv-missing">
    <h1 className="kv-title">无法找到这个页面</h1>
    <p className="kv-muted">页面可能已经调整，或当前账号没有访问权限。</p>
    <Link className="kv-button outlined kv-self-start" to={application.pages[0]?.path || '/admin/login'}>返回可访问的页面</Link>
  </div>;
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
  if (loading) return <div className="kv-startup"><Loading label="正在加载页面" height={340} /></div>;
  if (failure != null || !application || !session) return <div className="kv-stack kv-startup">
    <h1 className="kv-title">页面暂时无法加载</h1>
    <Notice kind="error">{errorMessage(failure)}</Notice><button className="kv-button outlined" type="button" onClick={() => { void load(); }}>重新加载</button>
  </div>;
  return <RuntimeContext.Provider value={{ application, session, client: api, refresh }}>
    <Shell><Routes>
      {application.pages.map((page) => <Route key={page.path} path={page.path} element={<RuntimePage page={page} />} />)}
      {!application.pages.some((page) => page.path === '/') && <Route path="/" element={<Navigate to={application.pages[0]?.path || '/admin/login'} replace />} />}
      <Route path="/admin/login" element={<Login />} />
      <Route path="/admin" element={<Suspense fallback={<Loading height={360} />}><Admin /></Suspense>} />
      <Route path="/admin/:resource" element={<Suspense fallback={<Loading height={360} />}><Admin /></Suspense>} />
      <Route path="*" element={<MissingPage />} />
    </Routes></Shell>
  </RuntimeContext.Provider>;
}
