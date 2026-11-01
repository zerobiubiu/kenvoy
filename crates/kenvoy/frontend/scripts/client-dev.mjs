// 开发服务器只读取已验收镜像；所有更新均由 Rust 通过私有 stdin 管道提交。
import { readFile, realpath } from 'node:fs/promises';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { timingSafeEqual } from 'node:crypto';
import { createServer } from 'vite';
import { configuration } from './client-builder.mjs';

const base = '/_kenvoy/vite/';
const key = 'x-kenvoy-vite-secret';
const inside = (file, root) => { const relative = path.relative(root, file); return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative)); };
const canonicalKey = file => process.platform === 'win32' ? file.replaceAll('\\', '/').toLowerCase() : file;
const deNamespace = file => file.startsWith('\\\\?\\') ? file.slice(4) : file;
const emit = value => process.stdout.write(`${JSON.stringify(value)}\n`);

async function main() {
  const initial = JSON.parse(await readFile(process.argv[2], 'utf8'));
  const secret = Buffer.from(initial.secret);
  if (!/^[a-f0-9]{64}$/.test(initial.secret)) throw new Error('开发服务器内部凭据无效');
  const origin = new URL(initial.public_origin);
  let request = initial.request;
  request.development_cache = initial.cache;
  let current = await configuration(request);
  let boundary = current.config.plugins[0];
  const framework = await realpath(request.framework_root);
  const mirror = await realpath(request.mirror_root);
  const toolchainDependencies = await realpath(path.join(request.toolchain_root, 'node_modules'));
  const projectDependencies = await realpath(path.join(request.project_root, 'node_modules')).catch(() => null);
  const cache = await realpath(initial.cache);
  let allowed = new Set(initial.files.map(file => canonicalKey(path.join(mirror, file))));
  let preamble = '';
  let closed = false;
  const authenticate = req => {
    const actual = req.headers[key];
    return typeof actual === 'string' && Buffer.byteLength(actual) === secret.length && timingSafeEqual(Buffer.from(actual), secret);
  };
  const logger = { info(message) { process.stderr.write(`${message}\n`); }, warn(message) { process.stderr.write(`${message}\n`); }, warnOnce(message) { this.warn(message); }, error(message) { process.stderr.write(`${message}\n`); }, clearScreen() {}, hasErrorLogged() { return false; }, hasWarned: false };
  const config = { ...current.config,
    base,
    customLogger: logger,
    cacheDir: cache,
    // 禁止自动观察真实项目或镜像；只有完整候选提交后主动 emit。
    server: { host: '127.0.0.1', port: 0, strictPort: true, open: false, cors: false, watch: null,
      origin: origin.origin, allowedHosts: [origin.hostname],
      fs: { strict: true, allow: [framework, mirror, toolchainDependencies, ...(projectDependencies ? [projectDependencies] : []), cache] },
      hmr: { protocol: origin.protocol === 'https:' ? 'wss' : 'ws', host: origin.hostname, clientPort: Number(origin.port || (origin.protocol === 'https:' ? 443 : 80)), path: 'hmr', overlay: true },
    },
    plugins: [{ name: 'kenvoy-development-gate', enforce: 'pre',
      async resolveId(...args) { if (path.isAbsolute(args[0]) && inside(args[0].split('?')[0],cache)) return null; return await boundary.resolveId.apply(this,args); },
      async load(...args) { if (path.isAbsolute(args[0]) && inside(args[0].split('?')[0],cache)) return null; return await boundary.load.apply(this,args); },
      configureServer(server) {
        server.middlewares.use(async (req,res,next) => {
          if (!authenticate(req)) { res.statusCode=403;res.end('开发资源仅允许宿主代理');return; }
          if (req.method !== 'GET' && req.method !== 'HEAD') { res.statusCode=405;res.end();return; }
          let pathname;
          try { pathname=decodeURIComponent(new URL(req.url,'http://localhost').pathname); } catch {res.statusCode=400;res.end();return;}
          if (!pathname.startsWith(base) || pathname.includes('\\') || pathname.includes('\0')) {res.statusCode=404;res.end();return;}
          const relative=pathname.slice(base.length);
          if (relative === 'preamble.js') {res.setHeader('content-type','text/javascript; charset=utf-8');res.setHeader('cache-control','no-store');res.end(preamble);return;}
          if (['','index.html','@vite/client','@vite/env','@react-refresh'].includes(relative) || relative.startsWith('@id/')) { next();return; }
          let file;
          if (relative.startsWith('@fs/')) file=relative.slice(4);
          else file=path.join(framework,relative);
          // 首次依赖优化仍在 deps_temp 写入时，交给 Vite 等待完成，不能抢先返回 404。
          const optimized=path.relative(cache,path.resolve(file)).replaceAll('\\','/');
          if (/^deps\/[A-Za-z0-9_$@.-]+\.js(?:\.map)?$/.test(optimized)) {
            try {const parent=await realpath(path.dirname(file));if(!inside(parent,cache)){res.statusCode=404;res.end();return;}}
            catch(error){if(error.code!=='ENOENT'){res.statusCode=404;res.end();return;}}
            next();return;
          }
          try { file=await realpath(file); } catch {res.statusCode=404;res.end();return;}
          const frameworkFile=inside(file,path.join(framework,'src')) && /\.(?:[cm]?[jt]sx?|css)$/.test(file);
          const dependency=inside(file,toolchainDependencies) || (projectDependencies && inside(file,projectDependencies));
          const optimizer=inside(file,cache) && /\.(?:js|map)$/.test(file);
          if (!(frameworkFile || dependency || optimizer || allowed.has(canonicalKey(file)))) {res.statusCode=404;res.end();return;}
          next();
        });
        server.httpServer.prependListener('upgrade',(req,socket)=>{
          if (!authenticate(req)) {socket.write('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');socket.destroy();}
        });
      },
    },...current.config.plugins.slice(1),{
      name:'kenvoy-external-refresh-preamble',
      transformIndexHtml: { order:'post',handler(html) {
        const blocks=[];
        const result=html.replace(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi,(all,attrs,source)=>{
          if (!source.trim() || /\bsrc\s*=/.test(attrs)) return all;
          if (!/\btype=["']module["']/.test(attrs)) throw new Error('开发入口出现未知 inline script');
          blocks.push(source); return `<script type="module" src="${base}preamble.js"></script>`;
        });
        preamble=blocks.join('\n');return result;
      }},
    }],
  };
  const server=await createServer(config);
  await server.listen();
  const address=server.httpServer.address();
  emit({id:0,ok:true,port:address.port});
  const input=createInterface({input:process.stdin,crlfDelay:Infinity});
  const shutdown=async()=>{if(closed)return;closed=true;await server.close();input.close();};
  try {
    for await (const line of input) {
      if (Buffer.byteLength(line)>4096) throw new Error('开发控制指令超过限制');
      let command;
      try {
        command=JSON.parse(line);
        if(command.type==='shutdown'){await shutdown();emit({id:command.id,ok:true});break;}
        if(command.type!=='apply')throw new Error('未知开发控制命令');
        const candidate=JSON.parse(await readFile(command.path,'utf8'));
        if(canonicalKey(deNamespace(candidate.request.mirror_root))!==canonicalKey(mirror))throw new Error('镜像目录不能改变');
        candidate.request.development_cache=cache;
        const replacement=await configuration(candidate.request);
        request=candidate.request;current=replacement;boundary=replacement.config.plugins[0];
        allowed=new Set(candidate.files.map(file=>canonicalKey(path.join(mirror,file))));
        if(command.structural){server.moduleGraph.invalidateAll();server.ws.send({type:'full-reload',path:'*'});}
        else for(const name of candidate.changed){server.watcher.emit('change',path.join(mirror,name));}
        emit({id:command.id,ok:true});
      } catch(error){emit({id:command?.id??null,ok:false,error:error instanceof Error?error.message:'开发更新失败'});}
    }
  } finally {await shutdown();}
}

main().catch(error=>{process.stderr.write(`${error instanceof Error?error.message:'开发服务失败'}\n`);process.exitCode=1;});
