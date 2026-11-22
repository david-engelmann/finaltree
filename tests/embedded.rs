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
use finaltree::render::render_compact;
use finaltree::sha256::sha256_hex;
use finaltree::stamping::{compare_fingerprints, run_and_stamp, verify_stamped_log};

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

/// Doctor a genuine record by standing the embedded
/// member present with an empty list, the digest
/// recomputed over the payload as written, in the
/// fixture discipline EM-6 states. The payload the
/// digest stands over is the record with its digest
/// and its generated patterns set aside and the member
/// standing where the record carries it.
fn record_with_empty_embedded(record: &JsonValue) -> JsonValue {
    let JsonValue::Object(pairs) = record else {
        panic!("the record is an object");
    };
    let mut payload: Vec<(String, JsonValue)> = Vec::new();
    let mut patterns = JsonValue::List(Vec::new());
    for (key, value) in pairs {
        match key.as_str() {
            "digest" => {}
            "generated_patterns" => patterns = value.clone(),
            "embedded" => payload.push((key.clone(), JsonValue::List(Vec::new()))),
            _ => payload.push((key.clone(), value.clone())),
        }
    }
    let digest = sha256_hex(render_compact(&JsonValue::Object(payload.clone())).as_bytes());
    let mut doctored = payload;
    doctored.push(("digest".to_string(), JsonValue::text(digest)));
    doctored.push(("generated_patterns".to_string(), patterns));
    JsonValue::Object(doctored)
}

#[test]
fn test_a_record_carrying_an_embedded_member_reads_back_whole() {
    let (_scratch, main) = make_tree_with_embedded("reads-back");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    let entries = embedded_entries(&dict);
    assert_eq!(entries.len(), 1, "the member carries the one entry");
    let restored = Fingerprint::from_dict(&dict).expect("the record reads back");
    assert_eq!(
        restored.embedded.len(),
        1,
        "the read record carries the member"
    );
    assert_eq!(
        embedded_entries(&restored.to_dict()),
        entries,
        "the member reads back as written"
    );
    assert_eq!(
        restored.to_dict(),
        dict,
        "the read record re-emits the record whole"
    );
}

#[test]
fn test_a_record_carrying_the_same_embedded_repository_twice_is_refused() {
    let (_scratch, main) = make_tree_with_embedded("duplicate");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    let JsonValue::Object(mut pairs) = dict else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "embedded" {
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
fn test_a_record_carrying_an_embedded_entry_that_does_not_decode_is_refused() {
    let (_scratch, main) = make_tree_with_embedded("undecodable-entry");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    let JsonValue::Object(mut pairs) = dict else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "embedded" {
            let JsonValue::List(items) = value else {
                panic!("the member is a list");
            };
            let JsonValue::Object(entry) = &mut items[0] else {
                panic!("the entry is an object");
            };
            entry.retain(|(field, _)| field.as_str() != "digest");
        }
    }
    let dict = JsonValue::Object(pairs);
    let error = Fingerprint::from_dict(&dict).expect_err("the entry refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
}

#[test]
fn test_a_record_carrying_an_empty_embedded_member_is_refused() {
    let (_scratch, main) = make_tree_with_embedded("empty-member");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record_with_empty_embedded(&record.to_dict());
    let error = Fingerprint::from_dict(&dict).expect_err("the empty member refuses");
    assert_eq!(error.to_string(), "the fingerprint record does not decode");
}

#[test]
fn test_a_removed_embedded_repository_names_an_embedded_difference() {
    let (_scratch, main) = make_tree_with_embedded("removed");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    fs::remove_dir_all(main.join("emb")).expect("the repository is removed");
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences
            .iter()
            .any(|line| line == "embedded removed emb"),
        "the removal is named, {differences:?}"
    );
}

#[test]
fn test_a_added_embedded_repository_names_an_embedded_difference() {
    let scratch = Scratch::new("embedded", "added");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    make_embedded_repo(&main.join("emb"));
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences.iter().any(|line| line == "embedded added emb"),
        "the addition is named, {differences:?}"
    );
}

#[test]
fn test_a_moved_embedded_head_names_an_embedded_difference() {
    let (_scratch, main) = make_tree_with_embedded("head-moved");
    let emb = main.join("emb");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    write_file(&emb, "g.txt", "second\n");
    git(&emb, &["add", "."]);
    git(&emb, &["commit", "-q", "-m", "second"]);
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences
            .iter()
            .any(|line| line == "embedded head moved emb"),
        "the head move is named, {differences:?}"
    );
}

