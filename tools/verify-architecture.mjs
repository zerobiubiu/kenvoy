import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// 验证实际 Cargo 元数据及普通依赖闭包，测试依赖不冒充产品依赖。
const root = fileURLToPath(new URL('../', import.meta.url));
const result = spawnSync('cargo', ['metadata', '--format-version', '1', '--offline', '--locked'], {
  cwd: root, encoding: 'utf8', windowsHide: true, maxBuffer: 16 * 1024 * 1024,
});
assert.equal(result.status, 0, result.error?.message || result.stderr || '无法读取 Cargo 元数据');
const metadata = JSON.parse(result.stdout);
const members = metadata.packages.filter((item) => metadata.workspace_members.includes(item.id));
assert.deepEqual(members.map((item) => item.name).sort(), ['kengine', 'kenvoy']);
const engine = members.find((item) => item.name === 'kengine');
const host = members.find((item) => item.name === 'kenvoy');
assert.equal(engine.targets.filter((item) => item.kind.includes('lib')).length, 1);
assert.equal(engine.targets.some((item) => item.kind.includes('bin')), false);
assert.equal(host.targets.filter((item) => item.kind.includes('bin')).length, 1);
assert.equal(host.targets.some((item) => item.kind.includes('lib')), false);
assert(host.dependencies.some((item) => item.name === 'kengine' && item.kind === null && item.path));
const nodes = new Map(metadata.resolve.nodes.map((item) => [item.id, item]));
const packages = new Map(metadata.packages.map((item) => [item.id, item]));
const visited = new Set();
const pending = [engine.id];
while (pending.length) {
  const id = pending.pop();
  if (visited.has(id)) continue;
  visited.add(id);
  for (const dependency of nodes.get(id).deps) {
    if (dependency.dep_kinds.some((kind) => kind.kind === null)) pending.push(dependency.pkg);
  }
}
const forbidden = new Set(['kenvoy', 'axum', 'tokio', 'tower', 'tower-http', 'rusqlite', 'rquickjs', 'notify']);
for (const id of visited) {
  assert(!forbidden.has(packages.get(id).name), `KEngine 普通依赖越界：${packages.get(id).name}`);
}
process.stdout.write(JSON.stringify({
  status: '通过', workspace: ['kengine:lib', 'kenvoy:bin'], dependency: 'kenvoy -> kengine',
  kengine_normal_dependency_packages: visited.size - 1,
}, null, 2) + '\n');
