# finaltree

Provenance for gate results, fingerprinting the exact tree under test, commit, dirty files, and generated artifacts, binding that fingerprint to every log line and count the gate reports, and verifying it matches the merge candidate before anyone says verified.

## What this is

Gate results get quoted from logs inherited from a superseded tree, and counts get reported from builds that never included the final commit, a failure careful teams currently defend against by checking log provenance by hand. Finaltree makes that rule a tool. It fingerprints the exact tree a gate ran on and binds the fingerprint to everything the gate reports, so a verdict can always be traced to the code it was measured on. The boundary with its siblings is one sentence. Finaltree asks what it ran on, where deadgate asks whether it ran and blindgate asks what it read.

## Status

Seeded in October 2026. The build has not started. Part of the impatient programming portfolio, in the Gate Truth arc.

## Licence

MIT. See LICENSE.
