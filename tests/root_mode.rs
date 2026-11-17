//! The pins of slice 11, the mode of the root itself,
//! over synthetic fixture repos the suite builds with
//! git. The submodule fixture stands as a local
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
    let scratch = Scratch::new("rootmode", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("rootmode", area);
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

fn set_root_mode(root: &Path, mode: u32) {
    let mut permissions = std::fs::metadata(root)
        .expect("the root stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(root, permissions).expect("the mode stands");
}

fn set_dir_mode(root: &Path, relative: &str, mode: u32) {
    let full = root.join(relative);
    let mut permissions = std::fs::metadata(&full)
        .expect("the directory stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(&full, permissions).expect("the mode stands");
}

/// The dir_modes member of a record as (path, mode) text pairs.
fn dir_mode_pairs(record: &JsonValue) -> Vec<(String, String)> {
    let Some(JsonValue::List(items)) = record.get("dir_modes") else {
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

#[test]
fn test_the_root_mode_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("apart");
    set_root_mode(&root, 0o755);
    let plain = fingerprint(&root).digest();
    set_root_mode(&root, 0o500);
    let closed = fingerprint(&root).digest();
    assert_ne!(plain, closed, "a root at 500 stamps apart");
    set_root_mode(&root, 0o777);
    let open = fingerprint(&root).digest();
    assert_ne!(plain, open, "a root at 777 stamps apart");
    assert_ne!(
        closed, open,
        "the two standings stamp apart from each other"
    );
    set_root_mode(&root, 0o1777);
    let sticky = fingerprint(&root).digest();
    assert_ne!(plain, sticky, "a sticky root stamps apart");
    assert_ne!(closed, sticky, "the sticky standing stamps apart from 500");
    assert_ne!(open, sticky, "the sticky standing stamps apart from 777");
    set_root_mode(&root, 0o755);
}

#[test]
fn test_a_tree_whose_root_stands_in_the_plain_family_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    let mut digests = Vec::new();
    for mode in [0o755, 0o775, 0o750, 0o700, 0o770] {
        set_root_mode(&root, mode);
        let record = fingerprint(&root);
        assert!(
            record.to_dict().get("dir_modes").is_none(),
            "a plain root standing at {mode:o} carries no member"
        );
        digests.push(record.digest());
    }
    assert!(
        digests.windows(2).all(|pair| pair[0] == pair[1]),
        "every plain root standing stamps as one"
    );
    let read =
        Fingerprint::from_dict(&fingerprint(&root).to_dict()).expect("the record reads back");
    assert_eq!(read.digest(), digests[0]);
    set_root_mode(&root, 0o755);
}

#[test]
fn test_a_root_mode_entry_records_its_spelling_and_mode() {
    let (_scratch, root) = make_repo("entry");
    set_root_mode(&root, 0o500);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dir_mode_pairs(&record),
        vec![(".".to_string(), "500".to_string())],
        "the root entry records its spelling and its mode"
    );
    set_root_mode(&root, 0o1777);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dir_mode_pairs(&record),
        vec![(".".to_string(), "1777".to_string())],
        "a sticky root spells its mode at four digits"
    );
    set_root_mode(&root, 0o755);
}

#[test]
fn test_a_root_holding_only_tracked_files_binds_its_mode() {
    let (_scratch, root) = make_repo("tracked");
    write_file(&root, "sub/tracked.txt", "tracked\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    set_root_mode(&root, 0o755);
    let plain = fingerprint(&root).digest();
    set_root_mode(&root, 0o500);
    let record = fingerprint(&root);
    assert_ne!(plain, record.digest());
    assert_eq!(
        dir_mode_pairs(&record.to_dict()),
        vec![(".".to_string(), "500".to_string())],
        "the root entry stands as the only entry of its member"
    );
    set_root_mode(&root, 0o755);
}

#[test]
fn test_a_root_mode_difference_names_a_directory_mode_difference() {
    let (_scratch, root) = make_repo("difference");
    set_root_mode(&root, 0o500);
    let closed = fingerprint(&root);
    set_root_mode(&root, 0o755);
    let plain = fingerprint(&root);
    let differences = compare_fingerprints(&closed, &plain).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["directory mode changed .".to_string()],
        "a plain side against a closed side names the one difference"
    );
    let reversed = compare_fingerprints(&plain, &closed).expect("the comparison runs");
    assert_eq!(
        reversed,
        vec!["directory mode changed .".to_string()],
        "the same pair reversed names it again"
    );
    set_root_mode(&root, 0o500);
    let closed_again = fingerprint(&root);
    let none = compare_fingerprints(&closed, &closed_again).expect("the comparison runs");
    assert!(
        none.is_empty(),
        "the one outside standing against itself names nothing"
    );
    set_root_mode(&root, 0o755);
}

#[test]
fn test_a_record_carrying_a_plain_root_mode_is_refused() {
    let (_scratch, root) = make_repo("plainrefusal");
    write_file(&root, "sub/helper.txt", "helper\n");
    set_dir_mode(&root, "sub", 0o500);
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "dir_modes" {
            let JsonValue::List(items) = value else {
                panic!("the member is a list");
            };
            for entry in items.iter_mut() {
                let JsonValue::Object(fields) = entry else {
                    panic!("the entry is an object");
                };
                for (field, standing) in fields.iter_mut() {
                    if field == "path" {
                        *standing = JsonValue::text(".");
                    }
                    if field == "mode" {
                        *standing = JsonValue::text("755");
                    }
                }
            }
        }
    }
    let rewritten = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&rewritten).expect_err("the plain root standing refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_record_carrying_the_root_entry_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    write_file(&root, "sub/helper.txt", "helper\n");
    set_dir_mode(&root, "sub", 0o500);
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "dir_modes" {
            let JsonValue::List(items) = value else {
                panic!("the member is a list");
            };
            for entry in items.iter_mut() {
                let JsonValue::Object(fields) = entry else {
                    panic!("the entry is an object");
                };
                for (field, standing) in fields.iter_mut() {
                    if field == "path" {
                        *standing = JsonValue::text(".");
                    }
                }
            }
            let first = items[0].clone();
            items.push(first);
        }
    }
    let doubled = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&doubled).expect_err("the doubled root entry refuses");
    assert_eq!(
        error.to_string(),
        "the fingerprint record carries the same member twice"
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_stamped_log_verifies_against_the_root_mode_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    set_root_mode(&root, 0o500);
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    set_root_mode(&root, 0o755);
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the opened root no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["directory mode changed .".to_string()]
    );
}

