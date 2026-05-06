//! S-2.1 — basic public-API checks for the data-probe foundation
//! types. Lives as an integration test (vs. inline unit test) so
//! the module path matches what real callers will see.

use async_trait::async_trait;
use ematix_probe_core::{AdapterError, DataAdapter, ProbePlan, RunSummary, Verdict};

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

#[tokio::test]
async fn empty_plan_evaluates_pass() {
    let plan = ProbePlan {
        schema: None,
        table: "any_table".to_string(),
        assertions: vec![],
    };
    let summary = StubAdapter.execute(&plan).await.unwrap();
    assert_eq!(summary.verdict, Verdict::Pass);
    assert!(summary.assertions.is_empty());
}
