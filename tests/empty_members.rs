//! The pins of slice 13, the explicitly empty optional
//! member, over synthetic fixture repos the suite builds
//! with git. Every refusal pin of the member family
//! doctors a genuine record of its fixture tree, standing
//! one optional member present with an empty list and
//! recomputing the digest over the payload as written,
//! so the record stands accepted on the tree before this
//! slice and the refusal on the built tree is the refusal
//! of EM-2 alone (EM-6). No pin in the family depends on
//! permission bits or on the running user.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{git, write_file, Scratch};
use finaltree::fingerprint::compute_fingerprint;
use finaltree::jsonvalue::JsonValue;
use finaltree::render::render_compact;
use finaltree::sha256::sha256_hex;
use finaltree::stamping::{run_and_stamp, verify_stamped_log};
use finaltree::Fingerprint;

/// A repo holding one committed file of its own, whose
/// record carries no optional member standing.
fn make_repo(area: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("emptymembers", area);
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(&root).expect("the root stands");
    git(&root, &["init", "-q", "-b", "main", "."]);
    write_file(&root, "own.txt", "own\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    (scratch, root)
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

fn set_file_mode(path: &Path, mode: u32) {
    let mut permissions = std::fs::metadata(path)
        .expect("the path stands")
        .permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions).expect("the mode stands");
}

/// Doctor a genuine record by standing one optional
/// member present with an empty list, the digest
/// recomputed over the payload as written, in the
/// fixture discipline EM-6 states. The payload the
/// digest stands over is the record with its digest and
/// its generated patterns set aside and the member
/// standing where the payload order stands it, behind
/// the untracked member, the rendering sorting the keys
/// as it sorts every payload.
fn record_with_empty_member(record: &JsonValue, member: &str) -> JsonValue {
    let JsonValue::Object(pairs) = record else {
        panic!("the record is an object");
    };
    let mut payload: Vec<(String, JsonValue)> = Vec::new();
    let mut patterns = JsonValue::List(Vec::new());
    for (key, value) in pairs {
        match key.as_str() {
            "digest" => {}
            "generated_patterns" => patterns = value.clone(),
            _ => payload.push((key.clone(), value.clone())),
        }
    }
    let untracked_at = payload
        .iter()
        .position(|(key, _)| key.as_str() == "untracked")
        .expect("the record carries the untracked member");
    payload.insert(
        untracked_at + 1,
        (member.to_string(), JsonValue::List(Vec::new())),
    );
    let digest = sha256_hex(render_compact(&JsonValue::Object(payload.clone())).as_bytes());
    let mut doctored = payload;
    doctored.push(("digest".to_string(), JsonValue::text(digest)));
    doctored.push(("generated_patterns".to_string(), patterns));
    JsonValue::Object(doctored)
}

/// The refusal of EM-2 asserted over the record of the
/// fixture tree carrying the named member standing
/// explicitly empty.
fn assert_empty_member_refused(member: &str, area: &str) {
    let (_scratch, root) = make_repo(area);
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get(member).is_none(),
        "the genuine record carries no {member} member"
    );
    let doctored = record_with_empty_member(&record, member);
    let error = Fingerprint::from_dict(&doctored).expect_err("the doctored record refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
}

#[test]
fn test_a_record_carrying_an_empty_submodules_member_is_refused() {
    assert_empty_member_refused("submodules", "submodules");
}

#[test]
fn test_a_record_carrying_an_empty_modes_member_is_refused() {
    assert_empty_member_refused("modes", "modes");
}

#[test]
fn test_a_record_carrying_an_empty_ignored_member_is_refused() {
    assert_empty_member_refused("ignored", "ignored");
}

#[test]
fn test_a_record_carrying_an_empty_empty_dirs_member_is_refused() {
    assert_empty_member_refused("empty_dirs", "emptydirs");
}

#[test]
fn test_a_record_carrying_an_empty_special_member_is_refused() {
    assert_empty_member_refused("special", "special");
}

#[test]
fn test_a_record_carrying_an_empty_dir_modes_member_is_refused() {
    assert_empty_member_refused("dir_modes", "dirmodes");
}

#[test]
fn test_a_record_carrying_an_empty_file_modes_member_is_refused() {
    assert_empty_member_refused("file_modes", "filemodes");
}

#[test]
fn test_a_record_carrying_an_empty_dot_git_member_is_refused() {
    assert_empty_member_refused("dot_git", "dotgit");
}

#[test]
fn test_the_verify_door_refuses_a_stamped_log_carrying_an_empty_member() {
    let (scratch, root) = make_repo("door");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&root, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let stamped = verify_stamped_log(&log, &root, None).expect("the verify runs");
    assert!(stamped.matched, "the log verifies against its own tree");
    let record = fingerprint(&root).to_dict();
    let doctored = record_with_empty_member(&record, "modes");
    let Some(JsonValue::Text(digest)) = doctored.get("digest") else {
        panic!("the doctored record carries its digest as a text");
    };
    let text = std::fs::read_to_string(&log).expect("the log stands");
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if line.starts_with("fingerprint-json ") {
            lines.push(format!("fingerprint-json {}", render_compact(&doctored)));
        } else if line.starts_with("fingerprint ") {
            lines.push(format!("fingerprint {digest}"));
        } else {
            lines.push(line.to_string());
        }
    }
    let doctored_log = scratch.0.join("doctored.log");
    std::fs::write(&doctored_log, lines.join("\n")).expect("the doctored log is written");
    let error = verify_stamped_log(&doctored_log, &root, None).expect_err("the verify refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
}

#[test]
fn test_a_record_carrying_no_optional_member_reads_and_reemits_whole() {
    let (_scratch, root) = make_repo("absent");
    let record = fingerprint(&root).to_dict();
    let restored = Fingerprint::from_dict(&record).expect("the record reads");
    assert!(restored.submodules.is_empty());
    assert!(restored.modes.is_empty());
    assert!(restored.ignored.is_empty());
    assert!(restored.empty_dirs.is_empty());
    assert!(restored.special.is_empty());
    assert!(restored.dir_modes.is_empty());
    assert!(restored.file_modes.is_empty());
    assert!(restored.dot_git.is_empty());
    assert_eq!(restored.to_dict(), record, "the record re-emits as written");
}

#[test]
fn test_a_record_carrying_members_standing_non_empty_reads_and_reemits_whole() {
    let (_scratch, root) = make_repo("standing");
    std::fs::create_dir_all(root.join("empty")).expect("the directory stands");
    write_file(&root, "tool.sh", "tool\n");
    set_file_mode(&root.join("tool.sh"), 0o755);
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("empty_dirs").is_some(),
        "the genuine record carries its empty_dirs member"
    );
    assert!(
        record.get("modes").is_some(),
        "the genuine record carries its modes member"
    );
    let restored = Fingerprint::from_dict(&record).expect("the record reads");
    assert_eq!(restored.empty_dirs, vec![JsonValue::text("empty")]);
    assert_eq!(restored.modes.len(), 1);
    assert_eq!(restored.to_dict(), record, "the record re-emits as written");
}
