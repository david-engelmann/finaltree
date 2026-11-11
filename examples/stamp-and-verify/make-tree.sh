#!/bin/sh
# Build the stamp and verify example tree beside this script.
# The tree is a small git repo holding two committed text
# files and the gate script the stamp run runs, standing
# clean at one commit. The git environment is pinned the way
# the repo test fixtures pin it, so the commit the script
# makes is the same commit on every run.
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
mkdir -p tree
printf 'alpha\n' > tree/a.txt
printf 'beta\n' > tree/b.txt
cp gate.sh tree/gate.sh
chmod +x tree/gate.sh
git -C tree init -q -b main
git -C tree add -A
git -C tree commit -qm initial
