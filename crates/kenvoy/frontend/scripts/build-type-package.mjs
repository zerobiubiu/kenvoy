import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runNpm } from './type-package-npm.mjs';

const frontend = fileURLToPath(new URL('../', import.meta.url));
const source = join(frontend, 'types-package');
const dist = join(frontend, 'dist');
const destination = join(dist, 'types-package');

function cliPackage() {
  const manifestPath = realpathSync(join(frontend, '../Cargo.toml'));
  const result = spawnSync('cargo', [
    'metadata', '--format-version', '1', '--no-deps', '--offline', '--locked', '--manifest-path', manifestPath,
  ], { cwd: frontend, encoding: 'utf8', timeout: 60_000, windowsHide: true });
  if (result.error) throw new Error(`无法读取 Kenvoy CLI 版本：${result.error.message}`, { cause: result.error });
  if (result.status !== 0) throw new Error(`读取 Kenvoy CLI Cargo metadata 失败：\n${result.stdout}${result.stderr}`);
  const metadata = JSON.parse(result.stdout);
  const cli = metadata.packages.find(pkg =>
    pkg.name === 'kenvoy' && realpathSync(pkg.manifest_path) === manifestPath);
  if (!cli?.targets.some(target => target.name === 'kenvoy' && target.kind.includes('bin'))) {
    throw new Error('Cargo metadata 未包含指定 manifest 的 Kenvoy CLI Binary，拒绝使用其他包的版本');
  }
  return cli;
}

function copyDeclarations(from, to) {
  mkdirSync(to, { recursive: true });
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    const input = join(from, entry.name);
    const output = join(to, entry.name);
    if (entry.isDirectory()) copyDeclarations(input, output);
    else if (entry.isFile() && entry.name.endsWith('.d.ts')) copyFileSync(input, output);
  }
}

function syncExampleDependencies(archive) {
  const examples = resolve(frontend, '../../../examples');
  if (!existsSync(examples)) return;
  // 仅同步本仓库示例的类型开发依赖，不触碰 CLI 创建的用户项目。
  for (const name of ['customer-app', 'extensions-app', 'options-app', 'uploads-app', 'i18n-app']) {
    const directory = join(examples, name);
    const path = join(directory, 'package.json');
    const original = readFileSync(path, 'utf8');
    const manifest = JSON.parse(original);
    if (!Object.hasOwn(manifest.devDependencies ?? {}, '@kenvoy/types')) {
      throw new Error(`示例 ${name} 缺少 @kenvoy/types 开发依赖，拒绝隐式新增其它接线`);
    }
    manifest.devDependencies['@kenvoy/types'] = `file:${relative(directory, archive).replaceAll('\\', '/')}`;
    const updated = `${JSON.stringify(manifest, null, 2)}\n`;
    if (original !== updated) writeFileSync(path, updated);
  }
}

function build() {
  const cli = cliPackage();
  const runtime = JSON.parse(readFileSync(join(frontend, 'package.json'), 'utf8'));
  const reactTypes = runtime.devDependencies['@types/react'];
  if (!/^\d+\.\d+\.\d+$/.test(reactTypes)) throw new Error('frontend 的 @types/react 开发依赖必须固定为实际验证的精确版本');
  if (!cli.license_file) throw new Error('Kenvoy CLI Cargo metadata 缺少 license_file');

  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });
  copyDeclarations(join(dist, 'types'), join(destination, 'sdk'));
  for (const entry of ['sdk.d.ts', 'presentation/jsx-runtime.d.ts', 'presentation/jsx-dev-runtime.d.ts']) {
    if (!existsSync(join(destination, 'sdk', entry))) throw new Error(`缺少真实 SDK 声明：${entry}；请先执行声明生成`);
  }
  for (const file of ['index.d.ts', 'README.md']) copyFileSync(join(source, file), join(destination, file));
  copyFileSync(resolve(dirname(cli.manifest_path), cli.license_file), join(destination, 'LICENSE'));

  const template = JSON.parse(readFileSync(join(source, 'package.template.json'), 'utf8'));
  const manifest = {
    ...template,
    version: cli.version,
    peerDependencies: { '@types/react': `^${reactTypes}` },
    ...(cli.repository ? { repository: cli.repository } : {}),
  };
  writeFileSync(join(destination, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`);

  // npm 只对本地纯声明目录打包，不执行生命周期脚本，也不访问 registry。
  const packResult = JSON.parse(runNpm([
    'pack', destination, '--ignore-scripts', '--offline', '--json', '--pack-destination', dist,
  ], frontend));
  const packed = Array.isArray(packResult) ? packResult
    : packResult && typeof packResult === 'object' ? Object.values(packResult) : [];
  const filename = `kenvoy-types-${cli.version}.tgz`;
  if (packed.length !== 1 || packed[0].name !== '@kenvoy/types'
    || packed[0].version !== cli.version || packed[0].filename !== filename) {
    throw new Error('npm pack 未生成与 CLI 版本对应的类型包');
  }
  const producedArchive = join(dist, filename);
  const digest = createHash('sha256').update(readFileSync(producedArchive)).digest('hex');
  // 本地包按内容寻址，避免 Bun 复用同一路径下的旧 tarball；正式资产仍由同版 crate pack。
  const localFilename = `kenvoy-types-${cli.version}-${digest.slice(0, 16)}.tgz`;
  const archive = join(dist, localFilename);
  renameSync(producedArchive, archive);
  writeFileSync(`${archive}.sha256`, `${digest}  ${localFilename}\n`);
  writeFileSync(join(dist, 'types-artifact.json'), `${JSON.stringify({
    file: localFilename, sha256: digest, version: cli.version,
  }, null, 2)}\n`);
  syncExampleDependencies(archive);
  console.log(`已生成 @kenvoy/types@${cli.version}：dist/types-package、dist/${localFilename} 及 SHA256 摘要`);
}

try {
  build();
} catch (error) {
  console.error(`构建 @kenvoy/types 失败：${error.message}`);
  process.exitCode = 1;
}
