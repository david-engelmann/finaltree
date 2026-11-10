# Examples

Two worked examples stand here. Each one builds a small synthetic git tree with a script, runs finaltree on it, and keeps the captured output of every run beside the inputs. The git environment in each script is pinned the way the repo test fixtures pin it, so the commit each script makes is the same commit on every run and the captured outputs reproduce byte for byte.

## Fingerprint

The fingerprint example builds a tree holding three committed files, one of them then modified in the working tree and one of them carrying a staged change, one untracked file, and one generated artifact under build. Run these commands from the fingerprint folder.

```
sh make-tree.sh
finaltree fingerprint --generated 'build/*' tree
```

The run exits 0 and prints the fingerprint record. The printed record stands captured in fingerprint.json in the same folder. The record names the modified file and the staged file and the untracked file as dirty, lists the artifact under build as generated, and carries one digest over the whole tree.

## Stamp and verify

The stamp and verify example builds a clean tree holding two text files and a small gate script. The stamp run runs the gate script inside the tree and writes the stamped log beside the tree, outside the tree it stamps. Verify then reads the log against the tree unchanged and answers match, and against the tree with one file changed and answers mismatch, naming the changed file. Run these commands from the stamp-and-verify folder.

```
sh make-tree.sh
finaltree stamp tree --log stamped.log -- sh gate.sh
finaltree verify stamped.log tree
printf 'beta changed\n' > tree/b.txt
finaltree verify stamped.log tree
```

The stamp run exits 0. The stamped log stands in stamped.log. The two verify answers stand in verify-match.txt and verify-mismatch.txt. The first verify run exits 0 and the second exits 1. The change to b.txt is the last step, and a tree built again from the script stands clean and matches the log again.
