"""finaltree. Provenance for gate results.

Fingerprints the exact tree a gate ran on and binds that
fingerprint to the evidence the gate reports.
"""

from .errors import FinaltreeError
from .fingerprint import (
    FORMAT,
    DirtyFile,
    Fingerprint,
    GeneratedFile,
    TrackedFile,
    UntrackedFile,
    compute_fingerprint,
)
from .stamping import (
    STAMP_FORMAT,
    StampedLog,
    VerificationResult,
    compare_fingerprints,
    read_stamped_log,
    run_and_stamp,
    verify_stamped_log,
)

__all__ = [
    "FORMAT",
    "STAMP_FORMAT",
    "DirtyFile",
    "Fingerprint",
    "FinaltreeError",
    "GeneratedFile",
    "StampedLog",
    "TrackedFile",
    "UntrackedFile",
    "VerificationResult",
    "compare_fingerprints",
    "compute_fingerprint",
    "read_stamped_log",
    "run_and_stamp",
    "verify_stamped_log",
]
