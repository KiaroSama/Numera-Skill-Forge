use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OwnedField {
    pub desired: Value,
    pub baseline: Value,
    pub baseline_present: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OwnedConfig {
    #[serde(default)]
    pub fields: BTreeMap<String, OwnedField>,
    #[serde(default)]
    pub created: bool,
    #[serde(default)]
    pub empty_parents: Vec<String>,
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
fn keys(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|key| key.replace("~1", "/").replace("~0", "~"))
        .collect()
}
fn flatten(value: &Value, path: &str, out: &mut BTreeMap<String, Value>) {
    if let Value::Object(map) = value
        && !map.is_empty()
    {
        for (key, value) in map {
            flatten(value, &format!("{path}/{}", escape(key)), out);
        }
        return;
    }
    out.insert(path.into(), value.clone());
}
fn object(current: Option<&[u8]>) -> Result<Value> {
    let value = match current {
        Some(bytes) => serde_json::from_slice(bytes)?,
        None => Value::Object(Map::new()),
    };
    if !value.is_object() {
        return Err(Error::new(
            "config-format",
            "Native JSON configuration must be an object",
        ));
    }
    Ok(value)
}
fn set(
    value: &mut Value,
    parts: &[String],
    after: Option<&Value>,
    preserve: &[String],
    path: &str,
) -> Result<()> {
    let map = value.as_object_mut().ok_or_else(|| {
        Error::new(
            "config-conflict",
            "Managed configuration parent was replaced by a non-object",
        )
    })?;
    let Some((key, rest)) = parts.split_first() else {
        return Err(Error::new(
            "config-format",
            "Root configuration replacement is not supported",
        ));
    };
    if rest.is_empty() {
        if let Some(after) = after {
            map.insert(key.clone(), after.clone());
        } else {
            map.remove(key);
        }
    } else {
        if !map.contains_key(key) {
            if after.is_none() {
                return Ok(());
            }
            map.insert(key.clone(), Value::Object(Map::new()));
        }
        let child_path = format!("{path}/{}", escape(key));
        let child = map.get_mut(key).expect("inserted parent");
        set(child, rest, after, preserve, &child_path)?;
        if after.is_none()
            && child.as_object().is_some_and(Map::is_empty)
            && !preserve.contains(&child_path)
        {
            map.remove(key);
        }
    }
    Ok(())
}
fn overlaps(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}
fn check_owners(
    pointer: &str,
    desired: Option<&Value>,
    others: &[&OwnedConfig],
) -> Result<Option<OwnedField>> {
    let mut shared = None;
    for config in others {
        for (path, field) in &config.fields {
            if !overlaps(pointer, path) {
                continue;
            }
            if path != pointer || desired.is_some_and(|value| value != &field.desired) {
                return Err(Error::new(
                    "ownership-conflict",
                    format!(
                        "Other enabled package owns incompatible configuration field: {pointer}"
                    ),
                ));
            }
            shared = Some(field.clone());
        }
    }
    Ok(shared)
}
fn expect(value: &Value, pointer: &str, expected: Option<&Value>) -> Result<()> {
    if value.pointer(pointer) != expected {
        return Err(Error::new(
            "config-conflict",
            format!("Managed configuration field changed by user: {pointer}"),
        ));
    }
    Ok(())
}

pub(crate) fn combine_owned(union: &mut OwnedConfig, other: &OwnedConfig) -> Result<()> {
    for (pointer, field) in &other.fields {
        check_owners(pointer, Some(&field.desired), &[union])?;
        if let Some(previous) = union.fields.get(pointer)
            && (previous.baseline != field.baseline
                || previous.baseline_present != field.baseline_present)
        {
            return Err(Error::new(
                "ownership-conflict",
                "Shared configuration baselines disagree",
            ));
        }
        union.fields.insert(pointer.clone(), field.clone());
    }
    union.created |= other.created;
    for parent in &other.empty_parents {
        if !union.empty_parents.contains(parent) {
            union.empty_parents.push(parent.clone());
        }
    }
    Ok(())
}

pub(crate) fn combine_desired(first: &Value, second: &Value) -> Result<Value> {
    let mut union = select_fields(&OwnedConfig::default(), first)?;
    let other = select_fields(&OwnedConfig::default(), second)?;
    combine_owned(&mut union, &other)?;
    let mut document = Value::Object(Map::new());
    for (pointer, field) in union.fields {
        set(
            &mut document,
            &keys(&pointer),
            Some(&field.desired),
            &[],
            "",
        )?;
    }
    Ok(document)
}

pub(crate) fn select_fields(union: &OwnedConfig, desired: &Value) -> Result<OwnedConfig> {
    if !desired.is_object() || desired.as_object().is_some_and(Map::is_empty) {
        return Err(Error::new(
            "config-format",
            "Native contribution must declare managed object fields",
        ));
    }
    let mut values = BTreeMap::new();
    flatten(desired, "", &mut values);
    let mut selected = OwnedConfig {
        fields: BTreeMap::new(),
        ..union.clone()
    };
    for (pointer, desired) in values {
        selected.fields.insert(
            pointer.clone(),
            union.fields.get(&pointer).cloned().unwrap_or(OwnedField {
                desired,
                baseline: Value::Null,
                baseline_present: false,
            }),
        );
    }
    Ok(selected)
}

pub(crate) fn compatible(desired: &Value, others: &[&OwnedConfig]) -> Result<()> {
    let mut fields = BTreeMap::new();
    flatten(desired, "", &mut fields);
    for (pointer, value) in fields {
        check_owners(&pointer, Some(&value), others)?;
    }
    Ok(())
}

/// Merge only explicitly declared native fields; arrays and empty objects are atomic values.
pub(crate) fn merge(
    before: Option<&[u8]>,
    previous: Option<&OwnedConfig>,
    desired: &Value,
    others: &[&OwnedConfig],
) -> Result<(OwnedConfig, Value)> {
    if !desired.is_object() {
        return Err(Error::new(
            "config-format",
            "Native contribution must be a JSON object",
        ));
    }
    let mut document = object(before)?;
    let mut values = BTreeMap::new();
    if desired.as_object().is_some_and(Map::is_empty) {
        return Err(Error::new(
            "config-format",
            "Native contribution must declare at least one managed field",
        ));
    }
    flatten(desired, "", &mut values);
    let mut owned = OwnedConfig {
        created: previous.map_or_else(
            || {
                others
                    .first()
                    .map_or(before.is_none(), |other| other.created)
            },
            |p| p.created,
        ),
        empty_parents: previous
            .map(|p| p.empty_parents.clone())
            .unwrap_or_else(|| {
                others
                    .iter()
                    .flat_map(|p| p.empty_parents.clone())
                    .collect()
            }),
        ..OwnedConfig::default()
    };
    if let Some(previous) = previous {
        for (pointer, field) in &previous.fields {
            expect(&document, pointer, Some(&field.desired))?;
            if !values.contains_key(pointer) && check_owners(pointer, None, others)?.is_none() {
                set(
                    &mut document,
                    &keys(pointer),
                    field.baseline_present.then_some(&field.baseline),
                    &previous.empty_parents,
                    "",
                )?;
            }
        }
    }
    for (pointer, desired) in values {
        let shared = check_owners(&pointer, Some(&desired), others)?;
        let previous_field = previous.and_then(|config| config.fields.get(&pointer));
        if let Some(shared) = &shared {
            expect(&document, &pointer, Some(&shared.desired))?;
        }
        let field = previous_field
            .or(shared.as_ref())
            .cloned()
            .unwrap_or_else(|| OwnedField {
                desired: desired.clone(),
                baseline: document.pointer(&pointer).cloned().unwrap_or(Value::Null),
                baseline_present: document.pointer(&pointer).is_some(),
            });
        let parts = keys(&pointer);
        let mut parent = String::new();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            parent.push('/');
            parent.push_str(&escape(part));
            if document
                .pointer(&parent)
                .and_then(Value::as_object)
                .is_some_and(Map::is_empty)
                && !owned.empty_parents.contains(&parent)
            {
                owned.empty_parents.push(parent.clone());
            }
        }
        set(
            &mut document,
            &parts,
            Some(&desired),
            &owned.empty_parents,
            "",
        )?;
        owned
            .fields
            .insert(pointer, OwnedField { desired, ..field });
    }
    Ok((owned, document))
}

