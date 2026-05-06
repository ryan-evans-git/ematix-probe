// Python bindings for ematix-probe.
//
// Phase 0 ships only `__version__`. Probe types, ProbePlan, and engine
// entrypoints land in subsequent phases per docs/PI_PLAN.md.

use pyo3::prelude::*;

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", ematix_probe_core::VERSION)?;
    Ok(())
}
