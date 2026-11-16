//! The pins of slice 7, special files, over synthetic
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
    let scratch = Scratch::new("specialfiles", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("specialfiles", area);
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

/// Bind a Unix socket at the path and hold it standing.
/// The listener is leaked so the socket file outlives the
/// fingerprint readings the pin takes.
fn make_socket(root: &Path, relative: &str) {
    let full = root.join(relative);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("the parent stands");
    }
    let listener = std::os::unix::net::UnixListener::bind(&full).expect("the socket stands");
    std::mem::forget(listener);
}

/// The special member of a record as (path, kind) pairs.
fn special_entries(record: &JsonValue) -> Vec<(String, String)> {
    let Some(member) = record.get("special") else {
        return Vec::new();
    };
    let JsonValue::List(items) = member else {
        panic!("the special member is a list");
    };
    items
        .iter()
        .map(|entry| {
            let (Some(JsonValue::Text(path)), Some(JsonValue::Text(kind))) =
                (entry.get("path"), entry.get("kind"))
            else {
                panic!("the entry carries a path and a kind");
            };
            (path.clone(), kind.clone())
        })
        .collect()
}

#[test]
fn test_the_special_file_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("standings");
    let bare = fingerprint(&root).digest();
    make_fifo(&root, "pipe");
    let piped = fingerprint(&root);
    assert_ne!(bare, piped.digest(), "one pipe binds its tree");
    assert_eq!(
        special_entries(&piped.to_dict()),
        vec![("pipe".to_string(), "fifo".to_string())],
        "the pipe stands named with its kind"
    );
    make_socket(&root, "sock");
    let socked = fingerprint(&root);
    assert_ne!(piped.digest(), socked.digest(), "a socket binds further");
    assert_eq!(
        special_entries(&socked.to_dict()),
        vec![
            ("pipe".to_string(), "fifo".to_string()),
            ("sock".to_string(), "socket".to_string())
        ],
        "both standings stand named, sorted by path"
    );
}

#[test]
fn test_a_tree_without_special_files_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    write_file(&root, "loose.txt", "loose\n");
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("special").is_none(),
        "no special member stands where no special file stands"
    );
    let read = Fingerprint::from_dict(&record).expect("the record reads back");
    assert_eq!(read.digest(), fingerprint(&root).digest());
}

#[test]
fn test_a_special_file_under_an_ignored_path_stands_named() {
    let (_scratch, root) = make_repo("ignoredpath");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "ignore rules"]);
    make_fifo(&root, "build/pipe");
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        special_entries(&record),
        vec![("build/pipe".to_string(), "fifo".to_string())],
        "the ignore rules reach no part of the special member"
    );
}

#[test]
fn test_a_special_file_standing_at_a_tracked_path_stands_named() {
    let (_scratch, root) = make_repo("trackedpath");
    std::fs::remove_file(root.join("own.txt")).expect("the file lifts");
    make_fifo(&root, "own.txt");
    let computed = fingerprint(&root);
    assert_eq!(
        special_entries(&computed.to_dict()),
        vec![("own.txt".to_string(), "fifo".to_string())],
        "the pipe stands named beside the tracked member's own reading"
    );
    let tracked = computed
        .to_dict()
        .get("tracked")
        .cloned()
        .expect("the tracked member stands");
    let JsonValue::List(items) = tracked else {
        panic!("the tracked member is a list");
    };
    assert!(
        items
            .iter()
            .any(|entry| entry.get("path") == Some(&JsonValue::text("own.txt"))),
        "the tracked member still names the path"
    );
}

#[test]
fn test_a_special_file_under_a_declared_pattern_stands_in_the_special_member() {
    let (_scratch, root) = make_repo("pattern");
    make_fifo(&root, "gen-pipe.bin");
    let computed = compute_fingerprint(&root, &["gen-*".to_string()])
        .expect("the tree fingerprints whole under the pattern");
    let record = computed.to_dict();
    assert_eq!(
        special_entries(&record),
        vec![("gen-pipe.bin".to_string(), "fifo".to_string())],
        "the pipe stands in the special member"
    );
    let generated = record.get("generated").cloned().expect("the member stands");
    let JsonValue::List(items) = generated else {
        panic!("the generated member is a list");
    };
    assert!(
        items.is_empty(),
        "the pipe stands nowhere in the generated member"
    );
}

