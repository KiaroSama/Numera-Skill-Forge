use crate::{
    Error, Result,
    model::{Capability, Component, Kind},
    validation,
};
use serde_json::{Value, json};
use std::path::Path;

mod profiles;
mod rendering;

pub fn capabilities() -> Vec<Capability> {
    profiles::profiles()
        .into_iter()
        .map(profiles::capability)
        .collect()
}

pub fn assess(components: &[Component], target: &str) -> Result<Value> {
    let capability = capabilities()
        .into_iter()
        .find(|c| c.harness == target)
        .ok_or_else(|| Error::new("target", "Unknown target"))?;
    let unsupported = components
        .iter()
        .filter_map(|c| {
            component_path(target, c).err().map(|error| {
                json!({
                    "component": c.id, "required": c.required, "reason": error.message
                })
            })
        })
        .collect::<Vec<_>>();
    let supported = !unsupported.iter().any(|c| c["required"] == true);
    Ok(
        json!({"target": target, "capability": capability, "supported": supported, "unsupported": unsupported}),
    )
}

fn native_identity(component: &Component, target: &str) -> bool {
    let explicit = component
        .metadata
        .get("native_target")
        .or_else(|| component.metadata.get("native_harness"));
    if let Some(identity) = explicit {
        return identity.as_str() == Some(target);
    }
    // Generic hooks/, agents/ and root plugin.json are ambiguous across hosts.
    let prefix = match target {
        "claude-code" => ".claude/",
        "cursor" => ".cursor/",
        "codex" => ".codex/",
        "gemini" => ".gemini/",
        "opencode" | "opencode-v1" => ".opencode/",
        "amp-native" => ".amp/plugins/",
        "cline" => ".cline/",
        "kilo" => ".kilo/",
        "continue" => ".continue/",
        "pi" => ".pi/",
        _ => return false,
    };
    component
        .root
        .split('/')
        .collect::<Vec<_>>()
        .windows(prefix.trim_end_matches('/').split('/').count())
        .any(|parts| format!("{}/", parts.join("/")) == prefix)
}

/// Destination inside a staged package. An empty path means a native plugin's complete root.
/// Main copies accepted bytes here before render; this function never accesses the source or host.
pub fn component_path(target: &str, component: &Component) -> Result<String> {
    let profile = profiles::profiles()
        .into_iter()
        .find(|p| p.target == target)
        .ok_or_else(|| Error::new("target", "Unknown target"))?;
    if matches!(component.kind, Kind::Skills | Kind::Assets | Kind::Mcps) {
        validation::identifier(&component.name)?;
    }
    if !component.root.is_empty() {
        validation::relative_path(&component.root)?;
    }
    if !profile.kinds.contains(&component.kind) {
        return Err(Error::new(
            "unsupported-component",
            format!(
                "No verified {:?} packaging contract for {target}",
                component.kind
            ),
        ));
    }
    if component.kind == Kind::Skills {
        return Ok(format!("skills/{}", component.name));
    }
    if component.kind == Kind::Assets {
        return Ok(format!("assets/{}", component.name));
    }
    if !native_identity(component, target) {
        return Err(Error::new(
            "native-identity-required",
            format!(
                "{} requires native_target={target} or an unambiguous host-specific source path; cross-host conversion is refused",
                component.id
            ),
        ));
    }
    if component.kind == Kind::Plugins {
        return Ok(String::new());
    }
    let filename = component
        .root
        .rsplit('/')
        .next()
        .ok_or_else(|| Error::new("component-path", "Missing component filename"))?;
    let directory = match component.kind {
        Kind::Agents => "agents",
        Kind::Commands => "commands",
        Kind::Rules => {
            if target == "gemini" {
                "policies"
            } else {
                "rules"
            }
        }
        Kind::Prompts => "prompts",
        Kind::OutputStyles => "output-styles",
        Kind::Workflows => "workflows",
        Kind::Themes => "themes",
        Kind::Mods => "extensions",
        _ => "",
    };
    if !directory.is_empty() {
        let valid = match (&component.kind, target) {
            (Kind::Commands | Kind::Rules, "gemini") => filename.ends_with(".toml"),
            (Kind::Mods, "pi") | (Kind::Workflows, "claude-code") => {
                filename.ends_with(".js") || filename.ends_with(".ts")
            }
            (Kind::Themes, _) => filename.ends_with(".json"),
            _ => {
                filename.ends_with(".md")
                    || (target == "cursor"
                        && (filename.ends_with(".mdc")
                            || filename.ends_with(".markdown")
                            || (component.kind == Kind::Commands && filename.ends_with(".txt"))))
            }
        };
        if !valid {
            return Err(Error::new(
                "native-format",
                "Native component file extension does not match the documented target contract",
            ));
        }
        let relative = crate::discovery::category(&component.root)
            .filter(|(kind, _)| kind == &component.kind)
            .map(|(_, relative)| relative)
            .unwrap_or(filename);
        return Ok(format!("{directory}/{relative}"));
    }
    match component.kind {
        Kind::Hooks => Ok(if target == "antigravity" {
            "hooks.json"
        } else {
            "hooks/hooks.json"
        }
        .into()),
        Kind::Mcps => Ok(match target {
            "claude-code" | "codex" => ".mcp.json".into(),
            "antigravity" => "mcp_config.json".into(),
            "gemini" => "forge-mcp-contribution.json".into(),
            "continue" => format!(".continue/mcpServers/{}.json", component.name),
            "windsurf" | "cline" => "mcp-config-contribution.json".into(),
            _ => "mcp.json".into(),
        }),
        Kind::Lsp => Ok(".lsp.json".into()),
        Kind::Settings => Ok("settings.json".into()),
        _ => Err(Error::new(
            "unsupported-component",
            "No verified native destination",
        )),
    }
}

pub fn manifest(root: &Path, id: &str, name: &str, version: &str, target: &str) -> Result<()> {
    render(root, id, name, version, target, &[])
}

/// Render documented metadata around already-copied accepted payloads, without loading native code.
pub fn render(
    root: &Path,
    id: &str,
    name: &str,
    version: &str,
    target: &str,
    components: &[Component],
) -> Result<()> {
    validation::identifier(id)?;
    let report = assess(components, target)?;
    if report["supported"] != true {
        return Err(Error::new(
            "unsupported-required",
            serde_json::to_string(&report)?,
        ));
    }
    rendering::render(root, id, name, version, target, components)
}

pub fn install_prefix(target: &str) -> Result<&'static str> {
    match target {
        "claude-code" => Ok(".claude/skills"),
        "codex" => Ok(".agents/skills"),
        "cursor" => Ok(".cursor/skills"),
        "hermes" => Ok("skills"),
        "antigravity" => Ok(".agents/skills"),
        "opencode" | "opencode-v1" | "opencode-v2" => Ok(".opencode/skills"),
        "copilot" | "vscode" => Ok(".github/skills"),
        "gemini" => Ok(".gemini/skills"),
        "amp" => Ok(".agents/skills"),
        "pi" => Ok(".pi/skills"),
        "kiro" => Ok(".kiro/skills"),
        "windsurf" => Ok(".devin/skills"),
        "cline" => Ok(".cline/skills"),
        "kilo" => Ok(".kilo/skills"),
        "continue" => Ok(".continue/skills"),
        "hermes-native" | "amp-native" | "openhands" | "openclaw" | "openclaw-native" => {
            Err(Error::new(
                "native-install-required",
                "Use documented host plugin import; file installation is not certified",
            ))
        }
        _ => Err(Error::new("target", "Unknown target")),
    }
}
