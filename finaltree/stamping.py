"""Stamped logs and verification against a candidate tree."""

from __future__ import annotations

import hashlib
import json
import subprocess
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

from .errors import FinaltreeError
from .fingerprint import Fingerprint, compute_fingerprint

STAMP_FORMAT = "finaltree-stamped-log/1"


@dataclass(frozen=True)
class StampedLog:
    fingerprint: Fingerprint
    command: tuple[str, ...]
    exit_code: int
    output_lines: tuple[str, ...]
    evidence_digest: str = ""


@dataclass(frozen=True)
class VerificationResult:
    matched: bool
    differences: tuple[str, ...]
    expected: Fingerprint
    actual: Fingerprint


def _evidence_digest(command_line, exit_code_line, output_lines):
    """Cover the command, the exit code, and the output as written."""
    material = "\n".join([command_line, exit_code_line, *output_lines])
    return hashlib.sha256(material.encode("utf-8")).hexdigest()


def render_stamped_log(fingerprint, command, exit_code, output):
    """Render the stamped log text for one gate run."""
    command_line = "command-json " + json.dumps(
        list(command), ensure_ascii=False
    )
    exit_code_line = f"exit-code {exit_code}"
    output_lines = [
        f"{fingerprint.digest} | {line}" for line in output.splitlines()
    ]
    lines = [
        STAMP_FORMAT,
        f"fingerprint {fingerprint.digest}",
        "fingerprint-json "
        + json.dumps(
            fingerprint.to_dict(), sort_keys=True, ensure_ascii=False
        ),
        command_line,
        exit_code_line,
        "evidence-digest "
        + _evidence_digest(command_line, exit_code_line, output_lines),
        "begin-output",
        *output_lines,
        "end-output",
    ]
    return "\n".join(lines) + "\n"


def read_stamped_log(path):
    """Read a stamped log back, refusing any log that does not parse."""
    path = Path(path)
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise FinaltreeError(f"the stamped log at {path} cannot be read") from exc
    except UnicodeDecodeError as exc:
        raise FinaltreeError(
            f"the stamped log at {path} does not decode"
        ) from exc
    lines = text.splitlines()
    if not lines or lines[0] != STAMP_FORMAT:
        raise FinaltreeError("the stamped log carries no finaltree header")
    recorded_digest = None
    fingerprint = None
    command = None
    command_line = None
    exit_code = None
    exit_code_line = None
    evidence_digest = None
    output_lines = []
    raw_output_lines = []
    in_output = False
    saw_end = False
    for line in lines[1:]:
        if saw_end:
            raise FinaltreeError(
                "the stamped log carries content after its output section"
            )
        if in_output:
            if line == "end-output":
                in_output = False
                saw_end = True
                continue
            prefix = f"{recorded_digest} | "
            if recorded_digest is None or not line.startswith(prefix):
                raise FinaltreeError(
                    "an output line in the stamped log carries no "
                    "fingerprint prefix"
                )
            raw_output_lines.append(line)
            output_lines.append(line[len(prefix):])
            continue
        if line == "begin-output":
            in_output = True
        elif line.startswith("fingerprint-json "):
            if fingerprint is not None:
                raise FinaltreeError(
                    "the stamped log carries its fingerprint record twice"
                )
            raw = line[len("fingerprint-json "):]
            try:
                data = json.loads(raw)
            except json.JSONDecodeError as exc:
                raise FinaltreeError(
                    "the fingerprint record in the stamped log does not decode"
                ) from exc
            fingerprint = Fingerprint.from_dict(data)
        elif line.startswith("fingerprint "):
            if recorded_digest is not None:
                raise FinaltreeError(
                    "the stamped log carries its fingerprint line twice"
                )
            recorded_digest = line[len("fingerprint "):].strip()
        elif line.startswith("command-json "):
            if command is not None:
                raise FinaltreeError(
                    "the stamped log carries its command line twice"
                )
            try:
                parsed = json.loads(line[len("command-json "):])
            except json.JSONDecodeError as exc:
                raise FinaltreeError(
                    "the command record in the stamped log does not decode"
                ) from exc
            if not isinstance(parsed, list):
                raise FinaltreeError(
                    "the command record in the stamped log does not decode"
                )
            command = tuple(parsed)
            command_line = line
        elif line.startswith("exit-code "):
            if exit_code is not None:
                raise FinaltreeError(
                    "the stamped log carries its exit code line twice"
                )
            try:
                exit_code = int(line[len("exit-code "):].strip())
            except ValueError as exc:
                raise FinaltreeError(
                    "the exit code in the stamped log does not decode"
                ) from exc
            exit_code_line = line
        elif line.startswith("evidence-digest "):
            if evidence_digest is not None:
                raise FinaltreeError(
                    "the stamped log carries its evidence line twice"
                )
            evidence_digest = line[len("evidence-digest "):].strip()
    if fingerprint is None:
        raise FinaltreeError(
            "the stamped log lacks the fingerprint record a verifier reads"
        )
    if recorded_digest is None:
        raise FinaltreeError(
            "the stamped log lacks the fingerprint line its header promises"
        )
    if recorded_digest != fingerprint.digest:
        raise FinaltreeError(
            "the stamped log digest line disagrees with its fingerprint record"
        )
    if command is None:
        raise FinaltreeError(
            "the stamped log lacks the command line its header promises"
        )
    if exit_code is None:
        raise FinaltreeError(
            "the stamped log lacks the exit code line its header promises"
        )
    if evidence_digest is None:
        raise FinaltreeError(
            "the stamped log lacks the evidence line its header promises"
        )
    if not saw_end:
        raise FinaltreeError("the stamped log output section never closes")
    if (
        _evidence_digest(command_line, exit_code_line, raw_output_lines)
        != evidence_digest
    ):
        raise FinaltreeError(
            "the evidence digest in the stamped log disagrees with the "
            "command and exit code and output it covers"
        )
    return StampedLog(
        fingerprint=fingerprint,
        command=command,
        exit_code=exit_code,
        output_lines=tuple(output_lines),
        evidence_digest=evidence_digest,
    )


