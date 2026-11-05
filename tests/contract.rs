//! The contract audit pins of the closing slice. Every
//! test name the contract carries stands disposed in
//! tests/contract_name_map.txt as a Rust test in the tree,
//! a recorded case in the command line battery, or a
//! recorded case or section in the slice 1 recordings, and
//! each disposition stands as claimed. The contract, the
//! readme, and the agents file carry no forbidden dash.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use finaltree::jsonvalue::{parse_json_lenient, JsonValue};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|_| panic!("the repo file {relative} stands"))
}

fn contract_names() -> BTreeSet<String> {
    let contract = read_repo_file("CONTRACT.md");
    let mut names = BTreeSet::new();
    for token in
        contract.split(|ch: char| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'))
    {
        if token.len() > 5 && token.starts_with("test_") {
            names.insert(token.to_string());
        }
    }
    names
}

fn rust_test_stands(file: &str, name: &str) -> bool {
    let Ok(text) = fs::read_to_string(repo_root().join(file)) else {
        return false;
    };
    text.contains(&format!("fn {name}("))
}

fn recording_case_stands(file: &str, wanted: &str) -> bool {
    let Ok(text) = fs::read_to_string(repo_root().join(file)) else {
        return false;
    };
    let Ok(document) = parse_json_lenient(&text) else {
        return false;
    };
    let in_list = |items: &Vec<JsonValue>| {
        items
            .iter()
            .any(|item| matches!(item.get("name"), Some(JsonValue::Text(name)) if name == wanted))
    };
    match &document {
        JsonValue::Object(pairs) => pairs.iter().any(|(_, value)| match value {
            JsonValue::List(items) => in_list(items),
            JsonValue::Object(_) => match value.get("cases") {
                Some(JsonValue::List(items)) => in_list(items),
                _ => false,
            },
            _ => false,
        }),
        _ => false,
    }
}

fn recording_section_stands(file: &str, wanted: &str) -> bool {
    let Ok(text) = fs::read_to_string(repo_root().join(file)) else {
        return false;
    };
    let Ok(document) = parse_json_lenient(&text) else {
        return false;
    };
    if let Some(value) = document.get(wanted) {
        return match value {
            JsonValue::List(items) => !items.is_empty(),
            JsonValue::Object(_) => match value.get("cases") {
                Some(JsonValue::List(items)) => !items.is_empty(),
                _ => false,
            },
            _ => false,
        };
    }
    if let Some(JsonValue::List(cases)) = document.get("cases") {
        return cases.iter().any(
            |item| matches!(item.get("section"), Some(JsonValue::Text(name)) if name == wanted),
        );
    }
    false
}

#[test]
fn test_every_contract_name_is_disposed() {
    let names = contract_names();
    let map = read_repo_file("tests/contract_name_map.txt");
    let mut disposed: BTreeSet<String> = BTreeSet::new();
    for line in map.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ' ');
        let name = parts.next().expect("a map line names its test");
        let kind = parts.next().expect("a map line names its disposition");
        let rest = parts.next().expect("a map line names its target");
        assert!(
            names.contains(name),
            "the map disposes a name the contract does not carry, {name}"
        );
        let stands = match kind {
            "rust-test" => rust_test_stands(rest, name),
            "recording-case" => {
                let (file, case) = rest
                    .split_once(' ')
                    .expect("a case disposition names its file and case");
                recording_case_stands(file, case)
            }
            "recording-section" => {
                let (file, section) = rest
                    .split_once(' ')
                    .expect("a section disposition names its file and section");
                recording_section_stands(file, section)
            }
            other => panic!("the map carries an unknown disposition {other}"),
        };
        assert!(stands, "the disposition of {name} stands as claimed");
        disposed.insert(name.to_string());
    }
    for name in &names {
        assert!(
            disposed.contains(name),
            "the contract name {name} stands disposed"
        );
    }
}

#[test]
fn test_contract_has_no_forbidden_dashes() {
    for file in ["CONTRACT.md", "README.md", "AGENTS.md"] {
        let text = read_repo_file(file);
        assert!(!text.contains('\u{2014}'), "{file} carries no em dash");
        assert!(!text.contains('\u{2013}'), "{file} carries no en dash");
    }
}
