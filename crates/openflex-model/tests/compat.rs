//! Compatibility tests for the published example models.
//!
//! Every `.ofm` shipped in `examples/` must keep loading after a schema change:
//! the format promises that old models stay valid (additive fields only, strict
//! validation opt-out never needed for shipped files).

use std::path::{Path, PathBuf};

use openflex_model::Model;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn ofm_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            ofm_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "ofm") {
            out.push(path);
        }
    }
}

#[test]
fn every_example_model_still_loads() {
    let examples = workspace_root().join("examples");
    let mut files = Vec::new();
    ofm_files(&examples, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no example models found in {examples:?}");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read example");
        if let Err(error) = Model::from_json(&text) {
            panic!("{} no longer loads: {error}", file.display());
        }
    }
}
