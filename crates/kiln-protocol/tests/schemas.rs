//! The committed JSON Schemas match the types (ADR-0011: schemas are exported
//! and versioned). Re-bless with `KILN_BLESS=1 cargo test -p kiln-protocol --test schemas`.

use std::path::PathBuf;

#[test]
fn committed_schemas_match_the_types() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas");
    let bless = std::env::var_os("KILN_BLESS").is_some();
    for (name, schema) in kiln_protocol::schemas() {
        let path = dir.join(name);
        let text = serde_json::to_string_pretty(&schema).unwrap_or_default() + "\n";
        if bless {
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{e}"));
            continue;
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; bless with KILN_BLESS=1", path.display()));
        assert_eq!(
            committed, text,
            "{name} is out of date: re-bless with KILN_BLESS=1 if the change is intended"
        );
    }
}
