use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrateBuild {
    pub name: String,
    pub version: String,
    #[serde(default = "default_target_kind")]
    pub target_kind: String,
    pub fresh: bool,
    pub likely_cause: String,
}

fn default_target_kind() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReport {
    pub timestamp: u64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub estimated_wasted_ms: u64,
    pub release: bool,
    pub features: Vec<String>,
    pub package: Option<String>,
    pub compiled: Vec<CrateBuild>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildHistory {
    pub builds: Vec<BuildReport>,
}
