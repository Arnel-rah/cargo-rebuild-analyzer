use crate::models::CrateBuild;
use serde_json::Value;

pub fn parse_message(line: &str) -> Option<CrateBuild> {
    let message: Value = serde_json::from_str(line).ok()?;

    if message.get("reason")?.as_str()? != "compiler-artifact" {
        return None;
    }

    let name = message.get("target")?.get("name")?.as_str()?.to_string();

    let package_id = message.get("package_id")?.as_str()?;

    let version = package_id
        .rsplit_once('#')?
        .1
        .rsplit_once('@')?
        .1
        .to_string();

    Some(CrateBuild { name, version })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compiler_artifact() {
        let json = r#"{
            "reason": "compiler-artifact",
            "package_id": "registry+https://github.com/rust-lang/crates.io-index#tokio@1.48.0",
            "target": {
                "name": "tokio"
            }
        }"#;

        let result = parse_message(json).unwrap();

        assert_eq!(result.name, "tokio");
        assert_eq!(result.version, "1.48.0");
    }

    #[test]
    fn ignores_other_cargo_messages() {
        assert!(parse_message(r#"{"reason":"build-finished","success":true}"#).is_none());
    }

    #[test]
    fn ignores_malformed_messages() {
        assert!(parse_message("not json").is_none());
    }
}
