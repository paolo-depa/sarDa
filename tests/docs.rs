//! Keeps the README in step with the code it documents.

use sarda::catalog;
use sarda::error::Kind;

fn readme() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap()
}

#[test]
fn readme_catalog_table_is_current() {
    let text = readme();
    let start =
        text.find("<!-- catalog:start -->").expect("start marker") + "<!-- catalog:start -->".len();
    let end = text.find("<!-- catalog:end -->").expect("end marker");
    assert_eq!(
        text[start..end].trim(),
        catalog::markdown_table().trim(),
        "README catalog table is stale; regenerate it with `cargo run -q -- --print-catalog`"
    );
}

#[test]
fn readme_lists_every_exit_code() {
    let text = readme();
    for kind in [
        Kind::Other,
        Kind::Usage,
        Kind::NoRuntime,
        Kind::MissingImage,
        Kind::NoData,
        Kind::ViewerFailed,
        Kind::PortUnavailable,
    ] {
        let row = format!("| {} |", kind.exit_code());
        assert!(
            text.contains(&row),
            "README exit code table lacks {row} ({})",
            kind.name()
        );
    }
}

#[test]
fn readme_names_the_default_images() {
    let text = readme();
    assert!(text.contains(sarda::cli::DEFAULT_GRAFANA_IMAGE));
    assert!(text.contains(sarda::cli::DEFAULT_SADF_IMAGE));
}
