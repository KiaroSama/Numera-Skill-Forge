use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
pub struct Registry {
    pub schema_version: u32,
    pub collections: Vec<Collection>,
}
#[derive(Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub repository: String,
    pub branch: String,
    pub reviewed_revision: String,
    pub skills: Vec<Skill>,
}
#[derive(Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub root: String,
    pub license: String,
    pub license_path: String,
    pub license_digest: String,
    #[serde(default)]
    pub notices: Vec<Notice>,
    #[serde(default)]
    pub attribution: String,
    #[serde(default)]
    pub reviewed_templates: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reviewed_documents: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_name: Option<ReviewedName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_design: Option<ReviewedDesign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_guide: Option<ReviewedGuide>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedDesign {
    pub name: String,
    pub source_digest: String,
    pub adapter_version: u32,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedGuide {
    pub name: String,
    pub path: String,
    pub source_digest: String,
    pub adapter_version: u32,
}
#[derive(Serialize, Deserialize)]
pub struct ReviewedName {
    pub source_name: String,
    pub name: String,
    pub source_digest: String,
}
#[derive(Serialize, Deserialize)]
pub struct Notice {
    pub path: String,
    pub digest: String,
    pub source: String,
    pub license: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub output_path: String,
    pub digest: String,
    pub mode: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_digest: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Accepted {
    pub collection: String,
    pub id: String,
    pub root: String,
    pub revision: String,
    pub version: String,
    pub license: String,
    pub content_digest: String,
    pub files: Vec<File>,
    pub generated: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub delivery: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_design: Option<ReviewedDesign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_guide: Option<ReviewedGuide>,
}
#[derive(Serialize, Deserialize)]
pub struct Lock {
    pub schema_version: u32,
    pub entries: BTreeMap<String, Accepted>,
}
impl Default for Lock {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: BTreeMap::new(),
        }
    }
}
