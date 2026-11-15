//! The pins of slice 5, ignored files, over synthetic
//! fixture repos the suite builds with git. The submodule
//! fixture stands as a local repository joined with the
//! file protocol standing allowed in the fixture alone.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{git, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::jsonvalue::JsonValue;
use finaltree::stamping::{compare_fingerprints, run_and_stamp, verify_stamped_log};
use finaltree::Fingerprint;

/// A repo holding one committed file of its own and an
/// ignore file setting the build directory aside.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("ignored", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

/// The ignored member of a record as (path, blob) text pairs.
fn ignored_pairs(record: &JsonValue) -> Vec<(String, String)> {
    let Some(member) = record.get("ignored") else {
        return Vec::new();
    };
    let JsonValue::List(items) = member else {
        panic!("the ignored member is a list");
    };
    items
        .iter()
        .map(|entry| {
            let JsonValue::Text(path) = entry.get("path").expect("the entry stands") else {
                panic!("the path is text");
            };
            let JsonValue::Text(blob) = entry.get("blob").expect("the entry stands") else {
                panic!("the blob is text");
            };
            (path.clone(), blob.clone())
        })
        .collect()
}

#[test]
fn test_the_ignored_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("standings");
    let bare = fingerprint(&root).digest();
    write_file(&root, "build/out.bin", "artifact one\n");
    let standing = fingerprint(&root).digest();
    write_file(&root, "build/out.bin", "artifact two\n");
    let changed = fingerprint(&root).digest();
    assert_ne!(bare, standing, "an ignored artifact binds its tree");
    assert_ne!(standing, changed, "its content binds with it");
}

#[test]
fn test_an_excluded_file_binds_its_fingerprint() {
    let (_scratch, root) = make_repo("excluded");
    let bare = fingerprint(&root).digest();
    write_file(&root, ".git/info/exclude", "local.txt\n");
    write_file(&root, "local.txt", "local only\n");
    let standing = fingerprint(&root);
    assert_ne!(bare, standing.digest());
    assert_eq!(
        ignored_pairs(&standing.to_dict())
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec!["local.txt".to_string()]
    );
}

#[test]
fn test_a_tree_without_ignored_files_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("ignored").is_none(),
        "no ignored member stands where no file is ignored"
    );
    let read = Fingerprint::from_dict(&record).expect("the record reads back");
    assert_eq!(read.digest(), fingerprint(&root).digest());
}

#[test]
fn test_an_ignored_entry_records_its_path_and_blob() {
    let (_scratch, root) = make_repo("entry");
    write_file(&root, "build/out.bin", "artifact one\n");
    let pairs = ignored_pairs(&fingerprint(&root).to_dict());
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, "build/out.bin");
    assert_eq!(pairs[0].1, "6517e9de361fe21e856f4d174c38f4e7a90f6028");
}

#[test]
fn test_an_ignored_difference_names_an_ignored_difference() {
    let (_scratch, root) = make_repo("difference");
    let bare = fingerprint(&root);
    write_file(&root, "build/out.bin", "artifact one\n");
    let standing = fingerprint(&root);
    let added = compare_fingerprints(&bare, &standing).expect("the comparison runs");
    assert_eq!(added, vec!["ignored file added build/out.bin".to_string()]);
    let removed = compare_fingerprints(&standing, &bare).expect("the comparison runs");
    assert_eq!(
        removed,
        vec!["ignored file removed build/out.bin".to_string()]
    );
    write_file(&root, "build/out.bin", "artifact two\n");
    let changed = fingerprint(&root);
    let content = compare_fingerprints(&standing, &changed).expect("the comparison runs");
    assert_eq!(
        content,
        vec!["ignored content changed build/out.bin".to_string()]
    );
}

#[test]
fn test_an_ignored_executable_stands_in_the_modes_member() {
    let (_scratch, root) = make_repo("modes");
    write_file(&root, "build/run.sh", "echo run\n");
    let full = root.join("build/run.sh");
    let mut permissions = std::fs::metadata(&full)
        .expect("the file stands")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&full, permissions).expect("the mode stands");
    let record = fingerprint(&root).to_dict();
    let JsonValue::List(modes) = record.get("modes").expect("the modes member stands") else {
        panic!("the modes member is a list");
    };
    assert_eq!(modes.len(), 1);
    assert_eq!(
        modes[0].get("path"),
        Some(&JsonValue::Text("build/run.sh".to_string()))
    );
    assert_eq!(
        modes[0].get("mode"),
        Some(&JsonValue::Text("100755".to_string()))
    );
}

#[test]
fn test_a_record_carrying_the_same_ignored_path_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    write_file(&root, "build/out.bin", "artifact one\n");
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "ignored" {
            let JsonValue::List(items) = value else {
                panic!("the member is a list");
            };
            let first = items[0].clone();
            items.push(first);
        }
    }
    let doubled = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&doubled).expect_err("the doubled member refuses");
    assert_eq!(
        error.to_string(),
        "the fingerprint record carries the same member twice"
    );
}

#[test]
fn test_a_stamped_log_verifies_against_the_ignored_files_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    write_file(&root, "build/out.bin", "artifact one\n");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    write_file(&root, "build/out.bin", "artifact two\n");
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the changed artifact no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["ignored content changed build/out.bin".to_string()]
    );
}

#[test]
fn test_a_submodules_ignored_file_binds_through_its_parent() {
    let scratch = Scratch::new("ignored", "nested");
    let source = scratch.0.join("source");
    std::fs::create_dir_all(&source).expect("the source stands");
    git(&source, &["init", "-q", "-b", "main", "."]);
    write_file(&source, "inner.txt", "inner\n");
    write_file(&source, ".gitignore", "build/\n");
    git(&source, &["add", "."]);
    git(&source, &["commit", "-q", "-m", "source"]);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    let source_text = source.to_string_lossy().into_owned();
    git(
        &root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &source_text,
            "sub",
        ],
    );
    git(&root, &["commit", "-q", "-m", "parent"]);
    let bare = fingerprint(&root).digest();
    write_file(&root, "sub/build/out.bin", "artifact one\n");
    let standing = fingerprint(&root).digest();
    assert_ne!(
        bare, standing,
        "the submodule's ignored file binds the parent through its digest"
    );
}
