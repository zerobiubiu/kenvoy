import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium, expect } from '@playwright/test';
import { projectFixture } from '../../../../tools/tests/project-harness.mjs';

const fixture = await projectFixture();
let browser;
let diagnosticServer, diagnosticPage;
const diagnostics={errors:[],console:[],failed:[],sockets:[]};
const secret = 'KV_SERVER_ONLY_DO_NOT_BUNDLE_82571';
try {
  const source = join(fixture.project, 'src');
  await mkdir(join(source, 'components'));
  await writeFile(join(fixture.project, 'package.json'), JSON.stringify({ name: 'extensions-acceptance', private: true, version: '0.1.0', dependencies: { clsx: '2.1.1', '@radix-ui/react-slot': '1.5.0' } }, null, 2));
  await writeFile(join(source, 'CustomerFields.diu.tsx'), `export default <><text name="name" label="客户姓名" required maxLength={100}/><text name="phone" label="联系电话"/><number name="age" label="年龄" integer min={0}/></>;`);
  await writeFile(join(source, 'math.ts'), 'export const discount = (amount: number): number => amount >= 10000 ? 0.8 : 0.9;');
  await writeFile(join(source, 'discount.server.ts'), `import {discount} from './math'; export async function calculate(amount:number){if(amount<0)throw new Error('${secret}');return await Promise.resolve(discount(amount));}`);
  await writeFile(join(source, 'components/index.ts'), `import './card.css'; export {CustomerCard} from './CustomerCard';`);
  await writeFile(join(source, 'components/card.css'), '.customer-card-test { padding-inline: 15px; }');
  await writeFile(join(source, 'components/CustomerCard.tsx'), `import {useState} from 'react';import {Slot} from '@radix-ui/react-slot';import clsx from 'clsx';import {callAction} from '@kenvoy/client';
    export function CustomerCard({title,onIncrement}:{title:string,onIncrement:(count:number)=>number}){
      const [count,setCount]=useState(0);const [discount,setDiscount]=useState('');
      return <section className={clsx('customer-card-test',{active:count>0})}><h2>{title}</h2><p>数量：{count}</p>
        <Slot onClick={()=>setCount(onIncrement(count))}><button>增加数量</button></Slot><button onClick={()=>{void callAction('calculateDiscount',[10000]).then(value=>setDiscount(String(value)));}}>计算折扣</button><p>折扣：{discount}</p></section>;
    }`);
  await writeFile(join(source, 'app.tsx'), `import CustomerFields from './CustomerFields.diu';import {CustomerCard} from './components';import {calculate} from './discount.server';const step=2;
    export default <app name="扩展验收"><resource name="customers"><CustomerFields/></resource>
      <page path="/customers" title="客户登记"><CustomerCard title="自定义客户卡片" onIncrement={(count:number)=>count+step}/><form resource="customers" mode="create"/></page>
      <action name="calculateDiscount" handler={calculate} runtime="server" permission="anonymous"/>
    </app>;`);
  const manifestBefore = await readFile(join(fixture.project, 'package.json'), 'utf8');
  const server = await fixture.start('dev', ['--install-deps'], 240000);
  diagnosticServer=server;
  assert.equal(await readFile(join(fixture.project, 'package.json'), 'utf8'), manifestBefore);
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ locale: 'zh-CN', viewport: { width: 1440, height: 1000 } });
  diagnosticPage=page;
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  const consoleErrors=[];page.on('console',message=>{if(message.type()==='error')consoleErrors.push(message.text());});
  const failed=[];page.on('response',response=>{if(response.status()>=400)failed.push({path:new URL(response.url()).pathname,status:response.status()});});
  diagnostics.errors=errors;diagnostics.failed=failed;
  page.on('console',message=>{if(['error','warning','debug'].includes(message.type()))diagnostics.console.push({type:message.type(),text:message.text()});});
  page.on('websocket',socket=>{
    const entry={path:new URL(socket.url()).pathname,frames:[],closed:false};diagnostics.sockets.push(entry);
    socket.on('framereceived',frame=>entry.frames.push(String(frame.payload).slice(0,5000)));
    socket.on('socketerror',message=>{entry.error=message;});socket.on('close',()=>{entry.closed=true;});
  });
  await page.goto(server.origin + '/customers');
  try {await expect(page.getByRole('heading', { name: '自定义客户卡片' })).toBeVisible({timeout:15000});}
  catch(error){console.error(JSON.stringify({errors,consoleErrors,failed,server:server.logs()},null,2));throw error;}
  await expect(page.locator('.customer-card-test')).toHaveCSS('padding-left', '15px');
  await page.getByRole('button', { name: '增加数量', exact: true }).click();
  await expect(page.getByText('数量：2', { exact: true })).toBeVisible();
  const calculation = page.waitForResponse(response => response.url().endsWith('/api/actions/calculateDiscount'));
  await page.getByRole('button', { name: '计算折扣', exact: true }).click();
  assert.equal((await calculation).status(), 200);
  await expect(page.getByText('折扣：0.8', { exact: true })).toBeVisible();
  const componentPath=join(source,'components/CustomerCard.tsx');
  const component=await readFile(componentPath,'utf8');
  const updatedComponent=component.replace('增加数量','继续增加');
  await writeFile(componentPath,updatedComponent);
  await expect(page.getByRole('button',{name:'继续增加',exact:true})).toBeVisible({timeout:30000});
  await expect(page.getByText('数量：2',{exact:true})).toBeVisible();
  await page.getByRole('button',{name:'继续增加',exact:true}).click();await expect(page.getByText('数量：4',{exact:true})).toBeVisible();
  await writeFile(join(source,'components/card.css'),'.customer-card-test { padding-inline: 21px; }');
  await expect(page.locator('.customer-card-test')).toHaveCSS('padding-left','21px',{timeout:30000});
  await expect(page.getByText('数量：4',{exact:true})).toBeVisible();
  await writeFile(componentPath,'export function CustomerCard( {');
  await expect(page.getByText('开发更新未通过',{exact:true})).toBeVisible({timeout:15000});
  await expect(page.getByText('数量：4',{exact:true})).toBeVisible();
  await writeFile(componentPath,updatedComponent.replace('继续增加','恢复后增加'));
  await expect(page.getByRole('button',{name:'恢复后增加',exact:true})).toBeVisible({timeout:30000});
  await expect(page.getByText('开发更新未通过',{exact:true})).toHaveCount(0,{timeout:15000});
  await expect(page.getByText('数量：4',{exact:true})).toBeVisible();
  await page.getByLabel('客户姓名', { exact: false }).fill('扩展客户');
  await page.getByRole('button', { name: '提交信息', exact: true }).click();
  await expect(page.getByText('提交成功', { exact: true })).toBeVisible();
  const manifest = JSON.parse(await readFile(join(fixture.project, 'dist/manifest.json'), 'utf8'));
  const version = join(fixture.project, 'dist/versions', manifest.build_id);
  assert.ok((await readFile(join(version, 'modules/server.json'), 'utf8')).includes(secret));
  for (const name of Object.keys(manifest.files).filter(name => name.startsWith('client/'))) {
    assert.equal((await readFile(join(version, name), 'utf8')).includes(secret), false, name);
  }
  assert.equal((await page.request.get(server.origin + '/modules/server.json')).status(), 404);
  const session = await (await page.request.get(server.origin + '/_kenvoy/session')).json();
  const failure = await page.request.post(server.origin + '/api/actions/calculateDiscount', {
    headers: { origin: server.origin, 'x-kenvoy-csrf': session.csrf_token, 'x-kenvoy-generation': String(session.generation) }, data: { args: [-1] },
  });
  assert.equal(failure.status(), 500); assert.equal((await failure.text()).includes(secret), false);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ status: '通过', hooks_props_inline_capture: true, radix_third_party_css_barrel: true, schema_component: true, server_ts_esm_promise: true, private_code_isolated: true, project_manifest_preserved: true, fast_refresh_preserves_hooks: true, css_update_preserves_state: true, invalid_candidate_preserves_mirror_and_recovers: true }, null, 2));
  await server.stop();
} catch(error) {
  const state=diagnosticPage?await diagnosticPage.request.get(diagnosticServer.origin+'/_kenvoy/development').then(response=>response.json()).catch(()=>null):null;
  const aria=diagnosticPage?await diagnosticPage.locator('body').ariaSnapshot().catch(()=>null):null;
  console.error(JSON.stringify({...diagnostics,state,aria,server:diagnosticServer?.logs()},null,2));
  throw error;
} finally { if (browser) await browser.close(); await fixture.close(); }
