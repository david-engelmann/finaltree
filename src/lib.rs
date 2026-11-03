//! finaltree. Provenance for gate results.
//!
//! Fingerprints the exact tree a gate ran on and binds that
//! fingerprint to the evidence the gate reports. The crate is
//! the Rust port of the Python package of the same name, slice 1
//! covering the fingerprint reading, the record and its digest,
//! the stamped log, and stamping and verification end to end.

pub mod errors;
pub mod fingerprint;
pub mod glob;
pub mod intparse;
pub mod jsonvalue;
pub mod render;
pub mod sha256;
pub mod stamping;

pub use errors::FinaltreeError;
pub use fingerprint::{
    compute_fingerprint, DirtyFile, Fingerprint, GeneratedFile, TrackedFile, UntrackedFile, FORMAT,
};
pub use stamping::{
    compare_fingerprints, read_stamped_log, run_and_stamp, verify_stamped_log, StampedLog,
    VerificationResult, STAMP_FORMAT,
};
