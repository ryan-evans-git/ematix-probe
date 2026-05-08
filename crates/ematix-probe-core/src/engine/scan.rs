//! Scan path: pull-based stream of Arrow `RecordBatch`es.
//!
//! Postgres pushes assertions down as SQL — counts come back from
//! the server and the engine never sees rows. Sources that can't
//! do that (local Parquet, in-process DuckDB, future S3 Parquet)
//! instead expose a `Scanner` and let the engine evaluate the
//! `ProbePlan` against batches in Rust. This module declares the
//! trait + the canonical batch alias so the two adapters added in
//! S-4.5 / S-4.6 can share scan-path evaluators (S-4.2..S-4.4).
//!
//! Design notes:
//! - `next_batch` is `async` so DuckDB / Parquet I/O can be
//!   non-blocking under the same `tokio` runtime that drives the
//!   Postgres adapter. Empty stream → first call returns
//!   `Ok(None)`.
//! - `schema` is sync because every backend knows the schema the
//!   moment the scanner is opened — no need to peek at a batch.
//!   Returning `SchemaRef` (an `Arc<Schema>`) lets evaluators clone
//!   cheaply across batch boundaries.
//! - The trait deliberately doesn't carry a `close`/`Drop` hook;
//!   resource cleanup happens via `Drop` on the concrete adapter
//!   (the underlying DuckDB connection or `parquet` reader).

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;

use crate::adapters::data::AdapterError;

/// Canonical Arrow batch type used by the scan path. Aliased so
/// downstream call sites don't have to depend on `arrow` directly
/// when they're only carrying batches through.
pub type ArrowBatch = RecordBatch;

/// Pull-based source of Arrow batches. Implementations are owned
/// by their adapter; the engine drives them via `&mut`.
#[async_trait]
pub trait Scanner: Send {
    /// Schema of the batches this scanner will yield. Stable for
    /// the lifetime of the scanner.
    fn schema(&self) -> SchemaRef;

    /// Pull the next batch, or `None` to signal end-of-stream.
    /// Returning a zero-row batch is allowed but discouraged —
    /// most evaluators handle it correctly, but it's wasted work.
    async fn next_batch(&mut self) -> Result<Option<ArrowBatch>, AdapterError>;
}
