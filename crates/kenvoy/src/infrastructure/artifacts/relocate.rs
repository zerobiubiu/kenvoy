use std::path::Path;

use super::{Result, invalid};
use crate::domain::model::ApplicationIR;

pub(super) fn relocate(application: &mut ApplicationIR, root: &Path) -> Result<()> {
    use crate::domain::model::{ComponentDefinition as C, NavigationItem as N};
    fn source(source: &mut crate::domain::source::SourceLocation, root: &Path) -> Result<()> {
        source.path = source
            .path
            .strip_prefix(root)
            .map_err(|_| invalid("应用源码位置越过项目根"))?
            .to_owned();
        Ok(())
    }
    fn components(nodes: &mut [C], root: &Path) -> Result<()> {
        for node in nodes {
            match node {
                C::Form { source: s, .. }
                | C::Table { source: s, .. }
                | C::Image { source: s, .. }
                | C::Text { source: s, .. } => source(s, root)?,
                C::Group {
                    source: s,
                    children,
                    ..
                }
                | C::Custom {
                    source: s,
                    children,
                    ..
                } => {
                    source(s, root)?;
                    components(children, root)?;
                }
            }
        }
        Ok(())
    }
    fn navigation(nodes: &mut [N], root: &Path) -> Result<()> {
        for node in nodes {
            match node {
                N::Link { source: s, .. } => source(s, root)?,
                N::Group {
                    source: s, items, ..
                } => {
                    source(s, root)?;
                    navigation(items, root)?;
                }
            }
        }
        Ok(())
    }
    for resource in &mut application.resources {
        source(&mut resource.source, root)?;
        for field in &mut resource.fields {
            source(&mut field.source, root)?;
        }
    }
    for page in &mut application.pages {
        source(&mut page.source, root)?;
        components(&mut page.components, root)?;
    }
    for action in &mut application.actions {
        source(&mut action.source, root)?;
    }
    for nav in &mut application.navigation {
        source(&mut nav.source, root)?;
        navigation(&mut nav.items, root)?;
    }
    for reference in &mut application.module_references {
        source(&mut reference.source, root)?;
        if !reference.external {
            reference.module = reference
                .module
                .strip_prefix(root)
                .map_err(|_| invalid("模块位置越过项目根"))?
                .to_owned();
        }
    }
    Ok(())
}
