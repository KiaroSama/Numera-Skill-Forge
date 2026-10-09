use super::component_path;
use crate::{
    Error, Result,
    model::{Component, Kind},
    storage, validation,
};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn render(
    root: &Path,
    id: &str,
    name: &str,
    version: &str,
    target: &str,
    components: &[Component],
) -> Result<()> {
    for component in components {
        let Ok(destination) = component_path(target, component) else {
            continue;
        };
        if component.kind == Kind::Plugins {
            continue;
        }
        if component.kind == Kind::Skills {
            required_file(root, &format!("{destination}/SKILL.md"))?;
        } else if component.kind != Kind::Assets {
            required_file(root, &destination)?;
            if matches!(
                component.kind,
                Kind::Hooks | Kind::Mcps | Kind::Lsp | Kind::Settings
            ) {
                let config: Value = storage::read(&validation::contained(root, &destination)?)?;
                if !config.is_object() {
                    return Err(Error::new(
                        "native-format",
                        "Native configuration must be a JSON object",
                    ));
                }
                if component.kind == Kind::Hooks
                    && target != "antigravity"
                    && !config["hooks"].is_object()
                {
                    return Err(Error::new(
                        "native-format",
                        "Native hook file requires a hooks event-map wrapper",
                    ));
                }
            }
        }
    }
    let native_plugin = components
        .iter()
        .any(|c| c.kind == Kind::Plugins && component_path(target, c).is_ok());
    let marker = match target {
        "claude-code" => Some(".claude-plugin/plugin.json"),
        "codex" => Some(".codex-plugin/plugin.json"),
        "cursor" => Some(".cursor-plugin/plugin.json"),
        "antigravity" => Some("plugin.json"),
        "gemini" => Some("gemini-extension.json"),
        "pi" => Some("package.json"),
        "hermes-native" => Some("plugin.yaml"),
        "openclaw-native" => Some("openclaw.plugin.json"),
        "hermes" | "copilot" | "vscode" | "kiro" | "openhands" | "openclaw" => Some("plugin.json"),
        _ => None,
    };
    if matches!(target, "claude-code" | "codex" | "cursor") && root.join("plugin.json").exists() {
        return Err(Error::new(
            "manifest-conflict",
            "Root portable manifest could shadow native components; build a separate native artifact",
        ));
    }
    if native_plugin {
        validate_native(root, target, marker)?;
        return Ok(());
    }
    let mut value = json!({"name": id, "version": version, "description": name});
    match target {
        "claude-code" | "codex" | "cursor" | "gemini" => {}
        "antigravity" => {
            value = json!({"$schema": "https://antigravity.google/schemas/v1/plugin.json", "name": id, "description": name})
        }
        "pi" => value["pi"] = json!({"skills": ["./skills"]}),
        "hermes" | "copilot" | "vscode" | "kiro" | "openhands" | "openclaw" => {
            value["$schema"] = json!("https://agent-plugins.org/schemas/1.0.0/plugin.schema.json");
            if target == "kiro" {
                value["author"] = json!({"name": "Numera Skill Forge (package assembler)"});
                value["keywords"] = json!([id]);
            }
        }
        "hermes-native" | "amp-native" | "openclaw-native" => {
            return Err(Error::new(
                "native-source-required",
                "Native runtime packaging requires an existing accepted plugin; no runtime stub is generated",
            ));
        }
        "opencode" | "opencode-v1" | "opencode-v2" | "amp" | "windsurf" | "cline" | "kilo"
        | "continue" => {
            return write_new(
                root,
                "forge-contribution.json",
                &json!({
                    "schema_version": 1, "target": target, "delivery": "native-config", "id": id, "version": version,
                    "activation": "not-installed", "verification": "static-contract; real-host-not-tested",
                    "components": components.iter().filter_map(|c| component_path(target, c).ok().map(|path| json!({"kind": c.kind, "path": path}))).collect::<Vec<_>>()
                }),
            );
        }
        _ => return Err(Error::new("target", "Unknown target")),
    }
    if target == "gemini" && root.join("forge-mcp-contribution.json").exists() {
        let contribution: Value = storage::read(&root.join("forge-mcp-contribution.json"))?;
        let servers = contribution
            .get("mcpServers")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                Error::new(
                    "native-format",
                    "Gemini MCP contribution requires mcpServers object",
                )
            })?;
        if servers.values().any(|server| server.get("trust").is_some()) {
            return Err(Error::new(
                "native-format",
                "Gemini extension MCP does not support trust",
            ));
        }
        value["mcpServers"] = json!(servers);
    }
    if target == "pi" {
        for (kind, key) in [
            (Kind::Mods, "extensions"),
            (Kind::Prompts, "prompts"),
            (Kind::Themes, "themes"),
        ] {
            let paths = components
                .iter()
                .filter(|c| c.kind == kind)
                .filter_map(|c| component_path(target, c).ok())
                .map(|path| format!("./{path}"))
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                value["pi"][key] = json!(paths);
            }
        }
    }
    let marker = marker.ok_or_else(|| Error::new("native-format", "Missing manifest contract"))?;
    write_new(root, marker, &value)?;
    validate_native(root, target, Some(marker))
}

