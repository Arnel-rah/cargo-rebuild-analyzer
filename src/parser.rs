use crate::models::CrateBuild;
use serde_json::Value;

pub fn parse_message(line: &str) -> Option<CrateBuild> {
    let message: Value = serde_json::from_str(line).ok()?;

    if message.get("reason")?.as_str()? != "compiler-artifact" {
        return None;
    }

    let package_id = message.get("package_id")?.as_str()?;
    let name = package_name(package_id)?;
    let target_kind = target_kind(&message)?;

    let version = package_id
        .rsplit_once('#')?
        .1
        .rsplit_once('@')?
        .1
        .to_string();

    let fresh = message
        .get("fresh")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let likely_cause = likely_cause(&target_kind, &name);

    Some(CrateBuild {
        name,
        version,
        target_kind,
        fresh,
        likely_cause,
    })
}

fn package_name(package_id: &str) -> Option<String> {
    package_id
        .rsplit_once('#')?
        .1
        .rsplit_once('@')?
        .0
        .to_string()
        .into()
}

fn target_kind(message: &Value) -> Option<String> {
    let kinds = message.get("target")?.get("kind")?.as_array()?;
    if kinds
        .iter()
        .any(|kind| kind.as_str() == Some("custom-build"))
    {
        return Some("build script".to_string());
    }

    kinds
        .first()
        .and_then(Value::as_str)
        .map(|kind| match kind {
            "lib" => "library",
            "bin" => "binary",
            "proc-macro" => "proc-macro",
            "test" => "test",
            "bench" => "benchmark",
            other => other,
        })
        .map(str::to_string)
}

fn likely_cause(target_kind: &str, name: &str) -> String {
    if target_kind == "build script" {
        return "build script".to_string();
    }

    if name.ends_with("-sys") || name.contains("openssl") || name == "ring" {
        return "native compilation".to_string();
    }

    "source or dependency change".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compiler_artifact() {
        let json = r#"{
            "reason": "compiler-artifact",
            "package_id": "registry+https://github.com/rust-lang/crates.io-index#tokio@1.48.0",
            "target": {"name": "tokio", "kind": ["lib"]},
            "fresh": false
        }"#;

        let result = parse_message(json).unwrap();

        assert_eq!(result.name, "tokio");
        assert_eq!(result.version, "1.48.0");
        assert_eq!(result.target_kind, "library");
        assert!(!result.fresh);
        assert_eq!(result.likely_cause, "source or dependency change");
    }

    #[test]
    fn ignores_other_cargo_messages() {
        assert!(parse_message(r#"{"reason":"build-finished","success":true}"#).is_none());
    }

    #[test]
    fn ignores_malformed_messages() {
        assert!(parse_message("not json").is_none());
    }

    #[test]
    fn identifies_build_scripts() {
        let message = r#"{
            "reason": "compiler-artifact",
            "package_id": "path+file:///tmp/project#demo@0.1.0",
            "target": {
                "name": "demo",
                "kind": ["custom-build"]
            },
            "fresh": true
        }"#;

        let result = parse_message(message).unwrap();

        assert!(result.fresh);
        assert_eq!(result.name, "demo");
        assert_eq!(result.target_kind, "build script");
        assert_eq!(result.likely_cause, "build script");
    }
}
