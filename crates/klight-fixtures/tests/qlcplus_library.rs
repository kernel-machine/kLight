//! Every QLC+ definition shipped in `fixtures/qlcplus` must load.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn qxf_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            qxf_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "qxf") {
            out.push(p);
        }
    }
}

#[test]
fn all_qlcplus_definitions_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/qlcplus");
    // The library is downloaded, not versioned: scripts/update-qlcplus-fixtures.sh
    if !dir.join("LICENSE").exists() {
        eprintln!("SKIPPED: fixtures/qlcplus not downloaded (run scripts/update-qlcplus-fixtures.sh)");
        return;
    }
    let mut files = Vec::new();
    qxf_files(&dir, &mut files);
    assert!(files.len() > 1000, "library missing: {} files", files.len());
    let mut failures = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        if let Err(e) = klight_fixtures::parse_qxf(&text) {
            failures.push(format!("{}: {e}", f.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
