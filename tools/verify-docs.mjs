import assert from 'node:assert/strict';
import { readFile, readdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const docs = path.join(root, 'docs');
const decoder = new TextDecoder('utf-8', { fatal: true });
const walk = async (directory) => {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) result.push(...await walk(filename));
    else if (entry.name.endsWith('.md')) result.push(filename);
  }
  return result;
};
const files = [...await walk(docs), path.join(root, 'AGENTS.md'), path.join(root, 'README.md')];
const graph = new Map();
let localLinks = 0;
for (const file of files) {
  const content = decoder.decode(await readFile(file));
  assert(!content.includes('\0') && !content.includes('\uFFFD'), `异常字符：${file}`);
  assert(!/[^\S\r\n]+\r?$/m.test(content), `行尾空格：${file}`);
  const targets = [];
  for (const match of content.matchAll(/\[[^\]]*\]\(([^\s)]+)(?:\s+"[^"]*")?\)/g)) {
    const target = match[1];
    if (/^(?:[a-z]+:|#)/i.test(target)) continue;
    const pathname = decodeURIComponent(target.split('#')[0]);
    if (!pathname) continue;
    const resolved = path.resolve(path.dirname(file), pathname);
    assert(await stat(resolved).catch(() => null), `失效链接：${file} -> ${target}`);
    targets.push(resolved);
    localLinks += 1;
  }
  graph.set(file, targets);
}
const reachable = new Set();
const pending = [path.join(docs, 'README.md')];
while (pending.length) {
  const file = pending.pop();
  if (reachable.has(file)) continue;
  reachable.add(file);
  pending.push(...(graph.get(file) ?? []));
}
for (const file of files.filter((item) => item.startsWith(docs + path.sep))) {
  assert(reachable.has(file), `未从 docs/README.md 索引：${file}`);
}
process.stdout.write(JSON.stringify({ status: '通过', markdown_files: files.length, local_links: localLinks }, null, 2) + '\n');
