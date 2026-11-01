//! P02 的隔离进程、白名单模块与 TypeScript 夹具。

use rquickjs::{
    Ctx, Module, Runtime,
    loader::{ImportAttributes, Loader, Resolver},
    module::Declared,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    process::{Child, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use swc_common::{FileName, GLOBALS, Globals, Mark, SourceMap, sync::Lrc};
use swc_ecma_ast::EsVersion;
use swc_ecma_codegen::to_code_default;
use swc_ecma_parser::{Syntax, TsSyntax, parse_file_as_program};
use swc_ecma_transforms_base::{fixer::fixer, resolver};
use swc_ecma_transforms_typescript::strip;

use super::{Denials, ENTRY, MAIN_TS, MATH_TS, ProbeResult, ROOT};

/// 即使引擎的 interrupt 回归失效，也由独立测试进程的宿主负责终止和回收。
pub(super) struct InterruptChildGuard(pub(super) Child);

impl InterruptChildGuard {
    pub(super) fn wait_until(&mut self, deadline: Instant) -> io::Result<Option<ExitStatus>> {
        loop {
            if self.0.try_wait()?.is_some() {
                return self.0.wait().map(Some);
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            self.0.wait()?;
            return Ok(());
        }
        if let Err(error) = self.0.kill()
            && self.0.try_wait()?.is_none()
        {
            return Err(error);
        }
        self.wait_until(Instant::now() + Duration::from_secs(2))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "P02 子进程终止后未及时退出"))?;
        Ok(())
    }
}

impl Drop for InterruptChildGuard {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("P02 子进程回收失败：{error}");
        }
    }
}

fn transpile_typescript(name: &str, source: &str) -> ProbeResult<String> {
    let source_map: Lrc<SourceMap> = Default::default();
    let file = source_map.new_source_file(FileName::Custom(name.into()).into(), source.to_owned());
    let mut recovered = Vec::new();
    let program = parse_file_as_program(
        &file,
        Syntax::Typescript(TsSyntax::default()),
        EsVersion::latest(),
        None,
        &mut recovered,
    )
    .map_err(|error| format!("P02 TypeScript 解析失败：{error:?}"))?;
    if !recovered.is_empty() {
        return Err(format!("P02 TypeScript 存在恢复错误：{recovered:?}").into());
    }
    Ok(GLOBALS.set(&Globals::default(), || {
        let unresolved = Mark::new();
        let top_level = Mark::new();
        let javascript = program
            .apply(resolver(unresolved, top_level, true))
            .apply(strip(unresolved, top_level))
            .apply(fixer(None));
        to_code_default(source_map, None, &javascript)
    }))
}

/// 只解析逻辑模块 ID，不访问磁盘，也不提供任意包或原生模块回退。
pub(super) struct AllowlistResolver {
    pub(super) modules: BTreeSet<String>,
    pub(super) denied: Denials,
}

impl Resolver for AllowlistResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        let candidate = normalize_relative(base, name);
        if attributes.is_none()
            && let Some(candidate) = candidate
            && self.modules.contains(&candidate)
        {
            return Ok(candidate);
        }
        self.denied
            .lock()
            .map_err(|_| rquickjs::Error::Unknown)?
            .push(name.to_owned());
        Err(rquickjs::Error::new_resolving_message(
            base,
            name,
            "P02 模块不在服务端白名单内",
        ))
    }
}

fn normalize_relative(base: &str, name: &str) -> Option<String> {
    if (!name.starts_with("./") && !name.starts_with("../"))
        || name.contains(['\\', ':', '%', '\0'])
    {
        return None;
    }
    let (directory, _) = base.rsplit_once('/')?;
    let mut segments: Vec<&str> = directory
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    for segment in name.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    let candidate = format!("/{}", segments.join("/"));
    candidate
        .starts_with(&format!("{ROOT}/"))
        .then_some(candidate)
}

pub(super) struct MemoryLoader {
    pub(super) modules: BTreeMap<String, String>,
    pub(super) dropped: Arc<AtomicUsize>,
}

impl Loader for MemoryLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if attributes.is_some() {
            return Err(rquickjs::Error::new_loading_message(
                name,
                "P02 不接受导入属性",
            ));
        }
        let source = self
            .modules
            .get(name)
            .ok_or_else(|| rquickjs::Error::new_loading_message(name, "P02 模块未注册"))?;
        Module::declare(ctx.clone(), name, source.as_bytes())
    }
}

impl Drop for MemoryLoader {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) fn fixture_modules() -> ProbeResult<BTreeMap<String, String>> {
    Ok(BTreeMap::from([
        (ENTRY.to_owned(), transpile_typescript("main.ts", MAIN_TS)?),
        (
            format!("{ROOT}/math.js"),
            transpile_typescript("math.ts", MATH_TS)?,
        ),
    ]))
}

pub(super) fn install_loader(
    runtime: &Runtime,
    modules: BTreeMap<String, String>,
    denied: Denials,
    dropped: Arc<AtomicUsize>,
) {
    runtime.set_loader(
        AllowlistResolver {
            modules: modules.keys().cloned().collect(),
            denied,
        },
        MemoryLoader { modules, dropped },
    );
}
