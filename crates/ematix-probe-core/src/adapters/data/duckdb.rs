//! DuckDB data adapter — first scan-path adapter.
//!
//! DuckDB is in-process: no network, no daemon. The adapter holds
//! one `Connection` for its lifetime (so an in-memory `":memory:"`
//! database persists across `execute_setup` + `execute` calls), and
//! every DB call is dispatched through `tokio::task::spawn_blocking`
//! since the `duckdb` crate is sync.
//!
//! Each `execute` runs `SELECT * FROM <qualified_table>` via
//! `Statement::query_arrow`, eager-collects the resulting batches
//! into a `Vec`, and feeds the engine's scan-path evaluator.
//!
//! Eager-collection trades memory for simplicity in v0.1. The
//! `duckdb` crate's `Statement::query_arrow` borrows from
//! `Statement` which borrows from `Connection`, and threading those
//! lifetimes through an `async fn next_batch` would need a
//! self-referential struct. We accept the memory cost on the bet
//! that probe-target tables in v0.1 fit comfortably in memory; the
//! Phase 3 S3 Parquet work will revisit streaming.
//!
//! Concurrency: the connection is wrapped in `Arc<Mutex<...>>` so
//! `&self` `execute` can lock and use it inside `spawn_blocking`.
//! In-flight executes serialize on the mutex; that's fine for v0.1
//! (data probes are not the hot path).

use std::sync::{Arc, Mutex};

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;

use crate::adapters::data::{AdapterError, DataAdapter};
use crate::engine::data::{ProbePlan, RunSummary};
use crate::engine::scan::{evaluate, ArrowBatch, Scanner};

/// In-process DuckDB adapter.
///
/// Holds the database path/URL (`":memory:"` for transient,
/// otherwise a filesystem path) and one long-lived connection
/// behind a mutex.
pub struct DuckDbAdapter {
    conn: Arc<Mutex<::duckdb::Connection>>,
}

impl DuckDbAdapter {
    /// Open the DuckDB database at `path` and validate by issuing a
    /// `SELECT 1`. `":memory:"` is the canonical in-memory marker.
    pub fn open(path: &str) -> Result<Self, AdapterError> {
        let conn = ::duckdb::Connection::open(path)
            .map_err(|e| AdapterError::Connection(format!("duckdb open {path:?}: {e}")))?;
        conn.execute_batch("SELECT 1")
            .map_err(|e| AdapterError::Query(format!("duckdb validation SELECT 1 failed: {e}")))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run a SQL batch (DDL + DML) against this adapter's database.
    /// Used by tests + examples to seed data; not part of the
    /// `DataAdapter` trait surface.
    pub async fn execute_setup(&self, sql: &str) -> Result<(), AdapterError> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), AdapterError> {
            let conn = conn
                .lock()
                .map_err(|_| AdapterError::Connection("duckdb mutex poisoned".into()))?;
            conn.execute_batch(&sql)
                .map_err(|e| AdapterError::Query(format!("duckdb setup batch failed: {e}")))?;
            Ok(())
        })
        .await
        .map_err(|e| AdapterError::Connection(format!("spawn_blocking join: {e}")))?
    }
}

#[async_trait]
impl DataAdapter for DuckDbAdapter {
    async fn execute(&self, plan: &ProbePlan) -> Result<RunSummary, AdapterError> {
        let conn = self.conn.clone();
        let qualified = qualified_table(plan.schema.as_deref(), &plan.table);

        let (schema, batches) = tokio::task::spawn_blocking(
            move || -> Result<(SchemaRef, Vec<RecordBatch>), AdapterError> {
                let conn = conn
                    .lock()
                    .map_err(|_| AdapterError::Connection("duckdb mutex poisoned".into()))?;
                let mut stmt = conn
                    .prepare(&format!("SELECT * FROM {qualified}"))
                    .map_err(|e| AdapterError::Query(format!("duckdb prepare failed: {e}")))?;
                let stream = stmt
                    .query_arrow([])
                    .map_err(|e| AdapterError::Query(format!("duckdb query_arrow failed: {e}")))?;
                let schema = stream.get_schema();
                let batches: Vec<RecordBatch> = stream.collect();
                Ok((schema, batches))
            },
        )
        .await
        .map_err(|e| AdapterError::Connection(format!("spawn_blocking join: {e}")))??;

        let mut scanner = VecScanner {
            schema,
            iter: batches.into_iter(),
        };
        evaluate(plan, &mut scanner).await
    }
}

/// Eager-loaded scanner: holds all batches in a Vec. Used by the
/// DuckDB adapter (and likely the Parquet adapter in S-4.6 once
/// memory pressure isn't a concern). For larger inputs that don't
/// fit in memory, a streaming variant will land alongside S3
/// Parquet in Phase 3.
struct VecScanner {
    schema: SchemaRef,
    iter: std::vec::IntoIter<RecordBatch>,
}

#[async_trait]
impl Scanner for VecScanner {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    async fn next_batch(&mut self) -> Result<Option<ArrowBatch>, AdapterError> {
        Ok(self.iter.next())
    }
}

fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn qualified_table(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}
