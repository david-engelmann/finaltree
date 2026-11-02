"""Round 1 review fix pins for finaltree.

Each test names the fix in CONTRACT.md it proves. The shapes are
the shapes the red team and the two axis review demonstrated, run
against synthetic repos built by the factory.
"""

import json
import os
import subprocess
import sys

import pytest

from finaltree import (
    DirtyFile,
    Fingerprint,
    GeneratedFile,
    TrackedFile,
    compute_fingerprint,
    read_stamped_log,
    run_and_stamp,
    verify_stamped_log,
)
from finaltree.cli import main
from finaltree.errors import FinaltreeError
from finaltree.stamping import render_stamped_log
from repo_factory import TEST_ENV, commit_all, git, make_repo, write

PASS_COMMAND = [
    sys.executable,
    "-c",
    "print('12 passed in 0.50s'); print('count 12 of 12')",
]
FAIL_COMMAND = [sys.executable, "-c", "import sys; print('3 failed'); sys.exit(3)"]
EMPTY_COMMAND = [sys.executable, "-c", "pass"]


def _stamp(tmp_path, root, command=PASS_COMMAND):
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, command, log_path)
    return log_path


def _tampered(tmp_path, log_path, transform):
    lines = log_path.read_text(encoding="utf-8").splitlines()
    bad_path = tmp_path / "tampered.log"
    bad_path.write_text(
        "\n".join(transform(lines)) + "\n", encoding="utf-8"
    )
    return bad_path


def _assert_refused(bad_path, root):
    with pytest.raises(FinaltreeError):
        read_stamped_log(bad_path)
    assert main(["verify", str(bad_path), str(root)]) == 2


# FIX-1. The index stands in the fingerprint.


def test_index_blob_stands_beside_the_worktree_blob(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "staged version X\n")
    git(root, "add", "a.txt")
    write(root, "a.txt", "alpha\n")
    fingerprint = compute_fingerprint(root)
    entry = {e.path: e for e in fingerprint.tracked}["a.txt"]
    assert entry.blob == git(root, "hash-object", "a.txt")
    index_line = git(root, "ls-files", "-s", "a.txt")
    assert entry.index_blobs == (index_line.split()[1],)
    assert entry.blob != entry.index_blobs[0]


def test_restaged_content_changes_the_fingerprint(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "staged version X\n")
    git(root, "add", "a.txt")
    write(root, "a.txt", "alpha\n")
    first = compute_fingerprint(root)
    write(root, "a.txt", "staged version Y, different\n")
    git(root, "add", "a.txt")
    write(root, "a.txt", "alpha\n")
    second = compute_fingerprint(root)
    assert first.digest != second.digest


def test_verify_names_an_index_difference(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "staged version X\n")
    git(root, "add", "a.txt")
    write(root, "a.txt", "alpha\n")
    log_path = _stamp(tmp_path, root)
    write(root, "a.txt", "staged version Y, different\n")
    git(root, "add", "a.txt")
    write(root, "a.txt", "alpha\n")
    result = verify_stamped_log(log_path, root)
    assert not result.matched
    assert result.differences == ("index content changed a.txt",)


# FIX-2. The stamped log covers its own claims.


