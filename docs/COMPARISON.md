# Finaltree Comparison

Recording `git rev-parse HEAD` at the top of a test log fails whenever a developer runs local gates with unstaged edits, staged index changes, untracked helper scripts, dirty submodules, or prebuilt generated files. Finaltree binds the entire working-tree state to the gate output.

## Capability Matrix

| Approach | Captures `HEAD`? | Captures unstaged + staged index blobs? | Captures untracked files & modes? | Captures recursive submodules & `--generated` globs? | Seals gate output lines against tampering? |
| --- | --- | --- | --- | --- | --- |
| **finaltree** | Yes | Yes (`blob` + `index_blobs`) | Yes (`100755` & `120000`) | Yes | Yes (`evidence-digest` + per-line prefix) |
| `git rev-parse HEAD` in CI script | Yes | No | No | No | No |
| `git diff --quiet` check | Partial | Misses untracked files | No | Partial | No |
| Build-system content hashes (Bazel, Nix) | Sandboxed inputs | Declared inputs only | Excludes undeclared files | Requires full hermetic rewrite | Build cache only |
| **deadgate** / **blindgate** (sibling Gate Truth tools) | N/A | N/A | N/A | N/A | Answer *whether* it ran and *what* it read |

## Boundary Within the Gate Truth Suite

- **Deadgate asks whether it ran.** Reconciles branch protection, workflow triggers, and run history.
- **Finaltree asks what it ran on.** Binds the exact working tree, index, untracked files, modes, generated artifacts, and submodules to every line of gate output and verifies match against the merge candidate.
- **Blindgate asks what it read.** Traces Linux syscalls under `strace` to verify which files the gate process actually opened during execution.
