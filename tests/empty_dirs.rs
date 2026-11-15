//! The pins of slice 6, empty directories, over synthetic
//! fixture repos the suite builds with git. The submodule
//! fixtures stand as local repositories joined with the
//! file protocol standing allowed in the fixture alone.

mod common;

use std::path::{Path, PathBuf};

use common::{git, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::jsonvalue::JsonValue;
use finaltree::stamping::{compare_fingerprints, run_and_stamp, verify_stamped_log};
use finaltree::Fingerprint;

/// A repo holding one committed file of its own.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("emptydirs", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

/// The empty_dirs member of a record as path texts.
fn empty_dir_paths(record: &JsonValue) -> Vec<String> {
    let Some(member) = record.get("empty_dirs") else {
        return Vec::new();
    };
    let JsonValue::List(items) = member else {
        panic!("the empty_dirs member is a list");
    };
    items
        .iter()
        .map(|entry| {
            let JsonValue::Text(path) = entry else {
                panic!("the entry is a path string");
            };
            path.clone()
        })
        .collect()
}

fn make_dir(root: &Path, relative: &str) {
    std::fs::create_dir_all(root.join(relative)).expect("the directory stands");
}

#[test]
fn test_the_empty_directory_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("standings");
    let bare = fingerprint(&root).digest();
    make_dir(&root, "hollow");
    let one = fingerprint(&root).digest();
    make_dir(&root, "hollow/inner");
    let chained = fingerprint(&root).digest();
    make_dir(&root, "elsewhere");
    let two = fingerprint(&root).digest();
    assert_ne!(bare, one, "one empty directory binds its tree");
    assert_ne!(one, chained, "a directory chained inside it binds further");
    assert_ne!(chained, two, "a second standing elsewhere binds further");
}

#[test]
fn test_a_tree_without_empty_directories_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    make_dir(&root, "holding");
    write_file(&root, "holding/kept.txt", "kept\n");
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("empty_dirs").is_none(),
        "no empty_dirs member stands where no directory stands empty"
    );
    let read = Fingerprint::from_dict(&record).expect("the record reads back");
    assert_eq!(read.digest(), fingerprint(&root).digest());
}

#[test]
fn test_an_empty_dirs_entry_records_the_chain_of_paths() {
    let (_scratch, root) = make_repo("entry");
    make_dir(&root, "hollow/inner");
    make_dir(&root, "lone");
    let paths = empty_dir_paths(&fingerprint(&root).to_dict());
    assert_eq!(
        paths,
        vec![
            "hollow".to_string(),
            "hollow/inner".to_string(),
            "lone".to_string()
        ],
        "every directory on a file free chain stands named, sorted by path"
    );
}

#[test]
fn test_a_directory_holding_a_symlink_is_not_empty() {
    let (_scratch, root) = make_repo("symlink");
    make_dir(&root, "holder");
    std::os::unix::fs::symlink("nowhere", root.join("holder/link")).expect("the link stands");
    make_dir(&root, "hollow");
    let paths = empty_dir_paths(&fingerprint(&root).to_dict());
    assert_eq!(
        paths,
        vec!["hollow".to_string()],
        "a directory holding a link holds an entry and stands named nowhere"
    );
}

#[test]
fn test_an_empty_directory_under_an_ignored_path_stands_named() {
    let (_scratch, root) = make_repo("ignoredpath");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "ignore rules"]);
    make_dir(&root, "build/inner");
    let paths = empty_dir_paths(&fingerprint(&root).to_dict());
    assert_eq!(
        paths,
        vec!["build".to_string(), "build/inner".to_string()],
        "the ignore rules reach no part of the empty_dirs member"
    );
}

#[test]
fn test_an_empty_directory_difference_names_an_empty_directory_difference() {
    let (_scratch, root) = make_repo("difference");
    let bare = fingerprint(&root);
    make_dir(&root, "hollow");
    let standing = fingerprint(&root);
    let added = compare_fingerprints(&bare, &standing).expect("the comparison runs");
    assert_eq!(added, vec!["empty directory added hollow".to_string()]);
    let removed = compare_fingerprints(&standing, &bare).expect("the comparison runs");
    assert_eq!(removed, vec!["empty directory removed hollow".to_string()]);
}

#[test]
fn test_a_record_carrying_the_same_empty_directory_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    make_dir(&root, "hollow");
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "empty_dirs" {
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
fn test_a_stamped_log_verifies_against_the_empty_directories_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    make_dir(&root, "hollow");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    make_dir(&root, "late");
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the new empty directory no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["empty directory added late".to_string()]
    );
}

#[test]
fn test_a_submodules_empty_directory_binds_through_its_parent() {
    let (scratch, root) = make_submodule_parent("nested");
    let _ = &scratch;
    let bare = fingerprint(&root).digest();
    make_dir(&root, "sub/hollow");
    let standing = fingerprint(&root);
    assert_ne!(
        bare,
        standing.digest(),
        "the submodule's empty directory binds the parent through its digest"
    );
    assert!(
        empty_dir_paths(&standing.to_dict()).is_empty(),
        "the parent's own member names nothing below the gitlink"
    );
}

#[test]
fn test_an_uninitialized_submodules_directory_stands_named_nowhere() {
    let (_scratch, root) = make_submodule_parent("uninitialized");
    git(&root, &["submodule", "deinit", "-q", "sub"]);
    assert!(
        root.join("sub").is_dir(),
        "the deinitialized directory stands in the tree"
    );
    let paths = empty_dir_paths(&fingerprint(&root).to_dict());
    assert!(
        !paths.iter().any(|path| path == "sub"),
        "a directory standing at a gitlink path stands named nowhere in the member"
    );
}

#[test]
fn test_a_directory_holding_only_an_ignored_file_is_not_empty() {
    let (_scratch, root) = make_repo("ignoredfile");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "ignore rules"]);
    write_file(&root, "build/out.bin", "artifact one\n");
    make_dir(&root, "hollow");
    let paths = empty_dir_paths(&fingerprint(&root).to_dict());
    assert_eq!(
        paths,
        vec!["hollow".to_string()],
        "a directory holding a file holds an entry, the file's ignored standing aside"
    );
}

/// A parent repo holding one submodule named sub, joined
/// from a local source repo with the file protocol.
fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("emptydirs", area);
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
