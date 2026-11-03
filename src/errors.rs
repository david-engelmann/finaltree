//! The error type mirroring the Python package error class.

use std::fmt;

/// A clear refusal from finaltree, carrying a readable reason.
///
/// Mirrors the Python `FinaltreeError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinaltreeError {
    message: String,
}

impl FinaltreeError {
    pub fn new(message: impl Into<String>) -> Self {
        FinaltreeError {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FinaltreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FinaltreeError {}
