"""Stamping and verification obligation tests for finaltree slice 2."""

import json
import sys
from pathlib import Path

import pytest

from finaltree import (
    compute_fingerprint,
    read_stamped_log,
    run_and_stamp,
    verify_stamped_log,
)
from finaltree.cli import main
from finaltree.errors import FinaltreeError
from repo_factory import commit_all, make_repo, write

PASS_COMMAND = [
    sys.executable,
    "-c",
    "print('12 passed in 0.50s'); print('count 12 of 12')",
]
FAIL_COMMAND = [sys.executable, "-c", "import sys; print('3 failed'); sys.exit(3)"]


def _snapshot(root):
    root = Path(root)
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file() and ".git" not in path.parts
    }


def test_stamp_binds_output_lines_to_the_fingerprint(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    exit_code = run_and_stamp(root, PASS_COMMAND, log_path)
    assert exit_code == 0
    stamped = read_stamped_log(log_path)
    fingerprint = compute_fingerprint(root)
    assert stamped.fingerprint.digest == fingerprint.digest
    assert stamped.command == tuple(PASS_COMMAND)
    assert stamped.exit_code == 0
    assert stamped.output_lines == ("12 passed in 0.50s", "count 12 of 12")
    raw_lines = log_path.read_text(encoding="utf-8").splitlines()
    begin = raw_lines.index("begin-output")
    end = raw_lines.index("end-output")
    for line in raw_lines[begin + 1:end]:
        assert line.startswith(f"{fingerprint.digest} | ")


def test_stamp_writes_the_log_when_the_gate_fails(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, FAIL_COMMAND, log_path)
    assert log_path.exists()
    stamped = read_stamped_log(log_path)
    assert stamped.exit_code == 3
    assert stamped.output_lines == ("3 failed",)


def test_stamp_returns_the_gate_exit_code(tmp_path):
    root = make_repo(tmp_path)
    assert run_and_stamp(root, FAIL_COMMAND, tmp_path / "a.log") == 3
    assert run_and_stamp(root, PASS_COMMAND, tmp_path / "b.log") == 0
    cli_log = tmp_path / "cli.log"
    assert (
        main(
            [
                "stamp",
                str(root),
                "--log",
                str(cli_log),
                "--",
                *FAIL_COMMAND,
            ]
        )
        == 3
    )


def test_verify_matches_the_tree_the_log_was_stamped_from(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, PASS_COMMAND, log_path)
    result = verify_stamped_log(log_path, root)
    assert result.matched
    assert result.differences == ()
    assert main(["verify", str(log_path), str(root)]) == 0


def test_verify_names_a_moved_commit(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, PASS_COMMAND, log_path)
    write(root, "a.txt", "alpha moved on\n")
    commit_all(root, "move the commit")
    result = verify_stamped_log(log_path, root)
    assert not result.matched
    assert any(
        line.startswith("commit moved from") for line in result.differences
    )
    assert main(["verify", str(log_path), str(root)]) == 1


def test_verify_names_a_changed_file(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, PASS_COMMAND, log_path)
    write(root, "a.txt", "alpha changed after the run\n")
    result = verify_stamped_log(log_path, root)
    assert "file content changed a.txt" in result.differences


def test_verify_names_a_cleaned_and_an_added_dirty_file(tmp_path):
    root = make_repo(tmp_path)
    write(root, "loose.txt", "loose\n")
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, PASS_COMMAND, log_path)
    (root / "loose.txt").unlink()
    write(root, "other.txt", "other\n")
    result = verify_stamped_log(log_path, root)
    assert "dirty file cleaned loose.txt (untracked)" in result.differences
    assert "dirty file added other.txt (untracked)" in result.differences


def test_verify_names_a_generated_difference_as_generated(tmp_path):
    root = make_repo(tmp_path)
    write(root, "dist/app.generated.js", "built once\n")
    commit_all(root, "add the generated bundle")
    log_path = tmp_path / "gate.log"
    run_and_stamp(
        root, PASS_COMMAND, log_path, generated_patterns=("dist/**",)
    )
    write(root, "dist/app.generated.js", "built twice\n")
    result = verify_stamped_log(log_path, root)
    assert (
        "generated artifact differs dist/app.generated.js"
        in result.differences
    )
    assert (
        "file content changed dist/app.generated.js"
        not in result.differences
    )


def test_verify_refuses_a_log_without_a_fingerprint_record(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "plain.log"
    log_path.write_text("12 passed in 0.50s\n", encoding="utf-8")
    with pytest.raises(FinaltreeError):
        read_stamped_log(log_path)
    assert main(["verify", str(log_path), str(root)]) == 2


def test_verify_refuses_a_log_whose_digest_disagrees_with_its_members(tmp_path):
    root = make_repo(tmp_path)
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, PASS_COMMAND, log_path)
    lines = log_path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if line.startswith("fingerprint-json "):
            data = json.loads(line[len("fingerprint-json "):])
            data["commit"] = "0" * 40
            lines[index] = "fingerprint-json " + json.dumps(
                data, sort_keys=True
            )
    log_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    with pytest.raises(FinaltreeError):
        read_stamped_log(log_path)
    assert main(["verify", str(log_path), str(root)]) == 2


def test_fingerprint_and_verify_leave_the_tree_unchanged(tmp_path):
    root = make_repo(tmp_path)
    write(root, "loose.txt", "loose\n")
    before = _snapshot(root)
    log_path = tmp_path / "gate.log"
    run_and_stamp(root, PASS_COMMAND, log_path)
    verify_stamped_log(log_path, root)
    compute_fingerprint(root)
    assert _snapshot(root) == before


def test_verify_names_a_tracked_generated_difference_as_generated(tmp_path):
    root = make_repo(tmp_path)
    write(root, "src/version.generated.txt", "version one\n")
    commit_all(root, "add the generated version file")
    log_path = tmp_path / "gate.log"
    run_and_stamp(
        root,
        PASS_COMMAND,
        log_path,
        generated_patterns=("src/*.generated.txt",),
    )
    write(root, "src/version.generated.txt", "version two\n")
    result = verify_stamped_log(log_path, root)
    assert (
        "generated artifact differs src/version.generated.txt"
        in result.differences
    )
    assert (
        "file content changed src/version.generated.txt"
        not in result.differences
    )
