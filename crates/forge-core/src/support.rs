use crate::{Error, Result, logging::redact, registry, storage, validation};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn retention(root: &Path, days: u64, keep: usize, apply: bool) -> Result<Value> {
    if !(1..=3650).contains(&days) || !(1..=10000).contains(&keep) {
        return Err(Error::new(
            "retention",
            "Retention requires days1-3650 and keep1-10000",
        ));
    }
    let directory = root.join(".forge-state/logs");
    if !directory.exists() {
        return Ok(json!({"removed":0,"candidates":[],"apply":apply}));
    }
    validation::reject_link(&directory)?;
    let mut logs = vec![];
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        validation::reject_link(&path)?;
        if path.extension().and_then(|s| s.to_str()) == Some("log") && path.is_file() {
            logs.push((fs::metadata(&path)?.modified()?, path));
        }
    }
    logs.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let now = std::time::SystemTime::now();
    let mut candidates = vec![];
    for (index, (modified, path)) in logs.iter().enumerate() {
        if index >= keep
            && now.duration_since(*modified).unwrap_or_default().as_secs() > days * 86400
        {
            candidates.push(
                path.file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::new("encoding", "Log name is not Unicode"))?
                    .to_owned(),
            );
            if apply {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(
        json!({"removed":if apply{candidates.len()}else{0},"candidates":candidates,"apply":apply,"preserved_latest":keep}),
    )
}
pub fn bundle(root: &Path, include_logs: bool) -> Result<Value> {
    for ancestor in root.ancestors() {
        validation::reject_link(ancestor)?;
    }
    let state = validation::contained(root, ".forge-state")?;
    if state.exists() {
        validation::reject_link(&state)?;
    }
    let r = registry::load(root)?;
    let mut logs = vec![];
    if include_logs {
        let directory = root.join(".forge-state/logs");
        if directory.exists() {
            validation::reject_link(&directory)?;
            let mut paths = fs::read_dir(&directory)?
                .map(|e| e.map(|v| v.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            paths.reverse();
            let mut total = 0;
            for path in paths
                .iter()
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("log"))
                .take(3)
            {
                validation::reject_link(path)?;
                let bytes = storage::bytes(path)?;
                total += bytes.len();
                if total > 1024 * 1024 {
                    return Err(Error::new("size-limit", "Selected support logs exceed1MiB"));
                }
                let text = std::str::from_utf8(&bytes)
                    .map_err(|_| Error::new("encoding", "Diagnostic log is not UTF-8"))?;
                logs.push(json!({"name":path.file_name().and_then(|s|s.to_str()),"lines":text.lines().map(redact).collect::<Vec<_>>()}));
            }
        }
    }
    let directory = root.join(".forge-state/support");
    fs::create_dir_all(&directory)?;
    validation::reject_link(&directory)?;
    let path = directory.join(format!(
        "support-{}.json",
        crate::transaction::operation_id()
    ));
    storage::write(
        &path,
        &json!({"schema_version":1,"forge_version":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS,"source_count":r.sources.len(),"package_count":r.packages.len(),"logs":logs,"privacy":"local-only; inspect before sharing; no source payload or credentials included"}),
    )?;
    Ok(json!({"path":path,"logs_included":include_logs,"uploaded":false}))
}
