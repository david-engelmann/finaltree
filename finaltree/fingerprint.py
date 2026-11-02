"""Fingerprinting of the exact tree a gate ran on.

The fingerprint is computed from a local working tree with git
only. It holds the HEAD commit, the tracked files with the git
blob hash of their working tree content and the blobs the index
holds for them, the dirty files named by kind, the untracked
files with the hash of their content, and the generated
artifacts matching declared patterns. The members are retained
beside the digest so a mismatch can be named and not just
detected.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

from .errors import FinaltreeError

FORMAT = "finaltree-fingerprint/1"


@dataclass(frozen=True)
class TrackedFile:
    path: str
    blob: str | None
    index_blobs: tuple[str, ...] = ()


@dataclass(frozen=True)
class DirtyFile:
    path: str
    kind: str


@dataclass(frozen=True)
class GeneratedFile:
    path: str
    blob: str


@dataclass(frozen=True)
class UntrackedFile:
    path: str
    blob: str | None


def _tracked_entry(entry):
    return {
        "path": entry.path,
        "blob": entry.blob,
        "index_blobs": list(entry.index_blobs),
    }


def _dirty_entry(entry):
    return {"path": entry.path, "kind": entry.kind}


def _generated_entry(entry):
    return {"path": entry.path, "blob": entry.blob}


def _untracked_entry(entry):
    return {"path": entry.path, "blob": entry.blob}


def _members_payload(commit, tracked, dirty, generated, untracked):
    """The member record both the digest and the emitted dict shape."""
    return {
        "format": FORMAT,
        "commit": commit,
        "tracked": [
            _tracked_entry(entry)
            for entry in sorted(tracked, key=lambda entry: entry.path)
        ],
        "dirty": [
            _dirty_entry(entry)
            for entry in sorted(
                dirty, key=lambda entry: (entry.path, entry.kind)
            )
        ],
        "generated": [
            _generated_entry(entry)
            for entry in sorted(generated, key=lambda entry: entry.path)
        ],
        "untracked": [
            _untracked_entry(entry)
            for entry in sorted(untracked, key=lambda entry: entry.path)
        ],
    }


def canonical_serialization(commit, tracked, dirty, generated, untracked=()):
    """The one canonical serialization the digest is taken over."""
    payload = _members_payload(commit, tracked, dirty, generated, untracked)
    return json.dumps(
        payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )


def digest_of(commit, tracked, dirty, generated, untracked=()):
    text = canonical_serialization(commit, tracked, dirty, generated, untracked)
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _refuse_duplicate_members(fingerprint):
    for members in (
        fingerprint.tracked,
        fingerprint.generated,
        fingerprint.untracked,
    ):
        paths = [entry.path for entry in members]
        if len(set(paths)) != len(paths):
            raise FinaltreeError(
                "the fingerprint record carries the same member twice"
            )
    pairs = [(entry.path, entry.kind) for entry in fingerprint.dirty]
    if len(set(pairs)) != len(pairs):
        raise FinaltreeError(
            "the fingerprint record carries the same member twice"
        )


@dataclass(frozen=True)
class Fingerprint:
    commit: str
    tracked: tuple[TrackedFile, ...]
    dirty: tuple[DirtyFile, ...]
    generated: tuple[GeneratedFile, ...]
    untracked: tuple[UntrackedFile, ...] = ()
    generated_patterns: tuple[str, ...] = ()

    @property
    def digest(self):
        return digest_of(
            self.commit,
            self.tracked,
            self.dirty,
            self.generated,
            self.untracked,
        )

    def to_dict(self):
        data = _members_payload(
            self.commit,
            self.tracked,
            self.dirty,
            self.generated,
            self.untracked,
        )
        data["digest"] = self.digest
        data["generated_patterns"] = list(self.generated_patterns)
        return data

    @classmethod
    def from_dict(cls, data):
        try:
            tracked = tuple(
                TrackedFile(
                    path=entry["path"],
                    blob=entry["blob"],
                    index_blobs=tuple(entry.get("index_blobs", ())),
                )
                for entry in data["tracked"]
            )
            dirty = tuple(
                DirtyFile(path=entry["path"], kind=entry["kind"])
                for entry in data["dirty"]
            )
            generated = tuple(
                GeneratedFile(path=entry["path"], blob=entry["blob"])
                for entry in data["generated"]
            )
            untracked = tuple(
                UntrackedFile(path=entry["path"], blob=entry["blob"])
                for entry in data.get("untracked", ())
            )
            fingerprint = cls(
                commit=data["commit"],
                tracked=tracked,
                dirty=dirty,
                generated=generated,
                untracked=untracked,
                generated_patterns=tuple(data.get("generated_patterns", ())),
            )
        except (KeyError, TypeError, AttributeError) as exc:
            raise FinaltreeError(
                "the fingerprint record does not decode"
            ) from exc
        _refuse_duplicate_members(fingerprint)
        if "digest" not in data:
            raise FinaltreeError("the fingerprint record carries no digest")
        try:
            digest = fingerprint.digest
        except UnicodeEncodeError as exc:
            raise FinaltreeError(
                "the fingerprint record holds a path that is not "
                "valid UTF-8"
            ) from exc
        if data["digest"] != digest:
            raise FinaltreeError(
                "the recorded digest disagrees with the recorded members"
            )
        return fingerprint


def _git(root, *args):
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        capture_output=True,
    )
    if result.returncode != 0:
        reason = result.stderr.decode("utf-8", "replace").strip()
        raise FinaltreeError(f"git {' '.join(args)} failed. {reason}")
    return result.stdout


def _git_text(root, *args):
    return _git(root, *args).decode("utf-8", "surrogateescape")


def _require_utf8_path(relative):
    try:
        relative.encode("utf-8")
    except UnicodeEncodeError as exc:
        raise FinaltreeError(
            "the tree holds a path git spells with bytes that are not "
            "valid UTF-8, so the path cannot stand in a fingerprint"
        ) from exc


def _blob_hash(root, relative):
    result = subprocess.run(
        ["git", "-C", str(root), "hash-object", "--", relative],
        capture_output=True,
    )
    if result.returncode != 0:
        reason = result.stderr.decode("utf-8", "replace").strip()
        raise FinaltreeError(f"git hash-object failed for {relative}. {reason}")
    return result.stdout.decode("ascii").strip()


def _link_blob(root, relative):
    target = os.readlink(root / relative)
    result = subprocess.run(
        ["git", "-C", str(root), "hash-object", "--stdin"],
        input=os.fsencode(target),
        capture_output=True,
    )
    if result.returncode != 0:
        reason = result.stderr.decode("utf-8", "replace").strip()
        raise FinaltreeError(f"git hash-object failed for {relative}. {reason}")
    return result.stdout.decode("ascii").strip()


def _content_blob(root, relative):
    """The blob of what stands at a path, with a link read as itself."""
    full = root / relative
    if full.is_symlink():
        return _link_blob(root, relative)
    if full.is_file():
        return _blob_hash(root, relative)
    return None


def _glob_regex(pattern):
    """Translate one glob pattern, where a double star crosses folders."""
    out = []
    index = 0
    length = len(pattern)
    while index < length:
        char = pattern[index]
        if char == "*":
            if pattern[index:index + 2] == "**":
                if pattern[index:index + 3] == "**/":
                    out.append("(?:.*/)?")
                    index += 3
                else:
                    out.append(".*")
                    index += 2
            else:
                out.append("[^/]*")
                index += 1
        elif char == "?":
            out.append("[^/]")
            index += 1
        elif char == "[":
            end = index + 1
            if end < length and pattern[end] == "!":
                end += 1
            if end < length and pattern[end] == "]":
                end += 1
            while end < length and pattern[end] != "]":
                end += 1
            if end >= length:
                out.append(re.escape("["))
                index += 1
            else:
                stuff = pattern[index + 1:end]
                if stuff.startswith("!"):
                    stuff = "^" + stuff[1:]
                stuff = stuff.replace("\\", "\\\\")
                out.append(f"[{stuff}]")
                index = end + 1
        else:
            out.append(re.escape(char))
            index += 1
    return "".join(out)


def _matches_pattern(relative, patterns):
    return any(
        re.fullmatch(_glob_regex(pattern), relative) is not None
        for pattern in patterns
    )


def _os_walk(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(name for name in dirnames if name != ".git")
        yield dirpath, dirnames, sorted(filenames)


def _generated_files(root, patterns):
    if not patterns:
        return ()
    found = []
    for dirpath, dirnames, filenames in _os_walk(root):
        for filename in filenames:
            full = Path(dirpath) / filename
            relative = full.relative_to(root).as_posix()
            _require_utf8_path(relative)
            if _matches_pattern(relative, patterns):
                blob = _content_blob(root, relative)
                if blob is None:
                    raise FinaltreeError(
                        f"the generated artifact at {relative} cannot be read"
                    )
                found.append(GeneratedFile(path=relative, blob=blob))
    return tuple(sorted(found, key=lambda entry: entry.path))


def _index_entries(root):
    """The blobs the index holds, per path, in stage order."""
    entries = {}
    listing = _git(root, "ls-files", "--stage", "-z")
    for raw in listing.split(b"\0"):
        if not raw:
            continue
        meta, _, raw_path = raw.partition(b"\t")
        fields = meta.split(b" ")
        if len(fields) != 3:
            raise FinaltreeError("the index listing does not decode")
        try:
            stage = int(fields[2])
        except ValueError as exc:
            raise FinaltreeError("the index listing does not decode") from exc
        blob = fields[1].decode("ascii")
        relative = raw_path.decode("utf-8", "surrogateescape")
        _require_utf8_path(relative)
        entries.setdefault(relative, []).append((stage, blob))
    return {
        relative: tuple(
            blob for stage, blob in sorted(staged, key=lambda pair: pair[0])
        )
        for relative, staged in entries.items()
    }


_UNMERGED_CODES = {
    ("D", "D"),
    ("A", "A"),
    ("A", "U"),
    ("U", "A"),
    ("D", "U"),
    ("U", "D"),
    ("U", "U"),
}


def compute_fingerprint(root, generated_patterns=()):
    """Compute the fingerprint of a working tree with git only."""
    root = Path(root)
    if not root.is_dir():
        raise FinaltreeError(f"the tree at {root} does not stand")
    inside = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "--is-inside-work-tree"],
        capture_output=True,
    )
    if inside.returncode != 0 or inside.stdout.strip() != b"true":
        raise FinaltreeError(f"the tree at {root} is not a git working tree")
    top_level = _git_text(root, "rev-parse", "--show-toplevel").strip()
    if Path(top_level).resolve() != root.resolve():
        raise FinaltreeError(
            f"the tree at {root} is not the top level of its working "
            f"tree, which stands at {top_level}"
        )
    try:
        commit = _git_text(root, "rev-parse", "HEAD").strip()
    except FinaltreeError as exc:
        raise FinaltreeError(
            "the tree holds no commit, so there is no HEAD to fingerprint"
        ) from exc
    patterns = tuple(generated_patterns)

    index = _index_entries(root)
    tracked = [
        TrackedFile(
            path=relative,
            blob=_content_blob(root, relative),
            index_blobs=index[relative],
        )
        for relative in sorted(index)
    ]

    generated = _generated_files(root, patterns)
    generated_paths = {entry.path for entry in generated}

    dirty = set()
    untracked = []
    status = _git(
        root,
        "status",
        "--porcelain=v1",
        "-z",
        "--no-renames",
        "--untracked-files=all",
    )
    for raw in status.split(b"\0"):
        if not raw:
            continue
        entry = raw.decode("utf-8", "surrogateescape")
        code_x, code_y, relative = entry[0], entry[1], entry[3:]
        _require_utf8_path(relative)
        if code_x == "?" and code_y == "?":
            if relative not in generated_paths:
                dirty.add(DirtyFile(path=relative, kind="untracked"))
                untracked.append(
                    UntrackedFile(
                        path=relative, blob=_content_blob(root, relative)
                    )
                )
            continue
        if (code_x, code_y) in _UNMERGED_CODES:
            dirty.add(DirtyFile(path=relative, kind="conflicted"))
            continue
        if code_x not in " ?":
            dirty.add(DirtyFile(path=relative, kind="staged"))
        if code_y not in " ?":
            dirty.add(DirtyFile(path=relative, kind="modified"))

    return Fingerprint(
        commit=commit,
        tracked=tuple(tracked),
        dirty=tuple(sorted(dirty, key=lambda e: (e.path, e.kind))),
        generated=generated,
        untracked=tuple(sorted(untracked, key=lambda e: e.path)),
        generated_patterns=patterns,
    )
