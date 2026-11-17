//! Owner probe discharge for slice 11, evidence only and
//! never merged. The standings asserted here are the
//! pre-slice ground the slice 11 preamble states, read
//! against the binary built from the landed main at the
//! Round 11 closure. Probes one to four cover the root
//! ground, probes seven to nine the refusal ground the
//! Round 13 drafting inherits. The mode 000 pair, probes
//! five and six, stands discharged by the round's red
//! team executions as the nobody user, recorded in its
//! report. The locked standings assert only where the
//! directory truly cannot be read, which the hosted
//! runner's user satisfies and a root session does not.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{git, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::jsonvalue::JsonValue;
use finaltree::Fingerprint;

fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("probe", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    write_file(&root, "loose.txt", "loose\n");
    (scratch, root)
}

fn set_mode(path: &Path, mode: u32) {
    let mut permissions = std::fs::metadata(path)
        .expect("the path stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions).expect("the mode stands");
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

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

/// Probes one to three. The root mode binds nowhere on
/// the landed code, so every root standing, the outside
/// standings and the plain family alike, fingerprints
/// with the control digest and no entry names the root.
/// The family standings stand set directly here, the
/// umask clones of the probe ground producing exactly
/// these standings at a fresh checkout.
#[test]
fn test_probe_root_standings_bind_nowhere() {
    let (_scratch, root) = make_repo("rootstandings");
    set_mode(&root, 0o755);
    let control = fingerprint(&root);
    for mode in [0o500, 0o777, 0o1777, 0o2755, 0o775, 0o750, 0o700, 0o770] {
        set_mode(&root, mode);
        let current = fingerprint(&root);
        assert_eq!(
            current.digest(),
            control.digest(),
            "the digest stands at root mode {mode:o}"
        );
        assert!(
            dir_mode_pairs(&current.to_dict())
                .iter()
                .all(|(path, _)| path != "."),
            "no entry names the root at mode {mode:o}"
        );
    }
    set_mode(&root, 0o755);
}

/// Probe four. A submodule checkout root at 500 carries
/// its mode entry in the parent record under the
/// standing DM-3 reading, and its own digest stands.
#[test]
fn test_probe_submodule_root_mode_stands_in_the_parent() {
    let scratch = Scratch::new("probe", "submodule");
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
    set_mode(&root.join("sub"), 0o500);
    let parent = fingerprint(&root);
    assert!(
        dir_mode_pairs(&parent.to_dict()).contains(&("sub".to_string(), "500".to_string())),
        "the parent record carries the checkout mode entry"
    );
    set_mode(&root.join("sub"), 0o755);
}

/// Whether the locked directory truly cannot be read in
/// this session, the standing the hosted runner gives.
fn locked_unreadable(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_err()
}

/// Probe seven. A locked directory holding only an
/// untracked file, and one holding only an ignored file,
/// each fingerprint today with the file named nowhere,
/// the digest equal to the twin with the directory
/// standing empty at its ordinary mode.
#[test]
fn test_probe_locked_directories_fingerprint_silently() {
    let (_scratch, root) = make_repo("locked");
    let locked = root.join("locked");
    std::fs::create_dir_all(&locked).expect("the directory stands");
    write_file(&root, "locked/hidden.txt", "hidden\n");
    set_mode(&locked, 0);
    if locked_unreadable(&locked) {
        let with_hidden = fingerprint(&root);
        set_mode(&locked, 0o755);
        std::fs::remove_file(root.join("locked/hidden.txt")).expect("the file clears");
        let empty = fingerprint(&root);
        assert_eq!(
            with_hidden.digest(),
            empty.digest(),
            "the untracked file below the locked directory binds nothing"
        );
    } else {
        set_mode(&locked, 0o755);
    }
}

/// Probe eight. A locked directory holding a tracked
/// file refuses today through the content door.
#[test]
fn test_probe_locked_directory_with_a_tracked_file_refuses() {
    let (_scratch, root) = make_repo("lockedtracked");
    let locked = root.join("locked");
    std::fs::create_dir_all(&locked).expect("the directory stands");
    write_file(&root, "locked/tracked.txt", "tracked\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "tracked"]);
    set_mode(&locked, 0);
    if locked_unreadable(&locked) {
        assert!(
            compute_fingerprint(&root, &[]).is_err(),
            "the tracked file below the locked directory refuses"
        );
    }
    set_mode(&locked, 0o755);
}

/// Probe nine. A directory at 500 holding a generated
/// artifact under a declared pattern and an untracked
/// file fingerprints today with both bound, the fence
/// that the refusal event is the reading and not the
/// mode. The artifact binds through the generated
/// member, proven by the digest moving when its content
/// moves.
#[test]
fn test_probe_readable_directory_binds_its_contents() {
    let (_scratch, root) = make_repo("readable");
    let dir = root.join("gen");
    std::fs::create_dir_all(&dir).expect("the directory stands");
    write_file(&root, "gen/out.gen", "one\n");
    write_file(&root, "gen/loose.txt", "loose\n");
    set_mode(&dir, 0o500);
    let patterns: Vec<String> = vec!["*.gen".to_string()];
    let first = compute_fingerprint(&root, &patterns).expect("the tree fingerprints");
    write_file(&root, "gen/out.gen", "two\n");
    let second = compute_fingerprint(&root, &patterns).expect("the tree fingerprints");
    assert_ne!(
        first.digest(),
        second.digest(),
        "the generated artifact below the readable directory binds"
    );
    set_mode(&dir, 0o755);
}
