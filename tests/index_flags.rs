//! The pins of slice 22, the index flag standings, over
//! synthetic fixture repos the suite builds with git. A
//! flag stands set on an entry through update-index, the
//! reading the common git builder carries in one line per
//! fixture, and the standings stand read back through the
//! verbose listing before any fingerprint stands read.

mod common;

use std::path::{Path, PathBuf};

use common::{git, make_factory_repo, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::jsonvalue::JsonValue;
use finaltree::stamping::{compare_fingerprints, run_and_stamp, verify_stamped_log};
use finaltree::{Fingerprint, TrackedFile};

/// A repo holding the factory files committed, a.txt as
/// alpha and b.txt as beta, the shape the slice its cells
/// stand on.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("index_flags", area);
    let root = scratch.0.join("repo");
    make_factory_repo(&root);
    (scratch, root)
}

fn read_record(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

fn tracked_entry<'a>(record: &'a Fingerprint, wanted: &str) -> &'a TrackedFile {
    record
        .tracked
        .iter()
        .find(|entry| entry.path.as_text() == Some(wanted))
        .expect("the tracked entry stands")
}

fn dirty_pairs(record: &Fingerprint) -> Vec<(String, String)> {
    record
        .dirty
        .iter()
        .map(|entry| {
            (
                entry.path.as_text().expect("the path is text").to_string(),
                entry.kind.as_text().expect("the kind is text").to_string(),
            )
        })
        .collect()
}

/// Sets the named flag on the entry through update-index
/// and reads the verbose listing back, so a machine where
/// the flag cannot stand fails the pin loudly at this seat
/// and never passes it in silence. The letter stands as
/// the listing prints it, S for the skip worktree standing
/// and the lowercase h for the assume unchanged standing.
fn set_flag(root: &Path, flag: &str, path: &str, letter: char) {
    git(root, &["update-index", flag, path]);
    let output = common::pinned(&mut std::process::Command::new("git"))
        .args(["ls-files", "-v"])
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "the verbose listing stands");
    let listing = String::from_utf8(output.stdout).expect("the listing is text");
    let line = listing
        .lines()
        .find(|standing| standing.ends_with(path))
        .expect("the entry stands listed");
    assert_eq!(
        line.chars().next(),
        Some(letter),
        "the flag letter stands at the entry"
    );
}

fn modified_pairs() -> Vec<(String, String)> {
    vec![("a.txt".to_string(), "modified".to_string())]
}

#[test]
fn test_a_skip_worktree_files_modification_names_no_dirty_entry() {
    let (_scratch, root) = make_repo("swmod");
    write_file(&root, "a.txt", "alpha changed\n");
    let plain = read_record(&root);
    assert_eq!(dirty_pairs(&plain), modified_pairs());
    set_flag(&root, "--skip-worktree", "a.txt", 'S');
    let flagged = read_record(&root);
    assert!(
        flagged.dirty.is_empty(),
        "no dirty entry stands, {:?}",
        flagged.dirty
    );
    let entry = tracked_entry(&flagged, "a.txt");
    assert_eq!(entry, tracked_entry(&plain, "a.txt"), "the content binds");
    assert_ne!(
        entry.blob, entry.index_blobs[0],
        "the working blob parts from the index blob"
    );
    assert_ne!(flagged.digest(), plain.digest(), "the records part");
    let differences = compare_fingerprints(&plain, &flagged).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["dirty file cleaned a.txt (modified)".to_string()],
        "the records part in the dirty member alone"
    );
}

#[test]
fn test_an_assume_unchanged_files_modification_names_no_dirty_entry() {
    let (_scratch, root) = make_repo("aumod");
    write_file(&root, "a.txt", "alpha changed\n");
    let plain = read_record(&root);
    assert_eq!(dirty_pairs(&plain), modified_pairs());
    set_flag(&root, "--assume-unchanged", "a.txt", 'h');
    let flagged = read_record(&root);
    assert!(
        flagged.dirty.is_empty(),
        "no dirty entry stands, {:?}",
        flagged.dirty
    );
    let entry = tracked_entry(&flagged, "a.txt");
    assert_eq!(entry, tracked_entry(&plain, "a.txt"), "the content binds");
    assert_ne!(
        entry.blob, entry.index_blobs[0],
        "the working blob parts from the index blob"
    );
    let (_other, twin) = make_repo("aumodtwin");
    write_file(&twin, "a.txt", "alpha changed\n");
    set_flag(&twin, "--skip-worktree", "a.txt", 'S');
    let skip_record = read_record(&twin);
    assert_eq!(
        flagged, skip_record,
        "the two flags stand as one standing whole"
    );
    assert_eq!(flagged.digest(), skip_record.digest());
}

