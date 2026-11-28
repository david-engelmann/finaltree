# Finaltree Operator Guide

Finaltree is an offline, zero-dependency Rust CLI and library that proves gate provenance. Teams routinely quote test counts or green verdicts from a log produced before a final fixup commit, with unstaged edits sitting in the working tree, or with stale generated code on disk. Finaltree computes a complete SHA-256 fingerprint of the working tree, prefixes every gate log line with that digest, seals the run under an evidence digest, and verifies that the merge candidate matches byte for byte.

## Installation

### Build from Git with Cargo

Requires Rust `1.80` or newer and `git` on `PATH` (Linux or macOS).

```sh
cargo install --git https://github.com/david-engelmann/finaltree --locked
```

### Prebuilt Release Archives

On Linux and macOS (`x86_64` and `aarch64`), install verified prebuilt archives via `install.sh` or `cargo-binstall` once release assets are attached (`v0.21.0` and `v0.22.0` onward).

```sh
curl -fsSL https://raw.githubusercontent.com/david-engelmann/finaltree/main/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
```

```sh
cargo binstall --git https://github.com/david-engelmann/finaltree finaltree
```

## All Three CLI Doors

```text
finaltree fingerprint [--generated PATTERN ...] ROOT
finaltree stamp ROOT --log PATH [--generated PATTERN ...] [COMMAND ...]
finaltree verify LOG ROOT [--generated PATTERN ...]
```

### Exit Codes

- `fingerprint` prints canonical `finaltree-fingerprint/1` JSON to standard output and exits `0` (or exits `2` on usage or refusal errors).
- `stamp` fingerprints `ROOT`, runs `COMMAND` with `ROOT` as its working directory, writes `finaltree-stamped-log/1` to `--log PATH`, and exits with the gate command's own exit code (`0` when the gate passed, non-zero when the gate failed, or `2` on usage/refusal before running).
- `verify` recomputes the fingerprint of `ROOT`, validates the stamped log's `evidence-digest` seal, prints `match` (exit `0`) when identical, prints `mismatch` followed by every differing member (exit `1`) when the tree changed, and exits `2` if `LOG` is malformed or tampered.

## Operational Best Practices

### 1. Pass `--` Before the Gate Command

Always separate `finaltree stamp` options from the gate command with `--` so flags such as `--locked`, `--release`, or `-q` pass straight to your gate binary.

```sh
finaltree stamp . --log /tmp/gate.stamped.log -- cargo test --locked
```

### 2. Keep `--log` and Scratch Build Outputs Outside `ROOT`

A stamped log belongs outside the tree it stamps, because writing `--log` inside `ROOT` creates a new untracked file that changes `ROOT` on subsequent `verify` runs. Likewise, because `stamp` fingerprints `ROOT` before running the gate command, direct compiler or test-runner scratch caches outside `ROOT` (for example `CARGO_TARGET_DIR=/tmp/ci-target` or `PYTEST_CACHE_DIR=/tmp/pytest-cache`) so gate execution never writes new files into `ROOT` (`--generated` is for pre-existing generated artifacts already standing in `ROOT` before `stamp` runs).

### 3. Check Both `finaltree verify` and `exit-code 0` Before Merging

`finaltree verify` proves *tree provenance* and *log integrity* (even a failing gate run produces a valid stamped log recording its non-zero `exit-code`). In merge gates, verify both tree provenance and a zero gate exit code.

```sh
finaltree verify /tmp/gate.stamped.log .
grep -qx 'exit-code 0' /tmp/gate.stamped.log
```

### 4. Sanitize `GIT_DIR` Inside Git Hooks

When invoking `finaltree` inside a `pre-commit` or `pre-push` hook, Git exports `GIT_DIR` and `GIT_WORK_TREE`, which can redirect child `git` invocations if `ROOT` contains submodules or nested trees. Clear inherited repository-location variables at the hook boundary.

```sh
env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE finaltree stamp . --log /tmp/pre-push.stamped.log -- cargo test --locked
```

## Worked Example

```sh
(
  set -eu
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT

  cp -R examples/stamp-and-verify "$WORK/demo"
  cd "$WORK/demo"
  sh make-tree.sh

  env -u GIT_DIR -u GIT_WORK_TREE GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    finaltree stamp tree --log stamped-run.log -- sh gate.sh

  env -u GIT_DIR -u GIT_WORK_TREE GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    finaltree verify stamped-run.log tree
)
```

## GitHub Actions Provenance Workflow

Stamp the test suite in one step (writing both `--log` and `CARGO_TARGET_DIR` outside the repository checkout) and verify provenance against the checkout before uploading release or gate evidence.

```yaml
name: stamped-gate

on:
  pull_request:

permissions:
  contents: read

jobs:
  provenance:
    runs-on: ubuntu-latest
    env:
      CARGO_TARGET_DIR: /tmp/cargo-gate-target
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive
      - name: Install finaltree
        run: cargo install --git https://github.com/david-engelmann/finaltree --locked
      - name: Stamp test suite execution
        run: finaltree stamp . --log /tmp/gate.stamped.log -- cargo test --locked
      - name: Verify merge-candidate provenance and zero exit code
        run: |
          finaltree verify /tmp/gate.stamped.log .
          grep -qx 'exit-code 0' /tmp/gate.stamped.log
      - uses: actions/upload-artifact@v4
        with:
          name: stamped-gate-log
          path: /tmp/gate.stamped.log
```