pub(crate) fn release(
    before: &[u8],
    owned: &OwnedConfig,
    others: &[&OwnedConfig],
) -> Result<Option<Value>> {
    let mut document = object(Some(before))?;
    for (pointer, field) in &owned.fields {
        expect(&document, pointer, Some(&field.desired))?;
        if check_owners(pointer, None, others)?.is_none() {
            set(
                &mut document,
                &keys(pointer),
                field.baseline_present.then_some(&field.baseline),
                &owned.empty_parents,
                "",
            )?;
        }
    }
    if owned.created && document.as_object().is_some_and(Map::is_empty) {
        Ok(None)
    } else {
        Ok(Some(document))
    }
}

pub(crate) fn activate(
    before: Option<&[u8]>,
    owned: &mut OwnedConfig,
    others: &[&OwnedConfig],
) -> Result<Value> {
    let mut document = object(before)?;
    for (pointer, field) in &mut owned.fields {
        let shared = check_owners(pointer, Some(&field.desired), others)?;
        let expected = shared
            .as_ref()
            .map(|f| &f.desired)
            .or_else(|| field.baseline_present.then_some(&field.baseline));
        expect(&document, pointer, expected)?;
        if let Some(shared) = shared {
            field.baseline = shared.baseline;
            field.baseline_present = shared.baseline_present;
        }
        set(
            &mut document,
            &keys(pointer),
            Some(&field.desired),
            &owned.empty_parents,
            "",
        )?;
    }
    if let Some(active) = others.first() {
        owned.created = active.created;
        for parent in &active.empty_parents {
            if !owned.empty_parents.contains(parent) {
                owned.empty_parents.push(parent.clone());
            }
        }
    }
    Ok(document)
}
