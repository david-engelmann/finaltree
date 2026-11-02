"""Fingerprint obligation tests for finaltree slice 2."""

import subprocess

import pytest

from finaltree import (
    DirtyFile,
    Fingerprint,
    GeneratedFile,
    TrackedFile,
    compute_fingerprint,
)
from finaltree.errors import FinaltreeError
from repo_factory import TEST_ENV, commit_all, git, make_repo, write


def test_fingerprint_holds_head_commit(tmp_path):
    root = make_repo(tmp_path)
    fingerprint = compute_fingerprint(root)
    assert fingerprint.commit == git(root, "rev-parse", "HEAD")
    assert len(fingerprint.commit) == 40


def test_repo_without_a_commit_is_refused(tmp_path):
    root = tmp_path / "empty"
    root.mkdir()
    subprocess.run(
        ["git", "init", "-q", "-b", "main", str(root)],
        check=True,
        capture_output=True,
        env=TEST_ENV,
    )
    with pytest.raises(FinaltreeError):
        compute_fingerprint(root)


def test_tracked_files_sorted_with_worktree_blob_hashes(tmp_path):
    root = make_repo(tmp_path)
    fingerprint = compute_fingerprint(root)
    paths = [entry.path for entry in fingerprint.tracked]
    assert paths == ["a.txt", "b.txt"]
    for entry in fingerprint.tracked:
        assert entry.blob == git(root, "hash-object", entry.path)
    write(root, "a.txt", "alpha changed\n")
    changed = compute_fingerprint(root)
    entry = {e.path: e.blob for e in changed.tracked}["a.txt"]
    assert entry == git(root, "hash-object", "a.txt")
    assert entry != {e.path: e.blob for e in fingerprint.tracked}["a.txt"]


def test_staged_addition_is_tracked_before_commit(tmp_path):
    root = make_repo(tmp_path)
    write(root, "c.txt", "gamma\n")
    git(root, "add", "c.txt")
    assert "c.txt" not in git(root, "ls-tree", "-r", "--name-only", "HEAD")
    fingerprint = compute_fingerprint(root)
    assert "c.txt" in [entry.path for entry in fingerprint.tracked]


def test_deleted_tracked_file_carries_null_blob(tmp_path):
    root = make_repo(tmp_path)
    (root / "b.txt").unlink()
    fingerprint = compute_fingerprint(root)
    entry = {e.path: e.blob for e in fingerprint.tracked}["b.txt"]
    assert entry is None


def test_dirty_entries_name_each_kind(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "alpha staged\n")
    git(root, "add", "a.txt")
    write(root, "b.txt", "beta modified\n")
    write(root, "loose.txt", "loose\n")
    fingerprint = compute_fingerprint(root)
    kinds = {(entry.path, entry.kind) for entry in fingerprint.dirty}
    assert ("a.txt", "staged") in kinds
    assert ("b.txt", "modified") in kinds
    assert ("loose.txt", "untracked") in kinds


def test_file_staged_and_modified_carries_both_entries(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "alpha staged\n")
    git(root, "add", "a.txt")
    write(root, "a.txt", "alpha staged and modified again\n")
    fingerprint = compute_fingerprint(root)
    kinds = {(entry.path, entry.kind) for entry in fingerprint.dirty}
    assert ("a.txt", "staged") in kinds
    assert ("a.txt", "modified") in kinds


def test_generated_artifacts_match_declared_patterns(tmp_path):
    root = make_repo(tmp_path)
    write(root, "dist/app.generated.js", "built once\n")
    fingerprint = compute_fingerprint(root, ("dist/**",))
    assert [entry.path for entry in fingerprint.generated] == [
        "dist/app.generated.js"
    ]
    assert fingerprint.generated[0].blob == git(
        root, "hash-object", "dist/app.generated.js"
    )
    assert all(
        entry.path != "dist/app.generated.js" for entry in fingerprint.dirty
    )


def test_ignored_generated_artifact_is_listed(tmp_path):
    root = make_repo(tmp_path)
    write(root, ".gitignore", "dist/\n")
    commit_all(root, "ignore the build output")
    write(root, "dist/out.bin", "built\n")
    assert git(root, "status", "--porcelain") == ""
    fingerprint = compute_fingerprint(root, ("dist/**",))
    assert [entry.path for entry in fingerprint.generated] == ["dist/out.bin"]


def test_generated_untracked_file_is_not_an_ordinary_dirty_entry(tmp_path):
    root = make_repo(tmp_path)
    write(root, "build.generated.js", "built\n")
    write(root, "loose.txt", "loose\n")
    fingerprint = compute_fingerprint(root, ("*.generated.js",))
    assert [entry.path for entry in fingerprint.generated] == [
        "build.generated.js"
    ]
    kinds = {(entry.path, entry.kind) for entry in fingerprint.dirty}
    assert ("build.generated.js", "untracked") not in kinds
    assert ("loose.txt", "untracked") in kinds


def test_digest_is_canonical_and_order_independent(tmp_path):
    root = make_repo(tmp_path)
    write(root, "loose.txt", "loose\n")
    fingerprint = compute_fingerprint(root)
    shuffled = Fingerprint(
        commit=fingerprint.commit,
        tracked=tuple(reversed(fingerprint.tracked)),
        dirty=tuple(reversed(fingerprint.dirty)),
        generated=fingerprint.generated,
        untracked=fingerprint.untracked,
        generated_patterns=fingerprint.generated_patterns,
    )
    assert shuffled.digest == fingerprint.digest
    assert compute_fingerprint(root).digest == fingerprint.digest


def test_any_member_change_changes_the_digest(tmp_path):
    root = make_repo(tmp_path)
    digests = [compute_fingerprint(root, ("dist/**",)).digest]
    write(root, "a.txt", "alpha changed\n")
    digests.append(compute_fingerprint(root, ("dist/**",)).digest)
    commit_all(root, "change a")
    digests.append(compute_fingerprint(root, ("dist/**",)).digest)
    write(root, "dist/out.bin", "built\n")
    digests.append(compute_fingerprint(root, ("dist/**",)).digest)
    write(root, "dist/out.bin", "built differently\n")
    digests.append(compute_fingerprint(root, ("dist/**",)).digest)
    assert len(set(digests)) == len(digests)


def test_fingerprint_members_are_retained_beside_the_digest(tmp_path):
    root = make_repo(tmp_path)
    fingerprint = compute_fingerprint(root)
    data = fingerprint.to_dict()
    assert data["digest"] == fingerprint.digest
    assert data["commit"] == fingerprint.commit
    restored = Fingerprint.from_dict(data)
    assert restored == fingerprint
    manual = Fingerprint(
        commit="a" * 40,
        tracked=(TrackedFile(path="x.txt", blob="b" * 40),),
        dirty=(DirtyFile(path="y.txt", kind="untracked"),),
        generated=(GeneratedFile(path="z.bin", blob="c" * 40),),
    )
    assert manual.digest != fingerprint.digest
