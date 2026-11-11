#!/bin/sh
# Build the fingerprint example tree beside this script.
# The tree is a small git repo holding three committed files,
# one of them then modified in the working tree and one of
# them carrying a staged change, one untracked file, and one
# generated artifact under build. The git environment is
# pinned the way the repo test fixtures pin it, so the commit
# the script makes is the same commit on every run.
set -eu
cd "$(dirname "$0")"
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_AUTHOR_NAME="Finaltree Tests"
export GIT_AUTHOR_EMAIL="tests@example.invalid"
export GIT_COMMITTER_NAME="Finaltree Tests"
export GIT_COMMITTER_EMAIL="tests@example.invalid"
export GIT_AUTHOR_DATE="2024-01-01T00:00:00+00:00"
export GIT_COMMITTER_DATE="2024-01-01T00:00:00+00:00"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR GIT_NAMESPACE GIT_PREFIX
rm -rf tree
mkdir -p tree/src tree/build
printf 'alpha\n' > tree/a.txt
printf 'beta\n' > tree/b.txt
printf 'print("hello")\n' > tree/src/app.py
git -C tree init -q -b main
git -C tree add -A
git -C tree commit -qm initial
printf 'alpha changed\n' > tree/a.txt
printf 'beta staged\n' > tree/b.txt
git -C tree add b.txt
printf 'gamma\n' > tree/notes.txt
printf 'artifact\n' > tree/build/out.bin
