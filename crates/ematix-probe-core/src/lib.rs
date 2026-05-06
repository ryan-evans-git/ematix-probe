//! ematix-probe-core
//!
//! Engine + adapter trait + assertion DSL grow phase by phase per
//! docs/PI_PLAN.md. Top-level surface stays small: the version
//! constants, the `engine` and `adapters` module trees, and a
//! curated re-export of the types most callers need.

pub mod adapters;
pub mod engine;

pub use adapters::data::{AdapterError, DataAdapter};
pub use engine::data::{Assertion, AssertionResult, ProbePlan, RunSummary, Verdict};

pub const VERSION: &str = "0.1.0-dev";

pub fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_dev_string() {
        assert_eq!(version(), "0.1.0-dev");
    }

    #[test]
    fn version_constant_matches_function() {
        assert_eq!(VERSION, version());
    }
}
