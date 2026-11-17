//! The pins of slice 10, entries named .git below the
//! root, over synthetic fixture repos the suite builds
//! with git. The submodule fixture stands as a local
//! repository joined with the file protocol standing
//! allowed in the fixture alone.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{git, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::jsonvalue::JsonValue;
use finaltree::stamping::{compare_fingerprints, run_and_stamp, verify_stamped_log};
use finaltree::Fingerprint;

/// A repo holding one committed file of its own.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("dotgit", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("dotgit", area);
    let source = scratch.0.join("source");
    std::fs::create_dir_all(&source).expect("the source stands");
    git(&source, &["init", "-q", "-b", "main", "."]);
    write_file(&source, "inner.txt", "inner\n");
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
    (scratch, root)
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

fn set_file_mode(root: &Path, relative: &str, mode: u32) {
    let full = root.join(relative);
    let mut permissions = std::fs::metadata(&full)
        .expect("the file stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(&full, permissions).expect("the mode stands");
}

fn make_fifo(root: &Path, relative: &str) {
    let full = root.join(relative);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("the parent stands");
    }
    let status = std::process::Command::new("mkfifo")
        .arg(&full)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "the pipe stands");
}

/// The dot_git member of a record as (path, blob) text pairs.
fn dot_git_pairs(record: &JsonValue) -> Vec<(String, String)> {
    let Some(member) = record.get("dot_git") else {
        return Vec::new();
    };
    let JsonValue::List(items) = member else {
        panic!("the dot_git member is a list");
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

/// The file_modes member of a record as (path, mode) text pairs.
fn file_mode_pairs(record: &JsonValue) -> Vec<(String, String)> {
    let Some(JsonValue::List(items)) = record.get("file_modes") else {
        return Vec::new();
    };
    items
        .iter()
        .map(|entry| {
            let JsonValue::Text(path) = entry.get("path").expect("the path stands") else {
                panic!("the path is a text");
            };
            let JsonValue::Text(mode) = entry.get("mode").expect("the mode stands") else {
                panic!("the mode is a text");
            };
            (path.clone(), mode.clone())
        })
        .collect()
}

/// The special member of a record as (path, kind) text pairs.
fn special_pairs(record: &JsonValue) -> Vec<(String, String)> {
    let Some(member) = record.get("special") else {
        return Vec::new();
    };
    let JsonValue::List(items) = member else {
        panic!("the special member is a list");
    };
    items
        .iter()
        .map(|entry| {
            let JsonValue::Text(path) = entry.get("path").expect("the entry stands") else {
                panic!("the path is text");
            };
            let JsonValue::Text(kind) = entry.get("kind").expect("the entry stands") else {
                panic!("the kind is text");
            };
            (path.clone(), kind.clone())
        })
        .collect()
}

#[test]
fn test_the_dot_git_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("standings");
    let bare = fingerprint(&root).digest();
    write_file(&root, "sub/.git", "payload one\n");
    let standing = fingerprint(&root).digest();
    assert_ne!(bare, standing, "a file named .git below the root binds");
    write_file(&root, "sub/.git", "payload two\n");
    let changed = fingerprint(&root).digest();
    assert_ne!(standing, changed, "its content binds with it");
    std::fs::remove_file(root.join("sub/.git")).expect("the file clears");
    std::os::unix::fs::symlink("../own.txt", root.join("sub/.git")).expect("the link stands");
    let linked = fingerprint(&root).digest();
    assert_ne!(changed, linked, "the link standing stamps apart");
    assert_ne!(
        bare, linked,
        "the link standing binds against the bare tree"
    );
}

#[test]
fn test_a_tree_whose_only_dot_git_is_its_own_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("dot_git").is_none(),
        "no dot_git member stands where the only .git is the repository's own"
    );
    let read = Fingerprint::from_dict(&record).expect("the record reads back");
    assert_eq!(read.digest(), fingerprint(&root).digest());
}

#[test]
fn test_a_regular_file_named_dot_git_binds_its_content() {
    let (_scratch, root) = make_repo("content");
    let bare = fingerprint(&root).digest();
    write_file(&root, "sub/.git", "helper one\n");
    let standing = fingerprint(&root);
    assert_ne!(bare, standing.digest());
    assert_eq!(
        dot_git_pairs(&standing.to_dict()),
        vec![(
            "sub/.git".to_string(),
            "494843e8b2cc248097033b19caa105ab151b8c6e".to_string()
        )],
        "the entry records the path and the git blob of its content"
    );
}

