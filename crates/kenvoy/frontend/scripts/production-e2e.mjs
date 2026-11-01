import assert from 'node:assert/strict';
import { readFile, rename, writeFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium, expect } from '@playwright/test';
import { projectFixture } from '../../../kenvoy-tests/browser/fixture.mjs';
const fixture = await projectFixture();
let browser;
try {
  await fixture.command(['--project','app','admin','init','--username','admin','--password-stdin'],'production-test-password-295\n');
  assert.equal(await stat(join(fixture.project,'dist/manifest.json')).then(()=>true,()=>false),false);
  let server=await fixture.start('run');
  const manifest=JSON.parse(await readFile(join(fixture.project,'dist/manifest.json'),'utf8'));
  browser=await chromium.launch({headless:true});const page=await browser.newPage();
  await page.goto(server.origin+'/customers');
  await page.getByLabel('客户姓名',{exact:false}).fill('生产客户');
  const pending=page.waitForResponse(response=>response.url().endsWith('/api/customers')&&response.request().method()==='POST');
  await page.getByRole('button',{name:'提交信息',exact:true}).click();const created=await pending;assert.equal(created.status(),201);const id=(await created.json()).data.id;
  await page.goto(server.origin+'/admin/login');await page.getByLabel('用户名',{exact:false}).fill('admin');await page.getByLabel('密码',{exact:false}).fill('production-test-password-295');await page.getByRole('button',{name:'登录',exact:true}).click();
  await expect(page.getByRole('cell',{name:'生产客户',exact:true})).toBeVisible();
  await server.stop();
  const entry=join(fixture.project,'src/app.tsx');await writeFile(entry,(await readFile(entry,'utf8')).replace('客户登记','生产登记'));
  server=await fixture.start('run');
  const rebuilt=JSON.parse(await readFile(join(fixture.project,'dist/manifest.json'),'utf8'));assert.notEqual(rebuilt.build_id,manifest.build_id);
  await page.goto(server.origin+'/customers');await expect(page.getByRole('heading',{name:'生产登记',exact:true})).toBeVisible();
  assert.equal((await page.request.get(server.origin+`/api/customers/${id}`)).status(),200);
  const started=Date.now();await server.stop();
  // 两个移动目标都由本夹具创建，且仍在同一个 mkdtemp 根中，保留源码与缓存供最后清理。
  await rename(join(fixture.project,'src'),join(fixture.root,'source-archive'));
  await rename(join(fixture.project,'.kenvoy'),join(fixture.root,'cache-archive'));
  const pathWithoutNode=process.platform==='win32'?join(process.env.SystemRoot,'System32'):'/usr/bin:/bin';
  server=await fixture.start('run',[],45000,{PATH:pathWithoutNode,Path:pathWithoutNode});
  await page.goto(server.origin+'/admin/customers');await expect(page.getByRole('cell',{name:'生产客户',exact:true})).toBeVisible();
  assert.equal((await page.request.get(server.origin+'/_kenvoy/development')).status(),404);
  await fixture.command(['--project','app','admin','add-user','--username','operator','--role','user','--password-stdin'],'artifact-user-password-591\n');
  assert.equal(JSON.parse(await readFile(join(fixture.project,'dist/manifest.json'),'utf8')).build_id,rebuilt.build_id);
  console.log(JSON.stringify({status:'通过',run_auto_build:true,source_change_rebuild:true,shared_database:true,artifact_only_without_node:true,artifact_only_account_management:true,production_dev_endpoint_absent:true,artifact_start_ms:Date.now()-started},null,2));
  await server.stop();
} finally {try {if(browser)await browser.close();} finally {await fixture.close();}}
