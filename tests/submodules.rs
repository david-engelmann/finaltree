//! The pins of slice 3, submodule state, over synthetic
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
fn make_submodule_repo(area: &str) -> (Scratch, PathBuf, PathBuf) {
    let scratch = Scratch::new("submodules", area);
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
    (scratch, main, source)
}

fn fingerprint(root: &Path) -> Fingerprint {
    compute_fingerprint(root, &[]).expect("the tree fingerprints")
}

fn commit_inside_submodule(main: &Path) {
    let sub = main.join("sub");
    write_file(&sub, "f.txt", "moved\n");
    git(&sub, &["add", "."]);
    git(&sub, &["commit", "-q", "-m", "moved"]);
}

#[test]
fn test_the_five_submodule_standings_fingerprint_apart() {
    let (_scratch, main, _source) = make_submodule_repo("standings");
    let sub = main.join("sub");
    let mut digests = Vec::new();
    digests.push(fingerprint(&main).digest());
    // The content of the submodule edited.
    write_file(&sub, "f.txt", "edited\n");
    digests.push(fingerprint(&main).digest());
    // A change staged inside the submodule.
    git(&sub, &["add", "."]);
    digests.push(fingerprint(&main).digest());
    git(&sub, &["reset", "-q", "--hard", "HEAD"]);
    // A file untracked inside the submodule.
    write_file(&sub, "new.txt", "untracked\n");
    digests.push(fingerprint(&main).digest());
    std::fs::remove_file(sub.join("new.txt")).expect("the untracked file is removed");
    // The head of the submodule moved past the recorded commit.
    commit_inside_submodule(&main);
    digests.push(fingerprint(&main).digest());
    git(&sub, &["reset", "-q", "--hard", "HEAD~1"]);
    // The submodule uninitialized, nothing checked out.
    git(&main, &["submodule", "deinit", "-q", "sub"]);
    digests.push(fingerprint(&main).digest());
    let mut apart = digests.clone();
    apart.sort();
    apart.dedup();
    assert_eq!(
        apart.len(),
        digests.len(),
        "every standing of the submodule fingerprints apart, {digests:?}"
    );
}

#[test]
fn test_a_tree_without_submodules_fingerprints_as_before() {
    let scratch = Scratch::new("submodules", "plain");
    let root = scratch.0.join("repo");
    common::make_factory_repo(&root);
    let record = fingerprint(&root).to_dict();
    assert!(
        record.get("submodules").is_none(),
        "a tree without submodules carries no submodules member"
    );
}

#[test]
fn test_a_submodule_entry_records_its_index_head_and_digest() {
    let (_scratch, main, _source) = make_submodule_repo("entry");
    let printed = {
        let output = common::pinned(&mut std::process::Command::new("git"))
            .args(["ls-files", "--stage", "sub"])
            .current_dir(&main)
            .output()
            .expect("git runs");
        String::from_utf8(output.stdout).expect("the listing is text")
    };
    let gitlink = printed
        .split_whitespace()
        .nth(1)
        .expect("the gitlink stands in the listing")
        .to_string();
    let record = fingerprint(&main);
    assert_eq!(record.submodules.len(), 1, "one submodule stands recorded");
    let entry = &record.submodules[0];
    assert_eq!(entry.path, JsonValue::text("sub"));
    assert_eq!(entry.index_commit, JsonValue::text(gitlink.clone()));
    assert_eq!(entry.head, JsonValue::text(gitlink));
    let own = fingerprint(&main.join("sub"));
    assert_eq!(entry.digest, JsonValue::text(own.digest()));
}

#[test]
fn test_a_moved_submodule_head_names_a_submodule_difference() {
    let (_scratch, main, _source) = make_submodule_repo("moved");
    let before = fingerprint(&main);
    commit_inside_submodule(&main);
    let after = fingerprint(&main);
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences
            .iter()
            .any(|line| line == "submodule head moved sub"),
        "the head move is named, {differences:?}"
    );
}

#[test]
fn test_a_dirty_submodule_names_a_content_difference() {
    let (_scratch, main, _source) = make_submodule_repo("dirty");
    let sub = main.join("sub");
    let before = fingerprint(&main);
    write_file(&sub, "f.txt", "edited\n");
    let after = fingerprint(&main);
    let differences = compare_fingerprints(&before, &after).expect("the comparison stands");
    assert!(
        differences
            .iter()
            .any(|line| line == "submodule content changed sub"),
        "the content change is named, {differences:?}"
    );
}

