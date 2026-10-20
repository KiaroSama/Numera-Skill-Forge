use super::model::{Accepted, Skill};
use crate::{Error, Result, install::Scope, storage, validation};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub fn validate(skill: &Skill) -> Result<()> {
    let Some(review) = skill.reviewed_distribution.as_ref() else {
        return Ok(());
    };
    let Some(native) = &review.native else {
        return Ok(());
    };
    let mut destinations = BTreeSet::new();
    for (source, destination) in &native.files {
        validation::relative_path(source)?;
        validation::relative_path(destination)?;
        if !review.files.contains_key(source)
            || super::policy::protected(destination)
            || !(destination.starts_with(".claude/hooks/") && destination.ends_with(".sh")
                || destination.starts_with(".claude/commands/") && destination.ends_with(".md")
                || destination.starts_with(".claude/skills/") && !destination.ends_with(".json"))
            || !destinations.insert(destination.to_ascii_lowercase())
        {
            return Err(Error::new(
                "native-review",
                "Invalid required native payload mapping",
            ));
        }
    }
    if native.files.is_empty()
        || native.hook_configs.is_empty()
        || native
            .hook_configs
            .iter()
            .any(|p| !review.files.contains_key(p) || !p.ends_with(".json"))
    {
        return Err(Error::new(
            "native-review",
            "Required native hook configuration is missing",
        ));
    }
    Ok(())
}

pub fn descriptor(accepted: &Accepted) -> Result<BTreeMap<String, Vec<u8>>> {
    if !required(accepted) {
        return Ok(BTreeMap::new());
    }
    let prefix = format!("packages/claude-code/{}", accepted.id);
    Ok(BTreeMap::from([(
        format!("{prefix}/plugin.json"),
        storage::encoded(&json!({
            "name":accepted.id,"version":accepted.version,"license":accepted.license,
            "required_delivery":"project-scoped Claude hooks and commands through explicit Forge approval",
            "activation":"not-performed"
        }))?,
    )]))
}

pub fn required(accepted: &Accepted) -> bool {
    accepted
        .reviewed_distribution
        .as_ref()
        .is_some_and(|r| r.native.is_some())
}

pub fn assess(accepted: &Accepted, target: &str, scope: Option<Scope>) -> Result<()> {
    if required(accepted) && (target != "claude-code" || scope.is_some_and(|s| s != Scope::Project))
    {
        return Err(Error::new(
            "unsupported-required",
            "This package requires project-scoped Claude hooks and commands; standalone skill-only conversion is refused",
        ));
    }
    Ok(())
}

pub fn installation(root: &Path, accepted: &Accepted) -> Result<(BTreeMap<String, String>, Value)> {
    let Some(review) = &accepted.reviewed_distribution else {
        return Ok((BTreeMap::new(), json!({})));
    };
    let Some(native) = &review.native else {
        return Ok((BTreeMap::new(), json!({})));
    };
    let mut files = BTreeMap::new();
    for (source, destination) in &native.files {
        let file = accepted
            .files
            .iter()
            .find(|f| &f.path == source)
            .ok_or_else(|| Error::new("native-review", "Required native original missing"))?;
        let prefix = format!("packages/claude-code/{}/", accepted.id);
        files.insert(
            file.output_path
                .strip_prefix(&prefix)
                .ok_or_else(|| Error::new("native-review", "Native source escapes package"))?
                .into(),
            destination.clone(),
        );
    }
    let mut hooks: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for source in &native.hook_configs {
        let file = accepted
            .files
            .iter()
            .find(|f| &f.path == source)
            .ok_or_else(|| Error::new("native-review", "Required hook configuration missing"))?;
        let value: Value = serde_json::from_slice(&storage::bytes(&validation::contained(
            root,
            &file.output_path,
        )?)?)?;
        let events = value["hooks"]
            .as_object()
            .ok_or_else(|| Error::new("native-review", "Original hooks must be an event map"))?;
        for (event, groups) in events {
            if !matches!(event.as_str(), "PreToolUse" | "PostToolUse") {
                return Err(Error::new(
                    "native-review",
                    "Unsupported required hook event",
                ));
            }
            let groups = groups
                .as_array()
                .ok_or_else(|| Error::new("native-review", "Hook groups must be arrays"))?;
            for group in groups {
                if group["matcher"] != "Bash" {
                    return Err(Error::new("native-review", "Required hook matcher differs"));
                }
                let handlers = group["hooks"]
                    .as_array()
                    .ok_or_else(|| Error::new("native-review", "Required hook handler missing"))?;
                for handler in handlers {
                    let command = handler["command"]
                        .as_str()
                        .ok_or_else(|| Error::new("native-review", "Required command missing"))?;
                    if handler["type"] != "command"
                        || !native
                            .files
                            .values()
                            .any(|p| command == format!("bash \"$CLAUDE_PROJECT_DIR\"/{p}"))
                    {
                        return Err(Error::new(
                            "native-review",
                            "Hook command does not reference a reviewed native script",
                        ));
                    }
                }
                hooks.entry(event.clone()).or_default().push(group.clone());
            }
        }
    }
    Ok((files, json!({"hooks": hooks})))
}
