//! The pins of slice 9, file permissions, over synthetic
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
    let scratch = Scratch::new("filemodes", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("filemodes", area);
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

fn file_modes_of(record: &JsonValue) -> Vec<(String, String)> {
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

#[test]
fn test_the_file_permission_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("apart");
    write_file(&root, "loose.txt", "loose\n");
    let plain = fingerprint(&root).digest();
    set_file_mode(&root, "loose.txt", 0o400);
    let closed = fingerprint(&root).digest();
    assert_ne!(plain, closed, "a file at 400 stamps apart");
    set_file_mode(&root, "loose.txt", 0o777);
    let open = fingerprint(&root).digest();
    assert_ne!(plain, open, "a file at 777 stamps apart");
    assert_ne!(
        closed, open,
        "the two standings stamp apart from each other"
    );
    set_file_mode(&root, "loose.txt", 0o644);
}

#[test]
fn test_a_tree_with_only_plain_file_modes_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    write_file(&root, "loose.txt", "loose\n");
    let mut quiet = Vec::new();
    for mode in [0o600, 0o640, 0o644, 0o660, 0o664] {
        set_file_mode(&root, "loose.txt", mode);
        let record = fingerprint(&root);
        assert!(
            record.to_dict().get("file_modes").is_none(),
            "a plain standing at {mode:o} carries no member"
        );
        quiet.push(record.digest());
    }
    assert!(
        quiet.windows(2).all(|pair| pair[0] == pair[1]),
        "every plain standing without an executable bit stamps as one"
    );
    let mut carrying = Vec::new();
    for mode in [0o700, 0o750, 0o755, 0o770, 0o775] {
        set_file_mode(&root, "loose.txt", mode);
        let record = fingerprint(&root);
        assert!(
            record.to_dict().get("file_modes").is_none(),
            "a plain standing at {mode:o} carries no member"
        );
        carrying.push(record.digest());
    }
    assert!(
        carrying.windows(2).all(|pair| pair[0] == pair[1]),
        "every plain standing with an executable bit stamps as one"
    );
    set_file_mode(&root, "loose.txt", 0o644);
}

#[test]
fn test_a_file_permission_entry_records_its_path_and_mode() {
    let (_scratch, root) = make_repo("entry");
    write_file(&root, "narrow.txt", "narrow\n");
    write_file(&root, "open.txt", "open\n");
    write_file(&root, "suid.txt", "suid\n");
    set_file_mode(&root, "narrow.txt", 0o400);
    set_file_mode(&root, "open.txt", 0o777);
    set_file_mode(&root, "suid.txt", 0o4755);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        file_modes_of(&record),
        vec![
            ("narrow.txt".to_string(), "400".to_string()),
            ("open.txt".to_string(), "777".to_string()),
            ("suid.txt".to_string(), "4755".to_string()),
        ],
        "the entries stand sorted by path with their octal spellings"
    );
    set_file_mode(&root, "narrow.txt", 0o644);
    set_file_mode(&root, "open.txt", 0o644);
    set_file_mode(&root, "suid.txt", 0o644);
}

#[test]
fn test_a_tracked_files_permission_binds() {
    let (_scratch, root) = make_repo("tracked");
    write_file(&root, "tracked.txt", "tracked\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    let plain = fingerprint(&root).digest();
    set_file_mode(&root, "tracked.txt", 0o400);
    let record = fingerprint(&root);
    assert_ne!(plain, record.digest());
    assert_eq!(
        file_modes_of(&record.to_dict()),
        vec![("tracked.txt".to_string(), "400".to_string())]
    );
    set_file_mode(&root, "tracked.txt", 0o644);
}

#[test]
fn test_an_executable_file_binds_its_permission_beside_its_mode_entry() {
    let (_scratch, root) = make_repo("overlap");
    write_file(&root, "tool.sh", "tool\n");
    set_file_mode(&root, "tool.sh", 0o711);
    let record = fingerprint(&root).to_dict();
    let JsonValue::List(modes) = record.get("modes").expect("the modes member stands") else {
        panic!("the modes member is a list");
    };
    assert_eq!(modes.len(), 1, "the executable standing stands in modes");
    assert_eq!(
        file_modes_of(&record),
        vec![("tool.sh".to_string(), "711".to_string())],
        "the full permission standing stands beside it"
    );
    set_file_mode(&root, "tool.sh", 0o644);
}

#[test]
fn test_a_file_permission_difference_names_a_file_permission_difference() {
    let (_scratch, root) = make_repo("difference");
    write_file(&root, "loose.txt", "loose\n");
    set_file_mode(&root, "loose.txt", 0o400);
    let closed = fingerprint(&root);
    set_file_mode(&root, "loose.txt", 0o644);
    let plain = fingerprint(&root);
    let differences = compare_fingerprints(&closed, &plain).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["file permission changed loose.txt".to_string()],
        "a plain side against a closed side names the one difference"
    );
    set_file_mode(&root, "loose.txt", 0o444);
    let read_only = fingerprint(&root);
    let differences = compare_fingerprints(&closed, &read_only).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["file permission changed loose.txt".to_string()],
        "two bound standings name the one difference"
    );
    set_file_mode(&root, "loose.txt", 0o644);
}

