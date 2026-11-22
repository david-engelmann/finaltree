//! The pins of slice 21, the unread metadata subjects, over
//! synthetic fixture repos the suite builds with git. The
//! extended attribute pin sets its attribute through a
//! python3 helper, the Rust standard library carrying no
//! extended attribute operation and the crate carrying no
//! dependency it could borrow.

mod common;

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use common::{make_factory_repo, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::stamping::{compare_fingerprints, run_and_stamp, verify_stamped_log};
use finaltree::Fingerprint;

/// A repo holding the factory files committed, one untracked
/// file of its own, and one untracked directory holding one
/// file, the shape the slice its cells stand on.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("metadata", area);
    let root = scratch.0.join("repo");
    make_factory_repo(&root);
    write_file(&root, "u.txt", "gamma\n");
    write_file(&root, "d/f.txt", "delta\n");
    (scratch, root)
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

fn set_modified_time(path: &Path, time: SystemTime) {
    let file = std::fs::File::open(path).expect("the path opens for its time standing");
    file.set_modified(time)
        .expect("the modification time stands");
}

fn modified_time(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .expect("the path stands")
        .modified()
        .expect("the modification time reads")
}

fn fixed_time(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

/// Sets the named user attribute on the path through a
/// python3 invocation and reads the value back the same way,
/// so a machine where the attribute cannot stand fails the
/// pin loudly at this seat and never passes it in silence.
fn set_attribute(path: &Path, value: &str) {
    let script = format!(
        "import os, sys; os.setxattr(sys.argv[1], 'user.finaltree_probe', sys.argv[2].encode())"
    );
    let output = Command::new("python3")
        .args(["-c", &script])
        .arg(path)
        .arg(value)
        .output()
        .expect("python3 runs");
    assert!(output.status.success(), "the attribute stands set");
    assert_eq!(attribute_value(path), value, "the attribute reads back");
}

fn attribute_value(path: &Path) -> String {
    let script = "import os, sys; sys.stdout.write(os.getxattr(sys.argv[1], 'user.finaltree_probe').decode())".to_string();
    let output = Command::new("python3")
        .args(["-c", &script])
        .arg(path)
        .output()
        .expect("python3 runs");
    assert!(output.status.success(), "the attribute reads");
    String::from_utf8(output.stdout).expect("the attribute value is text")
}

fn inode_of(path: &Path) -> u64 {
    std::fs::metadata(path).expect("the path stands").ino()
}

#[test]
fn test_a_change_in_a_files_modification_time_moves_nothing() {
    let (scratch, root) = make_repo("filemtime");
    let before = fingerprint(&root);
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0, "the stamped command stands run");
    let target = fixed_time(981_173_106);
    set_modified_time(&root.join("u.txt"), target);
    set_modified_time(&root.join("a.txt"), target);
    assert_eq!(modified_time(&root.join("u.txt")), target);
    assert_eq!(modified_time(&root.join("a.txt")), target);
    let after = fingerprint(&root);
    assert_eq!(after.digest(), before.digest(), "the digest stands unmoved");
    let differences = compare_fingerprints(&before, &after).expect("the comparison runs");
    assert!(
        differences.is_empty(),
        "no difference stands named, {differences:?}"
    );
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against the moved tree");
    assert!(result.differences.is_empty());
}

#[test]
fn test_a_change_in_a_directorys_modification_time_moves_nothing() {
    let (_scratch, root) = make_repo("dirmtime");
    let before = fingerprint(&root);
    let target = fixed_time(1_015_218_367);
    set_modified_time(&root.join("d"), target);
    set_modified_time(&root, target);
    assert_eq!(modified_time(&root.join("d")), target);
    assert_eq!(modified_time(&root), target);
    let after = fingerprint(&root);
    assert_eq!(after.digest(), before.digest(), "the digest stands unmoved");
    let differences = compare_fingerprints(&before, &after).expect("the comparison runs");
    assert!(
        differences.is_empty(),
        "no difference stands named, {differences:?}"
    );
}

#[test]
fn test_an_extended_attribute_on_a_file_moves_nothing() {
    let (_scratch, root) = make_repo("xattr");
    let before = fingerprint(&root);
    set_attribute(&root.join("u.txt"), "one");
    set_attribute(&root.join("d"), "two");
    let after = fingerprint(&root);
    assert_eq!(after.digest(), before.digest(), "the digest stands unmoved");
    let differences = compare_fingerprints(&before, &after).expect("the comparison runs");
    assert!(
        differences.is_empty(),
        "no difference stands named, {differences:?}"
    );
}

#[test]
fn test_a_file_replaced_at_a_new_inode_moves_nothing() {
    let (_scratch, root) = make_repo("inode");
    let before = fingerprint(&root);
    let untracked = root.join("u.txt");
    let tracked = root.join("a.txt");
    let untracked_inode = inode_of(&untracked);
    let tracked_inode = inode_of(&tracked);
    for (path, name, text) in [
        (&untracked, "u.txt.new", "gamma\n"),
        (&tracked, "a.txt.new", "alpha\n"),
    ] {
        let standing = root.join(name);
        std::fs::write(&standing, text).expect("the replacement stands written");
        let permissions = std::fs::metadata(path)
            .expect("the file stands")
            .permissions();
        std::fs::set_permissions(&standing, permissions).expect("the mode stands carried");
        std::fs::rename(&standing, path).expect("the replacement stands in place");
    }
    assert_ne!(inode_of(&untracked), untracked_inode, "a new inode stands");
    assert_ne!(inode_of(&tracked), tracked_inode, "a new inode stands");
    let after = fingerprint(&root);
    assert_eq!(after.digest(), before.digest(), "the digest stands unmoved");
    let differences = compare_fingerprints(&before, &after).expect("the comparison runs");
    assert!(
        differences.is_empty(),
        "no difference stands named, {differences:?}"
    );
}

#[test]
fn test_a_recreated_tree_fingerprints_as_the_tree_it_copies() {
    let (scratch, root) = make_repo("recreated");
    let before = fingerprint(&root);
    let copy_root = scratch.0.join("copy");
    make_factory_repo(&copy_root);
    write_file(&copy_root, "u.txt", "gamma\n");
    write_file(&copy_root, "d/f.txt", "delta\n");
    assert_ne!(
        inode_of(&copy_root.join("u.txt")),
        inode_of(&root.join("u.txt")),
        "every inode of the copy stands new"
    );
    assert_ne!(
        modified_time(&copy_root.join("u.txt")),
        modified_time(&root.join("u.txt")),
        "the copy stands at its own times"
    );
    let after = fingerprint(&copy_root);
    assert_eq!(
        after.digest(),
        before.digest(),
        "the digest stands identical"
    );
    let differences = compare_fingerprints(&before, &after).expect("the comparison runs");
    assert!(
        differences.is_empty(),
        "no difference stands named, {differences:?}"
    );
}