#[test]
fn test_a_dirty_embedded_repository_names_an_embedded_content_difference() {
    let (_scratch, main) = make_tree_with_embedded("content-changed");
    let emb = main.join("emb");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    write_file(&emb, "f.txt", "embedded beta\n");
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences
            .iter()
            .any(|line| line == "embedded content changed emb"),
        "the content change is named, {differences:?}"
    );
}

#[test]
fn test_a_stamped_log_verifies_against_the_embedded_tree_it_stamped() {
    let (scratch, main) = make_tree_with_embedded("stamped-log");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&main, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &main, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
}

#[test]
fn test_an_embedded_repository_with_an_unborn_head_records_its_head_as_null() {
    let (_scratch, main) = make_tree_with_embedded("unborn-head");
    let emb = main.join("emb");
    git(&emb, &["symbolic-ref", "HEAD", "refs/heads/unborn"]);
    assert!(
        !git_text(&emb, &["rev-parse", "main"]).is_empty(),
        "the repository holds its commit on the held branch"
    );
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let entries = embedded_entries(&record.to_dict());
    assert_eq!(entries.len(), 1, "the member carries the one entry");
    let (path, head, digest) = &entries[0];
    assert_eq!(path, "emb", "the entry records its path");
    assert!(
        matches!(head, JsonValue::Null),
        "the head stands null where the head reading fails"
    );
    assert!(
        matches!(digest, JsonValue::Null),
        "the digest stands null beside the head"
    );
}

#[test]
fn test_a_commit_landing_on_the_held_branch_of_an_unborn_head_repository_moves_nothing() {
    let (_scratch, main) = make_tree_with_embedded("unborn-held-commit");
    let emb = main.join("emb");
    git(&emb, &["symbolic-ref", "HEAD", "refs/heads/unborn"]);
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let held_before = git_text(&emb, &["rev-parse", "main"]);
    write_file(&emb, "held.txt", "held two\n");
    git(&emb, &["add", "."]);
    let tree = git_text(&emb, &["write-tree"]);
    let commit = git_text(
        &emb,
        &["commit-tree", tree.as_str(), "-p", "main", "-m", "held two"],
    );
    git(&emb, &["update-ref", "refs/heads/main", commit.as_str()]);
    assert_ne!(
        git_text(&emb, &["rev-parse", "main"]),
        held_before,
        "the held branch moves with the landed commit"
    );
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert_eq!(
        before.to_dict(),
        after.to_dict(),
        "the record stands identical across the held branch commit"
    );
    assert_eq!(
        before.digest(),
        after.digest(),
        "the record digest stands across the held branch commit"
    );
}

#[test]
fn test_a_change_inside_a_commitless_embedded_repository_moves_nothing() {
    let scratch = Scratch::new("embedded", "commitless-change");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let emb = main.join("emb");
    fs::create_dir_all(&emb).expect("the embedded root stands");
    git(&emb, &["init", "-q", "-b", "main", "."]);
    write_file(&emb, "f.txt", "uncommitted\n");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    write_file(&emb, "added.txt", "added\n");
    write_file(&emb, "f.txt", "changed\n");
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert_eq!(
        before.to_dict(),
        after.to_dict(),
        "the record stands identical across the interior change"
    );
    assert_eq!(
        before.digest(),
        after.digest(),
        "the record digest stands across the interior change"
    );
}

#[test]
fn test_a_corrupt_embedded_index_refuses_the_whole_computation() {
    let (_scratch, main) = make_tree_with_embedded("corrupt-index");
    let emb = main.join("emb");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert_eq!(
        embedded_entries(&before.to_dict()).len(),
        1,
        "the member stands before the corruption"
    );
    let index = emb.join(".git").join("index");
    let data = fs::read(&index).expect("the index stands");
    let mut corrupted = b"XXXX".to_vec();
    corrupted.resize(data.len(), 0);
    fs::write(&index, corrupted).expect("the index is overwritten");
    let error = compute_fingerprint(&main, &[]).expect_err("the computation refuses");
    assert!(
        error
            .to_string()
            .starts_with("git ls-files --stage -z failed."),
        "the refusal speaks the git reading its sentence, {error}"
    );
}