#[test]
fn test_a_file_below_a_directory_named_dot_git_stands_named() {
    let (_scratch, root) = make_repo("nested");
    write_file(&root, "plain/.git/inner.txt", "hidden\n");
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dot_git_pairs(&record),
        vec![(
            "plain/.git/inner.txt".to_string(),
            "136c05e0d0290e2dc7eb89bceaa27b5f011e780a".to_string()
        )],
        "a file below a folder named .git stands named in the member"
    );
}

#[test]
fn test_a_link_named_dot_git_binds_its_target_text() {
    let (_scratch, root) = make_repo("linked");
    std::fs::create_dir_all(root.join("sym")).expect("the folder stands");
    std::os::unix::fs::symlink("../own.txt", root.join("sym/.git")).expect("the link stands");
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dot_git_pairs(&record),
        vec![(
            "sym/.git".to_string(),
            "fc270c405472e2ac0d0c19957b372335686d7bfb".to_string()
        )],
        "the link stands as the git blob of its target text"
    );
}

#[test]
fn test_a_dot_git_file_at_an_outside_family_mode_binds_its_permission() {
    let (_scratch, root) = make_repo("permission");
    write_file(&root, "sub/.git", "helper one\n");
    set_file_mode(&root, "sub/.git", 0o400);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        file_mode_pairs(&record),
        vec![("sub/.git".to_string(), "400".to_string())],
        "the permission standing binds through the file_modes member"
    );
    assert_eq!(
        dot_git_pairs(&record),
        vec![(
            "sub/.git".to_string(),
            "494843e8b2cc248097033b19caa105ab151b8c6e".to_string()
        )],
        "the content standing binds beside it"
    );
    set_file_mode(&root, "sub/.git", 0o644);
}

#[test]
fn test_a_special_file_named_dot_git_stands_in_the_special_member() {
    let (_scratch, root) = make_repo("special");
    let bare = fingerprint(&root).digest();
    make_fifo(&root, "sub/.git");
    let standing = fingerprint(&root);
    assert_ne!(bare, standing.digest(), "the pipe binds its tree");
    let record = standing.to_dict();
    assert_eq!(
        special_pairs(&record),
        vec![("sub/.git".to_string(), "fifo".to_string())],
        "the pipe stands named in the special member"
    );
    assert!(
        record.get("dot_git").is_none(),
        "a special file stands in no dot_git entry"
    );
}

#[test]
fn test_a_dot_git_difference_names_a_dot_git_difference() {
    let (_scratch, root) = make_repo("difference");
    let bare = fingerprint(&root);
    write_file(&root, "sub/.git", "helper one\n");
    let standing = fingerprint(&root);
    let added = compare_fingerprints(&bare, &standing).expect("the comparison runs");
    assert_eq!(added, vec!["dot git file added sub/.git".to_string()]);
    let removed = compare_fingerprints(&standing, &bare).expect("the comparison runs");
    assert_eq!(removed, vec!["dot git file removed sub/.git".to_string()]);
    write_file(&root, "sub/.git", "helper two\n");
    let changed = fingerprint(&root);
    let content = compare_fingerprints(&standing, &changed).expect("the comparison runs");
    assert_eq!(
        content,
        vec!["dot git content changed sub/.git".to_string()]
    );
}

#[test]
fn test_a_record_carrying_the_same_dot_git_path_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    write_file(&root, "sub/.git", "helper one\n");
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "dot_git" {
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
fn test_a_stamped_log_verifies_against_the_dot_git_files_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    write_file(&root, "sub/.git", "helper one\n");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    write_file(&root, "sub/.git", "helper two\n");
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the changed file no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["dot git content changed sub/.git".to_string()]
    );
}

#[test]
fn test_a_submodules_dot_git_file_stands_named_nowhere_in_its_parent() {
    let (_scratch, root) = make_submodule_parent("submodule");
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("submodules").is_some(),
        "the fixture stands as a submodule parent"
    );
    assert!(
        record.get("dot_git").is_none(),
        "the pointer file a submodule checkout carries stands named nowhere in its parent"
    );
    assert!(dot_git_pairs(&record).is_empty());
    let inner = fingerprint(&root.join("sub")).to_dict();
    assert!(
        inner.get("dot_git").is_none(),
        "the pointer file stands at the submodule's own root under the root exclusion"
    );
}
