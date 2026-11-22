//! The pins of slice 4, file modes, over synthetic fixture
//! repos the suite builds with git. The submodule fixture
//! stands as a local repository joined with the file
//! protocol standing allowed in the fixture alone.

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
    let scratch = Scratch::new("modes", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

/// Stand a file executable or plain by its mode bits.
fn set_mode(root: &Path, relative: &str, mode: u32) {
    let full = root.join(relative);
    let mut permissions = std::fs::metadata(&full)
        .expect("the file stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(&full, permissions).expect("the mode stands");
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

fn fingerprint_generated(root: &Path, patterns: &[&str]) -> Fingerprint {
    let patterns: Vec<String> = patterns.iter().map(|pattern| pattern.to_string()).collect();
    compute_fingerprint(root, &patterns).expect("the tree fingerprints")
}

/// The modes member of a record as (path, mode) text pairs.
fn mode_pairs(record: &JsonValue) -> Vec<(String, String)> {
    let Some(member) = record.get("modes") else {
        return Vec::new();
    };
    let JsonValue::List(items) = member else {
        panic!("the modes member is a list");
    };
    items
        .iter()
        .map(|entry| {
            let JsonValue::Text(path) = entry.get("path").expect("the entry stands") else {
                panic!("the path is text");
            };
            let JsonValue::Text(mode) = entry.get("mode").expect("the entry stands") else {
                panic!("the mode is text");
            };
            (path.clone(), mode.clone())
        })
        .collect()
}

#[test]
fn test_the_untracked_mode_standings_fingerprint_apart() {
    let (_scratch, root) = make_repo("untracked");
    write_file(&root, "helper.sh", "echo helper\n");
    set_mode(&root, "helper.sh", 0o644);
    let plain = fingerprint(&root);
    assert!(
        plain.to_dict().get("modes").is_none(),
        "a plain untracked file carries no modes member"
    );
    set_mode(&root, "helper.sh", 0o755);
    let executable = fingerprint(&root);
    assert_ne!(
        plain.digest(),
        executable.digest(),
        "the executable standing fingerprints apart from the plain"
    );
    assert_eq!(
        mode_pairs(&executable.to_dict()),
        vec![("helper.sh".to_string(), "100755".to_string())]
    );
    set_mode(&root, "helper.sh", 0o644);
    assert_eq!(
        fingerprint(&root).digest(),
        plain.digest(),
        "the plain standing restored fingerprints as it stood"
    );
}

#[test]
fn test_a_generated_artifacts_mode_binds_its_fingerprint() {
    let (_scratch, root) = make_repo("generated");
    write_file(&root, "out.bin", "artifact\n");
    set_mode(&root, "out.bin", 0o644);
    let plain = fingerprint_generated(&root, &["out.bin"]);
    assert!(plain.to_dict().get("modes").is_none());
    set_mode(&root, "out.bin", 0o755);
    let executable = fingerprint_generated(&root, &["out.bin"]);
    assert_ne!(plain.digest(), executable.digest());
    assert_eq!(
        mode_pairs(&executable.to_dict()),
        vec![("out.bin".to_string(), "100755".to_string())]
    );
}

#[test]
fn test_a_tree_with_only_plain_modes_fingerprints_as_before() {
    let (_scratch, root) = make_repo("plain");
    write_file(&root, "notes.txt", "notes\n");
    write_file(&root, "out.bin", "artifact\n");
    set_mode(&root, "notes.txt", 0o644);
    set_mode(&root, "out.bin", 0o644);
    let record = fingerprint_generated(&root, &["out.bin"]).to_dict();
    assert!(
        record.get("modes").is_none(),
        "a tree with only plain modes carries no modes member"
    );
}

#[test]
fn test_an_untracked_symlink_stands_in_the_modes_member() {
    let (_scratch, root) = make_repo("symlink");
    write_file(&root, "helper.sh", "echo helper\n");
    set_mode(&root, "helper.sh", 0o644);
    let as_file = fingerprint(&root);
    std::os::unix::fs::symlink("own.txt", root.join("link.sh")).expect("the link stands");
    let with_link = fingerprint(&root);
    assert_eq!(
        mode_pairs(&with_link.to_dict()),
        vec![("link.sh".to_string(), "120000".to_string())]
    );
    assert_ne!(as_file.digest(), with_link.digest());
    // A regular file carrying the very text the link names
    // stands apart from the link itself.
    std::fs::remove_file(root.join("link.sh")).expect("the link falls");
    write_file(&root, "link.sh", "own.txt");
    let as_text = fingerprint(&root);
    assert!(as_text.to_dict().get("modes").is_none());
    assert_ne!(with_link.digest(), as_text.digest());
}

#[test]
fn test_a_mode_difference_names_a_file_mode_difference() {
    let (_scratch, root) = make_repo("difference");
    write_file(&root, "helper.sh", "echo helper\n");
    set_mode(&root, "helper.sh", 0o644);
    let expected = fingerprint(&root);
    set_mode(&root, "helper.sh", 0o755);
    let actual = fingerprint(&root);
    let differences = compare_fingerprints(&expected, &actual).expect("the comparison runs");
    assert_eq!(differences, vec!["file mode changed helper.sh".to_string()]);
}

#[test]
fn test_a_record_carrying_the_same_mode_path_twice_is_refused() {
    let (_scratch, root) = make_repo("duplicate");
    write_file(&root, "helper.sh", "echo helper\n");
    set_mode(&root, "helper.sh", 0o755);
    let record = fingerprint(&root).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "modes" {
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
fn test_a_stamped_log_verifies_against_the_modes_it_stamped() {
    let (scratch, root) = make_repo("stamped");
    write_file(&root, "helper.sh", "echo helper\n");
    set_mode(&root, "helper.sh", 0o755);
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    set_mode(&root, "helper.sh", 0o644);
    let moved = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(!moved.matched, "the moved mode no longer verifies");
    assert_eq!(
        moved.differences,
        vec!["file mode changed helper.sh".to_string()]
    );
}

/// A source repo holding f.txt, committed once.
fn make_source_repo(root: &Path) {
    std::fs::create_dir_all(root).expect("the source root stands");
    git(root, &["init", "-q", "-b", "main", "."]);
    write_file(root, "f.txt", "substance\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial"]);
}

/// A repo holding one file of its own and the source repo
/// standing as its submodule at the path sub.
fn make_submodule_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("modes", area);
    let source = scratch.0.join("source");
    let main = scratch.0.join("main");
    make_source_repo(&source);
    std::fs::create_dir_all(&main).expect("the main root stands");
    git(&main, &["init", "-q", "-b", "main", "."]);
    write_file(&main, "own.txt", "own\n");
    git(&main, &["add", "."]);
    git(&main, &["commit", "-q", "-m", "initial"]);
    let source_text = source.to_string_lossy().into_owned();
    git(
        &main,
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
    git(&main, &["commit", "-q", "-m", "add the submodule"]);
    (scratch, main)
}

#[test]
fn test_a_submodules_untracked_mode_binds_through_its_parent() {
    let (_scratch, main) = make_submodule_repo("submodule-mode");
    let sub = main.join("sub");
    write_file(&sub, "tool.sh", "echo tool\n");
    set_mode(&sub, "tool.sh", 0o644);
    let plain = fingerprint(&main);
    set_mode(&sub, "tool.sh", 0o755);
    let executable = fingerprint(&main);
    assert_ne!(
        plain.digest(),
        executable.digest(),
        "the mode below the gitlink binds through the parent digest"
    );
}

/// A repo holding one committed file of its own and an
/// ignore file setting the build directory aside.
fn make_ignored_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("modes", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    write_file(&root, ".gitignore", "build/\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

/// Strike the modes member out of a record its dict form,
/// the recorded digest left standing where it stands.
fn strike_modes(record: &Fingerprint) -> JsonValue {
    let JsonValue::Object(pairs) = record.to_dict() else {
        panic!("the record is an object");
    };
    JsonValue::Object(
        pairs
            .into_iter()
            .filter(|(key, _)| key != "modes")
            .collect(),
    )
}

/// Carry one orphan entry on a record and prove its whole
/// standing, the reading back, the binding of the digest,
/// the silence of the comparison in both directions, and
/// the refusal of the record the entry stands struck from.
fn prove_orphan_entry(base: &Fingerprint, path: &str, mode: &str) {
    let mut carried = base.clone();
    carried.modes.push(finaltree::fingerprint::ModeEntry {
        path: JsonValue::text(path),
        mode: JsonValue::text(mode),
    });
    let read = Fingerprint::from_dict(&carried.to_dict()).expect("the carried record reads back");
    assert_eq!(
        mode_pairs(&read.to_dict()),
        vec![(path.to_string(), mode.to_string())]
    );
    assert_ne!(
        read.digest(),
        base.digest(),
        "the orphan entry binds the digest of the record carrying it"
    );
    let forward = compare_fingerprints(&read, base).expect("the comparison runs");
    assert_eq!(forward, Vec::<String>::new());
    let backward = compare_fingerprints(base, &read).expect("the comparison runs");
    assert_eq!(backward, Vec::<String>::new());
    let error =
        Fingerprint::from_dict(&strike_modes(&carried)).expect_err("the struck record refuses");
    assert_eq!(
        error.to_string(),
        "the recorded digest disagrees with the recorded members"
    );
}

#[test]
fn test_an_ignored_files_mode_difference_names_no_difference() {
    let (_scratch, root) = make_ignored_repo("ignored-parting");
    write_file(&root, "build/run.sh", "echo run\n");
    set_mode(&root, "build/run.sh", 0o755);
    let executable = fingerprint(&root);
    assert_eq!(
        mode_pairs(&executable.to_dict()),
        vec![("build/run.sh".to_string(), "100755".to_string())]
    );
    set_mode(&root, "build/run.sh", 0o644);
    let plain = fingerprint(&root);
    assert!(
        plain.to_dict().get("modes").is_none(),
        "the plain ignored file carries no modes member"
    );
    assert_ne!(
        executable.digest(),
        plain.digest(),
        "the ignored mode binds the digest it stands under"
    );
    let forward = compare_fingerprints(&executable, &plain).expect("the comparison runs");
    assert_eq!(forward, Vec::<String>::new());
    let backward = compare_fingerprints(&plain, &executable).expect("the comparison runs");
    assert_eq!(backward, Vec::<String>::new());
    std::os::unix::fs::symlink("own.txt", root.join("build/link.sh")).expect("the link stands");
    let linked = fingerprint(&root);
    assert_eq!(
        mode_pairs(&linked.to_dict()),
        vec![("build/link.sh".to_string(), "120000".to_string())]
    );
    std::fs::remove_file(root.join("build/link.sh")).expect("the link falls");
    write_file(&root, "build/link.sh", "own.txt");
    set_mode(&root, "build/link.sh", 0o755);
    let as_text = fingerprint(&root);
    assert_eq!(
        mode_pairs(&as_text.to_dict()),
        vec![("build/link.sh".to_string(), "100755".to_string())]
    );
    assert_ne!(linked.digest(), as_text.digest());
    let forward = compare_fingerprints(&linked, &as_text).expect("the comparison runs");
    assert_eq!(forward, Vec::<String>::new());
    let backward = compare_fingerprints(&as_text, &linked).expect("the comparison runs");
    assert_eq!(backward, Vec::<String>::new());
}

#[test]
fn test_a_stamped_log_verifies_across_an_ignored_files_mode_change() {
    let (scratch, root) = make_ignored_repo("ignored-stamped");
    write_file(&root, "build/run.sh", "echo run\n");
    set_mode(&root, "build/run.sh", 0o755);
    let stamped = fingerprint(&root);
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
    set_mode(&root, "build/run.sh", 0o644);
    let moved = fingerprint(&root);
    assert_ne!(
        stamped.digest(),
        moved.digest(),
        "the moved ignored mode parts the digest"
    );
    let across = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(
        across.matched,
        "the verdict stands matched across the ignored mode change"
    );
    assert!(across.differences.is_empty());
}

#[test]
fn test_a_mode_entry_naming_a_path_outside_the_compared_members_names_no_difference() {
    let (_scratch, root) = make_repo("orphan-tracked");
    let base = fingerprint(&root);
    prove_orphan_entry(&base, "own.txt", "100755");
    prove_orphan_entry(&base, "ghost.txt", "120000");
    let (_scratch, root) = make_ignored_repo("orphan-ignored");
    write_file(&root, "build/out.bin", "artifact one\n");
    set_mode(&root, "build/out.bin", 0o644);
    let ignored_base = fingerprint(&root);
    assert!(
        ignored_base.to_dict().get("modes").is_none(),
        "the plain ignored file carries no modes member"
    );
    prove_orphan_entry(&ignored_base, "build/out.bin", "100755");
}

#[test]
fn test_a_file_moving_between_the_untracked_and_generated_members_names_its_mode_difference() {
    let (_scratch, root) = make_repo("movement");
    write_file(&root, "gen.bin", "artifact\n");
    set_mode(&root, "gen.bin", 0o644);
    let untracked_side = fingerprint(&root);
    set_mode(&root, "gen.bin", 0o755);
    let generated_side = fingerprint_generated(&root, &["gen.bin"]);
    let forward =
        compare_fingerprints(&untracked_side, &generated_side).expect("the comparison runs");
    assert_eq!(
        forward,
        vec![
            "dirty file cleaned gen.bin (untracked)".to_string(),
            "generated artifact added gen.bin".to_string(),
            "file mode changed gen.bin".to_string(),
        ]
    );
    let backward =
        compare_fingerprints(&generated_side, &untracked_side).expect("the comparison runs");
    assert_eq!(
        backward,
        vec![
            "dirty file added gen.bin (untracked)".to_string(),
            "generated artifact removed gen.bin".to_string(),
            "file mode changed gen.bin".to_string(),
        ]
    );
}

#[test]
fn test_an_ignored_file_standing_on_one_side_names_no_mode_difference() {
    let (_scratch, root) = make_ignored_repo("one-side");
    let bare = fingerprint(&root);
    write_file(&root, "build/out.bin", "artifact one\n");
    set_mode(&root, "build/out.bin", 0o755);
    let standing = fingerprint(&root);
    assert_eq!(
        mode_pairs(&standing.to_dict()),
        vec![("build/out.bin".to_string(), "100755".to_string())]
    );
    let removed = compare_fingerprints(&standing, &bare).expect("the comparison runs");
    assert_eq!(
        removed,
        vec!["ignored file removed build/out.bin".to_string()]
    );
    let added = compare_fingerprints(&bare, &standing).expect("the comparison runs");
    assert_eq!(added, vec!["ignored file added build/out.bin".to_string()]);
}
