//! The setup obligation pins of the repo, carried over
//! from the retired Python suite under their own names.
//! The fixture repo is the synthetic repo of the Python
//! repo factory, built with git under a pinned identity
//! and fixed dates in a scratch directory carrying the
//! process id in its name and removed when the test ends.

use std::fs;
use std::path::PathBuf;

mod common;
use common::{make_factory_repo, Scratch};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|_| panic!("the repo file {relative} stands"))
}

#[test]
fn test_seed_files_present() {
    let readme = read_repo_file("README.md");
    let licence = read_repo_file("LICENSE");
    let gitignore = read_repo_file(".gitignore");
    assert!(readme.contains("Finaltree asks what it ran on"));
    assert!(readme.contains("## Licence"));
    assert!(licence.contains("Permission is hereby granted"));
    assert!(gitignore.contains("__pycache__/"));
}

#[test]
fn test_agents_md_stands_from_the_canonical_template() {
    let agents = read_repo_file("AGENTS.md");
    let marker = "<!-- repo annex below this line -->";
    let split = agents
        .find(marker)
        .expect("the annex marker stands in AGENTS.md");
    let canonical_part = &agents[..split];
    let annex = &agents[split + marker.len()..];
    assert!(canonical_part.contains("39 repo storefront perfection program"));
    assert!(annex.contains("This annex is finaltree's alone"));
    assert!(annex.contains("Finaltree asks what a gate ran on"));
    let sentences = [
        "This annex is finaltree's alone",
        "Finaltree asks what a gate ran on",
        "Finaltree fingerprints the exact tree a gate ran on",
        "Fingerprints are computed from a local working tree with git only",
        "Fixtures are synthetic repos built in the tests",
        "The local gate is cargo test",
        "The closed round count stands in the README and moves only at a closure",
    ];
    let mut positions = Vec::new();
    for sentence in sentences {
        positions.push(
            annex
                .find(sentence)
                .unwrap_or_else(|| panic!("the annex carries the sentence {sentence}")),
        );
    }
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(
        positions, sorted,
        "the annex sentences stand in their order"
    );
}

#[test]
fn test_readme_records_the_closed_round_count() {
    let readme = read_repo_file("README.md");
    assert!(readme.contains("Closed rounds stand at 4 of 100"));
}

#[test]
fn test_readme_states_the_limits_of_the_fingerprint() {
    let readme = read_repo_file("README.md");
    assert!(readme.contains("stand outside the fingerprint entirely"));
    assert!(readme.contains("belongs outside the tree it stamps"));
}

#[test]
fn test_fixtures_are_synthetic_repos() {
    let scratch = Scratch::new("setup", "fixtures");
    let root = scratch.0.join("repo");
    make_factory_repo(&root);
    assert!(
        root.join(".git").is_dir(),
        "the fixture stands as a git repo"
    );
    assert!(
        root.starts_with(&scratch.0),
        "the fixture stands inside the scratch directory"
    );
    assert_eq!(
        fs::read_to_string(root.join("a.txt")).expect("the fixture file stands"),
        "alpha\n"
    );
}
