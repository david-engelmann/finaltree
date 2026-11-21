//! The embedded member pins of slice 15. An embedded
//! repository, a directory below the root that is itself
//! the top level of a working tree while standing in no
//! gitlink, binds through the embedded member by its head
//! and by the digest of its own fingerprint, and its
//! interior leaves the members that gathered it whole
//! before this slice. The pins read the member through
//! the record rendering, so their standing on the code
//! before this slice fails at their assertions in the
//! designed shapes. The fixtures stand as plain
//! repositories initialised inside the tree, needing no
//! protocol allowance.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use finaltree::fingerprint::{compute_fingerprint, Fingerprint};
use finaltree::jsonvalue::JsonValue;

use common::{git, make_factory_repo, pinned, write_file, Scratch};

fn make_embedded_repo(root: &Path) {
    fs::create_dir_all(root).expect("the embedded root stands");
    git(root, &["init", "-q", "-b", "main", "."]);
    write_file(root, "f.txt", "embedded alpha\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "embedded initial"]);
}

fn make_tree_with_embedded(tag: &str) -> (Scratch, PathBuf) {
    let scratch = Scratch::new("embedded", tag);
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    make_embedded_repo(&main.join("emb"));
    (scratch, main)
}

fn git_text(root: &Path, args: &[&str]) -> String {
    let output = pinned(&mut Command::new("git"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} stands in the fixture"
    );
    String::from_utf8(output.stdout)
        .expect("the git output is utf8")
        .trim()
        .to_string()
}

/// The entries of the embedded member of a record, as the
/// path text, the head value, and the digest value of each
/// entry, standing empty where the member does not stand.
fn embedded_entries(record: &JsonValue) -> Vec<(String, JsonValue, JsonValue)> {
    let Some(JsonValue::List(items)) = record.get("embedded") else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let path = item
                .get("path")
                .and_then(JsonValue::as_text)
                .expect("the entry carries its path")
                .to_string();
            let head = item
                .get("head")
                .expect("the entry carries its head")
                .clone();
            let digest = item
                .get("digest")
                .expect("the entry carries its digest")
                .clone();
            (path, head, digest)
        })
        .collect()
}

/// The path texts a record member carries, standing empty
/// where the member does not stand.
fn member_paths(record: &JsonValue, member: &str) -> Vec<String> {
    let Some(JsonValue::List(items)) = record.get(member) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            item.get("path")
                .and_then(JsonValue::as_text)
                .expect("the entry carries its path")
                .to_string()
        })
        .collect()
}

#[test]
fn test_embedded_standings_fingerprint_apart() {
    let (_scratch, main) = make_tree_with_embedded("standings-apart");
    let emb = main.join("emb");
    let mut digests = Vec::new();
    let clean = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert!(
        !embedded_entries(&clean.to_dict()).is_empty(),
        "the member stands in the clean record"
    );
    digests.push(clean.digest());
    write_file(&emb, "f.txt", "embedded beta\n");
    let edited = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert!(
        !embedded_entries(&edited.to_dict()).is_empty(),
        "the member stands in the edited record"
    );
    digests.push(edited.digest());
    write_file(&emb, "f.txt", "embedded gamma\n");
    let edited_again = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    digests.push(edited_again.digest());
    write_file(&emb, "loose.txt", "loose\n");
    let with_loose = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    digests.push(with_loose.digest());
    for (index, digest) in digests.iter().enumerate() {
        for other in &digests[index + 1..] {
            assert_ne!(digest, other, "the standings fingerprint apart");
        }
    }
}

