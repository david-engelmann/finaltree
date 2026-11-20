//! The pins of slice 12, the directory listing refusal,
//! over synthetic fixture repos the suite builds with
//! git. Each refusal pin guards its assertions behind a
//! listing of its own fixture directory by the test
//! process itself, so the pin asserts the refusal only
//! where the directory truly cannot be listed by the
//! user the suite runs as (RF-6), the hosted runner
//! supplying such a user and a root session supplying
//! none. The submodule fixture stands as a local
//! repository joined with the file protocol standing
//! allowed in the fixture alone.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{git, write_file, Scratch};
use finaltree::fingerprint::{compute_fingerprint, display_path};
use finaltree::jsonvalue::JsonValue;
use finaltree::stamping::{run_and_stamp, verify_stamped_log};

/// A repo holding one committed file of its own.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("lockeddirectories", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("lockeddirectories", area);
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

fn set_mode(path: &Path, mode: u32) {
    let mut permissions = std::fs::metadata(path)
        .expect("the path stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions).expect("the mode stands");
}

fn set_dir_mode(root: &Path, relative: &str, mode: u32) {
    set_mode(&root.join(relative), mode);
}

fn set_root_mode(root: &Path, mode: u32) {
    set_mode(root, mode);
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

/// Whether the test process itself cannot list the
/// directory, the guard of RF-6. Where the suite runs
/// as a user the permission bits do not bind, the
/// listing stands and the guarded pins assert nothing.
fn cannot_list(path: &Path) -> bool {
    std::fs::read_dir(path).is_err()
}

/// The computation over the tree refuses with the
/// expected sentence of RF-2.
fn assert_refusal(root: &Path, patterns: &[String], expected: &str) {
    let error = compute_fingerprint(root, patterns).expect_err("the computation refuses");
    assert_eq!(error.to_string(), expected);
}

/// The paths a member of the record names, in the
/// order the record carries them.
fn member_paths(record: &JsonValue, member: &str) -> Vec<String> {
    let Some(JsonValue::List(items)) = record.get(member) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|entry| {
            let JsonValue::Text(path) = entry.get("path").expect("the path stands") else {
                panic!("the path is a text");
            };
            path.clone()
        })
        .collect()
}

/// The blob a member of the record carries for a path.
fn member_blob(record: &JsonValue, member: &str, wanted: &str) -> String {
    let Some(JsonValue::List(items)) = record.get(member) else {
        panic!("the member stands");
    };
    for entry in items {
        if matches!(entry.get("path"), Some(JsonValue::Text(path)) if path == wanted) {
            let JsonValue::Text(blob) = entry.get("blob").expect("the blob stands") else {
                panic!("the blob is a text");
            };
            return blob.clone();
        }
    }
    panic!("the entry stands");
}

#[test]
fn test_a_locked_directory_holding_an_untracked_file_refuses() {
    let (_scratch, root) = make_repo("untracked");
    write_file(&root, "locked/loose.txt", "loose\n");
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_an_ignored_file_refuses() {
    let (_scratch, root) = make_repo("ignored");
    write_file(&root, ".gitignore", "ignored.bin\n");
    write_file(&root, "locked/ignored.bin", "ignored\n");
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_a_generated_artifact_refuses() {
    let (_scratch, root) = make_repo("generated");
    write_file(&root, "locked/out.bin", "artifact\n");
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(
            &root,
            &["locked/*".to_string()],
            "the directory at locked cannot be read",
        );
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_a_file_below_a_dot_git_name_refuses() {
    let (_scratch, root) = make_repo("dotgitname");
    write_file(&root, "sub/locked/.git", "payload\n");
    set_dir_mode(&root, "sub/locked", 0o000);
    if cannot_list(&root.join("sub/locked")) {
        assert_refusal(&root, &[], "the directory at sub/locked cannot be read");
    }
    set_dir_mode(&root, "sub/locked", 0o755);
}

#[test]
fn test_a_locked_directory_affirmed_empty_refuses() {
    let (_scratch, root) = make_repo("affirmed");
    write_file(&root, "plain/.git/locked/inner.txt", "inner\n");
    set_dir_mode(&root, "plain/.git/locked", 0o000);
    if cannot_list(&root.join("plain/.git/locked")) {
        assert_refusal(
            &root,
            &[],
            "the directory at plain/.git/locked cannot be read",
        );
    }
    set_dir_mode(&root, "plain/.git/locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_a_special_file_refuses() {
    let (_scratch, root) = make_repo("special");
    make_fifo(&root, "locked/pipe");
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_a_file_at_a_wide_mode_refuses() {
    let (_scratch, root) = make_repo("widefile");
    let wide = write_file(&root, "locked/wide.txt", "wide\n");
    set_mode(&wide, 0o400);
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_a_directory_at_a_wide_mode_refuses() {
    let (_scratch, root) = make_repo("widedir");
    write_file(&root, "locked/inner/deep.txt", "deep\n");
    set_dir_mode(&root, "locked/inner", 0o500);
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_holding_a_tracked_file_refuses() {
    let (_scratch, root) = make_repo("tracked");
    write_file(&root, "locked/tracked.txt", "tracked\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    write_file(&root, "locked/tracked.txt", "tracked again\n");
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_locked_directory_inside_a_submodule_refuses_the_parent() {
    let (_scratch, root) = make_submodule_parent("submodule");
    write_file(&root, "sub/locked/loose.txt", "loose\n");
    set_dir_mode(&root, "sub/locked", 0o000);
    if cannot_list(&root.join("sub/locked")) {
        assert_refusal(&root, &[], "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "sub/locked", 0o755);
}

#[test]
fn test_a_root_that_cannot_be_listed_refuses() {
    let (_scratch, root) = make_repo("rootcall");
    write_file(&root, "loose.txt", "loose\n");
    set_root_mode(&root, 0o111);
    if cannot_list(&root) {
        let expected = format!("the tree at {} cannot be read", display_path(&root));
        assert_refusal(&root, &[], &expected);
    }
    set_root_mode(&root, 0o755);
}

#[test]
fn test_the_verify_door_refuses_a_tree_that_gained_a_locked_directory() {
    let (scratch, root) = make_repo("verifydoor");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    write_file(&root, "locked/loose.txt", "loose\n");
    set_dir_mode(&root, "locked", 0o000);
    if cannot_list(&root.join("locked")) {
        let error = verify_stamped_log(&log, &root, None).expect_err("the verify refuses");
        assert_eq!(error.to_string(), "the directory at locked cannot be read");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_the_stamp_door_refuses_a_locked_tree() {
    let (scratch, root) = make_repo("stampdoor");
    write_file(&root, "locked/loose.txt", "loose\n");
    set_dir_mode(&root, "locked", 0o000);
    let log = scratch.0.join("stamped.log");
    if cannot_list(&root.join("locked")) {
        let error =
            run_and_stamp(&root, &["true".to_string()], &log, &[]).expect_err("the stamp refuses");
        assert_eq!(error.to_string(), "the directory at locked cannot be read");
        assert!(!log.exists(), "no log stands written");
    }
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_directory_at_mode_500_holding_an_untracked_file_fingerprints_whole() {
    let (_scratch, root) = make_repo("readable");
    write_file(&root, "gen/out.bin", "artifact one\n");
    write_file(&root, "gen/note.txt", "note\n");
    set_dir_mode(&root, "gen", 0o500);
    let patterns = vec!["gen/*.bin".to_string()];
    let record = compute_fingerprint(&root, &patterns).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert_eq!(
        member_paths(&dict, "generated"),
        vec!["gen/out.bin".to_string()],
        "the artifact stands bound in the generated member"
    );
    assert_eq!(
        member_paths(&dict, "untracked"),
        vec!["gen/note.txt".to_string()],
        "the untracked file stands bound beside it"
    );
    let blob_before = member_blob(&dict, "generated", "gen/out.bin");
    let digest_before = record.digest();
    write_file(&root, "gen/out.bin", "artifact two\n");
    let moved = compute_fingerprint(&root, &patterns).expect("the tree fingerprints");
    assert_ne!(
        blob_before,
        member_blob(&moved.to_dict(), "generated", "gen/out.bin"),
        "the bound blob moves with the artifact its content"
    );
    assert_ne!(digest_before, moved.digest());
    set_dir_mode(&root, "gen", 0o755);
}

#[test]
fn test_a_directory_at_mode_444_holding_a_tracked_file_fingerprints_without_refusal() {
    let (_scratch, root) = make_repo("grain");
    write_file(&root, "locked/tracked.txt", "tracked\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    write_file(&root, "locked/tracked.txt", "tracked again\n");
    set_dir_mode(&root, "locked", 0o444);
    let result = compute_fingerprint(&root, &[]);
    assert!(
        result.is_ok(),
        "the computation completes without the directory refusal"
    );
    set_dir_mode(&root, "locked", 0o755);
}

#[test]
fn test_a_root_at_mode_000_refuses_at_the_git_door() {
    let (_scratch, root) = make_repo("rootfence");
    set_root_mode(&root, 0o000);
    if cannot_list(&root) {
        let expected = format!(
            "the tree at {} is not a git working tree",
            display_path(&root)
        );
        assert_refusal(&root, &[], &expected);
    }
    set_root_mode(&root, 0o755);
}