#[test]
fn test_a_record_carrying_the_same_file_permission_path_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    write_file(&root, "loose.txt", "loose\n");
    set_file_mode(&root, "loose.txt", 0o400);
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "file_modes" {
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
    set_file_mode(&root, "loose.txt", 0o644);
}

#[test]
fn test_a_record_carrying_a_plain_file_permission_is_refused() {
    let (_scratch, root) = make_repo("plainrefusal");
    write_file(&root, "loose.txt", "loose\n");
    set_file_mode(&root, "loose.txt", 0o400);
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "file_modes" {
            let JsonValue::List(items) = value else {
                panic!("the member is a list");
            };
            for entry in items.iter_mut() {
                let JsonValue::Object(fields) = entry else {
                    panic!("the entry is an object");
                };
                for (field, standing) in fields.iter_mut() {
                    if field == "mode" {
                        *standing = JsonValue::text("644");
                    }
                }
            }
        }
    }
    let rewritten = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&rewritten).expect_err("the plain standing refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
    set_file_mode(&root, "loose.txt", 0o644);
}

#[test]
fn test_a_record_carrying_a_misspelled_file_permission_is_refused() {
    let (_scratch, root) = make_repo("misspelled");
    write_file(&root, "loose.txt", "loose\n");
    set_file_mode(&root, "loose.txt", 0o400);
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "file_modes" {
            let JsonValue::List(items) = value else {
                panic!("the member is a list");
            };
            for entry in items.iter_mut() {
                let JsonValue::Object(fields) = entry else {
                    panic!("the entry is an object");
                };
                for (field, standing) in fields.iter_mut() {
                    if field == "mode" {
                        *standing = JsonValue::text("100755");
                    }
                }
            }
        }
    }
    let rewritten = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&rewritten).expect_err("the git spelling refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
    set_file_mode(&root, "loose.txt", 0o644);
}

#[test]
fn test_a_stamped_log_verifies_against_the_file_permissions_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    write_file(&root, "loose.txt", "loose\n");
    set_file_mode(&root, "loose.txt", 0o400);
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    set_file_mode(&root, "loose.txt", 0o644);
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the opened file no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["file permission changed loose.txt".to_string()]
    );
}

#[test]
fn test_a_submodules_file_permission_binds_through_its_parent() {
    let (scratch, root) = make_submodule_parent("nested");
    let _ = &scratch;
    let bare = fingerprint(&root).digest();
    set_file_mode(&root, "sub/inner.txt", 0o400);
    let bound = fingerprint(&root);
    assert_ne!(
        bare,
        bound.digest(),
        "the interior standing moves the parent"
    );
    assert!(
        bound.to_dict().get("file_modes").is_none(),
        "the interior standing names no entry in the parent record"
    );
    set_file_mode(&root, "sub/inner.txt", 0o644);
}

#[test]
fn test_a_file_permission_under_an_ignored_path_stands_named() {
    let (_scratch, root) = make_repo("ignored");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "ignore"]);
    write_file(&root, "build/artifact.bin", "artifact\n");
    set_file_mode(&root, "build/artifact.bin", 0o400);
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        file_modes_of(&record),
        vec![("build/artifact.bin".to_string(), "400".to_string())],
        "the ignore rules reach no part of the member"
    );
    set_file_mode(&root, "build/artifact.bin", 0o644);
}

#[test]
fn test_a_symlink_stands_in_no_file_permission_entry() {
    let (_scratch, root) = make_repo("linked");
    write_file(&root, "target.txt", "target\n");
    std::os::unix::fs::symlink("target.txt", root.join("link.txt")).expect("the link stands");
    let record = fingerprint(&root).to_dict();
    assert!(
        file_modes_of(&record).is_empty(),
        "a link standing names no file permission entry"
    );
    assert!(
        record.get("file_modes").is_none(),
        "the member stands absent where only a link stands outside the family"
    );
}