def run_and_stamp(root, command, log_path, generated_patterns=()):
    """Fingerprint the tree, run the gate command, write the stamped log.

    The fingerprint is computed before the command runs, from the
    tree the command will run on. Returns the exit code of the
    gate command, with a signal death reported as one.
    """
    root = Path(root)
    fingerprint = compute_fingerprint(root, generated_patterns)
    completed = subprocess.run(
        list(command),
        cwd=str(root),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    output = completed.stdout.decode("utf-8", "replace")
    exit_code = completed.returncode if completed.returncode >= 0 else 1
    text = render_stamped_log(fingerprint, command, exit_code, output)
    Path(log_path).write_text(text, encoding="utf-8")
    return exit_code


def compare_fingerprints(expected, actual):
    """Name every member that differs between two fingerprints."""
    differences = []
    if expected.commit != actual.commit:
        differences.append(
            f"commit moved from {expected.commit} to {actual.commit}"
        )
    expected_generated = {e.path: e.blob for e in expected.generated}
    actual_generated = {e.path: e.blob for e in actual.generated}
    generated_paths = set(expected_generated) | set(actual_generated)

    def generated_differs(path):
        return path in generated_paths and (
            (path in expected_generated) != (path in actual_generated)
            or (
                path in expected_generated
                and path in actual_generated
                and expected_generated[path] != actual_generated[path]
            )
        )

    expected_tracked = {}
    for entry in expected.tracked:
        expected_tracked.setdefault(entry.path, []).append(entry)
    actual_tracked = {}
    for entry in actual.tracked:
        actual_tracked.setdefault(entry.path, []).append(entry)
    for path in sorted(set(expected_tracked) | set(actual_tracked)):
        if generated_differs(path):
            # The generated section below names this path, and a
            # generated difference is never also an ordinary change.
            continue
        if path not in actual_tracked:
            differences.append(f"tracked file removed {path}")
            continue
        if path not in expected_tracked:
            differences.append(f"tracked file added {path}")
            continue
        expected_entries = expected_tracked[path]
        actual_entries = actual_tracked[path]
        if Counter(
            (e.blob, e.index_blobs) for e in expected_entries
        ) != Counter((e.blob, e.index_blobs) for e in actual_entries):
            if Counter(e.blob for e in expected_entries) != Counter(
                e.blob for e in actual_entries
            ):
                differences.append(f"file content changed {path}")
            if Counter(e.index_blobs for e in expected_entries) != Counter(
                e.index_blobs for e in actual_entries
            ):
                differences.append(f"index content changed {path}")

    expected_untracked = {e.path: e.blob for e in expected.untracked}
    actual_untracked = {e.path: e.blob for e in actual.untracked}
    for path in sorted(set(expected_untracked) & set(actual_untracked)):
        if expected_untracked[path] != actual_untracked[path]:
            differences.append(f"untracked content changed {path}")

    expected_dirty = {(e.path, e.kind) for e in expected.dirty}
    actual_dirty = {(e.path, e.kind) for e in actual.dirty}
    for path, kind in sorted(expected_dirty - actual_dirty):
        differences.append(f"dirty file cleaned {path} ({kind})")
    for path, kind in sorted(actual_dirty - expected_dirty):
        differences.append(f"dirty file added {path} ({kind})")

    for path in sorted(generated_paths):
        if path not in actual_generated:
            differences.append(f"generated artifact removed {path}")
        elif path not in expected_generated:
            differences.append(f"generated artifact added {path}")
        elif expected_generated[path] != actual_generated[path]:
            differences.append(f"generated artifact differs {path}")
    return differences


def verify_stamped_log(log_path, candidate_root, generated_patterns=None):
    """Verify a stamped log against a candidate tree."""
    stamped = read_stamped_log(log_path)
    patterns = (
        stamped.fingerprint.generated_patterns
        if generated_patterns is None
        else tuple(generated_patterns)
    )
    actual = compute_fingerprint(candidate_root, patterns)
    differences = compare_fingerprints(stamped.fingerprint, actual)
    return VerificationResult(
        matched=not differences,
        differences=tuple(differences),
        expected=stamped.fingerprint,
        actual=actual,
    )