#[test]
fn test_an_embedded_entry_records_its_path_and_head_and_digest() {
    let (_scratch, main) = make_tree_with_embedded("entry-records");
    let emb = main.join("emb");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let entries = embedded_entries(&record.to_dict());
    assert_eq!(entries.len(), 1, "the member carries the one entry");
    let (path, head, digest) = &entries[0];
    assert_eq!(path, "emb", "the entry records its path");
    assert_eq!(
        head.as_text(),
        Some(git_text(&emb, &["rev-parse", "HEAD"]).as_str()),
        "the entry records the commit the repository stands at"
    );
    let own = compute_fingerprint(&emb, &[]).expect("the repository fingerprints");
    assert_eq!(
        digest.as_text(),
        Some(own.digest().as_str()),
        "the entry records the digest of the repository its own fingerprint"
    );
}

#[test]
fn test_a_commit_inside_an_embedded_repository_moves_its_head_entry() {
    let (_scratch, main) = make_tree_with_embedded("commit-inside");
    let emb = main.join("emb");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let before_entries = embedded_entries(&before.to_dict());
    assert_eq!(before_entries.len(), 1, "the member stands before");
    write_file(&emb, "g.txt", "second\n");
    git(&emb, &["add", "."]);
    git(&emb, &["commit", "-q", "-m", "second"]);
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let after_entries = embedded_entries(&after.to_dict());
    assert_eq!(after_entries.len(), 1, "the member stands after");
    assert_eq!(
        after_entries[0].1.as_text(),
        Some(git_text(&emb, &["rev-parse", "HEAD"]).as_str()),
        "the head entry records the new commit"
    );
    assert_ne!(
        before_entries[0].1, after_entries[0].1,
        "the head entry moves with the commit"
    );
    assert_ne!(
        before.digest(),
        after.digest(),
        "the record digest moves with the head entry"
    );
}

#[test]
fn test_an_embedded_repository_stands_in_neither_the_untracked_nor_the_dot_git_member() {
    let (_scratch, main) = make_tree_with_embedded("neither-member");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        !embedded_entries(&dict).is_empty(),
        "the member stands for the repository"
    );
    for member in ["untracked", "dot_git"] {
        for path in member_paths(&dict, member) {
            assert!(
                !path.starts_with("emb/"),
                "no path of {member} stands under the boundary, {path} stands"
            );
            assert_ne!(path, "emb", "no path of {member} names the boundary whole");
        }
    }
}

#[test]
fn test_an_embedded_repository_at_depth_two_binds_through_its_member() {
    let scratch = Scratch::new("embedded", "depth-two");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let deep = main.join("a").join("emb");
    make_embedded_repo(&deep);
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let entries = embedded_entries(&record.to_dict());
    assert_eq!(entries.len(), 1, "the member carries the one entry");
    let (path, head, digest) = &entries[0];
    assert_eq!(path, "a/emb", "the entry records its path at depth two");
    assert_eq!(
        head.as_text(),
        Some(git_text(&deep, &["rev-parse", "HEAD"]).as_str()),
        "the entry records the commit the repository stands at"
    );
    let own = compute_fingerprint(&deep, &[]).expect("the repository fingerprints");
    assert_eq!(
        digest.as_text(),
        Some(own.digest().as_str()),
        "the entry records the digest of the repository its own fingerprint"
    );
}

#[test]
fn test_the_separate_git_dir_form_binds_through_its_member() {
    let scratch = Scratch::new("embedded", "separate-git-dir");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let gitdir = scratch.0.join("elsewhere");
    let gitdir_text = gitdir.to_string_lossy().into_owned();
    git(
        &main,
        &[
            "init",
            "-q",
            "-b",
            "main",
            "--separate-git-dir",
            &gitdir_text,
            "emb",
        ],
    );
    let emb = main.join("emb");
    write_file(&emb, "f.txt", "separate alpha\n");
    git(&emb, &["add", "."]);
    git(&emb, &["commit", "-q", "-m", "separate initial"]);
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    let entries = embedded_entries(&dict);
    assert_eq!(entries.len(), 1, "the member carries the one entry");
    let (path, head, digest) = &entries[0];
    assert_eq!(path, "emb", "the entry records its path");
    assert_eq!(
        head.as_text(),
        Some(git_text(&emb, &["rev-parse", "HEAD"]).as_str()),
        "the entry records the commit the repository stands at"
    );
    let own = compute_fingerprint(&emb, &[]).expect("the repository fingerprints");
    assert_eq!(
        digest.as_text(),
        Some(own.digest().as_str()),
        "the entry records the digest of the repository its own fingerprint"
    );
    for path in member_paths(&dict, "dot_git") {
        assert!(
            !path.starts_with("emb/"),
            "no dot git path stands under the boundary, {path} stands"
        );
    }
}

