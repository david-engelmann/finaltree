"""Synthetic repo factory for the finaltree tests.

Every fixture is a synthetic git repo built under the test
scratch folder. Nothing here touches a real tree.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

TEST_ENV = {
    **os.environ,
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_AUTHOR_NAME": "Finaltree Tests",
    "GIT_AUTHOR_EMAIL": "tests@example.invalid",
    "GIT_COMMITTER_NAME": "Finaltree Tests",
    "GIT_COMMITTER_EMAIL": "tests@example.invalid",
}


def git(root, *args):
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        capture_output=True,
        text=True,
        env=TEST_ENV,
    )
    if result.returncode != 0:
        raise AssertionError(f"git {' '.join(args)} failed. {result.stderr}")
    return result.stdout.strip()


def write(root, relative, text):
    path = Path(root) / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def make_repo(tmp_path, name="repo"):
    root = Path(tmp_path) / name
    root.mkdir(parents=True)
    subprocess.run(
        ["git", "init", "-q", "-b", "main", str(root)],
        check=True,
        capture_output=True,
        env=TEST_ENV,
    )
    write(root, "a.txt", "alpha\n")
    write(root, "b.txt", "beta\n")
    git(root, "add", ".")
    git(root, "commit", "-q", "-m", "initial")
    return root


def commit_all(root, message="change"):
    git(root, "add", ".")
    git(root, "commit", "-q", "-m", message)
