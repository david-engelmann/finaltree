# Finaltree Wire Formats

Finaltree emits two deterministic formats, `finaltree-fingerprint/1` JSON and `finaltree-stamped-log/1` line-stamped logs, plus structured `verify` difference lines.

## 1. `finaltree-fingerprint/1`

Printed by `finaltree fingerprint` and embedded on the `fingerprint-json` header line of every stamped log.

```json
{
  "commit": "e8083ec406a6427fd5898c16b56e80d503909fd7",
  "digest": "e206b3556bbadf8893db16e57d5355885a9c01db9334cabf31fd1a80c1e7fb6b",
  "dirty": [
    {"kind": "modified", "path": "a.txt"},
    {"kind": "staged", "path": "b.txt"},
    {"kind": "untracked", "path": "notes.txt"}
  ],
  "format": "finaltree-fingerprint/1",
  "generated": [
    {"blob": "aa69f30c855e87859d78c03e48a5a64cd29cc90e", "path": "build/out.bin"}
  ],
  "generated_patterns": ["build/*"],
  "tracked": [
    {
      "blob": "ec20141f0267805d3db1349c4701ed1f84f3ee61",
      "index_blobs": ["4a58007052a65fbc2fc3f910f2855f45a4058e74"],
      "path": "a.txt"
    }
  ],
  "untracked": [
    {"blob": "af17f6cc87e4d5e4adec0018cbb73d3e2bd008c8", "path": "notes.txt"}
  ]
}
```

### Bound Members

- `commit` records `HEAD` (`null` on an unborn branch).
- `tracked` binds both the working-tree Git blob hash (`blob`, or `null` when deleted in working tree) and staged index stage blobs (`index_blobs`) for every tracked path.
- `untracked` binds Git blob hashes of untracked files not claimed by `--generated`.
- `dirty` classifies working-tree deviations as `staged`, `modified`, `untracked`, or `conflicted`.
- `generated` and `generated_patterns` bind pre-existing files matching `--generated PATTERN` globs separately from ordinary untracked files.
- Optional extended members appear whenever their corresponding entries stand in `ROOT`, including `submodules` (index gitlink, checked-out `HEAD`, and recursive `finaltree` digest), `modes` (executable `100755` and symbolic-link `120000` modes on untracked and generated files), `ignored` (ignored files present in `ROOT`), `embedded` (nested non-submodule Git repositories), `empty_dirs`, `special`, `dir_modes`, `file_modes`, and `dot_git`.
- `digest` holds the 64-hex SHA-256 digest over the canonical serialization of all fingerprint members.

## 2. `finaltree-stamped-log/1`

Written to `--log PATH` by `finaltree stamp`.

```text
finaltree-stamped-log/1
fingerprint ea30bdcfcff91b034c09dde045eed59ea966b810900a27e3a3f2f359494dafb8
fingerprint-json {"commit": "43984758cf79f17bdfbb4c1e11011c60f418cdd6", ...}
command-json ["sh", "gate.sh"]
exit-code 0
evidence-digest 82e0becf6b7142cb55cfe420bdaf03602d29b7e08617ad189525248464c30910
begin-output
ea30bdcfcff91b034c09dde045eed59ea966b810900a27e3a3f2f359494dafb8 | 12 tests ran
ea30bdcfcff91b034c09dde045eed59ea966b810900a27e3a3f2f359494dafb8 | 12 passed
end-output
```

### Integrity Rules

- Every captured output line between `begin-output` and `end-output` is prefixed with `<64-hex-fingerprint> | `, so excerpted lines still carry the exact tree digest they came from.
- `evidence-digest` is the SHA-256 digest binding the fingerprint, `command-json`, `exit-code`, and raw output bytes. Any edit to a line, header, or exit code causes `finaltree verify` to refuse the log with exit code `2`.

## 3. `verify` Output Grammar

- `match` (exit `0`) when the candidate tree's fingerprint matches the stamped log.
- `mismatch` (exit `1`) followed by deterministic difference lines such as `commit moved from <old> to <new>`, `file content changed <path>`, `index content changed <path>`, `untracked content changed <path>`, `dirty file added <path> (<kind>)`, `generated artifact ...`, `submodule ...`, or `file mode ...`.
