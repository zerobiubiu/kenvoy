use std::path::Path;

use serde_json::{Value, json};

use super::super::{
    config::Config,
    error::{ProjectError as Error, Result},
};

const FRONTEND_PACKAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/frontend/package.json"
));

pub(super) fn tsconfig(config: &Config) -> Value {
    let relative = |path: &Path| format!("./{}", path.to_string_lossy().replace('\\', "/"));
    let data = config.database.path.parent().unwrap_or(Path::new("data"));
    json!({
        "compilerOptions": {
            "target": "ES2022",
            "lib": ["ES2022", "DOM", "DOM.Iterable"],
            "module": "ESNext",
            "moduleResolution": "Bundler",
            "strict": true,
            "noEmit": true,
            "allowJs": true,
            "allowImportingTsExtensions": true,
            "jsx": "react-jsx",
            "jsxImportSource": "@kenvoy/client",
            "paths": {
                "@kenvoy/client": ["./.kenvoy/types/sdk.d.ts"],
                "@kenvoy/client/jsx-runtime": ["./.kenvoy/types/presentation/jsx-runtime.d.ts"],
                "@kenvoy/client/jsx-dev-runtime": ["./.kenvoy/types/presentation/jsx-dev-runtime.d.ts"]
            }
        },
        "include": ["./**/*.ts", "./**/*.tsx", "./**/*.js", "./**/*.jsx"],
        "exclude": ["**/node_modules", "./.kenvoy", "./migrations", relative(&config.build.out_dir), relative(data)]
    })
}

pub(super) fn package() -> Result<Value> {
    let frontend: Value = serde_json::from_str(FRONTEND_PACKAGE)?;
    let mut dependencies = serde_json::Map::new();
    for name in ["@types/react", "@types/react-dom", "typescript"] {
        let version = frontend["devDependencies"][name].as_str().ok_or_else(|| {
            Error::InvalidConfig(format!(
                "框架 frontend/package.json 缺少开发类型依赖版本：{name}"
            ))
        })?;
        dependencies.insert(name.into(), Value::String(version.into()));
    }
    Ok(json!({
        "private": true,
        "type": "module",
        "scripts": { "typecheck": "tsc --noEmit" },
        "devDependencies": dependencies
    }))
}

pub(super) fn encoded(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn install_command(package: &Value, created: bool) -> Result<String> {
    if created {
        return Ok("npm install --ignore-scripts".into());
    }
    let mut command = "npm install --save-dev --ignore-scripts".to_owned();
    for name in ["@types/react", "@types/react-dom", "typescript"] {
        let version = package["devDependencies"][name]
            .as_str()
            .ok_or_else(|| Error::InvalidConfig(format!("开发类型依赖版本缺失：{name}")))?;
        command.push(' ');
        command.push_str(name);
        command.push('@');
        command.push_str(version);
    }
    Ok(command)
}
