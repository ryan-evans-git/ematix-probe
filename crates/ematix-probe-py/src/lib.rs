//! Python bindings for ematix-probe.
//!
//! S-2.6 lands the `ProbePlan` / `Assertion` types + the assertion
//! factory functions that the Python `@probe.data` decorator uses
//! to assemble plans. S-2.7 adds end-to-end probe execution
//! (`run_postgres_probe`) that the Python-side `DataProbe.run`
//! routes through.

use std::path::PathBuf;

use ematix_probe_core as core;
use ematix_probe_core::adapters::data::duckdb::DuckDbAdapter;
use ematix_probe_core::adapters::data::parquet::ParquetAdapter;
use ematix_probe_core::adapters::data::postgres::PostgresAdapter;
use ematix_probe_core::adapters::data::s3_parquet::S3ParquetAdapter;
use ematix_probe_core::DataAdapter;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Python wrapper around `core::Assertion`. Constructed via the
/// module-level factory functions (`assertion_not_null`, etc.) and
/// passed back into `ProbePlan(...)`.
///
/// `from_py_object` opts in to the `FromPyObject` derive so
/// `Vec<PyAssertion>` arguments (used by `ProbePlan::new`) can be
/// converted from Python lists. pyo3 0.28+ requires this opt-in
/// for `#[pyclass]` types that also derive `Clone`.
#[pyclass(
    name = "Assertion",
    module = "ematix_probe._core",
    frozen,
    from_py_object
)]
#[derive(Clone)]
struct PyAssertion {
    inner: core::Assertion,
}

#[pymethods]
impl PyAssertion {
    fn __repr__(&self) -> String {
        format!("Assertion({:?})", self.inner)
    }

    /// Returns "not_null" / "unique" / "between" — useful for
    /// debugging and for the Python-side decorator's introspection.
    /// New variants added to `core::Assertion` will be reported as
    /// `"unknown"` until the binding catches up; the wildcard arm
    /// is required because the core enum is `#[non_exhaustive]`.
    fn kind(&self) -> &'static str {
        match &self.inner {
            core::Assertion::NotNull { .. } => "not_null",
            core::Assertion::Unique { .. } => "unique",
            core::Assertion::Between { .. } => "between",
            core::Assertion::Regex { .. } => "regex",
            core::Assertion::Enum { .. } => "enum",
            core::Assertion::RowCount { .. } => "row_count",
            core::Assertion::Freshness { .. } => "freshness",
            _ => "unknown",
        }
    }
}

/// Python wrapper around `core::ProbePlan`.
#[pyclass(name = "ProbePlan", module = "ematix_probe._core")]
struct PyProbePlan {
    inner: core::ProbePlan,
}

#[pymethods]
impl PyProbePlan {
    /// Construct from Python: `ProbePlan(schema, table, [Assertion, ...])`.
    /// `schema` may be `None`.
    #[new]
    fn new(schema: Option<String>, table: String, assertions: Vec<PyAssertion>) -> Self {
        Self {
            inner: core::ProbePlan {
                schema,
                table,
                assertions: assertions.into_iter().map(|a| a.inner).collect(),
            },
        }
    }

    #[getter]
    fn schema(&self) -> Option<String> {
        self.inner.schema.clone()
    }

    #[getter]
    fn table(&self) -> String {
        self.inner.table.clone()
    }

    /// Number of assertions in this plan.
    fn assertion_count(&self) -> usize {
        self.inner.assertions.len()
    }

    fn __len__(&self) -> usize {
        self.inner.assertions.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "ProbePlan(schema={:?}, table={:?}, assertions={})",
            self.inner.schema,
            self.inner.table,
            self.inner.assertions.len()
        )
    }
}

#[pyfunction]
fn assertion_not_null(column: String) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::NotNull { column },
    }
}

/// Per-assertion outcome surfaced to Python.
#[pyclass(name = "AssertionResult", module = "ematix_probe._core", frozen)]
struct PyAssertionResult {
    inner: core::AssertionResult,
}

#[pymethods]
impl PyAssertionResult {
    #[getter]
    fn assertion_index(&self) -> usize {
        self.inner.assertion_index
    }

    /// Verdict as a string ("pass" / "fail" / "error") for
    /// ergonomic Python comparisons.
    #[getter]
    fn verdict(&self) -> &'static str {
        verdict_str(self.inner.verdict)
    }

    #[getter]
    fn message(&self) -> Option<String> {
        self.inner.message.clone()
    }

    fn __repr__(&self) -> String {
        format!("AssertionResult({:?})", self.inner)
    }
}

/// Aggregate result returned by `DataProbe.run`.
#[pyclass(name = "RunReport", module = "ematix_probe._core", frozen)]
struct PyRunReport {
    inner: core::RunSummary,
}

#[pymethods]
impl PyRunReport {
    #[getter]
    fn verdict(&self) -> &'static str {
        verdict_str(self.inner.verdict)
    }

    #[getter]
    fn assertions(&self) -> Vec<PyAssertionResult> {
        self.inner
            .assertions
            .iter()
            .cloned()
            .map(|inner| PyAssertionResult { inner })
            .collect()
    }

    fn __len__(&self) -> usize {
        self.inner.assertions.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "RunReport(verdict={:?}, assertions={})",
            verdict_str(self.inner.verdict),
            self.inner.assertions.len()
        )
    }
}

fn verdict_str(v: core::Verdict) -> &'static str {
    match v {
        core::Verdict::Pass => "pass",
        core::Verdict::Fail => "fail",
        core::Verdict::Error => "error",
    }
}

