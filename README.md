# finaltree

Provenance for gate results, fingerprinting the exact tree under test, commit, dirty files, and generated artifacts, binding that fingerprint to every log line and count the gate reports, and verifying it matches the merge candidate before anyone says verified.

## What this is

Gate results get quoted from logs inherited from a superseded tree, and counts get reported from builds that never included the final commit, a failure careful teams currently defend against by checking log provenance by hand. Finaltree makes that rule a tool. It fingerprints the exact tree a gate ran on and binds the fingerprint to everything the gate reports, so a verdict can always be traced to the code it was measured on. The boundary with its siblings is one sentence. Finaltree asks what it ran on, where deadgate asks whether it ran and blindgate asks what it read.

## Status

- Closed rounds stand at 2 of 100
- Slice 1 is the seed concept and stands as the concept this repo was created with
- Slice 2 is built and proven on synthetic fixture repos only
- Slice 2 computes a fingerprint of a working tree holding the HEAD commit, the tracked files with the git blob hash of their working tree content and of the content staged for them in the index, the untracked files with the git blob hash of their content, the dirty files named as staged or modified or untracked or conflicted, and the generated artifacts matching declared patterns, all under one SHA-256 digest over a canonical serialization, with the members retained beside the digest
- Slice 2 runs a gate command and writes a stamped log whose header carries the fingerprint and an evidence digest over the command and the exit code and the output as written, and whose every output line is prefixed with the fingerprint digest
- Slice 2 verifies a stamped log against a candidate tree and answers match or mismatch, naming every member that differs, with a generated artifact difference named as generated and never as an ordinary change, a difference in staged content named as an index difference, and a difference in untracked content named as an untracked difference
- The Round 1 review fixes are recorded in the contract and bind the index and the untracked content into the fingerprint, cover the stamped log with its evidence digest, and refuse any stamped log that arrives short of one of its claims
- Ignored files that match no declared generated pattern stand outside the fingerprint entirely, and a stamped log belongs outside the tree it stamps, because a log written into the tree it stamps would change the tree it names
- Slice 2 is the Round 1 work increment and is built on a local branch, and the round closes only after audit and two-axis review and red team and owner approval
- Round 2 is the first Rust port slice and is built on a local branch, carrying the fingerprint reading and the record and the stamped log and stamping and verification in a Rust crate proven against recordings taken from the landed Python, and its review fix pass is built on the same branch answering the red team and the two axis review in full with the conformance census asserted in every section, and the round closes only after audit and two-axis review and red team and owner approval
- AGENTS.md stands in this repo, the canonical template of the program with this repo's annex below its marked line
- Part of the impatient programming portfolio, in the Gate Truth arc

## Licence

MIT. See LICENSE.