#[test]
fn test_an_uninitialized_submodule_reads_its_head_as_null() {
    let (_scratch, main, _source) = make_submodule_repo("uninitialized");
    git(&main, &["submodule", "deinit", "-q", "sub"]);
    let record = fingerprint(&main);
    assert_eq!(record.submodules.len(), 1);
    assert_eq!(record.submodules[0].head, JsonValue::Null);
    assert_eq!(record.submodules[0].digest, JsonValue::Null);
}

#[test]
fn test_a_nested_submodule_binds_through_its_parent() {
    let scratch = Scratch::new("submodules", "nested");
    // The inner source, then the middle repo holding it, then
    // the outer repo holding the middle.
    let inner = scratch.0.join("inner");
    make_source_repo(&inner);
    let middle = scratch.0.join("middle");
    std::fs::create_dir_all(&middle).expect("the middle root stands");
    git(&middle, &["init", "-q", "-b", "main", "."]);
    write_file(&middle, "own.txt", "middle\n");
    git(&middle, &["add", "."]);
    git(&middle, &["commit", "-q", "-m", "initial"]);
    let inner_text = inner.to_string_lossy().into_owned();
    git(
        &middle,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &inner_text,
            "deep",
        ],
    );
    git(&middle, &["commit", "-q", "-m", "add the inner"]);
    let outer = scratch.0.join("outer");
    std::fs::create_dir_all(&outer).expect("the outer root stands");
    git(&outer, &["init", "-q", "-b", "main", "."]);
    write_file(&outer, "own.txt", "outer\n");
    git(&outer, &["add", "."]);
    git(&outer, &["commit", "-q", "-m", "initial"]);
    let middle_text = middle.to_string_lossy().into_owned();
    git(
        &outer,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &middle_text,
            "mid",
        ],
    );
    git(&outer, &["commit", "-q", "-m", "add the middle"]);
    git(
        &outer,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "-q",
            "--init",
            "--recursive",
        ],
    );
    let before = fingerprint(&outer).digest();
    write_file(&outer.join("mid").join("deep"), "f.txt", "changed below\n");
    let after = fingerprint(&outer).digest();
    assert_ne!(
        before, after,
        "a change two levels down moves the fingerprint of the tree"
    );
}

