// ematix-probe-core
//
// Phase 0 contains only the version constant. Engine, adapters, and
// assertion DSL land in subsequent phases per docs/PI_PLAN.md.

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
