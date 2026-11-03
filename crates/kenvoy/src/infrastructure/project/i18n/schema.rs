use serde_json::{Map, Value, json};

use crate::domain::i18n::LanguagesConfig;

use super::super::error::{ProjectError as Error, Result};

pub const SCHEMA_REFERENCE: &str = "./language.schema.json";

/// Schema 只取决于已验证配置，校验时不读取或下载外部 Schema。
pub fn schema(config: &LanguagesConfig) -> Result<Value> {
    config
        .validate()
        .map_err(|error| Error::InvalidConfig(error.to_string()))?;
    let mut properties = Map::new();
    let mut requirements = Vec::with_capacity(config.definitions.len());
    for (code, definition) in &config.definitions {
        properties.insert(code.clone(), json!({"type": "string", "minLength": 1}));
        if let Some(alias) = &definition.alias {
            properties.insert(alias.clone(), json!({"type": "string", "minLength": 1}));
            requirements.push(json!({
                "oneOf": [{"required": [code]}, {"required": [alias]}]
            }));
        } else {
            requirements.push(json!({"required": [code]}));
        }
    }
    Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "minProperties": 1,
        "properties": {
            "$schema": {"type": "string", "const": SCHEMA_REFERENCE}
        },
        "required": ["$schema"],
        "additionalProperties": {
            "type": "object",
            "properties": properties,
            "required": [],
            "additionalProperties": false,
            "allOf": requirements
        }
    }))
}

pub(super) fn validator(schema: &Value) -> Result<jsonschema::Validator> {
    jsonschema::draft202012::options()
        .build(schema)
        .map_err(|error| Error::InvalidConfig(format!("生成 language.schema.json 失败：{error}")))
}

pub(super) fn encode(schema: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(schema).map_err(|error| {
        Error::InvalidConfig(format!("编码 language.schema.json 失败：{error}"))
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}