#[test]
fn test_a_corrupt_embedded_head_stands_as_no_repository() {
    let (_scratch, main) = make_tree_with_embedded("corrupt-head");
    let emb = main.join("emb");
    write_file(&emb, ".git/HEAD", "garbage-not-a-ref\n");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        dict.get("embedded").is_none(),
        "the record carries no embedded member"
    );
    assert!(
        member_paths(&dict, "untracked")
            .iter()
            .any(|path| path == "emb/f.txt"),
        "the working file binds as an ordinary untracked file"
    );
}

#[test]
fn test_an_interior_change_in_a_null_form_repository_names_no_embedded_difference() {
    let scratch = Scratch::new("embedded", "null-form-verify");
    let main = scratch.0.join("main");
    make_factory_repo(&main);
    let emb = main.join("emb");
    fs::create_dir_all(&emb).expect("the embedded root stands");
    git(&emb, &["init", "-q", "-b", "main", "."]);
    write_file(&emb, "f.txt", "uncommitted\n");
    let before = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    write_file(&emb, "f.txt", "changed\n");
    write_file(&emb, "added.txt", "added\n");
    let after = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences.is_empty(),
        "no difference stands named, {differences:?}"
    );
}

/// Set the mode of a fixture path, one of the two
/// fixture steps the pins of this slice add, the mode
/// step and the fifo step standing in no helper of
/// this file before this slice.
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("the path stands").permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).expect("the mode stands");
}

/// Stand a fifo at a fixture path, the second of the
/// two fixture steps the pins of this slice add.
fn make_fifo(root: &Path, relative: &str) {
    let full = root.join(relative);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).expect("the parent stands");
    }
    let status = Command::new("mkfifo")
        .arg(&full)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "the pipe stands");
}

/// Every path a record names outside the embedded
/// member, standing with the name of the member that
/// names it. The empty directories member carries its
/// paths as plain texts and every other member carries
/// its paths in its entries. The declared patterns
/// stand as the caller its words and not as standings
/// of the tree, so the sweep passes that member by with
/// the embedded member itself.
fn record_paths_by_member(record: &JsonValue) -> Vec<(String, String)> {
    let JsonValue::Object(pairs) = record else {
        panic!("the record is an object");
    };
    let mut found = Vec::new();
    for (key, value) in pairs {
        if key.as_str() == "embedded" || key.as_str() == "generated_patterns" {
            continue;
        }
        let JsonValue::List(items) = value else {
            continue;
        };
        for item in items {
            if let Some(text) = item.as_text() {
                found.push((key.clone(), text.to_string()));
            } else if let Some(path) = item.get("path").and_then(JsonValue::as_text) {
                found.push((key.clone(), path.to_string()));
            }
        }
    }
    found
}

#[test]
fn test_a_declared_pattern_reaches_no_file_inside_an_embedded_repository() {
    let (_scratch, main) = make_tree_with_embedded("pattern-boundary");
    write_file(&main, "out/artifact.bin", "outside\n");
    write_file(&main.join("emb"), "artifact.bin", "inside\n");
    let patterns = vec![
        "out/*.bin".to_string(),
        "emb/*.bin".to_string(),
        "emb/f.txt".to_string(),
    ];
    let record = compute_fingerprint(&main, &patterns).expect("the tree fingerprints");
    let generated = member_paths(&record.to_dict(), "generated");
    assert_eq!(
        generated,
        vec!["out/artifact.bin".to_string()],
        "the generated member carries the outside artifact alone"
    );
}

#[test]
fn test_files_inside_an_embedded_repository_stand_in_neither_the_modes_nor_the_file_modes_member() {
    let (_scratch, main) = make_tree_with_embedded("interior-modes");
    let emb = main.join("emb");
    let tool = write_file(&emb, "tool.sh", "tool\n");
    set_mode(&tool, 0o755);
    let wide = write_file(&emb, "wide.txt", "wide\n");
    set_mode(&wide, 0o777);
    let plain_tool = write_file(&main, "plain/tool.sh", "tool\n");
    set_mode(&plain_tool, 0o755);
    let plain_wide = write_file(&main, "plain/wide.txt", "wide\n");
    set_mode(&plain_wide, 0o777);
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        !embedded_entries(&dict).is_empty(),
        "the member stands for the repository"
    );
    assert!(
        member_paths(&dict, "modes")
            .iter()
            .any(|path| path == "plain/tool.sh"),
        "the ordinary executable binds through the modes member"
    );
    assert!(
        member_paths(&dict, "file_modes")
            .iter()
            .any(|path| path == "plain/wide.txt"),
        "the ordinary wide mode file binds through the file modes member"
    );
    for member in ["modes", "file_modes"] {
        for path in member_paths(&dict, member) {
            assert!(
                path != "emb" && !path.starts_with("emb/"),
                "no path of {member} stands under the boundary, {path} stands"
            );
        }
    }
}

