mod models;

fn main() {
    let report = models::BuildReport {
        compiled: vec![models::CrateBuild {
            name: "tokio".to_string(),
            version: "1.48.0".to_string(),
        }],
    };

    println!("{report:#?}");
}
