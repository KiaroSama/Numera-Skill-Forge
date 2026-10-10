use super::{
    Receipt, Scope, check_mode, current, destination, enabled_configs, file_mode, shared_file,
};
use crate::transaction::Change;
use crate::{Error, Result, adapters, build, config_merge, storage, transaction, validation};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub(super) struct Candidate {
    pub receipt: Receipt,
    pub files: BTreeMap<String, (Vec<u8>, u32)>,
    pub contributions: BTreeMap<String, (Value, Option<String>)>,
}

pub(super) fn candidate(
    root: &Path,
    id: &str,
    target: &str,
    selected: &Path,
    scope: Scope,
) -> Result<Candidate> {
    let package = build::package_root(root, id, target)?;
    if !package.is_dir() {
        return Err(Error::new(
            "build-required",
            "Build this target package first",
        ));
    }
    let destination = destination(selected)?;
    let report: Value = if package.join("compatibility.json").exists() {
        storage::read(&package.join("compatibility.json"))?
    } else {
        json!({})
    };
    if report.get("supported").is_some_and(|v| v != true) {
        return Err(Error::new(
            "unsupported-required",
            "Package compatibility report refuses this target",
        ));
    }
    let components = report["components"].as_array().cloned().unwrap_or_default();
    let contributions = contributions(&package, target, scope, &components)?;
    let native = components.iter().any(|c| {
        !matches!(c["kind"].as_str(), Some("skills" | "assets"))
            && !c["metadata"]["install_contribution"].is_object()
    });
    let prefix = if native {
        let prefix = native_prefix(target, scope, id)?;
        let entry_present = match target {
            "amp-native" => {
                package.join("index.ts").is_file() || package.join("index.js").is_file()
            }
            "antigravity" => package.join("plugin.json").is_file(),
            _ => false,
        };
        if !entry_present {
            return Err(Error::new(
                "native-format",
                "Native directory lacks its documented manifest or entry point",
            ));
        }
        if target == "antigravity" {
            let manifest: Value = storage::read(&package.join("plugin.json"))?;
            let name = manifest["name"].as_str().ok_or_else(|| {
                Error::new("native-format", "Antigravity manifest requires a name")
            })?;
            validation::identifier(name)?;
        }
        prefix
    } else if !package.join("skills").is_dir() && !contributions.is_empty() {
        format!(".forge-packages/{id}")
    } else {
        skill_prefix(target, scope)?.into()
    };
    let delivery = if native {
        "native-directory; host loading/enablement not verified"
    } else {
        "native skill/configuration installation; not runtime plugin enablement"
    };
    if !native && !package.join("skills").is_dir() && contributions.is_empty() {
        return Err(Error::new(
            "build-required",
            "Package has no installable skill or explicit native configuration contribution",
        ));
    }
    let receipt = Receipt {
        id: String::new(),
        package: id.into(),
        target: target.into(),
        destination,
        files: BTreeMap::new(),
        enabled: true,
        operation: transaction::operation_id(),
        scope,
        delivery: delivery.into(),
        modes: BTreeMap::new(),
        configs: BTreeMap::new(),
    };
    let source_modes = payload_modes(&package)?;
    let mut payloads = storage::files(&package)?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    for path in source_modes.keys() {
        payloads.insert(path.clone(), validation::contained(&package, path)?);
    }
    if payloads.len() > storage::MAX_FILES {
        return Err(Error::new(
            "size-limit",
            "Installation payload exceeds file count limit",
        ));
    }
    let unique_paths = payloads
        .keys()
        .map(|path| path.to_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    if unique_paths.len() != payloads.len() {
        return Err(Error::new(
            "case-collision",
            "Provenance payload paths collide on case-insensitive filesystems",
        ));
    }
    let mut total = 0u64;
    let mut files = BTreeMap::new();
    for (path, physical) in payloads {
        if contributions
            .values()
            .any(|(_, payload)| payload.as_deref() == Some(path.as_str()))
        {
            continue;
        }
        if !native
            && matches!(
                path.as_str(),
                "compatibility.json" | "provenance.json" | "plugin.json" | "package.json"
            )
        {
            continue;
        }
        if !native && (path.starts_with(".claude-plugin/") || path.starts_with(".cursor-plugin/")) {
            continue;
        }
        let relative = if native {
            format!("{prefix}/{path}")
        } else if let Some(path) = path.strip_prefix("skills/") {
            format!("{prefix}/{path}")
        } else {
            format!("{prefix}/.forge-packages/{id}/{path}")
        };
        if contributions.contains_key(&relative) {
            return Err(Error::new(
                "ownership-conflict",
                "Native JSON contribution overlaps a copied payload",
            ));
        }
        let mode = source_modes
            .get(&path)
            .copied()
            .unwrap_or(file_mode(&physical)?);
        let after = storage::bytes(&physical)?;
        total += after.len() as u64;
        if total > storage::MAX_TOTAL {
            return Err(Error::new(
                "size-limit",
                "Installation payload exceeds byte limit",
            ));
        }
        if files.insert(relative, (after, mode)).is_some() {
            return Err(Error::new(
                "ownership-conflict",
                "Source payloads map to the same installed path",
            ));
        }
    }
    Ok(Candidate {
        receipt,
        files,
        contributions,
    })
}

pub(super) fn reconcile(
    candidate: Candidate,
    previous: Option<&Receipt>,
    others: &[Receipt],
) -> Result<(Receipt, Vec<Change>)> {
    let mut receipt = candidate.receipt;
    let contributions = candidate.contributions;
    let mut changes = vec![];
    for (relative, (after, mode)) in candidate.files {
        let digest = storage::digest(&after);
        let before = current(&receipt.destination, &relative)?;
        let shared = shared_file(others, &relative, &digest, Some(&mode), false)?;
        if let Some(bytes) = &before {
            let current_digest = storage::digest(bytes);
            let prior = previous.and_then(|r| r.files.get(&relative));
            if prior != Some(&current_digest) && !(shared && current_digest == digest) {
                return Err(Error::new(
                    "ownership-conflict",
                    format!("Existing file is not unchanged Forge-owned material: {relative}"),
                ));
            }
            check_mode(
                &validation::contained(&receipt.destination, &relative)?,
                previous
                    .as_ref()
                    .and_then(|r| r.modes.get(&relative))
                    .or_else(|| shared.then_some(&mode)),
            )?;
        } else if previous
            .as_ref()
            .is_some_and(|r| r.files.contains_key(&relative))
            || others
                .iter()
                .any(|r| r.enabled && r.files.contains_key(&relative))
        {
            return Err(Error::new(
                "conflict",
                "Enabled package payload was removed by user",
            ));
        }
        let mode_changed = before.is_some()
            && file_mode(&validation::contained(&receipt.destination, &relative)?)? != mode;
        receipt.files.insert(relative.clone(), digest);
        receipt.modes.insert(relative.clone(), mode);
        if before.as_ref() != Some(&after) || (cfg!(unix) && mode_changed) {
            changes.push(Change {
                path: relative,
                before,
                after: Some(after),
            });
        }
    }
    if let Some(prior) = previous {
        for (path, digest) in &prior.files {
            if receipt.files.contains_key(path) {
                continue;
            }
            if contributions
                .keys()
                .any(|destination| destination.eq_ignore_ascii_case(path))
            {
                return Err(Error::new(
                    "ownership-conflict",
                    "Raw payload cannot become managed configuration in one update; remove its ownership explicitly first",
                ));
            }
            let before = current(&receipt.destination, path)?
                .ok_or_else(|| Error::new("conflict", "Owned file removed by user"))?;
            if storage::digest(&before) != *digest {
                return Err(Error::new(
                    "conflict",
                    "Removed package file was modified by user",
                ));
            }
            check_mode(
                &validation::contained(&receipt.destination, path)?,
                prior.modes.get(path),
            )?;
            if !shared_file(others, path, digest, prior.modes.get(path), true)? {
                changes.push(Change {
                    path: path.clone(),
                    before: Some(before),
                    after: None,
                });
            }
        }
    }
    for (path, (desired, _)) in contributions {
        if others.iter().any(|r| r.files.contains_key(&path)) || receipt.files.contains_key(&path) {
            return Err(Error::new(
                "ownership-conflict",
                "Configuration overlaps owned raw payload",
            ));
        }
        let before = current(&receipt.destination, &path)?;
        let all_configs = others
            .iter()
            .filter_map(|r| r.configs.get(&path))
            .collect::<Vec<_>>();
        config_merge::compatible(&desired, &all_configs)?;
        let (ownership, document) = config_merge::merge(
            before.as_deref(),
            previous.and_then(|r| r.configs.get(&path)),
            &desired,
            &enabled_configs(others, &path),
        )?;
        let after = Some(storage::encoded(&document)?);
        if before != after {
            changes.push(Change {
                path: path.clone(),
                before,
                after,
            });
        }
        receipt.configs.insert(path, ownership);
    }
    if let Some(prior) = previous {
        for (path, owned) in &prior.configs {
            if receipt.configs.contains_key(path) {
                continue;
            }
            let before = current(&receipt.destination, path)?.ok_or_else(|| {
                Error::new("config-conflict", "Managed configuration removed by user")
            })?;
            let after = config_merge::release(&before, owned, &enabled_configs(others, path))?
                .map(|v| storage::encoded(&v))
                .transpose()?;
            if Some(&before) != after.as_ref() {
                changes.push(Change {
                    path: path.clone(),
                    before: Some(before),
                    after,
                });
            }
        }
    }
    Ok((receipt, changes))
}

fn skill_prefix(target: &str, scope: Scope) -> Result<&'static str> {
    if scope == Scope::Project {
        return adapters::install_prefix(target);
    }
    match target {
        "hermes" => Ok(".hermes/skills"),
        "antigravity" => Ok(".gemini/config/skills"),
        "opencode" | "opencode-v1" | "opencode-v2" => Ok(".config/opencode/skills"),
        "copilot" => Ok(".copilot/skills"),
        "vscode" => Err(Error::new(
            "scope-contract",
            "VS Code user skill location is not verified; select project scope",
        )),
        "amp" => Ok(".config/agents/skills"),
        "pi" => Ok(".pi/agent/skills"),
        "windsurf" => Ok(".config/devin/skills"),
        _ => adapters::install_prefix(target),
    }
}
fn native_prefix(target: &str, scope: Scope, id: &str) -> Result<String> {
    let prefix = match (target, scope) {
        ("amp-native", Scope::Project) => ".amp/plugins",
        ("amp-native", Scope::User) => ".config/amp/plugins",
        ("antigravity", Scope::Project) => ".agents/plugins",
        ("antigravity", Scope::User) => ".gemini/config/plugins",
        _ => {
            return Err(Error::new(
                "native-install-required",
                "This profile requires documented host import/activation or lacks a verified directory installation contract; no host manager was invoked",
            ));
        }
    };
    Ok(format!("{prefix}/{id}"))
}
fn contributions(
    package: &Path,
    target: &str,
    scope: Scope,
    components: &[Value],
) -> Result<BTreeMap<String, (Value, Option<String>)>> {
    let mut result = BTreeMap::new();
    for component in components {
        let Some(spec) = component["metadata"]["install_contribution"].as_object() else {
            continue;
        };
        if !matches!(component["kind"].as_str(), Some("settings" | "mcps"))
            || component["metadata"]
                .get("native_target")
                .or_else(|| component["metadata"].get("native_harness"))
                .and_then(Value::as_str)
                != Some(target)
        {
            return Err(Error::new(
                "native-identity-required",
                "Configuration contributions require matching native identity and settings/MCP kind",
            ));
        }
        let key = if scope == Scope::Project {
            "project_path"
        } else {
            "user_path"
        };
        let path = spec.get(key).and_then(Value::as_str).ok_or_else(|| {
            Error::new(
                "scope-contract",
                "Contribution lacks explicit native configuration destination for this scope",
            )
        })?;
        validation::relative_path(path)?;
        if !path.ends_with(".json") {
            return Err(Error::new(
                "config-format",
                "Only explicit native JSON contributions support three-way merge",
            ));
        }
        let payload = spec
            .get("payload_path")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let value = match (spec.get("value"), &payload) {
            (Some(value), None) => value.clone(),
            (None, Some(payload)) => storage::read(&validation::contained(package, payload)?)?,
            _ => {
                return Err(Error::new(
                    "config-format",
                    "Declare exactly one native value or payload_path",
                ));
            }
        };
        if result
            .keys()
            .any(|existing: &String| existing.eq_ignore_ascii_case(path))
        {
            return Err(Error::new(
                "config-collision",
                "Native configuration destinations collide on case-insensitive filesystems",
            ));
        }
        if result.insert(path.into(), (value, payload)).is_some() {
            return Err(Error::new(
                "config-collision",
                "Multiple components declare one configuration file; combine explicitly before installation",
            ));
        }
    }
    Ok(result)
}
fn payload_modes(package: &Path) -> Result<BTreeMap<String, u32>> {
    if !package.join("provenance.json").exists() {
        return Ok(BTreeMap::new());
    }
    let provenance: Value = storage::read(&package.join("provenance.json"))?;
    let mut result = BTreeMap::new();
    if let Some(modes) = provenance["file_modes"].as_object() {
        for (path, mode) in modes {
            result.insert(path.clone(), normalize_mode(mode)?);
        }
    }
    if let Some(files) = provenance["files"].as_array() {
        for file in files {
            if let Some(path) = file["output_path"].as_str() {
                let id = package
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| Error::new("path", "Invalid package identity"))?;
                let prefix = format!("packages/claude-code/{id}/");
                let relative = path.strip_prefix(&prefix).ok_or_else(|| {
                    Error::new(
                        "integrity",
                        "Collection provenance path belongs to another package",
                    )
                })?;
                result.insert(relative.into(), normalize_mode(&file["mode"])?);
            }
        }
    }
    for path in result.keys() {
        validation::relative_path(path)?;
    }
    Ok(result)
}
fn normalize_mode(mode: &Value) -> Result<u32> {
    let mode = mode
        .as_u64()
        .and_then(|m| u32::try_from(m).ok())
        .or_else(|| {
            mode.as_str()
                .and_then(|m| u32::from_str_radix(m.trim_start_matches("0o"), 8).ok())
        })
        .ok_or_else(|| Error::new("mode", "Invalid provenance file mode"))?;
    match mode {
        0o644 | 100644 | 0o100644 => Ok(0o644),
        0o755 | 100755 | 0o100755 => Ok(0o755),
        _ if mode <= 0o777 => Ok(mode),
        _ => Err(Error::new("mode", "Unsupported provenance file mode")),
    }
}
