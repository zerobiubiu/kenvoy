import test from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const frontend = fileURLToPath(new URL('../', import.meta.url));
const fixtures = fileURLToPath(new URL('./types/', import.meta.url));
const compiler = join(frontend, 'node_modules/typescript/bin/tsc');

function compile(args, cwd) {
  const result = spawnSync(process.execPath, [compiler, ...args, '--pretty', 'false', '--locale', 'en'], {
    cwd, encoding: 'utf8', timeout: 60_000, windowsHide: true,
  });
  assert.ifError(result.error);
  assert.equal(result.signal, null, '类型检查不得被信号中断');
  return { status: result.status, output: `${result.stdout}${result.stderr}` };
}

function configuration(root, files, jsx = 'react-jsx') {
  const path = join(root, 'tsconfig.json');
  writeFileSync(path, JSON.stringify({
    compilerOptions: {
      target: 'ES2022', lib: ['ES2022', 'DOM', 'DOM.Iterable'], module: 'ESNext',
      moduleResolution: 'Bundler', strict: true, noEmit: true, jsx,
      jsxImportSource: '@kenvoy/client', types: ['react', 'react-dom'],
      allowImportingTsExtensions: true, verbatimModuleSyntax: true, erasableSyntaxOnly: true,
      paths: {
        '@kenvoy/client': ['./.kenvoy/types/sdk.d.ts'],
        '@kenvoy/client/jsx-runtime': ['./.kenvoy/types/presentation/jsx-runtime.d.ts'],
        '@kenvoy/client/jsx-dev-runtime': ['./.kenvoy/types/presentation/jsx-dev-runtime.d.ts'],
      },
    },
    files: files.map(file => `./src/${file}`),
  }, null, 2));
  return path;
}

function expectedErrors(files) {
  const expected = new Map();
  for (const file of files) {
    const lines = readFileSync(join(fixtures, file), 'utf8').split('\n');
    lines.forEach((line, index) => {
      const marker = /\/\/ @error ([a-z-]+)/.exec(line);
      if (marker) expected.set(`${file}:${index + 1}`, marker[1]);
    });
  }
  return expected;
}

test('真实 SDK 声明在仅有开发类型依赖的应用内保持严格 JSX 与 API 检查', { timeout: 180_000 }, () => {
  const root = mkdtempSync(join(tmpdir(), 'kenvoy-editor-types-'));
  try {
    mkdirSync(join(root, 'node_modules'), { recursive: true });
    const linkType = process.platform === 'win32' ? 'junction' : 'dir';
    // 不链接 React 运行包或 frontend 源码，声明只依赖开发类型及其真实传递依赖。
    for (const dependency of ['@types', 'csstype']) {
      symlinkSync(join(frontend, 'node_modules', dependency), join(root, 'node_modules', dependency), linkType);
    }
    const emitted = compile(['-p', join(frontend, 'tsconfig.types.json'), '--outDir', join(root, '.kenvoy/types')], frontend);
    assert.equal(emitted.status, 0, emitted.output);
    const positive = 'editor-positive.tsx';
    const negatives = ['editor-negative.tsx', 'editor-react-negative.tsx'];
    mkdirSync(join(root, 'src'));
    for (const file of [positive, ...negatives]) cpSync(join(fixtures, file), join(root, 'src', file));
    for (const jsx of ['react-jsx', 'react-jsxdev']) {
      const result = compile(['-p', configuration(root, [positive], jsx)], root);
      assert.equal(result.status, 0, `${jsx}\n${result.output}`);
    }
    const rejected = compile(['-p', configuration(root, negatives)], root);
    assert.notEqual(rejected.status, 0, '非法 SDK、DSL 和 React props 必须产生真实诊断');
    const expected = expectedErrors(negatives);
    assert.ok(expected.size > 0, '反向素材必须声明诊断位置');
    const diagnostics = [...rejected.output.matchAll(/([^\r\n]+\.tsx)\((\d+),\d+\): error TS\d+:/g)];
    const actual = new Set();
    for (const diagnostic of diagnostics) {
      const location = `${basename(diagnostic[1].trim())}:${diagnostic[2]}`;
      assert.ok(expected.has(location), `非预期诊断：${location}\n${rejected.output}`);
      actual.add(location);
    }
    assert.equal(diagnostics.length, [...rejected.output.matchAll(/error TS\d+:/g)].length, rejected.output);
    for (const [location, name] of expected) assert.ok(actual.has(location), `缺少诊断 ${name}（${location}）\n${rejected.output}`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
