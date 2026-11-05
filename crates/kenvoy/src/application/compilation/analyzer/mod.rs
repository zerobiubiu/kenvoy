mod application;
mod attributes;
mod boundaries;
mod components;
mod fields;
mod pages;
mod references;

use std::{collections::BTreeMap, path::Path};

use kengine::ProgramIR;

use crate::domain::{
    model::{
        APPLICATION_IR_VERSION, ActionDefinition, ApplicationIR, ModuleReference,
        NavigationDefinition, Operation, PageDefinition, PermissionDefinition, PermissionTarget,
        ResourceDefinition, Role,
    },
    source::{DslDiagnostic, SourceLocation},
};

use super::{client_slice::ClientCodeTransformer, static_eval::Evaluator};
use attributes::start_of_file;

#[derive(Debug)]
pub struct AnalyzeResult {
    pub application: Option<ApplicationIR>,
    pub diagnostics: Vec<DslDiagnostic>,
}

impl AnalyzeResult {
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

type Result<T> = std::result::Result<T, DslDiagnostic>;

pub fn analyze(
    program: &ProgramIR,
    entry: &Path,
    default_name: &str,
    transformer: &impl ClientCodeTransformer,
) -> AnalyzeResult {
    let nodes = match Evaluator::new(program, transformer).entry(entry) {
        Ok(nodes) => nodes,
        Err(error) => {
            return AnalyzeResult {
                application: None,
                diagnostics: vec![error],
            };
        }
    };
    let entry_source = program
        .modules
        .iter()
        .find(|module| module.path == entry)
        .map(|module| SourceLocation {
            path: entry.into(),
            span: module
                .exports
                .first()
                .map(|export| export.span.clone())
                .unwrap_or(kengine::SourceSpan {
                    start: 0,
                    end: 0,
                    start_position: kengine::Position { line: 1, column: 1 },
                    end_position: kengine::Position { line: 1, column: 1 },
                }),
        })
        .unwrap_or_else(|| SourceLocation {
            path: entry.into(),
            span: start_of_file(),
        });
    let mut analyzer = Analyzer {
        program,
        transformer,
        entry_source,
        resources: BTreeMap::new(),
        pages: vec![],
        actions: vec![],
        navigation: vec![],
        permissions: vec![],
        modules: vec![],
        component_overrides: BTreeMap::new(),
        diagnostics: vec![],
    };
    let name = match analyzer.root(&nodes, default_name) {
        Ok(name) => name,
        Err(error) => {
            analyzer.diagnostics.push(error);
            return AnalyzeResult {
                application: None,
                diagnostics: analyzer.diagnostics,
            };
        }
    };
    analyzer.check_boundaries();
    if !analyzer.diagnostics.is_empty() {
        return AnalyzeResult {
            application: None,
            diagnostics: analyzer.diagnostics,
        };
    }
    for resource in analyzer.resources.values() {
        for operation in [
            Operation::Create,
            Operation::List,
            Operation::Read,
            Operation::Update,
            Operation::Delete,
        ] {
            analyzer.permissions.push(PermissionDefinition {
                target: PermissionTarget::Resource(resource.name.clone()),
                operation,
                role: Role::Admin,
                fields: resource
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect(),
                form: None,
            });
        }
    }
    AnalyzeResult {
        application: Some(ApplicationIR {
            format_version: APPLICATION_IR_VERSION,
            name,
            resources: analyzer.resources.into_values().collect(),
            pages: analyzer.pages,
            actions: analyzer.actions,
            navigation: analyzer.navigation,
            permissions: analyzer.permissions,
            module_references: analyzer.modules,
            component_overrides: analyzer.component_overrides,
        }),
        diagnostics: vec![],
    }
}

struct Analyzer<'a, T> {
    program: &'a ProgramIR,
    transformer: &'a T,
    entry_source: SourceLocation,
    resources: BTreeMap<String, ResourceDefinition>,
    pages: Vec<PageDefinition>,
    actions: Vec<ActionDefinition>,
    navigation: Vec<NavigationDefinition>,
    permissions: Vec<PermissionDefinition>,
    modules: Vec<ModuleReference>,
    component_overrides: BTreeMap<String, String>,
    diagnostics: Vec<DslDiagnostic>,
}

#[cfg(test)]
mod tests;