#[test]
fn test_an_empty_directory_inside_an_embedded_repository_stands_in_no_member() {
    let (_scratch, main) = make_tree_with_embedded("interior-empty");
    fs::create_dir_all(main.join("emb").join("hollow")).expect("the empty directory stands");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        !embedded_entries(&dict).is_empty(),
        "the member stands for the repository"
    );
    for (member, path) in record_paths_by_member(&dict) {
        assert!(
            path != "emb" && !path.starts_with("emb/"),
            "no path of {member} stands under the boundary, {path} stands"
        );
    }
}

#[test]
fn test_a_special_file_inside_an_embedded_repository_stands_in_no_member() {
    let (_scratch, main) = make_tree_with_embedded("interior-special");
    make_fifo(&main.join("emb"), "pipe");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        !embedded_entries(&dict).is_empty(),
        "the member stands for the repository"
    );
    for (member, path) in record_paths_by_member(&dict) {
        assert!(
            path != "emb" && !path.starts_with("emb/"),
            "no path of {member} stands under the boundary, {path} stands"
        );
    }
}

#[test]
fn test_the_boundary_directory_of_an_embedded_repository_binds_its_mode_in_the_dir_modes_member() {
    let (_scratch, main) = make_tree_with_embedded("boundary-mode");
    set_mode(&main.join("emb"), 0o751);
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    let dict = record.to_dict();
    assert!(
        !embedded_entries(&dict).is_empty(),
        "the member stands for the repository"
    );
    let Some(JsonValue::List(dir_modes)) = dict.get("dir_modes") else {
        panic!("the dir modes member stands");
    };
    let standings: Vec<(String, String)> = dir_modes
        .iter()
        .map(|item| {
            let path = item
                .get("path")
                .and_then(JsonValue::as_text)
                .expect("the entry carries its path")
                .to_string();
            let mode = item
                .get("mode")
                .and_then(JsonValue::as_text)
                .expect("the entry carries its mode")
                .to_string();
            (path, mode)
        })
        .collect();
    assert!(
        standings.contains(&("emb".to_string(), "751".to_string())),
        "the boundary mode binds in the dir modes member, {standings:?}"
    );
    for (member, path) in record_paths_by_member(&dict) {
        if member == "dir_modes" && path == "emb" {
            continue;
        }
        assert!(
            path != "emb" && !path.starts_with("emb/"),
            "no path of {member} stands under the boundary, {path} stands"
        );
    }
}

#[test]
fn test_the_embedded_entry_type_stands_named_at_the_library_door() {
    let (_scratch, main) = make_tree_with_embedded("library-door");
    let emb = main.join("emb");
    let record = compute_fingerprint(&main, &[]).expect("the tree fingerprints");
    assert_eq!(record.embedded.len(), 1, "the member carries the one entry");
    let entry: &finaltree::EmbeddedEntry = &record.embedded[0];
    assert_eq!(
        entry.path.as_text(),
        Some("emb"),
        "the entry records its path"
    );
    assert_eq!(
        entry.head.as_text(),
        Some(git_text(&emb, &["rev-parse", "HEAD"]).as_str()),
        "the entry records the commit the repository stands at"
    );
    let own = compute_fingerprint(&emb, &[]).expect("the repository fingerprints");
    assert_eq!(
        entry.digest.as_text(),
        Some(own.digest().as_str()),
        "the entry records the digest of the repository its own fingerprint"
    );
    let rendered = embedded_entries(&record.to_dict());
    assert_eq!(
        rendered.len(),
        1,
        "the rendered member carries the one entry"
    );
    assert_eq!(rendered[0].0, "emb", "the rendered entry records its path");
    assert_eq!(
        rendered[0].1, entry.head,
        "the rendered head stands as the typed head"
    );
    assert_eq!(
        rendered[0].2, entry.digest,
        "the rendered digest stands as the typed digest"
    );
}
