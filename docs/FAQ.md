# Finaltree FAQ

## Why must `--log` live outside the stamped directory?

When `finaltree stamp ROOT --log PATH` executes, it fingerprints `ROOT` before running the gate command and writes the stamped log to `PATH`. If `PATH` resides inside `ROOT`, the newly created log file becomes an untracked file inside `ROOT`, so `finaltree verify` immediately afterward accurately reports `mismatch` (`dirty file added <path> (untracked)`). Always place `--log` in a sibling or temporary directory outside `ROOT`.

## Does `finaltree verify` fail when the stamped gate command exited non-zero?

`finaltree verify` verifies **provenance and unkeyed integrity**, checking that the log was not accidentally truncated or edited and that `ROOT` matches the exact tree the command ran on. Even a failing test run (`exit-code 1`) has valid provenance when investigating a failure. When gating a merge, join both checks as `finaltree verify stamped.log . && grep -qx 'exit-code 0' stamped.log`.

## Why did `verify` report a mismatch after `cargo test` or `pytest` wrote to a build folder?

If a gate command creates or modifies tracked, untracked, `--generated`, or gitignored files inside `ROOT` during its run (Finaltree binds `.gitignore`-ignored files under the `ignored` fingerprint member alongside tracked and untracked files), the post-run working tree differs from the pre-run tree that `stamp` fingerprinted. Direct build outputs outside `ROOT` (for example `CARGO_TARGET_DIR=/tmp/gate-target`) so `ROOT` stays immutable across the gate run.

## Do I need to pass `--generated` on both `stamp` and `verify`?

When `stamp` runs with `--generated 'build/*'`, the declared patterns are recorded inside the stamped log's `fingerprint-json`. On `verify`, Finaltree uses the stamped log's recorded patterns by default unless `--generated` is also passed on `verify` (in which case the patterns must agree).

## Does Finaltree modify my Git index or working tree?

Finaltree's Git inspection operations never stage, commit, reset, or delete files in `ROOT`. On `stamp`, Finaltree writes the stamped log to `--log PATH` (which should live outside `ROOT`) and runs the caller-supplied gate command with `ROOT` as its working directory.
