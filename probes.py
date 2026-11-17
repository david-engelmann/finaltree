#!/usr/bin/env python3
"""Slice 9 probes against the base binary (finaltree at 8b59554b).

Probe A: the gap. One fixture repo; vary an untracked file's
mode across outside-family standings and confirm the digest
stands identical across all of them and to the plain tree.
Also a tracked file at 0400, a generated artifact at 0777 and
an ignored file at 0400.

Probe B: the family. Create files at creation modes 0666 and
0777 under five umasks and read back the standings.

Probe C: checkout standings. A repo with one plain tracked
file and one executable tracked file, cloned under the five
umasks, both files' standings stated each time.
"""
import json
import os
import shutil
import subprocess
import sys

BIN = sys.argv[1]
ROOT = sys.argv[2]

ENV = dict(
    os.environ,
    GIT_AUTHOR_NAME="Finaltree Probe",
    GIT_AUTHOR_EMAIL="probe@example.invalid",
    GIT_COMMITTER_NAME="Finaltree Probe",
    GIT_COMMITTER_EMAIL="probe@example.invalid",
    GIT_AUTHOR_DATE="2005-04-07T22:13:13+00:00",
    GIT_COMMITTER_DATE="2005-04-07T22:13:13+00:00",
)


def run(args, cwd, **kw):
    return subprocess.run(args, cwd=cwd, env=ENV, capture_output=True, text=True, **kw)


def git(repo, *args):
    out = run(["git", "-C", repo, *args], cwd=repo)
    assert out.returncode == 0, (args, out.stderr)
    return out.stdout


def digest(repo, patterns=()):
    args = [BIN, "fingerprint", repo]
    for pattern in patterns:
        args += ["--generated", pattern]
    out = run(args, cwd=repo)
    assert out.returncode == 0, out.stderr
    record = json.loads(out.stdout)
    return record


def mode_of(path):
    return oct(os.stat(path).st_mode & 0o7777)


def fresh_repo(path):
    os.makedirs(path)
    git(path, "init", "-q", "-b", "main")
    with open(os.path.join(path, "tracked.txt"), "w") as handle:
        handle.write("tracked\n")
    git(path, "add", ".")
    git(path, "commit", "-q", "-m", "initial")
    return path


def probe_a():
    base = os.path.join(ROOT, "probe-a")
    shutil.rmtree(base, ignore_errors=True)
    repo = fresh_repo(base)
    plain = digest(repo)
    print("A plain tree digest", plain["digest"][:16], "members", sorted(plain.keys()))
    results = {}
    loose = os.path.join(repo, "loose.txt")
    with open(loose, "w") as handle:
        handle.write("loose\n")
    for mode in (0o777, 0o400, 0o711, 0o500, 0o4755):
        os.chmod(loose, mode)
        record = digest(repo)
        results[oct(mode)] = record["digest"][:16]
        print("A untracked", oct(mode), record["digest"][:16], "file_modes" in record)
    os.chmod(loose, 0o644)
    os.remove(loose)
    os.chmod(os.path.join(repo, "tracked.txt"), 0o400)
    record = digest(repo)
    print("A tracked 0400", record["digest"][:16], "file_modes" in record)
    os.chmod(os.path.join(repo, "tracked.txt"), 0o644)
    artifact = os.path.join(repo, "artifact.bin")
    with open(artifact, "w") as handle:
        handle.write("artifact\n")
    os.chmod(artifact, 0o777)
    record = digest(repo, ["*.bin"])
    print("A generated 0777", record["digest"][:16], "file_modes" in record)
    with open(os.path.join(repo, ".gitignore"), "w") as handle:
        handle.write("secret.txt\n")
    secret = os.path.join(repo, "secret.txt")
    with open(secret, "w") as handle:
        handle.write("secret\n")
    os.chmod(secret, 0o400)
    record = digest(repo)
    print("A ignored 0400", record["digest"][:16], "file_modes" in record)
    print("A all digests equal plain:", all(v == plain["digest"][:16] for v in results.values()))


def probe_b():
    base = os.path.join(ROOT, "probe-b")
    shutil.rmtree(base, ignore_errors=True)
    os.makedirs(base)
    for umask in (0o022, 0o002, 0o027, 0o077, 0o007):
        old = os.umask(umask)
        try:
            plain = os.path.join(base, "plain-%03o" % umask)
            fd = os.open(plain, os.O_CREAT | os.O_WRONLY, 0o666)
            os.close(fd)
            exe = os.path.join(base, "exe-%03o" % umask)
            fd = os.open(exe, os.O_CREAT | os.O_WRONLY, 0o777)
            os.close(fd)
        finally:
            os.umask(old)
        print("B umask %03o plain %s exec %s" % (umask, mode_of(plain), mode_of(exe)))


def probe_c():
    base = os.path.join(ROOT, "probe-c")
    shutil.rmtree(base, ignore_errors=True)
    src = fresh_repo(os.path.join(base, "src"))
    tool = os.path.join(src, "tool.sh")
    with open(tool, "w") as handle:
        handle.write("#!/bin/sh\ntrue\n")
    os.chmod(tool, 0o755)
    git(src, "add", ".")
    git(src, "commit", "-q", "-m", "tool")
    for umask in (0o022, 0o002, 0o027, 0o077, 0o007):
        dest = os.path.join(base, "clone-%03o" % umask)
        old = os.umask(umask)
        try:
            out = run(["git", "clone", "-q", src, dest], cwd=base)
            assert out.returncode == 0, out.stderr
        finally:
            os.umask(old)
        print(
            "C umask %03o tracked %s tool %s"
            % (umask, mode_of(os.path.join(dest, "tracked.txt")), mode_of(os.path.join(dest, "tool.sh")))
        )


if __name__ == "__main__":
    probe_a()
    probe_b()
    probe_c()
