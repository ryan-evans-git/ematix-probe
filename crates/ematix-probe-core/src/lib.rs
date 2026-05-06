// ematix-probe-core
//
// Engine + adapter trait + assertion DSL grow phase by phase per
// docs/PI_PLAN.md.

pub const VERSION: &str = "0.1.0-dev";

pub fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[test]
    fn version_returns_dev_string() {
        assert_eq!(version(), "0.1.0-dev");
    }

    #[test]
    fn version_constant_matches_function() {
        assert_eq!(VERSION, version());
    }

    // S-2.1 — foundational types + DataAdapter trait. RED expects
    // `ProbePlan`, `Verdict`, `RunSummary`, `AdapterError`, and
    // `DataAdapter` to exist in scope. Fails to compile until S-2.1
    // GREEN lands them.
    #[tokio::test]
    async fn empty_plan_evaluates_pass() {
        struct StubAdapter;

        #[async_trait]
        impl DataAdapter for StubAdapter {
            async fn execute(&self, _plan: &ProbePlan) -> Result<RunSummary, AdapterError> {
                Ok(RunSummary {
                    verdict: Verdict::Pass,
                    assertions: vec![],
                })
            }
        }

        let plan = ProbePlan {
            schema: None,
            table: "any_table".to_string(),
            assertions: vec![],
        };
        let summary = StubAdapter.execute(&plan).await.unwrap();
        assert_eq!(summary.verdict, Verdict::Pass);
        assert!(summary.assertions.is_empty());
    }
}
