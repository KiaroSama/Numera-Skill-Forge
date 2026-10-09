use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Skills,
    Agents,
    Commands,
    Settings,
    Hooks,
    Mcps,
    Mods,
    Plugins,
    Rules,
    Prompts,
    Workflows,
    Tools,
    Lsp,
    OutputStyles,
    Ui,
    Themes,
    Keybindings,
    Localization,
    Services,
    Providers,
    Channels,
    Onboarding,
    Assets,
    Evaluations,
    Vendor,
}
impl Kind {
    pub fn executable(&self) -> bool {
        matches!(
            self,
            Self::Hooks
                | Self::Mods
                | Self::Tools
                | Self::Ui
                | Self::Services
                | Self::Providers
                | Self::Channels
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "strategy", content = "value")]
#[derive(Default)]
pub enum RefPolicy {
    #[default]
    DefaultBranch,
    Branch(String),
    Commit(String),
    Tag(String),
    LatestStableRelease,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum UpdatePolicy {
    #[default]
    Review,
    ApprovedContentOnly,
    Pinned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub repository: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub reference: RefPolicy,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub excludes: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub update: UpdatePolicy,
    #[serde(default)]
    pub credential_reference: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub path: String,
    pub digest: String,
    pub size: u64,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub root: String,
    #[serde(default = "required")]
    pub required: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}
fn required() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub revision: String,
    pub content_digest: String,
    pub license_digest: String,
    pub files: Vec<Payload>,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub source: String,
    pub components: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub optional_exclusions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub schema_version: u32,
    pub sources: Vec<Source>,
    pub packages: Vec<Package>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            sources: vec![],
            packages: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accepted {
    pub snapshot: Snapshot,
    pub approved_digest: String,
    pub executable_approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lock {
    pub schema_version: u32,
    pub sources: BTreeMap<String, Accepted>,
    pub builds: BTreeMap<String, BuildIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildIdentity {
    pub input_digest: String,
    pub version: String,
    pub payload_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub harness: String,
    pub surface: String,
    pub profile: String,
    pub delivery: String,
    pub kinds: Vec<Kind>,
    pub documentation: String,
    pub verification: String,
    pub restrictions: Vec<String>,
}
