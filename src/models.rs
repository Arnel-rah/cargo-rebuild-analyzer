#[derive(Debug, Clone)]
pub struct CrateBuild {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    pub compiled: Vec<CrateBuild>,
}