/// Synchronous Python entry point: connect to Postgres, run the
/// plan, return a `RunReport`. Internally builds a current-thread
/// tokio runtime (PostgresAdapter is async). pyo3-asyncio
/// integration that exposes an awaitable to Python lands later
/// (Sprint 9, pytest plugin work).
///
/// Releasing the GIL via `py.detach` is important: without it,
/// the runtime's `block_on` would hold the GIL while waiting on
/// the network, blocking other Python threads. (`detach` replaced
/// `allow_threads` in pyo3 0.28.)
#[pyfunction]
fn run_postgres_probe(py: Python<'_>, url: String, plan: &PyProbePlan) -> PyResult<PyRunReport> {
    let core_plan = plan.inner.clone();
    let summary = py.detach(move || -> Result<core::RunSummary, core::AdapterError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| core::AdapterError::Connection(format!("tokio runtime: {e}")))?;
        runtime.block_on(async move {
            let adapter = PostgresAdapter::connect(&url).await?;
            adapter.execute(&core_plan).await
        })
    });
    summary
        .map(|inner| PyRunReport { inner })
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Synchronous Python entry point for the in-process DuckDB adapter.
/// `path` is the same value `source.duckdb(path)` carries — a
/// filesystem path, or `:memory:` for a transient DB.
#[pyfunction]
fn run_duckdb_probe(py: Python<'_>, path: String, plan: &PyProbePlan) -> PyResult<PyRunReport> {
    let core_plan = plan.inner.clone();
    let summary = py.detach(move || -> Result<core::RunSummary, core::AdapterError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| core::AdapterError::Connection(format!("tokio runtime: {e}")))?;
        runtime.block_on(async move {
            let adapter = DuckDbAdapter::open(&path)?;
            adapter.execute(&core_plan).await
        })
    });
    summary
        .map(|inner| PyRunReport { inner })
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Helper for tests/examples to seed a DuckDB database via a SQL
/// batch (DDL + DML). Not a regular `DataAdapter` operation; lives
/// here so Python tests don't need a separate `duckdb` PyPI dep.
#[pyfunction]
fn duckdb_setup(py: Python<'_>, path: String, sql: String) -> PyResult<()> {
    py.detach(move || -> Result<(), core::AdapterError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| core::AdapterError::Connection(format!("tokio runtime: {e}")))?;
        runtime.block_on(async move {
            let adapter = DuckDbAdapter::open(&path)?;
            adapter.execute_setup(&sql).await
        })
    })
    .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Synchronous Python entry point for the S3 Parquet adapter.
/// Mirrors `source.s3_parquet(bucket=, key=, region=,
/// endpoint_url=)` on the Python side.
#[pyfunction]
#[pyo3(signature = (bucket, key, region, endpoint_url, plan))]
fn run_s3_parquet_probe(
    py: Python<'_>,
    bucket: String,
    key: String,
    region: String,
    endpoint_url: Option<String>,
    plan: &PyProbePlan,
) -> PyResult<PyRunReport> {
    let core_plan = plan.inner.clone();
    let summary = py.detach(move || -> Result<core::RunSummary, core::AdapterError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| core::AdapterError::Connection(format!("tokio runtime: {e}")))?;
        runtime.block_on(async move {
            let adapter = S3ParquetAdapter::open(&bucket, &key, &region, endpoint_url.as_deref())?;
            adapter.execute(&core_plan).await
        })
    });
    summary
        .map(|inner| PyRunReport { inner })
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// Synchronous Python entry point for the local-file Parquet
/// adapter. `path` is the same value `source.parquet(path)`
/// carries.
#[pyfunction]
fn run_parquet_probe(py: Python<'_>, path: String, plan: &PyProbePlan) -> PyResult<PyRunReport> {
    let core_plan = plan.inner.clone();
    let summary = py.detach(move || -> Result<core::RunSummary, core::AdapterError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| core::AdapterError::Connection(format!("tokio runtime: {e}")))?;
        runtime.block_on(async move {
            let adapter = ParquetAdapter::open(&PathBuf::from(path))?;
            adapter.execute(&core_plan).await
        })
    });
    summary
        .map(|inner| PyRunReport { inner })
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
fn assertion_unique(column: String) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::Unique { column },
    }
}

#[pyfunction]
fn assertion_between(column: String, low: f64, high: f64) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::Between { column, low, high },
    }
}

#[pyfunction]
fn assertion_freshness(column: String, within_seconds: i64) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::Freshness {
            column,
            within_seconds,
        },
    }
}

#[pyfunction]
fn assertion_regex(column: String, pattern: String) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::Regex { column, pattern },
    }
}

#[pyfunction]
fn assertion_enum(column: String, allowed: Vec<String>) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::Enum { column, allowed },
    }
}

#[pyfunction]
#[pyo3(signature = (low=None, high=None))]
fn assertion_row_count(low: Option<i64>, high: Option<i64>) -> PyAssertion {
    PyAssertion {
        inner: core::Assertion::RowCount { low, high },
    }
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", core::VERSION)?;
    m.add_class::<PyAssertion>()?;
    m.add_class::<PyProbePlan>()?;
    m.add_class::<PyAssertionResult>()?;
    m.add_class::<PyRunReport>()?;
    m.add_function(wrap_pyfunction!(assertion_not_null, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_unique, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_between, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_freshness, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_regex, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_enum, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_row_count, m)?)?;
    m.add_function(wrap_pyfunction!(run_postgres_probe, m)?)?;
    m.add_function(wrap_pyfunction!(run_duckdb_probe, m)?)?;
    m.add_function(wrap_pyfunction!(run_parquet_probe, m)?)?;
    m.add_function(wrap_pyfunction!(run_s3_parquet_probe, m)?)?;
    m.add_function(wrap_pyfunction!(duckdb_setup, m)?)?;
    Ok(())
}
