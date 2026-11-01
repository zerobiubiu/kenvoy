use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    application::{
        error::{EngineError, ResolveError},
        options::{ParseOptions, ResolveOptions},
        ports::Host,
    },
    domain::{
        ir::ParseOutput,
        module::{
            ResolvedModule,
            resolution::{SpecifierKind, candidates, classify_specifier},
        },
    },
};

use super::parser;

pub(crate) struct NativeHost;

impl Host for NativeHost {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }

    fn read_source(&self, path: &Path) -> Result<String, EngineError> {
        fs::read_to_string(path).map_err(|source| EngineError::Io {
            path: path.into(),
            source,
        })
    }

    fn parse_source(&self, path: &Path, source: &str, options: ParseOptions) -> ParseOutput {
        parser::parse(path, source, options)
    }

    fn resolve_module(
        &self,
        importer: &Path,
        specifier: &str,
        options: &ResolveOptions,
    ) -> Result<ResolvedModule, ResolveError> {
        resolve(importer, specifier, options)
    }
}

fn resolve(
    importer: &Path,
    specifier: &str,
    options: &ResolveOptions,
) -> Result<ResolvedModule, ResolveError> {
    match classify_specifier(specifier, &options.externals) {
        SpecifierKind::External => {
            return Ok(ResolvedModule::External {
                specifier: specifier.into(),
            });
        }
        SpecifierKind::Unsupported => {
            return Err(ResolveError::UnsupportedSpecifier {
                specifier: specifier.into(),
            });
        }
        SpecifierKind::Relative => {}
    }
    let root = options
        .root
        .canonicalize()
        .map_err(|source| ResolveError::Io {
            path: options.root.clone(),
            source,
        })?;
    let importer = if importer.is_absolute() {
        importer.to_path_buf()
    } else {
        root.join(importer)
    };
    let base = importer.parent().unwrap_or(&root).join(specifier);
    let candidates = candidates(&base, options.map_js_to_ts);
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let path = candidate
            .canonicalize()
            .map_err(|source| ResolveError::Io {
                path: candidate,
                source,
            })?;
        if !path.starts_with(&root) {
            return Err(ResolveError::OutsideRoot { path, root });
        }
        return Ok(ResolvedModule::File { path });
    }
    Err(ResolveError::NotFound {
        importer,
        specifier: specifier.into(),
    })
}
