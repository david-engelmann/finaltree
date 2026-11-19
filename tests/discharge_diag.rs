//! Owner discharge diag for finaltree Round 13, slice 12,
//! evidence only and never to be merged. The four sets of
//! the drafting division stand here in the record
//! inspection form of the banked driver, over fixture
//! repos the file builds itself. On the built head AFTER
//! stands true and every locked standing refuses with
//! the slice sentences. On the contract head AFTER stands
//! false and the same fixtures carry the pre slice
//! standings the drafting states, the silence and the
//! equalities and the doors answering. Every assertion
//! stands guarded behind the listing check of RF-6, so
//! the file asserts only where the mode bits truly bind
//! the user the suite runs as.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{git, pinned, write_file, Scratch};
use finaltree::fingerprint::{compute_fingerprint, display_path};
use finaltree::jsonvalue::JsonValue;
use finaltree::render::render_printed;
use finaltree::stamping::{run_and_stamp, verify_stamped_log};

const AFTER: bool = false;

fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("dischargediag", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn make_submodule_parent(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("dischargediag", area);
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

fn cannot_list(path: &Path) -> bool {
    std::fs::read_dir(path).is_err()
}

fn assert_refusal(root: &Path, expected: &str) {
    let error = compute_fingerprint(root, &[]).expect_err("the computation refuses");
    assert_eq!(error.to_string(), expected);
}

fn record_text(root: &Path) -> String {
    let record = compute_fingerprint(root, &[]).expect("the tree fingerprints");
    render_printed(&record.to_dict())
}

fn blob_hash(root: &Path, relative: &str) -> String {
    let output = pinned(&mut std::process::Command::new("git"))
        .args(["hash-object", "--", relative])
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "the blob hash stands");
    String::from_utf8(output.stdout)
        .expect("the hash is text")
        .trim()
        .to_string()
}

/// Set A, the deciding shape. A tracked file below a
/// directory that cannot be listed, clean and modified.
/// Before the slice the two locked standings carry one
/// digest and the tracked member carries the null blob
/// in both. After it both standings refuse at the
/// directories gather in the directory sentence.
#[test]
fn test_set_a_the_deciding_shape() {
    let (_scratch, root) = make_repo("deciding");
    std::fs::create_dir_all(root.join("locked")).expect("the directory stands");
    write_file(&root, "locked/tracked.txt", "tracked-v1\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    let locked = root.join("locked");
    set_mode(&locked, 0);
    if cannot_list(&locked) {
        if AFTER {
            assert_refusal(&root, "the directory at locked cannot be read");
        } else {
            let clean = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            let entry = clean
                .tracked
                .iter()
                .find(|file| matches!(&file.path, JsonValue::Text(path) if path == "locked/tracked.txt"))
                .expect("the tracked member names the file");
            assert_eq!(
                entry.blob,
                JsonValue::Null,
                "the tracked blob stands null in the clean locked standing"
            );
            set_mode(&locked, 0o755);
            write_file(&root, "locked/tracked.txt", "tracked-v2\n");
            set_mode(&locked, 0);
            let modified = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            assert_eq!(
                clean.digest(),
                modified.digest(),
                "the locked clean and locked modified records stand equal"
            );
            let entry = modified
                .tracked
                .iter()
                .find(|file| matches!(&file.path, JsonValue::Text(path) if path == "locked/tracked.txt"))
                .expect("the tracked member names the file");
            assert_eq!(
                entry.blob,
                JsonValue::Null,
                "the tracked blob stands null in the modified locked standing"
            );
        }
    }
    if AFTER && cannot_list(&locked) {
        set_mode(&locked, 0o755);
        write_file(&root, "locked/tracked.txt", "tracked-v2\n");
        set_mode(&locked, 0);
        assert_refusal(&root, "the directory at locked cannot be read");
    }
    set_mode(&locked, 0o755);
}

/// Set A, the named nowhere claim. A locked directory
/// holding only an untracked file. Before the slice the
/// record names neither the file nor the hash of its
/// content in any member, and its digest equals the
/// twin with the directory standing locked and empty.
/// After it the standing refuses in the directory
/// sentence.
#[test]
fn test_set_a_the_named_nowhere_claim() {
    let (_scratch, root) = make_repo("nowhere");
    std::fs::create_dir_all(root.join("locked")).expect("the directory stands");
    write_file(&root, "locked/hidden.txt", "hidden\n");
    let blob = blob_hash(&root, "locked/hidden.txt");
    let locked = root.join("locked");
    set_mode(&locked, 0);
    if cannot_list(&locked) {
        if AFTER {
            assert_refusal(&root, "the directory at locked cannot be read");
        } else {
            let text = record_text(&root);
            assert!(
                !text.contains("hidden"),
                "the file name stands nowhere in the record"
            );
            assert!(
                !text.contains(&blob),
                "the blob of the file content stands nowhere in the record"
            );
            let with_hidden = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            set_mode(&locked, 0o755);
            std::fs::remove_file(root.join("locked/hidden.txt")).expect("the file clears");
            set_mode(&locked, 0);
            let empty = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            assert_eq!(
                with_hidden.digest(),
                empty.digest(),
                "the locked directory binds the same standing empty or holding the file"
            );
        }
    }
    set_mode(&locked, 0o755);
}

/// Set B, the pair inspections. The dot git name pair
/// of the scoping attempts. Before the slice each tree
/// fingerprints in silence, the name standing nowhere
/// and the digest equal to the twin with the locked
/// directory standing empty. After it each refuses at
/// its own descent in the directory sentence.
#[test]
fn test_set_b_the_dot_git_pair() {
    let (_scratch, root) = make_repo("dotgitname");
    std::fs::create_dir_all(root.join("sub/locked")).expect("the directories stand");
    write_file(&root, "sub/locked/.git", "helper\n");
    let locked = root.join("sub/locked");
    set_mode(&locked, 0);
    if cannot_list(&locked) {
        if AFTER {
            assert_refusal(&root, "the directory at sub/locked cannot be read");
        } else {
            let text = record_text(&root);
            assert!(
                !text.contains("locked/.git"),
                "the dot git name stands nowhere in the record"
            );
            let with_file = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            set_mode(&locked, 0o755);
            std::fs::remove_file(root.join("sub/locked/.git")).expect("the file clears");
            set_mode(&locked, 0);
            let empty = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            assert_eq!(
                with_file.digest(),
                empty.digest(),
                "the locked directory binds the same standing empty or holding the name"
            );
        }
    }
    set_mode(&locked, 0o755);

    let (_scratch2, root2) = make_repo("dotgitinside");
    std::fs::create_dir_all(root2.join("plain/.git/locked")).expect("the directories stand");
    write_file(&root2, "plain/.git/locked/file.txt", "ordinary\n");
    let locked2 = root2.join("plain/.git/locked");
    set_mode(&locked2, 0);
    if cannot_list(&locked2) {
        if AFTER {
            assert_refusal(&root2, "the directory at plain/.git/locked cannot be read");
        } else {
            let text = record_text(&root2);
            assert!(
                !text.contains("locked/file.txt"),
                "the file below the folder named dot git stands nowhere in the record"
            );
        }
    }
    set_mode(&locked2, 0o755);
}

/// Set C, the submodule propagation. Before the slice
/// the parent computation completes over a submodule
/// whose interior holds a locked directory, the
/// submodule digest standing equal with the interior
/// file standing or removed. After it the parent
/// refuses on the refusal the submodule computation
/// raises, naming the directory relative to the
/// submodule own root.
#[test]
fn test_set_c_the_submodule_propagation() {
    let (_scratch, root) = make_submodule_parent("propagation");
    let sub_locked = root.join("sub/locked");
    std::fs::create_dir_all(&sub_locked).expect("the directory stands");
    write_file(&root, "sub/locked/deep.txt", "subhidden\n");
    set_mode(&sub_locked, 0);
    if cannot_list(&sub_locked) {
        if AFTER {
            assert_refusal(&root, "the directory at locked cannot be read");
        } else {
            let parent = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            let digest_with = parent.submodules[0].digest.clone();
            set_mode(&sub_locked, 0o755);
            std::fs::remove_file(root.join("sub/locked/deep.txt")).expect("the file clears");
            set_mode(&sub_locked, 0);
            let parent = compute_fingerprint(&root, &[]).expect("the tree fingerprints");
            assert_eq!(
                digest_with, parent.submodules[0].digest,
                "the submodule digest stands equal with the interior file standing or removed"
            );
        }
    }
    set_mode(&sub_locked, 0o755);
}

/// Set C, the doors. A tree stamped readable gains a
/// locked directory holding an untracked file. Before
/// the slice the verify door answers over the
/// recomputation and the stamp door runs over the
/// locked tree and writes its log. After it both
/// doors refuse in the directory sentence, the stamp
/// refusal standing ahead of its run with no log
/// written.
#[test]
fn test_set_c_the_doors() {
    let (scratch, root) = make_repo("doors");
    std::fs::create_dir_all(root.join("locked")).expect("the directory stands");
    write_file(&root, "locked/late.txt", "late\n");
    let log = scratch.0.join("stamped.log");
    run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    let locked = root.join("locked");
    set_mode(&locked, 0);
    if cannot_list(&locked) {
        if AFTER {
            let error = verify_stamped_log(&log, &root, None).expect_err("the verify refuses");
            assert_eq!(
                error.to_string(),
                "the directory at locked cannot be read",
                "the verify door refuses in the directory sentence"
            );
            let log2 = scratch.0.join("stamped-locked.log");
            let error = run_and_stamp(&root, &["true".to_string()], &log2, &[])
                .expect_err("the stamp refuses");
            assert_eq!(
                error.to_string(),
                "the directory at locked cannot be read",
                "the stamp door refuses in the directory sentence"
            );
            assert!(!log2.exists(), "no log stands written by the refused stamp");
        } else {
            let answer = verify_stamped_log(&log, &root, None)
                .expect("the verify door answers over the recomputation");
            assert!(
                !answer.matched,
                "the verify answers a difference over the gained locked directory"
            );
            assert!(
                !answer.differences.is_empty(),
                "the difference stands named in the answer"
            );
            let log2 = scratch.0.join("stamped-locked.log");
            let stamped = run_and_stamp(&root, &["true".to_string()], &log2, &[]);
            assert!(stamped.is_ok(), "the stamp door runs over the locked tree");
            assert!(log2.exists(), "the stamp log stands written");
        }
    }
    set_mode(&locked, 0o755);
}

/// Set D, the root standings. The git half of this set
/// stands executed in the scoping session and banked
/// with its outputs. The binary half stands here. A
/// root at execute only, holding an untracked file at
/// the root, completes before the slice with the file
/// invisible, and refuses after it at the root call
/// in the tree sentence. A root at mode 000 refuses
/// at the git door on both heads in the standing git
/// door sentence.
#[test]
fn test_set_d_the_root_standings() {
    let (_scratch, root) = make_repo("rootbinary");
    write_file(&root, "loose.txt", "loose\n");
    set_mode(&root, 0o111);
    if cannot_list(&root) {
        if AFTER {
            let expected = format!("the tree at {} cannot be read", display_path(&root));
            assert_refusal(&root, &expected);
        } else {
            let text = record_text(&root);
            assert!(
                !text.contains("loose.txt"),
                "the untracked file at the root stands invisible in the record"
            );
        }
    }
    set_mode(&root, 0o755);
    set_mode(&root, 0);
    if cannot_list(&root) {
        let expected = format!(
            "the tree at {} is not a git working tree",
            display_path(&root)
        );
        assert_refusal(&root, &expected);
    }
    set_mode(&root, 0o755);
}