def test_stamped_log_carries_an_evidence_digest(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    lines = log_path.read_text(encoding="utf-8").splitlines()
    evidence = [
        line for line in lines if line.startswith("evidence-digest ")
    ]
    assert len(evidence) == 1
    stamped = read_stamped_log(log_path)
    assert stamped.evidence_digest == evidence[0].split(" ", 1)[1]
    assert len(stamped.evidence_digest) == 64


def test_verify_refuses_a_log_with_an_edited_count_line(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            line.replace("count 12 of 12", "count 99 of 99")
            for line in lines
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_a_log_with_a_deleted_output_line(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            line for line in lines if "count 12 of 12" not in line
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_a_log_with_a_forged_output_line(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)

    def forge(lines):
        digest = lines[1].split(" ", 1)[1]
        forged = f"{digest} | 9999 passed, forged"
        return lines[:-1] + [forged] + lines[-1:]

    _assert_refused(_tampered(tmp_path, log_path, forge), root)


def test_verify_refuses_a_log_with_a_flipped_exit_code(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root, FAIL_COMMAND)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            "exit-code 0" if line.startswith("exit-code ") else line
            for line in lines
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_a_log_without_a_fingerprint_line(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root, EMPTY_COMMAND)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            line for line in lines if not line.startswith("fingerprint ")
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_a_log_whose_record_carries_no_digest(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)

    def strip_digest(lines):
        out = []
        for line in lines:
            if line.startswith("fingerprint-json "):
                data = json.loads(line[len("fingerprint-json "):])
                del data["digest"]
                line = "fingerprint-json " + json.dumps(
                    data, sort_keys=True
                )
            out.append(line)
        return out

    _assert_refused(_tampered(tmp_path, log_path, strip_digest), root)


def test_verify_refuses_a_log_without_an_exit_code_line(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            line for line in lines if not line.startswith("exit-code ")
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_a_log_without_a_command_line(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            line for line in lines if not line.startswith("command-json ")
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_a_log_with_content_after_the_output_section(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)

    def append_record(lines):
        record = [
            line for line in lines if line.startswith("fingerprint-json ")
        ][0]
        return lines + [record]

    _assert_refused(_tampered(tmp_path, log_path, append_record), root)


# FIX-3. Untracked content is bound.


def test_untracked_content_change_changes_the_fingerprint(tmp_path):
    root = make_repo(tmp_path)
    write(root, "loose.txt", "content one\n")
    first = compute_fingerprint(root)
    write(root, "loose.txt", "content two, completely different\n")
    second = compute_fingerprint(root)
    assert first.digest != second.digest


def test_rewrite_after_rm_cached_changes_the_fingerprint(tmp_path):
    root = make_repo(tmp_path)
    git(root, "rm", "-q", "--cached", "a.txt")
    write(root, "a.txt", "worktree content one\n")
    first = compute_fingerprint(root)
    kinds = {(e.path, e.kind) for e in first.dirty}
    assert ("a.txt", "staged") in kinds
    assert ("a.txt", "untracked") in kinds
    entry = {e.path: e.blob for e in first.untracked}["a.txt"]
    assert entry == git(root, "hash-object", "a.txt")
    write(root, "a.txt", "worktree content two\n")
    second = compute_fingerprint(root)
    assert first.digest != second.digest


def test_verify_names_an_untracked_content_change(tmp_path):
    root = make_repo(tmp_path)
    write(root, "loose.txt", "content one\n")
    log_path = _stamp(tmp_path, root)
    write(root, "loose.txt", "content two\n")
    result = verify_stamped_log(log_path, root)
    assert not result.matched
    assert "untracked content changed loose.txt" in result.differences


# FIX-4. A link stands as itself.


def test_symlink_tracked_blob_is_the_link_itself(tmp_path):
    root = make_repo(tmp_path)
    os.symlink("a.txt", root / "link.txt")
    git(root, "add", "link.txt")
    git(root, "commit", "-q", "-m", "add the link")
    fingerprint = compute_fingerprint(root)
    entry = {e.path: e for e in fingerprint.tracked}["link.txt"]
    link_blob = git(root, "rev-parse", "HEAD:link.txt")
    assert entry.blob == link_blob
    assert entry.index_blobs == (link_blob,)
    assert entry.blob != git(root, "hash-object", "a.txt")


def test_broken_symlink_stands_apart_from_a_deleted_file(tmp_path):
    root = make_repo(tmp_path)
    os.symlink("a.txt", root / "link.txt")
    git(root, "add", "link.txt")
    git(root, "commit", "-q", "-m", "add the link")
    os.unlink(root / "link.txt")
    os.symlink("missing-target.txt", root / "link.txt")
    (root / "b.txt").unlink()
    fingerprint = compute_fingerprint(root)
    entries = {e.path: e for e in fingerprint.tracked}
    expected = subprocess.run(
        ["git", "-C", str(root), "hash-object", "--stdin"],
        input=b"missing-target.txt",
        capture_output=True,
        env=TEST_ENV,
    ).stdout.decode("ascii").strip()
    assert entries["b.txt"].blob is None
    assert entries["link.txt"].blob == expected
    assert entries["link.txt"].blob is not None


# FIX-5. Patterns mean what FT2-4 says.


def test_single_star_stays_within_one_folder(tmp_path):
    root = make_repo(tmp_path)
    write(root, "gen/top.txt", "top\n")
    write(root, "gen/deep/x.txt", "deep\n")
    fingerprint = compute_fingerprint(root, ("gen/*.txt",))
    assert [e.path for e in fingerprint.generated] == ["gen/top.txt"]


def test_bare_name_pattern_matches_no_nested_file(tmp_path):
    root = make_repo(tmp_path)
    write(root, "gen/top.txt", "top\n")
    fingerprint = compute_fingerprint(root, ("top.txt",))
    assert fingerprint.generated == ()
    write(root, "top.txt", "root top\n")
    fingerprint = compute_fingerprint(root, ("top.txt",))
    assert [e.path for e in fingerprint.generated] == ["top.txt"]


# FIX-6. The root is the top level and a conflict reads as a conflict.


def test_subdirectory_of_a_tree_is_refused(tmp_path):
    root = make_repo(tmp_path)
    write(root, "sub/inner.txt", "inner\n")
    commit_all(root, "add the sub folder")
    with pytest.raises(FinaltreeError) as excinfo:
        compute_fingerprint(root / "sub")
    top_level = git(root, "rev-parse", "--show-toplevel")
    assert "not the top level" in str(excinfo.value)
    assert top_level in str(excinfo.value)


def test_conflicted_file_reads_with_a_conflicted_kind(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "base\n")
    git(root, "add", "a.txt")
    git(root, "commit", "-q", "-m", "base two")
    git(root, "checkout", "-q", "-b", "side")
    write(root, "a.txt", "side\n")
    git(root, "commit", "-q", "-a", "-m", "side")
    git(root, "checkout", "-q", "main")
    write(root, "a.txt", "main\n")
    git(root, "commit", "-q", "-a", "-m", "main change")
    merged = subprocess.run(
        ["git", "-C", str(root), "merge", "side"],
        capture_output=True,
        env=TEST_ENV,
    )
    assert merged.returncode != 0
    fingerprint = compute_fingerprint(root)
    kinds = {(e.path, e.kind) for e in fingerprint.dirty}
    assert ("a.txt", "conflicted") in kinds
    assert ("a.txt", "staged") not in kinds
    assert ("a.txt", "modified") not in kinds
    entry = {e.path: e for e in fingerprint.tracked}["a.txt"]
    assert len(entry.index_blobs) == 3


# FIX-7. Verification compares whole member lists.


def test_record_carrying_a_member_twice_is_refused(tmp_path):
    root = make_repo(tmp_path)
    fingerprint = compute_fingerprint(root)
    doubled = Fingerprint(
        commit=fingerprint.commit,
        tracked=fingerprint.tracked + fingerprint.tracked[:1],
        dirty=fingerprint.dirty,
        generated=fingerprint.generated,
        untracked=fingerprint.untracked,
        generated_patterns=fingerprint.generated_patterns,
    )
    log_path = tmp_path / "gate.log"
    log_path.write_text(
        render_stamped_log(doubled, PASS_COMMAND, 0, "ok\n"),
        encoding="utf-8",
    )
    _assert_refused(log_path, root)


def test_verify_names_tracked_files_added_and_removed(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    git(root, "rm", "-q", "b.txt")
    write(root, "c.txt", "gamma\n")
    git(root, "add", "c.txt")
    result = verify_stamped_log(log_path, root)
    assert not result.matched
    assert "tracked file removed b.txt" in result.differences
    assert "tracked file added c.txt" in result.differences


# FIX-8. The remaining pins and the UTF-8 refusal.


def test_stamp_merges_stderr_into_the_output(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    command = [
        sys.executable,
        "-c",
        "import sys; print('out line'); "
        "print('err line', file=sys.stderr); sys.exit(3)",
    ]
    assert run_and_stamp(root, command, log_path) == 3
    stamped = read_stamped_log(log_path)
    assert stamped.exit_code == 3
    assert set(stamped.output_lines) == {"out line", "err line"}


def test_staging_the_same_content_changes_the_fingerprint(tmp_path):
    root = make_repo(tmp_path)
    write(root, "a.txt", "alpha changed\n")
    unstaged = compute_fingerprint(root)
    git(root, "add", "a.txt")
    staged = compute_fingerprint(root)
    unstaged_entry = {e.path: e for e in unstaged.tracked}["a.txt"]
    staged_entry = {e.path: e for e in staged.tracked}["a.txt"]
    assert unstaged_entry.blob == staged_entry.blob
    assert unstaged.digest != staged.digest


def test_digest_ignores_the_order_of_every_member_list(tmp_path):
    manual = Fingerprint(
        commit="a" * 40,
        tracked=(
            TrackedFile(path="x.txt", blob="b" * 40),
            TrackedFile(path="y.txt", blob="c" * 40),
        ),
        dirty=(
            DirtyFile(path="x.txt", kind="staged"),
            DirtyFile(path="y.txt", kind="untracked"),
        ),
        generated=(
            GeneratedFile(path="g1.bin", blob="d" * 40),
            GeneratedFile(path="g2.bin", blob="e" * 40),
        ),
    )
    twin = Fingerprint(
        commit=manual.commit,
        tracked=tuple(reversed(manual.tracked)),
        dirty=tuple(reversed(manual.dirty)),
        generated=tuple(reversed(manual.generated)),
    )
    assert manual.digest == twin.digest
    root = make_repo(tmp_path)
    write(root, "u1.txt", "one\n")
    write(root, "u2.txt", "two\n")
    fingerprint = compute_fingerprint(root)
    assert len(fingerprint.untracked) == 2
    twin = Fingerprint(
        commit=fingerprint.commit,
        tracked=fingerprint.tracked,
        dirty=fingerprint.dirty,
        generated=fingerprint.generated,
        untracked=tuple(reversed(fingerprint.untracked)),
        generated_patterns=fingerprint.generated_patterns,
    )
    assert twin.digest == fingerprint.digest


def test_verify_refuses_a_log_with_an_undecodable_fingerprint_record(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            "fingerprint-json {oops"
            if line.startswith("fingerprint-json ")
            else line
            for line in lines
        ],
    )
    _assert_refused(bad, root)


def test_verify_refuses_an_output_line_without_the_digest_prefix(tmp_path):
    root = make_repo(tmp_path)
    log_path = _stamp(tmp_path, root)
    bad = _tampered(
        tmp_path,
        log_path,
        lambda lines: [
            line.split(" | ", 1)[1]
            if " | count 12 of 12" in line
            else line
            for line in lines
        ],
    )
    _assert_refused(bad, root)


def test_path_spelled_with_non_utf8_bytes_is_refused(tmp_path):
    root = make_repo(tmp_path)
    raw_name = b"bad-\xff.txt"
    with open(os.path.join(os.fsencode(root), raw_name), "wb") as handle:
        handle.write(b"bad name\n")
    subprocess.run(
        [b"git", b"-C", os.fsencode(root), b"add", raw_name],
        check=True,
        capture_output=True,
        env=TEST_ENV,
    )
    subprocess.run(
        [b"git", b"-C", os.fsencode(root), b"commit", b"-q", b"-m", b"bad"],
        check=True,
        capture_output=True,
        env=TEST_ENV,
    )
    with pytest.raises(FinaltreeError):
        compute_fingerprint(root)


# FIX-9. The emitted record stands in canonical order.


def test_record_dict_emits_members_in_canonical_order(tmp_path):
    root = make_repo(tmp_path)
    write(root, "m.txt", "middle\n")
    git(root, "add", "m.txt")
    write(root, "u.txt", "loose\n")
    write(root, "dist/a.bin", "built a\n")
    write(root, "dist/z.bin", "built z\n")
    fingerprint = compute_fingerprint(root, ("dist/**",))
    shuffled = Fingerprint(
        commit=fingerprint.commit,
        tracked=tuple(reversed(fingerprint.tracked)),
        dirty=tuple(reversed(fingerprint.dirty)),
        generated=tuple(reversed(fingerprint.generated)),
        untracked=tuple(reversed(fingerprint.untracked)),
        generated_patterns=fingerprint.generated_patterns,
    )
    data = shuffled.to_dict()
    tracked_paths = [entry["path"] for entry in data["tracked"]]
    assert tracked_paths == sorted(tracked_paths)
    dirty_pairs = [
        (entry["path"], entry["kind"]) for entry in data["dirty"]
    ]
    assert dirty_pairs == sorted(dirty_pairs)
    generated_paths = [entry["path"] for entry in data["generated"]]
    assert generated_paths == sorted(generated_paths)
    untracked_paths = [entry["path"] for entry in data["untracked"]]
    assert untracked_paths == sorted(untracked_paths)
    assert shuffled.digest == fingerprint.digest
