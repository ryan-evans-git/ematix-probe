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

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_workspace_version() {
        // Sourced from Cargo at build time, so it tracks the
        // [workspace.package] version automatically. The check
        // here is "non-empty + matches the cargo env" to keep
        // the assertion stable across version bumps.
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }

    #[test]
    fn version_constant_matches_function() {
        assert_eq!(VERSION, version());
    }
}