fn write_new(root: &Path, relative: &str, value: &Value) -> Result<()> {
    let path = validation::contained(root, relative)?;
    if path.exists() {
        // Native metadata, dependencies and custom declarations belong to the accepted source.
        let _: Value = storage::read(&path)?;
        return Ok(());
    }
    storage::write(&path, value)
}

fn required_file(root: &Path, relative: &str) -> Result<()> {
    let path = validation::contained(root, relative)?;
    if !path.is_file() {
        return Err(Error::new(
            "native-entrypoint",
            format!("Native package requires {relative}"),
        ));
    }
    let _ = storage::bytes(&path)?;
    Ok(())
}

fn native_paths(root: &Path, value: &Value) -> Result<()> {
    match value {
        Value::String(path) if path.starts_with("./") => {
            let contained = validation::contained(root, &path[2..])?;
            if !contained.exists() {
                return Err(Error::new(
                    "native-path",
                    "Declared native component path does not exist",
                ));
            }
        }
        Value::String(path) if path != "." => {
            return Err(Error::new(
                "native-path",
                "Native component paths must start with ./ and remain inside the package",
            ));
        }
        Value::Array(values) => {
            for value in values {
                native_paths(root, value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_native(root: &Path, target: &str, marker: Option<&str>) -> Result<()> {
    if matches!(
        target,
        "amp-native" | "opencode" | "opencode-v1" | "opencode-v2"
    ) {
        if target == "opencode-v2" {
            required_file(root, "package.json")?;
        }
        if !["index.ts", "index.js"]
            .iter()
            .any(|path| root.join(path).is_file())
        {
            if target == "opencode" || target == "opencode-v1" {
                let files = storage::files(root)?;
                if files
                    .iter()
                    .any(|(path, _)| path.ends_with(".ts") || path.ends_with(".js"))
                {
                    return Ok(());
                }
            }
            return Err(Error::new(
                "native-entrypoint",
                "Preserved directory plugin requires index.ts or index.js; no executable entrypoint is synthesized",
            ));
        }
        return Ok(());
    }
    let marker =
        marker.ok_or_else(|| Error::new("native-format", "Native manifest is unverified"))?;
    required_file(root, marker)?;
    if target == "hermes-native" {
        let yaml: Value = serde_yaml_ng::from_slice(&storage::bytes(&root.join(marker))?)
            .map_err(|_| Error::new("native-format", "Invalid Hermes plugin.yaml"))?;
        if yaml.get("name").and_then(Value::as_str).is_none() {
            return Err(Error::new(
                "native-format",
                "Hermes plugin.yaml requires name",
            ));
        }
        return required_file(root, "__init__.py");
    }
    let manifest: Value = storage::read(&root.join(marker))?;
    if !manifest.is_object() {
        return Err(Error::new(
            "native-format",
            "Native manifest must be an object",
        ));
    }
    let key = if target == "openclaw-native" {
        "id"
    } else {
        "name"
    };
    if manifest
        .get(key)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(Error::new(
            "native-format",
            format!("Native manifest requires {key}"),
        ));
    }
    for key in [
        "skills",
        "agents",
        "commands",
        "hooks",
        "mcpServers",
        "lspServers",
        "outputStyles",
        "workflows",
    ] {
        if let Some(value) = manifest.get(key) {
            native_paths(root, value)?;
        }
    }
    if matches!(
        target,
        "hermes" | "copilot" | "vscode" | "kiro" | "openhands" | "openclaw"
    ) {
        if manifest["$schema"] != "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json" {
            return Err(Error::new(
                "native-format",
                "This portable profile requires Agent Plugins 1.0.0",
            ));
        }
        if root.join("mcp.json").exists() {
            let mcp: Value = storage::read(&root.join("mcp.json"))?;
            let servers = mcp
                .get("mcpServers")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    Error::new("native-format", "Portable MCP requires mcpServers object")
                })?;
            if mcp["$schema"] != "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json"
                || servers.values().any(|server| {
                    !matches!(server["type"].as_str(), Some("stdio" | "streamable-http"))
                })
            {
                return Err(Error::new(
                    "native-format",
                    "Portable subset requires versioned stdio/streamable-http MCP; no SSE conversion",
                ));
            }
        }
    }
    if target == "openclaw-native" {
        if !manifest["configSchema"].is_object() {
            return Err(Error::new(
                "native-format",
                "OpenClaw requires configSchema object",
            ));
        }
        let package: Value = storage::read(&validation::contained(root, "package.json")?)?;
        let entries = package["openclaw"]["extensions"]
            .as_array()
            .filter(|values| !values.is_empty())
            .ok_or_else(|| {
                Error::new(
                    "native-entrypoint",
                    "OpenClaw package.json requires openclaw.extensions",
                )
            })?;
        for entry in entries {
            let entry = entry.as_str().ok_or_else(|| {
                Error::new("native-format", "OpenClaw extension entries must be paths")
            })?;
            required_file(root, entry.trim_start_matches("./"))?;
        }
    }
    Ok(())
}
