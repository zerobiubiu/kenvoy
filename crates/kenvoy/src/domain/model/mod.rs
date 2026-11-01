//! 经语义校验的宿主业务模型。数据库、HTTP 和客户端投影消费同一资源定义。

mod module;
mod page;
mod permission;
mod resource;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use module::{ActionDefinition, ExecutionEnvironment, ModuleReference, ModuleReferenceKind};
pub use page::{
    ComponentDefinition, FormMode, NavigationDefinition, NavigationItem, PageDefinition, PropValue,
};
pub use permission::{Operation, PermissionDefinition, PermissionTarget, Role};
pub use resource::{
    FieldDefinition, FieldKind, FileConstraints, MAX_UPLOAD_FILES, MAX_UPLOAD_SIZE, OptionsSource,
    ResourceDefinition, SelectOption, UploadedFile,
};

pub const APPLICATION_IR_VERSION: u32 = 1;

fn deserialize_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(deserializer)?;
    if version != APPLICATION_IR_VERSION {
        return Err(serde::de::Error::custom(format!(
            "不支持 Application IR 版本 {version}"
        )));
    }
    Ok(version)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationIR {
    #[serde(deserialize_with = "deserialize_version")]
    pub format_version: u32,
    pub name: String,
    pub resources: Vec<ResourceDefinition>,
    pub pages: Vec<PageDefinition>,
    pub actions: Vec<ActionDefinition>,
    pub navigation: Vec<NavigationDefinition>,
    pub permissions: Vec<PermissionDefinition>,
    pub module_references: Vec<ModuleReference>,
    #[serde(default)]
    pub component_overrides: BTreeMap<String, String>,
}

impl ApplicationIR {
    pub fn resource(&self, name: &str) -> Option<&ResourceDefinition> {
        self.resources.iter().find(|resource| resource.name == name)
    }
}
