//! Python bindings for ematix-probe.
//!
//! S-2.6 lands the `ProbePlan` / `Assertion` types + the assertion
//! factory functions that the Python `@probe.data` decorator uses
//! to assemble plans. Probe execution (`run`) lands in S-2.7.

use ematix_probe_core as core;
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

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", core::VERSION)?;
    m.add_class::<PyAssertion>()?;
    m.add_class::<PyProbePlan>()?;
    m.add_function(wrap_pyfunction!(assertion_not_null, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_unique, m)?)?;
    m.add_function(wrap_pyfunction!(assertion_between, m)?)?;
    Ok(())
}
