import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { cp, mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { configuration } from '../scripts/client-builder.mjs';

const framework = fileURLToPath(new URL('..', import.meta.url));
const script = path.join(framework, 'scripts/client-builder.mjs');
const sha = text => createHash('sha256').update(text).digest('hex');

async function fixture(t, files, reference = 'src/barrel.ts') {
  const root = await mkdtemp(path.join(os.tmpdir(), 'kenvoy-builder-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const [name, source] of Object.entries(files)) {
    await mkdir(path.dirname(path.join(root, name)), { recursive: true });
    await writeFile(path.join(root, name), source);
  }
  const destination = path.join(root, '.kenvoy/stage/client');
  await mkdir(destination, { recursive: true });
  const request = {
    version: 1, project_root: root, framework_root: framework, toolchain_root: framework,
    destination, report: path.join(root, '.kenvoy/report.json'),
    private_modules: [path.join(root, 'src/private.server.ts')],
    declaration_modules: [path.join(root, 'src/app.tsx')],
    source_modules: Object.fromEntries(Object.entries(files).filter(([name]) => /\.[jt]sx?$/.test(name)).map(([name, source]) => [path.join(root, name), sha(source)])),
    external_packages: [], references: [{ id: 'customer-card', kind: 'component', module: path.join(root, reference), export_name: 'CustomerCard' }],
  };
  return { root, request, destination };
}
async function execute(request, root) {
  const input = path.join(root, '.kenvoy/request.json');
  await writeFile(input, JSON.stringify(request));
  return await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [script, input], { cwd: framework, windowsHide: true, env: { ...process.env, NODE_OPTIONS: '', NODE_ENV: 'production' } });
    let output = '';
    child.stdout.on('data', value => output += value);
    child.stderr.on('data', value => output += value);
    child.on('error', reject);
    const timer = setTimeout(() => { child.kill(); reject(new Error('客户端构建测试超时')); }, 60000);
    child.on('close', code => { clearTimeout(timer); resolve({ code, output }); });
  });
}

test('真实 Vite 自定义组件构建保留 Hooks、MUI、barrel CSS 和静态切片，未打包服务端入口', async t => {
  const { root, request, destination } = await fixture(t, {
    'src/Card.tsx': `import {useState} from 'react'; import {Button} from '@mui/material'; import {useAction} from '@kenvoy/client'; import clsx from 'clsx'; export function CustomerCard({label='客户'}) { const [count,setCount]=useState(0); const calculate=useAction('discount'); return <Button className={clsx('custom-provenance-marker',{active:count>0})} onClick={()=>{setCount(count+1);void calculate([count]);}}>{label}:{count}</Button>; }`,
    'src/barrel.ts': `import './card.css'; export {CustomerCard} from './Card';`,
    'src/card.css': '.custom-provenance-marker { color: rgb(11,22,33); }',
    'src/private.server.ts': 'export const PRIVATE_SERVER_ONLY_SENTINEL = "private-secret";',
    'src/app.tsx': 'export default <app name="不能进入浏览器"/>;',
    'postcss.config.cjs': 'throw new Error("不允许读取项目 PostCSS 配置");',
  });
  await mkdir(path.join(root, 'node_modules'), { recursive: true });
  await cp(path.join(framework, 'node_modules/clsx'), path.join(root, 'node_modules/clsx'), { recursive: true });
  await writeFile(path.join(root, 'package.json'), JSON.stringify({ dependencies: { clsx: '2.1.1' } }));
  request.external_packages.push('clsx');
  request.references.push({ id: 'inline-expression', kind: 'expression', module: path.join(root, 'src/app.tsx'), export_name: '', slice: {
    code: 'export const expression = () => ({label:"切片独立标记"});', export_name: 'expression', imports: {},
  } });
  request.references.push({ id: 'inline-component', kind: 'component', module: path.join(root, 'src/app.tsx'), export_name: '', slice: {
    code: 'import {Button} from "@mui/material"; export const component = () => <Button>内联JSX切片</Button>;', export_name: 'component', imports: { '@mui/material': { kind: 'external', module: '@mui/material' } },
  } });
  const result = await execute(request, root);
  assert.equal(result.code, 0, result.output);
  const assets = await readdir(path.join(destination, 'assets'));
  const content = (await Promise.all(assets.map(name => readFile(path.join(destination, 'assets', name), 'utf8')))).join('\n');
  assert.match(content, /custom-provenance-marker/);
  assert.match(content, /切片独立标记/);
  assert.match(content, /内联JSX切片/);
  assert.doesNotMatch(content, /PRIVATE_SERVER_ONLY_SENTINEL|不能进入浏览器/);
  const report = JSON.parse(await readFile(request.report, 'utf8'));
  assert.equal(report.version, 1);
  assert.ok(report.dependencies.some(item => item.name === 'clsx' && item.version === '2.1.1' && item.scope === 'project'));
});

test('客户端拒绝服务端模块、未分析的动态模块、源码变化和 CSS 私有资源', async t => {
  const cases = [
    { source: `export {CustomerCard} from './private.server';`, private: 'export const CustomerCard=()=>null;', pattern: /服务端私有模块/ },
    { source: `export const CustomerCard=()=>import('./late.ts');`, late: 'export default 1;', removeGraph: true, pattern: /已经分析的模块图/ },
    { source: `export const CustomerCard=()=>null;`, modified: true, pattern: /源码已改变/ },
    { source: `import './card.css'; export const CustomerCard=()=>null;`, css: '.secret { background:url(../data/secret.txt); }', pattern: /资源类型尚未开放/ },
  ];
  for (const item of cases) {
    const { root, request } = await fixture(t, { 'src/Card.tsx': item.source, 'src/private.server.ts': item.private ?? '', 'src/late.ts': item.late ?? '', 'src/card.css': item.css ?? '', 'data/secret.txt': 'PRIVATE_CSS_SENTINEL' }, 'src/Card.tsx');
    if (item.removeGraph) delete request.source_modules[path.join(root, 'src/late.ts')];
    if (item.modified) await writeFile(path.join(root, 'src/Card.tsx'), 'export const CustomerCard=()=>"changed";');
    const result = await execute(request, root);
    assert.notEqual(result.code, 0, result.output);
    assert.match(result.output, item.pattern);
  }
});

test('开发模块加载即使从 @id 进入也不读取原始资产或框架构建脚本', async t => {
  const {root,request}=await fixture(t,{'src/Card.tsx':'export const CustomerCard=()=>null;','src/private.css':'.never-publish{color:red}'},'src/Card.tsx');
  request.mirror_root=path.join(root,'.kenvoy/mirror');
  await mkdir(request.mirror_root,{recursive:true});
  const {config}=await configuration(request);
  await assert.rejects(config.plugins[0].load(path.join(root,'src/private.css')+'?raw'),/已经发布的客户端镜像/);
  await assert.rejects(config.plugins[0].load(script+'?raw'),/不公开框架构建脚本/);
});
