//! Postgres SQL load adapter — drives a parameterized query under
//! either scheduling discipline (`ConstantRate` / `VirtualUsers`)
//! and produces one [`Sample`] per tick.
//!
//! Per PRD: parameter values bind through `tokio-postgres`'s
//! prepared-statement API (`Client::prepare` + `Client::query`),
//! never via SQL string interpolation. Status mapping mirrors HTTP:
//! a successful round-trip is `status_code: Some(200)`; a SQL error
//! is `status_code: None, error: Some(message)`. Refinement of that
//! mapping (richer status values for Postgres-specific failure
//! classes) is S-8.6.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::types::ToSql;
use tokio_postgres::{Config, NoTls};

use crate::adapters::data::AdapterError;
use crate::engine::load::postgres::{LoadQuery, PgLoadPlan, QueryParam};
use crate::engine::load::scheduler::{ConstantRateScheduler, VuPool};
use crate::engine::load::{LoadMode, Sample};

/// Postgres load adapter. Stateless — a fresh deadpool is built
/// per `collect_samples` call so each plan runs in isolation.
pub struct PostgresLoadAdapter;

impl PostgresLoadAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Drive the plan and return per-tick samples. Each tick takes
    /// a client from the pool, prepares the statement (cheap once
    /// the server has cached the query plan), executes with the
    /// bound params, and emits a [`Sample`]. Open-model spawns one
    /// task per tick; closed-model hands the work closure to
    /// [`VuPool`].
    pub async fn collect_samples(&self, plan: &PgLoadPlan) -> Result<Vec<Sample>, AdapterError> {
        let pool = build_pool(&plan.target.dsn)?;
        let query = Arc::new(plan.target.query.clone());

        let samples = match plan.mode {
            LoadMode::ConstantRate { rps } => {
                Self::collect_constant_rate(pool, query, rps, plan.duration).await?
            }
            LoadMode::VirtualUsers { count } => {
                Self::collect_virtual_users(pool, query, count, plan.duration).await
            }
        };
        Ok(samples)
    }

    async fn collect_constant_rate(
        pool: Pool,
        query: Arc<LoadQuery>,
        rps: f64,
        duration: Duration,
    ) -> Result<Vec<Sample>, AdapterError> {
        let mut sched = ConstantRateScheduler::new(rps, duration);
        let mut handles = Vec::new();
        while let Some(tick) = sched.next_tick().await {
            let pool = pool.clone();
            let query = query.clone();
            handles.push(tokio::spawn(async move {
                run_query_to_sample(&pool, &query, tick.index).await
            }));
        }
        let mut samples = Vec::with_capacity(handles.len());
        for h in handles {
            samples.push(
                h.await
                    .map_err(|e| AdapterError::Connection(format!("join: {e}")))?,
            );
        }
        samples.sort_by_key(|s| s.tick_index);
        Ok(samples)
    }

    async fn collect_virtual_users(
        pool: Pool,
        query: Arc<LoadQuery>,
        count: usize,
        duration: Duration,
    ) -> Vec<Sample> {
        let pool = Arc::new(pool);
        VuPool::new(count, duration)
            .run(move |idx| {
                let pool = pool.clone();
                let query = query.clone();
                async move { run_query_to_sample(&pool, &query, idx).await }
            })
            .await
    }
}

impl Default for PostgresLoadAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn build_pool(dsn: &str) -> Result<Pool, AdapterError> {
    let pg_config = Config::from_str(dsn)
        .map_err(|e| AdapterError::Config(format!("invalid postgres URL: {e}")))?;
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
    Pool::builder(mgr)
        .build()
        .map_err(|e| AdapterError::Connection(format!("pool builder failed: {e}")))
}

/// One tick → one Sample. Connection-acquisition / prepare /
/// execute failures all land as `error: Some, status_code: None`.
async fn run_query_to_sample(pool: &Pool, query: &LoadQuery, tick_index: u64) -> Sample {
    let started = Instant::now();
    let result = execute_once(pool, query).await;
    let latency = started.elapsed();
    match result {
        Ok(()) => Sample {
            tick_index,
            latency,
            status_code: Some(200),
            error: None,
        },
        Err(e) => Sample {
            tick_index,
            latency,
            status_code: None,
            error: Some(e),
        },
    }
}

async fn execute_once(pool: &Pool, query: &LoadQuery) -> Result<(), String> {
    let client = pool.get().await.map_err(|e| format!("acquire: {e}"))?;
    let stmt = client
        .prepare(query.sql())
        .await
        .map_err(|e| format!("prepare: {e}"))?;

    let owned = bind_owned(query.params());
    let refs: Vec<&(dyn ToSql + Sync)> = owned
        .iter()
        .map(|b| b.as_ref() as &(dyn ToSql + Sync))
        .collect();

    client
        .query(&stmt, &refs[..])
        .await
        .map(|_rows| ())
        .map_err(|e| format!("query: {e}"))
}

/// Materialize each `QueryParam` into a heap-allocated trait object
/// so the slice of refs we hand to `Client::query` outlives the
/// individual values. `Null` binds as `Option::<i32>::None`; richer
/// typed-null binding is deferred.
fn bind_owned(params: &[QueryParam]) -> Vec<Box<dyn ToSql + Sync + Send>> {
    params
        .iter()
        .map(|p| -> Box<dyn ToSql + Sync + Send> {
            match p {
                QueryParam::Int(i) => Box::new(*i),
                QueryParam::Float(f) => Box::new(*f),
                QueryParam::Text(s) => Box::new(s.clone()),
                QueryParam::Bool(b) => Box::new(*b),
                QueryParam::Null => Box::new(Option::<i32>::None),
            }
        })
        .collect()
}
