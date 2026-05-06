//! Adapter modules. `adapters::data` defines the data-probe trait
//! and concrete backends (Postgres in S-2.2, DuckDB + Parquet in
//! Phase 2, S3 in Phase 3); `adapters::load` lands with HTTP +
//! Postgres SQL load drivers in Phases 4-5.

pub mod data;