#[test]
fn test_a_submodules_root_mode_binds_in_its_own_record() {
    let (scratch, root) = make_submodule_parent("submodule");
    let _ = &scratch;
    set_dir_mode(&root, "sub", 0o755);
    let inner_plain = fingerprint(&root.join("sub"));
    let parent_plain = fingerprint(&root).digest();
    set_dir_mode(&root, "sub", 0o500);
    let inner = fingerprint(&root.join("sub"));
    assert_ne!(
        inner_plain.digest(),
        inner.digest(),
        "the submodule its root mode moves its own digest"
    );
    assert_eq!(
        dir_mode_pairs(&inner.to_dict()),
        vec![(".".to_string(), "500".to_string())],
        "the root entry stands in the submodule its own record"
    );
    let parent = fingerprint(&root);
    assert_ne!(
        parent_plain,
        parent.digest(),
        "the standing moves the parent through the digest"
    );
    assert_eq!(
        dir_mode_pairs(&parent.to_dict()),
        vec![("sub".to_string(), "500".to_string())],
        "the checkout directory its own entry stands in the parent under DM-3"
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_root_mode_stands_beside_other_directory_modes() {
    let (_scratch, root) = make_repo("beside");
    write_file(&root, "sub/helper.txt", "helper\n");
    set_dir_mode(&root, "sub", 0o077);
    set_root_mode(&root, 0o500);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dir_mode_pairs(&record),
        vec![
            (".".to_string(), "500".to_string()),
            ("sub".to_string(), "077".to_string()),
        ],
        "the root entry stands first in the member under the standing sort"
    );
    set_dir_mode(&root, "sub", 0o755);
    set_root_mode(&root, 0o755);
}
