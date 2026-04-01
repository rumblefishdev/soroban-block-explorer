//! PostgreSQL connection pool configuration for Lambda.
//!
//! Lambda functions should use `max_connections(1)` with RDS Proxy.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Create a PgPool configured for Lambda (single connection, RDS Proxy).
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(1)
        .test_before_acquire(true)
        .connect(database_url)
        .await
}