#[test]
fn test_a_nested_embedded_repository_binds_through_its_parent() {
    let (_scratch, main) = make_tree_with_embedded("nested");
    let emb = main.join("emb");
    let inner = emb.join("inner");
    make_embedded_repo(&inner);
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let before_entries = embedded_entries(&before.to_dict());
    assert_eq!(
        before_entries.len(),
        1,
        "the outer member carries the parent alone"
    );
    assert_eq!(before_entries[0].0, "emb", "the entry names the parent");
    write_file(&inner, "f.txt", "inner beta\n");
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let after_entries = embedded_entries(&after.to_dict());
    assert_eq!(
        after_entries.len(),
        1,
        "the member still carries the parent alone"
    );
    assert_ne!(
        before_entries[0].2, after_entries[0].2,
        "the parent its digest binds the state of the nested repository"
    );
    assert_ne!(
        before.digest(),
        after.digest(),
        "the record digest moves with the nested state"
    );
}

#[test]
fn test_a_bare_repository_inside_the_tree_stands_as_ordinary_files() {
    let scratch = Scratch::new("embedded", "bare-form");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    git(&main, &["init", "-q", "--bare", "bare"]);
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        dict.get("embedded").is_none(),
        "the record carries no embedded member"
    );
    assert!(
        member_paths(&dict, "untracked")
            .iter()
            .any(|path| path == "bare/HEAD"),
        "the bare form its files stand as ordinary untracked files"
    );
}

#[test]
fn test_a_plain_directory_named_dot_git_binds_as_before() {
    let scratch = Scratch::new("embedded", "plain-dot-git");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    write_file(&main, "plain/.git/notes.txt", "notes\n");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        dict.get("embedded").is_none(),
        "the record carries no embedded member"
    );
    assert!(
        member_paths(&dict, "dot_git")
            .iter()
            .any(|path| path == "plain/.git/notes.txt"),
        "the plain directory named dot git binds through the dot git member"
    );
}

#[test]
fn test_a_tree_without_an_embedded_repository_fingerprints_as_before() {
    let scratch = Scratch::new("embedded", "no-embedded");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let first = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert!(
        first.to_dict().get("embedded").is_none(),
        "the record carries no embedded member"
    );
    let second = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert_eq!(
        first.digest(),
        second.digest(),
        "the digest stands unchanged across computations"
    );
    let restored = Fingerprint::from_dict(&first.to_dict()).expect("the record reads");
    assert_eq!(
        restored.digest(),
        first.digest(),
        "the record reads back at its digest"
    );
}

#[test]
fn test_an_embedded_repository_holding_no_commit_records_its_head_as_null() {
    let scratch = Scratch::new("embedded", "no-commit");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let emb = main.join("emb");
    fs::create_dir_all(&emb).expect("the embedded root stands");
    git(&emb, &["init", "-q", "-b", "main", "."]);
    write_file(&emb, "f.txt", "uncommitted\n");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let entries = embedded_entries(&record.to_dict());
    assert_eq!(entries.len(), 1, "the member carries the one entry");
    let (path, head, digest) = &entries[0];
    assert_eq!(path, "emb", "the entry records its path");
    assert!(
        matches!(head, JsonValue::Null),
        "the head stands null where the repository holds no commit"
    );
    assert!(
        matches!(digest, JsonValue::Null),
        "the digest stands null beside the head"
    );
}
