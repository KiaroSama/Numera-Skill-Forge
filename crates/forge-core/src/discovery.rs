use crate::{
    Error, Result,
    model::{Component, Kind, Payload, Snapshot},
    storage,
};
use std::{collections::BTreeMap, path::Path};

pub fn inspect(root: &Path, source: &str, revision: &str) -> Result<Snapshot> {
    let mut files = vec![];
    let mut components = vec![];
    let mut licenses = vec![];
    let entries = storage::payload_files(root)?;
    for (path, physical) in entries {
        let bytes = storage::bytes(&physical)?;
        if bytes.starts_with(b"version https://git-lfs.github.com/spec/") {
            return Err(Error::new(
                "lfs-unresolved",
                "Resolve and approve pinned LFS content before import",
            ));
        }
        if path.ends_with(".gitmodules") {
            return Err(Error::new(
                "submodule-review",
                "Submodules require a pinned explicit import plan",
            ));
        }
        let upper = path.to_ascii_uppercase();
        if upper.contains("LICENSE") || upper.contains("COPYING") || upper.contains("NOTICE") {
            licenses.extend_from_slice(&bytes);
        }
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(&physical)?.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = path.ends_with(".sh") || path.ends_with(".ps1") || path.ends_with(".exe");
        files.push(Payload {
            path: path.clone(),
            digest: storage::digest(&bytes),
            size: bytes.len() as u64,
            executable,
        });
        if let Some((kind, component_root, name)) = detect(&path) {
            let mut metadata = BTreeMap::new();
            if matches!(
                kind,
                Kind::Skills | Kind::Agents | Kind::Rules | Kind::Commands | Kind::OutputStyles
            ) {
                let text = std::str::from_utf8(&bytes)
                    .map_err(|_| Error::new("encoding", "Prompt payload is not UTF-8"))?;
                if let Some(rest) = text.strip_prefix("---\n")
                    && let Some((front, _)) = rest.split_once("\n---")
                {
                    let value: serde_json::Value = serde_yaml_ng::from_str(front)
                        .map_err(|_| Error::new("frontmatter", "Invalid YAML frontmatter"))?;
                    if let Some(map) = value.as_object() {
                        metadata.extend(map.iter().map(|(k, v)| (k.clone(), v.clone())));
                    }
                }
            }
            if kind == Kind::Plugins {
                let native = if path.contains(".claude-plugin/") {
                    Some("claude-code")
                } else if path.contains(".cursor-plugin/") {
                    Some("cursor")
                } else if path.contains(".codex-plugin/") {
                    Some("codex")
                } else if path.ends_with("plugin.yaml") {
                    Some("hermes-native")
                } else if path.ends_with("gemini-extension.json") {
                    Some("gemini")
                } else if path.ends_with("openclaw.plugin.json") {
                    Some("openclaw-native")
                } else {
                    None
                };
                if let Some(native) = native {
                    metadata.insert(
                        "native_target".into(),
                        serde_json::Value::String(native.into()),
                    );
                }
            }
            let name = metadata
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&name)
                .to_owned();
            let id = format!("{source}:{}", component_root);
            if !components.iter().any(|c: &Component| c.id == id) {
                components.push(Component {
                    id,
                    name,
                    kind,
                    root: component_root,
                    required: true,
                    metadata,
                });
            }
        }
    }
    let owning_roots = components
        .iter()
        .filter(|c| matches!(c.kind, Kind::Skills | Kind::Plugins))
        .map(|c| (c.id.clone(), c.root.clone()))
        .collect::<Vec<_>>();
    components.retain(|component| {
        !owning_roots.iter().any(|(id, root)| {
            id != &component.id
                && (root.is_empty() || component.root.starts_with(&format!("{root}/")))
        })
    });
    if licenses.is_empty() {
        return Err(Error::new(
            "license-review",
            "No license evidence found; redistribution requires owner review",
        ));
    }
    let content_digest = storage::digest(&storage::encoded(&files)?);
    Ok(Snapshot {
        source: source.into(),
        revision: revision.into(),
        content_digest,
        license_digest: storage::digest(&licenses),
        files,
        components,
    })
}
fn detect(path: &str) -> Option<(Kind, String, String)> {
    let parts = path.split('/').collect::<Vec<_>>();
    let name = parts.last()?.to_string();
    if name == "SKILL.md" {
        let root = parts[..parts.len() - 1].join("/");
        let n = parts
            .get(parts.len().saturating_sub(2))
            .copied()
            .unwrap_or("skill");
        return Some((Kind::Skills, root, n.into()));
    }
    if [
        "plugin.json",
        "plugin.yaml",
        "openclaw.plugin.json",
        "gemini-extension.json",
    ]
    .contains(&name.as_str())
    {
        let parent = parts[..parts.len() - 1].join("/");
        let root = if [".claude-plugin", ".cursor-plugin", ".codex-plugin"]
            .iter()
            .any(|prefix| parent.ends_with(prefix))
        {
            parent
                .rsplit_once('/')
                .map(|(p, _)| p.to_owned())
                .unwrap_or_default()
        } else {
            parent
        };
        return Some((Kind::Plugins, root, name));
    }
    let kind = category(path).map(|(kind, _)| kind).or_else(|| {
        Some(match name.as_str() {
            ".mcp.json" | "mcp.json" | "mcp_config.json" => Kind::Mcps,
            "settings.json" => Kind::Settings,
            ".lsp.json" => Kind::Lsp,
            "AGENTS.md" => Kind::Rules,
            _ => return None,
        })
    })?;
    Some((kind, path.into(), name.trim_end_matches(".md").into()))
}

pub(crate) fn category(path: &str) -> Option<(Kind, &str)> {
    for (end, _) in path.rmatch_indices('/') {
        let folder = path[..end].rsplit('/').next()?;
        let kind = match folder {
            "agents" => Kind::Agents,
            "commands" => Kind::Commands,
            "rules" | "steering" => Kind::Rules,
            "hooks" => Kind::Hooks,
            "mods" => Kind::Mods,
            "tools" => Kind::Tools,
            "workflows" => Kind::Workflows,
            "prompts" => Kind::Prompts,
            "output-styles" => Kind::OutputStyles,
            "themes" => Kind::Themes,
            "monitors" | "services" => Kind::Services,
            "providers" => Kind::Providers,
            "channels" => Kind::Channels,
            "keybindings" => Kind::Keybindings,
            "locales" => Kind::Localization,
            "evaluations" | "tests" => Kind::Evaluations,
            _ => continue,
        };
        return Some((kind, &path[end + 1..]));
    }
    None
}
