// 仅由 Rust 宿主以参数文件调用；不加载用户 Vite 配置、环境文件或 npm 安装脚本。
import { readFile, realpath, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { builtinModules } from 'node:module';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { build } from 'vite';
import react from '@vitejs/plugin-react';

// 只共享 React 基础运行时；组件库从应用自己的显式依赖解析。
const frameworkPackages = new Set(['react', 'react-dom', 'react-router']);
const builtins = new Set(builtinModules.flatMap(name => [name, `node:${name}`]));
const filesystemPath = value => value.startsWith('\\\\?\\UNC\\') ? `\\\\${value.slice(8)}` : value.startsWith('\\\\?\\') ? value.slice(4) : value;
const normalized = value => {
  const result = path.resolve(filesystemPath(value)).replaceAll('\\', '/').replace(/\/$/, '');
  return process.platform === 'win32' ? result.toLowerCase() : result;
};
const under = (file, directory) => {
  const relative = path.relative(directory, file);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
};
const packageName = value => value.startsWith('@') ? value.split('/').slice(0, 2).join('/') : value.split('/')[0];
const bare = value => !value.startsWith('.') && !path.isAbsolute(value) && !value.startsWith('\0') && !value.startsWith('virtual:');

export async function configuration(request) {
  if (request.version !== 1) throw new Error('客户端构建请求版本不受支持');
  const projectRoot = await realpath(request.project_root);
  const frameworkRoot = await realpath(request.framework_root);
  const toolchainRoot = await realpath(request.toolchain_root);
  const destination = await realpath(request.destination);
  const mirrorRoot = request.mirror_root ? await realpath(request.mirror_root) : null;
  const developmentCache = request.development_cache ? await realpath(request.development_cache) : null;
  if (!under(destination, projectRoot) || destination === projectRoot) throw new Error('客户端输出必须位于项目 staging 内');
  const privateModules = new Set(request.private_modules.map(normalized));
  const declarations = new Set(request.declaration_modules.map(normalized));
  const sourceModules = new Map(Object.entries(request.source_modules).map(([file, hash]) => [normalized(file), hash]));
  const allowedPackages = new Set(request.external_packages);
  const registryId = '\0kenvoy-registry';
  const slices = new Map();
  const imports = [];
  const components = [], functions = [], expressions = [];
  for (const [index, reference] of request.references.entries()) {
    const local = `kenvoy_ref_${index}`;
    let target;
    let exportName = reference.export_name;
    if (reference.slice) {
      target = `virtual:kenvoy-slice/${index}.jsx`;
      slices.set(`\0${target}`, reference.slice);
      exportName = reference.slice.export_name;
    } else {
      target = path.isAbsolute(reference.module) ? filesystemPath(reference.module).replaceAll('\\', '/') : reference.module;
    }
    if (typeof exportName !== 'string' || !exportName) throw new Error('客户端引用缺少明确导出名称');
    // 字符串导出名称使用 ESM 的字符串语法，不能拼接未转义的源码标识符。
    imports.push(`import { ${JSON.stringify(exportName)} as ${local} } from ${JSON.stringify(target)};`);
    const entry = `${JSON.stringify(reference.id)}:${local}`;
    if (reference.kind === 'component') components.push(entry);
    else if (reference.kind === 'expression') expressions.push(entry);
    else functions.push(entry);
  }
  const registry = `${imports.join('\n')}\nexport const components={${components}};\nexport const functions={${functions}};\nexport const expressions={${expressions}};`;
  const loaded = new Set();
  async function inspectFile(id) {
    if (id.startsWith('\0')) return;
    const file = id.split('?')[0].split('#')[0];
    if (!path.isAbsolute(file)) return;
    let canonical;
    try { canonical = await realpath(file); } catch { return; }
    const key = normalized(canonical);
    if (privateModules.has(key) || /\.server\./i.test(canonical) || declarations.has(key) || /\.diu\.[cm]?[jt]sx?$/i.test(canonical)) throw new Error('客户端构建拒绝加载服务端私有模块或声明入口');
    if (under(canonical, frameworkRoot)) {
      if (mirrorRoot && !under(canonical, path.join(frameworkRoot, 'src'))) throw new Error('开发模式不公开框架构建脚本');
      loaded.add(canonical); return;
    }
    if (developmentCache && under(canonical, developmentCache) && /\.(?:js|map)$/.test(canonical)) { loaded.add(canonical); return; }
    const frameworkDependency = under(canonical, path.join(toolchainRoot, 'node_modules'));
    const projectDependency = under(canonical, path.join(projectRoot, 'node_modules'));
    if (frameworkDependency || projectDependency) { loaded.add(canonical); return; }
    if (mirrorRoot && !under(canonical, mirrorRoot)) throw new Error('开发模式只能读取已经发布的客户端镜像');
    if (!under(canonical, projectRoot)) throw new Error('客户端模块或资源越过项目根目录');
    if ((!mirrorRoot || !under(canonical, mirrorRoot)) && (canonical.split(path.sep).some(part => part === '.git' || part === '.kenvoy' || part.startsWith('.env')) || /(?:^|[/\\])kenvoy\.toml$/i.test(canonical))) throw new Error('客户端不得加载项目内部配置或私有缓存');
    const extension = path.extname(canonical).toLowerCase();
    if (['.ts', '.tsx', '.js', '.jsx', '.mts', '.cts', '.mjs', '.cjs'].includes(extension)) {
      if (!sourceModules.has(key)) throw new Error('客户端源模块不在已经分析的模块图中');
    } else if (!['.css', '.svg', '.png', '.jpg', '.jpeg', '.gif', '.webp', '.avif', '.ico', '.woff', '.woff2', '.ttf', '.otf'].includes(extension)) {
      throw new Error('客户端资源类型尚未开放；不能把私有数据作为 raw 资源打包');
    }
    loaded.add(canonical);
    return canonical;
  }
  const config = {
    configFile: false,
    root: frameworkRoot,
    envDir: false,
    envPrefix: [],
    publicDir: false,
    base: '/',
    clearScreen: false,
    css: { postcss: { plugins: [] } },
    plugins: [
      {
        name: 'kenvoy-client-boundary',
        enforce: 'pre',
        async resolveId(id, importer) {
          if (builtins.has(id) || id.startsWith('node:') || /^(?:https?|file|data):/i.test(id)) throw new Error('客户端模块不能访问 Node 内置模块或远程模块');
          if (id === 'virtual:kenvoy-registry') return registryId;
          if (id.startsWith('virtual:kenvoy-slice/')) return `\0${id}`;
          if (id === '@kenvoy/client') return path.join(frameworkRoot, 'src/sdk.ts');
          if (id === '@kenvoy/client/jsx-runtime') return path.join(frameworkRoot, 'src/presentation/jsx-runtime.ts');
          if (id === '@kenvoy/client/jsx-dev-runtime') return path.join(frameworkRoot, 'src/presentation/jsx-dev-runtime.ts');
          const slice = slices.get(importer);
          if (slice) {
            const target = slice.imports[id];
            if (!target && !frameworkPackages.has(packageName(id))) throw new Error('客户端切片出现未登记的模块导入');
            if (target?.kind === 'file') { const file = filesystemPath(target.module).replaceAll('\\', '/'); await inspectFile(file); return file; }
            if (target) id = target.module;
          }
          const name = packageName(id);
          if (bare(id) && frameworkPackages.has(name)) {
            const resolved = await this.resolve(id, path.join(toolchainRoot, 'package.json'), { skipSelf: true });
            if (!resolved) throw new Error(`框架依赖 ${name} 尚未准备完成`);
            await inspectFile(resolved.id);
            return resolved;
          }
          const userImporter = importer && (importer === registryId || slices.has(importer) || (under(importer.split('?')[0], projectRoot) && !importer.includes('/node_modules/') && !importer.includes('\\node_modules\\') && !under(importer.split('?')[0], frameworkRoot)));
          if (bare(id) && userImporter) {
            if (!allowedPackages.has(name)) throw new Error(`用户依赖 ${name} 未在 package.json 声明`);
            const resolved = await this.resolve(id, path.join(projectRoot, 'package.json'), { skipSelf: true });
            if (!resolved) throw new Error(`用户依赖 ${name} 未安装；请显式使用 --install-deps`);
            await inspectFile(resolved.id);
            return resolved;
          }
          if (path.isAbsolute(id)) await inspectFile(id);
          return null;
        },
        async load(id) {
          if (id === registryId) return registry;
          if (slices.has(id)) return slices.get(id).code;
          const file = await inspectFile(id);
          // 把分析时的源码快照作为本次 load 结果，避免校验后再次读取不同内容。
          if (file && sourceModules.has(normalized(file)) && !id.includes('?')) {
            const source = await readFile(file);
            if (createHash('sha256').update(source).digest('hex') !== sourceModules.get(normalized(file))) throw new Error('分析后客户端源码已改变，请重新分析');
            return source.toString('utf8');
          }
          return null;
        },
        async generateBundle(_options, bundle) {
          // CSS url()/插件产生的资源也必须经过路径校验，不能只审 JS import。
          for (const output of Object.values(bundle)) {
            if (output.type === 'asset') for (const original of output.originalFileNames ?? []) await inspectFile(path.resolve(frameworkRoot, original));
          }
        },
      },
      react(),
    ],
    resolve: { dedupe: [...frameworkPackages] },
    build: { outDir: destination, emptyOutDir: false, manifest: false, sourcemap: false, assetsInlineLimit: 0 },
  };
  const dependencies = [];
  for (const name of [...frameworkPackages, 'vite', '@vitejs/plugin-react', 'rolldown']) {
    const metadata = JSON.parse(await readFile(path.join(toolchainRoot, 'node_modules', name, 'package.json'), 'utf8'));
    dependencies.push({ name, version: metadata.version, scope: 'framework' });
  }
  for (const name of allowedPackages) {
    if (frameworkPackages.has(name) || name === '@kenvoy/client') continue;
    try {
      const metadata = JSON.parse(await readFile(path.join(projectRoot, 'node_modules', name, 'package.json'), 'utf8'));
      dependencies.push({ name, version: metadata.version, scope: 'project' });
    } catch { /* 只记录已安装的允许包，未被引用的 devDependency 不触发隐式安装。 */ }
  }
  return { config, dependencies, loaded };
}

async function main() {
  if (process.argv[2] === '--probe') {
    console.log(JSON.stringify({ node: process.versions.node, platform: process.platform, architecture: process.arch, vite: true }));
    return;
  }
  const input = process.argv[2];
  if (!input) throw new Error('客户端构建缺少请求文件');
  const request = JSON.parse(await readFile(input, 'utf8'));
  const { config, dependencies, loaded } = await configuration(request);
  await build(config);
  await stat(path.join(request.destination, 'index.html'));
  await writeFile(request.report, JSON.stringify({ version: 1, dependencies, loaded_modules: loaded.size }), { flag: 'wx' });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main().catch(error => { console.error(error instanceof Error ? error.message : '客户端构建失败'); process.exitCode = 1; });
