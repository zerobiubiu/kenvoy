//! Cargo 产品边界与实际源码分层共同构成架构门禁。

mod cargo_graph;
mod layout;
mod rust;
mod rust_catalog;
mod rust_syntax;
mod rust_visitor;
mod typescript;

use crate::Result;
use std::path::Path;

pub(crate) fn verify(root: &Path) -> Result<()> {
    cargo_graph::verify(root)?;
    let stats = layout::inspect(root)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "通过",
            "owned_source_files": stats.files,
            "rust_source_files": stats.rust,
            "frontend_modules": stats.typescript,
            "maximum_source_lines": stats.maximum_lines,
            "source_line_limit": layout::MAX_LINES,
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests;