#[test]
fn test_a_flagged_files_deletion_names_no_dirty_entry() {
    let (_scratch, root) = make_repo("plaindel");
    std::fs::remove_file(root.join("b.txt")).expect("the file stands deleted");
    let plain = read_record(&root);
    assert_eq!(
        dirty_pairs(&plain),
        vec![("b.txt".to_string(), "modified".to_string())]
    );
    assert!(matches!(
        tracked_entry(&plain, "b.txt").blob,
        JsonValue::Null
    ));
    let mut records = Vec::new();
    for (area, flag, letter) in [
        ("swdel", "--skip-worktree", 'S'),
        ("audel", "--assume-unchanged", 'h'),
    ] {
        let (_held, flagged_root) = make_repo(area);
        set_flag(&flagged_root, flag, "b.txt", letter);
        std::fs::remove_file(flagged_root.join("b.txt")).expect("the file stands deleted");
        let record = read_record(&flagged_root);
        assert!(
            record.dirty.is_empty(),
            "no dirty entry stands, {:?}",
            record.dirty
        );
        let entry = tracked_entry(&record, "b.txt");
        assert!(
            matches!(entry.blob, JsonValue::Null),
            "the tracked blob stands null"
        );
        assert_eq!(
            entry,
            tracked_entry(&plain, "b.txt"),
            "the deletion binds as the plain deletion binds"
        );
        let differences = compare_fingerprints(&plain, &record).expect("the comparison runs");
        assert_eq!(
            differences,
            vec!["dirty file cleaned b.txt (modified)".to_string()],
            "the records part in the dirty member alone"
        );
        records.push(record);
        drop(_held);
    }
    assert_eq!(
        records[0], records[1],
        "the two flagged deletions stand as one record"
    );
}

#[test]
fn test_a_flag_on_an_unmodified_file_moves_nothing() {
    let (_scratch, root) = make_repo("baseclean");
    let base = read_record(&root);
    for (area, flag, letter) in [
        ("swclean", "--skip-worktree", 'S'),
        ("auclean", "--assume-unchanged", 'h'),
    ] {
        let (_held, flagged_root) = make_repo(area);
        set_flag(&flagged_root, flag, "a.txt", letter);
        let record = read_record(&flagged_root);
        assert_eq!(record, base, "the record stands identical whole");
        assert_eq!(record.digest(), base.digest(), "the digest stands unmoved");
        drop(_held);
    }
}

#[test]
fn test_the_two_flag_standings_verify_as_one_standing() {
    let (scratch, root) = make_repo("verify");
    write_file(&root, "a.txt", "alpha changed\n");
    let plain = read_record(&root);
    set_flag(&root, "--skip-worktree", "a.txt", 'S');
    let flagged = read_record(&root);
    let differences = compare_fingerprints(&plain, &flagged).expect("the comparison runs");
    assert_eq!(
        differences,
        vec!["dirty file cleaned a.txt (modified)".to_string()],
        "the plain parting names the one dirty difference"
    );
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0, "the stamped command stands run");
    git(&root, &["update-index", "--no-skip-worktree", "a.txt"]);
    set_flag(&root, "--assume-unchanged", "a.txt", 'h');
    let twin = read_record(&root);
    assert_eq!(twin, flagged, "the flagged standings stand identical");
    let matched = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(
        matched.matched,
        "the log verifies against the twin standing"
    );
    assert!(matched.differences.is_empty());
    git(&root, &["update-index", "--no-assume-unchanged", "a.txt"]);
    let unflagged = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!unflagged.matched, "the plain standing does not verify");
    assert_eq!(
        unflagged.differences,
        vec!["dirty file added a.txt (modified)".to_string()],
        "the plain parting names the one dirty difference"
    );
}

#[test]
fn test_an_intent_to_add_file_binds_at_its_empty_index_blob() {
    let (_scratch, root) = make_repo("ita");
    write_file(&root, "c.txt", "gamma\n");
    git(&root, &["add", "-N", "c.txt"]);
    let record = read_record(&root);
    let entry = tracked_entry(&record, "c.txt");
    assert_eq!(
        entry.index_blobs,
        vec![JsonValue::Text(
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".to_string()
        )],
        "the index blob stands as the empty blob"
    );
    assert_ne!(
        entry.blob, entry.index_blobs[0],
        "the working blob carries the content"
    );
    assert_eq!(
        dirty_pairs(&record),
        vec![("c.txt".to_string(), "modified".to_string())],
        "the dirty entry stands at the modified kind"
    );
    let (_other, staged_root) = make_repo("stagednew");
    write_file(&staged_root, "c.txt", "gamma\n");
    git(&staged_root, &["add", "c.txt"]);
    let staged = read_record(&staged_root);
    let staged_entry = tracked_entry(&staged, "c.txt");
    assert_eq!(
        staged_entry.index_blobs,
        vec![staged_entry.blob.clone()],
        "the staged standing carries the content blob in the index"
    );
    assert_eq!(
        dirty_pairs(&staged),
        vec![("c.txt".to_string(), "staged".to_string())],
        "the staged standing binds at the staged kind"
    );
    assert_ne!(record, staged, "the standings part at both seats");
}
