//! The pins of slice 8, directory modes, over synthetic
//! fixture repos the suite builds with git. The submodule
//! fixtures stand as local repositories joined with the
//! file protocol standing allowed in the fixture alone.

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
    let scratch = Scratch::new("dirmodes", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("dirmodes", area);
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

fn set_dir_mode(root: &Path, relative: &str, mode: u32) {
    let full = root.join(relative);
    let mut permissions = std::fs::metadata(&full)
        .expect("the directory stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(&full, permissions).expect("the mode stands");
}

fn dir_modes_of(record: &JsonValue) -> Vec<(String, String)> {
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
fn test_the_directory_mode_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("apart");
    write_file(&root, "sub/helper.txt", "helper\n");
    let plain = fingerprint(&root).digest();
    set_dir_mode(&root, "sub", 0o500);
    let closed = fingerprint(&root).digest();
    assert_ne!(plain, closed, "a directory at 500 stamps apart");
    set_dir_mode(&root, "sub", 0o777);
    let open = fingerprint(&root).digest();
    assert_ne!(plain, open, "a directory at 777 stamps apart");
    assert_ne!(
        closed, open,
        "the two standings stamp apart from each other"
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_tree_with_only_plain_directory_modes_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    write_file(&root, "sub/helper.txt", "helper\n");
    let mut digests = Vec::new();
    for mode in [0o755, 0o775, 0o750, 0o700, 0o770] {
        set_dir_mode(&root, "sub", mode);
        let record = fingerprint(&root);
        assert!(
            record.to_dict().get("dir_modes").is_none(),
            "a plain standing at {mode:o} carries no member"
        );
        digests.push(record.digest());
    }
    assert!(
        digests.windows(2).all(|pair| pair[0] == pair[1]),
        "every plain standing stamps as one"
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_directory_mode_entry_records_its_path_and_mode() {
    let (_scratch, root) = make_repo("entry");
    write_file(&root, "sub/helper.txt", "helper\n");
    write_file(&root, "sticky/note.txt", "note\n");
    write_file(&root, "narrow/deep.txt", "deep\n");
    set_dir_mode(&root, "sub", 0o500);
    set_dir_mode(&root, "sticky", 0o1777);
    set_dir_mode(&root, "narrow", 0o707);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dir_modes_of(&record),
        vec![
            ("narrow".to_string(), "707".to_string()),
            ("sticky".to_string(), "1777".to_string()),
            ("sub".to_string(), "500".to_string()),
        ],
        "the entries stand sorted by path with their octal spellings"
    );
    set_dir_mode(&root, "sub", 0o755);
    set_dir_mode(&root, "sticky", 0o755);
    set_dir_mode(&root, "narrow", 0o755);
}

#[test]
fn test_a_directory_holding_only_tracked_files_binds_its_mode() {
    let (_scratch, root) = make_repo("tracked");
    write_file(&root, "sub/tracked.txt", "tracked\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    let plain = fingerprint(&root).digest();
    set_dir_mode(&root, "sub", 0o500);
    let record = fingerprint(&root);
    assert_ne!(plain, record.digest());
    assert_eq!(
        dir_modes_of(&record.to_dict()),
        vec![("sub".to_string(), "500".to_string())]
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_an_empty_directory_binds_its_mode_beside_its_empty_standing() {
    let (_scratch, root) = make_repo("empty");
    std::fs::create_dir_all(root.join("hollow")).expect("the directory stands");
    set_dir_mode(&root, "hollow", 0o500);
    let record = fingerprint(&root).to_dict();
    let JsonValue::List(empty) = record.get("empty_dirs").expect("the member stands") else {
        panic!("the empty member is a list");
    };
    assert_eq!(empty.len(), 1, "the directory stands empty as well");
    assert_eq!(
        dir_modes_of(&record),
        vec![("hollow".to_string(), "500".to_string())]
    );
    set_dir_mode(&root, "hollow", 0o755);
}

#[test]
fn test_a_directory_mode_difference_names_a_directory_mode_difference() {
    let (_scratch, root) = make_repo("difference");
    write_file(&root, "sub/helper.txt", "helper\n");
    set_dir_mode(&root, "sub", 0o500);
    let closed = fingerprint(&root);
    set_dir_mode(&root, "sub", 0o755);
    let plain = fingerprint(&root);
    let differences = compare_fingerprints(&closed, &plain).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["directory mode changed sub".to_string()],
        "a plain side against a closed side names the one difference"
    );
    set_dir_mode(&root, "sub", 0o777);
    let open = fingerprint(&root);
    let differences = compare_fingerprints(&closed, &open).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["directory mode changed sub".to_string()],
        "two bound standings name the one difference"
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_record_carrying_the_same_directory_mode_path_twice_is_refused() {
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
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_record_carrying_a_plain_directory_mode_is_refused() {
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
                    if field == "mode" {
                        *standing = JsonValue::text("755");
                    }
                }
            }
        }
    }
    let rewritten = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&rewritten).expect_err("the plain standing refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_stamped_log_verifies_against_the_directory_modes_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    write_file(&root, "sub/helper.txt", "helper\n");
    set_dir_mode(&root, "sub", 0o500);
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    set_dir_mode(&root, "sub", 0o755);
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the opened directory no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["directory mode changed sub".to_string()]
    );
}

#[test]
fn test_a_submodules_directory_mode_binds_through_its_parent() {
    let (scratch, root) = make_submodule_parent("nested");
    let _ = &scratch;
    std::fs::create_dir_all(root.join("sub/inner")).expect("the directory stands");
    write_file(&root, "sub/inner/deep.txt", "deep\n");
    let bare = fingerprint(&root).digest();
    set_dir_mode(&root, "sub/inner", 0o500);
    let bound = fingerprint(&root);
    assert_ne!(
        bare,
        bound.digest(),
        "the interior standing moves the parent"
    );
    assert!(
        bound.to_dict().get("dir_modes").is_none(),
        "the interior standing names no entry in the parent record"
    );
    set_dir_mode(&root, "sub/inner", 0o755);
}

#[test]
fn test_a_gitlink_directorys_own_mode_binds_in_its_parent() {
    let (scratch, root) = make_submodule_parent("gitlink");
    let _ = &scratch;
    let bare = fingerprint(&root).digest();
    set_dir_mode(&root, "sub", 0o500);
    let bound = fingerprint(&root);
    assert_ne!(bare, bound.digest());
    assert_eq!(
        dir_modes_of(&bound.to_dict()),
        vec![("sub".to_string(), "500".to_string())],
        "the gitlink directory itself stands named in the parent"
    );
    set_dir_mode(&root, "sub", 0o755);
}

#[test]
fn test_a_directory_mode_under_an_ignored_path_stands_named() {
    let (_scratch, root) = make_repo("ignored");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "ignore"]);
    write_file(&root, "build/artifact.bin", "artifact\n");
    set_dir_mode(&root, "build", 0o500);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        dir_modes_of(&record),
        vec![("build".to_string(), "500".to_string())],
        "the ignore rules reach no part of the member"
    );
    set_dir_mode(&root, "build", 0o755);
}
