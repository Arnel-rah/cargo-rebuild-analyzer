use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrateBuild {
    pub name: String,
    pub version: String,
    pub fresh: bool,
    pub likely_cause: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReport {
    pub timestamp: u64,
    pub release: bool,
    pub features: Vec<String>,
    pub package: Option<String>,
    pub compiled: Vec<CrateBuild>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildHistory {
    pub builds: Vec<BuildReport>,
}