#[test]
fn test_a_special_file_difference_names_a_special_file_difference() {
    let (_scratch, root) = make_repo("difference");
    let bare = fingerprint(&root);
    make_fifo(&root, "pipe");
    let standing = fingerprint(&root);
    let added = compare_fingerprints(&bare, &standing).expect("the comparison runs");
    assert_eq!(added, vec!["special file added pipe".to_string()]);
    let removed = compare_fingerprints(&standing, &bare).expect("the comparison runs");
    assert_eq!(removed, vec!["special file removed pipe".to_string()]);
}

#[test]
fn test_a_special_file_kind_difference_names_a_special_file_changed() {
    let (_scratch, root) = make_repo("kindchange");
    make_fifo(&root, "endpoint");
    let piped = fingerprint(&root);
    std::fs::remove_file(root.join("endpoint")).expect("the pipe lifts");
    make_socket(&root, "endpoint");
    let socked = fingerprint(&root);
    let changed = compare_fingerprints(&piped, &socked).expect("the comparison runs");
    assert_eq!(changed, vec!["special file changed endpoint".to_string()]);
}

#[test]
fn test_a_record_carrying_the_same_special_file_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    make_fifo(&root, "pipe");
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "special" {
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
fn test_a_stamped_log_verifies_against_the_special_files_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    make_fifo(&root, "pipe");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    std::fs::remove_file(root.join("pipe")).expect("the pipe lifts");
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the lifted pipe no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["special file removed pipe".to_string()]
    );
}

#[test]
fn test_a_submodules_special_file_binds_through_its_parent() {
    let (scratch, root) = make_submodule_parent("nested");
    let _ = &scratch;
    let bare = fingerprint(&root).digest();
    make_fifo(&root, "sub/pipe");
    let standing = fingerprint(&root);
    assert_ne!(
        bare,
        standing.digest(),
        "the submodule's pipe binds the parent through its digest"
    );
    assert!(
        special_entries(&standing.to_dict()).is_empty(),
        "the parent's own member names nothing below the gitlink"
    );
    let own = fingerprint(&root.join("sub"));
    assert_eq!(
        special_entries(&own.to_dict()),
        vec![("pipe".to_string(), "fifo".to_string())],
        "the submodule's own fingerprint names the pipe"
    );
}

#[test]
fn test_a_special_file_inside_an_uninitialized_submodule_stands_named_nowhere() {
    let (_scratch, root) = make_submodule_parent("uninitialized");
    git(&root, &["submodule", "deinit", "-q", "sub"]);
    let entries = special_entries(&fingerprint(&root).to_dict());
    assert!(
        entries.is_empty(),
        "an uninitialized submodule names no special file"
    );
}

#[test]
fn test_a_directory_holding_only_a_special_file_is_not_empty() {
    let (_scratch, root) = make_repo("holding");
    make_fifo(&root, "holder/pipe");
    let record = fingerprint(&root).to_dict();
    assert_eq!(
        special_entries(&record),
        vec![("holder/pipe".to_string(), "fifo".to_string())],
        "the pipe stands named in the special member"
    );
    let empty = record.get("empty_dirs").cloned();
    let names: Vec<String> = match empty {
        Some(JsonValue::List(items)) => items
            .iter()
            .map(|entry| match entry {
                JsonValue::Text(path) => path.clone(),
                _ => panic!("the entry is a path string"),
            })
            .collect(),
        _ => Vec::new(),
    };
    assert!(
        !names.iter().any(|path| path == "holder"),
        "the holding directory stands named nowhere in the empty_dirs member"
    );
}
