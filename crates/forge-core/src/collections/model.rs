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
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub output_path: String,
    pub digest: String,
    pub mode: String,
    pub size: u64,
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