#[test]
fn test_a_record_carrying_the_same_submodule_twice_is_refused() {
    let (_scratch, main, _source) = make_submodule_repo("duplicate");
    let record = fingerprint(&main).to_dict();
    let JsonValue::Object(mut pairs) = record else {
        panic!("the record is an object");
    };
    for (key, value) in pairs.iter_mut() {
        if key == "submodules" {
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
fn test_a_stamped_log_verifies_against_the_submodule_tree_it_stamped() {
    let (scratch, main, _source) = make_submodule_repo("stamped");
    let log = scratch.0.join("stamped.log");
    let code = run_and_stamp(&main, &["true".to_string()], &log, &[]).expect("the stamp runs");
    assert_eq!(code, 0);
    let result = verify_stamped_log(&log, &main, None).expect("the verify runs");
    assert!(result.matched, "the log verifies against its own tree");
    assert!(result.differences.is_empty());
}

#[test]
fn test_a_checked_out_submodule_with_an_unborn_head_refuses_the_whole_computation() {
    let (_scratch, main, _source) = make_submodule_repo("unbornhead");
    let sub = main.join("sub");
    git(&sub, &["symbolic-ref", "HEAD", "refs/heads/unborn"]);
    assert!(
        compute_fingerprint(&main, &[]).is_err(),
        "a checked out submodule at an unborn head refuses the whole computation"
    );
}

#[test]
fn test_a_submodule_whose_dot_git_stands_but_reads_nowhere_stands_as_no_checkout() {
    // The dot git file carrying garbage the git reading refuses,
    // with the content of the checkout standing inside. The whole
    // computation refuses at the git status reading it runs over
    // the tree ahead of the checkout test, so no record stands
    // produced at this shape at all.
    {
        let (_scratch, main, _source) = make_submodule_repo("readsnowhere");
        let sub = main.join("sub");
        write_file(&sub, ".git", "garbage in place of a gitfile\n");
        assert!(
            compute_fingerprint(&main, &[]).is_err(),
            "a submodule whose dot git file carries garbage refuses the whole computation"
        );
    }
    // The dot git standing as a dangling symlink, under which the
    // top level reading returns the enclosing tree.
    {
        let (_scratch, main, _source) = make_submodule_repo("danglinglink");
        let sub = main.join("sub");
        std::fs::remove_file(sub.join(".git")).expect("the gitfile is removed");
        std::os::unix::fs::symlink("/nonexistent/finaltree/nowhere", sub.join(".git"))
            .expect("the dangling symlink stands");
        let record = fingerprint(&main);
        assert_eq!(record.submodules.len(), 1, "one submodule stands recorded");
        assert_eq!(record.submodules[0].head, JsonValue::Null);
        assert_eq!(record.submodules[0].digest, JsonValue::Null);
    }
}

#[test]
fn test_a_submodule_directory_holding_files_without_a_dot_git_stands_as_no_checkout() {
    let (_scratch, main, _source) = make_submodule_repo("barefiles");
    git(&main, &["submodule", "deinit", "-q", "sub"]);
    let sub = main.join("sub");
    write_file(&sub, "f.txt", "substance\n");
    let record = fingerprint(&main);
    assert_eq!(record.submodules.len(), 1, "one submodule stands recorded");
    assert_eq!(record.submodules[0].head, JsonValue::Null);
    assert_eq!(record.submodules[0].digest, JsonValue::Null);
}

#[test]
fn test_a_plain_clone_at_a_gitlink_path_stands_checked_out() {
    let (_scratch, main, source) = make_submodule_repo("plainclone");
    git(&main, &["submodule", "deinit", "-q", "sub"]);
    std::fs::remove_dir_all(main.join("sub")).expect("the emptied directory is removed");
    std::fs::remove_dir_all(main.join(".git").join("modules").join("sub"))
        .expect("the modules gitdir is removed");
    // The registration the builder committed in the parent tree
    // stands removed with the rest, so no registration stands
    // anywhere in the tree this pin fingerprints.
    git(&main, &["rm", "-q", ".gitmodules"]);
    let source_text = source.to_string_lossy().into_owned();
    git(
        &main,
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "-q",
            &source_text,
            "sub",
        ],
    );
    let record = fingerprint(&main);
    assert_eq!(record.submodules.len(), 1, "one submodule stands recorded");
    let entry = &record.submodules[0];
    assert_eq!(entry.head, entry.index_commit, "the clone binds its head");
    let own = fingerprint(&main.join("sub"));
    assert_eq!(entry.digest, JsonValue::text(own.digest()));
}

#[test]
fn test_a_corrupt_index_inside_a_checked_out_submodule_refuses_the_whole_computation() {
    let (_scratch, main, _source) = make_submodule_repo("corruptindex");
    let sub = main.join("sub");
    let index = main.join(".git").join("modules").join("sub").join("index");
    std::fs::write(&index, "garbage index standing in place of the real one\n")
        .expect("the corrupt index stands");
    // The head reading inside the submodule stands whole while its
    // stage listing refuses at its header.
    assert!(common::git_succeeds(&sub, &["rev-parse", "HEAD"]));
    assert!(
        compute_fingerprint(&main, &[]).is_err(),
        "a corrupt index inside a checked out submodule refuses the whole computation"
    );
    // The same corrupt index with the submodule standing ignored
    // in the parent its config, so the git status reading stands
    // over the fixture and the refusal rises at the interior seat
    // alone, the own fingerprint call the submodule item carries.
    {
        let (_scratch, main, _source) = make_submodule_repo("corruptindexignored");
        let sub = main.join("sub");
        git(&main, &["config", "submodule.sub.ignore", "all"]);
        let index = main.join(".git").join("modules").join("sub").join("index");
        std::fs::write(&index, "garbage index standing in place of the real one\n")
            .expect("the corrupt index stands");
        assert!(common::git_succeeds(&sub, &["rev-parse", "HEAD"]));
        assert!(
            compute_fingerprint(&main, &[]).is_err(),
            "a corrupt index inside a checked out submodule refuses the whole computation"
        );
    }
}

#[test]
fn test_a_submodule_reached_through_a_symlink_stands_checked_out() {
    let (_scratch, main, _source) = make_submodule_repo("symlinkpath");
    std::fs::rename(main.join("sub"), main.join("subcheckout")).expect("the checkout moves aside");
    std::os::unix::fs::symlink("subcheckout", main.join("sub")).expect("the symlink stands");
    let sub = main.join("sub");
    // The top level reading at the path through the symlink stands
    // whole, the checkout test standing passed at its own seat,
    // while the whole computation refuses at the git status
    // reading it runs over the tree, which admits no submodule
    // path standing as a symbolic link, so no record stands
    // produced at this shape at all.
    assert!(common::git_succeeds(
        &sub,
        &["rev-parse", "--show-toplevel"]
    ));
    assert!(
        compute_fingerprint(&main, &[]).is_err(),
        "a submodule reached through a symlink refuses the whole computation"
    );
}
